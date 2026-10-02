use crate::scrollbar::{Scrollbar, ScrollbarAxis, ScrollbarProps};
use crate::virtual_list::vlist_window;
use oppa::*;
use oppa_macros::Props;
use std::rc::Rc;
// ---------------------------------------------------------------------------
// DataGrid
// ---------------------------------------------------------------------------

/// Props for one grid cell's content (the column template receives
/// the stable [`Row`](oppa::Row) plus its laid geometry — content
/// fills the cell, never measures it — plus the source
/// [`Collection`](oppa::Collection) for granular `get_row` reads,
/// the [`VirtualRowProps`] twin — plus the grid's selection when
/// the author wires click-to-select (Round 24.1: `None` renders
/// selection-blind, like every pre-24.1 grid).
#[derive(Clone, Props)]
pub struct GridCellProps<T> {
    pub row: oppa::Row<T>,
    pub height: f32,
    pub width: f32,
    pub rows: oppa::Collection<T>,
    pub selected: Option<Signal<Option<oppa::RowId>>>,
}

/// One grid column: header label, fixed width, per-row cell
/// template (`fn`, not a closure — [`Ctx::child`] needs a stable
/// symbol for hot-reload identity, the M8/F6 rule).
#[derive(Clone)]
pub struct GridColumn<T> {
    pub header: SharedString,
    pub width: f32,
    pub cell: fn(&Ctx, &GridCellProps<T>) -> VNode,
}

/// Data grid props (controlled — the [`Collection`](oppa::Collection)
/// is author-owned; scroll position rides the framework-owned
/// per-instance offset like [`VirtualList`]).
#[derive(Clone, Props)]
pub struct DataGridProps<T> {
    /// Data source (version-tracked reads — page appends re-derive).
    pub rows: oppa::Collection<T>,
    /// Keep predicate (`None` keeps all — construction-time, like
    /// [`VirtualListProps`]).
    pub filter: Option<oppa::RowFilter<T>>,
    /// Stable ordering (`None` keeps commit order).
    pub sort: Option<oppa::RowSort<T>>,
    /// Fixed-width columns (the row width is their sum — authors
    /// size to the viewport; overflow past it clips in v1, stated).
    pub columns: Vec<GridColumn<T>>,
    /// Per-row height (variable — same prefix-sum machinery as
    /// [`VirtualList`]).
    pub row_height: Rc<dyn Fn(&T) -> f32>,
    /// Header height (pinned — see [`DataGrid`]).
    pub header_height: f32,
    /// Extra rows materialized above/below the visible band.
    pub overscan: usize,
    /// Viewport geometry (explicit — decision 213).
    pub width: f32,
    pub height: f32,
    /// Accessible name (Generic + label — no table roles exist in
    /// v1 semantics; adding roles would ripple every emitter,
    /// stated, not smuggled).
    pub label: SharedString,
    /// Debug label for the scroll container.
    pub debug: SharedString,
    /// Attached scrollbar overlay (Round 21.2, decision 329):
    /// same contract as [`VirtualListProps::scrollbar`] — the
    /// grid's instance offset drives the thumb.
    pub scrollbar: bool,
    /// Click-to-select (Round 24.1, decision 335): `Some` signal
    /// receiving the selected [`RowId`](oppa::RowId) (`None`
    /// clears); `None` renders selection-blind (every pre-24.1
    /// grid). Cell templates read it for highlight + actions.
    pub selected: Option<Signal<Option<oppa::RowId>>>,
}

impl<T> DataGridProps<T> {
    /// Defaults: no filter/sort, 4 rows overscan (the M8 margin),
    /// 28px header, 480×400 viewport, `"grid"` label/container.
    pub fn new(
        rows: oppa::Collection<T>,
        columns: Vec<GridColumn<T>>,
        row_height: impl Fn(&T) -> f32 + 'static,
    ) -> Self {
        Self {
            rows,
            filter: None,
            sort: None,
            columns,
            row_height: Rc::new(row_height),
            header_height: 28.0,
            overscan: 4,
            width: 480.0,
            height: 400.0,
            label: SharedString::from("data grid"),
            debug: SharedString::from("dgrid"),
            scrollbar: true,
            selected: None,
        }
    }

    /// Keep predicate (see [`Self::filter`]).
    pub fn filter(mut self, f: impl Fn(&T) -> bool + 'static) -> Self {
        self.filter = Some(Rc::new(f));
        self
    }

    /// Stable ordering (see [`Self::sort`]).
    pub fn sort(mut self, f: impl Fn(&T, &T) -> std::cmp::Ordering + 'static) -> Self {
        self.sort = Some(Rc::new(f));
        self
    }

    /// Viewport geometry (explicit sizes — decision 213).
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Overscan margin in rows.
    pub fn overscan(mut self, n: usize) -> Self {
        self.overscan = n;
        self
    }

    /// Debug label for the scroll container.
    pub fn debug(mut self, label: impl Into<SharedString>) -> Self {
        self.debug = label.into();
        self
    }

    /// Attached scrollbar overlay (see [`Self::scrollbar`]).
    pub fn scrollbar(mut self, show: bool) -> Self {
        self.scrollbar = show;
        self
    }

