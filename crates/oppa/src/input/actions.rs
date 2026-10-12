//! Shared text-edit actions (cross-platform).
//!
//! Every platform handles the same native text actions (select-all,
//! copy/cut/paste, undo/redo, caret motion, word jumps, line moves);
//! only the intake (which keycode, which gesture) and the pixels
//! (who draws handles/toolbar) differ per platform. This module owns
//! the shared funnel: [`EditAction`] names the action,
//! [`apply_edit_action`] performs it against an [`EditSession`] plus
//! a [`Clipboard`](crate::Clipboard), and [`action_for_key`] maps the
//! framework key space ([`keys`]) plus Shift/Ctrl onto it.
//!
//! Runners (desktop `run_edit_shortcut`/`step_session_nav`, Android
//! `apply_input_batch`) route through here instead of duplicating the
//! match. Alt/Meta chords never map (AltGr guard, the desktop step
//! precedent) — callers check those before calling.
//! [`text_for_hardware_key`] covers printable keys no action claims
//! (decision 460 — hardware/adb keys no IME translates).

use super::keys;
use crate::clipboard::Clipboard;
use crate::editing::EditSession;

/// One native text action, intake-agnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    SelectAll,
    Copy,
    Cut,
    Paste,
    /// Explicit selection delete (decision 459 — the toolbar Delete
    /// item; no key maps here, it arrives only as a tap action).
    DeleteSelection,
    Undo,
    Redo,
    Backspace,
    DeleteForward,
    MoveLeft,
    MoveRight,
    ExtendLeft,
    ExtendRight,
    WordLeft,
    WordRight,
    ExtendWordLeft,
    ExtendWordRight,
    ToStart,
    ToEnd,
    ExtendToStart,
    ExtendToEnd,
    LineUp,
    LineDown,
    ExtendLineUp,
    ExtendLineDown,
}

/// Performs one action on the session. Returns `true` when the action
/// ran (including quiet no-ops like backspace at start — the key was
/// consumed either way); clipboard failures report `false` so callers
/// can log loudly and continue.
pub fn apply_edit_action(
    session: &EditSession,
    action: EditAction,
    clipboard: &mut dyn Clipboard,
) -> bool {
    match action {
        EditAction::SelectAll => {
            session.select_all();
            true
        }
        EditAction::Copy => session.copy_selection_to(clipboard).is_ok(),
        EditAction::Cut => session.cut_selection_to(clipboard).is_ok(),
        EditAction::Paste => session.paste_from(clipboard).is_ok(),
        EditAction::DeleteSelection => {
            session.delete_selection();
            true
        }
        EditAction::Undo => {
            session.undo();
            true
        }
        EditAction::Redo => {
            session.redo();
            true
        }
        EditAction::Backspace => {
            session.backspace();
            true
        }
        EditAction::DeleteForward => {
            session.delete_forward();
            true
        }
        EditAction::MoveLeft => {
            session.caret_left();
            true
        }
        EditAction::MoveRight => {
            session.caret_right();
            true
        }
        EditAction::ExtendLeft => {
            session.extend_left();
            true
        }
        EditAction::ExtendRight => {
            session.extend_right();
            true
        }
        EditAction::WordLeft => {
            session.word_left();
            true
        }
        EditAction::WordRight => {
            session.word_right();
            true
        }
        EditAction::ExtendWordLeft => {
            session.extend_word_left();
            true
        }
        EditAction::ExtendWordRight => {
            session.extend_word_right();
            true
        }
        EditAction::ToStart => {
            session.caret_to_start();
            true
        }
        EditAction::ToEnd => {
            session.caret_to_end();
            true
        }
        EditAction::ExtendToStart => {
            session.extend_to_start();
            true
        }
        EditAction::ExtendToEnd => {
            session.extend_to_end();
            true
        }
        EditAction::LineUp => {
            if session.is_multiline() {
                session.line_up();
                true
            } else {
                false
            }
        }
        EditAction::LineDown => {
            if session.is_multiline() {
                session.line_down();
                true
            } else {
                false
            }
        }
        EditAction::ExtendLineUp => {
            if session.is_multiline() {
                session.extend_line_up();
                true
            } else {
                false
            }
        }
        EditAction::ExtendLineDown => {
            if session.is_multiline() {
                session.extend_line_down();
                true
            } else {
                false
            }
        }
    }
}

