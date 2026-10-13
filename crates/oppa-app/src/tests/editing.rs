use super::*;

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
