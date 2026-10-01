# ADR-0008: Hot-reload harness and platform parity

Status: Accepted (R3; locked #14–#16). Sources: `12-archive/DESIGN.md` §5;
`12-archive/BUILD-ORDER.md` (M2b/M9).

## Context

`hot-lib-reloader` requires proxy-declared functions (adding one =
main-binary recompile); `subsecond` patches code in place but not
type/layout changes.

## Decision

Thin custom harness: hot crate exports a stable
`component_manifest()` table; core re-scans after every swap
(~500 lines over `libloading`). Opaque generation-tagged props
with hot-side vtable clone/drop + drain-before-unload; RELOAD phase
between INPUT and EFFECTS, global apply, atomic registry flip.

## Alternatives

`hot-lib-reloader` (rejected: new functions cost recompiles);
`subsecond` (rejected as a marriage; wasm-reload shape reused for
Web). `wasmi`-hosted components for Android parity — deliberate v2.

## Consequences

Adding component functions is a body edit; `Tag`/core changes =
restart (mitigated by the large closed set + `Custom` hatch);
Android restart-only in v1, stated not implied; re-seed detection
and state lints enforced (M2b); fuzzer gate at M9.
