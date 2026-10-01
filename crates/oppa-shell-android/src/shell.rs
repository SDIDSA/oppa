//! The Android shell (M10): intake queue + [`PlatformShell`] impl.
//!
//! Mirrors `Win32Shell`'s trait shape on purpose (`pump_events` +
//! `set_ime` + a 1:1 payload-command queue the host loop drains in
//! event order): the JNI/Activity layer pushes
//! [`AndroidEvent`](super::events::AndroidEvent)s here, the loop
//! pumps M0-normalized [`Event`](oppa::shell::Event)s and drains
//! [`AndroidCmd`](super::events::AndroidCmd)s into
//! [`InputEvent`](oppa::InputEvent)s for the shared router. No
//! routing, no layout, no retained reads live here — same boundary
//! as Win32.
//!
//! IME: Android IME policy (show/hide + anchor) is an
//! `InputMethodManager` concern in the Activity layer; this shell
//! records the framework's [`ImeOps`](oppa::ImeOps) emissions
//! (`take_ime_log`) so the anchoring path is observable in tests,
//! exactly like the Win32 anchored-rects log.

use std::collections::VecDeque;

use oppa::handlers::HandlerId;
use oppa::ime::ImeOps;
use oppa::shell::{Event, EventKind, PlatformShell};

use super::events::{AndroidCmd, AndroidEvent, ShellError};
use super::lifecycle::{AndroidLifecycle, LifecycleState};

/// The handler id shell-pumped events dispatch under (the spike
/// host's `FIELD_EVENT` pattern: the app registers its field
/// mapping under this id; payloads ride the cmds queue).
pub fn field_event() -> HandlerId {
    HandlerId::from_symbol("android.field")
}

/// Shell construction parameters.
#[derive(Clone, Debug)]
pub struct ShellConfig {
    pub title: String,
    /// Display density (dp → px scale, e.g. 2.0 for xhdpi).
    pub density: f32,
    /// Initial surface size in dp.
    pub width_dp: f32,
    pub height_dp: f32,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            title: "oppa".to_string(),
            density: 1.0,
            width_dp: 360.0,
            height_dp: 640.0,
        }
    }
}

pub struct AndroidShell {
    queue: VecDeque<AndroidEvent>,
    cmds: VecDeque<AndroidCmd>,
    errors: Vec<ShellError>,
    ime_log: Vec<ImeOps>,
    lifecycle: AndroidLifecycle,
    density: f32,
    surface_px: (u32, u32),
    has_focus: bool,
    /// An editable field holds framework focus (runner-reported —
    /// the shell cannot name retained nodes). Pairs with
    /// `has_focus` for the IME visibility policy below.
    field_focused: bool,
    /// Last visibility the policy requested (change-only requests —
    /// the manager never gets a redundant show/hide).
    ime_shown: bool,
}

/// A keyboard visibility request for the JNI bridge (Round 3.1):
/// `showSoftInput` on editable focus, `hideSoftInputFromWindow`
/// on blur. Produced change-only by [`AndroidShell::poll_ime_request`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImeRequest {
    Show,
    Hide,
}

impl AndroidShell {
    pub fn new(config: ShellConfig) -> Self {
        let (w, h) = (
            (config.width_dp * config.density).round() as u32,
            (config.height_dp * config.density).round() as u32,
        );
        Self {
            queue: VecDeque::new(),
            cmds: VecDeque::new(),
            errors: Vec::new(),
            ime_log: Vec::new(),
            lifecycle: AndroidLifecycle::new(),
            density: config.density,
            surface_px: (w, h),
            has_focus: false,
            field_focused: false,
            ime_shown: false,
        }
    }

    /// JNI/Activity intake entry point (motion, keys, focus).
    pub fn push_event(&mut self, ev: AndroidEvent) {
        self.queue.push_back(ev);
    }

    /// Lifecycle entry point (Activity callbacks forward here).
    pub fn note_lifecycle(
        &mut self,
        to: LifecycleState,
    ) -> Result<(), super::lifecycle::LifecycleError> {
        self.lifecycle.transition(to)
    }

