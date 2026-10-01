//! G11 multi-pointer + long-press (decisions 227–229).
//!
//! The router holds one capture per pointer id; holding still past
//! `LONG_PRESS_TIMEOUT_S` fires the same press handler a tap would.
//! Time comes from the injected clock (`MockClock` here — deterministic;
//! `SystemClock` on device), and arms never create frame demand
//! (a held finger can neither spin the loop nor hang `run_until_idle`).

use std::rc::Rc;

use oppa::{find_retained_by_debug, ComponentHost, Ctx, Div, InputEvent, MockClock, Props, VNode};

#[derive(Clone)]
struct TwoButtons {
    count_a: oppa::Signal<u32>,
    count_b: oppa::Signal<u32>,
}

impl Props for TwoButtons {}

fn two_buttons(_ctx: &Ctx, p: &TwoButtons) -> VNode {
    let (a, b) = (p.count_a.clone(), p.count_b.clone());
    oppa::Row("row").children([
        Div("a")
            .style(oppa::Style::new().size(44, 24))
            .on_press(move || a.set(a.get() + 1))
            .build(),
        Div("b")
            .style(oppa::Style::new().size(44, 24))
            .on_press(move || b.set(b.get() + 1))
            .build(),
    ])
}

struct Rig {
    host: ComponentHost,
    props: TwoButtons,
    #[allow(dead_code)]
    handle: oppa::MountHandle<TwoButtons>,
    ax: f32,
    ay: f32,
    bx: f32,
    by: f32,
}

impl Rig {
    fn new() -> Self {
        Self::with_clock(Rc::new(MockClock::new()))
    }

    fn with_clock(clock: Rc<MockClock>) -> Self {
        let host = ComponentHost::with_clock(clock);
        host.set_viewport(100.0, 60.0);
        let rt = host.runtime();
        let props = TwoButtons {
            count_a: rt.signal(0u32),
            count_b: rt.signal(0u32),
        };
        let handle = host.mount("Two", props.clone(), two_buttons);
        host.run_until_idle();
        let center = |debug: &str| {
            let id = find_retained_by_debug(&host, debug)[0];
            let b = host.committed_box(id).expect("button box");
            (b.x + b.w / 2.0, b.y + b.h / 2.0)
        };
        let (ax, ay) = center("a");
        let (bx, by) = center("b");
        Self {
            host,
            props,
            handle,
            ax,
            ay,
            bx,
            by,
        }
    }

    fn down(&self, id: u32, which: char) {
        let (x, y) = if which == 'a' {
            (self.ax, self.ay)
        } else {
            (self.bx, self.by)
        };
        self.host
            .inject_input(InputEvent::pointer_down_id(id, x, y));
        self.host.run_until_idle();
    }

    fn up(&self, id: u32, which: char) {
        let (x, y) = if which == 'a' {
            (self.ax, self.ay)
        } else {
            (self.bx, self.by)
        };
        self.host.inject_input(InputEvent::pointer_up_id(id, x, y));
        self.host.run_until_idle();
    }
}

#[test]
fn two_pointers_dispatch_independently() {
    let rig = Rig::new();
    rig.down(0, 'a');
    rig.down(1, 'b');
    assert_eq!(rig.host.capture_count(), 2);
    assert!(rig.host.capture_node_for(0).is_some());
    assert!(rig.host.capture_node_for(1).is_some());
    assert_eq!(
        (rig.props.count_a.get(), rig.props.count_b.get()),
        (0, 0),
        "down never dispatches"
    );
    rig.up(0, 'a');
    assert_eq!(
        (rig.props.count_a.get(), rig.props.count_b.get()),
        (1, 0),
        "first finger dispatches"
    );
    assert!(rig.host.capture_node_for(0).is_none());
    assert!(
        rig.host.capture_node_for(1).is_some(),
        "second finger still held"
    );
    rig.up(1, 'b');
    assert_eq!(
        (rig.props.count_a.get(), rig.props.count_b.get()),
        (1, 1),
        "second finger dispatches"
    );
    assert_eq!(rig.host.capture_count(), 0);
}

