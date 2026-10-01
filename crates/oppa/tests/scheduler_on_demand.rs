//! Milestone #18 proofs: the phase loop is on-demand. Frames run only when
//! input/animation/reload/request_frame demand them; the seven phases
//! execute in locked order exactly once per frame; phase execution counts
//! are verified against a mock clock.

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use oppa::{Event, EventKind, HandlerId, MockClock, PassMask, Phase, PlatformShell, Runtime};

struct RecordingShell {
    pump_calls: Rc<Cell<u32>>,
    queued: RefCell<Vec<Event>>,
}

impl RecordingShell {
    fn new(pump_calls: Rc<Cell<u32>>) -> Self {
        Self {
            pump_calls,
            queued: RefCell::new(Vec::new()),
        }
    }

    fn queue(&self, event: Event) {
        self.queued.borrow_mut().push(event);
    }
}

impl PlatformShell for RecordingShell {
    fn pump_events(&mut self) -> Vec<Event> {
        self.pump_calls.set(self.pump_calls.get() + 1);
        std::mem::take(&mut *self.queued.borrow_mut())
    }
}

#[test]
fn idle_runtime_never_wakes() {
    let rt = Runtime::new();
    assert!(!rt.has_demand(), "fresh runtime has no demand");
    assert!(!rt.run_once(), "no demand = no frame");
    assert_eq!(rt.stats().frames, 0);
    assert_eq!(rt.stats().phase_runs, [0; 7]);
    assert_eq!(rt.take_phase_log(), Vec::<Phase>::new());
    assert!(!rt.run_once());
    assert!(!rt.run_once());
    assert_eq!(rt.stats().frames, 0, "still idle after repeated attempts");
}

#[test]
fn request_frame_runs_exactly_one_frame_with_all_seven_phases_in_order() {
    let rt = Runtime::new();
    rt.request_frame();
    assert!(rt.has_demand());
    assert!(rt.run_once(), "demand produced a frame");
    assert_eq!(rt.stats().frames, 1);
    assert_eq!(
        rt.take_phase_log(),
        vec![
            Phase::Time,
            Phase::Input,
            Phase::Reload,
            Phase::Effects,
            Phase::Layout,
            Phase::PaintCommit,
            Phase::A11y,
        ],
        "locked phase order (§9.1)"
    );
    assert_eq!(rt.stats().phase_runs, [1; 7]);
    assert!(!rt.has_demand(), "demand consumed by the frame");
    assert!(!rt.run_once(), "no new demand = no new frame");
    assert_eq!(rt.stats().frames, 1);
}

/// TIME is the only clock: animation callbacks receive the frame's
/// timestamp from the injected mock clock; the animation set drives frames
/// at the test-injected cadence and the loop idles once it drains.
#[test]
fn animation_drives_frames_until_it_drains_then_the_loop_idles() {
    let clock = Rc::new(MockClock::new());
    let rt = Runtime::with_clock(clock.clone());
    let s = rt.signal(0f64);
    let frames_seen = Rc::new(RefCell::new(Vec::new()));
    let effect_runs = Rc::new(Cell::new(0u32));
    rt.effect_named("follower", {
        let s = s.clone();
        let seen = frames_seen.clone();
        let runs = effect_runs.clone();
        move || {
            runs.set(runs.get() + 1);
            seen.borrow_mut().push(s.get());
        }
    });
    rt.add_animation({
        let s = s.clone();
        let count = Rc::new(Cell::new(0u32));
        move |now| {
            let n = count.get();
            if n >= 3 {
                return false;
            }
            count.set(n + 1);
            s.set(now);
            n + 1 < 3
        }
    });
    let mut ran = 0;
    for _ in 0..4 {
        clock.advance(1.0 / 60.0);
        if !rt.run_once() {
            break;
        }
        ran += 1;
    }
    assert_eq!(ran, 3, "animation active for exactly three frames");
    assert_eq!(
        *frames_seen.borrow(),
        vec![0.0, 1.0 / 60.0, 2.0 / 60.0, 3.0 / 60.0],
        "initial value + one write per animated frame"
    );
    assert_eq!(effect_runs.get(), 4, "initial run + one per animated frame");
    assert!(!rt.has_demand(), "animation drained: the loop idles");
    clock.advance(1.0);
    assert!(
        !rt.run_once(),
        "time alone wakes nothing (§9.1: no work = no wake)"
    );
    assert_eq!(rt.stats().frames, 3);
}

