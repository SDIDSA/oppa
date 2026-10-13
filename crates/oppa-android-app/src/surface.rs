//! Swapchain presentation: a wgpu `Surface` from the
//! `NativeWindow` + the GPU scene → intermediate texture → blit
//! → `present`.
//!
//! This replaces the software lock/post blit: what the user sees is
//! the GPU-composited swapchain image. The offscreen oracle
//! (`render_pixels` vs CPU, exact-0) still proves the scene bytes —
//! the presented pixels are the oracle pixels by construction.
//!
//! Sizing is dynamic (Round 7.6): the caller discovers the live
//! `NativeWindow` dimensions via [`wait_for_window`], refits the
//! scene to them, and passes them here — no hardcoded scene size,
//! so system-bar insets (e.g. 1080x2290 on a 1080x2400 panel) present
//! instead of bailing to the headless path.
//!
//! The GPU arm is the retained [`SceneState`]'s unified engine
//! (`SceneCore`, `oppa-app`): this module drives it through the
//! engine methods (`ensure_gpu_for_surface_pinned`,
//! `configure_gpu_surface`, `pick_gpu_mode`, `present_gpu`) and
//! never touches a backend directly, so the engine's surface
//! re-homing (theme toggle refit) can never present a destroyed
//! scene id.
//!
//! Window generations (decision 428): the SAF round-trip Stops the
//! activity, which destroys the `ANativeWindow` and recreates a new
//! one on return — same size, different pointer. Holding one
//! `&window`/`wgpu::Surface` for the whole run (the old shape)
//! presents into the dead surface forever after, with no error on
//! the emulator translator to trip the loud path. So the present
//! loop re-acquires the live window every generation: a lost,
//! swapped, or resized window (or Resume/Redraw) rebuilds the
//! surface (re-ensure + reconfigure + refit + repaint + present),
//! and a missing window parks the loop instead of spinning.

use android_activity::ndk::native_window::NativeWindow;
use android_activity::AndroidApp;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

use crate::SceneState;

/// Window + display pair: `NativeWindow` carries only the window
/// half, wgpu surface creation needs both (the display half on
/// Android is a unit value).
struct AndroidWindow<'a> {
    window: &'a NativeWindow,
}

impl<'a> HasWindowHandle for AndroidWindow<'a> {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.window.window_handle()
    }
}

impl<'a> HasDisplayHandle for AndroidWindow<'a> {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::android())
    }
}

/// Owned Android display half for the wgpu instance descriptor
/// (GLES requires the platform display at instance creation when
/// presenting; the value is a unit — owned, `Send + Sync`, `'static`
/// as `WgpuHasDisplayHandle` demands).
#[derive(Debug, Clone, Copy)]
struct AndroidDisplay;

impl HasDisplayHandle for AndroidDisplay {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::android())
    }
}

/// Probe-surface factory over the instance table (decision 432 —
/// the shared ladder's platform piece): one `wgpu` instance per
/// backend row, created once and reused across window generations
/// (the adapter/device re-ensure still runs per generation — the
/// old device was bound to the old surface). Instances are
/// box-leaked on first creation (the desktop leak precedent — the
/// table never shrank anyway, so the process-lifetime set is
/// identical; `'static` refs keep the returned surface independent
/// of table reallocs, which a plain vec-element borrow cannot
/// prove). The window wrapper stays caller-owned: the factory only
/// copies its `&` out, so the surface never borrows factory-built
/// locals.
struct ProofSurfaceFactory<'w> {
    instances: &'w mut Vec<(wgpu::Backends, &'static wgpu::Instance)>,
    wrap: &'w AndroidWindow<'w>,
}

impl<'x> oppa_app::SurfaceFactory<'x> for ProofSurfaceFactory<'x> {
    fn make_surface(
        &mut self,
        backends: wgpu::Backends,
    ) -> Result<oppa_app::BuiltSurface<'x>, String> {
        if self.instances.iter().all(|(b, _)| *b != backends) {
            // GLES requires the platform display handle at
            // instance creation when presenting (wgpu
            // contract); Vulkan ignores it. The Android
            // display half is a unit value, owned so it can
            // live in the descriptor.
            let mut desc =
                wgpu::InstanceDescriptor::new_with_display_handle(Box::new(AndroidDisplay));
            desc.backends = backends;
            let leaked: &'static wgpu::Instance = Box::leak(Box::new(wgpu::Instance::new(desc)));
            self.instances.push((backends, leaked));
        }
        let instance: &'x wgpu::Instance = self
            .instances
            .iter()
            .find(|(b, _)| *b == backends)
            .map(|(_, instance)| *instance)
            .expect("instance row recorded above");
        let surface = instance
            .create_surface(self.wrap)
            .map_err(|e| format!("{e:?}"))?;
        Ok(oppa_app::BuiltSurface { instance, surface })
    }
}

