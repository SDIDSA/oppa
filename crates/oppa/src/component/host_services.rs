use super::*;

impl ComponentHost {
    /// Installs the `TextService` the engine measures through (shared
    /// by handle — sessions borrow it on demand for pointer-mapped
    /// geometry, see `focused_ime_anchor`).
    pub fn set_text_service(&self, service: Box<dyn TextService>) {
        *self.inner.text_service.borrow_mut() = Some(Rc::from(service));
    }

    /// Installs the `BreakSource` the engine wraps through (v2 item 2).
    /// The source is shared (`Rc`): the host table keeps one clone and
    /// the layout engine keeps another.
    pub fn set_break_source(&self, source: Rc<dyn BreakSource>) {
        *self.inner.break_source.borrow_mut() = Some(source.clone());
        self.inner
            .layout
            .borrow_mut()
            .set_break_source(Some(source));
    }

    /// Root constraints in CSS px (scaled by the text config's DPR).
    /// A changed viewport invalidates the root for layout (decision
    /// 248): positions derive from the viewport, so a bare Cell write
    /// leaves every committed box stale and `run_until_idle` has no
    /// demand to run the Layout phase at all (decision-69 gating
    /// would skip `run_layout` even inside a frame). Same-value
    /// writes are a no-op (hot resize polls must not spin frames).
    /// No root yet (pre-mount sizing) writes the Cell only — the
    /// fresh mount dirties STRUCTURE|LAYOUT|PAINT anyway.
    pub fn set_viewport(&self, w_css: f32, h_css: f32) {
        if self.inner.viewport.get() == (w_css, h_css) {
            return;
        }
        self.inner.viewport.set((w_css, h_css));
        // Scoped read (the borrow guard must drop before the mutable
        // mark below — holding it across would panic loudly).
        let root = self.inner.rec.borrow().root();
        if let Some(root) = root {
            self.inner.rec.borrow_mut().mark_layout_dirty(&[root]);
            self.inner.rt.request_frame();
        }
    }

    /// Current viewport size in CSS px (Round 21.3, decision 330 —
    /// the popup-clamp read: menus, tooltips, and scrollbars clamp
    /// anchored coordinates against this, never hardcoded sizes).
    /// Untracked (never schedules).
    pub fn viewport_size(&self) -> (f32, f32) {
        self.inner.viewport.get()
    }

    /// Replaces the engine's text config (family/sizes/DPR/ellipsis).
    /// A changed value dirties LAYOUT on a live root like
    /// `set_viewport` does (a DPR flip with identical CSS sizes must
    /// still reflow — Round 2.4, decision 259; the measure-key miss
    /// alone cannot move text-less boxes, and same-value viewports
    /// no-op past it). Same-value writes are a no-op (config
    /// rewrites must never spin frames).
    pub fn set_layout_config(&self, config: LayoutTextConfig) {
        if *self.inner.layout.borrow().config() == config {
            return;
        }
        self.inner.layout.borrow_mut().set_config(config);
        let root = self.inner.rec.borrow().root();
        if let Some(root) = root {
            self.inner.rec.borrow_mut().mark_layout_dirty(&[root]);
            self.inner.rt.request_frame();
        }
    }

    /// The engine's current text config (read-modify-write cycles —
    /// e.g. a DPR flip that must preserve family/sizes — read here,
    /// never reconstructed from defaults).
    pub fn layout_config(&self) -> LayoutTextConfig {
        self.inner.layout.borrow().config().clone()
    }

    /// App theme handle (Round 11.2, decision 306): the host-level
    /// mode signal as a [`Theme`] (Light default, lazy). Components
    /// read it through [`Ctx::theme`].
    pub fn theme(&self) -> Theme {
        let mut slot = self.inner.theme.borrow_mut();
        let sig = match slot.clone() {
            Some(sig) => sig,
            None => {
                let sig = self.inner.rt.signal(ThemeMode::Light);
                *slot = Some(sig.clone());
                sig
            }
        };
        drop(slot);
        // Decision 477: the override table rides alongside the mode
        // signal (created together so every Theme handle — whenever
        // constructed — reads the live table, never a stale copy).
        let mut oslot = self.inner.theme_token_overrides.borrow_mut();
        let osig = match oslot.clone() {
            Some(osig) => osig,
            None => {
                let osig = self.inner.rt.signal([None, None]);
                *oslot = Some(osig.clone());
                osig
            }
        };
        Theme {
            signal: sig,
            overrides: osig,
        }
    }