/// A reload lands as a frame wake; RELOAD applies between INPUT and
/// EFFECTS — the hook's signal writes settle in the SAME frame's EFFECTS.
#[test]
fn reload_wakes_a_frame_and_applies_before_effects() {
    let rt = Runtime::new();
    let s = rt.signal(0i32);
    let seen = Rc::new(RefCell::new(Vec::new()));
    rt.effect_named("e", {
        let s = s.clone();
        let seen = seen.clone();
        move || seen.borrow_mut().push(s.get())
    });
    let hook_ran = Rc::new(Cell::new(0u32));
    rt.set_reload_hook({
        let s = s.clone();
        let hook_ran = hook_ran.clone();
        move |_rt: &Runtime| {
            let _ = _rt;
            hook_ran.set(hook_ran.get() + 1);
            s.set(42);
        }
    });
    assert!(!rt.has_demand());
    rt.request_reload();
    assert!(rt.has_demand());
    let log_before = rt.stats().phase_runs;
    assert!(rt.run_once());
    assert_eq!(hook_ran.get(), 1);
    assert_eq!(
        rt.stats().phase_runs[Phase::Reload.index()],
        log_before[Phase::Reload.index()] + 1
    );
    assert_eq!(
        *seen.borrow(),
        vec![0, 42],
        "reload write settled in the same frame"
    );
    let log = rt.take_phase_log();
    let reload_pos = log.iter().position(|p| *p == Phase::Reload).unwrap();
    let effects_pos = log.iter().position(|p| *p == Phase::Effects).unwrap();
    assert!(
        reload_pos < effects_pos,
        "RELOAD strictly before EFFECTS (§5.3)"
    );
    assert!(!rt.has_demand());
}

/// Worker results enter the UI thread only via the queue drained at INPUT:
/// submitting between frames does nothing until the next frame's INPUT,
/// and the drain performs the signal writes on the UI thread.
#[test]
fn worker_queue_drains_at_input_boundary() {
    let rt = Runtime::new();
    let s = rt.signal(0i32);
    let seen = Rc::new(RefCell::new(0i32));
    rt.effect_named("e", {
        let s = s.clone();
        let seen = seen.clone();
        move || *seen.borrow_mut() = s.get()
    });
    rt.worker_submit({
        let s = s.clone();
        move |_rt: &Runtime| {
            s.set(7);
        }
    });
    assert_eq!(
        rt.stats().frames,
        0,
        "queued worker result does not run a frame by itself"
    );
    assert!(rt.has_demand(), "but it is pending demand");
    assert!(rt.run_once());
    assert_eq!(
        *seen.borrow(),
        7,
        "drain performed the write on the UI thread"
    );
    assert_eq!(rt.stats().worker_applied, 1);
    assert_eq!(rt.stats().worker_discarded, 0);
    let log = rt.take_phase_log();
    let input_pos = log.iter().position(|p| *p == Phase::Input).unwrap();
    let effects_pos = log.iter().position(|p| *p == Phase::Effects).unwrap();
    assert!(input_pos < effects_pos, "drain at INPUT, settle at EFFECTS");
    assert!(!rt.has_demand());
}

/// §9.6: queued results from a retired hot generation are discarded at the
/// next INPUT drain — the same mechanism class as generational slot checks.
#[test]
fn retired_generation_queue_results_are_discarded() {
    let rt = Runtime::new();
    let s = rt.signal(0i32);
    let seen = Rc::new(RefCell::new(0i32));
    rt.effect_named("e", {
        let s = s.clone();
        let seen = seen.clone();
        move || *seen.borrow_mut() = s.get()
    });
    rt.worker_submit({
        let s = s.clone();
        move |_rt: &Runtime| s.set(11)
    }); // gen 0
    rt.advance_hot_generation(); // simulated swap: gen 0 retired
    assert!(rt.run_once());
    assert_eq!(
        rt.stats().worker_discarded,
        1,
        "retired-gen result dropped at drain"
    );
    assert_eq!(rt.stats().worker_applied, 0);
    assert_eq!(*seen.borrow(), 0, "retired-gen write never touched state");
    rt.worker_submit({
        let s = s.clone();
        move |_rt: &Runtime| s.set(9)
    }); // gen 1
    assert!(rt.run_once());
    assert_eq!(rt.stats().worker_applied, 1);
    assert_eq!(*seen.borrow(), 9);
}

