use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{error, info, warn};

use crate::audio::capture::{AudioCapture, AudioCaptureSession};
use crate::config::AppConfig;
use crate::input::hotkey::{HotkeyEvent, HotkeyListener};
use crate::output::typing::TextTyper;
use crate::platform::windows::{
    open_settings_dialog, pump_message_queue, show_error_dialog, TrayStatus, WindowsTray,
};
use crate::stt::engine::{DecodeOptions, WhisperCliTranscriber};

mod state;
mod streaming;

use state::{AppPhase, AppState};
use streaming::{remainder_after_typed, SentenceStreamingTranscriber};

pub fn run(config: AppConfig) -> Result<()> {
    validate_startup_paths(&config)?;
    info!("starting Hermes");

    let mut tray = WindowsTray::new()?;
    let mut state = AppState::new();
    tray.set_status(TrayStatus::Idle);

    let hotkey_listener = HotkeyListener::start(config.hotkey.clone())?;
    let transcriber = WhisperCliTranscriber::new(&config)?;
    let mut audio_capture: Option<AudioCapture> = None;
    let typer = TextTyper::new();

    let decode_options = DecodeOptions {
        language: config.language.clone(),
    };

    let mut current_session: Option<AudioCaptureSession> = None;
    let mut sentence_stream: Option<SentenceStreamingTranscriber> = None;
    let live_typing = config.type_output && config.stream_output;
    let mut typed_output = String::new();

    loop {
        pump_message_queue();
        tray.tick();
        if tray.should_quit() {
            break;
        }
        if tray.take_open_settings_requested() {
            if let Err(err) = open_settings_and_persist() {
                error!("failed to open settings dialog: {err:#}");
                show_error_dialog("Hermes Settings", &format!("{err:#}"));
            }
            tray.set_status(TrayStatus::Idle);
        }

        if let Some(event) = hotkey_listener.try_recv()? {
            match event {
                HotkeyEvent::Pressed => {
                    if current_session.is_none() {
                        if audio_capture.is_none() {
                            match AudioCapture::new().context("audio initialization failed") {
                                Ok(capture) => {
                                    audio_capture = Some(capture);
                                }
                                Err(err) => {
                                    error!("{err:#}");
                                    eprintln!("{err:#}");
                                    state.set_phase(AppPhase::Error);
                                    tray.set_status(TrayStatus::Error);
                                    continue;
                                }
                            }
                        }

                        let Some(capture) = audio_capture.as_ref() else {
                            continue;
                        };

                        match capture.start_session() {
                            Ok(session) => {
                                typed_output.clear();
                                current_session = Some(session);
                                sentence_stream = Some(SentenceStreamingTranscriber::new(
                                    transcriber.clone(),
                                    decode_options.clone(),
                                ));
                                state.set_phase(AppPhase::Recording);
                                tray.set_status(TrayStatus::Recording);
                                println!("[recording] started");
                                let _ = io::stdout().flush();
                            }
                            Err(err) => {
                                error!("failed to start recording session: {err:#}");
                                eprintln!("failed to start recording session: {err:#}");
                                audio_capture = None;
                                sentence_stream = None;
                                state.set_phase(AppPhase::Error);
                                tray.set_status(TrayStatus::Error);
                            }
                        }
                    }
                }
                HotkeyEvent::Released => {
                    if let Some(session) = current_session.take() {
                        println!("[recording] stopped");
                        let _ = io::stdout().flush();
                        let captured =
                            match session.finish().context("failed to end recording session") {
                                Ok(captured) => captured,
                                Err(err) => {
                                    error!("{err:#}");
                                    sentence_stream = None;
                                    state.set_phase(AppPhase::Error);
                                    tray.set_status(TrayStatus::Error);
                                    continue;
                                }
                            };
                        if captured.duration_ms < config.min_record_ms {
                            info!(
                                "discarded short capture ({} ms < {} ms)",
                                captured.duration_ms, config.min_record_ms
                            );
                            sentence_stream = None;
                            state.set_phase(AppPhase::Idle);
                            tray.set_status(TrayStatus::Idle);
                            continue;
                        }

                        state.set_phase(AppPhase::Transcribing);
                        tray.set_status(TrayStatus::Transcribing);
                        let transcript = match sentence_stream
                            .take()
                            .unwrap_or_else(|| {
                                SentenceStreamingTranscriber::new(
                                    transcriber.clone(),
                                    decode_options.clone(),
                                )
                            })
                            .finalize(&captured, &transcriber, &decode_options)
                            .context("transcription failed")
                        {
                            Ok(transcript) => transcript,
                            Err(err) => {
                                error!("{err:#}");
                                state.set_phase(AppPhase::Error);
                                tray.set_status(TrayStatus::Error);
                                continue;
                            }
                        };
                        let cleaned = maybe_append_terminal_punctuation(
                            transcript.text.trim().to_string(),
                            config.auto_punctuation,
                        );
                        if !cleaned.is_empty() {
                            print_transcript_to_terminal(&cleaned, transcript.latency_ms);
                        }

                        if config.type_output && !cleaned.is_empty() {
                            let to_type = if live_typing && !typed_output.is_empty() {
                                remainder_after_typed(&typed_output, &cleaned)
                            } else {
                                cleaned.clone()
                            };
                            if !to_type.is_empty() {
                                state.set_phase(AppPhase::Typing);
                                tray.set_status(TrayStatus::Typing);
                                if let Err(err) = typer.type_text(&to_type) {
                                    error!("failed to type output text: {err:#}");
                                    state.set_phase(AppPhase::Error);
                                    tray.set_status(TrayStatus::Error);
                                    continue;
                                }
                            }
                        }
                        state.set_phase(AppPhase::Idle);
                        tray.set_status(TrayStatus::Idle);
                    }
                }
                HotkeyEvent::ListenerError(message) => {
                    warn!("hotkey listener error: {message}");
                    state.set_phase(AppPhase::Error);
                    tray.set_status(TrayStatus::Error);
                }
            }
        }

        if let (Some(session), Some(stream)) = (current_session.as_ref(), sentence_stream.as_mut())
        {
            stream.tick(session);
            if live_typing {
                if let Some(delta) = stream.take_live_stream_delta() {
                    let to_type = if typed_output.is_empty() {
                        delta
                    } else {
                        format!(" {delta}")
                    };
                    state.set_phase(AppPhase::Typing);
                    tray.set_status(TrayStatus::Typing);
                    if let Err(err) = typer.type_text(&to_type) {
                        error!("failed to stream output text: {err:#}");
                        state.set_phase(AppPhase::Error);
                        tray.set_status(TrayStatus::Error);
                    } else {
                        typed_output.push_str(&to_type);
                        state.set_phase(AppPhase::Recording);
                        tray.set_status(TrayStatus::Recording);
                    }
                }
            }
        }

        if state.phase() == AppPhase::Error {
            thread::sleep(Duration::from_millis(250));
        } else {
            thread::sleep(Duration::from_millis(20));
        }
    }

    info!("Hermes exited");
    Ok(())
}