    /// Publishes a custom palette for one mode (decision 477):
    /// subsequent `tokens()` reads for that mode return it instead
    /// of the built-in palette (runners brand apps, tests pin
    /// palettes; `None` clears back to built-in). Tracked (the
    /// override signal notifies, so every themed body re-renders in
    /// place like a mode toggle — instances and state survive).
    pub fn set_theme_tokens_override(&self, mode: ThemeMode, tokens: Option<ThemeTokens>) {
        let osig = self
            .inner
            .theme_token_overrides
            .borrow()
            .clone()
            .unwrap_or_else(|| self.theme().overrides.clone());
        let mut arr = crate::reactive::untrack(|| osig.get());
        arr[super::override_idx(mode)] = tokens;
        osig.set(arr);
    }

    /// Current palette honoring overrides (tracked — same reads as
    /// [`Theme::tokens`], for host-side code that already holds the
    /// host instead of a [`Theme`]).
    pub fn theme_tokens(&self) -> ThemeTokens {
        self.theme().tokens()
    }

    /// Palette for `mode` honoring overrides (untracked — diagnostics
    /// and paint-time queries that must not schedule).
    pub fn theme_tokens_for(&self, mode: ThemeMode) -> ThemeTokens {
        let arr = self
            .inner
            .theme_token_overrides
            .borrow()
            .clone()
            .map(|osig| crate::reactive::untrack(|| osig.get()));
        arr.and_then(|a| a[super::override_idx(mode)])
            .unwrap_or(ThemeTokens::of(mode))
    }

    /// Switches the app theme (Round 11.2 — tests, settings screens,
    /// platform dark-mode listeners; every themed body re-renders,
    /// no instance re-created).
    pub fn set_theme(&self, mode: ThemeMode) {
        self.theme().set(mode);
    }

    /// App-wide image cache (Phase 36 PR4 / Phase 38c, decision 359):
    /// shared image registry holding loaded URLs and pre-decoded static pixels.
    pub fn images(&self) -> ImageCache {
        self.inner.images.borrow().clone()
    }

    /// Sets or replaces the host-level image cache.
    pub fn set_images(&self, images: ImageCache) {
        *self.inner.images.borrow_mut() = images;
    }

    /// Pushes one diagnostic entry (Phase 37b, decision 363 — G17):
    /// the zero-stdout ring (never prints — hosts drain for
    /// assertions and platform sinks). Untracked (logging never
    /// schedules — diagnostics observe, never drive).
    pub fn diag_log(&self, level: crate::diag::LogLevel, message: impl Into<String>) {
        self.inner.diag.borrow_mut().push(level, message);
    }

    /// Live diagnostic count (never exceeds capacity).
    pub fn diag_len(&self) -> usize {
        self.inner.diag.borrow().len()
    }

    /// Overwritten diagnostic count (exact).
    pub fn diag_dropped(&self) -> u64 {
        self.inner.diag.borrow().dropped()
    }

    /// Takes diagnostics at or above `floor`, leaving the log
    /// otherwise intact (below-floor entries keep their sequence
    /// numbers — filters never destroy, takes never rewind).
    pub fn take_diag_logs(&self, floor: crate::diag::LogLevel) -> Vec<crate::diag::LogEntry> {
        self.inner.diag.borrow_mut().take_at_or_above(floor)
    }

    /// Mobile / app lifecycle state signal (Round 18.3, decision 322):
    /// Active default, lazy. Components read this via `ctx.lifecycle()`.
    pub fn lifecycle(&self) -> Signal<crate::shell::AppLifecycleState> {
        let mut slot = self.inner.lifecycle.borrow_mut();
        if let Some(sig) = slot.clone() {
            return sig;
        }
        let sig = self
            .inner
            .rt
            .signal(crate::shell::AppLifecycleState::Active);
        *slot = Some(sig.clone());
        sig
    }

