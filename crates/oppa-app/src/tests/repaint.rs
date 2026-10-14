use super::*;

#[test]
fn continuous_repaint_override_parses_flag_values() {
    assert!(!parse_continuous_repaint_override(None));
    assert!(!parse_continuous_repaint_override(Some("")));
    assert!(!parse_continuous_repaint_override(Some("0")));
    assert!(!parse_continuous_repaint_override(Some("no")));
    assert!(!parse_continuous_repaint_override(Some("yes")));
    assert!(parse_continuous_repaint_override(Some("1")));
    assert!(parse_continuous_repaint_override(Some("true")));
    assert!(parse_continuous_repaint_override(Some("TRUE")));
    assert!(parse_continuous_repaint_override(Some("  True  ")));
}

#[test]
fn window_options_continuous_repaint_defaults_off_and_builder_sets() {
    let opts = WindowOptions::new("Bench", 400, 300);
    assert!(!opts.continuous_repaint, "event-driven by default");
    let opts = opts.with_continuous_repaint(true);
    assert!(opts.continuous_repaint, "builder arms benchmark mode");
    let opts = opts.with_continuous_repaint(false);
    assert!(!opts.continuous_repaint, "builder disarms again");
}

#[test]
fn desktop_loop_continuous_repaint_horizon_and_rearm() {
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Plain", PlainProps, plain_app);
    assert!(!loop_.continuous_repaint(), "event-driven by default");
    assert_eq!(
        loop_.pump_wait_timeout_ms(None),
        None,
        "idle loop blocks until input"
    );
    assert_eq!(
        loop_.pump_wait_timeout_ms(Some(8)),
        Some(8),
        "busy horizon passes through"
    );
    // Rearm is a quiet no-op unless armed (the event-driven path is
    // exactly as before).
    assert!(!loop_.host().has_demand(), "settled mount holds no demand");
    loop_.rearm_continuous_frame();
    assert!(
        !loop_.host().has_demand(),
        "disarmed rearm requests nothing"
    );
    // Armed: 0ms horizon preempts every idle/busy wait, and every
    // rearm marks demand the settle path turns back into frames.
    loop_.set_continuous_repaint(true);
    assert!(loop_.continuous_repaint(), "setter arms benchmark mode");
    assert_eq!(
        loop_.pump_wait_timeout_ms(None),
        Some(0),
        "armed horizon polls immediately"
    );
    assert_eq!(
        loop_.pump_wait_timeout_ms(Some(8)),
        Some(0),
        "armed horizon preempts busy waits"
    );
    loop_.rearm_continuous_frame();
    assert!(loop_.host().has_demand(), "rearm marks the scene dirty");
    assert!(
        loop_.host().run_until_idle() > 0,
        "demand settles into frames the pump repaints"
    );
    // Disarm restores the event-driven horizon.
    loop_.set_continuous_repaint(false);
    assert_eq!(
        loop_.pump_wait_timeout_ms(None),
        None,
        "disarmed loop blocks again"
    );
}

#[test]
fn continuous_repaint_runs_with_debug_overlay_enabled() {
    // Benchmark mode is orthogonal to the debug overlay (decision
    // 385 wrap): horizon, demand, and repaint behave identically
    // with the badge mounted.
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.host().set_debug_overlay(DebugOverlayConfig::all());
    loop_.host().set_presenter_name("Test");
    loop_.mount("Plain", PlainProps, plain_app);
    loop_.set_continuous_repaint(true);
    assert_eq!(
        loop_.pump_wait_timeout_ms(None),
        Some(0),
        "armed horizon polls immediately under the overlay"
    );
    loop_.repaint().expect("paints with overlay mounted");
    loop_.rearm_continuous_frame();
    assert!(loop_.host().has_demand(), "rearm marks demand");
    assert!(
        loop_.host().run_until_idle() > 0,
        "demand settles into frames"
    );
    loop_.repaint().expect("repaints the next frame");
}

