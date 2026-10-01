//! M10 Android contract: the shell classifies, the framework routes.
//!
//! The shared-contract proof uses the same Toggle and the same
//! assertions as M5's `m5_input`, fed through Android-classified
//! intake: a `MotionDown`/`MotionUp` pair in dp lands on the track
//! in px, flips `checked` through `SemanticsDiff` in the same
//! commit, and settles in one frame. If any shell forked the
//! pipeline (its own hit-test, routing, or flag writes), these
//! assertions against framework-owned state would fail. They pass
//! because both shells produce `InputEvent` and the host owns the
//! rest. Multi-touch routes per-pointer (one capture per id),
//! pause keeps state, and destroy-to-relaunch is deterministic
//! (restart equals cold start: same script, same pixels, same
//! semantics dump, per lock #16).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use oppa::shell::PlatformShell;
use oppa::{
    compute_semantics_diff, find_retained_by_debug, Color, ComponentHost, Ctx, HandlerId,
    InputEvent, RendererBackend, Semantics, SemanticsSnapshot, SharedString, Style, SurfaceDesc,
    VNode,
};
use oppa_macros::{component, Props};
use oppa_shell_android::{
    field_event, AndroidCmd, AndroidEvent, AndroidShell, LifecycleState, ShellConfig,
};

// ---------------------------------------------------------------------------
// Minimal Toggle (M5 shape, transition-free: the contract under test is
// input mapping, and a bare style settles in exactly one frame).
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct ToggleProps {
    label: SharedString,
    initial: bool,
    on_change: HandlerId,
}

#[component]
fn Toggle(ctx: &Ctx, props: &ToggleProps) -> VNode {
    let is_on = ctx.signal(props.initial);
    let bg = if is_on.get() {
        Color(0x44_44_44)
    } else {
        Color(0x55_55_55)
    };
    let rt = ctx.runtime();
    let on_change = props.on_change;
    oppa::Div("track")
        .style(Style::new().size(44, 24).bg(bg))
        .semantics(Semantics::switch().checked(is_on.get()).label(&props.label))
        .on_press(move || {
            is_on.set(!is_on.get());
            rt.dispatch(oppa::Event {
                kind: oppa::EventKind::Press,
                handler: on_change,
            });
        })
        .build()
}

struct Rig {
    host: ComponentHost,
    shell: AndroidShell,
    changes: Rc<Cell<u32>>,
    _handle: oppa::MountHandle<ToggleProps>,
}

impl Rig {
    fn new(density: f32) -> Self {
        let host = ComponentHost::new();
        host.set_viewport(100.0, 60.0);
        let shell = AndroidShell::new(ShellConfig {
            density,
            ..ShellConfig::default()
        });
        let changes = Rc::new(Cell::new(0u32));
        let on_change = HandlerId::from_symbol("test.m10.toggle.change");
        host.runtime().register_handler(on_change, {
            let changes = changes.clone();
            move || changes.set(changes.get() + 1)
        });
        let handle = host.mount(
            "Toggle",
            ToggleProps {
                label: Arc::from("Wi-Fi"),
                initial: false,
                on_change,
            },
            Toggle,
        );
        host.run_until_idle();
        Self {
            host,
            shell,
            changes,
            _handle: handle,
        }
    }

    /// Drives one Android intake event through the full path —
    /// shell classify → M0 pump → cmds → shared `InputEvent`
    /// injection → framework router → settled frame — and returns
    /// the pumped M0 kinds (the trait stream, asserted identical to
    /// Win32's shape: kind + `field_event` handler).
    fn drive(&mut self, ev: AndroidEvent) -> Vec<oppa::EventKind> {
        self.shell.push_event(ev);
        let pumped = self.shell.pump_events();
        let kinds = pumped.iter().map(|e| e.kind).collect::<Vec<_>>();
        for e in &pumped {
            assert_eq!(
                e.handler,
                field_event(),
                "shell stream must stay pre-routed to the field handler"
            );
        }
        for cmd in self.shell.take_cmds() {
            self.host.inject_input(cmd_to_input(cmd));
        }
        self.host.run_until_idle();
        kinds
    }

    fn checked(&self) -> Option<bool> {
        let track = find_retained_by_debug(&self.host, "track")[0];
        self.host.retained_semantics(track).and_then(|s| s.checked)
    }
}

