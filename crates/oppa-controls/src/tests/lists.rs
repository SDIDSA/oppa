use super::*;
use oppa::input::InputEvent;
use oppa::{
    find_retained_by_debug, Color, Column, ComponentHost, Ctx, Div, NodeId, Props, Semantics,
    SharedString, Style, VNode,
};
use oppa_macros::Props;
use std::rc::Rc;

struct VListHarness {
    host: ComponentHost,
    offset: oppa::ScrollOffset,
    list: NodeId,
    coll: oppa::Collection<VItem>,
}

fn mount_vlist(n_items: usize) -> VListHarness {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:vlist"));
    coll.ingest((0..n_items).map(|i| VItem { i }).collect::<Vec<_>>());
    let props = VirtualListProps::new(coll.clone(), vitem_h, vitem_row)
        .size(320.0, VL_VIEW_H)
        .overscan(VL_OVER)
        .debug("vlist");
    let handle = host.mount("VList", props, VirtualList);
    host.run_until_idle();
    let root = handle.root_instance();
    let offset = host.instance_scroll(root).expect("scroll handle");
    let list = find_retained_by_debug(&host, "vlist")[0];
    VListHarness {
        host,
        offset,
        list,
        coll,
    }
}

fn slot_bg(h: &VListHarness, id: NodeId) -> Option<Color> {
    // The zebra lives on the row content (the slot only
    // positions) — match the content node by box.
    let b = h.host.committed_box(id).expect("slot laid out");
    let row = find_retained_by_debug(&h.host, "vlist-row")
        .into_iter()
        .find(|r| h.host.committed_box(*r).is_some_and(|rb| approx(rb.y, b.y)))
        .expect("row content at slot box");
    let sid = h.host.retained_style(row).expect("row style");
    h.host
        .with_retained_mut(|_, styles| styles.get(sid).expect("style").bg)
}

/// Window math on the alternating pattern: [0,200]+2 overscan
/// covers rows 0..9 (tops 0..180, 7 visible + 2 over).
#[test]
fn virtual_list_windows_bounded_retained_nodes() {
    let h = mount_vlist(VL_N);
    assert_eq!(
        vlist_window(&[0.0, 20.0, 60.0], 80.0, 0.0, VL_VIEW_H, VL_OVER),
        (0, 3)
    );
    let slots = find_retained_by_debug(&h.host, "vlist-slot");
    assert_eq!(slots.len(), 9, "window [0,9), not all {VL_N}");
    // Slots sit at exact prefix offsets with exact heights.
    let mut seen: Vec<(f32, f32)> = slots
        .iter()
        .map(|id| {
            let b = h.host.committed_box(*id).expect("slot laid out");
            (b.y, b.h)
        })
        .collect();
    seen.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("finite"));
    for (slot, (y, hh)) in seen.iter().enumerate() {
        assert!(
            approx(*y, vitem_top(slot)),
            "slot {slot} at {y}, want {}",
            vitem_top(slot)
        );
        assert!(
            approx(*hh, vitem_h(&VItem { i: slot })),
            "slot {slot} h {hh}"
        );
    }
    // Zebra follows the item (slot 0 shows row 0 — even).
    let top_slot = slots
        .into_iter()
        .find(|id| h.host.committed_box(*id).is_some_and(|b| approx(b.y, 0.0)))
        .expect("slot at y=0");
    assert_eq!(slot_bg(&h, top_slot), Some(Color(0x22_22_22)));
    // Extent covers all rows (DOM spacer + scrollbar source).
    let list_box = h.host.committed_box(h.list).expect("list laid out");
    assert!(
        approx(list_box.content_h, 30_000.0),
        "1000 rows × 30 mean, got {}",
        list_box.content_h
    );
}

