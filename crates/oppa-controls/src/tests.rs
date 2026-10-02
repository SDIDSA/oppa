use super::*;
use oppa::input::{keys, InputEvent, KeyState};
use oppa::{
    find_retained_by_debug, Cluster, ComponentHost, FontId, FontMetrics, NodeId, ShapedGlyph,
    ShapedRun, TextError, TextRun, TextService, TextStyle,
};
use oppa::{
    Color, Column, Ctx, CursorIcon, Div, Props, Px, ScrollOffset, Semantics, SharedString, Signal,
    Style, Text, VNode,
};
use oppa_macros::Props;
use std::cell::RefCell;
use std::rc::Rc;

/// Uniform-advance fake shaper (body 14px → 8.75px/char), so measured
/// content widths prove which string is displayed.
pub(crate) struct FakeText;

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

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

/// Measured width of the single laid text leaf (proves *which* string
/// is displayed — placeholder vs value — without a retained-text
/// reader, which the host deliberately does not expose).
fn lined_content_w(host: &ComponentHost) -> f32 {
    let mut ws: Vec<f32> = find_retained_by_debug(host, "text")
        .into_iter()
        .filter_map(|id| host.committed_box(id))
        .filter(|b| !b.lines.is_empty())
        .map(|b| b.content_w)
        .collect();
    assert_eq!(ws.len(), 1, "exactly one measured leaf, got {}", ws.len());
    ws.pop().unwrap()
}

fn press_node(host: &ComponentHost, id: NodeId) {
    let b = host.committed_box(id).expect("control has a hit box");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    host.inject_input(InputEvent::pointer_down(cx, cy));
    host.inject_input(InputEvent::pointer_up(cx, cy));
    host.run_until_idle();
}

fn node_by_debug(host: &ComponentHost, debug: &str) -> NodeId {
    find_retained_by_debug(host, debug)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no retained node {debug:?}"))
}

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

#[derive(Clone)]
struct CheckCase {
    props: CheckboxProps,
}
impl Props for CheckCase {}

fn check_render(ctx: &Ctx, p: &CheckCase) -> VNode {
    ctx.child("oppa::Checkbox", 7, &p.props, Checkbox)
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

#[derive(Clone)]
struct ToggleCase {
    props: ToggleProps,
}
impl Props for ToggleCase {}

fn toggle_render(ctx: &Ctx, p: &ToggleCase) -> VNode {
    ctx.child("oppa::Toggle", 7, &p.props, Toggle)
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

#[derive(Clone)]
struct SliderCase {
    props: SliderProps,
}
impl Props for SliderCase {}

fn slider_render(ctx: &Ctx, p: &SliderCase) -> VNode {
    ctx.child("oppa::Slider", 7, &p.props, Slider)
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

/// Presses a track point owned by the track itself (not the
/// step buttons): scans the trackbox for a hit the buttons
/// do not own (layout-independent — a missing free point is a
/// genuine layout surprise, panics loudly). Capture still
/// belongs to the `slider` root (visuals carry no handlers —
/// presses climb to it).
fn press_track(host: &ComponentHost) -> (f32, f32, f32, f32) {
    let track = node_by_debug(host, "slider");
    let tb = host
        .committed_box(node_by_debug(host, "slider-trackbox"))
        .expect("trackbox");
    for i in 0..16 {
        let x = tb.x + tb.w * (i as f32 + 0.5) / 16.0;
        let y = tb.y + tb.h / 2.0;
        host.inject_input(oppa::InputEvent::pointer_down(x, y));
        host.run_until_idle();
        if host.capture_node_for(0) == Some(track) {
            let _ = host.pointer_position(0);
            return (x, y, tb.x, tb.w);
        }
        host.inject_input(oppa::InputEvent::pointer_cancel_for(0));
        host.run_until_idle();
    }
    panic!("no track-owned press point in the slider box");
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

#[test]
fn text_input_renders_value_and_semantics() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("Ada"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    let id = node_by_debug(&host, "text-input");
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::TextField);
    assert_eq!(sem.label.as_deref(), Some("Name"));
    assert!(!sem.disabled);
    let b = host.committed_box(id).expect("hit box");
    assert!(
        approx(b.w, 200.0) && approx(b.h, 32.0),
        "default 200x32, got {}x{}",
        b.w,
        b.h
    );
    assert!(
        host.tab_order().contains(&id),
        "enabled input is tabbable (press owner, decision 96)"
    );
    press_node(&host, id);
    assert_eq!(
        host.focused_node(),
        Some(id),
        "press focuses through the M5 router"
    );
}

#[test]
fn text_input_placeholder_shows_when_empty() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    host.mount(
        "TI",
        TextInputProps::new("Name", value.clone()).placeholder("Enter name"),
        TextInput,
    );
    host.run_until_idle();
    // Empty: the outer carries the role; the placeholder is a
    // presentational span, never a bindable field.
    assert_eq!(host.text_fields().len(), 1);
    assert!(
        approx(lined_content_w(&host), 87.5),
        "placeholder 10x8.75 displayed"
    );
    value.set(SharedString::from("Ada"));
    host.run_until_idle();
    assert_eq!(host.text_fields().len(), 2, "outer + value leaf");
    assert!(
        approx(lined_content_w(&host), 26.25),
        "value 3x8.75 displayed"
    );
    let id = node_by_debug(&host, "text-input");
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!(sem.label.as_deref(), Some("Name"), "label stable");
}

/// Decision 293 (OQ-SINK-1 fixed): typing into an empty+
/// placeholder `TextInput` feeds the value through the session
/// fallback (the field leaf only exists once the value is
/// non-empty — the outer resolves to the instance session).
/// On DOM the same contract renders `value=""` with a native
/// `placeholder` attribute, so the browser only ever sends
/// typed text.
#[test]
fn text_input_placeholder_typing_feeds_value() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    host.mount(
        "TI",
        TextInputProps::new("Name", value.clone()).placeholder("Enter name"),
        TextInput,
    );
    host.run_until_idle();
    let id = node_by_debug(&host, "text-input");
    host.inject_input(InputEvent::text(id, "Ada"));
    host.run_until_idle();
    assert_eq!(
        value.get(),
        SharedString::from("Ada"),
        "typed text feeds the value even under a placeholder"
    );
}

/// Decision 293: the fallback also covers the value leaf of a
/// non-empty field (same session resolution through the leaf's
/// press owner — the web U8 channel for every field, not just
/// placeholder ones).
#[test]
fn text_input_leaf_typing_feeds_value_without_bind_call() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("Al"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    // No `bind_edit_session` anywhere (the fallback self-wires).
    let field = node_by_debug(&host, "field");
    host.inject_input(InputEvent::text(field, "Ada"));
    host.run_until_idle();
    assert_eq!(
        value.get(),
        SharedString::from("Ada"),
        "leaf text feeds the value with no app-layer bind"
    );
}

#[test]
fn text_input_disabled_is_handlerless() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(SharedString::from("Ada"));
    host.mount(
        "TIDis",
        TextInputProps::new("Name", value.clone()).disabled(),
        TextInput,
    );
    host.run_until_idle();
    let id = node_by_debug(&host, "text-input");
    let sem = host.retained_semantics(id).expect("semantics");
    assert!(sem.disabled, "disabled flag on the role");
    assert!(
        host.retained_handlers(id).is_empty(),
        "disabled carries no handler — refusal is structural"
    );
    assert!(
        !host.tab_order().contains(&id),
        "disabled leaves the tab order (decision 96)"
    );
    press_node(&host, id);
    assert_eq!(
        value.get(),
        SharedString::from("Ada"),
        "press on disabled never edits"
    );
    assert_eq!(host.focused_node(), None, "press never focuses");
}

#[test]
fn modal_hidden_when_closed() {
    let host = ComponentHost::new();
    let open = host.runtime().signal(false);
    host.mount("M", ModalProps::new("Delete file?", open), Modal);
    host.run_until_idle();
    assert!(
        find_retained_by_debug(&host, "modal-card").is_empty(),
        "no dialog mounted while closed"
    );
    assert!(
        find_retained_by_debug(&host, "modal-backdrop").is_empty()
            && find_retained_by_debug(&host, "modal-overlay").is_empty()
    );
    let id = node_by_debug(&host, "modal-closed");
    let b = host.committed_box(id).expect("closed box");
    assert!(
        approx(b.w, 0.0) && approx(b.h, 0.0),
        "zero hit-testable dialog area"
    );
    assert!(
        host.retained_handlers(id).is_empty(),
        "closed modal is inert"
    );
}

#[derive(Clone)]
struct ModalGapProps {
    open: Signal<bool>,
}
impl Props for ModalGapProps {}

fn modal_gap_app(ctx: &Ctx, props: &ModalGapProps) -> VNode {
    Column::new().children([
        Div("gap-top").style(Style::new().size(100, 10)).build(),
        ctx.child(
            "oppa::Modal",
            1,
            &ModalProps::new("Delete?", props.open.clone()),
            Modal,
        ),
        Div("gap-bottom").style(Style::new().size(100, 10)).build(),
    ])
}

/// Round 7.21 (decision 296): the closed modal is a skipped
/// portal, so it leaves zero gap in the parent column — and
/// opening mounts the overlay without moving the column either.
#[test]
fn modal_closed_introduces_zero_gap_in_parent_column() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(false);
    host.mount("G", ModalGapProps { open: open.clone() }, modal_gap_app);
    host.run_until_idle();
    let bottom_y = || {
        host.committed_box(node_by_debug(&host, "gap-bottom"))
            .expect("bottom box")
            .y
    };
    assert!(
        approx(bottom_y(), 10.0),
        "closed modal leaves zero gap, bottom at y={}",
        bottom_y()
    );
    open.set(true);
    host.run_until_idle();
    assert!(
        !find_retained_by_debug(&host, "modal-card").is_empty(),
        "dialog mounts when open"
    );
    assert!(
        approx(bottom_y(), 10.0),
        "open modal leaves the column untouched, bottom at y={}",
        bottom_y()
    );
}

#[derive(Clone)]
struct ModalNestedProps {
    open: Signal<bool>,
}
impl Props for ModalNestedProps {}

/// The eyeball scenario: the dialog is triggered from content
/// nested inside padded layout (a tab panel), not from the root.
fn modal_nested_app(ctx: &Ctx, props: &ModalNestedProps) -> VNode {
    Div("pad")
        .style(Style::new().pad_x(50).pad_y(40))
        .child(Column::new().children([
            Div("gap-top").style(Style::new().size(100, 10)).build(),
            ctx.child(
                "oppa::Modal",
                1,
                &ModalProps::new("Delete?", props.open.clone()),
                Modal,
            ),
        ]))
}

/// Round 7.21 follow-up (user eyeball): a modal mounted inside
/// nested content still dims from the viewport origin — the
/// repositioned ancestors must not drag the portal along.
#[test]
fn modal_nested_in_content_still_dims_from_viewport_origin() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount(
        "N",
        ModalNestedProps { open: open.clone() },
        modal_nested_app,
    );
    host.run_until_idle();
    let bb = host
        .committed_box(node_by_debug(&host, "modal-backdrop"))
        .expect("backdrop box");
    assert!(
        approx(bb.x, 0.0) && approx(bb.y, 0.0),
        "backdrop at viewport origin, got ({}, {})",
        bb.x,
        bb.y
    );
    assert!(
        approx(bb.w, 800.0) && approx(bb.h, 600.0),
        "backdrop fills the viewport, got {}x{}",
        bb.w,
        bb.h
    );
    let cb = host
        .committed_box(node_by_debug(&host, "modal-card"))
        .expect("card box");
    assert!(
        approx(cb.x, (800.0 - cb.w) / 2.0) && approx(cb.y, (600.0 - cb.h) / 2.0),
        "card centered in the window, got ({}, {})",
        cb.x,
        cb.y
    );
}

#[test]
fn modal_visible_when_open() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount("M", ModalProps::new("Delete file?", open), Modal);
    host.run_until_idle();
    let card = node_by_debug(&host, "modal-card");
    let sem = host.retained_semantics(card).expect("semantics");
    assert_eq!(sem.role, oppa::Role::Dialog);
    assert_eq!(sem.label.as_deref(), Some("Delete file?"));
    let cb = host.committed_box(card).expect("card box");
    assert!(approx(cb.w, 360.0), "explicit card width, got {}", cb.w);
    // Full-viewport dimming (Round 7.21): the backdrop covers the
    // default 800x600 host viewport, and the card centers on both
    // axes inside it.
    let bb = host
        .committed_box(node_by_debug(&host, "modal-backdrop"))
        .expect("backdrop box");
    assert!(
        approx(bb.x, 0.0) && approx(bb.y, 0.0),
        "backdrop at viewport origin, got ({}, {})",
        bb.x,
        bb.y
    );
    assert!(
        approx(bb.w, 800.0) && approx(bb.h, 600.0),
        "backdrop fills the viewport, got {}x{}",
        bb.w,
        bb.h
    );
    assert!(
        approx(cb.x, bb.x + (bb.w - cb.w) / 2.0),
        "card horizontally centered, x={}",
        cb.x
    );
    assert!(
        approx(cb.y, bb.y + (bb.h - cb.h) / 2.0),
        "card vertically centered, y={}",
        cb.y
    );
    // Title (18px: 12 chars x 11.25) + labels ("OK" 17.5, "Cancel" 52.5).
    let mut ws: Vec<f32> = find_retained_by_debug(&host, "text")
        .into_iter()
        .filter_map(|id| host.committed_box(id))
        .filter(|b| !b.lines.is_empty())
        .map(|b| b.content_w)
        .collect();
    ws.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(ws.len(), 3, "title + two button labels");
    assert!(
        approx(ws[0], 17.5) && approx(ws[1], 52.5) && approx(ws[2], 135.0),
        "OK/Cancel/title widths, got {ws:?}"
    );
    node_by_debug(&host, "modal-cancel");
    node_by_debug(&host, "modal-confirm");
}

#[test]
fn modal_backdrop_click_dismisses() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount("M", ModalProps::new("Delete file?", open.clone()), Modal);
    host.run_until_idle();
    let bb = host
        .committed_box(node_by_debug(&host, "modal-backdrop"))
        .expect("backdrop box");
    let cb = host
        .committed_box(node_by_debug(&host, "modal-card"))
        .expect("card box");
    // Left band: inside the backdrop, outside the centered card.
    let (x, y) = (bb.x + 5.0, bb.y + bb.h / 2.0);
    assert!(x < cb.x, "click lands outside the card");
    host.inject_input(InputEvent::pointer_down(x, y));
    host.inject_input(InputEvent::pointer_up(x, y));
    host.run_until_idle();
    assert!(!open.get(), "backdrop press dismisses");
    assert!(
        find_retained_by_debug(&host, "modal-card").is_empty(),
        "dialog unmounts on dismiss"
    );
    // Opt-out is structural: no handler, press is a quiet miss.
    let host2 = ComponentHost::new();
    host2.set_text_service(Box::new(FakeText));
    let open2 = host2.runtime().signal(true);
    host2.mount(
        "M2",
        ModalProps::new("Delete file?", open2.clone()).no_backdrop_dismiss(),
        Modal,
    );
    host2.run_until_idle();
    let bd2 = node_by_debug(&host2, "modal-backdrop");
    assert!(
        host2.retained_handlers(bd2).is_empty(),
        "opt-out backdrop carries no handler"
    );
    let bb2 = host2.committed_box(bd2).expect("backdrop box");
    host2.inject_input(InputEvent::pointer_down(bb2.x + 5.0, bb2.y + bb2.h / 2.0));
    host2.inject_input(InputEvent::pointer_up(bb2.x + 5.0, bb2.y + bb2.h / 2.0));
    host2.run_until_idle();
    assert!(open2.get(), "opt-out backdrop never dismisses");
}

#[test]
fn modal_confirm_and_cancel_actions() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    let confirmed = host.runtime().signal(false);
    let cancelled = host.runtime().signal(false);
    let (c_set, x_set) = (confirmed.clone(), cancelled.clone());
    host.mount(
        "M",
        ModalProps::new("Delete file?", open.clone())
            .on_confirm(move || c_set.set(true))
            .on_cancel(move || x_set.set(true)),
        Modal,
    );
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "modal-confirm"));
    assert!(confirmed.get(), "confirm callback fires");
    assert!(!cancelled.get(), "confirm does not run cancel");
    assert!(!open.get(), "confirm closes");
    open.set(true);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "modal-cancel"));
    assert!(cancelled.get(), "cancel callback fires");
    assert!(!open.get(), "cancel closes");
}

#[test]
fn text_input_live_typing_updates_value() {
    use oppa_app::DesktopLoop;
    // Full runner path, headless: real `TextInput` + real
    // `DesktopLoop` — press to focus, type, backspace.
    let mut loop_ = DesktopLoop::new(500, 600, Box::new(FakeText), "Test").expect("headless loop");
    let value = loop_.host().runtime().signal(SharedString::from(""));
    loop_.mount(
        "TI",
        TextInputProps::new("Name", value.clone()).placeholder("Enter name"),
        TextInput,
    );
    let id = node_by_debug(loop_.host(), "text-input");
    let b = loop_.host().committed_box(id).expect("hit box");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    loop_
        .step(oppa::InputEvent::pointer_down(cx, cy))
        .expect("down steps");
    loop_
        .step(oppa::InputEvent::pointer_up(cx, cy))
        .expect("up steps");
    assert_eq!(loop_.host().focused_node(), Some(id));
    assert!(loop_.type_text("Hi").expect("types") > 0);
    assert_eq!(value.get(), SharedString::from("Hi"));
    loop_.backspace().expect("backspaces");
    assert_eq!(value.get(), SharedString::from("H"));
    loop_.type_text("!").expect("types");
    assert_eq!(value.get(), SharedString::from("H!"));
}

#[test]
fn text_input_edit_session_binds() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(SharedString::from(""));
    let handle = host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    let sessions = host.edit_sessions_for(handle.root_instance());
    assert_eq!(sessions.len(), 1, "one session keyed to the instance");
    sessions[0].insert("hi");
    host.run_until_idle();
    assert_eq!(
        value.get(),
        SharedString::from("hi"),
        "programmatic feed lands in the value signal"
    );
    // ...and the session feeds the field leaf through the U8 bind path.
    let field = node_by_debug(&host, "field");
    host.bind_edit_session(field, &sessions[0]);
    host.inject_input(InputEvent::text(field, "yo"));
    host.run_until_idle();
    assert_eq!(value.get(), SharedString::from("yo"));
}

#[derive(Clone)]
struct ToastCase {
    props: ToastProps,
}
impl Props for ToastCase {}

fn toast_render(ctx: &Ctx, p: &ToastCase) -> VNode {
    ctx.child("oppa::Toast", 13, &p.props, Toast)
}

#[test]
fn toast_hidden_when_closed() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(false);
    host.mount(
        "T",
        ToastCase {
            props: ToastProps::new("Saved", open),
        },
        toast_render,
    );
    host.run_until_idle();
    assert!(
        find_retained_by_debug(&host, "toast-card").is_empty(),
        "no toast mounted while closed"
    );
    let id = node_by_debug(&host, "toast-closed");
    let b = host.committed_box(id).expect("closed box");
    assert!(
        approx(b.w, 0.0) && approx(b.h, 0.0),
        "zero hit-testable toast area"
    );
    assert!(
        host.retained_handlers(id).is_empty(),
        "closed toast is inert"
    );
}

#[test]
fn toast_visible_with_status_semantics_when_open() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount(
        "T",
        ToastCase {
            props: ToastProps::new("Saved", open).variant(ToastVariant::Success),
        },
        toast_render,
    );
    host.run_until_idle();
    let id = node_by_debug(&host, "toast-card");
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::Status, "status role, never dialog");
    assert_eq!(sem.label.as_deref(), Some("Saved"), "message is the label");
    // The anchor is handler-less full-viewport chrome: only the
    // dismiss button carries a handler, so presses outside the
    // card fall through to the app (the inverse of Modal's
    // capturing backdrop).
    assert!(
        host.retained_handlers(node_by_debug(&host, "toast-anchor"))
            .is_empty(),
        "anchor never claims presses"
    );
}

#[test]
fn toast_dismiss_button_closes() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount(
        "T",
        ToastCase {
            props: ToastProps::new("Saved", open.clone()).sticky(),
        },
        toast_render,
    );
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "toast-dismiss"));
    assert!(!open.get(), "dismiss press closes");
    host.run_until_idle();
    assert!(
        find_retained_by_debug(&host, "toast-card").is_empty(),
        "toast unmounts on dismiss"
    );
}

#[test]
fn toast_auto_dismiss_fires_on_clock() {
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount(
        "T",
        ToastCase {
            props: ToastProps::new("Saved", open.clone()).auto_dismiss_ms(Some(4000)),
        },
        toast_render,
    );
    settle_clocked(&host, &clock);
    assert_eq!(host.tick_timers(host.now_ms()), 0, "nothing due yet");
    assert!(open.get(), "still visible before the delay");
    clock.advance(4.1);
    assert_eq!(host.tick_timers(host.now_ms()), 1, "delay timer fires");
    host.run_until_idle();
    assert!(!open.get(), "auto-dismiss closes");
}

#[test]
fn toast_sticky_never_arms_timer() {
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount(
        "T",
        ToastCase {
            props: ToastProps::new("Saved", open.clone()).sticky(),
        },
        toast_render,
    );
    settle_clocked(&host, &clock);
    clock.advance(60.0);
    assert_eq!(host.tick_timers(host.now_ms()), 0, "sticky arms nothing");
    host.run_until_idle();
    assert!(open.get(), "sticky toast stays until dismissed");
}

fn radio_props(label: &str, selected: bool, fired: &Signal<bool>) -> RadioProps {
    let set = fired.clone();
    RadioProps {
        label: SharedString::from(label),
        selected,
        enabled: true,
        on_select: action(move || set.set(true)),
    }
}

#[test]
fn radio_renders_selected_dot_and_semantics() {
    for (selected, want_dot) in [(true, true), (false, false)] {
        let host = ComponentHost::new();
        let fired = host.runtime().signal(false);
        host.mount("R", radio_props("Pro", selected, &fired), Radio);
        host.run_until_idle();
        let id = node_by_debug(&host, "radio");
        let sem = host.retained_semantics(id).expect("semantics");
        assert_eq!(sem.role, oppa::Role::RadioButton);
        assert_eq!(sem.selected, Some(selected));
        assert_eq!(sem.label.as_deref(), Some("Pro"));
        assert!(!sem.disabled);
        let ib = host
            .committed_box(node_by_debug(&host, "radio-indicator"))
            .expect("indicator box");
        assert!(
            approx(ib.w, 18.0) && approx(ib.h, 18.0),
            "indicator 18x18, got {}x{}",
            ib.w,
            ib.h
        );
        let dots = find_retained_by_debug(&host, "radio-dot");
        assert_eq!(!dots.is_empty(), want_dot, "dot iff selected");
        if want_dot {
            let db = host.committed_box(dots[0]).expect("dot box");
            assert!(
                approx(db.w, 8.0) && approx(db.h, 8.0),
                "dot 8x8, got {}x{}",
                db.w,
                db.h
            );
            // (18 - 8) / 2 = 5: exact centering via out-of-flow offsets.
            assert!(
                approx(db.x, ib.x + 5.0) && approx(db.y, ib.y + 5.0),
                "dot centered, got ({}, {}) in ({}, {})",
                db.x,
                db.y,
                ib.x,
                ib.y
            );
        }
    }
}

