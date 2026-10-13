// ---------------------------------------------------------------------------
// Session state, lifecycle, and readers
// ---------------------------------------------------------------------------

use super::*;

/// Undo/redo depth per session (decision 206): 32 entries each. A reasoned
/// bound (an editing burst rarely needs more; each entry holds one full
/// pre-edit string), not a derived law — override by editing this const
/// with a new decision, not silently.
pub const EDIT_UNDO_DEPTH: usize = 32;

/// Caret blink full-cycle period in seconds (Round 15.1, decision
/// 312): the caret is visible for the first half of each cycle and
/// hidden for the second (500ms on / 500ms off at the default). Any
/// caret or content mutation resets the phase to solid-visible (see
/// [`EditSession::note_caret_activity`]). A reasoned constant (the
/// platform caret convention), not a derived law — override by
/// editing this const with a new decision, not silently.
pub const CARET_BLINK_PERIOD_SECS: f64 = 1.0;

/// One undo/redo entry: the full pre-edit observable state.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EditSnapshot {
    pub(crate) content: String,
    pub(crate) caret: usize,
    pub(crate) sel: (usize, usize),
}

pub(crate) struct Composition {
    /// Anchor byte in the committed content.
    pub(crate) start: usize,
    pub(crate) text: String,
    /// Caret byte within the composition string.
    pub(crate) caret: usize,
}

/// Coalescing class of the last mutating op (decision 206).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum LastOp {
    /// No run open (fresh session, or broken by another op/undo/focus).
    Other,
    Insert,
    Backspace,
    DeleteForward,
}

/// Observable session state (spike-parity shape).
#[derive(Clone, Debug, PartialEq)]
pub struct EditState {
    pub content: String,
    pub caret: usize,
    pub sel: (usize, usize),
    pub composition: String,
}

/// Paste result (G3, decision 211): every no-op spells its reason —
/// callers never guess whether the paste landed.
#[derive(Clone, Debug, PartialEq)]
pub enum PasteOutcome {
    /// `insert`ed; the string is what landed (echoed for asserts/logs).
    Pasted(String),
    /// Clipboard empty or absent — content and undo stack untouched.
    Empty,
    /// A composition was active — content and clipboard untouched (the
    /// platform owns the field until commit/cancel, decision 207).
    WhileComposing,
}

pub(crate) struct SessionInner {
    pub(crate) rt: Runtime,
    pub(crate) content: Signal<SharedString>,
    pub(crate) caret: RefCell<usize>,
    pub(crate) sel: RefCell<(usize, usize)>,
    pub(crate) composition: RefCell<Option<Composition>>,
    pub(crate) undo: RefCell<Vec<EditSnapshot>>,
    pub(crate) redo: RefCell<Vec<EditSnapshot>>,
    pub(crate) last_op: RefCell<LastOp>,
    pub(crate) shaper: RefCell<Option<(Rc<dyn TextService>, TextStyle)>>,
    pub(crate) shaped_cache: RefCell<Option<(String, ShapedRun)>>,
    /// Caret blink phase epoch in clock seconds (Round 15.1, decision
    /// 312): the last moment any caret or content mutation reset the
    /// blink to solid-visible. Read against the session runtime's
    /// clock (the host clock — sessions are born from it, so phases
    /// agree with the frame timeline by construction).
    pub(crate) caret_epoch: Cell<f64>,
    /// Committed-content observer (round 5.4, OQ-G2-4 — uncontrolled
    /// fields): fired from `set_content` (the single mutation
    /// funnel — inserts, deletes, commits, undo/redo all land
    /// there; live composition does not touch content, so it never
    /// fires mid-composition). Re-entrant sets from inside the
    /// callback would recurse — observers must not write back
    /// (documented, not guarded: a guard would silently drop a
    /// legitimate second edit).
    pub(crate) on_change: RefCell<Option<ContentObserver>>,
    /// Masked (password) backing (Round 22.1, decision 331):
    /// published per render by `TextInput` from `masked` (default
    /// false). Copy/cut refuse while set (cleartext exfiltration
    /// guard — `Ok(None)`, the collapsed-selection shape); paste
    /// still lands (no exfiltration involved).
    pub(crate) masked: Cell<bool>,
    /// Shift-extension anchor (Round 22.1, decision 331): the fixed
    /// end of a keyboard selection while Shift extends it (`None`
    /// outside a shift gesture). Plain moves, pointer ops, content
    /// mutations, and select-all (re)seed it; the extend ops keep
    /// it — Shift+arrows accumulate natively instead of
    /// re-anchoring every press.
    pub(crate) shift_anchor: RefCell<Option<usize>>,
    /// Composition epoch (decision 461): bumped on every
    /// composition-state change that carries no content signal
    /// (begin/update/cancel — commits re-render through
    /// `on_change`). Controls read it while rendering so live
    /// preedit re-renders inline like committed text; sessions
    /// that never compose never bump it, so idle fields render
    /// exactly once.
    pub(crate) composition_epoch: Signal<u64>,
    /// 332): `None` = single-line; `Some(w)` = multi-line with
    /// wrap width `w` (published per render by `TextArea` as
    /// `width - padding`; `TextInput` never sets it). Gates the
    /// visual-line ops below and their loop routing.
    pub(crate) wrap_width: Cell<Option<f32>>,
    /// Preferred x for vertical caret travel, device px,
    /// composite-relative (Round 22.2): set from the caret's x on
    /// the first of a run of line moves and preserved across them
    /// (short lines clamp the caret but never the anchor —
    /// column affinity); cleared by any horizontal caret motion
    /// or content mutation.
    pub(crate) preferred_x: Cell<Option<f32>>,
}

