# Scrollbar

Status: current (decisions 318, 329). Source: `crates/oppa-controls/src/lib.rs`
(`Scrollbar`, `ScrollbarProps`).

Draggable overlay scrollbar over a `ScrollArea` viewport: track +
thumb that follows the content ratio (`max(24, viewport^2/content)`,
linear in the clamped offset). Track tap page-scrolls; thumb drag
captures the pointer; focused arrows page. Shows if and only if the
content overflows.

```rust
// Manual wiring over a raw ScrollArea (VirtualList/DataGrid attach
// automatically -- this is the escape hatch, not the daily path).
ctx.child("app::Bar", 7,
    &ScrollbarProps {
        target: SharedString::from("list"),
        offset: ctx.scroll_offset(),
        idle_hide_ms: Some(1200),
    },
    Scrollbar)
```

- `VirtualList` / `DataGrid` render the overlay sharing their
  instance offset (default on; `scrollbar` builder opts out) with
  viewport/extent from the settled box + `content_size`.
- `idle_hide_ms: Some(ms)` fades the chrome out after inactivity
  through the `use_timeout` hook (last event wins; hover/press keep
  it); `None` keeps the event-driven path (legacy, byte-identical).
- The press node spans a transparent 20 px gutter around the
  painted 12 px bar (summons without pixel-hunting; gutter presses
  page/drag — stated tradeoff).

Related: [controls overview](overview.md), [datagrid](datagrid.md).
