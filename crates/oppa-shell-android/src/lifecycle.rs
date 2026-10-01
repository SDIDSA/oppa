//! Android activity lifecycle as a state machine (M10).
//!
//! What "matters" for a retained scene graph, concretely (not
//! everything Android exposes — configuration changes, multi-window
//! modes, and picture-in-picture are platform-track follow-ups that
//! reduce to `SurfaceChanged` + pause/resume through this machine):
//!
//! - **Pause/Stopped closes the render gate.** The shell stops
//!   requesting frames; the retained graph is untouched — signals,
//!   `keyed_state`, and the `Store` stay alive across pause/resume
//!   (proven in `tests/android_contract.rs`: a probe value set
//!   before pause reads back after resume).
//! - **Resume opens the gate and raises one wake.** The host loop
//!   consumes it as a single `request_frame` (settled state
//!   re-presents; no event is fabricated).
//! - **Destroy arms restart.** Relaunch builds a **fresh** host —
//!   the cold-start path, lock #16 — carrying no state across.
//!   `tests/android_contract.rs` proves restart determinism
//!   (same script, same pixels, same semantics dump).
//! - **Illegal jumps are loud.** `transition` returns
//!   [`LifecycleError`]; the `on_*` glue panics on it (a lifecycle
//!   bug in the Activity layer must crash at the call site, never
//!   silently clamp the machine into a wrong state).

/// Activity states in framework order.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum LifecycleState {
    /// Before `onCreate` (shell constructed, nothing running).
    #[default]
    Initialized,
    /// `onCreate` ran (host built, surface pending).
    Created,
    /// `onStart` ran (visible).
    Started,
    /// `onResume` ran (interactive — the only frame-requesting state).
    Resumed,
    /// `onPause` ran (partially obscured — gate closed).
    Paused,
    /// `onStop` ran (invisible — gate closed, graph retained).
    Stopped,
    /// `onDestroy` ran (restart armed — relaunch is a fresh host).
    Destroyed,
}

/// Why a lifecycle step was refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LifecycleError {
    /// The jump is not an Activity-legal edge.
    IllegalTransition {
        from: LifecycleState,
        to: LifecycleState,
    },
}

impl std::fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LifecycleError::IllegalTransition { from, to } => write!(
                f,
                "illegal Android lifecycle jump {from:?} → {to:?} — Activity glue bug, refusing"
            ),
        }
    }
}

impl std::error::Error for LifecycleError {}

impl From<LifecycleState> for oppa::shell::AppLifecycleState {
    fn from(state: LifecycleState) -> Self {
        match state {
            LifecycleState::Resumed => oppa::shell::AppLifecycleState::Active,
            LifecycleState::Paused => oppa::shell::AppLifecycleState::Paused,
            LifecycleState::Stopped | LifecycleState::Destroyed => {
                oppa::shell::AppLifecycleState::Suspended
            }
            LifecycleState::Initialized | LifecycleState::Created | LifecycleState::Started => {
                oppa::shell::AppLifecycleState::Paused
            }
        }
    }
}

/// The machine: state + render gate + resume wake + restart arm.
#[derive(Clone, Debug)]
pub struct AndroidLifecycle {
    state: LifecycleState,
    render_gate: bool,
    pending_wake: bool,
    restart_armed: bool,
}

impl Default for AndroidLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl AndroidLifecycle {
    pub fn new() -> Self {
        Self {
            state: LifecycleState::Initialized,
            render_gate: false,
            pending_wake: false,
            restart_armed: false,
        }
    }

    /// Legal Activity edges (the full graph, including relaunch
    /// `Stopped → Started` and `Destroyed → Created`).
    fn legal(from: LifecycleState, to: LifecycleState) -> bool {
        use LifecycleState::*;
        matches!(
            (from, to),
            (Initialized, Created)
                | (Created, Started)
                | (Started, Resumed)
                | (Resumed, Paused)
                | (Paused, Resumed)
                | (Paused, Stopped)
                | (Stopped, Started)
                | (Stopped, Destroyed)
                | (Destroyed, Created)
        )
    }

