# Spec: Live Stream Output to Focused Window

## Objective

Hermes currently transcribes audio in partial windows while the push-to-talk hotkey is held, but only types the full transcript into the **focused Windows window** when the hotkey is **released**. Users want text to appear in the app or text field they have selected **while they are still speaking**, similar to live dictation.

**User story:** As a Windows user dictating with Hermes, I click into the app I want (Notepad, browser, chat, etc.), hold the hotkey, speak, and see completed sentences appear in that window in near real time without waiting for release.

**Success looks like:**
- Completed sentence chunks appear in the focused window during an active recording session.
- Releasing the hotkey types only the remaining tail, not the full transcript again.
- Existing release-only behavior remains available and is the default.
- Settings and README document the new option.

## Assumptions

```
ASSUMPTIONS I'M MAKING:
1. Target platform remains Windows-only (existing Hermes scope).
2. Output continues to use the focused window at typing time (SendInput / clipboard paste), not a fixed app binding.
3. Sentence-level streaming is sufficient for v1; word-by-word streaming is out of scope.
4. `type_output = true` remains the master switch for typing into external apps.
5. New `stream_output = true` enables incremental typing during recording; default is false for backward compatibility.
6. Partial transcription timing and sentence-commit logic in `SentenceStreamingTranscriber` stay as-is; we only expose committed deltas.
7. Users accept that clicking to another window mid-dictation moves subsequent chunks to the new focus target.
→ Correct me now or we'll proceed with these.
```

## Tech Stack

- Language: Rust 2021
- Key crates: `windows`, `clipboard-win`, `cpal`, `rdev`, `tracing`, `serde`, `toml`
- STT: local `whisper-cli.exe` via existing `WhisperCliTranscriber`
- Settings UI: embedded PowerShell dialog (`assets/settings-dialog.ps1`)

## Commands

```powershell
# Format
cargo fmt

# Unit tests
cargo test --release

# Build
cargo build --release

# Run (console visible)
cargo run --release

# Run diagnostics
cargo run --release -- --diagnose

# Open settings
cargo run --release -- --settings

# CI-equivalent local check
cargo fmt --check && cargo test --release
```

## Project Structure

```
src/
  app.rs                 → Main loop; orchestrates hotkey, streaming, typing
  app/streaming.rs       → Partial transcription + sentence accumulator
  config.rs              → TOML config (add stream_output)
  output/typing.rs       → Clipboard paste + Unicode fallback
  platform/windows/      → Settings dialog bridge
assets/
  settings-dialog.ps1    → Settings UI checkbox for stream output
tasks/
  plan.md                → Implementation plan
  todo.md                → Ordered task checklist
SPEC-live-stream-output.md → This spec
README.md                → User-facing docs update
```

## Code Style

Follow existing Hermes conventions:

- Small focused modules; prefer extending existing types over new abstractions.
- `Result` + `anyhow::Context` for operational errors; `thiserror` where already used.
- Config fields use `serde` defaults; snake_case in TOML.
- Log user-visible failures with `tracing::error!` / `warn!`; print concise console hints where the app already does.
- Unit tests live beside logic in `#[cfg(test)] mod tests`.

Example delta API shape (illustrative):

```rust
impl SentenceStreamingTranscriber {
    /// Returns newly committed sentence text since the last drain, if any.
    pub fn take_newly_committed(&mut self) -> Option<String> { /* ... */ }
}
```

## Testing Strategy

| Level | Scope | Framework |
|-------|-------|-----------|
| Unit | Sentence accumulator deltas, no double-type remainder math | `cargo test` in `streaming.rs` |
| Unit | Config parse/default for `stream_output` | `cargo test` in `config.rs` |
| Manual | End-to-end dictation into Notepad | Human on Windows |
| Manual | Regression: release-only mode unchanged | Human on Windows |

**Coverage expectation:** All new pure logic paths covered by unit tests. No automated test for `SendInput` (OS-level); manual verification covers typing.

**Not in scope for automated tests:** Microphone capture, whisper-cli subprocess, tray UI.

## Boundaries

### Always
- Preserve existing release-only typing when `stream_output = false`.
- Keep `type_output` as the master enable for external typing.
- Avoid duplicating full transcript on hotkey release when chunks were already streamed.
- Run `cargo fmt --check` and `cargo test --release` before considering work done.

### Ask first
- Changing default of `stream_output` to `true` (breaking UX expectation).
- Word-level or sub-sentence streaming (different architecture).
- Targeting a specific HWND instead of focused window.
- Adding new dependencies.

### Never
- Remove or break existing hotkey / tray / settings flows.
- Type partial/uncommitted unstable text in v1 (only committed sentences).
- Commit secrets or machine-local paths into tracked config examples.

## Behavior Specification

### Config

```toml
type_output = true      # existing: enable typing into focused window
stream_output = false   # new: when true, type committed sentences during recording
```

| type_output | stream_output | Behavior |
|-------------|---------------|----------|
| false | false | Console/log only (current) |
| false | true | Treat as false (stream requires typing enabled) |
| true | false | Type full transcript on release (current default) |
| true | true | Type each committed sentence during hold; type tail on release |

### Streaming rules

1. During recording, after each successful partial decode, the accumulator may commit a completed sentence (existing logic).
2. When `stream_output && type_output`, newly committed sentence text is typed immediately via `TextTyper`.
3. Track cumulative typed committed text to compute release-time remainder.
4. On release, finalize transcription, append punctuation if configured, type only the suffix not yet typed.
5. Chunks typed during recording should include trailing space or punctuation consistent with final output formatting.

### Focus behavior

- Each `type_text` call sends input to whatever window is focused **at that moment**.
- Document that users should keep the target app focused for best results.

## Success Criteria

- [ ] With `type_output = true` and `stream_output = true`, holding F8 and speaking causes at least one sentence to appear in Notepad before F8 is released.
- [ ] Releasing F8 does not repeat text already streamed; only the tail is appended.
- [ ] With `stream_output = false`, behavior matches current release-only typing.
- [ ] Settings dialog exposes a "Stream output" checkbox; saving persists to config.
- [ ] README documents the option and focus caveat.
- [ ] `cargo test --release` passes on Windows.
- [ ] New unit tests cover delta drain and release remainder logic.

## Open Questions

1. **Default for `stream_output`:** Keep `false` (safer) or default `true` for new installs?
2. **Spacing between streamed sentences:** Single space between chunks, or rely on accumulator punctuation only?
3. **Settings label:** "Stream output while recording" vs "Live typing" — preference?

## Approval

| Phase | Status | Reviewer | Date |
|-------|--------|----------|------|
| Spec | Approved | Captain | 2026-09-11 |
| Plan | Approved | Captain | 2026-09-11 |
| Tasks | Approved | Captain | 2026-09-11 |
| Implement | Complete | Firstmate | 2026-09-11 |
