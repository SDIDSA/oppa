//! M5 acceptance: normalized input events + hit-testing + focus, core-side.
//!
//! The §4.1 Toggle ported from `m2_reconciler.rs` (same track/knob theme,
//! same match, same semantics expression) plus one additive focus-ring
//! border line — driven by real [`InputEvent`] payloads through the
//! host's hit-test router, never by test-driven signal writes. Pixel
//! proof lives in `oppa-cpu/tests/m5_toggle.rs`; this file proves the
//! framework primitives (hit-test, hover/press/focus transitions, the
//! cancel tripwire, Tab determinism, one-frame settle).

#![allow(non_snake_case)]

use oppa::{
    find_retained_by_debug, Color, ComponentHost, Ctx, Div, Ease, Event, EventKind, HandlerId,
    InputEvent, KeyState, MsExt, Portal, Props, Semantics, SharedString, Signal, Style, Transition,
    VNode,
};
use oppa_macros::{component, Props};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
struct ToggleTheme {
    track_on: Color,
    track_off: Color,
    track_hover: Color,
    track_pressed: Color,
    track_disabled: Color,
    knob: Color,
    knob_shadow: Color,
}

const THEME: ToggleTheme = ToggleTheme {
    track_on: Color(0x44_44_44),
    track_off: Color(0x55_55_55),
    track_hover: Color(0x33_33_33),
    track_pressed: Color(0x22_22_22),
    track_disabled: Color(0x11_11_11),
    knob: Color(0x66_66_66),
    knob_shadow: Color(0x77_77_77),
};

const FOCUS_RING: Color = Color(0xAA_BB_CC);

#[derive(Clone, Props)]
struct ToggleProps {
    label: SharedString,
    initial: bool,
    enabled: bool,
    on_change: HandlerId,
    theme: ToggleTheme,
}

