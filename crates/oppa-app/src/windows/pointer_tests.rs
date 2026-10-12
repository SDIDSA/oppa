use super::window::{raw_client_size, should_skip_resize_for_client_area, window_minimized};
use super::*;
use oppa::text::{TextService, TextStyle};
use oppa::{
    Cluster, Color, Div, FontId, FontMetrics, ShapedGlyph, ShapedRun, SharedString, Signal, Style,
    Text, TextError, TextRun,
};

/// Uniform-advance fake shaper (mirrors the `DesktopLoop`
/// harness: body 14px → 8.75px/char).
struct FakeText;

impl TextService for FakeText {
    fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
        Vec::new()
    }

    fn shape(&self, text: &str, style: &TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        let em = style.font_size_px * style.device_pixel_ratio;
        let adv = em * 0.625;
        let metrics = FontMetrics {
            ascent: em * 0.75,
            descent: em * 0.25,
            line_gap: em * 0.125,
        };
        let mut glyphs = Vec::new();
        let mut clusters = Vec::new();
        for (k, (i, ch)) in text.char_indices().enumerate() {
            let len = ch.len_utf8();
            glyphs.push(ShapedGlyph {
                glyph_id: k as u32,
                x_advance: adv,
                x_offset: 0.0,
                y_offset: 0.0,
            });
            clusters.push(Cluster {
                byte_range: (i, i + len),
                glyph_range: (glyphs.len() - 1, glyphs.len()),
            });
        }
        Ok(ShapedRun {
            total_advance: adv * glyphs.len() as f32,
            text_len_bytes: text.len(),
            runs: vec![TextRun {
                byte_range: (0, text.len()),
                glyph_range: (0, glyphs.len()),
                rtl: false,
                script: 0,
                font_id: FontId(0),
                font_metrics: metrics,
            }],
            glyphs,
            clusters,
        })
    }
}

fn hidden_shell() -> Win32Shell {
    // Deterministic modifiers (decision 417): the shell proc samples
    // live Shift/Ctrl per message — pin both released (same override
    // the shell's own pointer tests pin; the synthetic `shift: true`
    // drive below bypasses sampling, so it is unaffected).
    oppa_shell_win::win::set_key_override(
        windows::Win32::UI::Input::KeyboardAndMouse::VK_SHIFT.0,
        Some(false),
    );
    oppa_shell_win::win::set_key_override(
        windows::Win32::UI::Input::KeyboardAndMouse::VK_CONTROL.0,
        Some(false),
    );
    Win32Shell::new(ShellConfig {
        title: "pointer-test".to_string(),
        width: 200,
        height: 150,
        record_messages: false,
        suppress_os_composition_window: true,
        visible: false,
        style: oppa::WindowStyle::Decorated,
    })
    .expect("hidden test window builds")
}

#[derive(Clone)]
struct FlipProps {
    on: Signal<bool>,
}
impl Props for FlipProps {}

fn flip_app(_ctx: &Ctx, props: &FlipProps) -> VNode {
    let on = props.on.clone();
    Div("screen")
        .style(Style::new().size(200, 150).bg(Color(0xFF_FF_FF)))
        .child(
            Div("flip")
                .style(Style::new().size(96, 32).bg(Color(0x88_88_88)))
                .semantics(oppa::Semantics::button().label("Flip"))
                .on_press(move || on.set(!on.get()))
                .child(VNode::from(Text {
                    text: SharedString::from("Flip"),
                    style: Text::body_secondary,
                })),
        )
}

