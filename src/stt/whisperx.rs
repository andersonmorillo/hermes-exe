use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use tracing::info;

use crate::config::AppConfig;
use crate::stt::engine::{
    cleanup_transcript_artifacts, normalize_transcript, python_launchers, write_wav_f32_16khz,
};
use crate::stt::engine::{DecodeOptions, Transcript, Transcriber};
use crate::tooling::write_embedded_tooling_script;

#[derive(Clone)]
pub struct WhisperXTranscriber {
    python_path: PathBuf,
    model: String,
    language: String,
    device: String,
    compute_type: String,
    model_dir: PathBuf,
    scratch_dir: PathBuf,
}

impl WhisperXTranscriber {
    pub fn new(config: &AppConfig) -> Result<Self> {
        let scratch_dir = config.data_dir().join("scratch");
        fs::create_dir_all(&scratch_dir)?;

        let python_path = config.resolved_whisperx_python_path();
        ensure_whisperx_runtime(&python_path)?;

        let model_dir = config.resolved_whisperx_model_dir();
        fs::create_dir_all(&model_dir)?;

        let (device, compute_type) =
            resolve_device_and_compute(&python_path, &config.whisperx_device, &config.whisperx_compute_type)?;

        Ok(Self {
            python_path,
            model: config.whisperx_model.clone(),
            language: config.whisperx_language.clone(),
            device,
            compute_type,
            model_dir,
            scratch_dir,
        })
    }
}

impl Transcriber for WhisperXTranscriber {
    fn transcribe(&self, pcm_mono_16khz: &[f32], options: &DecodeOptions) -> Result<Transcript> {
        if pcm_mono_16khz.is_empty() {
            return Ok(Transcript {
                text: String::new(),
                latency_ms: 0,
            });
        }

        let started = Instant::now();
        let nonce = rand_seed();
        let wav_path = self.scratch_dir.join(format!("ptt-{nonce}.wav"));
        write_wav_f32_16khz(&wav_path, pcm_mono_16khz)?;

        let language = if options.language.is_empty() {
            self.language.as_str()
        } else {
            options.language.as_str()
        };

        let result = self.run_whisperx(&wav_path, language);
        cleanup_transcript_artifacts(&wav_path);
        let text = result?;
        Ok(Transcript {
            text: normalize_transcript(&text),
            latency_ms: started.elapsed().as_millis(),
        })
    }
}

impl WhisperXTranscriber {
    fn run_whisperx(&self, wav_path: &Path, language: &str) -> Result<String> {
        let mut cmd = Command::new(&self.python_path);
        cmd.arg("-m")
            .arg("whisperx")
            .arg(wav_path)
            .arg("--model")
            .arg(&self.model)
            .arg("--language")
            .arg(language)
            .arg("--output_dir")
            .arg(&self.scratch_dir)
            .arg("--output_format")
            .arg("txt")
            .arg("--device")
            .arg(&self.device)
            .arg("--compute_type")
            .arg(&self.compute_type)
            .arg("--model_dir")
            .arg(&self.model_dir)
            .arg("--batch_size")
            .arg("8")
            .arg("--verbose")
            .arg("False");

        let output = cmd.output().with_context(|| {
            format!(
                "failed to execute WhisperX via {}",
                self.python_path.display()
            )
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!(
                "WhisperX failed (status: {}):\nstdout: {}\nstderr: {}",
                output.status,
                stdout,
                stderr
            );
        }

        let txt_path = transcript_txt_path(wav_path, &self.scratch_dir);
        if txt_path.exists() {
            return fs::read_to_string(&txt_path).with_context(|| {
                format!(
                    "failed to read WhisperX transcript at {}",
                    txt_path.display()
                )
            });
        }

        let json_path = wav_path.with_extension("json");
        if json_path.exists() {
            return parse_whisperx_json(&fs::read_to_string(&json_path)?);
        }

        bail!(
            "WhisperX completed but no transcript file was found (expected {} or {})",
            txt_path.display(),
            json_path.display()
        );
    }
}

pub fn resolve_device_and_compute(
    python_path: &Path,
    device: &str,
    compute_type: &str,
) -> Result<(String, String)> {
    let cuda_available = probe_cuda(python_path);
    let resolved_device = match device.trim().to_ascii_lowercase().as_str() {
        "auto" => {
            if cuda_available {
                "cuda".to_string()
            } else {
                "cpu".to_string()
            }
        }
        other => other.to_string(),
    };

    let resolved_compute = match compute_type.trim().to_ascii_lowercase().as_str() {
        "auto" => {
            if resolved_device == "cuda" {
                "float16".to_string()
            } else {
                "int8".to_string()
            }
        }
        other => other.to_string(),
    };

    Ok((resolved_device, resolved_compute))
}

pub fn probe_cuda(python_path: &Path) -> bool {
    if !python_path.exists() {
        return false;
    }

    Command::new(python_path)
        .args([
            "-c",
            "import torch; import sys; sys.exit(0 if torch.cuda.is_available() else 1)",
        ])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn transcript_txt_path(wav_path: &Path, output_dir: &Path) -> PathBuf {
    let stem = wav_path
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "transcript".to_string());
    output_dir.join(format!("{stem}.txt"))
}

fn parse_whisperx_json(raw: &str) -> Result<String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).context("failed to parse WhisperX JSON output")?;
    let segments = value
        .get("segments")
        .and_then(|segments| segments.as_array())
        .ok_or_else(|| anyhow!("WhisperX JSON output did not include segments"))?;

    let text = segments
        .iter()
        .filter_map(|segment| segment.get("text").and_then(|text| text.as_str()))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    Ok(text)
}

