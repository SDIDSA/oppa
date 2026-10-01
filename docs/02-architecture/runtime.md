# Runtime

Status: current (M0 loop + M2 component effects; LAYOUT/PAINT counting
stubs). Sources: `12-archive/DESIGN.md` §§5.3, 9.1, 9.6; code:
`crates/oppa/src/reactive/`, `crates/oppa/src/worker.rs`,
`crates/oppa/src/clock.rs`.

- **Owns:** the phase loop, propagation fixpoint, worker-queue drain,
  reload hook position, animation registry, generation tagging.
- **Does not own:** layout math, paint work, OS events (consumes them).
- **Loop:** `TIME → INPUT → RELOAD → EFFECTS → LAYOUT → PAINT/COMMIT →
  A11Y`, on-demand (`has_demand`: frame request, input, animation,
  reload, dirt, worker messages); vsync cadence while animating, idle
  otherwise.
- **Phases:** TIME (clock + animation registry; no user code); INPUT
  (shell pump + registry dispatch under one `BatchGuard` + worker drain,
  generation-tagged, retired discarded); RELOAD (drain → unload →
  rescan → re-run; global apply; atomic registry flip); EFFECTS
  (topo propagation, memo/component re-runs, reconcile, `TreeDiff`,
  pass masks); LAYOUT / PAINT-COMMIT (stubs); A11Y (semantics emission).
- **Threading:** single UI thread owns the pipeline (`!Send` signals);
  workers hand results via the INPUT-drained queue. Escape hatch
  (subtree-parallel layout) is measured-v2-only behind unchanged phases.
- **Pacing:** per-surface atomic commit/present on each surface's
  cadence; unchanged surfaces skip commit; skew ≤ 1 frame; no
  cross-window vsync alignment in v1.

Relevant specs: [propagation](../03-spec/ui/propagation.md).
Relevant tests: [testing](../07-testing/integration-tests.md).
Relevant ADRs: [ADR-0010](../10-decisions/ADR-0010-scheduler-threading.md).