#[test]
fn note_frame_presented_ticks_meter_and_refreshes_badge_throttled() {
    // The benchmark-idle artifact end to end: presents drive the FPS
    // clock (never reconciles), and the badge text refreshes through
    // a throttled root re-run — no 0 FPS after idle gaps.
    let clock = Rc::new(oppa::MockClock::new());
    let loop_ = DesktopLoop::with_clock(200, 150, Box::new(FakeText), "Test", clock.clone())
        .expect("headless loop builds");
    loop_.host().set_debug_overlay(DebugOverlayConfig::all());
    loop_.host().set_presenter_name("Test");
    loop_.mount("Plain", PlainProps, plain_app);
    loop_.host().run_until_idle();
    // Disabled guard first: a loop without the overlay notes nothing.
    loop_
        .host()
        .set_debug_overlay(DebugOverlayConfig::disabled());
    loop_.note_frame_presented();
    assert_eq!(
        loop_.host().run_until_idle(),
        0,
        "disabled overlay schedules no refresh"
    );
    loop_.host().set_debug_overlay(DebugOverlayConfig::all());
    // Rapid presents tick the clock but stay inside the throttle.
    loop_.note_frame_presented();
    clock.advance(0.1);
    loop_.note_frame_presented();
    assert_eq!(
        loop_.host().run_until_idle(),
        0,
        "no root re-run inside the throttle window"
    );
    // Past the throttle the next present schedules the refresh, and
    // the rebuilt badge names the present cadence + presenter.
    clock.advance(0.3);
    loop_.note_frame_presented();
    assert!(
        loop_.host().run_until_idle() > 0,
        "throttled present schedules the badge refresh"
    );
    let badge = loop_
        .host()
        .debug_overlay_badge()
        .expect("badge renders when enabled");
    assert!(badge.contains("FPS"), "badge carries fps, got {badge:?}");
    assert!(
        badge.contains("[Test]"),
        "badge rebuilt after presenter set, got {badge:?}"
    );
    // The regression itself: a 13s idle gap with no presents, then an
    // interaction reconcile, must not move the FPS needle.
    let fps_before = badge
        .split(" FPS")
        .next()
        .expect("fps prefix")
        .rsplit(' ')
        .next()
        .expect("fps number")
        .parse::<f64>()
        .expect("fps parses");
    clock.advance(13.0);
    loop_.host().mark_all_component_effects_dirty();
    loop_.host().run_until_idle();
    let badge_after = loop_
        .host()
        .debug_overlay_badge()
        .expect("badge still renders");
    assert!(
        badge_after.starts_with(&format!("{fps_before:.0} FPS")),
        "idle-gap reconcile leaves FPS alone, got {badge_after:?}"
    );
}

#[test]
fn desktop_with_hook_configures_live_loop() {
    // The `run_desktop_with` hook contract, headlessly: the same
    // `FnOnce(&mut DesktopLoop)` closure shape the runners invoke
    // after mount installs a close veto that `close_requested`
    // consults (the cookbook's dirty-gated veto).
    let mut loop_ = DesktopLoop::new(400, 300, Box::new(FakeText), "Hook").expect("headless loop");
    let dirty = loop_.host().runtime().signal(true);
    let gate = dirty.clone();
    let configure = |loop_: &mut DesktopLoop| {
        loop_.set_close_handler(Rc::new(move || !gate.get()));
    };
    configure(&mut loop_);
    assert!(!loop_.close_requested(), "dirty vetoes close");
    dirty.set(false);
    assert!(loop_.close_requested(), "clean closes");
}

#[test]
fn poll_hook_runs_restores_and_removes() {
    // Round 26.2 (decision 342): installed hooks run once per
    // `run_poll_hook` call against the live loop, survive the
    // call (take-call-restore), and `None` removes them.
    let mut loop_ = DesktopLoop::new(400, 300, Box::new(FakeText), "Hook").expect("headless loop");
    let fires = Rc::new(std::cell::Cell::new(0usize));
    let count = fires.clone();
    loop_.set_poll_hook(Some(Box::new(move |loop_: &mut DesktopLoop| {
        count.set(count.get() + 1);
        // The hook observes the live loop (viewport here;
        // Task Studio reads hooks signals the same way).
        let _ = loop_.viewport();
    })));
    loop_.run_poll_hook();
    loop_.run_poll_hook();
    assert_eq!(fires.get(), 2, "hook runs every invocation");
    loop_.set_poll_hook(None);
    loop_.run_poll_hook();
    assert_eq!(fires.get(), 2, "removal stops calls");
}

