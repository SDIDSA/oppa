# Getting started — build a desktop or web app

Status: current. This is the app-authoring entry point: a new
developer copies this page's blocks into an empty directory and ends
with a working app — a desktop window (§0) or a wasm app in a browser
tab (§1–3). Framework proofs live in [web platform](../06-platforms/web/overview.md)
(desktop: `run_desktop` in `crates/oppa-app/src/lib.rs`); the entry-point
pattern reference is [web-app API](../09-api/web-app.md), the app-architecture
follow-up is the [cookbook](../09-api/cookbook.md).

Proven 2026-09-26: an out-of-repo crate built exactly this way
(path deps, no workspace membership) compiled clean on host and
`wasm32-unknown-unknown`, rendered in headless Edge, and passed
click-driven assertions with zero console errors (round record:
`../04-planning/rounds.md` usability entry).
Proven 2026-09-30: the `hello` example shape below mounts headlessly
through `oppa-testkit` (tap increments, no window — round record:
`../04-planning/rounds.md` 25.1 entry).

## 0. Desktop in 5 minutes (Windows/Linux)

No `wasm-bindgen`, no browser, no server — one call opens a window,
mounts your root component, and runs the event/paint loop:

```rust
use oppa::{AlignItems, Column, Ctx, SharedString, Style, Text, VNode};
use oppa_app::{run_desktop, WindowOptions};
use oppa_controls::{Button, ButtonProps};

fn hello(ctx: &Ctx, _props: &()) -> VNode {
    let count = ctx.signal(0i32);
    let pressed = count.clone();
    Column::new()
        .style(
            Style::new()
                .align_items(AlignItems::Center)
                .pad_x(24)
                .pad_y(20)
                .gap(12),
        )
        .children([
            VNode::from(Text::new("Hello, Oppa").size(22).bold()),
            VNode::from(Text {
                text: SharedString::from(format!("Clicked {} times", count.get())),
                style: Text::body_secondary,
            }),
            ctx.child(
                "hello::Click",
                1,
                &ButtonProps::new("Click me", move || pressed.set(pressed.get() + 1))
                    .debug("hello-button"),
                Button,
            ),
        ])
}

fn main() {
    if let Err(e) = run_desktop(WindowOptions::new("Hello, Oppa", 400, 300), (), hello) {
        eprintln!("hello: FATAL: {e}");
        std::process::exit(1);
    }
}
```

Scaffold (outside this repo — same `Cargo.toml` shape as §1 below,
plus `oppa-app = { path = "OPPA/crates/oppa-app" }` and
`oppa-controls = { path = "OPPA/crates/oppa-controls" }`;
`fn main` in `src/main.rs`, not a `cdylib`).
Shortcut: copy [`templates/hello-desktop`](../../templates/hello-desktop/)
(it is this exact app as a standalone crate — proven to check
both in-checkout and copied-out with repointed paths):

```powershell
cargo run
```

Escape with nothing focused exits; every failure is a loud `Err`,
never a silent blank window. GPU-first with loud CPU fallback —
`OPPA_RENDERER=cpu` forces the software path when the GPU refuses.
Next: larger runnable shapes in-repo (`cargo run -p oppa-controls
--example showcase` for eleven controls in three tabs,
`--example kitchen_sink` for the full catalog,
`--example task_studio` for the multi-workflow reference app),
headless tests for your own roots via
[`oppa-testkit`](../../crates/oppa-testkit/src/lib.rs)
(`Harness::new` + `mount` + `tap` + assert — no window), and the
[cookbook](../09-api/cookbook.md) for navigation, persistence,
async, validation, and dialogs.

## Prerequisites (web path)

Rust stable (workspace uses 1.97; any recent stable works),
`wasm32-unknown-unknown` target (`rustup target add
wasm32-unknown-unknown`), `wasm-bindgen-cli` matching the
`wasm-bindgen` crate version in your app's `Cargo.lock` (the
workspace lockfile pins 0.2.128 today; a copied-out template
generates its own lock on first build, so read yours, then):

```powershell
cargo install wasm-bindgen-cli --version 0.2.128
```

(machine-local install, never committed — replace `0.2.128` with
whatever your `Cargo.lock` says). Node + an Edge/Chromium binary
only if you drive the page headless, Python for the static server.

## 1. Scaffold (outside this repo)

```powershell
cargo new --lib my-app
cd my-app
mkdir web
```

