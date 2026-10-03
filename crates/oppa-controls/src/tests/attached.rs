use super::*;
use oppa::input::{keys, InputEvent, KeyState};
use oppa::{find_retained_by_debug, Color, ComponentHost, Ctx, Div, NodeId, Style, VNode};
use std::cell::RefCell;
use std::rc::Rc;

/// Chrome rects at an arbitrary target box (the rig's
/// `chrome_rects` pinned to the rig box — attachments compute
/// their own track edge from the settled target box).
fn chrome_rects_at(plan: &oppa::FramePlan, track_x: f32, top_y: f32, view_h: f32) -> ChromePair {
    let mut track = Vec::new();
    let mut thumb = Vec::new();
    for op in &plan.ops {
        if let oppa::DrawOp::Rect { x, y, w, h, .. } = op {
            if approx(*x, track_x) && approx(*w, 12.0) {
                if approx(*y, top_y) && approx(*h, view_h) {
                    track.push((*x, *y, *w, *h));
                } else {
                    thumb.push((*x, *y, *w, *h));
                }
            }
        }
    }
    (track, thumb)
}

/// Mounts a `VirtualList` on a caller-owned host (clocked hosts
/// drive the idle fade deterministically).
fn mount_vlist_on(host: &ComponentHost, debug: &str) -> (oppa::ScrollOffset, NodeId) {
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:vlist-attach"));
    coll.ingest((0..VL_N).map(|i| VItem { i }).collect::<Vec<_>>());
    let props = VirtualListProps::new(coll, vitem_h, vitem_row)
        .size(320.0, VL_VIEW_H)
        .overscan(VL_OVER)
        .debug(debug);
    let handle = host.mount("VListAttach", props, VirtualList);
    host.run_until_idle();
    let offset = host
        .instance_scroll(handle.root_instance())
        .expect("list scroll handle");
    let list = find_retained_by_debug(host, debug)[0];
    (offset, list)
}

/// Round 21.2: dragging the attached thumb top-to-bottom scrolls
/// the virtualized row window to 100% — offset parks on max and
/// the window covers the final rows.
#[test]
fn vlist_attached_thumb_drag_scrolls_to_max() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let (offset, list) = mount_vlist_on(&host, "vlist");
    let tb = host.committed_box(list).expect("list laid out");
    let track_x = tb.x + tb.w - 12.0;
    // Thumb parks 0..24 at rest; grab its top, drag to the
    // track bottom — the ratio overshoots, the clamp parks max.
    drag(&host, track_x + 6.0, 5.0, track_x + 6.0, tb.y + tb.h - 1.0);
    let max = 30_000.0 - VL_VIEW_H;
    assert!(
        approx(offset.get(), max),
        "100% extent, got {}",
        offset.get()
    );
    // The window covers the final rows (last slot bottom ==
    // viewport bottom).
    let bottom = find_retained_by_debug(&host, "vlist-slot")
        .into_iter()
        .filter_map(|id| host.committed_box(id))
        .map(|b| b.y + b.h)
        .fold(0.0f32, f32::max);
    assert!(
        approx(bottom, VL_VIEW_H),
        "window shows the final rows, bottom {bottom}"
    );
}

/// Round 21.2: the attached overlay fades on the wall clock —
/// visible after a scroll, still visible before `idle_hide_ms`,
/// gone after, without another pointer event (and the content
/// offset never moves). Settles step the mock clock (a frozen
/// clock spins `run_until_idle` while the fade transition
/// lives — the M8 `run_once` pattern instead).
#[test]
fn vlist_attached_idle_fade_hides_after_timeout() {
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let (offset, list) = mount_vlist_on(&host, "vlist-idle");
    settle_clocked(&host, &clock);
    offset.set(500.0);
    settle_clocked(&host, &clock);
    let tb = host.committed_box(list).expect("list laid out");
    let at = (tb.x + tb.w - 12.0, tb.y, tb.h);
    let visible = || {
        let plan = scrollbar_plan(&host);
        let (track, thumb) = chrome_rects_at(&plan, at.0, at.1, at.2);
        !track.is_empty() && !thumb.is_empty()
    };
    assert!(visible(), "scroll latches the chrome on");
    // Half the idle budget: still parked, no pointer in sight.
    clock.advance(0.5);
    assert_eq!(host.tick_timers(host.now_ms()), 0, "nothing due yet");
    settle_clocked(&host, &clock);
    assert!(visible(), "still parked before idle_hide_ms");
    // Past 1200ms: the timer drops the flash, chrome gone.
    clock.advance(0.8);
    assert_eq!(host.tick_timers(host.now_ms()), 1, "idle timer fires");
    settle_clocked(&host, &clock);
    assert!(!visible(), "hides without another pointer event");
    assert!(
        approx(offset.get(), 500.0),
        "fade never moves content, got {}",
        offset.get()
    );
}

