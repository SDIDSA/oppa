use super::*;
use oppa::{
    Cluster, Color, Div, DrawOp, FontId, FontMetrics, JustifyContent, Row, ShapedGlyph, ShapedRun,
    SharedString, Signal, Style, Text, TextError, TextRun, ThemeMode, ThemeTokens,
};

/// Uniform-advance fake shaper (body 14px → 8.75px/char).
struct FakeText;

impl TextService for FakeText {
    fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
        Vec::new()
    }

    fn shape(&self, text: &str, style: &oppa::text::TextStyle) -> Result<ShapedRun, TextError> {
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

#[derive(Clone)]
struct FlipProps {
    on: Signal<bool>,
}
impl Props for FlipProps {}

fn flip_app(_ctx: &Ctx, props: &FlipProps) -> VNode {
    let on = props.on.clone();
    let bg = if on.get() {
        Color(0x22_66_CC)
    } else {
        Color(0x88_88_88)
    };
    Div("screen")
        .style(Style::new().size(200, 150).bg(Color(0xFF_FF_FF)))
        .child(
            Div("flip")
                .style(Style::new().size(96, 32).bg(bg))
                .semantics(oppa::Semantics::button().label("Flip"))
                .on_press(move || on.set(!on.get()))
                .child(VNode::from(Text {
                    text: oppa::SharedString::from("Flip"),
                    style: Text::body_secondary,
                })),
        )
}

fn harness() -> (DesktopLoop, Signal<bool>) {
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let on = loop_.host().runtime().signal(false);
    loop_.mount("Flip", FlipProps { on: on.clone() }, flip_app);
    (loop_, on)
}

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

/// Minimal TextInput-shaped tree (decision 240's shape, local so
/// this crate never depends on the controls catalog): focusable
/// outer with its own session over the value signal.
#[derive(Clone)]
struct FieldProps {
    value: Signal<SharedString>,
}
impl Props for FieldProps {}

fn field_app(ctx: &Ctx, props: &FieldProps) -> VNode {
    let session = ctx.edit_session(props.value.clone());
    Div("field-box")
        .style(Style::new().size(200, 32).bg(Color(0xFF_FF_FF)))
        .semantics(oppa::Semantics::text_field().label("Name"))
        .on_press(move || session.caret_to_end())
        .child(VNode::from(oppa::TextField {
            text: props.value.get(),
            style: Text::body_secondary,
            label: SharedString::from("Name"),
        }))
}

fn field_harness() -> (DesktopLoop, Signal<SharedString>) {
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let value = loop_.host().runtime().signal(SharedString::from(""));
    loop_.mount(
        "Field",
        FieldProps {
            value: value.clone(),
        },
        field_app,
    );
    (loop_, value)
}

fn focus_field(loop_: &mut DesktopLoop) {
    let id = oppa::find_retained_by_debug(loop_.host(), "field-box")
        .into_iter()
        .next()
        .expect("field box");
    let b = loop_.host().committed_box(id).expect("hit box");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    loop_
        .step(InputEvent::pointer_down(cx, cy))
        .expect("down steps");
    loop_
        .step(InputEvent::pointer_up(cx, cy))
        .expect("up steps");
    assert_eq!(loop_.host().focused_node(), Some(id));
}

/// Minimal TextArea-shaped tree (Round 22.2 — local like
/// `field_app`, with a multi-line session published the way
/// `TextArea` publishes it).
#[derive(Clone)]
struct AreaProps {
    value: Signal<SharedString>,
}
impl Props for AreaProps {}

fn area_app(ctx: &Ctx, props: &AreaProps) -> VNode {
    let session = ctx.edit_session(props.value.clone());
    session.set_wrap_width(Some(184.0));
    Div("area-box")
        .style(Style::new().size(200, 96).bg(Color(0xFF_FF_FF)))
        .semantics(oppa::Semantics::text_area().label("Notes"))
        .on_press(move || session.caret_to_end())
        .child(VNode::from(oppa::TextArea {
            text: props.value.get(),
            style: Text::body_secondary,
            label: SharedString::from("Notes"),
        }))
}

fn area_harness() -> (DesktopLoop, Signal<SharedString>) {
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let value = loop_.host().runtime().signal(SharedString::from(""));
    loop_.mount(
        "Area",
        AreaProps {
            value: value.clone(),
        },
        area_app,
    );
    (loop_, value)
}

fn focus_area(loop_: &mut DesktopLoop) {
    let id = oppa::find_retained_by_debug(loop_.host(), "area-box")
        .into_iter()
        .next()
        .expect("area box");
    let b = loop_.host().committed_box(id).expect("hit box");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    loop_
        .step(InputEvent::pointer_down(cx, cy))
        .expect("down steps");
    loop_
        .step(InputEvent::pointer_up(cx, cy))
        .expect("up steps");
    assert_eq!(loop_.host().focused_node(), Some(id));
}

#[test]
fn text_field_receives_typed_characters() {
    let (mut loop_, value) = field_harness();
    // Unfocused typing lands nowhere (quiet miss, router
    // precedent) — the signal is untouched.
    assert_eq!(loop_.type_text("Hi").expect("types"), 0);
    assert_eq!(value.get(), SharedString::from(""));
    focus_field(&mut loop_);
    assert!(loop_.type_text("Hi").expect("types") > 0);
    assert_eq!(value.get(), SharedString::from("Hi"));
    // Control chars never reach the session (shortcut shadows
    // ride `Key`, per the spike's WM_CHAR rule).
    assert_eq!(loop_.type_text("\x08\r").expect("types"), 0);
    assert_eq!(value.get(), SharedString::from("Hi"));
}

/// One pressed key with explicit modifiers (Round 22.1 — the
/// exact shape runners feed [`DesktopLoop::step`] for Shift/Ctrl
/// navigation).
fn nav_key(code: u32, shift: bool, ctrl: bool) -> InputEvent {
    InputEvent::Key {
        code,
        modifiers: Modifiers {
            shift,
            ctrl,
            ..Modifiers::NONE
        },
        state: KeyState::Pressed,
        repeat: false,
    }
}

fn session_of(loop_: &DesktopLoop) -> oppa::EditSession {
    loop_
        .host()
        .focused_field_session()
        .expect("focused session")
}

/// Round 22.2 (decision 332): ArrowUp/Down travel hard lines
/// with column affinity in a focused area, Shift+Up/Down
/// extends, and single-line fields keep flowing Up/Down to the
/// router (no session move, no consume).
#[test]
fn area_arrows_travel_hard_lines_and_single_line_ignores() {
    let (mut loop_, value) = area_harness();
    focus_area(&mut loop_);
    loop_.type_text("ab").expect("types");
    loop_
        .step(InputEvent::key(keys::ENTER, KeyState::Pressed))
        .expect("newline steps");
    loop_.type_text("cd").expect("types");
    assert_eq!(value.get(), SharedString::from("ab\ncd"));
    assert_eq!(session_of(&loop_).caret(), 5);
    loop_.step(nav_key(keys::UP, false, false)).expect("steps");
    assert_eq!(session_of(&loop_).caret(), 2, "Up onto the short line end");
    loop_
        .step(nav_key(keys::DOWN, false, false))
        .expect("steps");
    assert_eq!(session_of(&loop_).caret(), 5, "Down restores the column");
    loop_.step(nav_key(keys::UP, true, false)).expect("steps");
    assert_eq!(
        session_of(&loop_).selection(),
        (2, 5),
        "Shift+Up spans to the line above"
    );
    // Wrapped travel: 25 more chars wrap line 1 at 21 columns
    // (184px / 8.75px FakeText advances).
    loop_
        .step(nav_key(keys::DOWN, false, false))
        .expect("steps");
    loop_.type_text(&"e".repeat(25)).expect("types");
    assert_eq!(session_of(&loop_).caret(), 30);
    loop_.step(nav_key(keys::UP, false, false)).expect("steps");
    assert_eq!(
        session_of(&loop_).caret(),
        9,
        "Up keeps the column across wraps"
    );
    loop_
        .step(nav_key(keys::DOWN, false, false))
        .expect("steps");
    assert_eq!(session_of(&loop_).caret(), 30, "Down round-trips it");
    // Single-line fields ignore Up/Down (router-quiet, session
    // untouched).
    let (mut flat, _) = field_harness();
    focus_field(&mut flat);
    flat.type_text("hi").expect("types");
    flat.step(nav_key(keys::UP, false, false)).expect("steps");
    assert_eq!(
        flat.host()
            .focused_field_session()
            .expect("session")
            .caret(),
        2,
        "single-line Up moves nothing"
    );
}

/// Round 22.2 (decision 332): a multi-line selection commits
/// one highlight rect per covered line (CPU plan + GPU twin
/// retain both), and collapsing paints the line-aware caret
/// bar on the caret's own line.
#[test]
fn area_multiline_selection_paints_per_line_rects() {
    let (mut loop_, _) = area_harness();
    focus_area(&mut loop_);
    loop_.type_text("ab").expect("types");
    loop_
        .step(InputEvent::key(keys::ENTER, KeyState::Pressed))
        .expect("newline steps");
    loop_.type_text("cd").expect("types");
    // Select all across both hard lines (Shift+Up from the end).
    loop_.step(nav_key(keys::UP, true, false)).expect("steps");
    loop_.step(nav_key(keys::UP, true, false)).expect("steps");
    assert_eq!(session_of(&loop_).selection(), (0, 5));
    let plan = loop_.last_plan().expect("selection repaints");
    let sels: Vec<(f32, f32, f32, f32)> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            oppa::DrawOp::Rect {
                x, y, w, h, color, ..
            } if *color == oppa::SELECTION_FILL => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect();
    assert_eq!(sels.len(), 2, "one rect per line, got {sels:?}");
    assert!(
        sels[1].1 > sels[0].1,
        "rects stack in line order, got {sels:?}"
    );
    // The CPU backend retains the multiline paint (the GPU
    // twin consumes the identical plan — CPU-only loops leave
    // it untouched by design, and the GPU tests prove twin
    // encoding on text-free scenes; gpu refuses uninjected
    // faces loudly at paint, so no text paint runs headless).
    assert!(
        loop_.cpu_retained_op_count().unwrap_or(0) > 0,
        "CPU backend retains the selection paint"
    );
    // Collapse to the tail: the caret bar paints on line 1.
    loop_
        .step(nav_key(keys::DOWN, false, false))
        .expect("steps");
    assert_eq!(session_of(&loop_).selection(), (5, 5));
    let plan = loop_.last_plan().expect("caret repaints");
    let bars: Vec<(f32, f32, f32, f32)> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            oppa::DrawOp::Rect {
                x, y, w, h, color, ..
            } if (*w - oppa::CARET_WIDTH_PX).abs() < 1e-3 && *color == oppa::INK => {
                Some((*x, *y, *w, *h))
            }
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 1, "one caret bar, got {bars:?}");
    assert!(
        (bars[0].1 - sels[1].1).abs() < 1e-3,
        "caret sits on the second line, bar {bars:?} vs rect {sels:?}"
    );
}

