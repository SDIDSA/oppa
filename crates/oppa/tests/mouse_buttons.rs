//! Mouse button taxonomy (Round 9.2, decision 301): secondary taps
//! dispatch `SecondaryPress` + `ContextMenu` without ever firing the
//! primary `Press`; auxiliary taps stay quiet; chords never steal the
//! live capture; drags and swipes stay primary-only.

use oppa::{
    find_retained_by_debug, ComponentHost, Ctx, Div, InputEvent, NodeId, PointerButton, Props,
    SharedString, Signal, Style, Text, VNode,
};

#[derive(Clone)]
struct BtnProps {
    press: Signal<bool>,
    secondary: Signal<bool>,
    menu: Signal<bool>,
    drags: Signal<i32>,
}

impl Props for BtnProps {}

fn render_btn(_ctx: &Ctx, p: &BtnProps) -> VNode {
    let (press, secondary, menu, drags) = (
        p.press.clone(),
        p.secondary.clone(),
        p.menu.clone(),
        p.drags.clone(),
    );
    Div("screen").style(Style::new().size(400, 300)).child(
        Div("btn")
            .style(Style::new().size(96, 32))
            .on_press(move || press.set(true))
            .on_secondary_press(move || secondary.set(true))
            .on_context_menu(move || menu.set(true))
            .on_drag(move || drags.set(drags.get() + 1))
            .child(VNode::from(Text {
                text: SharedString::from("Menu"),
                style: Text::body_secondary,
            })),
    )
}

struct Rig {
    host: ComponentHost,
    props: BtnProps,
    center: (f32, f32),
}

impl Rig {
    fn new() -> Self {
        let host = ComponentHost::new();
        host.set_viewport(400.0, 300.0);
        let rt = host.runtime();
        let props = BtnProps {
            press: rt.signal(false),
            secondary: rt.signal(false),
            menu: rt.signal(false),
            drags: rt.signal(0),
        };
        host.mount("Btn", props.clone(), render_btn);
        host.run_until_idle();
        let id: NodeId = find_retained_by_debug(&host, "btn")[0];
        let b = host.committed_box(id).expect("button laid out");
        Self {
            host,
            props,
            center: (b.x + b.w / 2.0, b.y + b.h / 2.0),
        }
    }

    fn down(&self, button: PointerButton, x: f32, y: f32) {
        self.host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Down { button },
            x,
            y,
            modifiers: oppa::Modifiers::NONE,
        });
        self.host.run_until_idle();
    }

    fn up(&self, button: PointerButton, x: f32, y: f32) {
        self.host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Up { button },
            x,
            y,
            modifiers: oppa::Modifiers::NONE,
        });
        self.host.run_until_idle();
    }

    fn mv(&self, x: f32, y: f32) {
        self.host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Move,
            x,
            y,
            modifiers: oppa::Modifiers::NONE,
        });
        self.host.run_until_idle();
    }

    fn tap(&self, button: PointerButton) {
        let (x, y) = self.center;
        self.down(button, x, y);
        self.up(button, x, y);
    }
}

#[test]
fn secondary_tap_fires_secondary_and_menu_never_primary() {
    let rig = Rig::new();
    rig.tap(PointerButton::Secondary);
    assert!(
        !rig.props.press.get(),
        "secondary never fires primary press"
    );
    assert!(
        rig.props.secondary.get(),
        "secondary tap fires on_secondary_press"
    );
    assert!(rig.props.menu.get(), "secondary tap fires on_context_menu");
}

#[test]
fn primary_tap_fires_press_only() {
    let rig = Rig::new();
    rig.tap(PointerButton::Primary);
    assert!(rig.props.press.get(), "primary tap fires press");
    assert!(!rig.props.secondary.get(), "primary never fires secondary");
    assert!(!rig.props.menu.get(), "primary never fires menu");
}

#[test]
fn auxiliary_tap_stays_quiet() {
    let rig = Rig::new();
    rig.tap(PointerButton::Auxiliary);
    assert!(!rig.props.press.get(), "no primary press");
    assert!(!rig.props.secondary.get(), "no secondary press");
    assert!(!rig.props.menu.get(), "no menu");
}

#[test]
fn chord_never_steals_the_live_capture() {
    let rig = Rig::new();
    let (x, y) = rig.center;
    // Primary holds; secondary presses and releases mid-hold: the
    // capture stays primary-owned throughout, and only the primary
    // Up completes the tap (exactly one press).
    rig.down(PointerButton::Primary, x, y);
    let btn: NodeId = find_retained_by_debug(&rig.host, "btn")[0];
    assert_eq!(rig.host.capture_node_for(0), Some(btn));
    rig.down(PointerButton::Secondary, x, y);
    assert_eq!(
        rig.host.capture_node_for(0),
        Some(btn),
        "chord down keeps the primary capture"
    );
    rig.up(PointerButton::Secondary, x, y);
    assert_eq!(
        rig.host.capture_node_for(0),
        Some(btn),
        "chord release touches nothing"
    );
    assert!(!rig.props.press.get(), "no early press from the chord");
    assert!(!rig.props.secondary.get(), "no secondary from the chord");
    rig.up(PointerButton::Primary, x, y);
    assert!(
        rig.props.press.get(),
        "the owning Up completes exactly one tap"
    );
    assert!(!rig.props.secondary.get(), "still no secondary");
}

#[test]
fn secondary_drag_and_fling_stay_quiet() {
    let rig = Rig::new();
    let (x, y) = rig.center;
    // Secondary drag: Moves stream nothing into on_drag (primary-only
    // drags), and the far release is a quiet drag-release, not a tap.
    rig.down(PointerButton::Secondary, x, y);
    rig.mv(x + 60.0, y);
    rig.mv(x + 120.0, y);
    assert_eq!(rig.props.drags.get(), 0, "secondary moves never drag");
    rig.up(PointerButton::Secondary, x + 120.0, y);
    assert!(
        !rig.props.secondary.get(),
        "far release is quiet, not a tap"
    );
    assert!(!rig.props.menu.get(), "no menu on a drag release");
    // Primary drag still streams (the gate is button-scoped, not a
    // global drag kill).
    rig.down(PointerButton::Primary, x, y);
    rig.mv(x + 10.0, y);
    assert_eq!(rig.props.drags.get(), 1, "primary moves still drag");
    rig.up(PointerButton::Primary, x + 200.0, y + 200.0);
}
