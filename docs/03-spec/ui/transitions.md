# Transitions

Status: current (M8 implements the evaluator; data since M2).
Sources: `12-archive/DESIGN.md` §9.4; locked #22.

Transitions are style-delta interpolation A→B per property over a
duration — no second system, no per-component animation code.

Binding-edge rule: deltas from a re-run triggered through
`ctx.binding` carry `suppress_transitions: true` for exactly one
commit in the `TreeDiff` payload. GPU: the TIME-phase interpolator
creates no interpolator. DOM: the mapping writes target values with
transitions disabled for that commit.

v1 limit: suppression is per-commit, not per-cause. Per-value
provenance is deferred unless measurement shows it matters.

Tests (M8, proven): phantom-flash sweep — binding-edge commits create
zero interpolators (counted in the evaluator), image-diffed per frame
via the M4 oracle (CPU exact) and the M6 oracle (Vello tol-banded),
plus the DOM one-commit `transition:none` suppression; re-exercised
mid-reload in M9.
