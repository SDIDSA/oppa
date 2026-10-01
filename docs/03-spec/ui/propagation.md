# Propagation

Status: accepted (M0 implements; component-level proven M2).
Sources: `12-archive/DESIGN.md` §9.1; locked #19 + amendment #26.

- Order: topological by dependency depth; ties by call-site-stable
  creation order; at most one run per node per pass.
- Writes during a run: downstream not yet run → folds into the pass
  heap; downstream already ran (or own deps dirtied) → re-entry pass
  via `pending`.
- Budget: 3 passes/frame. Past it: debug panics with the cycle chain;
  release defers once to the next frame's EFFECTS (same budget), then
  parks with a rate-limited log. Component-level divergence follows
  this path — no new lock (decision 59: the release path is loud-log +
  deferred dirt + parked-with-reason, not silence).
- Memos: lazily marked, recomputed in topo order in EFFECTS; reads
  outside EFFECTS pull-recompute (tracked); structural `PartialEq`
  gate by default (`memo_with_eq` escape); **memos never write
  signals or create effects — panic in all profiles** (locked #26).
- Pull-recompute reading (decision 2): "no execution mid-INPUT", not
  "no dirty-marking" — a pulled memo recomputes immediately and marks
  dependents dirty; effects stay dirty until EFFECTS.
- Release budget (decision 4): exactly one retry frame, then park —
  every-frame retry would be the livelock the paragraph forbids.

Tests: `crates/oppa/tests/propagation.rs` (18),
`crates/oppa/tests/scheduler_on_demand.rs` (10).