/// The live presenter for one window generation (decision 433 —
/// the generic runner's shape): the ladder's GPU win (swapchain
/// surface + adapter record) or the terminal CPU row (no surface,
/// no GPU state — frames post the engine `rgba8` pixmap through
/// the native-window lock).
enum ProofPresenter<'a> {
    Gpu {
        surface: wgpu::Surface<'a>,
        adapter: String,
    },
    Cpu,
}

/// Presents one proof frame through the live presenter (decision
/// 433): the GPU swapchain with its cpu-ms report, or the CPU
/// pixmap through the native-window lock (wall-timed like the GPU
/// report so `present_cpu_avg_ms` stays comparable). Returns the ms
/// to accumulate into `present_ms`.
fn present_proof_frame(
    state: &mut SceneState,
    presenter: &ProofPresenter,
    window: &NativeWindow,
    tag: &str,
    why: &str,
) -> Result<f64, String> {
    match presenter {
        ProofPresenter::Gpu { surface, adapter } => {
            let report = state
                .scene
                .present_gpu(surface)
                .map_err(|e| format!("{tag} {why} [{adapter}]: {e}"))?;
            Ok(report.cpu_ms)
        }
        ProofPresenter::Cpu => {
            let t = std::time::Instant::now();
            oppa_android_runner::soft_present::present_software(&state.scene, window)
                .map_err(|e| format!("{tag} {why} [CPU]: {e}"))?;
            Ok(t.elapsed().as_secs_f64() * 1000.0)
        }
    }
}

/// Waits for the live `NativeWindow` (up to 30s), requests immersive
/// mode up front (system bars inset the window on phones — the app
/// first asks for its full window), and returns the window with its
/// LIVE dimensions. Loud `Err` when no window ever arrives (the
/// caller falls back to the headless tap phase — CPU-proven flips,
/// no on-screen proof).
///
/// Sizing settles (decision 396): the immersive request crosses to
/// the UI thread and lands asynchronously — measuring the first
/// sighting freezes a pre-immersive inset size (observed 1080x2290
/// stuck while the OS window later reached full bleed). So after the
/// request this samples the size until three consecutive samples
/// agree or 5s pass, and returns the latest: a growing window is
/// followed to rest, an unmoving one costs one extra second and a
/// loud log line, never a hang.
pub fn wait_for_window(
    app: &AndroidApp,
    log: &mut Vec<String>,
) -> Result<(NativeWindow, u32, u32), String> {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(30);
    let window = loop {
        if let Some(w) = app.native_window() {
            break w;
        }
        if Instant::now() > deadline {
            return Err("no native window within 30s".to_string());
        }
        app.poll_events(Some(Duration::from_millis(200)), |_| {});
    };
    // Immersive request BEFORE measuring: a decor view that is not
    // attached yet swallows the flags (success, no effect), so the
    // present loop below re-asserts — the size afterward is the only
    // truth, and whatever it is becomes the scene size (Round 7.6:
    // never a silent scale, never a size bailout).
    let imm = oppa_android_runner::imm_mode::hide_system_bars(app, super::JAVA_PACKAGE)
        .map(|s| format!("{s} "))
        .unwrap_or_else(|e| format!("immersive refused [{e}] "));
    let settle_end = Instant::now() + Duration::from_secs(5);
    let mut last = (0u32, 0u32);
    let mut steady = 0u32;
    let (ww, wh) = loop {
        let (cw, ch) = (window.width().max(1) as u32, window.height().max(1) as u32);
        if (cw, ch) == last {
            steady += 1;
        } else {
            last = (cw, ch);
            steady = 0;
        }
        if steady >= 3 || Instant::now() > settle_end {
            break (cw, ch);
        }
        // Pump, never sleep: lifecycle events (pause/destroy) keep
        // draining while the size settles.
        app.poll_events(Some(Duration::from_millis(200)), |_| {});
    };
    let settled_note = if steady >= 3 {
        "settled".to_string()
    } else {
        "settle-timeout (size may still be pre-immersive)".to_string()
    };
    log.push(format!("{imm}window={ww}x{wh} {settled_note}"));
    eprintln!("oppa-phase: window {ww}x{wh}");
    Ok((window, ww, wh))
}

