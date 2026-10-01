# Layout subsystem — overview

Status: design current (locked #6); implementation **Current (M3)** —
engine in `crates/oppa/src/layout.rs`, ledger + LAYOUT-phase wiring on
the component host; wrap measured at 1 shape + 0 re-shapes, bidi
oracle-proven (see `04-planning/state.md` §5i).
Sources: `12-archive/DESIGN.md` §§2.3, 9.3; `12-archive/BUILD-ORDER.md` (M3).

- **Owns:** measure/position of dirty subtrees, `LayoutBox` per node,
  measure↔layout protocol, DPR rounding rules, one-frame-delayed
  feedback wiring.
- **Does not own:** shaping (consumes `TextService`), painting,
  event routing.
- **Inputs:** retained tree + LAYOUT pass-masks + `TextService`
  interface (stub metrics to start; unit-tests against hand-built
  trees while the reconciler lands).
- **v1 scope:** flexbox subset + block-lite + absolute positioning
  (`.x`/`.absolute_y` enumerated into scope), inline text runs (line
  break, BiDi; ellipsis optional-v1).
- **Named unknowns:** wrap round-trip count (measured here, not
  assumed); DirectWrite-vs-wasm metric drift.
- **Specs:** [constraints](../../03-spec/layout/constraints.md).
- **Tests:** layout tests land with M3
  ([testing](../../07-testing/integration-tests.md)).
