use super::*;

impl ComponentHost {
    /// Test hook for the framework scroll seam: the instance's
    /// `scroll_offset` handle (the same signal TIME-physics / INPUT-scroll
    /// feeds write through on real backends).
    pub fn instance_scroll(&self, instance: u64) -> Option<ScrollOffset> {
        self.inner
            .instances
            .borrow()
            .get(&instance)
            .and_then(|rec| rec.scroll.clone().map(|signal| ScrollOffset { signal }))
    }

    /// Test hook for the horizontal scroll seam (Round 9.3): the
    /// instance's `scroll_x` handle — the `instance_scroll` twin.
    pub fn instance_scroll_x(&self, instance: u64) -> Option<ScrollOffset> {
        self.inner
            .instances
            .borrow()
            .get(&instance)
            .and_then(|rec| rec.scroll_x.clone().map(|signal| ScrollOffset { signal }))
    }

    /// Test hook for the 2D scroll seam (Phase 36 PR2b): the
    /// instance's combined handle — `Some` exactly when the instance
    /// created both halves (same signals the 1D twins return).
    pub fn instance_scroll_2d(&self, instance: u64) -> Option<ScrollOffset2D> {
        let inner = self.inner.instances.borrow();
        let rec = inner.get(&instance)?;
        Some(ScrollOffset2D {
            x: rec.scroll_x.clone()?,
            y: rec.scroll.clone()?,
        })
    }

    /// Binds a scroll-target node to a framework-owned offset signal
    /// (§9.3 INPUT feed, M7, decision 112): scroll events routed to
    /// `target` accumulate their `dy` into this signal at the INPUT
    /// phase boundary, winning over the Round 24.2 owner self-wire.
    /// Re-binding replaces the feed. This is the headless feed of the
    /// mapping the DOM shell performs from browser scroll events (Web)
    /// and TIME physics performs (GPU) — one signal, identical
    /// semantics, mechanism-only difference.
    pub fn bind_scroll(&self, target: NodeId, offset: ScrollOffset) {
        self.inner
            .scroll_feeds
            .borrow_mut()
            .insert(target, offset.signal);
    }

    /// Binds a scroll-target node to a framework-owned horizontal
    /// offset signal (Round 9.3, decision 302): the `bind_scroll`
    /// twin — scroll events routed to `target` accumulate their `dx`
    /// into the signal, clamped to `[0, content_w - w]` of the
    /// target's committed box (no horizontal windowing helper exists
    /// yet, so an unclamped offset could scroll into void with no
    /// app-side recovery — the vertical feed keeps its unclamped
    /// app-windowed contract, M7/M8 proofs depend on it, stated).
    /// Re-binding replaces the feed; unbound targets ignore `dx`.
    pub fn bind_scroll_x(&self, target: NodeId, offset: ScrollOffset) {
        self.inner
            .scroll_x_feeds
            .borrow_mut()
            .insert(target, offset.signal);
    }

    /// Binds a scroll-target node to a framework-owned 2D offset
    /// (Phase 36 PR2b, decision 354 — G15): `dy` accumulates into the
    /// `y` half (the `bind_scroll` contract), `dx` into the `x` half
    /// (the `bind_scroll_x` contract, clamped to the target's
    /// `content_w` overflow). Re-binding replaces both feeds — one
    /// call for 2D `ScrollArea` containers instead of two 1D binds.
    pub fn bind_scroll_2d(&self, target: NodeId, offset: &ScrollOffset2D) {
        self.inner
            .scroll_feeds
            .borrow_mut()
            .insert(target, offset.y.clone());
        self.inner
            .scroll_x_feeds
            .borrow_mut()
            .insert(target, offset.x.clone());
    }

    /// The offset signal bound to `target`, if any (diagnostics/tests).
    pub fn bound_scroll(&self, target: NodeId) -> Option<f32> {
        self.inner
            .scroll_feeds
            .borrow()
            .get(&target)
            .cloned()
            .map(|sig| untrack(|| sig.get()))
    }