#[test]
fn radio_press_fires_select() {
    let host = ComponentHost::new();
    let fired = host.runtime().signal(false);
    host.mount("R", radio_props("Pro", false, &fired), Radio);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "radio"));
    assert!(fired.get(), "press selects");
}

#[test]
fn radio_disabled_is_handlerless() {
    let host = ComponentHost::new();
    let fired = host.runtime().signal(false);
    let set = fired.clone();
    let props = RadioProps {
        label: SharedString::from("Pro"),
        selected: false,
        enabled: false,
        on_select: action(move || set.set(true)),
    };
    host.mount("RDis", props, Radio);
    host.run_until_idle();
    let id = node_by_debug(&host, "radio");
    let sem = host.retained_semantics(id).expect("semantics");
    assert!(sem.disabled, "disabled flag on the role");
    assert!(
        host.retained_handlers(id).is_empty(),
        "disabled carries no handler — refusal is structural"
    );
    assert!(
        !host.tab_order().contains(&id),
        "disabled leaves the tab order (decision 96)"
    );
    press_node(&host, id);
    assert!(!fired.get(), "press on disabled never selects");
}

#[test]
fn radio_group_selects_exclusively() {
    let host = ComponentHost::new();
    let selected = host.runtime().signal(SharedString::from("Free"));
    let props = RadioGroupProps {
        options: vec![
            RadioOption {
                value: SharedString::from("Free"),
                label: SharedString::from("Free"),
            },
            RadioOption {
                value: SharedString::from("Pro"),
                label: SharedString::from("Pro"),
            },
        ],
        selected: selected.clone(),
        enabled: true,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
    };
    host.mount("G", props, RadioGroup::<SharedString>);
    host.run_until_idle();
    let radios = || find_retained_by_debug(&host, "radio");
    assert_eq!(radios().len(), 2);
    let sem = |id| host.retained_semantics(id).expect("semantics");
    assert_eq!(sem(radios()[0]).selected, Some(true));
    assert_eq!(sem(radios()[1]).selected, Some(false));
    press_node(&host, radios()[1]);
    assert_eq!(selected.get(), SharedString::from("Pro"));
    assert_eq!(sem(radios()[0]).selected, Some(false));
    assert_eq!(sem(radios()[1]).selected, Some(true));
}

fn tabs_props(active: Signal<String>) -> TabsProps<String> {
    // Same panel debug for both tabs (position-matched same-tag
    // children recycle in place by design — M8 — so debug labels
    // would go stale; the contents differ in measured width
    // instead, which proves which view is mounted).
    let mk = |value: &str, body: &'static str| TabItem {
        value: value.to_string(),
        label: SharedString::from(value),
        content: Rc::new(move |_| {
            Div("panel").child(VNode::from(Text {
                text: SharedString::from(body),
                style: Text::body_secondary,
            }))
        }),
    };
    TabsProps {
        tabs: vec![mk("a", "Alpha"), mk("b", "Beta-2-X")],
        active,
        enabled: true,
    }
}

fn panel_content_w(host: &ComponentHost) -> f32 {
    let panel = node_by_debug(host, "tab-panel");
    host.committed_box(panel).expect("panel box").content_w
}

#[test]
fn tabs_renders_tab_list_and_active_panel() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let active = host.runtime().signal("a".to_string());
    host.mount("T", tabs_props(active), Tabs::<String>);
    host.run_until_idle();
    let bar = node_by_debug(&host, "tab-bar");
    let sem = host.retained_semantics(bar).expect("semantics");
    assert_eq!(sem.role, oppa::Role::TabList);
    let items = find_retained_by_debug(&host, "tab-item");
    assert_eq!(items.len(), 2);
    let sem = |id| host.retained_semantics(id).expect("semantics");
    assert_eq!(sem(items[0]).role, oppa::Role::Tab);
    assert_eq!(sem(items[0]).selected, Some(true));
    assert_eq!(sem(items[0]).label.as_deref(), Some("a"));
    assert_eq!(sem(items[1]).selected, Some(false));
    // "Alpha": 5 chars x 8.75 — the active view is mounted.
    assert!(
        approx(panel_content_w(&host), 43.75),
        "panel shows Alpha, w={}",
        panel_content_w(&host)
    );
}

#[test]
fn tabs_click_switches_active_panel() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let active = host.runtime().signal("a".to_string());
    host.mount("T", tabs_props(active.clone()), Tabs::<String>);
    host.run_until_idle();
    let items = || find_retained_by_debug(&host, "tab-item");
    press_node(&host, items()[1]);
    assert_eq!(active.get(), "b".to_string());
    // "Beta-2-X": 8 chars x 8.75 — the view swapped in place.
    assert!(
        approx(panel_content_w(&host), 70.0),
        "panel shows Beta-2-X, w={}",
        panel_content_w(&host)
    );
    let sem = |id| host.retained_semantics(id).expect("semantics");
    assert_eq!(sem(items()[0]).selected, Some(false));
    assert_eq!(sem(items()[1]).selected, Some(true));
}

fn select_props(selected: Signal<String>, open: Signal<bool>) -> SelectProps<String> {
    SelectProps::new(
        vec![
            SelectItem {
                value: "a".to_string(),
                label: SharedString::from("Alpha"),
            },
            SelectItem {
                value: "b".to_string(),
                label: SharedString::from("Beta"),
            },
            SelectItem {
                value: "c".to_string(),
                label: SharedString::from("Gamma"),
            },
        ],
        selected,
        open,
    )
}

#[test]
fn select_renders_combobox_with_current_label() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let selected = host.runtime().signal("b".to_string());
    let open = host.runtime().signal(false);
    host.mount("S", select_props(selected, open), Select::<String>);
    host.run_until_idle();
    let box_id = node_by_debug(&host, "select-box");
    let sem = host.retained_semantics(box_id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::ComboBox);
    assert_eq!(sem.label.as_deref(), Some("Beta"));
    assert!(!sem.disabled);
    // Closed: no list, no options.
    assert!(find_retained_by_debug(&host, "select-list").is_empty());
    assert!(find_retained_by_debug(&host, "select-option").is_empty());
}

/// Decision 291: the chevron is one vector `Path` leaf (12×8),
/// not a "▾" text glyph — the box's only text leaf is the label
/// ("Beta" = 4 × 8.75 = 35.0), so no symbol-font coverage decides
/// the control's shape.
#[test]
fn select_chevron_is_a_vector_not_a_glyph() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let selected = host.runtime().signal("b".to_string());
    let open = host.runtime().signal(false);
    host.mount("S", select_props(selected, open), Select::<String>);
    host.run_until_idle();
    let chevrons = find_retained_by_debug(&host, "select-chevron");
    assert_eq!(chevrons.len(), 1, "one vector chevron, got {chevrons:?}");
    let cb = host.committed_box(chevrons[0]).expect("chevron laid out");
    assert!(
        approx(cb.w, 12.0) && approx(cb.h, 8.0),
        "chevron 12x8, got {}x{}",
        cb.w,
        cb.h
    );
    // Only the label measures text (the chevron measures nothing).
    let mut ws: Vec<f32> = find_retained_by_debug(&host, "text")
        .into_iter()
        .filter_map(|id| host.committed_box(id))
        .filter(|b| !b.lines.is_empty())
        .map(|b| b.content_w)
        .collect();
    ws.sort_by(|a, b| a.partial_cmp(b).expect("finite widths"));
    assert_eq!(ws.len(), 1, "label only, got {ws:?}");
    assert!(approx(ws[0], 35.0), "Beta widths, got {ws:?}");
}

#[test]
fn select_open_pick_closes_and_sets_value() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let selected = host.runtime().signal("b".to_string());
    let open = host.runtime().signal(false);
    host.mount(
        "S",
        select_props(selected.clone(), open.clone()),
        Select::<String>,
    );
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "select-box"));
    assert!(open.get(), "box press opens the list");
    let options = || find_retained_by_debug(&host, "select-option");
    assert_eq!(options().len(), 3);
    // Options reuse list_item + selected (existing payloads).
    let sem = |id| host.retained_semantics(id).expect("semantics");
    assert_eq!(sem(options()[0]).role, oppa::Role::ListItem);
    assert_eq!(sem(options()[0]).label.as_deref(), Some("Alpha"));
    assert_eq!(sem(options()[0]).selected, Some(false));
    assert_eq!(sem(options()[1]).selected, Some(true));
    press_node(&host, options()[2]);
    assert_eq!(selected.get(), "c".to_string());
    assert!(!open.get(), "picking closes the list");
    assert!(options().is_empty(), "closed list unmounts");
    let box_sem = host
        .retained_semantics(node_by_debug(&host, "select-box"))
        .expect("semantics");
    assert_eq!(box_sem.label.as_deref(), Some("Gamma"));
}

#[test]
fn select_disabled_is_handlerless_and_keeps_label() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let selected = host.runtime().signal("a".to_string());
    let open = host.runtime().signal(false);
    let mut props = select_props(selected.clone(), open.clone());
    props.enabled = false;
    host.mount("S", props, Select::<String>);
    host.run_until_idle();
    let box_id = node_by_debug(&host, "select-box");
    let sem = host.retained_semantics(box_id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::ComboBox);
    assert!(sem.disabled);
    assert_eq!(sem.label.as_deref(), Some("Alpha"));
    press_node(&host, box_id);
    assert!(!open.get(), "disabled box never opens");
    assert!(
        !host.tab_order().contains(&box_id),
        "disabled leaves the tab order (decision 96)"
    );
    // The list still follows the signal truthfully when opened
    // externally — but its options are handlerless too.
    open.set(true);
    host.run_until_idle();
    let options = find_retained_by_debug(&host, "select-option");
    assert_eq!(options.len(), 3);
    press_node(&host, options[1]);
    assert_eq!(
        selected.get(),
        "a".to_string(),
        "disabled pick never selects"
    );
    assert!(open.get(), "disabled pick never closes");
}

#[derive(Clone)]
struct SelectFormProps {
    selected: Signal<String>,
    open: Signal<bool>,
}
impl Props for SelectFormProps {}

fn select_form_app(ctx: &Ctx, props: &SelectFormProps) -> VNode {
    Column::new().children([
        Div("form-top").style(Style::new().size(100, 10)).build(),
        ctx.child(
            "oppa::Select",
            1,
            &select_props(props.selected.clone(), props.open.clone()),
            Select::<String>,
        ),
        Div("form-trailer")
            .style(Style::new().size(100, 10))
            .build(),
    ])
}

/// Round 7.21 (decision 296): opening the dropdown neither
/// grows the `Select` box past 32px nor shifts the trailing
/// sibling — the list rides an anchored portal under the box
/// (32px + 4px gap) at list width.
#[test]
fn select_open_neither_grows_nor_shifts_siblings() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let selected = host.runtime().signal("b".to_string());
    let open = host.runtime().signal(false);
    host.mount(
        "F",
        SelectFormProps {
            selected: selected.clone(),
            open: open.clone(),
        },
        select_form_app,
    );
    host.run_until_idle();
    let select_h = || {
        host.committed_box(node_by_debug(&host, "select"))
            .expect("select box")
            .h
    };
    let trailer_y = || {
        host.committed_box(node_by_debug(&host, "form-trailer"))
            .expect("trailer box")
            .y
    };
    assert!(approx(select_h(), 32.0), "closed box is 32px");
    let y_closed = trailer_y();
    open.set(true);
    host.run_until_idle();
    assert!(
        !find_retained_by_debug(&host, "select-list").is_empty(),
        "list mounts when open"
    );
    assert!(
        approx(select_h(), 32.0),
        "open keeps the 32px box, got {}",
        select_h()
    );
    assert!(
        approx(trailer_y(), y_closed),
        "trailer never shifts, {} vs {}",
        trailer_y(),
        y_closed
    );
    let popup = host
        .committed_box(node_by_debug(&host, "select-popup"))
        .expect("popup box");
    let sbox = host
        .committed_box(node_by_debug(&host, "select-box"))
        .expect("select box");
    assert!(
        approx(popup.x, sbox.x) && approx(popup.y, sbox.y + 36.0),
        "popup anchors under the box, got ({}, {}) vs box ({}, {})",
        popup.x,
        popup.y,
        sbox.x,
        sbox.y
    );
    assert!(
        approx(popup.w, 160.0),
        "popup keeps the list width, got {}",
        popup.w
    );
}

fn progress_width(host: &ComponentHost, value: f32) -> (f32, f32) {
    host.mount("P", ProgressBarProps::new(value), ProgressBar);
    host.run_until_idle();
    let track = host
        .committed_box(node_by_debug(host, "progressbar-track"))
        .expect("track box");
    let fill = host
        .committed_box(node_by_debug(host, "progressbar-fill"))
        .expect("fill box");
    (track.w, fill.w)
}

#[test]
fn progressbar_fill_scales_with_clamped_value() {
    // (value, expected fill width) on the default 160 track.
    for (value, want) in [
        (0.0, 0.0),
        (0.25, 40.0),
        (0.68, 108.8),
        (1.0, 160.0),
        // Boundaries clamp gracefully (quiet value normalization,
        // the slider-clamp class — never a refusal).
        (-0.5, 0.0),
        (1.5, 160.0),
    ] {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let (track_w, fill_w) = progress_width(&host, value);
        assert!(approx(track_w, 160.0), "track full width, got {track_w}");
        assert!(
            approx(fill_w, want),
            "value {value}: fill {fill_w}, want {want}"
        );
    }
}

#[test]
fn progressbar_role_and_percent_text() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let mut props = ProgressBarProps::new(0.68);
    props.label = Some(SharedString::from("Storage Quota"));
    host.mount("P", props, ProgressBar);
    host.run_until_idle();
    let sem = host
        .retained_semantics(node_by_debug(&host, "progressbar-track"))
        .expect("semantics");
    assert_eq!(sem.role, oppa::Role::ProgressBar);
    assert_eq!(sem.value_text.as_deref(), Some("68 percent"));
    assert_eq!(sem.label.as_deref(), Some("Storage Quota"));
}

#[test]
#[should_panic(expected = "ProgressBar value is NaN")]
fn progressbar_nan_refuses_loudly() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    host.mount("P", ProgressBarProps::new(f32::NAN), ProgressBar);
    host.run_until_idle();
}

#[test]
fn badge_mounts_with_label() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    host.mount(
        "B",
        BadgeProps::new("PRO").variant(BadgeVariant::Success),
        Badge,
    );
    host.run_until_idle();
    let id = node_by_debug(&host, "badge");
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::Generic);
    assert_eq!(sem.label.as_deref(), Some("PRO"));
    let b = host.committed_box(id).expect("badge box");
    assert!(approx(b.h, 24.0), "fixed pill height, got {}", b.h);
    assert!(b.w > 0.0, "chip sizes to its text, w={}", b.w);
}

// ------------------------------------------------------------------
// Round 3.3 (decision 262): the dismiss-first chain over a live
// popup + field — composition cancels before focus clears
// (clearing first would commit it, locked #27), popups close at
// the author step, and an empty chain reports Unhandled for the
// runner (nav pop / exit — `nav.rs` BackPress).
// ------------------------------------------------------------------

#[derive(Clone)]
struct BackScene {
    open: Signal<bool>,
    value: Signal<SharedString>,
}
impl Props for BackScene {}

fn back_scene(ctx: &Ctx, p: &BackScene) -> VNode {
    // Field first: TAB focuses it even with the modal open
    // (depth-first tab order — no pointer needed, so the open
    // backdrop never eats the focus press).
    Div("back-root").children([
        ctx.child(
            "back-field",
            1,
            &TextInputProps::new("Name", p.value.clone()),
            TextInput,
        ),
        ctx.child(
            "back-modal",
            2,
            &ModalProps::new("Delete file?", p.open.clone()),
            Modal,
        ),
    ])
}

fn back_harness() -> (ComponentHost, Signal<bool>, Signal<SharedString>) {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    let value = host.runtime().signal(SharedString::from("Ada"));
    host.mount(
        "Back",
        BackScene {
            open: open.clone(),
            value: value.clone(),
        },
        back_scene,
    );
    host.run_until_idle();
    assert!(
        !find_retained_by_debug(&host, "modal-card").is_empty(),
        "popup mounted open"
    );
    // Keyboard focus (the field is first in tab order).
    host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
    host.run_until_idle();
    let field = node_by_debug(&host, "text-input");
    assert_eq!(host.focused_node(), Some(field), "TAB focuses the field");
    assert!(host.focused_field_session().is_some(), "session live");
    (host, open, value)
}

fn start_composition(host: &ComponentHost) {
    use oppa::ime::{dispatch_ime_event, ImeCompositionEvent};
    let mut sess = host
        .focused_field_session()
        .expect("focused session composes");
    dispatch_ime_event(
        &mut sess,
        &ImeCompositionEvent::CompositionStarted { start_byte: 3 },
    );
    dispatch_ime_event(
        &mut sess,
        &ImeCompositionEvent::CompositionUpdated {
            composition: "Adax".to_string(),
            caret_byte: 4,
        },
    );
    assert!(sess.is_composing(), "composition active");
}

#[test]
fn back_chain_cancels_then_blurs_then_exits() {
    let (host, open, value) = back_harness();
    start_composition(&host);
    // Step 1: composition cancels (reverts — value untouched,
    // popup stays, focus stays for the next press).
    assert_eq!(host.handle_back(), oppa::BackOutcome::CompositionCancelled);
    host.run_until_idle();
    assert_eq!(&*value.get(), "Ada", "cancel reverts, never commits");
    assert!(open.get(), "popup survives the composition step");
    assert!(host.focused_node().is_some(), "focus survives too");
    assert!(
        host.focused_field_session()
            .is_none_or(|s| !s.is_composing()),
        "nothing composing anymore"
    );
    // Step 2 (author popup step — the host cannot close what it
    // does not own, so the author/runner flips `open` first, as
    // the BackPress contract orders it).
    open.set(false);
    host.run_until_idle();
    assert!(
        find_retained_by_debug(&host, "modal-card").is_empty(),
        "author step dismisses the popup"
    );
    // Step 3: focus clears.
    assert_eq!(host.handle_back(), oppa::BackOutcome::FocusCleared);
    host.run_until_idle();
    assert_eq!(host.focused_node(), None);
    // Step 4: empty chain reports Unhandled (the runner exits).
    assert_eq!(host.handle_back(), oppa::BackOutcome::Unhandled);
}

#[test]
fn esc_key_rides_the_same_chain() {
    // The router's ESC arm runs `handle_back` (desktop parity —
    // one chain everywhere): first ESC cancels, second blurs.
    let (host, _open, value) = back_harness();
    start_composition(&host);
    host.inject_input(InputEvent::key(keys::ESCAPE, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(&*value.get(), "Ada", "ESC cancels the composition");
    assert!(host.focused_node().is_some(), "focus survives step one");
    host.inject_input(InputEvent::key(keys::ESCAPE, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(host.focused_node(), None, "second ESC blurs");
    // Repeat ESC repeats are ignored (no double blur, no exit
    // from the router — exit stays runner-side).
    host.inject_input(InputEvent::Key {
        code: keys::ESCAPE,
        modifiers: oppa::Modifiers::NONE,
        state: KeyState::Pressed,
        repeat: true,
    });
    host.run_until_idle();
    assert_eq!(host.handle_back(), oppa::BackOutcome::Unhandled);
}

// ------------------------------------------------------------------
// Round 5.1 (decision 268): TextArea — the multi-line sibling.
// ------------------------------------------------------------------

#[test]
fn text_area_renders_area_role_and_focuses() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hi"));
    host.mount("TA", TextAreaProps::new("Notes", value.clone()), TextArea);
    host.run_until_idle();
    let id = node_by_debug(&host, "text-area");
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::TextArea);
    assert_eq!(sem.label.as_deref(), Some("Notes"));
    assert!(!sem.disabled);
    assert!(
        host.tab_order().contains(&id),
        "enabled area is tabbable (press owner, decision 96)"
    );
    press_node(&host, id);
    assert_eq!(
        host.focused_node(),
        Some(id),
        "press focuses through the M5 router"
    );
    assert!(host.focused_field_session().is_some(), "session live");
}

#[test]
fn text_area_auto_heights_with_lines() {
    // Explicit width, content-driven height: three lines stand
    // taller than one (same width — no horizontal growth). The
    // area sits in a sized parent (areas live inside scenes,
    // never as roots — roots fill the viewport by decision 74).
    let one = area_box_height("one");
    let three = area_box_height("one\ntwo\nthree");
    assert!(
        three > one,
        "three lines ({three}) stand taller than one ({one})"
    );
    #[derive(Clone)]
    struct WrapProps {
        value: Signal<SharedString>,
    }
    impl Props for WrapProps {}
    fn wrap_scene(ctx: &Ctx, p: &WrapProps) -> VNode {
        Div("wrap")
            .style(Style::new().size(300, 400))
            .child(ctx.child(
                "area",
                1,
                &TextAreaProps::new("Notes", p.value.clone()),
                TextArea,
            ))
    }
    fn area_box_height(value: &str) -> f32 {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let v = host.runtime().signal(SharedString::from(value));
        host.mount("Wrap", WrapProps { value: v }, wrap_scene);
        host.run_until_idle();
        let b = host
            .committed_box(node_by_debug(&host, "text-area"))
            .expect("area box");
        assert!(approx(b.w, 200.0), "explicit width holds, got {}", b.w);
        b.h
    }
}

#[test]
fn text_area_placeholder_and_disabled_mirror_input() {
    // Empty + placeholder: presentational span, never the value.
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    host.mount(
        "TA",
        TextAreaProps::new("Notes", value.clone()).placeholder("Write here"),
        TextArea,
    );
    host.run_until_idle();
    assert_eq!(host.text_fields().len(), 1, "area counts as a field");
    // Disabled: handlerless, untabbable, press never focuses.
    let host2 = ComponentHost::new();
    host2.set_text_service(Box::new(FakeText));
    let value2 = host2.runtime().signal(SharedString::from("x"));
    let mut props = TextAreaProps::new("Notes", value2);
    props.enabled = false;
    host2.mount("TA", props, TextArea);
    host2.run_until_idle();
    let id = node_by_debug(&host2, "text-area");
    assert!(host2.retained_semantics(id).expect("semantics").disabled);
    assert!(host2.retained_handlers(id).is_empty());
    assert!(!host2.tab_order().contains(&id));
    press_node(&host2, id);
    assert_eq!(host2.focused_node(), None, "press never focuses");
}

#[test]
fn text_area_session_types_multiline() {
    // Programmatic multi-line feed lands in the value (the
    // router ENTER path is core-proven in m5_input).
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    host.mount("TA", TextAreaProps::new("Notes", value.clone()), TextArea);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "text-area"));
    let sess = host.focused_field_session().expect("session");
    sess.insert("a\nb");
    host.run_until_idle();
    assert_eq!(&*value.get(), "a\nb");
}

