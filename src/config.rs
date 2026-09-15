use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub hotkey: HotkeyConfig,
    pub stt_backend: String,
    pub model_path: PathBuf,
    pub whisper_cli_path: PathBuf,
    pub whisperx_python: PathBuf,
    pub whisperx_model: String,
    pub whisperx_language: String,
    pub whisperx_device: String,
    pub whisperx_compute_type: String,
    pub whisperx_model_dir: PathBuf,
    pub min_record_ms: u64,
    pub auto_punctuation: bool,
    pub type_output: bool,
    pub stream_output: bool,
    pub allow_terminal_output: bool,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub modifier: String,
    pub key: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        let dirs = app_dirs();
        let default_models_dir = dirs.data_local_dir().join("models");
        let model_path = default_models_dir.join("ggml-medium.en.bin");

        Self {
            hotkey: HotkeyConfig {
                modifier: "none".to_string(),
                key: "f8".to_string(),
            },
            stt_backend: "whispercpp".to_string(),
            model_path,
            whisper_cli_path: default_whisper_cli_path(),
            whisperx_python: default_whisperx_python_path(),
            whisperx_model: "small".to_string(),
            whisperx_language: "en".to_string(),
            whisperx_device: "auto".to_string(),
            whisperx_compute_type: "auto".to_string(),
            whisperx_model_dir: PathBuf::new(),
            min_record_ms: 200,
            auto_punctuation: true,
            type_output: true,
            stream_output: false,
            allow_terminal_output: false,
            language: "en".to_string(),
        }
    }
}

impl AppConfig {
    pub fn load_or_create_default() -> Result<Self> {
        let config_path = Self::config_path();
        if config_path.exists() {
            let content = fs::read_to_string(&config_path).with_context(|| {
                format!("failed to read config file at {}", config_path.display())
            })?;
            let mut cfg: Self = toml::from_str(&content)
                .with_context(|| "failed to parse config TOML".to_string())?;
            if cfg.migrate_whisper_cli_path() {
                if let Err(err) = cfg.save() {
                    eprintln!("warning: failed to persist migrated whisper_cli_path: {err:#}");
                }
            }
            return Ok(cfg);
        }

        let default_cfg = Self::default();
        default_cfg.save()?;
        Ok(default_cfg)
    }

    pub fn config_path() -> PathBuf {
        app_dirs().config_dir().join("config.toml")
    }

    pub fn logs_dir(&self) -> PathBuf {
        app_dirs().data_local_dir().to_path_buf()
    }

    pub fn data_dir(&self) -> PathBuf {
        app_dirs().data_local_dir().to_path_buf()
    }

    pub fn resolved_whisper_cli_path(&self) -> PathBuf {
        resolve_app_relative_path(&self.whisper_cli_path)
    }

    pub fn resolved_whisperx_python_path(&self) -> PathBuf {
        resolve_app_relative_path(&self.whisperx_python)
    }

    pub fn resolved_whisperx_model_dir(&self) -> PathBuf {
        if self.whisperx_model_dir.as_os_str().is_empty() {
            self.data_dir().join("whisperx-models")
        } else {
            resolve_app_relative_path(&self.whisperx_model_dir)
        }
    }

    pub fn uses_whisperx(&self) -> bool {
        self.stt_backend.eq_ignore_ascii_case("whisperx")
    }

    pub fn uses_whispercpp(&self) -> bool {
        self.stt_backend.is_empty() || self.stt_backend.eq_ignore_ascii_case("whispercpp")
    }

    pub fn effective_stream_output(&self) -> bool {
        self.stream_output && self.uses_whispercpp()
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path();
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let toml = toml::to_string_pretty(self)?;
        fs::write(config_path, toml)?;
        Ok(())
    }

    fn migrate_whisper_cli_path(&mut self) -> bool {
        let resolved = self.resolved_whisper_cli_path();
        if resolved.exists() {
            if is_legacy_default_whisper_cli_path(&self.whisper_cli_path) {
                let packaged_path = default_whisper_cli_path();
                if resolve_app_relative_path(&packaged_path).exists() {
                    self.whisper_cli_path = packaged_path;
                    return true;
                }
            }
            return false;
        }

        let candidates = whisper_cli_candidates();
        if let Some(path) = candidates
            .into_iter()
            .find(|p| resolve_app_relative_path(p).exists())
        {
            self.whisper_cli_path = path;
            return true;
        }

        false
    }
}

