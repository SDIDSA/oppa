// ---------------------------------------------------------------------------
// Input stepping and editing shortcuts
// ---------------------------------------------------------------------------

use super::*;

impl DesktopLoop {
    /// Injects one framework input event, settles, and repaints.
    /// Returns the repaint's damage-rect count (0 = nothing to
    /// present). Paint failures are loud `Err`s.
    ///
    /// Ctrl+letter editing shortcuts are consumed here (decision
    /// 246): one interception point for both platform arms,
    /// headless-testable through this method. Only pressed keys with
    /// ctrl held and neither alt nor meta (AltGr is ctrl+alt at the
    /// OS level and types real chars on many layouts — swallowing
    /// those as shortcuts would eat user text; Win-key combos never
    /// reach the app on Windows and stay router-quiet on Linux).
    /// Consumed shortcuts never reach `inject_input` (no duplicate
    /// handling, no character entry).
    pub fn step(&mut self, ev: InputEvent) -> Result<usize, String> {
        if let InputEvent::Key {
            code,
            modifiers,
            state,
            ..
        } = &ev
        {
            if *state == KeyState::Pressed
                && is_edit_shortcut_modifier(*modifiers)
                && is_edit_shortcut_code(*code)
            {
                return self.run_edit_shortcut(*code);
            }
            // Round 22.1 (decision 331): navigation keys drive the
            // focused session directly (plain/shift/ctrl word-step
            // and extension included) — consumed so controls never
            // see them twice; anything unmapped (or with no focused
            // field) flows to the router, so slider/scrollbar arrows
            // keep working.
            if *state == KeyState::Pressed
                && !modifiers.alt
                && !modifiers.meta
                && self.step_session_nav(*code, modifiers.shift, modifiers.ctrl)?
            {
                self.scene.host().run_until_idle();
                return self.repaint();
            }
            // Round 22.2 (decision 332): vertical keys drive the
            // focused multi-line session (same consume contract —
            // single-line Up/Down still flows to the router).
            if *state == KeyState::Pressed
                && !modifiers.alt
                && !modifiers.meta
                && !modifiers.ctrl
                && self.step_session_vline(*code, modifiers.shift)?
            {
                self.scene.host().run_until_idle();
                return self.repaint();
            }
        }
        self.scene.host().inject_input(ev);
        self.scene.host().run_until_idle();
        self.repaint()
    }

    /// Runs one editing shortcut against the focused field's session
    /// (decision 246): select-all / undo / redo / copy / cut / paste.
    /// Quiet `Ok(0)` when no field is focused (router precedent for
    /// unhandled keys) or when the op is a no-op (copy/cut with a
    /// collapsed caret, paste of an empty clipboard or
    /// mid-composition — the session spells those, see
    /// `PasteOutcome`); settle + repaint only when content or
    /// selection changed. Backend clipboard failures are loud on
    /// stderr but never fatal: a locked clipboard is a routine
    /// transient (the G3 contract says retry-next-frame), and killing
    /// the app over it would punish the user for another app's lock
    /// — stated, not silent.
    pub fn run_edit_shortcut(&mut self, code: u32) -> Result<usize, String> {
        let Some(session) = self.scene.host().focused_field_session() else {
            return Ok(0);
        };
        let changed = match code {
            keys::A => {
                session.select_all();
                true
            }
            keys::Z => {
                session.undo();
                true
            }
            keys::Y => {
                session.redo();
                true
            }
            keys::C => match session.copy_selection_to(&mut *self.clipboard) {
                Ok(copied) => copied.is_some(),
                Err(e) => {
                    eprintln!("oppa-app: clipboard copy failed: {e}");
                    return Ok(0);
                }
            },
            keys::X => match session.cut_selection_to(&mut *self.clipboard) {
                Ok(cut) => cut.is_some(),
                Err(e) => {
                    eprintln!("oppa-app: clipboard cut failed: {e}");
                    return Ok(0);
                }
            },
            keys::V => match session.paste_from(&mut *self.clipboard) {
                Ok(PasteOutcome::Pasted(_)) => true,
                Ok(_) => false,
                Err(e) => {
                    eprintln!("oppa-app: clipboard paste failed: {e}");
                    return Ok(0);
                }
            },
            _ => return Ok(0),
        };
        if !changed {
            return Ok(0);
        }
        self.scene.host().run_until_idle();
        self.repaint()
    }

    /// Drives one navigation key into the focused field's session
    /// (Round 22.1, decision 331): Left/Right/Home/End in
    /// plain/shift/ctrl/ctrl+shift combinations — char-step,
    /// extension, word-step, word-extension, and jump/extend-to-ends
    /// (the session spells the moves; masked fields navigate
    /// identically, only copy/cut refuse). Returns true when a move
    /// ran (the caller settles + repaints); false with no focused
    /// session or an unmapped key — the key then flows to the
    /// router untouched (slider/scrollbar/menu arrows keep their
    /// owners). Alt/Meta combinations never reach here (the `step`
    /// guard above keeps AltGr typing intact).
    fn step_session_nav(&mut self, code: u32, shift: bool, ctrl: bool) -> Result<bool, String> {
        use keys::{END, HOME, LEFT, RIGHT};
        use oppa::EditSession;
        let op: fn(&EditSession) = match (code, shift, ctrl) {
            (LEFT, false, false) => EditSession::caret_left,
            (RIGHT, false, false) => EditSession::caret_right,
            (LEFT, true, false) => EditSession::extend_left,
            (RIGHT, true, false) => EditSession::extend_right,
            (LEFT, false, true) => EditSession::word_left,
            (RIGHT, false, true) => EditSession::word_right,
            (LEFT, true, true) => EditSession::extend_word_left,
            (RIGHT, true, true) => EditSession::extend_word_right,
            (HOME, false, _) => EditSession::caret_to_start,
            (END, false, _) => EditSession::caret_to_end,
            (HOME, true, _) => EditSession::extend_to_start,
            (END, true, _) => EditSession::extend_to_end,
            _ => return Ok(false),
        };
        let Some(session) = self.scene.host().focused_field_session() else {
            return Ok(false);
        };
        op(&session);
        Ok(true)
    }

    /// Drives one vertical key into the focused multi-line session
    /// (Round 22.2, decision 332 — ArrowUp/ArrowDown with optional
    /// Shift): visual-line travel with preferred-x affinity on
    /// `TextArea` sessions, router-quiet otherwise (single-line
    /// fields keep the 22.1 behavior — Up/Down flow to directional
    /// handlers — and slider/scrollbar/menu arrows keep their
    /// owners). Same consume/settle contract as `step_session_nav`.
    fn step_session_vline(&mut self, code: u32, shift: bool) -> Result<bool, String> {
        use keys::{DOWN, UP};
        use oppa::EditSession;
        let op: fn(&EditSession) = match (code, shift) {
            (UP, false) => EditSession::line_up,
            (DOWN, false) => EditSession::line_down,
            (UP, true) => EditSession::extend_line_up,
            (DOWN, true) => EditSession::extend_line_down,
            _ => return Ok(false),
        };
        let Some(session) = self.scene.host().focused_field_session() else {
            return Ok(false);
        };
        if !session.is_multiline() {
            return Ok(false);
        }
        op(&session);
        Ok(true)
    }
}
