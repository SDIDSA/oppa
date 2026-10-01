# Styling model

Status: current as design (locked #8); M2 interning implemented.
Sources: `12-archive/DESIGN.md` §§2.2, 4; code: `crates/oppa/src/style.rs`,
`crates/oppa/src/interner.rs`.

Typed style structs, **not a CSS cascade** (locked #8):

```rust
struct Style {
    layout:   LayoutProps,    // flex/stack, optional grid, box model
    paint:    PaintProps,     // bg, border, radius, shadow, opacity, clip, text
    behavior: BehaviorProps,  // scrollable, focusable, pointer_mode, semantics
}
```

No specificity, no cascade merging. A small explicit set (text style,
direction) inherits down the tree. Themes are token tables resolved to
`Style`s — a theme flip is a `StyleId` change per node (cheap). Styles
are interned (`StyleId` + table), so payloads are shared across
thousands of nodes; interning happens once at the reconcile boundary so
component bodies stay pure values (M2 delta).

Structural customization is by **slots** (`props.leading:
Option<Component>` — direct function composition), not by
stringly-typed template-part contracts.

See also: [API: styling](../09-api/styling.md),
[widget-model](widget-model.md).
