# oppa — cross-platform GUI framework

A retained, declarative UI framework in Rust targeting **Windows,
Linux, Android, and Web**: one UI model, diff-driven per-platform
presenters (Vello on desktop GPU, DOM on Web, CPU fallback).

Priorities: rendering performance > developer ergonomics > binary
size > hot reload.

## Map

- [`PROJECT.md`](PROJECT.md) — one-page working map (start here).
- [`docs/`](docs/README.md) — six living files describing `HEAD`
  (architecture, spec, state, decisions, glossary, contributing)
  plus per-platform pages and ADRs.
- [`AGENTS.md`](AGENTS.md) — instructions for AI coding agents.

## Build

```powershell
cargo test
cargo clippy --all-targets
cargo fmt --all -- --check
```

Full gates (wasm check included): [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md).

Status: v1 milestone chain M0–M10 closed; productization through Phase 35
closed. Current state: [`docs/STATE.md`](docs/STATE.md).
