# Plan: WhisperX GPU Backend (Default STT Engine)

**Spec:** [`SPEC-whisperx-backend.md`](../SPEC-whisperx-backend.md)  
**Capability map:** [`CAPABILITY-MAP-whisperx-backend.md`](../CAPABILITY-MAP-whisperx-backend.md)

## Summary

Add WhisperX as a second transcription backend invoked via a Python venv subprocess, make it the **default after `setup.ps1`**, and **prefer CUDA + float16** when an NVIDIA GPU is present. Keep whisper.cpp as a fallback backend for users who want CPU-only or live `stream_output`.

Hermes already has a `Transcriber` trait and embedded `ptt_tooling.py` bootstrap pattern. The main work is: config + factory, WhisperX transcriber, Python runtime installer, app/streaming refactor off the concrete `WhisperCliTranscriber` type, setup/diagnostics/docs.

## Dependency Graph

```
backend-config (stt_backend, whisperx_* fields, migration)
        │
        ├── whisperx-runtime (ptt_tooling: ensure-whisperx, CUDA torch, verify)
        │         │
        │         └── setup.ps1 / setup command (--stt-backend whisperx default)
        │
        ├── whisperx-transcriber (src/stt/whisperx.rs: subprocess + parse)
        │         │
        │         └── stt factory (create_transcriber → Box<dyn Transcriber>)
        │
        ├── backend-wiring
        │         ├── app.rs: factory, release transcribe path
        │         ├── streaming.rs: generic Transcriber in worker thread
        │         ├── stream_output guard (whispercpp only when streaming on)
        │         └── --diagnose: GPU, venv, whisperx CLI, model cache
        │
        └── setup-docs
                  ├── settings-dialog.ps1 (backend picker)
                  ├── README / PUBLISH updates
                  └── .gitignore whisperx-runtime/
```

**Build order (vertical slices):**

1. Config + factory stub  
2. Python `ensure-whisperx` (can test standalone)  
3. WhisperX transcriber + unit tests  
4. App wiring + streaming generic refactor  
5. Setup integration + diagnose  
6. Settings UI + docs  
7. Manual GPU PTT validation on captain machine  

## Module plans

### Module: `backend-config`

**Files:** `src/config.rs`, tests in same file

- Add `stt_backend: String` with serde default `"whispercpp"` for **migration** (existing installs unchanged).
- Add WhisperX fields: `whisperx_python`, `whisperx_model`, `whisperx_language`, `whisperx_device`, `whisperx_compute_type`, `whisperx_model_dir`.
- Defaults for **new** configs created by setup: `stt_backend = "whisperx"`, `whisperx_model = "small"`, `whisperx_device = "auto"`, `whisperx_compute_type = "auto"`.
- Path helpers: `resolved_whisperx_python()`, `resolved_whisperx_model_dir()`.
- `AppConfig::apply_setup_defaults(backend)` helper used by setup script writer (or setup writes TOML directly).

### Module: `whisperx-runtime`

**Files:** `scripts/ptt_tooling.py`, `.gitignore`

**`ensure-whisperx` command:**

1. Create `{repo_or_exe_dir}/whisperx-runtime/venv`.
2. Upgrade pip; install PyTorch with CUDA index when `nvidia-smi` succeeds:
   - `pip install torch torchaudio --index-url https://download.pytorch.org/whl/cu124` (pin after smoke test)
3. `pip install whisperx==3.8.6` (pinned).
4. Run probe: `venv\Scripts\python -c "import torch; print(torch.cuda.is_available())"`.
5. Optional warmup: download default model on setup (`--model small --language en` dry transcribe of 1s silence WAV or documented first-run).

**`verify-whisperx`:** venv exists, import whisperx, CUDA bool printed, CLI `--help`.

**Setup changes:**

- `setup` defaults to `--stt-backend whisperx`.
- Still runs `ensure-runtime` (whisper.cpp) unless `--skip-whispercpp`.
- Writes/updates config with whisperx defaults.
- Flags: `--skip-whisperx`, `--stt-backend whispercpp`, `--whisperx-model`.

### Module: `whisperx-transcriber`

**Files:** `src/stt/whisperx.rs`, `src/stt.rs` (mod + factory), `src/stt/engine.rs` (export trait)

**Behavior:**

1. Write scratch WAV (reuse existing helper from `engine.rs` — extract shared `write_wav_f32_16khz` if needed).
2. Resolve device/compute:
   - `auto` + CUDA available → `cuda` + `float16`
   - else → `cpu` + `int8`
