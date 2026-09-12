# Tasks: Live Stream Output to Focused Window

**Spec:** [`SPEC-live-stream-output.md`](../SPEC-live-stream-output.md)  
**Plan:** [`plan.md`](plan.md)

Implement in order. Do not skip verification steps.

---

## Task 1: Add `stream_output` config field

**Description:** Extend `AppConfig` with a backward-compatible `stream_output` boolean defaulting to `false`.

**Acceptance criteria:**
- [ ] `stream_output` serializes/deserializes in `config.toml`
- [ ] Existing configs without the field load with `stream_output = false`
- [ ] Default config includes `stream_output = false`

**Verification:**
- [ ] `cargo test --release config` (or full `cargo test --release`)
- [ ] Manual: inspect generated default TOML after fresh load

**Dependencies:** None

**Files likely touched:**
- `src/config.rs`

**Estimated scope:** Small (1 file)

---

## Task 2: Expose committed-text deltas from streaming transcriber

**Description:** Add API on `SentenceStreamingTranscriber` to drain newly committed sentence text since the last drain, plus accessor for full committed prefix used at finalize.

**Acceptance criteria:**
- [ ] `take_newly_committed()` returns `Some(text)` only when new stable sentence(s) were committed since last drain
- [ ] Repeated calls without new commits return `None`
- [ ] Unit tests cover multi-sentence progression and empty/no-op drains

**Verification:**
- [ ] `cargo test --release streaming`
- [ ] All existing streaming tests still pass

**Dependencies:** Task 1 (none strictly, but keep config work landed first)

**Files likely touched:**
- `src/app/streaming.rs`

**Estimated scope:** Small (1 file)

---

## Checkpoint A
- [ ] `cargo fmt --check`
- [ ] `cargo test --release` green

---

## Task 3: Wire live typing in app main loop

**Description:** During recording, type committed deltas when `type_output && stream_output`. On release, type only the remainder not already sent.

**Acceptance criteria:**
- [ ] Live mode types at least one sentence before hotkey release (manual)
- [ ] Release does not duplicate already-streamed text
- [ ] Release-only mode (`stream_output = false`) behavior unchanged
- [ ] Typing errors surface via tray Error state and log

**Verification:**
- [ ] `cargo test --release`
- [ ] `cargo build --release`
- [ ] Manual: Notepad, hold F8, speak two sentences, confirm first appears before release; release adds only tail

**Dependencies:** Tasks 1, 2

**Files likely touched:**
- `src/app.rs`

**Estimated scope:** Medium (1 file, non-trivial logic)

---

## Task 4: Settings dialog for stream output

**Description:** Add "Stream output while recording" checkbox to settings UI and wire through Rust launcher.

**Acceptance criteria:**
- [ ] Checkbox reflects current config on open
- [ ] Saving writes `stream_output` to TOML
- [ ] CI PowerShell parse step still passes

**Verification:**
- [ ] `cargo test --release`
- [ ] Manual: `hermes.exe --settings`, toggle, save, confirm TOML
- [ ] Parse `assets/settings-dialog.ps1` (CI step)

**Dependencies:** Task 1

**Files likely touched:**
- `assets/settings-dialog.ps1`
- `src/platform/windows/settings_dialog.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint B
- [ ] Live streaming works in Notepad
- [ ] Release-only regression passes
- [ ] Settings persist correctly

---

## Task 5: README and spec status update

**Description:** Document the new option, config interaction, and focus behavior in README. Mark spec approval status when captain signs off.

**Acceptance criteria:**
- [ ] README lists `stream_output` with example TOML
- [ ] README explains `type_output` + `stream_output` matrix
- [ ] Focus caveat documented

**Verification:**
- [ ] Read-through for accuracy against implemented behavior
- [ ] `cargo fmt --check && cargo test --release`

**Dependencies:** Tasks 3, 4

**Files likely touched:**
- `README.md`
- `SPEC-live-stream-output.md` (approval table only, after captain review)

**Estimated scope:** Small (2 files)

---

## Checkpoint C (Definition of Done)
- [ ] All tasks above complete
- [ ] Success criteria in spec met
- [ ] Captain manual sign-off on Windows
