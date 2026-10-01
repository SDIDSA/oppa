# Programming model

Status: current (reactive core + scheduler implemented, M0).
Sources: `12-archive/DESIGN.md` §§2.1, 9.1; `04-planning/state.md` §§3–4; code:
`crates/oppa/src/reactive/`, `crates/oppa/src/handlers.rs`.

The whole framework runs on **five primitives** (locked #9) with scheduler
semantics (locked #18–#20):

- `Signal<T>` — type-erased `Arc<T>` slots; `get`/`get_arc`/`set`/`update`.
- `Memo<T>` — lazy, structural-`PartialEq` gate by default
  (`memo_with_eq` escape); settled in EFFECTS; pull-recompute on reads
  outside EFFECTS. **Memos never write signals or create effects —
  unconditional panic in all profiles** (locked #26; amends §9.1).
- `Effect` — immediate initial run, re-runs in EFFECTS when dirtied.
  A mounted component *is* an effect (M2).
- `BatchGuard` — defers invalidation fan-out to the outermost batch end;
  input → visual response completes within one frame.
- `untrack(f)` — no dependencies recorded inside.

Propagation contract (locked #19): topological order by dependency depth,
ties by call-site-stable creation order, one run per node per pass; writes
during a run fold in (downstream not yet run) or schedule a re-entry pass;
**budget 3 passes/frame** — past it, debug panics with the cycle path,
release defers once then parks with a rate-limited log. Never silent
livelock.

Frame loop (locked #18), single UI thread, on-demand:

```text
TIME → INPUT → RELOAD → EFFECTS → LAYOUT → PAINT/COMMIT → A11Y
```

No user code runs mid-LAYOUT or mid-PAINT; layout feedback is one frame
delayed by design. Workers (image decode, glyph atlas, wgpu submission,
async executor) are framework-owned and hand results back via a queue
drained at INPUT (locked #20). Multi-surface: one global state boundary,
per-surface atomic commit/present, cross-surface skew ≤ 1 frame (locked
#21).

See also: [runtime](../02-architecture/runtime.md),
[state-model](state-model.md),
[spec: propagation](../03-spec/ui/propagation.md).
