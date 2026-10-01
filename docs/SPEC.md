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
- Handler identity is `(NodeId, kind)`; component state key is call-site hash + ordinal (type change panics). Static children key the same way via `ctx.child_auto` (call-site + ordinal, no `TypeId`); keyed siblings via `ctx.child_keyed` (explicit key); load-bearing manual keys (cross-instance addressing) keep `ctx.child`.
- Hot reload swaps code, never types; the retained tree is never referenced by user code, only by id.

## Layout

- Flexbox subset + block-lite + absolute (`.x` / `.absolute_y`) + minimal grid (`Tag::Grid`: Px/Fr/Auto tracks, row-major auto-flow, spans; no explicit placement). No variable-height rows.
- `flex_grow` splits the remainder by weight over the intrinsic base (`fill` ≡ weight 1); `flex_shrink` absorbs overflow opt-in; explicit sizes always win; resolved sizes clamp into `min`/`max` (explicit contradictions refuse loudly).
- Text is pre-shaped before positioning. Shaping units are device px (`font_px × DPR`); one `round_to_device_px` at commit positions only, extents stay subpixel.
- Effects reading settled metrics re-run next-frame `EFFECTS`.

## Rendering

- Backends receive `TreeDiff` + per-frame `FramePlan` (dirty subtrees only) with `DrawOp::{Rect,RImg,RRect,Text,Path,PushClip,PushLayer,Pop}` plus damage. `Canvas` lowers to those ops (never a new one).
- `Text(ShapedRun)` arrives pre-shaped with `baseline`, `em_size`, per-run `fonts`. `Color` stays alpha-less.
- Shadows blur natively per backend (Vello gaussian, CPU box-blur, CSS `box-shadow`); blur 0 stays the offset solid, pixel-exact everywhere.
- Static images serve one cache deposit on all three backends (CPU/Vello pixels, DOM data-URI PNG); bare URL keys unchanged on DOM.
- Oracle exactness: CPU incremental == full render, pixel 0; CPU-vs-Vello sharp 0, curves tol-16 ≤ 60; blurred shadows tol-16 < 10% pixels (hardware-calibrated); static GPU frames 0; scroll produces 0 structural ops with ≤ 1 frame trail.

## Input

- One normalized `InputEvent` (`Pointer` / `Scroll` / `Key` / `Ime` / `Focus`) everywhere.
- Pointer: hit-test deepest-wins, later-sibling on ties, no root fallback; capture on Down, Up only inside the capture subtree, Cancel clears.
- Keyboard: `Key(code, mods, state, repeat)`; Tab wraps in deterministic press-node DFS pre-order; Enter/Space pulse press; Escape blurs; Slider arrows step by `step` and `Home`/`End` jump to `min`/`max` (Phase 38a); Tree rows walk with Up/Down (visible neighbors), Left (collapse, else parent), Right (expand, else first child); Splitter arrows nudge the fraction ±0.05; DatePicker arrows step days (Left/Right ±1, Up/Down ±7, `Home`/`End` month edges) — all pinning quietly at their ends (Phase 38b); Toolbar/Menubar Left/Right rove the highlight cursor (wrapping, skipping separators/disabled), Enter invokes it, Menubar Down opens (Phase 38c); other keys go to the focused handler else no-op.
- Focus: at most one editing session; focus loss mid-composition commits; click follows focus.
- IME: single `dispatch_ime_event` seam for scripted + platform feeds; `ImeOps` via `PlatformShell::set_ime`.

## Text

- `TextService::{enumerate_fonts, shape → ShapedRun, measure_line}`. Caret = cluster leading edge; hit-test = cluster midpoint, ties → leading edge.
- Shaped-cluster rules: decomposed é = 1 cluster; ZWJ sequences = 1 cluster; combining-caret steps cluster starts (shaperless stays scalar).
- Fallback precedence: requested family → slice chain → loud `Backend U+XXXX` (never tofu/`.notdef`); unknown family is always loud. No color-emoji rendering in v1.
- Multi-span RichText: spans share one size; each span shapes with its own weight (no cross-span shaping, never re-shaped on wrap); concatenated bytes are the caret/selection space with leading affinity at span boundaries; ink splits paint (one op/span per ink run, single-ink scenes unchanged).
- Web display serves the measured font bytes (`@font-face` data URI); residual browser-shaper differences are stated tolerance, not contract.
- Editing: controlled components (content = author signal; caret/selection/composition/undo = core session, survives hot swap). Authority: GPU backend owns on GPU, DOM `<input>` owns on Web, shared op-suite is the contract. Clipboard is plain-text only via the `Clipboard` trait; non-Win32 clipboard refuses loudly.
- Form validation (Phase 38a, G7): `invalid` / `required` / `error_message` announce through `Semantics` (validators stay app-side — the payload only announces); `helper_text` renders a visual-only caption. `error_message` wins while `invalid`; controls without a message keep byte-identical trees.
- Pickers (Phase 38b): `Splitter` fraction clamps into `min_first_px` / `min_second_px` (contradictions — negative/non-finite geometry or minima exceeding the total — refuse loudly); `DatePicker` binds every path into `[min, max]` (arrows pin, month flips past the bound are `None`/disabled, out-of-range days render handlerless, parses outside the bound are ignored); `Tree` flatten refuses missing parents and parent loops loudly.
- Bars, files, leaves (Phase 38c): `Toolbar`/`Menubar` cells are handlerless visuals in one press-owner container (focus never fragments); taps hit-test, separators/disabled are quiet no-ops; `Menubar` opens at most one standalone `Menu` (stale indexes empty quietly) and its bar-blur edge closes unless focus moved into the list; `FilePicker` sets the controlled path on `Some(Ok)` (open takes the first path), dismissal and refusals surface in the caption and leave the path, missing backends refuse loudly; `RichText`/`Image`/`Canvas` display controls render the Phase 36 leaves (bare leaves byte-identical, labeled wrappers announce); `NavHost` renders the stack's current route and empties quietly past unknown names.
- Storage roots (Phase 38d, G23): `app_data_dir(app_name)` resolves the OS data dir (`%APPDATA%` / macOS support / `$XDG_DATA_HOME` or `~/.local/share`; wasm refuses loudly); root-escape names (empty, separators, `..`, absolute, drive-prefixed) refuse loudly before any env reads; the `FsSandbox` jail stays lexical (symlinks can point out — apps canonicalize at open when traversal matters).

## Transitions

- Style-delta animation over `bg`+`opacity`: single A→B tweens (`.transition(duration + easing)`; CSS subset on DOM, rest jumps) and multi-stop keyframe tracks (`.keyframes(stops + mode)`; per-segment easing; once/loop/ping-pong; keyframes win over tweens; the committed target closes the final leg).
- Keyframe tracks run on every backend (GPU resolvers; DOM stepped inline re-declarations per frame) and settle exact at the target (once); loop/ping-pong run until a new delta restarts or a stamp snaps.
- Binding-edge (`ctx.binding`) re-runs stamp the commit `suppress_transitions` for exactly one commit (per-commit, not per-cause) — live tracks cancel and snap, never restart.

## Reload freeze gate

- Frozen renderers: no retired-generation touch/apply/resume across the reload-during-X scenarios (fuzzer gate green). True unload was never built — retire-don't-unload stands; v2 scope.
