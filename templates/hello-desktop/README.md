# hello-desktop — Oppa starter template

Minimal desktop app: one signal, one button, one text row.
Proven shape (same as `oppa-controls --example hello`).

## Use it

```powershell
# From an Oppa checkout (recommended — names the package, points
# path deps at the checkout, rewrites this paragraph):
cargo run -p cargo-oppa -- oppa new ~/my-app
# Or copy out manually (then point the three path deps at your
# Oppa checkout yourself):
cp -r templates/hello-desktop ~/my-app
cd ~/my-app
# Point the three path deps at your Oppa checkout, then:
cargo run
```

Escape with nothing focused exits. `OPPA_RENDERER=cpu` forces the
software path. Next steps: [contributing](../../docs/CONTRIBUTING.md)
(gates + entry points), [architecture](../../docs/ARCHITECTURE.md),
`oppa-testkit` headless tests.

## Verify it (from the Oppa checkout)

```powershell
cargo check --manifest-path templates/hello-desktop/Cargo.toml
```
