// ---------------------------------------------------------------------------
// Edit operations, clipboard, and undo machinery
// ---------------------------------------------------------------------------

use super::caret::{clamp_byte, ordered, trim_to_depth};
use super::*;

impl EditSession {
    /// Inserts `text` over the current selection. No-op while composing
    /// (the platform owns the field mid-composition — deliveries arrive
    /// as `composition_updated`) and for empty text. Contiguous collapsed
    /// inserts coalesce into one undo entry (decision 206).
    pub fn insert(&self, text: &str) {
        if text.is_empty() || self.is_composing() {
            return;
        }
        self.ensure_clamped();
        let (a, b) = ordered(*self.inner.sel.borrow());
        if a == b {
            self.push_undo_coalesce(LastOp::Insert);
        } else {
            self.push_undo_force();
        }
        let old = self.content_text();
        let mut new = String::with_capacity(old.len() + text.len());
        new.push_str(&old[..a]);
        new.push_str(text);
        new.push_str(&old[b..]);
        let caret = a + text.len();
        *self.inner.caret.borrow_mut() = caret;
        *self.inner.sel.borrow_mut() = (caret, caret);
        self.set_content(new);
        self.note_caret_activity();
        if a == b {
            *self.inner.last_op.borrow_mut() = LastOp::Insert;
        } else {
            self.break_run();
        }
    }

    /// Deletes the current selection (the explicit cut path). Collapsed
    /// selections are a no-op. Always a discrete undo entry (decision
    /// 206). No-op while composing (decision 205 rule — see `insert`).
    pub fn delete_selection(&self) {
        if self.is_composing() {
            return;
        }
        self.ensure_clamped();
        let (a, b) = ordered(*self.inner.sel.borrow());
        if a == b {
            return;
        }
        self.push_undo_force();
        let old = self.content_text();
        let mut new = String::with_capacity(old.len() - (b - a));
        new.push_str(&old[..a]);
        new.push_str(&old[b..]);
        *self.inner.caret.borrow_mut() = a;
        *self.inner.sel.borrow_mut() = (a, a);
        self.set_content(new);
        self.break_run();
        self.note_caret_activity();
    }

    /// Backspace: deletes the selection, else the char before the caret.
    /// Consecutive collapsed backspaces coalesce (decision 206). No-op
    /// while composing and at the start with a collapsed caret.
    pub fn backspace(&self) {
        if self.is_composing() {
            return;
        }
        self.ensure_clamped();
        let (a, b) = ordered(*self.inner.sel.borrow());
        if a != b {
            self.push_undo_force();
            let old = self.content_text();
            let mut new = String::with_capacity(old.len() - (b - a));
            new.push_str(&old[..a]);
            new.push_str(&old[b..]);
            *self.inner.caret.borrow_mut() = a;
            *self.inner.sel.borrow_mut() = (a, a);
            self.set_content(new);
            self.break_run();
            self.note_caret_activity();
            return;
        }
        if a == 0 {
            return;
        }
        let old = self.content_text();
        let prev_start = old[..a].char_indices().last().map(|(i, _)| i).unwrap_or(0);
        self.push_undo_coalesce(LastOp::Backspace);
        let mut new = String::with_capacity(old.len() - (a - prev_start));
        new.push_str(&old[..prev_start]);
        new.push_str(&old[a..]);
        *self.inner.caret.borrow_mut() = prev_start;
        *self.inner.sel.borrow_mut() = (prev_start, prev_start);
        self.set_content(new);
        *self.inner.last_op.borrow_mut() = LastOp::Backspace;
        self.note_caret_activity();
    }

