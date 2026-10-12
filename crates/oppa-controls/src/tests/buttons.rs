use super::*;
use oppa::input::{keys, InputEvent, KeyState};
use oppa::{find_retained_by_debug, ComponentHost, Ctx, Props, SharedString, Signal, VNode};

#[derive(Clone)]
struct ButtonCase {
    props: ButtonProps,
}
impl Props for ButtonCase {}

fn button_render(ctx: &Ctx, p: &ButtonCase) -> VNode {
    ctx.child("oppa::Button", 7, &p.props, Button)
}

#[test]
fn button_press_and_keyboard_activate() {
    let host = ComponentHost::new();
    let fired = host.runtime().signal(false);
    let set = fired.clone();
    let props = ButtonProps::new("OK", move || set.set(true));
    let case = ButtonCase { props };
    host.mount("Btn", case, button_render);
    host.run_until_idle();
    let id = node_by_debug(&host, "button");
    // Semantics: button role + label, enabled.
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::Button);
    assert_eq!(sem.label.as_deref(), Some("OK"));
    assert!(!sem.disabled);
    // Pointer press activates.
    press_node(&host, id);
    assert!(fired.get(), "press activates");
    // Keyboard: focus + Enter activates (M5 router).
    fired.set(false);
    host.inject_input(InputEvent::Focus { node: Some(id) });
    host.run_until_idle();
    host.inject_input(InputEvent::key(keys::ENTER, KeyState::Pressed));
    host.run_until_idle();
    assert!(fired.get(), "Enter on focused button activates");
}

#[test]
fn button_disabled_is_handlerless_and_untabbable() {
    let host = ComponentHost::new();
    let fired = host.runtime().signal(false);
    let set = fired.clone();
    let props = ButtonProps::new("OK", move || set.set(true)).disabled();
    host.mount("BtnDis", ButtonCase { props }, button_render);
    host.run_until_idle();
    let id = node_by_debug(&host, "button");
    assert!(
        host.retained_handlers(id).is_empty(),
        "disabled carries no handler — refusal is structural"
    );
    assert!(
        !host.tab_order().contains(&id),
        "disabled leaves the tab order (decision 96)"
    );
    press_node(&host, id);
    assert!(!fired.get(), "press on disabled never activates");
}

#[test]
fn checkbox_flips_and_reports_checked() {
    let host = ComponentHost::new();
    let checked = host.runtime().signal(false);
    let props = CheckboxProps {
        label: SharedString::from("T&C"),
        checked: checked.clone(),
        enabled: true,
        on_change: None,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
    };
    host.mount("Chk", CheckCase { props }, check_render);
    host.run_until_idle();
    let id = node_by_debug(&host, "checkbox");
    press_node(&host, id);
    assert!(checked.get(), "press checks");
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!((sem.role, sem.checked), (oppa::Role::Checkbox, Some(true)));
    press_node(&host, id);
    assert!(!checked.get(), "press unchecks");
}

/// Decision 291: the drawn box is always present, the vector
/// check only when checked, and the label shows beside the box
/// (the old `"[x] label"` text mark is gone). The check is one
/// `Path` leaf (no text leaf, no arm squares), so widths prove
/// the label alone (FakeText 8.75px/char — "T&C" = 26.25) while
/// the retained node proves the vector.
#[test]
fn checkbox_renders_drawn_box_check_iff_checked_and_label() {
    fn box_of(host: &ComponentHost) {
        let b = host
            .committed_box(node_by_debug(host, "checkbox-box"))
            .expect("box laid out");
        assert_eq!((b.w, b.h), (20.0, 20.0));
    }
    fn text_widths(host: &ComponentHost) -> Vec<f32> {
        let mut ws: Vec<f32> = find_retained_by_debug(host, "text")
            .into_iter()
            .filter_map(|id| host.committed_box(id))
            .filter(|b| !b.lines.is_empty())
            .map(|b| b.content_w)
            .collect();
        ws.sort_by(|a, b| a.partial_cmp(b).expect("finite widths"));
        ws
    }
    fn assert_widths(host: &ComponentHost, expect: &[f32], what: &str) {
        let got = text_widths(host);
        assert_eq!(got.len(), expect.len(), "{what}: leaf count, got {got:?}");
        for (g, e) in got.iter().zip(expect.iter()) {
            assert!(approx(*g, *e), "{what}: width {g} ≈ {e}, got {got:?}");
        }
    }
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let checked = host.runtime().signal(false);
    let props = CheckboxProps {
        label: SharedString::from("T&C"),
        checked: checked.clone(),
        enabled: true,
        on_change: None,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
    };
    host.mount("ChkV", CheckCase { props }, check_render);
    host.run_until_idle();
    box_of(&host);
    assert!(
        find_retained_by_debug(&host, "checkbox-check").is_empty(),
        "unchecked: no check unit"
    );
    assert_widths(&host, &[26.25], "label only, no mark");
    press_node(&host, node_by_debug(&host, "checkbox"));
    assert!(checked.get());
    box_of(&host);
    assert_widths(&host, &[26.25], "label only — the check is vector");
    // One Path leaf filling the box exactly (in-flow, same size —
    // committed box matches the drawn box, no arm squares anywhere).
    let check = node_by_debug(&host, "checkbox-check");
    let cb = host.committed_box(check).expect("check laid out");
    assert_eq!((cb.w, cb.h), (20.0, 20.0));
    let bb = host
        .committed_box(node_by_debug(&host, "checkbox-box"))
        .expect("box laid out");
    assert!(
        approx(cb.x, bb.x) && approx(cb.y, bb.y),
        "check fills the box exactly, check {cb:?} box {bb:?}"
    );
    assert!(
        find_retained_by_debug(&host, "checkbox-check-arm").is_empty(),
        "no stepped-square arms — the mark is one vector"
    );
}