    /// Steps the machine, applying the retained-graph side effects.
    pub fn transition(&mut self, to: LifecycleState) -> Result<(), LifecycleError> {
        if !Self::legal(self.state, to) {
            return Err(LifecycleError::IllegalTransition {
                from: self.state,
                to,
            });
        }
        self.state = to;
        match to {
            LifecycleState::Resumed => {
                self.render_gate = true;
                self.pending_wake = true;
            }
            LifecycleState::Paused | LifecycleState::Stopped => {
                self.render_gate = false;
            }
            LifecycleState::Destroyed => {
                self.render_gate = false;
                self.restart_armed = true;
            }
            LifecycleState::Initialized | LifecycleState::Created | LifecycleState::Started => {}
        }
        Ok(())
    }

    /// `onCreate` / `onStart` / `onResume` / `onPause` / `onStop` /
    /// `onDestroy` glue entry points (panic on illegal jumps — loud).
    pub fn on_create(&mut self) {
        self.transition(LifecycleState::Created).expect("lifecycle");
    }
    pub fn on_start(&mut self) {
        self.transition(LifecycleState::Started).expect("lifecycle");
    }
    pub fn on_resume(&mut self) {
        self.transition(LifecycleState::Resumed).expect("lifecycle");
    }
    pub fn on_pause(&mut self) {
        self.transition(LifecycleState::Paused).expect("lifecycle");
    }
    pub fn on_stop(&mut self) {
        self.transition(LifecycleState::Stopped).expect("lifecycle");
    }
    pub fn on_destroy(&mut self) {
        self.transition(LifecycleState::Destroyed)
            .expect("lifecycle");
    }

    pub fn state(&self) -> LifecycleState {
        self.state
    }

    /// Whether the shell may request frames (open only in `Resumed`).
    pub fn render_gate(&self) -> bool {
        self.render_gate
    }

    /// Takes the resume wake (one `request_frame` for the host loop).
    pub fn take_wake(&mut self) -> bool {
        std::mem::replace(&mut self.pending_wake, false)
    }

    /// Whether `onDestroy` ran (relaunch must build a fresh host).
    pub fn restart_armed(&self) -> bool {
        self.restart_armed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_activity_cycle_gates_and_arms() {
        let mut lc = AndroidLifecycle::new();
        assert!(!lc.render_gate());
        lc.on_create();
        lc.on_start();
        assert!(!lc.render_gate());
        lc.on_resume();
        assert!(lc.render_gate());
        assert!(lc.take_wake());
        assert!(!lc.take_wake());
        lc.on_pause();
        assert!(!lc.render_gate());
        lc.on_resume();
        assert!(lc.take_wake());
        lc.on_pause();
        lc.on_stop();
        assert!(!lc.restart_armed());
        lc.on_destroy();
        assert!(lc.restart_armed());
        // Relaunch is a fresh cycle through Created.
        lc.on_create();
        assert_eq!(lc.state(), LifecycleState::Created);
    }

    #[test]
    fn illegal_jumps_are_loud() {
        let mut lc = AndroidLifecycle::new();
        assert_eq!(
            lc.transition(LifecycleState::Resumed),
            Err(LifecycleError::IllegalTransition {
                from: LifecycleState::Initialized,
                to: LifecycleState::Resumed,
            })
        );
        lc.on_create();
        lc.on_start();
        lc.on_resume();
        assert!(lc.transition(LifecycleState::Destroyed).is_err());
        assert!(lc.transition(LifecycleState::Created).is_err());
    }

    #[test]
    fn stop_to_start_restart_keeps_no_restart_arm() {
        let mut lc = AndroidLifecycle::new();
        lc.on_create();
        lc.on_start();
        lc.on_resume();
        lc.on_pause();
        lc.on_stop();
        lc.on_start();
        assert!(!lc.restart_armed());
        assert_eq!(lc.state(), LifecycleState::Started);
    }
}
