# Web-app entry point

Status: current (the `oppa-web` binding shape, proven by the
out-of-repo usability app + the sink-on-web rounds). This page is the pattern reference
for "my own scene in a browser tab"; the copy-paste tutorial is
[getting started](../05-implementation/getting-started.md).

`WebApp` mounts any root component (decision 275): `WebApp::new`
for the demo, `WebApp::new_with_root("Name", props, render)` for
your app, `WebApp::new_sink` for the kitchen sink. Living example
(not prose):
[`crates/oppa-web/src/lib.rs`](../../crates/oppa-web/src/lib.rs),
[`web/bootstrap.js`](../../crates/oppa-web/web/bootstrap.js),
[`web/index.html`](../../crates/oppa-web/web/index.html).

## Host (your crate, `cdylib` + `wasm-bindgen`)

Use the shared harness (Round 29.1, decision 344) — `WasmHost`
owns the `ComponentHost`, the `MockClock` (wasm has no `Instant`
— time enters only through rAF timestamps), the `DomBackend` +
`StyleSheet`, and the diff cursor, with the inject/settle/sync
binding bodies. Your crate adds ~25 lines of `#[wasm_bindgen]`
glue over your own root (mount + click/key/hover/text/tick
forwarding + the bootstrap from `web/`):

```rust
use oppa_web::WasmHost;

let mut app = WasmHost::mount_root("MyApp", props, MyRoot);
let patch = app.click(x, y); // Option<String>: patch JSON or None
```

Need pre-mount wiring (shared signals, preloaded images — the
demo's case)? Split the calls: `WasmHost::boot()` for the rig,
wire + mount + settle through the plain host API, then
`WasmHost::shell(host, clock, images)`. The legacy shape below
is what the harness abbreviates (kept as the pattern reference
for binding authors):

```rust
use oppa::{Color, ComponentHost, MockClock, RendererBackend, SurfaceDesc};
use oppa_cpu::FramePlanBuilder;
use oppa_dom::{render_page, DomBackend, StyleSheet};

fn sync_and_render(host: &ComponentHost, dom: &mut DomBackend,
    sheet: &mut StyleSheet, surface: oppa::SurfaceId,
    builder: &FramePlanBuilder, cursor: &mut usize) -> (String, usize) {
    for diff in host.diffs_from(*cursor) {
        dom.commit(&diff).expect("dom commit failed");
    }
    *cursor = host.diff_count();
    let stats = host.with_retained_mut(|rec, styles|
        dom.sync(rec, styles, sheet).expect("dom sync failed"));
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    dom.paint(surface, &plan).expect("dom paint failed");
    (render_page("title", dom, sheet), stats.touched)
}
```

Bindings (all return the new page HTML, or `None` when untouched —
the bootstrap swaps only on non-null):

- `new()`: `ComponentHost::with_clock`, `set_viewport(800, 600)`,
  `mount("Name", props, render)`, `run_until_idle`,
  `DomBackend::new(1.0)` + `create_surface`, `StyleSheet::new`,
  `FramePlanBuilder::new`, initial `sync_and_render`.
- `click(x, y)`: `pointer_down` + `pointer_up` through
  `inject_input`, `run_until_idle`, `sync_and_render`.
- `key(code, pressed)` / `hover(x, y)`: same shape, one event.
- `tick(now_ms)`: `clock.set(now_ms / 1000.0)`,
  `run_until_idle`, `sync_and_render` (TIME tails settle through
  these; a settled tick returns `None`).

## Page (your `web/` dir)

- `index.html`: container div (fixed 800×600 in v1 — the module
  viewport must match), module script tag.
- `bootstrap.js`: `import init, { YourApp } from
  "./pkg/your_app.js"` (bindgen names the module after the crate,
  `-` → `_`); pointer/key listeners call the bindings with
  container-relative CSS px; `requestAnimationFrame` feeds
  `tick`; swap `innerHTML` on non-null. Guard the tick loop with
  try/catch so one bad frame never kills it.

## Text entry (U8)

Fields are verdict-(b): the browser owns the `<input>` (caret,
selection, undo); the framework observes values and sets
initial/programmatic ones. Pattern (proven by an out-of-repo
todo app with a working box — typed text became list rows in
headless Edge, focus/value surviving unrelated swaps):

```rust
// Component owns the value signal and renders it.
let value = ctx.signal(SharedString::from(""));
TextField { text: value.get(), style: Text::title_small, label: "Name".into() }.into()
```

Reserve space explicitly: an unmeasured field commits 0×0 and
reserves no space, so the next sibling stacks onto its origin
and paints over it (proven with elementFromPoint — the later
sibling wins). Wrap the field in a sized container:

```rust
oppa::Div("fieldwrap")
    .style(Style::new().size(300, 32))
    .child(TextField { text: value.get(), .. }.into())
```

```rust
// App layer binds field nodes to signals once (labels match).
let fields = host.text_fields();
host.bind_text(fields[0], value.clone());
```

- `InputEvent::Text { target, value }` sets the full value at
  the INPUT boundary (feed-only — fields carry no handlers;
  unbound targets are quiet no-ops; the stream is
  level-triggered and self-heals). The component re-renders
  by reading the signal — no other wiring.
- Page side: forward `input` events as `text(pid, value)`
  (`data-pid` resolves via `DomBackend::node_for_pid`;
  unknown pids return `None`, never panic) and preserve the
  focused input across HTML swaps (record pid + selection
  pre-swap, restore post-swap) — see `oppa-web`'s
  `bootstrap.js`.
- Bound: fields render at their committed (possibly zero)
  size — there is no usable-size story for unmeasured text in
  wasm yet (paragraph/layout track); latin typing is proven,
  IME composition is deferred.

## Bounds (stated, not hidden)

- Incremental DOM patching: `take_patch` keyed diffs (swaps/attrs/
  selections/spacers/removes/places) mutate in place — focus
  survives background updates (decision 307, Round 12.1). The
  focused-field preservation shim above stays as defense in depth.
- Text measures through the bundled DejaVu Sans rustybuzz service
  (decision 274, Round 6.3) — scenes lay out with real advances,
  not zero widths. Browser-side metric drift and IME composition
  stay open (the DWrite-side audit and the BiDi rig extension are
  named follow-ups, not gates).
