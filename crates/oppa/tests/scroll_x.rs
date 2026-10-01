//! Horizontal scroll feed (Round 9.3, decision 302): wheel/trackpad
//! `dx` accumulates into the `bind_scroll_x` signal — clamped to the
//! target's content bounds — and is ignored while unbound (horizontal
//! scrolling is opt-in per container, never ambient).

use oppa::{
    find_retained_by_debug, ComponentHost, Ctx, Div, InputEvent, NodeId, Props, ScrollOffset,
    Style, VNode,
};

#[derive(Clone)]
struct HProps;
impl Props for HProps {}

/// 200×40 viewport holding one 300-wide strip (content_w 300, so the
/// horizontal bound is `[0, 100]`).
fn h_scroll(ctx: &Ctx, _: &HProps) -> VNode {
    let _x = ctx.scroll_x();
    oppa::ScrollArea("hlist")
        .style(Style::new().size(200, 40))
        .on_scroll(|| {})
        .child(Div("wide").style(Style::new().size(300, 20)).build())
}

struct Rig {
    host: ComponentHost,
    list: NodeId,
    offset_x: ScrollOffset,
}

impl Rig {
    fn new() -> Self {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let handle = host.mount("HScroll", HProps, h_scroll);
        host.run_until_idle();
        let root = handle.root_instance();
        let offset_x = host.instance_scroll_x(root).expect("scroll_x handle");
        let list = find_retained_by_debug(&host, "hlist")[0];
        let b = host.committed_box(list).expect("viewport laid out");
        assert_eq!((b.w, b.content_w), (200.0, 300.0));
        host.bind_scroll_x(list, offset_x.clone());
        Self {
            host,
            list,
            offset_x,
        }
    }

    fn scroll(&self, dx: f32, dy: f32) {
        self.host.inject_input(InputEvent::Scroll {
            target: self.list,
            dx,
            dy,
        });
        self.host.run_until_idle();
    }
}

#[test]
fn dx_accumulates_into_the_bound_horizontal_feed() {
    let rig = Rig::new();
    assert_eq!(rig.offset_x.get(), 0.0);
    rig.scroll(30.0, 0.0);
    assert_eq!(rig.offset_x.get(), 30.0);
    rig.scroll(30.0, 0.0);
    assert_eq!(rig.offset_x.get(), 60.0);
    // The vertical feed is untouched by construction (no bind, no
    // signal — `dy` still dispatches to the `on_scroll` handler and
    // is otherwise ignored).
    assert_eq!(rig.host.bound_scroll(rig.list), None);
}

#[test]
fn dx_clamps_to_content_bounds() {
    let rig = Rig::new();
    // Past the end pins at content_w - w = 100.
    rig.scroll(500.0, 0.0);
    assert_eq!(rig.offset_x.get(), 100.0);
    // Back up within range accumulates normally.
    rig.scroll(-30.0, 0.0);
    assert_eq!(rig.offset_x.get(), 70.0);
    // Past the start floors at 0.
    rig.scroll(-500.0, 0.0);
    assert_eq!(rig.offset_x.get(), 0.0);
}

#[test]
fn dx_ignores_unbound_targets() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.mount("HScroll", HProps, h_scroll);
    host.run_until_idle();
    let list = find_retained_by_debug(&host, "hlist")[0];
    // No `bind_scroll_x` anywhere: the handler still dispatches (M5
    // dispatch-only rule), but no signal moves.
    host.inject_input(InputEvent::Scroll {
        target: list,
        dx: 40.0,
        dy: 40.0,
    });
    host.run_until_idle();
    assert_eq!(host.bound_scroll_x(list), None);
}

#[test]
fn narrow_content_pins_at_rest() {
    #[derive(Clone)]
    struct NarrowProps;
    impl Props for NarrowProps {}
    fn narrow(ctx: &Ctx, _: &NarrowProps) -> VNode {
        let _x = ctx.scroll_x();
        oppa::ScrollArea("narrow")
            .style(Style::new().size(200, 40))
            .on_scroll(|| {})
            .child(Div("thin").style(Style::new().size(100, 20)).build())
    }
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let handle = host.mount("Narrow", NarrowProps, narrow);
    host.run_until_idle();
    let root = handle.root_instance();
    let offset_x = host.instance_scroll_x(root).expect("scroll_x handle");
    let list = find_retained_by_debug(&host, "narrow")[0];
    let b = host.committed_box(list).expect("viewport laid out");
    assert_eq!((b.w, b.content_w), (200.0, 100.0));
    host.bind_scroll_x(list, offset_x.clone());
    host.inject_input(InputEvent::Scroll {
        target: list,
        dx: 50.0,
        dy: 0.0,
    });
    host.run_until_idle();
    assert_eq!(
        offset_x.get(),
        0.0,
        "content narrower than the viewport never scrolls"
    );
}
