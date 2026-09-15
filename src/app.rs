use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{error, info, warn};

use crate::audio::capture::{AudioCapture, AudioCaptureSession};
use crate::config::AppConfig;
use crate::input::hotkey::{HotkeyEvent, HotkeyListener};
use crate::output::typing::{PasteStyle, TextTyper};
use crate::platform::windows::{
    capture_foreground_target, open_settings_dialog, play_recording_start_beep,
    play_recording_stop_beep, pump_message_queue, show_error_dialog, ForegroundTarget,
    TrayStatus, WindowsTray,
};
use crate::stt::engine::{DecodeOptions, Transcriber};
use crate::stt::whisperx::probe_cuda;
use crate::stt::{create_transcriber, BackendTranscriber};

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
    let transcriber = create_transcriber(&config)?;
    if config.stream_output && config.uses_whisperx() {
        info!("stream_output is disabled while stt_backend=whisperx (WhisperX uses release-only transcription in v1)");
    }
    let mut audio_capture: Option<AudioCapture> = None;
    let typer = TextTyper::new();

    let decode_options = DecodeOptions {
        language: config.language.clone(),
    };

    let mut current_session: Option<AudioCaptureSession> = None;
    let mut sentence_stream: Option<SentenceStreamingTranscriber> = None;
    let live_typing = config.type_output && config.effective_stream_output();
    let mut typed_output = String::new();
    let mut recording_target: Option<ForegroundTarget> = None;

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
                                recording_target =
                                    capture_foreground_target(config.allow_terminal_output);
                                if recording_target.is_none() {
                                    let hint = if config.allow_terminal_output {
                                        "click a text field before holding Right Ctrl+Right Shift"
                                    } else {
                                        "click a text field (not a terminal) before holding Right Ctrl+Right Shift, or enable allow_terminal_output"
                                    };
                                    warn!("no text target captured; {hint}");
                                    eprintln!(
                                        "[hermes] no text target captured; transcript will print here only"
                                    );
                                    let _ = io::stdout().flush();
                                }
                                current_session = Some(session);
                                sentence_stream = if config.effective_stream_output() {
                                    Some(SentenceStreamingTranscriber::new(
                                        transcriber.clone(),
                                        decode_options.clone(),
                                    ))
                                } else {
                                    None
                                };
                                state.set_phase(AppPhase::Recording);
                                tray.set_status(TrayStatus::Recording);
                                play_recording_start_beep();
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
                        play_recording_stop_beep();
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
                            recording_target = None;
                            state.set_phase(AppPhase::Idle);
                            tray.set_status(TrayStatus::Idle);
                            continue;
                        }

                        state.set_phase(AppPhase::Transcribing);
                        tray.set_status(TrayStatus::Transcribing);
                        let transcript = match transcribe_release(
                            sentence_stream.take(),
                            &captured,
                            &transcriber,
                            &decode_options,
                        ) {
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
                            let to_type = release_typing_text(
                                live_typing,
                                &typed_output,
                                &cleaned,
                            );
                            if !to_type.is_empty() {
                                state.set_phase(AppPhase::Typing);
                                tray.set_status(TrayStatus::Typing);
                                if let Err(err) =
                                    type_into_target(&typer, &to_type, recording_target.as_ref())
                                {
                                    error!("failed to type output text: {err:#}");
                                    state.set_phase(AppPhase::Error);
                                    tray.set_status(TrayStatus::Error);
                                    continue;
                                }
                            }
                        }
                        recording_target = None;
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
                    if let Err(err) =
                        type_into_target(&typer, &to_type, recording_target.as_ref())
                    {
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
    println!("stt backend: {}", config.stt_backend);
    println!("hotkey: {}+{}", config.hotkey.modifier, config.hotkey.key);
    println!("language: {}", config.language);
    println!(
        "stream output effective: {}",
        config.effective_stream_output()
    );

    if config.uses_whisperx() {
        let python_path = config.resolved_whisperx_python_path();
        println!("whisperx python: {}", python_path.display());
        println!("whisperx model: {}", config.whisperx_model);
        println!(
            "whisperx model dir: {}",
            config.resolved_whisperx_model_dir().display()
        );
        println!(
            "whisperx python binary: {}",
            if python_path.exists() { "OK" } else { "MISSING" }
        );
        println!(
            "cuda available: {}",
            if probe_cuda(&python_path) {
                "YES"
            } else {
                "NO"
            }
        );
    } else {
        println!("model path: {}", config.model_path.display());
        println!(
            "whisper-cli path: {}",
            config.resolved_whisper_cli_path().display()
        );
        println!("inference mode: cpu_only");
        println!(
            "model file: {}",
            if config.model_path.exists() {
                "OK"
            } else {
                "MISSING"
            }
        );
        println!(
            "whisper-cli binary: {}",
            if config.resolved_whisper_cli_path().exists() {
                "OK"
            } else {
                "MISSING"
            }
        );
    }

    let audio = AudioCapture::new();
    println!(
        "audio input device: {}",
        if audio.is_ok() { "OK" } else { "FAILED" }
    );

    println!("diagnostics complete");
    Ok(())
}

fn transcribe_release(
    sentence_stream: Option<SentenceStreamingTranscriber>,
    captured: &crate::audio::capture::CapturedAudio,
    transcriber: &BackendTranscriber,
    options: &DecodeOptions,
) -> Result<crate::stt::engine::Transcript> {
    if let Some(stream) = sentence_stream {
        return stream
            .finalize(captured, transcriber, options)
            .context("transcription failed");
    }

    transcriber
        .transcribe(&captured.pcm_16khz_mono, options)
        .context("transcription failed")
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

fn release_typing_text(live_typing: bool, typed_output: &str, cleaned: &str) -> String {
    if !live_typing || typed_output.is_empty() {
        return cleaned.to_string();
    }

    let remainder = remainder_after_typed(typed_output, cleaned);
    if remainder.is_empty()
        && normalize_typing_text(typed_output) != normalize_typing_text(cleaned)
    {
        return cleaned.to_string();
    }

    remainder
}

fn normalize_typing_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn type_into_target(
    typer: &TextTyper,
    text: &str,
    target: Option<&ForegroundTarget>,
) -> Result<()> {
    let Some(target) = target else {
        return Ok(());
    };

    target.restore();
    let paste_style = if target.is_terminal() {
        PasteStyle::Terminal
    } else {
        PasteStyle::Standard
    };
    typer.type_text(text, paste_style)
}
