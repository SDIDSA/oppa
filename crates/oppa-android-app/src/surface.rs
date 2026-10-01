//! Swapchain presentation (v1 remainder, Gap 1): a wgpu `Surface`
//! from the `NativeWindow` + the Vello scene → intermediate texture
//! → blit → `present`.
//!
//! This replaces the software lock/post blit: what the user sees is
//! the GPU-composited swapchain image. The offscreen oracle
//! (`render_pixels` vs CPU, exact-0) still proves the scene bytes —
//! the same `Scene` object feeds both arms, so the presented pixels
//! are the oracle pixels by construction.
//!
//! Sizing is dynamic (Round 7.6): the caller discovers the live
//! `NativeWindow` dimensions via [`wait_for_window`], refits the
//! scene to them, and passes them here — no hardcoded scene size,
//! so system-bar insets (e.g. 1080x2290 on a 1080x2400 panel) present
//! instead of bailing to the headless path.

use android_activity::ndk::native_window::NativeWindow;
use android_activity::AndroidApp;
use oppa_shell_android::events::AndroidCmd;
use oppa_vello::VelloBackend;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

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

/// Waits for the live `NativeWindow` (up to 30s), requests immersive
/// mode up front (system bars inset the window on phones — the app
/// first asks for its full window), and returns the window with its
/// LIVE dimensions. Loud `Err` when no window ever arrives (the
/// caller falls back to the headless tap phase — CPU-proven flips,
/// no on-screen proof).
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
    let imm = super::imm_mode::hide_system_bars(app)
        .map(|s| format!("{s} "))
        .unwrap_or_else(|e| format!("immersive refused [{e}] "));
    let (ww, wh) = (window.width().max(1) as u32, window.height().max(1) as u32);
    log.push(format!("{imm}window={ww}x{wh}"));
    eprintln!("oppa-phase: window {ww}x{wh}");
    Ok((window, ww, wh))
}

