# Software backend (CPU fallback)

Status: current (M4 — first runnable, proven). Sources: `12-archive/DESIGN.md` §6;
`12-archive/BUILD-ORDER.md` §§M4, 3; `04-planning/state.md` §5j.

tiny-skia CPU backend implementing the full contract: `FramePlan`
builder from dirty subtrees, per-surface commit, `SemanticsDiff`
computed + dumped, plus the headless image-diff / full-repaint-assert
oracle as a fourth pseudo-backend (permanent CI smoke test and
image-diff substrate).

Why M4 is first: it exercises every contract type with zero optional
machinery (no events, hit-testing, transitions, IME) and proves
text-as-data end to end. Android uses this backend as the
Caps-negotiated fallback on hostile GPUs (unvalidated at mobile
resolutions — a re-testable bet, see
[performance: mobile](../../08-performance/mobile.md)).

M7 note: the shared `FramePlanBuilder` now carries exact `em_size`
+ per-run `fonts` on every `DrawOp::Text` (decision 110); the CPU
replay ignores both (glyph cells unchanged — M4/M5 suites green
without modification). The DOM backend (M7) consumes the same
builder for its damage/stats discipline.