    /// Updates the app lifecycle state (Round 18.3 — pause/resume/save_state).
    /// Pausing or suspending halts tickers and throttles frames.
    pub fn set_lifecycle(&self, state: crate::shell::AppLifecycleState) {
        let sig = self.lifecycle();
        if untrack(|| sig.get()) != state {
            sig.set(state);
        }
    }

    /// Whether app tickers and frame loops should be suspended due to lifecycle state.
    pub fn is_lifecycle_suspended(&self) -> bool {
        matches!(
            untrack(|| self.lifecycle().get()),
            crate::shell::AppLifecycleState::Paused | crate::shell::AppLifecycleState::Suspended
        )
    }

    /// Keyboard-focus modality signal (Round 23.2, decision 334):
    /// true while the current focus arrived via keyboard (Tab /
    /// Shift+Tab set it, pointer focus clears it). Controls paint
    /// their themed focus ring only while set AND focused.
    pub fn focus_visible(&self) -> Signal<bool> {
        let mut slot = self.inner.focus_visible.borrow_mut();
        if let Some(sig) = slot.clone() {
            return sig;
        }
        let sig = self.inner.rt.signal(false);
        *slot = Some(sig.clone());
        sig
    }

    /// Updates the modality, notifying only on change (a redundant
    /// write must not invalidate every ring reader).
    pub(crate) fn set_focus_visible(&self, visible: bool) {
        let sig = self.focus_visible();
        if untrack(|| sig.get()) != visible {
            sig.set(visible);
        }
    }

    /// Last routed position of a live pointer, in device px (round
    /// 5.3, OQ-G2-1 — the drag query: `on_drag` handlers read this
    /// for the primary capture alongside the owner's committed
    /// box). Set on Down/Move, cleared on Up/Cancel/unmount (a
    /// released pointer has no position — `None`, never stale).
    /// Untracked (never schedules).
    pub fn pointer_position(&self, id: u32) -> Option<(f32, f32)> {
        self.inner.input.borrow().positions.get(&id).copied()
    }

    /// Position of the lowest-id live capture, in device px (round
    /// 5.3 — the legacy single-capture read extended with position:
    /// single-pointer flows (slider drags) skip id plumbing; the
    /// multi-pointer exact read is [`ComponentHost::pointer_position`]).
    /// `None` with no live capture. Untracked (never schedules).
    pub fn capture_position(&self) -> Option<(f32, f32)> {
        let input = self.inner.input.borrow();
        let id = *input.captures.keys().min()?;
        input.positions.get(&id).copied()
    }

    /// Last tap-Up position that dispatched a `Press`, device px (Round
    /// 8.1, decision 297 — tap-to-caret: handlers that need the click
    /// point read this; live captures are already gone at Up time).
    /// `None` after keyboard activation or before any tap. Untracked.
    pub fn last_press_position(&self) -> Option<(f32, f32)> {
        self.inner.input.borrow().last_press_pos
    }

    /// Release point of the last plain drag, device px (Round 21.3,
    /// decision 330 — the drag-select query: `on_drag_release`
    /// handlers hit-test through this; `None` before any
    /// drag-release. Untracked (never schedules).
    pub fn last_drag_release(&self) -> Option<(f32, f32)> {
        self.inner.input.borrow().drag_release_pos
    }

    /// Modifiers sampled at that tap Up (Round 8.1 — Shift+Click).
    /// Untracked.
    pub fn last_press_modifiers(&self) -> input::Modifiers {
        self.inner.input.borrow().last_press_modifiers
    }

    /// Click count of the tap that dispatched the last `Press` (Round
    /// 8.2, decision 298 — 1 = caret tap, 2 = double-click word, 3+ =
    /// triple-click line; 0 before any tap or after keyboard
    /// activation). Untracked.
    pub fn last_press_click_count(&self) -> u32 {
        self.inner.input.borrow().last_press_count
    }

    /// Down point of one live pointer (device px — the drag anchor for
    /// `on_drag` selection streams; `None` = id not down). Untracked.
    pub fn press_origin(&self, id: u32) -> Option<(f32, f32)> {
        self.inner.longpress.borrow().get(&id).map(|a| (a.x, a.y))
    }