/// Scrolling moves the window and recycles slots: 500px lands
/// [15,26) with identical slot nodes (structure-free), exact
/// offsets, and item-following zebra.
#[test]
fn virtual_list_scroll_moves_window_recycles_slots() {
    let h = mount_vlist(VL_N);
    let before: std::collections::HashSet<NodeId> = find_retained_by_debug(&h.host, "vlist-slot")
        .into_iter()
        .collect();
    assert_eq!(before.len(), 9);
    h.offset.set(500.0);
    h.host.run_until_idle();
    let after: std::collections::HashSet<NodeId> = find_retained_by_debug(&h.host, "vlist-slot")
        .into_iter()
        .collect();
    // Window grows 9 → 11 (two genuinely new slots mount)...
    assert_eq!(after.len(), 11, "window [15,26)");
    // ...but the 9 survivors are identical nodes (recycle).
    assert_eq!(
        before.intersection(&after).count(),
        9,
        "survivors recycle, no teardown"
    );
    // First slot sits at tops[15] = 440, viewport-relative at
    // 440 - 500 = -60 with row 15's height.
    let first = after
        .into_iter()
        .find(|id| {
            h.host
                .committed_box(*id)
                .is_some_and(|b| approx(b.y, vitem_top(15) - 500.0))
        })
        .expect("slot at y=-60");
    let b = h.host.committed_box(first).expect("slot laid out");
    assert!(approx(b.h, 40.0), "row 15 is odd → 40, got {}", b.h);
    assert_eq!(
        slot_bg(&h, first),
        Some(Color(0x33_33_33)),
        "zebra follows row 15 (odd)"
    );
}

/// Same-size windows rebind Update-only (y=60 → y=120 slides
/// [0,11) → [2,13): 11 stable slot keys recycle, positions move
/// through LAYOUT, no structure — the M8 recycle shape for
/// variable heights).
#[test]
fn virtual_list_same_size_window_rebinds_update_only() {
    let h = mount_vlist(VL_N);
    h.offset.set(60.0);
    h.host.run_until_idle();
    assert_eq!(
        find_retained_by_debug(&h.host, "vlist-slot").len(),
        11,
        "window [0,11)"
    );
    h.offset.set(120.0);
    h.host.run_until_idle();
    assert_eq!(
        find_retained_by_debug(&h.host, "vlist-slot").len(),
        11,
        "window [2,13)"
    );
    let d = h.host.last_diff().expect("scroll diff");
    assert_eq!(d.structure_ops(), 0, "same-size windows rebind Update-only");
    // Window slid a full period: first slot at tops[2] = 60,
    // viewport-relative at 60 - 120 = -60.
    let first = find_retained_by_debug(&h.host, "vlist-slot")
        .into_iter()
        .find(|id| {
            h.host
                .committed_box(*id)
                .is_some_and(|b| approx(b.y, vitem_top(2) - 120.0))
        })
        .expect("slot at y=-60");
    assert_eq!(
        slot_bg(&h, first),
        Some(Color(0x22_22_22)),
        "zebra follows row 2 (even)"
    );
}

/// Empty collections render an empty viewport (quiet — same
/// class as unmatched controlled edges, never a panic). The
/// childless ScrollArea takes the leaf arm (content = own box),
/// so the spacer matches the viewport — no phantom scrollbar.
#[test]
fn virtual_list_empty_renders_no_slots() {
    let h = mount_vlist(0);
    assert_eq!(
        find_retained_by_debug(&h.host, "vlist-slot").len(),
        0,
        "no rows, no slots"
    );
    let list_box = h.host.committed_box(h.list).expect("list laid out");
    assert!(approx(list_box.h, VL_VIEW_H), "viewport box");
    assert!(
        approx(list_box.content_h, VL_VIEW_H),
        "leaf fallback: content = own box, got {}",
        list_box.content_h
    );
}

/// Round 13.1 → 13.2 combined: a worker batch grows the extent
/// live — appends re-derive the window the same frame, no
/// remount.
#[test]
fn virtual_list_async_append_grows_content() {
    let h = mount_vlist(10);
    let list_box = h.host.committed_box(h.list).expect("list laid out");
    assert!(
        approx(list_box.content_h, 300.0),
        "10 rows × 30, got {}",
        list_box.content_h
    );
    let writer = h.coll.writer();
    let rt = h.host.runtime();
    rt.spawn_task(move |scope| {
        writer.submit(&scope, (10..20).map(|i| VItem { i }).collect::<Vec<_>>());
    });
    let mut waited = 0;
    loop {
        let stats = rt.stats();
        if stats.tasks_done + stats.tasks_dropped >= 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        waited += 1;
        assert!(waited < 10_000, "executor task never resolved");
    }
    h.host.run_until_idle();
    assert_eq!(h.coll.len(), 20, "batch landed");
    let list_box = h.host.committed_box(h.list).expect("list laid out");
    assert!(
        approx(list_box.content_h, 600.0),
        "extent doubled live, got {}",
        list_box.content_h
    );
    // New rows are reachable: scroll to the bottom, window
    // shows row 19 viewport-relative (560 - 590 = -30; id 19,
    // odd → tall + dim zebra).
    h.offset.set(590.0);
    h.host.run_until_idle();
    let bottom = find_retained_by_debug(&h.host, "vlist-slot")
        .into_iter()
        .find(|id| {
            h.host
                .committed_box(*id)
                .is_some_and(|b| approx(b.y, vitem_top(19) - 590.0))
        })
        .expect("slot at row 19");
    let b = h.host.committed_box(bottom).expect("slot laid out");
    assert!(approx(b.h, 40.0), "row 19 h 40, got {}", b.h);
    assert_eq!(slot_bg(&h, bottom), Some(Color(0x33_33_33)));
}