#[test]
fn close_flag_drains_through_veto_consult() {
    // The runner contract, headlessly: a drained component
    // request consults the veto -- refused while dirty, exit
    // once clean (what both pumps do per iteration).
    let mut loop_ = DesktopLoop::new(400, 300, Box::new(FakeText), "Hook").expect("headless loop");
    let dirty = loop_.host().runtime().signal(true);
    let gate = dirty.clone();
    loop_.set_close_handler(Rc::new(move || !gate.get()));
    loop_.host().request_close();
    assert!(loop_.host().take_close_request(), "flag drains");
    assert!(!loop_.close_requested(), "dirty veto holds");
    dirty.set(false);
    loop_.host().request_close();
    assert!(loop_.host().take_close_request(), "flag drains again");
    assert!(loop_.close_requested(), "clean exits");
}

/// Headless loops start CPU-only with both scene surfaces live
/// (GPU twin untouched until a runner enables GPU — a faceless
/// shaper must never trip the GPU loud-no-face rule on the CPU
/// path).
#[test]
fn loop_starts_cpu_only_with_both_surfaces() {
    let (mut loop_, _) = harness();
    assert_eq!(loop_.renderer_kind(), RendererKind::Cpu);
    assert!(!loop_.is_gpu());
    assert_eq!(loop_.renderer_name(), "cpu");
    assert_eq!(loop_.gpu_present_mode(), None);
    let damage = loop_.repaint().expect("repaints");
    assert!(damage > 0);
    assert!(loop_.cpu_retained_op_count().unwrap_or(0) > 0);
    assert_eq!(
        loop_.gpu_retained_op_count(),
        Some(0),
        "CPU-only repaints leave the GPU twin untouched"
    );
    assert_eq!(loop_.rgba8().expect("pixels").len(), 200 * 150 * 4);
}

/// Text-free scene for GPU paint tests (a Text run without an
/// injected face would refuse loudly on GPU by contract, while
/// the CPU path still draws bars — so the dual test stays
/// faceless by having no text at all).
#[derive(Clone)]
struct PlainProps;
impl Props for PlainProps {}

fn plain_app(_ctx: &Ctx, _props: &PlainProps) -> VNode {
    Div("root")
        .style(Style::new().size(200, 150).bg(Color(0xFF_FF_FF)))
        .child(
            Div("plate")
                .style(Style::new().size(96, 32).bg(Color(0x22_66_CC)))
                .build(),
        )
}

/// Opting out of the CPU paint skips the pixmap while the GPU twin
/// keeps encoding (decision 436): after `set_cpu_paint(false)` a
/// damaging remount leaves the pixmap bytes untouched and the paint
/// counter still (the twin encodes regardless — it has no opt-out);
/// `disable_gpu` repaints the CPU catch-up so the fallback never
/// serves stale pixels.
#[test]
fn cpu_paint_opt_out_skips_pixmap_keeps_twin() {
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Plain", PlainProps, plain_app);
    loop_.enable_gpu(wgpu::PresentMode::Fifo);
    let damage = loop_.repaint().expect("gpu repaint works");
    assert!(damage > 0);
    assert!(loop_.gpu_retained_op_count().unwrap_or(0) > 0);
    let pixmap_before = loop_.rgba8().expect("pixels").to_vec();
    let paints_before = loop_.cpu_paints_total();
    assert!(paints_before > 0);
    // Same root name, red plate: reconciler updates in place (dirty).
    loop_.mount("Plain", PlainProps, red_app);
    loop_.scene_mut().set_cpu_paint(false);
    let damage = loop_.repaint().expect("repaint with cpu opted out works");
    assert!(damage > 0, "the recolor damages");
    assert_eq!(
        loop_.cpu_paints_total(),
        paints_before,
        "opted-out repaint never reaches CpuBackend::paint"
    );
    assert_eq!(
        loop_.rgba8().expect("pixels"),
        pixmap_before.as_slice(),
        "skipped paint leaves the pixmap bytes untouched"
    );
    assert!(
        loop_.gpu_retained_op_count().unwrap_or(0) > 0,
        "twin still holds an encoded scene"
    );
    loop_
        .disable_gpu()
        .expect("disable repaints the cpu catch-up");
    assert_eq!(
        loop_.cpu_paints_total(),
        paints_before + 1,
        "fallback repaints the pixmap exactly once"
    );
    assert_ne!(
        loop_.rgba8().expect("pixels"),
        pixmap_before.as_slice(),
        "catch-up repaint picks up the red plate"
    );
}

