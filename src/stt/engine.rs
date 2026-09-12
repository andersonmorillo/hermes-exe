use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use hound::{SampleFormat, WavSpec, WavWriter};
use tracing::info;

use crate::config::AppConfig;
use crate::tooling::write_embedded_tooling_script;

#[derive(Debug, Clone)]
pub struct DecodeOptions {
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub latency_ms: u128,
}

pub trait Transcriber {
    fn transcribe(&self, pcm_mono_16khz: &[f32], options: &DecodeOptions) -> Result<Transcript>;
}

#[derive(Clone)]
pub struct WhisperCliTranscriber {
    whisper_cli_path: PathBuf,
    model_path: PathBuf,
    scratch_dir: PathBuf,
}

impl WhisperCliTranscriber {
    pub fn new(config: &AppConfig) -> Result<Self> {
        let scratch_dir = config.data_dir().join("scratch");
        fs::create_dir_all(&scratch_dir)?;

        let whisper_cli_path = config.resolved_whisper_cli_path();
        ensure_runtime_files(&whisper_cli_path)?;

        Ok(Self {
            whisper_cli_path,
            model_path: config.model_path.clone(),
            scratch_dir,
        })
    }
}

impl Transcriber for WhisperCliTranscriber {
    fn transcribe(&self, pcm_mono_16khz: &[f32], options: &DecodeOptions) -> Result<Transcript> {
        if pcm_mono_16khz.is_empty() {
            return Ok(Transcript {
                text: String::new(),
                latency_ms: 0,
            });
        }

        ensure_model_file(&self.model_path)?;

        let started = Instant::now();
        let nonce = rand_seed();
        let wav_path = self.scratch_dir.join(format!("ptt-{nonce}.wav"));
        let out_prefix = self.scratch_dir.join(format!("ptt-{nonce}-transcript"));
        write_wav_f32_16khz(&wav_path, pcm_mono_16khz)?;

        let result = self.run_whisper(&wav_path, &out_prefix, options);
        cleanup_temp_files(&wav_path, &out_prefix);
        let text = result?;
        Ok(Transcript {
            text: normalize_transcript(&text),
            latency_ms: started.elapsed().as_millis(),
        })
    }
}

