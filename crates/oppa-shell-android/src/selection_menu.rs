//! Native selection-menu sync (decision 443): the host-testable
//! half of Android's text toolbar.
//!
//! GPU-rendered fields own no Android view, so the platform's
//! select-all/copy/paste toolbar cannot appear on its own. The
//! device half (in `oppa-android-runner`) hosts a floating
//! `ActionMode` on the hidden IME proxy and mirrors the focused
//! session into it; this crate owns everything provable without a
//! JVM:
//!
//! - [`MenuAction`]: toolbar taps crossing from Java (the runner's
//!   native entries push these; the pump applies them to the focused
//!   session through the loop clipboard).
//! - [`MenuActionQueue`]: the thread-safe FIFO (same shape as
//!   [`ImeTextQueue`](super::ime_queue::ImeTextQueue) — Java pushes
//!   on the UI thread, the pump drains on the loop thread).
//! - [`MenuSync`]: the show/update/hide state machine (selection-
//!   driven, dismissal-aware — a user-dismissed toolbar stays down
//!   until the selection changes, the platform rule).
//! - [`byte_to_utf16_units`]: Rust byte offsets are not Java string
//!   indices (UTF-16 units) — the mirror converts at the boundary.
//!
//! std-only, no JNI linkage (the M10 crate rule).

use std::collections::VecDeque;
use std::sync::Mutex;

/// One proxy-originated UI action crossing from Java (decision
/// 443 toolbar taps + decision 449 selection reports — the pump
/// applies both to the focused session through the loop clipboard;
/// FIFO order, last writer wins by construction).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    /// Select-all item (session-local, no clipboard involved).
    SelectAll,
    /// Copy item (selection → loop clipboard).
    Copy,
    /// Cut item (selection → loop clipboard + delete).
    Cut,
    /// Paste item (loop clipboard → over the selection).
    Paste,
    /// The toolbar was dismissed by the user (mode destroyed without
    /// a tap — the state machine suppresses re-show until the
    /// selection changes).
    Dismissed,
    /// The proxy's selection moved under a native gesture
    /// (decision 449 — selection-handle drags and Editor word
    /// selects act on the proxy buffer; the session follows so
    /// copy/cut read the visible span). UTF-16 units, anchor/cursor
    /// order preserved (start may exceed end — the session orders).
    SelectionChanged { start16: usize, end16: usize },
}

/// Thread-safe FIFO from the UI thread to the pump (same contract
/// as the IME text queue: unbounded — taps are loop-rate, the pump
/// drains per frame, and a cap would silently drop user intent).
#[derive(Debug, Default)]
pub struct MenuActionQueue {
    items: Mutex<VecDeque<MenuAction>>,
}

impl MenuActionQueue {
    pub fn new() -> Self {
        Self {
            items: Mutex::new(VecDeque::new()),
        }
    }

    /// Pushes one action (the JNI callback path — never blocks; a
    /// poisoned mutex fails loudly instead of swallowing the tap).
    pub fn push(&self, action: MenuAction) {
        self.items
            .lock()
            .expect("menu action queue mutex poisoned")
            .push_back(action);
    }

    /// Pushes a proxy selection report (the `onSelectionChanged`
    /// native entry's body — Java pre-clamps negatives, the pump
    /// converts units to bytes).
    pub fn push_selection(&self, start16: usize, end16: usize) {
        self.push(MenuAction::SelectionChanged { start16, end16 });
    }

    /// Drains everything in FIFO order (the pump path — one drain
    /// per frame).
    pub fn drain(&self) -> Vec<MenuAction> {
        self.items
            .lock()
            .expect("menu action queue mutex poisoned")
            .drain(..)
            .collect()
    }

    /// Pending count (diagnostics/tests — the pump never branches
    /// on it).
    pub fn len(&self) -> usize {
        self.items
            .lock()
            .expect("menu action queue mutex poisoned")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One mirrored toolbar frame: the committed field text plus the
/// selection in Java string units plus the anchor rect in device px
/// (the field's committed box — stable across shift-extends, so the
/// toolbar never chases the caret; selection-tight anchoring is a
/// follow-up, stated) plus what drove the selection into existence
/// (the toolbar raises for touch-driven selections only — decision
/// 451; cause flips re-fire like any other snapshot change).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuSnapshot {
    pub text: String,
    pub start16: usize,
    pub end16: usize,
    pub rect: (i32, i32, i32, i32),
    pub cause: SelCause,
}

/// What drove the focused selection into existence (decision 451):
/// the native toolbar is touch UI — it raises for touch-driven
/// selections (taps, holds, drags, handle reports, toolbar taps)
/// and stays down for keyboard-driven ones (Ctrl+A, Shift+nav —
/// verified in the shipped framework source: no key path starts the
/// mode, and Ctrl+A even hides a live one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelCause {
    Touch,
    Key,
}

