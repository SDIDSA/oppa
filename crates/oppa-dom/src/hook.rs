//! PAINT-phase wiring for the DOM backend: same hook shape as the CPU
//! (`oppa-cpu`) and GPU (`oppa-gpu`) hooks — every presenter joins
//! the same phase discipline.
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
        // (Round 15.1's DOM caret bar is gone — decision 472 leaves
        // the caret to the browser — but the plan `Rect` still rides
        // here for the CPU oracle.) Decision 454: the
        // handle pair rides the same hook (plan bar + `Circle` ops +
        // DOM `handle` divs from one host query).
        let selection = host2.focused_selection_paint();
        let caret = host2.focused_caret_paint();
        let handles = host2.focused_handles_paint();
        builder.set_selection(selection);
        builder.set_caret(caret);
        builder.set_handles(handles);
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
            be.set_handles(handles);
            let stats = host2.with_retained_mut(|rec, styles| {
                // Phase 36 PR4 (decision 357): feed the backend's
                // keyframe tracker every frame (settling is
                // unconditional — diff-less animation frames still
                // retire tracks); the stepped path samples it during
                // sync below.
                be.track_animation(&diffs, rec, styles, now);
                // Decisions 521-522: publish live layout track specs
                // (the backend samples them on its own clock — same
                // per-frame publish as every overlay above).
                let specs = host2.with_evaluator(|ev| ev.layout_track_specs());
                be.set_layout_anim(specs);
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
        Color, ComponentHost, Ctx, Div, Ease, KeyframeStop, Keyframes, Props, Signal, Style,
        Transition, VNode,
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
    /// runtime (deterministic clock — no MockClock needed). Returns
    /// the frame's touched work (exit frames must report work, never
    /// quiet — the patch gate).
    fn drive(
        host: &ComponentHost,
        backend: &std::rc::Rc<RefCell<DomBackend>>,
        sheet: &mut StyleSheet,
        cursor: &mut usize,
        now: f64,
    ) -> usize {
        let diffs = host.diffs_from(*cursor);
        {
            let mut be = backend.borrow_mut();
            for diff in &diffs {
                be.commit(diff).expect("commit");
            }
        }
        *cursor = host.diff_count();
        // Decision 520: re-showed exits cancel through the host drain
        // on the presenter leg (`DomPresenter::commit` owns this in
        // production) — the rig mirrors it before sync so the twin
        // never steps a superseded ghost.
        {
            let cancelled = host.drain_cancelled_exits();
            if !cancelled.is_empty() {
                backend.borrow_mut().cancel_kept_exits(&cancelled);
            }
        }
        host.with_retained_mut(|rec, styles| {
            let mut be = backend.borrow_mut();
            be.track_animation(&diffs, rec, styles, now);
            // Decisions 521-522: publish live layout track specs
            // (mirrors the presenter legs above).
            let specs = host.with_evaluator(|ev| ev.layout_track_specs());
            be.set_layout_anim(specs);
            be.sync(rec, styles, sheet).expect("sync").touched
        })
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

    /// Reads the stepped `opacity:` out of an inline style (the exit
    /// fade probe — parses the declaration the backend wrote).
    fn stepped_opacity(inline: &str) -> f32 {
        let (_, rest) = inline
            .split_once("opacity:")
            .expect("stepped inline carries opacity");
        rest.split(';')
            .next()
            .expect("opacity terminated")
            .parse()
            .expect("opacity parses")
    }

    #[derive(Clone)]
    struct ExitProps {
        open: Signal<bool>,
    }
    impl Props for ExitProps {}

    /// Dismissable card: open renders the exit-declaring plate,
    /// closed renders nothing (the overlay dismiss shape).
    fn exit_scene(_ctx: &Ctx, props: &ExitProps) -> VNode {
        if !props.open.get() {
            return VNode::Hole;
        }
        Div("card")
            .style(
                Style::new()
                    .size(100, 50)
                    .bg(Color(0x22_66_CC))
                    .exit(Transition::exit()),
            )
            .build()
    }

    /// Decision 520: dismissal keeps the ghost past removal (stepped
    /// inline fade + click-through + frozen CSS transitions, with
    /// touched work on every fading frame), then drops it at settle;
    /// re-showing mid-exit cancels through the drain.
    #[test]
    fn exit_keeps_ghost_then_drops_at_settle() {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let open = host.runtime().signal(true);
        host.mount("X", ExitProps { open: open.clone() }, exit_scene);
        host.run_until_idle();
        let backend = Rc::new(RefCell::new(DomBackend::new(1.0)));
        let mut sheet = StyleSheet::new(1.0);
        let mut cursor = 0;
        drive(&host, &backend, &mut sheet, &mut cursor, 0.0);
        let id = host
            .with_retained_mut(|rec, _| rec.find_by_debug("card"))
            .into_iter()
            .next()
            .expect("card node");
        // Dismiss: the host settles its own exit in real time; the
        // backend below replays the removal at explicit twin times
        // (deterministic ghost, no wall clock).
        open.set(false);
        host.run_until_idle();
        // Removal frame at twin t=0: ghost kept, click-through,
        // transitions frozen, first stepped values live.
        let touched = drive(&host, &backend, &mut sheet, &mut cursor, 0.0);
        assert!(touched > 0, "keep frame reports work");
        {
            let be = backend.borrow();
            let el = be.element(id).expect("ghost kept past removal");
            assert!(el.no_transition, "stepped fade owns the motion");
            assert!(
                el.inline_kf.contains("pointer-events:none"),
                "ghosts never take input, got {:?}",
                el.inline_kf
            );
            assert!(
                el.inline_kf.contains("opacity:"),
                "first stepped values live, got {:?}",
                el.inline_kf
            );
        }
        // Mid-exit: strictly interior opacity, still stepping work.
        let touched = drive(&host, &backend, &mut sheet, &mut cursor, 0.09);
        assert!(touched > 0, "fading frames report work");
        let mid = {
            let be = backend.borrow();
            be.element(id).expect("ghost mid-exit").inline_kf.clone()
        };
        let op = stepped_opacity(&mid);
        assert!(op > 0.0 && op < 1.0, "ghost fades mid-exit, got {op}");
        // Settle: the ghost drops (deletion is work too).
        let touched = drive(&host, &backend, &mut sheet, &mut cursor, 5.0);
        assert!(touched > 0, "drop frame reports work");
        assert!(
            backend.borrow().element(id).is_none(),
            "ghost drops at settle"
        );
        // Settled frames go quiet again.
        let touched = drive(&host, &backend, &mut sheet, &mut cursor, 6.0);
        assert_eq!(touched, 0, "quiet after the exit");
    }

    /// Decision 520: re-showing mid-exit cancels the ghost through
    /// the host drain (pixel-backend cancel parity — no crossfade
    /// over fresh paint).
    #[test]
    fn exit_reshow_cancels_ghost() {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let open = host.runtime().signal(true);
        host.mount("X", ExitProps { open: open.clone() }, exit_scene);
        host.run_until_idle();
        let backend = Rc::new(RefCell::new(DomBackend::new(1.0)));
        let mut sheet = StyleSheet::new(1.0);
        let mut cursor = 0;
        drive(&host, &backend, &mut sheet, &mut cursor, 0.0);
        let id = host
            .with_retained_mut(|rec, _| rec.find_by_debug("card"))
            .into_iter()
            .next()
            .expect("card node");
        open.set(false);
        // Bounded frames, never idle (production lockstep: the
        // backend pins the same commit the host tracks — idling
        // here would settle+finalize host-side first, evicting the
        // instance the cancel key needs).
        host.run_once();
        drive(&host, &backend, &mut sheet, &mut cursor, 0.0);
        assert!(
            backend.borrow().element(id).is_some(),
            "ghost kept on dismiss"
        );
        // Re-show before the fade lands: the kept ghost releases and
        // the live subtree paints (fresh mount jumps, no track).
        open.set(true);
        host.run_once();
        drive(&host, &backend, &mut sheet, &mut cursor, 0.0);
        assert!(
            backend.borrow().element(id).is_none(),
            "re-show cancels the ghost"
        );
    }

    /// Decisions 521-522: the fence substitutes interpolated geometry
    /// mid-flight (derived inline width strictly interior), reports
    /// touched work every changing frame, and settles back to the
    /// committed target with no clip on leaves.
    #[test]
    fn fence_substitutes_geometry_then_settles() {
        use oppa::MockClock;
        #[derive(Clone)]
        struct FenceProps {
            wide: Signal<bool>,
        }
        impl Props for FenceProps {}
        fn fence_scene(ctx: &Ctx, props: &FenceProps) -> VNode {
            let _ = ctx.signal(0u32);
            let w = if props.wide.get() { 200.0 } else { 100.0 };
            Div("card")
                .style(
                    Style::new()
                        .size(w, 50.0)
                        .bg(Color(0x22_66_CC))
                        .layout_anim(Transition::new(200, Ease::Linear)),
                )
                .build()
        }
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        host.set_viewport(800.0, 600.0);
        let wide = host.runtime().signal(false);
        host.mount("Fence", FenceProps { wide: wide.clone() }, fence_scene);
        host.run_until_idle();
        let backend = Rc::new(RefCell::new(DomBackend::new(1.0)));
        let mut sheet = StyleSheet::new(1.0);
        let mut cursor = 0;
        drive(&host, &backend, &mut sheet, &mut cursor, clock.get());
        let id = host
            .with_retained_mut(|rec, _| rec.find_by_debug("card"))
            .into_iter()
            .next()
            .expect("card node");
        wide.set(true);
        host.run_once();
        clock.advance(0.1);
        host.run_once();
        let touched = drive(&host, &backend, &mut sheet, &mut cursor, clock.get());
        assert!(touched > 0, "fence frames report touched work");
        {
            let be = backend.borrow();
            let geom = be.element(id).expect("el").inline_geom.clone();
            assert!(
                geom.contains("width:150px"),
                "fence substitutes interior geometry, got {geom:?}"
            );
            assert!(
                !geom.contains("overflow:hidden"),
                "leaf fences clip nothing, got {geom:?}"
            );
        }
        clock.advance(1.0);
        host.run_until_idle();
        drive(&host, &backend, &mut sheet, &mut cursor, clock.get());
        {
            let be = backend.borrow();
            let geom = be.element(id).expect("el").inline_geom.clone();
            assert!(
                geom.contains("width:200px"),
                "settled fence derives the target, got {geom:?}"
            );
        }
        assert!(!host.has_demand(), "quiet after the fence");
    }

    /// Decision 521: FLIP steps the offset inline while live (no CSS
    /// transition owns it) and reverts to bare markup at settle.
    #[test]
    fn flip_steps_inline_then_reverts() {
        use oppa::{Column, MockClock};
        #[derive(Clone)]
        struct FlipProps {
            extra: Signal<bool>,
        }
        impl Props for FlipProps {}
        fn flip_scene(ctx: &Ctx, props: &FlipProps) -> VNode {
            let _ = ctx.signal(0u32);
            let anim = Transition::new(200, Ease::Linear);
            let mut kids = Vec::new();
            if props.extra.get() {
                kids.push(Div("extra").style(Style::new().size(100.0, 20.0)).build());
            }
            for (i, name) in ["flip-a", "flip-b"].into_iter().enumerate() {
                kids.push(
                    Div(name)
                        .key(i as u64)
                        .style(Style::new().size(100.0, 20.0).layout_anim(anim))
                        .build(),
                );
            }
            Column::new().children(kids)
        }
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        host.set_viewport(800.0, 600.0);
        let extra = host.runtime().signal(false);
        host.mount(
            "Flip",
            FlipProps {
                extra: extra.clone(),
            },
            flip_scene,
        );
        host.run_until_idle();
        let backend = Rc::new(RefCell::new(DomBackend::new(1.0)));
        let mut sheet = StyleSheet::new(1.0);
        let mut cursor = 0;
        drive(&host, &backend, &mut sheet, &mut cursor, clock.get());
        let row = host
            .with_retained_mut(|rec, _| rec.find_by_debug("flip-a"))
            .into_iter()
            .next()
            .expect("row node");
        extra.set(true);
        host.run_once();
        clock.advance(0.1);
        host.run_once();
        let touched = drive(&host, &backend, &mut sheet, &mut cursor, clock.get());
        assert!(touched > 0, "flip frames report touched work");
        {
            let be = backend.borrow();
            let el = be.element(row).expect("el");
            assert!(
                el.inline_kf.contains("translate("),
                "live flips step inline, got {:?}",
                el.inline_kf
            );
            assert!(
                el.no_transition,
                "stepped offsets freeze the CSS mapping while flipping"
            );
        }
        clock.advance(1.0);
        host.run_until_idle();
        drive(&host, &backend, &mut sheet, &mut cursor, clock.get());
        {
            let be = backend.borrow();
            let el = be.element(row).expect("el");
            assert_eq!(el.inline_kf, "", "settled flips step nothing");
            assert!(!el.no_transition, "the freeze clears at settle");
        }
        assert!(!host.has_demand(), "quiet after the flip");
    }

    /// Decision 522 rung 2: a fenced node with children clips to the
    /// interpolated box (accordion reveal, no per-frame reflow).
    #[test]
    fn fenced_parent_clips_children() {
        use oppa::MockClock;
        #[derive(Clone)]
        struct AccProps {
            wide: Signal<bool>,
        }
        impl Props for AccProps {}
        fn acc_scene(ctx: &Ctx, props: &AccProps) -> VNode {
            let _ = ctx.signal(0u32);
            let w = if props.wide.get() { 200.0 } else { 100.0 };
            Div("acc")
                .style(
                    Style::new()
                        .size(w, 50.0)
                        .bg(Color(0x22_66_CC))
                        .layout_anim(Transition::new(200, Ease::Linear)),
                )
                .child(Div("acc-kid").style(Style::new().size(200.0, 50.0)).build())
        }
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        host.set_viewport(800.0, 600.0);
        let wide = host.runtime().signal(false);
        host.mount("Acc", AccProps { wide: wide.clone() }, acc_scene);
        host.run_until_idle();
        let backend = Rc::new(RefCell::new(DomBackend::new(1.0)));
        let mut sheet = StyleSheet::new(1.0);
        let mut cursor = 0;
        drive(&host, &backend, &mut sheet, &mut cursor, clock.get());
        let id = host
            .with_retained_mut(|rec, _| rec.find_by_debug("acc"))
            .into_iter()
            .next()
            .expect("acc node");
        wide.set(true);
        host.run_once();
        clock.advance(0.1);
        host.run_once();
        drive(&host, &backend, &mut sheet, &mut cursor, clock.get());
        {
            let be = backend.borrow();
            let geom = be.element(id).expect("el").inline_geom.clone();
            assert!(
                geom.contains("overflow:hidden"),
                "fenced parents clip children, got {geom:?}"
            );
        }
        clock.advance(1.0);
        host.run_until_idle();
        drive(&host, &backend, &mut sheet, &mut cursor, clock.get());
        {
            let be = backend.borrow();
            let geom = be.element(id).expect("el").inline_geom.clone();
            assert!(
                geom.contains("width:200px") && !geom.contains("overflow:hidden"),
                "settled accordion derives the bare target, got {geom:?}"
            );
        }
        assert!(!host.has_demand(), "quiet after the reveal");
    }
}
