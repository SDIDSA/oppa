# oppa — cross-platform GUI framework (v1 closed, v2 underway)

A retained, declarative UI framework in Rust targeting **Windows,
Linux, Android, and Web**: one UI model, diff-driven per-platform
presenters (Vello on desktop GPU, DOM on Web, CPU fallback).

Priorities: rendering performance > developer ergonomics > binary
size > hot reload.

## Map

- [`PROJECT.md`](PROJECT.md) — one-page working map (start here).
- [`docs/`](docs/README.md) — full documentation tree
  (vision → design → architecture → spec → planning →
  implementation → platforms → testing → performance → API →
  decisions → experiments).
- [`AGENTS.md`](AGENTS.md) — instructions for AI coding agents.
- Raw records (preserved): `12-archive/DESIGN.md` (v1 closed design reference),
  `12-archive/BUILD-ORDER.md` (milestone plan), `04-planning/state.md` (implementation
  snapshot), `04-planning/rounds.md` (round history), `12-archive/IME-SESSION.md`,
  `spike/` (experiment rigs + evidence).

## Build

```powershell
cargo test
cargo clippy --all-targets
cargo fmt --all -- --check
```

Status: v1 milestone chain M0–M10 closed plus remainder gaps;
Phases 8–24 productized the framework (controls catalog, Task
Studio reference app) and Phase 25 closed the real-dev last mile
(hello example + desktop quickstart, Toast, app cookbook,
API-doc refresh, packaging recipes) — full workspace suite green
throughout (see `docs/04-planning/rounds.md`).
v1 record: [`docs/HANDOFF-V1.md`](docs/HANDOFF-V1.md).
New developers start at [getting
started](docs/05-implementation/getting-started.md), not the
planning logs below.
