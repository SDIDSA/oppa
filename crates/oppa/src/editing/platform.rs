// ---------------------------------------------------------------------------
// Platform feeds, focus policy, and IME surface
// ---------------------------------------------------------------------------

use super::caret::{clamp_byte, ordered};
use super::*;

impl EditSession {
    /// Applies a platform-owned full value (U8 `InputEvent::Text`
    /// feed, Phase 39a decision 377): replaces content wholesale
    /// through the committing funnel, so `on_change` reports exactly
    /// like a native insert (round-5.4 parity — the two commit
    /// paths each report once per commit; controls subscribe once,
    /// never per path). No undo push (undo is browser-owned
    /// wherever this feed runs — verdict (b)); caret/selection
    /// collapse to the end (the feed carries no caret — decision
    /// 188's shape, clamped like every other mutation).
    pub fn apply_platform_value(&self, value: &str) {
        self.set_content(value.to_string());
        let end = value.len();
        *self.inner.caret.borrow_mut() = end;
        *self.inner.sel.borrow_mut() = (end, end);
        self.ensure_clamped();
        self.note_caret_activity();
    }

    /// Focus-loss policy (locked #27, decision 208): commits an active
    /// composition with its current text; no-op when not composing.
    /// The host calls this on every real focus change; shells with
    /// external focus management call `notify_edit_focus_lost`.
    pub fn notify_focus_lost(&self) {
        let text = self.composition_text();
        if !self.is_composing() {
            return;
        }
        self.commit_active(&text);
    }

    /// Programmatic full-value feed (decision 208): replaces the content
    /// wholesale (the `InputEvent::Text` level-triggered shape), drops
    /// any active composition buffer (the platform value supersedes it),
    /// collapses caret/selection to the end. One undo entry; no-op when
    /// the value already matches and no composition is active.
    pub fn apply_platform_text(&self, value: &str) {
        if !self.is_composing() && self.content_text() == value {
            return;
        }
        self.push_undo_force();
        *self.inner.composition.borrow_mut() = None;
        let end = value.len();
        // `value` comes from the platform; floor to a boundary defensively.
        let end = clamp_byte(value, end);
        *self.inner.caret.borrow_mut() = end;
        *self.inner.sel.borrow_mut() = (end, end);
        self.set_content(value.to_string());
        self.break_run();
        self.note_caret_activity();
    }

    /// Commits the active composition buffer with `committed` (`&self`
    /// so both the `&mut` trait entry and the shared-handle
    /// `notify_focus_lost` use it). No-op when not composing.
    pub(crate) fn commit_active(&self, committed: &str) {
        let taken = self.inner.composition.borrow_mut().take();
        let Some(c) = taken else {
            return;
        };
        // No undo push: the pre-composition snapshot taken at
        // `composition_started` is the atomic unit (decision 206).
        // A new edit still branches: redo clears.
        self.inner.redo.borrow_mut().clear();
        let old = self.content_text();
        let start = clamp_byte(&old, c.start);
        let mut new = String::with_capacity(old.len() + committed.len());
        new.push_str(&old[..start]);
        new.push_str(committed);
        new.push_str(&old[start..]);
        let caret = start + committed.len();
        *self.inner.caret.borrow_mut() = caret;
        *self.inner.sel.borrow_mut() = (caret, caret);
        self.set_content(new);
        self.break_run();
        self.note_caret_activity();
    }
}

fn c_start(inner: &SessionInner) -> usize {
    inner
        .composition
        .borrow()
        .as_ref()
        .map(|c| c.start)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// ImeCompositionHandler — the session as the core-side IME sink (spike
// parity, except focus-loss, which commits per locked #27 instead of the
// spike's pre-merge cancel).
// ---------------------------------------------------------------------------

impl ImeCompositionHandler for EditSession {
    fn composition_started(&mut self, start_byte: usize) {
        // Atomic-unit undo: snapshot the pre-composition state.
        let content = self.content_text();
        let start = clamp_byte(&content, start_byte);
        self.push_undo_force();
        *self.inner.composition.borrow_mut() = Some(Composition {
            start,
            text: String::new(),
            caret: 0,
        });
        *self.inner.caret.borrow_mut() = start;
        *self.inner.sel.borrow_mut() = (start, start);
        self.break_run();
        self.note_caret_activity();
        self.bump_composition_epoch();
    }

    fn composition_updated(&mut self, composition: &str, caret_byte: usize) {
        if !self.is_composing() {
            // An update with no begin: tolerate (platforms race), anchor
            // at the delivered composite caret position (spike parity).
            self.composition_started(caret_byte.saturating_sub(composition.len()));
        }
        let start = c_start(&self.inner);
        if let Some(c) = self.inner.composition.borrow_mut().as_mut() {
            c.text = composition.to_string();
            c.caret = clamp_byte(&c.text, caret_byte.saturating_sub(start));
        }
        let end = start + composition.len();
        let caret = clamp_byte(&self.composite_text(), caret_byte.min(end));
        *self.inner.caret.borrow_mut() = caret;
        *self.inner.sel.borrow_mut() = (caret, caret);
        *self.inner.shaped_cache.borrow_mut() = None;
        self.break_run();
        self.note_caret_activity();
        self.bump_composition_epoch();
    }

    fn composition_committed(&mut self, committed: &str) {
        if self.is_composing() {
            self.commit_active(committed);
        } else {
            self.insert(committed);
        }
    }

    fn composition_cancelled(&mut self) {
        let taken = self.inner.composition.borrow_mut().take();
        if let Some(c) = taken {
            *self.inner.caret.borrow_mut() = c.start;
            *self.inner.sel.borrow_mut() = (c.start, c.start);
            self.note_caret_activity();
            self.bump_composition_epoch();
        }
        self.break_run();
    }

    fn delete_range(&mut self, range: (usize, usize)) {
        let (a, b) = ordered(range);
        let old = self.content_text();
        let a = clamp_byte(&old, a);
        let b = clamp_byte(&old, b).max(a);
        self.push_undo_force();
        let removed = b - a;
        let mut new = String::with_capacity(old.len() - removed);
        new.push_str(&old[..a]);
        new.push_str(&old[b..]);
        // Re-anchor any active composition so its start tracks content.
        if let Some(c) = self.inner.composition.borrow_mut().as_mut() {
            if c.start >= b {
                c.start -= removed;
            } else if c.start > a {
                c.start = a;
            }
            let start = c.start;
            *self.inner.caret.borrow_mut() = start;
        } else {
            *self.inner.caret.borrow_mut() = a;
            *self.inner.sel.borrow_mut() = (a, a);
        }
        self.set_content(new);
        self.break_run();
        self.note_caret_activity();
    }
}
