use std::fs;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::config::AppConfig;
use crate::tooling::write_embedded_tooling_script;

const SETTINGS_DIALOG_SCRIPT: &str = include_str!("../../../assets/settings-dialog.ps1");

pub fn open_settings_dialog(config: &AppConfig) -> Result<Option<AppConfig>> {
    let script_path = std::env::temp_dir().join("hermes-settings-dialog.ps1");
    let tooling_script_path = write_embedded_tooling_script("settings")?;
    fs::write(&script_path, SETTINGS_DIALOG_SCRIPT).with_context(|| {
        format!(
            "failed to write settings dialog script to {}",
            script_path.display()
        )
    })?;

    let output = (|| {
        Command::new("powershell.exe")
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(&script_path)
            .arg("-ToolingScriptPath")
            .arg(tooling_script_path.to_string_lossy().to_string())
            .arg("-ModelPath")
            .arg(config.model_path.to_string_lossy().to_string())
            .arg("-WhisperCliPath")
            .arg(config.whisper_cli_path.to_string_lossy().to_string())
            .arg("-Language")
            .arg(&config.language)
            .arg("-HotkeyModifier")
            .arg(&config.hotkey.modifier)
            .arg("-HotkeyKey")
            .arg(&config.hotkey.key)
            .arg("-MinRecordMs")
            .arg(config.min_record_ms.to_string())
            .arg("-AutoPunctuation")
            .arg(config.auto_punctuation.to_string())
            .arg("-TypeOutput")
            .arg(config.type_output.to_string())
            .arg("-StreamOutput")
            .arg(config.stream_output.to_string())
            .arg("-AllowTerminalOutput")
            .arg(config.allow_terminal_output.to_string())
            .output()
            .context("failed to launch settings dialog via PowerShell")
    })();
    let _ = fs::remove_file(&tooling_script_path);
    let _ = fs::remove_file(&script_path);
    let output = output?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            bail!(
                "settings dialog process exited with status {}",
                output.status
            );
        }
        bail!("settings dialog failed: {stderr}");
    }

    let stdout = String::from_utf8(output.stdout)
        .context("settings dialog returned non-UTF8 output on stdout")?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let updated =
        toml::from_str::<AppConfig>(trimmed).context("settings dialog returned invalid TOML")?;
    Ok(Some(updated))
}
