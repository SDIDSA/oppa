# hello-web — Oppa starter template

Minimal web app: the desktop hello counter behind ~25 lines of
`#[wasm_bindgen]` glue over [`WasmHost`](../../crates/oppa-web/src/host.rs).
Proven shape (same harness the demo and sink ride).

## Use it

```powershell
# From an Oppa checkout (recommended — names the package, points
# path deps at the checkout, rewrites this paragraph):
cargo run -p cargo-oppa -- oppa new ~/my-web-app --web
# Or copy out manually (then point the three path deps at your
# Oppa checkout yourself):
cp -r templates/hello-web ~/my-web-app
cd ~/my-web-app
# Point the three path deps at your Oppa checkout, then:
cargo build --release --target wasm32-unknown-unknown
wasm-bindgen target/wasm32-unknown-unknown/release/hello_web.wasm --target web --out-dir web/pkg
python -m http.server 8931 --directory web
# Open http://localhost:8931/index.html
```

`wasm-bindgen` CLI must match the `wasm-bindgen` crate version in
your app's `Cargo.lock` (the workspace lockfile pins 0.2.128 today;
this template generates its own lock on first build, so read yours
and `cargo install wasm-bindgen-cli --version <that-version>` —
machine-local install, never committed). `web/pkg/` generated
output is yours, never committed. `OPPA_RENDERER` does not apply on
web (the browser owns pixels); the module viewport is fixed at
800x600 to match the harness.

Next steps: [contributing](../../docs/CONTRIBUTING.md) (gates + entry
points), [architecture](../../docs/ARCHITECTURE.md) (web backend + shells),
[glossary](../../docs/GLOSSARY.md).

## Verify it (from the Oppa checkout)

```powershell
cargo check --manifest-path templates/hello-web/Cargo.toml --target wasm32-unknown-unknown
node --check templates/hello-web/web/bootstrap.js
```

The `wasm-bindgen` + serve steps above stay manual (machine-local
CLI pin — see [packaging](../../docs/06-platforms/packaging.md)).