/// `AndroidCmd` → shared `InputEvent` (the one line the host loop
/// runs on every platform — stated here so a fork would show as a
/// diff against this function, not as silent drift).
fn cmd_to_input(cmd: AndroidCmd) -> InputEvent {
    cmd.to_input_event()
}

// ---------------------------------------------------------------------------
// 1. Shared contract: Android-classified press flips the Toggle.
// ---------------------------------------------------------------------------

#[test]
fn android_press_roundtrips_the_shared_pipeline() {
    // Density 2.0: dp (5,6) → px (10,12), inside the 44×24 track.
    let mut rig = Rig::new(2.0);
    assert_eq!(rig.checked(), Some(false));
    assert_eq!(rig.changes.get(), 0);

    let kinds = rig.drive(AndroidEvent::MotionDown {
        pointer_index: 0,
        x_dp: 5.0,
        y_dp: 6.0,
    });
    assert_eq!(kinds, vec![oppa::EventKind::Press]);
    // Press alone captures + focuses (M5 capture machine); the flip
    // dispatches on release-inside, exactly like Win32 intake.
    assert_eq!(rig.checked(), Some(false));

    let kinds = rig.drive(AndroidEvent::MotionUp {
        pointer_index: 0,
        x_dp: 5.0,
        y_dp: 6.0,
    });
    assert_eq!(kinds, vec![oppa::EventKind::Release]);
    assert_eq!(rig.checked(), Some(true));
    assert_eq!(rig.changes.get(), 1);

    // Release-outside is a silent no-op (decision 95 — same rule).
    rig.drive(AndroidEvent::MotionDown {
        pointer_index: 0,
        x_dp: 5.0,
        y_dp: 6.0,
    });
    rig.drive(AndroidEvent::MotionUp {
        pointer_index: 0,
        x_dp: 90.0,
        y_dp: 50.0,
    });
    assert_eq!(rig.checked(), Some(true));
    assert_eq!(rig.changes.get(), 1);

    // Cancel clears capture with no dispatch (the M5 tripwire).
    rig.drive(AndroidEvent::MotionDown {
        pointer_index: 0,
        x_dp: 5.0,
        y_dp: 6.0,
    });
    let kinds = rig.drive(AndroidEvent::MotionCancel);
    assert_eq!(kinds, vec![oppa::EventKind::Release]);
    assert_eq!(rig.checked(), Some(true));
    assert_eq!(rig.changes.get(), 1);
}

#[test]
fn android_keys_classify_without_forking_quiet_rules() {
    let mut rig = Rig::new(1.0);
    // Unknown keycode: classifies, router stays quiet (decision 96).
    rig.drive(AndroidEvent::KeyDown { keycode: 42 });
    rig.drive(AndroidEvent::KeyUp { keycode: 42 });
    assert_eq!(rig.checked(), Some(false));
    assert_eq!(rig.changes.get(), 0);
    // BACK dismisses (mapped to ESCAPE at the classifier): blurs, and
    // with nothing else focused the Toggle is untouched.
    rig.drive(AndroidEvent::KeyDown {
        keycode: oppa_shell_android::events::keycodes::BACK,
    });
    assert_eq!(rig.checked(), Some(false));
    // Round 3.3: the mapping itself is pinned (BACK == ESC at the
    // classifier — the dismiss-first chain rides `handle_back`
    // through the shared ESC arm, one chain everywhere).
    assert_eq!(
        oppa_shell_android::events::classify_keycode(oppa_shell_android::events::keycodes::BACK),
        oppa::input::keys::ESCAPE,
        "BACK classifies to ESCAPE"
    );
}

// ---------------------------------------------------------------------------
// 2. Multi-touch: two fingers drive two independent press lifecycles.
// ---------------------------------------------------------------------------