#[component]
fn Toggle(ctx: &Ctx, props: &ToggleProps) -> VNode {
    let is_on = ctx.signal(props.initial);
    let hovered = ctx.hovered();
    let pressed = ctx.pressed();
    let focused = ctx.focused();

    let track = match (props.enabled, pressed.get(), hovered.get(), is_on.get()) {
        (false, _, _, _) => props.theme.track_disabled,
        (_, true, _, _) => props.theme.track_pressed,
        (_, _, true, _) => props.theme.track_hover,
        (_, _, _, true) => props.theme.track_on,
        _ => props.theme.track_off,
    };
    let knob_x = if is_on.get() { 23.0 } else { 3.0 };

    let rt = ctx.runtime();
    let on_change = props.on_change;

    // M5 additive line vs the M2 port: the focus ring. Same §4.1 match,
    // same track/knob shapes, same semantics expression.
    let mut track_style = Style::new()
        .size(44, 24)
        .radius(12)
        .bg(track)
        .opacity(props.enabled.then_some(1.0))
        .transition(Transition::new(120.ms(), Ease::Out));
    if focused.get() {
        track_style = track_style.border(2, FOCUS_RING);
    }

    oppa::Div("track")
        .style(track_style)
        .semantics(
            Semantics::switch()
                .checked(is_on.get())
                .label(&props.label)
                .disabled(!props.enabled),
        )
        .on_press(move || {
            is_on.set(!is_on.get());
            rt.dispatch(Event {
                kind: EventKind::Press,
                handler: on_change,
            });
        })
        .child(
            oppa::Div("knob")
                .style(
                    Style::new()
                        .size(18, 18)
                        .circle()
                        .bg(props.theme.knob)
                        .x(knob_x)
                        .shadow(1, 2, props.theme.knob_shadow),
                )
                .build(),
        )
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct ToggleRig {
    host: ComponentHost,
    changes: Rc<Cell<u32>>,
    #[allow(dead_code)]
    handle: oppa::MountHandle<ToggleProps>,
}

impl ToggleRig {
    fn new(initial: bool) -> Self {
        let host = ComponentHost::new();
        let changes = Rc::new(Cell::new(0u32));
        let on_change = HandlerId::from_symbol("test.m5.toggle.change");
        host.runtime().register_handler(on_change, {
            let changes = changes.clone();
            move || changes.set(changes.get() + 1)
        });
        let handle = host.mount(
            "Toggle",
            ToggleProps {
                label: Arc::from("Wi-Fi"),
                initial,
                enabled: true,
                on_change,
                theme: THEME,
            },
            Toggle,
        );
        host.run_until_idle();
        Self {
            host,
            changes,
            handle,
        }
    }

    fn track(&self) -> oppa::NodeId {
        find_retained_by_debug(&self.host, "track")[0]
    }

    fn knob(&self) -> oppa::NodeId {
        find_retained_by_debug(&self.host, "knob")[0]
    }

    fn checked(&self) -> Option<bool> {
        self.host
            .retained_semantics(self.track())
            .and_then(|s| s.checked)
    }
}

/// Track 44×24 at (0,0); knob 18×18 at x=3 (off) or x=23 (on).
/// (10,12) is inside the off-knob; (30,12) is track-only when off.
const KNOB_OFF_POINT: (f32, f32) = (10.0, 12.0);
const TRACK_ONLY_OFF_POINT: (f32, f32) = (30.0, 12.0);
const MISS_POINT: (f32, f32) = (100.0, 100.0);

// ---------------------------------------------------------------------------
// 1. Hit-test: knob-over-track overlap under the stated rule + loud misses
// ---------------------------------------------------------------------------

#[test]
fn hit_test_knob_overlap_and_miss() {
    let rig = ToggleRig::new(false);
    let (track, knob) = (rig.track(), rig.knob());
    assert_ne!(track, knob);

    // Off: knob at x=3..21 — (10,12) is the knob (deepest wins).
    assert_eq!(
        rig.host.hit_test(KNOB_OFF_POINT.0, KNOB_OFF_POINT.1),
        Some(knob),
        "knob-over-track resolves to the knob"
    );
    // (30,12) is past the knob — the track itself.
    assert_eq!(
        rig.host
            .hit_test(TRACK_ONLY_OFF_POINT.0, TRACK_ONLY_OFF_POINT.1),
        Some(track)
    );
    // Misses map to nothing, loudly (no silent root fallback).
    assert_eq!(rig.host.hit_test(MISS_POINT.0, MISS_POINT.1), None);
    assert_eq!(rig.host.hit_test(-5.0, 12.0), None);

    // Flip through the M2-compatible direct path (seam works both ways),
    // then the overlap rule follows the knob to its new position.
    let press = rig
        .host
        .retained_handlers(track)
        .into_iter()
        .find(|(k, _)| *k == EventKind::Press)
        .map(|(_, id)| id)
        .expect("press handler");
    rig.host.runtime().dispatch(Event {
        kind: EventKind::Press,
        handler: press,
    });
    rig.host.run_until_idle();
    assert_eq!(rig.checked(), Some(true));
    // Knob now at x=23..41: (30,12) is the knob, (10,12) is the track.
    assert_eq!(
        rig.host
            .hit_test(TRACK_ONLY_OFF_POINT.0, TRACK_ONLY_OFF_POINT.1),
        Some(knob)
    );
    assert_eq!(
        rig.host.hit_test(KNOB_OFF_POINT.0, KNOB_OFF_POINT.1),
        Some(track)
    );
}

// ---------------------------------------------------------------------------
// 2. Hover / press / focus primitive transitions via real input
// ---------------------------------------------------------------------------

#[test]
fn hover_press_focus_transitions_via_real_input() {
    let rig = ToggleRig::new(false);
    let track = rig.track();
    let root = rig.handle.root_instance();

    // Move over the track: hover writes through the hit-test.
    rig.host.inject_input(InputEvent::pointer_move(
        TRACK_ONLY_OFF_POINT.0,
        TRACK_ONLY_OFF_POINT.1,
    ));
    rig.host.run_until_idle();
    assert_eq!(rig.host.hovered_node(), Some(track));
    assert_eq!(
        rig.host.debug_instance_flags(root),
        (true, false, false),
        "hovered mirrors, pressed/focused clear"
    );

    // Down: capture + pressed + focus follows click.
    rig.host.inject_input(InputEvent::pointer_down(
        TRACK_ONLY_OFF_POINT.0,
        TRACK_ONLY_OFF_POINT.1,
    ));
    rig.host.run_until_idle();
    assert_eq!(rig.host.capture_node(), Some(track));
    assert_eq!(rig.host.focused_node(), Some(track));
    assert_eq!(
        rig.host.debug_instance_flags(root),
        (true, true, true),
        "pressed + focus mirror the down"
    );

    // Up on the target: dispatches (flips), clears pressed + capture.
    rig.host.inject_input(InputEvent::pointer_up(
        TRACK_ONLY_OFF_POINT.0,
        TRACK_ONLY_OFF_POINT.1,
    ));
    rig.host.run_until_idle();
    assert_eq!(rig.changes.get(), 1, "up on target dispatches");
    assert_eq!(rig.checked(), Some(true));
    assert_eq!(rig.host.capture_node(), None);
    assert_eq!(
        rig.host.debug_instance_flags(root),
        (true, false, true),
        "pressed clears, hover + focus hold"
    );
}

// ---------------------------------------------------------------------------
// 3. Cancel tripwire: press, cancel/leave, release outside → clean
// ---------------------------------------------------------------------------

#[test]
fn cancel_leaves_no_stuck_pressed() {
    let rig = ToggleRig::new(false);

    rig.host.inject_input(InputEvent::pointer_down(
        TRACK_ONLY_OFF_POINT.0,
        TRACK_ONLY_OFF_POINT.1,
    ));
    rig.host.run_until_idle();
    assert!(rig.host.capture_node().is_some(), "capture held");

    // The adversarial case: cancel, drag out, release outside.
    rig.host.inject_input(InputEvent::pointer_cancel());
    rig.host
        .inject_input(InputEvent::pointer_move(MISS_POINT.0, MISS_POINT.1));
    rig.host
        .inject_input(InputEvent::pointer_up(MISS_POINT.0, MISS_POINT.1));
    // M8 (decision 122): the Toggle's track carries a 120 ms transition,
    // so the commit frame settles the #7 substance (capture cleared, no
    // dispatch, state untouched — still same-frame) while the
    // interpolation tail runs its duration. Frame counts now read
    // commit-frame + tail; the substance asserts stay exactly as sharp.
    assert!(rig.host.run_once(), "cancel path drains in one frame");
    assert_eq!(rig.host.capture_node(), None, "capture cleared");
    assert_eq!(rig.changes.get(), 0, "cancel never dispatches");
    assert_eq!(rig.checked(), Some(false), "state untouched");
    rig.host.run_until_idle();
    assert_eq!(rig.changes.get(), 0, "tail dispatches nothing either");
    assert_eq!(rig.checked(), Some(false));

    // And the tail alone is safe: a release with no capture is a no-op.
    rig.host
        .inject_input(InputEvent::pointer_up(MISS_POINT.0, MISS_POINT.1));
    rig.host.run_until_idle();
    assert_eq!(rig.changes.get(), 0);
    assert_eq!(rig.checked(), Some(false));
}

#[test]
fn release_outside_after_leave_does_not_dispatch() {
    let rig = ToggleRig::new(false);

    rig.host.inject_input(InputEvent::pointer_down(
        TRACK_ONLY_OFF_POINT.0,
        TRACK_ONLY_OFF_POINT.1,
    ));
    rig.host.run_until_idle();
    // Drag out (no cancel message — the plain leave path)...
    rig.host
        .inject_input(InputEvent::pointer_move(MISS_POINT.0, MISS_POINT.1));
    // ...then release outside: pressed clears, nothing dispatches.
    rig.host
        .inject_input(InputEvent::pointer_up(MISS_POINT.0, MISS_POINT.1));
    rig.host.run_until_idle();

    assert_eq!(rig.host.capture_node(), None);
    assert_eq!(rig.host.hovered_node(), None, "drag-out clears hover");
    assert_eq!(rig.changes.get(), 0, "release outside never toggles");
    assert_eq!(rig.checked(), Some(false));
}

// ---------------------------------------------------------------------------
// 4. Press on the knob routes to the track (handler-less child)
// ---------------------------------------------------------------------------

#[test]
fn press_on_knob_routes_to_track_handler() {
    let rig = ToggleRig::new(false);

    // Down on the knob (which carries no handler of its own)...
    rig.host
        .inject_input(InputEvent::pointer_down(KNOB_OFF_POINT.0, KNOB_OFF_POINT.1));
    rig.host.run_until_idle();
    assert_eq!(
        rig.host.hovered_node(),
        Some(rig.knob()),
        "hover reports the raw hit"
    );
    assert_eq!(
        rig.host.capture_node(),
        Some(rig.track()),
        "capture resolves to the press owner"
    );

    // Up on the knob (inside the capture subtree) dispatches.
    rig.host
        .inject_input(InputEvent::pointer_up(KNOB_OFF_POINT.0, KNOB_OFF_POINT.1));
    rig.host.run_until_idle();
    assert_eq!(rig.changes.get(), 1);
    assert_eq!(rig.checked(), Some(true));
}

// ---------------------------------------------------------------------------
// 5. Tab order: deterministic, Tab/Shift+Tab walk it, repeats identical
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct ThreeProps;

#[component]
fn ThreeToggles(ctx: &Ctx, _props: &ThreeProps) -> VNode {
    // Unrolled (not looped): one signal per call site, per the §5.1 rule.
    let a_on = ctx.signal(false);
    let b_on = ctx.signal(true);
    let c_on = ctx.signal(false);
    let mk = |debug: &'static str, on: oppa::Signal<bool>| {
        let on2 = on.clone();
        oppa::Div(debug)
            .style(Style::new().size(44, 24).bg(Color(0x55_55_55)))
            .semantics(Semantics::switch().checked(on.get()))
            .on_press(move || on2.set(!on2.get()))
            .build()
    };
    oppa::Column::new().children([mk("t0", a_on), mk("b1", b_on), mk("c2", c_on)])
}