#[test]
fn shared_owner_flag_clears_on_last_lift() {
    // Two fingers, one button: the per-instance pressed flag is shared
    // (decision 227) — it holds while ANY finger is down.
    let rig = Rig::new();
    let root = rig.handle.root_instance();
    rig.down(0, 'a');
    rig.down(1, 'a');
    assert!(rig.host.debug_instance_flags(root).1, "pressed while held");
    rig.up(0, 'a');
    assert_eq!(rig.props.count_a.get(), 1);
    assert!(
        rig.host.debug_instance_flags(root).1,
        "first lift dispatches but the flag holds for finger 1"
    );
    rig.up(1, 'a');
    assert_eq!(rig.props.count_a.get(), 2);
    assert!(!rig.host.debug_instance_flags(root).1, "last lift clears");
}

#[test]
fn cancel_for_clears_one_global_clears_all() {
    let rig = Rig::new();
    rig.down(0, 'a');
    rig.down(1, 'b');
    rig.host.inject_input(InputEvent::pointer_cancel_for(0));
    rig.host.run_until_idle();
    assert!(rig.host.capture_node_for(0).is_none());
    assert!(rig.host.capture_node_for(1).is_some());
    assert_eq!(
        (rig.props.count_a.get(), rig.props.count_b.get()),
        (0, 0),
        "cancel never dispatches"
    );
    rig.host.inject_input(InputEvent::pointer_cancel());
    rig.host.run_until_idle();
    assert_eq!(rig.host.capture_count(), 0, "global tripwire clears all");
    let root = rig.handle.root_instance();
    assert!(
        !rig.host.debug_instance_flags(root).1,
        "no stuck pressed anywhere (hover/focus hold per M5)"
    );
}

#[test]
fn capture_node_reads_lowest_id() {
    // Legacy single-capture read stays deterministic under G11.
    let rig = Rig::new();
    rig.down(5, 'b');
    rig.down(2, 'a');
    let a = find_retained_by_debug(&rig.host, "a")[0];
    assert_eq!(rig.host.capture_node(), Some(a));
}

#[test]
fn longpress_fires_on_pump_past_deadline_without_double_dispatch() {
    let clock = Rc::new(MockClock::new());
    let rig = Rig::with_clock(clock.clone());
    rig.down(0, 'a');
    assert_eq!(rig.props.count_a.get(), 0);
    assert_eq!(rig.host.longpress_armed_count(), 1);
    // Holding idles cleanly (no self-demand — the M5 hold pattern):
    // a pump before the deadline fires nothing.
    clock.set(0.1);
    rig.host.run_until_idle();
    assert_eq!(rig.props.count_a.get(), 0, "early pump never fires");
    assert_eq!(rig.host.longpress_armed_count(), 1, "still armed");
    // Advance past the 0.5 s deadline, pump: fires while held.
    clock.set(oppa::input::LONG_PRESS_TIMEOUT_S + 0.1);
    rig.host.run_once();
    assert_eq!(rig.props.count_a.get(), 1, "hold fires past deadline");
    assert!(rig.host.capture_node_for(0).is_some(), "finger still held");
    // Release after the fire: consumed, never double.
    rig.up(0, 'a');
    assert_eq!(rig.props.count_a.get(), 1, "no double dispatch");
    assert_eq!(rig.host.capture_count(), 0);
    assert_eq!(rig.host.longpress_armed_count(), 0);
}

#[test]
fn longpress_tap_before_deadline_dispatches_once() {
    let clock = Rc::new(MockClock::new());
    let rig = Rig::with_clock(clock.clone());
    rig.down(0, 'a');
    clock.set(0.1);
    rig.up(0, 'a');
    assert_eq!(rig.props.count_a.get(), 1, "tap dispatches");
    // Later pumps find no arm — nothing more fires.
    clock.set(100.0);
    rig.host.run_until_idle();
    assert_eq!(rig.props.count_a.get(), 1);
}

