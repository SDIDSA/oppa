# AGENTS.md — AI coding agent instructions

Read [`PROJECT.md`](PROJECT.md) first. Then load only the docs for
your subsystem (architecture overview + spec + ADRs + implementation
guide). The full tree index is [`docs/README.md`](docs/README.md).

## Hard constraints

1. Do not invent architecture, numbers, or historical reasoning.
2. Do not silently resolve ambiguous design decisions — document them
   as open questions.
3. Do not delete meaningful existing information; archived records
   (`docs/12-archive/`), living logs (`docs/04-planning/state.md`,
   `docs/04-planning/rounds.md`), and `spike/` evidence are
   authoritative-raw and stay.
4. Do not treat experiments (`docs/11-experiments/`, `spike/`) as
   architecture or plans as implemented functionality.
5. Do not modify implementation to make documentation fit; validate
   claims about current behavior against `crates/`.
6. Respect status labels: Current / Planned / Proposed /
   Experimental / Deprecated-Superseded.
7. Terminology: [`docs/00-vision/terminology.md`](docs/00-vision/terminology.md)
   wins on naming disputes.
8. Every code round: update `04-planning/rounds.md` (delta) + `04-planning/state.md`
   (snapshot); re-verify `cargo test`, `cargo clippy --all-targets`,
   `cargo fmt --all -- --check`; record the table.
9. Loud failures over silent behavior changes (see
   `docs/05-implementation/error-handling.md`).
10. Keep documents small and single-question; cross-reference with
    relative links; never link to nonexistent files.

## Subsystem quick index

- Reactive/scheduler → `docs/02-architecture/runtime.md` +
  `docs/03-spec/ui/propagation.md` + ADR-0010/0013
- Components/reconciler → `docs/01-design/widget-model.md` +
  `docs/03-spec/ui/widget-tree.md` + ADR-0003/0007
- Text/IME → `docs/02-architecture/text/overview.md` +
  `docs/03-spec/text/` + ADR-0012
- Layout → `docs/01-design/layout-model.md` +
  `docs/03-spec/layout/constraints.md` + ADR-0004
- Rendering/backends → `docs/02-architecture/rendering/` +
  `docs/03-spec/rendering/` + ADR-0001/0009
- Platforms → `docs/06-platforms/<windows|linux|android|web>/overview.md`
- Hot reload → `docs/01-design/developer-experience.md` + ADR-0008
