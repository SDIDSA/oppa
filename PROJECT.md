# PROJECT.md — working map

## What this is

Cross-platform GUI framework (Rust): retained declarative scene graph
× fine-grained reactivity → diff-driven presenters. Targets: Windows,
Linux, Android, Web. See [`docs/00-vision/vision.md`](docs/00-vision/vision.md).

## Major layers (and where they live)

```text
signals/state → components → VNode (ephemeral) → RetainedNode tree
  → layout → display lists + damage → renderer commits
```

| Layer | Code | Docs |
|---|---|---|
| Reactive core + scheduler + storage | `crates/oppa/src/reactive/`, `arena.rs`, `worker.rs`, `clock.rs`, `handlers.rs` | `docs/02-architecture/runtime.md`, `docs/03-spec/ui/propagation.md` |
| Components + reconciler | `component.rs`, `vnode.rs`, `reconciler.rs`, `style.rs`, `semantics.rs`, `interner.rs`, `pass_mask.rs`, `hash.rs` | `docs/01-design/widget-model.md`, `docs/03-spec/ui/widget-tree.md` |
| Text | `text.rs`, `ime.rs`, `crates/oppa-text-dwrite/` | `docs/02-architecture/text/overview.md`, `docs/03-spec/text/` |
| Shell | `shell.rs`, `crates/oppa-shell-win/` | `docs/02-architecture/windowing/overview.md` |
| Macros | `crates/oppa-macros/` | `docs/09-api/widget.md` |
| Spike rig (not product) | `crates/spike-textedit/`, `spike/` | `docs/11-experiments/` |

Missing by design (planned): none in the M0–M10 chain — layout engine (M3),
backends (M4/M6/M7), emitters, and the hot-reload harness (M2b) all shipped
(see `docs/HANDOFF-V1.md` and `docs/04-planning/state.md`). Do not assume a
feature exists without checking `state.md`; genuinely open items live in
`docs/04-planning/backlog.md` and the Phase 19 brief
(`docs/04-planning/production-readiness-plan.md`).

## Doc entry points

- Architecture: `docs/02-architecture/overview.md`
- Specs: `docs/03-spec/` (behavioral contracts)
- Decisions: `docs/10-decisions/README.md` (ADRs 0001–0013; full lock
  list `12-archive/DESIGN.md` §7)
- Planning: `docs/04-planning/backlog.md`, `current-sprint.md`
- Implementation: `docs/05-implementation/` (start: `getting-started.md`)
- Raw records: `12-archive/DESIGN.md`, `12-archive/BUILD-ORDER.md`, `04-planning/state.md`, `04-planning/rounds.md`

## Agent rule

An agent working on a subsystem needs `PROJECT.md` + the relevant
architecture overview + the relevant spec + the relevant ADRs + the
relevant implementation guide — not the whole tree. Status labels
(Current / Planned / Proposed / Experimental / Superseded) are
mandatory reading: never present planned architecture as implemented.
