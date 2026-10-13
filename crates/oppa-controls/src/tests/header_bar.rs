use super::*;
use oppa::{Column, Ctx, Props, SharedString, VNode};
use std::cell::Cell;

#[derive(Clone)]
struct BarCase {
    props: HeaderBarProps,
}
impl Props for BarCase {}

fn bar_render(ctx: &Ctx, p: &BarCase) -> VNode {
    // The bar is always nested in real scenes (never the root) —
    // the page column stretches it across while it packs to its
    // content height.
    Column::new().children([ctx.child("oppa::HeaderBar", 1, &p.props, HeaderBar)])
}

fn mount_bar(props: HeaderBarProps) -> ComponentHost {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    host.set_viewport(400.0, 300.0);
    host.mount("Bar", BarCase { props }, bar_render);
    host.run_until_idle();
    host
}

#[derive(Clone)]
struct ButtonsCase {
    props: CaptionButtonsProps,
}
impl Props for ButtonsCase {}

fn buttons_render(ctx: &Ctx, p: &ButtonsCase) -> VNode {
    ctx.child("oppa::CaptionButtons", 1, &p.props, CaptionButtons)
}

fn mount_buttons(props: CaptionButtonsProps) -> ComponentHost {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    host.set_viewport(400.0, 300.0);
    host.mount("Buttons", ButtonsCase { props }, buttons_render);
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

fn point_in_rects(x: f32, y: f32, rects: &[[f32; 4]]) -> bool {
    rects
        .iter()
        .any(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3])
}

/// Decision 494: the bar is a transparent trait marker — custom
/// chrome renders inside the `oppa::HeaderBar` node with no visuals
/// of its own, and the container stays handlerless (an owner press
/// would swallow the OS drag gesture into the framework).
#[test]
fn header_bar_renders_custom_chrome_handlerless() {
    use oppa::{Row, Text};
    let host = mount_bar(
        HeaderBarProps::new()
            .content(|_| Row("custom-brand").child(VNode::from(Text::new("Brand").size(13)))),
    );
    let bar = node_by_debug(&host, oppa::HEADER_BAR_DEBUG);
    let brand = node_by_debug(&host, "custom-brand");
    assert!(
        host.committed_box(brand).is_some(),
        "custom chrome lays out inside the bar"
    );
    let bar_presses: Vec<_> = host
        .retained_handlers(bar)
        .into_iter()
        .filter(|(k, _)| *k == oppa::EventKind::Press)
        .collect();
    assert!(
        bar_presses.is_empty(),
        "bar container stays handlerless (OS drag owns the gesture)"
    );
}

/// Decision 494: handlerless chrome drags (drag area covers the
/// content box exactly), and an empty bar publishes nothing.
#[test]
fn header_bar_drag_covers_handlerless_chrome_only() {
    use oppa::{Row, Text};
    let host = mount_bar(
        HeaderBarProps::new()
            .content(|_| Row("custom-box").child(VNode::from(Text::new("Hi").size(13)))),
    );
    let rects = oppa::window_drag_rects(&host);
    assert!(!rects.is_empty(), "custom chrome publishes a drag region");
    let bar_box = host
        .committed_box(node_by_debug(&host, oppa::HEADER_BAR_DEBUG))
        .expect("bar laid out");
    let area: f32 = rects.iter().map(|r| r[2] * r[3]).sum();
    assert!(
        (area - bar_box.w * bar_box.h).abs() < 1.0,
        "drag area {area} == chrome box {}",
        bar_box.w * bar_box.h
    );
    let empty = mount_bar(HeaderBarProps::new());
    assert!(
        oppa::window_drag_rects(&empty).is_empty(),
        "content-free bar publishes nothing"
    );
}

/// Decision 494: press-owning descendants subtract from the drag
/// region (interactive children keep normal hit-testing while bare
/// chrome drags).
#[test]
fn header_bar_press_owning_children_subtract_from_drag() {
    use oppa::{Row, Text};
    let host = mount_bar(HeaderBarProps::new().content(|_| {
        Row("custom-press")
            .style(oppa::Style::new().size(60, 30))
            .on_press(|| {})
            .child(VNode::from(Text::new("T").size(13)))
    }));
    let tools = node_by_debug(&host, "custom-press");
    let toolbox = host.committed_box(tools).expect("tools laid out");
    let rects = oppa::window_drag_rects(&host);
    assert!(
        !point_in_rects(
            toolbox.x + toolbox.w / 2.0,
            toolbox.y + toolbox.h / 2.0,
            &rects
        ),
        "press-owning children keep their own hit-testing"
    );
}

