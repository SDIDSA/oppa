use super::*;

impl ComponentHost {
    /// Live momentum fling count (Round 10.2 — the 9.1 wait policy
    /// ticks at the poll bound while flings live; diagnostics/tests).
    /// Untracked (never schedules).
    pub fn fling_count(&self) -> usize {
        self.inner.flings.borrow().len()
    }

    /// Starts momentum from a released drag (Round 10.2, decision
    /// 304): release velocity measures the trailing samples inside
    /// the fling window, negated into content direction (like the
    /// drag stream itself). At or above
    /// [`FLING_MIN_VELOCITY_PX_S`](crate::input::
    /// FLING_MIN_VELOCITY_PX_S) it replaces any live fling on the
    /// container (re-flings win — the latest intent owns the motion);
    /// below it — including single-sample and zero-span releases,
    /// which measure zero and never divide — the release settles in
    /// place. Untracked.
    pub(super) fn maybe_start_fling(&self, drag: ScrollDrag, now: f64) {
        let Some(first) = drag.samples.first().copied() else {
            return;
        };
        let Some(last) = drag.samples.last().copied() else {
            return;
        };
        let dt = (last.2 - first.2).max(0.0);
        if dt <= 0.0 {
            return;
        }
        // Finger-space velocity, negated into content direction.
        let vx = -((last.0 - first.0) / dt as f32);
        let vy = -((last.1 - first.1) / dt as f32);
        if !vx.is_finite() || !vy.is_finite() {
            return;
        }
        if vx.hypot(vy) < input::FLING_MIN_VELOCITY_PX_S {
            return;
        }
        let mut flings = self.inner.flings.borrow_mut();
        flings.retain(|f| f.container != drag.container);
        flings.push(ScrollFling {
            container: drag.container,
            vx,
            vy,
            last_t: now,
        });
    }

    /// Ticks one live fling against the clock (Round 10.2, decision
    /// 304): the head fling integrates its exponential decay over
    /// `now - last_t` exactly (frame-rate independent — `v * tau *
    /// (1 - e^(-dt/tau))`), feeds the displacement, and retires below
    /// [`FLING_MIN_VELOCITY_PX_S`](crate::input::
    /// FLING_MIN_VELOCITY_PX_S) or when its container unmounts.
    /// Returns true when an offset moved (the caller repaints).
    /// Strictly one fling per call (paced decay over successive
    /// ticks, never a burst) and zero-`dt` ticks move nothing
    /// (same-tick re-entry is a safe no-op, never a spin).
    /// Untracked.
    pub fn tick_flings(&self) -> bool {
        if self.is_lifecycle_suspended() {
            return false;
        }
        let now = self.inner.rt.now_secs();
        let mut moved = false;
        // Settled heads drain through the loop; the first live fling
        // ticks and stops it (one live tick per call — paced decay,
        // never a burst).
        loop {
            let job = self.inner.flings.borrow().first().copied();
            let Some(mut f) = job else {
                break;
            };
            // A retired container scrolls no further (its feeds are
            // gone with it — the unmount path need not chase flings).
            if self.inner.rec.borrow().get(f.container).is_none() {
                self.inner.flings.borrow_mut().remove(0);
                continue;
            }
            let dt = (now - f.last_t).max(0.0);
            let decay = (-dt / input::FLING_DECAY_TAU_S).exp();
            let k = input::FLING_DECAY_TAU_S * (1.0 - decay);
            let (dx, dy) = (f.vx * k as f32, f.vy * k as f32);
            f.vx *= decay as f32;
            f.vy *= decay as f32;
            f.last_t = now;
            let settled = f.vx.hypot(f.vy) < input::FLING_MIN_VELOCITY_PX_S;
            if dx != 0.0 || dy != 0.0 {
                self.feed_scroll_deltas(f.container, dx, dy);
                moved = true;
            }
            let mut flings = self.inner.flings.borrow_mut();
            if settled {
                flings.remove(0);
            } else {
                flings[0] = f;
                break;
            }
        }
        moved
    }