#[test]
fn longpress_move_past_slop_disarms() {
    let clock = Rc::new(MockClock::new());
    let rig = Rig::with_clock(clock.clone());
    rig.down(0, 'a');
    // Drag to a true miss (outside both buttons): disarms the hold...
    // (ax + slop + 5 stays inside the 44px button — a tap there
    // correctly dispatches, so the miss must clear both boxes.)
    rig.host
        .inject_input(InputEvent::pointer_move_id(0, -10.0, -10.0));
    rig.host.run_until_idle();
    assert_eq!(rig.host.longpress_armed_count(), 0, "slop disarms");
    clock.set(100.0);
    rig.host.run_until_idle();
    assert_eq!(rig.props.count_a.get(), 0, "disarmed hold never fires");
    // ...and the release-outside stays a no-op (M5 leave path).
    rig.host
        .inject_input(InputEvent::pointer_up_id(0, -10.0, -10.0));
    rig.host.run_until_idle();
    assert_eq!(rig.props.count_a.get(), 0);
}

#[test]
#[should_panic(expected = "without an id")]
fn id_less_down_is_malformed() {
    let rig = Rig::new();
    rig.host.inject_input(InputEvent::Pointer {
        id: None,
        action: oppa::PointerAction::Down {
            button: oppa::PointerButton::Primary,
        },
        x: rig.ax,
        y: rig.ay,
        modifiers: oppa::Modifiers::NONE,
    });
    rig.host.run_until_idle();
}

#[test]
fn hover_updates_from_any_pointer_id() {
    // Hover stays uniform (M5 rule, unchanged): moves from any id move it.
    let rig = Rig::new();
    rig.host
        .inject_input(InputEvent::pointer_move_id(3, rig.bx, rig.by));
    rig.host.run_until_idle();
    let b = find_retained_by_debug(&rig.host, "b")[0];
    assert_eq!(rig.host.hovered_node(), Some(b));
}

// ---------------------------------------------------------------------------
// OQ-G11-2 distinct hold actions: `on_long_press` fires on holds,
// taps still fire press, missing hold handlers fall back to press.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct HoldButton {
    press_count: oppa::Signal<u32>,
    hold_count: oppa::Signal<u32>,
    with_hold: bool,
}

impl Props for HoldButton {}

fn hold_button(_ctx: &Ctx, p: &HoldButton) -> VNode {
    let press = p.press_count.clone();
    let hold = p.hold_count.clone();
    let mut el = Div("hold")
        .style(oppa::Style::new().size(44, 24))
        .on_press(move || press.set(press.get() + 1));
    if p.with_hold {
        el = el.on_long_press(move || hold.set(hold.get() + 1));
    }
    el.build()
}

struct HoldRig {
    host: ComponentHost,
    clock: Rc<MockClock>,
    props: HoldButton,
    #[allow(dead_code)]
    handle: oppa::MountHandle<HoldButton>,
    hx: f32,
    hy: f32,
}

impl HoldRig {
    fn new(with_hold: bool) -> Self {
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        host.set_viewport(100.0, 60.0);
        let rt = host.runtime();
        let props = HoldButton {
            press_count: rt.signal(0u32),
            hold_count: rt.signal(0u32),
            with_hold,
        };
        let handle = host.mount("Hold", props.clone(), hold_button);
        host.run_until_idle();
        let id = find_retained_by_debug(&host, "hold")[0];
        let b = host.committed_box(id).expect("hold box");
        Self {
            host,
            clock,
            props,
            handle,
            hx: b.x + b.w / 2.0,
            hy: b.y + b.h / 2.0,
        }
    }

    fn counts(&self) -> (u32, u32) {
        (self.props.press_count.get(), self.props.hold_count.get())
    }
}

#[derive(Clone)]
struct HoldOnly {
    hold: oppa::Signal<u32>,
}

impl Props for HoldOnly {}

fn hold_only_render(_ctx: &Ctx, p: &HoldOnly) -> VNode {
    let hold = p.hold.clone();
    Div("holdonly")
        .style(oppa::Style::new().size(44, 24))
        .on_long_press(move || hold.set(hold.get() + 1))
        .build()
}