/// Round 22.1 (decision 331): Shift+arrows extend from the
/// caret, Shift+Home/End span to the edges.
#[test]
fn shift_navigation_extends_selection() {
    let (mut loop_, _) = field_harness();
    // Unfocused: flows to the router, session untouched.
    loop_
        .step(nav_key(keys::RIGHT, true, false))
        .expect("steps");
    focus_field(&mut loop_);
    loop_.type_text("hello").expect("types");
    loop_.step(nav_key(keys::LEFT, true, false)).expect("steps");
    assert_eq!(session_of(&loop_).selection(), (4, 5));
    loop_.step(nav_key(keys::LEFT, true, false)).expect("steps");
    assert_eq!(session_of(&loop_).selection(), (3, 5));
    loop_.step(nav_key(keys::HOME, true, false)).expect("steps");
    assert_eq!(session_of(&loop_).selection(), (0, 5));
    assert_eq!(session_of(&loop_).caret(), 0);
    // The anchor persists: Shift+End walks back to it,
    // collapsing the selection onto the anchor.
    loop_.step(nav_key(keys::END, true, false)).expect("steps");
    assert_eq!(session_of(&loop_).selection(), (5, 5));
    assert_eq!(session_of(&loop_).caret(), 5);
}

/// Round 22.1: Ctrl+arrows word-step, Ctrl+Shift+arrows extend
/// by word, plain arrows step by character.
#[test]
fn ctrl_arrows_word_step_and_plain_step() {
    let (mut loop_, _) = field_harness();
    focus_field(&mut loop_);
    loop_.type_text("hello world").expect("types");
    assert_eq!(session_of(&loop_).caret(), 11);
    loop_
        .step(nav_key(keys::LEFT, false, false))
        .expect("steps");
    assert_eq!(session_of(&loop_).caret(), 10, "plain steps a char");
    loop_.step(nav_key(keys::LEFT, false, true)).expect("steps");
    assert_eq!(
        session_of(&loop_).caret(),
        6,
        "word-steps to the word start"
    );
    loop_.step(nav_key(keys::LEFT, false, true)).expect("steps");
    assert_eq!(
        session_of(&loop_).caret(),
        0,
        "word start skips back a word"
    );
    loop_.step(nav_key(keys::LEFT, false, true)).expect("steps");
    assert_eq!(session_of(&loop_).caret(), 0, "pins at the start");
    loop_.step(nav_key(keys::RIGHT, true, true)).expect("steps");
    assert_eq!(session_of(&loop_).selection(), (0, 5), "word-extends");
    loop_
        .step(nav_key(keys::HOME, false, false))
        .expect("steps");
    assert_eq!(session_of(&loop_).caret(), 0, "Home jumps");
    loop_.step(nav_key(keys::END, false, false)).expect("steps");
    assert_eq!(session_of(&loop_).caret(), 11, "End jumps");
}

/// Round 22.1: copy/cut/paste round-trip through the loop
/// clipboard, and masked fields refuse exfiltration (Ctrl+C/X
/// no-op, content intact).
#[test]
fn clipboard_round_trip_and_masked_refusal() {
    let (mut loop_, _) = field_harness();
    focus_field(&mut loop_);
    loop_.type_text("hello").expect("types");
    loop_.step(ctrl_key(keys::A)).expect("steps");
    loop_.step(ctrl_key(keys::C)).expect("steps");
    assert_eq!(
        loop_.clipboard().read_text_now().expect("reads"),
        Some("hello".to_string()),
        "copy lands on the clipboard"
    );
    loop_.step(ctrl_key(keys::X)).expect("steps");
    assert_eq!(
        session_of(&loop_).content_text(),
        "",
        "cut deletes the selection"
    );
    loop_.step(ctrl_key(keys::V)).expect("steps");
    assert_eq!(
        session_of(&loop_).content_text(),
        "hello",
        "paste replaces the selection and advances"
    );
    // Masked: copy and cut refuse, content intact, clipboard
    // untouched.
    session_of(&loop_).set_masked(true);
    loop_.step(ctrl_key(keys::A)).expect("steps");
    loop_.clipboard().write_text("sentinel").expect("writes");
    loop_.step(ctrl_key(keys::C)).expect("steps");
    assert_eq!(
        loop_.clipboard().read_text_now().expect("reads"),
        Some("sentinel".to_string()),
        "masked copy exfiltrates nothing"
    );
    loop_.step(ctrl_key(keys::X)).expect("steps");
    assert_eq!(
        session_of(&loop_).content_text(),
        "hello",
        "masked cut deletes nothing"
    );
    assert_eq!(
        loop_.clipboard().read_text_now().expect("reads"),
        Some("sentinel".to_string()),
        "masked cut writes nothing"
    );
}

/// One pressed Ctrl+letter key event (decision 246): the exact
/// shape both platform runners feed [`DesktopLoop::step`].
fn ctrl_key(code: u32) -> InputEvent {
    InputEvent::Key {
        code,
        modifiers: Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        },
        state: KeyState::Pressed,
        repeat: false,
    }
}

#[test]
fn ctrl_a_selects_all_and_ctrl_c_copies() {
    let (mut loop_, value) = field_harness();
    // Unfocused shortcuts miss quietly (router precedent).
    assert_eq!(loop_.step(ctrl_key(keys::A)).expect("steps"), 0);
    focus_field(&mut loop_);
    loop_.type_text("hello").expect("types");
    // Select-all then copy: the clipboard proves the selection
    // spanned the whole field (a collapsed caret would copy
    // nothing and leave the clipboard empty).
    loop_.step(ctrl_key(keys::A)).expect("steps");
    loop_.step(ctrl_key(keys::C)).expect("steps");
    assert_eq!(value.get(), SharedString::from("hello"));
    assert_eq!(
        loop_.clipboard().read_text_now().expect("reads"),
        Some("hello".to_string())
    );
    // Cut removes the selection and keeps the clipboard text.
    loop_.step(ctrl_key(keys::X)).expect("steps");
    assert_eq!(value.get(), SharedString::from(""));
    assert_eq!(
        loop_.clipboard().read_text_now().expect("reads"),
        Some("hello".to_string())
    );
}

#[test]
fn ctrl_v_pastes_clipboard_text() {
    let (mut loop_, value) = field_harness();
    focus_field(&mut loop_);
    loop_
        .clipboard()
        .write_text("pasted")
        .expect("clipboard writes");
    loop_.step(ctrl_key(keys::V)).expect("steps");
    assert_eq!(value.get(), SharedString::from("pasted"));
    // Pasting over a selection replaces it (session `insert`
    // rule, not append).
    loop_.step(ctrl_key(keys::A)).expect("steps");
    loop_.clipboard().write_text("over").expect("writes");
    loop_.step(ctrl_key(keys::V)).expect("steps");
    assert_eq!(value.get(), SharedString::from("over"));
}

