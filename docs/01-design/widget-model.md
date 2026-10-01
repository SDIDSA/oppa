# Widget model

Status: current (M2 headless-complete against both locked §4 examples).
Sources: `12-archive/DESIGN.md` §§2.1–2.2, 4; `04-planning/state.md` §5g; code:
`crates/oppa/src/vnode.rs`, `crates/oppa/src/component.rs`,
`crates/oppa/src/reconciler.rs`.

A retained, declarative scene graph produced by a fine-grained reactive
model, consumed by diff-driven presenters. **Retained at the model level,
immediate at the submit level. Two trees, not three** (locked #4): with
fine-grained dependency tracking, invalidation knows which component
instances to re-run, so the ephemeral VNodes diff locally against retained
nodes — Flutter's widget→element→renderobject triple collapses.

```text
signals/state → components → VNode tree (ephemeral, discarded)
             → reconcile vs. RetainedNode tree (stable identity)
             → layout → paint → display lists + damage → a11y diff
             → renderer commits & presents
```

Components are plain functions returning `VNode`
(`Element | Text | Fragment | Hole`); `Ctx` scopes per-instance state
keyed by call-site source-hash + ordinal (inserting a signal re-seeds
later sites — same rule as React hooks). Per-item transient state uses
`ctx.keyed_state` (LRU, default capacity 64). Virtualization is
model-level: `ScrollArea` + spacer + **slot keys** (keys are slots, not
items, so scrolling yields zero structure ops); fixed-height rows in v1
(locked #12–#13).

Load-bearing rules: handlers-as-ids (retained tree copyable,
serializable, hot-swap-safe); closed-set `Tag` + `Custom` escape hatch;
interned styles; per-node dirty masks. Six mechanical M2 deltas D1–D6 vs.
`12-archive/DESIGN.md` §4 are recorded in `04-planning/state.md` §5g / `04-planning/rounds.md` (M2 entry) —
nothing silently reshaped.

See also: [architecture overview](../02-architecture/overview.md),
[spec: widget-tree](../03-spec/ui/widget-tree.md),
[spec: reconciliation](../03-spec/ui/reconciliation.md).
