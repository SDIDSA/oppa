# IME composition events

Status: accepted (surface current; real wiring current on Windows).
Sources: `04-planning/state.md` §4.2; code: `crates/oppa/src/ime.rs`;
`crates/oppa-shell-win/src/tsf.rs`, `src/win.rs`.

Normalized shape (backend-agnostic):

```rust
ImeCompositionEvent {
    CompositionStarted { start_byte },
    CompositionUpdated { composition, caret_byte },  // composite coords
    CompositionCommitted { committed },
    CompositionCancelled,
    DeleteRange { range },
}
```

- Single dispatch seam: `dispatch_ime_event` — scripted feeds and
  platform wiring route through it (no lost/duplicated delivery).
- `ImeOps { SetCaretRect, ShowCandidateWindow, HideCandidateWindow }`
  via `PlatformShell::set_ime`; real anchoring on Windows
  (`ImmSetCompositionWindow` CFS_POINT + `ImmSetCandidateWindow`).
- Composition-over-selection ordering (to be verified, not asserted):
  mapper feeds `CompositionStarted{anchor}` before
  `DeleteRange{selection}` so the atomic pre-composition undo
  snapshot captures pre-deletion content (decision 28).
- TSF path: TIP transactions become the same `ImeMessage`s as IMM;
  non-empty final span commits, empty cancels; real-TIP behavior is
  translated, not second-guessed (decision 38).
- CDP scripting cannot drive delete-range-mid-composition — that path
  required the real-IME passes (see
  [experiments](../../11-experiments/ime-verification.md)).