/// Maps the framework key space onto an action.
///
/// - `ctrl` (no Alt/Meta — callers guard those) selects the shortcut
///   row: A select-all, Z/Y undo/redo, C/X/V clipboard, Ctrl+arrows
///   word jumps, Ctrl+Home/End to-ends.
/// - Plain (and Shift-extended) keys select the navigation row:
///   Backspace/Delete, arrows, Home/End, multiline-gated Up/Down.
/// - Returns `None` for unmapped keys (function keys, Alt/Meta
///   chords, single-line Up/Down) — the caller forwards those to the
///   router like any other unhandled key.
pub fn action_for_key(
    code: u32,
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
) -> Option<EditAction> {
    if alt || meta {
        return None;
    }
    if ctrl {
        return match (code, shift) {
            (c, _) if c == keys::A => Some(EditAction::SelectAll),
            (c, _) if c == keys::Z => Some(EditAction::Undo),
            (c, _) if c == keys::Y => Some(EditAction::Redo),
            (c, _) if c == keys::C => Some(EditAction::Copy),
            (c, _) if c == keys::X => Some(EditAction::Cut),
            (c, _) if c == keys::V => Some(EditAction::Paste),
            (c, false) if c == keys::LEFT => Some(EditAction::WordLeft),
            (c, false) if c == keys::RIGHT => Some(EditAction::WordRight),
            (c, true) if c == keys::LEFT => Some(EditAction::ExtendWordLeft),
            (c, true) if c == keys::RIGHT => Some(EditAction::ExtendWordRight),
            (c, false) if c == keys::HOME => Some(EditAction::ToStart),
            (c, false) if c == keys::END => Some(EditAction::ToEnd),
            (c, true) if c == keys::HOME => Some(EditAction::ExtendToStart),
            (c, true) if c == keys::END => Some(EditAction::ExtendToEnd),
            _ => None,
        };
    }
    match (code, shift) {
        (c, _) if c == keys::BACKSPACE => Some(EditAction::Backspace),
        (c, _) if c == keys::DELETE => Some(EditAction::DeleteForward),
        (c, false) if c == keys::LEFT => Some(EditAction::MoveLeft),
        (c, false) if c == keys::RIGHT => Some(EditAction::MoveRight),
        (c, true) if c == keys::LEFT => Some(EditAction::ExtendLeft),
        (c, true) if c == keys::RIGHT => Some(EditAction::ExtendRight),
        (c, false) if c == keys::HOME => Some(EditAction::ToStart),
        (c, false) if c == keys::END => Some(EditAction::ToEnd),
        (c, true) if c == keys::HOME => Some(EditAction::ExtendToStart),
        (c, true) if c == keys::END => Some(EditAction::ExtendToEnd),
        (c, false) if c == keys::UP => Some(EditAction::LineUp),
        (c, false) if c == keys::DOWN => Some(EditAction::LineDown),
        (c, true) if c == keys::UP => Some(EditAction::ExtendLineUp),
        (c, true) if c == keys::DOWN => Some(EditAction::ExtendLineDown),
        _ => None,
    }
}

