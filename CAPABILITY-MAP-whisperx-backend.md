# Capability Map: WhisperX Backend Support

Hermes today uses **whisper.cpp** (`whisper-cli.exe`) only. The request is to add **[WhisperX](https://github.com/m-bain/whisperx)** as a selectable transcription engine and make it the **default after setup**.

WhisperX is **not** a ggml model file. It is a Python/PyTorch pipeline (built on faster-whisper) that can add word-level alignment and optional speaker diarization. Hermes will invoke it as a subprocess, similar to `whisper-cli.exe`.

| Module id | Responsibility | Depends on |
|---|---|---|
| `backend-config` | Config schema: `stt_backend`, WhisperX settings, defaults, migration from whisper.cpp-only configs | — |
| `whisperx-runtime` | Python venv, `pip install whisperx`, model cache dir, `ensure-whisperx` / setup integration | — |
| `whisperx-transcriber` | Rust `Transcriber` impl: WAV → `whisperx` CLI → parse `.txt` / `.json` output | `backend-config`, `whisperx-runtime` |
| `backend-wiring` | App factory selects backend; streaming behavior per backend; diagnostics | `backend-config`, `whisperx-transcriber` |
| `setup-docs` | `setup.ps1`, README, settings UI backend picker, `--diagnose` updates | `whisperx-runtime`, `backend-config` |

**Build order:** `backend-config` → `whisperx-runtime` (parallel with transcriber stub) → `whisperx-transcriber` → `backend-wiring` → `setup-docs`

**Out of scope for v1 (unless captain expands):**
- Speaker diarization in typed output (needs Hugging Face token + pyannote)
- Word-level timestamps surfaced in the UI
- GPU auto-tuning beyond a simple `device` config flag
- Live streaming partial decode via WhisperX (CLI is file-batch oriented)