// ------------------------------------------------------------------
// Round 13.3 (decision 310): data grid over a collection
// ------------------------------------------------------------------

struct GridHarness {
    host: ComponentHost,
    offset: oppa::ScrollOffset,
    list: NodeId,
    coll: oppa::Collection<VItem>,
}

fn mount_grid(n_items: usize) -> (GridHarness, oppa::MountHandle<DataGridProps<VItem>>) {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.set_text_service(Box::new(FakeText));
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:dgrid"));
    coll.ingest((0..n_items).map(|i| VItem { i }).collect::<Vec<_>>());
    let props = DataGridProps::new(coll.clone(), grid_cols(), vitem_h)
        .size(GRID_VIEW_W, GRID_VIEW_H)
        .overscan(VL_OVER)
        .debug("dgrid");
    let handle = host.mount("DGrid", props, DataGrid);
    host.run_until_idle();
    let root = handle.root_instance();
    let offset = host.instance_scroll(root).expect("scroll handle");
    let list = find_retained_by_debug(&host, "dgrid")[0];
    (
        GridHarness {
            host,
            offset,
            list,
            coll,
        },
        handle,
    )
}

fn cell_bg(h: &GridHarness, id: NodeId) -> Option<Color> {
    // The zebra lives on the template root (the cell Div only
    // geometries) — match the body node by box.
    let b = h.host.committed_box(id).expect("cell laid out");
    let body = find_retained_by_debug(&h.host, "dgrid-body")
        .into_iter()
        .find(|r| {
            h.host
                .committed_box(*r)
                .is_some_and(|rb| approx(rb.y, b.y) && approx(rb.x, b.x))
        })
        .expect("body content at cell box");
    let sid = h.host.retained_style(body).expect("body style");
    h.host
        .with_retained_mut(|_, styles| styles.get(sid).expect("style").bg)
}

/// Header + window + extent: 200 alternating rows (total 6000),
/// 3 columns (total 300 wide), viewport 320×200 with a 28px pinned
/// header → 24 cells in [0,8), header pinned at y=0 with full
/// column width.
#[test]
fn datagrid_renders_header_window_and_extent() {
    let (h, _) = mount_grid(GRID_N);
    assert_eq!(
        find_retained_by_debug(&h.host, "dgrid-cell").len(),
        24,
        "8 rows × 3 cols, not all 200"
    );
    let header = find_retained_by_debug(&h.host, "dgrid-header");
    assert_eq!(header.len(), 1, "one sticky header");
    let hb = h.host.committed_box(header[0]).expect("header laid out");
    assert!(approx(hb.y, 0.0), "header at viewport top, got {}", hb.y);
    assert!(approx(hb.w, 300.0), "header spans columns, got {}", hb.w);
    assert!(approx(hb.h, 28.0), "default header height");
    // First-row cells sit below the pinned header with exact
    // column widths.
    let mut xs: Vec<f32> = find_retained_by_debug(&h.host, "dgrid-cell")
        .into_iter()
        .filter_map(|id| h.host.committed_box(id))
        .filter(|b| approx(b.y, 28.0))
        .map(|b| b.x)
        .collect();
    xs.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    assert_eq!(xs.len(), 3, "three first-row cells");
    assert!(approx(xs[0], 0.0) && approx(xs[1], 100.0) && approx(xs[2], 220.0));
    // Extent covers all rows plus the pinned header (spacer source).
    let list_box = h.host.committed_box(h.list).expect("list laid out");
    assert!(
        approx(list_box.content_h, 6028.0),
        "200 rows × 30 mean + 28 header, got {}",
        list_box.content_h
    );
}