/// Desktop GPU-serving runs skip the CPU pixmap too (decision
/// 473 — the decision-436 opt-out extended past Android): after the
/// bring-up shape (`enable_gpu` + `set_cpu_paint(false)`) damaging
/// repaints leave the pixmap bytes and the paint counter still, and
/// `refresh_cpu_pixmap` repaints the fallback exactly once without
/// leaving GPU mode (counter +1, bytes catch up, still GPU-active
/// so the next frame retries the swapchain).
#[test]
fn desktop_cpu_paint_opt_out_skips_and_refreshes_once() {
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Plain", PlainProps, plain_app);
    loop_.enable_gpu(wgpu::PresentMode::Fifo);
    loop_.repaint().expect("gpu repaint works");
    let pixmap_before = loop_.rgba8().expect("pixels").to_vec();
    let paints_before = loop_.cpu_paints_total();
    assert!(paints_before > 0);
    // Bring-up shape: GPU primary takes over, CPU pixmap retires.
    loop_.scene_mut().set_cpu_paint(false);
    loop_.mount("Plain", PlainProps, red_app);
    loop_.repaint().expect("repaint with cpu opted out works");
    assert_eq!(
        loop_.cpu_paints_total(),
        paints_before,
        "opted-out repaint never reaches CpuBackend::paint"
    );
    assert_eq!(
        loop_.rgba8().expect("pixels"),
        pixmap_before.as_slice(),
        "skipped paint leaves the pixmap bytes untouched"
    );
    // Transient-failure shape: one fresh blit source, GPU mode kept.
    loop_
        .scene_mut()
        .refresh_cpu_pixmap()
        .expect("catch-up repaint works");
    assert_eq!(
        loop_.cpu_paints_total(),
        paints_before + 1,
        "refresh repaints the pixmap exactly once"
    );
    assert_ne!(
        loop_.rgba8().expect("pixels"),
        pixmap_before.as_slice(),
        "catch-up repaint picks up the red plate"
    );
    assert!(loop_.is_gpu(), "refresh never leaves GPU mode");
    // Second refresh with the paint back on is a quiet no-op.
    loop_.scene_mut().set_cpu_paint(true);
    loop_
        .scene_mut()
        .refresh_cpu_pixmap()
        .expect("no-op refresh works");
    assert_eq!(
        loop_.cpu_paints_total(),
        paints_before + 1,
        "refresh with the paint on touches nothing"
    );
}

/// Red-plate twin of [`plain_app`] (same node names, so remounting
/// over it updates in place instead of rebuilding).
fn red_app(_ctx: &Ctx, _props: &PlainProps) -> VNode {
    Div("root")
        .style(Style::new().size(200, 150).bg(Color(0xFF_FF_FF)))
        .child(
            Div("plate")
                .style(Style::new().size(96, 32).bg(Color(0xCC_22_22)))
                .build(),
        )
}

/// Enabling GPU paints the same frame plan into both backends
/// (CPU pixmap stays the fallback source); disabling returns to
/// CPU-only without losing the viewport. A bare `enable_gpu` with
/// no override resolves to the GPU automatic default.
#[test]
fn enable_gpu_paints_both_backends_then_disables() {
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Plain", PlainProps, plain_app);
    loop_.enable_gpu(wgpu::PresentMode::Fifo);
    assert!(loop_.is_gpu());
    assert_eq!(loop_.renderer_kind(), RendererKind::GpuAuto);
    assert_eq!(loop_.renderer_name(), "gpu-auto");
    assert_eq!(loop_.gpu_present_mode(), Some(wgpu::PresentMode::Fifo));
    let damage = loop_.repaint().expect("gpu repaint works");
    assert!(damage > 0);
    assert!(loop_.cpu_retained_op_count().unwrap_or(0) > 0);
    assert!(
        loop_.gpu_retained_op_count().unwrap_or(0) > 0,
        "GPU twin encodes the same plan"
    );
    assert_eq!(loop_.rgba8().expect("pixels").len(), 200 * 150 * 4);
    loop_
        .disable_gpu()
        .expect("disable repaints the cpu catch-up");
    assert!(!loop_.is_gpu());
    assert_eq!(loop_.gpu_present_mode(), None);
    loop_.repaint().expect("cpu repaint after disable works");
}