    /// Down point of the lowest-id live pointer (Round 8.2 — the
    /// single-pointer drag anchor, mirroring [`ComponentHost::
    /// capture_position`]). `None` with no live capture. Untracked.
    pub fn lowest_press_origin(&self) -> Option<(f32, f32)> {
        let id = *self.inner.input.borrow().captures.keys().min()?;
        self.press_origin(id)
    }

    /// Marks nodes PAINT-dirty (Round 8.2 — session selection writes no
    /// signals, so field handlers dirty their owner explicitly after
    /// pointer selection ops; the TIME drive's per-frame rule,
    /// decision 125, without a standing animation). Returns the dirty
    /// count. Untracked.
    pub fn mark_paint_dirty(&self, nodes: &[NodeId]) -> usize {
        self.inner.rec.borrow_mut().mark_paint_dirty(nodes)
    }

    /// The focused field's painted selection, if any (Round 8.2 —
    /// builders highlight laid lines under the focus owner overlapping
    /// the session range; collapsed selections paint nothing). The
    /// range is display-space (decision 458 — masked fields lay
    /// bullets, so content bytes map on before painting). `None`
    /// when nothing is focused, focus is outside a session-owning
    /// field, or the selection is collapsed. Untracked.
    pub fn focused_selection_paint(&self) -> Option<crate::render::SelectionPaint> {
        let focus = self.focused_node()?;
        let session = self.focused_field_session()?;
        let sel = session.selection();
        if sel.0 == sel.1 {
            return None;
        }
        let (a, b) = (sel.0.min(sel.1), sel.0.max(sel.1));
        Some(crate::render::SelectionPaint {
            field: focus,
            range: (
                session.composite_byte_to_display(a),
                session.composite_byte_to_display(b),
            ),
        })
    }

    /// The focused field's painted caret bar, if any (Round 15.1,
    /// decision 312 — the bar collapsed selections refused to paint
    /// in 8.2 now renders: when the session is focused, its selection
    /// is collapsed, and the blink phase is visible, the caret
    /// resolves to the active cluster's leading edge through the same
    /// shaper + laid-line rule as [`focused_ime_anchor`](Self::focused_ime_anchor)).
    /// `None` when nothing is focused, focus is outside a
    /// session-owning field, a range is selected (the highlight paints
    /// instead — one overlay at a time), the blink half-cycle is
    /// hidden, or no text box exists yet (quiet miss — advisory
    /// geometry, never a wiring bug). Untracked.
    pub fn focused_caret_paint(&self) -> Option<crate::render::CaretPaint> {
        let focus = self.focused_node()?;
        let session = self.focused_field_session()?;
        let sel = session.selection();
        if sel.0 != sel.1 {
            return None;
        }
        if !session.caret_visible() {
            return None;
        }
        let composite = session.composite_text();
        let rec = self.inner.rec.borrow();
        // First laid text leaf under focus (the field payload) plus
        // the first committed text box as the empty-leaf fallback —
        // the same walk as `focused_ime_anchor` (kept adjacent, not
        // shared, so the IME anchor's behavior stays byte-identical).
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
        let (x, y, h) = match lined {
            Some(b) => {
                let (x, line_y, line_h, line_baseline, em_size) = if composite.is_empty() {
                    let line = b.lines.first();
                    let (lh, base, em) = line
                        .map(|l| (l.height, l.baseline, l.em_size))
                        .unwrap_or((b.h, b.h * 0.8, 0.0));
                    if lh <= 0.0 {
                        return None;
                    }
                    (b.x, line.map(|l| l.y).unwrap_or(0.0), lh, base, em)
                } else {
                    let caret = session.composite_byte_to_display(session.caret());
                    let (li, _) = b.caret_position(caret);
                    let line = b.lines.get(li)?;
                    let cfg = self.inner.layout.borrow().config().clone();
                    let service = self.inner.text_service.borrow().clone()?;
                    let (size_px, dpr) = (
                        line.em_size / cfg.device_pixel_ratio.max(f32::EPSILON),
                        cfg.device_pixel_ratio,
                    );
                    let mut style = TextStyle::new(&cfg.family, size_px);
                    style.device_pixel_ratio = dpr;
                    session.set_shaper(service, style);
                    let rect = session.caret_rect()?;
                    if rect.height <= 0.0 {
                        return None;
                    }
                    (
                        b.x + rect.x,
                        line.y,
                        rect.height,
                        line.baseline,
                        line.em_size,
                    )
                };
                let em = if em_size > 0.0 {
                    em_size
                } else {
                    line_h * 0.75
                };
                let caret_h = (em * 1.10).min(line_h);
                let y_offset = (line_baseline - em * 0.85).max(0.0).min(line_h - caret_h);
                (x, b.y + line_y + y_offset, caret_h)
            }
            None => {
                let b = boxed?;
                let h = b.lines.first().map(|l| l.height).unwrap_or(b.h);
                if h <= 0.0 {
                    return None;
                }
                let caret_h = h * 0.82;
                (b.x, b.y + (h - caret_h) * 0.5, caret_h)
            }
        };
        drop(rec);
        Some(crate::render::CaretPaint {
            field: focus,
            x,
            y,
            h,
            color: self.caret_ink(focus),
        })
    }