/// Round 7.19 (decision 294): `drive_cmd` steps the discrete
/// pointer feed into `DesktopLoop` — Down captures, Moves hold
/// the capture owner, Up classifies the tap, Cancel releases
/// without firing, and the legacy `Click` tap still works.
#[test]
fn drive_cmd_steps_discrete_pointer_down_move_up() {
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let on = loop_.host().runtime().signal(false);
    loop_.mount("Flip", FlipProps { on: on.clone() }, flip_app);
    let mut loop_ = loop_;
    let mut shell = hidden_shell();
    let mut ime = WinImeMapper::new();

    let id = oppa::find_retained_by_debug(loop_.host(), "flip")
        .into_iter()
        .next()
        .expect("flip node");
    let b = loop_.host().committed_box(id).expect("hit box");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);

    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::PointerDown {
            x: cx,
            y: cy,
            shift: false,
            button: oppa::PointerButton::Primary,
        },
    )
    .expect("down steps");
    assert_eq!(
        loop_.host().capture_node_for(0),
        Some(id),
        "down captures the button"
    );

    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::PointerMove { x: cx + 5.0, y: cy },
    )
    .expect("move steps");
    assert_eq!(
        loop_.host().capture_node_for(0),
        Some(id),
        "moves hold the capture owner"
    );

    let out = drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::PointerUp {
            x: cx,
            y: cy,
            shift: false,
            button: oppa::PointerButton::Primary,
        },
    )
    .expect("up steps");
    assert!(out.damaged, "the tap repaints");
    assert!(on.get(), "down + up on the button fires the press");
    assert_eq!(
        loop_.host().capture_node_for(0),
        None,
        "up releases the capture"
    );

    // Cancel mid-press releases without firing.
    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::PointerDown {
            x: cx,
            y: cy,
            shift: false,
            button: oppa::PointerButton::Primary,
        },
    )
    .expect("down steps");
    assert_eq!(
        loop_.host().capture_node_for(0),
        Some(id),
        "second down captures again"
    );
    drive_cmd(&mut loop_, &mut shell, &mut ime, Cmd::PointerCancel).expect("cancel steps");
    assert_eq!(
        loop_.host().capture_node_for(0),
        None,
        "cancel releases the capture"
    );
    assert!(
        on.get(),
        "cancel never fires the press (still flipped exactly once)"
    );

    // The legacy Click still synthesizes its tap.
    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::Click {
            dbl_click: false,
            shift: false,
            x: cx as i32,
            y: cy as i32,
        },
    )
    .expect("legacy click steps");
    assert!(!on.get(), "legacy click tap flips back");
}

/// Round 8.2 (decision 298): `drive_cmd` forwards release shift
/// into the router's tap modifiers (the Shift+Click field arm).
#[test]
fn drive_cmd_forwards_shift_to_tap_modifiers() {
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let on = loop_.host().runtime().signal(false);
    loop_.mount("Flip", FlipProps { on: on.clone() }, flip_app);
    let mut loop_ = loop_;
    let mut shell = hidden_shell();
    let mut ime = WinImeMapper::new();

    let id = oppa::find_retained_by_debug(loop_.host(), "flip")
        .into_iter()
        .next()
        .expect("flip node");
    let b = loop_.host().committed_box(id).expect("hit box");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);

    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::PointerDown {
            x: cx,
            y: cy,
            shift: false,
            button: oppa::PointerButton::Primary,
        },
    )
    .expect("down steps");
    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::PointerUp {
            x: cx,
            y: cy,
            shift: true,
            button: oppa::PointerButton::Primary,
        },
    )
    .expect("up steps");
    assert!(on.get(), "shift tap still activates");
    assert!(
        loop_.host().last_press_modifiers().shift,
        "release shift reaches the router"
    );
}

