//! Pointer drag scrolling (Round 10.1, decision 303): a held pointer
//! that moves past tap slop inside a feed-bound `ScrollArea` streams
//! its displacement into the container feeds — and the child tap
//! never fires, even if the release returns inside slop. Momentum
//! (Round 10.2, decision 304): fast releases keep scrolling across
//! ticks until the velocity decays.

use std::rc::Rc;

use oppa::{
    find_retained_by_debug, ComponentHost, Ctx, Div, InputEvent, MockClock, PointerButton, Props,
    ScrollOffset, Style, VNode,
};

#[derive(Clone)]
struct ListProps {
    pressed: oppa::Signal<bool>,
}

impl Props for ListProps {}

fn scroll_list(ctx: &Ctx, p: &ListProps) -> VNode {
    let offset = ctx.scroll_offset();
    let _ = offset.get();
    let pressed = p.pressed.clone();
    oppa::ScrollArea("list")
        .style(Style::new().size(200, 100))
        .on_scroll(|| {})
        .children([
            Div("btn")
                .style(Style::new().size(200, 32))
                .on_press(move || pressed.set(true))
                .build(),
            Div("tall").style(Style::new().size(200, 300)).build(),
        ])
}

struct Rig {
    host: ComponentHost,
    props: ListProps,
    offset: ScrollOffset,
    btn_center: (f32, f32),
}

impl Rig {
    fn new() -> Self {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let rt = host.runtime();
        let props = ListProps {
            pressed: rt.signal(false),
        };
        let handle = host.mount("List", props.clone(), scroll_list);
        host.run_until_idle();
        let root = handle.root_instance();
        let offset = host.instance_scroll(root).expect("scroll handle");
        let list = find_retained_by_debug(&host, "list")[0];
        host.bind_scroll(list, offset.clone());
        let btn = find_retained_by_debug(&host, "btn")[0];
        let b = host.committed_box(btn).expect("button laid out");
        Self {
            host,
            props,
            offset,
            btn_center: (b.x + b.w / 2.0, b.y + b.h / 2.0),
        }
    }

    fn down(&self, x: f32, y: f32) {
        self.host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Down {
                button: PointerButton::Primary,
            },
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

    fn up(&self, x: f32, y: f32) {
        self.host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Up {
                button: PointerButton::Primary,
            },
            x,
            y,
            modifiers: oppa::Modifiers::NONE,
        });
        self.host.run_until_idle();
    }
}

/// The brief's verification, exactly: Down + Move(0, -50) displaces
/// the container by 50 — and the child Button never fires.
#[test]
fn touch_drag_scrolls_and_suppresses_the_child_tap() {
    let rig = Rig::new();
    let (x, y) = rig.btn_center;
    rig.down(x, y);
    assert_eq!(rig.offset.get(), 0.0, "press alone scrolls nothing");
    rig.mv(x, y - 50.0);
    assert_eq!(rig.offset.get(), 50.0, "full displacement streams");
    rig.up(x, y - 50.0);
    assert_eq!(rig.offset.get(), 50.0, "release holds the offset");
    assert!(
        !rig.props.pressed.get(),
        "the child Button never fires on_press"
    );
}

/// A drag that returns inside slop still never taps (the drag owned
/// the gesture — slop is a press rule, not a scroll rule).
#[test]
fn drag_out_and_back_inside_slop_never_taps() {
    let rig = Rig::new();
    let (x, y) = rig.btn_center;
    rig.down(x, y);
    rig.mv(x, y - 50.0);
    assert_eq!(rig.offset.get(), 50.0);
    rig.mv(x, y - 5.0);
    assert_eq!(rig.offset.get(), 5.0, "return streams back down");
    rig.up(x, y - 5.0);
    assert!(
        !rig.props.pressed.get(),
        "no tap after a scrolled drag, even on return"
    );
}

/// Sub-slop wiggles stream nothing and still tap (plain taps behave
/// exactly as before — no dead zone, no lost presses).
#[test]
fn sub_slop_wiggle_streams_nothing_and_still_taps() {
    let rig = Rig::new();
    let (x, y) = rig.btn_center;
    rig.down(x, y);
    rig.mv(x + 3.0, y + 4.0);
    assert_eq!(rig.offset.get(), 0.0, "slop streams nothing");
    rig.up(x + 3.0, y + 4.0);
    assert!(rig.props.pressed.get(), "the tap still fires");
    assert_eq!(rig.offset.get(), 0.0, "and scrolled nothing");
}

/// Unbound containers never capture drags (opt-in per container —
/// the wheel arm's per-axis ignore rule, mechanism-shared).
#[test]
fn unbound_containers_never_capture_drags() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let props = ListProps {
        pressed: rt.signal(false),
    };
    host.mount("List", props.clone(), scroll_list);
    host.run_until_idle();
    // No `bind_scroll` anywhere: the handler still dispatches, but
    // no signal moves and the tap still fires.
    let btn = find_retained_by_debug(&host, "btn")[0];
    let b = host.committed_box(btn).expect("button laid out");
    let (x, y) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    let ptr = |action, px: f32, py: f32| InputEvent::Pointer {
        id: Some(0),
        action,
        x: px,
        y: py,
        modifiers: oppa::Modifiers::NONE,
    };
    host.inject_input(ptr(
        oppa::PointerAction::Down {
            button: PointerButton::Primary,
        },
        x,
        y,
    ));
    host.run_until_idle();
    host.inject_input(ptr(oppa::PointerAction::Move, x, y - 50.0));
    host.run_until_idle();
    assert_eq!(
        host.bound_scroll(find_retained_by_debug(&host, "list")[0]),
        None
    );
    host.inject_input(ptr(
        oppa::PointerAction::Up {
            button: PointerButton::Primary,
        },
        x,
        y - 50.0,
    ));
    host.run_until_idle();
    assert!(
        !props.pressed.get(),
        "unbound far release stays drag-quiet (pre-10.1 behavior, unchanged)"
    );
}