    /// Effective caret ink for a field (Round 15.1, decision 312 +
    /// the theme contract round): the field's own `Style::ink`, else
    /// the nearest ancestor's, else the host theme's `text_primary`
    /// (Light's is the contract [`INK`](crate::render::INK)) — the
    /// same inherit rule the plan builder resolves for text, so the
    /// caret bar paints in the field's text color on every presenter.
    /// Untracked.
    fn caret_ink(&self, field: NodeId) -> crate::style::Color {
        let rec = self.inner.rec.borrow();
        let styles = self.inner.styles.borrow();
        let mut cur = Some(field);
        while let Some(c) = cur {
            if let Some(ink) = rec
                .get(c)
                .and_then(|n| styles.get(n.style))
                .and_then(|s| s.ink)
            {
                return ink;
            }
            cur = rec.get(c).and_then(|n| n.parent);
        }
        self.theme_tokens_for(self.theme().mode()).text_primary
    }

    /// The focused field's renderer-drawn selection handles, if any
    /// (decision 454): one [`HandlePaint`](crate::render::HandlePaint)
    /// per selection edge, resolved through laid clusters (the
    /// `caret_position` rule — wrap affinity matches the highlight
    /// exactly, and no shaper install means handles paint headless).
    /// `None` when nothing is focused, focus is outside a
    /// session-owning field, the selection is collapsed (the caret
    /// bar owns that state), no text box is laid yet (quiet miss —
    /// advisory geometry like the caret, never a wiring bug), or the
    /// host hides them (decision 474 — desktop runners opt out;
    /// `selection_handles_visible`). Untracked.
    pub fn focused_handles_paint(
        &self,
    ) -> Option<(crate::render::HandlePaint, crate::render::HandlePaint)> {
        if !self.inner.selection_handles_visible.get() {
            return None;
        }
        let focus = self.focused_node()?;
        let session = self.focused_field_session()?;
        let sel = session.selection();
        let (lo, hi) = (sel.0.min(sel.1), sel.0.max(sel.1));
        if lo == hi {
            return None;
        }
        // First laid text leaf under focus (the caret walk minus the
        // shaper: edges resolve from committed clusters, so the
        // geometry survives without a text service).
        let rec = self.inner.rec.borrow();
        let mut stack = vec![focus];
        let mut lined: Option<LayoutBox> = None;
        while let Some(id) = stack.pop() {
            let Some(n) = rec.get(id) else {
                continue;
            };
            if n.tag == crate::vnode::Tag::Text {
                if let Some(b) = n.layout.clone() {
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
        let b = lined?;
        // Display-space edges (decision 458 — masked fields lay
        // bullets, so content bytes map on before touching clusters).
        let (li_lo, x_lo) = b.caret_position(session.composite_byte_to_display(lo));
        let (li_hi, x_hi) = b.caret_position(session.composite_byte_to_display(hi));
        let (line_lo, line_hi) = (b.lines.get(li_lo)?, b.lines.get(li_hi)?);
        drop(rec);
        let color = self.caret_ink(focus);
        Some((
            crate::render::HandlePaint {
                field: focus,
                edge: crate::input::HandleEdge::Start,
                x: b.x + x_lo,
                y: b.y + line_lo.y,
                h: line_lo.height,
                color,
            },
            crate::render::HandlePaint {
                field: focus,
                edge: crate::input::HandleEdge::End,
                x: b.x + x_hi,
                y: b.y + line_hi.y,
                h: line_hi.height,
                color,
            },
        ))
    }

    /// The live handle edge for the drag stream (decision 454): the
    /// confirmed edge of the lowest-id live pointer (the `field_drag`
    /// anchor precedent — one drag stream, one edge). `None` with no
    /// confirmed handle grab — candidates inside slop and plain
    /// presses stream the `drag_x` selection.
    /// Untracked.
    pub fn handle_drag_edge(&self) -> Option<crate::input::HandleEdge> {
        let input = self.inner.input.borrow();
        let id = *input.captures.keys().min()?;
        if !input.handle_confirmed.contains(&id) {
            return None;
        }
        input.handle_drags.get(&id).copied()
    }

    /// The candidate handle edge for the drag stream (decision 454):
    /// the armed edge of the lowest-id live pointer whether or not it
    /// confirmed yet. `field_drag` swallows the whole stream under a
    /// candidate (in-slop moves stay quiet instead of clobbering the
    /// anchor through `drag_x`) and moves the edge once confirmed.
    /// Untracked.
    pub fn handle_candidate_edge(&self) -> Option<crate::input::HandleEdge> {
        let input = self.inner.input.borrow();
        let id = *input.captures.keys().min()?;
        input.handle_drags.get(&id).copied()
    }

    /// Arms the collapsed-hold toolbar (decision 459): the next
    /// snapshot build offers the Paste bar for the collapsed caret.
    /// Called by `field_long_press` when the hold lands on no word.
    pub fn arm_caret_toolbar(&self) {
        self.inner.input.borrow().caret_toolbar_armed.set(true);
    }

    /// Consumes a pending collapsed-hold toolbar request (runner-side —
    /// the snapshot build calls this once per pump; `true` arms the
    /// Paste bar for the current collapsed caret, never twice).
    pub fn take_caret_toolbar(&self) -> bool {
        self.inner.input.borrow().caret_toolbar_armed.replace(false)
    }

    /// Which scene-toolbar item owns the tap at `(x, y)` (decision
    /// 455): overlay items are handlerless presentational nodes, so
    /// taps resolve geometrically against their committed boxes (top
    /// z-layer wins — the overlay paints last, so the first item in
    /// model order containing the point owns it). `None` with no
    /// published toolbar or no item in reach — the Down falls through
    /// to the tap path. Untracked (router geometry, never effects).
    pub fn toolbar_hit(&self, x: f32, y: f32) -> Option<crate::input::ToolbarAction> {
        let model = crate::reactive::untrack(|| self.toolbar_model().get())?;
        for item in &model.items {
            let Some(b) = self.committed_box_by_debug(item.debug_name()) else {
                continue;
            };
            if x >= b.x && x < b.x + b.w && y >= b.y && y < b.y + b.h {
                return Some(item.action());
            }
        }
        None
    }

    /// Which handle edge owns the grab at `(x, y)` (decision 454):
    /// the nearest live handle bar-or-knob within
    /// [`HANDLE_HIT_SLOP_PX`](crate::input::HANDLE_HIT_SLOP_PX)
    /// (knob centre below the line bottom; ties prefer the nearer
    /// edge, start wins exact ties). `None` with no live handles or
    /// no grab in reach — the Down falls through to the tap path.
    /// Hidden handles (decision 474) never arm: publication is
    /// `None` there, so this misses exactly like a bare field.
    /// Untracked.
    pub fn handle_hit(&self, x: f32, y: f32) -> Option<crate::input::HandleEdge> {
        use crate::input::{HandleEdge, HANDLE_HIT_SLOP_PX};
        use crate::render::HANDLE_KNOB_R_PX;
        let (start, end) = self.focused_handles_paint()?;
        let score = |h: &crate::render::HandlePaint| {
            let bar_dx = (x - h.x).abs();
            let bar_dy = if y < h.y {
                h.y - y
            } else if y > h.y + h.h {
                y - (h.y + h.h)
            } else {
                0.0
            };
            let bar = bar_dx.hypot(bar_dy);
            let knob = ((x - h.x).powi(2) + (y - (h.y + h.h + HANDLE_KNOB_R_PX)).powi(2)).sqrt()
                - HANDLE_KNOB_R_PX;
            bar.min(knob)
        };
        let (ds, de) = (score(&start), score(&end));
        if ds <= HANDLE_HIT_SLOP_PX || de <= HANDLE_HIT_SLOP_PX {
            Some(if ds <= de {
                HandleEdge::Start
            } else {
                HandleEdge::End
            })
        } else {
            None
        }
    }

    /// Seconds until the focused caret's next blink flip, if any
    /// (Round 15.2, decision 313 — the runner tick query): `Some`
    /// only when a field session is focused with a collapsed
    /// selection (the one overlay that flips without input — the
    /// highlight never blinks). `None` covers everything else:
    /// unfocused, open selection, session-less focus, or an
    /// ambiguous multi-session owner (advisory timing never
    /// resolves ambiguity — typing still refuses loudly there).
    /// Untracked (never schedules).
    pub fn caret_blink_in_secs(&self) -> Option<f64> {
        use crate::editing::CARET_BLINK_PERIOD_SECS;
        let focus = self.focused_node()?;
        // The `focused_field_session` walk without the ambiguity
        // panic (see that fn — timers must never introduce a panic
        // on paths typing never reaches).
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
        let sessions = self.edit_sessions_for(inst);
        let [session] = sessions.as_slice() else {
            return None;
        };
        let sel = session.selection();
        if sel.0 != sel.1 {
            return None;
        }
        let period = CARET_BLINK_PERIOD_SECS;
        let pos = (self.inner.rt.now_secs() - session.caret_epoch()).rem_euclid(period);
        Some(if pos < period * 0.5 {
            period * 0.5 - pos
        } else {
            period - pos
        })
    }

    /// Text origin x (device px) of the first laid `Text` leaf under
    /// `node` (Round 8.1 — tap-to-caret subtracts this to form the
    /// session-local x for `click_x`/`shift_click_x`). `None` when no
    /// laid text exists yet (empty/placeholder fields keep the
    /// `caret_to_end` fallback). Untracked.
    pub fn text_origin_under(&self, node: NodeId) -> Option<f32> {
        let rec = self.inner.rec.borrow();
        let mut stack = vec![node];
        while let Some(id) = stack.pop() {
            let Some(n) = rec.get(id) else {
                continue;
            };
            if n.tag == crate::vnode::Tag::Text {
                if let Some(b) = n.layout.clone() {
                    return Some(b.x);
                }
            }
            for child in n.children.iter().rev() {
                stack.push(*child);
            }
        }
        None
    }

    /// Installs the host text service as the session's shaper, sized to
    /// `style` through the layout config (Round 8.1 — the same rule the
    /// IME anchor uses: family + class size + DPR; `Custom` sizes are
    /// absolute). Quiet when the host has no service (decision-207
    /// graceful no-ops preserved). Untracked.
    pub fn ensure_session_shaper(&self, session: &EditSession, style: TextClass) {
        let cfg = self.inner.layout.borrow().config().clone();
        let service = self.inner.text_service.borrow().clone();
        let Some(service) = service else {
            return;
        };
        let (size_px, weight) = match style {
            TextClass::TitleSmall => (cfg.title_px, FontWeight::NORMAL),
            TextClass::BodySecondary => (cfg.body_px, FontWeight::NORMAL),
            TextClass::Custom { size_px, weight } => (size_px as f32, weight),
        };
        let mut text_style = TextStyle::new(&cfg.family, size_px);
        text_style.device_pixel_ratio = cfg.device_pixel_ratio;
        text_style.weight = weight;
        session.set_shaper(service, text_style);
    }
}
