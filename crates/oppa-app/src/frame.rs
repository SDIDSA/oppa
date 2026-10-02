// ---------------------------------------------------------------------------
// Repaint, blink, timers
// ---------------------------------------------------------------------------

use super::*;

impl DesktopLoop {
    /// Settles and repaints unconditionally (warmup frames, resizes).
    /// Commits always land in both backends (cheap bookkeeping);
    /// pixels encode into the CPU pixmap always (the GDI /
    /// softbuffer source and the GPU fallback) plus into the Vello
    /// twin whenever GPU is active. CPU-only repaints skip Vello so
    /// a faceless test shaper never trips the Vello loud-no-face
    /// rule while the CPU path still draws bars.
    pub fn repaint(&mut self) -> Result<usize, String> {
        for d in self.host.diffs_from(0) {
            self.cpu
                .commit(&d)
                .map_err(|e| format!("desktop loop: cpu commit: {e:?}"))?;
            self.vello
                .commit(&d)
                .map_err(|e| format!("desktop loop: vello commit: {e:?}"))?;
            self.custom_gpu
                .commit(&d)
                .map_err(|e| format!("desktop loop: custom_gpu commit: {e:?}"))?;
        }
        // Round 15.2 (decision 313): the text overlays ride every
        // committed frame — the shared build-scoped rule from the
        // paint hooks (selection + caret resolved outside the
        // retained borrow). Before this, live presented frames
        // committed with both overlays `None`: zero selection rects
        // and zero carets on GPU and GDI alike.
        self.builder
            .set_selection(self.host.focused_selection_paint());
        self.builder.set_caret(self.host.focused_caret_paint());
        // Theme contract round: the build theme rides the same
        // per-frame publish rule (default text ink follows the host
        // mode), and a toggled page background refits the surfaces
        // before painting (same one-refit path as resize/DPR —
        // steady state is a color compare, never surface churn).
        let mode = self.host.theme().mode();
        self.builder.set_theme_mode(mode);
        let want_bg = ThemeTokens::of(mode).background;
        if want_bg != self.page_bg {
            self.page_bg = want_bg;
            self.refit_surface()?;
        }
        let plan = self
            .host
            .with_retained_mut(|rec, styles| self.builder.build_full(rec, styles));
        self.last_caret_shown = self.builder.caret().is_some();
        self.last_plan = Some(plan.clone());
        let damage = plan.damage.len();
        let images = self.host.images();
        for op in &plan.ops {
            if let oppa::DrawOp::RImg { image, .. } = op {
                if !self.cpu.has_image(*image) && images.pixels_of(*image).is_some() {
                    self.cpu.insert_cached(&images, *image);
                }
                if self.active == RendererKind::Gpu
                    && !self.vello.has_image(*image)
                    && images.pixels_of(*image).is_some()
                {
                    self.vello.insert_cached(&images, *image);
                } else if self.active == RendererKind::CustomGpu
                    && !self.custom_gpu.has_image(*image)
                    && images.pixels_of(*image).is_some()
                {
                    self.custom_gpu.insert_cached(&images, *image);
                }
            }
        }
        self.cpu
            .paint(self.surface, &plan)
            .map_err(|e| format!("desktop loop: cpu paint: {e:?}"))?;
        if self.active == RendererKind::Gpu {
            self.vello
                .paint(self.vello_surface, &plan)
                .map_err(|e| format!("desktop loop: vello paint: {e:?}"))?;
        } else if self.active == RendererKind::CustomGpu {
            self.custom_gpu
                .paint(self.custom_gpu_surface, &plan)
                .map_err(|e| format!("desktop loop: custom_gpu paint: {e:?}"))?;
        }
        Ok(damage)
    }

    /// The last committed frame plan (Round 15.2, decision 313 —
    /// proves which overlay `Rect`s a repaint committed; `None`
    /// before the first repaint, never a live borrow).
    pub fn last_plan(&self) -> Option<FramePlan> {
        self.last_plan.clone()
    }

    /// Seconds until the focused caret's next blink flip, if any
    /// (Round 15.2, decision 313 — the runner wake query): `Some`
    /// while a collapsed caret is focused (wake then to repaint the
    /// flip); `None` otherwise (unfocused, selected, session-less —
    /// the loop stays event-driven). Pure over the host —
    /// headless-tested.
    pub fn blink_tick_in_secs(&self) -> Option<f64> {
        self.host.caret_blink_in_secs()
    }

    /// True when the caret overlay's presence flipped since the last
    /// repaint (Round 15.2, decision 313 — the blink demand): the
    /// flip writes no signals and dirties nothing, so the tick path
    /// compares presence instead of waiting for demand that never
    /// comes. Pure over the host — headless-tested.
    pub fn caret_overlay_dirty(&self) -> bool {
        self.host.focused_caret_paint().is_some() != self.last_caret_shown
    }

    /// Settles and repaints iff the caret overlay flipped (Round
    /// 15.2, decision 313 — the timer-tick path runners call when
    /// no input arrived): returns `true` when it repainted, `false`
    /// when settled-quiet (never a spin — a quiet poll schedules
    /// nothing). Paint failures are loud `Err`s like `repaint`.
    pub fn poll_blink(&mut self) -> Result<bool, String> {
        self.host.run_until_idle();
        if self.caret_overlay_dirty() {
            self.repaint()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Earliest component-timer due instant in host-clock ms (Round
    /// 21.1, decision 328 — the wake-query twin of
    /// `blink_tick_in_secs`): `Some` while a component timer is
    /// armed (runners wake then to fire it); `None` with no timers
    /// or while lifecycle-suspended. Pure over the host —
    /// headless-tested.
    pub fn next_timer_due_ms(&self) -> Option<f64> {
        let now = self.host.now_ms();
        self.host.next_timer_due_ms(now)
    }

    /// Fires due component timers, settling + repainting when
    /// anything fired (Round 21.1, decision 328 — the timer-tick
    /// path runners call on wake): returns the fired count. Zero
    /// when nothing was due (quiet — never a spin: firing
    /// schedules only what callbacks write, and the horizon then
    /// moves past `now`). Paint failures are loud `Err`s.
    pub fn tick_timers(&mut self) -> Result<usize, String> {
        let now = self.host.now_ms();
        let fired = self.host.tick_timers(now);
        if fired > 0 {
            self.host.run_until_idle();
            self.repaint()?;
        }
        Ok(fired)
    }
}