// ---------------------------------------------------------------------------
// Momentum (Round 10.2, decision 304)
// ---------------------------------------------------------------------------

/// Clock-driven rig: the same list on a `MockClock`, so release
/// velocities and decay ticks are deterministic (SystemClock moves
/// arrive microseconds apart — real flings, unassertable speeds).
struct ClockRig {
    host: ComponentHost,
    clock: Rc<MockClock>,
    offset: ScrollOffset,
    center: (f32, f32),
}

impl ClockRig {
    fn new() -> Self {
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        host.set_viewport(800.0, 600.0);
        let rt = host.runtime();
        let props = ListProps {
            pressed: rt.signal(false),
        };
        let handle = host.mount("List", props, scroll_list);
        host.run_until_idle();
        let root = handle.root_instance();
        let offset = host.instance_scroll(root).expect("scroll handle");
        let list = find_retained_by_debug(&host, "list")[0];
        host.bind_scroll(list, offset.clone());
        let btn = find_retained_by_debug(&host, "btn")[0];
        let b = host.committed_box(btn).expect("button laid out");
        Self {
            host,
            clock,
            offset,
            center: (b.x + b.w / 2.0, b.y + b.h / 2.0),
        }
    }

    fn ptr(&self, action: oppa::PointerAction, x: f32, y: f32) {
        self.host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action,
            x,
            y,
            modifiers: oppa::Modifiers::NONE,
        });
        self.host.run_until_idle();
    }

    fn down(&self, x: f32, y: f32) {
        self.ptr(
            oppa::PointerAction::Down {
                button: PointerButton::Primary,
            },
            x,
            y,
        );
    }

    fn mv(&self, x: f32, y: f32) {
        self.ptr(oppa::PointerAction::Move, x, y);
    }

    fn up(&self, x: f32, y: f32) {
        self.ptr(
            oppa::PointerAction::Up {
                button: PointerButton::Primary,
            },
            x,
            y,
        );
    }

    /// One paced tick: advance the clock, then tick exactly one fling.
    fn tick(&self, dt: f64) -> bool {
        self.clock.advance(dt);
        self.host.tick_flings()
    }
}

/// The brief's verification: a fling release keeps scrolling across
/// successive ticks (100 → ~142 after one 50ms tick at 1000px/s)
/// until the velocity decays to zero and the fling retires.
#[test]
fn fling_release_continues_across_ticks_until_settled() {
    let rig = ClockRig::new();
    let (x, y) = rig.center;
    // Two 50px moves 50ms apart: 1000px/s release speed (content
    // direction +y — finger retreats upward).
    rig.down(x, y);
    rig.clock.advance(0.05);
    rig.mv(x, y - 50.0);
    rig.clock.advance(0.05);
    rig.mv(x, y - 100.0);
    rig.up(x, y - 100.0);
    assert_eq!(rig.offset.get(), 100.0, "the drag lands first");
    assert_eq!(rig.host.fling_count(), 1, "fast release flings");
    // One tick moves (decay-integrated, frame-rate independent).
    assert!(rig.tick(0.05), "momentum moves the offset");
    let after_one = rig.offset.get();
    assert!(
        after_one > 100.0,
        "fling release continues scrolling, got {after_one}"
    );
    // Ticks continue until the velocity decays to zero...
    let mut ticks = 0;
    while rig.host.fling_count() > 0 && ticks < 200 {
        rig.tick(0.05);
        ticks += 1;
    }
    assert_eq!(rig.host.fling_count(), 0, "the fling settles");
    let final_offset = rig.offset.get();
    assert!(
        final_offset > 200.0,
        "momentum travels (100 drag + ~135 fling), got {final_offset}"
    );
    // ...and stays settled (further ticks are safe no-ops).
    rig.tick(0.05);
    assert_eq!(rig.offset.get(), final_offset);
    assert!(!rig.host.tick_flings(), "settled ticks move nothing");
}

/// A slow release settles in place (a careful placement, not a
/// fling — below the velocity threshold, and outside the sampling
/// window anyway).
#[test]
fn slow_release_settles_without_momentum() {
    let rig = ClockRig::new();
    let (x, y) = rig.center;
    rig.down(x, y);
    rig.clock.advance(1.0);
    rig.mv(x, y - 50.0);
    rig.up(x, y - 50.0);
    assert_eq!(rig.offset.get(), 50.0);
    assert_eq!(rig.host.fling_count(), 0, "slow release never flings");
    assert!(!rig.tick(0.05), "no motion without a fling");
    assert_eq!(rig.offset.get(), 50.0);
}

/// Grabbing content stops momentum (a new touch cancels the fling —
/// the native grab rule).
#[test]
fn grab_stops_momentum() {
    let rig = ClockRig::new();
    let (x, y) = rig.center;
    rig.down(x, y);
    rig.clock.advance(0.05);
    rig.mv(x, y - 50.0);
    rig.clock.advance(0.05);
    rig.mv(x, y - 100.0);
    rig.up(x, y - 100.0);
    assert_eq!(rig.host.fling_count(), 1);
    rig.down(x, y);
    assert_eq!(rig.host.fling_count(), 0, "grab cancels the fling");
    let frozen = rig.offset.get();
    rig.tick(0.05);
    assert_eq!(rig.offset.get(), frozen, "cancelled momentum stays put");
}