    /// Streams `(dx, dy)` into a scroll container's bound feeds
    /// (Round 10.1, decision 303 — the wheel arm and the drag arm
    /// share this: one rule for both mechanisms). Explicit `dy` feeds
    /// accumulate unconditionally when bound (the M7 shape — even a
    /// zero delta writes, preserved exactly, never silently
    /// optimized); when no explicit vertical feed is bound, the target
    /// self-wires to its `Scroll` handler owner's instance offset
    /// (Round 24.2 — the text-feed precedent: controls such as
    /// `VirtualList` and `DataGrid` own an offset but never call
    /// `bind_scroll`). The self-wired write clamps to the target's
    /// committed content bounds (the 9.3 rule); `dx` accumulates into
    /// the bound horizontal feed when bound, clamped the same way;
    /// unbound axes ignore (the M5 dispatch-only rule).
    pub(super) fn feed_scroll_deltas(&self, target: NodeId, dx: f32, dy: f32) {
        if let Some(sig) = self.inner.scroll_feeds.borrow().get(&target).cloned() {
            sig.update(|v| v + dy);
        } else if dy != 0.0 {
            if let Some((sig, max_y)) = Self::owner_scroll_signal(&self.inner, target) {
                sig.update(|v| (v + dy).clamp(0.0, max_y));
            }
        }
        if dx != 0.0 {
            if let Some(sig) = self.inner.scroll_x_feeds.borrow().get(&target).cloned() {
                let max_x = self
                    .inner
                    .rec
                    .borrow()
                    .get(target)
                    .and_then(|n| n.layout.clone())
                    .map(|b| (b.content_w - b.w).max(0.0))
                    .unwrap_or(0.0);
                sig.update(|v| (v + dx).clamp(0.0, max_x));
            } else if let Some((sig, max_x)) = Self::owner_scroll_x_signal(&self.inner, target) {
                // Phase 36 PR2b (decision 354): the Round 24.2
                // self-wire transposed — unbound `dx` feeds the
                // target's handler-owner instance `scroll_x` (created
                // via `ctx.scroll_x()` / `ctx.scroll_2d()`), clamped
                // to the `content_w` overflow like the bound feed.
                // Targets with no Scroll handler (or no owner offset)
                // keep the M5 dispatch-only behavior.
                sig.update(|v| (v + dx).clamp(0.0, max_x));
            }
        }
    }

    /// Resolves a scroll target's self-wired vertical feed (Round 24.2):
    /// the target's `Scroll` handler names its declaring component
    /// instance, and that instance's framework-owned offset is the feed
    /// when the body created one with `ctx.scroll_offset()`. Returns
    /// the signal with the target's committed `[0, content_h - h]`
    /// bound. `None` when the target carries no `Scroll` handler, its
    /// owner is gone, the owner never created an offset, or the target
    /// has no committed box yet (quiet — the M5 dispatch-only rule,
    /// not a wiring bug).
    /// Takes `inner` (not `self`) so layout-phase code shares it
    /// without a host handle.
    pub(super) fn owner_scroll_signal(
        inner: &HostInner,
        target: NodeId,
    ) -> Option<(Signal<f32>, f32)> {
        let hid = {
            let rec = inner.rec.borrow();
            input::handler_of(&rec, target, EventKind::Scroll)?
        };
        let instance = inner.rt.handler_owner(hid)?;
        let signal = { inner.instances.borrow().get(&instance)?.scroll.clone()? };
        let max_y = {
            inner
                .rec
                .borrow()
                .get(target)?
                .layout
                .clone()
                .map(|b| (b.content_h - b.h).max(0.0))?
        };
        Some((signal, max_y))
    }

    /// Resolves a scroll target's self-wired horizontal feed (Phase 36
    /// PR2b, decision 354): the [`owner_scroll_signal`]
    /// twin transposed — the target's `Scroll` handler names its
    /// declaring component instance, and that instance's `scroll_x`
    /// offset is the feed when the body created one with
    /// `ctx.scroll_x()` / `ctx.scroll_2d()`. Clamped to the target's
    /// committed `[0, content_w - w]` (`content_w` overflow — narrow
    /// content pins at rest). `None` under the same quiet conditions
    /// as the vertical twin (the M5 dispatch-only rule). Takes
    /// `inner` (not `self`) like the twin.
    pub(super) fn owner_scroll_x_signal(
        inner: &HostInner,
        target: NodeId,
    ) -> Option<(Signal<f32>, f32)> {
        let hid = {
            let rec = inner.rec.borrow();
            input::handler_of(&rec, target, EventKind::Scroll)?
        };
        let instance = inner.rt.handler_owner(hid)?;
        let signal = { inner.instances.borrow().get(&instance)?.scroll_x.clone()? };
        let max_x = {
            inner
                .rec
                .borrow()
                .get(target)?
                .layout
                .clone()
                .map(|b| (b.content_w - b.w).max(0.0))?
        };
        Some((signal, max_x))
    }