/// Identity of the live window: the raw `ANativeWindow` pointer
/// behind the handle. A Stop/Resume round-trip recreates the window
/// at the same size, so size alone cannot detect the swap — the
/// pointer can (`None` when the handle refuses to resolve, which
/// also forces a heal rather than trusting a stale surface).
fn window_ptr(window: &NativeWindow) -> Option<usize> {
    let handle = window.window_handle().ok()?;
    match handle.as_raw() {
        raw_window_handle::RawWindowHandle::AndroidNdk(h) => {
            Some(h.a_native_window.as_ptr() as usize)
        }
        _ => None,
    }
}

/// One lifecycle poll: folds Pause/Stop into the persistent `paused`
/// flag (the old per-iteration local reset every pass, so Pause
/// never stuck and Stop was ignored entirely) and returns the edge
/// flags the loop acts on once. `heal` forces a full swapchain
/// rebuild (return paths — Resume/Start/Terminate); `redraw` only
/// asks for a repaint + present on the live surface (same-size
/// WindowResized/ContentRectChanged/RedrawNeeded/InitWindow edges —
/// the emulator repeats WindowResized ~1 Hz with no size change, so
/// rebuilding on it thrashed a ~700 ms re-ensure per pass).
struct PollOutcome {
    destroyed: bool,
    heal: bool,
    redraw: bool,
}

fn poll_once(app: &AndroidApp, timeout: std::time::Duration, paused: &mut bool) -> PollOutcome {
    use android_activity::{MainEvent, PollEvent};
    let mut out = PollOutcome {
        destroyed: false,
        heal: false,
        redraw: false,
    };
    app.poll_events(Some(timeout), |event| match event {
        PollEvent::Main(MainEvent::Destroy) => {
            out.destroyed = true;
        }
        PollEvent::Main(MainEvent::Pause) | PollEvent::Main(MainEvent::Stop) => {
            *paused = true;
        }
        PollEvent::Main(MainEvent::Resume { .. }) | PollEvent::Main(MainEvent::Start) => {
            *paused = false;
            out.heal = true;
        }
        PollEvent::Main(MainEvent::TerminateWindow { .. }) => {
            out.heal = true;
        }
        PollEvent::Main(MainEvent::InitWindow { .. })
        | PollEvent::Main(MainEvent::WindowResized { .. })
        | PollEvent::Main(MainEvent::RedrawNeeded { .. })
        | PollEvent::Main(MainEvent::ContentRectChanged { .. }) => {
            out.redraw = true;
        }
        _ => {}
    });
    out
}

/// Liveness beat (logcat only — above the pause/present checks, so
/// it proves the loop iterates, never that frames present; the
/// `window=`/`paused=` tail says which).
fn maybe_beat(
    t_open: &std::time::Instant,
    last_beat: &mut std::time::Instant,
    batches: u32,
    heals: u32,
    paused: bool,
    window: bool,
) {
    use std::time::Duration;
    if last_beat.elapsed() > Duration::from_secs(10) {
        eprintln!(
            "oppa-loop: alive +{:.0}s batches={batches} heals={heals} paused={paused} window={window}",
            t_open.elapsed().as_secs_f64()
        );
        *last_beat = std::time::Instant::now();
    }
}

/// Ticks the sink's pick registry once (decision 431): an async
/// settlement publishes into the component signals without another
/// press. Returns the settlement count — the loop treats non-zero
/// as activity (step + present that iteration, so the fresh caption
/// reaches the screen even in the event-paced phase). Pure queries
/// when nothing is outstanding (no backend traffic, never a dialog).
fn drain_picks(state: &SceneState) -> usize {
    let Some(dialogs) = state.dialogs.as_ref() else {
        return 0;
    };
    dialogs.borrow().registry.borrow_mut().poll_all()
}