#[test]
fn second_finger_routes_with_its_own_capture() {
    // G11 retires the v1 refusal: the same track pressed by two
    // fingers dispatches twice (one press lifecycle per id —
    // decision 227), through the full shell-to-router path.
    let mut rig = Rig::new(2.0);
    rig.drive(AndroidEvent::MotionDown {
        pointer_index: 0,
        x_dp: 5.0,
        y_dp: 6.0,
    });
    assert!(rig.host.capture_node_for(0).is_some());
    rig.drive(AndroidEvent::MotionDown {
        pointer_index: 1,
        x_dp: 5.0,
        y_dp: 6.0,
    });
    assert!(rig.host.capture_node_for(1).is_some());
    assert_eq!(rig.host.capture_count(), 2);
    assert_eq!(rig.changes.get(), 0, "down never dispatches");
    rig.drive(AndroidEvent::MotionUp {
        pointer_index: 0,
        x_dp: 5.0,
        y_dp: 6.0,
    });
    assert_eq!(rig.changes.get(), 1, "first finger dispatches");
    assert!(rig.host.capture_node_for(0).is_none());
    assert!(rig.host.capture_node_for(1).is_some(), "second holds");
    rig.drive(AndroidEvent::MotionUp {
        pointer_index: 1,
        x_dp: 5.0,
        y_dp: 6.0,
    });
    assert_eq!(rig.changes.get(), 2, "second finger dispatches");
    assert_eq!(rig.host.capture_count(), 0);
}

// ---------------------------------------------------------------------------
// 3. Lifecycle: pause keeps state, destroy arms restart.
// ---------------------------------------------------------------------------

#[test]
fn pause_resume_keeps_retained_state() {
    let mut rig = Rig::new(1.0);
    rig.drive(AndroidEvent::MotionDown {
        pointer_index: 0,
        x_dp: 10.0,
        y_dp: 12.0,
    });
    rig.drive(AndroidEvent::MotionUp {
        pointer_index: 0,
        x_dp: 10.0,
        y_dp: 12.0,
    });
    assert_eq!(rig.checked(), Some(true));

    rig.shell.note_lifecycle(LifecycleState::Created).unwrap();
    rig.shell.note_lifecycle(LifecycleState::Started).unwrap();
    rig.shell.note_lifecycle(LifecycleState::Resumed).unwrap();
    assert!(rig.shell.lifecycle().render_gate());
    assert!(rig.shell.lifecycle_mut().take_wake());

    // Pause: gate closes, graph untouched.
    rig.shell.note_lifecycle(LifecycleState::Paused).unwrap();
    assert!(!rig.shell.lifecycle().render_gate());
    rig.host.run_until_idle();
    assert_eq!(rig.checked(), Some(true));
    assert_eq!(rig.changes.get(), 1);

    // Resume: one wake, same state.
    rig.shell.note_lifecycle(LifecycleState::Resumed).unwrap();
    assert!(rig.shell.lifecycle_mut().take_wake());
    rig.host.run_until_idle();
    assert_eq!(rig.checked(), Some(true));

    // Stop → destroy arms restart; relaunch starts clean.
    rig.shell.note_lifecycle(LifecycleState::Paused).unwrap();
    rig.shell.note_lifecycle(LifecycleState::Stopped).unwrap();
    assert!(!rig.shell.lifecycle().restart_armed());
    rig.shell.note_lifecycle(LifecycleState::Destroyed).unwrap();
    assert!(rig.shell.lifecycle().restart_armed());
}

#[test]
fn android_lifecycle_sync_updates_host_and_suspends_tickers() {
    let mut rig = Rig::new(1.0);
    assert_eq!(
        rig.host.lifecycle().get(),
        oppa::shell::AppLifecycleState::Active
    );
    assert!(!rig.host.is_lifecycle_suspended());

    // Resumed -> Active
    rig.shell
        .note_lifecycle_and_sync(&rig.host, LifecycleState::Created)
        .unwrap();
    assert_eq!(
        rig.host.lifecycle().get(),
        oppa::shell::AppLifecycleState::Paused
    );
    assert!(rig.host.is_lifecycle_suspended());

    rig.shell
        .note_lifecycle_and_sync(&rig.host, LifecycleState::Started)
        .unwrap();
    rig.shell
        .note_lifecycle_and_sync(&rig.host, LifecycleState::Resumed)
        .unwrap();
    assert_eq!(
        rig.host.lifecycle().get(),
        oppa::shell::AppLifecycleState::Active
    );
    assert!(!rig.host.is_lifecycle_suspended());

    // Pause -> Paused (tickers suspended)
    rig.shell
        .note_lifecycle_and_sync(&rig.host, LifecycleState::Paused)
        .unwrap();
    assert_eq!(
        rig.host.lifecycle().get(),
        oppa::shell::AppLifecycleState::Paused
    );
    assert!(rig.host.is_lifecycle_suspended());
    assert!(!rig.host.tick_flings());

    // Resume -> Active (tickers resumed)
    rig.shell
        .note_lifecycle_and_sync(&rig.host, LifecycleState::Resumed)
        .unwrap();
    assert_eq!(
        rig.host.lifecycle().get(),
        oppa::shell::AppLifecycleState::Active
    );
    assert!(!rig.host.is_lifecycle_suspended());

    // Stop -> Suspended
    rig.shell
        .note_lifecycle_and_sync(&rig.host, LifecycleState::Paused)
        .unwrap();
    rig.shell
        .note_lifecycle_and_sync(&rig.host, LifecycleState::Stopped)
        .unwrap();
    assert_eq!(
        rig.host.lifecycle().get(),
        oppa::shell::AppLifecycleState::Suspended
    );
    assert!(rig.host.is_lifecycle_suspended());
}

