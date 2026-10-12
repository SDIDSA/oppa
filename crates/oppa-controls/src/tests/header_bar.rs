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

fn cell_bg(host: &ComponentHost, debug: &str) -> Option<oppa::Color> {
    let id = node_by_debug(host, debug);
    host.with_retained_mut(|rec, styles| {
        let n = rec.get(id).expect("live node");
        styles.get(n.style).cloned().unwrap_or_default().bg
    })
}

/// Decision 486: the tracked hover signal re-renders the bar with
/// the glow — hovered buttons tint (close goes signal red, others
/// the themed wash), clearing reverts to bare chrome. Paint-only:
/// geometry never moves.
#[test]
fn caption_hover_paints_glow_on_the_hovered_button() {
    let (min, _) = press_count();
    let (max, _) = press_count();
    let (close, _) = press_count();
    let mut props = HeaderBarProps::new("Glow").height(40.0);
    props.on_minimize = Some(min);
    props.on_maximize = Some(max);
    props.on_close = Some(close);
    let host = mount_bar(props);
    for debug in ["header-min", "header-max", "header-close"] {
        assert_eq!(cell_bg(&host, debug), None, "{debug} starts bare");
    }
    host.set_caption_hover(Some(oppa::CaptionButton::Maximize));
    host.run_until_idle();
    let wash = host.theme_tokens().border;
    assert_eq!(
        cell_bg(&host, "header-max"),
        Some(wash),
        "hovered button takes the themed wash"
    );
    assert_eq!(cell_bg(&host, "header-min"), None, "rest stays bare");
    assert_eq!(cell_bg(&host, "header-close"), None, "rest stays bare");
    host.set_caption_hover(Some(oppa::CaptionButton::Close));
    host.run_until_idle();
    assert_eq!(
        cell_bg(&host, "header-close"),
        Some(CLOSE_HOVER_BG),
        "close hover goes signal red"
    );
    assert_eq!(cell_bg(&host, "header-max"), None, "glow follows");
    host.set_caption_hover(None);
    host.run_until_idle();
    for debug in ["header-min", "header-max", "header-close"] {
        assert_eq!(cell_bg(&host, debug), None, "{debug} reverts on clear");
    }
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

/// Decision 487: every deposited glyph carries an anti-aliased
/// fringe (partial-alpha pixels — the baked-icon rule that keeps
/// icons smooth on the un-antialiased GPU path), themed variants
/// differ per palette, and the close-hover white is shared.
#[test]
fn caption_glyph_deposits_carry_aa_fringe_and_theme() {
    use crate::header_bar::CaptionIcons;
    use oppa::ThemeMode;
    let host = ComponentHost::new();
    let dark = CaptionIcons::deposit(&host.images(), ThemeMode::Dark);
    let light = CaptionIcons::deposit(&host.images(), ThemeMode::Light);
    assert_ne!(dark.minimize, light.minimize, "ink follows the palette");
    assert_ne!(dark.maximize, light.maximize, "ink follows the palette");
    assert_ne!(dark.close, light.close, "ink follows the palette");
    assert_eq!(
        dark.close_white, light.close_white,
        "close hover shares one white"
    );
    for id in [dark.minimize, dark.maximize, dark.close, dark.close_white] {
        let (_, _, rgba) = host
            .images()
            .pixels_of(id)
            .expect("deposited glyph reads back");
        assert!(
            rgba.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255),
            "{id:?} carries partial-alpha fringe"
        );
        assert!(
            rgba.chunks_exact(4).any(|p| p[3] == 255),
            "{id:?} carries a solid core"
        );
    }
}

/// Decision 487: each caption glyph renders as a 16px leaf exactly
/// centered in its 46px button box (horizontal and vertical — the
/// user's explicit centering requirement, proven geometrically, not
/// by inspection).
#[test]
fn caption_glyphs_center_exactly_in_their_buttons() {
    let (min, _) = press_count();
    let (max, _) = press_count();
    let (close, _) = press_count();
    let mut props = HeaderBarProps::new("Centered").height(40.0);
    props.on_minimize = Some(min);
    props.on_maximize = Some(max);
    props.on_close = Some(close);
    let host = mount_bar(props);
    for debug in ["header-min", "header-max", "header-close"] {
        let button = node_by_debug(&host, debug);
        let bbox = host.committed_box(button).expect("button laid out");
        // Children collected inside the retained borrow; boxes read
        // outside it (a host read inside would trip the borrow).
        let kids =
            host.with_retained_mut(|rec, _| rec.get(button).expect("live button").children.clone());
        let leaf = kids
            .into_iter()
            .find(|id| host.committed_box(*id).is_some())
            .expect("button carries a laid icon leaf");
        let ibox = host.committed_box(leaf).expect("icon laid out");
        assert!(
            (ibox.w - CAPTION_ICON_PX).abs() < 1.0,
            "{debug} glyph renders at {CAPTION_ICON_PX}px, got {ibox:?}"
        );
        let (bcx, bcy) = (bbox.x + bbox.w / 2.0, bbox.y + bbox.h / 2.0);
        let (icx, icy) = (ibox.x + ibox.w / 2.0, ibox.y + ibox.h / 2.0);
        assert!(
            (bcx - icx).abs() < 1.0 && (bcy - icy).abs() < 1.0,
            "{debug} glyph centers exactly ({icx:.1},{icy:.1} vs {bcx:.1},{bcy:.1})"
        );
    }
}

/// Decision 487: a theme toggle swaps the glyph variant (the
/// control reads the tracked theme — inks follow the palette
/// live, never stale across a toggle).
#[test]
fn caption_glyphs_follow_theme_toggles() {
    let (min, _) = press_count();
    let mut props = HeaderBarProps::new("Themed").height(40.0);
    props.on_minimize = Some(min);
    let host = mount_bar(props);
    let leaf_of = |host: &ComponentHost| {
        let button = node_by_debug(host, "header-min");
        host.with_retained_mut(|rec, _| rec.get(button).expect("live button").children[0])
    };
    let before = host
        .retained_image(leaf_of(&host))
        .expect("glyph leaf carries an image");
    host.set_theme(oppa::ThemeMode::Dark);
    host.run_until_idle();
    let after = host
        .retained_image(leaf_of(&host))
        .expect("glyph leaf carries an image");
    assert_ne!(before, after, "toggle swaps the ink variant");
}