/// Presents the engine's GPU scene twin on the visible window
/// through the real swapchain and drives the tap phase: GL row first
/// — the proven offscreen row on this stack — then Vulkan; both
/// attempts recorded. `width`/`height` seed the run record (the live
/// window size is adopted per generation — presenting at a stale
/// size is a protocol error, so each generation configures exactly
/// what the live window reports). After two initial presents (the
/// durable visible image), polls activity + touch input until the
/// budget elapses (`Some`) or the activity is destroyed — `None`
/// runs interactive-until-Destroy — running one
/// [`crate::apply_tap_batch`] per command batch and presenting
/// whenever it reports a change. `log` collects one line per batch
/// (plus open/close lines): the caller appends it under the per-batch
/// oracle records in `taps.txt` (decision 399), never over them.
///
/// `benchmark` arms the decision-393 uncapped loop (bounded phase
/// only): the activity poll never sleeps (`Duration::ZERO`, the
/// desktop `pump_wait_timeout_ms → Some(0)` rule), the step runs
/// every frame even with no input (settle → full repaint, both
/// arms), and every frame presents + ticks the badge clock — the
/// overlay badge then reads live present FPS instead of tap-cadence
/// values. The swapchain configures the first offered uncapped mode
/// (`SceneCore::pick_uncapped_mode`: Immediate → Mailbox) with a loud
/// fallback to vsync Fifo where the translator offers nothing else.
/// Every present and the presenter-name publish go through the
/// engine's host (`note_frame_presented` /
/// `set_presenter_name`): the presenter name publishes `Gpu/GL` /
/// `Gpu/Vulkan` once the winning row is known, so the badge can
/// never disagree with the pixels on screen.
///
/// Window generations (decision 428): each pass re-acquires the
/// live window — a lost/swapped/resized window, or a Resume/Start/
/// terminate edge, rebuilds the swapchain (re-ensure + reconfigure +
/// refit + repaint + present, counted as a heal) instead of
/// presenting into the dead surface; same-window redraw edges
/// (repeating WindowResized with no size change, content-rect,
/// redraw-needed — rebuilding on those thrashed) repaint + present
/// on the live surface with no rebuild; a missing window parks the
/// loop (100 ms sleeps, never a hot spin, benchmark or not) instead
/// of failing it. In-generation present failures stay loud `Err`s (a black
/// screen is then an honest failure in `error.txt`, never a silent
/// fallback to the removed blit).
///
/// The present always goes through `SceneCore::present_gpu`, which
/// addresses the engine's CURRENT GPU surface id: a theme rebase
/// re-homes the scene surfaces inside the engine (refit on toggle),
/// so no scene id is ever threaded here to go stale — presenting a
/// destroyed surface (the old failure mode) cannot happen.
#[allow(clippy::too_many_arguments)]
pub fn drive_present_loop(
    app: &AndroidApp,
    state: &mut SceneState,
    width: u32,
    height: u32,
    touch: &mut oppa_android_runner::touch::TouchDriver,
    budget_secs: Option<u64>,
    log: &mut Vec<String>,
    benchmark: bool,
    density: f32,
    tap_lines: &mut Vec<String>,
    taps_path: &std::path::Path,
) -> Result<String, String> {
    use std::time::{Duration, Instant};
    let t_open = Instant::now();
    // Tap phase: input batches -> engine step -> present on
    // change. `None` budget never expires on time (interactive-
    // until-Destroy — the deadline is only constructed for
    // `Some`, so no `u64::MAX` overflow is possible).
    let end = budget_secs.map(|b| Instant::now() + Duration::from_secs(b));
    let over_budget = || end.map(|e| Instant::now() >= e).unwrap_or(false);
    eprintln!("oppa-phase: presenting {width}x{height} (live window adopted per generation)");
    // wgpu instances, one per backend row, reused across window
    // generations (the adapter/device re-ensure still runs per
    // generation — the old device was bound to the old surface).
    let mut instances: Vec<(wgpu::Backends, &'static wgpu::Instance)> = Vec::new();
    // The recorded winner re-probes first after a heal (a picker
    // trip re-runs GL, not Vulkan-after-GL).
    let mut won: Option<wgpu::Backends> = None;
    let mut paused = false;
    let mut destroyed = false;
    let mut batches = 0u32;
    let mut heals = 0u32;
    let mut presents = 0u32;
    let mut present_ms = 0.0f64;
    let mut attempts: Vec<String> = Vec::new();
    let mut last_beat = Instant::now();
    let mut last_size = (width.max(1), height.max(1));
    let mut last_adapter = "none".to_string();
    let mut last_format = "none".to_string();
    let mut last_mode = "none".to_string();
    // Benchmark never sleeps while presenting (desktop `Some(0)`
    // horizon rule); event-driven waits 200ms per poll. Parked
    // passes (paused/windowless) always sleep — see below.
    let poll_timeout = if benchmark {
        Duration::ZERO
    } else {
        Duration::from_millis(200)
    };
    // Runner-tracked bars mode (boot is hidden/immersive —
    // `wait_for_window` hides up front; heals re-assert this instead
    // of forcing hidden, so a shown-bars run survives turnover).
    let mut bars_visible = false;

    'generation: loop {
        if destroyed || over_budget() {
            break;
        }
        maybe_beat(
            &t_open,
            &mut last_beat,
            batches,
            heals,
            paused,
            app.native_window().is_some(),
        );
        // Acquire the live window, polling lifecycle while waiting
        // so Resume/Pause keep flowing. A missing window (Stop
        // destroyed it, Resume has not recreated it yet — the SAF
        // trip shape) parks here instead of failing the run; the
        // one-shot snapshot this replaces gave up permanently.
        let window: NativeWindow = loop {
            if destroyed || over_budget() {
                break 'generation;
            }
            let out = poll_once(app, Duration::from_millis(100), &mut paused);
            if out.destroyed {
                destroyed = true;
                break 'generation;
            }
            if let Some(w) = app.native_window() {
                if !paused {
                    break w;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let ptr = window_ptr(&window);
        let (ww, hh) = (window.width().max(1) as u32, window.height().max(1) as u32);
        // Ordered presenter ladder (decision 432): GL first, Vulkan
        // once it has won (winner-stickiness — the recorded winner
        // re-probes first after a heal), closed with the terminal
        // CPU row (decision 433: proof frames post the `rgba8`
        // pixmap through the native-window lock instead of failing
        // the run). The benchmark uncapped-mode choice rides the row
        // tail (event-driven phases stay Fifo).
        let default_order = if won == Some(wgpu::Backends::VULKAN) {
            &oppa_app::ANDROID_GPU_ORDER_VULKAN_FIRST
        } else {
            &oppa_app::ANDROID_GPU_ORDER_GL_FIRST
        };
        let rows = oppa_app::presenter_rows(default_order, true);
        let wrap = AndroidWindow { window: &window };
        let mut factory = ProofSurfaceFactory {
            instances: &mut instances,
            wrap: &wrap,
        };
        let mut attempt = move |scene: &mut oppa_app::SceneCore,
                           row: oppa_app::GpuRow,
                           instance: &wgpu::Instance,
                           surface: &wgpu::Surface<'_>|
         -> Result<oppa_app::GpuApplied, oppa_app::RowFail> {
            let tag = format!("{:?}", row.backends);
            // The Android bring-up shape pins the renderer kind
            // (zygote-inherited env must not decide the arm — see
            // `SceneCore::enable_gpu_pinned`): `GpuAuto` records what
            // was attempted, never a silent substitution. The winning
            // backend is published in the badge vocabulary below.
            let adapter = match scene.ensure_gpu_for_surface_pinned(
                oppa_app::RendererKind::GpuAuto,
                instance,
                surface,
            ) {
                Ok(adapter) => {
                    eprintln!("oppa-phase: surface-gpu {tag} {adapter}");
                    adapter
                }
                Err(e) => {
                    return Err(oppa_app::RowFail::Next(format!("{tag} refused [{e}]")));
                }
            };
            let mode = if benchmark {
                scene.pick_uncapped_mode(surface)
            } else {
                wgpu::PresentMode::Fifo
            };
            let format = match scene.configure_gpu_surface(surface, ww, hh, mode) {
                Ok(f) => f,
                Err(e) => {
                    return Err(oppa_app::RowFail::Next(format!(
                        "{tag} configure [{adapter}]: {e}"
                    )));
                }
            };
            Ok(oppa_app::GpuApplied {
                adapter,
                mode,
                format,
            })
        };
        let (tag, presenter, adapter, format, mode) =
            match oppa_app::walk_presenter_rows(&rows, &mut factory, &mut attempt, &mut state.scene)
            {
                Ok(oppa_app::LadderOutcome::Gpu(up)) => {
                    let tag = format!("{:?}", up.backends);
                    // Refused rows join the run record (the old
                    // `attempts` vec rode the walk; the ladder hands
                    // them back on the win).
                    attempts.extend(up.attempts);
                    won = Some(up.backends);
                    // The swapchain row won: publish the presenter
                    // name in the desktop badge vocabulary so
                    // `[Gpu/GL]` / `[Gpu/Vulkan]` can never disagree
                    // with the pixels on screen.
                    state.scene.host().set_presenter_name(
                        if up.backends == wgpu::Backends::GL {
                            "Gpu/GL"
                        } else {
                            "Gpu/Vulkan"
                        },
                    );
                    (
                        tag,
                        ProofPresenter::Gpu {
                            surface: up.surface,
                            adapter: up.adapter.clone(),
                        },
                        up.adapter,
                        up.format,
                        up.mode,
                    )
                }
                // Terminal CPU row (decision 433): no surface, no
                // GPU state — proof frames post below through the
                // native-window lock. `won` is untouched (the next
                // generation re-probes GPU first). Record sentinels
                // are the honest values: RGBA8 pixmap pixels,
                // software pacing, `cpu` adapter.
                Ok(oppa_app::LadderOutcome::Cpu) => {
                    eprintln!("oppa-phase: software present (CPU pixmap)");
                    state.scene.host().set_presenter_name("Cpu");
                    (
                        "cpu".to_string(),
                        ProofPresenter::Cpu,
                        "cpu".to_string(),
                        wgpu::TextureFormat::Rgba8Unorm,
                        wgpu::PresentMode::Fifo,
                    )
                }
                Err(e) => {
                    // No row served the live window: loud, like the old
                    // no-surface error — unless the window died mid-probe,
                    // which retries instead of failing the run.
                    if app.native_window().is_some() {
                        return Err(format!(
                            "no surface-compatible GPU: {}",
                            e.attempts.join("; ")
                        ));
                    }
                    continue 'generation;
                }
            };
        last_size = (ww, hh);
        last_adapter = adapter.clone();
        last_format = format!("{format:?}");
        last_mode = format!("{mode:?}");
        // Adopt the live size (a recreated window usually matches,
        // but the size is the only truth — Round 7.6 rule).
        if state.scene.viewport() != (ww, hh) {
            state
                .scene
                .resize(ww, hh)
                .map_err(|e| format!("{tag} refit {ww}x{hh}: {e}"))?;
            state
                .scene
                .repaint()
                .map_err(|e| format!("{tag} repaint after refit: {e}"))?;
            touch
                .shell_mut()
                .note_surface_changed(ww as f32 / density, hh as f32 / density);
        }
        // Terminal CPU row: lock the window buffers to the scene
        // size once per generation (a recreated window needs fresh
        // geometry; per-frame posts assume it).
        if matches!(presenter, ProofPresenter::Cpu) {
            oppa_android_runner::soft_present::ensure_software_window(&window, ww, hh)
                .map_err(|e| format!("{tag} heal-setup: {e}"))?;
        }
        // Heal present: repaint through the tap machinery (the
        // blank-guard asserts content — a black heal fails loudly,
        // never silently) and present the fresh frame, so the
        // compositor stops showing black (a never-presented window
        // shows black, not the last frame). The bars mode is
        // re-asserted mode-aware (sticky flags clear on visibility
        // changes; a detached view swallows them silently, so this
        // never fails the run).
        oppa_android_runner::imm_mode::drain_system_bars_request(
            state.scene.host(),
            app,
            super::JAVA_PACKAGE,
            &mut bars_visible,
        );
        oppa_android_runner::imm_mode::apply_system_bars(app, super::JAVA_PACKAGE, bars_visible);
        crate::apply_tap_batch(state, &[], density, tap_lines, taps_path)?;
        present_ms += present_proof_frame(state, &presenter, &window, &tag, "heal-present")?;
        presents += 1;
        state.scene.note_frame_presented();
        heals += 1;
        if heals == 1 {
            log.push(format!(
                "open surface={ww}x{hh} format={format:?} present_mode={mode:?} benchmark={benchmark} adapter={adapter}"
            ));
        } else {
            log.push(format!(
                "reopen heal={heals} window={ww}x{hh} format={format:?} present_mode={mode:?} adapter={adapter}"
            ));
        }
        // Steady loop on this generation's surface until the budget
        // ends, the activity dies, or the window turns over.
        loop {
            if destroyed || over_budget() {
                break 'generation;
            }
            maybe_beat(
                &t_open,
                &mut last_beat,
                batches,
                heals,
                paused,
                app.native_window().is_some(),
            );
            let out = poll_once(app, poll_timeout, &mut paused);
            if out.destroyed {
                destroyed = true;
                break 'generation;
            }
            if out.heal {
                break;
            }
            if paused {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            // Same live window still? A swap or resize (or loss)
            // heals through the next generation instead of
            // presenting into the dead surface.
            let same = match app.native_window() {
                None => false,
                Some(w) => {
                    window_ptr(&w) == ptr
                        && (w.width().max(1) as u32, w.height().max(1) as u32) == (ww, hh)
                }
            };
            if !same {
                break;
            }
            // Soft redraw (same window, same size): repaint + present
            // on the live surface, no rebuild — and no immersive
            // re-assert (that is what feeds the WindowResized echo).
            if out.redraw {
                crate::apply_tap_batch(state, &[], density, tap_lines, taps_path)?;
                present_ms +=
                    present_proof_frame(state, &presenter, &window, &tag, "redraw-present")?;
                presents += 1;
                state.scene.note_frame_presented();
            }
            let cmds = touch
                .pump_activity(app)
                .map_err(|e| format!("touch pump: {e}"))?;
            // Benchmark steps every frame even with no input (the
            // settle inside re-runs the throttled badge refresh, the
            // repaint picks it up, the present below clocks it);
            // event-driven skips idle frames entirely — except a
            // freshly drained pick settlement, which steps + presents
            // so the caption reaches the screen with no press.
            let drained = drain_picks(state);
            let idle = cmds.is_empty() && drained == 0;
            if idle && !benchmark {
                continue;
            }
            if !idle {
                batches += 1;
            }
            let elapsed_ms = t_open.elapsed().as_secs_f64() * 1000.0;
            let (changed, note, _gpu_scene) =
                crate::apply_tap_batch(state, &cmds, density, tap_lines, taps_path)?;
            if !idle {
                log.push(format!("batch={batches} t={elapsed_ms:.0}ms {note}"));
            }
            if drained > 0 {
                log.push(format!(
                    "drained={drained} t={elapsed_ms:.0}ms pick-settlement"
                ));
            }
            if changed || benchmark {
                // Mode-aware bars re-assert (sticky flags clear —
                // the old unconditional hide would fight a shown-bars
                // app every presented frame; drains ride along so tap
                // scripts can flip the mode mid-run).
                oppa_android_runner::imm_mode::drain_system_bars_request(
                    state.scene.host(),
                    app,
                    super::JAVA_PACKAGE,
                    &mut bars_visible,
                );
                oppa_android_runner::imm_mode::apply_system_bars(app, super::JAVA_PACKAGE, bars_visible);
                // The engine presents its CURRENT surface (a theme
                // toggle refit re-homes it inside the engine — no id
                // rides this loop to go stale) or, on the CPU row,
                // the fresh pixmap through the window lock.
                present_ms +=
                    present_proof_frame(state, &presenter, &window, &tag, "re-present")?;
                presents += 1;
                state.scene.note_frame_presented();
            }
        }
    }
    let s = touch.stats_summary();
    Ok(format!(
        "surface={}x{} format={} present_mode={} benchmark={benchmark} adapter={} attempts=[{}] presents={presents} present_cpu_avg_ms={:.1} batches={batches} heals={heals} destroyed={destroyed} {s}",
        last_size.0,
        last_size.1,
        last_format,
        last_mode,
        last_adapter,
        attempts.join(", "),
        if presents > 0 {
            present_ms / presents as f64
        } else {
            0.0
        },
    ))
}
