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

    /// App-readable system-bars mode (Android screen-mode toggle):
    /// `false` = hidden/immersive (the boot mode), `true` = status +
    /// navigation shown. Tracked (mode chrome readers re-render on
    /// flips); writing it via [`ComponentHost::set_system_bars_visible`]
    /// never dirties layout (the scene stays full-bleed either way —
    /// only the host bars show/hide).
    pub fn system_bars_visible(&self) -> Signal<bool> {
        let mut slot = self.inner.system_bars_visible_sig.borrow_mut();
        if let Some(sig) = slot.clone() {
            return sig;
        }
        let sig = self.inner.rt.signal(false);
        *slot = Some(sig.clone());
        sig
    }

    /// Requests the system-bars mode from component code (Android
    /// screen-mode toggle): writes the visible signal on change and
    /// arms the runner command (last write wins — rapid flips drain
    /// once). Runners drain it per pump iteration
    /// ([`ComponentHost::take_system_bars_request`]); hosts with no
    /// runner drain (headless/desktop/Web) keep the signal update and
    /// stay effect-free (advisory chrome, the cursor precedent).
    pub fn set_system_bars_visible(&self, visible: bool) {
        let sig = self.system_bars_visible();
        if untrack(|| sig.get()) != visible {
            sig.set(visible);
        }
        self.inner.system_bars_request.set(Some(visible));
    }

    /// Drains the app-requested system-bars mode (runner-side --
    /// Android pumps call this once per iteration and apply the mode
    /// through the bars JNI bridge; `None` means no request since the
    /// last drain, never a default to apply).
    pub fn take_system_bars_request(&self) -> Option<bool> {
        self.inner.system_bars_request.take()
    }

    /// Runner-published scene toolbar (decision 455): the live model
    /// the root wrap renders as a floating overlay. Tracked (the root
    /// wrap reads it — publishing re-renders the root, clearing
    /// reverts to the bare tree). Lazy like the bars signal above.
    pub fn toolbar_model(&self) -> Signal<Option<crate::input::ToolbarModel>> {
        let mut slot = self.inner.toolbar_model_sig.borrow_mut();
        if let Some(sig) = slot.clone() {
            return sig;
        }
        let sig = self.inner.rt.signal(None);
        *slot = Some(sig.clone());
        sig
    }

    /// Publishes the scene toolbar model (runner-side — the Android
    /// pump calls this per decision from its input controller).
    /// Change-only (an identical re-publish never re-runs the root —
    /// per-pump `Update` frames with the same model stay free).
    pub fn set_toolbar_model(&self, model: Option<crate::input::ToolbarModel>) {
        let sig = self.toolbar_model();
        if untrack(|| sig.get()) != model {
            sig.set(model);
        }
    }

    /// Native caption hover (decision 486): the live caption button
    /// under the cursor, if any. Tracked (the `HeaderBar` reads it —
    /// hover flips re-render the bar with the glow, clearing reverts
    /// to bare chrome). Lazy like the toolbar signal above.
    pub fn caption_hover(&self) -> Signal<Option<crate::CaptionButton>> {
        let mut slot = self.inner.caption_hover_sig.borrow_mut();
        if let Some(sig) = slot.clone() {
            return sig;
        }
        let sig = self.inner.rt.signal(None);
        *slot = Some(sig.clone());
        sig
    }

    /// Publishes the native caption hover (runner-side — the Windows
    /// pump drains the shell hover state per iteration). Change-only
    /// (an identical re-publish never re-renders — per-pump repeats
    /// with the same hover stay free, so hovering never spins the
    /// loop).
    pub fn set_caption_hover(&self, hover: Option<crate::CaptionButton>) {
        let sig = self.caption_hover();
        if untrack(|| sig.get()) != hover {
            sig.set(hover);
        }
    }

    /// Maximized window state (decision 495): tracked, default
    /// `false` (headless loops never maximize). `CaptionButtons`
    /// reads it to swap the maximize glyph for the restore glyph.
    /// Lazy like the hover signal above.
    pub fn maximized(&self) -> Signal<bool> {
        let mut slot = self.inner.maximized_sig.borrow_mut();
        if let Some(sig) = slot.clone() {
            return sig;
        }
        let sig = self.inner.rt.signal(false);
        *slot = Some(sig.clone());
        sig
    }

    /// Publishes the maximized state (runner-side — desktop pumps
    /// drain the live OS state per iteration). Change-only (an
    /// identical re-publish never re-renders — per-pump repeats stay
    /// free).
    pub fn set_maximized(&self, maximized: bool) {
        let sig = self.maximized();
        if untrack(|| sig.get()) != maximized {
            sig.set(maximized);
        }
    }

    /// Queues one scene-toolbar tap (router-side — overlay items
    /// resolve geometrically on the loop thread; the runner drains
    /// per pump and applies through the loop clipboard).
    pub fn push_toolbar_action(&self, action: crate::input::ToolbarAction) {
        self.inner.toolbar_actions.borrow_mut().push_back(action);
    }

    /// Drains queued scene-toolbar taps in arrival order (runner-side —
    /// one drain per pump; empty drains are empty, never an error).
    pub fn drain_toolbar_actions(&self) -> Vec<crate::input::ToolbarAction> {
        self.inner.toolbar_actions.borrow_mut().drain(..).collect()
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

    /// One-shot transition-completion observer (decision 512): `f`
    /// fires from the TIME phase once `node` carries no live track —
    /// a settled Once tween/keyframe, a cancelled or stamped track,
    /// or an unmounted (pruned) one. Loops pend until cancelled
    /// (they never settle — observing one only pays off after a
    /// cancel/stamp). Already-settled nodes fire on the next pump
    /// (uniform async contract — never synchronous). One-shot:
    /// consumed on fire, so chains re-observe per link. Callbacks run
    /// in TIME-phase runtime context — setting a signal there chains
    /// the next step through the normal pump. Observation starts at
    /// registration (retirements before that are history — read
    /// `is_settled` through [`ComponentHost::with_evaluator`] first
    /// when history matters). Untracked.
    pub fn observe_settled(&self, node: NodeId, f: impl FnOnce() + 'static) {
        self.inner
            .settle_observers
            .borrow_mut()
            .push((node, Box::new(f)));
        self.inner.rt.request_frame();
        self.ensure_settle_drain();
    }

    pub fn run_until_idle(&self) -> usize {
        self.fire_due_longpresses();
        self.inner.rt.run_until_idle()
    }

    /// Pumps at most `max_frames` frames, stopping early at
    /// quiescence — returns frames run (decision 515). Runners use
    /// this wherever loop tracks may live: bare `run_until_idle`
    /// spins forever under a loop (loops never retire), freezing
    /// the UI thread — a cap keeps event pumps responsive while
    /// Once-only worlds quiesce early, identically to today.
    /// Frozen-clock tests keep `run_once` stepping (a cap cannot
    /// retire time-bound tracks without flowing time).
    pub fn pump_frames(&self, max_frames: usize) -> usize {
        for done in 0..max_frames {
            if !self.run_once() {
                return done;
            }
        }
        max_frames
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

    /// Whether renderer-drawn selection handles publish (decision
    /// 474): `true` by default; desktop Windows/Linux runners set
    /// `false` at startup (mouse selections never grow touch
    /// handles), Android keeps `true`. Untracked (publication reads
    /// it without scheduling; flips apply on the next publish).
    pub fn selection_handles_visible(&self) -> bool {
        self.inner.selection_handles_visible.get()
    }

    /// Hides or restores renderer-drawn selection handles
    /// (runner-side — see
    /// [`selection_handles_visible`](Self::selection_handles_visible)).
    pub fn set_selection_handles_visible(&self, visible: bool) {
        self.inner.selection_handles_visible.set(visible);
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