    /// Streams one held Move into the pointer's drag-scroll state
    /// (Round 10.1, decision 303): a live drag re-anchors and feeds
    /// its move-to-move delta (finger retreat grows the offset —
    /// native direction); otherwise a capture displaced past tap
    /// slop inside a feed-bound scroll container arms the drag from
    /// the Down origin (the activating move streams its full
    /// displacement — no dead zone, so Down + Move(0,-50) scrolls
    /// exactly 50) and disarms the tap (the arm's `scrolling` flag).
    /// Moves with no capture, inside slop, outside scroll containers,
    /// or non-finite stay out (tap/swipe lifecycles untouched;
    /// malformed positions never poison a feed). Round 10.2: primary
    /// drags only (a secondary drag streams nothing — the 9.2
    /// primary-only rule, mechanism-shared); every streamed position
    /// trails into the release-velocity samples (window-pruned).
    pub(super) fn update_scroll_drag(&self, rt: &Runtime, id: u32, x: f32, y: f32) {
        let now = rt.now_secs();
        let live = {
            let mut drags = self.inner.scroll_drags.borrow_mut();
            match drags.get_mut(&id) {
                Some(drag) => {
                    let dx = x - drag.last_x;
                    let dy = y - drag.last_y;
                    drag.last_x = x;
                    drag.last_y = y;
                    drag.samples.push((x, y, now));
                    prune_samples(&mut drag.samples, now);
                    Some((drag.container, dx, dy))
                }
                None => None,
            }
        };
        if let Some((container, dx, dy)) = live {
            if dx.is_finite() && dy.is_finite() && (dx != 0.0 || dy != 0.0) {
                self.feed_scroll_deltas(container, -dx, -dy);
            }
            return;
        }
        // Activation: displacement past tap slop on a primary capture
        // whose owner lives in a feed-bound scroll container.
        let (owner, ox, oy, primary) = match (
            self.inner.input.borrow().captures.get(&id).copied(),
            self.inner
                .longpress
                .borrow()
                .get(&id)
                .map(|a| (a.x, a.y, a.button)),
        ) {
            (Some(owner), Some((ox, oy, button))) => {
                (owner, ox, oy, button == input::PointerButton::Primary)
            }
            _ => return,
        };
        if !primary {
            return;
        }
        let (dx0, dy0) = (x - ox, y - oy);
        if !dx0.is_finite() || !dy0.is_finite() || dx0.hypot(dy0) <= input::TAP_SLOP_PX {
            return;
        }
        let container = {
            let rec = self.inner.rec.borrow();
            input::scroll_owner_node(&rec, owner)
        };
        let Some(container) = container else {
            return;
        };
        // Opt-in per container (mirrors the wheel arm's per-axis
        // ignore rule — a container with no bound feed in either axis
        // never captures drags, so plain tap targets behave exactly
        // as before).
        let bound = self.inner.scroll_feeds.borrow().contains_key(&container)
            || self.inner.scroll_x_feeds.borrow().contains_key(&container);
        if !bound {
            return;
        }
        // Seed the velocity samples with the Down origin (the release
        // velocity measures origin-to-moves, windowed — a flick's
        // trailing motion names its speed).
        let t0 = self
            .inner
            .longpress
            .borrow()
            .get(&id)
            .map(|a| a.t0)
            .unwrap_or(now);
        let mut samples = vec![(ox, oy, t0), (x, y, now)];
        prune_samples(&mut samples, now);
        self.inner.scroll_drags.borrow_mut().insert(
            id,
            ScrollDrag {
                container,
                last_x: x,
                last_y: y,
                samples,
            },
        );
        if let Some(arm) = self.inner.longpress.borrow_mut().get_mut(&id) {
            arm.scrolling = true;
        }
        self.feed_scroll_deltas(container, -dx0, -dy0);
    }
}