/// Decision 494: the marker composes with the button markers —
/// custom chrome plus `CaptionButtons` publishes bar-minus-buttons
/// drag plus all three native zones.
#[test]
fn header_bar_composes_with_caption_buttons() {
    use oppa::{Row, Text};
    let (min_a, _min_n) = press_count();
    let (max_a, _max_n) = press_count();
    let (close_a, _close_n) = press_count();
    let buttons = CaptionButtonsProps::new()
        .on_minimize(move || min_a())
        .on_maximize(move || max_a())
        .on_close(move || close_a());
    let host = mount_bar(HeaderBarProps::new().content(move |ctx| {
        Row("custom-chrome").children([
            VNode::from(Text::new("Brand").size(13)),
            ctx.child_auto(&buttons, CaptionButtons),
        ])
    }));
    for debug in ["header-min", "header-max", "header-close"] {
        let id = node_by_debug(&host, debug);
        assert!(
            host.retained_handlers(id)
                .iter()
                .any(|(k, _)| *k == oppa::EventKind::Press),
            "{debug} owns its press"
        );
    }
    let rects = oppa::window_drag_rects(&host);
    let bar_box = host
        .committed_box(node_by_debug(&host, oppa::HEADER_BAR_DEBUG))
        .expect("bar laid out");
    let area: f32 = rects.iter().map(|r| r[2] * r[3]).sum();
    // Rounding-bounded (not bit-exact): committed positions round
    // while extents stay subpixel, so nested intrinsic children can
    // overhang the bar by a rounding residue (a whole unsubtracted
    // button would move the area by 1840 — two orders above this).
    let expect = bar_box.w * bar_box.h - 3.0 * CAPTION_BUTTON_W * 40.0;
    assert!(
        (area - expect).abs() < 64.0,
        "drag area {area} == bar minus buttons {expect}"
    );
    assert_eq!(
        oppa::native_caption_zones(&host).len(),
        3,
        "all three caption zones publish"
    );
}

