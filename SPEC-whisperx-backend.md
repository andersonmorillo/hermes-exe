# Spec: WhisperX Backend (Default STT Engine)

## Objective

Add **[WhisperX](https://github.com/m-bain/whisperx)** as a second speech-to-text engine in Hermes and make it the **default engine after `setup.ps1`**.

WhisperX improves transcription quality (batched Whisper + optional alignment) at the cost of heavier dependencies (Python venv, PyTorch, larger download) and higher latency than whisper.cpp. Hermes remains a **local, push-to-talk dictation app for Windows** — WhisperX replaces whisper.cpp as the default inference path, while whisper.cpp stays available for users who want a lighter/faster stack.

**User stories**

1. **New clone:** `git clone` → `.\setup.ps1` → open new terminal → `hermes.exe` works with WhisperX without manual Python package steps.
2. **Existing user:** Config without `stt_backend` keeps current whisper.cpp behavior until they opt in or re-run setup.
3. **Power user:** Settings or config can switch back to `whispercpp` and pick ggml model variants as today.

**Success looks like**

- `stt_backend = "whisperx"` is written by default on fresh setup.
- Push-to-talk release transcription returns text via WhisperX on a short utterance (< 10 s).
- `hermes.exe --diagnose` reports WhisperX venv, CLI, model, and device readiness.
- whisper.cpp path still works when `stt_backend = "whispercpp"`.
- README documents both backends, tradeoffs, and setup commands.

## Assumptions

```
ASSUMPTIONS I'M MAKING:
1. Windows 10/11 only (existing Hermes scope).
2. WhisperX is invoked via CLI subprocess (`whisperx audio.wav ...`), not embedded Python in Rust.
3. v1 uses transcription-only flags (no `--align`, no `--diarize`) for acceptable PTT latency.
4. Default WhisperX model is `small` on GPU setups (quality/speed balance); `base` documented for low-VRAM; configurable via `whisperx_model`.
5. **GPU-first:** `whisperx_device = "auto"` prefers CUDA when available (`--device cuda --compute_type float16`); falls back to CPU + int8 only when CUDA is missing.
6. Live `stream_output` stays whisper.cpp-only in v1; when `stt_backend = "whisperx"`, Hermes auto-disables streaming and logs once (release-only transcription).
7. Python 3.10+ and pip remain setup prerequisites (same as today).
8. Existing `%LOCALAPPDATA%` ggml models are not deleted; they remain for whisper.cpp users.
→ Correct me now or we'll proceed with these.
```

## Tech Stack

| Layer | Choice |
|---|---|
| App | Rust 2021 (unchanged) |
| whisper.cpp backend | Existing `whisper-cli.exe` subprocess |
| WhisperX backend | Python venv + `whisperx` CLI (PyPI `whisperx`, PyTorch) |
| Setup tooling | `scripts/ptt_tooling.py` extended with `ensure-whisperx` |
| Config | TOML (`config.toml`) |

## Commands

```powershell
# Full setup (WhisperX default after this spec)
git clone https://github.com/jvit1/hermes.git
cd hermes
.\setup.ps1

# Setup with explicit backend
python .\scripts\ptt_tooling.py setup --stt-backend whisperx
python .\scripts\ptt_tooling.py setup --stt-backend whispercpp   # legacy path

# Install / verify WhisperX runtime only
python .\scripts\ptt_tooling.py ensure-whisperx
python .\scripts\ptt_tooling.py verify-whisperx

# Run
hermes.exe
hermes.exe --diagnose
hermes.exe --settings

# Tests
cargo fmt --check
cargo test --release
```

## Project Structure

```
src/
  stt/
    engine.rs              # Transcriber trait + WhisperCliTranscriber (existing)
    whisperx.rs            # NEW: WhisperXTranscriber subprocess impl
    mod.rs                 # backend factory: create_transcriber(config)
  config.rs                # stt_backend, whisperx_* fields
scripts/
  ptt_tooling.py           # ensure-whisperx, setup --stt-backend, verify-whisperx
assets/
  settings-dialog.ps1      # backend radio + whisperx model dropdown
setup.ps1                  # default backend whisperx
SPEC-whisperx-backend.md   # this file
CAPABILITY-MAP-whisperx-backend.md
tasks/plan.md              # implementation plan (Phase 2)
tasks/todo.md              # task list (Phase 3)
```

## Config Schema (proposed)

File: `%APPDATA%\Hermes\Hermes\config\config.toml`

```toml
# STT engine: "whisperx" (default for new setups) | "whispercpp"
stt_backend = "whisperx"

# --- whisper.cpp (when stt_backend = "whispercpp") ---
model_path = "...\\ggml-base.en.bin"
whisper_cli_path = "whisper-runtime\\whisper-cli.exe"

# --- WhisperX (when stt_backend = "whisperx") ---
whisperx_python = "whisperx-runtime\\venv\\Scripts\\python.exe"   # relative to exe dir
whisperx_model = "base"          # tiny | base | small | medium | large-v2 | large-v3
whisperx_language = "en"
whisperx_device = "auto"         # auto | cuda | cpu
whisperx_compute_type = "auto"   # auto | float16 | int8
whisperx_model_dir = ""          # empty = %LOCALAPPDATA%\Hermes\Hermes\data\whisperx-models

# Existing fields unchanged: hotkey, type_output, stream_output, allow_terminal_output, ...
```

**Migration:** Missing `stt_backend` → treat as `"whispercpp"` (no behavior change for existing installs).

## Code Style

Follow existing Hermes patterns: `anyhow` errors, `tracing` logs, subprocess via `std::process::Command`, path resolution relative to exe dir, bootstrap via embedded `ptt_tooling.py` when files missing.

Example factory (sketch):

```rust
pub fn create_transcriber(config: &AppConfig) -> Result<Box<dyn Transcriber>> {
    match config.stt_backend.as_str() {
        "whisperx" => Ok(Box::new(WhisperXTranscriber::new(config)?)),
        "whispercpp" | "" => Ok(Box::new(WhisperCliTranscriber::new(config)?)),
        other => bail!("unknown stt_backend: {other}"),
    }
}
```

## WhisperX invocation (v1)

Per utterance (on hotkey release, and optionally for streaming — disabled in v1):

```text
{whisperx_python} -m whisperx {wav_path}
  --model {whisperx_model}
  --language {whisperx_language}
  --output_dir {scratch_dir}
  --output_format txt
  --device {cuda|cpu}
  --compute_type {float16|int8}
  --no_align          # if supported; otherwise omit align flags only
  --verbose False
```

Parse transcript from `{scratch_dir}/{stem}.txt` or JSON fallback.

**Bootstrap (`ensure-whisperx`):**

1. Create `{exe_dir}/whisperx-runtime/venv` if missing.
2. `pip install whisperx` (pin version in script, e.g. `whisperx==3.8.6`).
3. Pre-download default model on setup: `whisperx --model base ...` dry run or documented first-run cache.
4. Verify `python -m whisperx --help` succeeds.

## Testing Strategy

| Level | What |
|---|---|
| Unit (Rust) | Config defaults/migration; transcript parse from fixture `.txt`/`.json`; backend factory routing |
| Integration (manual) | `setup.ps1` on clean machine; `--diagnose`; PTT phrase typed into Notepad |
| Regression | Existing whisper.cpp tests unchanged; `stt_backend = whispercpp` golden path |

No CI GPU requirement — CPU `--device cpu` path must pass in CI or be marked manual-only with `verify-whisperx` skip flag.

## Boundaries

**Always**

- Keep whisper.cpp backend working.
- Fail with actionable errors when venv/CLI/model missing (offer `ensure-whisperx` hint).
- Never commit venv, PyTorch wheels, or downloaded models.

**Ask first**

- Pinning PyTorch/CUDA wheel index URLs (platform-specific).
- Enabling `--align` or `--diarize` by default (latency + HF token).
- Breaking change: forcing existing users to WhisperX on app upgrade.

**Never**

- Cloud STT APIs.
- Commit secrets / Hugging Face tokens.
- Block app startup indefinitely on model download (bootstrap on first transcribe is OK with progress log).

## Success Criteria

- [ ] Fresh `.\setup.ps1` sets `stt_backend = "whisperx"` and installs venv.
- [ ] `hermes.exe --diagnose` passes for WhisperX backend.
- [ ] 5-second English PTT utterance transcribed and typed locally.
- [ ] Config switch to `whispercpp` restores whisper-cli behavior.
- [ ] README + settings document both backends.
- [ ] `stream_output = true` with WhisperX: either disabled with UI note or documented limitation.

## Captain decisions (2026-03-12)

| Question | Decision |
|---|---|
| Default model | **`small`** for GPU default setup; `base` fallback documented |
| Streaming | **Disable** live typing when WhisperX is active (v1) |
| GPU | **Auto-detect CUDA**, prefer GPU + float16 |
| Alignment / diarization | **Out of scope** v1 |
| Setup scope | **Dual stack:** WhisperX default; whisper.cpp still installed for fallback / streaming |

## Approval

- [x] Captain confirmed NVIDIA GPU available
- [x] Captain approved WhisperX as default backend with GPU
- [ ] Captain approved this plan (`tasks/plan-whisperx-backend.md`)
- [ ] Captain approved task list (`tasks/todo-whisperx-backend.md`)

## Risks

| Risk | Mitigation |
|---|---|
| High latency on CPU | Default `base`; document GPU; keep whispercpp fallback |
| Large pip install | Progress in setup; optional `--skip-whisperx` |
| Streaming regression | Explicit backend guard in `app.rs` |
| PyTorch/CUDA fragility | Pin versions; CPU fallback; diagnose command |
