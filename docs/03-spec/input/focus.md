# Focus

Status: accepted as design; ordering current (M5 — see
`04-planning/state.md` §5k). Sources: `12-archive/DESIGN.md` §§2.2,
9.2.

- At most one active editing session: the focused field.
- `Focus(change)` is a first-class `InputEvent`; `ctx.focused()` is a
  framework-owned per-instance signal.
- **Focus loss mid-composition commits** the in-progress composition
  (adopted spec, locked #27 — every native platform commits on blur;
  the spike session's cancel-on-blur was the outlier).
- Tab order (decision 96): press-handler nodes in depth-first
  pre-order (retained child order — same tree twice yields the same
  order, asserted). Focus follows pointer click; explicit `Focus`
  targets must be live press-owner nodes (else loud refusal).
- TSF note (current): per-step `SetFocus` re-assert is gated on focus
  change (churn reduction, not credited for readings) — see
  `12-archive/IME-SESSION.md`.
