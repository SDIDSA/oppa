use super::*;

impl ComponentHost {
    /// One back-press step (round 3.3, OQ-G11-2 — the dismiss-first
    /// chain over host-owned state): an active composition cancels
    /// first (before any focus change — clearing focus would commit
    /// it instead, locked #27), else a focused node blurs, else the
    /// press is unhandled (the runner pops navigation or exits —
    /// see the `BackPress` section in `nav.rs` for the full
    /// cross-layer order). No settle inside (batch-safe like every
    /// other direct host mutation — runners settle/repaint after;
    /// `route_key` rides the INPUT batch, direct callers settle
    /// explicitly).
    pub fn handle_back(&self) -> BackOutcome {
        if self.cancel_edit_compositions() {
            return BackOutcome::CompositionCancelled;
        }
        if self.focused_node().is_some() {
            self.set_focus_node(None);
            return BackOutcome::FocusCleared;
        }
        BackOutcome::Unhandled
    }

    /// Requests window close from component code (Round 26.2,
    /// decision 342 -- desktop-only drain): sets the close flag;
    /// desktop runners drain it per pump iteration through the veto
    /// consult, so a dirty veto re-raises instead of exiting.
    /// Android/Web runners never drain it (stated parity gap --
    /// the flag simply stays set there).
    pub fn request_close(&self) {
        self.inner.close_requested_flag.set(true);
    }

    /// Drains the component close request (runner-side -- pumps call
    /// this once per iteration; a second call without a new request
    /// reads false, never a stuck exit).
    pub fn take_close_request(&self) -> bool {
        self.inner.close_requested_flag.replace(false)
    }

    /// Starts a platform-driven fetch (round 4.1, web fetch — the
    /// wasm half of decision 221): sets the keyed `FetchState<String>`
    /// to `Loading` synchronously (first paint already shows it, the
    /// native `spawn_fetch` rule) and bumps the key's generation,
    /// returning it. The platform binding passes the generation back
    /// to [`ComponentHost::resolve_fetch`] with the promise result —
    /// same generation-discard rule as the native path (§9.6).
    pub fn start_fetch(&self, key: u64) -> u64 {
        use crate::fetch::FetchState;
        let mut gens = self.inner.fetch_gens.borrow_mut();
        let gen = gens.get(&key).copied().unwrap_or(0) + 1;
        gens.insert(key, gen);
        self.inner
            .rt
            .keyed_state::<FetchState<String>>(key, || FetchState::Idle)
            .set(FetchState::Loading);
        gen
    }

    /// Resolves a platform-driven fetch (round 4.1): applies
    /// `Ready`/`Failed` iff `generation` is still current for `key`
    /// (a result landing after a re-fetch is discarded — `false`,
    /// never applied half-swapped). Unknown keys (never started)
    /// discard too (`false` — a typo'd name addresses nothing, and
    /// inventing `Loading` for it would lie about state). No settle
    /// inside (batch-safe — the binding settles by syncing after).
    pub fn resolve_fetch(&self, key: u64, generation: u64, result: Result<String, String>) -> bool {
        use crate::fetch::FetchState;
        let current = self.inner.fetch_gens.borrow().get(&key).copied();
        if current != Some(generation) {
            return false;
        }
        self.inner
            .rt
            .keyed_state::<FetchState<String>>(key, || FetchState::Idle)
            .set(match result {
                Ok(text) => FetchState::Ready(text),
                Err(e) => FetchState::Failed(e),
            });
        true
    }

    /// Reads the rendezvous `FetchState<String>` for `key`
    /// (diagnostics/bindings — `Idle` when never started; read-only,
    /// never schedules).
    pub fn fetch_snapshot(&self, key: u64) -> crate::fetch::FetchState<String> {
        use crate::fetch::FetchState;
        self.inner
            .rt
            .keyed_state::<FetchState<String>>(key, || FetchState::Idle)
            .get()
    }

    /// Reads the TIME transition evaluator (§9.4, M8): backends resolve
    /// evaluated paint values through this; tests read the interpolator
    /// instruments (created/suppressed/active) through it.
    pub fn with_evaluator<R>(&self, f: impl FnOnce(&TransitionEvaluator) -> R) -> R {
        f(&self.inner.evaluator.borrow())
    }

