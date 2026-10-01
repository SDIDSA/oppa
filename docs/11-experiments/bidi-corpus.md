# Experiment: bidi / combining / ZWJ corpus round

Status: **Partial** — combining parity + ZWJ single-cluster closed
(locked #29); visual ordering deferred to M3.
Sources: `04-planning/state.md` §§5f–5g; `04-planning/rounds.md` (bidi entry).

Corpus rig v1→v2: Latin+Arabic+digits (visual reordering),
decomposed e-acute (pairs precomposed `héllo`), ZWJ technologist —
all non-ASCII as `\u{...}` escapes (mojibake-proof by
construction). Measured on both arms (Segoe UI, no fallback
surprises): combining 0/0.9 px, ZWJ 0/0.52 px, zero hit-test
mismatches; bidi FAILs both oracles in the predicted shape (65 px
DPR1 — the M0b-deferred work reasserting itself, not new).

Classification: visual order = expected limitation → M3 layout
engine; ZWJ double-click (`(1,1)` vs browser `(0,4)`) = session
word-rule gap (no emoji/ZWJ `word_class`), same family as the CJK
dictionary divergence → shared-suite spec items for the M2 editing
session; c3 FAIL = pre-existing CDP/Edge drift by construction
proof (re-baseline owed, out of scope).

Unit pins (`shape.rs` 13→16): Arabic `rtl` flags, decomposed
single cluster, ZWJ single cluster. Spec:
[bidi](../03-spec/text/bidi.md).