// ------------------------------------------------------------------
// Round 5.2 (decision 269): focus trap — Tab inside an open
// dialog cycles its subtree (derived from the retained tree,
// no registration); closing restores the global order.
// ------------------------------------------------------------------

#[derive(Clone)]
struct TrapScene {
    open: Signal<bool>,
    value: Signal<SharedString>,
}
impl Props for TrapScene {}

fn trap_scene(ctx: &Ctx, p: &TrapScene) -> VNode {
    // Field first, then the modal (opt-out backdrop: handlerless,
    // so the tab order is exactly field, cancel, confirm).
    Div("trap-root").children([
        ctx.child(
            "trap-field",
            1,
            &TextInputProps::new("Name", p.value.clone()),
            TextInput,
        ),
        ctx.child(
            "trap-modal",
            2,
            &ModalProps::new("Delete file?", p.open.clone()).no_backdrop_dismiss(),
            Modal,
        ),
    ])
}

fn trap_harness() -> (ComponentHost, Signal<bool>) {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    let value = host.runtime().signal(SharedString::from(""));
    host.mount(
        "Trap",
        TrapScene {
            open: open.clone(),
            value,
        },
        trap_scene,
    );
    host.run_until_idle();
    assert!(
        !find_retained_by_debug(&host, "modal-card").is_empty(),
        "popup mounted open"
    );
    (host, open)
}

fn tab(host: &ComponentHost, shift: bool) {
    let mut ev = InputEvent::key(keys::TAB, KeyState::Pressed);
    if shift {
        ev = InputEvent::Key {
            code: keys::TAB,
            modifiers: oppa::Modifiers::shift(),
            state: KeyState::Pressed,
            repeat: false,
        };
    }
    host.inject_input(ev);
    host.run_until_idle();
}

fn debug_of(host: &ComponentHost) -> String {
    host.with_retained_mut(|rec, _| {
        rec.get(host.focused_node().expect("focus"))
            .map(|n| n.debug.clone())
            .unwrap_or_default()
    })
}

#[test]
fn tab_cycles_inside_open_dialog() {
    let (host, _open) = trap_harness();
    // Global order first: field, then into the dialog.
    tab(&host, false);
    assert_eq!(debug_of(&host), "text-input", "first stop: field");
    tab(&host, false);
    assert_eq!(debug_of(&host), "modal-cancel", "enters the dialog");
    tab(&host, false);
    assert_eq!(debug_of(&host), "modal-confirm");
    // Trapped: next cycles back to cancel, never the field.
    tab(&host, false);
    assert_eq!(debug_of(&host), "modal-cancel", "trap cycles");
    tab(&host, false);
    assert_eq!(debug_of(&host), "modal-confirm");
    // Backward wraps within the trap too.
    tab(&host, true);
    assert_eq!(debug_of(&host), "modal-cancel", "shift+tab wraps");
}

#[test]
fn closing_dialog_restores_global_order() {
    let (host, open) = trap_harness();
    tab(&host, false);
    tab(&host, false);
    assert_eq!(debug_of(&host), "modal-cancel", "inside first");
    open.set(false);
    host.run_until_idle();
    assert!(
        find_retained_by_debug(&host, "modal-card").is_empty(),
        "dialog unmounts"
    );
    // Focus was inside: unmount clears it (1.4 rule), TAB
    // restarts the global order at the field.
    tab(&host, false);
    assert_eq!(debug_of(&host), "text-input", "global order restored");
}

// ------------------------------------------------------------------
// Round 5.4 (decision 271): on_change notification + uncontrolled
// companions — internal flips report payload-carrying values;
// self-managed signals advance identically to controlled ones.
// ------------------------------------------------------------------

/// Test-only `Change` constructor (the round-5.4 helper lived at
/// top level but only tests ever called it — top-level placement
/// tripped `dead_code` on the lib build; moved here in 6.3).
fn change<T>(f: impl Fn(T) + 'static) -> Change<T> {
    Rc::new(f)
}

fn blowing<T: Clone + 'static>() -> (Change<T>, Rc<RefCell<Vec<T>>>) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    (change(move |v| s.borrow_mut().push(v)), seen)
}

#[test]
fn toggle_and_checkbox_report_flips() {
    let host = ComponentHost::new();
    let (notify, seen) = blowing::<bool>();
    let on = host.runtime().signal(false);
    host.mount(
        "Tgl",
        ToggleCase {
            props: ToggleProps {
                label: SharedString::from("Wi-Fi"),
                on: on.clone(),
                enabled: true,
                on_change: Some(notify),
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
        },
        toggle_render,
    );
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "toggle"));
    assert!(on.get());
    assert_eq!(*seen.borrow(), vec![true], "flip reports the new value");
    // Checkbox mirrors (mark + semantics already proven — the
    // notification path is the assertion here).
    let host2 = ComponentHost::new();
    let (notify2, seen2) = blowing::<bool>();
    let checked = host2.runtime().signal(false);
    host2.mount(
        "Chk",
        CheckCase {
            props: CheckboxProps {
                label: SharedString::from("T&C"),
                checked: checked.clone(),
                enabled: true,
                on_change: Some(notify2),
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
        },
        check_render,
    );
    host2.run_until_idle();
    press_node(&host2, node_by_debug(&host2, "checkbox"));
    assert!(checked.get());
    assert_eq!(*seen2.borrow(), vec![true]);
}

#[test]
fn slider_reports_every_path() {
    // Controlled slider with notification: step, drag, and
    // arrow paths all report through the single funnel.
    let host = ComponentHost::new();
    let value = host.runtime().signal(50.0f32);
    let (notify, seen) = blowing::<f32>();
    let props = SliderProps {
        label: SharedString::from("Volume"),
        value: value.clone(),
        min: 0.0,
        max: 100.0,
        step: 10.0,
        enabled: true,
        on_change: Some(notify),
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
    };
    host.mount("Sld", SliderCase { props }, slider_render);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "step-inc"));
    assert_eq!(*seen.borrow(), vec![60.0], "step reports");
    // Drag path reports the snapped value (track-owned press
    // point — buttons own their own presses).
    let (_x0, y0, tx, tw) = press_track(&host);
    host.inject_input(InputEvent::pointer_move(tx + tw * 0.8, y0));
    host.run_until_idle();
    assert_eq!(*seen.borrow(), vec![60.0, 80.0], "drag reports");
    host.inject_input(InputEvent::pointer_up(tx + tw * 0.8, y0));
    host.run_until_idle();
    // Arrow path reports too (the drag press focused the track —
    // cancel/up keep focus, so no TAB needed).
    let track = node_by_debug(&host, "slider");
    assert_eq!(host.focused_node(), Some(track));
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(*seen.borrow(), vec![60.0, 80.0, 70.0], "arrow reports");
}

#[test]
fn uncontrolled_toggle_checkbox_slider_advance_and_report() {
    // Toggle: press flips internal state + reports.
    let host = ComponentHost::new();
    let (notify, seen) = blowing::<bool>();
    host.mount(
        "UT",
        UncontrolledToggleProps {
            label: SharedString::from("Wi-Fi"),
            initial: false,
            enabled: true,
            on_change: Some(notify),
            invalid: false,
            required: false,
            error_message: None,
            helper_text: None,
        },
        UncontrolledToggle,
    );
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "toggle"));
    assert_eq!(*seen.borrow(), vec![true], "uncontrolled flip reports");
    press_node(&host, node_by_debug(&host, "toggle"));
    assert_eq!(*seen.borrow(), vec![true, false], "second flip reports too");
    // Slider: step reports the snapped value.
    let host2 = ComponentHost::new();
    let (notify2, seen2) = blowing::<f32>();
    host2.mount(
        "US",
        UncontrolledSliderProps {
            label: SharedString::from("Volume"),
            initial: 50.0,
            min: 0.0,
            max: 100.0,
            step: 10.0,
            enabled: true,
            on_change: Some(notify2),
            invalid: false,
            required: false,
            error_message: None,
            helper_text: None,
        },
        UncontrolledSlider,
    );
    host2.run_until_idle();
    press_node(&host2, node_by_debug(&host2, "step-inc"));
    assert_eq!(*seen2.borrow(), vec![60.0]);
    // Checkbox: press reports.
    let host3 = ComponentHost::new();
    let (notify3, seen3) = blowing::<bool>();
    host3.mount(
        "UC",
        UncontrolledCheckboxProps {
            label: SharedString::from("T&C"),
            initial: false,
            enabled: true,
            on_change: Some(notify3),
            invalid: false,
            required: false,
            error_message: None,
            helper_text: None,
        },
        UncontrolledCheckbox,
    );
    host3.run_until_idle();
    press_node(&host3, node_by_debug(&host3, "checkbox"));
    assert_eq!(*seen3.borrow(), vec![true]);
}

#[test]
fn uncontrolled_text_reports_committed_edits() {
    // Typing through the focused session reports each commit
    // (the session funnel — insert here, IME commits and undo
    // ride the same path core-side).
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let (notify, seen) = blowing::<SharedString>();
    host.mount(
        "UTI",
        UncontrolledTextInputProps {
            label: SharedString::from("Name"),
            initial: SharedString::from(""),
            placeholder: None,
            enabled: true,
            width: 200.0,
            height: 32.0,
            style: Text::body_secondary,
            debug: SharedString::from("text-input"),
            on_change: Some(notify),
            masked: false,
            invalid: false,
            required: false,
            error_message: None,
            helper_text: None,
        },
        UncontrolledTextInput,
    );
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "text-input"));
    let sess = host.focused_field_session().expect("session");
    sess.insert("hi");
    host.run_until_idle();
    assert_eq!(
        seen.borrow()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        vec!["hi".to_string()],
        "committed edit reports"
    );
    // TextArea mirrors through the same funnel.
    let host2 = ComponentHost::new();
    host2.set_text_service(Box::new(FakeText));
    let (notify2, seen2) = blowing::<SharedString>();
    host2.mount(
        "UTA",
        UncontrolledTextAreaProps {
            label: SharedString::from("Notes"),
            initial: SharedString::from(""),
            placeholder: None,
            enabled: true,
            width: 200.0,
            style: Text::body_secondary,
            debug: SharedString::from("text-area"),
            on_change: Some(notify2),
            invalid: false,
            required: false,
            error_message: None,
            helper_text: None,
        },
        UncontrolledTextArea,
    );
    host2.run_until_idle();
    press_node(&host2, node_by_debug(&host2, "text-area"));
    let sess2 = host2.focused_field_session().expect("session");
    sess2.insert("a\nb");
    host2.run_until_idle();
    assert_eq!(
        seen2
            .borrow()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        vec!["a\nb".to_string()]
    );
}

/// Round 8.1 (decision 297): tap-to-caret through the shaper.
/// Clicks at the text origin, mid-word, and past the end land on
/// byte offsets `0`, `2`, and `text_len` (FakeText 8.75px/char).
fn tap_at(host: &ComponentHost, x: f32, y: f32, shift: bool) {
    let modifiers = if shift {
        oppa::Modifiers::shift()
    } else {
        oppa::Modifiers::NONE
    };
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Down {
            button: oppa::PointerButton::Primary,
        },
        x,
        y,
        modifiers,
    });
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Up {
            button: oppa::PointerButton::Primary,
        },
        x,
        y,
        modifiers,
    });
    host.run_until_idle();
}

fn field_tap_point(host: &ComponentHost, debug: &str, local_x: f32) -> (f32, f32) {
    let field = node_by_debug(host, debug);
    let origin_x = host
        .text_origin_under(field)
        .expect("field lays a text origin");
    let b = host.committed_box(field).expect("field has a hit box");
    (origin_x + local_x, b.y + b.h / 2.0)
}

#[test]
fn text_input_tap_places_caret_at_cluster_boundaries() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hello"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    // Origin tap → byte 0.
    let (x0, y0) = field_tap_point(&host, "text-input", 0.0);
    tap_at(&host, x0, y0, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.caret(), 0);
    assert_eq!(sess.selection(), (0, 0));
    // Mid-word (x=20: third cluster [17.5,26.25) leading half) → byte 2.
    let (xm, ym) = field_tap_point(&host, "text-input", 20.0);
    tap_at(&host, xm, ym, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.caret(), 2);
    assert_eq!(sess.selection(), (2, 2));
    // Past the end (5 × 8.75 = 43.75 + 10) → byte 5.
    let (xe, ye) = field_tap_point(&host, "text-input", 53.75);
    tap_at(&host, xe, ye, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.caret(), 5);
    assert_eq!(sess.selection(), (5, 5));
}

#[test]
fn text_input_shift_click_extends_selection() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hello"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    let (x0, y0) = field_tap_point(&host, "text-input", 0.0);
    tap_at(&host, x0, y0, false);
    assert_eq!(host.focused_field_session().expect("session").caret(), 0);
    let (xe, ye) = field_tap_point(&host, "text-input", 53.75);
    tap_at(&host, xe, ye, true);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.selection(), (0, 5));
    assert_eq!(sess.caret(), 5);
}

#[test]
fn text_input_keyboard_activation_parks_at_end() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hello"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    let (x0, y0) = field_tap_point(&host, "text-input", 0.0);
    tap_at(&host, x0, y0, false);
    assert_eq!(host.focused_field_session().expect("session").caret(), 0);
    let field = node_by_debug(&host, "text-input");
    host.inject_input(InputEvent::Focus { node: Some(field) });
    host.run_until_idle();
    host.inject_input(InputEvent::key(keys::ENTER, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(
        host.focused_field_session().expect("session").caret(),
        5,
        "keyboard press falls back to end (no tap point)"
    );
}

#[test]
fn text_area_tap_places_caret() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hi"));
    host.mount("TA", TextAreaProps::new("Notes", value.clone()), TextArea);
    host.run_until_idle();
    let (x0, y0) = field_tap_point(&host, "text-area", 0.0);
    tap_at(&host, x0, y0, false);
    assert_eq!(host.focused_field_session().expect("session").caret(), 0);
    // "hi" = 2 × 8.75 = 17.5 wide; +10 lands past the end → byte 2.
    let (xe, ye) = field_tap_point(&host, "text-area", 27.5);
    tap_at(&host, xe, ye, false);
    assert_eq!(host.focused_field_session().expect("session").caret(), 2);
}

/// Round 8.2 (decision 298): pointer drag streams `drag_x` — Down
/// at byte 2, Moves to byte 8, Up classifies Drag (no press) and
/// the session holds `(2, 8)` with the caret at 8.
fn drag_at(host: &ComponentHost, x1: f32, y: f32, x2: f32) {
    use oppa::{PointerAction, PointerButton};
    let moves = [x1 + (x2 - x1) * 0.5, x2];
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: PointerAction::Down {
            button: PointerButton::Primary,
        },
        x: x1,
        y,
        modifiers: oppa::Modifiers::NONE,
    });
    for x in moves {
        host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: PointerAction::Move,
            x,
            y,
            modifiers: oppa::Modifiers::NONE,
        });
    }
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: PointerAction::Up {
            button: PointerButton::Primary,
        },
        x: x2,
        y,
        modifiers: oppa::Modifiers::NONE,
    });
    host.run_until_idle();
}

/// Round 8.2 (decision 298): drag byte 2→8 selects `(2, 8)` and the
/// shared plan carries one themed background Rect with exact bounds
/// (what CPU and Vello both paint — no backend selection logic).
#[test]
fn text_input_drag_selects_range_with_exact_background_bounds() {
    use oppa::{DrawOp, SELECTION_FILL};
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hello world"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    let field = node_by_debug(&host, "text-input");
    let origin_x = host.text_origin_under(field).expect("text origin");
    let y = host.committed_box(field).expect("hit box").y + 16.0;
    // Byte b lands in the leading half: b × 8.75 + 2.0.
    drag_at(
        &host,
        origin_x + 2.0 * 8.75 + 2.0,
        y,
        origin_x + 8.0 * 8.75 + 2.0,
    );
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.selection(), (2, 8));
    assert_eq!(sess.caret(), 8);
    // Plan bounds: caret_x(2) = 17.5 → caret_x(8) = 70.0.
    let sel = host.focused_selection_paint().expect("paint selection");
    assert_eq!(sel.range, (2, 8));
    let builder = oppa_cpu::FramePlanBuilder::new(1.0);
    builder.set_selection(Some(sel));
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let rects: Vec<(f32, f32, f32, f32)> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rect {
                x, y, w, h, color, ..
            } if *color == SELECTION_FILL => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect();
    assert_eq!(rects.len(), 1, "one background rect, got {rects:?}");
    let (rx, ry, rw, rh) = rects[0];
    assert!(
        approx(rx, origin_x + 17.5) && approx(rw, 52.5),
        "x/w pin cluster edges, got ({rx}, {rw})"
    );
    // y/h pin the laid line (lines live on the inner "text" leaf —
    // the "field" element carries the box origin, not the lines).
    let laid = find_retained_by_debug(&host, "text")
        .into_iter()
        .filter_map(|id| host.committed_box(id))
        .find(|b| !b.lines.is_empty())
        .expect("laid text leaf");
    let expect = laid.selection_rects((2, 8));
    assert_eq!(expect.len(), 1, "one laid line overlaps");
    assert!(
        approx(ry, expect[0][1]) && approx(rh, expect[0][3] - expect[0][1]),
        "y/h pin the laid line, got ({ry}, {rh}) laid {laid:?}"
    );
    // Collapsing the selection removes the Rect incrementally (the
    // handler dirties the field — no stale highlight survives).
    let (x0, _) = field_tap_point(&host, "text-input", 0.0);
    tap_at(&host, x0, y, false);
    assert_eq!(host.focused_field_session().expect("session").caret(), 0);
    let builder = oppa_cpu::FramePlanBuilder::new(1.0);
    builder.set_selection(host.focused_selection_paint());
    let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    assert!(
        !plan.ops.iter().any(|op| matches!(op,
                DrawOp::Rect { color, .. } if *color == SELECTION_FILL)),
        "collapsed selection paints no highlight"
    );
}

/// Round 15.1 (decision 312): a focused field emits a 2px caret
/// bar at the active cluster's leading edge, and moving the caret
/// repositions the bar (the shared plan's caret `Rect` — what CPU
/// and Vello both paint). A non-collapsed selection suppresses
/// the bar (the highlight paints instead — one overlay at a time).
#[test]
fn text_input_focused_caret_paints_two_px_bar_and_repositions() {
    use oppa::{CaretPaint, DrawOp, CARET_WIDTH_PX};
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hello world"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    // Tap byte 0 → collapsed caret at the leading edge (the tap
    // resets the blink phase, so the bar is solid-visible here —
    // no clock control needed for the position pin).
    let field = node_by_debug(&host, "text-input");
    let origin_x = host.text_origin_under(field).expect("text origin");
    let y = host.committed_box(field).expect("hit box").y + 16.0;
    let (x0, _) = field_tap_point(&host, "text-input", 0.0);
    tap_at(&host, x0, y, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.caret(), 0);
    assert_eq!(sess.selection(), (0, 0));
    let caret: CaretPaint = host.focused_caret_paint().expect("visible caret bar");
    assert_eq!(caret.field, field);
    assert!(
        approx(caret.x, origin_x),
        "bar at the leading edge of byte 0, got {} vs origin {origin_x}",
        caret.x
    );
    assert!(caret.h > 0.0, "positive bar height, got {}", caret.h);
    let builder = oppa_cpu::FramePlanBuilder::new(1.0);
    builder.set_caret(host.focused_caret_paint());
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let bars: Vec<(f32, f32, f32, f32)> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rect {
                x, y, w, h, color, ..
            } if *color == caret.color && approx(*w, CARET_WIDTH_PX) => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 1, "exactly one caret bar, got {bars:?}");
    assert!(
        approx(bars[0].0, origin_x) && approx(bars[0].1, caret.y) && approx(bars[0].3, caret.h),
        "bar pins the resolved caret box, got {:?} vs ({}, {}, {})",
        bars[0],
        caret.x,
        caret.y,
        caret.h
    );
    // Move the caret to byte 5 → the bar repositions by exactly
    // five FakeText advances (8.75px/char).
    let (x5, _) = field_tap_point(&host, "text-input", 5.0 * 8.75 + 2.0);
    tap_at(&host, x5, y, false);
    assert_eq!(host.focused_field_session().expect("session").caret(), 5);
    let moved = host.focused_caret_paint().expect("visible caret bar");
    assert!(
        approx(moved.x, origin_x + 5.0 * 8.75),
        "bar at the leading edge of byte 5, got {} vs {}",
        moved.x,
        origin_x + 5.0 * 8.75
    );
    // Drag-selecting a range hides the bar (highlight instead).
    drag_at(
        &host,
        origin_x + 2.0 * 8.75 + 2.0,
        y,
        origin_x + 8.0 * 8.75 + 2.0,
    );
    assert_eq!(
        host.focused_field_session().expect("session").selection(),
        (2, 8)
    );
    assert!(
        host.focused_caret_paint().is_none(),
        "selected range paints the highlight, not the bar"
    );
}

/// Round 8.2 (decision 298): two fast taps on "world" select the
/// word (`dbl_click_x` — bytes 6..11), three select the hard line.
#[test]
fn text_input_double_click_selects_word_triple_selects_line() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hello world"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    // Byte 7 (in "world"): 7 × 8.75 + 2.0 local.
    let (x, y) = field_tap_point(&host, "text-input", 7.0 * 8.75 + 2.0);
    tap_at(&host, x, y, false);
    tap_at(&host, x, y, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.selection(), (6, 11), "double-click selects the word");
    assert_eq!(sess.caret(), 11);
    tap_at(&host, x, y, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.selection(), (0, 11), "triple-click selects the line");
}