/// Committed-content observer: fired with the new content on every
/// committed session mutation (round 5.4 — the `set_on_change`
/// payload; `Rc` so controls share it into closures).
pub type ContentObserver = Rc<dyn Fn(SharedString)>;

/// The product editing session: a cloneable handle over one field's
/// core-side editing state. `Clone` shares the state (handles are cheap);
/// per-instance identity comes from [`Ctx::edit_session`](crate::component::Ctx::edit_session).
#[derive(Clone)]
pub struct EditSession {
    pub(crate) inner: Rc<SessionInner>,
}

impl EditSession {
    /// Creates a session over an author-owned content signal. Prefer
    /// [`Ctx::edit_session`](crate::component::Ctx::edit_session), which
    /// keys the session to the component instance; direct construction is
    /// the headless/test path.
    pub fn new(rt: Runtime, content: Signal<SharedString>) -> Self {
        let epoch = rt.now_secs();
        let composition_epoch = rt.signal(0u64);
        Self {
            inner: Rc::new(SessionInner {
                rt,
                content,
                caret: RefCell::new(0),
                sel: RefCell::new((0, 0)),
                composition: RefCell::new(None),
                composition_epoch,
                undo: RefCell::new(Vec::new()),
                redo: RefCell::new(Vec::new()),
                last_op: RefCell::new(LastOp::Other),
                shaper: RefCell::new(None),
                shaped_cache: RefCell::new(None),
                caret_epoch: Cell::new(epoch),
                on_change: RefCell::new(None),
                masked: Cell::new(false),
                shift_anchor: RefCell::new(None),
                wrap_width: Cell::new(None),
                preferred_x: Cell::new(None),
            }),
        }
    }

    /// Installs the committed-content observer (round 5.4 —
    /// uncontrolled fields report through this; replaces any
    /// previous observer, last writer wins like every other
    /// session install).
    pub fn set_on_change(&self, f: ContentObserver) {
        *self.inner.on_change.borrow_mut() = Some(f);
    }

    /// Installs the shaper used for pointer-mapped geometry (hit-testing,
    /// caret rects). The host text service + the field's text style.
    pub fn set_shaper(&self, service: Rc<dyn TextService>, style: TextStyle) {
        *self.inner.shaper.borrow_mut() = Some((service, style));
        *self.inner.shaped_cache.borrow_mut() = None;
    }

    /// Removes the shaper: pointer-mapped ops go back to graceful no-ops.
    pub fn clear_shaper(&self) {
        *self.inner.shaper.borrow_mut() = None;
        *self.inner.shaped_cache.borrow_mut() = None;
    }