#[test]
fn hold_fires_long_press_tap_fires_press_no_double() {
    let rig = HoldRig::new(true);
    rig.host
        .inject_input(InputEvent::pointer_down(rig.hx, rig.hy));
    rig.host.run_until_idle();
    assert_eq!(rig.counts(), (0, 0));
    rig.clock.set(oppa::input::LONG_PRESS_TIMEOUT_S + 0.1);
    rig.host.run_once();
    assert_eq!(rig.counts(), (0, 1), "hold fires the hold handler");
    assert!(rig.host.capture_node_for(0).is_some(), "finger still held");
    rig.host
        .inject_input(InputEvent::pointer_up(rig.hx, rig.hy));
    rig.host.run_until_idle();
    assert_eq!(rig.counts(), (0, 1), "release after fire never doubles");
    assert_eq!(rig.host.capture_count(), 0);
}

#[test]
fn tap_with_hold_handler_still_fires_press() {
    let rig = HoldRig::new(true);
    rig.host
        .inject_input(InputEvent::pointer_down(rig.hx, rig.hy));
    rig.clock.set(0.1);
    rig.host
        .inject_input(InputEvent::pointer_up(rig.hx, rig.hy));
    rig.host.run_until_idle();
    assert_eq!(rig.counts(), (1, 0), "tap fires press, not hold");
}

#[test]
fn hold_without_hold_handler_falls_back_to_press() {
    // Additive compat: owners without `on_long_press` behave exactly
    // as G11 (the hold fires their press handler).
    let rig = HoldRig::new(false);
    rig.host
        .inject_input(InputEvent::pointer_down(rig.hx, rig.hy));
    rig.host.run_until_idle();
    assert_eq!(rig.counts(), (0, 0));
    rig.clock.set(oppa::input::LONG_PRESS_TIMEOUT_S + 0.1);
    rig.host.run_once();
    assert_eq!(rig.counts(), (1, 0), "fallback fires press on hold");
    rig.host
        .inject_input(InputEvent::pointer_up(rig.hx, rig.hy));
    rig.host.run_until_idle();
    assert_eq!(rig.counts(), (1, 0), "no double on release");
}

#[test]
fn hold_only_node_stays_inert() {
    // Decision 96 stands: press ownership routes holds, so a node
    // with only `on_long_press` arms nothing (documented in the
    // builder docs — inert by rule, not by silence).
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(100.0, 60.0);
    let hold = host.runtime().signal(0u32);
    host.mount(
        "HoldOnly",
        HoldOnly { hold: hold.clone() },
        hold_only_render,
    );
    host.run_until_idle();
    let id = find_retained_by_debug(&host, "holdonly")[0];
    let b = host.committed_box(id).expect("box");
    let (x, y) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    host.inject_input(InputEvent::pointer_down(x, y));
    host.run_until_idle();
    assert_eq!(host.capture_count(), 0, "no press owner, no capture");
    assert_eq!(host.longpress_armed_count(), 0, "no press owner, no arm");
    clock.set(oppa::input::LONG_PRESS_TIMEOUT_S + 1.0);
    host.run_until_idle();
    assert_eq!(hold.get(), 0, "inert hold never fires");
}

// ---------------------------------------------------------------------------
// Round 3.2 (decision 261): tap/swipe recognition — the Up path
// tells taps from flings by displacement from the Down origin, and
// swipes dispatch `Swipe` to the capture owner, never `Scroll`.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SwipeButtons {
    press_a: oppa::Signal<u32>,
    swipe_a: oppa::Signal<u32>,
    press_b: oppa::Signal<u32>,
    swipe_b: oppa::Signal<u32>,
    scroll_seen: oppa::Signal<u32>,
    with_swipe: bool,
}

impl Props for SwipeButtons {}

