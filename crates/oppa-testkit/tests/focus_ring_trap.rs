//! Round 23.2 (decision 334): keyboard focus modality, themed
//! focus rings, and modal focus-trap completeness — driven through
//! the public [`Harness`](oppa_testkit::Harness) plus the host
//! escape hatch for keys and plans (testkit composes public API
//! only, so these are app-achievable flows, not backdoors).

use std::rc::Rc;

use oppa::{
    input::keys, ComponentHost, Ctx, Div, DrawOp, Modifiers, Props, SharedString, Signal,
    ThemeTokens, VNode,
};
use oppa_controls::{Action, Button, ButtonProps, Checkbox, CheckboxProps, Modal, ModalProps};
use oppa_testkit::Harness;

fn tab(app: &Harness) {
    app.key(keys::TAB);
}

fn shift_tab(app: &Harness) {
    app.key_with(
        keys::TAB,
        Modifiers {
            shift: true,
            ..Modifiers::NONE
        },
    );
}

/// Focus-ring paint ops in a fresh CPU plan (the inset `Border`
/// paints border-colored shapes — `Rect`, or `RRect` where the
/// control rounds its corners — presence reads the plan, never
/// node labels).
fn ring_rects(app: &Harness) -> Vec<(f32, f32, f32, f32)> {
    let ring = ThemeTokens::light().focus_ring;
    let builder = oppa_cpu::FramePlanBuilder::new(1.0);
    let plan = app
        .host()
        .with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    plan.ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rect {
                x, y, w, h, color, ..
            } if *color == ring => Some((*x, *y, *w, *h)),
            DrawOp::RRect {
                x, y, w, h, color, ..
            } if *color == ring => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect()
}

#[derive(Clone)]
struct RingScreenProps {
    checked: Signal<bool>,
}

impl Props for RingScreenProps {}

fn ring_screen(ctx: &Ctx, p: &RingScreenProps) -> VNode {
    let press_a: Action = Rc::new(|| {});
    Div("screen").children([
        ctx.child(
            "oppa::RingA",
            1,
            &ButtonProps {
                label: SharedString::from("A"),
                enabled: true,
                width: 96.0,
                height: 32.0,
                debug: SharedString::from("ring-a"),
                on_press: press_a,
            },
            Button,
        ),
        ctx.child(
            "oppa::RingB",
            2,
            &CheckboxProps {
                label: SharedString::from("B"),
                checked: p.checked.clone(),
                enabled: true,
                on_change: None,
            },
            Checkbox,
        ),
    ])
}

fn ring_harness() -> Harness {
    let app = Harness::new();
    let checked = app.host().runtime().signal(false);
    app.mount("S", RingScreenProps { checked }, ring_screen);
    app
}

fn focused_debug(app: &Harness) -> String {
    let host: &ComponentHost = app.host();
    let focus = host.focused_node().expect("focus set");
    host.with_retained_mut(|rec, _| rec.get(focus).map(|n| n.debug.clone()).unwrap_or_default())
}

/// Tab sets `focus_visible` and rings the focused control;
/// pointer focus clears the modality (focused, ringless).
#[test]
fn tab_shows_ring_and_click_clears_it() {
    let app = ring_harness();
    assert!(
        ring_rects(&app).is_empty(),
        "unfocused controls paint no ring"
    );
    assert!(!app.host().focus_visible().get());
    tab(&app);
    assert!(
        app.host().focus_visible().get(),
        "Tab sets keyboard modality"
    );
    assert_eq!(focused_debug(&app), "ring-a", "Tab focuses first");
    let rings = ring_rects(&app);
    assert_eq!(rings.len(), 1, "one ring on the focused button: {rings:?}");
    // Pointer focus keeps focus, drops the modality (and the ring).
    app.tap("checkbox");
    assert_eq!(focused_debug(&app), "checkbox", "tap focuses the row");
    assert!(
        !app.host().focus_visible().get(),
        "pointer clears keyboard modality"
    );
    assert!(
        ring_rects(&app).is_empty(),
        "pointer-focused controls paint no ring"
    );
}

/// Shift+Tab walks back; the ring follows focus.
#[test]
fn shift_tab_walks_back_with_ring() {
    let app = ring_harness();
    tab(&app);
    tab(&app);
    assert_eq!(focused_debug(&app), "checkbox");
    assert_eq!(ring_rects(&app).len(), 1, "ring on the checkbox");
    shift_tab(&app);
    assert_eq!(focused_debug(&app), "ring-a", "Shift+Tab walks back");
    assert_eq!(ring_rects(&app).len(), 1, "ring follows focus");
}

#[derive(Clone)]
struct TrapScreenProps {
    open: Signal<bool>,
}

impl Props for TrapScreenProps {}

fn trap_screen(ctx: &Ctx, p: &TrapScreenProps) -> VNode {
    let press_out: Action = Rc::new(|| {});
    Div("screen").children([
        ctx.child(
            "oppa::Outside",
            1,
            &ButtonProps {
                label: SharedString::from("Outside"),
                enabled: true,
                width: 96.0,
                height: 32.0,
                debug: SharedString::from("trap-outside"),
                on_press: press_out,
            },
            Button,
        ),
        ctx.child(
            "oppa::TrapModal",
            2,
            &ModalProps::new("Trap", p.open.clone()),
            Modal,
        ),
    ])
}

/// Tab/Shift+Tab inside an open Modal cycle strictly among the
/// portal's focusable descendants, never escaping to the
/// background. (The backdrop is a press-owner tab stop by the
/// locked decision-96 rule, so first entry lands there; the next
/// Tab enters the card and the trap holds from then on.)
#[test]
fn modal_traps_tab_cycling() {
    let app = Harness::new();
    let open = app.host().runtime().signal(false);
    app.mount("S", TrapScreenProps { open: open.clone() }, trap_screen);
    // Focus the background control, then open under it.
    app.tap("trap-outside");
    assert_eq!(focused_debug(&app), "trap-outside");
    open.set(true);
    app.run_idle();
    // Tab enters the portal (backdrop stop first — press-owner by
    // rule 96), then the card.
    tab(&app);
    assert_eq!(focused_debug(&app), "modal-backdrop");
    tab(&app);
    assert_eq!(focused_debug(&app), "modal-cancel", "Tab enters the card");
    tab(&app);
    assert_eq!(focused_debug(&app), "modal-confirm", "Tab advances in-card");
    tab(&app);
    assert_eq!(
        focused_debug(&app),
        "modal-cancel",
        "Tab wraps inside the trap"
    );
    shift_tab(&app);
    assert_eq!(
        focused_debug(&app),
        "modal-confirm",
        "Shift+Tab wraps back inside the trap"
    );
    // Closing dissolves the trap: the background is reachable again.
    open.set(false);
    app.run_idle();
    tab(&app);
    assert_eq!(
        focused_debug(&app),
        "trap-outside",
        "closed trap restores the global order"
    );
}
