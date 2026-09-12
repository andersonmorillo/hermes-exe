use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

const EMBEDDED_TOOLING_SCRIPT: &str = include_str!("../scripts/ptt_tooling.py");

pub fn write_embedded_tooling_script(prefix: &str) -> Result<PathBuf> {
    let script_path =
        std::env::temp_dir().join(format!("hermes-{prefix}-{}-tooling.py", std::process::id()));
    fs::write(&script_path, EMBEDDED_TOOLING_SCRIPT).with_context(|| {
        format!(
            "failed to write embedded tooling script to {}",
            script_path.display()
        )
    })?;
    Ok(script_path)
}