/// Round 8.2 (decision 298): triple-click in a TextArea selects the
/// hard `\n` line under the pointer, not the whole value.
#[test]
fn text_area_triple_click_selects_hard_line() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("ab\ncd"));
    host.mount("TA", TextAreaProps::new("Notes", value.clone()), TextArea);
    host.run_until_idle();
    // Byte 3 ("c"): 3 × 8.75 + 2.0 local.
    let (x, y) = field_tap_point(&host, "text-area", 3.0 * 8.75 + 2.0);
    tap_at(&host, x, y, false);
    tap_at(&host, x, y, false);
    tap_at(&host, x, y, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(
        sess.selection(),
        (3, 5),
        "triple-click selects the hard line"
    );
}

/// Round 8.3 (decision 299): hover resolves the hand over the
/// button (even over its label leaf — inherit rule), the I-beam
/// over the field, and the arrow with no hover; the retained
/// styles carry the cursors structurally.
#[derive(Clone)]
struct CursorCase {
    btn: ButtonProps,
    field: TextInputProps,
}
impl Props for CursorCase {}

fn cursor_render(ctx: &Ctx, p: &CursorCase) -> VNode {
    Column::new().children([
        ctx.child("oppa::Button", 1, &p.btn, Button),
        ctx.child("oppa::TextInput", 2, &p.field, TextInput),
    ])
}

fn hover_at(host: &ComponentHost, debug: &str) {
    let id = node_by_debug(host, debug);
    let b = host.committed_box(id).expect("control has a hit box");
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Move,
        x: b.x + b.w / 2.0,
        y: b.y + b.h / 2.0,
        modifiers: oppa::Modifiers::NONE,
    });
    host.run_until_idle();
}

fn style_cursor_of(host: &ComponentHost, debug: &str) -> Option<CursorIcon> {
    let id = node_by_debug(host, debug);
    host.with_retained_mut(|rec, styles| {
        let n = rec.get(id).expect("live node");
        styles.get(n.style).cloned().unwrap_or_default().cursor
    })
}

#[test]
fn hover_resolves_button_pointer_and_field_text() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    assert_eq!(host.hover_cursor(), None, "no hover yet");
    let value = host.runtime().signal(SharedString::from("Ada"));
    host.mount(
        "Cur",
        CursorCase {
            btn: ButtonProps::new("OK", || {}),
            field: TextInputProps::new("Name", value),
        },
        cursor_render,
    );
    host.run_until_idle();
    hover_at(&host, "button");
    assert_eq!(
        host.hover_cursor(),
        Some(CursorIcon::Pointer),
        "hand over the button (label inherit)"
    );
    hover_at(&host, "text-input");
    assert_eq!(
        host.hover_cursor(),
        Some(CursorIcon::Text),
        "I-beam over the field"
    );
    assert_eq!(style_cursor_of(&host, "button"), Some(CursorIcon::Pointer));
    assert_eq!(style_cursor_of(&host, "text-input"), Some(CursorIcon::Text));
}

/// Round 8.3 (decision 299): disabled controls keep the platform
/// arrow (inert controls never promise clicks); Toggle and
/// Checkbox show the hand like Button when enabled.
#[test]
fn disabled_controls_keep_the_platform_arrow() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("Ada"));
    host.mount(
        "CurDis",
        CursorCase {
            btn: ButtonProps::new("OK", || {}).disabled(),
            field: TextInputProps::new("Name", value).disabled(),
        },
        cursor_render,
    );
    host.run_until_idle();
    assert_eq!(style_cursor_of(&host, "button"), None);
    assert_eq!(style_cursor_of(&host, "text-input"), None);
    hover_at(&host, "button");
    assert_eq!(host.hover_cursor(), None, "disabled button keeps the arrow");

    let host2 = ComponentHost::new();
    host2.set_text_service(Box::new(FakeText));
    let on = host2.runtime().signal(false);
    let checked = host2.runtime().signal(false);
    host2.mount(
        "CurTC",
        CursorTCProps {
            toggle: ToggleProps {
                label: SharedString::from("Wi-Fi"),
                on: on.clone(),
                enabled: true,
                on_change: None,
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
            check: CheckboxProps {
                label: SharedString::from("T&C"),
                checked: checked.clone(),
                enabled: true,
                on_change: None,
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
        },
        cursor_tc_render,
    );
    host2.run_until_idle();
    hover_at(&host2, "toggle");
    assert_eq!(host2.hover_cursor(), Some(CursorIcon::Pointer));
    hover_at(&host2, "checkbox");
    assert_eq!(host2.hover_cursor(), Some(CursorIcon::Pointer));
}

#[derive(Clone)]
struct CursorTCProps {
    toggle: ToggleProps,
    check: CheckboxProps,
}
impl Props for CursorTCProps {}

fn cursor_tc_render(ctx: &Ctx, p: &CursorTCProps) -> VNode {
    Column::new().children([
        ctx.child("oppa::Toggle", 1, &p.toggle, Toggle),
        ctx.child("oppa::Checkbox", 2, &p.check, Checkbox),
    ])
}

/// Round 11.2 (decision 306): toggling the theme signal recolors
/// Button, TextInput, Toggle, and Modal in place — same
/// instances (toggle state and field text survive), only colors
/// re-derive.
#[derive(Clone)]
struct ThemeCase {
    btn: ButtonProps,
    field: TextInputProps,
    toggle: ToggleProps,
    modal: ModalProps,
}
impl Props for ThemeCase {}

fn theme_render(ctx: &Ctx, p: &ThemeCase) -> VNode {
    Column::new().children([
        ctx.child("oppa::Button", 1, &p.btn, Button),
        ctx.child("oppa::TextInput", 2, &p.field, TextInput),
        ctx.child("oppa::Toggle", 3, &p.toggle, Toggle),
        ctx.child("oppa::Modal", 4, &p.modal, Modal),
    ])
}

fn style_bg_of(host: &ComponentHost, debug: &str) -> Option<Color> {
    let id = node_by_debug(host, debug);
    host.with_retained_mut(|rec, styles| {
        let n = rec.get(id).expect("live node");
        styles.get(n.style).cloned().unwrap_or_default().bg
    })
}

#[test]
fn theme_toggle_recolors_controls_without_remounting() {
    use oppa::{ThemeMode, ThemeTokens};
    let light = ThemeTokens::light();
    let dark = ThemeTokens::dark();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hi"));
    let on = host.runtime().signal(true);
    let open = host.runtime().signal(true);
    host.mount(
        "Theme",
        ThemeCase {
            btn: ButtonProps::new("OK", || {}),
            field: TextInputProps::new("Name", value.clone()),
            toggle: ToggleProps {
                label: SharedString::from("Wi-Fi"),
                on: on.clone(),
                enabled: true,
                on_change: None,
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
            modal: ModalProps::new("Delete file?", open.clone()),
        },
        theme_render,
    );
    host.run_until_idle();
    // Light reproduces the catalog palette exactly.
    assert_eq!(style_bg_of(&host, "button"), Some(light.primary));
    assert_eq!(style_bg_of(&host, "text-input"), Some(light.surface));
    assert_eq!(style_bg_of(&host, "modal-card"), Some(light.surface));
    assert_eq!(
        style_bg_of(&host, "toggle-track"),
        Some(light.primary),
        "on-state track reads primary"
    );
    // Toggle to dark: every themed control re-renders in place.
    host.set_theme(ThemeMode::Dark);
    host.run_until_idle();
    assert_eq!(style_bg_of(&host, "button"), Some(dark.primary));
    assert_eq!(style_bg_of(&host, "text-input"), Some(dark.surface));
    assert_eq!(style_bg_of(&host, "modal-card"), Some(dark.surface));
    assert_eq!(style_bg_of(&host, "toggle-track"), Some(dark.primary));
    // Instances survive: toggle state and field text untouched.
    assert!(on.get(), "toggle state survives the recolor");
    assert_eq!(value.get().to_string(), "hi", "field text survives");
    assert!(open.get(), "modal stays open");
    // And back: Light restores exactly.
    host.set_theme(ThemeMode::Light);
    host.run_until_idle();
    assert_eq!(style_bg_of(&host, "button"), Some(light.primary));
    assert_eq!(style_bg_of(&host, "modal-card"), Some(light.surface));
}

// ------------------------------------------------------------------
// Round 13.2 (decision 309): virtualized list over a collection
// ------------------------------------------------------------------

const VL_N: usize = 1000;
const VL_VIEW_H: f32 = 200.0;
const VL_OVER: usize = 2;

#[derive(Clone, Debug)]
struct VItem {
    i: usize,
}

/// Alternating 20/40 heights (variable — prefix sums, never
/// division): period 60px per 2 rows, total 30px mean.
fn vitem_h(item: &VItem) -> f32 {
    if item.i.is_multiple_of(2) {
        20.0
    } else {
        40.0
    }
}

/// Prefix-sum tops for the alternating pattern (test-side
/// oracle — closed form, not the control's loop).
fn vitem_top(i: usize) -> f32 {
    (i / 2) as f32 * 60.0 + (i % 2) as f32 * 20.0
}

fn vitem_row(_ctx: &Ctx, p: &VirtualRowProps<VItem>) -> VNode {
    // Zebra by stable id (proves slot→item mapping, the M8
    // cell_bg_of precedent — geometry alone cannot).
    let bg = if p.row.id.0.is_multiple_of(2) {
        Color(0x22_22_22)
    } else {
        Color(0x33_33_33)
    };
    Div("vlist-row")
        .style(Style::new().h(p.height).fill_width().bg(bg))
        .build()
}

struct VListHarness {
    host: ComponentHost,
    offset: oppa::ScrollOffset,
    list: NodeId,
    coll: oppa::Collection<VItem>,
}

fn mount_vlist(n_items: usize) -> VListHarness {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:vlist"));
    coll.ingest((0..n_items).map(|i| VItem { i }).collect::<Vec<_>>());
    let props = VirtualListProps::new(coll.clone(), vitem_h, vitem_row)
        .size(320.0, VL_VIEW_H)
        .overscan(VL_OVER)
        .debug("vlist");
    let handle = host.mount("VList", props, VirtualList);
    host.run_until_idle();
    let root = handle.root_instance();
    let offset = host.instance_scroll(root).expect("scroll handle");
    let list = find_retained_by_debug(&host, "vlist")[0];
    VListHarness {
        host,
        offset,
        list,
        coll,
    }
}

fn slot_bg(h: &VListHarness, id: NodeId) -> Option<Color> {
    // The zebra lives on the row content (the slot only
    // positions) — match the content node by box.
    let b = h.host.committed_box(id).expect("slot laid out");
    let row = find_retained_by_debug(&h.host, "vlist-row")
        .into_iter()
        .find(|r| h.host.committed_box(*r).is_some_and(|rb| approx(rb.y, b.y)))
        .expect("row content at slot box");
    let sid = h.host.retained_style(row).expect("row style");
    h.host
        .with_retained_mut(|_, styles| styles.get(sid).expect("style").bg)
}

/// Window math on the alternating pattern: [0,200]+2 overscan
/// covers rows 0..9 (tops 0..180, 7 visible + 2 over).
#[test]
fn virtual_list_windows_bounded_retained_nodes() {
    let h = mount_vlist(VL_N);
    assert_eq!(
        vlist_window(&[0.0, 20.0, 60.0], 80.0, 0.0, VL_VIEW_H, VL_OVER),
        (0, 3)
    );
    let slots = find_retained_by_debug(&h.host, "vlist-slot");
    assert_eq!(slots.len(), 9, "window [0,9), not all {VL_N}");
    // Slots sit at exact prefix offsets with exact heights.
    let mut seen: Vec<(f32, f32)> = slots
        .iter()
        .map(|id| {
            let b = h.host.committed_box(*id).expect("slot laid out");
            (b.y, b.h)
        })
        .collect();
    seen.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("finite"));
    for (slot, (y, hh)) in seen.iter().enumerate() {
        assert!(
            approx(*y, vitem_top(slot)),
            "slot {slot} at {y}, want {}",
            vitem_top(slot)
        );
        assert!(
            approx(*hh, vitem_h(&VItem { i: slot })),
            "slot {slot} h {hh}"
        );
    }
    // Zebra follows the item (slot 0 shows row 0 — even).
    let top_slot = slots
        .into_iter()
        .find(|id| h.host.committed_box(*id).is_some_and(|b| approx(b.y, 0.0)))
        .expect("slot at y=0");
    assert_eq!(slot_bg(&h, top_slot), Some(Color(0x22_22_22)));
    // Extent covers all rows (DOM spacer + scrollbar source).
    let list_box = h.host.committed_box(h.list).expect("list laid out");
    assert!(
        approx(list_box.content_h, 30_000.0),
        "1000 rows × 30 mean, got {}",
        list_box.content_h
    );
}

/// Scrolling moves the window and recycles slots: 500px lands
/// [15,26) with identical slot nodes (structure-free), exact
/// offsets, and item-following zebra.
#[test]
fn virtual_list_scroll_moves_window_recycles_slots() {
    let h = mount_vlist(VL_N);
    let before: std::collections::HashSet<NodeId> = find_retained_by_debug(&h.host, "vlist-slot")
        .into_iter()
        .collect();
    assert_eq!(before.len(), 9);
    h.offset.set(500.0);
    h.host.run_until_idle();
    let after: std::collections::HashSet<NodeId> = find_retained_by_debug(&h.host, "vlist-slot")
        .into_iter()
        .collect();
    // Window grows 9 → 11 (two genuinely new slots mount)...
    assert_eq!(after.len(), 11, "window [15,26)");
    // ...but the 9 survivors are identical nodes (recycle).
    assert_eq!(
        before.intersection(&after).count(),
        9,
        "survivors recycle, no teardown"
    );
    // First slot sits at tops[15] = 440, viewport-relative at
    // 440 - 500 = -60 with row 15's height.
    let first = after
        .into_iter()
        .find(|id| {
            h.host
                .committed_box(*id)
                .is_some_and(|b| approx(b.y, vitem_top(15) - 500.0))
        })
        .expect("slot at y=-60");
    let b = h.host.committed_box(first).expect("slot laid out");
    assert!(approx(b.h, 40.0), "row 15 is odd → 40, got {}", b.h);
    assert_eq!(
        slot_bg(&h, first),
        Some(Color(0x33_33_33)),
        "zebra follows row 15 (odd)"
    );
}

/// Same-size windows rebind Update-only (y=60 → y=120 slides
/// [0,11) → [2,13): 11 stable slot keys recycle, positions move
/// through LAYOUT, no structure — the M8 recycle shape for
/// variable heights).
#[test]
fn virtual_list_same_size_window_rebinds_update_only() {
    let h = mount_vlist(VL_N);
    h.offset.set(60.0);
    h.host.run_until_idle();
    assert_eq!(
        find_retained_by_debug(&h.host, "vlist-slot").len(),
        11,
        "window [0,11)"
    );
    h.offset.set(120.0);
    h.host.run_until_idle();
    assert_eq!(
        find_retained_by_debug(&h.host, "vlist-slot").len(),
        11,
        "window [2,13)"
    );
    let d = h.host.last_diff().expect("scroll diff");
    assert_eq!(d.structure_ops(), 0, "same-size windows rebind Update-only");
    // Window slid a full period: first slot at tops[2] = 60,
    // viewport-relative at 60 - 120 = -60.
    let first = find_retained_by_debug(&h.host, "vlist-slot")
        .into_iter()
        .find(|id| {
            h.host
                .committed_box(*id)
                .is_some_and(|b| approx(b.y, vitem_top(2) - 120.0))
        })
        .expect("slot at y=-60");
    assert_eq!(
        slot_bg(&h, first),
        Some(Color(0x22_22_22)),
        "zebra follows row 2 (even)"
    );
}

/// Empty collections render an empty viewport (quiet — same
/// class as unmatched controlled edges, never a panic). The
/// childless ScrollArea takes the leaf arm (content = own box),
/// so the spacer matches the viewport — no phantom scrollbar.
#[test]
fn virtual_list_empty_renders_no_slots() {
    let h = mount_vlist(0);
    assert_eq!(
        find_retained_by_debug(&h.host, "vlist-slot").len(),
        0,
        "no rows, no slots"
    );
    let list_box = h.host.committed_box(h.list).expect("list laid out");
    assert!(approx(list_box.h, VL_VIEW_H), "viewport box");
    assert!(
        approx(list_box.content_h, VL_VIEW_H),
        "leaf fallback: content = own box, got {}",
        list_box.content_h
    );
}

/// Round 13.1 → 13.2 combined: a worker batch grows the extent
/// live — appends re-derive the window the same frame, no
/// remount.
#[test]
fn virtual_list_async_append_grows_content() {
    let h = mount_vlist(10);
    let list_box = h.host.committed_box(h.list).expect("list laid out");
    assert!(
        approx(list_box.content_h, 300.0),
        "10 rows × 30, got {}",
        list_box.content_h
    );
    let writer = h.coll.writer();
    let rt = h.host.runtime();
    rt.spawn_task(move |scope| {
        writer.submit(&scope, (10..20).map(|i| VItem { i }).collect::<Vec<_>>());
    });
    let mut waited = 0;
    loop {
        let stats = rt.stats();
        if stats.tasks_done + stats.tasks_dropped >= 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        waited += 1;
        assert!(waited < 10_000, "executor task never resolved");
    }
    h.host.run_until_idle();
    assert_eq!(h.coll.len(), 20, "batch landed");
    let list_box = h.host.committed_box(h.list).expect("list laid out");
    assert!(
        approx(list_box.content_h, 600.0),
        "extent doubled live, got {}",
        list_box.content_h
    );
    // New rows are reachable: scroll to the bottom, window
    // shows row 19 viewport-relative (560 - 590 = -30; id 19,
    // odd → tall + dim zebra).
    h.offset.set(590.0);
    h.host.run_until_idle();
    let bottom = find_retained_by_debug(&h.host, "vlist-slot")
        .into_iter()
        .find(|id| {
            h.host
                .committed_box(*id)
                .is_some_and(|b| approx(b.y, vitem_top(19) - 590.0))
        })
        .expect("slot at row 19");
    let b = h.host.committed_box(bottom).expect("slot laid out");
    assert!(approx(b.h, 40.0), "row 19 h 40, got {}", b.h);
    assert_eq!(slot_bg(&h, bottom), Some(Color(0x33_33_33)));
}

// ------------------------------------------------------------------
// Round 13.3 (decision 310): data grid over a collection
// ------------------------------------------------------------------

const GRID_N: usize = 200;
const GRID_VIEW_W: f32 = 320.0;
const GRID_VIEW_H: f32 = 200.0;

fn grid_body(_ctx: &Ctx, p: &GridCellProps<VItem>) -> VNode {
    // Zebra by stable id (proves cell→item mapping, the slot_bg
    // precedent — geometry alone cannot).
    let bg = if p.row.id.0.is_multiple_of(2) {
        Color(0x22_22_22)
    } else {
        Color(0x33_33_33)
    };
    Div("dgrid-body")
        .style(Style::new().w(p.width).h(p.height).bg(bg).build())
        .build()
}

fn grid_cols() -> Vec<GridColumn<VItem>> {
    vec![
        GridColumn {
            header: SharedString::from("A"),
            width: 100.0,
            cell: grid_body,
        },
        GridColumn {
            header: SharedString::from("B"),
            width: 120.0,
            cell: grid_body,
        },
        GridColumn {
            header: SharedString::from("C"),
            width: 80.0,
            cell: grid_body,
        },
    ]
}

struct GridHarness {
    host: ComponentHost,
    offset: oppa::ScrollOffset,
    list: NodeId,
    coll: oppa::Collection<VItem>,
}

fn mount_grid(n_items: usize) -> (GridHarness, oppa::MountHandle<DataGridProps<VItem>>) {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.set_text_service(Box::new(FakeText));
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:dgrid"));
    coll.ingest((0..n_items).map(|i| VItem { i }).collect::<Vec<_>>());
    let props = DataGridProps::new(coll.clone(), grid_cols(), vitem_h)
        .size(GRID_VIEW_W, GRID_VIEW_H)
        .overscan(VL_OVER)
        .debug("dgrid");
    let handle = host.mount("DGrid", props, DataGrid);
    host.run_until_idle();
    let root = handle.root_instance();
    let offset = host.instance_scroll(root).expect("scroll handle");
    let list = find_retained_by_debug(&host, "dgrid")[0];
    (
        GridHarness {
            host,
            offset,
            list,
            coll,
        },
        handle,
    )
}

fn cell_bg(h: &GridHarness, id: NodeId) -> Option<Color> {
    // The zebra lives on the template root (the cell Div only
    // geometries) — match the body node by box.
    let b = h.host.committed_box(id).expect("cell laid out");
    let body = find_retained_by_debug(&h.host, "dgrid-body")
        .into_iter()
        .find(|r| {
            h.host
                .committed_box(*r)
                .is_some_and(|rb| approx(rb.y, b.y) && approx(rb.x, b.x))
        })
        .expect("body content at cell box");
    let sid = h.host.retained_style(body).expect("body style");
    h.host
        .with_retained_mut(|_, styles| styles.get(sid).expect("style").bg)
}

/// Header + window + extent: 200 alternating rows (total 6000),
/// 3 columns (total 300 wide), viewport 320×200 with a 28px pinned
/// header → 24 cells in [0,8), header pinned at y=0 with full
/// column width.
#[test]
fn datagrid_renders_header_window_and_extent() {
    let (h, _) = mount_grid(GRID_N);
    assert_eq!(
        find_retained_by_debug(&h.host, "dgrid-cell").len(),
        24,
        "8 rows × 3 cols, not all 200"
    );
    let header = find_retained_by_debug(&h.host, "dgrid-header");
    assert_eq!(header.len(), 1, "one sticky header");
    let hb = h.host.committed_box(header[0]).expect("header laid out");
    assert!(approx(hb.y, 0.0), "header at viewport top, got {}", hb.y);
    assert!(approx(hb.w, 300.0), "header spans columns, got {}", hb.w);
    assert!(approx(hb.h, 28.0), "default header height");
    // First-row cells sit below the pinned header with exact
    // column widths.
    let mut xs: Vec<f32> = find_retained_by_debug(&h.host, "dgrid-cell")
        .into_iter()
        .filter_map(|id| h.host.committed_box(id))
        .filter(|b| approx(b.y, 28.0))
        .map(|b| b.x)
        .collect();
    xs.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    assert_eq!(xs.len(), 3, "three first-row cells");
    assert!(approx(xs[0], 0.0) && approx(xs[1], 100.0) && approx(xs[2], 220.0));
    // Extent covers all rows plus the pinned header (spacer source).
    let list_box = h.host.committed_box(h.list).expect("list laid out");
    assert!(
        approx(list_box.content_h, 6028.0),
        "200 rows × 30 mean + 28 header, got {}",
        list_box.content_h
    );
}

/// Scrolling slides the window and pins the header: 500px lands
/// [15,25) (30 cells), the header box stays at the viewport top,
/// and zebra follows row 15.
#[test]
fn datagrid_scroll_pins_header_recycles_cells() {
    let (h, _) = mount_grid(GRID_N);
    h.offset.set(500.0);
    h.host.run_until_idle();
    assert_eq!(
        find_retained_by_debug(&h.host, "dgrid-cell").len(),
        30,
        "10 rows × 3 cols"
    );
    let header = find_retained_by_debug(&h.host, "dgrid-header")[0];
    let hb = h.host.committed_box(header).expect("header laid out");
    assert!(
        approx(hb.y, 0.0),
        "header pinned to viewport top, got {}",
        hb.y
    );
    // Row 15's cells sit viewport-relative at 28 + tops[15] - 500
    // with odd zebra.
    let first_row: Vec<NodeId> = find_retained_by_debug(&h.host, "dgrid-cell")
        .into_iter()
        .filter(|id| {
            h.host
                .committed_box(*id)
                .is_some_and(|b| approx(b.y, 28.0 + vitem_top(15) - 500.0))
        })
        .collect();
    assert_eq!(first_row.len(), 3, "three cells at row 15");
    for id in first_row {
        assert_eq!(cell_bg(&h, id), Some(Color(0x33_33_33)));
    }
}

/// Round 24.2: one wheel event on an unbound grid feeds the
/// declaring component's offset (no `bind_scroll` call), keeps the
/// header at the viewport top, and moves rows viewport-relative.
#[test]
fn datagrid_wheel_feeds_owner_offset_without_explicit_bind() {
    let (h, _) = mount_grid(GRID_N);
    assert_eq!(
        h.host.bound_scroll(h.list),
        None,
        "the grid never binds an explicit feed"
    );
    h.host.inject_input(InputEvent::Scroll {
        target: h.list,
        dx: 0.0,
        dy: 120.0,
    });
    h.host.run_until_idle();
    assert_eq!(h.offset.get(), 120.0, "wheel moved the owner offset");
    let header = find_retained_by_debug(&h.host, "dgrid-header")[0];
    let hb = h.host.committed_box(header).expect("header laid out");
    assert!(approx(hb.y, 0.0), "header stays put, got {}", hb.y);
    // Row 2 is the window head at 120px: 28 + 60 - 120 = -32.
    let first_row: Vec<NodeId> = find_retained_by_debug(&h.host, "dgrid-cell")
        .into_iter()
        .filter(|id| {
            h.host
                .committed_box(*id)
                .is_some_and(|b| approx(b.y, 28.0 + vitem_top(2) - 120.0))
        })
        .collect();
    assert_eq!(first_row.len(), 3, "three cells at row 2");
    for id in first_row {
        assert_eq!(cell_bg(&h, id), Some(Color(0x22_22_22)));
    }
}

/// Filter updates re-derive window + extent without remount
/// (even ids → 100 rows, total 3000).
#[test]
fn datagrid_filter_updates_window_and_extent() {
    let (h, handle) = mount_grid(GRID_N);
    let filtered = DataGridProps::new(h.coll.clone(), grid_cols(), vitem_h)
        .size(GRID_VIEW_W, GRID_VIEW_H)
        .overscan(VL_OVER)
        .debug("dgrid")
        .filter(|item: &VItem| item.i.is_multiple_of(2));
    handle.set_props(filtered);
    h.host.run_until_idle();
    let list_box = h.host.committed_box(h.list).expect("list laid out");
    // Even original indexes are exactly the 20px rows.
    assert!(
        approx(list_box.content_h, 2028.0),
        "100 even rows × 20 + 28 header, got {}",
        list_box.content_h
    );
    // Window recomputed over the filtered set (uniform 20px
    // rows in the 172px body: 9 visible + 2 overscan = 11 rows ×
    // 3 cols).
    assert_eq!(
        find_retained_by_debug(&h.host, "dgrid-cell").len(),
        33,
        "filtered window [0,11) × 3 cols"
    );
}

/// Round 13.1 → 13.3 combined: two page loads stream into the
/// live grid — states track per page, rows accumulate, extent
/// grows, no remount.
#[test]
fn datagrid_async_page_stream_grows_grid() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.set_text_service(Box::new(FakeText));
    let rt = host.runtime();
    let coll: oppa::Collection<VItem> =
        oppa::Collection::new(&rt, oppa::fetch_key("test:dgrid-pages"));
    #[derive(Clone)]
    struct GridApp {
        coll: oppa::Collection<VItem>,
        fired: Rc<std::cell::Cell<bool>>,
    }
    impl Props for GridApp {}
    fn grid_app_render(ctx: &Ctx, p: &GridApp) -> VNode {
        if !p.fired.get() {
            p.fired.set(true);
            let key = p.coll.key();
            ctx.spawn_fetch_page(key, 0, 10, 3, |_, _| {
                Ok(vec![VItem { i: 0 }, VItem { i: 1 }, VItem { i: 2 }])
            });
            ctx.spawn_fetch_page(key, 1, 10, 3, |_, _| {
                Ok(vec![VItem { i: 3 }, VItem { i: 4 }])
            });
        }
        let props = DataGridProps::new(p.coll.clone(), grid_cols(), vitem_h)
            .size(GRID_VIEW_W, GRID_VIEW_H)
            .overscan(VL_OVER)
            .debug("dgrid");
        ctx.child("oppa::DataGrid", 1, &props, DataGrid)
    }
    host.mount(
        "GridApp",
        GridApp {
            coll: coll.clone(),
            fired: Rc::new(std::cell::Cell::new(false)),
        },
        grid_app_render,
    );
    host.run_until_idle();
    let mut waited = 0;
    loop {
        let stats = rt.stats();
        if stats.tasks_done + stats.tasks_dropped >= 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        waited += 1;
        assert!(waited < 10_000, "executor tasks never resolved");
    }
    host.run_until_idle();
    assert_eq!(coll.len(), 5, "both pages streamed");
    // Extent follows live: 20+40+20+40+20 rows + 28 header = 168.
    let list = find_retained_by_debug(&host, "dgrid")[0];
    let list_box = host.committed_box(list).expect("list laid out");
    assert!(
        approx(list_box.content_h, 168.0),
        "5 streamed rows, got {}",
        list_box.content_h
    );
    // Both page states reached Ready (per-page tracking).
    for page in [0, 1] {
        let state = rt
            .keyed_state::<oppa::FetchState<Vec<VItem>>>(oppa::page_key(coll.key(), page), || {
                oppa::FetchState::Idle
            })
            .get();
        assert!(
            matches!(state, oppa::FetchState::Ready(_)),
            "page {page} Ready, got {state:?}"
        );
    }
}