fn mount_three() -> ComponentHost {
    let host = ComponentHost::new();
    host.mount("Three", ThreeProps, ThreeToggles);
    host.run_until_idle();
    host
}

fn debug_order(host: &ComponentHost) -> Vec<String> {
    host.tab_order()
        .iter()
        .map(|id| {
            host.with_retained_mut(|rec, _| {
                rec.get(*id).map(|n| n.debug.clone()).unwrap_or_default()
            })
        })
        .collect()
}

#[test]
fn tab_order_deterministic_and_tab_walks_it() {
    let host = mount_three();
    // Same tree twice → same order (asserted against a fresh build).
    let host2 = mount_three();
    assert_eq!(debug_order(&host), debug_order(&host2));
    assert_eq!(debug_order(&host), vec!["t0", "b1", "c2"]);

    // Tab walks forward with wrap; the walk repeats identically from
    // the same starting focus (blur first so both walks start clean).
    let walk = |host: &ComponentHost| {
        host.inject_input(InputEvent::key(
            oppa::input::keys::ESCAPE,
            KeyState::Pressed,
        ));
        host.run_until_idle();
        let mut seen = Vec::new();
        for _ in 0..4 {
            host.inject_input(InputEvent::key(oppa::input::keys::TAB, KeyState::Pressed));
            host.run_until_idle();
            seen.push(host.with_retained_mut(|rec, _| {
                rec.get(host.focused_node().expect("focus"))
                    .map(|n| n.debug.clone())
                    .unwrap_or_default()
            }));
        }
        seen
    };
    assert_eq!(walk(&host), vec!["t0", "b1", "c2", "t0"]);
    assert_eq!(walk(&host2), walk(&host), "repeat runs identical");

    // Shift+Tab walks backward with wrap.
    host.inject_input(InputEvent::Key {
        code: oppa::input::keys::TAB,
        modifiers: oppa::Modifiers::shift(),
        state: KeyState::Pressed,
        repeat: false,
    });
    host.run_until_idle();
    let back = host.with_retained_mut(|rec, _| {
        rec.get(host.focused_node().expect("focus"))
            .map(|n| n.debug.clone())
            .unwrap_or_default()
    });
    assert_eq!(back, "c2", "shift+tab from t0 wraps to last");
}

