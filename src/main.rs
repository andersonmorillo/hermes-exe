mod app;
mod audio;
mod config;
mod input;
mod output;
mod platform;
mod stt;
mod tooling;

use std::path::Path;

use anyhow::Result;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

use crate::config::AppConfig;

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let run_in_background = args.iter().any(|arg| arg == "--background");
    let open_settings = args.iter().any(|arg| arg == "--settings");
    maybe_hide_console_window(run_in_background);

    let config = AppConfig::load_or_create_default()?;
    let logs_dir = config.logs_dir();
    let _log_guard = init_logging(logs_dir.as_path())?;

    let run_diagnostics = args.iter().any(|arg| arg == "--diagnose");
    if run_diagnostics {
        app::run_diagnostics(&config)?;
    } else if open_settings {
        app::open_settings_once()?;
    } else {
        app::run(config)?;
    }

    Ok(())
}

fn init_logging(logs_dir: &Path) -> Result<WorkerGuard> {
    std::fs::create_dir_all(logs_dir)?;
    let appender = tracing_appender::rolling::daily(logs_dir, "hermes.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    Ok(guard)
}

#[cfg(target_os = "windows")]
fn maybe_hide_console_window(should_hide: bool) {
    if !should_hide {
        return;
    }

    use windows::Win32::System::Console::GetConsoleWindow;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

    unsafe {
        let hwnd = GetConsoleWindow();
        if !hwnd.0.is_null() {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn maybe_hide_console_window(_should_hide: bool) {}