    /// The horizontal offset signal bound to `target`, if any
    /// (Round 9.3 — diagnostics/tests twin of [`ComponentHost::
    /// bound_scroll`]).
    pub fn bound_scroll_x(&self, target: NodeId) -> Option<f32> {
        self.inner
            .scroll_x_feeds
            .borrow()
            .get(&target)
            .cloned()
            .map(|sig| untrack(|| sig.get()))
    }

    /// Both offsets bound to `target`, if any (Phase 36 PR2b —
    /// diagnostics/tests twin of the 1D pair; `None` unless both
    /// halves are bound — a half-bound target is a wiring bug, never
    /// a silent half-read).
    pub fn bound_scroll_2d(&self, target: NodeId) -> Option<ScrollXY> {
        Some(ScrollXY {
            x: self.bound_scroll_x(target)?,
            y: self.bound_scroll(target)?,
        })
    }

    /// Binds a field node to the app-owned value signal
    /// (U8, decision 188): `InputEvent::Text` routed to `target`
    /// sets the full current value at the INPUT phase boundary.
    /// Re-binding replaces the feed. The signal is app-owned
    /// (created by the component, passed to the app layer) so the
    /// component renders the observed value by reading it.
    pub fn bind_text(&self, target: NodeId, value: Signal<SharedString>) {
        self.inner.field_feeds.borrow_mut().insert(target, value);
    }

    /// The value signal bound to `target`, read untracked
    /// (diagnostics/tests).
    pub fn bound_text(&self, target: NodeId) -> Option<SharedString> {
        self.inner
            .field_feeds
            .borrow()
            .get(&target)
            .cloned()
            .map(|sig| untrack(|| sig.get()))
    }

