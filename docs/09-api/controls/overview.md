# Controls

Status: current (catalog ships — implementations in
`crates/oppa-controls/src/lib.rs` + `menu.rs`; patterns below
remain the design reference). Source:
`12-archive/DESIGN.md` §4.

Shipped catalog (each composes the core vocabulary — no new `Tag`,
decision 212; all controlled-state, press-only, disabled-drops-
handler, decision 213; `Uncontrolled*` companions where noted):

- **Inputs:** Button ([page](button.md)), Checkbox ([page](checkbox.md),
  controlled `Signal<bool>`), Toggle ([page](toggle.md)), Slider
  ([page](slider.md), step-snap + clamp), TextInput (masked, clipboard
  shortcuts), TextArea (multi-line, vertical caret nav), Radio +
  RadioGroup, Tabs, Select.
- **Display:** ProgressBar, Badge, Tooltip (dwell-mounted anchored card).
- **Collections:** VirtualList (prefix-sum windows, slot recycle),
  DataGrid (column templates, pinned header, paged fetch), Scrollbar
  (draggable overlay, wall-clock idle fade; auto-attached by both
  list controls).
- **Overlays:** Modal (viewport dim + focus trap), Menu / MenuItem /
  ContextMenu (arrows, hover highlight, viewport clamping,
  drag-select), Toast (transient `status` card, auto-dismiss).
- **Resilience:** ErrorBoundary (child-panic fallback + retry).

Documented patterns (the §4 originals — the catalog entries above
are their shipped form):

- **Toggle switch** (`12-archive/DESIGN.md` §4.1): props + local signals +
  `match` over `(enabled, pressed, hovered, is_on)` + declarative
  `.transition(...)` + inline `.semantics(...)` + `.on_press` —
  the six traditional toolkit pieces collapsed into one function.
- **Virtualized list cell** (`12-archive/DESIGN.md` §4.2): `ScrollArea` +
  spacer + slot keys + `ctx.binding` item memo + `ContactRow`
  re-derivation; selection via per-instance memos (exactly two
  rows re-render); `keyed_state` escape hatch for transient
  per-item state.
- **Editable field**: behavior-flagged component over existing tags;
  content in an author-owned signal; the focused field's session owns
  the value (the runner routes typing/Backspace/editing shortcuts
  there — decisions 243/246); clipboard ops via the `Clipboard`
  trait — shipped as TextInput / TextArea (masked, word-step,
  select-all/copy/cut/paste, vertical caret nav). Full contract:
  [editing spec](../../03-spec/text/editing.md).

Semantics extension (G2, decision 214, plus 241/244/245/247/251/337):
`Role::Button / Checkbox / Slider / Dialog / RadioButton / Tab /
TabList / ComboBox / ProgressBar / Status` with matching
`Semantics::{button, checkbox, slider, dialog, radio, tab,
tab_list, combobox, progressbar, status}` builders + `value_text`;
emitter arms in `oppa-dom` (ARIA), `oppa-atspi`, `oppa-uia`
(control types + Checkbox Toggle pattern; Button Invoke + Slider
RangeValue are OQ-G2-2).

Individual `controls/*.md` pages: button, checkbox, toggle,
slider, text-input (TextInput + TextArea), select, tabs, radio,
progress (ProgressBar + Badge), scrollbar, modal, toast, datagrid
(DataGrid + VirtualList), menu (Menu + ContextMenu + Tooltip),
error-boundary. The catalog is fully paged — remaining controls
are documented by their rustdoc + the rundown above.