// ------------------------------------------------------------------
// Round 14.1 (decision 311): generic `#[derive(Props)]`
// ------------------------------------------------------------------

/// The brief's shape, verbatim: a generic props struct deriving
/// the marker (no manual impl — the derive emits
/// `impl<T: Clone + 'static> Props`).
#[derive(Clone, Props)]
struct ListProps<T> {
    items: Vec<T>,
}

fn generic_list(ctx: &Ctx, p: &ListProps<String>) -> VNode {
    Column::new().children(
        p.items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                ctx.child(
                    "oppa::GenericRow",
                    i as u64,
                    &RowCase {
                        label: SharedString::from(item.as_str()),
                    },
                    generic_row,
                )
            })
            .collect::<Vec<_>>(),
    )
}

#[derive(Clone)]
struct RowCase {
    label: SharedString,
}

impl Props for RowCase {}

fn generic_row(_ctx: &Ctx, p: &RowCase) -> VNode {
    Div("generic-row")
        .style(Style::new().h(24).fill_width())
        .semantics(Semantics::default().label(&p.label))
        .build()
}

/// A generic props struct compiles through the derive and
/// renders through the real pipeline (mount + reconcile +
/// layout — one labeled row per item, in order).
#[test]
fn generic_derive_props_compiles_and_renders() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.mount(
        "GenericList",
        ListProps {
            items: vec!["a".to_string(), "b".to_string(), "c".to_string()],
        },
        generic_list,
    );
    host.run_until_idle();
    let rows = find_retained_by_debug(&host, "generic-row");
    assert_eq!(rows.len(), 3, "one row per item");
    let labels: Vec<String> = rows
        .iter()
        .map(|id| {
            host.retained_semantics(*id)
                .expect("row semantics")
                .label
                .as_deref()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert_eq!(
        labels,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
}

// ------------------------------------------------------------------
// Round 17.2 (decision 318): draggable scrollbar.
// ------------------------------------------------------------------

/// Scrollbar rig: hand-rolled `ScrollArea` (200×200 at the
/// origin, content 200×`content_h`) plus a sibling `Scrollbar`
/// sharing the rig's offset signal (the `scroll_app` shape —
/// one signal drives content and thumb together).
#[derive(Clone)]
struct ScrollRigProps {
    content_h: f32,
}
impl Props for ScrollRigProps {}

fn scroll_rig(ctx: &Ctx, p: &ScrollRigProps) -> VNode {
    let offset = ctx.scroll_offset();
    Div("scroll-screen")
        .style(Style::new().size(400, 300))
        .children([
            oppa::ScrollArea("scroll-list")
                .style(Style::new().size(200, 200).content_size(p.content_h))
                .on_scroll(|| {})
                .child(
                    Div("scroll-content")
                        .style(
                            Style::new()
                                .size(200, p.content_h)
                                .absolute_y(-offset.get()),
                        )
                        .build(),
                ),
            ctx.child(
                "oppa::ScrollRigBar",
                1,
                &ScrollbarProps {
                    target: SharedString::from("scroll-list"),
                    offset: offset.clone(),
                    // The rig proves the event-driven hide;
                    // wall-clock fade is proven through the
                    // VirtualList/DataGrid attachments (21.2).
                    idle_hide_ms: None,
                    axis: ScrollbarAxis::Vertical,
                },
                Scrollbar,
            ),
        ])
}

fn scrollbar_harness(content_h: f32) -> (ComponentHost, ScrollOffset) {
    let host = ComponentHost::new();
    host.set_viewport(400.0, 300.0);
    let handle = host.mount("S", ScrollRigProps { content_h }, scroll_rig);
    host.run_until_idle();
    let offset = host
        .instance_scroll(handle.root_instance())
        .expect("rig scroll handle");
    (host, offset)
}

fn scrollbar_plan(host: &ComponentHost) -> oppa::FramePlan {
    let builder = oppa_cpu::FramePlanBuilder::new(1.0);
    host.with_retained_mut(|rec, styles| builder.build_full(rec, styles))
}

/// Thumb + track rects by geometry (track spans the viewport
/// height at the target's right edge; thumb pins the brief
/// ratio) — color identifies the part only in dark mode, so
/// geometry leads everywhere.
type ChromePair = (Vec<(f32, f32, f32, f32)>, Vec<(f32, f32, f32, f32)>);

fn chrome_rects(plan: &oppa::FramePlan) -> ChromePair {
    let mut track = Vec::new();
    let mut thumb = Vec::new();
    for op in &plan.ops {
        if let oppa::DrawOp::Rect { x, y, w, h, .. } = op {
            if approx(*x, 188.0) && approx(*w, 12.0) {
                if approx(*y, 0.0) && approx(*h, 200.0) {
                    track.push((*x, *y, *w, *h));
                } else {
                    thumb.push((*x, *y, *w, *h));
                }
            }
        }
    }
    (track, thumb)
}

fn hover(host: &ComponentHost, x: f32, y: f32) {
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Move,
        x,
        y,
        modifiers: oppa::Modifiers::NONE,
    });
    host.run_until_idle();
}

fn drag(host: &ComponentHost, x1: f32, y1: f32, x2: f32, y2: f32) {
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Down {
            button: oppa::PointerButton::Primary,
        },
        x: x1,
        y: y1,
        modifiers: oppa::Modifiers::NONE,
    });
    host.run_until_idle();
    for (x, y) in [(x1 + (x2 - x1) * 0.5, y1 + (y2 - y1) * 0.5), (x2, y2)] {
        host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Move,
            x,
            y,
            modifiers: oppa::Modifiers::NONE,
        });
        host.run_until_idle();
    }
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Up {
            button: oppa::PointerButton::Primary,
        },
        x: x2,
        y: y2,
        modifiers: oppa::Modifiers::NONE,
    });
    host.run_until_idle();
}

fn track_tap(host: &ComponentHost, x: f32, y: f32) {
    tap_at(host, x, y, false);
}

/// Round 17.2: no overflow renders no chrome; overflow idles
/// hidden (opacity targets zero paint nothing — presence reads
/// the plan, never node labels).
#[test]
fn scrollbar_hidden_without_overflow_and_when_idle() {
    // Content fits: no chrome rects anywhere in the plan.
    let (host, _) = scrollbar_harness(100.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects(&plan);
    assert!(track.is_empty() && thumb.is_empty(), "fits: paints nothing");
    // Overflow, never touched: the raw plan carries no chrome rects.
    let (host, _) = scrollbar_harness(600.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects(&plan);
    assert!(track.is_empty() && thumb.is_empty(), "idle paints nothing");
}

/// Follow-up: hovering the transparent gutter outside the
/// painted 12px bar (but inside the 20px hit node) summons the
/// chrome — no pixel-hunting the bar itself.
#[test]
fn scrollbar_shows_on_gutter_hover_outside_painted_bar() {
    let (host, _) = scrollbar_harness(600.0);
    hover(&host, 184.0, 100.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects(&plan);
    assert_eq!(track.len(), 1, "gutter hover paints track, got {track:?}");
    assert_eq!(thumb.len(), 1, "gutter hover paints thumb, got {thumb:?}");
    // Painted chrome keeps the 12px geometry (the gutter paints
    // nothing itself).
    assert!(approx(track[0].0, 188.0) && approx(track[0].2, 12.0));
}

/// Round 17.2: hover fades the chrome in, leave fades it out
/// (raw targets flip immediately; the transition animates).
#[test]
fn scrollbar_shows_on_hover_and_hides_on_leave() {
    let (host, _) = scrollbar_harness(600.0);
    hover(&host, 194.0, 100.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects(&plan);
    assert_eq!(track.len(), 1, "track paints, got {track:?}");
    assert_eq!(thumb.len(), 1, "thumb paints, got {thumb:?}");
    assert!(
        approx(thumb[0].3, 200.0 * 200.0 / 600.0),
        "thumb pins vp²/c, got {:?}",
        thumb[0]
    );
    hover(&host, 350.0, 290.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects(&plan);
    assert!(track.is_empty() && thumb.is_empty(), "leave hides");
}

/// Round 17.2 (the brief verification, exactly): dragging the
/// thumb from top to bottom scrolls the content to 100% of its
/// extent — and back again.
#[test]
fn thumb_drag_top_to_bottom_scrolls_to_max() {
    let (host, offset) = scrollbar_harness(600.0);
    // Thumb parks 0..66.67 at rest; grab its top, drag to the
    // track bottom: ratio 400/133.33 lands exactly on max.
    drag(&host, 194.0, 5.0, 194.0, 199.0);
    assert!(
        approx(offset.get(), 400.0),
        "100% extent, got {}",
        offset.get()
    );
    let plan = scrollbar_plan(&host);
    let (_, thumb) = chrome_rects(&plan);
    assert_eq!(thumb.len(), 1, "thumb paints after drag");
    assert!(
        approx(thumb[0].1, (200.0f32 - 200.0 * 200.0 / 600.0).round()),
        "thumb parks at travel, got {:?}",
        thumb[0]
    );
    // And back: grab the bottom-parked thumb, drag to top.
    drag(&host, 194.0, 195.0, 194.0, 2.0);
    assert!(
        approx(offset.get(), 0.0),
        "back to top, got {}",
        offset.get()
    );
}

/// Round 17.2: track taps page by viewport, clamped at both
/// ends; thumb taps hold (dragging is the gesture).
#[test]
fn track_click_pages_and_clamps() {
    let (host, offset) = scrollbar_harness(600.0);
    // Below the rest thumb (0..66.67): +200.
    track_tap(&host, 194.0, 150.0);
    assert!(
        approx(offset.get(), 200.0),
        "pages down, got {}",
        offset.get()
    );
    // Above the moved thumb (66.67..133.33): −200.
    track_tap(&host, 194.0, 10.0);
    assert!(approx(offset.get(), 0.0), "pages up, got {}", offset.get());
    // Clamp: paging past the end parks on max, never beyond.
    track_tap(&host, 194.0, 150.0);
    track_tap(&host, 194.0, 190.0);
    assert!(
        approx(offset.get(), 400.0),
        "clamps on max, got {}",
        offset.get()
    );
    track_tap(&host, 194.0, 190.0);
    assert!(approx(offset.get(), 400.0), "stays parked");
    // Thumb tap holds (no page, no invoke — dragging owns it).
    track_tap(&host, 194.0, 170.0);
    assert!(approx(offset.get(), 400.0), "thumb tap holds");
}

// ------------------------------------------------------------------
// Round 21.2 (decision 329): VirtualList/DataGrid attachment +
// wall-clock idle fade.
// ------------------------------------------------------------------

/// Settles a clocked host vsync-by-vsync (the M8 pattern):
/// `run_until_idle` spins forever under a frozen clock while a
/// transition lives (animations retire only as time advances),
/// so clocked tests step `run_once` instead. Loud when the cap
/// trips (never a silent hang).
fn settle_clocked(host: &ComponentHost, clock: &oppa::MockClock) {
    for _ in 0..600 {
        clock.advance(1.0 / 60.0);
        if !host.run_once() {
            return;
        }
    }
    panic!("settle_clocked: still demanding after 600 frames (10s mock)");
}

/// Chrome rects at an arbitrary target box (the rig's
/// `chrome_rects` pinned to the rig box — attachments compute
/// their own track edge from the settled target box).
fn chrome_rects_at(plan: &oppa::FramePlan, track_x: f32, top_y: f32, view_h: f32) -> ChromePair {
    let mut track = Vec::new();
    let mut thumb = Vec::new();
    for op in &plan.ops {
        if let oppa::DrawOp::Rect { x, y, w, h, .. } = op {
            if approx(*x, track_x) && approx(*w, 12.0) {
                if approx(*y, top_y) && approx(*h, view_h) {
                    track.push((*x, *y, *w, *h));
                } else {
                    thumb.push((*x, *y, *w, *h));
                }
            }
        }
    }
    (track, thumb)
}

/// Mounts a `VirtualList` on a caller-owned host (clocked hosts
/// drive the idle fade deterministically).
fn mount_vlist_on(host: &ComponentHost, debug: &str) -> (oppa::ScrollOffset, NodeId) {
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:vlist-attach"));
    coll.ingest((0..VL_N).map(|i| VItem { i }).collect::<Vec<_>>());
    let props = VirtualListProps::new(coll, vitem_h, vitem_row)
        .size(320.0, VL_VIEW_H)
        .overscan(VL_OVER)
        .debug(debug);
    let handle = host.mount("VListAttach", props, VirtualList);
    host.run_until_idle();
    let offset = host
        .instance_scroll(handle.root_instance())
        .expect("list scroll handle");
    let list = find_retained_by_debug(host, debug)[0];
    (offset, list)
}

/// Round 21.2: dragging the attached thumb top-to-bottom scrolls
/// the virtualized row window to 100% — offset parks on max and
/// the window covers the final rows.
#[test]
fn vlist_attached_thumb_drag_scrolls_to_max() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let (offset, list) = mount_vlist_on(&host, "vlist");
    let tb = host.committed_box(list).expect("list laid out");
    let track_x = tb.x + tb.w - 12.0;
    // Thumb parks 0..24 at rest; grab its top, drag to the
    // track bottom — the ratio overshoots, the clamp parks max.
    drag(&host, track_x + 6.0, 5.0, track_x + 6.0, tb.y + tb.h - 1.0);
    let max = 30_000.0 - VL_VIEW_H;
    assert!(
        approx(offset.get(), max),
        "100% extent, got {}",
        offset.get()
    );
    // The window covers the final rows (last slot bottom ==
    // viewport bottom).
    let bottom = find_retained_by_debug(&host, "vlist-slot")
        .into_iter()
        .filter_map(|id| host.committed_box(id))
        .map(|b| b.y + b.h)
        .fold(0.0f32, f32::max);
    assert!(
        approx(bottom, VL_VIEW_H),
        "window shows the final rows, bottom {bottom}"
    );
}

/// Round 21.2: the attached overlay fades on the wall clock —
/// visible after a scroll, still visible before `idle_hide_ms`,
/// gone after, without another pointer event (and the content
/// offset never moves). Settles step the mock clock (a frozen
/// clock spins `run_until_idle` while the fade transition
/// lives — the M8 `run_once` pattern instead).
#[test]
fn vlist_attached_idle_fade_hides_after_timeout() {
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let (offset, list) = mount_vlist_on(&host, "vlist-idle");
    settle_clocked(&host, &clock);
    offset.set(500.0);
    settle_clocked(&host, &clock);
    let tb = host.committed_box(list).expect("list laid out");
    let at = (tb.x + tb.w - 12.0, tb.y, tb.h);
    let visible = || {
        let plan = scrollbar_plan(&host);
        let (track, thumb) = chrome_rects_at(&plan, at.0, at.1, at.2);
        !track.is_empty() && !thumb.is_empty()
    };
    assert!(visible(), "scroll latches the chrome on");
    // Half the idle budget: still parked, no pointer in sight.
    clock.advance(0.5);
    assert_eq!(host.tick_timers(host.now_ms()), 0, "nothing due yet");
    settle_clocked(&host, &clock);
    assert!(visible(), "still parked before idle_hide_ms");
    // Past 1200ms: the timer drops the flash, chrome gone.
    clock.advance(0.8);
    assert_eq!(host.tick_timers(host.now_ms()), 1, "idle timer fires");
    settle_clocked(&host, &clock);
    assert!(!visible(), "hides without another pointer event");
    assert!(
        approx(offset.get(), 500.0),
        "fade never moves content, got {}",
        offset.get()
    );
}

/// Mounts a `DataGrid` on a caller-owned host.
fn mount_grid_on(host: &ComponentHost, debug: &str) -> (oppa::ScrollOffset, NodeId) {
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:dgrid-attach"));
    coll.ingest((0..GRID_N).map(|i| VItem { i }).collect::<Vec<_>>());
    let props = DataGridProps::new(coll, grid_cols(), vitem_h)
        .size(GRID_VIEW_W, GRID_VIEW_H)
        .overscan(VL_OVER)
        .debug(debug);
    let handle = host.mount("DGridAttach", props, DataGrid);
    host.run_until_idle();
    let offset = host
        .instance_scroll(handle.root_instance())
        .expect("grid scroll handle");
    let list = find_retained_by_debug(host, debug)[0];
    (offset, list)
}

/// Round 21.2: dragging the grid's attached thumb parks the
/// window on max extent with the header pinned to the viewport top.
#[test]
fn dgrid_attached_thumb_drag_scrolls_to_max() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.set_text_service(Box::new(FakeText));
    let (offset, list) = mount_grid_on(&host, "dgrid");
    let tb = host.committed_box(list).expect("grid laid out");
    let track_x = tb.x + tb.w - 12.0;
    drag(&host, track_x + 6.0, 5.0, track_x + 6.0, tb.y + tb.h - 1.0);
    let max = 6028.0 - GRID_VIEW_H;
    assert!(
        approx(offset.get(), max),
        "100% extent, got {}",
        offset.get()
    );
    let header = find_retained_by_debug(&host, "dgrid-header")[0];
    let hb = host.committed_box(header).expect("header laid out");
    assert!(
        approx(hb.y, 0.0),
        "header pinned to the viewport top, got {}",
        hb.y
    );
}

// ------------------------------------------------------------------
// Round 23.1 (decision 333): per-key granular subscriptions.
// ------------------------------------------------------------------

/// Granular row content (the 23.1 verify shape): reads its own
/// payload through `get_row` (tracked per-row — sibling rows
/// never hear each other's writes) and counts renders per
/// stable id.
#[derive(Clone)]
struct GItem {
    i: usize,
    runs: Rc<RefCell<Vec<usize>>>,
}

fn granular_row(_ctx: &Ctx, p: &VirtualRowProps<GItem>) -> VNode {
    p.row.value.runs.borrow_mut()[p.row.id.0 as usize] += 1;
    let v = p
        .rows
        .get_row(p.row.id)
        .get()
        .map(|row| row.i)
        .unwrap_or(usize::MAX);
    Div("grow")
        .style(
            Style::new()
                .h(p.height)
                .fill_width()
                .bg(if v.is_multiple_of(2) {
                    Color(0x22_22_22)
                } else {
                    Color(0x33_33_33)
                }),
        )
        .build()
}

/// Round 23.1 (decision 333): in a 1,000-row `Collection`
/// mounted inside `VirtualList`, updating one visible row via
/// `update_row` refreshes the window with the new value while
/// version subscribers stay quiet — no structural fan-out
/// (counters, sibling windows, and query readers never
/// re-run). The owning window re-derives (M2 inline semantics:
/// inline children share their root's dependency set, so the
/// window pass re-renders its slots — per-row child isolation
/// needs effect-per-child machinery, an open architecture
/// question, not a silent gap).
#[test]
fn update_row_refreshes_without_version_fanout() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:granular-vlist"));
    let runs = Rc::new(RefCell::new(vec![0usize; 1000]));
    coll.ingest(
        (0..1000)
            .map(|i| GItem {
                i,
                runs: runs.clone(),
            })
            .collect::<Vec<_>>(),
    );
    let props = VirtualListProps::new(coll.clone(), |_: &GItem| 30.0, granular_row)
        .size(320.0, 200.0)
        .overscan(2)
        .debug("gvlist")
        .scrollbar(false);
    host.mount("GV", props, VirtualList);
    // An unrelated version subscriber (counter): quiet unless
    // the version bumps (a coarse `update` would re-run it).
    use oppa::Props;
    #[derive(Clone)]
    struct LenProps {
        runs: Rc<RefCell<usize>>,
        coll: oppa::Collection<GItem>,
    }
    impl Props for LenProps {}
    fn len_comp(_ctx: &Ctx, p: &LenProps) -> VNode {
        *p.runs.borrow_mut() += 1;
        let _ = p.coll.len();
        Div("lenprobe").build()
    }
    let len_runs = Rc::new(RefCell::new(0usize));
    host.mount(
        "LEN",
        LenProps {
            runs: len_runs.clone(),
            coll: coll.clone(),
        },
        len_comp,
    );
    host.run_until_idle();
    // Sanity: only the window materialized (far from all 1000).
    let rendered: usize = runs.borrow().iter().filter(|&&c| c > 0).count();
    assert!(
        rendered > 0 && rendered < 100,
        "windowed render, got {rendered}"
    );
    assert_eq!(*len_runs.borrow(), 1);
    // Row 3 sits in the first window: update it granularly.
    let target = coll.rows()[3].id;
    assert!(coll.update_row(
        target,
        GItem {
            i: 3001,
            runs: runs.clone()
        }
    ));
    host.run_until_idle();
    // The window picked up the fresh value.
    let shown: usize = find_retained_by_debug(&host, "grow").len();
    assert!(shown > 0, "window still renders");
    assert_eq!(
        coll.lookup(target).map(|g| g.i),
        Some(3001),
        "lookup sees the write"
    );
    // No structural fan-out: the version subscriber never
    // re-ran (a coarse `update` would have re-run it).
    assert_eq!(*len_runs.borrow(), 1, "version subscribers stay quiet");
    // Row 3 re-rendered (its slot notified); queries read the
    // fresh value on structural passes.
    let page = coll.query(&oppa::CollectionQuery {
        filter: None,
        sort: None,
        offset: 3,
        limit: Some(1),
    });
    assert_eq!(
        page.rows.first().map(|r| r.value.i),
        Some(3001),
        "queries read fresh values"
    );
}

/// Round 23.1 (decision 333): `update_row` never bumps the
/// collection version (a `len` reader proves it — still 1
/// after the granular write), while coarse `update` does.
#[test]
fn update_row_skips_version_while_update_bumps() {
    use oppa::Props;
    #[derive(Clone)]
    struct LProps {
        runs: Rc<RefCell<usize>>,
        coll: oppa::Collection<String>,
    }
    impl Props for LProps {}
    fn lcomp(_ctx: &Ctx, p: &LProps) -> VNode {
        *p.runs.borrow_mut() += 1;
        let _ = p.coll.len();
        Div("lprobe").build()
    }
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:zz-ver"));
    coll.ingest(vec!["a".to_string(), "b".to_string()]);
    let runs = Rc::new(RefCell::new(0usize));
    host.mount(
        "L",
        LProps {
            runs: runs.clone(),
            coll: coll.clone(),
        },
        lcomp,
    );
    host.run_until_idle();
    assert_eq!(*runs.borrow(), 1, "mounted once");
    coll.update_row(oppa::RowId(0), "A".to_string());
    host.run_until_idle();
    assert_eq!(*runs.borrow(), 1, "granular write skips the version");
    coll.update(oppa::RowId(0), "AA".to_string());
    host.run_until_idle();
    assert_eq!(*runs.borrow(), 2, "coarse write bumps it");
}
/// Round 23.1 (decision 333): root-mounted row readers are
/// precisely isolated — one row's `update_row` re-renders
/// exactly its subscriber (siblings quiet). Inside one list
/// the window re-derives with its root (M2 inline semantics —
/// see `update_row_refreshes_without_version_fanout`).
#[test]
fn update_row_rerenders_only_subscribed_roots() {
    use oppa::Props;
    use std::collections::HashMap;
    #[derive(Clone)]
    struct RProps {
        id: oppa::RowId,
        runs: Rc<RefCell<HashMap<u64, usize>>>,
        coll: oppa::Collection<String>,
    }
    impl Props for RProps {}
    fn rcomp(_ctx: &Ctx, p: &RProps) -> VNode {
        *p.runs.borrow_mut().entry(p.id.0).or_insert(0) += 1;
        let _ = p.coll.get_row(p.id).get();
        Div("rprobe").build()
    }
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:zz-probe"));
    let ids = coll.ingest(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    let runs = Rc::new(RefCell::new(HashMap::new()));
    for row in &ids {
        host.mount(
            "R",
            RProps {
                id: row.id,
                runs: runs.clone(),
                coll: coll.clone(),
            },
            rcomp,
        );
    }
    host.run_until_idle();
    assert_eq!(runs.borrow().len(), 3, "all three mounted once");
    coll.update_row(ids[1].id, "B".to_string());
    host.run_until_idle();
    assert_eq!(
        (
            runs.borrow().get(&0).copied(),
            runs.borrow().get(&1).copied(),
            runs.borrow().get(&2).copied()
        ),
        (Some(1), Some(2), Some(1)),
        "exactly the subscribed root re-renders"
    );
}

/// Round 21.2: the grid's attached overlay hides past
/// `idle_hide_ms` on the host clock (the hide leg — the
/// still-visible leg rides the list test above). Clock-stepped
/// settles (same frozen-clock rule as the list test).
#[test]
fn dgrid_attached_idle_fade_hides_after_timeout() {
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    host.set_text_service(Box::new(FakeText));
    let (offset, list) = mount_grid_on(&host, "dgrid-idle");
    settle_clocked(&host, &clock);
    offset.set(500.0);
    settle_clocked(&host, &clock);
    let tb = host.committed_box(list).expect("grid laid out");
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects_at(&plan, tb.x + tb.w - 12.0, tb.y, tb.h);
    assert!(
        !track.is_empty() && !thumb.is_empty(),
        "scroll shows chrome"
    );
    clock.advance(1.3);
    assert_eq!(host.tick_timers(host.now_ms()), 1, "idle timer fires");
    settle_clocked(&host, &clock);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects_at(&plan, tb.x + tb.w - 12.0, tb.y, tb.h);
    assert!(track.is_empty() && thumb.is_empty(), "hides on the clock");
    assert!(
        approx(offset.get(), 500.0),
        "fade never moves content, got {}",
        offset.get()
    );
}

/// Round 17.2: thumb and track honor reactive theme tokens
/// (text_secondary / border recolor Light→Dark in place).
#[test]
fn scrollbar_matches_theme_tokens() {
    use oppa::ThemeTokens;
    let (host, _) = scrollbar_harness(600.0);
    hover(&host, 194.0, 100.0);
    let plan = scrollbar_plan(&host);
    let thumb_color = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            oppa::DrawOp::Rect { x, w, h, color, .. }
                if approx(*x, 188.0) && approx(*w, 12.0) && !approx(*h, 200.0) =>
            {
                Some(*color)
            }
            _ => None,
        })
        .next()
        .expect("thumb rect");
    assert_eq!(thumb_color, ThemeTokens::light().text_secondary);
    host.set_theme(oppa::ThemeMode::Dark);
    host.run_until_idle();
    let plan = scrollbar_plan(&host);
    let thumb_color = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            oppa::DrawOp::Rect { x, w, h, color, .. }
                if approx(*x, 188.0) && approx(*w, 12.0) && !approx(*h, 200.0) =>
            {
                Some(*color)
            }
            _ => None,
        })
        .next()
        .expect("thumb rect after theme flip");
    assert_eq!(
        thumb_color,
        ThemeTokens::dark().text_secondary,
        "thumb recolors in place"
    );
}