#[test]
fn ctrl_z_undos_and_ctrl_y_redos() {
    let (mut loop_, value) = field_harness();
    focus_field(&mut loop_);
    loop_.type_text("ab").expect("types");
    loop_.backspace().expect("backspaces");
    assert_eq!(value.get(), SharedString::from("a"));
    // Two discrete undo levels: the backspace entry, then the
    // coalesced insert run.
    loop_.step(ctrl_key(keys::Z)).expect("steps");
    assert_eq!(value.get(), SharedString::from("ab"));
    loop_.step(ctrl_key(keys::Z)).expect("steps");
    assert_eq!(value.get(), SharedString::from(""));
    // ... and back through the same two levels.
    loop_.step(ctrl_key(keys::Y)).expect("steps");
    assert_eq!(value.get(), SharedString::from("ab"));
    loop_.step(ctrl_key(keys::Y)).expect("steps");
    assert_eq!(value.get(), SharedString::from("a"));
}

/// Clock-injected field rig (Round 15.2 — the headless blink
/// harness: a `MockClock` owner advances time explicitly, so flip
/// demand proves without sleeps).
fn clock_harness() -> (DesktopLoop, Signal<SharedString>, Rc<oppa::MockClock>) {
    let clock = Rc::new(oppa::MockClock::new());
    let loop_ = DesktopLoop::with_clock(200, 150, Box::new(FakeText), "Test", clock.clone())
        .expect("headless loop builds");
    let value = loop_.host().runtime().signal(SharedString::from(""));
    loop_.mount(
        "Field",
        FieldProps {
            value: value.clone(),
        },
        field_app,
    );
    (loop_, value, clock)
}

/// Round 15.2 (decision 313): selecting text in a focused field
/// commits themed selection `Rect`s in the loop's display list —
/// live frames ran `build_full` with the overlay `None`, so GPU
/// and GDI presented zero rects.
#[test]
fn desktop_repaint_commits_selection_rects() {
    let (mut loop_, _) = field_harness();
    focus_field(&mut loop_);
    loop_.type_text("hello world").expect("types");
    // Select-all through the session (collapsed caret → open
    // range), then commit exactly like the input path does.
    loop_
        .host()
        .focused_field_session()
        .expect("session")
        .select_all();
    loop_.repaint().expect("repaints");
    let plan = loop_.last_plan().expect("committed plan");
    let sels: Vec<(f32, f32, f32, f32)> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            oppa::DrawOp::Rect {
                x, y, w, h, color, ..
            } if *color == oppa::SELECTION_FILL => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect();
    assert_eq!(sels.len(), 1, "one line, one highlight, got {sels:?}");
    let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
        .into_iter()
        .next()
        .expect("field box");
    let origin_x = loop_.host().text_origin_under(field).expect("text origin");
    // 11 FakeText advances at 8.75px from the text origin.
    assert!(
        (sels[0].0 - origin_x).abs() < 1e-3 && (sels[0].2 - 96.25).abs() < 1e-2,
        "highlight pins the selected bytes, got {:?} vs origin {origin_x}",
        sels[0]
    );
    // One overlay at a time: the open range hides the caret bar.
    assert!(
        !plan.ops.iter().any(|op| matches!(op,
                oppa::DrawOp::Rect { w, color, .. }
                    if (*w - oppa::CARET_WIDTH_PX).abs() < 1e-3 && *color == oppa::INK)),
        "selected range paints no caret bar"
    );
}

/// Round 15.2 (decision 313): a focused collapsed caret commits
/// its 2px bar in the loop's display list (the same zero-bar gap
/// as the selection — one fix covers both overlays).
#[test]
fn desktop_repaint_commits_caret_bar() {
    let (mut loop_, _) = field_harness();
    focus_field(&mut loop_);
    // The tap reset the blink phase, and typing resets it again —
    // the bar is solid-visible at commit time (no clock needed
    // for the position pin).
    loop_.type_text("hello").expect("types");
    let plan = loop_.last_plan().expect("type_text repaints");
    let bars: Vec<(f32, f32, f32, f32)> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            oppa::DrawOp::Rect {
                x, y, w, h, color, ..
            } if (*w - oppa::CARET_WIDTH_PX).abs() < 1e-3 && *color == oppa::INK => {
                Some((*x, *y, *w, *h))
            }
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 1, "exactly one caret bar, got {bars:?}");
    let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
        .into_iter()
        .next()
        .expect("field box");
    let origin_x = loop_.host().text_origin_under(field).expect("text origin");
    // Trailing caret after five FakeText advances.
    assert!(
        (bars[0].0 - (origin_x + 5.0 * 8.75)).abs() < 1e-2,
        "bar at the trailing edge, got {:?} vs {}",
        bars[0],
        origin_x + 43.75
    );
    assert!(bars[0].3 > 0.0, "positive bar height");
}

/// Round 15.2 (decision 313): the blink flip demands a repaint
/// with no input — `caret_overlay_dirty` goes true on the hidden
/// half-cycle, `poll_blink` commits it (the bar leaves the plan),
/// and the tick horizon points at the next flip.
#[test]
fn blink_flip_demands_and_poll_repaints() {
    let (mut loop_, _, clock) = clock_harness();
    focus_field(&mut loop_);
    loop_.type_text("hi").expect("types");
    assert!(
        !loop_.caret_overlay_dirty(),
        "settled: snapshot matches the committed overlay"
    );
    let wake = loop_
        .blink_tick_in_secs()
        .expect("focused caret arms a tick");
    assert!(
        wake > 0.0 && wake <= 0.5,
        "horizon within the visible half, got {wake}"
    );
    clock.advance(0.6);
    assert!(loop_.caret_overlay_dirty(), "hidden flip demands a repaint");
    assert!(loop_.poll_blink().expect("polls"), "poll commits the flip");
    let plan = loop_.last_plan().expect("committed plan");
    assert!(
        !plan.ops.iter().any(|op| matches!(op,
                oppa::DrawOp::Rect { w, color, .. }
                    if (*w - oppa::CARET_WIDTH_PX).abs() < 1e-3 && *color == oppa::INK)),
        "hidden phase commits no bar"
    );
    assert!(!loop_.caret_overlay_dirty(), "snapshot follows the flip");
    assert!(
        !loop_.poll_blink().expect("polls"),
        "settled poll stays quiet"
    );
    let next = loop_.blink_tick_in_secs().expect("next flip armed");
    assert!(
        next > 0.3 && next <= 0.5,
        "wake points at the visible flip, got {next}"
    );
}

/// A save backend that always fails (backend-error grace shape).
struct FailingSave;
impl oppa::SaveFileDialog for FailingSave {
    fn save(&mut self, _: oppa::FileDialogOptions) -> Result<Option<PathBuf>, oppa::PickError> {
        Err(oppa::PickError::Backend("boom".to_string()))
    }
}

/// Round 16.1 (decision 314): save prompts return the scripted
/// destination in order (each call reaches the backend — never
/// a cached answer); dismissal and missing backends settle
/// `None` without panic, and backend errors degrade loudly to
/// `None` instead of killing the app.
#[test]
fn save_file_dialog_returns_pick_and_graceful_none() {
    use oppa::{FileDialogOptions, FileFilter, ScriptedSaveDialog};
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    // No backend installed: graceful None (headless/web rule).
    assert_eq!(loop_.save_file_dialog(FileDialogOptions::default()), None);
    let mut scripted = ScriptedSaveDialog::new();
    scripted.push_response(Some(PathBuf::from("/tmp/a.txt")));
    scripted.push_response(Some(PathBuf::from("/tmp/b.txt")));
    loop_.set_save_dialog(Box::new(scripted));
    let options = FileDialogOptions {
        title: "Save".to_string(),
        filters: vec![FileFilter {
            name: "Text".to_string(),
            patterns: vec!["*.txt".to_string()],
        }],
        default_name: "a.txt".to_string(),
        initial_dir: Some(PathBuf::from("/tmp")),
    };
    assert_eq!(
        loop_.save_file_dialog(options),
        Some(PathBuf::from("/tmp/a.txt"))
    );
    assert_eq!(
        loop_.save_file_dialog(FileDialogOptions::default()),
        Some(PathBuf::from("/tmp/b.txt")),
        "second call reaches the backend too"
    );
    // Exhausted script = dismissed.
    assert_eq!(loop_.save_file_dialog(FileDialogOptions::default()), None);
    // Backend failure degrades to None (loud on stderr, no panic).
    loop_.set_save_dialog(Box::new(FailingSave));
    assert_eq!(loop_.save_file_dialog(FileDialogOptions::default()), None);
}

/// Round 16.1 (decision 314): folder prompts return the scripted
/// directory; dismissal and missing backends settle `None`
/// without panic.
#[test]
fn pick_folder_dialog_returns_dir_and_graceful_none() {
    use oppa::{FolderDialogOptions, ScriptedFolderDialog};
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    assert_eq!(
        loop_.pick_folder_dialog(FolderDialogOptions::default()),
        None
    );
    let mut scripted = ScriptedFolderDialog::new();
    scripted.push_response(Some(PathBuf::from("/tmp/out")));
    loop_.set_folder_dialog(Box::new(scripted));
    assert_eq!(
        loop_.pick_folder_dialog(FolderDialogOptions {
            title: "Pick".to_string(),
            initial_dir: Some(PathBuf::from("/tmp")),
        }),
        Some(PathBuf::from("/tmp/out"))
    );
    assert_eq!(
        loop_.pick_folder_dialog(FolderDialogOptions::default()),
        None
    );
}

