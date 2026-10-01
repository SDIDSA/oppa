# Layout API

Status: current (v1 scope + `DataGrid`, decision 310). Sources: `12-archive/DESIGN.md` §§2.2, 4.2;
[layout constraints](../03-spec/layout/constraints.md).

```rust
Style::new().size(44, 24).radius(12).pad_x(12).gap(2)
    .h(viewport_h).fill_width()
    .absolute_y(slot as f32 * row_h)
ScrollArea("list")
    .content_size(total_h)
    .children((0..n_slots).map(|slot| /* keyed Row per slot */))
```

`ctx.scroll_offset()` reads identically on all backends (GPU:
TIME-physics-fed; Web: INPUT-event-fed). Fixed-height virtualized
rows in `VirtualList`; column templates + pinned header + paged
fetch in `DataGrid` (decision 310 — keyed `Collection` source,
slot recycle, Update-only rebinds). Per-side pads/margins and
per-corner radii ride one shared clamp rule on all three
presenters (decision 305). Per-item transient state via
`ctx.keyed_state`. Variable-height rows stay v2.