    /// Synchronizes current lifecycle state into the component host (Round 18.3, decision 322).
    pub fn sync_lifecycle(&self, host: &oppa::ComponentHost) {
        let app_state: oppa::shell::AppLifecycleState = self.lifecycle.state().into();
        host.set_lifecycle(app_state);
    }

    /// Transitions lifecycle state and synchronizes into the component host (Round 18.3, decision 322).
    pub fn note_lifecycle_and_sync(
        &mut self,
        host: &oppa::ComponentHost,
        to: LifecycleState,
    ) -> Result<(), super::lifecycle::LifecycleError> {
        self.lifecycle.transition(to)?;
        self.sync_lifecycle(host);
        Ok(())
    }

    /// Surface change entry point (`surfaceChanged`: new size in dp).
    pub fn note_surface_changed(&mut self, width_dp: f32, height_dp: f32) {
        self.surface_px = (
            (width_dp * self.density).round() as u32,
            (height_dp * self.density).round() as u32,
        );
    }

    pub fn surface_size_px(&self) -> (u32, u32) {
        self.surface_px
    }

    pub fn density(&self) -> f32 {
        self.density
    }

    /// Live device pixel ratio (G10, decision 226): the display
    /// density (dp-to-px is already density-driven — surface size,
    /// intake classification). Same alias shape as the Linux shell.
    pub fn device_pixel_ratio(&self) -> f32 {
        self.density
    }

    pub fn lifecycle(&self) -> &AndroidLifecycle {
        &self.lifecycle
    }

    pub fn lifecycle_mut(&mut self) -> &mut AndroidLifecycle {
        &mut self.lifecycle
    }

    pub fn has_focus(&self) -> bool {
        self.has_focus
    }

    /// Reports editable-field focus from the runner (framework focus
    /// state — the shell cannot name retained nodes) and returns the
    /// visibility request, if the policy state changed (convenience
    /// over set-then-poll for the field-driven path).
    pub fn note_field_focus(&mut self, editable_focused: bool) -> Option<ImeRequest> {
        self.field_focused = editable_focused;
        self.poll_ime_request()
    }

    /// IME visibility policy (Round 3.1): show iff the window has
    /// focus AND an editable field does; change-only (a redundant
    /// show/hide would flicker the manager and is never emitted).
    /// The runner calls this after every pump and after
    /// `note_field_focus`; the JNI bridge executes the request.
    pub fn poll_ime_request(&mut self) -> Option<ImeRequest> {
        let want = self.has_focus && self.field_focused;
        if want && !self.ime_shown {
            self.ime_shown = true;
            Some(ImeRequest::Show)
        } else if !want && self.ime_shown {
            self.ime_shown = false;
            Some(ImeRequest::Hide)
        } else {
            None
        }
    }

    /// Drains classified payload commands in event order.
    pub fn take_cmds(&mut self) -> Vec<AndroidCmd> {
        self.cmds.drain(..).collect()
    }

    /// Drains refused-intake errors (loud, counted — the test asserts
    /// multi-touch lands here and nowhere else).
    pub fn take_errors(&mut self) -> Vec<ShellError> {
        std::mem::take(&mut self.errors)
    }

    /// The framework's `ImeOps` emissions in order (anchoring path
    /// observability — the Win32 anchored-rects log's sibling).
    pub fn take_ime_log(&mut self) -> Vec<ImeOps> {
        std::mem::take(&mut self.ime_log)
    }
}

/// The M0 kind a command classifies as (same table as Win32's
/// `kind_of` — the trait stream is identical across shells). Text
/// commands map to `Ime` (their origin) but never reach the trait
/// stream — `pump_events` emits no `Event` for runner-matched
/// commands (no fabricated targets, the window-focus precedent).
fn kind_of(cmd: &AndroidCmd) -> EventKind {
    match cmd {
        AndroidCmd::PointerDown { .. } => EventKind::Press,
        AndroidCmd::PointerUp { .. } => EventKind::Release,
        AndroidCmd::PointerMove { .. } => EventKind::PointerMove,
        AndroidCmd::PointerCancel => EventKind::Release,
        AndroidCmd::Key { .. } => EventKind::Key,
        AndroidCmd::CommitText { .. } | AndroidCmd::DeleteSurrounding { .. } => EventKind::Ime,
    }
}

