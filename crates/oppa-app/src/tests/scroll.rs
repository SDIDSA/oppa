use super::*;
use oppa::{JustifyContent, Row};

/// Scroll scene (decision 250): a 120x100 `on_scroll` list at
/// x=40 holding one item pinned at `-offset` (tracked read — the
/// feed write re-runs the body, so scrolled content visibly
/// moves and the repaint damages). The canonical shape:
/// `ctx.scroll_offset()` + `.on_scroll` + `bind_scroll`.
#[derive(Clone)]
struct ScrollCase;
impl Props for ScrollCase {}

fn scroll_app(ctx: &Ctx, _props: &ScrollCase) -> VNode {
    let offset = ctx.scroll_offset();
    Div("root").child(
        Div("list")
            .style(Style::new().size(120, 100).x(40))
            .on_scroll(|| {})
            .child(
                Div("item")
                    .style(
                        Style::new()
                            .size(120, 30)
                            .absolute_y(-offset.get())
                            .bg(Color(0x22_66_CC)),
                    )
                    .build(),
            ),
    )
}

#[test]
fn desktop_loop_mouse_wheel_scrolls_target_and_ignores_miss() {
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let handle = loop_.mount("Scroll", ScrollCase, scroll_app);
    let root = handle.root_instance();
    let offset = loop_.host().instance_scroll(root).expect("scroll handle");
    let list = oppa::find_retained_by_debug(loop_.host(), "list")
        .into_iter()
        .next()
        .expect("list node");
    loop_.host().bind_scroll(list, offset.clone());
    // (50, 10) hits the item — walk-up resolves the list — the
    // feed accumulates +50 and the moved item damages.
    let damage = loop_.scroll_at(50.0, 10.0, 0.0, 50.0).expect("scrolls");
    assert!(damage > 0, "scrolled content repaints");
    assert_eq!(offset.get(), 50.0);
    let item = oppa::find_retained_by_debug(loop_.host(), "item")
        .into_iter()
        .next()
        .expect("item node");
    assert_eq!(loop_.host().committed_box(item).expect("item box").y, -50.0);
    // (10, 50) hits root-only dead space — quiet miss, feed kept.
    assert_eq!(loop_.scroll_at(10.0, 50.0, 0.0, 50.0).expect("misses"), 0);
    assert_eq!(offset.get(), 50.0);
}

/// Horizontal scroll scene (Round 9.3, decision 302): a 200×40
/// `on_scroll` viewport holding a 300-wide strip (bound `[0,
/// 100]`). `scroll_at` with `dx != 0` moves the bound horizontal
/// feed — the brief's verification, end to end through hit-test
/// resolution.
#[derive(Clone)]
struct HScrollCase;
impl Props for HScrollCase {}

fn h_scroll_app(ctx: &Ctx, _props: &HScrollCase) -> VNode {
    let x = ctx.scroll_x();
    Div("root").child(
        oppa::ScrollArea("hlist")
            .style(Style::new().size(200, 40).x(40))
            .on_scroll(|| {})
            .child(
                Div("wide")
                    .style(Style::new().size(300, 20).x(x.get()))
                    .build(),
            ),
    )
}

#[test]
fn scroll_at_moves_the_bound_horizontal_feed() {
    let mut loop_ =
        DesktopLoop::new(400, 300, Box::new(FakeText), "Test").expect("headless loop builds");
    let handle = loop_.mount("HScroll", HScrollCase, h_scroll_app);
    let root = handle.root_instance();
    let offset_x = loop_
        .host()
        .instance_scroll_x(root)
        .expect("scroll_x handle");
    let list = oppa::find_retained_by_debug(loop_.host(), "hlist")
        .into_iter()
        .next()
        .expect("list node");
    loop_.host().bind_scroll_x(list, offset_x.clone());
    // (50, 10) hits the strip — walk-up resolves the list — `dx`
    // accumulates and the moved strip damages.
    let damage = loop_.scroll_at(50.0, 10.0, 30.0, 0.0).expect("scrolls");
    assert!(damage > 0, "scrolled content repaints");
    assert_eq!(offset_x.get(), 30.0);
    // Past the end pins at content_w - w = 100.
    loop_.scroll_at(50.0, 10.0, 500.0, 0.0).expect("scrolls");
    assert_eq!(offset_x.get(), 100.0);
}

#[test]
fn text_field_backspace_deletes_character() {
    let (mut loop_, value) = field_harness();
    focus_field(&mut loop_);
    loop_.type_text("abc").expect("types");
    assert_eq!(value.get(), SharedString::from("abc"));
    loop_.backspace().expect("backspaces");
    assert_eq!(value.get(), SharedString::from("ab"));
    // Delete-forward at the end is a session no-op (still "ab").
    loop_.delete_forward().expect("deletes");
    assert_eq!(value.get(), SharedString::from("ab"));
    // Unfocused edits miss quietly too.
    loop_.step(InputEvent::Focus { node: None }).expect("blurs");
    assert_eq!(loop_.backspace().expect("backspaces"), 0);
    assert_eq!(value.get(), SharedString::from("ab"));
}

/// Resize scene (decision 248): an unsized root fills the
/// viewport (the layout root rule) with one fixed plate
/// main-axis-centered in it — the plate's x derives from the
/// viewport width every layout pass, so stale boxes after a
/// resize are observable. Cross-axis centering is deliberately
/// not asserted: the root-fill fix-up stretches the root box
/// after children place (layout.rs root rule), so root children
/// center vertically against content height during the pass —
/// proven engine shape, not this round's question. (No
/// `fill_height` exists — only `fill_width` — so the root fills
/// by leaving the size unset, not by a fill pair.)
#[derive(Clone)]
struct ResizeProps;
impl Props for ResizeProps {}

fn resize_app(_ctx: &Ctx, _props: &ResizeProps) -> VNode {
    Row("root")
        .style(Style::new().justify_content(JustifyContent::Center))
        .child(
            Div("plate")
                .style(Style::new().size(96, 32).bg(Color(0x22_66_CC)))
                .build(),
        )
}

#[test]
fn desktop_loop_resize_refits_surface_and_recomputes_layout() {
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Resize", ResizeProps, resize_app);
    let root = oppa::find_retained_by_debug(loop_.host(), "root")
        .into_iter()
        .next()
        .expect("root node");
    let plate = oppa::find_retained_by_debug(loop_.host(), "plate")
        .into_iter()
        .next()
        .expect("plate node");
    let root_box = loop_.host().committed_box(root).expect("root box");
    assert_eq!((root_box.w, root_box.h), (200.0, 150.0));
    // (200-96)/2 — main-axis-centered in the initial viewport.
    let plate_box = loop_.host().committed_box(plate).expect("plate box");
    assert_eq!(plate_box.x, 52.0);
    assert_eq!(loop_.rgba8().expect("pixels").len(), 200 * 150 * 4);

    loop_.resize(400, 300).expect("resize works");
    loop_.repaint().expect("repaint works");
    assert_eq!(loop_.viewport(), (400, 300));
    assert_eq!(loop_.rgba8().expect("pixels").len(), 400 * 300 * 4);
    // Root refills the new viewport; the plate re-centers —
    // without decision-248 invalidation both would still read the
    // 200x150 boxes (no demand, gated-out layout).
    let root_box = loop_.host().committed_box(root).expect("root box");
    assert_eq!((root_box.w, root_box.h), (400.0, 300.0));
    let plate_box = loop_.host().committed_box(plate).expect("plate box");
    assert_eq!(plate_box.x, 152.0);
}
