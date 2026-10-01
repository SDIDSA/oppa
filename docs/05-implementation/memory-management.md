# Memory management

Status: current. Sources: `12-archive/DESIGN.md` §3; `04-planning/state.md` §3.1; code:
`crates/oppa/src/arena.rs`, `crates/oppa/src/interner.rs`.

- `GenArena<T>`: slot arena with free-list reuse; **generation bumps
  on reuse**; `retire` returns the value. No silent stale access.
- Reactive nodes live in three separate arenas (signals/memos/
  effects) with typed handles; `NodeId`/`NodeArena<T>` back retained
  nodes.
- Signals are generational slots (`Arc`-erased); props cross the
  hot boundary opaquely (hot-side vtable clone/drop + generation
  tag) with drain-before-unload.
- `keyed_state`: core-side `KeyedStore`, default capacity 64 (~5×
  the §4 13-slot window), per-runtime overridable; eviction retires
  the slot; re-access by key re-seeds (decisions 50–51). Per-list
  namespacing is M8 scope.
- Styles interned (`Interner<T>` → `StyleId`, structural dedup);
  text shared via `Arc<str>`.
- No signal-drop refcount semantics (retirement is explicit; the M2b
  fuzzer drives it). `!Send` throughout — no synchronization needed
  for generational slots.
