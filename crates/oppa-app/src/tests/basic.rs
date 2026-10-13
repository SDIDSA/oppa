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

/// Decision 515: input steps return with loops live (the Play-loops
/// desktop freeze — bare `run_until_idle` in the step path spun
/// forever on never-retiring loop tracks, hanging the UI thread;
/// bounded pumps keep the pump responsive). Would hang pre-fix.
#[test]
fn step_input_returns_while_loops_run() {
    use oppa::{Ease, KeyframeStop, Keyframes, Transition};
    #[derive(Clone)]
    struct LoopProbe {
        on: oppa::Signal<bool>,
        hit: oppa::Signal<bool>,
    }
    impl Props for LoopProbe {}
    fn loop_probe(_ctx: &Ctx, p: &LoopProbe) -> VNode {
        let mut style = Style::new().size(60, 40).bg(Color(0x22_66_CC));
        if p.on.get() {
            style = style
                .opacity(Some(0.4))
                .transition(Transition::new(200, Ease::Linear))
                .keyframes(Keyframes::new(vec![
                    KeyframeStop::new(200, Ease::Linear).opacity(Some(0.2)),
                    KeyframeStop::new(200, Ease::Linear).opacity(Some(1.0)),
                ]));
        }
        let hit = p.hit.clone();
        Div("loop-probe")
            .style(Style::new().size(200, 150))
            .children([
                Div("loop-leaf").style(style).build(),
                Div("tbtn")
                    .style(Style::new().size(96, 32))
                    .on_press(move || hit.set(true))
                    .build(),
            ])
    }
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let rt = loop_.host().runtime();
    let (on, hit) = (rt.signal(false), rt.signal(false));
    loop_.mount(
        "Probe",
        LoopProbe {
            on: on.clone(),
            hit: hit.clone(),
        },
        loop_probe,
    );
    loop_.repaint().expect("mount paints");
    // Start the loop (delta + keyframes arm).
    on.set(true);
    loop_.host().pump_frames(4);
    loop_.host().with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "loop track live");
    });
    // Pointer steps dispatch through live loops instead of hanging.
    let id = oppa::find_retained_by_debug(loop_.host(), "tbtn")
        .into_iter()
        .next()
        .expect("button node");
    let b = loop_.host().committed_box(id).expect("hit box");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    loop_
        .step(InputEvent::pointer_down(cx, cy))
        .expect("down steps under loops");
    loop_
        .step(InputEvent::pointer_up(cx, cy))
        .expect("up steps under loops");
    assert!(hit.get(), "press dispatched while looping");
    loop_.host().with_evaluator(|ev| {
        assert!(ev.active_count() >= 1, "loop still live after input");
    });
    assert!(loop_.host().has_demand(), "demand flows on");
}
#[test]
fn window_options_default_decorated_with_style_builders() {
    use oppa::WindowStyle;
    let plain = WindowOptions::new("App", 400, 300);
    assert_eq!(plain.window_style, WindowStyle::Decorated);
    assert_eq!(
        WindowOptions::new("A", 1, 1).decorated().window_style,
        WindowStyle::Decorated
    );
    assert_eq!(
        WindowOptions::new("A", 1, 1).undecorated().window_style,
        WindowStyle::Undecorated
    );
    assert_eq!(
        WindowOptions::new("A", 1, 1).transparent().window_style,
        WindowStyle::Transparent
    );
    assert_eq!(
        WindowOptions::new("A", 1, 1).extended().window_style,
        WindowStyle::Extended
    );
    assert_eq!(
        WindowOptions::new("A", 1, 1)
            .with_window_style(WindowStyle::Extended)
            .window_style,
        WindowStyle::Extended
    );
}

/// Decision 482: headless loops record `Decorated` with opaque
/// frames until a runner stores the launch style (swapchain
/// reconfigures re-apply the alpha mode from it, so a transparent
/// window never reverts on resize/outdated retries).
#[test]
fn loop_records_window_style_with_opaque_default() {
    use oppa::WindowStyle;
    use wgpu::CompositeAlphaMode;
    let (mut loop_, _) = harness();
    assert_eq!(loop_.window_style(), WindowStyle::Decorated);
    assert!(!loop_.transparent_clear());
    assert_eq!(loop_.surface_alpha_mode(), CompositeAlphaMode::Auto);
    loop_.set_window_style(WindowStyle::Transparent);
    loop_.set_transparent_clear(true);
    assert_eq!(loop_.window_style(), WindowStyle::Transparent);
    assert!(loop_.transparent_clear());
    assert_eq!(
        loop_.surface_alpha_mode(),
        CompositeAlphaMode::PreMultiplied
    );
}

/// Decision 485: native caption buttons default on (the JavaFX
/// `EXTENDED` shape — platform buttons unless the app implements
/// its own) and flip through the builder; headless loops record the
/// choice for the Windows runner to publish zones from.
#[test]
fn window_options_native_caption_defaults_on_and_builder_flips() {
    assert!(
        WindowOptions::new("App", 400, 300).native_caption_buttons,
        "native caption zones default on"
    );
    assert!(
        !WindowOptions::new("A", 1, 1)
            .with_native_caption_buttons(false)
            .native_caption_buttons,
        "opt-out disables the OS zones"
    );
    let (mut loop_, _) = harness();
    assert!(loop_.native_caption_buttons(), "headless loops record on");
    loop_.set_native_caption_buttons(false);
    assert!(
        !loop_.native_caption_buttons(),
        "the runner publishes from the recorded flag"
    );
}
