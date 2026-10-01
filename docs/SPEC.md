# Behavioral contracts (current)

Status: current. Last verified: 2026-10-01.
Observable guarantees only. Where mechanisms differ per backend, the shared
contract test suite — not prose — defines sameness.

## Reactivity and propagation

- Invalidation runs topo-by-depth, at most 1 run per node per pass, 3 passes per frame (debug panics with the chain; release defers once, then parks and logs).
- Memos never write signals — panic in all profiles.
- Single UI thread; `ctx.spawn` work is generation-tagged and discarded if stale; timers pause while lifecycle is suspended.

## Reconciliation

- Old + new VNode diff to `TreeDiff`. Keyed children diff in place; unkeyed by order; incompatible types replace.
- Fixed pass-mask map: structure → `STRUCTURE|LAYOUT|PAINT`; style-id subset; text → `TEXT|PAINT`; semantics → `SEMANTICS`; handler kind-set → `PAINT`.
- Handler identity is `(NodeId, kind)`; component state key is call-site hash + ordinal (type change panics).
- Hot reload swaps code, never types; the retained tree is never referenced by user code, only by id.

## Layout

- Flexbox subset + block-lite + absolute (`.x` / `.absolute_y`) + minimal grid (`Tag::Grid`: Px/Fr/Auto tracks, row-major auto-flow, spans; no explicit placement). No variable-height rows.
- `flex_grow` splits the remainder by weight over the intrinsic base (`fill` ≡ weight 1); `flex_shrink` absorbs overflow opt-in; explicit sizes always win; resolved sizes clamp into `min`/`max` (explicit contradictions refuse loudly).
- Text is pre-shaped before positioning. Shaping units are device px (`font_px × DPR`); one `round_to_device_px` at commit positions only, extents stay subpixel.
- Effects reading settled metrics re-run next-frame `EFFECTS`.

## Rendering

- Backends receive `TreeDiff` + per-frame `FramePlan` (dirty subtrees only) with `DrawOp::{Rect,RImg,RRect,Text,Path,PushClip,PushLayer,Pop}` plus damage.
- `Text(ShapedRun)` arrives pre-shaped with `baseline`, `em_size`, per-run `fonts`. `Color` stays alpha-less.
- Oracle exactness: CPU incremental == full render, pixel 0; CPU-vs-Vello sharp 0, curves tol-16 ≤ 60; static GPU frames 0; scroll produces 0 structural ops with ≤ 1 frame trail.

## Input

- One normalized `InputEvent` (`Pointer` / `Scroll` / `Key` / `Ime` / `Focus`) everywhere.
- Pointer: hit-test deepest-wins, later-sibling on ties, no root fallback; capture on Down, Up only inside the capture subtree, Cancel clears.
- Keyboard: `Key(code, mods, state, repeat)`; Tab wraps in deterministic press-node DFS pre-order; Enter/Space pulse press; Escape blurs; other keys go to the focused handler else no-op.
- Focus: at most one editing session; focus loss mid-composition commits; click follows focus.
- IME: single `dispatch_ime_event` seam for scripted + platform feeds; `ImeOps` via `PlatformShell::set_ime`.

## Text

- `TextService::{enumerate_fonts, shape → ShapedRun, measure_line}`. Caret = cluster leading edge; hit-test = cluster midpoint, ties → leading edge.
- Shaped-cluster rules: decomposed é = 1 cluster; ZWJ sequences = 1 cluster; combining-caret steps cluster starts (shaperless stays scalar).
- Fallback precedence: requested family → slice chain → loud `Backend U+XXXX` (never tofu/`.notdef`); unknown family is always loud. No color-emoji rendering in v1.
- Editing: controlled components (content = author signal; caret/selection/composition/undo = core session, survives hot swap). Authority: GPU backend owns on GPU, DOM `<input>` owns on Web, shared op-suite is the contract. Clipboard is plain-text only via the `Clipboard` trait; non-Win32 clipboard refuses loudly.

## Transitions

- v1 animates style-delta A→B interpolation only (`.transition(duration + easing)`; CSS subset on DOM, rest jumps).
- Binding-edge (`ctx.binding`) re-runs stamp the commit `suppress_transitions` for exactly one commit (per-commit, not per-cause).

## Reload freeze gate

- Frozen renderers: no retired-generation touch/apply/resume across the reload-during-X scenarios (fuzzer gate green). True unload was never built — retire-don't-unload stands; v2 scope.