/// Presents `surface_id`'s scene on the visible window through the
/// real swapchain and drives the tap phase: GL row first — the
/// proven offscreen row on this stack — then Vulkan; both attempts
/// recorded. `width`/`height` are the LIVE window dimensions the
/// caller discovered via [`wait_for_window`] (the scene was refit to
/// them before this call — presenting at a stale size is a protocol
/// error, so this function configures exactly what it is given).
/// After two initial presents (the durable visible image), polls
/// activity + touch input until the budget elapses (`Some`) or the
/// activity is destroyed — `None` runs interactive-until-Destroy —
/// running `step` per command batch and presenting whenever it
/// reports a change. `log` collects one line per batch (plus
/// open/close lines) for `taps.txt`.
///
/// Any failure is a loud `Err` (the screen staying black is then an
/// honest failure in `error.txt`, never a silent fallback to the
/// removed blit).
#[allow(clippy::too_many_arguments)]
pub fn drive_present_loop(
    app: &AndroidApp,
    window: &NativeWindow,
    backend: &mut VelloBackend,
    surface_id: oppa::SurfaceId,
    width: u32,
    height: u32,
    touch: &mut super::touch::TouchDriver,
    budget_secs: Option<u64>,
    log: &mut Vec<String>,
    mut step: impl FnMut(&mut VelloBackend, &[AndroidCmd]) -> Result<(bool, String), String>,
) -> Result<String, String> {
    use android_activity::{MainEvent, PollEvent};
    use std::time::{Duration, Instant};
    let t_open = Instant::now();
    let (width, height) = (width.max(1), height.max(1));
    eprintln!("oppa-phase: presenting {width}x{height}");
    let wrap = AndroidWindow { window };
    let mut attempts: Vec<String> = Vec::new();
    for backends in [wgpu::Backends::GL, wgpu::Backends::VULKAN] {
        let tag = format!("{backends:?}");
        // GLES requires the platform display handle at instance
        // creation when presenting (wgpu contract); Vulkan ignores
        // it. The Android display half is a unit value, owned so it
        // can live in the descriptor.
        let mut desc = wgpu::InstanceDescriptor::new_with_display_handle(Box::new(AndroidDisplay));
        desc.backends = backends;
        let instance = wgpu::Instance::new(desc);
        let surface = instance
            .create_surface(&wrap)
            .map_err(|e| format!("{tag} create_surface: {e:?}"))?;
        let adapter = match backend.ensure_gpu_for_surface(&instance, &surface) {
            Ok(adapter) => {
                eprintln!("oppa-phase: surface-gpu {tag} {adapter}");
                adapter
            }
            Err(e) => {
                attempts.push(format!("{tag} refused [{e:?}]"));
                continue;
            }
        };
        let format = backend
            .configure_surface(&surface, width, height)
            .map_err(|e| format!("{tag} configure [{adapter}]: {e:?}"))?;
        // Two presents: the first frame after configure is
        // occasionally consumed by swapchain setup on some
        // drivers; the second is the durable visible image.
        // Immersive is re-asserted best-effort (sticky flags
        // clear on visibility changes; a detached view swallows
        // them silently, so this never fails the run).
        let mut presents = 0u32;
        let mut present_ms = 0.0f64;
        for _ in 0..2 {
            let _ = super::imm_mode::hide_system_bars(app);
            let report = backend
                .present_surface(surface_id, &surface)
                .map_err(|e| format!("{tag} present [{adapter}]: {e:?}"))?;
            presents += 1;
            present_ms += report.cpu_ms;
        }
        log.push(format!(
            "open surface={width}x{height} format={format:?} adapter={adapter}"
        ));
        // Tap phase: input batches -> step -> present on change.
        // `None` budget never expires on time (interactive-until-
        // Destroy — the deadline is only constructed for `Some`,
        // so no `u64::MAX` overflow is possible).
        let end = budget_secs.map(|b| Instant::now() + Duration::from_secs(b));
        let mut batches = 0u32;
        let mut destroyed = false;
        let mut last_beat = Instant::now();
        while end.map(|e| Instant::now() < e).unwrap_or(true) && !destroyed {
            if last_beat.elapsed() > Duration::from_secs(10) {
                eprintln!(
                    "oppa-loop: alive +{:.0}s batches={batches}",
                    t_open.elapsed().as_secs_f64()
                );
                last_beat = Instant::now();
            }
            let mut paused = false;
            app.poll_events(Some(Duration::from_millis(200)), |event| match event {
                PollEvent::Main(MainEvent::Destroy) => {
                    destroyed = true;
                }
                PollEvent::Main(MainEvent::Pause) => {
                    paused = true;
                }
                PollEvent::Main(MainEvent::Resume { .. }) => {
                    paused = false;
                }
                _ => {}
            });
            if destroyed {
                break;
            }
            if paused {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            let cmds = touch
                .pump_activity(app)
                .map_err(|e| format!("touch pump: {e}"))?;
            if cmds.is_empty() {
                continue;
            }
            batches += 1;
            let elapsed_ms = t_open.elapsed().as_secs_f64() * 1000.0;
            let (changed, note) = step(&mut *backend, &cmds)?;
            log.push(format!("batch={batches} t={elapsed_ms:.0}ms {note}"));
            if changed {
                let _ = super::imm_mode::hide_system_bars(app);
                let report = backend
                    .present_surface(surface_id, &surface)
                    .map_err(|e| format!("{tag} re-present [{adapter}]: {e:?}"))?;
                presents += 1;
                present_ms += report.cpu_ms;
            }
        }
        let s = touch.stats_summary();
        return Ok(format!(
            "surface={width}x{height} format={format:?} adapter={adapter} attempts=[{}] presents={presents} present_cpu_avg_ms={:.1} batches={batches} destroyed={destroyed} {s}",
            attempts.join(", "),
            if presents > 0 {
                present_ms / presents as f64
            } else {
                0.0
            },
        ));
    }
    Err(format!(
        "no surface-compatible GPU: {}",
        attempts.join("; ")
    ))
}