pub fn open_settings_once() -> Result<()> {
    open_settings_and_persist()
}

pub fn run_diagnostics(config: &AppConfig) -> Result<()> {
    println!("Hermes diagnostics");
    println!("config path: {}", AppConfig::config_path().display());
    println!("model path: {}", config.model_path.display());
    println!(
        "whisper-cli path: {}",
        config.resolved_whisper_cli_path().display()
    );
    println!("hotkey: {}+{}", config.hotkey.modifier, config.hotkey.key);
    println!("inference mode: cpu_only");
    println!("language: {}", config.language);

    if config.model_path.exists() {
        println!("model file: OK");
    } else {
        println!("model file: MISSING");
    }

    if config.resolved_whisper_cli_path().exists() {
        println!("whisper-cli binary: OK");
    } else {
        println!("whisper-cli binary: MISSING");
    }

    let audio = AudioCapture::new();
    println!(
        "audio input device: {}",
        if audio.is_ok() { "OK" } else { "FAILED" }
    );

    println!("diagnostics complete");
    Ok(())
}

fn validate_startup_paths(config: &AppConfig) -> Result<()> {
    let model_parent = config
        .model_path
        .parent()
        .map(Path::to_path_buf)
        .context("model path has no parent directory")?;
    std::fs::create_dir_all(&model_parent)?;

    let data_dir = config.data_dir();
    std::fs::create_dir_all(data_dir)?;
    Ok(())
}

fn open_settings_and_persist() -> Result<()> {
    let current_config = AppConfig::load_or_create_default()?;
    if let Some(updated_config) = open_settings_dialog(&current_config)? {
        updated_config.save()?;
        println!("[settings] saved to {}", AppConfig::config_path().display());
        println!("[settings] restart app to apply all changes");
        let _ = io::stdout().flush();
    }
    Ok(())
}

fn maybe_append_terminal_punctuation(mut text: String, enabled: bool) -> String {
    if !enabled || text.is_empty() {
        return text;
    }
    let last = text.chars().last().unwrap_or_default();
    if matches!(last, '.' | '!' | '?') {
        return text;
    }
    text.push('.');
    text
}

fn print_transcript_to_terminal(text: &str, latency_ms: u128) {
    info!("transcribed in {} ms", latency_ms);
    println!("[{latency_ms}ms] {text}");
    let _ = io::stdout().flush();
}
