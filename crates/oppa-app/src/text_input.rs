// ---------------------------------------------------------------------------
// Keyboard text, IME feeds, scroll
// ---------------------------------------------------------------------------

use super::*;

impl DesktopLoop {
    /// The Escape-at-root exit rule (shared by both platform loops):
    /// a pressed Escape with nothing focused exits; anywhere else it
    /// flows to the router (which clears focus). Pure over the host
    /// — headless-tested below.
    pub fn escape_exits(&self, code: u32, pressed: bool) -> bool {
        pressed && code == oppa::input::keys::ESCAPE && self.host.focused_node().is_none()
    }

    /// Types `text` into the focused text field's session, if any
    /// (decision 243): caret-aware insert with undo coalescing, then
    /// settle + repaint. Returns the repaint's damage count; `Ok(0)`
    /// when no field is focused (quiet miss — router precedent for
    /// unhandled keys). Control chars are skipped (the spike's
    /// WM_CHAR rule: shortcut shadows like Enter/Escape arrive via
    /// `Key`, never as text) — an all-control input paints nothing.
    pub fn type_text(&mut self, text: &str) -> Result<usize, String> {
        let printable: String = text.chars().filter(|c| (*c as u32) >= 0x20).collect();
        if printable.is_empty() {
            return Ok(0);
        }
        let Some(session) = self.host.focused_field_session() else {
            return Ok(0);
        };
        session.insert(&printable);
        self.host.run_until_idle();
        self.repaint()
    }

    /// Backspace into the focused field's session (selection, else
    /// char before caret — unicode-correct, undo-coalesced). `Ok(0)`
    /// when no field is focused. Consumed by the runner glue (never
    /// also injected — a future field `Key` handler must not see it
    /// twice).
    pub fn backspace(&mut self) -> Result<usize, String> {
        let Some(session) = self.host.focused_field_session() else {
            return Ok(0);
        };
        session.backspace();
        self.host.run_until_idle();
        self.repaint()
    }

    /// Delete-forward into the focused field's session (selection,
    /// else char after caret). Same quiet-miss and consume rules as
    /// [`DesktopLoop::backspace`].
    pub fn delete_forward(&mut self) -> Result<usize, String> {
        let Some(session) = self.host.focused_field_session() else {
            return Ok(0);
        };
        session.delete_forward();
        self.host.run_until_idle();
        self.repaint()
    }

    /// Feeds normalized IME composition events into the focused
    /// field's session (Round 2.1, decision 256 — the Windows runner
    /// maps `WM_IME_*` snapshots and the Linux runner maps winit `Ime`
    /// events into these; see `feed_ime_preedit` / `feed_ime_commit` /
    /// `feed_ime_cancel` for the platform-shaped halves). Dispatches
    /// through the one `dispatch_ime_event` seam, then settles +
    /// repaints, returning the repaint's damage count. Quiet `Ok(0)`
    /// when no field is focused (the `type_text` precedent — an IME
    /// event with no focused field is a focus race, never a wiring
    /// bug). Runners pump the candidate anchor
    /// ([`DesktopLoop::ime_anchor`]) after every call.
    pub fn feed_ime(&mut self, events: &[ImeCompositionEvent]) -> Result<usize, String> {
        let Some(mut session) = self.host.focused_field_session() else {
            return Ok(0);
        };
        for ev in events {
            dispatch_ime_event(&mut session, ev);
        }
        self.host.run_until_idle();
        self.repaint()
    }