/// Whether the pump may fire the toolbar for this snapshot
/// (decision 451): touch-driven selections raise it, keyboard-driven
/// ones stay quiet (the verified native rule). Updates and hides
/// flow regardless — only the initial raise is gated (a live toolbar
/// tracks selection moves from any source, the accepted
/// simplification).
pub fn show_permitted(snapshot: &MenuSnapshot) -> bool {
    snapshot.cause == SelCause::Touch
}

/// What the pump should do about the toolbar this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuOp {
    /// Mirror + start the floating mode.
    Show(MenuSnapshot),
    /// Re-mirror into the live mode (text/selection/rect moved).
    Update(MenuSnapshot),
    /// Finish the mode (no selection, focus lost, or dismissed).
    Hide,
    /// Steady state (shown and current, or hidden and unwanted).
    None,
}

/// Selection-driven toolbar state (decision 443): `Some` snapshot
/// while a field session is focused with a non-collapsed selection,
/// `None` otherwise. Dismissal-aware: a user-dismissed toolbar stays
/// down until the snapshot changes (re-showing unprompted next frame
/// would fight the user's swipe-away — the platform keeps selection
/// without the toolbar until it moves).
#[derive(Debug, Default)]
pub struct MenuSync {
    shown: bool,
    last: Option<MenuSnapshot>,
    dismissed: Option<MenuSnapshot>,
}