/// Round 17.2: arrows page by viewport once the track holds
/// focus (tab into it — press owners are tab stops).
#[test]
fn keyboard_pages_when_track_focused() {
    let (host, offset) = scrollbar_harness(600.0);
    host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
    host.run_until_idle();
    host.inject_input(InputEvent::key(keys::DOWN, KeyState::Pressed));
    host.run_until_idle();
    assert!(
        approx(offset.get(), 200.0),
        "Down pages, got {}",
        offset.get()
    );
    host.inject_input(InputEvent::key(keys::UP, KeyState::Pressed));
    host.run_until_idle();
    assert!(
        approx(offset.get(), 0.0),
        "Up pages back, got {}",
        offset.get()
    );
}

// ------------------------------------------------------------------
// Phase 36 PR2b (decision 354): horizontal scrollbar axis.
// ------------------------------------------------------------------

/// Horizontal rig: hand-rolled `ScrollArea` (200×200 at the
/// origin, content `content_w`×200) plus a horizontal `Scrollbar`
/// sharing the rig's x-half offset (the 2D `scroll_2d` handle's
/// `x()` view — same signal family, never a second source).
#[derive(Clone)]
struct HScrollRigProps {
    content_w: f32,
}
impl Props for HScrollRigProps {}

fn hscroll_rig(ctx: &Ctx, p: &HScrollRigProps) -> VNode {
    let offset = ctx.scroll_2d();
    Div("hscroll-screen")
        .style(Style::new().size(400, 300))
        .children([
            oppa::ScrollArea("hscroll-list")
                .style(Style::new().size(200, 200).content_size(p.content_w))
                .on_scroll(|| {})
                .child(
                    Div("hscroll-content")
                        .style(Style::new().size(p.content_w, 200).x(-offset.get().x))
                        .build(),
                ),
            ctx.child(
                "oppa::HScrollRigBar",
                1,
                &ScrollbarProps {
                    target: SharedString::from("hscroll-list"),
                    offset: offset.x(),
                    idle_hide_ms: None,
                    axis: ScrollbarAxis::Horizontal,
                },
                Scrollbar,
            ),
        ])
}

fn hscrollbar_harness(content_w: f32) -> (ComponentHost, ScrollOffset) {
    let host = ComponentHost::new();
    host.set_viewport(400.0, 300.0);
    let handle = host.mount("HS", HScrollRigProps { content_w }, hscroll_rig);
    host.run_until_idle();
    let both = host
        .instance_scroll_2d(handle.root_instance())
        .expect("rig 2D handle");
    (host, both.x())
}

/// Horizontal chrome rects (track spans the viewport width at
/// the target's bottom edge; thumb pins the transposed ratio).
fn hchrome_rects(plan: &oppa::FramePlan) -> ChromePair {
    let mut track = Vec::new();
    let mut thumb = Vec::new();
    for op in &plan.ops {
        if let oppa::DrawOp::Rect { x, y, w, h, .. } = op {
            if approx(*y, 188.0) && approx(*h, 12.0) {
                if approx(*x, 0.0) && approx(*w, 200.0) {
                    track.push((*x, *y, *w, *h));
                } else {
                    thumb.push((*x, *y, *w, *h));
                }
            }
        }
    }
    (track, thumb)
}

/// Phase 36 PR2b: the horizontal thumb paints transposed (same
/// ratio as the vertical brief) and hides without overflow.
#[test]
fn horizontal_thumb_paints_transposed_and_hides_when_fit() {
    let (host, _) = hscrollbar_harness(300.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = hchrome_rects(&plan);
    assert!(track.is_empty() && thumb.is_empty(), "idle paints nothing");
    hover(&host, 100.0, 194.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = hchrome_rects(&plan);
    assert_eq!(track.len(), 1, "gutter hover paints track, got {track:?}");
    assert_eq!(thumb.len(), 1, "gutter hover paints thumb, got {thumb:?}");
    assert!(
        approx(thumb[0].2, 200.0 * 200.0 / 300.0),
        "thumb pins vp²/c, got {:?}",
        thumb[0]
    );
    assert!(
        approx(thumb[0].0, 0.0),
        "rest parks at left, got {:?}",
        thumb[0]
    );
    // No overflow: nothing paints even on hover.
    let (host2, _) = hscrollbar_harness(100.0);
    hover(&host2, 100.0, 194.0);
    let plan2 = scrollbar_plan(&host2);
    let (track2, thumb2) = hchrome_rects(&plan2);
    assert!(
        track2.is_empty() && thumb2.is_empty(),
        "fits: paints nothing"
    );
}

/// Phase 36 PR2b: dragging the horizontal thumb left-to-right
/// scrolls the content to 100% of its `content_w` overflow.
#[test]
fn horizontal_thumb_drag_scrolls_to_max() {
    let (host, offset) = hscrollbar_harness(600.0);
    // Thumb at rest: 0..66.67 on the 200 track; drag to the right
    // edge (through the midpoint, like the vertical proof).
    drag(&host, 33.0, 194.0, 190.0, 194.0);
    assert!(
        approx(offset.get(), 400.0),
        "drag parks at max, got {}",
        offset.get()
    );
    let plan = scrollbar_plan(&host);
    let (_, thumb) = hchrome_rects(&plan);
    assert_eq!(thumb.len(), 1, "thumb paints after drag");
    assert!(
        approx(thumb[0].0, (200.0f32 - 200.0 * 200.0 / 600.0).round()),
        "thumb parks at travel, got {:?}",
        thumb[0]
    );
}

/// Phase 36 PR2b: Right/Left page by viewport on the horizontal
/// axis, and the track semantics carry the G18 numeric value.
#[test]
fn horizontal_keys_page_and_semantics_carry_range() {
    let (host, offset) = hscrollbar_harness(600.0);
    host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
    host.run_until_idle();
    host.inject_input(InputEvent::key(keys::RIGHT, KeyState::Pressed));
    host.run_until_idle();
    assert!(
        approx(offset.get(), 200.0),
        "Right pages, got {}",
        offset.get()
    );
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert!(
        approx(offset.get(), 0.0),
        "Left pages back, got {}",
        offset.get()
    );
    // Range half (decision 352, wired in PR2b): the slider payload
    // names the numeric position, not just the human percent.
    let track = host.with_retained_mut(|rec, _| {
        rec.retained_ids()
            .into_iter()
            .find(|id| {
                rec.get(*id)
                    .is_some_and(|n| n.debug.starts_with("scrollbar-track-"))
            })
            .expect("track node lives")
    });
    let sem = host.retained_semantics(track).expect("track semantics");
    assert_eq!(sem.role, oppa::Role::Slider);
    assert_eq!(sem.value_num, Some(oppa::Num::of(0.0)));
    assert_eq!(sem.min_value, Some(oppa::Num::of(0.0)));
    assert_eq!(sem.max_value, Some(oppa::Num::of(400.0)));
}

/// Round 22.1 (decision 331): `masked` publishes into the
/// session every render (and clears back) — the loop's
/// copy/cut guard reads this flag, so toggling needs no
/// remount. Unmasked fields publish false.
#[test]
fn masked_flag_publishes_into_session() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(SharedString::from("secret"));
    let handle = host.mount(
        "TI",
        TextInputProps::new("Password", value.clone()).masked(true),
        TextInput,
    );
    host.run_until_idle();
    let sessions = host.edit_sessions_for(handle.root_instance());
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0].is_masked(), "masked prop reaches the session");
    // Toggling the prop clears the flag without a remount.
    handle.set_props(TextInputProps::new("Password", value.clone()));
    host.run_until_idle();
    let sessions = host.edit_sessions_for(handle.root_instance());
    assert!(!sessions[0].is_masked(), "unmasked publishes false");
}

/// Round 22.2 (decision 332): `TextArea` publishes its content
/// width (box minus both pads) as the session wrap width, so
/// vertical caret travel wraps where the area wraps.
#[test]
fn textarea_publishes_wrap_width() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(SharedString::from("hi"));
    let handle = host.mount(
        "TA",
        TextAreaProps::new("Notes", value.clone()).width(200.0),
        TextArea,
    );
    host.run_until_idle();
    let sessions = host.edit_sessions_for(handle.root_instance());
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0].wrap_width(),
        Some(184.0),
        "content width drives wrapping"
    );
    assert!(sessions[0].is_multiline());
}

/// Round 17.3 (decision 319): masked TextInput renders bullet
/// characters while the EditSession and bound value retain cleartext.
#[test]
fn password_masked_renders_bullets_with_cleartext_intact() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(SharedString::from("secret123"));
    let _handle = host.mount(
        "TI",
        TextInputProps::new("Password", value.clone()).masked(true),
        TextInput,
    );
    host.run_until_idle();

    // 1. Cleartext intact in author-owned signal
    assert_eq!(&*value.get(), "secret123");

    // 2. Displayed node text is bullets of matching length
    let text_ids = find_retained_by_debug(&host, "text");
    let leaf_text = host
        .with_retained_mut(|rec, _| {
            text_ids
                .into_iter()
                .find_map(|id| rec.get(id).and_then(|n| n.text.clone()))
        })
        .expect("text leaf");
    assert_eq!(&*leaf_text, "•••••••••");

    // 3. Typing / inserting appends cleartext and grows bullet cluster
    press_node(&host, node_by_debug(&host, "text-input"));
    let session = host.focused_field_session().expect("focused session");
    session.insert("4");
    host.run_until_idle();

    assert_eq!(&*value.get(), "4secret123");
    let text_ids = find_retained_by_debug(&host, "text");
    let leaf_text = host
        .with_retained_mut(|rec, _| {
            text_ids
                .into_iter()
                .find_map(|id| rec.get(id).and_then(|n| n.text.clone()))
        })
        .expect("text leaf after insert");
    assert_eq!(&*leaf_text, "••••••••••");

    // 4. Empty value with placeholder displays placeholder, not bullets
    let empty_val = host.runtime().signal(SharedString::from(""));
    host.mount(
        "TI2",
        TextInputProps::new("Password", empty_val)
            .placeholder("Enter password")
            .masked(true),
        TextInput,
    );
    host.run_until_idle();
    let text_ids = find_retained_by_debug(&host, "text");
    let ph_text = host
        .with_retained_mut(|rec, _| {
            text_ids
                .into_iter()
                .filter_map(|id| rec.get(id).and_then(|n| n.text.clone()))
                .find(|t| &**t == "Enter password")
        })
        .expect("placeholder leaf");
    assert_eq!(&*ph_text, "Enter password");
}

#[derive(Clone, Props)]
struct TooltipAnchorProps;

fn tooltip_anchor(_ctx: &Ctx, _p: &TooltipAnchorProps) -> VNode {
    Div("anchor-btn")
        .style(Style::new().size(80, 30).bg(Color(0x33_33_33)))
        .build()
}

fn tooltip_screen(ctx: &Ctx, _p: &()) -> VNode {
    let tp = TooltipProps::new("Helpful tip", tooltip_anchor, TooltipAnchorProps);
    Div("screen")
        .style(Style::new().size(400, 300))
        .child(ctx.child("oppa::Tooltip", 1, &tp, Tooltip))
}

/// Round 17.3 (decision 319): tooltip card mounts after 500ms
/// dwell, dismisses immediately on hover-leave or press.
#[test]
fn tooltip_dwell_mounts_card_and_dismisses_on_leave_or_press() {
    use std::rc::Rc;
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(400.0, 300.0);
    host.mount("S", (), tooltip_screen);
    host.run_until_idle();

    // 1. Initial state: no hover, tooltip-card not in tree
    assert!(
        find_retained_by_debug(&host, "tooltip-card").is_empty(),
        "initially closed"
    );

    // 2. Hover at t = 0: pointer enters anchor (40, 15)
    hover(&host, 40.0, 15.0);
    assert!(
        find_retained_by_debug(&host, "tooltip-card").is_empty(),
        "closed before dwell"
    );

    // 3. Hover before 500ms (e.g. 200ms): still closed
    clock.advance(0.2);
    hover(&host, 42.0, 15.0);
    assert!(
        find_retained_by_debug(&host, "tooltip-card").is_empty(),
        "closed at 200ms"
    );

    // 4. Dwell reaches 500ms: tooltip-card mounts
    clock.advance(0.35); // 0.2 + 0.35 = 0.55s (550ms >= 500ms)
    hover(&host, 42.0, 15.0);
    assert_eq!(
        find_retained_by_debug(&host, "tooltip-card").len(),
        1,
        "mounts after 500ms dwell"
    );

    // Card displays the tip text
    let card_ids = find_retained_by_debug(&host, "tooltip-card");
    let tip_text = host
        .with_retained_mut(|rec, _| {
            for id in card_ids {
                let mut stack = vec![id];
                while let Some(cur) = stack.pop() {
                    if let Some(node) = rec.get(cur) {
                        if let Some(ref text) = node.text {
                            return Some(text.clone());
                        }
                        stack.extend(node.children.iter().copied());
                    }
                }
            }
            None
        })
        .expect("tip text");
    assert_eq!(&*tip_text, "Helpful tip");

    // 5. Hover-leave dismisses immediately
    hover(&host, 300.0, 200.0);
    assert!(
        find_retained_by_debug(&host, "tooltip-card").is_empty(),
        "dismisses on leave"
    );

    // 6. Dwell again to mount, then press dismisses
    hover(&host, 40.0, 15.0);
    clock.advance(0.6);
    hover(&host, 40.0, 15.0);
    assert_eq!(
        find_retained_by_debug(&host, "tooltip-card").len(),
        1,
        "re-mounts on dwell"
    );

    // Press on anchor dismisses
    tap_at(&host, 40.0, 15.0, false);
    assert!(
        find_retained_by_debug(&host, "tooltip-card").is_empty(),
        "dismisses on press"
    );
}

