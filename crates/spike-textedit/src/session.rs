//! The Windows-GPU arm's editing session — the framework-authority editing
//! model the spike tests (DESIGN §9.2 "settled now" list, exercised as far
//! as M0 allows): content lives in an author-owned `Signal<String>`
//! (controlled-component pattern, locked #24), caret/selection/composition/
//! undo are session state core-side, IME events flow through the M0b
//! normalized surface (`ImeCompositionFeed` → `dispatch_ime_event` → this
//! handler), and the candidate-window anchor is emitted through
//! `PlatformShell::set_ime` exactly as the contract wires it.
//!
//! v1 scopes held to this round's brief: single line, single-level undo
//! (one entry — the state before the last edit; IME commits are atomic via
//! the snapshot taken at `CompositionStarted`), cluster-stepped caret
//! motion via `ShapedRun`'s pure math, and the composition buffer held
//! session-side (in-progress text is *not* written into the content signal
//! until commit — the §9.2 controlled pattern).
//!
//! Spike simplification stated plainly: there is no renderer or window yet
//! (round scope), so `set_ime` is recorded by a [`RecordingShell`] and read
//! back by the rig; the reactive core participates as the content store +
//! frame discipline (`run_once` after each scripted step), not as a full
//! component tree.

use oppa::ime::{ImeCompositionEvent, ImeCompositionHandler, ImeOps};
use oppa::reactive::{Runtime, Signal};
use oppa::shell::PlatformShell;
use oppa::text::{CaretRect, ShapedRun, TextService, TextStyle};
use std::cell::RefCell;
use std::rc::Rc;

/// Shell stand-in that records every `set_ime` op — the rig's view of what
/// the platform would receive for candidate-window anchoring.
pub struct RecordingShell {
    pub ime_ops: Rc<RefCell<Vec<ImeOps>>>,
}

impl PlatformShell for RecordingShell {
    fn pump_events(&mut self) -> Vec<oppa::shell::Event> {
        Vec::new()
    }
    fn set_ime(&mut self, ops: ImeOps) {
        self.ime_ops.borrow_mut().push(ops);
    }
}

/// One canonical composition-stream record (criterion 3): phase, text,
/// caret (as delivered and in composite coordinates), and the session's
/// full observable state right after the event.
#[derive(Clone, Debug, PartialEq)]
pub struct CanonStep {
    pub phase: &'static str,
    pub text: String,
    /// Caret byte as delivered by the event (composite coordinates).
    pub caret_byte: usize,
    /// Content signal value immediately after the event.
    pub content: String,
    /// In-progress composition string right after the event (empty = none).
    pub composition: String,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub content: String,
    pub caret: usize,
    pub sel: (usize, usize),
}

struct Composition {
    /// Anchor byte in the committed content.
    start: usize,
    text: String,
    /// Caret byte within the composition string.
    caret: usize,
}

/// The single-line editing session. All byte offsets in `caret`/`sel` are
/// UTF-8 byte positions in the *committed content*; composition carets are
/// composite coordinates (content prefix + composition text), matching the
/// M0b test's `CompositionUpdated{caret_byte}` convention.
pub struct EditingSession {
    pub rt: Runtime,
    pub content: Signal<String>,
    caret: usize,
    sel: (usize, usize),
    composition: Option<Composition>,
    undo: Option<Snapshot>,
    service: Rc<dyn TextService>,
    style: TextStyle,
    ime_ops: Rc<RefCell<Vec<ImeOps>>>,
    pub canonical: Vec<CanonStep>,
    /// Cached shaping of the last shaped text (validity checked by string).
    shaped: RefCell<Option<(String, ShapedRun)>>,
}

impl EditingSession {
    pub fn new(
        rt: Runtime,
        service: Rc<dyn TextService>,
        style: TextStyle,
        initial: String,
        ime_ops: Rc<RefCell<Vec<ImeOps>>>,
    ) -> Self {
        let content = rt.signal_named("field.content", initial);
        Self {
            rt,
            caret: 0,
            sel: (0, 0),
            composition: None,
            undo: None,
            service,
            style,
            ime_ops,
            canonical: Vec::new(),
            shaped: RefCell::new(None),
            content,
        }
    }

    // -- state observables --------------------------------------------------

