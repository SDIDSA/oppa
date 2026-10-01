# DataGrid / VirtualList

Status: current (decisions 309, 310, 329). Source: `crates/oppa-controls/src/lib.rs`
(`DataGrid`, `DataGridProps`, `GridColumn`, `VirtualList`, `VirtualListProps`).

Virtualized collections over an author-owned keyed `Collection`
(stable `RowId`s, filter/sort/page; single-row writes notify only
that row's slot — decision 333). Both controls attach the
interactive `Scrollbar` overlay by default (thumb drag, track page,
wheel sync; 1200 ms idle fade — decision 329).

```rust
let coll = Collection::new(&ctx.host().runtime(), ctx.fetch_key("tasks"));
coll.ingest(sample_tasks());
ctx.child("app::Grid", 5,
    &DataGridProps::new(coll.clone(), columns, |_: &Task| 36.0)
        .size(640.0, 400.0),
    DataGrid::<Task>)
```

- `DataGridProps::new(rows, columns, row_height)` — 480x400 viewport
  + 28 px pinned header + 4-row overscan by default (`.size(..)`,
  `.filter(..)`, `.sort(..)` builders). `GridColumn { header, width,
  cell }` renders each cell; row width is the column sum (overflow
  past the viewport clips in v1 — stated).
- `VirtualListProps::new(rows, row_height, render_row)` is the
  single-column sibling (prefix-sum windows, slot recycle,
  Update-only rebinds — decision 309).
- Paged remote data streams in via `spawn_fetch_page`
  ([async-fetch](../async-fetch.md)); Task Studio (grid + search +
  sort + inspector) is the full precedent.
- Semantics: Generic + label (no table roles exist in v1 — adding
  roles would ripple every emitter, stated).

Related: [controls overview](overview.md).
