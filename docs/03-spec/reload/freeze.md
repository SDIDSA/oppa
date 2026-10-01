# Reload fuzzer gate (renderer-freeze precondition)

Status: **DECLARED (M9)** — the BUILD-ORDER M9 gate is green on the
evidence below. Sources: `12-archive/BUILD-ORDER.md` (M9);
`12-archive/DESIGN.md` §§5.3, 8.4, 9.6; code:
`crates/oppa-reload/tests/m9_reload_gate.rs`;
`docs/04-planning/rounds.md` (M9 entry).

- **Gate:** fuzzer green is the precondition for calling renderers
  frozen (§8.4's placement). **Satisfied:** all five
  reload-during-X scenarios pass under tabulated adversarial
  pressure with zero engine violations, asserting the mechanical
  property (no generational slot touched after its generation
  retires; no retired-generation task/worker result applied; no
  cancelled future resumed) — not crash-absence. Counts are the
  default-seed run; seeds 1 and 42 repeat green with same-shape
  counters (see the ROUNDS entry).
- **Harness verdict (M9 §1):** M2b left a working swap path, not
  scaffolding — manifest scan, real dylib swap, typed drain/adopt,
  retire-not-unload, generation-tagged executor, per-run symbol
  resolution, shared run stacks. M9 adds the product-loop timing
  (direct + RELOAD-hook swaps interleaved with scroll / transition /
  IME / burst / task load). True unload (shared-core linking) stays
  deferred per decision 61 — the retire model stands, bounded leak
  counted per `ReloadReport`.
- **Residuals (tracked, not gate inputs):** TSF/TIP machinery is not
  fuzzed headless (needs a real OS IME); the shell crate holds no
  reactive handles (grep-verified — no generational surface), and
  the real-IME evidence stands (locked #28). Per-iteration fuzzing
  runs over `StaticSource`; the real-dylib path is proven once per
  run by `real_dylib.rs` (same `reload_to` code path).

Contract tests: `crates/oppa-reload/tests/m9_reload_gate.rs` (6),
`reload_cycle.rs` (8), `fuzz_reload.rs` (1), `real_dylib.rs` (1).
