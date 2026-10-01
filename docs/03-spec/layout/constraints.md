# Layout constraints

Status: accepted as v1 scope (locked #6); engine **Current (M3)** —
upheld as built (wrap measured within the ≤2-pass bound, no scope
finding; feedback one frame delayed via the settled generation).
Sources: `12-archive/DESIGN.md` §§2.3, 7.6; `12-archive/BUILD-ORDER.md` (M3).

- v1: flexbox subset + block-lite + absolute positioning
  (`.x` / `.absolute_y` in scope — both showcase examples use them).
- Inline text runs: line break, BiDi; ellipsis optional-v1.
- Deferred: grid, variable-height rows (prefix-sum reworks
  slot-identity semantics — rework of #13, not an add-on).
- Web bound: flat flexbox / text-in-flex (browser lays out hosted
  text flow; engine lays out boxes).
- Measure↔layout protocol exists; wrap round-trip count is measured
  at M3 — if wrap needs >2 passes or re-entrancy, that is a scoped
  finding, not a re-architecture.
- Feedback: an effect reading settled metrics re-runs in the next
  frame's EFFECTS (one frame delayed by design); effects may not
  write what layout reads.

No other layout documents exist because the project uses no other
layout modes — flex/grid/absolute pages beyond this scope would be
invention.
