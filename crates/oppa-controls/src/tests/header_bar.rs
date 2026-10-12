use super::*;
use oppa::{Ctx, Props, SharedString, VNode};
use std::cell::Cell;

#[derive(Clone)]
struct BarCase {
    props: HeaderBarProps,
}
impl Props for BarCase {}

fn bar_render(ctx: &Ctx, p: &BarCase) -> VNode {
    ctx.child("oppa::HeaderBar", 1, &p.props, HeaderBar)
}

fn mount_bar(props: HeaderBarProps) -> ComponentHost {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    host.set_viewport(400.0, 300.0);
    host.mount("Bar", BarCase { props }, bar_render);
    host.run_until_idle();
    host
}

fn press_count() -> (Action, Rc<Cell<usize>>) {
    let n = Rc::new(Cell::new(0));
    let c = n.clone();
    (action(move || c.set(c.get() + 1)), n)
}

/// Decision 482: the bar renders its title with the three bridged
/// caption buttons; the chrome root is handlerless (the OS owns the
/// drag gesture over it) while each provided button owns a press.
#[test]
fn header_bar_renders_title_with_bridged_caption_buttons() {
    let (min, min_n) = press_count();
    let (max, max_n) = press_count();
    let (close, close_n) = press_count();
    let mut props = HeaderBarProps::new("My App").height(40.0);
    props.on_minimize = Some(min);
    props.on_maximize = Some(max);
    props.on_close = Some(close);
    let host = mount_bar(props);
    let bar = node_by_debug(&host, oppa::HEADER_BAR_DEBUG);
    let bar_presses: Vec<_> = host
        .retained_handlers(bar)
        .into_iter()
        .filter(|(k, _)| *k == oppa::EventKind::Press)
        .collect();
    assert!(
        bar_presses.is_empty(),
        "bar chrome stays handlerless (OS drag owns the gesture)"
    );
    for debug in ["header-min", "header-max", "header-close"] {
        let id = node_by_debug(&host, debug);
        assert!(
            host.retained_handlers(id)
                .iter()
                .any(|(k, _)| *k == oppa::EventKind::Press),
            "{debug} owns its press"
        );
    }
    press_node(&host, node_by_debug(&host, "header-min"));
    press_node(&host, node_by_debug(&host, "header-max"));
    press_node(&host, node_by_debug(&host, "header-close"));
    assert_eq!((min_n.get(), max_n.get(), close_n.get()), (1, 1, 1));
}

/// Decision 482: missing buttons render nothing (never a dead
/// affordance) — a bare bar is title chrome only.
#[test]
fn header_bar_without_actions_renders_title_only() {
    let host = mount_bar(HeaderBarProps::new("Bare"));
    let bar = node_by_debug(&host, oppa::HEADER_BAR_DEBUG);
    assert!(
        host.committed_box(bar).is_some(),
        "title-only bar still lays out"
    );
    for debug in ["header-min", "header-max", "header-close"] {
        assert!(
            find_retained_by_debug(&host, debug).is_empty(),
            "{debug} leaves no trace when unwired"
        );
    }
}

fn point_in_rects(x: f32, y: f32, rects: &[[f32; 4]]) -> bool {
    rects
        .iter()
        .any(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3])
}

/// Decision 482: `window_drag_rects` covers the bar minus its
/// caption buttons — bare chrome drags (area-exact), button boxes
/// keep their own hit-testing.
#[test]
fn window_drag_rects_covers_bar_minus_caption_buttons() {
    let (min, _) = press_count();
    let (max, _) = press_count();
    let (close, _) = press_count();
    let mut props = HeaderBarProps::new("My App").height(40.0);
    props.on_minimize = Some(min);
    props.on_maximize = Some(max);
    props.on_close = Some(close);
    let host = mount_bar(props);
    let rects = oppa::window_drag_rects(&host);
    assert!(!rects.is_empty(), "bar publishes a drag region");
    let bar_box = host
        .committed_box(node_by_debug(&host, oppa::HEADER_BAR_DEBUG))
        .expect("bar laid out");
    assert!(
        (bar_box.w - 400.0).abs() < 1.0 && (bar_box.h - 40.0).abs() < 1.0,
        "bar fills the 400-wide viewport at 40px, got {bar_box:?}"
    );
    // Area-exact: bar area minus the three 46x40 caption targets.
    let area: f32 = rects.iter().map(|r| r[2] * r[3]).sum();
    let expect = bar_box.w * bar_box.h - 3.0 * CAPTION_BUTTON_W * 40.0;
    assert!(
        (area - expect).abs() < 1.0,
        "drag area {area} == bar minus buttons {expect}"
    );
    // Bare chrome drags; every button center keeps its own target.
    assert!(
        point_in_rects(bar_box.x + 60.0, bar_box.y + 20.0, &rects),
        "bare bar chrome is draggable"
    );
    for debug in ["header-min", "header-max", "header-close"] {
        let b = host
            .committed_box(node_by_debug(&host, debug))
            .expect("button laid out");
        assert!(
            !point_in_rects(b.x + b.w / 2.0, b.y + b.h / 2.0, &rects),
            "{debug} center keeps its own hit-testing"
        );
    }
}

/// Decision 482: no header bar means no drag region (never `None` —
/// an empty vec states it).
#[test]
fn window_drag_rects_empty_without_header() {
    #[derive(Clone)]
    struct Plain;
    impl Props for Plain {}
    fn plain_render(ctx: &Ctx, _p: &Plain) -> VNode {
        ctx.child_auto(&ButtonProps::new("OK", || {}), Button)
    }
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    host.set_viewport(400.0, 300.0);
    host.mount("Plain", Plain, plain_render);
    host.run_until_idle();
    assert!(
        oppa::window_drag_rects(&host).is_empty(),
        "button-only scenes publish nothing"
    );
    // The marker label is reserved: apps never hand-write it (a
    // hand-labeled node would silently become a drag region).
    assert_eq!(
        SharedString::from(oppa::HEADER_BAR_DEBUG),
        SharedString::from("oppa::HeaderBar")
    );
}