3. Invoke:
   ```text
   {python} -m whisperx {wav} --model {model} --language {lang}
     --output_dir {scratch} --output_format txt
     --device {device} --compute_type {compute_type}
     --batch_size 8 --verbose False
   ```
   (No `--align`, no `--diarize` in v1.)
4. Parse `{stem}.txt`; normalize via existing `normalize_transcript`.
5. Bootstrap: if venv missing, call embedded `ptt_tooling.py ensure-whisperx` (mirror whisper runtime pattern).

**Unit tests:** fixture `.txt` parse; device resolution table; factory routing.

### Module: `backend-wiring`

**Files:** `src/app.rs`, `src/app/streaming.rs`, `src/main.rs` (diagnose)

**App changes:**

- Replace `WhisperCliTranscriber::new` with `create_transcriber(&config)?` returning `Box<dyn Transcriber + Send + Sync>` or an enum `BackendTranscriber { WhisperCpp(...), WhisperX(...) }`.
- **Streaming rule:** if `config.stream_output && config.stt_backend != "whispercpp"`, log warning and treat streaming as off (or auto-fallback to whisper.cpp for stream worker only — **rejected** for v1 complexity; simpler to disable streaming).
- Release path uses same boxed transcriber.
- `SentenceStreamingTranscriber::new` accepts `Box<dyn Transcriber + Send + 'static>` instead of concrete type; worker thread owns the box.

**Diagnostics (`--diagnose`):**

- Print active backend, whisperx python path, CUDA probe output, model name, model dir.
- Existing whisper.cpp checks when backend is whispercpp or dual-stack installed.

### Module: `setup-docs`

**Files:** `README.md`, `PUBLISH.md`, `assets/settings-dialog.ps1`, `src/platform/windows/settings_dialog.rs`, `setup.ps1`

- Settings: radio `whisper.cpp` / `WhisperX`; show ggml variant vs whisperx model dropdown based on selection.
- README: backend comparison table (CPU whisper.cpp vs GPU WhisperX), setup commands, streaming limitation.
- `.gitignore`: `whisperx-runtime/`

## Risks and mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| PyTorch+CUDA wheel mismatch | Setup fails on some drivers | Probe in ensure-whisperx; clear error; document driver min; CPU fallback |
| WhisperX cold start loads model each invoke | High latency | v1 accept; document; v2 sidecar out of scope |
| Large pip download (~2 GB) | Slow first setup | Progress messages; `--skip-whisperx` |
| Streaming regression | Duplicate/wrong live text | Disable streaming when whisperx; test whispercpp path |
| CI has no GPU | Tests fail | CPU-only unit tests in Rust; mark GPU verify manual |
| ffmpeg missing | WhisperX CLI fails | Check in verify-whisperx; document dependency |

## Verification checkpoints

### Checkpoint A — config + runtime (Tasks 1–2)
- `cargo test --release` green
- `python scripts/ptt_tooling.py ensure-whisperx` succeeds on captain machine
- `torch.cuda.is_available()` == True

### Checkpoint B — transcriber (Tasks 3–4)
- Unit tests pass
- Manual: transcribe 5s WAV via Rust diag hook or temporary test binary

### Checkpoint C — app integration (Tasks 5–6)
- `hermes.exe --diagnose` reports WhisperX + CUDA
- PTT phrase in Notepad with `stt_backend = whisperx`
- Latency acceptable vs medium.en CPU (captain sign-off)

### Checkpoint D — setup + docs (Tasks 7–8)
- Fresh `.\setup.ps1` → config whisperx, venv present
- Switch to whispercpp in settings still works
- README accurate

## Out of scope (v1)

- WhisperX live streaming / sidecar process
- `--align` and `--diarize`
- macOS / Linux
- In-process PyO3 embedding
- Automatic migration of existing installs to whisperx (opt-in via setup)

## Estimated effort

| Slice | Files | Sessions |
|-------|-------|----------|
| Config + factory | 2–3 | 1 |
| Python runtime | 1–2 | 1 |
| WhisperX transcriber | 2–3 | 1 |
| App + streaming refactor | 2–3 | 1–2 |
| Setup + diagnose + UI + docs | 4–5 | 1 |
| **Total** | **~12 files** | **~4–5 focused sessions** |

## Approval

- [x] Spec assumptions updated for GPU-first default
- [ ] Captain approves this plan
- [ ] Captain approves task list before implementation begins
