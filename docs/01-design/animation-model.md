# Animation model

Status: current as design (locked #18, #22); evaluator current (M8).
Sources: `12-archive/DESIGN.md` §§4.1, 9.1, 9.4.

v1 ships exactly one primitive: **declarative `.transition(...)`** —
interpolated style deltas with duration + easing. Transitions are
TIME-driven on GPU backends (the scheduler's TIME phase is the only
clock); on DOM they compile to CSS transitions and the browser is the
compositor — so v1's animatable-property and easing subsets are the
CSS-expressible ones, and properties outside the subset jump. Tween DSLs
and implicit-vs-explicit animation are v2.

**Transition × recycle-rebind** (locked #22): a slot-keyed recycled cell
keeps its `NodeId` across item swaps, so a rebind style delta is
indistinguishable from a real state change and would phantom-animate.
Resolution: deltas produced by a re-run triggered through a **binding
edge** (`ctx.binding`) carry `suppress_transitions` for exactly one
commit — values jump, no interpolator. Accepted v1 limit: suppression is
per-commit, not per-cause (a coincident real change under the same stamp
is suppressed too).

Proven as data in M2 (`TreeDiff.suppress_transitions` from the
scheduler's binding-edge flag); honored by the M8 evaluator (TIME
interpolator on GPU, CSS mapping + one-commit suppression on DOM);
proven by the M8 stress (recycled slots with 120 ms transitions under
scripted offset sweep, image-diffed per frame on both backends).

See also: [spec: transitions](../03-spec/ui/transitions.md),
[planning: backlog](../04-planning/backlog.md).