    /// Click-to-select signal (see [`Self::selected`]).
    pub fn selected(mut self, selected: Signal<Option<oppa::RowId>>) -> Self {
        self.selected = Some(selected);
        self
    }
}

/// Data grid: a [`VirtualList`] window with per-column templates
/// and a sticky header (Round 13.3, decision 310; viewport-relative
/// coordinates Round 24.2). Only window rows materialize (stable slot
/// keys + `ctx.child` cells keyed by slot × column — scrolling
/// rebinds Update-only); the header pins at the viewport top with
/// `absolute_y(0.0)` while rows render at
/// `header_height + tops[idx] - scroll_y` (desktop `ScrollArea`s clip
/// without translating, so content coordinates must subtract the
/// offset to move). The header rides inside the scroll container, so
/// horizontal pans move header and rows together, and it paints above
/// by child order. Filter updates and page appends re-derive the
/// window the same frame, no remount.
pub fn DataGrid<T: Clone + 'static>(ctx: &Ctx, props: &DataGridProps<T>) -> VNode {
    assert!(
        !props.columns.is_empty(),
        "DataGrid needs at least one column — a zero-column grid is an authoring bug, never an empty table"
    );
    let t = ctx.theme().tokens();
    let offset = ctx.scroll_offset();
    // Tracked read: window, header pin, and rows re-derive when the
    // offset moves OR the collection mutates (one version signal).
    let y = offset.get();
    let ncols = props.columns.len();
    let total_w: f32 = props.columns.iter().map(|c| c.width).sum();
    let query = oppa::CollectionQuery {
        filter: props.filter.clone(),
        sort: props.sort.clone(),
        offset: 0,
        limit: None,
    };
    let page = props.rows.query(&query);
    let n = page.total;
    // Prefix sums (tops[i] = content y of row i).
    let mut tops = Vec::with_capacity(n + 1);
    let mut cursor = 0.0f32;
    for row in &page.rows {
        tops.push(cursor);
        cursor += (props.row_height)(&row.value);
    }
    let total = cursor;
    // The pinned header occupies the top of the viewport, so the row
    // window covers the remaining body height (Round 24.2).
    let body_h = (props.height - props.header_height).max(0.0);
    let (first, end) = vlist_window(&tops, total, y, body_h, props.overscan);
    let header_cells = props
        .columns
        .iter()
        .map(|col| {
            Div("dgrid-headcell")
                .style(Style::new().w(col.width).h(props.header_height).build())
                .child(VNode::from(Text::new(col.header.clone()).size(12).bold()))
        })
        .collect::<Vec<_>>();
    let mut children = page.rows[first..end]
        .iter()
        .enumerate()
        .map(|(slot, row)| {
            let idx = first + slot;
            let h = tops.get(idx + 1).copied().unwrap_or(total) - tops[idx];
            let cells = props
                .columns
                .iter()
                .enumerate()
                .map(|(col, column)| {
                    let cell_props = GridCellProps {
                        row: row.clone(),
                        height: h,
                        width: column.width,
                        rows: props.rows.clone(),
                        selected: props.selected.clone(),
                    };
                    Div("dgrid-cell")
                        .style(Style::new().w(column.width).h(h).build())
                        .child(ctx.child(
                            "oppa::DGridCell",
                            (slot * ncols.max(1) + col) as u64,
                            &cell_props,
                            column.cell,
                        ))
                })
                .collect::<Vec<_>>();
            Row("dgrid-row")
                .style(
                    Style::new()
                        .absolute_y(props.header_height + tops[idx] - y)
                        .h(h)
                        .w(total_w)
                        .build(),
                )
                .key(slot as u64)
                .children(cells)
        })
        .collect::<Vec<_>>();
    // Sticky header, pinned to the viewport top (keyed — stable
    // identity across windows, never remounted by slides).
    children.push(
        Row("dgrid-header")
            .style(
                Style::new()
                    .absolute_y(0.0)
                    .h(props.header_height)
                    .w(total_w)
                    .bg(t.surface)
                    .border_bottom(1, t.border)
                    .align_items(AlignItems::Center)
                    .build(),
            )
            .key(u64::MAX)
            .semantics(Semantics::default().label(&props.label))
            .children(header_cells),
    );
    let area = oppa::ScrollArea(&props.debug)
        .style(
            Style::new()
                .size(props.width, props.height)
                // Rows scroll beneath the pinned header, so the extent
                // spans both: max offset parks the last row bottom at
                // the viewport bottom (Round 24.2).
                .content_size(total + props.header_height)
                .build(),
        )
        .on_scroll(|| {})
        .children(children);
    if !props.scrollbar {
        return area;
    }
    // Attached overlay (Round 21.2, decision 329): same contract as
    // VirtualList — the grid's instance offset drives the thumb.
    let bar = ctx.child_auto(
        &ScrollbarProps {
            target: props.debug.clone(),
            offset: offset.clone(),
            idle_hide_ms: Some(1200),
            axis: ScrollbarAxis::Vertical,
        },
        Scrollbar,
    );
    Div(format!("{}-wrap", props.debug).as_str()).children([area, bar])
}