/// Round 9.2 (decision 301): a secondary tap through `drive_cmd`
/// fires `on_secondary_press` (and `on_context_menu` when
/// declared) without ever firing the primary `on_press`.
#[test]
fn drive_cmd_secondary_tap_fires_secondary_never_primary() {
    use oppa::{Ctx, Props, VNode};
    #[derive(Clone)]
    struct MenuBtnProps {
        primary: Signal<bool>,
        secondary: Signal<bool>,
        menu: Signal<bool>,
    }
    impl Props for MenuBtnProps {}
    fn menu_btn(_ctx: &Ctx, p: &MenuBtnProps) -> VNode {
        let (primary, secondary, menu) = (p.primary.clone(), p.secondary.clone(), p.menu.clone());
        Div("menubtn")
            .style(Style::new().size(96, 32))
            .semantics(oppa::Semantics::button().label("Menu"))
            .on_press(move || primary.set(true))
            .on_secondary_press(move || secondary.set(true))
            .on_context_menu(move || menu.set(true))
            .build()
    }
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let (primary, secondary, menu) = (
        loop_.host().runtime().signal(false),
        loop_.host().runtime().signal(false),
        loop_.host().runtime().signal(false),
    );
    loop_.mount(
        "Menu",
        MenuBtnProps {
            primary: primary.clone(),
            secondary: secondary.clone(),
            menu: menu.clone(),
        },
        menu_btn,
    );
    let mut loop_ = loop_;
    let mut shell = hidden_shell();
    let mut ime = WinImeMapper::new();

    let id = oppa::find_retained_by_debug(loop_.host(), "menubtn")
        .into_iter()
        .next()
        .expect("button node");
    let b = loop_.host().committed_box(id).expect("hit box");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    for cmd in [
        Cmd::PointerDown {
            x: cx,
            y: cy,
            shift: false,
            button: oppa::PointerButton::Secondary,
        },
        Cmd::PointerUp {
            x: cx,
            y: cy,
            shift: false,
            button: oppa::PointerButton::Secondary,
        },
    ] {
        drive_cmd(&mut loop_, &mut shell, &mut ime, cmd).expect("secondary steps");
    }
    assert!(!primary.get(), "secondary never fires the primary press");
    assert!(secondary.get(), "secondary tap fires on_secondary_press");
    assert!(menu.get(), "secondary tap fires on_context_menu");
    // Auxiliary taps stay quiet (taxonomy without v1 behavior).
    secondary.set(false);
    menu.set(false);
    for cmd in [
        Cmd::PointerDown {
            x: cx,
            y: cy,
            shift: false,
            button: oppa::PointerButton::Auxiliary,
        },
        Cmd::PointerUp {
            x: cx,
            y: cy,
            shift: false,
            button: oppa::PointerButton::Auxiliary,
        },
    ] {
        drive_cmd(&mut loop_, &mut shell, &mut ime, cmd).expect("auxiliary steps");
    }
    assert!(
        !primary.get() && !secondary.get() && !menu.get(),
        "auxiliary is quiet"
    );
}

/// Round 8.3 (decision 299): `drive_cmd` publishes the hovered
/// style's cursor to the shell on Down/Move (hand over the styled
/// button, arrow over empty space).
#[test]
fn drive_cmd_move_publishes_hover_cursor() {
    use oppa::{Ctx, Props, VNode};
    #[derive(Clone)]
    struct CursorBtn;
    impl Props for CursorBtn {}
    fn cursor_btn(_ctx: &Ctx, _: &CursorBtn) -> VNode {
        Div("cursor-btn")
            .style(
                Style::new()
                    .size(96, 32)
                    .bg(Color(0x22_66_CC))
                    .cursor(oppa::CursorIcon::Pointer),
            )
            .on_press(|| {})
            .build()
    }
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Cur", CursorBtn, cursor_btn);
    let mut loop_ = loop_;
    let mut shell = hidden_shell();
    let mut ime = WinImeMapper::new();

    let id = oppa::find_retained_by_debug(loop_.host(), "cursor-btn")
        .into_iter()
        .next()
        .expect("button node");
    let b = loop_.host().committed_box(id).expect("hit box");
    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::PointerMove {
            x: b.x + b.w / 2.0,
            y: b.y + b.h / 2.0,
        },
    )
    .expect("move steps");
    assert_eq!(
        shell.shared.borrow().current_cursor,
        Some(oppa::CursorIcon::Pointer),
        "hover over the styled button shows the hand"
    );
    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::PointerMove { x: 199.0, y: 149.0 },
    )
    .expect("move steps");
    assert_eq!(
        shell.shared.borrow().current_cursor,
        Some(oppa::CursorIcon::Default),
        "empty space restores the arrow"
    );
}