#[test]
fn toggle_flips_like_the_m5_pattern() {
    let host = ComponentHost::new();
    let on = host.runtime().signal(false);
    let props = ToggleProps {
        label: SharedString::from("Wi-Fi"),
        on: on.clone(),
        enabled: true,
        on_change: None,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
    };
    host.mount("Tgl", ToggleCase { props }, toggle_render);
    host.run_until_idle();
    let id = node_by_debug(&host, "toggle");
    press_node(&host, id);
    assert!(on.get());
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!((sem.role, sem.checked), (oppa::Role::Switch, Some(true)));
}

/// Round 7.9: pill track + knob + visible label (the old
/// `"on"`/`"off"` text box is gone). The knob sits at x = 2 off
/// / 22 on (20px knob in a 44px track); the only text leaf is
/// the label ("Wi-Fi" = 43.75).
#[test]
fn toggle_renders_switch_knob_side_and_label() {
    fn knob_x(host: &ComponentHost) -> f32 {
        let track = host
            .committed_box(node_by_debug(host, "toggle-track"))
            .expect("track laid out");
        assert_eq!((track.w, track.h), (44.0, 24.0));
        let knob = host
            .committed_box(node_by_debug(host, "toggle-knob"))
            .expect("knob laid out");
        assert_eq!((knob.w, knob.h), (20.0, 20.0));
        knob.x - track.x
    }
    fn label_only(host: &ComponentHost) {
        let ws: Vec<f32> = find_retained_by_debug(host, "text")
            .into_iter()
            .filter_map(|id| host.committed_box(id))
            .filter(|b| !b.lines.is_empty())
            .map(|b| b.content_w)
            .collect();
        assert_eq!(ws.len(), 1, "one text leaf (the label), got {ws:?}");
        assert!(approx(ws[0], 43.75), "label widths, got {ws:?}");
    }
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let on = host.runtime().signal(false);
    let props = ToggleProps {
        label: SharedString::from("Wi-Fi"),
        on: on.clone(),
        enabled: true,
        on_change: None,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
    };
    host.mount("TglV", ToggleCase { props }, toggle_render);
    host.run_until_idle();
    assert_eq!(knob_x(&host), 2.0);
    label_only(&host);
    press_node(&host, node_by_debug(&host, "toggle"));
    assert!(on.get());
    assert_eq!(knob_x(&host), 22.0);
    label_only(&host);
}

#[test]
fn slider_steps_snap_and_clamp_with_value_text() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(50.0f32);
    let props = SliderProps {
        label: SharedString::from("Volume"),
        value: value.clone(),
        min: 0.0,
        max: 100.0,
        step: 10.0,
        enabled: true,
        on_change: None,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
    };
    host.mount("Sld", SliderCase { props }, slider_render);
    host.run_until_idle();
    let track = node_by_debug(&host, "slider");
    let sem = host.retained_semantics(track).expect("semantics");
    assert_eq!(sem.role, oppa::Role::Slider);
    assert_eq!(sem.value_text.as_deref(), Some("50 percent"));
    press_node(&host, node_by_debug(&host, "step-inc"));
    assert_eq!(value.get(), 60.0);
    // Off-grid values snap on the next step.
    value.set(53.0);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "step-inc"));
    assert_eq!(value.get(), 60.0, "53 snaps to the 50-grid then steps");
    // Ends clamp (quiet no-op past the end).
    value.set(100.0);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "step-inc"));
    assert_eq!(value.get(), 100.0);
    press_node(&host, node_by_debug(&host, "step-dec"));
    assert_eq!(value.get(), 90.0);
}