impl WhisperCliTranscriber {
    fn run_whisper(
        &self,
        wav_path: &Path,
        out_prefix: &Path,
        options: &DecodeOptions,
    ) -> Result<String> {
        let mut cmd = Command::new(&self.whisper_cli_path);
        cmd.arg("-m")
            .arg(&self.model_path)
            .arg("-f")
            .arg(wav_path)
            .arg("-l")
            .arg(&options.language)
            .arg("-otxt")
            .arg("-nt")
            .arg("-of")
            .arg(out_prefix)
            .arg("-ng");

        let output = cmd.output().with_context(|| {
            format!(
                "failed to execute whisper CLI at {}",
                self.whisper_cli_path.display()
            )
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let hint = whisper_cli_status_hint(output.status.code())
                .map(|h| format!("\n{h}"))
                .unwrap_or_default();
            bail!(
                "whisper CLI failed (status: {}):\nstdout: {}\nstderr: {}{}",
                output.status,
                stdout,
                stderr,
                hint
            );
        }

        let txt_path = resolve_transcript_path(wav_path, out_prefix).ok_or_else(|| {
            let candidates = candidate_output_paths(wav_path, out_prefix)
                .into_iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow!(
                "failed to locate transcription output file. tried: [{candidates}]\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })?;
        let text = fs::read_to_string(&txt_path).with_context(|| {
            format!(
                "failed to read transcription output file at {}",
                txt_path.display()
            )
        })?;
        Ok(text)
    }
}

fn write_wav_f32_16khz(path: &Path, pcm: &[f32]) -> Result<()> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec)
        .with_context(|| format!("failed to create {}", path.display()))?;
    for sample in pcm {
        let clamped = sample.clamp(-1.0, 1.0);
        writer.write_sample((clamped * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(())
}

fn cleanup_temp_files(wav_path: &Path, out_prefix: &Path) {
    let _ = fs::remove_file(wav_path);
    let _ = fs::remove_file(wav_path.with_extension("txt"));
    let _ = fs::remove_file(wav_path.with_extension("json"));
    let _ = fs::remove_file(wav_path.with_extension("vtt"));
    let _ = fs::remove_file(wav_path.with_extension("srt"));
    let _ = fs::remove_file(PathBuf::from(format!("{}.txt", wav_path.display())));
    let _ = fs::remove_file(PathBuf::from(format!("{}.json", wav_path.display())));
    let _ = fs::remove_file(PathBuf::from(format!("{}.vtt", wav_path.display())));
    let _ = fs::remove_file(PathBuf::from(format!("{}.srt", wav_path.display())));
    let _ = fs::remove_file(out_prefix.with_extension("txt"));
    let _ = fs::remove_file(out_prefix.with_extension("json"));
    let _ = fs::remove_file(out_prefix.with_extension("vtt"));
    let _ = fs::remove_file(out_prefix.with_extension("srt"));
}

fn resolve_transcript_path(wav_path: &Path, out_prefix: &Path) -> Option<PathBuf> {
    candidate_output_paths(wav_path, out_prefix)
        .into_iter()
        .find(|p| p.exists())
}

fn candidate_output_paths(wav_path: &Path, out_prefix: &Path) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    let raw = vec![
        out_prefix.with_extension("txt"),
        PathBuf::from(format!("{}.txt", wav_path.display())),
        wav_path.with_extension("txt"),
    ];

    for path in raw {
        let key = path.to_string_lossy().to_string();
        if seen.insert(key) {
            candidates.push(path);
        }
    }

    candidates
}

fn normalize_transcript(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn rand_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    (now as u64) ^ ((now >> 32) as u64)
}

fn whisper_cli_status_hint(code: Option<i32>) -> Option<&'static str> {
    // 0xC0000135 (STATUS_DLL_NOT_FOUND) is surfaced by Windows as -1073741515.
    if code == Some(-1073741515) {
        return Some(
            "hint: whisper-cli failed to start because required DLLs are missing. Place the whisper.cpp runtime DLLs next to whisper-cli.exe (for example: whisper.dll, ggml.dll, ggml-base.dll, ggml-cpu.dll).",
        );
    }
    None
}

fn ensure_model_file(model_path: &Path) -> Result<()> {
    if model_path.exists() {
        return Ok(());
    }

    let Some(variant) = known_model_variant(model_path) else {
        bail!("Whisper model not found at {}", model_path.display());
    };

    println!(
        "[model] {} is missing, attempting automatic download",
        model_path.display()
    );
    let _ = io::stdout().flush();

    match try_bootstrap_model(model_path, variant) {
        Ok(()) => {
            if !model_path.exists() {
                bail!(
                    "automatic model download completed, but the model is still missing at {}",
                    model_path.display()
                );
            }
            println!("[model] {} ready", variant);
            let _ = io::stdout().flush();
            Ok(())
        }
        Err(error) => {
            bail!(
                "Whisper model not found at {}\nautomatic model download failed: {error:#}\nmanual fix: install Python 3 so `py` or `python` is on PATH and re-run Hermes, or download the `{variant}` model from Settings.",
                model_path.display()
            );
        }
    }
}

fn ensure_runtime_files(whisper_cli_path: &Path) -> Result<()> {
    if validate_runtime_files(whisper_cli_path).is_ok() {
        return Ok(());
    }

    let runtime_dir = runtime_dir(whisper_cli_path)?;
    println!(
        "[runtime] whisper runtime missing, attempting automatic download into {}",
        runtime_dir.display()
    );
    let _ = io::stdout().flush();

    match try_bootstrap_runtime(whisper_cli_path) {
        Ok(()) => {
            validate_runtime_files(whisper_cli_path).with_context(|| {
                format!(
                    "automatic runtime download completed, but the runtime is still incomplete in {}",
                    runtime_dir.display()
                )
            })?;
            info!("whisper runtime ready at {}", runtime_dir.display());
            println!("[runtime] whisper runtime ready");
            let _ = io::stdout().flush();
            Ok(())
        }
        Err(bootstrap_error) => {
            let manual_hint = format!(
                "manual fix: install Python 3 so `py` or `python` is on PATH and re-run Hermes, or manually place whisper-cli.exe with whisper.dll, ggml.dll, ggml-base.dll, and ggml-cpu.dll in {}",
                runtime_dir.display()
            );
            let initial_error = validate_runtime_files(whisper_cli_path)
                .err()
                .unwrap_or_else(|| anyhow!("whisper runtime validation failed"));
            bail!(
                "{initial_error:#}\nautomatic runtime download failed: {bootstrap_error:#}\n{manual_hint}"
            );
        }
    }
}

fn validate_runtime_files(whisper_cli_path: &Path) -> Result<()> {
    if !whisper_cli_path.exists() {
        bail!(
            "whisper-cli executable not found at {}",
            whisper_cli_path.display()
        );
    }

    let runtime_dir = runtime_dir(whisper_cli_path)?;
    let missing = missing_runtime_files(runtime_dir);

    if !missing.is_empty() {
        bail!(
            "whisper runtime files are missing in {}: {}",
            runtime_dir.display(),
            missing.join(", ")
        );
    }

    Ok(())
}

fn try_bootstrap_runtime(whisper_cli_path: &Path) -> Result<()> {
    let runtime_dir = runtime_dir(whisper_cli_path)?;
    fs::create_dir_all(runtime_dir)?;

    let script_path = write_embedded_tooling_script("runtime")
        .context("failed to prepare embedded Python runtime bootstrap helper")?;
    let result = (|| {
        let asset_name = preferred_runtime_asset_name();
        let mut attempts = Vec::new();

        for (program, prefix_args) in python_launchers() {
            let mut cmd = Command::new(program);
            cmd.args(prefix_args)
                .arg(&script_path)
                .arg("ensure-runtime")
                .arg("--runtime-dir")
                .arg(runtime_dir)
                .arg("--asset-name")
                .arg(asset_name);

            match cmd.status() {
                Ok(status) if status.success() => return Ok(()),
                Ok(status) => attempts.push(format!(
                    "{} {} exited with status {}",
                    program,
                    prefix_args.join(" "),
                    status
                )),
                Err(error) => attempts.push(format!(
                    "{} {} failed to start: {}",
                    program,
                    prefix_args.join(" "),
                    error
                )),
            }
        }

        bail!(
            "failed to invoke Python runtime bootstrap helper at {}:\n{}",
            script_path.display(),
            attempts.join("\n")
        );
    })();
    let _ = fs::remove_file(&script_path);
    result
}

fn try_bootstrap_model(model_path: &Path, variant: &str) -> Result<()> {
    if let Some(parent) = model_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let script_path = write_embedded_tooling_script("model")
        .context("failed to prepare embedded Python model bootstrap helper")?;
    let result = (|| {
        let mut attempts = Vec::new();

        for (program, prefix_args) in python_launchers() {
            let mut cmd = Command::new(program);
            cmd.args(prefix_args)
                .arg(&script_path)
                .arg("download-model")
                .arg("--variant")
                .arg(variant)
                .arg("--output")
                .arg(model_path);

            match cmd.status() {
                Ok(status) if status.success() => return Ok(()),
                Ok(status) => attempts.push(format!(
                    "{} {} exited with status {}",
                    program,
                    prefix_args.join(" "),
                    status
                )),
                Err(error) => attempts.push(format!(
                    "{} {} failed to start: {}",
                    program,
                    prefix_args.join(" "),
                    error
                )),
            }
        }

        bail!(
            "failed to invoke Python model bootstrap helper at {}:\n{}",
            script_path.display(),
            attempts.join("\n")
        );
    })();
    let _ = fs::remove_file(&script_path);
    result
}

fn runtime_dir(whisper_cli_path: &Path) -> Result<&Path> {
    whisper_cli_path
        .parent()
        .context("whisper-cli path has no parent directory")
}

fn missing_runtime_files(runtime_dir: &Path) -> Vec<&'static str> {
    ["whisper.dll", "ggml.dll", "ggml-base.dll", "ggml-cpu.dll"]
        .into_iter()
        .filter(|name| !runtime_dir.join(name).exists())
        .collect()
}

fn known_model_variant(model_path: &Path) -> Option<&'static str> {
    let filename = model_path
        .file_name()?
        .to_string_lossy()
        .to_ascii_lowercase();
    match filename.as_str() {
        "ggml-tiny.en.bin" => Some("tiny.en"),
        "ggml-base.en.bin" => Some("base.en"),
        "ggml-small.en.bin" => Some("small.en"),
        "ggml-medium.en.bin" => Some("medium.en"),
        "ggml-large-v3.bin" => Some("large-v3"),
        _ => None,
    }
}

fn python_launchers() -> [(&'static str, &'static [&'static str]); 2] {
    [("py", &["-3"]), ("python", &[])]
}

#[cfg(target_arch = "x86")]
fn preferred_runtime_asset_name() -> &'static str {
    "whisper-bin-Win32.zip"
}

#[cfg(not(target_arch = "x86"))]
fn preferred_runtime_asset_name() -> &'static str {
    "whisper-bin-x64.zip"
}
