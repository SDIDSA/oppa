# Input subsystem — overview

Status: design current (locked #7, #23); implementation current (M5 —
`crates/oppa/src/input.rs` + host router in `component.rs`).
Sources: `12-archive/DESIGN.md` §§2.2, 9.3; `04-planning/state.md`
§§5b.1, 5k.

- **Owns:** normalized `InputEvent` plumbing, GPU hit-test walk,
  `pressed/hovered/focused` primitives, keyboard events, deterministic
  Tab order over the retained tree.
- **Does not own:** OS event production (shell), scroll physics
  authority on Web (browser).
- **Current code:** `Event { kind, handler }` minimal shape (kept as
  the dispatch seam) + `oppa-shell-win` `Cmd` payload queue
  (decision 27) + the M5 `InputEvent` enum, hit-test walk
  (deepest-wins, later-sibling ties, loud misses), capture/focus
  router draining in INPUT's `BatchGuard`, handler→instance routing
  table (root-run granularity — inline-child per-slot attribution is
  M8 scope, decision 97).
- **Proven at M5:** locked #7 plus the §4.1 claim (one propagation
  mechanism replacing six — stuck-pressed-on-cancel solved in
  framework primitives, cancel-case measured clean); Toggle
  end-to-end with its `Semantics::switch` payload; input→visual
  within one frame (measured 1).
- **Specs:** [pointer](../../03-spec/input/pointer.md),
  [keyboard](../../03-spec/input/keyboard.md),
  [focus](../../03-spec/input/focus.md),
  [IME](../../03-spec/input/ime.md).