/// Round 9.1 (decision 300): the wait horizon blocks on settled
/// loops and ticks at the poll bound while background work is
/// live (requested frames, armed holds) — the event-driven
/// policy, headless-proven here (the OS wait itself is shell
/// scope, proven in `oppa-shell-win`).
#[test]
fn wait_timeout_blocks_when_idle_ticks_when_busy() {
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let on = loop_.host().runtime().signal(false);
    loop_.mount("Flip", FlipProps { on: on.clone() }, flip_app);
    let loop_ = loop_;
    assert_eq!(
        wait_timeout_ms(loop_.host()),
        None,
        "settled loop blocks until input"
    );
    // Requested frames are demand (worker drain shape).
    loop_.host().runtime().request_frame();
    assert_eq!(wait_timeout_ms(loop_.host()), Some(8));
    loop_.host().run_until_idle();
    assert_eq!(wait_timeout_ms(loop_.host()), None);
    // An armed hold ticks (holds never demand frames by design —
    // the policy polls them explicitly).
    let id = oppa::find_retained_by_debug(loop_.host(), "flip")
        .into_iter()
        .next()
        .expect("flip node");
    let b = loop_.host().committed_box(id).expect("hit box");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    loop_
        .host()
        .inject_input(oppa::InputEvent::pointer_down(cx, cy));
    loop_.host().run_until_idle();
    assert_eq!(loop_.host().capture_node_for(0), Some(id), "hold is down");
    assert_eq!(wait_timeout_ms(loop_.host()), Some(8));
    loop_
        .host()
        .inject_input(oppa::InputEvent::pointer_cancel());
    loop_.host().run_until_idle();
    assert_eq!(wait_timeout_ms(loop_.host()), None);
}

/// Minimal TextInput-shaped tree (decision 240's shape, local so
/// this module never depends on the controls catalog): focusable
/// outer with its own session over the value signal.
#[derive(Clone)]
struct BlinkFieldProps {
    value: Signal<SharedString>,
}
impl Props for BlinkFieldProps {}

fn blink_field_app(ctx: &Ctx, props: &BlinkFieldProps) -> VNode {
    let session = ctx.edit_session(props.value.clone());
    Div("blink-field")
        .style(Style::new().size(200, 32).bg(Color(0xFF_FF_FF)))
        .semantics(oppa::Semantics::text_field().label("Name"))
        .on_press(move || session.caret_to_end())
        .child(VNode::from(oppa::TextField {
            text: props.value.get(),
            style: Text::body_secondary,
            label: SharedString::from("Name"),
        }))
}

/// Round 15.2 (decision 313): a focused caret wakes the
/// event-driven wait at the next blink flip (within the
/// half-period) instead of blocking past it; unfocused loops
/// still block until input.
#[test]
fn wait_timeout_wakes_for_caret_blink() {
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let value = loop_.host().runtime().signal(SharedString::from(""));
    loop_.mount(
        "BlinkField",
        BlinkFieldProps {
            value: value.clone(),
        },
        blink_field_app,
    );
    let mut loop_ = loop_;
    assert_eq!(wait_timeout_ms(loop_.host()), None, "unfocused loop blocks");
    // Tap the field: collapsed caret, blink reset to visible.
    let id = oppa::find_retained_by_debug(loop_.host(), "blink-field")
        .into_iter()
        .next()
        .expect("field");
    let b = loop_.host().committed_box(id).expect("hit box");
    loop_
        .step(oppa::InputEvent::pointer_down(b.x + 1.0, b.y + 1.0))
        .expect("down steps");
    loop_
        .step(oppa::InputEvent::pointer_up(b.x + 1.0, b.y + 1.0))
        .expect("up steps");
    assert_eq!(loop_.host().focused_node(), Some(id));
    match wait_timeout_ms(loop_.host()) {
        Some(ms) => assert!(
            (1..=500).contains(&ms),
            "blink wake within the half-period, got {ms}"
        ),
        None => panic!("focused caret must wake the wait"),
    }
}

