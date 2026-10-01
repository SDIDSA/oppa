# Developer experience

Status: harness current (M2b); product loop planned (M9).
Sources: `12-archive/DESIGN.md` §5; `12-archive/BUILD-ORDER.md` (M2b/M9).

Authoring: components in the host language (Rust) — no custom DSL (the
Slint lesson: a DSL means a second toolchain and a split FFI story).
One propagation mechanism; framework-owned `pressed/hovered/focused`
primitives; inline semantics; slots for structural customization.

Hot reload (locked #14): thin custom harness — the hot crate exports a
stable `component_manifest()` table; the core re-scans after every swap.
Props cross opaquely (type-erased, hot-side vtable clone/drop,
generation-tagged) with **drain-before-unload** ordering; registry
re-resolution is atomic with the swap at the RELOAD phase (global apply).
Proven in M2b (`oppa-reload` + fuzzer + a real cdylib swap test —
see `04-planning/state.md` §5h), with three corrections the paper
design did not contain: per-run symbol resolution (never stale code),
retire-not-unload (M2b never unloads — hot-vtabled values stay valid;
true unload is M9 shared-core-linking work), and shared-state run
stacks (tracking survives image boundaries).

| Edit class | v1 support |
|---|---|
| Body edit, new match arm (existing node kind) | Yes |
| New signal in body | Yes, later signals re-seed (enforced invariant, not prose — §8.1) |
| Changed closure captures (signals/ids only) | Yes |
| New component function (manifest scan) | Yes |
| New `Props` field (opaque props + drained) | Yes, else restart |
| New `Tag`/`VNode` variant or core type change | No — rebuild + restart |

Platform parity (locked #16): Windows/Linux dylib swap ≈ 0.1–1 s; Web
wasm swap ≈ 0.1 s; **Android is restart-only in v1** (incremental
rebuild → `adb install -r` → relaunch, ~2–10 s) — stated, not implied.
The wasm-interpreted component runtime (`wasmi`) is a deliberate v2
decision. Compile times are budgeted via crate splitting + `mold`/`lld`;
the wasm/dylib boundary keeps the core out of the iteration loop.

See also: [planning: M2b/M9](../04-planning/backlog.md),
[ADR-0008](../10-decisions/ADR-0008-hot-reload.md).
