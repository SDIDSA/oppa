# PROJECT.md — working map

## What this is

Cross-platform GUI framework (Rust): retained declarative scene graph
× fine-grained reactivity → diff-driven presenters. Targets: Windows,
Linux, Android, Web. Priorities: rendering performance > developer
ergonomics > binary size > hot reload.

## Major layers (and where they live)

```text
signals/state → components → VNode (ephemeral) → RetainedNode tree
  → layout → display lists + damage → renderer commits
```

| Layer | Code | Docs |
|---|---|---|
| Reactive core + scheduler + storage | `crates/oppa/src/reactive/`, `arena.rs`, `worker.rs`, `clock.rs`, `handlers.rs` | `docs/ARCHITECTURE.md`, `docs/SPEC.md` |
| Components + reconciler | `component.rs`, `vnode.rs`, `reconciler.rs`, `style.rs`, `semantics.rs`, `interner.rs`, `pass_mask.rs`, `hash.rs` | `docs/ARCHITECTURE.md`, `docs/SPEC.md` |
| Text | `text.rs`, `ime.rs`, `crates/oppa-text-dwrite/` | `docs/ARCHITECTURE.md`, `docs/SPEC.md` |
| Shell | `shell.rs`, `crates/oppa-shell-win/` | `docs/ARCHITECTURE.md`, `docs/06-platforms/` |
| Macros | `crates/oppa-macros/` | `docs/ARCHITECTURE.md` |
| Spike rig (not product) | `crates/spike-textedit/`, `spike/` | not authoritative |

v1 milestone chain M0–M10 closed; productization through Phase 35 closed
(Decisions 1–351). Current state — and only current state — lives in
`docs/STATE.md`. Do not assume a feature exists without checking it;
genuinely open items live in `docs/STATE.md` (Next / Blocked).

## Doc entry points

- Architecture (current): `docs/ARCHITECTURE.md`
- Contracts (current): `docs/SPEC.md`
- State (current): `docs/STATE.md` — the only planning file
- Decisions (current): `docs/DECISIONS.md` (rationale: `docs/10-decisions/`)
- Platforms: `docs/06-platforms/<windows|linux|android|web>/overview.md`
- Contributing: `docs/CONTRIBUTING.md`
- Glossary: `docs/GLOSSARY.md`
- Frozen history (read-only): `docs/12-archive/` + git log

## Agent rule

An agent working on a subsystem needs `PROJECT.md` + `docs/ARCHITECTURE.md`
+ `docs/SPEC.md` + `docs/STATE.md` + the relevant ADR / platform page —
not the whole tree. Living docs describe `HEAD`: overwrite superseded text
in place; history is `git log`.