/// Round 21.1 (decision 328): an armed component timer wakes the
/// event-driven wait at its due instant; a timerless loop still
/// blocks until input.
#[test]
fn wait_timeout_wakes_for_component_timer() {
    #[derive(Clone)]
    struct TimerArm;
    impl Props for TimerArm {}
    fn timer_arm_app(ctx: &Ctx, _props: &TimerArm) -> VNode {
        ctx.use_timeout(5000.0, || {});
        VNode::Hole
    }
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    assert_eq!(wait_timeout_ms(loop_.host()), None, "no timers blocks");
    loop_.mount("TimerArm", TimerArm, timer_arm_app);
    loop_.host().run_until_idle();
    match wait_timeout_ms(loop_.host()) {
        Some(ms) => assert!(
            (1..=5000).contains(&ms),
            "timer wake within the delay, got {ms}"
        ),
        None => panic!("armed timer must wake the wait"),
    }
}

/// Round 16.2 (decision 315): a system-theme command re-queries
/// the installed source into the reactive signal (damaged); with
/// no source it stays a quiet no-op.
#[test]
fn drive_cmd_system_theme_syncs_source_into_signal() {
    use oppa::{ScriptedThemeSource, ThemeMode};
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let on = loop_.host().runtime().signal(false);
    loop_.mount("Flip", FlipProps { on: on.clone() }, flip_app);
    let mut shell = hidden_shell();
    let mut ime = WinImeMapper::new();
    // No source: quiet no-op.
    let out = drive_cmd(&mut loop_, &mut shell, &mut ime, Cmd::SystemThemeChanged).expect("drives");
    assert!(!out.damaged, "no source never repaints");
    assert_eq!(loop_.host().theme().mode(), ThemeMode::Light);
    // Scripted Dark: applies + damages.
    let mut source = ScriptedThemeSource::new();
    source.push_reading(ThemeMode::Dark);
    loop_.set_theme_source(Box::new(source));
    let out = drive_cmd(&mut loop_, &mut shell, &mut ime, Cmd::SystemThemeChanged).expect("drives");
    assert!(out.damaged, "mode flip repaints");
    assert_eq!(loop_.host().theme().mode(), ThemeMode::Dark);
}

/// Round 16.3 (decision 316): a close command destroys the
/// window by default, but a refusing handler suppresses
/// destruction — the pump keeps running instead of exiting.
#[test]
fn drive_cmd_close_veto_suppresses_destruction() {
    use ::windows::Win32::UI::WindowsAndMessaging::IsWindow;
    // Default (no handler): approved → destroyed.
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let mut shell = hidden_shell();
    let mut ime = WinImeMapper::new();
    let hwnd = shell.hwnd();
    let out = drive_cmd(&mut loop_, &mut shell, &mut ime, Cmd::CloseRequested).expect("drives");
    assert!(!out.exit && !out.damaged, "close is quiet either way");
    assert!(
        unsafe { !IsWindow(Some(hwnd)).as_bool() },
        "approved close destroys"
    );
    // Refusing handler: suppressed → alive.
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.set_close_handler(std::rc::Rc::new(|| false));
    let mut shell = hidden_shell();
    let mut ime = WinImeMapper::new();
    let hwnd = shell.hwnd();
    let out = drive_cmd(&mut loop_, &mut shell, &mut ime, Cmd::CloseRequested).expect("drives");
    assert!(!out.exit && !out.damaged, "veto is quiet too");
    assert!(
        unsafe { IsWindow(Some(hwnd)).as_bool() },
        "vetoed close keeps the pump running"
    );
}

#[derive(Clone)]
struct TrackProps {
    value: Signal<f32>,
}
impl Props for TrackProps {}

