# hello-desktop — Oppa starter template

Minimal desktop app: one signal, one button, one text row.
Proven shape (same as `oppa-controls --example hello`).

## Use it

```powershell
# Copy OUT of this repo (it only builds here via relative paths):
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
