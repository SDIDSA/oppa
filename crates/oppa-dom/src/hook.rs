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
            // Collect once: commit, tracking, and the cursor all read
            // the same diff list (the cursor advances past exactly
            // what both consumers saw).
            let diffs = host2.diffs_from(seen2.get());
            for diff in &diffs {
                be.commit(diff).expect("dom backend commit failed");
            }
            seen2.set(host2.diff_count());
            be.set_selection(selection);
            be.set_caret(caret);
            let stats = host2.with_retained_mut(|rec, styles| {
                // Phase 36 PR4 (decision 357): feed the backend's
                // keyframe tracker every frame (settling is
                // unconditional — diff-less animation frames still
                // retire tracks); the stepped path samples it during
                // sync below.
                be.track_animation(&diffs, rec, styles, now);
                be.sync(rec, styles, &mut sheet.borrow_mut())
                    .expect("dom backend sync failed")
            });
            last_touched.set(stats.touched);
            be.paint(surface, &plan).expect("dom backend paint failed");
        }
        paint_calls.set(paint_calls.get() + 1);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use oppa::{
        Color, ComponentHost, Ctx, Div, Ease, KeyframeStop, Keyframes, Props, Style, Transition,
        VNode,
    };

    #[derive(Clone)]
    struct KfProps {
        bg: Color,
    }
    impl Props for KfProps {}

    /// Keyframed box: 120ms two-stop track to the committed target.
    fn kf_scene(ctx: &Ctx, props: &KfProps) -> VNode {
        let _ = ctx.signal(0u32);
        Div("box")
            .style(
                Style::new()
                    .size(100, 50)
                    .bg(props.bg)
                    .keyframes(Keyframes::new(vec![
                        KeyframeStop::new(60, Ease::Linear).bg(Color(0x50_50_50))
                    ])),
            )
            .build()
    }

    /// Drives one hook-style frame: commit diffs, track animation at
    /// `now`, sync. Mirrors `install_dom_paint_hook` without the
    /// runtime (deterministic clock — no MockClock needed).
    fn drive(
        host: &ComponentHost,
        backend: &std::rc::Rc<RefCell<DomBackend>>,
        sheet: &mut StyleSheet,
        cursor: &mut usize,
        now: f64,
    ) {
        let diffs = host.diffs_from(*cursor);
        {
            let mut be = backend.borrow_mut();
            for diff in &diffs {
                be.commit(diff).expect("commit");
            }
        }
        *cursor = host.diff_count();
        host.with_retained_mut(|rec, styles| {
            let mut be = backend.borrow_mut();
            be.track_animation(&diffs, rec, styles, now);
            be.sync(rec, styles, sheet).expect("sync");
        });
    }

    /// Phase 36 PR4 (decision 357): the DOM stepped path re-declares
    /// live keyframe values inline every frame (mid-track differs
    /// from both endpoints), and settles to the bare target (plain
    /// tweens never take this path — the CSS mapping owns them).
    #[test]
    fn keyframes_step_inline_while_live_and_settle_bare() {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let handle = host.mount(
            "Kf",
            KfProps {
                bg: Color(0x10_10_10),
            },
            kf_scene,
        );
        host.run_until_idle();
        let backend = Rc::new(RefCell::new(DomBackend::new(1.0)));
        let mut sheet = StyleSheet::new(1.0);
        let mut cursor = 0;
        drive(&host, &backend, &mut sheet, &mut cursor, 0.0);
        // Delta to white starts the track (60ms stop + 60ms closing).
        handle.set_props(KfProps {
            bg: Color(0xFF_FF_FF),
        });
        host.run_until_idle();
        drive(&host, &backend, &mut sheet, &mut cursor, 0.03);
        let mid = {
            let be = backend.borrow();
            let id = host
                .with_retained_mut(|rec, _| rec.find_by_debug("box"))
                .into_iter()
                .next()
                .expect("box node");
            be.element(id).expect("box element").inline_kf.clone()
        };
        assert!(
            mid.contains("background-color:#"),
            "live keyframes step inline, got {mid:?}"
        );
        assert!(
            !mid.contains("#ffffff"),
            "mid-track differs from the target, got {mid:?}"
        );
        // Settle: stepped values clear (bare target, CSS owns rest).
        drive(&host, &backend, &mut sheet, &mut cursor, 5.0);
        let settled = {
            let be = backend.borrow();
            let id = host
                .with_retained_mut(|rec, _| rec.find_by_debug("box"))
                .into_iter()
                .next()
                .expect("box node");
            be.element(id).expect("box element").inline_kf.clone()
        };
        assert_eq!(settled, "", "settled tracks step nothing");
    }

    /// Phase 36 PR4: plain tweens never take the stepped path (the
    /// CSS mapping owns them — no inline keyframe declarations).
    #[test]
    fn plain_tweens_keep_the_css_mapping() {
        #[derive(Clone)]
        struct TwProps;
        impl Props for TwProps {}
        fn tw_scene(ctx: &Ctx, _: &TwProps) -> VNode {
            let _ = ctx.signal(0u32);
            Div("box")
                .style(
                    Style::new()
                        .size(100, 50)
                        .bg(Color(0x10_10_10))
                        .transition(Transition::new(120, Ease::Out)),
                )
                .build()
        }
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        host.mount("Tw", TwProps, tw_scene);
        host.run_until_idle();
        let backend = Rc::new(RefCell::new(DomBackend::new(1.0)));
        let mut sheet = StyleSheet::new(1.0);
        let mut cursor = 0;
        drive(&host, &backend, &mut sheet, &mut cursor, 0.0);
        drive(&host, &backend, &mut sheet, &mut cursor, 0.03);
        let be = backend.borrow();
        let box_id = host
            .with_retained_mut(|rec, _| rec.find_by_debug("box"))
            .into_iter()
            .next()
            .expect("box node");
        assert_eq!(
            be.element(box_id).expect("el").inline_kf,
            "",
            "tweens never step inline"
        );
    }
}