/// Round 16.2 (decision 315): a scripted OS reading flips the
/// reactive theme in place — `ctx.theme().mode()` follows, the
/// themed fill recolors in the committed plan, and redundant
/// readings stay quiet (no repaint spin).
#[test]
fn sync_system_theme_forwards_os_mode_and_recolors() {
    use oppa::{ScriptedThemeSource, ThemeMode, ThemeTokens};
    #[derive(Clone)]
    struct ThemeProps;
    impl Props for ThemeProps {}
    fn theme_app(ctx: &Ctx, _: &ThemeProps) -> VNode {
        Div("themed")
            .style(Style::new().size(200, 150).bg(ctx.theme().tokens().surface))
            .build()
    }
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Theme", ThemeProps, theme_app);
    // No source installed: quiet no-op (headless default).
    assert!(!loop_.sync_system_theme().expect("syncs"));
    assert_eq!(loop_.host().theme().mode(), ThemeMode::Light);
    let mut source = ScriptedThemeSource::new();
    source.push_reading(ThemeMode::Dark);
    loop_.set_theme_source(Box::new(source));
    assert!(
        loop_.sync_system_theme().expect("syncs"),
        "dark reading applies"
    );
    assert_eq!(loop_.host().theme().mode(), ThemeMode::Dark);
    let plan = loop_.last_plan().expect("sync repaints");
    let dark = ThemeTokens::dark().surface;
    assert!(
        plan.ops.iter().any(|op| matches!(op,
                oppa::DrawOp::Rect { color, .. } if *color == dark)),
        "themed fill recolors in place"
    );
    // Exhausted script repeats Dark: same mode stays quiet.
    assert!(!loop_.sync_system_theme().expect("syncs"));
    assert_eq!(loop_.host().theme().mode(), ThemeMode::Dark);
}

/// Round 16.3 (decision 316): window-chrome calls forward into
/// the installed control in order, stay quiet with none, and
/// the close veto defaults to closing while a refusing handler
/// suppresses.
#[test]
fn window_chrome_forwards_and_close_veto_decides() {
    use oppa::{ScriptedWindowControl, WindowCall, WindowIcon};
    use std::rc::Rc;
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    // No control installed: every call is a quiet no-op.
    loop_.set_title("Hi");
    loop_.set_min_size(Some((100, 80)));
    loop_.set_max_size(Some((400, 300)));
    loop_.set_fullscreen(true);
    loop_.set_icon(None);
    // No handler installed: closes (pre-16.3 behavior).
    assert!(loop_.close_requested(), "default closes");
    // The scripted double shares its record across clones: the
    // loop owns one behind the trait object, the test reads
    // through its own.
    let control = ScriptedWindowControl::new();
    let mut loop_ = loop_;
    loop_.set_window_control(Box::new(control.clone()));
    loop_.set_title("Hi");
    loop_.set_min_size(Some((100, 80)));
    loop_.set_max_size(None);
    loop_.set_fullscreen(true);
    loop_.set_fullscreen(false);
    let icon = WindowIcon::new(vec![9; 4 * 4 * 4], 4, 4).expect("valid icon builds");
    loop_.set_icon(Some(icon.clone()));
    loop_.set_icon(None);
    assert_eq!(
        control.calls(),
        vec![
            WindowCall::SetTitle("Hi".to_string()),
            WindowCall::SetMinSize(Some((100, 80))),
            WindowCall::SetMaxSize(None),
            WindowCall::SetFullscreen(true),
            WindowCall::SetFullscreen(false),
            WindowCall::SetIcon(Some(icon)),
            WindowCall::SetIcon(None),
        ]
    );
    // Veto: refusing handler suppresses, approving closes.
    loop_.set_close_handler(Rc::new(|| false));
    assert!(!loop_.close_requested(), "refusal suppresses exit");
    loop_.set_close_handler(Rc::new(|| true));
    assert!(loop_.close_requested(), "approval closes");
}

/// Round 21.1 (decision 328): the loop fires due component
/// timers through the wake query + tick path — the timeout flips
/// the signal the body reads (fire → settle → repaint), the
/// re-render re-arms per-render, and quiet ticks settle nothing.
#[test]
fn loop_tick_timers_fires_settles_and_repaints() {
    #[derive(Clone)]
    struct TimerCase {
        fired: Signal<bool>,
    }
    impl Props for TimerCase {}
    fn timer_app(ctx: &Ctx, props: &TimerCase) -> VNode {
        let f = props.fired.clone();
        let f2 = props.fired.clone();
        ctx.use_timeout(100.0, move || {
            f2.set(true);
        });
        Div("root").child(
            Div("box")
                .style(Style::new().size(if f.get() { 10.0 } else { 20.0 }, 10.0))
                .build(),
        )
    }
    let clock = Rc::new(oppa::MockClock::new());
    let mut loop_ = DesktopLoop::with_clock(200, 150, Box::new(FakeText), "Test", clock.clone())
        .expect("headless loop builds");
    let fired = loop_.host().runtime().signal(false);
    loop_.mount(
        "Timer",
        TimerCase {
            fired: fired.clone(),
        },
        timer_app,
    );
    loop_.repaint().expect("paints the base");
    assert_eq!(
        loop_.next_timer_due_ms(),
        Some(100.0),
        "armed timeout names its horizon"
    );
    clock.advance(0.05);
    assert_eq!(loop_.tick_timers().expect("ticks"), 0, "quiet before due");
    assert!(!fired.get());
    clock.advance(0.05);
    assert_eq!(loop_.tick_timers().expect("ticks"), 1, "fires at due");
    assert!(fired.get(), "callback flipped the signal");
    // The fire re-rendered (tracked read) and re-armed
    // per-render: the horizon moved exactly one period out.
    assert_eq!(
        loop_.next_timer_due_ms(),
        Some(200.0),
        "re-render re-arms per-render"
    );
    assert_eq!(
        loop_.tick_timers().expect("ticks"),
        0,
        "no immediate refire"
    );
}

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

/// Round 2.1: normalized IME feeds ride the focused session with
/// atomic undo, and the anchor tracks the composition caret.
#[test]
fn ime_preedit_commit_cycle_with_atomic_undo() {
    let (mut loop_, value) = field_harness();
    // Unfocused feeds miss quietly (the type_text precedent).
    assert_eq!(loop_.feed_ime_preedit("ni", Some(2)).expect("feeds"), 0);
    assert_eq!(loop_.feed_ime_commit("你").expect("feeds"), 0);
    assert_eq!(loop_.feed_ime_cancel().expect("feeds"), 0);
    assert_eq!(value.get(), SharedString::from(""));
    focus_field(&mut loop_);
    // Preedit overlays the composition — committed value untouched.
    loop_.feed_ime_preedit("nihao", Some(5)).expect("feeds");
    assert_eq!(value.get(), SharedString::from(""));
    let session = loop_.host().focused_field_session().expect("session");
    assert_eq!(session.composition_text(), "nihao");
    assert!(session.is_composing());
    // Commit applies as one undo unit (pre-composition snapshot).
    loop_.feed_ime_commit("你好").expect("feeds");
    assert_eq!(value.get(), SharedString::from("你好"));
    assert!(!session.is_composing());
    session.undo();
    assert_eq!(value.get(), SharedString::from(""));
}

#[test]
fn ime_cancel_discards_preedit_and_empty_feeds_noop() {
    let (mut loop_, value) = field_harness();
    focus_field(&mut loop_);
    loop_.feed_ime_preedit("ka", Some(2)).expect("feeds");
    let session = loop_.host().focused_field_session().expect("session");
    assert!(session.is_composing());
    loop_.feed_ime_cancel().expect("feeds");
    assert_eq!(value.get(), SharedString::from(""));
    assert!(!session.is_composing());
    // A cancel with nothing composing, an empty preedit (the
    // winit pre-commit clear), and an empty commit all no-op.
    assert_eq!(loop_.feed_ime_cancel().expect("feeds"), 0);
    assert_eq!(loop_.feed_ime_preedit("", None).expect("feeds"), 0);
    assert_eq!(loop_.feed_ime_commit("").expect("feeds"), 0);
    assert!(!session.is_composing());
}

#[test]
fn ime_anchor_tracks_the_composition_caret() {
    let (mut loop_, _) = field_harness();
    assert_eq!(loop_.ime_anchor(), None, "unfocused: no anchor");
    focus_field(&mut loop_);
    // FakeText at body 14px: 8.75px/char; the field-box leaf sits
    // at the origin, so the anchor x is the caret advance.
    loop_.feed_ime_preedit("nihao", Some(2)).expect("feeds");
    let near = loop_.ime_anchor().expect("anchor while composing");
    assert_eq!(near[0], 17.5, "caret x at 2 chars, got {near:?}");
    assert_eq!((near[1], near[3]), (0.0, 14.0), "line box, got {near:?}");
    loop_.feed_ime_preedit("nihao", Some(5)).expect("feeds");
    let far = loop_.ime_anchor().expect("anchor follows caret");
    assert_eq!(far[0], 43.75, "caret x at 5 chars, got {far:?}");
    assert!(far[0] > near[0], "anchor moves with the caret");
    // After commit the anchor sits at the trailing caret of the
    // committed text (2 FakeText glyphs x 8.75).
    loop_.feed_ime_commit("你好").expect("feeds");
    let done = loop_.ime_anchor().expect("anchor after commit");
    assert_eq!(done[0], 17.5, "trailing caret kept, got {done:?}");
}

