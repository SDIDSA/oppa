use crate::scrollbar::{Scrollbar, ScrollbarAxis, ScrollbarProps};
use oppa::*;
use oppa_macros::Props;
use std::rc::Rc;
// ---------------------------------------------------------------------------
// VirtualList
// ---------------------------------------------------------------------------

/// First/last visible row indexes (half-open `[first, end)`) for a
/// variable-height window (Round 13.2, decision 309): pure over the
/// prefix-sum `tops` (+ `total` bottom) — shared by the control and
/// its tests so both compute the same window. `first` backs up
/// `overscan` rows from the first row whose bottom edge passes `y`;
/// `end` advances `overscan` past the last row whose top edge
/// precedes `y + viewport_h`. Empty and past-the-end inputs yield
/// an empty window (quiet — same class as unmatched controlled
/// edges, documented not silent).
pub fn vlist_window(
    tops: &[f32],
    total: f32,
    y: f32,
    viewport_h: f32,
    overscan: usize,
) -> (usize, usize) {
    let n = tops.len();
    if n == 0 || viewport_h <= 0.0 {
        return (0, 0);
    }
    let bottoms = |i: usize| {
        if i + 1 < n {
            tops[i + 1]
        } else {
            total
        }
    };
    // First row with bottom > y (binary search over the monotone
    // bottoms — the variable-height generalization of M8's
    // `window_first` division).
    let mut lo = 0usize;
    let mut hi = n;
    while lo < hi {
        let mid = (lo + hi) / 2;
        if bottoms(mid) <= y {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let first = lo.saturating_sub(overscan).min(n);
    // First row with top >= y + viewport_h, then overscan forward.
    let edge = y + viewport_h;
    let mut lo = 0usize;
    let mut hi = n;
    while lo < hi {
        let mid = (lo + hi) / 2;
        if tops[mid] < edge {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let end = (lo + overscan).min(n);
    (first.min(end), end)
}

/// Props for one virtualized row's content (the `render_row`
/// component receives the stable [`Row`](oppa::Row) plus its laid
/// height — content fills the slot, never measures it — plus the
/// source [`Collection`](oppa::Collection) so rows can read their
/// own payload granularly through `get_row` (Round 23.1 —
/// `update_row` then re-renders exactly the touched row).
#[derive(Clone, Props)]
pub struct VirtualRowProps<T> {
    pub row: oppa::Row<T>,
    pub height: f32,
    pub rows: oppa::Collection<T>,
}

/// Virtualized list props (controlled — the [`Collection`](oppa::Collection)
/// is author-owned; scroll position rides the framework-owned
/// per-instance offset like the M8 trace).
#[derive(Clone, Props)]
pub struct VirtualListProps<T> {
    /// Data source (version-tracked reads — appends re-derive).
    pub rows: oppa::Collection<T>,
    /// Keep predicate (`None` keeps all — construction-time, like
    /// the M8 window fns; swapping closures re-derives on the next
    /// tracked run).
    pub filter: Option<oppa::RowFilter<T>>,
    /// Stable ordering (`None` keeps commit order).
    pub sort: Option<oppa::RowSort<T>>,
    /// Per-row height (variable — prefix sums drive offsets; must
    /// be finite non-negative, refused loudly downstream by layout
    /// like every bad size, never silently clamped).
    pub row_height: Rc<dyn Fn(&T) -> f32>,
    /// Row content (`fn`, not a closure — [`Ctx::child`] needs a
    /// stable symbol for hot-reload identity, the M8/F6 rule).
    pub render_row: fn(&Ctx, &VirtualRowProps<T>) -> VNode,
    /// Extra rows materialized above/below the visible band.
    pub overscan: usize,
    /// Viewport geometry (explicit — decision 213).
    pub width: f32,
    pub height: f32,
    /// Debug label for the scroll container.
    pub debug: SharedString,
    /// Attached scrollbar overlay (Round 21.2, decision 329):
    /// `true` renders the interactive 17.2 `Scrollbar` over the
    /// viewport, sharing this instance's offset signal (thumb drag,
    /// track page-scroll, and wheel stay in sync by construction)
    /// with a 1200ms wall-clock idle fade. Viewport height and
    /// total extent flow from the settled box + `content_size` —
    /// no second source.
    pub scrollbar: bool,
}

impl<T> VirtualListProps<T> {
    /// Defaults: no filter/sort, 4 rows overscan (the M8 margin),
    /// 320×400 viewport, `"vlist"` container.
    pub fn new(
        rows: oppa::Collection<T>,
        row_height: impl Fn(&T) -> f32 + 'static,
        render_row: fn(&Ctx, &VirtualRowProps<T>) -> VNode,
    ) -> Self {
        Self {
            rows,
            filter: None,
            sort: None,
            row_height: Rc::new(row_height),
            render_row,
            overscan: 4,
            width: 320.0,
            height: 400.0,
            debug: SharedString::from("vlist"),
            scrollbar: true,
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
}

/// Virtualized list: a `ScrollArea` window over a
/// [`Collection`](oppa::Collection) (Round 13.2, decision 309).
/// Only the window materializes (slot `Div`s with stable
/// slot-index keys + one `ctx.child` row each — the M8 recycle
/// shape, so scrolling commits LAYOUT-only updates, never
/// structure); row payloads stay data until their slot binds.
/// Variable heights flow through prefix sums (shared [`vlist_window`]
/// math — binary-searched, never divided). The extent comes from
/// the match total, so the DOM spacer + scrollbar follow for free.
///
/// Filter/sort are construction-time (see [`VirtualListProps`]);
/// the collection version re-derives everything live (appends
/// grow the extent, removals collapse it — same frame, no
/// remount).
pub fn VirtualList<T: Clone + 'static>(ctx: &Ctx, props: &VirtualListProps<T>) -> VNode {
    let offset = ctx.scroll_offset();
    // Tracked read: the window re-derives when the offset moves OR
    // the collection mutates (one version signal — Store-coarse,
    // memo gates dedup downstream).
    let y = offset.get();
    let query = oppa::CollectionQuery {
        filter: props.filter.clone(),
        sort: props.sort.clone(),
        offset: 0,
        limit: None,
    };
    let page = props.rows.query(&query);
    let n = page.total;
    // Prefix sums (tops[i] = content y of row i; total bottom).
    let mut tops = Vec::with_capacity(n + 1);
    let mut cursor = 0.0f32;
    for row in &page.rows {
        tops.push(cursor);
        cursor += (props.row_height)(&row.value);
    }
    let total = cursor;
    // Desktop ScrollAreas clip without translating: viewport-relative
    // slots subtract the offset (Round 24.2 — content coordinates
    // would otherwise never move on screen).
    let (first, end) = vlist_window(&tops, total, y, props.height, props.overscan);
    let width = props.width;
    let children = page.rows[first..end]
        .iter()
        .enumerate()
        .map(|(slot, row)| {
            let idx = first + slot;
            let h = tops.get(idx + 1).copied().unwrap_or(total) - tops[idx];
            let row_props = VirtualRowProps {
                row: row.clone(),
                height: h,
                rows: props.rows.clone(),
            };
            Div("vlist-slot")
                .style(Style::new().absolute_y(tops[idx] - y).h(h).w(width).build())
                .key(slot as u64)
                .child(ctx.child_keyed(slot as u64, &row_props, props.render_row))
        })
        .collect::<Vec<_>>();
    let area = oppa::ScrollArea(&props.debug)
        .style(
            Style::new()
                .size(width, props.height)
                .content_size(total)
                .build(),
        )
        .on_scroll(|| {})
        .children(children);
    if !props.scrollbar {
        return area;
    }
    // Attached overlay (Round 21.2, decision 329): the same offset
    // signal drives rows and thumb together (wheel, thumb drag, and
    // track page-scroll stay in sync by construction); viewport
    // height + total extent flow from the settled box + the
    // `content_size` above — no second source. The overlay is a
    // portal (out-of-flow), so the wrapper sizes exactly to the
    // area and committed boxes move nowhere.
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
