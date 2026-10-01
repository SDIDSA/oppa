# Glossary (current)

Status: current. Last verified: 2026-10-01.
Canonical terms — if any other document uses a different name for the same
concept, this file wins.

- **Signal / Memo / Effect / BatchGuard / untrack** — the five reactive primitives. Nothing else is a primitive.
- **Component** — a plain function `fn(&Ctx, &P) -> VNode`. Not a class; exists only in the ephemeral layer.
- **VNode** — the ephemeral tree a component returns (`Element | Text | Fragment | Hole`). Discarded after reconciliation.
- **RetainedNode / retained tree** — the stable-identity tree layout and renderers consume. Keyed by `NodeId` (generational arena id).
- **Tag** — the closed set of node kinds (`Div, Stack, Row, Column, Text, Image, ScrollArea, Grid, Canvas` + `Custom(u64)` escape hatch).
- **Style / StyleId** — typed style structs, interned into `StyleId`. Never "CSS class" or "stylesheet".
- **TreeDiff** — structural update (added/removed/moved ids + per-node payload deltas) sent to renderer backends.
- **FramePlan** — per-frame ordered display list + damage + layer plans, built from dirty subtrees only.
- **Caps** — backend capability negotiation (layers, blur/backdrop, MSAA).
- **Presenter / renderer backend** — implements `RendererBackend`; diff-driven; holds backend-mechanism state but no application state.
- **PlatformShell** — per-platform surface/window/IME/DPI/lifecycle.
- **TextService** — per-platform shaping/measurement/fallback.
- **Editing session** — framework-owned caret/selection/composition/undo state for the focused field, core-side.
- **Binding edge (`ctx.binding`)** — a memo variant whose value change is an identity event; stamps one commit with `suppress_transitions`.
- **PassMask** — per-node dirty flags (`STRUCTURE | STYLE | LAYOUT | PAINT | TEXT | SEMANTICS`).
- **DrawOp** — display-list operations (`Rect`, `RImg`, `RRect`, `Text`, `Path`, `PushClip`, `PushLayer`, `Pop`).
- **SemanticsDiff** — accessibility tree diff shipped in commits.
- **Validation marks (`invalid` / `required` / `error_message`)** — form-validation announcement payload (decision 352); validators stay app-side, the payload only announces.
- **Range bounds (`value_num` / `min_value` / `max_value`)** — the numeric half of the Slider/ProgressBar announcement (decision 352); `value_text` stays the human half, and `None` means no value interface.
- **AT action** — an assistive-technology invocation driving back into the framework (UIA Invoke/RangeValue, AT-SPI Action/Value, DOM native); always through host-loop callbacks, never direct framework access (decision 352).
- **Grid** — the minimal 2D container (`Tag::Grid`, decision 353): `Px`/`Fr`/`Auto` track templates, row-major auto-flow with spans only (no explicit placement); variable-height virtualized rows stay out.
- **Flex share** — weighted `flex_grow` remainder split over the intrinsic base (decision 353); `fill_width`/`fill_height` ride the same pool with weight 1; opt-in `flex_shrink` absorbs overflow.
- **2D scroll (`ScrollXY` / `ScrollOffset2D`)** — the plain-data 2D position value and the one view over the instance's `scroll` + `scroll_x` signals (decision 354); mixing 1D and 2D handles shares state, never forks it.
- **RichText (`TextSpan`)** — a multi-span text leaf sharing one size (decision 355): each span shapes with its own weight and paints with its own ink; concatenated bytes are the caret/selection space.
- **Keyframes** — a multi-stop track over `bg`+`opacity` with per-segment easing and once/loop/ping-pong playback (decision 357); the committed target closes the final leg.
- **Canvas (`CanvasOp`)** — a retained childless leaf painting a spec of rect/rounded-rect/path/text ops in local space (decision 358); lowers to existing `DrawOp`s, never a new one.
- **Call-site keying** — instance identity from `(Location::caller(), ordinal)` (decision 360): `ctx.child_auto` for static children, `ctx.child_keyed` for keyed siblings; no `TypeId`, so reload state survives the rlib↔dylib boundary.
- **Fetcher** — the pluggable fetch backend seam (decision 362): `fetch(url)` off-thread; scripted doubles, app closures, and the wasm platform binding meet here, never a built-in client.
- **Persisted** — signal-backed write-through persistence (decision 364): reads track, writes hit the signal and the `KvStore` synchronously; collections snapshot values in commit order through the same hook.
- **Slot** — a stable virtualization position in `ScrollArea`; keys are slots, not items.
