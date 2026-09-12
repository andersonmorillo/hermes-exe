#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(not(target_os = "windows"))]
compile_error!("Hermes currently supports Windows only.");