    pub fn composite_text(&self) -> String {
        match &self.composition {
            None => self.content.get(),
            Some(c) => {
                let content = self.content.get();
                let mut out = String::with_capacity(content.len() + c.text.len());
                out.push_str(&content[..c.start.min(content.len())]);
                out.push_str(&c.text);
                out.push_str(&content[c.start.min(content.len())..]);
                out
            }
        }
    }

    /// Caret byte in composite coordinates.
    pub fn composite_caret_byte(&self) -> usize {
        match &self.composition {
            None => self.caret,
            Some(c) => c.start + c.caret,
        }
    }

    pub fn selection(&self) -> (usize, usize) {
        self.sel
    }

    pub fn composition_string(&self) -> String {
        self.composition
            .as_ref()
            .map(|c| c.text.clone())
            .unwrap_or_default()
    }

    /// The active composition's anchor byte in the committed content
    /// (None when no composition is active). The real-IME mapper uses it
    /// to convert composition-relative carets into composite coordinates.
    pub fn composition_start_byte(&self) -> Option<usize> {
        self.composition.as_ref().map(|c| c.start)
    }

    pub fn observable(&self) -> SessionState {
        SessionState {
            content: self.content.get(),
            caret: self.caret,
            sel: self.sel,
            composition: self.composition_string(),
        }
    }

    /// The candidate-window anchor for the current state, shaped through the
    /// `TextService` (device px), and recorded through the shell seam.
    pub fn caret_rect(&self) -> Option<CaretRect> {
        let composite = self.composite_text();
        if composite.is_empty() {
            return None;
        }
        let run = self.shape_cached(&composite)?;
        let byte = self.composite_caret_byte();
        Some(run.caret_rect(byte))
    }

