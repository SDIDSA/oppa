//! Product editing sessions (G1 — decisions 205–208).
//!
//! Ports the spike's `EditingSession` (`crates/spike-textedit`, experiment,
//! not product) into the framework as [`EditSession`]: a cloneable,
//! per-instance handle over author-owned content plus core-side
//! caret/selection/composition/undo state.
//!
//! Design (see decisions 205–208; rationale in git history):
//!
//! - **Residence (205).** Content is an author-owned
//!   [`Signal`](crate::reactive::Signal)`<SharedString>` (the controlled
//!   pattern, locked #24). Everything else (caret, selection, composition
//!   buffer, undo/redo stacks) is core-side, keyed per component instance
//!   through [`Ctx::edit_session`](crate::component::Ctx::edit_session)
//!   under the same call-site source-hash + ordinal rule as
//!   `ctx.signal` (a body edit inserting a session above shifts later
//!   sites → re-seed, never shuffle, §5.1). Instance records live
//!   host-side, so sessions survive hot swaps (only props drain, §5.3).
//!   The first run's content signal wins (same init rule as
//!   `ctx.signal`).
//! - **Undo (206).** Bounded multi-level undo + redo, depth
//!   [`EDIT_UNDO_DEPTH`] (32) each. Contiguous `insert` runs coalesce
//!   into one entry (collapsed selection, no intervening op);
//!   consecutive `backspace` runs and consecutive `delete_forward` runs
//!   coalesce the same way; composition commits are atomic (the
//!   pre-composition snapshot is the unit); any other op breaks the run;
//!   a new edit clears redo. The spike's single-level behavior is the
//!   special case depth-1 of this discipline.
//! - **Shaping (207).** Pointer-mapped ops (`click_x`, `shift_click_x`,
//!   `drag_x`, `dbl_click_x`, `select_line_x`, `cluster_leading_x`,
//!   `caret_rect`) shape
//!   the current composite through an installed shaper
//!   ([`EditSession::set_shaper`]). With no shaper installed they are
//!   graceful no-ops (`None` / `0.0` / unchanged selection): a headless
//!   session without a text service is setup state, not a wiring bug.
//!   With a shaper installed, a shaping *failure* panics loudly — the
//!   backend contract says non-empty text shapes. Char-boundary ops
//!   (caret moves, select-all, insert, deletes, undo/redo) always work.
//!   Every byte offset is clamped to a char boundary (floored) against
//!   the current composite, so platform-supplied offsets can never
//!   panic a slice.
//! - **Focus/feed wiring (208).** [`EditSession::notify_focus_lost`]
//!   commits an active composition with its current text (locked #27 —
//!   this corrects the spike's pre-merge cancel policy, whose
//!   `focus_loss_cancels_per_session_policy` test is now stale
//!   evidence). The host calls it automatically on every real focus
//!   change ([`ComponentHost::set_focus_node`](crate::component::ComponentHost)
//!   path — additive no-op when no sessions exist) and shells that
//!   manage focus externally call
//!   [`ComponentHost::notify_edit_focus_lost`](crate::component::ComponentHost::notify_edit_focus_lost).
//!   [`EditSession::apply_platform_text`] is the programmatic full-value
//!   feed (pushes one undo entry, collapses to end); the routed
//!   `InputEvent::Text` stream reaches the same content signal through
//!   [`ComponentHost::bind_edit_session`](crate::component::ComponentHost::bind_edit_session)
//!   (thin wrapper over `bind_text`, decision 188).
//!
//! Out of scope (open questions OQ-G1-1..3 in the round entry):
//! coalescing across author-owned raw signal writes (invisible to the
//! session — use the session ops); automatic router focus for
//! handler-less `TextField` nodes (v1 focusable means press-owner,
//! decision 96, so field blur arrives via `None`/external notify, not
//! via focusing the field); CJK dictionary segmentation beyond the
//! spike's per-character rule.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::clipboard::{Clipboard, ClipboardError};
use crate::ime::ImeCompositionHandler;
use crate::reactive::{Runtime, Signal};
use crate::text::{CaretRect, ShapedRun, TextService, TextStyle};
use crate::vnode::SharedString;

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
struct EditSnapshot {
    content: String,
    caret: usize,
    sel: (usize, usize),
}

struct Composition {
    /// Anchor byte in the committed content.
    start: usize,
    text: String,
    /// Caret byte within the composition string.
    caret: usize,
}

/// Coalescing class of the last mutating op (decision 206).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LastOp {
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

struct SessionInner {
    rt: Runtime,
    content: Signal<SharedString>,
    caret: RefCell<usize>,
    sel: RefCell<(usize, usize)>,
    composition: RefCell<Option<Composition>>,
    undo: RefCell<Vec<EditSnapshot>>,
    redo: RefCell<Vec<EditSnapshot>>,
    last_op: RefCell<LastOp>,
    shaper: RefCell<Option<(Rc<dyn TextService>, TextStyle)>>,
    shaped_cache: RefCell<Option<(String, ShapedRun)>>,
    /// Caret blink phase epoch in clock seconds (Round 15.1, decision
    /// 312): the last moment any caret or content mutation reset the
    /// blink to solid-visible. Read against the session runtime's
    /// clock (the host clock — sessions are born from it, so phases
    /// agree with the frame timeline by construction).
    caret_epoch: Cell<f64>,
    /// Committed-content observer (round 5.4, OQ-G2-4 — uncontrolled
    /// fields): fired from `set_content` (the single mutation
    /// funnel — inserts, deletes, commits, undo/redo all land
    /// there; live composition does not touch content, so it never
    /// fires mid-composition). Re-entrant sets from inside the
    /// callback would recurse — observers must not write back
    /// (documented, not guarded: a guard would silently drop a
    /// legitimate second edit).
    on_change: RefCell<Option<ContentObserver>>,
    /// Masked (password) backing (Round 22.1, decision 331):
    /// published per render by `TextInput` from `masked` (default
    /// false). Copy/cut refuse while set (cleartext exfiltration
    /// guard — `Ok(None)`, the collapsed-selection shape); paste
    /// still lands (no exfiltration involved).
    masked: Cell<bool>,
    /// Shift-extension anchor (Round 22.1, decision 331): the fixed
    /// end of a keyboard selection while Shift extends it (`None`
    /// outside a shift gesture). Plain moves, pointer ops, content
    /// mutations, and select-all (re)seed it; the extend ops keep
    /// it — Shift+arrows accumulate natively instead of
    /// re-anchoring every press.
    shift_anchor: RefCell<Option<usize>>,
    /// Multi-line wrap width in device px (Round 22.2, decision
    /// 332): `None` = single-line; `Some(w)` = multi-line with
    /// wrap width `w` (published per render by `TextArea` as
    /// `width - padding`; `TextInput` never sets it). Gates the
    /// visual-line ops below and their loop routing.
    wrap_width: Cell<Option<f32>>,
    /// Preferred x for vertical caret travel, device px,
    /// composite-relative (Round 22.2): set from the caret's x on
    /// the first of a run of line moves and preserved across them
    /// (short lines clamp the caret but never the anchor —
    /// column affinity); cleared by any horizontal caret motion
    /// or content mutation.
    preferred_x: Cell<Option<f32>>,
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
    inner: Rc<SessionInner>,
}