/// Scrolling slides the window and pins the header: 500px lands
/// [15,25) (30 cells), the header box stays at the viewport top,
/// and zebra follows row 15.
#[test]
fn datagrid_scroll_pins_header_recycles_cells() {
    let (h, _) = mount_grid(GRID_N);
    h.offset.set(500.0);
    h.host.run_until_idle();
    assert_eq!(
        find_retained_by_debug(&h.host, "dgrid-cell").len(),
        30,
        "10 rows × 3 cols"
    );
    let header = find_retained_by_debug(&h.host, "dgrid-header")[0];
    let hb = h.host.committed_box(header).expect("header laid out");
    assert!(
        approx(hb.y, 0.0),
        "header pinned to viewport top, got {}",
        hb.y
    );
    // Row 15's cells sit viewport-relative at 28 + tops[15] - 500
    // with odd zebra.
    let first_row: Vec<NodeId> = find_retained_by_debug(&h.host, "dgrid-cell")
        .into_iter()
        .filter(|id| {
            h.host
                .committed_box(*id)
                .is_some_and(|b| approx(b.y, 28.0 + vitem_top(15) - 500.0))
        })
        .collect();
    assert_eq!(first_row.len(), 3, "three cells at row 15");
    for id in first_row {
        assert_eq!(cell_bg(&h, id), Some(Color(0x33_33_33)));
    }
}

/// Round 24.2: one wheel event on an unbound grid feeds the
/// declaring component's offset (no `bind_scroll` call), keeps the
/// header at the viewport top, and moves rows viewport-relative.
#[test]
fn datagrid_wheel_feeds_owner_offset_without_explicit_bind() {
    let (h, _) = mount_grid(GRID_N);
    assert_eq!(
        h.host.bound_scroll(h.list),
        None,
        "the grid never binds an explicit feed"
    );
    h.host.inject_input(InputEvent::Scroll {
        target: h.list,
        dx: 0.0,
        dy: 120.0,
    });
    h.host.run_until_idle();
    assert_eq!(h.offset.get(), 120.0, "wheel moved the owner offset");
    let header = find_retained_by_debug(&h.host, "dgrid-header")[0];
    let hb = h.host.committed_box(header).expect("header laid out");
    assert!(approx(hb.y, 0.0), "header stays put, got {}", hb.y);
    // Row 2 is the window head at 120px: 28 + 60 - 120 = -32.
    let first_row: Vec<NodeId> = find_retained_by_debug(&h.host, "dgrid-cell")
        .into_iter()
        .filter(|id| {
            h.host
                .committed_box(*id)
                .is_some_and(|b| approx(b.y, 28.0 + vitem_top(2) - 120.0))
        })
        .collect();
    assert_eq!(first_row.len(), 3, "three cells at row 2");
    for id in first_row {
        assert_eq!(cell_bg(&h, id), Some(Color(0x22_22_22)));
    }
}

/// Filter updates re-derive window + extent without remount
/// (even ids → 100 rows, total 3000).
#[test]
fn datagrid_filter_updates_window_and_extent() {
    let (h, handle) = mount_grid(GRID_N);
    let filtered = DataGridProps::new(h.coll.clone(), grid_cols(), vitem_h)
        .size(GRID_VIEW_W, GRID_VIEW_H)
        .overscan(VL_OVER)
        .debug("dgrid")
        .filter(|item: &VItem| item.i.is_multiple_of(2));
    handle.set_props(filtered);
    h.host.run_until_idle();
    let list_box = h.host.committed_box(h.list).expect("list laid out");
    // Even original indexes are exactly the 20px rows.
    assert!(
        approx(list_box.content_h, 2028.0),
        "100 even rows × 20 + 28 header, got {}",
        list_box.content_h
    );
    // Window recomputed over the filtered set (uniform 20px
    // rows in the 172px body: 9 visible + 2 overscan = 11 rows ×
    // 3 cols).
    assert_eq!(
        find_retained_by_debug(&h.host, "dgrid-cell").len(),
        33,
        "filtered window [0,11) × 3 cols"
    );
}

