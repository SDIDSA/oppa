# Archived scratch pad — current-sprint.md as of 2026-09-26 (M4–M6 era)

Frozen verbatim from `docs/04-planning/current-sprint.md` before the
2026-09-29 reconciliation (Phase 18 close / Decision 322). Preserved per
`AGENTS.md` rule 3: authoritative-raw records stay; this snapshot captured a
running scratch pad of "Just finished (M4..M8) / Next (M9)" notes from the
M4–M6 era and does not know M9/M10, the v1 close-out, or Phases 8–18
happened. For the authoritative state see `state.md`; for the current plan
see `current-sprint.md`.

---

# Current sprint

Status: current snapshot 2026-09-26.

**Just finished (M4):** CPU backend + FramePlan builder + image-diff
oracle (= MVP, first runnable) — one static component (styled Div +
one text line) through core→reconciler→layout→FramePlan→CPU
backend→PNG plus a SemanticsDiff dump (`oppa::render` contract types
+ `oppa-cpu` tiny-skia backend + dirty-subtree builder + headless
oracle, 10 acceptance tests). Locked #5 proven implementable
(backend shares no core code beyond the contract types + public
retained reads). Findings: F2 (fresh text leaves now get
STRUCTURE|LAYOUT|PAINT — genuine fix, M2/M3 green) and F1
(position-only LAYOUT moves don't rebuild plans — documented
limitation, M5+ work). Measured: static rebuild 0 ops / 3 skipped
(zero raster); text change 1 op vs full 2; DWrite "Hi" advances
[11.359375, 3.875] flow unmodified with an asserted subpixel AA
fringe; cells solid (`text_as_paths=false` — M6 starts here). 177
tests green (was 165); clippy + fmt clean. No lock needed changing
(decisions 83–92 are interpretations; #5 proven). Full delta:
`04-planning/rounds.md` (M4 entry); snapshot: `04-planning/state.md`
§5j.

**Just finished (M5):** events, hit-testing, focus, first real widget
— the §4.1 Toggle end-to-end on the CPU backend driven by real
`InputEvent` payloads through framework primitives (hit-test walk +
capture/focus router in INPUT's `BatchGuard`), incl. its
`Semantics::switch` payload. Locked #7 proven; #3 exercised by a
stateful interactive widget. M4's forced decisions resolved: border +
ink are stated `Style` fields (inset double-RRect ring, inherited ink;
no backend change); `Color` alpha re-recorded open (M6 owner); F1
re-recorded open (M6 owner — the Toggle's moves carry PAINT, so it
never hits F1). Measured: cancel case clean (no stuck pressed, no
dispatch); input→visual 1 frame. 192 tests green (was 177); clippy +
fmt clean. No lock needed changing (decisions 93–102 are
interpretations; #7 proven). Full delta: `04-planning/rounds.md` (M5
entry); snapshot: `04-planning/state.md` §5k.

**Just finished (M6):** Vello backend + driver matrix (first GPU
presenter) — same dirty-subtree FramePlans through a second
rasterizer (`oppa-vello`: Scene encoder + single-face glyph atlas +
per-surface present ledger + CPU-vs-Vello oracle, 18 acceptance
tests). Locked #17 proven; #21 observable and bounded; #18 serviced
with the compositor owned. M5's opens resolved: alpha stays opaque +
separate opacity (no representation change, pixel proof); F1 closed
by engine-side PAINT stamping. One contract lock touch:
`DrawOp::Text` gains `baseline` (CPU ignores it). Tripwire PASS on
evidence (RTX 3060 Ti + fallback rows; glyph review beats the M4
floor); Skia hatch stays costed, unbuilt. Measured: atlas delta 0.0
(Hi advances match the M4 baseline); static-frame GPU work 0;
strict-geometry diff 0/0, curves tol-16 12 (bound 60). 210 tests
green (was 192); clippy + fmt clean. Full delta:
`04-planning/rounds.md` (M6 entry); snapshot: `04-planning/state.md`
§5l (decisions 103–109).

**Just finished (M7):** DOM backend (third presenter on the
M4-proved contract — TreeDiff→DOM mutations, StyleId→CSS rules,
§9.3 native scroll, external hole, ARIA incl. the spike-verdict
text/edit path) + exact em size + per-run font identity (the M6
remainder, closed as a second lock touch with three-backend
proof) + the measured parity corpus. Locked #2 proven; #23
INPUT-fed with ≤1-frame trail. Finding F3 closed in full; F5
found and fixed in the round. Measured: scroll-tick structure
ops 0 end-to-end, offset trail ≤1 frame, parity 10/10 gated rows
in the flat subset (untracked text width exact) + 1 record row,
three-backend rounded boxes identical. 244 tests green (was
210); clippy + fmt clean. Full delta:
`04-planning/rounds.md` (M7 entry); snapshot:
`04-planning/state.md` §5m (decisions 110–118).

**Just finished (M8):** virtualization + transition evaluator (§9.4
stamp end-to-end — the full §4.2 payoff trace against real backends:
recycled ContactList/ContactRow with slot keys + window-tracking
positions, TIME evaluator honoring the binding-edge stamp, window-lag
compensation under a scripted offset sweep). Locked #13 proven (0
structure ops/tick, 20 repainted cells ≤ ~30, identity selection via
keyed_state); #22/§9.4 proven (flash count 0, interpolators 0 on
stamped commits, TIME curve + DOM one-commit suppression). M7's
carried opens resolved: one overscan constant stands (4 both
backends — decision 119, measured +2 vs +4); per-slot flag
attribution defined slot-scoped + proven (decision 123). Findings:
F6 (inline-child handler ownership) + tail-freeze + splice order +
straddle margin found and fixed in-round; slot-position CSS churn
recorded as follow-up (touches #111). M5 frame counts reframed
(commit-frame + tail, #7 untouched — decision 122). Measured:
repaint 62 ops / 121 damage-nodes / 20 cells max per tick (over=4;
50/16 at over=2); oracle exact 0 every tick CPU (281 ticks) and
Vello (126 ticks, GPU readback); lag cover ±4 rows/frame. 270 tests
green (was 244); clippy + fmt clean. Full delta:
`04-planning/rounds.md` (M8 entry); snapshot: `04-planning/state.md`
§5n (decisions 119–128).

**Next (M9):**

Reload product loop + fuzzer gate (renderer-freeze precondition) —
re-exercise the M8 sweep mid-reload (scroll + transition + INPUT
burst under swap); the F6 owner-stamp path and the evaluated-hook
plans both cross the hot boundary. Open items carried from M8:
slot-position CSS churn dedup (touches #111 — decide, M9/cleanup),
GLES weakest-hardware row (M10 Android-device owner), Win32→
`InputEvent` shell mapping (platform track).

**Blocking:** nothing — M4 was the last contract-proof gate. The M4
oracle is the permanent CI substrate M8 consumes frame-by-frame.

**Blocking:** nothing — M3 was parallel with M2b per the dependency
graph (`12-archive/BUILD-ORDER.md` §1). Only two cross-track gates
exist in the whole plan: spike verdict → DOM text path (cleared as
#27) and drain-before-unload → scheduler (already locked).