/// Slider-shaped drag surface (the `Slider` track contract in
/// miniature, without the controls dependency): press captures,
/// moves map the capture x over the track box to 0..=100 snapped
/// to 10s — the exact router path `Slider`'s `on_drag` reads
/// through `capture_position`.
fn track_app(ctx: &Ctx, props: &TrackProps) -> VNode {
    let host = ctx.host();
    let value = props.value.clone();
    Div("screen")
        .style(Style::new().size(200, 150).bg(Color(0xFF_FF_FF)))
        .child(
            Div("track")
                .style(Style::new().size(100, 32).bg(Color(0xEE_EE_EE)))
                .on_press(|| {})
                .on_drag(move || {
                    let Some((x, _)) = host.capture_position() else {
                        return;
                    };
                    let track = oppa::find_retained_by_debug(&host, "track")
                        .into_iter()
                        .next()
                        .expect("track retained");
                    let b = host.committed_box(track).expect("track laid out");
                    let frac = ((x - b.x) / b.w).clamp(0.0, 1.0);
                    value.set((frac * 10.0).round() * 10.0);
                })
                .build(),
        )
}

/// A Slider-shaped control fed Down + Moves through `drive_cmd`
/// updates its value from the pointer x (the Windows drag gap,
/// closed: Down focuses + captures, Moves drive the drag math,
/// Up keeps the value and drops the capture).
#[test]
fn drive_cmd_pointer_drag_drives_a_slider_shaped_track() {
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let value = loop_.host().runtime().signal(0.0f32);
    loop_.mount(
        "Track",
        TrackProps {
            value: value.clone(),
        },
        track_app,
    );
    let mut loop_ = loop_;
    let mut shell = hidden_shell();
    let mut ime = WinImeMapper::new();

    let track = oppa::find_retained_by_debug(loop_.host(), "track")
        .into_iter()
        .next()
        .expect("track node");
    let b = loop_.host().committed_box(track).expect("track box");
    let mid_y = b.y + b.h / 2.0;

    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::PointerDown {
            x: b.x + 10.0,
            y: mid_y,
            shift: false,
            button: oppa::PointerButton::Primary,
        },
    )
    .expect("down steps");
    assert_eq!(
        loop_.host().capture_node_for(0),
        Some(track),
        "press captures the track"
    );
    assert_eq!(value.get(), 0.0, "press alone sets nothing");

    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::PointerMove {
            x: b.x + 80.0,
            y: mid_y,
        },
    )
    .expect("move steps");
    assert!(
        (value.get() - 80.0).abs() < 0.001,
        "drag maps x to value, got {}",
        value.get()
    );

    // Past the end pins (clamp, never wrap).
    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::PointerMove {
            x: b.x + 200.0,
            y: mid_y,
        },
    )
    .expect("move steps");
    assert!(
        (value.get() - 100.0).abs() < 0.001,
        "drag past end pins max, got {}",
        value.get()
    );

    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::PointerUp {
            x: b.x + 200.0,
            y: mid_y,
            shift: false,
            button: oppa::PointerButton::Primary,
        },
    )
    .expect("up steps");
    assert!(
        (value.get() - 100.0).abs() < 0.001,
        "release keeps the value, got {}",
        value.get()
    );
    assert_eq!(
        loop_.host().capture_node_for(0),
        None,
        "release drops the capture"
    );
}

/// Round 7.20 (decision 295): firing the registered resize hook
/// (the `WM_SIZE` path the modal sizing loop takes) resizes the
/// viewport and repaints, so border drags present live frames
/// instead of DWM-stretching a stale buffer.
#[test]
fn resize_callback_resizes_viewport_and_repaints() {
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let on = loop_.host().runtime().signal(false);
    loop_.mount("Flip", FlipProps { on: on.clone() }, flip_app);
    let state = Rc::new(RefCell::new(AppState { loop_, gpu: None }));
    let mut shell = hidden_shell();
    let hwnd = shell.hwnd();
    let cb = make_resize_callback(state.clone(), hwnd);
    shell.set_resize_callback(cb.clone());

    assert_eq!(state.borrow().loop_.viewport(), (200, 150));
    cb(300, 200);
    assert_eq!(
        state.borrow().loop_.viewport(),
        (300, 200),
        "the hook resizes the scene"
    );
    assert_eq!(
        state.borrow().loop_.rgba8().expect("pixels").len(),
        300 * 200 * 4,
        "the paint surface refits to the live size"
    );
    // A settled re-fire is a quiet no-op (no repaint storm).
    cb(300, 200);
    assert_eq!(state.borrow().loop_.viewport(), (300, 200));
    // Zero sizes never reach the scene (no surface exists).
    cb(0, 200);
    assert_eq!(state.borrow().loop_.viewport(), (300, 200));
}