    /// Delete-forward: deletes the selection, else the char after the
    /// caret. Consecutive collapsed delete-forwards coalesce (decision
    /// 206). No-op while composing and at the end with a collapsed
    /// caret.
    pub fn delete_forward(&self) {
        if self.is_composing() {
            return;
        }
        self.ensure_clamped();
        let (a, b) = ordered(*self.inner.sel.borrow());
        if a != b {
            self.push_undo_force();
            let old = self.content_text();
            let mut new = String::with_capacity(old.len() - (b - a));
            new.push_str(&old[..a]);
            new.push_str(&old[b..]);
            *self.inner.caret.borrow_mut() = a;
            *self.inner.sel.borrow_mut() = (a, a);
            self.set_content(new);
            self.break_run();
            self.note_caret_activity();
            return;
        }
        let old = self.content_text();
        if a >= old.len() {
            return;
        }
        let next_end = old[a..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| a + i)
            .unwrap_or(old.len());
        self.push_undo_coalesce(LastOp::DeleteForward);
        let mut new = String::with_capacity(old.len() - (next_end - a));
        new.push_str(&old[..a]);
        new.push_str(&old[next_end..]);
        self.set_content(new);
        *self.inner.last_op.borrow_mut() = LastOp::DeleteForward;
        self.note_caret_activity();
    }

    pub fn undo(&self) {
        let Some(entry) = self.inner.undo.borrow_mut().pop() else {
            return;
        };
        self.inner.redo.borrow_mut().push(self.snapshot());
        self.restore(entry);
        self.break_run();
        self.note_caret_activity();
    }

    pub fn redo(&self) {
        let Some(entry) = self.inner.redo.borrow_mut().pop() else {
            return;
        };
        self.inner.undo.borrow_mut().push(self.snapshot());
        trim_to_depth(&mut self.inner.undo.borrow_mut());
        self.restore(entry);
        self.break_run();
        self.note_caret_activity();
    }

    /// The current selection's text over the *committed* content: `None`
    /// when collapsed, when composing (the platform owns the field
    /// mid-composition — deliveries arrive as `composition_updated`),
    /// or when empty. Byte-safe (selection clamps first).
    pub fn selected_text(&self) -> Option<String> {
        if self.is_composing() {
            return None;
        }
        self.ensure_clamped();
        let (a, b) = ordered(*self.inner.sel.borrow());
        if a == b {
            return None;
        }
        Some(self.content_text()[a..b].to_string())
    }

    /// Copies the selection into `clipboard`. Returns the copied text,
    /// or `None` (clipboard untouched) when there is no selectable text
    /// (collapsed, composing, empty — see [`EditSession::selected_text`])
    /// or the session backs a masked field (Round 22.1 — cleartext
    /// exfiltration guard, same `Ok(None)` shape, never an error).
    /// Backend write failures propagate as `Err` (e.g. clipboard locked
    /// by another app — retry next frame, never a silent drop).
    pub fn copy_selection_to(
        &self,
        clipboard: &mut dyn Clipboard,
    ) -> Result<Option<String>, ClipboardError> {
        if self.is_masked() {
            return Ok(None);
        }
        let Some(text) = self.selected_text() else {
            return Ok(None);
        };
        clipboard.write_text(&text)?;
        Ok(Some(text))
    }

    /// Cuts the selection into `clipboard` (copy + `delete_selection`).
    /// Returns the cut text, or `None` (clipboard and content untouched)
    /// when there is no selectable text or the session is masked
    /// (Round 22.1 — same guard as copy). Never cuts mid-composition
    /// (decision 207 — the platform owns the field until commit/cancel).
    pub fn cut_selection_to(
        &self,
        clipboard: &mut dyn Clipboard,
    ) -> Result<Option<String>, ClipboardError> {
        if self.is_masked() {
            return Ok(None);
        }
        let Some(text) = self.selected_text() else {
            return Ok(None);
        };
        clipboard.write_text(&text)?;
        self.delete_selection();
        Ok(Some(text))
    }