// ---------------------------------------------------------------------------
// 4. Restart == cold start (lock #16, mechanical).
// ---------------------------------------------------------------------------

/// Builds a fresh host, drives the fixed script through
/// Android-classified intake, settles fully, and returns the
/// surface PNG + the semantics dump + the checked value.
fn build_and_drive() -> (Vec<u8>, String, bool) {
    let host = ComponentHost::new();
    host.set_viewport(100.0, 60.0);
    let mut shell = AndroidShell::new(ShellConfig {
        density: 2.0,
        ..ShellConfig::default()
    });
    let on_change = HandlerId::from_symbol("test.m10.restart.change");
    let changes = Rc::new(Cell::new(0u32));
    host.runtime().register_handler(on_change, {
        let changes = changes.clone();
        move || changes.set(changes.get() + 1)
    });
    host.mount(
        "Toggle",
        ToggleProps {
            label: Arc::from("Wi-Fi"),
            initial: false,
            on_change,
        },
        Toggle,
    );
    host.run_until_idle();

    let backend = Rc::new(RefCell::new(oppa_cpu::CpuBackend::new()));
    let surface = backend
        .borrow_mut()
        .create_surface(SurfaceDesc {
            width_px: 100,
            height_px: 60,
            background: Color(0xFF_FF_FF),
        })
        .expect("restart surface");
    let paint_calls = Rc::new(Cell::new(0usize));
    let plan_ops = Rc::new(Cell::new(0usize));
    oppa_cpu::install_paint_hook(&host, backend.clone(), surface, 1.0, paint_calls, plan_ops);

    // The fixed script, driven identically on both builds (three
    // presses: ends on, proving dispatch happened odd times).
    for _ in 0..3 {
        for ev in [
            AndroidEvent::MotionDown {
                pointer_index: 0,
                x_dp: 5.0,
                y_dp: 6.0,
            },
            AndroidEvent::MotionUp {
                pointer_index: 0,
                x_dp: 5.0,
                y_dp: 6.0,
            },
        ] {
            shell.push_event(ev);
            for e in shell.pump_events() {
                assert_eq!(e.handler, field_event());
            }
            for cmd in shell.take_cmds() {
                host.inject_input(cmd.to_input_event());
            }
            host.run_until_idle();
        }
    }
    assert_eq!(changes.get(), 3);
    host.run_until_idle();
    assert!(!host.runtime().has_demand(), "script must settle fully");

    let png = backend
        .borrow()
        .encode_png(surface)
        .expect("restart png encodes");
    let mut snap = SemanticsSnapshot::new();
    let dump = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap).dump());
    let track = find_retained_by_debug(&host, "track")[0];
    let checked = host
        .retained_semantics(track)
        .and_then(|s| s.checked)
        .unwrap_or(false);
    (png, dump, checked)
}

#[test]
fn restart_reproduces_cold_start() {
    // Relaunch == fresh host (no state carried — lock #16); the same
    // script must produce the same pixels and the same a11y tree.
    let (png_a, dump_a, checked_a) = build_and_drive();
    let (png_b, dump_b, checked_b) = build_and_drive();
    assert!(!png_a.is_empty() && png_a.len() > 100);
    assert_eq!(png_a, png_b, "restart pixels diverged from cold start");
    assert_eq!(dump_a, dump_b, "restart semantics diverged from cold start");
    assert!(checked_a && checked_b);
    assert!(dump_a.contains("Wi-Fi") && dump_a.contains("Switch"));
}
