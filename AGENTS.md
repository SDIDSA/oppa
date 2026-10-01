# AGENTS.md — AI coding agent instructions

Read [`PROJECT.md`](PROJECT.md) first. Then load only what your subsystem needs:
`docs/ARCHITECTURE.md` + `docs/SPEC.md` + `docs/STATE.md` + the relevant
ADR in `docs/10-decisions/` or platform page in `docs/06-platforms/`.
The index is [`docs/README.md`](docs/README.md).

## Hard constraints

1. Do not invent architecture, numbers, or historical reasoning.
2. Do not silently resolve ambiguous design decisions — record them as 1-line
   rows in `docs/STATE.md` (blocked) or `docs/DECISIONS.md` (accepted).
3. Living docs describe `HEAD`. Overwrite superseded text in place, in the same
   edit — never accumulate snapshots, per-round files, or history prose.
   History is `git log`. (`docs/12-archive/` stays frozen and read-only.)
4. Do not treat experiments or plans as implemented functionality.
5. Do not modify implementation to make documentation fit; validate claims
   about current behavior against `crates/`.
6. Terminology: [`docs/GLOSSARY.md`](docs/GLOSSARY.md) wins on naming disputes.
7. Every code round: update `docs/STATE.md` + the affected `ARCHITECTURE.md` /
   `SPEC.md` section in the same change; re-verify `cargo test`,
   `cargo clippy --all-targets`, `cargo fmt --all -- --check`; record gates in
   `CONTRIBUTING.md` order.
8. Loud failures over silent behavior changes (see
   `docs/CONTRIBUTING.md`).
9. Keep documents small and single-question; one fact lives in one place —
   cross-reference with relative links; never link to nonexistent files.
10. No new doc files without deleting or merging an old one.
