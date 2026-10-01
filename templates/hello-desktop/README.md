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
software path. Next steps: [getting started](../../docs/05-implementation/getting-started.md)
(§0 desktop, §1–3 web), [cookbook](../../docs/09-api/cookbook.md),
`oppa-testkit` headless tests.

## Verify it (from the Oppa checkout)

```powershell
cargo check --manifest-path templates/hello-desktop/Cargo.toml
```