// ---------------------------------------------------------------------------
// 6. Keyboard activation + Escape + quiet unhandled keys
// ---------------------------------------------------------------------------

#[test]
fn keyboard_space_activates_focused_and_ends_clean() {
    let rig = ToggleRig::new(false);
    let track = rig.track();

    // Focus via Tab (single focusable → itself), then Space toggles.
    rig.host
        .inject_input(InputEvent::key(oppa::input::keys::TAB, KeyState::Pressed));
    rig.host.run_until_idle();
    assert_eq!(rig.host.focused_node(), Some(track));

    rig.host
        .inject_input(InputEvent::key(oppa::input::keys::SPACE, KeyState::Pressed));
    rig.host.run_until_idle();
    assert_eq!(rig.changes.get(), 1, "space activates");
    assert_eq!(rig.checked(), Some(true));
    assert_eq!(rig.host.capture_node(), None, "no capture leaks");
}

#[test]
fn escape_clears_focus_and_other_keys_are_quiet() {
    let rig = ToggleRig::new(false);

    rig.host
        .inject_input(InputEvent::key(oppa::input::keys::TAB, KeyState::Pressed));
    rig.host.run_until_idle();
    assert!(rig.host.focused_node().is_some());

    rig.host.inject_input(InputEvent::key(
        oppa::input::keys::ESCAPE,
        KeyState::Pressed,
    ));
    rig.host.run_until_idle();
    assert_eq!(rig.host.focused_node(), None, "escape blurs");

    // An unhandled key with no focus is an accepted no-op, never loud.
    rig.host
        .inject_input(InputEvent::key(0x41, KeyState::Pressed));
    rig.host.run_until_idle();
    assert_eq!(rig.changes.get(), 0);
    assert_eq!(rig.checked(), Some(false));
}