fn ensure_whisperx_runtime(python_path: &Path) -> Result<()> {
    if python_path.exists() {
        return Ok(());
    }

    let runtime_dir = python_path
        .parent()
        .and_then(|scripts| scripts.parent())
        .context("whisperx python path has no runtime directory")?;

    println!(
        "[runtime] WhisperX runtime missing, attempting automatic setup into {}",
        runtime_dir.display()
    );

    match try_bootstrap_whisperx(runtime_dir) {
        Ok(()) if python_path.exists() => {
            info!("WhisperX runtime ready at {}", runtime_dir.display());
            println!("[runtime] WhisperX runtime ready");
            Ok(())
        }
        Ok(()) => bail!(
            "automatic WhisperX setup completed, but python is still missing at {}",
            python_path.display()
        ),
        Err(error) => bail!(
            "WhisperX python not found at {}\nautomatic setup failed: {error:#}\nmanual fix: run `python scripts/ptt_tooling.py ensure-whisperx` from the Hermes repo.",
            python_path.display()
        ),
    }
}

fn try_bootstrap_whisperx(runtime_dir: &Path) -> Result<()> {
    fs::create_dir_all(runtime_dir)?;

    let script_path = write_embedded_tooling_script("whisperx")
        .context("failed to prepare embedded Python WhisperX bootstrap helper")?;
    let result = (|| {
        let mut attempts = Vec::new();

        for (program, prefix_args) in python_launchers() {
            let mut cmd = Command::new(program);
            cmd.args(prefix_args)
                .arg(&script_path)
                .arg("ensure-whisperx")
                .arg("--runtime-dir")
                .arg(runtime_dir);

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
            "failed to invoke Python WhisperX bootstrap helper at {}:\n{}",
            script_path.display(),
            attempts.join("\n")
        );
    })();
    let _ = fs::remove_file(&script_path);
    result
}

fn rand_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    (now as u64) ^ ((now >> 32) as u64)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{parse_whisperx_json, resolve_device_and_compute};

    #[test]
    fn parse_whisperx_json_joins_segment_text() {
        let raw = r#"{"segments":[{"text":" hello"},{"text":" world "}]}"#;
        assert_eq!(
            parse_whisperx_json(raw).expect("json should parse"),
            "hello world"
        );
    }

    #[test]
    fn resolve_auto_device_prefers_cpu_when_python_missing() {
        let (device, compute) =
            resolve_device_and_compute(Path::new("missing-python.exe"), "auto", "auto")
                .expect("resolution should succeed");
        assert_eq!(device, "cpu");
        assert_eq!(compute, "int8");
    }
}
