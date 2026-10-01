# hello-web — Oppa starter template

Minimal web app: the desktop hello counter behind ~25 lines of
`#[wasm_bindgen]` glue over [`WasmHost`](../../crates/oppa-web/src/host.rs).
Proven shape (same harness the demo and sink ride).

## Use it

```powershell
# Copy OUT of this repo (it only builds here via relative paths):
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

Next steps: [getting started](../../docs/05-implementation/getting-started.md)
(§1–3 web), [web-app API](../../docs/09-api/web-app.md),
[cookbook](../../docs/09-api/cookbook.md).

## Verify it (from the Oppa checkout)

```powershell
cargo check --manifest-path templates/hello-web/Cargo.toml --target wasm32-unknown-unknown
node --check templates/hello-web/web/bootstrap.js
```

The `wasm-bindgen` + serve steps above stay manual (machine-local
CLI pin — see [packaging](../../docs/06-platforms/packaging.md)).