    /// Gets-or-creates the calling component instance's editing session
    /// over `content` (G1, decision 205): the `Ctx::edit_session` back-end.
    /// Keyed by call-site source-hash + ordinal exactly like
    /// `instance_signal` (a body edit inserting a session above shifts
    /// later sites → re-seed, never shuffle, §5.1); the first run's
    /// content signal wins (same init rule as `ctx.signal`).
    pub(super) fn instance_edit_session(
        &self,
        instance: u64,
        key: SiteKey,
        content: Signal<SharedString>,
    ) -> EditSession {
        let mut instances = self.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&instance)
            .expect("edit session on dead instance");
        if let Some(sess) = rec.edit_sessions.get(&key) {
            return sess.clone();
        }
        let sess = EditSession::new(self.inner.rt.clone(), content);
        rec.edit_sessions.insert(key, sess.clone());
        sess
    }

    /// Live editing sessions of one component instance, oldest-call-site
    /// first is NOT guaranteed (map order) — diagnostics/tests/shells.
    /// Untracked (never schedules).
    pub fn edit_sessions_for(&self, instance: u64) -> Vec<EditSession> {
        self.inner
            .instances
            .borrow()
            .get(&instance)
            .map(|rec| rec.edit_sessions.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Binds a field node to a session's content signal (G1, decision
    /// 208): the routed `InputEvent::Text` stream then feeds the session
    /// through the U8 `bind_text` path (decision 188). Re-binding
    /// replaces the feed.
    pub fn bind_edit_session(&self, target: NodeId, session: &EditSession) {
        self.bind_text(target, session.content_signal());
    }

    /// U8 fallback feed (decision 293): routes a full-value text event
    /// to the content signal of the target's owning instance session —
    /// the same signal [`bind_edit_session`](Self::bind_edit_session)
    /// would bind, so observable semantics match the explicit path
    /// exactly (full replace, no caret/undo bookkeeping — decision
    /// 188's feed shape, not the session's insert path).
    ///
    /// Guards (each a quiet no-op, the arm's doctrine): the target
    /// must lie in a text field; the press owner must lie inside that
    /// same field (disabled shells carry no handler so they resolve
    /// outward-or-None and never capture; foreign ancestors likewise);
    /// the owner instance must hold exactly one session (zero —
    /// hand-built fields with no session — and multi-session
    /// ambiguity both miss; the strict runner path
    /// ([`focused_field_session`](Self::focused_field_session)) keeps
    /// panicking on ambiguity instead).
    pub(super) fn feed_text_to_owner_session(&self, target: NodeId, value: &str) {
        let owner = {
            let rec = self.inner.rec.borrow();
            let owner = match input::press_owner_node(&rec, target) {
                Some(owner) => owner,
                None => return,
            };
            // Fields containing the target, deepest first (the walk
            // climbs from the target, so the first field that also
            // contains the owner wins — leaf and outer resolve to the
            // same owner, nested fields resolve deterministically).
            let mut field = None;
            let mut cur = Some(target);
            while let Some(c) = cur {
                let Some(n) = rec.get(c) else { break };
                if n.semantics.as_ref().is_some_and(|s| {
                    s.role == crate::semantics::Role::TextField
                        || s.role == crate::semantics::Role::TextArea
                }) && input::is_within(&rec, owner, c)
                {
                    field = Some(c);
                    break;
                }
                cur = n.parent;
            }
            if field.is_none() {
                return;
            }
            owner
        };
        let hid = {
            let rec = self.inner.rec.borrow();
            match input::press_handler_of(&rec, owner) {
                Some(hid) => hid,
                None => return,
            }
        };
        let inst = match self.inner.rt.handler_owner(hid) {
            Some(inst) => inst,
            None => return,
        };
        // Exactly one session feeds; zero or multi miss quietly
        // (documented above — the arm's doctrine, not an assert).
        // The platform commit reports through `on_change` like a
        // native insert (Phase 39a, decision 377 — round-5.4 parity),
        // collapses the caret to the end (the feed carries none),
        // and resets the blink phase like every session op (Round
        // 15.1, decision 312).
        if let [sess] = self.edit_sessions_for(inst).as_slice() {
            sess.apply_platform_value(value);
        }
    }

    /// Focused text field's editing session, if any (runner typing
    /// path, decision 243): focused node → press owner → owning
    /// instance → that instance's session. `None` when nothing is
    /// focused, focus is not inside a text field, or the instance
    /// holds no session (quiet miss — router precedent for unhandled
    /// keys). Panics loudly on multiple sessions (an ambiguous typing
    /// target is never guessed — map order is not call-site order).
    /// Untracked (never schedules).
    pub fn focused_field_session(&self) -> Option<EditSession> {
        let focus = self.focused_node()?;
        let fields = self.text_fields();
        let in_field = fields.iter().any(|f| {
            let rec = self.inner.rec.borrow();
            input::is_within(&rec, *f, focus) || input::is_within(&rec, focus, *f)
        });
        if !in_field {
            return None;
        }
        let rec = self.inner.rec.borrow();
        let owner = input::press_owner_node(&rec, focus)?;
        let hid = input::press_handler_of(&rec, owner)?;
        let inst = self.inner.rt.handler_owner(hid)?;
        drop(rec);
        match self.edit_sessions_for(inst).as_slice() {
            [] => None,
            [s] => Some(s.clone()),
            many => panic!(
                "focused text input owns {} edit sessions — ambiguous typing target, refused, never guessed",
                many.len()
            ),
        }
    }

    /// Candidate-window anchor for the focused field in client (device)
    /// px (Round 2.1, decision 256): the focused session's caret rect
    /// resolved against the field payload leaf's laid lines — the point
    /// shells hand to the OS candidate window (`ImmSetCandidateWindow`
    /// / winit `set_ime_cursor_area`) after every IME step, so the
    /// candidate follows the composition caret dynamically.
    ///
    /// Shaper policy: the anchor installs the host text service on the
    /// session on demand, sized to the laid leaf's exact `em_size`
    /// (config default while the leaf lays no lines — composing into
    /// an empty field). The install persists, so pointer-mapped
    /// session geometry works in app fields too; shaper-less hosts
    /// keep the decision-207 graceful no-ops. `None` when nothing is
    /// focused, focus is outside a field, the host has no service, the
    /// composite is empty, or no text box exists yet (quiet miss —
    /// anchoring is advisory, never a wiring bug). Untracked.
    pub fn focused_ime_anchor(&self) -> Option<[f32; 4]> {
        let session = self.focused_field_session()?;
        let focus = self.focused_node()?;
        let rec = self.inner.rec.borrow();
        // First laid text leaf under focus (the field payload — the
        // `TextField` conversion shape carries the value as its text
        // child, decision 113), plus the first committed text box as
        // the empty-leaf fallback.
        let mut stack = vec![focus];
        let mut lined: Option<LayoutBox> = None;
        let mut boxed: Option<LayoutBox> = None;
        while let Some(id) = stack.pop() {
            let Some(n) = rec.get(id) else {
                continue;
            };
            if n.tag == crate::vnode::Tag::Text {
                if let Some(b) = n.layout.clone() {
                    if boxed.is_none() {
                        boxed = Some(b.clone());
                    }
                    if !b.lines.is_empty() {
                        lined = Some(b);
                        break;
                    }
                }
            }
            for child in n.children.iter().rev() {
                stack.push(*child);
            }
        }
        drop(rec);
        let cfg = self.inner.layout.borrow().config().clone();
        let service = self.inner.text_service.borrow().clone()?;
        let (size_px, dpr) = match lined.as_ref() {
            Some(b) => (
                b.lines[0].em_size / cfg.device_pixel_ratio.max(f32::EPSILON),
                cfg.device_pixel_ratio,
            ),
            None => (cfg.default_px, cfg.device_pixel_ratio),
        };
        let mut style = TextStyle::new(&cfg.family, size_px);
        style.device_pixel_ratio = dpr;
        session.set_shaper(service, style);
        let rect = session.caret_rect()?;
        match lined {
            Some(b) => {
                // The session caret's visual line (forward affinity —
                // future multi-line fields resolve here too); geometry
                // rides the session's own shaper rect, whose
                // baseline-relative y the line baseline converts to
                // box space.
                let (li, _) = b.caret_position(session.caret());
                let line = b.lines.get(li)?;
                Some([
                    b.x + rect.x,
                    b.y + line.y + line.baseline + rect.y,
                    rect.width,
                    rect.height,
                ])
            }
            // No laid lines (empty field): caret x from the shaper at
            // the text origin; y at the would-be line top.
            None => {
                let b = boxed?;
                Some([b.x + rect.x, b.y, rect.width, rect.height])
            }
        }
    }

    /// Commits every active composition in every live session (G1,
    /// decision 208 — locked #27 commit-on-focus-loss). Called
    /// automatically on real focus changes (see `set_focus_node`;
    /// additive no-op when no session is composing) and directly by
    /// shells that manage focus externally (e.g. DOM blur).
    pub fn notify_edit_focus_lost(&self) {
        let instances = self.inner.instances.borrow();
        for rec in instances.values() {
            for sess in rec.edit_sessions.values() {
                sess.notify_focus_lost();
            }
        }
    }

    /// Cancels every active composition in every live session (round
    /// 3.3, OQ-G11-2 — the back-press path: back reverts the
    /// composition where focus loss would commit it). Returns true
    /// when at least one session was composing (the caller maps
    /// that to `BackOutcome::CompositionCancelled`). No-op when
    /// nothing composes.
    pub fn cancel_edit_compositions(&self) -> bool {
        use crate::ime::{dispatch_ime_event, ImeCompositionEvent};
        let mut cancelled = false;
        let instances = self.inner.instances.borrow();
        for rec in instances.values() {
            for sess in rec.edit_sessions.values() {
                if sess.is_composing() {
                    let mut sess = sess.clone();
                    dispatch_ime_event(&mut sess, &ImeCompositionEvent::CompositionCancelled);
                    cancelled = true;
                }
            }
        }
        cancelled
    }
}
