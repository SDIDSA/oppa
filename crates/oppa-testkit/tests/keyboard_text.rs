//! Round 35 (decision 351): `Harness::key` / `key_with` /
//! `type_text` / `press_labeled` — the keyboard/text half every app
//! test needs, promoted from hand-rolled `inject_input` rigs
//! (`focus_ring_trap.rs` `tab()`/`shift_tab()`, the controls-suite
//! `press_labeled_button`, the `focused_field_session` + `insert`
//! pair). All flows compose public API only.

use oppa::{
    input::keys, Ctx, Div, Modifiers, Props, Semantics, SharedString, Signal, Style, VNode,
};
use oppa_controls::{Button, ButtonProps, TextInput, TextInputProps};
use oppa_testkit::Harness;

fn focused_debug(app: &Harness) -> String {
    let host = app.host();
    let focus = host.focused_node().expect("focus set");
    host.with_retained_mut(|rec, _| rec.get(focus).map(|n| n.debug.clone()).unwrap_or_default())
}

#[derive(Clone)]
struct PadProps {
    left: Signal<SharedString>,
    right: Signal<SharedString>,
}

impl Props for PadProps {}

fn pad_scene(_ctx: &Ctx, props: &PadProps) -> VNode {
    let (left, right) = (props.left.clone(), props.right.clone());
    Div("pad")
        .style(Style::new().size(200, 64))
        .semantics(Semantics::switch().label("pad"))
        .on_press(|| {})
        .on_key_left(move || left.set(SharedString::from("L")))
        .on_key_right(move || right.set(SharedString::from("R")))
        .build()
}

/// `key` dispatches to the focused node's directional handlers.
#[test]
fn key_press_dispatches_to_focused_handler() {
    let app = Harness::new();
    let left = app.host().runtime().signal(SharedString::from("-"));
    let right = app.host().runtime().signal(SharedString::from("-"));
    app.mount(
        "Pad",
        PadProps {
            left: left.clone(),
            right: right.clone(),
        },
        pad_scene,
    );
    app.tap("pad");
    assert_eq!(focused_debug(&app), "pad", "tap focuses the pad");
    app.key(keys::LEFT);
    assert_eq!(&*left.get(), "L");
    assert_eq!(&*right.get(), "-", "right untouched");
    app.key(keys::RIGHT);
    assert_eq!(&*right.get(), "R");
}

#[derive(Clone)]
struct TwoButtons;

impl Props for TwoButtons {}

fn two_buttons(ctx: &Ctx, _props: &TwoButtons) -> VNode {
    Div("screen").children([
        ctx.child(
            "oppa::BtnA",
            1,
            &ButtonProps {
                debug: SharedString::from("btn-a"),
                ..ButtonProps::new("A", || {})
            },
            Button,
        ),
        ctx.child(
            "oppa::BtnB",
            2,
            &ButtonProps {
                debug: SharedString::from("btn-b"),
                ..ButtonProps::new("B", || {})
            },
            Button,
        ),
    ])
}

/// `key` walks Tab order forward; `key_with` + Shift walks back.
#[test]
fn key_with_shift_walks_tab_back() {
    let app = Harness::new();
    app.mount("S", TwoButtons, two_buttons);
    app.key(keys::TAB);
    assert_eq!(focused_debug(&app), "btn-a", "Tab focuses first");
    app.key(keys::TAB);
    assert_eq!(focused_debug(&app), "btn-b", "Tab advances");
    app.key_with(
        keys::TAB,
        Modifiers {
            shift: true,
            ..Modifiers::NONE
        },
    );
    assert_eq!(focused_debug(&app), "btn-a", "Shift+Tab walks back");
}

#[derive(Clone)]
struct FieldProps {
    value: Signal<SharedString>,
}

impl Props for FieldProps {}

fn field_screen(ctx: &Ctx, props: &FieldProps) -> VNode {
    let field = TextInputProps {
        debug: SharedString::from("field"),
        ..TextInputProps::new("Name", props.value.clone())
    };
    Div("screen").child(ctx.child("oppa::Field", 1, &field, TextInput))
}

fn dejavu(app: &Harness) {
    let (svc, _) = oppa_text_rustybuzz::RustybuzzService::from_bytes_with_chain(
        &[("DejaVuSans.ttf", oppa_fonts::DEJAVU_SANS)],
        &[],
    )
    .expect("bundled font parses");
    app.host().set_text_service(Box::new(svc));
    app.host().set_layout_config(oppa::LayoutTextConfig {
        family: oppa_fonts::DEJAVU_SANS_FAMILY.to_string(),
        ..Default::default()
    });
}

/// `type_text` inserts into the focused session and reports the
/// count; control chars and unfocused misses stay quiet zeros.
#[test]
fn type_text_inserts_into_focused_field() {
    let app = Harness::new();
    dejavu(&app);
    let value = app.host().runtime().signal(SharedString::from(""));
    app.mount(
        "S",
        FieldProps {
            value: value.clone(),
        },
        field_screen,
    );
    assert_eq!(app.type_text("Ada"), 0, "nothing focused: quiet miss");
    assert_eq!(&*value.get(), "", "miss writes nothing");
    app.tap("field");
    assert!(
        app.host().focused_field_session().is_some(),
        "tap arms the session"
    );
    assert_eq!(app.type_text("Ada"), 3, "three printable chars");
    assert_eq!(&*value.get(), "Ada", "session writes through");
    assert_eq!(app.type_text("!"), 1);
    assert_eq!(&*value.get(), "Ada!", "caret appends");
    assert_eq!(app.type_text("\n"), 0, "control chars skip");
    assert_eq!(&*value.get(), "Ada!", "skips write nothing");
}

#[derive(Clone)]
struct GoProps {
    pressed: Signal<bool>,
}

impl Props for GoProps {}

fn go_screen(ctx: &Ctx, props: &GoProps) -> VNode {
    let hit = props.pressed.clone();
    let go = ButtonProps {
        debug: SharedString::from("go-btn"),
        ..ButtonProps::new("Go", move || hit.set(true))
    };
    Div("screen").child(ctx.child("oppa::Go", 1, &go, Button))
}

/// `press_labeled` finds the button by its label, not its debug.
#[test]
fn press_labeled_presses_by_label() {
    let app = Harness::new();
    let pressed = app.host().runtime().signal(false);
    app.mount(
        "S",
        GoProps {
            pressed: pressed.clone(),
        },
        go_screen,
    );
    app.press_labeled("Go");
    assert!(pressed.get(), "label-addressed press fires");
}

#[test]
#[should_panic(expected = "in the tab order")]
fn press_labeled_missing_label_is_loud() {
    let app = Harness::new();
    let pressed = app.host().runtime().signal(false);
    app.mount("S", GoProps { pressed }, go_screen);
    app.press_labeled("Typo");
}