fn app_dirs() -> ProjectDirs {
    ProjectDirs::from("dev", "Hermes", "Hermes")
        .expect("valid project directories must be available on Windows")
}

fn whisper_cli_candidates() -> Vec<PathBuf> {
    vec![
        default_whisper_cli_path(),
        PathBuf::from("whisper-cli.exe"),
        absolute_packaged_whisper_cli_path(),
        current_dir_whisper_cli_path(),
    ]
}

pub fn resolve_app_relative_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    if let Some(exe_dir) = current_exe_dir() {
        return exe_dir.join(path);
    }

    if let Ok(cwd) = std::env::current_dir() {
        return cwd.join(path);
    }

    path.to_path_buf()
}

pub fn current_exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe_path| exe_path.parent().map(Path::to_path_buf))
}

fn default_whisper_cli_path() -> PathBuf {
    PathBuf::from("whisper-runtime").join("whisper-cli.exe")
}

fn default_whisperx_python_path() -> PathBuf {
    PathBuf::from("whisperx-runtime")
        .join("venv")
        .join("Scripts")
        .join("python.exe")
}

fn absolute_packaged_whisper_cli_path() -> PathBuf {
    current_exe_dir()
        .map(|dir| dir.join(default_whisper_cli_path()))
        .unwrap_or_else(default_whisper_cli_path)
}

fn current_dir_whisper_cli_path() -> PathBuf {
    std::env::current_dir()
        .map(|dir| dir.join(default_whisper_cli_path()))
        .unwrap_or_else(|_| default_whisper_cli_path())
}

fn is_legacy_default_whisper_cli_path(path: &Path) -> bool {
    path == Path::new("whisper-cli.exe")
}

#[cfg(test)]
mod tests {
    use super::AppConfig;

    #[test]
    fn app_config_ignores_removed_gpu_fields() {
        let config = toml::from_str::<AppConfig>(
            r#"
backend = "gpu_then_cpu"
model_path = "C:\\models\\ggml-medium.en.bin"
whisper_cli_path = "whisper-runtime\\whisper-cli.exe"
gpu_layers = 999
min_record_ms = 200
auto_punctuation = true
type_output = true
language = "en"

[hotkey]
modifier = "none"
key = "f8"
"#,
        )
        .expect("legacy config should parse");

        assert_eq!(config.language, "en");
        assert_eq!(config.min_record_ms, 200);
        assert!(!config.stream_output);
    }

    #[test]
    fn default_config_includes_stream_output_default() {
        let config = AppConfig::default();
        assert!(!config.stream_output);
        assert!(!config.allow_terminal_output);
    }

    #[test]
    fn default_config_omits_removed_gpu_fields() {
        let toml = toml::to_string(&AppConfig::default()).expect("default config should serialize");
        assert!(!toml.contains("\nbackend ="));
        assert!(!toml.contains("gpu_layers"));
        assert!(toml.contains("stt_backend"));
    }

    #[test]
    fn legacy_config_defaults_to_whispercpp_backend() {
        let config = toml::from_str::<AppConfig>(
            r#"
model_path = "C:\\models\\ggml-base.en.bin"
whisper_cli_path = "whisper-runtime\\whisper-cli.exe"
language = "en"

[hotkey]
modifier = "none"
key = "f8"
"#,
        )
        .expect("legacy config should parse");

        assert!(config.uses_whispercpp());
        assert!(!config.uses_whisperx());
        assert!(!config.effective_stream_output());
    }

    #[test]
    fn whisperx_config_round_trips() {
        let config = toml::from_str::<AppConfig>(
            r#"
stt_backend = "whisperx"
whisperx_model = "small"
whisperx_device = "auto"
whisperx_python = "whisperx-runtime\\venv\\Scripts\\python.exe"
model_path = "C:\\models\\ggml-base.en.bin"
whisper_cli_path = "whisper-runtime\\whisper-cli.exe"
language = "en"
stream_output = true

[hotkey]
modifier = "rctrl"
key = "rshift"
"#,
        )
        .expect("whisperx config should parse");

        assert!(config.uses_whisperx());
        assert!(!config.effective_stream_output());
        assert_eq!(config.whisperx_model, "small");
    }
}