/// Round 20.2 (decision 325): `drive_cmd` forwards a horizontal
/// `Scroll` cmd (`dx != 0`, `dy == 0` — the `WM_MOUSEHWHEEL`
/// shape) into the bound horizontal feed, end to end through
/// hit-test resolution. Positive `dx` grows the feed, negative
/// `dx` shrinks it (clamped, never wrapping).
#[test]
fn hwheel_scroll_cmd_drives_bound_horizontal_feed() {
    use oppa::Style;
    #[derive(Clone)]
    struct HScrollCase;
    impl Props for HScrollCase {}
    fn h_scroll_app(ctx: &Ctx, _props: &HScrollCase) -> VNode {
        let x = ctx.scroll_x();
        oppa::Div("root").child(
            oppa::ScrollArea("hlist")
                .style(Style::new().size(200, 40).x(40))
                .on_scroll(|| {})
                .child(
                    oppa::Div("wide")
                        .style(Style::new().size(300, 20).x(x.get()))
                        .build(),
                ),
        )
    }
    let loop_ =
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
    let mut loop_ = loop_;
    let mut shell = hidden_shell();
    let mut ime = WinImeMapper::new();
    // (50, 10) hits the strip — walk-up resolves the list.
    let out = drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::Scroll {
            x: 50.0,
            y: 10.0,
            dx: 30.0,
            dy: 0.0,
        },
    )
    .expect("horizontal scroll steps");
    assert!(out.damaged, "moved strip repaints");
    assert_eq!(offset_x.get(), 30.0, "positive dx grows the feed");
    // Tilt the other way: the feed shrinks back, clamped at 0.
    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::Scroll {
            x: 50.0,
            y: 10.0,
            dx: -120.0,
            dy: 0.0,
        },
    )
    .expect("reverse scroll steps");
    assert_eq!(offset_x.get(), 0.0, "negative dx shrinks, clamped at 0");
}

/// Round 20.2 (decision 325): the resize-skip decision over raw
/// client values — zero/negative areas and iconic state skip,
/// live sizes proceed. Destroyed windows never reach this (the
/// `hwnd_alive` break runs first); the skip is strictly the
/// minimize case.
#[test]
fn skip_decision_covers_minimized_and_zero_areas() {
    assert!(!should_skip_resize_for_client_area(200, 150, false));
    assert!(!should_skip_resize_for_client_area(1, 1, false));
    assert!(should_skip_resize_for_client_area(0, 0, false));
    assert!(should_skip_resize_for_client_area(0, 150, false));
    assert!(should_skip_resize_for_client_area(200, 0, false));
    assert!(should_skip_resize_for_client_area(-3, 100, false));
    assert!(should_skip_resize_for_client_area(200, 150, true));
    assert!(should_skip_resize_for_client_area(0, 0, true));
}

/// Round 20.2 (decision 325): the OS bridge reads a live window
/// as drawable — no false skip in normal operation (a hidden
/// test window is not iconic and reports its real client area).
#[test]
fn live_window_reports_drawable_client_area() {
    let shell = hidden_shell();
    let hwnd = shell.hwnd();
    assert!(!window_minimized(hwnd), "test window is not iconic");
    let (w, h) = raw_client_size(hwnd);
    assert!(w > 0 && h > 0, "live client area is drawable, got {w}x{h}");
    assert!(!client_area_missing(hwnd), "live window never skips");
}

