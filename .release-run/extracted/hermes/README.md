# Hermes

Windows-first push-to-talk speech-to-text app in Rust, using local `whisper.cpp` inference through `whisper-cli.exe`.

## Overview

`Hermes` is a tray-based Windows app for local voice dictation:

- Hold a hotkey to record
- Release to transcribe
- Print the transcript to the console
- Optionally paste/type the transcript into the focused app
- Run `whisper-cli` in CPU-only mode for simpler, predictable behavior

Current default behavior:

- Hold `F8` to record
- Release `F8` to transcribe
- Type the transcript into the active window when `type_output = true`

## Project Layout

- `src/main.rs`: entrypoint, CLI flags, logging setup
- `src/app.rs`: runtime loop and state transitions
- `src/audio/capture.rs`: microphone capture and resampling
- `src/input/hotkey.rs`: global hotkey listener
- `src/output/typing.rs`: clipboard paste and Unicode typing fallback
- `src/stt/engine.rs`: `whisper-cli` invocation and runtime/model bootstrap
- `src/platform/windows/settings_dialog.rs`: PowerShell-backed settings dialog
- `scripts/ptt_tooling.py`: build, diagnostics, packaging, startup helpers, model download
- `whisper-runtime/`: expected location for `whisper-cli.exe` and required DLLs

## Requirements

- Windows 10 or 11
- Python 3.10+
- A working microphone

For development from source:

- Rust toolchain with `cargo`

Required runtime files in `whisper-runtime/`:

- `whisper-cli.exe`
- `whisper.dll`
- `ggml.dll`
- `ggml-base.dll`
- `ggml-cpu.dll`

Default config path:

- `%APPDATA%\Hermes\Hermes\config\config.toml`

Default data directory:

- `%LOCALAPPDATA%\Hermes\Hermes\data\`

Default model path:

- `%LOCALAPPDATA%\Hermes\Hermes\data\models\ggml-medium.en.bin`

## Setup

### Quick start

If you are not developing Hermes, download the latest Windows zip from GitHub Releases.

If you already have `hermes.exe`, the normal setup is just:

1. Run `hermes.exe`.
2. Hermes creates `config.toml` automatically on first launch.
3. If `whisper-cli.exe` and its DLLs are missing, Hermes tries to download the runtime automatically on startup.
4. If the selected standard model is missing, Hermes can download it automatically when you transcribe for the first time or when you save settings.
5. Use the tray menu or `hermes.exe --settings` to adjust model, hotkey, and typing behavior.

For most users, that is enough. The main external requirement is that `py` or `python` is available on `PATH`, because Hermes uses an embedded Python helper for runtime and model downloads.

### Build from source

```powershell
cargo build --release
```

The executable will be written to:

```text
target\release\hermes.exe
```

You can also run directly from source:

```powershell
cargo run --release
```

### Manual runtime setup

Place `whisper-cli.exe` and its required DLLs in:

```text
whisper-runtime\
```

The app expects the runtime DLLs to live next to `whisper-cli.exe`.

If the runtime is missing, Hermes tries to download the official Windows
`whisper.cpp` release asset automatically by calling:

```powershell
python .\scripts\ptt_tooling.py ensure-runtime --runtime-dir .\target\release\whisper-runtime
```

You can also run that command yourself ahead of time.

### Manual model setup

Download the default model:

```powershell
python .\scripts\ptt_tooling.py download-model --variant medium.en
```

Or choose a different model:

```powershell
python .\scripts\ptt_tooling.py download-model --variant base.en
python .\scripts\ptt_tooling.py download-model --variant small.en
python .\scripts\ptt_tooling.py download-model --variant large-v3
```

You can also download a model from the Settings dialog:

- launch the app with `--settings` or use the tray menu
- choose a model variant in `Download Model`
- click `Download Selected Model`
- the dialog will update `model_path` after the download completes

### Verify the local setup

```powershell
python .\scripts\ptt_tooling.py verify-runtime
```

This checks:

- the runtime directory exists
- required DLLs are present
- the default model path exists

### Run diagnostics

```powershell
python .\scripts\ptt_tooling.py diagnose
```

Diagnostics print:

- config path
- model path
- `whisper-cli` path
- hotkey configuration
- inference mode
- language
- microphone availability

### Launch options

Foreground:

```powershell
cargo run --release
```

Or, if you already built the app:

```powershell
.\target\release\hermes.exe
```

Background with hidden console:

```powershell
cargo run --release -- --background
```

Or:

```powershell
.\target\release\hermes.exe --background
```

Settings dialog:

```powershell
cargo run --release -- --settings
```

Or:

```powershell
.\target\release\hermes.exe --settings
```

## Configuration

On first run, the app creates `config.toml` automatically.

Current settings:

- `hotkey.modifier`
- `hotkey.key`
- `model_path`
- `whisper_cli_path`
- `min_record_ms`
- `auto_punctuation`
- `type_output`
- `language`

Default values are defined in `src/config.rs`.

You can edit settings by:

- editing `config.toml` manually
- opening the tray menu and choosing `Settings`
- launching the settings dialog with `--settings`

The Settings dialog uses a model dropdown for standard Whisper variants and
stores the corresponding `model_path` internally. If the selected standard
model is missing, Hermes can download it automatically.

The settings dialog can also download model files through Python if `py` or `python` is available on `PATH`.

Supported hotkey modes in the current implementation:

- `none + f8`
- `rctrl + rshift`

Unsupported hotkey combinations currently fall back to `F8`.

Example config:

```toml
model_path = "C:\\Users\\<you>\\AppData\\Local\\Hermes\\Hermes\\data\\models\\ggml-medium.en.bin"
whisper_cli_path = "whisper-runtime\\whisper-cli.exe"
min_record_ms = 200
auto_punctuation = true
type_output = true
language = "en"