// ---------------------------------------------------------------------------
// 7. One frame: event → settled state within a single frame
// ---------------------------------------------------------------------------

#[test]
fn one_frame_input_to_settled_state() {
    let rig = ToggleRig::new(false);

    rig.host.inject_input(InputEvent::pointer_down(
        TRACK_ONLY_OFF_POINT.0,
        TRACK_ONLY_OFF_POINT.1,
    ));
    rig.host.inject_input(InputEvent::pointer_up(
        TRACK_ONLY_OFF_POINT.0,
        TRACK_ONLY_OFF_POINT.1,
    ));
    // M8 (decision 122): commit-frame + tail (see the cancel test) — the
    // down+up drain in INPUT and settle the toggle state in the same
    // frame; the track-bg interpolation tail follows its duration.
    assert!(
        rig.host.run_once(),
        "down+up drain in INPUT, settle same frame"
    );
    assert_eq!(rig.checked(), Some(true));
    assert_eq!(rig.changes.get(), 1);
    rig.host.run_until_idle();
}

// ---------------------------------------------------------------------------
// 8. Loud routing failures (never silent behavior changes)
// ---------------------------------------------------------------------------

fn silence() {
    std::panic::set_hook(Box::new(|_| {}));
}

fn restore() {
    let _ = std::panic::take_hook();
}

fn panic_text(payload: &Box<dyn std::any::Any + Send>) -> String {
    (**payload)
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| (**payload).downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

#[test]
fn scroll_ime_and_stale_focus_targets_fail_loudly() {
    // One fresh host per death case: a caught panic mid-INPUT can drop
    // the phase's hook restoration, so post-panic hosts are not reused.
    let cases: Vec<InputEvent> = {
        let probe = ToggleRig::new(false);
        let track = probe.track();
        vec![
            InputEvent::Scroll {
                target: track,
                dx: 0.0,
                dy: 10.0,
            },
            InputEvent::Ime { target: track },
            InputEvent::Focus {
                node: Some(oppa::NodeId::new(9999, 0)),
            },
        ]
    };
    for ev in cases {
        let rig = ToggleRig::new(false);
        silence();
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rig.host.inject_input(ev.clone());
            rig.host.run_until_idle();
        }))
        .unwrap_err();
        restore();
        let text = panic_text(&payload);
        assert!(
            text.contains("no ") || text.contains("not a live"),
            "loud refusal, got: {text}"
        );
    }
}