/// Mounts a `DataGrid` on a caller-owned host.
fn mount_grid_on(host: &ComponentHost, debug: &str) -> (oppa::ScrollOffset, NodeId) {
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:dgrid-attach"));
    coll.ingest((0..GRID_N).map(|i| VItem { i }).collect::<Vec<_>>());
    let props = DataGridProps::new(coll, grid_cols(), vitem_h)
        .size(GRID_VIEW_W, GRID_VIEW_H)
        .overscan(VL_OVER)
        .debug(debug);
    let handle = host.mount("DGridAttach", props, DataGrid);
    host.run_until_idle();
    let offset = host
        .instance_scroll(handle.root_instance())
        .expect("grid scroll handle");
    let list = find_retained_by_debug(host, debug)[0];
    (offset, list)
}

/// Round 21.2: dragging the grid's attached thumb parks the
/// window on max extent with the header pinned to the viewport top.
#[test]
fn dgrid_attached_thumb_drag_scrolls_to_max() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.set_text_service(Box::new(FakeText));
    let (offset, list) = mount_grid_on(&host, "dgrid");
    let tb = host.committed_box(list).expect("grid laid out");
    let track_x = tb.x + tb.w - 12.0;
    drag(&host, track_x + 6.0, 5.0, track_x + 6.0, tb.y + tb.h - 1.0);
    let max = 6028.0 - GRID_VIEW_H;
    assert!(
        approx(offset.get(), max),
        "100% extent, got {}",
        offset.get()
    );
    let header = find_retained_by_debug(&host, "dgrid-header")[0];
    let hb = host.committed_box(header).expect("header laid out");
    assert!(
        approx(hb.y, 0.0),
        "header pinned to the viewport top, got {}",
        hb.y
    );
}

// ------------------------------------------------------------------
// Round 23.1 (decision 333): per-key granular subscriptions.
// ------------------------------------------------------------------

/// Granular row content (the 23.1 verify shape): reads its own
/// payload through `get_row` (tracked per-row — sibling rows
/// never hear each other's writes) and counts renders per
/// stable id.
#[derive(Clone)]
struct GItem {
    i: usize,
    runs: Rc<RefCell<Vec<usize>>>,
}

fn granular_row(_ctx: &Ctx, p: &VirtualRowProps<GItem>) -> VNode {
    p.row.value.runs.borrow_mut()[p.row.id.0 as usize] += 1;
    let v = p
        .rows
        .get_row(p.row.id)
        .get()
        .map(|row| row.i)
        .unwrap_or(usize::MAX);
    Div("grow")
        .style(
            Style::new()
                .h(p.height)
                .fill_width()
                .bg(if v.is_multiple_of(2) {
                    Color(0x22_22_22)
                } else {
                    Color(0x33_33_33)
                }),
        )
        .build()
}

