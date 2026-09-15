# Hermes

**Local push-to-talk dictation for Windows.** Hold a hotkey, speak, and Hermes transcribes on your machine — then types the text into the window you selected. Default setup uses **[WhisperX](https://github.com/m-bain/whisperx)** with GPU when available; [whisper.cpp](https://github.com/ggml-org/whisper.cpp) remains available as a CPU fallback.

MIT licensed. Copyright (c) 2026 John Vithoulkas. See [LICENSE](LICENSE).

## What it does

- **Push-to-talk** — hold hotkey to record, release to transcribe
- **Fully local** — WhisperX (GPU) or whisper.cpp (CPU); no cloud API
- **Types for you** — pastes into the focused app (Ctrl+V, or Ctrl+Shift+V in terminals)
- **Live streaming** — optional word-by-word output while you still hold the hotkey
- **Audio cues** — beep when recording starts and stops
- **System tray** — runs in the background with a settings dialog

## Quick start

### Clone and set up (recommended for developers)

**Requirements:** Windows 10/11, [Rust](https://rustup.rs), Python 3.10+, microphone. **NVIDIA GPU recommended** for WhisperX default backend.

```powershell
git clone https://github.com/jvit1/hermes.git
cd hermes
.\setup.ps1
```

`setup.ps1` will:

1. Build `target\release\hermes.exe`
2. Create a WhisperX Python venv in `target\release\whisperx-runtime\` (GPU PyTorch when CUDA is detected)
3. Download the whisper.cpp runtime into `target\release\whisper-runtime\` (fallback backend)
4. Write config with `stt_backend = "whisperx"` and default model **`small`**
5. Add `target\release` to your **user PATH** so `hermes.exe` works from any folder
6. Run diagnostics

Open a **new terminal**, then:

```powershell
hermes.exe
```

Equivalent manual command:

```powershell
python .\scripts\ptt_tooling.py setup
```

Setup options:

```powershell
.\setup.ps1 --no-add-path                       # skip PATH update
python .\scripts\ptt_tooling.py setup --stt-backend whispercpp --model-variant base.en
python .\scripts\ptt_tooling.py setup --skip-build --skip-whisperx
python .\scripts\ptt_tooling.py ensure-whisperx # WhisperX runtime only
python .\scripts\ptt_tooling.py verify-whisperx
```

### From a release zip (no build)

1. Download the latest **Windows zip** from [GitHub Releases](https://github.com/jvit1/hermes/releases).
2. Unzip and run `hermes.exe`.
3. On first run, Hermes creates config and can download missing runtime/model files.
4. Click the app where you want text, hold **Right Ctrl + Right Shift**, speak, release.

## Recommended hotkey

Default in code is F8. For daily use (especially with Cursor/terminals), **Right Ctrl + Right Shift** avoids conflicting with copy/paste:

```toml
[hotkey]
modifier = "rctrl"
key = "rshift"
```

Also supported: `ctrl + shift` (any left/right), `none + f8`.

## Configuration

Config file: `%APPDATA%\Hermes\Hermes\config\config.toml`

| Setting | Purpose |
|---------|---------|
| `stt_backend` | `whisperx` (default after setup) or `whispercpp` |
| `whisperx_model` | WhisperX model: `tiny`, `base`, `small`, `medium`, `large-v2`, `large-v3` |
| `whisperx_python` | Path to WhisperX venv python (default beside exe) |
| `type_output` | Type/paste transcript into focused window |
| `stream_output` | Stream stable words while hotkey is held (**whisper.cpp only**) |
| `allow_terminal_output` | Allow dictation into terminals (uses Ctrl+Shift+V) |
| `model_path` | Path to ggml Whisper model (whisper.cpp backend) |
| `hotkey.modifier` / `hotkey.key` | Push-to-talk combo |

Example:

```toml
stt_backend = "whisperx"
whisperx_python = "whisperx-runtime\\venv\\Scripts\\python.exe"
whisperx_model = "small"
whisperx_device = "auto"
model_path = "C:\\Users\\<you>\\AppData\\Local\\Hermes\\Hermes\\data\\models\\ggml-base.en.bin"
whisper_cli_path = "whisper-runtime\\whisper-cli.exe"
min_record_ms = 200
auto_punctuation = true
type_output = true
stream_output = false
allow_terminal_output = false
language = "en"

[hotkey]
modifier = "rctrl"
key = "rshift"
```

### Backend comparison

| Backend | Hardware | Live streaming | Best for |
|---------|----------|----------------|----------|
| **WhisperX** (default) | GPU preferred | No (release-only in v1) | Better quality + faster on NVIDIA GPU |
| **whisper.cpp** | CPU only | Yes | Lightweight fallback, live streaming |

**Tips**

- Click the **destination window first**, then hold the hotkey.
- Terminals are ignored by default; set `allow_terminal_output = true` to dictate into Cursor terminal, Windows Terminal, etc.
- Restart Hermes after changing settings.

## CLI flags

```powershell
hermes.exe                 # tray app (console visible)
hermes.exe --background    # hide console
hermes.exe --settings      # settings dialog
hermes.exe --diagnose      # print setup diagnostics
```

## Project layout

```
src/                 Rust application
scripts/             Python tooling (models, packaging, diagnostics)
whisper-runtime/     whisper-cli.exe + DLLs
assets/              Settings dialog script
```

## Python tooling

```powershell
python .\scripts\ptt_tooling.py --help
python .\scripts\ptt_tooling.py download-model --variant base.en
python .\scripts\ptt_tooling.py ensure-runtime --runtime-dir .\target\release\whisper-runtime
python .\scripts\ptt_tooling.py package --zip
```

## Troubleshooting

**No text in the target app**

- Click the text field before holding the hotkey
- Confirm `type_output = true`
- For terminals: `allow_terminal_output = true`

**Transcription fails**

- Run `hermes.exe --diagnose`
- Confirm model file and `whisper-runtime\whisper-cli.exe` exist

**Terminal copy/paste broken**

- Use **left** Ctrl/Shift for terminal shortcuts
- Use **right** Ctrl/Shift only for Hermes push-to-talk

## Third-party components

- **[whisper.cpp](https://github.com/ggml-org/whisper.cpp)** — speech recognition runtime (`whisper-cli.exe` and DLLs)
- **Whisper models** — downloaded from Hugging Face (see `scripts/ptt_tooling.py`)

Hermes shells out to `whisper-cli`; it does not embed whisper.cpp source. Follow whisper.cpp's license for those binaries.

## License

Copyright (c) 2026 John Vithoulkas

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

See [LICENSE](LICENSE) for the full MIT license text.