/// Decision 494: provided buttons own their presses (bridged where
/// no OS zones exist).
#[test]
fn caption_buttons_render_wired_cells() {
    let (min, min_n) = press_count();
    let (max, max_n) = press_count();
    let (close, close_n) = press_count();
    let mut props = CaptionButtonsProps::new();
    props.on_minimize = Some(min);
    props.on_maximize = Some(max);
    props.on_close = Some(close);
    let host = mount_buttons(props);
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

/// Decision 494: missing buttons render nothing (never a dead
/// affordance) — a buttonless marker is a zero-size row with no
/// zones.
#[test]
fn caption_buttons_without_actions_render_nothing() {
    let host = mount_buttons(CaptionButtonsProps::new());
    for debug in ["header-min", "header-max", "header-close"] {
        assert!(
            find_retained_by_debug(&host, debug).is_empty(),
            "{debug} leaves no trace when unwired"
        );
    }
    assert!(
        oppa::native_caption_zones(&host).is_empty(),
        "buttonless marker publishes no zones"
    );
}

/// Decision 486: the tracked hover signal re-renders the buttons
/// with the glow — hovered buttons tint (close goes signal red,
/// others the themed wash), clearing reverts to bare chrome.
/// Paint-only: geometry never moves.
#[test]
fn caption_hover_paints_glow_on_the_hovered_button() {
    let (min, _) = press_count();
    let (max, _) = press_count();
    let (close, _) = press_count();
    let mut props = CaptionButtonsProps::new();
    props.on_minimize = Some(min);
    props.on_maximize = Some(max);
    props.on_close = Some(close);
    let host = mount_buttons(props);
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
    // hand-labeled node would silently become a drag region —
    // compose `HeaderBar` instead).
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
    for id in [
        dark.minimize,
        dark.maximize,
        dark.restore,
        dark.close,
        dark.close_white,
    ] {
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

/// Decision 487: each caption glyph renders as a 10px leaf exactly
/// centered in its 46px button box (horizontal and vertical —
/// geometrically proven, not by inspection).
#[test]
fn caption_glyphs_center_exactly_in_their_buttons() {
    let (min, _) = press_count();
    let (max, _) = press_count();
    let (close, _) = press_count();
    let mut props = CaptionButtonsProps::new();
    props.on_minimize = Some(min);
    props.on_maximize = Some(max);
    props.on_close = Some(close);
    let host = mount_buttons(props);
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
    let mut props = CaptionButtonsProps::new();
    props.on_minimize = Some(min);
    let host = mount_buttons(props);
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

/// Decision 495: the maximize cell swaps its glyph with the live
/// maximized state — square while windowed, overlapping-squares
/// restore while maximized, square again on restore.
#[test]
fn caption_maximize_cell_swaps_restore_glyph_with_state() {
    use crate::header_bar::CaptionIcons;
    let (min, _) = press_count();
    let (max, _) = press_count();
    let (close, _) = press_count();
    let mut props = CaptionButtonsProps::new();
    props.on_minimize = Some(min);
    props.on_maximize = Some(max);
    props.on_close = Some(close);
    let host = mount_buttons(props);
    let leaf_of = |host: &ComponentHost| {
        let button = node_by_debug(host, "header-max");
        host.with_retained_mut(|rec, _| rec.get(button).expect("live button").children[0])
    };
    let windowed = host
        .retained_image(leaf_of(&host))
        .expect("glyph leaf carries an image");
    let want_max = CaptionIcons::deposit(&host.images(), oppa::ThemeMode::Light).maximize;
    assert_eq!(windowed, want_max, "windowed wears the square");
    host.set_maximized(true);
    host.run_until_idle();
    let maximized = host
        .retained_image(leaf_of(&host))
        .expect("glyph leaf carries an image");
    let want_restore = CaptionIcons::deposit(&host.images(), oppa::ThemeMode::Light).restore;
    assert_eq!(maximized, want_restore, "maximized wears restore");
    host.set_maximized(false);
    host.run_until_idle();
    let restored = host
        .retained_image(leaf_of(&host))
        .expect("glyph leaf carries an image");
    assert_eq!(restored, want_max, "restoring swaps back");
}

/// Decision 489: caption cells are exactly adjacent 46px targets
/// sharing one top (native position, zero inset) — the app docks
/// the row flush and the close box ends at the chrome edge, so the
/// published zones land where the OS expects caption buttons.
#[test]
fn caption_buttons_dock_flush_in_row() {
    let (min, _) = press_count();
    let (max, _) = press_count();
    let (close, _) = press_count();
    let mut props = CaptionButtonsProps::new();
    props.on_minimize = Some(min);
    props.on_maximize = Some(max);
    props.on_close = Some(close);
    let host = mount_buttons(props);
    let min_box = host
        .committed_box(node_by_debug(&host, "header-min"))
        .expect("min laid out");
    let max_box = host
        .committed_box(node_by_debug(&host, "header-max"))
        .expect("max laid out");
    let close_box = host
        .committed_box(node_by_debug(&host, "header-close"))
        .expect("close laid out");
    for (debug, b) in [("min", &min_box), ("max", &max_box), ("close", &close_box)] {
        assert!(
            (b.w - CAPTION_BUTTON_W).abs() < 1.0 && (b.h - 40.0).abs() < 1.0,
            "{debug} cell is 46x40, got {b:?}"
        );
    }
    assert!(
        (min_box.y - max_box.y).abs() < 1.0 && (max_box.y - close_box.y).abs() < 1.0,
        "cells ride one top"
    );
    assert!(
        ((min_box.x + min_box.w) - max_box.x).abs() < 1.0
            && ((max_box.x + max_box.w) - close_box.x).abs() < 1.0,
        "cells sit exactly adjacent with zero inset"
    );
}

/// Decision 510: the shared caption outbox publishes level-triggered
/// request signals and drains each exactly once (barless designs
/// idle empty — the hook drains nothing).
#[test]
fn caption_outbox_publishes_and_drains_once() {
    let host = ComponentHost::new();
    let outbox = CaptionOutbox::default();
    assert!(
        outbox.get().is_none(),
        "nothing published before first render"
    );
    assert!(
        outbox.take_actions().is_empty(),
        "draining an empty outbox is quiet"
    );
    let minimize = host.runtime().signal(false);
    let maximize = host.runtime().signal(false);
    let close = host.runtime().signal(false);
    outbox.publish(minimize.clone(), maximize.clone(), close.clone());
    assert!(
        outbox.take_actions().is_empty(),
        "unarmed signals drain nothing"
    );
    minimize.set(true);
    close.set(true);
    assert_eq!(
        outbox.take_actions(),
        vec![CaptionAction::Minimize, CaptionAction::Close],
        "armed requests drain oldest-first"
    );
    assert!(
        outbox.take_actions().is_empty(),
        "each press drains exactly once"
    );
    assert!(!minimize.get() && !close.get(), "reading clears the level");
}