[hotkey]
modifier = "none"
key = "f8"
```

## Running

Run the app:

```powershell
cargo run --release
```

Run in background mode:

```powershell
cargo run --release -- --background
```

Open settings only:

```powershell
cargo run --release -- --settings
```

Run diagnostics:

```powershell
cargo run --release -- --diagnose
```

## Python Tooling

Show help:

```powershell
python .\scripts\ptt_tooling.py --help
```

Build release executable:

```powershell
python .\scripts\ptt_tooling.py build
```

Verify runtime and model:

```powershell
python .\scripts\ptt_tooling.py verify-runtime
```

Download the official whisper.cpp Windows runtime:

```powershell
python .\scripts\ptt_tooling.py ensure-runtime --runtime-dir .\target\release\whisper-runtime
```

Download a model:

```powershell
python .\scripts\ptt_tooling.py download-model --variant medium.en
```

Run diagnostics against the built executable:

```powershell
python .\scripts\ptt_tooling.py diagnose
```

Install a Startup shortcut that launches the app with `--background`:

```powershell
python .\scripts\ptt_tooling.py install-startup
```

Remove the Startup shortcut:

```powershell
python .\scripts\ptt_tooling.py remove-startup
```

Build and package a distributable folder and zip:

```powershell
python .\scripts\ptt_tooling.py package --zip
```

## Packaging

The packaging flow copies:

- `hermes.exe`
- the chosen runtime directory
- this README
- `scripts/ptt_tooling.py`
- startup helper scripts
- an optional bundled model

Packaged output is written under `dist/` when you run the packaging command.

Packaged installs resolve relative runtime paths from the directory containing `hermes.exe`, so the default `whisper_cli_path` works when `whisper-runtime\whisper-cli.exe` is shipped next to the app binary.

## Notes

- This project shells out to `whisper-cli.exe`; it does not link directly to `whisper.cpp`.
- Hermes always invokes `whisper-cli` with `-ng`, so inference runs in CPU-only mode.
- The app writes rolling logs under the local app-data directory.
- Settings are saved immediately, but some changes require restarting the app.
- The current codebase is Windows-only.

## Troubleshooting

If transcription fails immediately:

- confirm `whisper_cli_path` points to a real executable
- confirm the required DLLs are in the same directory as `whisper-cli.exe`
- confirm `model_path` points to an existing model file
- run `python .\scripts\ptt_tooling.py verify-runtime`
- run `python .\scripts\ptt_tooling.py diagnose`

If the app starts but no text appears:

- confirm your microphone is the default Windows input device
- confirm the target app accepts simulated paste or keyboard input
- keep the console visible and watch for runtime errors