fn swipe_buttons(_ctx: &Ctx, p: &SwipeButtons) -> VNode {
    let build = |debug: &'static str,
                 press: oppa::Signal<u32>,
                 swipe: oppa::Signal<u32>,
                 scroll_seen: oppa::Signal<u32>,
                 with_swipe: bool| {
        let mut el = Div(debug)
            .style(oppa::Style::new().size(44, 24))
            .on_press(move || press.set(press.get() + 1))
            .on_scroll(move || scroll_seen.set(scroll_seen.get() + 1));
        if with_swipe {
            el = el.on_swipe(move || swipe.set(swipe.get() + 1));
        }
        el.build()
    };
    oppa::Row("row").children([
        build(
            "a",
            p.press_a.clone(),
            p.swipe_a.clone(),
            p.scroll_seen.clone(),
            p.with_swipe,
        ),
        build(
            "b",
            p.press_b.clone(),
            p.swipe_b.clone(),
            p.scroll_seen.clone(),
            p.with_swipe,
        ),
    ])
}

struct SwipeRig {
    host: ComponentHost,
    clock: Rc<MockClock>,
    props: SwipeButtons,
    #[allow(dead_code)]
    handle: oppa::MountHandle<SwipeButtons>,
    ax: f32,
    ay: f32,
    bx: f32,
    by: f32,
}

impl SwipeRig {
    fn new(with_swipe: bool) -> Self {
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        host.set_viewport(100.0, 60.0);
        let rt = host.runtime();
        let props = SwipeButtons {
            press_a: rt.signal(0u32),
            swipe_a: rt.signal(0u32),
            press_b: rt.signal(0u32),
            swipe_b: rt.signal(0u32),
            scroll_seen: rt.signal(0u32),
            with_swipe,
        };
        let handle = host.mount("Swipe", props.clone(), swipe_buttons);
        host.run_until_idle();
        let center = |debug: &str| {
            let id = find_retained_by_debug(&host, debug)[0];
            let b = host.committed_box(id).expect("button box");
            (b.x + b.w / 2.0, b.y + b.h / 2.0)
        };
        let (ax, ay) = center("a");
        let (bx, by) = center("b");
        Self {
            host,
            clock,
            props,
            handle,
            ax,
            ay,
            bx,
            by,
        }
    }

    fn down(&self, id: u32, x: f32, y: f32) {
        self.host
            .inject_input(InputEvent::pointer_down_id(id, x, y));
        self.host.run_until_idle();
    }

    fn mv(&self, id: u32, x: f32, y: f32, dt: f64) {
        self.clock.set(self.clock.get() + dt);
        self.host
            .inject_input(InputEvent::pointer_move_id(id, x, y));
        self.host.run_until_idle();
    }

    fn up(&self, id: u32, x: f32, y: f32, dt: f64) {
        self.clock.set(self.clock.get() + dt);
        self.host.inject_input(InputEvent::pointer_up_id(id, x, y));
        self.host.run_until_idle();
    }

    fn counts(&self) -> (u32, u32, u32, u32, u32) {
        (
            self.props.press_a.get(),
            self.props.swipe_a.get(),
            self.props.press_b.get(),
            self.props.swipe_b.get(),
            self.props.scroll_seen.get(),
        )
    }
}

#[test]
fn swipe_dispatches_to_down_owner_without_press() {
    // Down on A, fling right 40px in 0.2s, release over B: A's
    // swipe fires (capture owner — the Android touch-target rule),
    // nothing presses anywhere.
    let rig = SwipeRig::new(true);
    rig.down(0, rig.ax, rig.ay);
    rig.mv(0, rig.ax + 40.0, rig.ay, 0.1);
    rig.up(0, rig.ax + 40.0, rig.ay, 0.1);
    assert_eq!(
        rig.counts(),
        (0, 1, 0, 0, 0),
        "swipe fires A's swipe only (press_a, swipe_a, press_b, swipe_b, scroll)"
    );
    assert_eq!(rig.host.capture_count(), 0);
}