    /// Pastes `clipboard`'s current text over the selection (via
    /// `insert`, so coalescing/undo rules apply — the paste breaks any
    /// open run first, making it a discrete undo entry). Empty or
    /// absent clipboard text is a quiet `Ok(PasteOutcome::Empty)` (no
    /// undo entry); a still-pending async read is `Err(Pending)` (retry
    /// next frame, never block); backend failures propagate. Pasting
    /// mid-composition is `Ok(PasteOutcome::WhileComposing)` (the
    /// platform owns the field — same rule as `insert`).
    pub fn paste_from(
        &self,
        clipboard: &mut dyn Clipboard,
    ) -> Result<PasteOutcome, ClipboardError> {
        let text = clipboard.read_text_now()?;
        let Some(text) = text else {
            return Ok(PasteOutcome::Empty);
        };
        if text.is_empty() {
            return Ok(PasteOutcome::Empty);
        }
        if self.is_composing() {
            return Ok(PasteOutcome::WhileComposing);
        }
        self.break_run();
        self.insert(&text);
        Ok(PasteOutcome::Pasted(text))
    }

    pub(crate) fn snapshot(&self) -> EditSnapshot {
        self.ensure_clamped();
        EditSnapshot {
            content: self.content_text(),
            caret: self.composite_caret_byte(),
            sel: *self.inner.sel.borrow(),
        }
    }

    /// Unconditional push (discrete edits, composition starts,
    /// platform feeds). Clears redo (a new edit branches).
    pub(crate) fn push_undo_force(&self) {
        let entry = self.snapshot();
        self.inner.undo.borrow_mut().push(entry);
        trim_to_depth(&mut self.inner.undo.borrow_mut());
        self.inner.redo.borrow_mut().clear();
    }

    /// Coalescing push (decision 206): continues the open run of `class`
    /// without pushing (the run's first snapshot stays the unit), else
    /// pushes. Never clears redo on the coalesced path (the run's start
    /// already branched).
    pub(crate) fn push_undo_coalesce(&self, class: LastOp) {
        if *self.inner.last_op.borrow() == class {
            return;
        }
        self.push_undo_force();
    }

    pub(crate) fn break_run(&self) {
        *self.inner.last_op.borrow_mut() = LastOp::Other;
    }

    pub(crate) fn restore(&self, entry: EditSnapshot) {
        *self.inner.composition.borrow_mut() = None;
        *self.inner.caret.borrow_mut() = clamp_byte(&entry.content, entry.caret);
        let sel = (
            clamp_byte(&entry.content, entry.sel.0),
            clamp_byte(&entry.content, entry.sel.1),
        );
        *self.inner.sel.borrow_mut() = sel;
        self.set_content(entry.content);
    }

    pub(crate) fn set_content(&self, content: String) {
        let shared = SharedString::from(content.as_str());
        self.inner.content.set(shared.clone());
        *self.inner.shaped_cache.borrow_mut() = None;
        // Content mutations collapse any shift gesture (the new
        // selection is whatever the mutating op sets after this).
        self.clear_caret_anchors();
        self.inner.rt.request_frame();
        if let Some(notify) = self.inner.on_change.borrow().clone() {
            notify(shared);
        }
    }

    /// Clamps caret/selection onto char boundaries of the current
    /// composite (the `InputEvent::Text` feed can move content under the
    /// session; the stream is level-triggered and self-heals — decision
    /// 208).
    pub(crate) fn ensure_clamped(&self) {
        let composite = self.composite_text();
        let mut caret = self.composite_caret_byte();
        caret = clamp_byte(&composite, caret);
        let sel = *self.inner.sel.borrow();
        let sel = (clamp_byte(&composite, sel.0), clamp_byte(&composite, sel.1));
        let composing = self.is_composing();
        if composing {
            if let Some(c) = self.inner.composition.borrow_mut().as_mut() {
                c.caret = caret.saturating_sub(c.start).min(c.text.len());
                // `c.text` is platform-supplied; keep the caret within it.
                c.caret = clamp_byte(&c.text, c.caret);
            }
            // Stored caret stays in composite coordinates while composing
            // (spike parity — readers use `composite_caret_byte`).
            *self.inner.caret.borrow_mut() = caret;
        } else {
            *self.inner.caret.borrow_mut() = caret;
        }
        *self.inner.sel.borrow_mut() = sel;
    }
}