/// Scroll-pipeline regression (Task Studio follow-up): a real
/// `WM_MOUSEWHEEL` wheel-down through the hidden shell moves an
/// unbound `DataGrid`'s owner offset end to end — shell queue →
/// `pump_events` → `take_cmds` → `drive_cmd` → hit-test → owner
/// self-wire. No `bind_scroll` anywhere (the production controls
/// never call it). Covers the attached-overlay swallow: the
/// `Scrollbar` portal spans the whole target, and hit-testing
/// must fall through its handlerless box to the rows beneath.
#[test]
fn wheel_down_scrolls_unbound_datagrid_end_to_end() {
    use ::windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
    use ::windows::Win32::Graphics::Gdi::ClientToScreen;
    use ::windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_MOUSEWHEEL};
    use oppa_controls::{DataGrid, DataGridProps, GridCellProps, GridColumn};

    #[derive(Clone, Debug)]
    struct Item;
    fn cell(_ctx: &Ctx, p: &GridCellProps<Item>) -> VNode {
        Div("wcell")
            .style(Style::new().w(p.width).h(p.height))
            .build()
    }

    let mut loop_ =
        DesktopLoop::new(400, 300, Box::new(FakeText), "Test").expect("headless loop builds");
    let rt = loop_.host().runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:wheel-grid"));
    coll.ingest((0..20).map(|_| Item).collect::<Vec<_>>());
    let props = DataGridProps::new(
        coll,
        vec![GridColumn {
            header: SharedString::from("A"),
            width: 320.0,
            cell,
        }],
        |_: &Item| 36.0,
    )
    .size(320.0, 200.0)
    .overscan(2)
    .debug("wgrid");
    let handle = loop_.mount("WGrid", props, DataGrid);
    let root = handle.root_instance();
    let offset = loop_
        .host()
        .instance_scroll(root)
        .expect("grid scroll handle");
    let list = oppa::find_retained_by_debug(loop_.host(), "wgrid")
        .into_iter()
        .next()
        .expect("grid node");
    assert_eq!(
        loop_.host().bound_scroll(list),
        None,
        "the grid never binds an explicit feed"
    );
    // The wheel point resolves through the overlay to the area.
    assert_eq!(
        loop_.host().scroll_target_at(60.0, 100.0),
        Some(list),
        "hit-testing falls through the handlerless overlay portal"
    );

    // Wheel-down one notch over the grid body (client 60,100 →
    // screen coords for the message pack).
    let mut shell = hidden_shell();
    let mut pt = POINT { x: 60, y: 100 };
    unsafe {
        let _ = ClientToScreen(shell.hwnd(), &mut pt);
    }
    unsafe {
        let wparam = WPARAM(((-120i16 as u16 as u32) << 16) as usize);
        let lparam = LPARAM(((pt.y << 16) | pt.x) as isize);
        SendMessageW(shell.hwnd(), WM_MOUSEWHEEL, Some(wparam), Some(lparam));
    }
    let _ = shell.pump_events();
    let cmds = shell.take_cmds();
    assert_eq!(cmds.len(), 1, "one wheel tick dispatches one cmd");
    let mut ime = WinImeMapper::new();
    let mut moved = false;
    for cmd in cmds {
        let out = drive_cmd(&mut loop_, &mut shell, &mut ime, cmd).expect("wheel drives");
        moved |= out.damaged;
    }
    assert!(moved, "wheel damaged the frame");
    assert_eq!(offset.get(), 120.0, "wheel-down moved the owner offset");
    // Rows rode viewport-relative under the pinned header: row 1
    // sits at 28 + 36 - 120 = -56 with the header at the top.
    let header = oppa::find_retained_by_debug(loop_.host(), "dgrid-header")[0];
    let hb = loop_.host().committed_box(header).expect("header laid out");
    assert!(hb.y.abs() < 0.01, "header stays put, got {}", hb.y);
    let row1 = oppa::find_retained_by_debug(loop_.host(), "dgrid-cell")
        .into_iter()
        .find(|id| {
            loop_
                .host()
                .committed_box(*id)
                .is_some_and(|b| (b.y + 56.0).abs() < 0.01)
        })
        .expect("row 1 rode to y=-56");
    let _ = row1;
}