// ---------------------------------------------------------------------------
// Round 1.4 (decision 255): overlay portals — top-layer hit testing,
// unmount clearing focus traps and active captures.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct LayerProps {
    overlay_presses: Rc<Cell<u32>>,
    app_presses: Rc<Cell<u32>>,
}

impl Props for LayerProps {}

/// Portal listed FIRST so only portal priority can explain an overlay
/// win (plain sibling order would hand the point to the later app).
fn render_layered(_ctx: &Ctx, props: &LayerProps) -> VNode {
    let overlay = props.overlay_presses.clone();
    let app = props.app_presses.clone();
    Div("root").children([
        Portal("p").child(
            Div("overlay")
                .style(Style::new().size(100, 20))
                .on_press(move || overlay.set(overlay.get() + 1))
                .build(),
        ),
        Div("app")
            .style(Style::new().size(100, 20))
            .on_press(move || app.set(app.get() + 1))
            .build(),
    ])
}

#[test]
fn portal_wins_hit_test_over_app_content() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let overlay = Rc::new(Cell::new(0u32));
    let app = Rc::new(Cell::new(0u32));
    host.mount(
        "layers",
        LayerProps {
            overlay_presses: overlay.clone(),
            app_presses: app.clone(),
        },
        render_layered,
    );
    host.run_until_idle();
    let overlay_id = find_retained_by_debug(&host, "overlay")[0];
    // (10,10) sits inside both boxes — the portal leaf wins despite
    // being the earlier sibling.
    assert_eq!(host.hit_test(10.0, 10.0), Some(overlay_id));
    host.inject_input(InputEvent::pointer_down(10.0, 10.0));
    host.inject_input(InputEvent::pointer_up(10.0, 10.0));
    host.run_until_idle();
    assert_eq!(overlay.get(), 1, "overlay press dispatched");
    assert_eq!(app.get(), 0, "app under the overlay never fires");
}

#[derive(Clone)]
struct ShowProps {
    show: Signal<bool>,
    overlay_presses: Rc<Cell<u32>>,
}

impl Props for ShowProps {}

fn render_show(_ctx: &Ctx, props: &ShowProps) -> VNode {
    if props.show.get() {
        let overlay = props.overlay_presses.clone();
        Div("root").child(
            Portal("p").child(
                Div("overlay")
                    .style(Style::new().size(100, 20))
                    .on_press(move || overlay.set(overlay.get() + 1))
                    .build(),
            ),
        )
    } else {
        Div("root").child(Div("plain").style(Style::new().size(100, 20)).build())
    }
}

#[test]
fn unmounting_portal_clears_focus_and_capture() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let show = host.runtime().signal(true);
    let presses = Rc::new(Cell::new(0u32));
    host.mount(
        "show",
        ShowProps {
            show: show.clone(),
            overlay_presses: presses.clone(),
        },
        render_show,
    );
    host.run_until_idle();
    // Down on the overlay: capture + focus follow the press owner.
    host.inject_input(InputEvent::pointer_down(10.0, 10.0));
    host.run_until_idle();
    assert_eq!(host.capture_count(), 1, "capture live during press");
    assert!(host.focused_node().is_some(), "focus follows click");
    // Unmount mid-press: the portal subtree retires.
    show.set(false);
    host.run_until_idle();
    assert!(
        find_retained_by_debug(&host, "overlay").is_empty(),
        "portal subtree unmounted"
    );
    assert_eq!(
        host.focused_node(),
        None,
        "no focus trap survives the overlay"
    );
    assert_eq!(host.capture_count(), 0, "capture released on unmount");
    // The matching Up is a quiet stray — no panic, no ghost dispatch.
    host.inject_input(InputEvent::pointer_up(10.0, 10.0));
    host.run_until_idle();
    assert_eq!(presses.get(), 0, "retired owner never dispatches");
}