    /// The author-owned content signal (the binding target for
    /// `bind_edit_session` / `InputEvent::Text` feeds).
    pub fn content_signal(&self) -> Signal<SharedString> {
        self.inner.content.clone()
    }

    /// Publishes masked (password) backing (Round 22.1, decision
    /// 331): `TextInput` writes its `masked` prop here every render
    /// (default false). Copy/cut refuse while set.
    pub fn set_masked(&self, masked: bool) {
        self.inner.masked.set(masked);
    }

    /// Whether this session backs a masked field (see
    /// [`EditSession::set_masked`]).
    pub fn is_masked(&self) -> bool {
        self.inner.masked.get()
    }

    // -- state observables -------------------------------------------------

    /// Committed content text (without the in-progress composition).
    pub fn content_text(&self) -> String {
        self.inner.content.get().to_string()
    }

    /// Full composite: committed content with the active composition
    /// spliced at its anchor.
    pub fn composite_text(&self) -> String {
        let content = self.inner.content.get();
        match self.inner.composition.borrow().as_ref() {
            None => content.to_string(),
            Some(c) => {
                let start = c.start.min(content.len());
                let mut out = String::with_capacity(content.len() + c.text.len());
                out.push_str(&content[..start]);
                out.push_str(&c.text);
                out.push_str(&content[start..]);
                out
            }
        }
    }

    /// Display text: what the laid leaf actually shows. Masked fields
    /// render one bullet per composite char (decision 331 — the leaf
    /// binds `display_text`, never content), so pointer-mapped
    /// geometry must shape THIS, not the composite: cleartext byte
    /// offsets never line up with bullet bytes (one bullet is 3
    /// bytes) and cleartext advances never match bullet advances.
    /// Unmasked sessions return the composite unchanged.
    pub fn display_text(&self) -> String {
        if !self.is_masked() {
            return self.composite_text();
        }
        "•".repeat(self.composite_text().chars().count())
    }

    /// Display byte → composite byte (decision 458): char-index
    /// mapping (masking is 1 char per char, composition included, so
    /// both spaces share char ordinals). Identity when unmasked.
    /// Out-of-range/mid-char inputs floor onto boundaries — platform
    /// offsets never panic.
    pub fn display_byte_to_composite(&self, byte: usize) -> usize {
        if !self.is_masked() {
            return caret::clamp_byte(&self.composite_text(), byte);
        }
        let display = self.display_text();
        let byte = caret::clamp_byte(&display, byte);
        let idx = display[..byte].chars().count();
        self.composite_text()
            .char_indices()
            .nth(idx)
            .map(|(b, _)| b)
            .unwrap_or_else(|| self.composite_text().len())
    }

    /// Composite byte → display byte (decision 458): the reverse
    /// mapping for host paint queries resolving caret/handles against
    /// laid (display) clusters. Identity when unmasked.
    pub fn composite_byte_to_display(&self, byte: usize) -> usize {
        if !self.is_masked() {
            return caret::clamp_byte(&self.composite_text(), byte);
        }
        let composite = self.composite_text();
        let byte = caret::clamp_byte(&composite, byte);
        let idx = composite[..byte].chars().count();
        self.display_text()
            .char_indices()
            .nth(idx)
            .map(|(b, _)| b)
            .unwrap_or_else(|| self.display_text().len())
    }

    /// Caret byte in composite coordinates.
    pub fn composite_caret_byte(&self) -> usize {
        match self.inner.composition.borrow().as_ref() {
            None => *self.inner.caret.borrow(),
            Some(c) => c.start + c.caret,
        }
    }

    pub fn caret(&self) -> usize {
        self.ensure_clamped();
        self.composite_caret_byte()
    }

    pub fn selection(&self) -> (usize, usize) {
        self.ensure_clamped();
        *self.inner.sel.borrow()
    }