/// Round 2.1 Windows half: snapshotted `WM_IME_*` messages through
/// the app mapper into the focused session, with commit-echo
/// swallow and candidate anchoring. Headless (scripted messages —
/// the OS IME itself is the one input no harness can script).
#[cfg(windows)]
mod win_ime {
    use super::*;
    use crate::windows::ime::utf16_offset_to_byte;
    use crate::windows::{drive_cmd, drive_ime, WinImeMapper};
    use oppa::EditSession;
    use oppa_shell_win::{ImeMessage, ShellConfig, ShellEvent, Win32Shell};
    use std::rc::Rc;

    fn mapper() -> WinImeMapper {
        WinImeMapper::new()
    }

    fn session_of(loop_: &DesktopLoop) -> EditSession {
        loop_.host().focused_field_session().expect("session")
    }

    fn hidden_shell() -> Win32Shell {
        Win32Shell::new(ShellConfig {
            title: "ime-test".to_string(),
            width: 200,
            height: 150,
            record_messages: false,
            suppress_os_composition_window: true,
            visible: false,
        })
        .expect("hidden test window builds")
    }

    #[test]
    fn utf16_offsets_map_to_byte_boundaries() {
        assert_eq!(utf16_offset_to_byte("nihao", 5), 5);
        assert_eq!(utf16_offset_to_byte("nihao", 99), 5);
        // "héllo": é is 2 bytes / 1 unit — unit 2 lands on byte 3.
        assert_eq!(utf16_offset_to_byte("héllo", 2), 3);
        // Surrogate pair: 2 units, 4 bytes — mid-pair floors in.
        assert_eq!(utf16_offset_to_byte("a👍b", 2), 1);
        assert_eq!(utf16_offset_to_byte("a👍b", 3), 5);
    }

    #[test]
    fn mapper_start_update_commit_cancel_sequence() {
        let (mut loop_, value) = field_harness();
        focus_field(&mut loop_);
        let mut ime = mapper();
        let s = session_of(&loop_);
        // Start over a collapsed caret: single Started.
        let evs = ime.map_message(&ImeMessage::StartComposition, &s);
        assert_eq!(
            evs,
            vec![oppa::ImeCompositionEvent::CompositionStarted { start_byte: 0 }]
        );
        loop_.feed_ime(&evs).expect("feeds");
        // Update while composing: bare Updated.
        let s = session_of(&loop_);
        let evs = ime.map_message(
            &ImeMessage::Composition {
                gcs_flags: 0,
                comp: Some("nihao".to_string()),
                attrs: None,
                cursor_pos: 5,
                delta_start: 0,
                result: None,
            },
            &s,
        );
        assert_eq!(
            evs,
            vec![oppa::ImeCompositionEvent::CompositionUpdated {
                composition: "nihao".to_string(),
                caret_byte: 5,
            }]
        );
        // Result commits and arms the echo swallow (UTF-16 units).
        let s = session_of(&loop_);
        let evs = ime.map_message(
            &ImeMessage::Composition {
                gcs_flags: 0,
                comp: None,
                attrs: None,
                cursor_pos: 0,
                delta_start: 0,
                result: Some("你好".to_string()),
            },
            &s,
        );
        assert_eq!(
            evs,
            vec![oppa::ImeCompositionEvent::CompositionCommitted {
                committed: "你好".to_string(),
            }]
        );
        assert_eq!(ime.swallow_pending, 2, "two BMP units held");
        loop_.feed_ime(&evs).expect("feeds");
        assert_eq!(value.get(), SharedString::from("你好"));
        // End after the commit: nothing to cancel.
        let s = session_of(&loop_);
        assert!(ime.map_message(&ImeMessage::EndComposition, &s).is_empty());
        // A fresh update-cancel round trips through cancel.
        let s = session_of(&loop_);
        let start = ime.map_message(&ImeMessage::StartComposition, &s);
        loop_.feed_ime(&start).expect("feeds");
        let s = session_of(&loop_);
        let evs = ime.map_message(&ImeMessage::EndComposition, &s);
        assert_eq!(evs, vec![oppa::ImeCompositionEvent::CompositionCancelled]);
        loop_.feed_ime(&evs).expect("feeds");
        assert_eq!(
            value.get(),
            SharedString::from("你好"),
            "cancel keeps commits"
        );
    }

    #[test]
    fn mapper_start_replaces_the_selection_atomically() {
        let (mut loop_, value) = field_harness();
        focus_field(&mut loop_);
        loop_.type_text("hello").expect("types");
        let s = session_of(&loop_);
        s.select_all();
        let mut ime = mapper();
        let evs = ime.map_message(&ImeMessage::StartComposition, &s);
        assert_eq!(
            evs,
            vec![
                oppa::ImeCompositionEvent::CompositionStarted { start_byte: 0 },
                oppa::ImeCompositionEvent::DeleteRange { range: (0, 5) },
            ],
            "started-before-delete (pre-deletion undo snapshot)"
        );
        loop_.feed_ime(&evs).expect("feeds");
        loop_.feed_ime_commit("你好").expect("feeds");
        assert_eq!(value.get(), SharedString::from("你好"));
        session_of(&loop_).undo();
        assert_eq!(
            value.get(),
            SharedString::from("hello"),
            "undo restores pre-selection"
        );
    }

    #[test]
    fn drive_cmd_swallows_commit_echo_but_passes_real_chars() {
        let (mut loop_, value) = field_harness();
        focus_field(&mut loop_);
        let mut shell = hidden_shell();
        let mut ime = mapper();
        // Commit "ab" (2 echo units) through the driver.
        let s = session_of(&loop_);
        let evs = ime.map_message(
            &ImeMessage::Composition {
                gcs_flags: 0,
                comp: None,
                attrs: None,
                cursor_pos: 0,
                delta_start: 0,
                result: Some("ab".to_string()),
            },
            &s,
        );
        loop_.feed_ime(&evs).expect("feeds");
        assert_eq!(value.get(), SharedString::from("ab"));
        // The two echo WM_CHARs die quietly.
        for ch in ['a', 'b'] {
            let out = drive_cmd(
                &mut loop_,
                &mut shell,
                &mut ime,
                oppa_shell_win::Cmd::Char { ch },
            )
            .expect("drives");
            assert!(!out.damaged, "echo paints nothing");
        }
        assert_eq!(value.get(), SharedString::from("ab"));
        assert_eq!(ime.swallow_pending, 0, "echo consumed");
        // A real keystroke clears nothing pending (already zero)
        // and its char inserts.
        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            oppa_shell_win::Cmd::Key {
                vk: oppa::input::keys::A,
                shift: false,
                ctrl: false,
            },
        )
        .expect("drives");
        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            oppa_shell_win::Cmd::Char { ch: 'c' },
        )
        .expect("drives");
        assert_eq!(value.get(), SharedString::from("abc"));
    }

    #[test]
    fn drive_ime_end_to_end_feeds_and_anchors() {
        let (mut loop_, value) = field_harness();
        focus_field(&mut loop_);
        let mut shell = hidden_shell();
        let mut ime = mapper();
        let seq = [
            ImeMessage::StartComposition,
            ImeMessage::Composition {
                gcs_flags: 0,
                comp: Some("ni".to_string()),
                attrs: None,
                cursor_pos: 2,
                delta_start: 0,
                result: None,
            },
            ImeMessage::Composition {
                gcs_flags: 0,
                comp: None,
                attrs: None,
                cursor_pos: 0,
                delta_start: 0,
                result: Some("你".to_string()),
            },
            ImeMessage::EndComposition,
        ];
        let mut damaged = false;
        for msg in &seq {
            damaged |= drive_ime(&mut loop_, &mut shell, &mut ime, msg).expect("drives");
        }
        assert!(damaged, "composition damages");
        assert_eq!(value.get(), SharedString::from("你"));
        // The candidate anchor tracked the session (run-relative
        // rect at the trailing caret of one CJK char).
        let anchor = loop_.ime_anchor().expect("anchor after commit");
        assert_eq!(anchor[0], 8.75, "one FakeText advance, got {anchor:?}");
        let anchored = shell.take_anchored_rects();
        assert!(!anchored.is_empty(), "shell anchored the candidate");
        assert_eq!(
            anchored.last().unwrap()[0],
            anchor[0],
            "shell saw the live rect, got {anchored:?}"
        );
        session_of(&loop_).undo();
        assert_eq!(value.get(), SharedString::from(""));
    }

    #[test]
    fn pump_moves_queue_to_cmds_and_fires_the_ime_callback() {
        use oppa::shell::PlatformShell;
        let mut shell = hidden_shell();
        let seen: Rc<std::cell::RefCell<Vec<ImeMessage>>> =
            Rc::new(std::cell::RefCell::new(Vec::new()));
        {
            let seen = seen.clone();
            shell.set_ime_callback(Rc::new(move |msg| seen.borrow_mut().push(msg.clone())));
        }
        shell
            .shared
            .borrow_mut()
            .queue
            .push_back(ShellEvent::Char { ch: 'a' });
        shell
            .shared
            .borrow_mut()
            .queue
            .push_back(ShellEvent::Ime(ImeMessage::StartComposition));
        let _ = shell.pump_events();
        assert_eq!(
            *seen.borrow(),
            vec![ImeMessage::StartComposition],
            "callback fires for IME messages"
        );
        assert_eq!(
            shell.take_cmds(),
            vec![oppa_shell_win::Cmd::Char { ch: 'a' }],
            "chars pair to commands after the pump (the missing-pump fix)"
        );
    }
}