impl MenuSync {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the loop currently believes the mode is up (the pump
    /// uses this only for bookkeeping — show/update/hide all route
    /// through [`MenuSync::decide`]).
    pub fn is_shown(&self) -> bool {
        self.shown
    }

    /// The cause carried by the last decided snapshot, if any
    /// (decision 451 — the pump carries it across input-quiet pumps
    /// so IME-only batches don't flip a keyboard-driven snapshot to
    /// touch and wrongly raise the toolbar).
    pub fn last_cause(&self) -> Option<SelCause> {
        self.last.as_ref().map(|s| s.cause)
    }

    /// Records a user dismissal at the current snapshot (the
    /// `Dismissed` action path).
    pub fn note_dismissed(&mut self, at: Option<MenuSnapshot>) {
        self.shown = false;
        self.dismissed = at;
    }

    /// Records a refused start (the mode reported null — the pump
    /// retries next frame instead of wedging in `shown`). Keeps the
    /// last snapshot (the retry re-shows the same frame) and any
    /// dismissal pin (a refusal is not a dismissal).
    pub fn note_show_failed(&mut self) {
        self.shown = false;
    }

    /// Drops all state (window-generation heals: the decor the mode
    /// hung on is gone, and the re-attached proxy restarts clean —
    /// a carried dismissal would pin the toolbar down across the
    /// heal for a selection that never moved).
    pub fn reset(&mut self) {
        self.shown = false;
        self.last = None;
        self.dismissed = None;
    }

    /// One pump decision from the current toolbarwant (`None` = no
    /// focused session, focus outside a field, or a collapsed
    /// selection — anything that is not showable text).
    pub fn decide(&mut self, want: Option<MenuSnapshot>) -> MenuOp {
        match want {
            None => {
                if self.shown {
                    self.shown = false;
                    self.last = None;
                    MenuOp::Hide
                } else {
                    self.last = None;
                    MenuOp::None
                }
            }
            Some(snap) => {
                if Some(&snap) == self.dismissed.as_ref() {
                    // Dismissed at exactly this snapshot — stay down.
                    // If Java still shows (destroy callback lost),
                    // finish it so both sides agree.
                    if self.shown {
                        self.shown = false;
                        self.last = Some(snap);
                        MenuOp::Hide
                    } else {
                        self.last = Some(snap);
                        MenuOp::None
                    }
                } else if !self.shown {
                    self.shown = true;
                    self.last = Some(snap.clone());
                    MenuOp::Show(snap)
                } else if self.last.as_ref() != Some(&snap) {
                    self.last = Some(snap.clone());
                    MenuOp::Update(snap)
                } else {
                    MenuOp::None
                }
            }
        }
    }
}

/// Converts a Rust byte offset into Java string units (UTF-16 code
/// units): clamps past-the-end to the text end and mid-char offsets
/// down to the enclosing char boundary (a sliced surrogate half is
/// never a selection edge — the mirror floors instead of panicking).
pub fn byte_to_utf16_units(text: &str, byte: usize) -> usize {
    let mut b = byte.min(text.len());
    while b > 0 && !text.is_char_boundary(b) {
        b -= 1;
    }
    text[..b].encode_utf16().count()
}

/// Quiet period after the last proxy selection report before Rust
/// re-fires the toolbar (decision 450): a finger mid-drag reports
/// continuously, and firing a synthetic long-press into a live drag
/// churns the Editor (vanishing handles, flickering toolbar). Past
/// this window the gesture is settled (lifted or parked) and the
/// fire restores the toolbar. Reasoned (covers the ~650ms synthetic
/// UP plus Editor settle with margin), not derived — override by
/// editing this const with a new decision, not silently.
pub const DRAG_SETTLE_MS: u64 = 800;

/// True when no proxy selection report landed within the settle
/// window (decision 450): `None` (never reported) counts as settled
/// — the first show off a tap/keys selection fires immediately with
/// zero latency regression. Saturating (a future timestamp settles
/// instead of panicking — clocks only move forward, but callers
/// must never be able to wedge the toolbar on arithmetic).
pub fn drag_settled(last_report: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    match last_report {
        None => true,
        Some(t) => now.saturating_duration_since(t).as_millis() >= DRAG_SETTLE_MS as u128,
    }
}

/// Decides the dismissal pin for one pump batch (decision 449):
/// a bare `Dismissed` pins the pre-batch snapshot (user swipe-away —
/// re-show only after the selection moves), but a dismissal near a
/// `SelectionChanged` (same batch or, OR-ed by the caller, the
/// previous one) is a gesture side-effect — the Editor kills the
/// mode when a handle drag starts AND when it settles — pinning
/// there would wedge the toolbar down across the very selection the
/// drag produced, while native re-shows on release. Returns `None`
/// (no pin) in the gesture case so the new snapshot re-shows; a
/// later genuine swipe still pins (no selection anywhere near it —
/// self-healing within one cycle either way).
pub fn dismissal_pin(pre: Option<MenuSnapshot>, batch_has_selection: bool) -> Option<MenuSnapshot> {
    if batch_has_selection {
        None
    } else {
        pre
    }
}

/// Converts Java string units (UTF-16 code units, e.g. a proxy
/// `onProxySelectionChanged` span) back into a Rust byte offset:
/// clamps past-the-end, floors mid-surrogate-half offsets to the
/// enclosing pair start (a lone half is never an edge — the report
/// floors instead of panicking, the `byte_to_utf16_units` mirror).
pub fn utf16_to_byte_units(text: &str, units: usize) -> usize {
    let mut taken = 0usize;
    let mut byte = 0usize;
    for (b, ch) in text.char_indices() {
        let w = ch.len_utf16();
        if taken + w > units {
            break;
        }
        taken += w;
        byte = b + ch.len_utf8();
        if taken == units {
            break;
        }
    }
    byte
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_fifo_and_empty_drain() {
        let q = MenuActionQueue::new();
        assert!(q.is_empty());
        q.push(MenuAction::SelectAll);
        q.push(MenuAction::Copy);
        q.push(MenuAction::Dismissed);
        assert_eq!(q.len(), 3);
        assert_eq!(
            q.drain(),
            vec![
                MenuAction::SelectAll,
                MenuAction::Copy,
                MenuAction::Dismissed,
            ]
        );
        assert!(q.is_empty());
        assert!(q.drain().is_empty(), "empty drain is empty, not an error");
    }

    fn snap(text: &str, s: usize, e: usize) -> MenuSnapshot {
        MenuSnapshot {
            text: text.to_string(),
            start16: s,
            end16: e,
            rect: (0, 0, 100, 40),
            cause: SelCause::Touch,
        }
    }

    fn snap_keyed(text: &str, s: usize, e: usize) -> MenuSnapshot {
        MenuSnapshot {
            cause: SelCause::Key,
            ..snap(text, s, e)
        }
    }

    #[test]
    fn show_update_hide_cycle() {
        let mut sync = MenuSync::new();
        let a = snap("hello", 0, 5);
        assert_eq!(sync.decide(Some(a.clone())), MenuOp::Show(a.clone()));
        assert!(sync.is_shown());
        assert_eq!(sync.decide(Some(a.clone())), MenuOp::None);
        let b = snap("hello", 1, 5);
        assert_eq!(sync.decide(Some(b.clone())), MenuOp::Update(b.clone()));
        assert_eq!(sync.decide(Some(b)), MenuOp::None);
        assert_eq!(sync.decide(None), MenuOp::Hide);
        assert!(!sync.is_shown());
        assert_eq!(sync.decide(None), MenuOp::None);
    }

    #[test]
    fn dismissal_pins_until_selection_moves() {
        let mut sync = MenuSync::new();
        let a = snap("hello", 0, 5);
        assert_eq!(sync.decide(Some(a.clone())), MenuOp::Show(a.clone()));
        // User swipes the toolbar away at this exact snapshot.
        sync.note_dismissed(Some(a.clone()));
        assert!(!sync.is_shown());
        // Same selection next frame: stays down (no fight).
        assert_eq!(sync.decide(Some(a.clone())), MenuOp::None);
        // Selection moves: shows again.
        let b = snap("hello", 0, 4);
        assert_eq!(sync.decide(Some(b.clone())), MenuOp::Show(b));
    }

    #[test]
    fn dismissal_without_snapshot_hides_next_show() {
        // Dismissed with no snapshot readable (focus raced the
        // callback): next identical select still shows — a lost
        // dismissal never pins the toolbar down forever.
        let mut sync = MenuSync::new();
        let a = snap("hello", 0, 5);
        assert_eq!(sync.decide(Some(a.clone())), MenuOp::Show(a.clone()));
        sync.note_dismissed(None);
        assert_eq!(sync.decide(Some(a.clone())), MenuOp::Show(a));
    }

    #[test]
    fn reset_clears_dismissal_across_heals() {
        let mut sync = MenuSync::new();
        let a = snap("hello", 0, 5);
        assert_eq!(sync.decide(Some(a.clone())), MenuOp::Show(a.clone()));
        sync.note_dismissed(Some(a.clone()));
        sync.reset();
        assert_eq!(sync.decide(Some(a.clone())), MenuOp::Show(a));
    }

    #[test]
    fn refused_start_retries_same_frame_without_unpinning() {
        let mut sync = MenuSync::new();
        let a = snap("hello", 0, 5);
        assert_eq!(sync.decide(Some(a.clone())), MenuOp::Show(a.clone()));
        // Java refused (null mode): belief drops, same snapshot
        // re-shows next pump (never wedges shown-but-down).
        sync.note_show_failed();
        assert!(!sync.is_shown());
        assert_eq!(sync.decide(Some(a.clone())), MenuOp::Show(a));
    }

    #[test]
    fn dismissal_pin_skips_gesture_batches() {
        let a = snap("hello", 0, 7);
        // Bare dismissal pins the pre-batch snapshot (swipe-away).
        assert_eq!(dismissal_pin(Some(a.clone()), false), Some(a.clone()));
        // Dismissal riding with a selection report pins nothing (the
        // drag killed the mode transiently — the new snapshot
        // re-shows on release, the native shape).
        assert_eq!(dismissal_pin(Some(a), true), None);
        // No snapshot to pin (focus raced the callback): nothing.
        assert_eq!(dismissal_pin(None, false), None);
    }

    #[test]
    fn drag_settle_waits_out_the_gesture() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        // Never reported: settled (first shows fire immediately).
        assert!(drag_settled(None, now));
        // Just reported: not settled (defer the re-fire).
        assert!(!drag_settled(Some(now), now));
        assert!(!drag_settled(
            now.checked_sub(Duration::from_millis(DRAG_SETTLE_MS - 1)),
            now
        ));
        // Past the window: settled (restore the toolbar).
        assert!(drag_settled(
            now.checked_sub(Duration::from_millis(DRAG_SETTLE_MS)),
            now
        ));
        assert!(drag_settled(now.checked_sub(Duration::from_secs(10)), now));
    }

