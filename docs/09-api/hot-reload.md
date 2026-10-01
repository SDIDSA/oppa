# Hot reload in apps (G14)

Status: current (decision 233). Sources: `crates/oppa-reload`
(`HotRegistry`, `StaticSource`, `DylibSource`), example
`crates/oppa-reload/examples/app_loop.rs`.

The app loop (run the example: `cargo run -p oppa-reload
--example app_loop`):

1. Boot the `ComponentHost` and mount typed components (normal
   app boot — no harness involved yet).
2. `HotRegistry::new(host)` + `install(source)` — the harness
   learns the manifest symbols (dev: `DylibSource` rescans the
   built cdylib; the example uses `StaticSource`, same loop).
3. Drive frames normally. On file change: rescan, then
   `reload_to(next)` — props drain through the outgoing
   manifest, the swap applies atomically at RELOAD, survivors
   re-render under the new code.
4. Read the `ReloadReport` (`ok()`, `evicted` with reasons) —
   evictions are the restart class (§5.1: symbol gone, props
   layout changed), never silent reinterpretation.

What survives: author-owned signals, `keyed_state`, core-side
sessions (`EditSession`, scroll offsets) — residence, not luck
(ADR-0013). What does not: props layouts across a shape change
(drain + evict, loudly).

Android stays restart-only (locked #16, ADR-0008) — relaunch
builds a fresh host (proven pixel-identical to cold start).
