# Layout model

Status: current as design (locked #6); engine implementation **Current**
(M3: `crates/oppa/src/layout.rs` + LAYOUT-phase wiring; 165 tests green —
see `04-planning/state.md` §5i). Sources: `12-archive/DESIGN.md` §§2.3, 9.3; `12-archive/BUILD-ORDER.md` (M3).

**Layout is framework-owned, written once; renderers never compute
layout** (locked #6). One engine produces a `LayoutBox` per node
(x, y, w, h, content size, shaped text runs). Rationale: renderer-side
layout means N implementations of the hardest code; the Web backend must
not run browser layout for boxes (split-brain); results must be stable
for hot reload and a11y bounds.

v1 scope: flexbox subset + block-lite + absolute positioning. Deferred:
grid, variable-height rows (prefix-sum index). Text layout of runs
belongs to the model layer — the display list carries pre-shaped,
pre-positioned glyph runs; renderers only rasterize them.

Web caveat (documented parity limitation, not a bug): our engine lays out
boxes; the browser lays out text flow inside hosted text nodes. v1 Web is
constrained to flat flexbox / text-in-flex. DPR rounding is shared and
identical on all backends (commit positions only; shaping stays
subpixel).

Measure↔layout protocol unknowns are named, not assumed: wrap
round-trip count is measured at M3; DirectWrite-vs-wasm metric drift
feeds the rounding rules.

See also: [layout architecture](../02-architecture/layout/overview.md),
[spec: layout constraints](../03-spec/layout/constraints.md),
[spec: coordinate system](../03-spec/rendering/coordinate-system.md).
