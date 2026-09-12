# Plan: Live Stream Output to Focused Window

**Spec:** [`SPEC-live-stream-output.md`](../SPEC-live-stream-output.md)

## Summary

Expose committed sentence deltas from the existing `SentenceStreamingTranscriber`, type them during recording when `stream_output` is enabled, and type only the untyped remainder on hotkey release. Add config, settings UI, tests, and README updates. No changes to whisper inference, hotkey capture, or audio pipeline.

## Dependency Graph

```
config.stream_output (TOML + defaults)
        │
        ├── streaming.rs: take_newly_committed() API
        │         │
        │         └── app.rs: live typing loop + release remainder
        │                   │
        │                   └── (uses existing TextTyper — no change required)
        │
        ├── settings-dialog.ps1 + settings_dialog.rs
        │
        └── README.md
```

**Build order:** config → streaming API → app loop → settings → docs → manual QA

## Components

### 1. Config (`src/config.rs`)

- Add `pub stream_output: bool` with `#[serde(default)]` default `false`.
- Include in `Default for AppConfig`.
- Add unit test: legacy configs without the field deserialize with `stream_output = false`.

### 2. Streaming delta API (`src/app/streaming.rs`)

- Track `last_drained_committed_len` or equivalent inside `SentenceStreamingTranscriber`.
- Add `take_newly_committed(&mut self) -> Option<String>` returning text committed since last call.
- Add `committed_text(&self) -> &str` (or internal accessor) for release remainder calculation.
- Unit tests:
  - Multiple `observe` commits produce sequential deltas.
  - `take_newly_committed` returns `None` when nothing new.
  - After finalize path, remainder math excludes already-committed typed text.

### 3. App orchestration (`src/app.rs`)

- Pass `&TextTyper` and stream flags into the recording loop (already has both).
- After `stream.tick(session)`, if `config.type_output && config.stream_output`:
  - Call `take_newly_committed()`; for each non-empty delta, `typer.type_text` with appropriate leading space if not first chunk.
  - Track `typed_prefix` string synced with what was sent to the focused window.
- On hotkey release (existing finalize block):
  - Compute `cleaned` final transcript as today.
  - If streaming was active, type `suffix = cleaned.strip_prefix(typed_prefix)` or equivalent overlap-safe remainder helper.
  - If streaming was inactive, keep current full `type_text(&cleaned)` path.
- Handle errors: failed mid-stream type sets tray to Error but does not crash the loop.

### 4. Settings UI

- `assets/settings-dialog.ps1`: add checkbox "Stream output while recording", serialize `stream_output`.
- `src/platform/windows/settings_dialog.rs`: pass new flag into PowerShell args (mirror `TypeOutput` pattern).

### 5. Documentation

- `README.md`: document `stream_output`, interaction with `type_output`, focus caveat, example config snippet.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Double-typing on release | Duplicate text in target app | Track typed prefix; unit test remainder; manual Notepad check |
| Transcript revision invalidates earlier chunk | Wrong words left in target | v1 only types stable committed sentences (existing accumulator gate) |
| Clipboard churn during stream | User clipboard overwritten repeatedly | Same as current single paste; document in README |
| Focus changes mid-dictation | Text splits across windows | Document expected behavior; no HWND pinning in v1 |
| Settings script parse failure in CI | CI break | Run existing PowerShell parse step after editing dialog |

## Verification Checkpoints

### Checkpoint A (after Tasks 1–2)
- `cargo test --release` passes
- Config parses with and without new field

### Checkpoint B (after Tasks 3–4)
- Manual: Notepad + `stream_output = true` shows live sentences
- Manual: `stream_output = false` unchanged
- Settings checkbox persists value

### Checkpoint C (after Task 5)
- README accurate
- Full CI command sequence passes locally on Windows

## Out of Scope (v1)

- Word-level streaming
- Pinning output to a specific window handle
- Undo/revision of already-typed text when whisper revises a partial
- Linux/macOS ports

## Estimated Effort

| Task group | Files | Size |
|------------|-------|------|
| Config + streaming API | 2 | Small |
| App loop wiring | 1 | Medium |
| Settings + docs | 3 | Small |
| **Total** | **~6 files** | **~1 focused session** |

## Approval

- [x] Captain approved spec assumptions and success criteria
- [x] Captain approved this plan
- [x] Implemented (see `tasks/todo.md`)
