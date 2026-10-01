# State model

Status: current (M0 + M2). Sources: `12-archive/DESIGN.md` §§2.2, 3, 9.6;
`04-planning/state.md` §§3, 5g; code: `crates/oppa/src/arena.rs`,
`crates/oppa/src/component.rs`.

- **Signals are generational slots** (`Arc`-erased); node storage is an
  arena + `NodeId` with generation checks (locked #11). Retire is
  explicit and loud — no silent stale access, ever.
- **Residence rule** (locked #25): anything that must survive a hot swap
  lives core-side — `Store<T>` (core-owned reactive collection, reached
  through id-handles), `image_cache` (content-addressed framework
  service), signals, `keyed_state`. Hot crates hold no surviving state
  (crate-level lint).
- **Controlled-component pattern for editable content**: the field's
  content lives in an author-owned signal; the editing session writes it
  back through that signal on every edit (locked #24).
- **Async tasks**: `ctx.spawn` runs futures on the framework executor
  under the handler capture rule (signals/ids only); every task and
  queue message carries its hot generation; at RELOAD, outgoing
  futures are cancelled and retired-generation queued results are
  discarded at the next INPUT drain.
- **Costs, named**: cancel-at-reload restarts in-flight work; the
  authoring pattern is incremental writes through signals at await
  points. `Store` tracking is coarse in M2 (one version signal);
  per-key granular subscriptions are M8 scope.

See also: [memory management](../05-implementation/memory-management.md),
[spec: widget-tree](../03-spec/ui/widget-tree.md).
