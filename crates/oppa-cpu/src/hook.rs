//! PAINT-phase wiring: FramePlan build + backend commit in the phase,
//! same hook shape as M3's `set_layout_pass`.
//!
//! Renderer state (surfaces, retained ops, node registry) lives in the
//! [`CpuBackend`](crate::backend::CpuBackend), keyed by
//! [`NodeId`](oppa::NodeId); boxes stay core-side on the retained nodes
//! (lock #25). Post-swap re-runs re-measure/re-layout through the
//! dirty-mask path and the backend replays the rebuilt dirty subtrees —
//! no full rebuild, no new locks (see the M4 round entry).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use oppa::ComponentHost;
use oppa::{RendererBackend, SurfaceId};

use crate::backend::CpuBackend;
use crate::builder::FramePlanBuilder;

/// Installs the paint pass on the host's runtime: each PAINT phase drains
/// the frame's dirty masks into a [`FramePlan`](oppa::FramePlan), commits
/// every newly arrived diff in order, and paints. Failures panic loudly
/// (a backend error is never swallowed into a blank frame).
///
/// M8 (§9.4): the plan paints TIME-evaluated values (live interpolations
/// mid-flight, stamped recycles at their jumped targets) — the GPU half
/// of the transition evaluator. Off-interpolation the overlay is the
/// identity (same walk, same masks, same ops as the raw build).
///
/// `paint_calls` / `last_plan_ops` are test observability (how many PAINT
/// phases ran, how many ops the latest plan carried).
pub fn install_paint_hook(
    host: &ComponentHost,
    backend: Rc<RefCell<CpuBackend>>,
    surface: SurfaceId,
    dpr: f32,
    paint_calls: Rc<Cell<usize>>,
    last_plan_ops: Rc<Cell<usize>>,
) {
    let host2 = host.clone();
    let seen = Rc::new(Cell::new(host.diff_count()));
    let seen2 = seen.clone();
    let builder = FramePlanBuilder::new(dpr);
    host.runtime().set_paint_pass(move |_rt| {
        let now = host2.runtime().now_secs();
        // Round 8.2: the build-scoped selection rides the focused
        // session (computed outside the retained borrow below).
        // Round 15.1: the caret bar rides the same hook (one overlay
        // rule — every presenter blinks from the host clock).
        builder.set_selection(host2.focused_selection_paint());
        builder.set_caret(host2.focused_caret_paint());
        let plan = host2.with_retained_mut(|rec, styles| {
            host2.with_evaluator(|ev| builder.build_incremental_evaluated(rec, styles, ev, now))
        });
        last_plan_ops.set(plan.ops.len());
        {
            let mut be = backend.borrow_mut();
            for diff in host2.diffs_from(seen2.get()) {
                be.commit(&diff).expect("cpu backend commit failed");
            }
            seen2.set(host2.diff_count());
            be.paint(surface, &plan).expect("cpu backend paint failed");
        }
        paint_calls.set(paint_calls.get() + 1);
    });
}