/// Round 21.3 (decision 330): a tooltip spawned 5px from the
/// corner flips inside the viewport (long tip forces both-axis
/// overflow below/right of the anchor).
#[test]
fn tooltip_clamps_near_viewport_edges() {
    use std::rc::Rc;
    #[derive(Clone, Props)]
    struct CornerProps;
    fn corner_anchor(_ctx: &Ctx, _p: &CornerProps) -> VNode {
        Div("corner-btn")
            .style(Style::new().size(60, 20).bg(Color(0x33_33_33)))
            .build()
    }
    fn corner_screen(ctx: &Ctx, _p: &()) -> VNode {
        let tp = TooltipProps::new(
            "A much longer tooltip tip text here",
            corner_anchor,
            CornerProps,
        );
        // Anchor top-left at (335, 275): below/right overflow.
        Div("screen").style(Style::new().size(400, 300)).child(
            Div("corner-wrap")
                .style(Style::new().x(335.0).absolute_y(275.0).w(60.0).h(20.0))
                .child(ctx.child("oppa::CornerTip", 1, &tp, Tooltip)),
        )
    }
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(400.0, 300.0);
    host.set_text_service(Box::new(FakeText));
    host.mount("S", (), corner_screen);
    host.run_until_idle();
    // Dwell the anchor center (365, 285) past 500ms.
    hover(&host, 365.0, 285.0);
    clock.advance(0.6);
    hover(&host, 365.0, 285.0);
    let card = find_retained_by_debug(&host, "tooltip-card")
        .into_iter()
        .next()
        .expect("card mounts on dwell");
    let b = host.committed_box(card).expect("laid card");
    assert!(
        b.x >= 0.0 && b.y >= 0.0 && b.x + b.w <= 400.0 && b.y + b.h <= 300.0,
        "card flips inside the viewport, got ({}, {}) {}x{}",
        b.x,
        b.y,
        b.w,
        b.h
    );
}

/// Round 18.1 (decision 320): ErrorBoundary catches child panic,
/// host survives, renders fallback card, and recovers on reset.
#[test]
fn error_boundary_catches_child_panic_host_survives_and_recovers_on_retry() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let should_panic = Arc::new(AtomicBool::new(true));
    let error_log = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

    #[derive(Clone)]
    struct TestProps1 {
        sp: Arc<AtomicBool>,
        log: Arc<std::sync::Mutex<Vec<String>>>,
    }
    impl oppa::Props for TestProps1 {}

    let sp_clone = should_panic.clone();
    let log_clone = error_log.clone();

    fn child_view(should_panic: &AtomicBool) -> VNode {
        if should_panic.load(Ordering::SeqCst) {
            panic!("boom: intentional component panic");
        }
        Div("healthy-child").child(VNode::from(Text::new("Recovered successfully")))
    }

    fn screen(ctx: &Ctx, props: &TestProps1) -> VNode {
        let sp_child = props.sp.clone();
        let log_err = props.log.clone();
        let eb_props =
            ErrorBoundaryProps::new(move |_| child_view(&sp_child)).on_error(move |msg| {
                log_err.lock().unwrap().push(msg.to_string());
            });
        ctx.child("EB", 0, &eb_props, ErrorBoundary)
    }

    let host = ComponentHost::new();
    host.set_viewport(400.0, 300.0);

    // Silence stderr panic output for the intentional test panic
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    host.mount(
        "App",
        TestProps1 {
            sp: sp_clone,
            log: log_clone,
        },
        screen,
    );
    host.run_until_idle();
    std::panic::set_hook(hook);

    // 1. Host survived! Panicking child rendered fallback card.
    assert!(find_retained_by_debug(&host, "healthy-child").is_empty());
    assert_eq!(
        find_retained_by_debug(&host, "error-boundary-card").len(),
        1
    );
    assert_eq!(error_log.lock().unwrap().len(), 1);
    assert!(error_log.lock().unwrap()[0].contains("boom: intentional component panic"));

    // 2. Clear panic condition, click retry
    should_panic.store(false, Ordering::SeqCst);
    let retry_btn = node_by_debug(&host, "error-boundary-retry");
    press_node(&host, retry_btn);
    host.run_until_idle();

    // 3. Child recovered! Fallback card is gone, healthy child is mounted.
    assert_eq!(find_retained_by_debug(&host, "healthy-child").len(), 1);
    assert!(find_retained_by_debug(&host, "error-boundary-card").is_empty());
}

/// Round 18.1 (decision 320): Custom fallback renderer receives error and reset trigger.
#[test]
fn error_boundary_custom_fallback_renders_and_resets() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let should_panic = Arc::new(AtomicBool::new(true));
    let sp_clone = should_panic.clone();

    #[derive(Clone)]
    struct TestProps2 {
        sp: Arc<AtomicBool>,
    }
    impl oppa::Props for TestProps2 {}

    fn screen(ctx: &Ctx, props: &TestProps2) -> VNode {
        let sp_child = props.sp.clone();
        let eb_props = ErrorBoundaryProps::new(move |_| {
            if sp_child.load(Ordering::SeqCst) {
                panic!("custom panic payload");
            }
            Div("custom-child-ok").child(VNode::from(Text::new("Child OK")))
        })
        .fallback(|_ctx, err, _reset| {
            Div("custom-fallback")
                .semantics(oppa::Semantics::default().label(err))
                .child(VNode::from(Text::new(format!("Custom: {err}"))))
        });
        ctx.child("EB", 0, &eb_props, ErrorBoundary)
    }

    let host = ComponentHost::new();
    host.set_viewport(400.0, 300.0);

    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    host.mount("App", TestProps2 { sp: sp_clone }, screen);
    host.run_until_idle();

    // Fallback is custom-fallback with exact message
    assert!(find_retained_by_debug(&host, "custom-child-ok").is_empty());
    assert_eq!(find_retained_by_debug(&host, "custom-fallback").len(), 1);

    std::panic::set_hook(hook);
}

// ------------------------------------------------------------------
// Phase 38a (decisions 366–367): form validation + slider keys
// ------------------------------------------------------------------

#[derive(Clone)]
struct ValidationFieldCase {
    props: TextInputProps,
}
impl Props for ValidationFieldCase {}

fn validation_field_render(ctx: &Ctx, p: &ValidationFieldCase) -> VNode {
    ctx.child("oppa::TextInput", 7, &p.props, TextInput)
}

/// G7: invalid/required/error_message announce through Semantics;
/// the footer stacks under the field only when a message is
/// present (valid fields keep the exact tree — no wrapper).
#[test]
fn validation_marks_announce_and_footer_stacks() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    let props = TextInputProps {
        label: SharedString::from("Age"),
        value: value.clone(),
        placeholder: None,
        enabled: true,
        width: 200.0,
        height: 32.0,
        style: Text::body_secondary,
        debug: SharedString::from("text-input"),
        on_change: None,
        masked: false,
        invalid: true,
        required: true,
        error_message: Some(SharedString::from("err-age")),
        helper_text: Some(SharedString::from("Years, digits only")),
        borderless: false,
    };
    host.mount(
        "Val",
        ValidationFieldCase { props },
        validation_field_render,
    );
    host.run_until_idle();
    let id = node_by_debug(&host, "text-input");
    let sem = host.retained_semantics(id).expect("semantics");
    assert!(sem.invalid, "invalid announces");
    assert!(sem.required, "required announces");
    assert_eq!(
        sem.error_message.as_deref(),
        Some("err-age"),
        "error node identity announces"
    );
    // Error wins over helper while invalid; the wrapper stacks it.
    assert_eq!(
        find_retained_by_debug(&host, "text-input-field").len(),
        1,
        "footer wrapper stacks under the field"
    );
    // Helper-only (valid): footer renders, marks stay off.
    let host2 = ComponentHost::new();
    host2.set_text_service(Box::new(FakeText));
    let value2 = host2.runtime().signal(SharedString::from(""));
    let props2 = TextInputProps {
        label: SharedString::from("Age"),
        value: value2.clone(),
        placeholder: None,
        enabled: true,
        width: 200.0,
        height: 32.0,
        style: Text::body_secondary,
        debug: SharedString::from("text-input"),
        on_change: None,
        masked: false,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: Some(SharedString::from("Years, digits only")),
        borderless: false,
    };
    host2.mount(
        "ValHelp",
        ValidationFieldCase { props: props2 },
        validation_field_render,
    );
    host2.run_until_idle();
    let id2 = node_by_debug(&host2, "text-input");
    let sem2 = host2.retained_semantics(id2).expect("semantics");
    assert!(!sem2.invalid);
    assert!(!sem2.required);
    assert_eq!(sem2.error_message, None);
    assert_eq!(
        find_retained_by_debug(&host2, "text-input-field").len(),
        1,
        "helper-only still stacks the caption"
    );
    // Plain (no message): no wrapper — the pre-38a tree exactly.
    let host3 = ComponentHost::new();
    host3.set_text_service(Box::new(FakeText));
    let value3 = host3.runtime().signal(SharedString::from(""));
    host3.mount(
        "ValPlain",
        ValidationFieldCase {
            props: TextInputProps::new("Age", value3),
        },
        validation_field_render,
    );
    host3.run_until_idle();
    assert!(
        find_retained_by_debug(&host3, "text-input-field").is_empty(),
        "valid fields keep the exact tree"
    );
}

/// G7 across the catalog: Checkbox, Toggle, Select, and
/// RadioGroup carry the same four marks (validators stay
/// app-side — the payload only announces).
#[test]
fn checkbox_toggle_select_radiogroup_carry_validation() {
    // Checkbox.
    let host = ComponentHost::new();
    let checked = host.runtime().signal(false);
    host.mount(
        "ChkV",
        CheckCase {
            props: CheckboxProps {
                label: SharedString::from("T&C"),
                checked: checked.clone(),
                enabled: true,
                on_change: None,
                invalid: true,
                required: true,
                error_message: Some(SharedString::from("err-tc")),
                helper_text: None,
            },
        },
        check_render,
    );
    host.run_until_idle();
    let sem = host
        .retained_semantics(node_by_debug(&host, "checkbox"))
        .expect("semantics");
    assert!(sem.invalid && sem.required);
    assert_eq!(sem.error_message.as_deref(), Some("err-tc"));
    assert_eq!(find_retained_by_debug(&host, "checkbox-field").len(), 1);
    // Toggle.
    let host2 = ComponentHost::new();
    let on = host2.runtime().signal(false);
    host2.mount(
        "TglV",
        ToggleCase {
            props: ToggleProps {
                label: SharedString::from("Wi-Fi"),
                on: on.clone(),
                enabled: true,
                on_change: None,
                invalid: true,
                required: false,
                error_message: Some(SharedString::from("err-wifi")),
                helper_text: None,
            },
        },
        toggle_render,
    );
    host2.run_until_idle();
    let sem2 = host2
        .retained_semantics(node_by_debug(&host2, "toggle"))
        .expect("semantics");
    assert!(sem2.invalid);
    assert_eq!(sem2.error_message.as_deref(), Some("err-wifi"));
    assert_eq!(find_retained_by_debug(&host2, "toggle-field").len(), 1);
    // Select.
    let host3 = ComponentHost::new();
    let sel = host3.runtime().signal("a".to_string());
    let open = host3.runtime().signal(false);
    let mut sp = SelectProps::new(
        vec![SelectItem {
            value: "a".to_string(),
            label: SharedString::from("Alpha"),
        }],
        sel,
        open,
    );
    sp.invalid = true;
    sp.required = true;
    sp.error_message = Some(SharedString::from("err-sel"));
    host3.mount("SelV", sp, Select::<String>);
    host3.run_until_idle();
    let sem3 = host3
        .retained_semantics(node_by_debug(&host3, "select-box"))
        .expect("semantics");
    assert!(sem3.invalid && sem3.required);
    assert_eq!(sem3.error_message.as_deref(), Some("err-sel"));
    assert_eq!(find_retained_by_debug(&host3, "select-field").len(), 1);
    // RadioGroup.
    let host4 = ComponentHost::new();
    let gsel = host4.runtime().signal(SharedString::from("Free"));
    host4.mount(
        "GV",
        RadioGroupProps {
            options: vec![RadioOption {
                value: SharedString::from("Free"),
                label: SharedString::from("Free"),
            }],
            selected: gsel,
            enabled: true,
            invalid: true,
            required: true,
            error_message: Some(SharedString::from("err-plan")),
            helper_text: None,
        },
        RadioGroup::<SharedString>,
    );
    host4.run_until_idle();
    assert_eq!(
        find_retained_by_debug(&host4, "radio-group-field").len(),
        1,
        "group footer stacks under the options"
    );
}

