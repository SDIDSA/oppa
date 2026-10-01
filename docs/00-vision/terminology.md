# Terminology

Status: current. Canonical terms for this project. If older documents use
different names for the same concept, this file wins.

- **Signal / Memo / Effect / BatchGuard / untrack** — the five reactive
  primitives (locked #9). Nothing else is a primitive; `ctx.edit_session`
  is a service over them, not a sixth primitive.
- **Component** — a plain function `fn(&Ctx, &P) -> VNode`. Not a class.
  Components exist only in the ephemeral layer.
- **VNode** — the ephemeral tree a component returns
  (`Element | Text | Fragment | Hole`). Discarded after reconciliation.
- **RetainedNode / retained tree** — the stable-identity tree renderers
  and layout consume. Keyed by `NodeId` (generational arena id).
- **Tag** — the closed set of node kinds
  (`Div, Stack, Row, Column, Text, Image, ScrollArea` + `Custom(u64)`
  escape hatch). New core variants require restart; new component
  functions do not.
- **Style / StyleId** — typed style structs, interned into `StyleId`.
  Never "CSS class" or "stylesheet".
- **TreeDiff** — structural update (added/removed/moved ids + per-node
  payload deltas) sent to renderer backends.
- **FramePlan** — per-frame ordered display list + damage + layer plans,
  built from dirty subtrees only.
- **Caps** — backend capability negotiation (layers, blur/backdrop, MSAA).
- **Presenter / renderer backend** — implements `RendererBackend`;
  diff-driven; holds backend-mechanism state (DOM nodes, glyph atlases,
  layer caches, browser-hosted scroll/editing sessions) but no
  application/UI-model state.
- **PlatformShell** — per-platform surface/window/IME/DPI/lifecycle
  (`pump_events`, `request_frame`, `set_dpi_aware`, `set_ime`,
  `set_cursor`, `semantics`, `text`).
- **TextService** — per-platform shaping/measurement/fallback.
- **Editing session** — framework-owned caret/selection/composition/undo
  state for the focused field, core-side, over the five primitives.
- **Binding edge (`ctx.binding`)** — a memo variant whose value change is
  an identity event; stamps one commit with `suppress_transitions`.
- **PassMask** — per-node dirty flags
  (`STRUCTURE | STYLE | LAYOUT | PAINT | TEXT | SEMANTICS`).
- **DrawOp** — display-list operations (`Rect`, `RImg`, `RRect`, `Text`,
  `Path`, `PushClip`, `PushLayer`, `Pop`).
- **SemanticsDiff** — accessibility tree diff shipped in commits.
- **Slot** — a stable virtualization position in `ScrollArea`; keys are
  slots, not items.

Related: [widget-model](../01-design/widget-model.md),
[styling-model](../01-design/styling-model.md).
