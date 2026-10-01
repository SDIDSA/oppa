//! PAINT-phase wiring for the DOM backend: same hook shape as the CPU
//! (`oppa-cpu`) and Vello (`oppa-vello`) hooks (M7 — the third presenter
//! joins the same phase discipline).
//!
//! Each PAINT phase builds the shared dirty-subtree [`FramePlan`](oppa::FramePlan)
//! (the builder is shared — same plans, same damage discipline; the DOM
//! arm reads no raster from it), commits every newly arrived diff in
//! order, re-derives the DOM from retained reads, and records the paint.
//! Failures panic loudly (a backend error is never swallowed into a
//! blank page).
//!
//! M8 (§9.4): the plan builds through the same TIME-evaluated overlay as
//! both rasterizers (uniform rule — off-interpolation it is the identity).
//! The DOM's interpolation itself happens in the browser (the `transition:`
//! declarations + one-commit `transition:none` suppression); the plan here
//! stays what it always was, a commit-work meter.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use oppa::{ComponentHost, RendererBackend};

use crate::css::StyleSheet;
use crate::dom::DomBackend;

/// Installs the DOM paint pass on the host's runtime. `paint_calls` /
/// `last_touched` are test observability (how many PAINT phases ran,
/// how many elements the latest sync touched).
#[allow(clippy::too_many_arguments)]
pub fn install_dom_paint_hook(
    host: &ComponentHost,
    backend: Rc<RefCell<DomBackend>>,
    sheet: Rc<RefCell<StyleSheet>>,
    surface: oppa::SurfaceId,
    dpr: f32,
    paint_calls: Rc<Cell<usize>>,
    last_touched: Rc<Cell<usize>>,
) {
    let host2 = host.clone();
    let seen = Rc::new(Cell::new(host.diff_count()));
    let seen2 = seen.clone();
    let builder = oppa_cpu::FramePlanBuilder::new(dpr);
    host.runtime().set_paint_pass(move |_rt| {
        let now = host2.runtime().now_secs();
        // Round 8.2: the shared-plan selection (rasterizers) and the
        // DOM highlight divs (below) ride the same focused session.
        // Round 15.1: the caret bar rides the same hook (plan `Rect`
        // + DOM `caret` div from one host query).
        let selection = host2.focused_selection_paint();
        let caret = host2.focused_caret_paint();
        builder.set_selection(selection);
        builder.set_caret(caret);
        let plan = host2.with_retained_mut(|rec, styles| {
            host2.with_evaluator(|ev| builder.build_incremental_evaluated(rec, styles, ev, now))
        });
        {
            let mut be = backend.borrow_mut();
            for diff in host2.diffs_from(seen2.get()) {
                be.commit(&diff).expect("dom backend commit failed");
            }
            seen2.set(host2.diff_count());
            be.set_selection(selection);
            be.set_caret(caret);
            let stats = host2.with_retained_mut(|rec, styles| {
                be.sync(rec, styles, &mut sheet.borrow_mut())
                    .expect("dom backend sync failed")
            });
            last_touched.set(stats.touched);
            be.paint(surface, &plan).expect("dom backend paint failed");
        }
        paint_calls.set(paint_calls.get() + 1);
    });
}