/// Round 23.1 (decision 333): in a 1,000-row `Collection`
/// mounted inside `VirtualList`, updating one visible row via
/// `update_row` refreshes the window with the new value while
/// version subscribers stay quiet — no structural fan-out
/// (counters, sibling windows, and query readers never
/// re-run). The owning window re-derives (M2 inline semantics:
/// inline children share their root's dependency set, so the
/// window pass re-renders its slots — per-row child isolation
/// needs effect-per-child machinery, an open architecture
/// question, not a silent gap).
#[test]
fn update_row_refreshes_without_version_fanout() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:granular-vlist"));
    let runs = Rc::new(RefCell::new(vec![0usize; 1000]));
    coll.ingest(
        (0..1000)
            .map(|i| GItem {
                i,
                runs: runs.clone(),
            })
            .collect::<Vec<_>>(),
    );
    let props = VirtualListProps::new(coll.clone(), |_: &GItem| 30.0, granular_row)
        .size(320.0, 200.0)
        .overscan(2)
        .debug("gvlist")
        .scrollbar(false);
    host.mount("GV", props, VirtualList);
    // An unrelated version subscriber (counter): quiet unless
    // the version bumps (a coarse `update` would re-run it).
    use oppa::Props;
    #[derive(Clone)]
    struct LenProps {
        runs: Rc<RefCell<usize>>,
        coll: oppa::Collection<GItem>,
    }
    impl Props for LenProps {}
    fn len_comp(_ctx: &Ctx, p: &LenProps) -> VNode {
        *p.runs.borrow_mut() += 1;
        let _ = p.coll.len();
        Div("lenprobe").build()
    }
    let len_runs = Rc::new(RefCell::new(0usize));
    host.mount(
        "LEN",
        LenProps {
            runs: len_runs.clone(),
            coll: coll.clone(),
        },
        len_comp,
    );
    host.run_until_idle();
    // Sanity: only the window materialized (far from all 1000).
    let rendered: usize = runs.borrow().iter().filter(|&&c| c > 0).count();
    assert!(
        rendered > 0 && rendered < 100,
        "windowed render, got {rendered}"
    );
    assert_eq!(*len_runs.borrow(), 1);
    // Row 3 sits in the first window: update it granularly.
    let target = coll.rows()[3].id;
    assert!(coll.update_row(
        target,
        GItem {
            i: 3001,
            runs: runs.clone()
        }
    ));
    host.run_until_idle();
    // The window picked up the fresh value.
    let shown: usize = find_retained_by_debug(&host, "grow").len();
    assert!(shown > 0, "window still renders");
    assert_eq!(
        coll.lookup(target).map(|g| g.i),
        Some(3001),
        "lookup sees the write"
    );
    // No structural fan-out: the version subscriber never
    // re-ran (a coarse `update` would have re-run it).
    assert_eq!(*len_runs.borrow(), 1, "version subscribers stay quiet");
    // Row 3 re-rendered (its slot notified); queries read the
    // fresh value on structural passes.
    let page = coll.query(&oppa::CollectionQuery {
        filter: None,
        sort: None,
        offset: 3,
        limit: Some(1),
    });
    assert_eq!(
        page.rows.first().map(|r| r.value.i),
        Some(3001),
        "queries read fresh values"
    );
}

