# Widget tree

Status: accepted (M2 implements). Sources: `12-archive/DESIGN.md` §§2.2, 4;
`04-planning/state.md` §5g; code: `crates/oppa/src/reconciler.rs`.

- Roots are single Elements; fragments compose child lists only;
  holes hold no retained slots.
- Children diff: keyed in place, unkeyed by order; incompatible
  pairs replace (`Remove` + `Add`, never silent coercion); keyed
  reorders move.
- Slot-keyed virtualization: keys are slots, not items — scrolling
  yields zero structure ops; retained identity (layout boxes,
  hit-test entries, layers) persists across item swaps.
- Handler identity is `(NodeId, kind)`, one per kind per node;
  re-runs rebind closures under retained ids (zero steady-state
  churn); only a changed kind-set is an update. Ownership is
  stamped at render time (`Ctx::child` innermost-wins +
  `run_instance` — the reconciler drains closures in the root
  effect, where the running owner is always the root; M8 finding
  F6), so per-instance flags on inline children attribute to the
  child instance, never the root. Multi-handler-per-kind
  is M5 scope.
- Per-instance state keys: call-site source-hash + per-run ordinal;
  a type change at an unchanged site panics (restart class).
- `TreeDiff { ops, suppress_transitions }`; `structure_ops()` counts
  structural ops; value-only changes yield zero structure ops
  (asserted — the §4.2 trace's diff half).

Tests: `crates/oppa/tests/m2_reconciler.rs`
(scroll/selection/rebind/eviction).
