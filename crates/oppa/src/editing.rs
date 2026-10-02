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

mod caret;
mod edit_ops;
mod geometry;
mod platform;
mod session;

pub(crate) use session::{Composition, EditSnapshot, LastOp, SessionInner};
pub use session::{
    ContentObserver, EditSession, EditState, PasteOutcome, CARET_BLINK_PERIOD_SECS, EDIT_UNDO_DEPTH,
};

#[cfg(test)]
mod tests;