    #[test]
    fn utf16_units_ascii_and_multibyte() {
        assert_eq!(byte_to_utf16_units("hello", 0), 0);
        assert_eq!(byte_to_utf16_units("hello", 5), 5);
        // e-acute is 2 bytes, 1 UTF-16 unit.
        assert_eq!(byte_to_utf16_units("caf\u{e9}", 5), 4);
        // Thumbs-up is 4 bytes (1 scalar), 2 UTF-16 units.
        assert_eq!(byte_to_utf16_units("a\u{1f44d}b", 1), 1);
        assert_eq!(byte_to_utf16_units("a\u{1f44d}b", 5), 3);
        assert_eq!(byte_to_utf16_units("a\u{1f44d}b", 6), 4);
    }

    #[test]
    fn utf16_units_clamp_past_end_and_mid_char() {
        // Past the end saturates (never panics on a racing length).
        assert_eq!(byte_to_utf16_units("hi", 99), 2);
        // Mid-char floors to the boundary (never a half unit).
        assert_eq!(byte_to_utf16_units("caf\u{e9}", 4), 3);
        assert_eq!(byte_to_utf16_units("", 7), 0);
    }

    #[test]
    fn utf16_to_bytes_round_trips_and_floors() {
        // ASCII identity both ways.
        assert_eq!(utf16_to_byte_units("hello", 0), 0);
        assert_eq!(utf16_to_byte_units("hello", 5), 5);
        // Thumbs-up is 2 UTF-16 units / 4 bytes: unit edges map
        // exactly, mid-pair floors to the pair start (never split).
        assert_eq!(utf16_to_byte_units("a\u{1f44d}b", 1), 1);
        assert_eq!(utf16_to_byte_units("a\u{1f44d}b", 2), 1);
        assert_eq!(utf16_to_byte_units("a\u{1f44d}b", 3), 5);
        assert_eq!(utf16_to_byte_units("a\u{1f44d}b", 4), 6);
        // Past the end saturates at the text end.
        assert_eq!(utf16_to_byte_units("hi", 99), 2);
        assert_eq!(utf16_to_byte_units("", 7), 0);
        // Round trip through both conversions is exact on edges.
        let text = "a\u{1f44d}b";
        for byte in [0, 1, 5, 6] {
            let units = byte_to_utf16_units(text, byte);
            assert_eq!(utf16_to_byte_units(text, units), byte, "byte {byte}");
        }
    }

