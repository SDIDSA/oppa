# Application API

Status: current (state shapes below + the desktop runner). Source:
`04-planning/state.md` §8 (API quick reference); `12-archive/DESIGN.md` §4.
App architecture follow-up: [cookbook](cookbook.md).

Application developers write plain-function components in Rust
against the reactive primitives; the framework owns the loop,
surfaces, and presenters.

State layer (signals, memos, effects, batches):

```rust
let rt = Runtime::new();
let s  = rt.signal(0i32);
let m  = rt.memo(|| s.get() * 2);
let e  = rt.effect(|| { s.get(); });
let _g = rt.batch();
rt.request_frame(); rt.run_once(); rt.run_until_idle();
```

Running (desktop): one call opens the window, mounts the root, and
pumps until close (`crates/oppa-app/src/lib.rs` — Escape at root
exits; every failure is a loud `Err`):

```rust
oppa_app::run_desktop(
    oppa_app::WindowOptions::new("My app", 800, 600),
    (),
    MyRoot,
).unwrap();
// With window ownership (close veto, dialog/theme overrides):
oppa_app::run_desktop_with(options, props, MyRoot, |loop_| {
    loop_.set_close_handler(std::rc::Rc::new(|| true));
}).unwrap();
```

Headless tests ride `oppa-testkit` (`Harness::new` + `mount` +
`tap` + assert — no window).

Async: `ctx.spawn_fetch` / `spawn_fetch_page` on the framework
executor (Send-only bodies; results enter through the `keyed_state`
rendezvous at INPUT, generation-tagged — full contract:
[async-fetch](async-fetch.md)). Images: PNG + SVG decode in
`oppa-image`, content-addressed `image_cache.load` at the scene
seam. This page describes usage, not internals —
internals live in [architecture](../02-architecture/overview.md).
No internal documentation is duplicated here.