/// Round 2.4 (decision 259): DPR re-basing keeps CSS stable,
/// doubles device geometry, and re-shapes text.
#[test]
fn set_device_pixel_ratio_rescales_device_keep_css() {
    let (mut loop_, _) = field_harness();
    // field_harness mounts 200x150 at dpr 1: field-box 200x32.
    let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
        .into_iter()
        .next()
        .expect("field box");
    let before = loop_.host().committed_box(field).expect("field box");
    assert_eq!((before.w, before.h), (200.0, 32.0));
    assert_eq!(loop_.dpr(), 1.0);
    loop_.set_device_pixel_ratio(2.0).expect("re-bases");
    assert_eq!(loop_.dpr(), 2.0);
    // CSS stable (200x150), device doubled: viewport() reads
    // device px, so it doubles too.
    assert_eq!(loop_.viewport(), (400, 300));
    let after = loop_.host().committed_box(field).expect("field box");
    assert_eq!((after.w, after.h), (400.0, 64.0), "device geometry doubles");
    assert_eq!(
        loop_.rgba8().expect("pixels").len(),
        400 * 300 * 4,
        "surface refits to device px"
    );
    // Text re-shaped at the new scale (measure keys carry the
    // DPR — a re-shape ran, not a stale reuse).
    assert!(
        loop_.host().layout_stats().nodes_shaped > 0,
        "text re-shapes at the new DPR"
    );
    // Same-value re-set is a no-op.
    loop_.set_device_pixel_ratio(2.0).expect("no-op");
    // Back down restores exactly.
    loop_.set_device_pixel_ratio(1.0).expect("re-bases");
    let back = loop_.host().committed_box(field).expect("field box");
    assert_eq!((back.w, back.h), (200.0, 32.0));
    assert_eq!(loop_.rgba8().expect("pixels").len(), 200 * 150 * 4);
}

#[test]
fn resize_at_non_unity_dpr_divides_to_css() {
    let (mut loop_, _) = field_harness();
    loop_.set_device_pixel_ratio(2.0).expect("re-bases");
    loop_.resize(400, 300).expect("resize works");
    // 400x300 device px at dpr 2 = 200x150 CSS: the field keeps
    // its full-width row (block child of the root).
    let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
        .into_iter()
        .next()
        .expect("field box");
    let box_ = loop_.host().committed_box(field).expect("field box");
    assert_eq!((box_.w, box_.h), (400.0, 64.0));
    assert_eq!(loop_.viewport(), (400, 300));
}

#[test]
#[should_panic(expected = "refused")]
fn zero_dpr_refuses_loudly() {
    let (mut loop_, _) = field_harness();
    let _ = loop_.set_device_pixel_ratio(0.0);
}

#[test]
#[should_panic(expected = "refused")]
fn nan_dpr_refuses_loudly() {
    let (mut loop_, _) = field_harness();
    let _ = loop_.set_device_pixel_ratio(f32::NAN);
}

/// Decision 389: `OPPA_RENDERER` parsing is pure and
/// case-insensitive; anything unrecognized (including the retired
/// `vello*` and `custom*` spellings) resolves to auto (GPU-first
/// with loud CPU fallback — the wrapper logs). Explicit backend
/// pins (`gpu-vulkan` / `gpu-dx12` / `gpu-gl`, hyphen or
/// underscore) parse to their variants; bare `vulkan` stays auto
/// (never an implicit pin).
#[test]
fn renderer_override_parses_cpu_gpu_auto() {
    assert_eq!(parse_renderer_override(None), None);
    assert_eq!(
        parse_renderer_override(Some("cpu")),
        Some(RendererKind::Cpu)
    );
    assert_eq!(
        parse_renderer_override(Some("CPU")),
        Some(RendererKind::Cpu)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu")),
        Some(RendererKind::Gpu)
    );
    assert_eq!(
        parse_renderer_override(Some("  Gpu  ")),
        Some(RendererKind::Gpu)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu-auto")),
        Some(RendererKind::GpuAuto)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu_auto")),
        Some(RendererKind::GpuAuto)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu-vulkan")),
        Some(RendererKind::GpuVulkan)
    );
    assert_eq!(
        parse_renderer_override(Some("GPU-VULKAN")),
        Some(RendererKind::GpuVulkan)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu-dx12")),
        Some(RendererKind::GpuDx12)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu_dx12")),
        Some(RendererKind::GpuDx12)
    );
    assert_eq!(
        parse_renderer_override(Some("  Gpu-GL  ")),
        Some(RendererKind::GpuGl)
    );
    assert_eq!(parse_renderer_override(Some("")), None);
    assert_eq!(parse_renderer_override(Some("vulkan")), None);
    // Retired spellings never parse (decisions 388–389 — no silent
    // substitution).
    assert_eq!(parse_renderer_override(Some("vello")), None);
    assert_eq!(parse_renderer_override(Some("vello-vulkan")), None);
    assert_eq!(parse_renderer_override(Some("vello-dx12")), None);
    assert_eq!(parse_renderer_override(Some("custom_gpu")), None);
    assert_eq!(parse_renderer_override(Some("custom-auto")), None);
    assert_eq!(parse_renderer_override(Some("custom-dx12")), None);
}

#[test]
fn presenter_badge_name_follows_the_active_renderer() {
    // The overlay badge must name the pixels. The name derives from
    // the active renderer instead of a hardcoded per-outcome string.
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    assert_eq!(loop_.renderer_kind(), RendererKind::Cpu);
    assert_eq!(loop_.presenter_badge_name(), "CPU");
    loop_.enable_gpu(wgpu::PresentMode::Fifo);
    assert_eq!(loop_.renderer_kind(), RendererKind::GpuAuto);
    assert_eq!(loop_.presenter_badge_name(), "Gpu");
    loop_.disable_gpu();
    assert_eq!(loop_.presenter_badge_name(), "CPU");
    // The derived name is what the badge renders.
    loop_.enable_gpu(wgpu::PresentMode::Fifo);
    loop_.host().set_debug_overlay(DebugOverlayConfig::all());
    loop_
        .host()
        .set_presenter_name(loop_.presenter_badge_name());
    let badge = loop_
        .host()
        .debug_overlay_badge()
        .expect("badge renders when enabled");
    assert!(
        badge.contains("[Gpu]"),
        "badge names the active renderer, got {badge:?}"
    );
}

#[test]
fn gpu_backend_pins_share_the_gpu_twin_and_badge() {
    // Decision 389: every GPU variant paints the same twin with
    // the bare `Gpu` badge until bring-up records a winner.
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    for kind in [
        RendererKind::Gpu,
        RendererKind::GpuAuto,
        RendererKind::GpuVulkan,
        RendererKind::GpuDx12,
        RendererKind::GpuGl,
    ] {
        loop_.scene_mut().active = kind;
        assert!(loop_.is_gpu(), "is_gpu for {kind:?}");
        assert_eq!(loop_.presenter_badge_name(), "Gpu");
    }
    loop_.scene_mut().active = RendererKind::GpuDx12;
    assert_eq!(loop_.renderer_name(), "gpu-dx12");
    loop_.scene_mut().active = RendererKind::GpuVulkan;
    assert_eq!(loop_.renderer_name(), "gpu-vulkan");
    loop_.scene_mut().active = RendererKind::GpuGl;
    assert_eq!(loop_.renderer_name(), "gpu-gl");
    loop_.scene_mut().active = RendererKind::Cpu;
    assert!(!loop_.is_gpu());
}

#[test]
fn gpu_badge_names_the_recorded_wgpu_backend() {
    // Decision 389: the badge names the actual API
    // (`[Gpu/DX12]`), never a bare `Gpu` once bring-up recorded
    // a winner; headless loops with no recording stay `Gpu`.
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    assert_eq!(loop_.gpu_api(), None);
    loop_.scene_mut().active = RendererKind::GpuAuto;
    assert_eq!(loop_.presenter_badge_name(), "Gpu");
    loop_.set_gpu_api(GpuApi::Dx12);
    assert_eq!(loop_.gpu_api(), Some(GpuApi::Dx12));
    assert_eq!(loop_.presenter_badge_name(), "Gpu/DX12");
    loop_.set_gpu_api(GpuApi::Vulkan);
    assert_eq!(loop_.presenter_badge_name(), "Gpu/Vulkan");
    loop_.set_gpu_api(GpuApi::Gl);
    assert_eq!(loop_.presenter_badge_name(), "Gpu/GL");
    assert_eq!(GpuApi::from_row_name("dx12"), Some(GpuApi::Dx12));
    assert_eq!(GpuApi::from_row_name("vulkan"), Some(GpuApi::Vulkan));
    assert_eq!(GpuApi::from_row_name("gl"), Some(GpuApi::Gl));
    assert_eq!(GpuApi::from_row_name("metal"), None);
    loop_.disable_gpu();
    assert_eq!(loop_.gpu_api(), None);
    assert_eq!(loop_.presenter_badge_name(), "CPU");
}