#[test]
fn enable_gpu_paints_and_retains() {
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Plain", PlainProps, plain_app);
    loop_.enable_gpu(wgpu::PresentMode::Fifo);
    assert!(loop_.is_gpu());
    assert_eq!(loop_.renderer_name(), "gpu-auto");
    assert_eq!(loop_.renderer_kind(), RendererKind::GpuAuto);
    let damage = loop_.repaint().expect("gpu repaint works");
    assert!(damage > 0);
    assert!(loop_.cpu_retained_op_count().unwrap_or(0) > 0);
    assert!(
        loop_.gpu_retained_op_count().unwrap_or(0) > 0,
        "GPU backend encodes and retains ops"
    );
    loop_
        .disable_gpu()
        .expect("disable repaints the cpu catch-up");
    assert!(!loop_.is_gpu());
    assert_eq!(loop_.renderer_name(), "cpu");
}

/// Refit keeps both surfaces in lockstep (a later `enable_gpu`
/// never presents a stale size).
#[test]
fn resize_refits_both_surfaces_in_lockstep() {
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Plain", PlainProps, plain_app);
    loop_.enable_gpu(wgpu::PresentMode::Fifo);
    loop_.repaint().expect("paints");
    loop_.resize(400, 300).expect("resize works");
    assert_eq!(loop_.viewport(), (400, 300));
    loop_.repaint().expect("paints at the new size");
    assert_eq!(loop_.rgba8().expect("pixels").len(), 400 * 300 * 4);
    assert!(loop_.gpu_retained_op_count().unwrap_or(0) > 0);
}

/// Decision 516: repaint paints TIME-evaluated values (live tweens
/// mid-flight) — the retained build showed only targets, so every
/// desktop tween jumped start→end in one present (the motion-lab
/// report). Would fail pre-fix (mid-flight pixel reads the target,
/// never gray). Near-black target (pure `0x000000` is value-identical
/// to `TRANSPARENT` and snaps/skips by the pre-existing M6/505
/// transparency rule — never use it as a tween endpoint).
#[test]
fn repaint_paints_midflight_values() {
    use oppa::{Ease, Transition};
    #[derive(Clone)]
    struct FadeProps {
        dark: oppa::Signal<bool>,
    }
    impl Props for FadeProps {}
    fn fade_app(_ctx: &Ctx, p: &FadeProps) -> VNode {
        Div("fade-root")
            .style(
                Style::new()
                    .size(200, 150)
                    .bg(if p.dark.get() {
                        Color(0x01_01_01)
                    } else {
                        Color(0xFF_FF_FF)
                    })
                    .transition(Transition::new(120, Ease::Linear)),
            )
            .build()
    }
    let clock = Rc::new(oppa::MockClock::new());
    let mut loop_ = DesktopLoop::with_clock(200, 150, Box::new(FakeText), "Test", clock.clone())
        .expect("headless loop builds");
    let dark = loop_.host().runtime().signal(false);
    loop_.mount("Fade", FadeProps { dark: dark.clone() }, fade_app);
    loop_.host().run_until_idle();
    loop_.repaint().expect("entry paints");
    let px = |loop_: &DesktopLoop| {
        let rgba = loop_.rgba8().expect("pixels");
        let o = (75 * 200 + 100) * 4;
        (rgba[o], rgba[o + 1], rgba[o + 2])
    };
    assert_eq!(px(&loop_), (255, 255, 255), "entry white");
    // Flip to black: a 120ms linear tween starts.
    dark.set(true);
    clock.advance(1.0 / 60.0);
    loop_.host().run_once();
    clock.advance(0.06);
    loop_.repaint().expect("mid-flight paints");
    let mid = px(&loop_);
    assert!(
        (0x60..0xA0).contains(&mid.0) && mid.0 == mid.1 && mid.1 == mid.2,
        "mid-flight pixel reads gray, got {mid:?}"
    );
    // Past the end: exact near-black, settled quiet.
    clock.advance(0.2);
    loop_.repaint().expect("settled paints");
    assert_eq!(px(&loop_), (1, 1, 1), "target near-black exact");
}
