//! Unified Android input controller (rewrite foundation).
//!
//! The legacy pump scatters one gesture across five places: `events`
//! classifies, `ime_queue`/`selection_menu` queue, `present.rs`
//! owns cause tracking + dismissal pins + drag-settle + suppression
//! flags, and the JNI bridge executes mirrors. This controller is the
//! single host-testable owner of the Android input story going
//! forward:
//!
//! - intake classification stays in [`events`](super::events) (dp→px,
//!   keycode map — unchanged);
//! - the controller consumes classified [`AndroidCmd`]s plus drained
//!   [`ImeTextItem`]s and [`MenuAction`]s and emits renderer-owned
//!   decisions: [`ToolbarOp`] (renderer-drawn toolbar data, never a
//!   system `ActionMode` start) and handle-drag applications straight
//!   into the focused [`EditSession`];
//! - text actions route through the shared
//!   [`apply_edit_action`](oppa::apply_edit_action) funnel, so Android
//!   shortcuts stay byte-identical with desktop by construction;
//! - pixels stay renderer-owned: the controller publishes *data*
//!   (toolbar items + anchor, handle edges); the scene draws them.
//!
//! The proxy `EditText` keeps exactly one job: the keyboard's
//! `InputConnection` (commits/composition/deletes). It no longer hosts
//! selection, handles, or menus — those are Rust state + our pixels.
//!
//! std-only, no JNI linkage (the M10 crate rule). The runner keeps
//! executing JNI effects; the controller only decides.

use oppa::input::actions::{action_for_key, apply_edit_action, EditAction};
use oppa::{Clipboard, EditSession, ImeCompositionEvent, InputEvent, Modifiers, ToolbarAction};

use super::events::AndroidCmd;
use super::ime_queue::ImeTextItem;
use super::selection_menu::{
    dismissal_pin, drag_settled, utf16_to_byte_units, MenuAction, MenuSnapshot, MenuSync, SelCause,
};

/// Shared toolbar vocabulary (decision 455 — the model lives in
/// `oppa::input` so scene rendering stays cross-platform; re-exported
/// here so runner imports never fork).
pub use oppa::input::{ToolbarItem, ToolbarModel};

/// What the controller wants drawn this pump.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolbarOp {
    Show(ToolbarModel),
    Update(ToolbarModel),
    Hide,
    None,
}

/// Which handle the finger grabbed (renderer-drawn handles hit-test
/// scene-side; the drag streams here as byte offsets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleSide {
    Start,
    End,
}

/// Unified per-pump Android input state: selection cause, toolbar
/// mirror, dismissal pin, drag-settle clock, and the suppressed-show
/// flag (keyboard-driven selections stay quiet until a touch lands —
/// the verified native rule, decision 451, now owned here instead of
/// `present.rs` locals).
#[derive(Debug, Default)]
pub struct AndroidInput {
    sync: MenuSync,
    show_suppressed: bool,
    last_report_at: Option<std::time::Instant>,
    prev_batch_had_selection: bool,
}

