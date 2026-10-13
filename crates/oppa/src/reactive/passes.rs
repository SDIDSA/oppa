// ---------------------------------------------------------------------------
// Pass hooks and demand introspection
// ---------------------------------------------------------------------------

use super::*;

impl Runtime {
    /// Registers a TIME-phase animation. While the animation set is
    /// non-empty frames continue at the (test-injected) cadence; when it
    /// drains the loop idles (§9.1: static UI ≈ 0 CPU).
    pub fn add_animation<F: FnMut(f64) -> bool + 'static>(&self, f: F) {
        self.state.borrow_mut().animations.push(Box::new(f));
    }

    pub fn set_reload_hook<F: FnMut(&Runtime) + 'static>(&self, f: F) {
        self.state.borrow_mut().reload_hook = Some(Box::new(f));
    }

    pub fn set_a11y_pass<F: FnMut() + 'static>(&self, f: F) {
        self.state.borrow_mut().a11y_pass = Some(Box::new(f));
    }

    /// Installs the framework-owned layout pass (M3: the component host
    /// installs the engine run; the phase never runs user or component
    /// code — §9.1). Replaces any previous pass.
    pub fn set_layout_pass<F: FnMut(&Runtime) + 'static>(&self, f: F) {
        self.state.borrow_mut().layout_pass = Some(Box::new(f));
    }

    /// Installs the paint pass (M4: whoever owns the presenter installs
    /// the FramePlan build + backend commit; same hook shape as
    /// [`set_layout_pass`](Runtime::set_layout_pass)). Replaces any
    /// previous pass. The phase never runs user or component code.
    pub fn set_paint_pass<F: FnMut(&Runtime) + 'static>(&self, f: F) {
        self.state.borrow_mut().paint_pass = Some(Box::new(f));
    }

    pub fn set_shell(&self, shell: Box<dyn crate::shell::PlatformShell>) {
        self.state.borrow_mut().shell = Some(shell);
    }

    pub fn stats(&self) -> Stats {
        let core = self.state.borrow();
        let mut stats = core.stats;
        stats.tasks_done = core.task_pump.tasks_done.load(Ordering::SeqCst);
        stats.tasks_dropped = core.task_pump.tasks_dropped.load(Ordering::SeqCst);
        stats
    }

    /// Drains the per-frame phase log (scheduler tests assert phase order
    /// against it).
    pub fn take_phase_log(&self) -> Vec<Phase> {
        std::mem::take(&mut self.state.borrow_mut().phase_log)
    }

    pub fn has_demand(&self) -> bool {
        let core = self.state.borrow();
        core.frame_requested
            || core.reload_requested
            || !core.pending.is_empty()
            || !core.animations.is_empty()
            || !core.queued_events.is_empty()
            || !core.queued_inputs.is_empty()
            || !core.worker_queue.is_empty()
            // Task traffic is demand too: pending tasks will produce
            // outbox results, and outbox results need an INPUT drain.
            || !core.task_pump.queue.lock().expect("task queue lock").is_empty()
            || !core.task_pump.outbox.lock().expect("task outbox lock").is_empty()
    }

    /// Returns which sources currently contribute to `has_demand`.
    pub fn demand_sources(&self) -> Vec<&'static str> {
        let core = self.state.borrow();
        let mut sources = Vec::new();
        if core.frame_requested {
            sources.push("frame_requested");
        }
        if core.reload_requested {
            sources.push("reload_requested");
        }
        if !core.pending.is_empty() {
            sources.push("pending");
        }
        if !core.animations.is_empty() {
            sources.push("animations");
        }
        if !core.queued_events.is_empty() {
            sources.push("queued_events");
        }
        if !core.queued_inputs.is_empty() {
            sources.push("queued_inputs");
        }
        if !core.worker_queue.is_empty() {
            sources.push("worker_queue");
        }
        if !core
            .task_pump
            .queue
            .lock()
            .expect("task queue lock")
            .is_empty()
        {
            sources.push("task_pump_queue");
        }
        if !core
            .task_pump
            .outbox
            .lock()
            .expect("task outbox lock")
            .is_empty()
        {
            sources.push("task_pump_outbox");
        }
        sources
    }

    /// Diagnostic query of labels for items currently in `pending`.
    pub fn pending_labels(&self) -> Vec<String> {
        let core = self.state.borrow();
        core.pending
            .iter()
            .map(|d| match d {
                crate::reactive::state::Dependent::Memo(id) => {
                    format!(
                        "Memo({:?}: {:?})",
                        id,
                        core.memos.try_get(*id).ok().and_then(|m| m.label.clone())
                    )
                }
                crate::reactive::state::Dependent::Effect(id) => {
                    format!(
                        "Effect({:?}: {:?})",
                        id,
                        core.effects.try_get(*id).ok().and_then(|e| e.label.clone())
                    )
                }
            })
            .collect()
    }
}