    #[test]
    fn queue_carries_selection_reports_in_order() {
        let q = MenuActionQueue::new();
        q.push(MenuAction::Copy);
        q.push_selection(2, 6);
        q.push(MenuAction::Dismissed);
        assert_eq!(
            q.drain(),
            vec![
                MenuAction::Copy,
                MenuAction::SelectionChanged {
                    start16: 2,
                    end16: 6,
                },
                MenuAction::Dismissed,
            ]
        );
    }

    #[test]
    fn cause_flip_refires_like_any_snapshot_change() {
        // Same span, Touch vs Key: unequal snapshots, so a touch on
        // an already key-selected span re-drives the toolbar — a
        // cause flip counts exactly like a span move (the pump
        // converts a suppressed-then-touched Update into a fresh
        // fire; double-tap after Ctrl+A raises the toolbar).
        let mut sync = MenuSync::new();
        let touched = snap("hello", 0, 5);
        let keyed = snap_keyed("hello", 0, 5);
        assert_ne!(touched, keyed);
        assert_eq!(sync.decide(Some(keyed.clone())), MenuOp::Show(keyed));
        assert_eq!(sync.decide(Some(touched.clone())), MenuOp::Update(touched));
        assert_eq!(sync.last_cause(), Some(SelCause::Touch));
    }

    #[test]
    fn show_permitted_gates_only_the_initial_raise() {
        // Touch raises, keyboard stays quiet (verified native rule).
        assert!(show_permitted(&snap("hello", 0, 5)));
        assert!(!show_permitted(&snap_keyed("hello", 0, 5)));
    }

    #[test]
    fn last_cause_tracks_the_decided_frame() {
        let mut sync = MenuSync::new();
        assert_eq!(sync.last_cause(), None);
        let keyed = snap_keyed("hello", 0, 5);
        assert!(matches!(sync.decide(Some(keyed)), MenuOp::Show(_)));
        assert_eq!(sync.last_cause(), Some(SelCause::Key));
        // Collapse clears it (typeless frames carry nothing).
        assert_eq!(sync.decide(None), MenuOp::Hide);
        assert_eq!(sync.last_cause(), None);
    }
}