/// Inserts committed soft-keyboard text into the focused field's
/// session (Round 3.1): the runner-matched `CommitText` logic —
/// `true` iff a session was focused (the insert ran, empty text
/// included — inserts no-op on empty by session rule). Settles the
/// host (reactive effects observe the keystroke); repaint stays
/// runner-side (needs builder/cpu, not the shell's business).
pub fn commit_text_to_focused(host: &oppa::ComponentHost, text: &str) -> bool {
    let Some(session) = host.focused_field_session() else {
        return false;
    };
    session.insert(text);
    host.run_until_idle();
    true
}

/// Deletes surrounding text through the focused field's session
/// (Round 3.1): the runner-matched `DeleteSurrounding` logic —
/// `before_chars` backspaces then `after_chars` delete-forwards.
/// Counts are chars (session rule); `InputConnection` counts UTF-16
/// units, so astral-plane text may delete one unit more or less per
/// side — BMP-exact, stated bound. `true` iff a session was
/// focused. Settles like [`commit_text_to_focused`].
pub fn delete_surrounding_to_focused(
    host: &oppa::ComponentHost,
    before_chars: u32,
    after_chars: u32,
) -> bool {
    let Some(session) = host.focused_field_session() else {
        return false;
    };
    for _ in 0..before_chars {
        session.backspace();
    }
    for _ in 0..after_chars {
        session.delete_forward();
    }
    host.run_until_idle();
    true
}

impl PlatformShell for AndroidShell {
    fn pump_events(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        loop {
            let next = self.queue.pop_front();
            let Some(ev) = next else { break };
            // Window focus stays shell-side (no fabricated targets).
            if let AndroidEvent::FocusChanged(f) = ev {
                self.has_focus = f;
                continue;
            }
            match ev.classify(self.density) {
                Ok(Some(cmd)) => {
                    // Runner-matched text commands ride the cmds
                    // queue with no M0 event (the window-focus
                    // precedent — the loop consumes them directly,
                    // nothing routes them).
                    let runner_matched = matches!(
                        cmd,
                        AndroidCmd::CommitText { .. } | AndroidCmd::DeleteSurrounding { .. }
                    );
                    if !runner_matched {
                        out.push(Event {
                            kind: kind_of(&cmd),
                            handler: field_event(),
                        });
                    }
                    self.cmds.push_back(cmd);
                }
                Ok(None) => {}
                Err(e) => self.errors.push(e),
            }
        }
        out
    }

    fn set_ime(&mut self, ops: ImeOps) {
        self.ime_log.push(ops);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{classify_keycode, keycodes};
    use oppa::shell::PlatformShell;

    /// Round 5.3: D-pad maps to the arrow kinds (BACK→ESC pin
    /// lives in the contract test — both halves of the key table
    /// stay pinned).
    #[test]
    fn dpad_classifies_to_arrows() {
        assert_eq!(
            classify_keycode(keycodes::DPAD_LEFT),
            oppa::input::keys::LEFT
        );
        assert_eq!(classify_keycode(keycodes::DPAD_UP), oppa::input::keys::UP);
        assert_eq!(
            classify_keycode(keycodes::DPAD_RIGHT),
            oppa::input::keys::RIGHT
        );
        assert_eq!(
            classify_keycode(keycodes::DPAD_DOWN),
            oppa::input::keys::DOWN
        );
    }

    #[test]
    fn pump_classifies_dp_to_px_and_queues_cmds_in_order() {
        let mut shell = AndroidShell::new(ShellConfig {
            density: 2.0,
            ..ShellConfig::default()
        });
        shell.push_event(AndroidEvent::MotionDown {
            pointer_index: 0,
            x_dp: 5.0,
            y_dp: 6.0,
        });
        shell.push_event(AndroidEvent::MotionUp {
            pointer_index: 0,
            x_dp: 5.0,
            y_dp: 6.0,
        });
        let events = shell.pump_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, EventKind::Press);
        assert_eq!(events[0].handler, field_event());
        assert_eq!(events[1].kind, EventKind::Release);
        assert_eq!(
            shell.take_cmds(),
            vec![
                AndroidCmd::PointerDown {
                    id: 0,
                    x: 10.0,
                    y: 12.0
                },
                AndroidCmd::PointerUp {
                    id: 0,
                    x: 10.0,
                    y: 12.0
                },
            ]
        );
        assert!(shell.take_errors().is_empty());
    }