    /// Preedit feed (the Linux/winit half of Round 2.1): non-empty
    /// preedit text ensures an open composition — anchored at the
    /// selection start, replacing the selection exactly like the
    /// Windows mapper — and updates it with the byte-wise caret
    /// (winit units; the session floors to a char boundary). Empty
    /// preedit is a no-op (winit sends it right before `Commit`, and
    /// a lone clear must leave the composition open for the commit —
    /// stated, not silent). Returns repaint damage; quiet `Ok(0)`
    /// when unfocused.
    pub fn feed_ime_preedit(
        &mut self,
        text: &str,
        caret_byte: Option<usize>,
    ) -> Result<usize, String> {
        if text.is_empty() {
            return Ok(0);
        }
        let Some(session) = self.host.focused_field_session() else {
            return Ok(0);
        };
        let sel = session.selection();
        let mut events = Vec::new();
        if !session.is_composing() {
            events.push(ImeCompositionEvent::CompositionStarted { start_byte: sel.0 });
            if sel.0 != sel.1 {
                events.push(ImeCompositionEvent::DeleteRange { range: sel });
            }
        }
        let start = session.composition_start_byte().unwrap_or(sel.0);
        events.push(ImeCompositionEvent::CompositionUpdated {
            composition: text.to_string(),
            caret_byte: start + caret_byte.unwrap_or(text.len()),
        });
        self.feed_ime(&events)
    }

    /// Commit feed: replaces the open composition (or inserts cold —
    /// the session spells it) as one atomic undo unit. Empty commits
    /// and unfocused commits are quiet `Ok(0)` no-ops.
    pub fn feed_ime_commit(&mut self, text: &str) -> Result<usize, String> {
        if text.is_empty() {
            return Ok(0);
        }
        self.feed_ime(&[ImeCompositionEvent::CompositionCommitted {
            committed: text.to_string(),
        }])
    }

    /// Cancel feed: drops the open composition, caret back to anchor.
    /// Quiet `Ok(0)` when nothing composes or nothing is focused.
    pub fn feed_ime_cancel(&mut self) -> Result<usize, String> {
        let Some(session) = self.host.focused_field_session() else {
            return Ok(0);
        };
        if !session.is_composing() {
            return Ok(0);
        }
        self.feed_ime(&[ImeCompositionEvent::CompositionCancelled])
    }

    /// Candidate-window anchor for the focused field in client px
    /// (Round 2.1): the host-resolved caret rect — runners hand it to
    /// the OS candidate window after every IME step so the candidate
    /// follows the composition caret dynamically. `None` when
    /// unfocused or unshaped (quiet — anchoring is advisory).
    pub fn ime_anchor(&self) -> Option<[f32; 4]> {
        self.host.focused_ime_anchor()
    }

    /// Routes one wheel tick at device-px `(x, y)` to the scrollable
    /// under the cursor (decision 250): hit-test + walk-up resolve
    /// the target, the `Scroll` event injects (handler dispatch +
    /// explicit-or-self-wired feed accumulation in the router), then
    /// settle + repaint. Quiet `Ok(0)` over non-scrollable area (router
    /// precedent for unhandled keys — scrolling dead space is not a
    /// wiring bug). `dx` feeds the bound horizontal feed when the
    /// target opted in (`bind_scroll_x` — Round 9.3, decision 302),
    /// `dy` the vertical feed as before (explicit feeds win; otherwise
    /// the target's handler owner self-wires, Round 24.2).
    pub fn scroll_at(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> Result<usize, String> {
        let Some(target) = self.host.scroll_target_at(x, y) else {
            return Ok(0);
        };
        self.host
            .inject_input(InputEvent::Scroll { target, dx, dy });
        self.host.run_until_idle();
        self.repaint()
    }

    /// Injects a raster face for one shaper font id (decision 200):
    /// without it the CPU backend draws advance-cell bars and the
    /// Vello twin refuses loudly at paint; with it, real outlines on
    /// both. Best-effort by contract (callers warn, never die, when
    /// the face is unreadable). Always lands in both backends so a
    /// later `enable_gpu` never paints tofu.
    pub fn set_font_for(&mut self, id: oppa::text::FontId, bytes: Vec<u8>, index: u32) {
        self.cpu.set_font_for(id, bytes.clone(), index);
        self.vello.set_font_for(id, bytes.clone(), index);
        self.custom_gpu.set_font_for(id, bytes, index);
    }
}