impl AndroidInput {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a proxy selection report timestamp (drag-settle clock
    /// — mirrors the decision-450 gate: re-fires wait out the quiet
    /// window so a synthetic press never lands mid-drag).
    pub fn note_selection_report(&mut self, at: std::time::Instant) {
        self.last_report_at = Some(at);
        self.prev_batch_had_selection = true;
    }

    /// Clears per-batch report memory (call once per pump after the
    /// batch was consumed).
    pub fn end_batch(&mut self, had_selection: bool) {
        self.prev_batch_had_selection = had_selection;
    }

    /// Drops all mirror state (window-generation heals: the decor the
    /// old toolbar hung on is gone).
    pub fn reset(&mut self) {
        self.sync.reset();
        self.show_suppressed = false;
        self.last_report_at = None;
        self.prev_batch_had_selection = false;
    }

    /// Whether the last show was suppressed for a keyboard-driven
    /// selection (observability for the runner's logcat line — a later
    /// touch converts the next update into a fresh fire).
    pub fn show_suppressed(&self) -> bool {
        self.show_suppressed
    }

    /// Records a refused toolbar start (null mode — retry next pump
    /// instead of wedging in `shown`).
    pub fn note_show_failed(&mut self) {
        self.sync.note_show_failed();
    }

    /// Records a user dismissal at the pre-batch snapshot (the
    /// `Dismissed` action path when no session work is involved).
    pub fn note_dismissed(&mut self, at: Option<MenuSnapshot>) {
        self.sync.note_dismissed(at);
    }

    /// Derives the selection cause for this pump: pointer/menu/report
    /// activity marks touch, keys alone mark keyboard, quiet pumps
    /// inherit the last decided cause so IME traffic never flips it
    /// (the decision-451 rule, moved here from the runner).
    pub fn cause_for(
        &self,
        cmds: &[AndroidCmd],
        menu_actions: &[MenuAction],
        ime_items: &[ImeTextItem],
    ) -> SelCause {
        let touch = cmds.iter().any(|c| {
            matches!(
                c,
                AndroidCmd::PointerDown { .. }
                    | AndroidCmd::PointerMove { .. }
                    | AndroidCmd::PointerUp { .. }
            )
        }) || menu_actions
            .iter()
            .any(|a| !matches!(a, MenuAction::Dismissed))
            || !ime_items.is_empty();
        if touch {
            SelCause::Touch
        } else if cmds.iter().any(|c| matches!(c, AndroidCmd::Key { .. })) {
            SelCause::Key
        } else {
            self.sync.last_cause().unwrap_or(SelCause::Touch)
        }
    }

    /// Applies one toolbar/menu action to the focused session through
    /// the shared funnel. Returns `true` when the frame needs a
    /// repaint. `dismiss_pin` is the pre-decided dismissal marker for
    /// this batch (decision 449 — `None` near a selection report, so
    /// a drag-killed bar re-shows instead of wedging).
    pub fn apply_menu_action(
        &mut self,
        session: &EditSession,
        action: MenuAction,
        clipboard: &mut dyn Clipboard,
        dismiss_pin: Option<MenuSnapshot>,
    ) -> bool {
        match action {
            MenuAction::Dismissed => {
                self.sync.note_dismissed(dismiss_pin);
                false
            }
            MenuAction::SelectAll => {
                apply_edit_action(session, EditAction::SelectAll, clipboard);
                true
            }
            MenuAction::Copy => apply_edit_action(session, EditAction::Copy, clipboard),
            MenuAction::Cut => apply_edit_action(session, EditAction::Cut, clipboard),
            MenuAction::Paste => apply_edit_action(session, EditAction::Paste, clipboard),
            MenuAction::SelectionChanged { start16, end16 } => {
                if session.is_composing() {
                    return false;
                }
                let text = session.content_text();
                let a = utf16_to_byte_units(&text, start16);
                let b = utf16_to_byte_units(&text, end16);
                session.select_range(a, b);
                true
            }
        }
    }

    /// Applies one scene-toolbar tap to the focused session through
    /// the shared funnel (decision 455 — overlay items resolve
    /// geometrically in the router and arrive here; masked/composing
    /// guards ride the session rules like the menu path). Returns
    /// `true` when the frame needs a repaint.
    pub fn apply_toolbar_action(
        &self,
        session: &EditSession,
        action: ToolbarAction,
        clipboard: &mut dyn Clipboard,
    ) -> bool {
        let edit = match action {
            ToolbarAction::SelectAll => EditAction::SelectAll,
            ToolbarAction::Copy => EditAction::Copy,
            ToolbarAction::Cut => EditAction::Cut,
            ToolbarAction::Paste => EditAction::Paste,
            ToolbarAction::Delete => EditAction::DeleteSelection,
        };
        apply_edit_action(session, edit, clipboard)
    }

    /// Computes the dismissal pin for one pump batch (decision 449):
    /// a bare `Dismissed` pins the pre-batch snapshot, but a
    /// dismissal near a selection report pins nothing.
    pub fn pin_for(
        &self,
        pre: Option<MenuSnapshot>,
        batch_has_selection: bool,
    ) -> Option<MenuSnapshot> {
        dismissal_pin(pre, batch_has_selection || self.prev_batch_had_selection)
    }

    /// Applies a renderer-drawn handle drag: moves one selection edge
    /// to `byte` (floored to a char boundary by the session), keeping
    /// the other edge anchored. Composing skips (the composition owns
    /// the field).
    pub fn apply_handle_drag(&self, session: &EditSession, side: HandleSide, byte: usize) -> bool {
        if session.is_composing() {
            return false;
        }
        let (a, b) = session.selection();
        let (start, end) = (a.min(b), a.max(b));
        match side {
            HandleSide::Start => session.select_range(byte, end),
            HandleSide::End => session.select_range(start, byte),
        }
        true
    }

    /// Maps one hardware key press onto the focused session through
    /// the shared funnel. Returns `Some(consumed)` when the session
    /// existed (press-only; releases are never second edits);
    /// `None` with no focused session so the caller forwards to the
    /// router.
    pub fn apply_key(
        &self,
        session: &EditSession,
        code: u32,
        modifiers: Modifiers,
        clipboard: &mut dyn Clipboard,
    ) -> Option<bool> {
        if modifiers.alt || modifiers.meta {
            return None;
        }
        let action = action_for_key(
            code,
            modifiers.shift,
            modifiers.ctrl,
            modifiers.alt,
            modifiers.meta,
        )?;
        // Ctrl clipboard chords + select-all/undo/redo always
        // consume; navigation consumes except single-line Up/Down
        // (the `apply_edit_action` multiline gate reports those as
        // `false` so directional handlers keep their owners).
        let consumed = apply_edit_action(session, action, clipboard);
        if consumed {
            Some(true)
        } else {
            None
        }
    }

    /// Commits one hardware-typed character to the focused session
    /// through the soft-commit funnel (decision 460): hardware/adb
    /// keys no IME translates arrive at the native queue with no
    /// action mapping — without this they die in the router, which
    /// is exactly the password-field silence (Gboard translates
    /// plain fields into `commitText` on the connection but refuses
    /// the password variation, and the proxy `onKeyDown` unicode
    /// path never fires for these keys). `CompositionCommitted` carries composing and
    /// masked semantics identical to soft commits by construction.
    /// Returns `true` when the frame needs a repaint.
    pub fn apply_hardware_text(&self, session: &mut EditSession, ch: char) -> bool {
        oppa::dispatch_ime_event(
            session,
            &ImeCompositionEvent::CompositionCommitted {
                committed: ch.to_string(),
            },
        );
        true
    }

    /// One toolbar decision from the current snapshot. `want` carries
    /// the renderer's selection frame (`None` = collapsed/none).
    /// Gating order: dismissal pin → keyboard-suppress → drag-settle
    /// defer → fire. Touch updates onto a suppressed belief convert to
    /// a fresh show.
    pub fn decide_toolbar(&mut self, want: Option<(MenuSnapshot, ToolbarModel)>) -> ToolbarOp {
        let snapshot = want.as_ref().map(|(s, _)| s.clone());
        match self.sync.decide(snapshot) {
            super::selection_menu::MenuOp::Show(snap) => {
                if snap.cause != SelCause::Touch {
                    self.show_suppressed = true;
                    return ToolbarOp::None;
                }
                if !drag_settled(self.last_report_at, std::time::Instant::now()) {
                    self.sync.note_show_failed();
                    return ToolbarOp::None;
                }
                self.show_suppressed = false;
                let model = want.map(|(_, m)| m).expect("show carries a model");
                ToolbarOp::Show(model)
            }
            super::selection_menu::MenuOp::Update(snap) => {
                let model = want.map(|(_, m)| m).expect("update carries a model");
                if self.show_suppressed
                    && snap.cause == SelCause::Touch
                    && drag_settled(self.last_report_at, std::time::Instant::now())
                {
                    self.show_suppressed = false;
                    return ToolbarOp::Show(model);
                }
                ToolbarOp::Update(model)
            }
            super::selection_menu::MenuOp::Hide => {
                self.show_suppressed = false;
                ToolbarOp::Hide
            }
            super::selection_menu::MenuOp::None => ToolbarOp::None,
        }
    }

    /// Builds the renderer-drawn toolbar model for a selection: all
    /// four items while non-collapsed (Paste arms whenever the loop
    /// clipboard holds text — the caller filters when it knows the
    /// clipboard is empty; showing it unconditionally keeps the model
    /// host-testable without clipboard probes).
    pub fn toolbar_model(anchor: (i32, i32, i32, i32)) -> ToolbarModel {
        Self::toolbar_model_for(anchor, true, true, false)
    }

    /// Builds the renderer-drawn toolbar model for one editor state
    /// (decision 459): ranged selections offer the full set (minus
    /// Copy/Cut while masked — cleartext never leaves a password
    /// field, so deletion gets its own item instead of hiding behind
    /// Cut); collapsed carets offer Select-all (only with text) plus
    /// Paste (the hold-to-paste path needs no selection).
    pub fn toolbar_model_for(
        anchor: (i32, i32, i32, i32),
        ranged: bool,
        has_text: bool,
        masked: bool,
    ) -> ToolbarModel {
        let mut items = Vec::new();
        if has_text {
            items.push(ToolbarItem::SelectAll);
        }
        if ranged && !masked {
            items.push(ToolbarItem::Copy);
            items.push(ToolbarItem::Cut);
        }
        if ranged && masked {
            items.push(ToolbarItem::Delete);
        }
        items.push(ToolbarItem::Paste);
        ToolbarModel { items, anchor }
    }

    /// Converts classified pointer commands into shared pipeline
    /// events (the only commands that reach `inject_input`; text and
    /// key commands are runner-matched through the session funnel).
    pub fn pointer_event(cmd: AndroidCmd) -> Option<InputEvent> {
        match cmd {
            AndroidCmd::PointerDown { .. }
            | AndroidCmd::PointerMove { .. }
            | AndroidCmd::PointerUp { .. }
            | AndroidCmd::PointerCancel => Some(cmd.to_input_event()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oppa::InMemoryClipboard;

    fn session_with(text: &str) -> (oppa::Runtime, EditSession) {
        let rt = oppa::Runtime::new();
        let content = rt.signal(oppa::SharedString::from(text.to_string()));
        let session = EditSession::new(rt.clone(), content);
        (rt, session)
    }

    #[test]
    fn menu_actions_ride_the_shared_funnel() {
        let (_rt, session) = session_with("hello");
        let mut ctl = AndroidInput::new();
        let mut clip = InMemoryClipboard::new();
        session.select_all();
        assert!(ctl.apply_menu_action(&session, MenuAction::Copy, &mut clip, None));
        session.select_range(0, 0);
        assert!(ctl.apply_menu_action(&session, MenuAction::Paste, &mut clip, None));
        assert_eq!(session.content_text(), "hellohello");
    }

    #[test]
    fn key_shortcut_selects_all_through_funnel() {
        let (_rt, session) = session_with("hello");
        let ctl = AndroidInput::new();
        let mut clip = InMemoryClipboard::new();
        let mods = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        assert_eq!(
            ctl.apply_key(&session, oppa::input::keys::A, mods, &mut clip),
            Some(true)
        );
        assert_eq!(session.selection(), (0, 5));
    }

    #[test]
    fn hardware_text_commits_through_the_soft_funnel() {
        let (_rt, mut session) = session_with("ab");
        session.caret_to_end();
        let ctl = AndroidInput::new();
        assert!(ctl.apply_hardware_text(&mut session, 'c'));
        assert_eq!(session.content_text(), "abc");
    }

    #[test]
    fn hardware_text_commits_into_masked_sessions() {
        let (_rt, mut session) = session_with("");
        session.set_masked(true);
        let ctl = AndroidInput::new();
        assert!(ctl.apply_hardware_text(&mut session, 's'));
        assert_eq!(session.content_text(), "s");
    }

    #[test]
    fn alt_chord_falls_through_to_router() {
        let (_rt, session) = session_with("hello");
        let ctl = AndroidInput::new();
        let mut clip = InMemoryClipboard::new();
        let mods = Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::NONE
        };
        assert_eq!(
            ctl.apply_key(&session, oppa::input::keys::A, mods, &mut clip),
            None
        );
    }

    #[test]
    fn scene_toolbar_taps_apply_through_the_shared_funnel() {
        use oppa::ToolbarAction;
        let (_rt, session) = session_with("hello");
        let ctl = AndroidInput::new();
        let mut clip = InMemoryClipboard::new();
        session.select_all();
        // Copy arms the loop clipboard through the same backend the
        // chords use (decision 455 — one code path for both).
        assert!(ctl.apply_toolbar_action(&session, ToolbarAction::Copy, &mut clip));
        session.select_range(0, 0);
        assert!(ctl.apply_toolbar_action(&session, ToolbarAction::Paste, &mut clip));
        assert_eq!(session.content_text(), "hellohello");
        // Cut deletes through the same funnel.
        session.select_range(0, 5);
        assert!(ctl.apply_toolbar_action(&session, ToolbarAction::Cut, &mut clip));
        assert_eq!(session.content_text(), "hello");
        // Delete drops the selection without touching any clipboard.
        session.select_range(0, 5);
        assert!(ctl.apply_toolbar_action(&session, ToolbarAction::Delete, &mut clip));
        assert_eq!(session.content_text(), "");
        // Collapsed delete is a quiet no-op.
        assert!(ctl.apply_toolbar_action(&session, ToolbarAction::Delete, &mut clip));
        assert_eq!(session.content_text(), "");
    }

    #[test]
    fn toolbar_items_follow_editor_state() {
        // Ranged + plain: the full set.
        let full = AndroidInput::toolbar_model_for((0, 0, 10, 10), true, true, false);
        assert_eq!(
            full.items,
            vec![
                ToolbarItem::SelectAll,
                ToolbarItem::Copy,
                ToolbarItem::Cut,
                ToolbarItem::Paste,
            ]
        );
        // Ranged + masked: no copy/cut (cleartext never leaves),
        // deletion rides its own item; Paste stays last (stable
        // position across states).
        let masked = AndroidInput::toolbar_model_for((0, 0, 10, 10), true, true, true);
        assert_eq!(
            masked.items,
            vec![
                ToolbarItem::SelectAll,
                ToolbarItem::Delete,
                ToolbarItem::Paste,
            ]
        );
        // Collapsed with text: select-all plus paste (hold-to-paste).
        let caret = AndroidInput::toolbar_model_for((0, 0, 10, 10), false, true, false);
        assert_eq!(
            caret.items,
            vec![ToolbarItem::SelectAll, ToolbarItem::Paste]
        );
        // Collapsed + empty: paste alone.
        let empty = AndroidInput::toolbar_model_for((0, 0, 10, 10), false, false, false);
        assert_eq!(empty.items, vec![ToolbarItem::Paste]);
    }

    #[test]
    fn keyboard_selection_suppresses_until_touch() {
        let mut ctl = AndroidInput::new();
        let key_snap = MenuSnapshot {
            text: "hi".to_string(),
            start16: 0,
            end16: 2,
            rect: (0, 0, 10, 10),
            cause: SelCause::Key,
        };
        let model = AndroidInput::toolbar_model((0, 0, 10, 10));
        assert_eq!(
            ctl.decide_toolbar(Some((key_snap, model.clone()))),
            ToolbarOp::None
        );
        let touch_snap = MenuSnapshot {
            text: "hi".to_string(),
            start16: 0,
            end16: 1,
            rect: (0, 0, 10, 10),
            cause: SelCause::Touch,
        };
        assert!(matches!(
            ctl.decide_toolbar(Some((touch_snap, model))),
            ToolbarOp::Update(_) | ToolbarOp::Show(_)
        ));
    }

    #[test]
    fn handle_drag_moves_one_edge() {
        let (_rt, session) = session_with("hello");
        let ctl = AndroidInput::new();
        session.select_range(1, 4);
        assert!(ctl.apply_handle_drag(&session, HandleSide::Start, 0));
        assert_eq!(session.selection(), (0, 4));
        assert!(ctl.apply_handle_drag(&session, HandleSide::End, 5));
        assert_eq!(session.selection(), (0, 5));
    }

    #[test]
    fn dismissal_and_retry_passthroughs() {
        let mut ctl = AndroidInput::new();
        assert!(!ctl.show_suppressed());
        let snap = MenuSnapshot {
            text: "hi".to_string(),
            start16: 0,
            end16: 2,
            rect: (0, 0, 10, 10),
            cause: SelCause::Touch,
        };
        let model = AndroidInput::toolbar_model((0, 0, 10, 10));
        assert!(matches!(
            ctl.decide_toolbar(Some((snap.clone(), model.clone()))),
            ToolbarOp::Show(_)
        ));
        // Refused start retries the same frame instead of wedging.
        ctl.note_show_failed();
        assert!(matches!(
            ctl.decide_toolbar(Some((snap.clone(), model.clone()))),
            ToolbarOp::Show(_)
        ));
        // User dismissal pins until the selection moves.
        ctl.note_dismissed(Some(snap.clone()));
        assert_eq!(
            ctl.decide_toolbar(Some((snap, model.clone()))),
            ToolbarOp::None
        );
        // Reset clears the pin across heals.
        ctl.reset();
        let fresh = MenuSnapshot {
            text: "hi".to_string(),
            start16: 0,
            end16: 2,
            rect: (0, 0, 10, 10),
            cause: SelCause::Touch,
        };
        assert!(matches!(
            ctl.decide_toolbar(Some((fresh, model))),
            ToolbarOp::Show(_)
        ));
    }
}