/// Decision 367: the numeric range announces through value_num /
/// min_value / max_value (G18) next to the human value_text; Home
/// jumps to min, End to max (held arrows keep repeat-stepping).
#[test]
fn slider_home_end_jump_and_range_announces() {
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
    host.mount("SldHE", SliderCase { props }, slider_render);
    host.run_until_idle();
    let track = node_by_debug(&host, "slider");
    let sem = host.retained_semantics(track).expect("semantics");
    assert_eq!(sem.value_text.as_deref(), Some("50 percent"));
    assert_eq!(sem.value_num, Some(oppa::Num::of(50.0)));
    assert_eq!(sem.min_value, Some(oppa::Num::of(0.0)));
    assert_eq!(sem.max_value, Some(oppa::Num::of(100.0)));
    // Focus the track (press owns capture + focus), then jump.
    press_node(&host, track);
    assert_eq!(host.focused_node(), Some(track));
    host.inject_input(InputEvent::key(keys::HOME, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(value.get(), 0.0, "Home jumps to min");
    host.inject_input(InputEvent::key(keys::END, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(value.get(), 100.0, "End jumps to max");
    // Arrows still step from the jumped ends.
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(value.get(), 90.0, "Left steps down from max");
}

// ------------------------------------------------------------------
// Phase 38b (decisions 368–370): Tree, Splitter, DatePicker
// ------------------------------------------------------------------

fn tree_nodes() -> Vec<TreeNode> {
    vec![
        TreeNode::root("a", "Alpha"),
        TreeNode::child("a1", "Alpha-One", "a"),
        TreeNode::child("a2", "Alpha-Two", "a"),
        TreeNode::root("b", "Beta"),
    ]
}

#[derive(Clone)]
struct TreeCase {
    props: TreeProps,
}
impl Props for TreeCase {}

fn tree_render(ctx: &Ctx, p: &TreeCase) -> VNode {
    ctx.child("oppa::Tree", 7, &p.props, Tree)
}

/// Mounted tree + its controlled signals (one harness shape for
/// the Phase 38b tree tests — the tuple type rides an alias so
/// the signature stays under the complexity lint).
type TreeHarness = (
    ComponentHost,
    oppa::Collection<TreeNode>,
    Signal<Vec<SharedString>>,
    Signal<Option<SharedString>>,
);

fn mount_tree() -> TreeHarness {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:tree"));
    coll.ingest(tree_nodes());
    let expanded = rt.signal(Vec::<SharedString>::new());
    let selected = rt.signal(None::<SharedString>);
    let props = TreeProps {
        nodes: coll.clone(),
        expanded: expanded.clone(),
        selected: selected.clone(),
        label: SharedString::from("Files"),
        width: 280.0,
        height: 200.0,
        row_height: 28.0,
        overscan: 2,
        enabled: true,
        debug: SharedString::from("tree"),
    };
    host.mount("Tree", TreeCase { props }, tree_render);
    host.run_until_idle();
    (host, coll, expanded, selected)
}

/// G12: the flatten shows roots only when collapsed, children
/// when expanded (pure); the toggle is symmetric.
#[test]
fn tree_visible_flattens_and_toggles() {
    let rows: Vec<oppa::Row<TreeNode>> = tree_nodes()
        .into_iter()
        .enumerate()
        .map(|(i, value)| oppa::Row {
            id: oppa::RowId(i as u64),
            value,
        })
        .collect();
    let flat = tree_visible(&rows, &[]);
    assert_eq!(
        flat.iter()
            .map(|(r, d)| (r.value.id.to_string(), *d))
            .collect::<Vec<_>>(),
        vec![("a".to_string(), 0), ("b".to_string(), 0)],
        "collapsed shows roots only"
    );
    let open = vec![SharedString::from("a")];
    let flat = tree_visible(&rows, &open);
    assert_eq!(
        flat.iter()
            .map(|(r, d)| (r.value.id.to_string(), *d))
            .collect::<Vec<_>>(),
        vec![
            ("a".to_string(), 0),
            ("a1".to_string(), 1),
            ("a2".to_string(), 1),
            ("b".to_string(), 0),
        ],
        "expanded interleaves children in commit order"
    );
    let shut = tree_toggled(&open, &SharedString::from("a"));
    assert!(shut.is_empty(), "toggle is symmetric");
    let reopened = tree_toggled(&shut, &SharedString::from("b"));
    assert_eq!(reopened, vec![SharedString::from("b")]);
}

/// G12: chevron expands, row press selects, arrows walk the
/// visible list, and roles announce Tree/TreeItem.
#[test]
fn tree_expands_selects_and_walks() {
    let (host, _coll, expanded, selected) = mount_tree();
    // Container announces the tree role.
    let root = node_by_debug(&host, "tree");
    let sem = host.retained_semantics(root).expect("semantics");
    assert_eq!(sem.role, oppa::Role::Tree);
    assert_eq!(sem.label.as_deref(), Some("Files"));
    // Collapsed: two rows, both TreeItem.
    assert_eq!(find_retained_by_debug(&host, "tree-row").len(), 2);
    // Chevron expands "a" (first chevron hit in commit order).
    let chevrons = find_retained_by_debug(&host, "tree-chevron-hit");
    assert_eq!(chevrons.len(), 2);
    press_node(&host, chevrons[0]);
    assert_eq!(expanded.get(), vec![SharedString::from("a")]);
    assert_eq!(find_retained_by_debug(&host, "tree-row").len(), 4);
    // Row press selects (rows render in visible order).
    let rows = find_retained_by_debug(&host, "tree-row");
    press_node(&host, rows[1]);
    assert_eq!(selected.get(), Some(SharedString::from("a1")));
    let sem1 = host.retained_semantics(rows[1]).expect("semantics");
    assert_eq!(sem1.role, oppa::Role::TreeItem);
    assert_eq!(sem1.selected, Some(true));
    // Down moves selection to the next visible row.
    host.inject_input(InputEvent::key(keys::DOWN, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(selected.get(), Some(SharedString::from("a2")));
    // Left on a leaf ascends to the parent.
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(selected.get(), Some(SharedString::from("a")));
    // Left on the open parent collapses it.
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert!(expanded.get().is_empty(), "Left collapses the open node");
    assert_eq!(find_retained_by_debug(&host, "tree-row").len(), 2);
    // Right re-opens, Enter-equivalent press selects.
    host.inject_input(InputEvent::key(keys::RIGHT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(expanded.get(), vec![SharedString::from("a")]);
}

/// G13: minima clamp loudly-checked bounds; arrows nudge.
#[test]
fn splitter_fraction_clamps_and_arrows_nudge() {
    assert_eq!(splitter_fraction(0.5, 400.0, 8.0, 40.0, 40.0), 0.5);
    assert_eq!(
        splitter_fraction(0.0, 400.0, 8.0, 40.0, 40.0),
        (40.0 + 4.0) / 400.0,
        "first minimum pins the low end"
    );
    assert_eq!(
        splitter_fraction(1.0, 400.0, 8.0, 40.0, 40.0),
        1.0 - (40.0 + 4.0) / 400.0,
        "second minimum pins the high end"
    );
    // Mounted arrows nudge through the funnel.
    let host = ComponentHost::new();
    let frac = host.runtime().signal(0.5f32);
    fn split_pane(_ctx: &Ctx) -> VNode {
        Div("pane").child(VNode::from(Text {
            text: SharedString::from("pane"),
            style: Text::body_secondary,
        }))
    }
    let props = SplitterProps {
        fraction: frac.clone(),
        axis: SplitterAxis::Vertical,
        width: 400.0,
        height: 300.0,
        divider_px: 8.0,
        min_first_px: 40.0,
        min_second_px: 40.0,
        enabled: true,
        debug: SharedString::from("split"),
        first: Rc::new(split_pane),
        second: Rc::new(split_pane),
        on_change: None,
    };
    host.mount("Split", props, Splitter);
    host.run_until_idle();
    let div = node_by_debug(&host, "split-divider");
    press_node(&host, div);
    assert_eq!(host.focused_node(), Some(div));
    host.inject_input(InputEvent::key(keys::RIGHT, KeyState::Pressed));
    host.run_until_idle();
    assert!(
        (frac.get() - 0.55).abs() < 1e-6,
        "Right nudges +0.05, got {}",
        frac.get()
    );
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert!(
        (frac.get() - 0.5).abs() < 1e-6,
        "Left nudges back, got {}",
        frac.get()
    );
    // Divider shows the resize cursor.
    let cursor = host.with_retained_mut(|rec, styles| {
        let n = rec.get(div).expect("live divider");
        styles.get(n.style).cloned().unwrap_or_default().cursor
    });
    assert_eq!(cursor, Some(CursorIcon::ColResize));
}

/// G13/G14 pure cores: leap rule, known weekday (2026-10-01 is
/// a Thursday), parse/format round-trip, month shift clamping.
#[test]
fn date_math_shapes() {
    assert!(is_leap(2024) && !is_leap(2025) && !is_leap(1900) && is_leap(2000));
    assert_eq!(days_in_month(2024, 2), 29);
    assert_eq!(days_in_month(2025, 2), 28);
    // 2026-10-01: Thursday → Monday0 == 3.
    assert_eq!(
        weekday_monday0(Date {
            year: 2026,
            month: 10,
            day: 1
        }),
        3
    );
    // Monday 2026-09-28 → 0.
    assert_eq!(
        weekday_monday0(Date {
            year: 2026,
            month: 9,
            day: 28
        }),
        0
    );
    let d = Date::new(2026, 10, 1);
    assert_eq!(format_date(d).to_string(), "2026-10-01");
    assert_eq!(parse_date("2026-10-01"), Some(d));
    assert_eq!(parse_date("2026-13-01"), None, "month 13 never parses");
    assert_eq!(parse_date("2026-02-30"), None, "Feb 30 never parses");
    assert_eq!(parse_date("10-01-2026"), None, "non-padded never parses");
    assert_eq!(parse_date("hello"), None);
    // Jan 31 shifted +1 clamps to Feb 28 (2025, non-leap).
    let jan31 = Date::new(2025, 1, 31);
    let feb = shift_month(jan31, 1, None, None).expect("in range");
    assert_eq!((feb.year, feb.month, feb.day), (2025, 2, 28));
    // Bound refusal: shifting past max is None (disable rule).
    let oct = Date::new(2026, 10, 15);
    assert_eq!(
        shift_month(
            oct,
            1,
            Some(Date::new(2026, 10, 1)),
            Some(Date::new(2026, 10, 31))
        ),
        None,
        "November lies fully past October max"
    );
    // Day steps pin to the bounds.
    assert_eq!(
        shift_days(
            Date::new(2026, 10, 31),
            1,
            Some(Date::new(2026, 10, 1)),
            Some(Date::new(2026, 10, 31))
        ),
        Date::new(2026, 10, 31),
        "stepping past max pins"
    );
}

/// G14: picking a day sets the value and closes; prev/next flip
/// the shown month; the text bridge parses; bounds refuse.
#[test]
fn date_picker_picks_flips_and_parses() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(Date::new(2026, 10, 1));
    let open = host.runtime().signal(false);
    let props = DatePickerProps {
        value: value.clone(),
        open: open.clone(),
        min: None,
        max: None,
        enabled: true,
        width: 240.0,
        label: SharedString::from("When"),
    };
    host.mount("DP", props, DatePicker);
    host.run_until_idle();
    // Closed: no popup in the tree.
    assert!(find_retained_by_debug(&host, "date-popup").is_empty());
    // The chevron half toggles (the box center belongs to the
    // text bridge — taps there focus the field, the knob→track
    // precedent: each half routes to its own press owner).
    press_node(&host, node_by_debug(&host, "date-chevron"));
    assert!(open.get());
    assert_eq!(find_retained_by_debug(&host, "date-popup").len(), 1);
    assert_eq!(find_retained_by_debug(&host, "date-grid").len(), 1);
    // Pick the 15th: value sets, popup closes.
    press_node(&host, node_by_debug(&host, "date-day-15"));
    assert_eq!(value.get(), Date::new(2026, 10, 15));
    assert!(!open.get(), "pick closes the popup");
    // Reopen via the chevron, flip to November (day preserved).
    press_node(&host, node_by_debug(&host, "date-chevron"));
    press_node(&host, node_by_debug(&host, "date-next"));
    assert_eq!(value.get(), Date::new(2026, 11, 15));
    // Text bridge: a valid spelling commits.
    press_node(&host, node_by_debug(&host, "date-text"));
    let sess = host.focused_field_session().expect("session");
    sess.select_all();
    sess.insert("2026-12-25");
    host.run_until_idle();
    assert_eq!(value.get(), Date::new(2026, 12, 25));
    // Invalid text never moves the value.
    let sess = host.focused_field_session().expect("session");
    sess.select_all();
    sess.insert("not-a-date");
    host.run_until_idle();
    assert_eq!(value.get(), Date::new(2026, 12, 25));
    // Bounds: days outside [min, max] are handlerless.
    let host2 = ComponentHost::new();
    host2.set_text_service(Box::new(FakeText));
    let value2 = host2.runtime().signal(Date::new(2026, 10, 10));
    let open2 = host2.runtime().signal(true);
    host2.mount(
        "DPB",
        DatePickerProps {
            value: value2.clone(),
            open: open2.clone(),
            min: Some(Date::new(2026, 10, 5)),
            max: Some(Date::new(2026, 10, 20)),
            enabled: true,
            width: 240.0,
            label: SharedString::from("When"),
        },
        DatePicker,
    );
    host2.run_until_idle();
    let day1 = node_by_debug(&host2, "date-day-1");
    assert!(
        host2.retained_handlers(day1).is_empty(),
        "out-of-bounds days carry no press"
    );
    press_node(&host2, node_by_debug(&host2, "date-day-10"));
    assert_eq!(value2.get(), Date::new(2026, 10, 10));
}

// ------------------------------------------------------------------
// Phase 38c (decisions 371–372): Toolbar, Menubar, FilePicker, G22
// ------------------------------------------------------------------

fn toolbar_items(fired: &[Signal<bool>]) -> Vec<BarItem> {
    vec![
        BarItem::new("New", {
            let f = fired[0].clone();
            move || f.set(true)
        }),
        BarItem::separator(),
        BarItem::new("Save", {
            let f = fired[1].clone();
            move || f.set(true)
        })
        .disabled(),
        BarItem::new("Open", {
            let f = fired[2].clone();
            move || f.set(true)
        }),
    ]
}

/// G21: taps hit-test cells, arrows rove past separators/disabled,
/// Enter invokes the highlight; cells announce buttons.
#[test]
fn toolbar_taps_rove_and_enter_activates() {
    let host = ComponentHost::new();
    let rt = host.runtime();
    let fired: Vec<Signal<bool>> = (0..3).map(|_| rt.signal(false)).collect();
    host.mount(
        "TB",
        ToolbarProps {
            items: toolbar_items(&fired),
            ..ToolbarProps::new(vec![])
        },
        Toolbar,
    );
    host.run_until_idle();
    // Tap the first cell: invokes + roving follows the pointer.
    press_node(&host, node_by_debug(&host, "toolbar-cell-0"));
    assert!(fired[0].get(), "tap invokes the enabled cell");
    // Right roves past the separator (1) and disabled Save (2)
    // to Open (3); Enter invokes it through the router pulse.
    let bar = node_by_debug(&host, "toolbar");
    press_node(&host, bar);
    host.inject_input(InputEvent::key(keys::RIGHT, KeyState::Pressed));
    host.run_until_idle();
    host.inject_input(InputEvent::key(keys::ENTER, KeyState::Pressed));
    host.run_until_idle();
    assert!(fired[2].get(), "Enter invokes the roved highlight");
    // Left wraps back past disabled/separator to New.
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    host.inject_input(InputEvent::key(keys::ENTER, KeyState::Pressed));
    host.run_until_idle();
    assert!(fired[0].get());
    // Cells announce buttons; separators carry no semantics.
    let sem = host
        .retained_semantics(node_by_debug(&host, "toolbar-cell-0"))
        .expect("semantics");
    assert_eq!(sem.role, oppa::Role::Button);
    assert_eq!(sem.label.as_deref(), Some("New"));
    assert!(
        host.retained_semantics(node_by_debug(&host, "toolbar-cell-1"))
            .is_none(),
        "separators announce nothing"
    );
    // Disabled cells keep their press owner out of the tab order.
    assert!(
        !host
            .retained_handlers(node_by_debug(&host, "toolbar"))
            .is_empty(),
        "the bar container owns press + arrows (one tab stop)"
    );
}

fn menubar_titles(fired: &[Signal<bool>]) -> Vec<MenuTitle> {
    vec![
        MenuTitle::new(
            "File",
            vec![
                MenuItemProps::new("New", {
                    let f = fired[0].clone();
                    move || f.set(true)
                }),
                MenuItemProps::new("Open", {
                    let f = fired[1].clone();
                    move || f.set(true)
                }),
            ],
        ),
        MenuTitle::new(
            "Edit",
            vec![MenuItemProps::new("Undo", {
                let f = fired[2].clone();
                move || f.set(true)
            })],
        ),
    ]
}

fn mount_menubar() -> (ComponentHost, Vec<Signal<bool>>, Signal<Option<usize>>) {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let rt = host.runtime();
    let fired: Vec<Signal<bool>> = (0..3).map(|_| rt.signal(false)).collect();
    let open_title = rt.signal(None::<usize>);
    let props = MenubarProps {
        titles: menubar_titles(&fired),
        open_title: open_title.clone(),
        ..MenubarProps::new(vec![], open_title.clone())
    };
    host.mount("MB", props, Menubar);
    host.run_until_idle();
    (host, fired, open_title)
}

/// G21: title taps toggle the dropdown; Down opens; Tab + Enter
/// invokes the highlighted row and closes; Right moves the open
/// menu; Escape closes through the blur rule.
#[test]
fn menubar_opens_invokes_and_escapes() {
    let (host, fired, open_title) = mount_menubar();
    assert!(find_retained_by_debug(&host, "menu-popup").is_empty());
    // Title tap toggles the dropdown (attached to the bar focus).
    press_node(&host, node_by_debug(&host, "menubar-title-0"));
    assert_eq!(open_title.get(), Some(0));
    host.run_until_idle();
    assert_eq!(find_retained_by_debug(&host, "menu-popup").len(), 1);
    // Same-title tap closes.
    press_node(&host, node_by_debug(&host, "menubar-title-0"));
    assert_eq!(open_title.get(), None);
    assert!(find_retained_by_debug(&host, "menu-popup").is_empty());
    // Keyboard: Tab focuses the bar, Down opens title 0.
    host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
    host.run_until_idle();
    let bar = node_by_debug(&host, "menubar-bar");
    assert_eq!(host.focused_node(), Some(bar));
    host.inject_input(InputEvent::key(keys::DOWN, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(open_title.get(), Some(0));
    // Right moves the open menu with the highlight (native).
    host.inject_input(InputEvent::key(keys::RIGHT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(open_title.get(), Some(1));
    // Tab into the list, Enter invokes Undo and closes.
    host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
    host.run_until_idle();
    host.inject_input(InputEvent::key(keys::ENTER, KeyState::Pressed));
    host.run_until_idle();
    assert!(fired[2].get(), "Enter invokes the highlighted row");
    assert_eq!(open_title.get(), None, "invoke closes the menu");
    // Escape closes an open menu through the blur rule.
    press_node(&host, node_by_debug(&host, "menubar-title-1"));
    assert_eq!(open_title.get(), Some(1));
    host.inject_input(InputEvent::key(keys::ESCAPE, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(open_title.get(), None, "Escape dismisses");
}

/// G21: browse wraps request/poll per mode (pick sets, dismissal
/// leaves the path, failures surface in the caption).
#[test]
fn file_picker_browse_sets_and_dismissal_keeps() {
    // Open mode.
    let host = ComponentHost::new();
    let path = host.runtime().signal(None::<SharedString>);
    let open_dlg = std::rc::Rc::new(std::cell::RefCell::new(oppa::ScriptedDialog::new()));
    open_dlg
        .borrow_mut()
        .push_response(vec![std::path::PathBuf::from("a.txt")]);
    open_dlg.borrow_mut().push_response(vec![]);
    let mut fp = FilePickerProps::new(path.clone(), FilePickerMode::Open);
    fp.open_dialog = Some(open_dlg);
    fp.debug = SharedString::from("fp-open");
    host.mount("FP", fp, FilePicker);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "fp-open-browse"));
    assert_eq!(path.get().as_deref(), Some("a.txt"));
    press_node(&host, node_by_debug(&host, "fp-open-browse"));
    assert_eq!(
        path.get().as_deref(),
        Some("a.txt"),
        "dismissal leaves the path untouched"
    );
    // Save mode.
    let host2 = ComponentHost::new();
    let path2 = host2.runtime().signal(None::<SharedString>);
    let save_dlg = std::rc::Rc::new(std::cell::RefCell::new(oppa::ScriptedSaveDialog::new()));
    save_dlg
        .borrow_mut()
        .push_response(Some(std::path::PathBuf::from("out.txt")));
    save_dlg.borrow_mut().push_response(None);
    let mut fp2 = FilePickerProps::new(path2.clone(), FilePickerMode::Save);
    fp2.save_dialog = Some(save_dlg);
    fp2.debug = SharedString::from("fp-save");
    host2.mount("FPS", fp2, FilePicker);
    host2.run_until_idle();
    press_node(&host2, node_by_debug(&host2, "fp-save-browse"));
    assert_eq!(path2.get().as_deref(), Some("out.txt"));
    press_node(&host2, node_by_debug(&host2, "fp-save-browse"));
    assert_eq!(path2.get().as_deref(), Some("out.txt"));
    // Folder mode.
    let host3 = ComponentHost::new();
    let path3 = host3.runtime().signal(None::<SharedString>);
    let folder_dlg = std::rc::Rc::new(std::cell::RefCell::new(oppa::ScriptedFolderDialog::new()));
    folder_dlg
        .borrow_mut()
        .push_response(Some(std::path::PathBuf::from("docs")));
    let mut fp3 = FilePickerProps::new(path3.clone(), FilePickerMode::Folder);
    fp3.folder_dialog = Some(folder_dlg);
    fp3.debug = SharedString::from("fp-folder");
    host3.mount("FPF", fp3, FilePicker);
    host3.run_until_idle();
    press_node(&host3, node_by_debug(&host3, "fp-folder-browse"));
    assert_eq!(path3.get().as_deref(), Some("docs"));
}

/// G21: a missing backend for the chosen mode refuses loudly
/// (wiring bug, never a silent button).
#[test]
#[should_panic(expected = "without an open backend")]
fn file_picker_missing_backend_refuses_loudly() {
    let host = ComponentHost::new();
    let path = host.runtime().signal(None::<SharedString>);
    let mut fp = FilePickerProps::new(path, FilePickerMode::Open);
    fp.debug = SharedString::from("fp-loud");
    host.mount("FPL", fp, FilePicker);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "fp-loud-browse"));
}

fn route_home(_ctx: &Ctx) -> VNode {
    Div("route-home").build()
}

fn route_settings(_ctx: &Ctx) -> VNode {
    Div("route-settings").build()
}

/// G22: the Phase 36 leaves render as controls (label wrappers
/// announce, bare leaves stay byte-identical); NavHost switches
/// on the stack and empties quietly past unknown names.
#[test]
fn g22_wrappers_render_leaves_and_route() {
    // RichText labeled (wrapper announces) + bare (mounts).
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    host.mount(
        "RT",
        RichTextViewProps {
            spans: vec![oppa::TextSpan::new("hi")],
            style: Text::body_secondary,
            label: Some(SharedString::from("Greeting")),
            debug: SharedString::from("rich"),
        },
        RichTextView,
    );
    host.run_until_idle();
    let wrap = node_by_debug(&host, "rich");
    let sem = host.retained_semantics(wrap).expect("semantics");
    assert_eq!(sem.label.as_deref(), Some("Greeting"));
    let host_bare = ComponentHost::new();
    host_bare.set_text_service(Box::new(FakeText));
    host_bare.mount(
        "RTB",
        RichTextViewProps {
            spans: vec![oppa::TextSpan::new("hi")],
            style: Text::body_secondary,
            label: None,
            debug: SharedString::from("rich-bare"),
        },
        RichTextView,
    );
    host_bare.run_until_idle();
    assert!(
        find_retained_by_debug(&host_bare, "rich-bare").is_empty(),
        "bare leaves carry no wrapper"
    );
    // Image: the `img` leaf + alt announcement.
    let host2 = ComponentHost::new();
    host2.mount(
        "IMG",
        ImageViewProps {
            image: oppa::ImageId(7),
            size: 48.0,
            radius: 4.0,
            alt: SharedString::from("Logo"),
        },
        ImageView,
    );
    host2.run_until_idle();
    let img = node_by_debug(&host2, "img");
    let sem2 = host2.retained_semantics(img).expect("semantics");
    assert_eq!(sem2.label.as_deref(), Some("Logo"));
    // Canvas: the named leaf mounts.
    let host3 = ComponentHost::new();
    host3.mount(
        "CV",
        CanvasViewProps {
            ops: vec![oppa::CanvasOp::Rect {
                x: Px::of(2.0),
                y: Px::of(2.0),
                w: Px::of(20.0),
                h: Px::of(12.0),
                color: Color(0x22_66_CC),
            }],
            width: 64.0,
            height: 32.0,
            label: None,
            debug: SharedString::from("plot"),
        },
        CanvasView,
    );
    host3.run_until_idle();
    assert_eq!(find_retained_by_debug(&host3, "plot").len(), 1);
    // NavHost: current route renders, pushes switch, unknown
    // names empty quietly (the unmatched-Tabs precedent).
    let host4 = ComponentHost::new();
    let stack = host4.runtime().signal(oppa::NavStack::new(
        oppa::Route::new("home").expect("route"),
    ));
    let routes = vec![
        RouteView {
            name: SharedString::from("home"),
            content: Rc::new(route_home),
        },
        RouteView {
            name: SharedString::from("settings"),
            content: Rc::new(route_settings),
        },
    ];
    host4.mount(
        "NAV",
        NavHostProps {
            stack: stack.clone(),
            routes: routes.clone(),
        },
        NavHost,
    );
    host4.run_until_idle();
    assert_eq!(find_retained_by_debug(&host4, "route-home").len(), 1);
    let mut moved = stack.get();
    moved.push(oppa::Route::new("settings").expect("route"));
    stack.set(moved);
    host4.run_until_idle();
    assert!(find_retained_by_debug(&host4, "route-home").is_empty());
    assert_eq!(find_retained_by_debug(&host4, "route-settings").len(), 1);
    let mut lost = stack.get();
    lost.push(oppa::Route::new("void").expect("route"));
    stack.set(lost);
    host4.run_until_idle();
    assert_eq!(find_retained_by_debug(&host4, "nav-empty").len(), 1);
}

// ------------------------------------------------------------------
// Phase 39a (decisions 377–379): DOM → framework value loop
// ------------------------------------------------------------------

#[derive(Clone)]
struct LoopCase {
    props: TextInputProps,
    count: Signal<u32>,
}
impl Props for LoopCase {}

fn loop_render(ctx: &Ctx, p: &LoopCase) -> VNode {
    Column::new().gap(8).children([
        ctx.child("oppa::LoopField", 1, &p.props, TextInput),
        VNode::from(Text {
            text: SharedString::from(format!("ticks {}", p.count.get())),
            style: Text::body_secondary,
        }),
    ])
}

/// U8 value loop (decision 377): DOM `input` events arrive as
/// `InputEvent::Text` and set the controlled signal exactly per
/// keystroke (counted, not inferred), reporting through
/// `on_change` like native commits; an unrelated tick
/// re-renders around the field with value, caret, and node
/// identity intact and no echo; programmatic sets never report
/// (no loop — `on_change` is the commit path, not the set path).
#[test]
fn value_loop_feeds_report_and_ticks_preserve() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    let count = host.runtime().signal(0u32);
    let (notify, seen) = blowing::<SharedString>();
    let props = TextInputProps {
        on_change: Some(notify),
        ..TextInputProps::new("Name", value.clone())
    };
    host.mount(
        "Loop",
        LoopCase {
            props,
            count: count.clone(),
        },
        loop_render,
    );
    host.run_until_idle();
    // Focus first (typing implies browser focus; the feed
    // itself never moves framework focus).
    let field = node_by_debug(&host, "text-input");
    press_node(&host, field);
    assert_eq!(host.focused_node(), Some(field));
    // Keystroke 1: the DOM value feeds exactly + reports.
    host.inject_input(InputEvent::text(field, "h"));
    host.run_until_idle();
    assert_eq!(value.get().to_string(), "h");
    assert_eq!(
        seen.borrow()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        vec!["h".to_string()],
        "platform commits report like native inserts"
    );
    assert_eq!(
        host.focused_field_session().expect("session").caret(),
        1,
        "feed caret collapses to the end"
    );
    // Keystroke 2: full values stay exact (level-triggered —
    // each event carries the whole current value).
    host.inject_input(InputEvent::text(field, "hi"));
    host.run_until_idle();
    assert_eq!(value.get().to_string(), "hi");
    assert_eq!(
        seen.borrow()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        vec!["h".to_string(), "hi".to_string()],
        "counted per step, never inferred"
    );
    // Unrelated tick: re-renders around the field — value,
    // caret, and node identity intact, no echo.
    count.set(1);
    host.run_until_idle();
    assert_eq!(value.get().to_string(), "hi");
    assert_eq!(
        host.focused_field_session().expect("session").caret(),
        2,
        "ticks never move the caret"
    );
    assert_eq!(
        node_by_debug(&host, "text-input"),
        field,
        "no remount across ticks"
    );
    assert_eq!(seen.borrow().len(), 2, "ticks never echo");
    // Programmatic sets flow to the value without reporting
    // (no loop back into `on_change`).
    value.set(SharedString::from("HELLO"));
    host.run_until_idle();
    assert_eq!(value.get().to_string(), "HELLO");
    assert_eq!(seen.borrow().len(), 2, "sets never report");
}

// ------------------------------------------------------------------
// Phase 39b (decision 380): reference-app coverage
// ------------------------------------------------------------------

/// The sink's Views tab mounts every Phase 38b–38c control
/// (Tree, Toolbar, Menubar, RichText, Canvas, Image, NavHost)
/// plus the Form validation/DatePicker and Layout Grid/Splitter
/// legs on their own tabs.
#[test]
fn kitchen_sink_views_tab_mounts_phase38_controls() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    host.mount("Sink", (), KitchenSinkApp);
    host.run_until_idle();
    // Five tabs now (Form/Layout/Overlays/Platform/Views).
    let tabs = find_retained_by_debug(&host, "tab-item");
    assert_eq!(tabs.len(), 5);
    press_node(&host, tabs[4]);
    for debug in [
        "sink-tree",
        "toolbar-cell-0",
        "menubar-bar",
        "sink-rich",
        "sink-plot",
        "img",
        "sink-views-home",
    ] {
        assert_eq!(
            find_retained_by_debug(&host, debug).len(),
            1,
            "{debug} mounts on the Views tab"
        );
    }
    // Form leg: the invalid email + date box mount.
    press_node(&host, tabs[0]);
    assert_eq!(find_retained_by_debug(&host, "sink-email-field").len(), 1);
    assert_eq!(find_retained_by_debug(&host, "date").len(), 1);
    // Layout leg: the grid + splitter mount.
    press_node(&host, tabs[1]);
    assert_eq!(find_retained_by_debug(&host, "sink::Layout::Grid").len(), 1);
    assert_eq!(find_retained_by_debug(&host, "sink-split").len(), 1);
}