    /// One scripted step done → run a frame (INPUT→…→PAINT discipline) and
    /// emit the IME anchor op through the shell, exactly as the frame's IME
    /// anchoring step would.
    pub fn commit_frame(&self) {
        let _ = self.rt.run_once();
        if let Some(rect) = self.caret_rect() {
            self.ime_ops.borrow_mut().push(ImeOps::SetCaretRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            });
        }
    }

    pub fn take_canonical(&mut self) -> Vec<CanonStep> {
        std::mem::take(&mut self.canonical)
    }

    // -- pointer/selection ops ----------------------------------------------

    pub fn click_x(&mut self, x: f32) {
        let Some(byte) = self.hit_test(x) else {
            return;
        };
        self.caret = byte;
        self.sel = (byte, byte);
    }

    pub fn shift_click_x(&mut self, x: f32) {
        let Some(byte) = self.hit_test(x) else {
            return;
        };
        self.sel = if byte >= self.caret {
            (self.caret, byte)
        } else {
            (byte, self.caret)
        };
        self.caret = byte;
    }

    pub fn drag_x(&mut self, x1: f32, x2: f32) {
        let (Some(b1), Some(b2)) = (self.hit_test(x1), self.hit_test(x2)) else {
            return;
        };
        self.sel = (b1.min(b2), b1.max(b2));
        self.caret = self.sel.1;
    }

    pub fn dbl_click_x(&mut self, x: f32) {
        let Some(byte) = self.hit_test(x) else {
            return;
        };
        let (a, b) = self.word_range(byte);
        self.sel = (a, b);
        self.caret = b;
    }

    pub fn caret_move(&mut self, steps: i32) {
        let byte = self.caret_boundary(steps);
        self.caret = byte;
        self.sel = (byte, byte);
    }

    /// Shift-extended caret move: the selection spans from the anchor
    /// (the caret before the move) to the new caret position — the same
    /// convention `shift_click_x` uses. Real-keyboard wiring needs this
    /// (Home + Shift+End select-all in the real-IME pass).
    pub fn extend_caret(&mut self, steps: i32) {
        let byte = self.caret_boundary(steps);
        self.sel = if byte >= self.caret {
            (self.caret, byte)
        } else {
            (byte, self.caret)
        };
        self.caret = byte;
    }

    /// The byte boundary `steps` caret positions from the current caret
    /// (cluster/char-stepped over the current composite).
    fn caret_boundary(&self, steps: i32) -> usize {
        let text = self.composite_text();
        let mut boundaries: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
        boundaries.push(text.len());
        let idx = boundaries
            .iter()
            .position(|&b| b >= self.caret)
            .unwrap_or(boundaries.len() - 1);
        let target = (idx as i64 + steps as i64).clamp(0, boundaries.len() as i64 - 1) as usize;
        boundaries[target]
    }

    pub fn caret_to_end(&mut self) {
        let end = self.composite_text().len();
        self.caret = end;
        self.sel = (end, end);
    }

    pub fn caret_to_start(&mut self) {
        self.caret = 0;
        self.sel = (0, 0);
    }

    /// Select-all (Ctrl+A): the selection spans the whole composite, the
    /// caret at its trailing edge (the same trailing-edge convention the
    /// shift-extended ops use).
    pub fn select_all(&mut self) {
        let end = self.composite_text().len();
        self.caret = end;
        self.sel = (0, end);
    }

    /// The leading-edge x (device px, composite-relative) of cluster/char
    /// `index` in the CURRENT composite — the coordinate the framework arm
    /// itself would click at for "place caret at index" (criterion 4's
    /// geometry-addressed ops resolve against evolving text, so both arms
    /// click the same x against the same current value).
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

    // -- editing ops ---------------------------------------------------------

    pub fn insert(&mut self, text: &str) {
        if self.composition.is_some() {
            return;
        }
        self.push_undo();
        let old = self.content.get();
        let (a, b) = ordered(self.sel);
        let mut new = String::with_capacity(old.len() + text.len());
        new.push_str(&old[..a]);
        new.push_str(text);
        new.push_str(&old[b..]);
        self.caret = a + text.len();
        self.sel = (self.caret, self.caret);
        self.content.set(new);
        *self.shaped.borrow_mut() = None;
    }

    pub fn delete_selection(&mut self) {
        let (a, b) = ordered(self.sel);
        if a == b {
            return;
        }
        self.push_undo();
        let old = self.content.get();
        let mut new = String::with_capacity(old.len() - (b - a));
        new.push_str(&old[..a]);
        new.push_str(&old[b..]);
        self.caret = a;
        self.sel = (a, a);
        self.content.set(new);
        *self.shaped.borrow_mut() = None;
    }

    pub fn undo(&mut self) {
        if let Some(s) = self.undo.take() {
            self.content.set(s.content);
            self.caret = s.caret;
            self.sel = s.sel;
            *self.shaped.borrow_mut() = None;
        }
    }

    fn push_undo(&mut self) {
        self.undo = Some(Snapshot {
            content: self.content.get(),
            caret: self.caret,
            sel: self.sel,
        });
    }

    // -- geometry helpers ----------------------------------------------------

    fn shape_cached(&self, text: &str) -> Option<ShapedRun> {
        if let Some((cached, run)) = self.shaped.borrow().as_ref() {
            if cached == text {
                return Some(run.clone());
            }
        }
        let run = self.service.shape(text, &self.style).ok()?;
        *self.shaped.borrow_mut() = Some((text.to_string(), run.clone()));
        Some(run)
    }

    /// x (device px, composite-relative) → content/composite byte via the
    /// cluster-midpoint rule.
    fn hit_test(&mut self, x: f32) -> Option<usize> {
        let composite = self.composite_text();
        if composite.is_empty() {
            return Some(0);
        }
        let run = self.shape_cached(&composite)?;
        Some(run.byte_offset_for_x(x))
    }

    /// The spike's word rule (stated in ROUNDS.md; the corpus probes Latin
    /// words and CJK ideographs only): contiguous alphanumeric runs form one
    /// word; each CJK ideograph/kana/hangul character is its own word;
    /// everything else is a separator (double-click selects nothing). The
    /// hit byte resolves to the char under the same containment convention
    /// as the cluster map (`b0 <= b < b1`), so a caret exactly on a char's
    /// start byte belongs to that char.
    fn word_range(&self, byte: usize) -> (usize, usize) {
        let text = self.composite_text();
        if text.is_empty() {
            return (0, 0);
        }
        let byte = byte.min(text.len());
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
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionState {
    pub content: String,
    pub caret: usize,
    pub sel: (usize, usize),
    pub composition: String,
}

#[derive(Clone, Copy, PartialEq)]
enum WordClass {
    Word,
    Ideograph,
    Separator,
}

fn word_class(c: char) -> WordClass {
    if is_ideographish(c) {
        WordClass::Ideograph
    } else if c.is_alphanumeric() {
        WordClass::Word
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

fn ordered(sel: (usize, usize)) -> (usize, usize) {
    (sel.0.min(sel.1), sel.0.max(sel.1))
}

// ---------------------------------------------------------------------------
// ImeCompositionHandler — the editing session as the core-side IME sink.
// Every callback logs one canonical step BEFORE mutating-observable state
// changes settle, so the criterion-3 stream is a faithful per-event record.
// ---------------------------------------------------------------------------

impl EditingSession {
    fn log(&mut self, phase: &'static str, text: &str, caret_byte: usize) {
        self.canonical.push(CanonStep {
            phase,
            text: text.to_string(),
            caret_byte,
            content: self.content.get(),
            composition: self.composition_string(),
        });
    }
}

impl ImeCompositionHandler for EditingSession {
    fn composition_started(&mut self, start_byte: usize) {
        // Atomic-unit undo: snapshot the pre-composition state (DESIGN §9.2:
        // "IME composition commits as atomic units").
        self.undo = Some(Snapshot {
            content: self.content.get(),
            caret: self.caret,
            sel: self.sel,
        });
        self.composition = Some(Composition {
            start: start_byte.min(self.content.get().len()),
            text: String::new(),
            caret: 0,
        });
        self.caret = self.composition.as_ref().unwrap().start;
        self.sel = (self.caret, self.caret);
        self.log("start", "", start_byte);
    }

    fn composition_updated(&mut self, composition: &str, caret_byte: usize) {
        if self.composition.is_none() {
            // An update with no begin: tolerate (platforms race), anchor at
            // the delivered composite caret position.
            self.composition_started(caret_byte.saturating_sub(composition.len()));
        }
        let start = self.composition.as_ref().unwrap().start;
        self.composition.as_mut().unwrap().text = composition.to_string();
        self.composition.as_mut().unwrap().caret = caret_byte.saturating_sub(start);
        self.caret = caret_byte.min(start + composition.len());
        self.sel = (self.caret, self.caret);
        self.log("update", composition, caret_byte);
    }

    fn composition_committed(&mut self, committed: &str) {
        if let Some(c) = self.composition.take() {
            let old = self.content.get();
            let start = c.start.min(old.len());
            let mut new = String::with_capacity(old.len() + committed.len());
            new.push_str(&old[..start]);
            new.push_str(committed);
            new.push_str(&old[start..]);
            self.caret = start + committed.len();
            self.sel = (self.caret, self.caret);
            self.content.set(new);
            *self.shaped.borrow_mut() = None;
        } else {
            self.insert(committed);
        }
        self.log("commit", committed, self.caret);
    }

    fn composition_cancelled(&mut self) {
        if let Some(c) = self.composition.take() {
            self.caret = c.start;
            self.sel = (c.start, c.start);
        }
        self.log("cancel", "", self.caret);
    }

    fn delete_range(&mut self, range: (usize, usize)) {
        let (a, b) = ordered(range);
        let a = a.min(self.content.get().len());
        let b = b.min(self.content.get().len()).max(a);
        let old = self.content.get();
        let removed = b - a;
        let mut new = String::with_capacity(old.len() - removed);
        new.push_str(&old[..a]);
        new.push_str(&old[b..]);
        self.content.set(new);
        *self.shaped.borrow_mut() = None;
        // Re-anchor any active composition so its start tracks the content.
        if let Some(c) = self.composition.as_mut() {
            if c.start >= b {
                c.start -= removed;
            } else if c.start > a {
                c.start = a;
            }
            self.caret = c.start;
        } else {
            self.caret = a;
            self.sel = (a, a);
        }
        self.log("delete-range", "", a);
    }
}

use crate::rig::ImeStep;

/// Convenience: drive one scenario's steps through the session.
pub fn run_ime_steps(session: &mut EditingSession, steps: &[ImeStep]) {
    for step in steps {
        let event = match step {
            ImeStep::Start { start_byte } => ImeCompositionEvent::CompositionStarted {
                start_byte: *start_byte,
            },
            ImeStep::Update {
                composition,
                caret_byte,
            } => ImeCompositionEvent::CompositionUpdated {
                composition: composition.clone(),
                caret_byte: *caret_byte,
            },
            ImeStep::Commit { committed } => ImeCompositionEvent::CompositionCommitted {
                committed: committed.clone(),
            },
            ImeStep::Cancel => ImeCompositionEvent::CompositionCancelled,
            ImeStep::DeleteRange { range } => ImeCompositionEvent::DeleteRange { range: *range },
            ImeStep::FocusLoss => {
                // The session's focus-loss policy: cancel, commit nothing.
                session.composition_cancelled();
                continue;
            }
        };
        let mut feed = oppa::ime::ImeCompositionFeed::new();
        feed.push(event);
        feed.drain(session);
    }
}
