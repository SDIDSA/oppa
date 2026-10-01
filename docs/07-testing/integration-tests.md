# Integration tests

Status: current (M0/M2/M2b); layout/input/a11y suites planned with
their milestones. Source: `04-planning/state.md` §§3, 5g.5, 5h.3.

| Suite | Asserts | File |
|---|---|---|
| Propagation (18) | #19 contract: topo order, fold-in vs re-entry, budget, equality gate | `crates/oppa/tests/propagation.rs` |
| Scheduler on-demand (10) | #18 loop: demand gating, phase order | `tests/scheduler_on_demand.rs` |
| Storage generations (6) | #11: bump-on-reuse, loud stale access | `tests/storage_generations.rs` |
| Handler registry (8) | #11/#5.3: symbol-hash keying, atomic flip | `tests/handler_registry.rs` |
| M2 reconciler (10) | toggle acceptance; write-back settle (debug-assert / release defer-park); zero-structure-op scroll; rebind stamp; `keyed_state` LRU ×3 + eviction; reseeding; opaque props | `tests/m2_reconciler.rs` |
| Reload cycle (8) | #14/#25/§5.3/§9.6: keep/reseed/revive, eviction ×2, registry re-resolution, task apply/discard, keyed survival, hook path | `crates/oppa-reload/tests/reload_cycle.rs` |
| Fuzzer v1 (1) | §8.4 + task path: 150 seeded ops, keep/reseed/revive model, exactly-once task accounting | `crates/oppa-reload/tests/fuzz_reload.rs` |
| Real dylib (1) | #14 with real code pages: adopt/reseed/tracking/discovery/registry across images | `crates/oppa-reload/tests/real_dylib.rs` (+ `fixture/hot-fixture`) |

Planned: layout tests (M3, hand-built retained trees + wrap
round-trip count), input tests (M5: stuck-pressed-on-cancel,
Tab order, one-frame input→visual), a11y tests (emitter mapping
per platform), cross-backend box-compare (M6/M7 DPR determinism).