#[test]
fn slider_disabled_steppers_are_handlerless() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(50.0f32);
    let props = SliderProps {
        label: SharedString::from("Volume"),
        value: value.clone(),
        min: 0.0,
        max: 100.0,
        step: 10.0,
        enabled: false,
        on_change: None,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
    };
    host.mount("SldDis", SliderCase { props }, slider_render);
    host.run_until_idle();
    for debug in ["step-dec", "step-inc"] {
        let id = node_by_debug(&host, debug);
        assert!(
            host.retained_handlers(id).is_empty(),
            "{debug} handlerless when disabled"
        );
        press_node(&host, id);
    }
    assert_eq!(value.get(), 50.0, "disabled steps never move");
    // The track sheds its drag/arrow/focus handlers too.
    let track = node_by_debug(&host, "slider");
    assert!(
        host.retained_handlers(track).is_empty(),
        "disabled track handlerless"
    );
    assert!(
        !host.tab_order().contains(&track),
        "disabled track leaves the tab order"
    );
}

/// Round 5.3 (decision 270): drag + arrows on the track.
fn slider_harness() -> (ComponentHost, Signal<f32>) {
    let host = ComponentHost::new();
    let value = host.runtime().signal(50.0f32);
    let props = SliderProps {
        label: SharedString::from("Volume"),
        value: value.clone(),
        min: 0.0,
        max: 100.0,
        step: 10.0,
        enabled: true,
        on_change: None,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
    };
    host.mount("Sld", SliderCase { props }, slider_render);
    host.run_until_idle();
    (host, value)
}

/// Round 7.10: rail spans the trackbox, fill covers the value
/// fraction, knob rides the fraction edge (16px knob over 96px:
/// x = frac × 80 — Round 7.15 reflowed the steppers to flank).
/// Geometry follows the value signal.
#[test]
fn slider_rail_fill_knob_track_value() {
    fn geom(host: &ComponentHost) -> ((f32, f32), f32, f32) {
        let rail = host
            .committed_box(node_by_debug(host, "slider-rail"))
            .expect("rail laid out");
        assert_eq!((rail.w, rail.h), (96.0, 6.0));
        let fill = host
            .committed_box(node_by_debug(host, "slider-fill"))
            .expect("fill laid out");
        let knob = host
            .committed_box(node_by_debug(host, "slider-knob"))
            .expect("knob laid out");
        assert_eq!((knob.w, knob.h), (16.0, 16.0));
        let trackbox = host
            .committed_box(node_by_debug(host, "slider-trackbox"))
            .expect("trackbox laid out");
        assert_eq!((trackbox.w, trackbox.h), (96.0, 32.0));
        ((rail.w, rail.h), fill.w, knob.x - trackbox.x)
    }
    let (host, value) = slider_harness();
    let (_, fill, knob_x) = geom(&host);
    assert!(approx(fill, 48.0), "half fill at 50, got {fill}");
    assert!(approx(knob_x, 40.0), "knob mid-track at 50, got {knob_x}");
    value.set(100.0);
    host.run_until_idle();
    let (_, fill, knob_x) = geom(&host);
    assert!(approx(fill, 96.0), "full fill at 100, got {fill}");
    assert!(approx(knob_x, 80.0), "knob at end, got {knob_x}");
    value.set(0.0);
    host.run_until_idle();
    let (_, fill, knob_x) = geom(&host);
    assert!(approx(fill, 0.0), "empty fill at 0, got {fill}");
    assert!(approx(knob_x, 0.0), "knob at start, got {knob_x}");
}

