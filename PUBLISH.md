# Publish description (Cursor / GitHub)

Use the **one-liner** below as the repo description when saving or publishing this project in Cursor.

## One-liner

Local push-to-talk speech-to-text for Windows: hold Right Ctrl+Right Shift, speak, and Hermes transcribes with whisper.cpp and types into the app you selected.

## Short description

Hermes is a Windows tray app for offline voice dictation. It records while you hold a hotkey, transcribes locally with whisper.cpp (no cloud), plays a beep when recording starts, and pastes the result into the focused window — or streams words live while you speak. Works with editors, browsers, and terminals (Ctrl+Shift+V).

**Stack:** Rust, whisper-cli, CPU-only inference  
**License:** MIT — Copyright (c) 2026 John Vithoulkas. See [LICENSE](LICENSE).

## Suggested tags

`windows` `speech-to-text` `whisper` `push-to-talk` `rust` `offline` `dictation` `local-ai`

## Clone and run (for new users)

```powershell
git clone https://github.com/jvit1/hermes.git
cd hermes
.\setup.ps1
```

Then open a new terminal and run `hermes.exe` from any folder.

## Before publishing

- Do not commit `%APPDATA%` / `%LOCALAPPDATA%` config or downloaded models
- `whisper-runtime/` is gitignored; `setup.ps1` downloads it automatically
- Respect the MIT license: keep `LICENSE` and the copyright notice in copies