    /// Mutable evaluator access (the TIME-phase drive: `settle(now)`).
    pub fn with_evaluator_mut<R>(&self, f: impl FnOnce(&mut TransitionEvaluator) -> R) -> R {
        f(&mut self.inner.evaluator.borrow_mut())
    }

    pub fn run_until_idle(&self) -> usize {
        self.fire_due_longpresses();
        self.inner.rt.run_until_idle()
    }

    /// Runs one frame if there is demand (M8: TIME-stepped tests advance
    /// the injected clock one vsync at a time through this).
    pub fn run_once(&self) -> bool {
        self.fire_due_longpresses();
        self.inner.rt.run_once()
    }

    /// Fires every long-press arm whose deadline has passed (G11,
    /// decision 228 — the pump half of the no-self-demand rule: arms
    /// never create frame demand themselves, so a held finger can
    /// neither spin the loop nor hang `run_until_idle`; they fire on
    /// the first host pump at/after the deadline. Shell pump loops
    /// call `run_until_idle` per pump, so device fires land on time;
    /// on-demand loops fire on the next event or frame — and an
    /// Up-after-deadline dispatches identically either way, so the
    /// bound is unobservable there).
    pub fn fire_due_longpresses(&self) {
        if self.is_lifecycle_suspended() {
            return;
        }
        let now = self.inner.rt.now_secs();
        let ids: Vec<u32> = self.inner.longpress.borrow().keys().copied().collect();
        for id in ids {
            self.fire_arm_if_due(id, now);
        }
    }

    /// Ticks hover dwell for the currently hovered node (Round 17.3,
    /// decision 319 — called from shell pumps / on-demand loops).
    /// Suspended while the app is paused or suspended (Round 18.3, decision 322).
    pub fn tick_dwell(&self) {
        if self.is_lifecycle_suspended() {
            return;
        }
        if let Some(h) = self.inner.input.borrow().hover {
            self.bump_hover_move(h);
        }
    }

    pub fn inject_input(&self, event: InputEvent) {
        self.inner.rt.push_input(event);
    }

    // -- Debug Performance Overlay -------------------------------------

    /// Configures the debug performance overlay (FPS, frame time, dirty nodes, presenter).
    pub fn set_debug_overlay(&self, config: crate::debug_overlay::DebugOverlayConfig) {
        let mut meter = self.inner.debug_overlay.borrow_mut();
        meter.config = config;
    }

    /// Current debug performance overlay configuration.
    pub fn debug_overlay(&self) -> crate::debug_overlay::DebugOverlayConfig {
        self.inner.debug_overlay.borrow().config
    }

    /// Sets the presenter badge name (e.g. "Gpu", "CPU", "DOM") for the debug overlay.
    pub fn set_presenter_name(&self, name: impl Into<String>) {
        self.inner.debug_overlay.borrow_mut().presenter_name = Some(name.into());
    }

    /// Formatted debug overlay badge text, if enabled and non-empty.
    pub fn debug_overlay_badge(&self) -> Option<String> {
        let meter = self.inner.debug_overlay.borrow();
        if meter.config.enabled {
            let badge = meter.format_badge();
            if !badge.is_empty() {
                return Some(badge);
            }
        }
        None
    }

    /// Notes one presented frame for the debug overlay (runners call
    /// this after every present, both modes): advances the meter
    /// clock on present cadence, then — throttled to
    /// [`OVERLAY_BADGE_REFRESH_SECS`](crate::debug_overlay::OVERLAY_BADGE_REFRESH_SECS)
    /// — dirties the live root effects so the next settle re-runs
    /// the root and the badge text re-renders (child effects are
    /// never touched). Quiet no-op unless the overlay is enabled
    /// (one RefCell check on the present path).
    pub fn note_overlay_frame(&self) {
        if !self.inner.debug_overlay.borrow().config.enabled {
            return;
        }
        let now = self.inner.rt.now_secs();
        let mut meter = self.inner.debug_overlay.borrow_mut();
        meter.note_frame(now);
        if now - meter.last_refresh_secs >= crate::debug_overlay::OVERLAY_BADGE_REFRESH_SECS {
            meter.last_refresh_secs = now;
            let effects = self.inner.effects.borrow();
            let mut roots = self.inner.root_instances.borrow_mut();
            roots.retain(|inst| effects.contains_key(inst));
            for inst in roots.iter() {
                if let Some(effect) = effects.get(inst) {
                    self.inner.rt.mark_effect_dirty(effect.id());
                }
            }
        }
    }
}