#[test]
fn swipe_without_handler_stays_quiet_never_presses() {
    // Same fling, no `on_swipe` declared: quiet (a swipe is not a
    // tap — unhandled-key precedent, never a fallback press).
    let rig = SwipeRig::new(false);
    rig.down(0, rig.ax, rig.ay);
    rig.mv(0, rig.ax + 40.0, rig.ay, 0.1);
    rig.up(0, rig.ax + 40.0, rig.ay, 0.1);
    assert_eq!(rig.counts(), (0, 0, 0, 0, 0), "handlerless swipe is quiet");
}

#[test]
fn swipe_is_not_scroll() {
    // The swipe path cannot name `Scroll` by construction — the
    // scroll handler stays silent through a fling (scroll answers
    // only `Scroll` input events, i.e. wheels).
    let rig = SwipeRig::new(true);
    rig.down(0, rig.ax, rig.ay);
    rig.mv(0, rig.ax + 40.0, rig.ay, 0.1);
    rig.up(0, rig.ax + 40.0, rig.ay, 0.1);
    assert_eq!(rig.props.scroll_seen.get(), 0, "no scroll dispatch");
    assert_eq!(rig.props.swipe_a.get(), 1, "swipe dispatch instead");
}

#[test]
fn slow_drag_release_is_quiet() {
    // Far but slow: move early (disarms before the hold deadline),
    // release late (past the swipe window) — neither tap (past
    // slop) nor swipe (too slow): a drag release dispatches
    // nothing. (Staying still past the deadline would be a hold —
    // the deadline fires while held, by rule.)
    let rig = SwipeRig::new(true);
    rig.down(0, rig.ax, rig.ay);
    rig.mv(0, rig.ax + 60.0, rig.ay, 0.1);
    rig.up(0, rig.ax + 60.0, rig.ay, 1.1);
    assert_eq!(rig.counts(), (0, 0, 0, 0, 0), "drag release is quiet");
}

#[test]
fn tap_near_slop_edge_still_presses() {
    // Drift 9px (inside the 10px slop) then release fast: a tap.
    let rig = SwipeRig::new(true);
    rig.down(0, rig.ax, rig.ay);
    rig.mv(0, rig.ax + 9.0, rig.ay, 0.05);
    rig.up(0, rig.ax + 9.0, rig.ay, 0.05);
    assert_eq!(rig.counts(), (1, 0, 0, 0, 0), "near-slop lift taps");
}

#[test]
fn two_fingers_tap_and_swipe_independently() {
    // G11 per-id lifecycles extend to gestures: finger A taps A
    // (press), finger B flings from B (swipe, no press).
    let rig = SwipeRig::new(true);
    rig.down(0, rig.ax, rig.ay);
    rig.down(1, rig.bx, rig.by);
    rig.up(0, rig.ax, rig.ay, 0.05);
    rig.mv(1, rig.bx - 40.0, rig.by, 0.1);
    rig.up(1, rig.bx - 40.0, rig.by, 0.1);
    assert_eq!(
        rig.counts(),
        (1, 0, 0, 1, 0),
        "tap presses A, swipe fires B's swipe"
    );
}

// ---------------------------------------------------------------------------
// Round 5.3 (decision 270): drag moves — every Move while captured
// notifies the owner's `on_drag`; positions ride the host query.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct DragPad {
    press: oppa::Signal<u32>,
    drags: oppa::Signal<u32>,
    with_drag: bool,
}

impl Props for DragPad {}

fn drag_pad(_ctx: &Ctx, p: &DragPad) -> VNode {
    let mut el = Div("pad")
        .style(oppa::Style::new().size(100, 60))
        .on_press({
            let press = p.press.clone();
            move || press.set(press.get() + 1)
        });
    if p.with_drag {
        let drags = p.drags.clone();
        el = el.on_drag(move || drags.set(drags.get() + 1));
    }
    el.build()
}

struct DragRig {
    host: ComponentHost,
    props: DragPad,
    #[allow(dead_code)]
    handle: oppa::MountHandle<DragPad>,
    cx: f32,
    cy: f32,
}