/// Maps a framework key code onto printable text for hardware keys
/// that reach a runner without IME translation (decision 460).
///
/// Background: on Android the proxy's `onKeyDown` unicode path
/// never fires for hardware/adb keys (decision 461 entry-probe —
/// the IME stage consumes/transforms them first).
/// Fields that Gboard translates (plain text → `commitText` on the
/// connection) type fine; fields it refuses (password variation in
/// incognito mode) go silent — the native queue sees the key with no
/// action mapping and no text handling. Runners call this after
/// [`action_for_key`] returns `None` (shortcuts/navigation keep
/// priority) and commit the result through the same
/// `CompositionCommitted` funnel soft commits use, so composing and
/// masked semantics stay identical.
///
/// Scope is deliberately narrow — only characters the framework key
/// space names unambiguously: ASCII letters (Shift-aware), ASCII
/// digits unshifted, and space. Shifted digits stay `None`
/// (layout-dependent symbols — AZERTY vs QWERTY disagree — so the
/// runner keeps today's router-forward instead of risking a wrong
/// character); everything else (including Enter/Tab/Escape, which
/// runners route as activation/focus keys) stays `None`.
pub fn text_for_hardware_key(code: u32, shift: bool) -> Option<char> {
    match code {
        0x41..=0x5A => {
            let c = (code as u8) as char;
            Some(if shift { c } else { c.to_ascii_lowercase() })
        }
        0x30..=0x39 if !shift => Some((code as u8) as char),
        keys::SPACE => Some(' '),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::InMemoryClipboard;

    #[test]
    fn ctrl_shortcuts_map() {
        assert_eq!(
            action_for_key(keys::A, false, true, false, false),
            Some(EditAction::SelectAll)
        );
        assert_eq!(
            action_for_key(keys::C, false, true, false, false),
            Some(EditAction::Copy)
        );
        assert_eq!(
            action_for_key(keys::V, false, true, false, false),
            Some(EditAction::Paste)
        );
        assert_eq!(
            action_for_key(keys::LEFT, false, true, false, false),
            Some(EditAction::WordLeft)
        );
        assert_eq!(
            action_for_key(keys::LEFT, true, true, false, false),
            Some(EditAction::ExtendWordLeft)
        );
    }

    #[test]
    fn alt_meta_never_map() {
        assert_eq!(action_for_key(keys::A, false, true, true, false), None);
        assert_eq!(action_for_key(keys::A, false, true, false, true), None);
        assert_eq!(action_for_key(keys::LEFT, false, false, true, false), None);
    }

    #[test]
    fn plain_nav_maps() {
        assert_eq!(
            action_for_key(keys::BACKSPACE, false, false, false, false),
            Some(EditAction::Backspace)
        );
        assert_eq!(
            action_for_key(keys::LEFT, true, false, false, false),
            Some(EditAction::ExtendLeft)
        );
        assert_eq!(
            action_for_key(keys::UP, false, false, false, false),
            Some(EditAction::LineUp)
        );
        assert_eq!(
            action_for_key(keys::ENTER, false, false, false, false),
            None
        );
    }

    #[test]
    fn funnel_edits_and_consumes() {
        let rt = crate::reactive::Runtime::new();
        let content = rt.signal(crate::vnode::SharedString::from("hi"));
        let session = EditSession::new(rt.clone(), content.clone());
        let mut clip = InMemoryClipboard::new();
        assert!(apply_edit_action(
            &session,
            EditAction::SelectAll,
            &mut clip
        ));
        assert!(apply_edit_action(&session, EditAction::Copy, &mut clip));
        assert!(apply_edit_action(&session, EditAction::MoveLeft, &mut clip));
        // Single-line Up/Down do not consume (router keeps them).
        assert!(!apply_edit_action(&session, EditAction::LineUp, &mut clip));
    }

    #[test]
    fn hardware_text_maps_letters_digits_space() {
        assert_eq!(text_for_hardware_key(0x41, false), Some('a'));
        assert_eq!(text_for_hardware_key(0x41, true), Some('A'));
        assert_eq!(text_for_hardware_key(0x5A, false), Some('z'));
        assert_eq!(text_for_hardware_key(0x5A, true), Some('Z'));
        assert_eq!(text_for_hardware_key(0x30, false), Some('0'));
        assert_eq!(text_for_hardware_key(0x39, false), Some('9'));
        assert_eq!(text_for_hardware_key(keys::SPACE, false), Some(' '));
        assert_eq!(text_for_hardware_key(keys::SPACE, true), Some(' '));
    }

    #[test]
    fn hardware_text_refuses_ambiguous_and_routed() {
        // Shifted digits are layout-dependent symbols — never guess.
        assert_eq!(text_for_hardware_key(0x30, true), None);
        // Activation/focus/navigation keys route elsewhere.
        assert_eq!(text_for_hardware_key(keys::ENTER, false), None);
        assert_eq!(text_for_hardware_key(keys::TAB, false), None);
        assert_eq!(text_for_hardware_key(keys::ESCAPE, false), None);
        assert_eq!(text_for_hardware_key(keys::BACKSPACE, false), None);
        assert_eq!(text_for_hardware_key(keys::DELETE, false), None);
        assert_eq!(text_for_hardware_key(keys::LEFT, false), None);
        // Below/above the mapped ranges.
        assert_eq!(text_for_hardware_key(0x2F, false), None);
        assert_eq!(text_for_hardware_key(0x3A, false), None);
        assert_eq!(text_for_hardware_key(0x40, false), None);
        assert_eq!(text_for_hardware_key(0x5B, false), None);
    }
}
