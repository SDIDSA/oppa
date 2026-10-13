// ---------------------------------------------------------------------------
// Repaint, blink, timers (delegating to SceneCore)
// ---------------------------------------------------------------------------

use super::*;

impl DesktopLoop {
    /// Settles and repaints unconditionally (warmup frames, resizes).
    /// The full engine rule lives on [`SceneCore::repaint`] — commits
    /// in both backends, overlay publish, theme contract, one plan,
    /// image sync, both-arm paints (GPU arm only when active).
    pub fn repaint(&mut self) -> Result<usize, String> {
        self.scene.repaint()
    }

    /// The last committed frame plan (Round 15.2, decision 313 —
    /// proves which overlay `Rect`s a repaint committed; `None`
    /// before the first repaint, never a live borrow).
    pub fn last_plan(&self) -> Option<FramePlan> {
        self.scene.last_plan()
    }

    /// Seconds until the focused caret's next blink flip, if any
    /// (Round 15.2, decision 313 — the runner wake query). Pure over
    /// the host — headless-tested.
    pub fn blink_tick_in_secs(&self) -> Option<f64> {
        self.scene.blink_tick_in_secs()
    }

    /// True when the caret overlay's presence flipped since the last
    /// repaint (Round 15.2, decision 313 — the blink demand). Pure
    /// over the host — headless-tested.
    pub fn caret_overlay_dirty(&self) -> bool {
        self.scene.caret_overlay_dirty()
    }

    /// Settles and repaints iff the caret overlay flipped (Round
    /// 15.2, decision 313 — the timer-tick path runners call when
    /// no input arrived): returns `true` when it repainted, `false`
    /// when settled-quiet (never a spin — a quiet poll schedules
    /// nothing). Paint failures are loud `Err`s like `repaint`.
    pub fn poll_blink(&mut self) -> Result<bool, String> {
        self.scene.poll_blink()
    }

    /// Earliest component-timer due instant in host-clock ms (Round
    /// 21.1, decision 328 — the wake-query twin of
    /// `blink_tick_in_secs`): `Some` while a component timer is
    /// armed (runners wake then to fire it); `None` with no timers
    /// or while lifecycle-suspended. Pure over the host —
    /// headless-tested.
    pub fn next_timer_due_ms(&self) -> Option<f64> {
        self.scene.next_timer_due_ms()
    }

    /// Fires due component timers, settling + repainting when
    /// anything fired (Round 21.1, decision 328 — the timer-tick
    /// path runners call on wake): returns the fired count. Zero
    /// when nothing was due (quiet — never a spin: firing
    /// schedules only what callbacks write, and the horizon then
    /// moves past `now`). Paint failures are loud `Err`s.
    pub fn tick_timers(&mut self) -> Result<usize, String> {
        self.scene.tick_timers()
    }

    /// Resolves the pump wait timeout around the platform horizon
    /// (headless-testable choke point both runners share): `Some(0)`
    /// (immediate poll — never sleeps) while continuous repaint
    /// runs, otherwise the platform-computed idle horizon unchanged
    /// (event-driven waits, blink/timer wakes).
    pub fn pump_wait_timeout_ms(&self, idle_horizon_ms: Option<u32>) -> Option<u32> {
        if self.continuous_repaint {
            Some(0)
        } else {
            idle_horizon_ms
        }
    }

    /// Re-arms one continuous frame after a present (benchmark mode):
    /// marks the scene dirty and requests the next frame so the
    /// runner's settle path repaints + presents again instead of
    /// going idle. Quiet no-op unless continuous repaint is armed
    /// (runners call this after every present; the guard keeps the
    /// event-driven path exactly as before).
    pub fn rearm_continuous_frame(&self) {
        if self.continuous_repaint {
            self.scene.host().runtime().request_frame();
        }
    }

    /// Notes one presented frame for the debug overlay (runners call
    /// this after every present, both modes — the same engine rule
    /// the Android presenter drives): the meter clock ticks on
    /// present cadence and the badge text refreshes at most 4Hz
    /// through a throttled root re-run. Quiet no-op unless the
    /// overlay is enabled.
    pub fn note_frame_presented(&self) {
        self.scene.note_frame_presented();
    }
}