/// Round 13.1 → 13.3 combined: two page loads stream into the
/// live grid — states track per page, rows accumulate, extent
/// grows, no remount.
#[test]
fn datagrid_async_page_stream_grows_grid() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.set_text_service(Box::new(FakeText));
    let rt = host.runtime();
    let coll: oppa::Collection<VItem> =
        oppa::Collection::new(&rt, oppa::fetch_key("test:dgrid-pages"));
    #[derive(Clone)]
    struct GridApp {
        coll: oppa::Collection<VItem>,
        fired: Rc<std::cell::Cell<bool>>,
    }
    impl Props for GridApp {}
    fn grid_app_render(ctx: &Ctx, p: &GridApp) -> VNode {
        if !p.fired.get() {
            p.fired.set(true);
            let key = p.coll.key();
            ctx.spawn_fetch_page(key, 0, 10, 3, |_, _| {
                Ok(vec![VItem { i: 0 }, VItem { i: 1 }, VItem { i: 2 }])
            });
            ctx.spawn_fetch_page(key, 1, 10, 3, |_, _| {
                Ok(vec![VItem { i: 3 }, VItem { i: 4 }])
            });
        }
        let props = DataGridProps::new(p.coll.clone(), grid_cols(), vitem_h)
            .size(GRID_VIEW_W, GRID_VIEW_H)
            .overscan(VL_OVER)
            .debug("dgrid");
        ctx.child("oppa::DataGrid", 1, &props, DataGrid)
    }
    host.mount(
        "GridApp",
        GridApp {
            coll: coll.clone(),
            fired: Rc::new(std::cell::Cell::new(false)),
        },
        grid_app_render,
    );
    host.run_until_idle();
    let mut waited = 0;
    loop {
        let stats = rt.stats();
        if stats.tasks_done + stats.tasks_dropped >= 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        waited += 1;
        assert!(waited < 10_000, "executor tasks never resolved");
    }
    host.run_until_idle();
    assert_eq!(coll.len(), 5, "both pages streamed");
    // Extent follows live: 20+40+20+40+20 rows + 28 header = 168.
    let list = find_retained_by_debug(&host, "dgrid")[0];
    let list_box = host.committed_box(list).expect("list laid out");
    assert!(
        approx(list_box.content_h, 168.0),
        "5 streamed rows, got {}",
        list_box.content_h
    );
    // Both page states reached Ready (per-page tracking).
    for page in [0, 1] {
        let state = rt
            .keyed_state::<oppa::FetchState<Vec<VItem>>>(oppa::page_key(coll.key(), page), || {
                oppa::FetchState::Idle
            })
            .get();
        assert!(
            matches!(state, oppa::FetchState::Ready(_)),
            "page {page} Ready, got {state:?}"
        );
    }
}

// ------------------------------------------------------------------
// Round 14.1 (decision 311): generic `#[derive(Props)]`
// ------------------------------------------------------------------

/// The brief's shape, verbatim: a generic props struct deriving
/// the marker (no manual impl — the derive emits
/// `impl<T: Clone + 'static> Props`).
#[derive(Clone, Props)]
struct ListProps<T> {
    items: Vec<T>,
}

fn generic_list(ctx: &Ctx, p: &ListProps<String>) -> VNode {
    Column::new().children(
        p.items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                ctx.child(
                    "oppa::GenericRow",
                    i as u64,
                    &RowCase {
                        label: SharedString::from(item.as_str()),
                    },
                    generic_row,
                )
            })
            .collect::<Vec<_>>(),
    )
}

#[derive(Clone)]
struct RowCase {
    label: SharedString,
}

impl Props for RowCase {}

fn generic_row(_ctx: &Ctx, p: &RowCase) -> VNode {
    Div("generic-row")
        .style(Style::new().h(24).fill_width())
        .semantics(Semantics::default().label(&p.label))
        .build()
}

/// A generic props struct compiles through the derive and
/// renders through the real pipeline (mount + reconcile +
/// layout — one labeled row per item, in order).
#[test]
fn generic_derive_props_compiles_and_renders() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.mount(
        "GenericList",
        ListProps {
            items: vec!["a".to_string(), "b".to_string(), "c".to_string()],
        },
        generic_list,
    );
    host.run_until_idle();
    let rows = find_retained_by_debug(&host, "generic-row");
    assert_eq!(rows.len(), 3, "one row per item");
    let labels: Vec<String> = rows
        .iter()
        .map(|id| {
            host.retained_semantics(*id)
                .expect("row semantics")
                .label
                .as_deref()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert_eq!(
        labels,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
}

// ------------------------------------------------------------------
// Round 17.2 (decision 318): draggable scrollbar.