// ---------------------------------------------------------------------------
// 9. Round 5.1 (decision 268): Enter in a multi-line field inserts
// a newline instead of activating (single-line fields and buttons
// keep the M5 press rule, unchanged).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AreaProps {
    value: Signal<SharedString>,
}

impl oppa::Props for AreaProps {}

fn AreaScene(ctx: &Ctx, props: &AreaProps) -> VNode {
    let session = ctx.edit_session(props.value.clone());
    let to_end = session.clone();
    let _ = session;
    oppa::Div("area-box")
        .style(Style::new().size(200, 64))
        .semantics(Semantics::text_area().label("Notes"))
        .on_press(move || to_end.caret_to_end())
        .child(VNode::from(oppa::TextArea {
            text: props.value.get(),
            style: oppa::Text::body_secondary,
            label: SharedString::from("Notes"),
        }))
}

#[test]
fn enter_in_textarea_inserts_newline_never_activates() {
    let host = ComponentHost::new();
    host.set_viewport(300.0, 200.0);
    let value = host.runtime().signal(SharedString::from("ab"));
    host.mount(
        "Area",
        AreaProps {
            value: value.clone(),
        },
        AreaScene,
    );
    host.run_until_idle();
    // Click-focus parks the caret at the end (the control rule).
    let field = find_retained_by_debug(&host, "area-box")[0];
    let b = host.committed_box(field).expect("field box");
    host.inject_input(InputEvent::pointer_down(b.x + b.w / 2.0, b.y + b.h / 2.0));
    host.inject_input(InputEvent::pointer_up(b.x + b.w / 2.0, b.y + b.h / 2.0));
    host.run_until_idle();
    assert_eq!(host.focused_node(), Some(field));
    assert!(host.focused_field_session().is_some());
    // Enter inserts a newline at the caret (no press dispatch).
    host.inject_input(InputEvent::key(oppa::input::keys::ENTER, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(&*value.get(), "ab\n", "enter inserts newline");
    // Repeat Enter is ignored (held keys never spam newlines).
    host.inject_input(InputEvent::Key {
        code: oppa::input::keys::ENTER,
        modifiers: oppa::Modifiers::NONE,
        state: KeyState::Pressed,
        repeat: true,
    });
    host.run_until_idle();
    assert_eq!(&*value.get(), "ab\n", "repeat enter ignored");
}

#[test]
fn enter_outside_textarea_still_activates() {
    // Guard: the textarea rule diverts nothing else (SPACE on a
    // focused toggle still toggles — section 6 proves it; ENTER on
    // a focused toggle still presses here).
    let rig = ToggleRig::new(false);
    let track = rig.track();
    rig.host
        .inject_input(InputEvent::key(oppa::input::keys::TAB, KeyState::Pressed));
    rig.host.run_until_idle();
    assert_eq!(rig.host.focused_node(), Some(track));
    rig.host
        .inject_input(InputEvent::key(oppa::input::keys::ENTER, KeyState::Pressed));
    rig.host.run_until_idle();
    assert_eq!(rig.changes.get(), 1, "enter activates buttons");
    assert_eq!(rig.checked(), Some(true));
}

// ---------------------------------------------------------------------------
// 10. Round 5.3 (decision 270): directional arrow keys — the
// focused owner's directional handler wins, else the generic Key
// handler, else quiet. Held arrows repeat (no suppression).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct KeyPadProps {
    left: Signal<SharedString>,
    right: Signal<SharedString>,
}

impl oppa::Props for KeyPadProps {}

fn key_pad_scene(_ctx: &Ctx, props: &KeyPadProps) -> VNode {
    let (left, right) = (props.left.clone(), props.right.clone());
    oppa::Div("pad")
        .style(Style::new().size(200, 64))
        .semantics(Semantics::switch().label("pad"))
        .on_press(|| {})
        .on_key_left(move || left.set(SharedString::from("L")))
        .on_key_right(move || right.set(SharedString::from("R")))
        .build()
}

fn key_pad_harness() -> (ComponentHost, Signal<SharedString>, Signal<SharedString>) {
    let host = ComponentHost::new();
    host.set_viewport(300.0, 200.0);
    let left = host.runtime().signal(SharedString::from("-"));
    let right = host.runtime().signal(SharedString::from("-"));
    host.mount(
        "Pad",
        KeyPadProps {
            left: left.clone(),
            right: right.clone(),
        },
        key_pad_scene,
    );
    host.run_until_idle();
    let pad = find_retained_by_debug(&host, "pad")[0];
    let b = host.committed_box(pad).expect("pad box");
    host.inject_input(InputEvent::pointer_down(b.x + b.w / 2.0, b.y + b.h / 2.0));
    host.inject_input(InputEvent::pointer_up(b.x + b.w / 2.0, b.y + b.h / 2.0));
    host.run_until_idle();
    assert_eq!(host.focused_node(), Some(pad));
    (host, left, right)
}

#[test]
fn arrows_dispatch_directionally_with_repeat() {
    let (host, left, right) = key_pad_harness();
    host.inject_input(InputEvent::key(oppa::input::keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(&*left.get(), "L");
    assert_eq!(&*right.get(), "-");
    host.inject_input(InputEvent::key(oppa::input::keys::RIGHT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(&*right.get(), "R");
    // Held arrows repeat-step (no suppression — standard).
    host.inject_input(InputEvent::Key {
        code: oppa::input::keys::LEFT,
        modifiers: oppa::Modifiers::NONE,
        state: KeyState::Pressed,
        repeat: true,
    });
    host.run_until_idle();
    assert_eq!(&*left.get(), "L", "repeat still dispatches");
    // Up/Down with no declared handler fall back to generic Key —
    // none declared here either, so quiet (ambient rule).
    host.inject_input(InputEvent::key(oppa::input::keys::UP, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(&*left.get(), "L");
    assert_eq!(&*right.get(), "R");
}

// ---------------------------------------------------------------------------
// Portal transparency (Task Studio follow-up): a handlerless overlay
// portal never claims a hit through its own box — only its children
// participate. An attached `Scrollbar` portal spans its whole target;
// claiming swallowed wheel/scroll plus press routing for every row
// beneath it.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct OverlayProps;
impl Props for OverlayProps {}

fn overlay_scene(_ctx: &Ctx, _: &OverlayProps) -> VNode {
    Div("screen").style(Style::new().size(200, 200)).children([
        oppa::ScrollArea("list")
            .style(Style::new().size(200, 200))
            .on_scroll(|| {})
            .child(Div("row").style(Style::new().size(200, 40)).build()),
        Portal("veil")
            .style(Style::new().size(200, 200))
            .child(Div("chip").style(Style::new().size(20, 20).x(180)).build()),
    ])
}

#[test]
fn handlerless_portal_never_claims_hits() {
    let host = ComponentHost::new();
    host.set_viewport(400.0, 300.0);
    host.mount("Overlay", OverlayProps, overlay_scene);
    host.run_until_idle();
    let list = find_retained_by_debug(&host, "list")[0];
    let row = find_retained_by_debug(&host, "row")[0];
    let chip = find_retained_by_debug(&host, "chip")[0];
    // Inside the veil's box but over the row: the veil must not
    // claim it — the row wins.
    assert_eq!(host.hit_test(60.0, 10.0), Some(row));
    // Inside the veil's box but past the row: falls through to the
    // area itself, so wheel routing still resolves.
    assert_eq!(host.hit_test(60.0, 100.0), Some(list));
    assert_eq!(host.scroll_target_at(60.0, 100.0), Some(list));
    // The veiled chip still wins on its own box (children first).
    assert_eq!(host.hit_test(190.0, 10.0), Some(chip));
}
