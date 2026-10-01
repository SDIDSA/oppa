# Build system

Status: current. Source: root `Cargo.toml`; `04-planning/state.md` §1.

Virtual workspace, resolver 2:

| Crate | Deps | Notes |
|---|---|---|
| `crates/oppa` | std only (zero-dependency) | core: reactive, storage, scheduler, TextService contract, reconciler, components |
| `crates/oppa-macros` | zero-dep proc-macro | `#[component]` + `#[derive(Props)]`; dev-dep of `oppa` for M2 tests |
| `crates/oppa-text-dwrite` | `windows` 0.62 | Windows-only by definition |
| `crates/oppa-shell-win` | `windows` 0.62 features | minimal Windows shell |
| `crates/spike-textedit` | `windows` features + `vello` 0.10 / `wgpu` 29 / `raw-window-handle` / `pollster` | spike rig + debug renderer (throwaway) |

Compile-time budget: crate splitting + `mold`/`lld`; the
wasm/dylib component boundary keeps the core out of the iteration
loop (`12-archive/DESIGN.md` §3). The spike's Web arm (`spike/web/`) is
`puppeteer-core` Node tooling, not part of the Rust workspace.
