use super::*;

// ---------------------------------------------------------------------------
// 3. §9.3 native scroll: container + spacer + anchor-off, INPUT-fed offset
// ---------------------------------------------------------------------------

#[test]
fn scroll_container_shape_spacer_overscan_anchor() {
    assert_eq!(OVERSCAN_SLOTS, 4, "the v1 overscan constant, stated");
    let rig = Rig::new("Slots", SlotProps { n: 3 }, render_slots, 1.0);
    let list = rig.scroll_target("list");
    let el = rig.backend.element(list).expect("list").clone();
    assert_eq!(el.kind, oppa_dom::HtmlKind::Scroll);
    // Viewport: committed box (120×60 style).
    assert!(
        el.inline_geom.contains("height:60px"),
        "viewport height: {}",
        el.inline_geom
    );
    // Spacer: content extent floored by content_size(200).
    assert_eq!(el.spacer_h, Some(200.0), "spacer == content floor");
    assert_eq!(el.children.len(), 3, "slots ride inside the spacer");
    let html = rig.backend.render_node(list);
    assert!(html.contains("overflow:auto;"), "{html}");
    assert!(
        html.contains("height:200px;"),
        "spacer carries the extent: {html}"
    );
    assert!(html.contains("data-scroll"), "{html}");
    assert!(html.contains("data-slot"), "slots are marked: {html}");
    let page = render_page("t", &rig.backend, &rig.sheet);
    assert!(
        page.contains("[data-slot]{overflow-anchor:none;}"),
        "anchor-off lives in the sheet, not per element"
    );
}

#[test]
fn overscan_window_math() {
    assert_eq!(scroll_window(0.0, 20.0, 100.0, 50), (0, 9));
    assert_eq!(scroll_window(200.0, 20.0, 100.0, 50), (6, 19));
    assert_eq!(
        scroll_window(0.0, 20.0, 100.0, 5),
        (0, 5),
        "clamped to the row count"
    );
    assert_eq!(scroll_window(1000.0, 20.0, 100.0, 50), (46, 50));
    assert_eq!(
        scroll_window(0.0, 0.0, 100.0, 50),
        (0, 0),
        "loud zero row_h"
    );
    assert_eq!(scroll_window(-30.0, 20.0, 100.0, 50), (0, 8));
}

#[test]
fn offset_currency_within_one_frame_on_injected_clock() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Slots", SlotProps { n: 3 }, render_slots);
    host.run_until_idle();
    let list = oppa::find_retained_by_debug(&host, "list")[0];
    let offset = host
        .instance_scroll(handle.root_instance())
        .expect("scroll handle");
    host.bind_scroll(list, offset.clone());
    assert_eq!(offset.get(), 0.0);

    // The browser scrolled (absolute, browser-owned); the shell maps it
    // to a delta and injects at the INPUT boundary.
    let mut backend = DomBackend::new(1.0);
    assert_eq!(backend.note_browser_scroll(list, 48.0), None, "first sight");
    host.inject_input(InputEvent::Scroll {
        target: list,
        dx: 0.0,
        dy: 48.0,
    });
    assert_eq!(offset.get(), 0.0, "not applied before the frame");
    clock.advance(1.0 / 60.0);
    assert!(host.runtime().run_once(), "the frame runs");
    assert_eq!(offset.get(), 48.0, "INPUT-fed offset lands within 1 frame");
    assert_eq!(backend.scroll_top(list), Some(48.0), "ledger agrees");
    assert_eq!(host.bound_scroll(list), Some(48.0), "signal agrees");
    // Consumed exactly once: a second frame applies nothing more.
    clock.advance(1.0 / 60.0);
    host.runtime().run_once();
    assert_eq!(offset.get(), 48.0, "no double-apply");
}