    /// Resets the caret blink phase to solid-visible (Round 15.1,
    /// decision 312): every caret or content mutation calls this, so
    /// typing, cursor movement, and taps always show the caret
    /// immediately instead of catching a hidden half-cycle.
    /// Untracked (never schedules — callers already demand frames).
    pub fn note_caret_activity(&self) {
        self.inner.caret_epoch.set(self.inner.rt.now_secs());
    }

    /// True while the caret blink phase is visible (Round 15.1,
    /// decision 312): the first half of every
    /// [`CARET_BLINK_PERIOD_SECS`] cycle after the last
    /// [`note_caret_activity`](Self::note_caret_activity). The host's
    /// `focused_caret_paint` gates emission on this, so every
    /// presenter blinks from one clock by construction.
    pub fn caret_visible(&self) -> bool {
        let dt = self.inner.rt.now_secs() - self.inner.caret_epoch.get();
        dt.rem_euclid(CARET_BLINK_PERIOD_SECS) < CARET_BLINK_PERIOD_SECS * 0.5
    }

    /// Last blink-reset time in clock seconds (diagnostics/tests).
    pub fn caret_epoch(&self) -> f64 {
        self.inner.caret_epoch.get()
    }

    pub fn composition_text(&self) -> String {
        self.inner
            .composition
            .borrow()
            .as_ref()
            .map(|c| c.text.clone())
            .unwrap_or_default()
    }

    /// The active composition's anchor byte in the committed content
    /// (`None` when no composition is active). The real-IME mapper uses
    /// it to convert composition-relative carets into composite
    /// coordinates (spike parity).
    pub fn composition_start_byte(&self) -> Option<usize> {
        self.inner.composition.borrow().as_ref().map(|c| c.start)
    }

    pub fn is_composing(&self) -> bool {
        self.inner.composition.borrow().is_some()
    }

    /// Composition epoch handle (decision 461): controls read (and
    /// subscribe to) this while rendering so live preedit shows
    /// inline. Bumped on begin/update/cancel — commits re-render
    /// through `on_change`, so they never bump.
    pub fn composition_epoch(&self) -> Signal<u64> {
        self.inner.composition_epoch.clone()
    }

    /// Bumps the composition epoch (decision 461 — see
    /// [`EditSession::composition_epoch`]).
    pub(crate) fn bump_composition_epoch(&self) {
        let next = self.inner.composition_epoch.get() + 1;
        self.inner.composition_epoch.set(next);
    }

    pub fn observable(&self) -> EditState {
        self.ensure_clamped();
        EditState {
            content: self.content_text(),
            caret: self.composite_caret_byte(),
            sel: *self.inner.sel.borrow(),
            composition: self.composition_text(),
        }
    }

    /// Undo stack depth (diagnostics/tests).
    pub fn undo_depth(&self) -> usize {
        self.inner.undo.borrow().len()
    }

    /// Redo stack depth (diagnostics/tests).
    pub fn redo_depth(&self) -> usize {
        self.inner.redo.borrow().len()
    }

    /// The candidate-window anchor for the current state, shaped through
    /// the installed shaper (device px). `None` for empty composites or
    /// with no shaper installed (decision 207). Masked sessions shape
    /// the display text (decision 458 — the laid leaf shows bullets).
    pub fn caret_rect(&self) -> Option<CaretRect> {
        let text = self.display_text();
        if text.is_empty() {
            return None;
        }
        let run = self.shape_cached(&text)?;
        let caret = match self.is_masked() {
            true => self.composite_byte_to_display(self.composite_caret_byte()),
            false => self.composite_caret_byte(),
        };
        Some(run.caret_rect(caret))
    }

    /// Leading-edge x (device px, display-relative) of char `index` in
    /// the current composite. `0.0` for empty composites or with no
    /// shaper installed (spike parity, decision 207). Masked sessions
    /// resolve through the display text (decision 458).
    pub fn cluster_leading_x(&self, index: usize) -> f32 {
        let text = self.display_text();
        if text.is_empty() {
            return 0.0;
        }
        let Some(run) = self.shape_cached(&text) else {
            return 0.0;
        };
        let byte = text
            .char_indices()
            .nth(index)
            .map(|(b, _)| b)
            .unwrap_or(text.len());
        run.caret_x(byte)
    }
}
