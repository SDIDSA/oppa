# hello-web — Oppa starter template

Minimal web app: the desktop hello counter behind one
[`export_app!`](../../crates/oppa-web/src/host.rs) line over
[`WasmHost`](../../crates/oppa-web/src/host.rs) plus the shared
[`oppa-bootstrap.js`](../../crates/oppa-web/web/oppa-bootstrap.js)
rig (same harness the demo and sink ride — one canonical copy, no
per-app fork).

## Use it

```powershell
# From an Oppa checkout (recommended — names the package, points
# path deps at the checkout, rewrites this paragraph):
cargo run -p cargo-oppa -- oppa new ~/my-web-app --web
# Or copy out manually (then point the three path deps at your
# Oppa checkout yourself):
cp -r templates/hello-web ~/my-web-app
cd ~/my-web-app
# One command: builds the wasm, runs wasm-bindgen, serves web/,
# opens the browser (the web equivalent of desktop `cargo run`):
cargo run -p cargo-oppa -- oppa serve
# (Or, once installed: `cargo oppa serve`.)
# Open http://127.0.0.1:8932/index.html
```

`cargo oppa serve` demands the `wasm-bindgen` CLI on PATH with the
exact version this template pins (`wasm-bindgen = "=0.2.128"`,
mirroring the checkout's lockfile — a cargo-oppa test keeps the
three in sync). CLI/lock skew refuses loudly with the install line
instead of corrupting the glue. Flags: `--port <n>` (default
8932), `--dev` (fast builds; default is release), `--no-browser`
(print the URL only). The dev server sends `Cache-Control:
no-store` (stale-wasm traps never bite) and serves `/` as
`index.html`. `web/pkg/` generated output is
yours, never committed. `OPPA_RENDERER` does not apply on web (the
browser owns pixels); the module viewport is fixed at 800x600 to
match the harness.

PWA shell (G20): `web/manifest.json` + `web/sw.js` (cache-first,
versioned `CACHE` per release) ship with the template and are
registered out of the box — serve over http(s) (workers
refuse `file://`), load once online, reload offline. Icons are
your branding (none ships).

Next steps: [contributing](../../docs/CONTRIBUTING.md) (gates + entry
points), [architecture](../../docs/ARCHITECTURE.md) (web backend + shells),
[glossary](../../docs/GLOSSARY.md).

## Verify it (from the Oppa checkout)

```powershell
cargo check --manifest-path templates/hello-web/Cargo.toml --target wasm32-unknown-unknown
node --check templates/hello-web/web/oppa-bootstrap.js
```