/// INPUT pumps the shell once per frame and dispatches pre-routed events
/// through the registry (writes land batched in the same frame's EFFECTS).
#[test]
fn shell_events_dispatch_through_the_handler_registry() {
    let rt = Runtime::new();
    let s = rt.signal(0i32);
    let seen = Rc::new(RefCell::new(0i32));
    rt.effect_named("e", {
        let s = s.clone();
        let seen = seen.clone();
        move || *seen.borrow_mut() = s.get()
    });
    let pump_calls = Rc::new(Cell::new(0u32));
    let id = HandlerId::from_symbol("btn.press");
    rt.register_handler(id, {
        let s = s.clone();
        move || s.set(11)
    });
    let shell = RecordingShell::new(pump_calls.clone());
    shell.queue(Event {
        kind: EventKind::Press,
        handler: id,
    });
    rt.set_shell(Box::new(shell));
    rt.request_frame();
    assert!(rt.run_once());
    assert_eq!(pump_calls.get(), 1, "shell pumped exactly once this frame");
    assert_eq!(*seen.borrow(), 11);
}

#[test]
fn phases_count_independently_across_many_frames() {
    let rt = Runtime::new();
    let s = rt.signal(0i32);
    let e = rt.effect_named("e", {
        let s = s.clone();
        move || {
            s.get();
        }
    });
    for i in 0..5 {
        rt.request_frame();
        assert!(rt.run_once());
        s.set(i);
    }
    rt.run_until_idle();
    let stats = rt.stats();
    assert_eq!(
        stats.frames, 6,
        "five requested + one woken by the final set"
    );
    assert_eq!(stats.phase_runs[Phase::Time.index()], 6);
    assert_eq!(stats.phase_runs[Phase::Effects.index()], 6);
    assert_eq!(
        stats.phase_runs[Phase::Layout.index()],
        6,
        "stub still counts"
    );
    assert_eq!(
        stats.phase_runs[Phase::PaintCommit.index()],
        6,
        "stub still counts"
    );
    let _ = e;
}

#[test]
fn effects_and_memos_never_run_mid_layout_or_paint() {
    // §9.1's no-user-code-mid-LAYOUT/PAINT rule is phase-constructed; at M0
    // the phases are stubs, so the strongest checkable form is: reactive
    // runs happen only in INPUT (via dispatch) and EFFECTS.
    let rt = Runtime::new();
    let s = rt.signal(0i32);
    let run_log = Rc::new(RefCell::new(Vec::new()));
    rt.effect_named("e", {
        let s = s.clone();
        let run_log = run_log.clone();
        move || {
            s.get();
            run_log.borrow_mut().push("effect");
        }
    });
    rt.request_frame();
    s.set(1);
    rt.run_once();
    let log = rt.take_phase_log();
    assert_eq!(log.len(), 7);
    let effects_pos = log.iter().position(|p| *p == Phase::Effects).unwrap();
    let layout_pos = log.iter().position(|p| *p == Phase::Layout).unwrap();
    let paint_pos = log.iter().position(|p| *p == Phase::PaintCommit).unwrap();
    assert!(effects_pos < layout_pos && layout_pos < paint_pos);
    assert_eq!(*run_log.borrow(), vec!["effect", "effect"]);
    assert_eq!(rt.stats().passes_last_frame, 1);
}

#[test]
fn pass_mask_flags_flow_through_a_frame() {
    // PassMask is the dirty-flag type the EFFECTS/LAYOUT/PAINT contract
    // shares; smoke-test the composition the scheduler will rely on.
    let mut dirty = PassMask::EMPTY;
    dirty |= PassMask::STYLE | PassMask::SEMANTICS;
    assert!(dirty.contains(PassMask::STYLE));
    assert!(dirty.contains(PassMask::SEMANTICS));
    assert!(!dirty.contains(PassMask::LAYOUT));
    dirty |= PassMask::LAYOUT;
    assert_eq!(dirty.to_string(), "STYLE|LAYOUT|SEMANTICS");
}
