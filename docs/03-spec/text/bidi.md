# Bidi and complex clusters

Status: accepted (locked #29 closed in M3 — oracle ≤2px at all boundaries).
Sources: `12-archive/DESIGN.md` §§2.3, 9.2; `04-planning/state.md` §§5f–5g.

Measured (corpus rig v2, both arms agreeing):

- Decomposed e-acute (U+0065 U+0301) shapes **one cluster** —
  identical granularity to precomposed U+00E9 (combining-mark parity
  closed).
- ZWJ technologist (U+1F469 U+200D U+1F4BB, 11 bytes) is **one
  cluster**; zero cross-backend geometry/hit-test mismatches
  (single-cluster closed).
- Unit pins: Arabic bytes in `rtl` runs + Latin non-rtl (`shape.rs`
  13→16 tests).

Closed in M3 (was deferred with evidence, not assumed):

- **Bidi visual ordering → M3 layout engine: closed.** Measured 65 device-px
  (DPR1) divergence on Latin+Arabic+digits reproduced, then collapsed
  to ≤2px at all 14 boundaries against the oracle (freeze declared
  pre-M9). Runs carry `rtl` metadata; visual order comes from the engine.
- Word segmentation incl. ZWJ-emoji words (session has no
  emoji/ZWJ-aware `word_class`: emoji classify Separator — same
  family as the CJK dictionary divergence) and scalar caret-stepping
  through combining clusters are shared-suite spec items proven by
  the M1 spike editing session (`spike-textedit`).

No typography.md exists: ligature features, variable-font axes, and
line-breaking beyond single-line fields are unscoped v1 work.