impl DragRig {
    fn new(with_drag: bool) -> Self {
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock);
        host.set_viewport(100.0, 60.0);
        let rt = host.runtime();
        let props = DragPad {
            press: rt.signal(0u32),
            drags: rt.signal(0u32),
            with_drag,
        };
        let handle = host.mount("Drag", props.clone(), drag_pad);
        host.run_until_idle();
        let id = find_retained_by_debug(&host, "pad")[0];
        let b = host.committed_box(id).expect("pad box");
        Self {
            host,
            props,
            handle,
            cx: b.x + b.w / 2.0,
            cy: b.y + b.h / 2.0,
        }
    }
}

#[test]
fn drag_moves_notify_owner_and_track_positions() {
    let rig = DragRig::new(true);
    rig.host
        .inject_input(InputEvent::pointer_down_id(0, rig.cx, rig.cy));
    rig.host.run_until_idle();
    assert_eq!(
        rig.host.pointer_position(0),
        Some((rig.cx, rig.cy)),
        "down records the position"
    );
    rig.host
        .inject_input(InputEvent::pointer_move_id(0, rig.cx + 10.0, rig.cy));
    rig.host.run_until_idle();
    rig.host
        .inject_input(InputEvent::pointer_move_id(0, rig.cx + 20.0, rig.cy));
    rig.host.run_until_idle();
    assert_eq!(rig.props.drags.get(), 2, "every move notifies");
    assert_eq!(
        rig.host.pointer_position(0),
        Some((rig.cx + 20.0, rig.cy)),
        "query tracks the latest move"
    );
    assert_eq!(
        rig.host.capture_position(),
        Some((rig.cx + 20.0, rig.cy)),
        "legacy read agrees"
    );
    rig.host
        .inject_input(InputEvent::pointer_up_id(0, rig.cx + 20.0, rig.cy));
    rig.host.run_until_idle();
    assert_eq!(
        rig.host.pointer_position(0),
        None,
        "release clears (never stale)"
    );
    assert_eq!(rig.host.capture_position(), None);
    // Fast far lift: the swipe... has no handler here, so quiet —
    // and the press counter proves no tap either (slop gate).
    assert_eq!(rig.props.press.get(), 0);
}

#[test]
fn drag_without_handler_stays_quiet_but_tracks() {
    // Moves without `on_drag` change nothing dispatch-wise (M5
    // behavior), while positions still track (uniform router
    // bookkeeping — the query, not the event, is the contract).
    let rig = DragRig::new(false);
    rig.host
        .inject_input(InputEvent::pointer_down_id(0, rig.cx, rig.cy));
    rig.host.run_until_idle();
    rig.host
        .inject_input(InputEvent::pointer_move_id(0, rig.cx + 10.0, rig.cy));
    rig.host.run_until_idle();
    assert_eq!(rig.props.drags.get(), 0, "no handler, no dispatch");
    assert_eq!(
        rig.host.pointer_position(0),
        Some((rig.cx + 10.0, rig.cy)),
        "positions track regardless"
    );
}

#[test]
fn two_finger_moves_dispatch_per_pointer() {
    // G11 extends to drags: each captured pointer's moves notify
    // (same handler twice — the query inside disambiguates by id).
    let rig = DragRig::new(true);
    rig.host
        .inject_input(InputEvent::pointer_down_id(0, rig.cx, rig.cy));
    rig.host
        .inject_input(InputEvent::pointer_down_id(1, rig.cx, rig.cy));
    rig.host.run_until_idle();
    rig.host
        .inject_input(InputEvent::pointer_move_id(0, rig.cx + 5.0, rig.cy));
    rig.host
        .inject_input(InputEvent::pointer_move_id(1, rig.cx - 5.0, rig.cy));
    rig.host.run_until_idle();
    assert_eq!(rig.props.drags.get(), 2, "one dispatch per pointer move");
    assert_eq!(rig.host.pointer_position(0), Some((rig.cx + 5.0, rig.cy)));
    assert_eq!(rig.host.pointer_position(1), Some((rig.cx - 5.0, rig.cy)));
}
