# Non-goals

Status: current. Sources: `12-archive/DESIGN.md` §7 (deferred to v2),
`12-archive/BUILD-ORDER.md` §5.

Explicitly not v1:

- Custom GPU rasterizer (replaces Vello behind the same `FramePlan`).
- Native-hybrid presenters on desktop (embedding HWNDs/X11 windows) —
  escape hatch, not a v1 path.
- Animation model beyond the `.transition(...)` primitive (tween DSL,
  implicit vs. explicit animations).
- Text-stack consolidation (one bundled cross-OS shaping stack vs.
  per-OS services).
- Wasm-hosted component runtime for Android hot-reload parity.
- Grid layout; variable-height list rows (prefix-sum index);
  multi-window / per-surface reload granularity.
- Tween/implicit-animation DSL; image/video stack beyond static
  pre-decoded images; global/cross-field undo (v1 ships a minimal
  per-field stack).
- macOS / iOS targets — the project targets Windows, Linux, Android,
  and Web only. No macOS/iOS documents exist in this tree by design.

See also [`10-decisions/`](../10-decisions/README.md) for the
rationale behind each deferral, and
[`04-planning/backlog.md`](../04-planning/backlog.md) for what is
accepted future work vs. rejected.
