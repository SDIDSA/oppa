# ADR-0007: Handlers-as-ids and generational storage

Status: Accepted (R2; locked #11–#13). Sources: `12-archive/DESIGN.md` §§2–4;
`04-planning/state.md` §3.1.

## Context

A retained tree in an ownership language must survive reloads,
recycling, and slot reuse without aliasing or stale access.

## Decision

The retained tree is never referenced by user code — only by id.
Handlers-as-ids (stable symbol-hash registry, atomic flip);
signals as generational slots; nodes in a generation-checked arena;
components keyed explicitly; `keyed_state` for transient per-item
state; virtualization model-level (`ScrollArea` + spacer + slot
keys; fixed-height rows v1).

## Alternatives

Reference-captured closures / GC-ambient state — unavailable in
Rust without rebuilding a borrow checker out of `shared_ptr`
(the C++ rejection).

## Consequences

Retained tree copyable, serializable, hot-swap-safe; recycling is
reconciler reuse (no user-visible pool machinery); phantom
transitions on rebind handled by the binding-edge stamp (locked
#22); stale access is loud, never silent.
