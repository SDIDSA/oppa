use super::*;

impl ComponentHost {
    /// Current host-clock time in ms (Round 21.1, decision 328 —
    /// the component-timer base: timers key off the same TIME the
    /// scheduler advances, so `MockClock` tests and runners agree).
    pub fn now_ms(&self) -> f64 {
        self.inner.rt.now_secs() * 1000.0
    }

    /// Registers a component timer (Round 21.1, decision 328):
    /// absolute `due_ms`, `None` period for one-shots. Returns the
    /// id the owner's render cleanup cancels (re-render or
    /// unmount — the 18.2 lifecycle).
    pub(crate) fn push_timer(
        &self,
        owner: u64,
        due_ms: f64,
        period_ms: Option<f64>,
        callback: Rc<dyn Fn()>,
    ) -> TimerId {
        let id = self.inner.next_timer.get();
        self.inner.next_timer.set(id + 1);
        self.inner.timers.borrow_mut().push(HostTimer {
            id,
            owner,
            due_ms,
            period_ms,
            callback,
        });
        TimerId(id)
    }

    /// Cancels one timer (Round 21.1 — early cancel plus the
    /// cleanup path): `true` when an entry died, `false` for an
    /// unknown/already-fired id (quiet — double-cancel is not a
    /// wiring bug).
    pub fn cancel_timer(&self, id: TimerId) -> bool {
        let mut timers = self.inner.timers.borrow_mut();
        let before = timers.len();
        timers.retain(|t| t.id != id.0);
        timers.len() != before
    }

    /// Earliest live timer due instant in host-clock ms (Round 21.1
    /// — the runner wake query): `None` with no timers, or while
    /// lifecycle-suspended (suspension freezes timer progression —
    /// runners must not wake for frozen timers).
    pub fn next_timer_due_ms(&self, now_ms: f64) -> Option<f64> {
        assert!(
            now_ms.is_finite(),
            "next_timer_due_ms: clock must be finite, got {now_ms}"
        );
        if self.is_lifecycle_suspended() {
            return None;
        }
        self.inner
            .timers
            .borrow()
            .iter()
            .map(|t| t.due_ms)
            .reduce(f64::min)
    }

    /// Fires every timer due at `now_ms` (Round 21.1, decision 328):
    /// one-shots are consumed, intervals advance (missed periods
    /// snap to `now + period` — steady cadence resumes, never a
    /// catch-up burst), and timers whose owner instance is gone are
    /// dropped (the unmount cleanup is the primary path; this sweep
    /// is the backstop). Returns the fired count. Zero — and frozen,
    /// advancing nothing — while lifecycle-suspended. Due entries
    /// are collected before any callback runs (callbacks may
    /// register or cancel timers — never a borrow panic, never a
    /// skipped entry). Untracked (never schedules — the caller
    /// settles, like every other ticker).
    pub fn tick_timers(&self, now_ms: f64) -> usize {
        assert!(
            now_ms.is_finite(),
            "tick_timers: clock must be finite, got {now_ms}"
        );
        if self.is_lifecycle_suspended() {
            return 0;
        }
        let mut fired: Vec<Rc<dyn Fn()>> = Vec::new();
        self.inner.timers.borrow_mut().retain_mut(|t| {
            if self.inner.instances.borrow().get(&t.owner).is_none() {
                return false;
            }
            if t.due_ms <= now_ms {
                fired.push(t.callback.clone());
                match t.period_ms {
                    None => return false,
                    Some(p) => {
                        t.due_ms = (t.due_ms + p).max(now_ms + p);
                        return true;
                    }
                }
            }
            true
        });
        let n = fired.len();
        for cb in fired {
            cb();
        }
        n
    }
}