`Cargo.toml` (replace `OPPA` with this repo's path; forward
slashes work on Windows). Shortcut: copy
[`templates/hello-web`](../../templates/hello-web/) — the app
below as a standalone crate with its `web/` page (proven
in-checkout and copied-out):

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
oppa = { path = "OPPA/crates/oppa" }
oppa-controls = { path = "OPPA/crates/oppa-controls" }
oppa-web = { path = "OPPA/crates/oppa-web" }
oppa-macros = { path = "OPPA/crates/oppa-macros" }
wasm-bindgen = "0.2"
```

This is the [`templates/hello-web`](../../templates/hello-web/)
manifest plus `oppa-macros` (only §2's `#[derive(Props)]` needs it;
the template itself runs on the other four). Keep them in sync —
if this block and `templates/hello-web/Cargo.toml` disagree, the
template wins and this page is stale: file it as a docs bug.

No workspace membership needed. Framework deps are path deps into
your Oppa checkout: there is no registry release and no SemVer
promise yet — pin your app to a checkout revision (git rev) and
note it, so upgrades are deliberate.

## 2. Components (the shapes that compile)

Components are plain functions. Names are conventionally
`PascalCase` (expect `non_snake_case` warnings — rustc default,
harmless). Props are `Clone` structs with `#[derive(Props)]`
(no generics).

```rust
use oppa::{Color, Column, ComponentHost, Ctx, SharedString, Style, Text, VNode};
use oppa_macros::{component, Props};

#[derive(Clone, Props)]
struct RowProps {
    index: usize,
    done: bool,
    todos: oppa::Signal<Vec<bool>>,
}

#[component]
fn TodoRow(ctx: &Ctx, props: &RowProps) -> VNode {
    let _ = ctx;
    let todos = props.todos.clone();
    let idx = props.index;
    let bg = if props.done { Color(0x44_44_44) } else { Color(0x55_55_55) };
    oppa::Div("row")
        .style(Style::new().size(300, 32).bg(bg))
        // on_press takes Fn() + 'static: clone signals in, never borrow.
        .on_press(move || {
            // update takes FnOnce(T) -> T: RETURN the new value.
            todos.update(|mut items| { items[idx] = !items[idx]; items });
        })
        // Text is a STRUCT literal, not a constructor fn:
        .child(Text { text: SharedString::from("row"), style: Text::title_small }.into())
}
```

Rules the sketches don't say: `Div/Row/Stack/ScrollArea` return an
`ElementBuilder`; `Column` is `Column::new()`. `.child()` and
`.children()` each take `VNode`s and **terminate the chain**
(they return `VNode`, so one of them comes last and there is no
`.build()` after). Child *components* instantiate inline and
return `VNode`:

```rust
#[derive(Clone, Props)]
struct AppProps;

#[component]
fn TodoApp(ctx: &Ctx, _props: &AppProps) -> VNode {
    let todos = ctx.signal(vec![false, true]);
    let rows: Vec<VNode> = todos.get().iter().enumerate().map(|(i, done)| {
        // key is u64 and disambiguates siblings; name is hot-reload identity.
        ctx.child("row", i as u64, &RowProps { index: i, done: *done, todos: todos.clone() }, TodoRow)
    }).collect();
    oppa::Div("screen")
        .style(Style::new().size(800, 600).bg(Color(0xFF_FF_FF)))
        .child(Column::new().children(rows))
}
```

Mount from a host (`ComponentHost::new()` on host, `with_clock`
under wasm — see [web-app](../09-api/web-app.md)); plain
`#[component]` + `mount` is the whole app story.
`component_manifest!` is hot-reload machinery, not app setup.

## 3. wasm build + page

```powershell
cargo build --target wasm32-unknown-unknown --release
wasm-bindgen target/wasm32-unknown-unknown/release/my_app.wasm --target web --out-dir web/pkg
```

(The JS module name follows the crate name with `-` → `_`.)
`web/index.html` + `web/bootstrap.js` are yours to author — copy
the roles from [`oppa-web`'s page](../../crates/oppa-web/web/index.html):
a container div, pointer/key listeners into your `click`/`key`
bindings, rAF timestamps into `tick`, innerHTML swap on
non-null returns. Then:

```powershell
python -m http.server 8931 --directory web
```

Open `http://localhost:8931/index.html`. No bundler, no threads,
no COOP/COEP headers (baseline wasm only).

## 4. Contribute back here (not app setup)

Framework development (workspace tests, clippy, fmt) stays:

```powershell
cargo test          # whole workspace, serial (-j1) if GPU tests flake
cargo clippy --all-targets   # must be clean
cargo fmt --all -- --check   # must be clean
```