#[test]
fn slider_drag_sets_value_from_pointer_x() {
    let (host, value) = slider_harness();
    let (_x0, y0, tx, tw) = press_track(&host);
    // Drag to 80% across the track: value follows on-grid
    // (grid points are float-exact — no .5-boundary risk).
    let x1 = tx + tw * 0.8;
    host.inject_input(oppa::InputEvent::pointer_move(x1, y0));
    host.run_until_idle();
    assert_eq!(value.get(), 80.0, "drag maps x to value");
    // Drag past the end pins (clamp, never wrap).
    host.inject_input(oppa::InputEvent::pointer_move(tx + tw + 50.0, y0));
    host.run_until_idle();
    assert_eq!(value.get(), 100.0, "drag past end pins max");
    host.inject_input(oppa::InputEvent::pointer_up(x1, y0));
    host.run_until_idle();
    assert_eq!(value.get(), 100.0, "release keeps the value");
}

#[test]
fn slider_arrows_step_when_focused() {
    let (host, value) = slider_harness();
    // Focus the track (TAB — it is a tab stop now).
    host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
    host.run_until_idle();
    let track = node_by_debug(&host, "slider");
    assert_eq!(host.focused_node(), Some(track));
    host.inject_input(InputEvent::key(keys::RIGHT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(value.get(), 60.0, "right steps up");
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(value.get(), 50.0, "left steps down");
    // Held arrows repeat-step.
    host.inject_input(InputEvent::Key {
        code: keys::UP,
        modifiers: oppa::Modifiers::NONE,
        state: KeyState::Pressed,
        repeat: true,
    });
    host.run_until_idle();
    assert_eq!(value.get(), 60.0, "held up repeats");
}

#[test]
fn snap_value_grids_clamps_and_refuses_loudly() {
    assert_eq!(snap_value(53.0, 0.0, 100.0, 10.0), 50.0);
    assert_eq!(snap_value(55.0, 0.0, 100.0, 10.0), 60.0);
    assert_eq!(snap_value(999.0, 0.0, 100.0, 10.0), 100.0);
    assert_eq!(snap_value(-999.0, 0.0, 100.0, 10.0), 0.0);
    assert_eq!(value_text(50.0), "50 percent");
    assert_eq!(value_text(12.5), "12.5 percent");
}

#[test]
#[should_panic(expected = "step must be positive")]
fn snap_value_rejects_zero_step() {
    let _ = snap_value(1.0, 0.0, 10.0, 0.0);
}

#[test]
#[should_panic(expected = "max")]
fn snap_value_rejects_inverted_range() {
    let _ = snap_value(1.0, 10.0, 0.0, 1.0);
}

/// Decision 478: a set icon renders ahead of the label with the
/// group centered as one unit — same outer box, same label
/// semantics, press still activates (the icon is decorative and
/// handlerless).
#[test]
fn button_icon_renders_and_still_activates() {
    let host = ComponentHost::new();
    let fired = host.runtime().signal(false);
    let set = fired.clone();
    let mut rgba = Vec::with_capacity(4 * 4 * 4);
    for _ in 0..16 {
        rgba.extend_from_slice(&[255, 0, 0, 255]);
    }
    let image = host.images().insert_pixels("btn-icon", 4, 4, rgba);
    let props = ButtonProps::new("OK", move || set.set(true))
        .icon(image)
        .icon_size(18.0);
    host.mount("BtnIcon", ButtonCase { props }, button_render);
    host.run_until_idle();
    // Same outer box as the iconless default.
    let outer = host
        .committed_box(node_by_debug(&host, "button"))
        .expect("box");
    assert!(
        approx(outer.w, 96.0) && approx(outer.h, 32.0),
        "outer box unchanged, got {}x{}",
        outer.w,
        outer.h
    );
    // Fixed 18px slot, left of the label, inside the box.
    let icon = host
        .committed_box(node_by_debug(&host, "button-icon"))
        .expect("icon slot laid out");
    assert!(
        approx(icon.w, 18.0) && approx(icon.h, 18.0),
        "icon slot 18x18, got {}x{}",
        icon.w,
        icon.h
    );
    assert!(
        icon.x >= outer.x && icon.x + icon.w <= outer.x + outer.w,
        "icon inside the box"
    );
    assert!(
        approx(icon.y + icon.h / 2.0, outer.y + outer.h / 2.0),
        "icon vertically centered, got {} vs {}",
        icon.y + icon.h / 2.0,
        outer.y + outer.h / 2.0
    );
    // Label semantics unchanged, press still activates.
    let id = node_by_debug(&host, "button");
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!(sem.label.as_deref(), Some("OK"));
    press_node(&host, id);
    assert!(fired.get(), "press activates with an icon set");
}