impl EditSession {
    /// Creates a session over an author-owned content signal. Prefer
    /// [`Ctx::edit_session`](crate::component::Ctx::edit_session), which
    /// keys the session to the component instance; direct construction is
    /// the headless/test path.
    pub fn new(rt: Runtime, content: Signal<SharedString>) -> Self {
        let epoch = rt.now_secs();
        Self {
            inner: Rc::new(SessionInner {
                rt,
                content,
                caret: RefCell::new(0),
                sel: RefCell::new((0, 0)),
                composition: RefCell::new(None),
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
    /// with no shaper installed (decision 207).
    pub fn caret_rect(&self) -> Option<CaretRect> {
        let composite = self.composite_text();
        if composite.is_empty() {
            return None;
        }
        let run = self.shape_cached(&composite)?;
        Some(run.caret_rect(self.composite_caret_byte()))
    }

    /// Leading-edge x (device px, composite-relative) of char `index` in
    /// the current composite. `0.0` for empty composites or with no
    /// shaper installed (spike parity, decision 207).
    pub fn cluster_leading_x(&self, index: usize) -> f32 {
        let composite = self.composite_text();
        if composite.is_empty() {
            return 0.0;
        }
        let Some(run) = self.shape_cached(&composite) else {
            return 0.0;
        };
        let byte = composite
            .char_indices()
            .nth(index)
            .map(|(b, _)| b)
            .unwrap_or(composite.len());
        run.caret_x(byte)
    }

    // -- pointer/selection ops ----------------------------------------------

    pub fn click_x(&self, x: f32) {
        self.ensure_clamped();
        let Some(byte) = self.hit_test(x) else {
            return;
        };
        *self.inner.caret.borrow_mut() = byte;
        *self.inner.sel.borrow_mut() = (byte, byte);
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
        self.inner.rt.request_frame();
    }

    pub fn shift_click_x(&self, x: f32) {
        self.ensure_clamped();
        let Some(byte) = self.hit_test(x) else {
            return;
        };
        // Keyboard anchor survives into the click (native
        // Shift+Click extends from the selection anchor, then the
        // click re-seeds it).
        let anchor = self.extend_anchor();
        *self.inner.sel.borrow_mut() = if byte >= anchor {
            (anchor, byte)
        } else {
            (byte, anchor)
        };
        *self.inner.caret.borrow_mut() = byte;
        *self.inner.shift_anchor.borrow_mut() = Some(anchor);
        // Horizontal motion resets column affinity (line moves
        // re-seed it — see `preferred_x`).
        self.inner.preferred_x.set(None);
        self.break_run();
        self.note_caret_activity();
        self.inner.rt.request_frame();
    }

    pub fn drag_x(&self, x1: f32, x2: f32) {
        self.ensure_clamped();
        let (Some(b1), Some(b2)) = (self.hit_test(x1), self.hit_test(x2)) else {
            return;
        };
        *self.inner.sel.borrow_mut() = (b1.min(b2), b1.max(b2));
        *self.inner.caret.borrow_mut() = b1.max(b2);
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
        self.inner.rt.request_frame();
    }

    pub fn dbl_click_x(&self, x: f32) {
        self.ensure_clamped();
        let Some(byte) = self.hit_test(x) else {
            return;
        };
        let (a, b) = self.word_range(byte);
        *self.inner.sel.borrow_mut() = (a, b);
        *self.inner.caret.borrow_mut() = b;
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
        self.inner.rt.request_frame();
    }

    /// Line selection for a triple-click (Round 8.2, decision 298): the
    /// hard line (`\n`-delimited; soft wraps stay laid-out flow, not
    /// selection units in v1) containing the hit byte. Single-line
    /// fields select the whole content. No-op with no shaper (the
    /// decision-207 graceful rule — pointer ops never panic setup
    /// state); caret lands at the line end.
    pub fn select_line_x(&self, x: f32) {
        self.ensure_clamped();
        let Some(byte) = self.hit_test(x) else {
            return;
        };
        let text = self.composite_text();
        let start = text[..byte].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let end = text[byte..]
            .find('\n')
            .map(|i| byte + i)
            .unwrap_or(text.len());
        let (start, end) = (clamp_byte(&text, start), clamp_byte(&text, end));
        *self.inner.sel.borrow_mut() = (start, end);
        *self.inner.caret.borrow_mut() = end;
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
        self.inner.rt.request_frame();
    }

    pub fn caret_move(&self, steps: i32) {
        self.ensure_clamped();
        // Collapse-first (native arrows): an open selection
        // collapses toward the step direction before stepping.
        let (a, b) = ordered(*self.inner.sel.borrow());
        if a != b {
            let edge = if steps < 0 { a } else { b };
            *self.inner.caret.borrow_mut() = edge;
            *self.inner.sel.borrow_mut() = (edge, edge);
            self.clear_caret_anchors();
            self.break_run();
            self.note_caret_activity();
            return;
        }
        let byte = self.caret_boundary(steps);
        *self.inner.caret.borrow_mut() = byte;
        *self.inner.sel.borrow_mut() = (byte, byte);
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
    }

    /// Clears the caret anchors (plain horizontal moves, pointer
    /// ops, and content mutations re-seed them — see `shift_anchor`
    /// and `preferred_x`). Line moves manage `shift_anchor`
    /// directly (plain line steps clear it, extends keep it) and
    /// never touch `preferred_x`.
    fn clear_caret_anchors(&self) {
        *self.inner.shift_anchor.borrow_mut() = None;
        self.inner.preferred_x.set(None);
    }

    /// The fixed end of a shift extension (Round 22.1): the live
    /// anchor when a shift gesture runs, else the far end of any
    /// open selection (select-all extends from 0), else the caret.
    fn extend_anchor(&self) -> usize {
        if let Some(a) = *self.inner.shift_anchor.borrow() {
            return a;
        }
        let caret = *self.inner.caret.borrow();
        let (a, b) = ordered(*self.inner.sel.borrow());
        if a == b {
            caret
        } else if caret == a {
            b
        } else {
            a
        }
    }

    /// Shift-extended caret move (Round 22.1 — Shift+Left /
    /// Shift+Right): spans from the shift anchor (persisted across
    /// presses, so selections accumulate natively) to the new
    /// caret.
    pub fn extend_caret(&self, steps: i32) {
        self.ensure_clamped();
        let anchor = self.extend_anchor();
        let byte = self.caret_boundary(steps);
        *self.inner.sel.borrow_mut() = (anchor.min(byte), anchor.max(byte));
        *self.inner.caret.borrow_mut() = byte;
        *self.inner.shift_anchor.borrow_mut() = Some(anchor);
        self.inner.preferred_x.set(None);
        self.break_run();
        self.note_caret_activity();
    }

    /// Word-step caret move (Round 22.1, decision 331 — Ctrl+Left /
    /// Ctrl+Right): `steps` word edges from the caret (mid-word
    /// stops at the word end first, then word starts; CJK steps per
    /// character; separators never land — see
    /// [`EditSession::word_boundary`]). Collapses the selection
    /// like [`EditSession::caret_move`] (open selections collapse
    /// toward the step direction first).
    pub fn word_move(&self, steps: i32) {
        self.ensure_clamped();
        let (a, b) = ordered(*self.inner.sel.borrow());
        if a != b {
            let edge = if steps < 0 { a } else { b };
            *self.inner.caret.borrow_mut() = edge;
            *self.inner.sel.borrow_mut() = (edge, edge);
            self.clear_caret_anchors();
            self.break_run();
            self.note_caret_activity();
            return;
        }
        let byte = self.word_boundary(steps);
        *self.inner.caret.borrow_mut() = byte;
        *self.inner.sel.borrow_mut() = (byte, byte);
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
    }

    /// Word-step selection extension (Round 22.1 — Ctrl+Shift+Left /
    /// Ctrl+Shift+Right): spans from the shift anchor (persisted,
    /// like [`EditSession::extend_caret`]) to the word-stepped
    /// caret.
    pub fn extend_word(&self, steps: i32) {
        self.ensure_clamped();
        let anchor = self.extend_anchor();
        let byte = self.word_boundary(steps);
        *self.inner.sel.borrow_mut() = (anchor.min(byte), anchor.max(byte));
        *self.inner.caret.borrow_mut() = byte;
        *self.inner.shift_anchor.borrow_mut() = Some(anchor);
        self.inner.preferred_x.set(None);
        self.break_run();
        self.note_caret_activity();
    }

    /// Single-step navigation vocabulary (Round 22.1, decision 331 —
    /// the `DesktopLoop` shortcut table names these, one per
    /// key shape, so runners never spell raw step counts).
    pub fn caret_left(&self) {
        self.caret_move(-1);
    }

    pub fn caret_right(&self) {
        self.caret_move(1);
    }

    pub fn extend_left(&self) {
        self.extend_caret(-1);
    }

    pub fn extend_right(&self) {
        self.extend_caret(1);
    }

    pub fn word_left(&self) {
        self.word_move(-1);
    }

    pub fn word_right(&self) {
        self.word_move(1);
    }

    pub fn extend_word_left(&self) {
        self.extend_word(-1);
    }

    pub fn extend_word_right(&self) {
        self.extend_word(1);
    }

    /// Publishes the multi-line wrap width in device px (Round
    /// 22.2, decision 332): `None` (default) = single-line;
    /// `Some(w)` = multi-line wrapping at `w` (`TextArea` publishes
    /// `width - padding` every render). Must be finite and positive
    /// when set (loud panic otherwise — a wrap width is never
    /// zero/NaN, the scene-scale rule).
    pub fn set_wrap_width(&self, width: Option<f32>) {
        if let Some(w) = width {
            assert!(
                w.is_finite() && w > 0.0,
                "set_wrap_width: width must be finite and positive, got {w}"
            );
        }
        self.inner.wrap_width.set(width);
    }

    /// The published wrap width, if multi-line (see
    /// [`EditSession::set_wrap_width`]).
    pub fn wrap_width(&self) -> Option<f32> {
        self.inner.wrap_width.get()
    }

    /// Whether this session navigates visual lines (see
    /// [`EditSession::set_wrap_width`] — the loop routes Up/Down
    /// here only when true).
    pub fn is_multiline(&self) -> bool {
        self.wrap_width().is_some()
    }

    /// Moves the caret one visual line up, preserving the
    /// preferred-x anchor across lines (Round 22.2 — ArrowUp):
    /// short lines clamp the caret but never the anchor. At the
    /// first line the caret goes to the line start. Collapses open
    /// selections toward the step first (the plain-move rule). A
    /// loud no-op without a wrap width (single-line sessions never
    /// route here — the loop guards on `is_multiline`).
    pub fn line_up(&self) {
        self.line_step(-1, false);
    }

    /// Moves the caret one visual line down (Round 22.2 —
    /// ArrowDown): mirror of [`EditSession::line_up` (at the last
    /// line the caret goes to the text end).
    pub fn line_down(&self) {
        self.line_step(1, false);
    }

    /// Extends one visual line up (Round 22.2 — Shift+ArrowUp):
    /// spans from the shift anchor to the anchored line caret.
    pub fn extend_line_up(&self) {
        self.line_step(-1, true);
    }

    /// Extends one visual line down (Round 22.2 — Shift+ArrowDown).
    pub fn extend_line_down(&self) {
        self.line_step(1, true);
    }

    /// Shared vertical step (dir −1 up / +1 down, extend selects):
    /// collapse-first for plain moves, shift-anchor spans for
    /// extends, preferred-x preserved across the run.
    fn line_step(&self, dir: i32, extend: bool) {
        self.ensure_clamped();
        let (a, b) = ordered(*self.inner.sel.borrow());
        if !extend && a != b {
            let edge = if dir < 0 { a } else { b };
            *self.inner.caret.borrow_mut() = edge;
            *self.inner.sel.borrow_mut() = (edge, edge);
            *self.inner.shift_anchor.borrow_mut() = None;
            self.break_run();
            self.note_caret_activity();
            return;
        }
        let caret = *self.inner.caret.borrow();
        let anchor = if extend { self.extend_anchor() } else { caret };
        let x = self
            .inner
            .preferred_x
            .get()
            .unwrap_or_else(|| self.caret_visual_x(caret));
        let byte = self.line_byte(dir, caret, x);
        if extend {
            *self.inner.sel.borrow_mut() = (anchor.min(byte), anchor.max(byte));
            *self.inner.shift_anchor.borrow_mut() = Some(anchor);
        } else {
            *self.inner.sel.borrow_mut() = (byte, byte);
            *self.inner.shift_anchor.borrow_mut() = None;
        }
        *self.inner.caret.borrow_mut() = byte;
        // Preserve the anchor across the run (set once — short
        // lines must not shrink it).
        if self.inner.preferred_x.get().is_none() {
            self.inner.preferred_x.set(Some(x));
        }
        self.break_run();
        self.note_caret_activity();
    }

    /// Caret x in device px, composite-relative (Round 22.2): the
    /// visual-line lookup when shaped, else the composite start
    /// (graceful without a shaper — decision-207 rule).
    fn caret_visual_x(&self, byte: usize) -> f32 {
        let composite = self.composite_text();
        if composite.is_empty() {
            return 0.0;
        }
        let Some(lines) = self.shaped_visual_lines(&composite) else {
            return 0.0;
        };
        let byte = byte.min(composite.len());
        let line = line_containing(&lines, &composite, byte);
        x_in_line(line, byte)
    }

    /// Byte `dir` lines from `caret` at preferred x `x` (Round
    /// 22.2): hard and wrapped lines alike, column affinity by
    /// midpoint. Past the first line goes to the line start, past
    /// the last to the text end. Without a shaper, hard-line
    /// character columns (graceful, decision-207).
    fn line_byte(&self, dir: i32, caret: usize, x: f32) -> usize {
        let composite = self.composite_text();
        if composite.is_empty() {
            return 0;
        }
        let caret = caret.min(composite.len());
        let Some(lines) = self.shaped_visual_lines(&composite) else {
            return hard_line_byte(&composite, caret, dir, x);
        };
        let li = line_index_containing(&lines, &composite, caret);
        if dir < 0 {
            if li == 0 {
                return lines[0].start;
            }
            return byte_at_x(&lines[li - 1], &composite, x);
        }
        if li + 1 >= lines.len() {
            return composite.len();
        }
        byte_at_x(&lines[li + 1], &composite, x)
    }

    /// Visual lines over the composite (Round 22.2): shaped +
    /// wrapped at the published width (UAX #14 opportunities flow
    /// through the shaper path — `layout_text` consumes the
    /// installed break source the way the engine does). `None`
    /// with no wrap width, empty text, or no shaper (callers fall
    /// back to hard lines).
    fn shaped_visual_lines(&self, composite: &str) -> Option<Vec<VisualLine>> {
        let width = self.wrap_width()?;
        if composite.is_empty() {
            return None;
        }
        let run = self.shape_cached(composite)?;
        let metrics = run.runs.first().map(|r| r.font_metrics)?;
        let laid = crate::layout::layout_text(
            &run,
            composite,
            width,
            None,
            metrics.ascent,
            metrics.descent,
            metrics.line_gap,
        );
        // Line starts: each laid line's first cluster (visual
        // order = logical for the LTR wrap scope here; bidi
        // line-edge affinity follows the suite-guided standing
        // item) plus every hard-line start (blank lines own no
        // clusters but still break).
        let mut breaks = vec![0usize];
        for line in &laid {
            if let Some(c) = line.clusters.first() {
                breaks.push(c.byte_range.0);
            }
        }
        let mut s = 0usize;
        for part in composite.split_inclusive('\n') {
            s += part.len();
            // Hard starts break (the trailing length is a span end,
            // never a break — else caret==len strands on a phantom
            // empty span).
            if s < composite.len() {
                breaks.push(s);
            }
        }
        breaks.sort_unstable();
        breaks.dedup();
        // Cluster x-map per span (logical byte order; x is visual
        // device px, composite-relative). Newline clusters map to
        // no span (terminators, not content — past-end clamps to
        // the content end instead).
        let mut all: Vec<(usize, usize, f32, f32)> = Vec::new();
        for line in &laid {
            for c in &line.clusters {
                let (s, e) = c.byte_range;
                if composite.as_bytes()[s.min(composite.len())..e.min(composite.len())]
                    .contains(&b'\n')
                {
                    continue;
                }
                all.push((s, e, c.x, c.width));
            }
        }
        all.sort_by_key(|c| c.0);
        let mut out = Vec::with_capacity(breaks.len());
        for (i, &start) in breaks.iter().enumerate() {
            let end = breaks.get(i + 1).copied().unwrap_or(composite.len());
            let clusters = all
                .iter()
                .copied()
                .filter(|c| c.0 >= start && c.0 < end.max(start + 1))
                .collect();
            out.push(VisualLine {
                start,
                end,
                clusters,
            });
        }
        Some(out)
    }

    /// Word starts + word ends over the current composite (the
    /// Ctrl-step address space, Round 22.1): every `Word`-run start
    /// and end, every `Ideograph` char start and end, plus both text
    /// ends — separators never bound. Returns `(starts, edges)`
    /// (both sorted, deduplicated; `starts` excludes the trailing
    /// edge, which opens no word).
    fn word_edges(&self) -> (Vec<usize>, Vec<usize>) {
        let text = self.composite_text();
        let mut starts = Vec::new();
        let mut edges = vec![0, text.len()];
        let mut run: Option<(usize, WordClass, usize)> = None;
        for (i, c) in text.char_indices() {
            match word_class(c) {
                WordClass::Word => {
                    if !matches!(run, Some((_, WordClass::Word, _))) {
                        // A class change bounds both sides (close the
                        // old run, open the new -- alnum and emoji
                        // never share a unit, UAX #29 breaks them).
                        if let Some((s, _, _)) = run.take() {
                            starts.push(s);
                            edges.push(i);
                        }
                        run = Some((i, WordClass::Word, 0));
                        edges.push(i);
                    }
                }
                WordClass::Regional => {
                    // UAX #29 WB15/WB16: pair from the last non-RI.
                    // An odd trailing count completes the pair (keep
                    // the run); an even count (or any other run)
                    // opens a fresh pair.
                    match run {
                        Some((s, WordClass::Regional, t)) if t % 2 == 1 => {
                            run = Some((s, WordClass::Regional, t + 1));
                        }
                        _ => {
                            if let Some((s, _, _)) = run.take() {
                                starts.push(s);
                                edges.push(i);
                            }
                            run = Some((i, WordClass::Regional, 1));
                            edges.push(i);
                        }
                    }
                }
                WordClass::Emoji => {
                    // ZWJ is transparent inside a regional run
                    // (WB11-class join -- the family sequence stays
                    // one unit); everywhere else the class rule
                    // holds (VS16 included: it breaks RI runs).
                    if c as u32 == 0x200D {
                        if let Some((_, WordClass::Regional, _)) = run {
                            continue;
                        }
                    }
                    if !matches!(run, Some((_, WordClass::Emoji, _))) {
                        if let Some((s, _, _)) = run.take() {
                            starts.push(s);
                            edges.push(i);
                        }
                        run = Some((i, WordClass::Emoji, 0));
                        edges.push(i);
                    }
                }
                WordClass::Ideograph => {
                    if let Some((s, _, _)) = run.take() {
                        starts.push(s);
                        edges.push(i);
                    }
                    starts.push(i);
                    edges.push(i);
                    edges.push(i + c.len_utf8());
                }
                WordClass::Separator => {
                    if let Some((s, _, _)) = run.take() {
                        starts.push(s);
                        edges.push(i);
                    }
                }
            }
        }
        if let Some((s, _, _)) = run {
            starts.push(s);
            edges.push(text.len());
        }
        starts.sort_unstable();
        starts.dedup();
        edges.sort_unstable();
        edges.dedup();
        (starts, edges)
    }

    /// The byte boundary `steps` word edges from the current caret
    /// (positive forward, negative back, clamped at both ends):
    /// Right lands on the next edge (mid-word stops at the word
    /// end first, then the next word start); Left lands on the
    /// previous edge, except from a word start it skips to the
    /// previous word start (the Windows two-phase convention —
    /// separators never land).
    fn word_boundary(&self, steps: i32) -> usize {
        let text = self.composite_text();
        let caret = self.composite_caret_byte().min(text.len());
        let (starts, edges) = self.word_edges();
        let mut pos = caret;
        if steps >= 0 {
            for _ in 0..steps {
                pos = edges
                    .iter()
                    .copied()
                    .find(|&b| b > pos)
                    .unwrap_or(text.len());
            }
        } else {
            for _ in 0..-steps {
                if pos > 0 && pos < text.len() && starts.contains(&pos) {
                    pos = starts.iter().copied().rev().find(|&b| b < pos).unwrap_or(0);
                } else {
                    pos = edges.iter().copied().rev().find(|&b| b < pos).unwrap_or(0);
                }
            }
        }
        pos
    }

    /// The byte boundary `steps` caret positions from the current caret
    /// (char-stepped over the current composite, clamped at both ends).
    fn caret_boundary(&self, steps: i32) -> usize {
        let text = self.composite_text();
        let caret = self.composite_caret_byte().min(text.len());
        // Round 33 (decision 349): step shaped clusters, not
        // scalars -- one press crosses a base+combining-mark
        // cluster (production shapers merge them per the closed
        // combining parity). No shaper degrades to scalar bounds
        // (decision 207 -- never a panic, just finer steps).
        let mut boundaries: Vec<usize> = match self.shape_cached(&text) {
            Some(run) => {
                let mut b: Vec<usize> = run.clusters.iter().map(|c| c.byte_range.0).collect();
                b.sort_unstable();
                b.dedup();
                b
            }
            None => text.char_indices().map(|(b, _)| b).collect(),
        };
        boundaries.push(text.len());
        let idx = boundaries
            .iter()
            .position(|&b| b >= caret)
            .unwrap_or(boundaries.len() - 1);
        let target = (idx as i64 + steps as i64).clamp(0, boundaries.len() as i64 - 1) as usize;
        boundaries[target]
    }

    pub fn caret_to_end(&self) {
        let end = self.composite_text().len();
        *self.inner.caret.borrow_mut() = end;
        *self.inner.sel.borrow_mut() = (end, end);
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
    }

    pub fn caret_to_start(&self) {
        *self.inner.caret.borrow_mut() = 0;
        *self.inner.sel.borrow_mut() = (0, 0);
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
    }

    /// Extends the selection to the content start (Round 22.1 —
    /// Shift+Home): spans from the shift anchor (persisted) to
    /// byte 0, caret leading.
    pub fn extend_to_start(&self) {
        self.ensure_clamped();
        let anchor = self.extend_anchor();
        *self.inner.sel.borrow_mut() = (0, anchor);
        *self.inner.caret.borrow_mut() = 0;
        *self.inner.shift_anchor.borrow_mut() = Some(anchor);
        self.inner.preferred_x.set(None);
        self.break_run();
        self.note_caret_activity();
    }

    /// Extends the selection to the content end (Round 22.1 —
    /// Shift+End): spans from the shift anchor to the trailing
    /// edge, caret leading.
    pub fn extend_to_end(&self) {
        self.ensure_clamped();
        let anchor = self.extend_anchor();
        let end = self.composite_text().len();
        *self.inner.sel.borrow_mut() = (anchor, end);
        *self.inner.caret.borrow_mut() = end;
        *self.inner.shift_anchor.borrow_mut() = Some(anchor);
        self.inner.preferred_x.set(None);
        self.break_run();
        self.note_caret_activity();
    }

    /// Select-all: the selection spans the whole composite, the caret at
    /// its trailing edge (spike parity). Re-seeds the shift anchor
    /// at 0 (a following Shift+arrow extends from the head, native).
    pub fn select_all(&self) {
        let end = self.composite_text().len();
        *self.inner.caret.borrow_mut() = end;
        *self.inner.sel.borrow_mut() = (0, end);
        *self.inner.shift_anchor.borrow_mut() = Some(0);
        self.inner.preferred_x.set(None);
        self.break_run();
        self.note_caret_activity();
    }

    // -- editing ops --------------------------------------------------------

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

    // -- clipboard ops (G3, decision 211) --------------------------------------

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

    // -- undo machinery ------------------------------------------------------

    fn snapshot(&self) -> EditSnapshot {
        self.ensure_clamped();
        EditSnapshot {
            content: self.content_text(),
            caret: self.composite_caret_byte(),
            sel: *self.inner.sel.borrow(),
        }
    }

    /// Unconditional push (discrete edits, composition starts,
    /// platform feeds). Clears redo (a new edit branches).
    fn push_undo_force(&self) {
        let entry = self.snapshot();
        self.inner.undo.borrow_mut().push(entry);
        trim_to_depth(&mut self.inner.undo.borrow_mut());
        self.inner.redo.borrow_mut().clear();
    }

    /// Coalescing push (decision 206): continues the open run of `class`
    /// without pushing (the run's first snapshot stays the unit), else
    /// pushes. Never clears redo on the coalesced path (the run's start
    /// already branched).
    fn push_undo_coalesce(&self, class: LastOp) {
        if *self.inner.last_op.borrow() == class {
            return;
        }
        self.push_undo_force();
    }

    fn break_run(&self) {
        *self.inner.last_op.borrow_mut() = LastOp::Other;
    }

    fn restore(&self, entry: EditSnapshot) {
        *self.inner.composition.borrow_mut() = None;
        *self.inner.caret.borrow_mut() = clamp_byte(&entry.content, entry.caret);
        let sel = (
            clamp_byte(&entry.content, entry.sel.0),
            clamp_byte(&entry.content, entry.sel.1),
        );
        *self.inner.sel.borrow_mut() = sel;
        self.set_content(entry.content);
    }

    fn set_content(&self, content: String) {
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
    fn ensure_clamped(&self) {
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

    // -- geometry helpers ----------------------------------------------------

    /// Shapes `text` through the installed shaper. `None` with no shaper
    /// (graceful — decision 207); panics loudly when an installed shaper
    /// fails on non-empty text (backend contract violation).
    fn shape_cached(&self, text: &str) -> Option<ShapedRun> {
        if let Some((cached, run)) = self.inner.shaped_cache.borrow().as_ref() {
            if cached == text {
                return Some(run.clone());
            }
        }
        let (service, style) = self.inner.shaper.borrow().as_ref().cloned()?;
        let run = service.shape(text, &style).unwrap_or_else(|e| {
            panic!(
                "installed text shaper failed on {text:?}: {e:?} — \
                 non-empty text must shape (backend contract, decision 207)"
            )
        });
        *self.inner.shaped_cache.borrow_mut() = Some((text.to_string(), run.clone()));
        Some(run)
    }

    /// x (device px, composite-relative) → composite byte via the
    /// cluster-midpoint leading-edge rule. `None` with no shaper.
    fn hit_test(&self, x: f32) -> Option<usize> {
        let composite = self.composite_text();
        if composite.is_empty() {
            return Some(0);
        }
        let run = self.shape_cached(&composite)?;
        Some(run.byte_offset_for_x(x))
    }

    /// The spike's word rule (spike parity, OQ-G1-3): contiguous
    /// alphanumeric runs form one word; each CJK ideograph/kana/hangul
    /// character is its own word; everything else is a separator
    /// (double-click selects nothing). Full dictionary segmentation is
    /// a later round.
    fn word_range(&self, byte: usize) -> (usize, usize) {
        let text = self.composite_text();
        if text.is_empty() {
            return (0, 0);
        }
        let byte = clamp_byte(&text, byte);
        let mut char_start = 0usize;
        let mut clicked = '\0';
        for (i, c) in text.char_indices() {
            if i > byte {
                break;
            }
            char_start = i;
            clicked = c;
        }
        match word_class(clicked) {
            WordClass::Separator => (byte, byte),
            WordClass::Ideograph => (char_start, char_start + clicked.len_utf8()),
            WordClass::Word => {
                let mut start = char_start;
                let mut end = char_start + clicked.len_utf8();
                while start > 0 {
                    let Some((prev_start, prev)) = text[..start].char_indices().last() else {
                        break;
                    };
                    if word_class(prev) != WordClass::Word {
                        break;
                    }
                    start = prev_start;
                }
                for c in text[end..].chars() {
                    if word_class(c) != WordClass::Word {
                        break;
                    }
                    end += c.len_utf8();
                }
                (start, end)
            }
            WordClass::Emoji => {
                // Emoji run expansion (Round 31): same walk with the
                // Emoji class, so ZWJ sequences select whole.
                let mut start = char_start;
                let mut end = char_start + clicked.len_utf8();
                while start > 0 {
                    let Some((prev_start, prev)) = text[..start].char_indices().last() else {
                        break;
                    };
                    if word_class(prev) != WordClass::Emoji {
                        break;
                    }
                    start = prev_start;
                }
                for c in text[end..].chars() {
                    if word_class(c) != WordClass::Emoji {
                        break;
                    }
                    end += c.len_utf8();
                }
                (start, end)
            }
            WordClass::Regional => {
                // Pair from the last non-RI (Round 32, WB15/WB16):
                // collect the consecutive run, align the clicked
                // index down to its pair, take two (or the lone
                // tail). A ZWJ glued inside an RI run selects alone
                // here (pathological input -- no real flag sequence
                // joins with ZWJ; edges still step it as one unit).
                let mut run_start = char_start;
                while run_start > 0 {
                    let Some((prev_start, prev)) = text[..run_start].char_indices().last() else {
                        break;
                    };
                    if word_class(prev) != WordClass::Regional {
                        break;
                    }
                    run_start = prev_start;
                }
                let mut ris: Vec<usize> = Vec::new();
                let mut bb = run_start;
                while bb < text.len() {
                    let Some(ch) = text[bb..].chars().next() else {
                        break;
                    };
                    if word_class(ch) != WordClass::Regional {
                        break;
                    }
                    ris.push(bb);
                    bb += ch.len_utf8();
                }
                let k = ris.iter().position(|&x| x == char_start).unwrap_or(0);
                let pair = k & !1;
                let first = ris[pair];
                let first_len = text[first..]
                    .chars()
                    .next()
                    .map(|c| c.len_utf8())
                    .unwrap_or(0);
                let end = match ris.get(pair + 1) {
                    Some(&second) => {
                        second
                            + text[second..]
                                .chars()
                                .next()
                                .map(|c| c.len_utf8())
                                .unwrap_or(0)
                    }
                    None => first + first_len,
                };
                (first, end)
            }
        }
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

fn trim_to_depth(stack: &mut Vec<EditSnapshot>) {
    while stack.len() > EDIT_UNDO_DEPTH {
        stack.remove(0);
    }
}

fn ordered(sel: (usize, usize)) -> (usize, usize) {
    (sel.0.min(sel.1), sel.0.max(sel.1))
}

/// One visual line for caret travel (Round 22.2): byte span plus
/// the logical-order cluster x-map (`(start, end, x, width)` in
/// device px, composite-relative).
#[derive(Clone, Debug, PartialEq)]
struct VisualLine {
    start: usize,
    end: usize,
    clusters: Vec<(usize, usize, f32, f32)>,
}

/// Index of the visual line containing `byte` (Round 22.2): the
/// last span starting at or before it, except a wrap-boundary
/// caret (previous byte is not `\n`) belongs to the ending line
/// (end-of-line affinity); hard starts after `\n` open the next.
/// `byte == len` belongs to the last span.
fn line_index_containing(lines: &[VisualLine], composite: &str, byte: usize) -> usize {
    let byte = byte.min(composite.len());
    let mut idx = 0;
    for (i, line) in lines.iter().enumerate() {
        if line.start > byte {
            break;
        }
        idx = i;
    }
    // Wrap-end affinity: a caret exactly on a wrap break (not
    // after `\n`, not the text end) renders at the ending line.
    if byte > 0
        && byte < composite.len()
        && !composite[..byte].ends_with('\n')
        && lines.get(idx).is_some_and(|l| l.start == byte)
        && idx > 0
    {
        idx -= 1;
    }
    idx
}

fn line_containing<'a>(lines: &'a [VisualLine], composite: &str, byte: usize) -> &'a VisualLine {
    let idx = line_index_containing(lines, composite, byte);
    &lines[idx.min(lines.len().saturating_sub(1))]
}

/// Caret x for `byte` inside its line (Round 22.2): the
/// containing cluster's leading edge (mid-cluster bytes snap by
/// midpoint, the shared tie rule); past the last cluster, the
/// line end; cluster-less (blank) lines sit at their start (x
/// origin — the line has no advances of its own).
fn x_in_line(line: &VisualLine, byte: usize) -> f32 {
    let mut x = 0.0;
    for &(s, e, cx, w) in &line.clusters {
        if byte <= s {
            return cx;
        }
        if byte < e {
            let mid = s + (e - s) / 2;
            return if byte < mid { cx } else { cx + w };
        }
        x = cx + w;
    }
    x
}

/// Byte in `line` nearest device-px `x` (Round 22.2): cluster
/// starts by midpoint, the line start before the first, the
/// content end past the last (a trailing `\n` belongs to the
/// break, never the caret — column affinity clamps, never
/// wraps).
fn byte_at_x(line: &VisualLine, composite: &str, x: f32) -> usize {
    let end = content_end(line, composite);
    if line.clusters.is_empty() {
        return line.start.min(end);
    }
    let first = &line.clusters[0];
    if x < first.2 {
        return line.start.min(first.0);
    }
    for &(s, e, cx, w) in &line.clusters {
        if x < cx + w / 2.0 {
            return s.clamp(line.start, end);
        }
        if x < cx + w {
            return e.min(end);
        }
    }
    end
}

/// Content end of a span (Round 22.2): the span end, unless the
/// span closes with `\n` (hard break) — then the newline byte,
/// which belongs to the break, is excluded.
fn content_end(line: &VisualLine, composite: &str) -> usize {
    if line.end > line.start
        && line.end <= composite.len()
        && composite.as_bytes().get(line.end - 1) == Some(&b'\n')
    {
        line.end - 1
    } else {
        line.end
    }
}

/// Hard-line fallback without a shaper (Round 22.2, graceful
/// decision-207 rule): previous/next `\n` boundaries with
/// character-column affinity (counts, not advances).
fn hard_line_byte(composite: &str, caret: usize, dir: i32, x: f32) -> usize {
    let caret = caret.min(composite.len());
    // Hard line spans.
    let mut starts = vec![0usize];
    for (i, c) in composite.char_indices() {
        if c == '\n' {
            starts.push(i + 1);
        }
    }
    let mut li = 0;
    for (i, &s) in starts.iter().enumerate() {
        if s > caret {
            break;
        }
        li = i;
    }
    // Column of the caret within its line (chars), or the x
    // column when travelling (callers pass the visual x; without
    // advances the char column stands in).
    let col = if x <= 0.0 {
        composite[starts[li]..caret].chars().count()
    } else {
        x as usize
    };
    let target = if dir < 0 {
        if li == 0 {
            return starts[0];
        }
        li - 1
    } else {
        if li + 1 >= starts.len() {
            return composite.len();
        }
        li + 1
    };
    let base = starts[target];
    let end = if target + 1 < starts.len() {
        // Exclude the line's own `\n`.
        starts[target + 1] - 1
    } else {
        composite.len()
    };
    let mut byte = base;
    for _ in 0..col {
        let next = composite[byte..end.min(composite.len())]
            .chars()
            .next()
            .map(|c| byte + c.len_utf8())
            .unwrap_or(end);
        if next > end {
            break;
        }
        byte = next;
    }
    byte.min(end)
}

/// Floors `byte` onto a char boundary of `s` (clamping OOB first), so
/// platform-supplied offsets can never panic a slice (decision 207).
fn clamp_byte(s: &str, byte: usize) -> usize {
    let mut b = byte.min(s.len());
    while !s.is_char_boundary(b) {
        b -= 1;
    }
    b
}

#[derive(Clone, Copy, PartialEq)]
enum WordClass {
    Word,
    Ideograph,
    /// Emoji run (Round 31, decision 347): Extended_Pictographic
    /// clusters + ZWJ/VS16 glue select and step as one unit.
    /// Regional indicators pair separately (Round 32, decision 348).
    Emoji,
    /// Regional-indicator pair unit (Round 32, decision 348 --
    /// UAX #29 WB15/WB16): consecutive RIs pair from the last
    /// non-RI (or string start); a lone RI is its own unit.
    Regional,
    Separator,
}

fn word_class(c: char) -> WordClass {
    if is_ideographish(c) {
        WordClass::Ideograph
    } else if c.is_alphanumeric() {
        WordClass::Word
    } else if is_regional(c) {
        WordClass::Regional
    } else if is_emoji_joiner(c) {
        // After alphanumeric on purpose: only current separators
        // change class (enclosed alphanumerics keep Word).
        WordClass::Emoji
    } else {
        WordClass::Separator
    }
}

fn is_ideographish(c: char) -> bool {
    let u = c as u32;
    (0x3400..=0x4DBF).contains(&u)
        || (0x4E00..=0x9FFF).contains(&u)
        || (0xF900..=0xFAFF).contains(&u)
        || (0x3040..=0x30FF).contains(&u) // hiragana + katakana
        || (0xAC00..=0xD7AF).contains(&u) // hangul
        || u >= 0x20000
}

/// Regional indicator for flag pairing (Round 32, decision 348).
fn is_regional(c: char) -> bool {
    (0x1F1E6..=0x1F1FF).contains(&(c as u32))
}

/// Pictograph unit for word runs (Round 31, decision 347 --
/// hand ranges, the is_ideographish precedent; core stays
/// zero-dependency). Pragmatic Extended_Pictographic cover plus
/// the ZWJ/VS16 glue: contiguous emoji select and step as one
/// unit, and ZWJ keeps the run open across join sequences
/// (family emoji select whole). Lone joiners select alone
/// (harmless -- invisible chars). Regional indicators live in
/// `is_regional` (pair grouping, Round 32); CJK radicals/strokes
/// are untouched (outside these ranges, still separators).
fn is_emoji_joiner(c: char) -> bool {
    let u = c as u32;
    u == 0x00A9 // (c)
        || u == 0x00AE // (r)
        || u == 0x200D // ZWJ
        || u == 0xFE0F // VS16
        || u == 0x2122 // (tm)
        || (0x203C..=0x2049).contains(&u) // bang/query marks
        || (0x2190..=0x21FF).contains(&u) // arrows
        || (0x2300..=0x23FF).contains(&u) // misc technical
        || (0x2600..=0x27BF).contains(&u) // symbols + dingbats
        || (0x2B00..=0x2BFF).contains(&u) // more symbols
        || (0x1F000..=0x1F1E5).contains(&u) // emoticons to symbols
        || (0x1F200..=0x1FAFF).contains(&u) // ideograph supplements to extended-A
}

// ---------------------------------------------------------------------------
// ImeCompositionHandler — the session as the core-side IME sink (spike
// parity, except focus-loss, which commits per locked #27 instead of the
// spike's pre-merge cancel).
// ---------------------------------------------------------------------------

impl EditSession {
    /// Commits the active composition buffer with `committed` (`&self`
    /// so both the `&mut` trait entry and the shared-handle
    /// `notify_focus_lost` use it). No-op when not composing.
    fn commit_active(&self, committed: &str) {
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

#[cfg(test)]
mod tests;
