# Unit tests

Status: current. Source: `04-planning/state.md` §§1, 3–4.

- Core (no backend, run everywhere): IME feed ordering/cancel/
  delete-range mapping; cluster-map round-trips incl. surrogate
  pairs; device-rounding determinism across DPRs 1.0/1.25/1.5/2.0;
  style/semantics/vnode/reconciler lib units; `m2_reconciler` lib
  twins (mount/update-only, one-commit stamp, keyed recycle).
- `oppa-text-dwrite` (`tests/shape.rs`, 16 tests, real system
  fonts, hand-checked): glyph counts, width == Σ advances, Segoe UI
  relations, multi-byte clusters, emoji single-cluster, caret
  monotonicity + round-trips, DPR doubling, tracking convention,
  `enumerate_fonts`, loud `FontNotFound`/`EmptyText`, italic axis,
  Arabic `rtl` flags, decomposed/ZWJ single clusters.
- `spike-textedit` (`tests/session.rs`, 10 tests, fake shaper):
  session regression (word rule, undo, composition anchoring,
  delete-range, canonical stream).

Run: `cargo test` (debug) · `cargo test --release -p oppa --test
m2_reconciler` (release park twin; debug-assert twin compiles out
by the same `cfg(debug_assertions)` gating as M0's budget tests).
