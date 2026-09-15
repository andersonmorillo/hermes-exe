# Tasks: WhisperX GPU Backend (Default STT Engine)

**Spec:** [`SPEC-whisperx-backend.md`](../SPEC-whisperx-backend.md)  
**Plan:** [`plan-whisperx-backend.md`](plan-whisperx-backend.md)

Implement in order. Do not skip verification steps.

---

## Task 1: Add backend config schema (`backend-config`)

**Description:** Extend `AppConfig` with `stt_backend` and WhisperX settings. Preserve backward compatibility for existing configs.

**Acceptance criteria:**
- [ ] `stt_backend` deserializes; missing field defaults to `"whispercpp"`
- [ ] WhisperX fields have sensible defaults (`whisperx_model = "small"`, `whisperx_device = "auto"`, etc.)
- [ ] Path helpers resolve `whisperx_python` relative to exe dir
- [ ] Unit tests cover legacy TOML and new whisperx TOML round-trip

**Verification:**
- [ ] `cargo test --release config`
- [ ] `cargo fmt --check`

**Dependencies:** None

**Files:**
- `src/config.rs`

**Estimated scope:** Small

---

## Task 2: Python WhisperX runtime installer (`whisperx-runtime`)

**Description:** Add `ensure-whisperx` and `verify-whisperx` to `ptt_tooling.py`. Install venv, CUDA PyTorch when GPU detected, pin `whisperx`.

**Acceptance criteria:**
- [ ] `python scripts/ptt_tooling.py ensure-whisperx` creates `whisperx-runtime/venv`
- [ ] `verify-whisperx` prints CUDA availability and exits 0 on captain machine
- [ ] `whisperx-runtime/` added to `.gitignore`
- [ ] Clear error when Python missing or pip install fails

**Verification:**
- [ ] Run ensure + verify on Windows with NVIDIA GPU
- [ ] `venv\Scripts\python -c "import whisperx; import torch; print(torch.cuda.is_available())"`

**Dependencies:** None (parallel with Task 1)

**Files:**
- `scripts/ptt_tooling.py`
- `.gitignore`

**Estimated scope:** Medium

---

## Checkpoint A
- [ ] Config tests green
- [ ] WhisperX venv installs and CUDA probe true on captain machine

---

## Task 3: STT factory and module layout (`backend-config`)

**Description:** Add `src/stt.rs` factory `create_transcriber(config) -> Box<dyn Transcriber + Send + Sync>` routing whispercpp vs whisperx.

**Acceptance criteria:**
- [ ] Factory returns whispercpp transcriber when `stt_backend = "whispercpp"`
- [ ] Factory returns error for unknown backend string
- [ ] Existing whispercpp tests unchanged

**Verification:**
- [ ] `cargo test --release`
- [ ] Factory unit test with mock/minimal config

**Dependencies:** Task 1

**Files:**
- `src/stt.rs` (new or extend)
- `src/stt/engine.rs`
- `src/lib.rs` or module tree updates

**Estimated scope:** Small

---

## Task 4: WhisperX transcriber implementation (`whisperx-transcriber`)

**Description:** Implement `WhisperXTranscriber` in `src/stt/whisperx.rs`: WAV write, GPU/CPU device resolution, subprocess invoke, txt parse, bootstrap via embedded tooling.

**Acceptance criteria:**
- [ ] `transcribe()` returns normalized text from fixture WAV (manual or integration test)
- [ ] Auto device: CUDA + float16 when probe succeeds; else CPU + int8
- [ ] Actionable error when venv/CLI missing (mentions ensure-whisperx)
- [ ] Unit tests for output parsing and device resolution

**Verification:**
- [ ] `cargo test --release whisperx`
- [ ] Manual transcribe of short WAV on GPU (< 3s model load excluded for v1)

**Dependencies:** Tasks 2, 3

**Files:**
- `src/stt/whisperx.rs` (new)
- `src/stt/engine.rs` (shared WAV helper extraction if needed)