    #[test]
    fn second_pointer_routes_with_its_own_id() {
        // G11 retires the v1 refusal: index 1 classifies to pointer
        // id 1 (the router holds one capture per id — decision 227).
        let mut shell = AndroidShell::new(ShellConfig::default());
        shell.push_event(AndroidEvent::MotionDown {
            pointer_index: 1,
            x_dp: 5.0,
            y_dp: 6.0,
        });
        let events = shell.pump_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::Press);
        assert_eq!(
            shell.take_cmds(),
            vec![AndroidCmd::PointerDown {
                id: 1,
                x: 5.0,
                y: 6.0
            },]
        );
        assert!(shell.take_errors().is_empty(), "nothing refuses anymore");
    }

    #[test]
    fn focus_stays_shell_side_and_surface_tracks_density() {
        let mut shell = AndroidShell::new(ShellConfig {
            title: "t".to_string(),
            density: 2.0,
            width_dp: 360.0,
            height_dp: 640.0,
        });
        assert_eq!(shell.surface_size_px(), (720, 1280));
        shell.push_event(AndroidEvent::FocusChanged(true));
        assert!(shell.pump_events().is_empty());
        assert!(shell.has_focus());
        shell.note_surface_changed(400.0, 800.0);
        assert_eq!(shell.surface_size_px(), (800, 1600));
    }

    #[test]
    fn ime_ops_are_logged_in_order() {
        let mut shell = AndroidShell::new(ShellConfig::default());
        shell.set_ime(ImeOps::ShowCandidateWindow);
        shell.set_ime(ImeOps::HideCandidateWindow);
        assert_eq!(
            shell.take_ime_log(),
            vec![ImeOps::ShowCandidateWindow, ImeOps::HideCandidateWindow]
        );
    }

    /// Round 3.1 (decision 260): soft-keyboard text classifies to
    /// runner-matched commands with NO M0 event (no fabricated
    /// targets — the loop consumes them directly).
    #[test]
    fn text_intake_classifies_without_m0_events() {
        let mut shell = AndroidShell::new(ShellConfig::default());
        shell.push_event(AndroidEvent::CommitText {
            text: "nihao".to_string(),
        });
        shell.push_event(AndroidEvent::DeleteSurrounding {
            before_chars: 1,
            after_chars: 0,
        });
        assert!(
            shell.pump_events().is_empty(),
            "runner-matched text emits no trait events"
        );
        assert_eq!(
            shell.take_cmds(),
            vec![
                AndroidCmd::CommitText {
                    text: "nihao".to_string()
                },
                AndroidCmd::DeleteSurrounding {
                    before_chars: 1,
                    after_chars: 0,
                },
            ]
        );
        assert!(shell.take_errors().is_empty());
    }

    #[test]
    #[should_panic(expected = "must match CommitText first")]
    fn commit_text_refuses_pipeline_mapping_loudly() {
        AndroidCmd::CommitText {
            text: "x".to_string(),
        }
        .to_input_event();
    }

    #[test]
    #[should_panic(expected = "must match DeleteSurrounding first")]
    fn delete_surrounding_refuses_pipeline_mapping_loudly() {
        AndroidCmd::DeleteSurrounding {
            before_chars: 1,
            after_chars: 0,
        }
        .to_input_event();
    }

    /// Round 3.1 (decision 260): the visibility policy matrix —
    /// show on editable focus, change-only, hide on every blur.
    #[test]
    fn ime_visibility_policy_matrix() {
        let mut shell = AndroidShell::new(ShellConfig::default());
        // No window focus: field focus alone requests nothing.
        assert_eq!(shell.note_field_focus(true), None);
        // Window gains focus with the field focused: one Show.
        shell.push_event(AndroidEvent::FocusChanged(true));
        assert!(shell.pump_events().is_empty());
        assert_eq!(shell.poll_ime_request(), Some(ImeRequest::Show));
        // Steady state: no repeats (no manager flicker).
        assert_eq!(shell.poll_ime_request(), None);
        assert_eq!(shell.note_field_focus(true), None);
        // Field blur: one Hide.
        assert_eq!(shell.note_field_focus(false), Some(ImeRequest::Hide));
        assert_eq!(shell.poll_ime_request(), None);
        // Show again, then window blur hides (field state untouched).
        assert_eq!(shell.note_field_focus(true), Some(ImeRequest::Show));
        shell.push_event(AndroidEvent::FocusChanged(false));
        assert!(shell.pump_events().is_empty());
        assert_eq!(shell.poll_ime_request(), Some(ImeRequest::Hide));
        assert_eq!(shell.poll_ime_request(), None);
    }

    /// A field scene for the session helpers: one editable field
    /// over an app-owned value signal (the desktop `field_app`
    /// shape, Android-shell edition).
    #[derive(Clone)]
    struct FieldProps {
        value: oppa::Signal<oppa::SharedString>,
    }
    impl oppa::Props for FieldProps {}

    fn field_app(ctx: &oppa::Ctx, props: &FieldProps) -> oppa::VNode {
        let session = ctx.edit_session(props.value.clone());
        let _ = session;
        oppa::Div("field-box")
            .style(oppa::Style::new().size(200.0, 32.0))
            .semantics(oppa::Semantics::text_field().label("Name"))
            .on_press(|| {})
            .child(oppa::VNode::from(oppa::TextField {
                text: props.value.get(),
                style: oppa::Text::body_secondary,
                label: oppa::SharedString::from("Name"),
            }))
    }

    fn field_host() -> (oppa::ComponentHost, oppa::Signal<oppa::SharedString>) {
        let host = oppa::ComponentHost::new();
        host.set_viewport(200.0, 150.0);
        let value = host.runtime().signal(oppa::SharedString::from(""));
        host.mount(
            "Field",
            FieldProps {
                value: value.clone(),
            },
            field_app,
        );
        host.run_until_idle();
        // Tap the field to focus it (click-driven focus, the
        // framework rule — no fabricated targets).
        let id = oppa::find_retained_by_debug(&host, "field-box")
            .into_iter()
            .next()
            .expect("field box");
        let b = host.committed_box(id).expect("field box");
        host.inject_input(oppa::InputEvent::pointer_down(
            b.x + b.w / 2.0,
            b.y + b.h / 2.0,
        ));
        host.inject_input(oppa::InputEvent::pointer_up(
            b.x + b.w / 2.0,
            b.y + b.h / 2.0,
        ));
        host.run_until_idle();
        assert!(
            host.focused_field_session().is_some(),
            "tap focuses the field session"
        );
        (host, value)
    }

    /// Round 3.1 (decision 260): committed text lands in the
    /// focused session through the runner helper (the whole
    /// commit-into-field path, host-proven).
    #[test]
    fn commit_text_reaches_the_focused_session() {
        let (host, value) = field_host();
        assert!(commit_text_to_focused(&host, "nihao"));
        assert_eq!(&*value.get(), "nihao");
        assert!(commit_text_to_focused(&host, "\u{4F60}"));
        assert_eq!(&*value.get(), "nihao\u{4F60}");
    }

    #[test]
    fn surrounding_delete_edits_the_focused_session() {
        let (host, value) = field_host();
        assert!(commit_text_to_focused(&host, "nihao"));
        assert!(delete_surrounding_to_focused(&host, 2, 0));
        assert_eq!(&*value.get(), "nih");
        assert!(delete_surrounding_to_focused(&host, 0, 1));
        assert_eq!(&*value.get(), "nih");
        assert!(delete_surrounding_to_focused(&host, 0, 0));
        assert_eq!(&*value.get(), "nih");
    }

    #[test]
    fn unfocused_helpers_report_false_and_touch_nothing() {
        let host = oppa::ComponentHost::new();
        host.set_viewport(200.0, 150.0);
        let value = host.runtime().signal(oppa::SharedString::from("abc"));
        host.mount(
            "Field",
            FieldProps {
                value: value.clone(),
            },
            field_app,
        );
        host.run_until_idle();
        assert!(host.focused_field_session().is_none());
        assert!(!commit_text_to_focused(&host, "x"));
        assert!(!delete_surrounding_to_focused(&host, 1, 1));
        assert_eq!(&*value.get(), "abc");
    }

    /// Round 3.2 (decision 261): shell intake drives gesture
    /// recognition end to end — MotionDown/Move/Up through
    /// classification into the shared router, with router time
    /// from a mock clock. A fast far lift swipes (no press);
    /// a tap presses.
    #[derive(Clone)]
    struct SwipeProps {
        press: oppa::Signal<u32>,
        swipe: oppa::Signal<u32>,
    }
    impl oppa::Props for SwipeProps {}

    fn swipe_app(_ctx: &oppa::Ctx, props: &SwipeProps) -> oppa::VNode {
        let press = props.press.clone();
        let swipe = props.swipe.clone();
        oppa::Div("pad")
            .style(oppa::Style::new().size(200.0, 200.0))
            .on_press(move || press.set(press.get() + 1))
            .on_swipe(move || swipe.set(swipe.get() + 1))
            .build()
    }

    fn swipe_harness() -> (
        oppa::ComponentHost,
        std::rc::Rc<oppa::MockClock>,
        AndroidShell,
        SwipeProps,
        (f32, f32),
    ) {
        let clock = std::rc::Rc::new(oppa::MockClock::new());
        let host = oppa::ComponentHost::with_clock(clock.clone());
        host.set_viewport(200.0, 200.0);
        let press = host.runtime().signal(0u32);
        let swipe = host.runtime().signal(0u32);
        let props = SwipeProps {
            press: press.clone(),
            swipe: swipe.clone(),
        };
        host.mount("Swipe", props.clone(), swipe_app);
        host.run_until_idle();
        let id = oppa::find_retained_by_debug(&host, "pad")[0];
        let b = host.committed_box(id).expect("pad box");
        let center = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        let shell = AndroidShell::new(ShellConfig::default());
        (host, clock, shell, props, center)
    }

    fn pump_shell_into_host(
        shell: &mut AndroidShell,
        host: &oppa::ComponentHost,
        clock: &oppa::MockClock,
        dt: f64,
    ) {
        clock.set(clock.get() + dt);
        let _ = shell.pump_events();
        for cmd in shell.take_cmds() {
            // Text commands never reach the pipeline (runner-matched
            // — the refusal tests above); pointer/key cmds inject.
            if !matches!(
                cmd,
                AndroidCmd::CommitText { .. } | AndroidCmd::DeleteSurrounding { .. }
            ) {
                host.inject_input(cmd.to_input_event());
            }
        }
        host.run_until_idle();
    }

    #[test]
    fn shell_swipe_reaches_router_swipe_without_press() {
        let (host, clock, mut shell, props, (cx, cy)) = swipe_harness();
        shell.push_event(AndroidEvent::MotionDown {
            pointer_index: 0,
            x_dp: cx,
            y_dp: cy,
        });
        pump_shell_into_host(&mut shell, &host, &clock, 0.0);
        shell.push_event(AndroidEvent::MotionMove {
            pointer_index: 0,
            x_dp: cx + 40.0,
            y_dp: cy,
        });
        pump_shell_into_host(&mut shell, &host, &clock, 0.1);
        shell.push_event(AndroidEvent::MotionUp {
            pointer_index: 0,
            x_dp: cx + 40.0,
            y_dp: cy,
        });
        pump_shell_into_host(&mut shell, &host, &clock, 0.1);
        assert_eq!(props.press.get(), 0, "fling never presses");
        assert_eq!(props.swipe.get(), 1, "fling swipes");
    }

    #[test]
    fn shell_tap_reaches_router_press() {
        let (host, clock, mut shell, props, (cx, cy)) = swipe_harness();
        shell.push_event(AndroidEvent::MotionDown {
            pointer_index: 0,
            x_dp: cx,
            y_dp: cy,
        });
        pump_shell_into_host(&mut shell, &host, &clock, 0.0);
        shell.push_event(AndroidEvent::MotionUp {
            pointer_index: 0,
            x_dp: cx,
            y_dp: cy,
        });
        pump_shell_into_host(&mut shell, &host, &clock, 0.05);
        assert_eq!(props.press.get(), 1, "tap presses");
        assert_eq!(props.swipe.get(), 0, "tap never swipes");
    }
}
