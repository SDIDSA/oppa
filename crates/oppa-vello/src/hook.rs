//! PAINT-phase wiring for the Vello backend: FramePlan build + backend
//! commit in the phase, same hook shape as the CPU hook (M4) and M3's
//! `set_layout_pass`.
//!
//! Renderer state (surfaces, retained ops, node registry, present ledger)
//! lives in the [`VelloBackend`](crate::backend::VelloBackend), keyed by
//! [`NodeId`](oppa::NodeId); boxes stay core-side on the retained nodes
//! (lock #25). Failures panic loudly (a backend error is never swallowed
//! into a blank frame).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use oppa::ComponentHost;
use oppa::{RendererBackend, SurfaceId};

use crate::backend::VelloBackend;
use oppa_cpu::FramePlanBuilder;

/// Installs the paint pass on the host's runtime: each PAINT phase drains
/// the frame's dirty masks into a [`FramePlan`](oppa::FramePlan), commits
/// every newly arrived diff in order, and paints on the Vello backend.
///
/// M8 (§9.4): the plan paints TIME-evaluated values (same overlay as the
/// CPU hook — one evaluator, identical mid-flight colors modulo each
/// rasterizer's AA ramp).
///
/// `paint_calls` / `last_plan_ops` are test observability (how many PAINT
/// phases ran, how many ops the latest plan carried).
pub fn install_vello_paint_hook(
    host: &ComponentHost,
    backend: Rc<RefCell<VelloBackend>>,
    surface: SurfaceId,
    dpr: f32,
    paint_calls: Rc<Cell<usize>>,
    last_plan_ops: Rc<Cell<usize>>,
) {
    // Shared so the id can follow an in-place surface swap (resize):
    // the id is just a convenient wrapper over the same cell.
    let surface = Rc::new(Cell::new(surface));
    install_vello_paint_hook_shared(host, backend, surface, dpr, paint_calls, last_plan_ops);
}

/// Like [`install_vello_paint_hook`] but the paint target is a shared,
/// externally-updatable id (the app swaps it on resize).
pub fn install_vello_paint_hook_shared(
    host: &ComponentHost,
    backend: Rc<RefCell<VelloBackend>>,
    surface: Rc<Cell<SurfaceId>>,
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
        // session (same shared-builder rule as the CPU hook — Vello
        // encodes the emitted `Rect` ops with zero selection logic).
        // Round 15.1: the caret bar rides the same hook.
        builder.set_selection(host2.focused_selection_paint());
        builder.set_caret(host2.focused_caret_paint());
        let plan = host2.with_retained_mut(|rec, styles| {
            host2.with_evaluator(|ev| builder.build_incremental_evaluated(rec, styles, ev, now))
        });
        last_plan_ops.set(plan.ops.len());
        {
            let mut be = backend.borrow_mut();
            for diff in host2.diffs_from(seen2.get()) {
                be.commit(&diff).expect("vello backend commit failed");
            }
            seen2.set(host2.diff_count());
            be.paint(surface.get(), &plan)
                .expect("vello backend paint failed");
        }
        paint_calls.set(paint_calls.get() + 1);
    });
}