**Estimated scope:** Medium

---

## Checkpoint B
- [ ] Factory + WhisperX transcriber compile and unit tests pass
- [ ] Standalone whisperx CLI transcribes sample WAV on GPU

---

## Task 5: Refactor streaming for generic Transcriber (`backend-wiring`)

**Description:** Change `SentenceStreamingTranscriber` to accept `Box<dyn Transcriber + Send + 'static>` instead of `WhisperCliTranscriber`.

**Acceptance criteria:**
- [ ] Streaming worker thread owns boxed transcriber
- [ ] Existing streaming unit tests pass with whispercpp backend
- [ ] No regression in `take_live_stream_delta` behavior

**Verification:**
- [ ] `cargo test --release streaming`

**Dependencies:** Task 3

**Files:**
- `src/app/streaming.rs`

**Estimated scope:** Small–medium

---

## Task 6: Wire app, streaming guard, diagnostics (`backend-wiring`)

**Description:** Use factory in `app.rs`; disable `stream_output` when `stt_backend = "whisperx"` with one info log; extend `--diagnose` for WhisperX + CUDA.

**Acceptance criteria:**
- [ ] App starts with `stt_backend = "whisperx"` after setup
- [ ] PTT release transcribes via WhisperX on GPU
- [ ] `stream_output = true` + whisperx → streaming off, release-only still works
- [ ] `--diagnose` reports backend, python path, CUDA status

**Verification:**
- [ ] `cargo build --release`
- [ ] Manual PTT in Notepad (5s phrase)
- [ ] `hermes.exe --diagnose`

**Dependencies:** Tasks 4, 5

**Files:**
- `src/app.rs`
- `src/main.rs` (diagnose output)

**Estimated scope:** Medium

---

## Checkpoint C
- [ ] Captain confirms GPU transcription faster/better than medium.en CPU
- [ ] whispercpp fallback still works when config switched

---

## Task 7: Setup integration and config seeding (`setup-docs`)

**Description:** Update `setup` command and `setup.ps1` to default `--stt-backend whisperx`, run ensure-whisperx, keep whisper.cpp with `--skip-whispercpp` opt-out, write default config.

**Acceptance criteria:**
- [ ] `.\setup.ps1` installs both runtimes by default
- [ ] Fresh config has `stt_backend = "whisperx"`, `whisperx_model = "small"`
- [ ] `--skip-whisperx` and `--stt-backend whispercpp` work

**Verification:**
- [ ] Run setup on clean venv dir (or `--skip-build`)
- [ ] Inspect generated config TOML

**Dependencies:** Tasks 2, 6

**Files:**
- `scripts/ptt_tooling.py`
- `setup.ps1`

**Estimated scope:** Small

---

## Task 8: Settings UI and documentation (`setup-docs`)

**Description:** Backend picker in settings dialog; README/PUBLISH backend comparison and GPU requirements.

**Acceptance criteria:**
- [ ] Settings saves `stt_backend` and appropriate model field
- [ ] README documents WhisperX default, GPU requirement, streaming limitation
- [ ] PUBLISH.md mentions dual-backend setup

**Verification:**
- [ ] `hermes.exe --settings` toggle backends
- [ ] Read-through README

**Dependencies:** Task 7

**Files:**
- `assets/settings-dialog.ps1`
- `src/platform/windows/settings_dialog.rs`
- `README.md`
- `PUBLISH.md`

**Estimated scope:** Small–medium

---

## Checkpoint D (Definition of Done)
- [ ] All tasks complete
- [ ] Spec success criteria met
- [ ] Captain manual sign-off on Windows GPU machine
- [ ] Optional: revert captain config to whisperx + small after testing

---

## Post-implementation (captain machine)
- [ ] Switch `%APPDATA%\Hermes\Hermes\config\config.toml` to `stt_backend = "whisperx"` OR re-run `.\setup.ps1`
- [ ] Revert `model_path` from medium.en if still set (whispercpp field unused when on whisperx)