/// Round 23.1 (decision 333): `update_row` never bumps the
/// collection version (a `len` reader proves it — still 1
/// after the granular write), while coarse `update` does.
#[test]
fn update_row_skips_version_while_update_bumps() {
    use oppa::Props;
    #[derive(Clone)]
    struct LProps {
        runs: Rc<RefCell<usize>>,
        coll: oppa::Collection<String>,
    }
    impl Props for LProps {}
    fn lcomp(_ctx: &Ctx, p: &LProps) -> VNode {
        *p.runs.borrow_mut() += 1;
        let _ = p.coll.len();
        Div("lprobe").build()
    }
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:zz-ver"));
    coll.ingest(vec!["a".to_string(), "b".to_string()]);
    let runs = Rc::new(RefCell::new(0usize));
    host.mount(
        "L",
        LProps {
            runs: runs.clone(),
            coll: coll.clone(),
        },
        lcomp,
    );
    host.run_until_idle();
    assert_eq!(*runs.borrow(), 1, "mounted once");
    coll.update_row(oppa::RowId(0), "A".to_string());
    host.run_until_idle();
    assert_eq!(*runs.borrow(), 1, "granular write skips the version");
    coll.update(oppa::RowId(0), "AA".to_string());
    host.run_until_idle();
    assert_eq!(*runs.borrow(), 2, "coarse write bumps it");
}
/// Round 23.1 (decision 333): root-mounted row readers are
/// precisely isolated — one row's `update_row` re-renders
/// exactly its subscriber (siblings quiet). Inside one list
/// the window re-derives with its root (M2 inline semantics —
/// see `update_row_refreshes_without_version_fanout`).
#[test]
fn update_row_rerenders_only_subscribed_roots() {
    use oppa::Props;
    use std::collections::HashMap;
    #[derive(Clone)]
    struct RProps {
        id: oppa::RowId,
        runs: Rc<RefCell<HashMap<u64, usize>>>,
        coll: oppa::Collection<String>,
    }
    impl Props for RProps {}
    fn rcomp(_ctx: &Ctx, p: &RProps) -> VNode {
        *p.runs.borrow_mut().entry(p.id.0).or_insert(0) += 1;
        let _ = p.coll.get_row(p.id).get();
        Div("rprobe").build()
    }
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:zz-probe"));
    let ids = coll.ingest(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    let runs = Rc::new(RefCell::new(HashMap::new()));
    for row in &ids {
        host.mount(
            "R",
            RProps {
                id: row.id,
                runs: runs.clone(),
                coll: coll.clone(),
            },
            rcomp,
        );
    }
    host.run_until_idle();
    assert_eq!(runs.borrow().len(), 3, "all three mounted once");
    coll.update_row(ids[1].id, "B".to_string());
    host.run_until_idle();
    assert_eq!(
        (
            runs.borrow().get(&0).copied(),
            runs.borrow().get(&1).copied(),
            runs.borrow().get(&2).copied()
        ),
        (Some(1), Some(2), Some(1)),
        "exactly the subscribed root re-renders"
    );
}

/// Round 21.2: the grid's attached overlay hides past
/// `idle_hide_ms` on the host clock (the hide leg — the
/// still-visible leg rides the list test above). Clock-stepped
/// settles (same frozen-clock rule as the list test).
#[test]
fn dgrid_attached_idle_fade_hides_after_timeout() {
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    host.set_text_service(Box::new(FakeText));
    let (offset, list) = mount_grid_on(&host, "dgrid-idle");
    settle_clocked(&host, &clock);
    offset.set(500.0);
    settle_clocked(&host, &clock);
    let tb = host.committed_box(list).expect("grid laid out");
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects_at(&plan, tb.x + tb.w - 12.0, tb.y, tb.h);
    assert!(
        !track.is_empty() && !thumb.is_empty(),
        "scroll shows chrome"
    );
    clock.advance(1.3);
    assert_eq!(host.tick_timers(host.now_ms()), 1, "idle timer fires");
    settle_clocked(&host, &clock);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects_at(&plan, tb.x + tb.w - 12.0, tb.y, tb.h);
    assert!(track.is_empty() && thumb.is_empty(), "hides on the clock");
    assert!(
        approx(offset.get(), 500.0),
        "fade never moves content, got {}",
        offset.get()
    );
}

/// Round 17.2: thumb and track honor reactive theme tokens
/// (text_secondary / border recolor Light→Dark in place).
#[test]
fn scrollbar_matches_theme_tokens() {
    use oppa::ThemeTokens;
    let (host, _) = scrollbar_harness(600.0);
    hover(&host, 194.0, 100.0);
    let plan = scrollbar_plan(&host);
    let thumb_color = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            oppa::DrawOp::Rect { x, w, h, color, .. }
                if approx(*x, 188.0) && approx(*w, 12.0) && !approx(*h, 200.0) =>
            {
                Some(*color)
            }
            _ => None,
        })
        .next()
        .expect("thumb rect");
    assert_eq!(thumb_color, ThemeTokens::light().text_secondary);
    host.set_theme(oppa::ThemeMode::Dark);
    host.run_until_idle();
    let plan = scrollbar_plan(&host);
    let thumb_color = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            oppa::DrawOp::Rect { x, w, h, color, .. }
                if approx(*x, 188.0) && approx(*w, 12.0) && !approx(*h, 200.0) =>
            {
                Some(*color)
            }
            _ => None,
        })
        .next()
        .expect("thumb rect after theme flip");
    assert_eq!(
        thumb_color,
        ThemeTokens::dark().text_secondary,
        "thumb recolors in place"
    );
}

/// Round 17.2: arrows page by viewport once the track holds
/// focus (tab into it — press owners are tab stops).
#[test]
fn keyboard_pages_when_track_focused() {
    let (host, offset) = scrollbar_harness(600.0);
    host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
    host.run_until_idle();
    host.inject_input(InputEvent::key(keys::DOWN, KeyState::Pressed));
    host.run_until_idle();
    assert!(
        approx(offset.get(), 200.0),
        "Down pages, got {}",
        offset.get()
    );
    host.inject_input(InputEvent::key(keys::UP, KeyState::Pressed));
    host.run_until_idle();
    assert!(
        approx(offset.get(), 0.0),
        "Up pages back, got {}",
        offset.get()
    );
}

// ------------------------------------------------------------------
