use super::*;
use oppa::{DrawOp, ThemeMode, ThemeTokens};

#[test]
fn loop_mounts_and_paints_pixels() {
    let (mut loop_, _) = harness();
    let damage = loop_.repaint().expect("repaint works");
    assert!(damage > 0, "first paint damages");
    let rgba = loop_.rgba8().expect("pixels read back");
    assert_eq!(rgba.len(), 200 * 150 * 4);
    // White scene background present (opaque).
    assert!(rgba.chunks_exact(4).any(|p| p == [255, 255, 255, 255]));
}

/// Theme contract round (decision 323): toggling Dark repaints
/// the page background and re-inks the default text — the hybrid
/// from the field (dark inputs, black text, white page) came
/// from neither following the host theme. The probe app carries
/// no root background (transparent — the page shows through, the
/// way the real sink root does).
#[test]
fn loop_dark_theme_repaints_page_and_default_ink() {
    #[derive(Clone)]
    struct BareProps;
    impl Props for BareProps {}
    fn bare_app(_ctx: &Ctx, _p: &BareProps) -> VNode {
        VNode::from(Text {
            text: SharedString::from("hi"),
            style: Text::body_secondary,
        })
    }
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Bare", BareProps, bare_app);
    loop_.repaint().expect("light paint works");
    loop_.host().set_theme(ThemeMode::Dark);
    loop_.host().run_until_idle();
    loop_.repaint().expect("dark repaint works");
    // Page background follows the theme (opaque near-black).
    let dark_bg = ThemeTokens::dark().background;
    let dark_px = [
        ((dark_bg.0 >> 16) & 0xFF) as u8,
        ((dark_bg.0 >> 8) & 0xFF) as u8,
        (dark_bg.0 & 0xFF) as u8,
        255,
    ];
    let rgba = loop_.rgba8().expect("pixels read back");
    assert!(
        rgba.chunks_exact(4).any(|p| p == dark_px),
        "dark page background paints"
    );
    assert!(
        !rgba.chunks_exact(4).any(|p| p == [255, 255, 255, 255]),
        "no Light page white survives the toggle"
    );
    // Default text ink follows the theme (the probe carries no
    // explicit ink).
    let dark_ink = ThemeTokens::dark().text_primary;
    let inks: Vec<Color> = loop_
        .last_plan()
        .expect("plan committed")
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { ink, .. } => Some(*ink),
            _ => None,
        })
        .collect();
    assert!(!inks.is_empty(), "the scene carries text");
    assert!(
        inks.iter().all(|i| *i == dark_ink),
        "dark defaults follow the theme: {inks:?}"
    );
    // Toggling back restores the Light page exactly (one refit
    // per toggle, steady state churn-free).
    loop_.host().set_theme(ThemeMode::Light);
    loop_.host().run_until_idle();
    loop_.repaint().expect("light repaint works");
    let rgba = loop_.rgba8().expect("pixels read back");
    assert!(rgba.chunks_exact(4).any(|p| p == [255, 255, 255, 255]));
}

#[test]
fn loop_simulated_press_flips_and_damages() {
    let (mut loop_, on) = harness();
    let id = oppa::find_retained_by_debug(loop_.host(), "flip")
        .into_iter()
        .next()
        .expect("flip node");
    let b = loop_.host().committed_box(id).expect("hit box");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    let damage = loop_
        .step(InputEvent::pointer_down(cx, cy))
        .expect("down steps");
    let damage = damage
        + loop_
            .step(InputEvent::pointer_up(cx, cy))
            .expect("up steps");
    assert!(on.get(), "simulated press flips the signal");
    assert!(damage > 0, "the flip repaints damage");
}

#[test]
fn escape_exits_only_at_root() {
    let (mut loop_, _) = harness();
    assert!(
        loop_.escape_exits(oppa::input::keys::ESCAPE, true),
        "nothing focused: Escape exits"
    );
    assert!(
        !loop_.escape_exits(oppa::input::keys::ENTER, true),
        "other keys never exit"
    );
    assert!(
        !loop_.escape_exits(oppa::input::keys::ESCAPE, false),
        "releases never exit"
    );
    // Focus the button: Escape now routes (clears focus) instead.
    let id = oppa::find_retained_by_debug(loop_.host(), "flip")
        .into_iter()
        .next()
        .expect("flip node");
    let b = loop_.host().committed_box(id).expect("hit box");
    loop_
        .step(InputEvent::pointer_down(b.x + 1.0, b.y + 1.0))
        .expect("down steps");
    loop_
        .step(InputEvent::pointer_up(b.x + 1.0, b.y + 1.0))
        .expect("up steps");
    // The press flipped focus onto the button (press owner).
    assert_eq!(loop_.host().focused_node(), Some(id));
    assert!(
        !loop_.escape_exits(oppa::input::keys::ESCAPE, true),
        "focused Escape routes, never exits"
    );
}