#[test]
fn gpu_family_parses_pins_and_badges_the_recorded_api() {
    // Decision 389: `gpu` / `gpu-auto` request the automatic
    // rasterizer; the backend pins select one wgpu row; every
    // variant paints the GPU twin and badges the recorded API.
    assert_eq!(
        parse_renderer_override(Some("gpu")),
        Some(RendererKind::Gpu)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu-auto")),
        Some(RendererKind::GpuAuto)
    );
    assert_eq!(
        parse_renderer_override(Some("gpu_vulkan")),
        Some(RendererKind::GpuVulkan)
    );
    assert_eq!(
        parse_renderer_override(Some("GPU-DX12")),
        Some(RendererKind::GpuDx12)
    );
    assert_eq!(
        parse_renderer_override(Some("  gpu-gl  ")),
        Some(RendererKind::GpuGl)
    );
    for kind in [
        RendererKind::Gpu,
        RendererKind::GpuAuto,
        RendererKind::GpuVulkan,
        RendererKind::GpuDx12,
        RendererKind::GpuGl,
    ] {
        assert!(kind.is_gpu(), "is_gpu for {kind:?}");
    }
    assert!(!RendererKind::Cpu.is_gpu());
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.scene_mut().active = RendererKind::GpuDx12;
    assert_eq!(loop_.renderer_name(), "gpu-dx12");
    assert!(loop_.is_gpu());
    assert_eq!(loop_.presenter_badge_name(), "Gpu");
    loop_.set_gpu_api(GpuApi::Dx12);
    assert_eq!(loop_.presenter_badge_name(), "Gpu/DX12");
    loop_.set_gpu_api(GpuApi::Gl);
    assert_eq!(loop_.presenter_badge_name(), "Gpu/GL");
}

#[test]
fn continuous_repaint_override_parses_flag_values() {
    assert!(!parse_continuous_repaint_override(None));
    assert!(!parse_continuous_repaint_override(Some("")));
    assert!(!parse_continuous_repaint_override(Some("0")));
    assert!(!parse_continuous_repaint_override(Some("no")));
    assert!(!parse_continuous_repaint_override(Some("yes")));
    assert!(parse_continuous_repaint_override(Some("1")));
    assert!(parse_continuous_repaint_override(Some("true")));
    assert!(parse_continuous_repaint_override(Some("TRUE")));
    assert!(parse_continuous_repaint_override(Some("  True  ")));
}

#[test]
fn window_options_continuous_repaint_defaults_off_and_builder_sets() {
    let opts = WindowOptions::new("Bench", 400, 300);
    assert!(!opts.continuous_repaint, "event-driven by default");
    let opts = opts.with_continuous_repaint(true);
    assert!(opts.continuous_repaint, "builder arms benchmark mode");
    let opts = opts.with_continuous_repaint(false);
    assert!(!opts.continuous_repaint, "builder disarms again");
}

#[test]
fn desktop_loop_continuous_repaint_horizon_and_rearm() {
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Plain", PlainProps, plain_app);
    assert!(!loop_.continuous_repaint(), "event-driven by default");
    assert_eq!(
        loop_.pump_wait_timeout_ms(None),
        None,
        "idle loop blocks until input"
    );
    assert_eq!(
        loop_.pump_wait_timeout_ms(Some(8)),
        Some(8),
        "busy horizon passes through"
    );
    // Rearm is a quiet no-op unless armed (the event-driven path is
    // exactly as before).
    assert!(!loop_.host().has_demand(), "settled mount holds no demand");
    loop_.rearm_continuous_frame();
    assert!(
        !loop_.host().has_demand(),
        "disarmed rearm requests nothing"
    );
    // Armed: 0ms horizon preempts every idle/busy wait, and every
    // rearm marks demand the settle path turns back into frames.
    loop_.set_continuous_repaint(true);
    assert!(loop_.continuous_repaint(), "setter arms benchmark mode");
    assert_eq!(
        loop_.pump_wait_timeout_ms(None),
        Some(0),
        "armed horizon polls immediately"
    );
    assert_eq!(
        loop_.pump_wait_timeout_ms(Some(8)),
        Some(0),
        "armed horizon preempts busy waits"
    );
    loop_.rearm_continuous_frame();
    assert!(loop_.host().has_demand(), "rearm marks the scene dirty");
    assert!(
        loop_.host().run_until_idle() > 0,
        "demand settles into frames the pump repaints"
    );
    // Disarm restores the event-driven horizon.
    loop_.set_continuous_repaint(false);
    assert_eq!(
        loop_.pump_wait_timeout_ms(None),
        None,
        "disarmed loop blocks again"
    );
}

#[test]
fn continuous_repaint_runs_with_debug_overlay_enabled() {
    // Benchmark mode is orthogonal to the debug overlay (decision
    // 385 wrap): horizon, demand, and repaint behave identically
    // with the badge mounted.
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.host().set_debug_overlay(DebugOverlayConfig::all());
    loop_.host().set_presenter_name("Test");
    loop_.mount("Plain", PlainProps, plain_app);
    loop_.set_continuous_repaint(true);
    assert_eq!(
        loop_.pump_wait_timeout_ms(None),
        Some(0),
        "armed horizon polls immediately under the overlay"
    );
    loop_.repaint().expect("paints with overlay mounted");
    loop_.rearm_continuous_frame();
    assert!(loop_.host().has_demand(), "rearm marks demand");
    assert!(
        loop_.host().run_until_idle() > 0,
        "demand settles into frames"
    );
    loop_.repaint().expect("repaints the next frame");
}

#[test]
fn note_frame_presented_ticks_meter_and_refreshes_badge_throttled() {
    // The benchmark-idle artifact end to end: presents drive the FPS
    // clock (never reconciles), and the badge text refreshes through
    // a throttled root re-run — no 0 FPS after idle gaps.
    let clock = Rc::new(oppa::MockClock::new());
    let loop_ = DesktopLoop::with_clock(200, 150, Box::new(FakeText), "Test", clock.clone())
        .expect("headless loop builds");
    loop_.host().set_debug_overlay(DebugOverlayConfig::all());
    loop_.host().set_presenter_name("Test");
    loop_.mount("Plain", PlainProps, plain_app);
    loop_.host().run_until_idle();
    // Disabled guard first: a loop without the overlay notes nothing.
    loop_
        .host()
        .set_debug_overlay(DebugOverlayConfig::disabled());
    loop_.note_frame_presented();
    assert_eq!(
        loop_.host().run_until_idle(),
        0,
        "disabled overlay schedules no refresh"
    );
    loop_.host().set_debug_overlay(DebugOverlayConfig::all());
    // Rapid presents tick the clock but stay inside the throttle.
    loop_.note_frame_presented();
    clock.advance(0.1);
    loop_.note_frame_presented();
    assert_eq!(
        loop_.host().run_until_idle(),
        0,
        "no root re-run inside the throttle window"
    );
    // Past the throttle the next present schedules the refresh, and
    // the rebuilt badge names the present cadence + presenter.
    clock.advance(0.3);
    loop_.note_frame_presented();
    assert!(
        loop_.host().run_until_idle() > 0,
        "throttled present schedules the badge refresh"
    );
    let badge = loop_
        .host()
        .debug_overlay_badge()
        .expect("badge renders when enabled");
    assert!(badge.contains("FPS"), "badge carries fps, got {badge:?}");
    assert!(
        badge.contains("[Test]"),
        "badge rebuilt after presenter set, got {badge:?}"
    );
    // The regression itself: a 13s idle gap with no presents, then an
    // interaction reconcile, must not move the FPS needle.
    let fps_before = badge
        .split(" FPS")
        .next()
        .expect("fps prefix")
        .rsplit(' ')
        .next()
        .expect("fps number")
        .parse::<f64>()
        .expect("fps parses");
    clock.advance(13.0);
    loop_.host().mark_all_component_effects_dirty();
    loop_.host().run_until_idle();
    let badge_after = loop_
        .host()
        .debug_overlay_badge()
        .expect("badge still renders");
    assert!(
        badge_after.starts_with(&format!("{fps_before:.0} FPS")),
        "idle-gap reconcile leaves FPS alone, got {badge_after:?}"
    );
}

#[test]
fn desktop_with_hook_configures_live_loop() {
    // The `run_desktop_with` hook contract, headlessly: the same
    // `FnOnce(&mut DesktopLoop)` closure shape the runners invoke
    // after mount installs a close veto that `close_requested`
    // consults (the cookbook's dirty-gated veto).
    let mut loop_ = DesktopLoop::new(400, 300, Box::new(FakeText), "Hook").expect("headless loop");
    let dirty = loop_.host().runtime().signal(true);
    let gate = dirty.clone();
    let configure = |loop_: &mut DesktopLoop| {
        loop_.set_close_handler(Rc::new(move || !gate.get()));
    };
    configure(&mut loop_);
    assert!(!loop_.close_requested(), "dirty vetoes close");
    dirty.set(false);
    assert!(loop_.close_requested(), "clean closes");
}

