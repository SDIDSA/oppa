# v2 item 1 spec — TIME interpolation beyond `.transition`

Status: planned (v2 backlog item 1; nothing here is implemented).
Scope: one question — **what does keyframed TIME interpolation cover,
and how is it proven?** Build answers the open questions below; the
spec does not pre-answer them.

## What v1 banked (the extension substrate, all verified in-tree)

- Authoring: `Style.transition: Option<Transition { dur_ms, ease }>`
  ([style.rs](../../crates/oppa/src/style.rs)); `Ease::{Out, In, InOut,
  Linear}` as closed-form cubics in `ease_at`, deterministic,
  monotonic, exact at both ends
  ([transition.rs](../../crates/oppa/src/transition.rs)).
- Evaluator: single A→B `Interp` per `(node, prop)`, fed per commit by
  `track_commit` with the frame clock; backends paint via
  `resolve_bg` / `resolve_opacity` through the evaluated plan builds
  ([builder.rs](../../crates/oppa-cpu/src/builder.rs)); `settle`
  retires; `created` / `suppressed` / `active_count` / `live_nodes`
  instrument; all three hooks consume it (cpu/vello/dom `hook.rs`).
- Stamps: per-commit `suppress_transitions` cancels live tracks and
  snaps (per-cause provenance deferred — standing v1 limit).
- Animatable set: `bg` + `opacity` only; everything else jumps **by
  lock**; transparent endpoints snap; sRGB lerp (gamma-correct is
  stated v2).
- Oracle standard: stamped sweeps show zero `created`-delta and
  per-frame image diffs (CPU exact, Vello tol-banded, DOM
  one-commit suppression) — see the contract
  [transitions.md](../03-spec/ui/transitions.md) and the M8 sweeps
  (`oppa-cpu` / `oppa-vello` `tests/m8_sweep.rs`,
  `oppa-dom/tests/m8_transitions.rs`).
- wasm drive: `WebApp::tick(now_ms)` advances TIME and syncs
  ([lib.rs](../../crates/oppa-web/src/lib.rs)); the browser harness
  is [webapp.mjs](../../spike/web/webapp.mjs).

## What v2 item 1 adds (proposed, not decided)

1. **Multi-keyframe tracks**: segment chains (A→B→C minimum) over the
   v1 animatable set, per-segment easing, evaluated through the same
   `track_commit` / `resolve_*` / `settle` pipeline with the same
   stamp semantics (stamped commits cancel live tracks and snap).
2. **Richer easings**: extensions of the closed-form set under the
   M8 constraint (deterministic, monotonic, exact at both ends —
   curve shape stays tuning, not contract).
3. **DOM mapping** for multi-segment tracks honoring the one-commit
   stamp (mechanism open — see Q4).
4. **wasm proof**: a keyframed scene driven by `tick()` in the
   browser harness — intermediate ticks change HTML, the tail
   settles to `None`, zero console errors.

## Acceptance (mechanical, M8 oracle standard)

- Keyframe tracks (≥2 segments) on `bg` and/or `opacity` with
  per-segment easing: unstamped tracks interpolate monotonically per
  segment and settle exact at the final target; stamped commits show
  zero `created`-delta and snap (counted, not inferred).
- Per-frame image diffs through the extended M8 sweeps: CPU exact,
  Vello tol-banded, DOM mapping rows green; M7 parity corpus still
  green.
- Browser harness: keyframed scene progresses across `tick()` calls
  and settles; `webapp.json` records pass with zero console errors.

## Boundary (this item does not)

- A general tween DSL / implicit-vs-explicit animation model stays
  out: [non-goals.md](../00-vision/non-goals.md) remains the true v1
  record (this item promotes only the deferred keyframe+easing half;
  the promotion is recorded here, the v1 page is untouched).
- No new animatable properties unless the build explicitly re-opens
  the decision-120 lock (see Q3) — geometry/shadow/text keep jumping
  until then.
- No change to the per-commit (not per-cause) stamp limit.

## Open questions (documented, never silently resolved)

- **Q1.** Keyframe authoring shape: `Style`-attached track type
  (the border/ink precedent) vs a separate primitive?
- **Q2.** Easing extension: parameterized cubic-bezier, more
  closed-form names, or both? (M8 constraint stands regardless.)
- **Q3.** Animatable set: keyframes over `bg`+`opacity` only, or does
  the build re-open the decision-120 lock? (Lock touch needs an
  explicit decision at build time.)
- **Q4.** DOM mapping for segments: CSS `@keyframes`, chained
  transitions, or stepped re-declarations with suppression?
- **Q5.** Stamp vs in-flight track: cancel + snap to final target
  (v1 behavior generalized) — or restart the track? Proposed
  default is v1 behavior; the build decides.
- **Q6.** M9-gate interplay: keyframe tracks reuse the
  `created`/`suppressed` instruments (intended); fuzzer hardening
  (v2 item 7) runs after items 1–3 minimum, so gate impact is
  measured then.