#[test]
fn poll_hook_runs_restores_and_removes() {
    // Round 26.2 (decision 342): installed hooks run once per
    // `run_poll_hook` call against the live loop, survive the
    // call (take-call-restore), and `None` removes them.
    let mut loop_ = DesktopLoop::new(400, 300, Box::new(FakeText), "Hook").expect("headless loop");
    let fires = Rc::new(std::cell::Cell::new(0usize));
    let count = fires.clone();
    loop_.set_poll_hook(Some(Box::new(move |loop_: &mut DesktopLoop| {
        count.set(count.get() + 1);
        // The hook observes the live loop (viewport here;
        // Task Studio reads hooks signals the same way).
        let _ = loop_.viewport();
    })));
    loop_.run_poll_hook();
    loop_.run_poll_hook();
    assert_eq!(fires.get(), 2, "hook runs every invocation");
    loop_.set_poll_hook(None);
    loop_.run_poll_hook();
    assert_eq!(fires.get(), 2, "removal stops calls");
}

#[test]
fn close_flag_drains_through_veto_consult() {
    // The runner contract, headlessly: a drained component
    // request consults the veto -- refused while dirty, exit
    // once clean (what both pumps do per iteration).
    let mut loop_ = DesktopLoop::new(400, 300, Box::new(FakeText), "Hook").expect("headless loop");
    let dirty = loop_.host().runtime().signal(true);
    let gate = dirty.clone();
    loop_.set_close_handler(Rc::new(move || !gate.get()));
    loop_.host().request_close();
    assert!(loop_.host().take_close_request(), "flag drains");
    assert!(!loop_.close_requested(), "dirty veto holds");
    dirty.set(false);
    loop_.host().request_close();
    assert!(loop_.host().take_close_request(), "flag drains again");
    assert!(loop_.close_requested(), "clean exits");
}

/// Headless loops start CPU-only with both scene surfaces live
/// (GPU twin untouched until a runner enables GPU — a faceless
/// shaper must never trip the GPU loud-no-face rule on the CPU
/// path).
#[test]
fn loop_starts_cpu_only_with_both_surfaces() {
    let (mut loop_, _) = harness();
    assert_eq!(loop_.renderer_kind(), RendererKind::Cpu);
    assert!(!loop_.is_gpu());
    assert_eq!(loop_.renderer_name(), "cpu");
    assert_eq!(loop_.gpu_present_mode(), None);
    let damage = loop_.repaint().expect("repaints");
    assert!(damage > 0);
    assert!(loop_.cpu_retained_op_count().unwrap_or(0) > 0);
    assert_eq!(
        loop_.gpu_retained_op_count(),
        Some(0),
        "CPU-only repaints leave the GPU twin untouched"
    );
    assert_eq!(loop_.rgba8().expect("pixels").len(), 200 * 150 * 4);
}

/// Text-free scene for GPU paint tests (a Text run without an
/// injected face would refuse loudly on GPU by contract, while
/// the CPU path still draws bars — so the dual test stays
/// faceless by having no text at all).
#[derive(Clone)]
struct PlainProps;
impl Props for PlainProps {}

fn plain_app(_ctx: &Ctx, _props: &PlainProps) -> VNode {
    Div("root")
        .style(Style::new().size(200, 150).bg(Color(0xFF_FF_FF)))
        .child(
            Div("plate")
                .style(Style::new().size(96, 32).bg(Color(0x22_66_CC)))
                .build(),
        )
}

/// Enabling GPU paints the same frame plan into both backends
/// (CPU pixmap stays the fallback source); disabling returns to
/// CPU-only without losing the viewport. A bare `enable_gpu` with
/// no override resolves to the GPU automatic default.
#[test]
fn enable_gpu_paints_both_backends_then_disables() {
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Plain", PlainProps, plain_app);
    loop_.enable_gpu(wgpu::PresentMode::Fifo);
    assert!(loop_.is_gpu());
    assert_eq!(loop_.renderer_kind(), RendererKind::GpuAuto);
    assert_eq!(loop_.renderer_name(), "gpu-auto");
    assert_eq!(loop_.gpu_present_mode(), Some(wgpu::PresentMode::Fifo));
    let damage = loop_.repaint().expect("gpu repaint works");
    assert!(damage > 0);
    assert!(loop_.cpu_retained_op_count().unwrap_or(0) > 0);
    assert!(
        loop_.gpu_retained_op_count().unwrap_or(0) > 0,
        "GPU twin encodes the same plan"
    );
    assert_eq!(loop_.rgba8().expect("pixels").len(), 200 * 150 * 4);
    loop_.disable_gpu();
    assert!(!loop_.is_gpu());
    assert_eq!(loop_.gpu_present_mode(), None);
    loop_.repaint().expect("cpu repaint after disable works");
}

#[test]
fn enable_gpu_paints_and_retains() {
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Plain", PlainProps, plain_app);
    loop_.enable_gpu(wgpu::PresentMode::Fifo);
    assert!(loop_.is_gpu());
    assert_eq!(loop_.renderer_name(), "gpu-auto");
    assert_eq!(loop_.renderer_kind(), RendererKind::GpuAuto);
    let damage = loop_.repaint().expect("gpu repaint works");
    assert!(damage > 0);
    assert!(loop_.cpu_retained_op_count().unwrap_or(0) > 0);
    assert!(
        loop_.gpu_retained_op_count().unwrap_or(0) > 0,
        "GPU backend encodes and retains ops"
    );
    loop_.disable_gpu();
    assert!(!loop_.is_gpu());
    assert_eq!(loop_.renderer_name(), "cpu");
}

/// Refit keeps both surfaces in lockstep (a later `enable_gpu`
/// never presents a stale size).
#[test]
fn resize_refits_both_surfaces_in_lockstep() {
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Plain", PlainProps, plain_app);
    loop_.enable_gpu(wgpu::PresentMode::Fifo);
    loop_.repaint().expect("paints");
    loop_.resize(400, 300).expect("resize works");
    assert_eq!(loop_.viewport(), (400, 300));
    loop_.repaint().expect("paints at the new size");
    assert_eq!(loop_.rgba8().expect("pixels").len(), 400 * 300 * 4);
    assert!(loop_.gpu_retained_op_count().unwrap_or(0) > 0);
}

/// Round 2.4 Windows half (decision 259): a real
/// `WM_DPICHANGED` through the window proc snapshots the
/// suggested rect + DPI into a pipeline command, and driving it
/// re-bases + resizes + repaints. Headless (hidden window + sent
/// message — the OS crossing itself is the one input no harness
/// can script).
#[cfg(windows)]
mod win_dpi {
    use super::*;
    use crate::windows::{drive_cmd, WinImeMapper};
    use oppa_shell_win::{Cmd, ShellConfig, Win32Shell};

    fn hidden_shell() -> Win32Shell {
        Win32Shell::new(ShellConfig {
            title: "dpi-test".to_string(),
            width: 200,
            height: 150,
            record_messages: false,
            suppress_os_composition_window: true,
            visible: false,
        })
        .expect("hidden test window builds")
    }

    #[test]
    fn wndproc_dpi_changed_snapshots_rect_and_dpi() {
        use ::windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
        use ::windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_DPICHANGED};
        use oppa::shell::PlatformShell;
        let mut shell = hidden_shell();
        let rect = RECT {
            left: 10,
            top: 20,
            right: 810,
            bottom: 620,
        };
        let dpi = 192u32;
        unsafe {
            SendMessageW(
                shell.hwnd(),
                WM_DPICHANGED,
                Some(WPARAM(((dpi << 16) | dpi) as usize)),
                Some(LPARAM(&rect as *const RECT as isize)),
            );
        }
        let _ = shell.pump_events();
        assert_eq!(
            shell.take_cmds(),
            vec![Cmd::DpiChanged {
                x: 10,
                y: 20,
                w: 800,
                h: 600,
                dpi: 192,
            }],
            "suggested rect + x-axis DPI snapshot at message time"
        );
    }

    #[test]
    fn drive_cmd_dpi_changed_rebases_and_repaints() {
        let (mut loop_, _) = field_harness();
        let mut shell = hidden_shell();
        let mut ime = WinImeMapper::new();
        let out = drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::DpiChanged {
                x: 0,
                y: 0,
                w: 400,
                h: 300,
                dpi: 192,
            },
        )
        .expect("drives");
        assert!(out.damaged, "repaint follows the re-base");
        assert_eq!(loop_.dpr(), 2.0, "192 dpi re-bases to 2.0");
        assert_eq!(loop_.viewport(), (400, 300));
        let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
            .into_iter()
            .next()
            .expect("field box");
        let box_ = loop_.host().committed_box(field).expect("field box");
        assert_eq!((box_.w, box_.h), (400.0, 64.0), "crisp at 2x: {box_:?}");
        assert_eq!(loop_.rgba8().expect("pixels").len(), 400 * 300 * 4);
    }
}
