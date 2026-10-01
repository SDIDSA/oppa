//! M10 on-device proof app: renders the Kitchen Sink scene
//! (`oppa-controls::KitchenSinkApp`) offscreen through both rasterizer
//! arms and writes raw pixels + timings to app-private files for
//! host-side comparison, then presents the scene on-screen through
//! the real swapchain and drives the tap + IME validation phases.
//!
//! No Java hand-rolls: `android-activity` provides the NativeActivity
//! lifecycle (`android_main`); JNI is used for platform queries only
//! (font list, input-method policy). The workload reuses the exact
//! workspace pieces the host oracle proves — `FramePlanBuilder`,
//! `CpuBackend`, `VelloBackend` (`ensure_gpu_gles` for the
//! GL-constrained device), `AndroidShell` classification into the
//! shared `InputEvent` pipeline, `oppa-text-android` shaping,
//! core `dispatch_ime_event`. The visible image goes through
//! `wgpu::Surface` from the `NativeWindow` (`surface.rs`: scene to
//! intermediate texture, blit, present). Text rides the device's own
//! `/system/fonts` through `AndroidTextService` (layout family
//! `Roboto`), with every face injected into both backends —
//! otherwise Vello refuses text runs loudly at paint.
//!
//! Output files (internal data dir): `cpu_off.rgba`,
//! `cpu_on.rgba`, `{gl,vulkan}_on.rgba` (raw RGBA8 row-major,
//! live-window WxH — exactly one GPU path writes, GL first then Vulkan),
//! `shapes.txt` (reference-corpus shaping), `fonts_jni.txt`
//! (platform font list), `taps.txt` (input batches + density),
//! `imm.txt` (composition feeds + IMM policy record), `meta.txt`
//! (adapter/timings/sizes/state incl. swapchain + text + tap
//! records + scoped-storage resolution/validation), `error.txt`
//! on any caught failure instead of a silent NAV crash, `DONE` last.

mod app_storage;
mod fonts_jni;
mod frameloop;
mod ime_bridge;
mod ime_validate;
mod imm_mode;
mod surface;
mod text_validate;
mod touch;

use std::path::Path;
use std::time::Instant;

use android_activity::{AndroidApp, MainEvent, PollEvent};
use oppa::{Color, ComponentHost, Ctx, Props, RendererBackend, SurfaceDesc, VNode};
use oppa_cpu::{CpuBackend, FramePlanBuilder};
use oppa_text_android::AndroidTextService;
use oppa_vello::VelloBackend;

/// Default scene size before the live window is known
/// (`SceneState::refit` adopts the real `NativeWindow` dimensions —
/// system-bar insets included — as soon as `surface::wait_for_window`
/// returns them).
const MW: f32 = 1080.0;
const MH: f32 = 2400.0;
/// Tap-phase budget (seconds) for the bounded evidence phase:
/// `adb input` gestures must land here. The post-proof interactive
/// phase runs unbounded (until Destroy).
const TAP_BUDGET_SECS: u64 = 100;
/// Scene background: white, matching the desktop Kitchen Sink
/// (`oppa-app` paints the same scene on a white CPU surface — one
/// showcase, one look, every target). The blank-vs-rendered
/// distinction this sacrifices (white is the OS default window
/// background too) is covered by the `assert_content` loud guard in
/// the pixel proof and every tap batch: a uniform pixmap fails the
/// run instead of passing silently.
const SINK_BG: Color = Color(0xFF_FF_FF);
/// Layout family on-device (must exist in `/system/fonts`; the
/// fallback chain covers the rest).
const SINK_FAMILY: &str = "Roboto";

fn sink_desc(w: u32, h: u32) -> SurfaceDesc {
    SurfaceDesc {
        width_px: w.max(1),
        height_px: h.max(1),
        background: SINK_BG,
    }
}

/// Pluggable app root (Round 6.4, decision 275): mounts an
/// arbitrary root component with typed props into a live host.
/// The on-device `android_main` below mounts
/// `oppa_controls::KitchenSinkApp` (the shared showcase — same root
/// Desktop runs through `run_desktop` and Web through
/// `WebApp::new_with_root`); embedders supply their own component
/// here instead of forking the shell — the same runner symmetry
/// Desktop has through `run_desktop` and Web has through
/// `WebApp::new_with_root`.
pub fn mount_app<P: Props>(host: &ComponentHost, props: P, render: fn(&Ctx, &P) -> VNode) {
    host.mount("MobileApp", props, render);
}

/// Straightens a tiny-skia pixmap to raw RGBA8 (loud on any
/// non-opaque pixel — the m10 oracle's conversion, same rule).
fn straight_rgba(px: &tiny_skia::Pixmap) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(px.width() as usize * px.height() as usize * 4);
    for p in px.pixels() {
        if p.alpha() != 255 {
            return Err("non-opaque pixel in fallback surface".to_string());
        }
        out.push(p.red());
        out.push(p.green());
        out.push(p.blue());
        out.push(255);
    }
    Ok(out)
}

/// One tap batch: inject commands, run, repaint the CPU arm
/// (and the Vello scene when a GPU backend is handed in), assert the
/// repainted pixmap still carries content (the blank-screen guard),
/// and append the taps record incrementally (a later failure keeps
/// the earlier batches).
#[allow(clippy::too_many_arguments)]
fn apply_tap_batch(
    host: &mut ComponentHost,
    builder: &mut FramePlanBuilder,
    cpu: &mut CpuBackend,
    csurf: oppa::SurfaceId,
    vello: Option<(&mut VelloBackend, oppa::SurfaceId)>,
    cmds: &[oppa_shell_android::events::AndroidCmd],
    density: f32,
    tap_lines: &mut Vec<String>,
    taps_path: &std::path::Path,
) -> Result<(bool, String), String> {
    for cmd in cmds.iter().cloned() {
        host.inject_input(cmd.to_input_event());
    }
    host.run_until_idle();
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    cpu.paint(csurf, &plan)
        .map_err(|e| format!("cpu paint tap: {e:?}"))?;
    if let Some((backend, vsurf)) = vello {
        backend
            .paint(vsurf, &plan)
            .map_err(|e| format!("vello paint tap: {e:?}"))?;
    }
    let px = cpu.pixmap(csurf).ok_or("cpu pixmap missing")?;
    let (w, h) = (px.width(), px.height());
    let bytes = straight_rgba(px)?;
    assert_content(&bytes, w, h, SINK_BG, "tap repaint")?;
    let line = format!("cmds={cmds:?} density={density}");
    tap_lines.push(line.clone());
    let _ = std::fs::write(taps_path, tap_lines.join("\n") + "\n");
    Ok((true, line))
}

/// Headless tap phase (no window/surface): drains live input
/// through the shell mapping and repaints the CPU arm per batch.
/// State flips are CPU-proven here; on-screen proof needs the
/// swapchain path (used when the window serves the scene size).
/// Runs the full budget so `adb input` gestures land.
#[allow(clippy::too_many_arguments)]
fn drive_headless_taps(
    app: &AndroidApp,
    host: &mut ComponentHost,
    builder: &mut FramePlanBuilder,
    cpu: &mut CpuBackend,
    csurf: oppa::SurfaceId,
    touch: &mut touch::TouchDriver,
    density: f32,
    dir: &std::path::Path,
    budget_secs: u64,
    log: &mut Vec<String>,
) -> Result<String, String> {
    use android_activity::{MainEvent, PollEvent};
    use std::time::{Duration, Instant};
    let taps_path = dir.join("taps.txt");
    let mut tap_lines: Vec<String> = Vec::new();
    log.push("headless: no window at scene size — CPU-proven flips only".to_string());
    let end = Instant::now() + Duration::from_secs(budget_secs);
    let mut batches = 0u32;
    let t_open = Instant::now();
    while Instant::now() < end {
        let mut destroyed = false;
        let mut paused = false;
        app.poll_events(Some(Duration::from_millis(200)), |event| match event {
            PollEvent::Main(MainEvent::Destroy) => {
                destroyed = true;
            }
            PollEvent::Main(MainEvent::Pause) => {
                paused = true;
                host.set_lifecycle(oppa::shell::AppLifecycleState::Paused);
            }
            PollEvent::Main(MainEvent::Resume { .. }) => {
                paused = false;
                host.set_lifecycle(oppa::shell::AppLifecycleState::Active);
            }
            PollEvent::Main(MainEvent::SaveState { .. }) => {
                host.set_lifecycle(oppa::shell::AppLifecycleState::Suspended);
            }
            PollEvent::Main(MainEvent::Stop) => {
                paused = true;
                host.set_lifecycle(oppa::shell::AppLifecycleState::Suspended);
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
        let (_, note) = apply_tap_batch(
            host,
            builder,
            cpu,
            csurf,
            None,
            &cmds,
            density,
            &mut tap_lines,
            &taps_path,
        )?;
        log.push(format!("batch={batches} t={elapsed_ms:.0}ms {note}"));
    }
    std::fs::write(&taps_path, tap_lines.join("\n") + "\n")
        .map_err(|e| format!("write taps.txt: {e}"))?;
    let s = touch.stats_summary();
    Ok(format!(
        "presented=headless-no-window batches={batches} {s}"
    ))
}

/// Loud blank-screen guard (the round's goal): `bytes` must be
/// exactly `w*h*4` RGBA8 and contain at least one non-`bg` pixel.
/// A uniform pixmap means the scene never painted (the
/// blank/white-screen class) — an `Err`, never a silent pass.
fn assert_content(bytes: &[u8], w: u32, h: u32, bg: Color, what: &str) -> Result<(), String> {
    if bytes.len() != w as usize * h as usize * 4 {
        return Err(format!("{what}: {} bytes != {w}x{h}x4", bytes.len()));
    }
    let (br, bgg, bb) = (
        ((bg.0 >> 16) & 0xFF) as u8,
        ((bg.0 >> 8) & 0xFF) as u8,
        (bg.0 & 0xFF) as u8,
    );
    let blank = bytes
        .chunks_exact(4)
        .all(|p| p[0] == br && p[1] == bgg && p[2] == bb && p[3] == 255);
    if blank {
        return Err(format!(
            "{what}: pixmap uniformly {bg:?} (blank — scene never painted)"
        ));
    }
    Ok(())
}

/// The retained scene: everything the tap-phase step needs to
/// inject, run, repaint both arms, and read back.
struct SceneState {
    host: ComponentHost,
    builder: FramePlanBuilder,
    cpu: CpuBackend,
    csurf: oppa::SurfaceId,
    backend: VelloBackend,
    vsurf: oppa::SurfaceId,
    /// Scene size in device px (adopted from the live window by
    /// [`SceneState::refit` — `MW`x`MH` until then).
    size: (u32, u32),
    /// Output scale: device px per dp (Round 7.7 — mirrors the
    /// desktop `DesktopLoop` DPR: the host viewport is dp, surfaces
    /// and plans are device px, text advances scale here; 1.0 until
    /// `set_density` learns the config density).
    dpr: f32,
}

impl SceneState {
    fn setup() -> Result<Self, String> {
        Self::setup_with((), oppa_controls::KitchenSinkApp)
    }

    /// Pluggable scene setup (Round 6.4): the same retained +
    /// CPU + Vello bring-up over any root component (the
    /// headless-testable shape of [`mount_app`] above). The
    /// proof loop calls `setup()` (the Kitchen Sink); embedders
    /// pass their own root.
    ///
    /// Text rides the device's `/system/fonts` through
    /// `AndroidTextService` (layout family `Roboto`, the desktop
    /// `LinuxTextService` pattern from `oppa-app`: collect faces
    /// first, move the service into the host, inject every face
    /// into BOTH backends before the first paint — otherwise Vello
    /// refuses text runs loudly and the CPU arm draws bars).
    pub fn setup_with<P: Props>(props: P, render: fn(&Ctx, &P) -> VNode) -> Result<Self, String> {
        let (texts, skipped) = AndroidTextService::from_dir(Path::new("/system/fonts"))
            .map_err(|e| format!("android fonts: {e}"))?;
        if !skipped.is_empty() {
            eprintln!("oppa-android-app: skipped font files: {skipped:?}");
        }
        let families = texts.families().to_vec();
        eprintln!("oppa-android-app: font families: {families:?}");
        let mut font_pairs: Vec<(oppa::text::FontId, Vec<u8>, u32)> = Vec::new();
        for id in texts.all_font_ids() {
            if let Some((bytes, index)) = texts.face_bytes(id) {
                font_pairs.push((id, bytes.to_vec(), index));
            }
        }
        if font_pairs.is_empty() {
            return Err("android fonts: no usable faces in /system/fonts".to_string());
        }
        let host = ComponentHost::new();
        host.set_viewport(MW, MH);
        host.set_layout_config(oppa::layout::LayoutTextConfig {
            family: SINK_FAMILY.to_string(),
            ..Default::default()
        });
        host.set_text_service(Box::new(texts));
        mount_app(&host, props, render);
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        let size = (MW as u32, MH as u32);
        let mut cpu = CpuBackend::new();
        let csurf = cpu
            .create_surface(sink_desc(size.0, size.1))
            .map_err(|e| format!("cpu surface: {e:?}"))?;
        for d in host.diffs_from(0) {
            cpu.commit(&d).map_err(|e| format!("cpu commit: {e:?}"))?;
        }
        let mut backend = VelloBackend::new();
        let vsurf = backend
            .create_surface(sink_desc(size.0, size.1))
            .map_err(|e| format!("vello surface: {e:?}"))?;
        for d in host.diffs_from(0) {
            backend
                .commit(&d)
                .map_err(|e| format!("vello commit: {e:?}"))?;
        }
        for (id, bytes, index) in font_pairs {
            cpu.set_font_for(id, bytes.clone(), index);
            backend.set_font_for(id, bytes, index);
        }
        Ok(Self {
            host,
            builder,
            cpu,
            csurf,
            backend,
            vsurf,
            size,
            dpr: 1.0,
        })
    }

    /// Re-bases the scene on the config density (Round 7.7 — the
    /// desktop Round 2.4 rule ported: dp viewport, device-px plans).
    /// Layout config + builder follow the new ratio, the host
    /// viewport rescales to dp, layout re-runs (measure keys carry
    /// the DPR, so text re-shapes), and both arms repaint.
    /// Same-value calls are a no-op. Non-finite or non-positive
    /// scales panic loudly (a scale is never zero/NaN — the driver
    /// maps `dpi/160` first).
    fn set_density(&mut self, dpr: f32) -> Result<(), String> {
        if !dpr.is_finite() || dpr <= 0.0 {
            panic!(
                "android scene: device pixel ratio {dpr} refused — scales are finite and positive"
            );
        }
        if dpr == self.dpr {
            return Ok(());
        }
        self.dpr = dpr;
        let mut config = self.host.layout_config();
        config.device_pixel_ratio = dpr;
        self.host.set_layout_config(config);
        self.builder.set_dpr(dpr);
        let (w, h) = self.size;
        self.host.set_viewport(w as f32 / dpr, h as f32 / dpr);
        self.host.run_until_idle();
        self.repaint("density")?;
        Ok(())
    }

    /// Viewport in dp (what the router hit-tests touch against —
    /// intake arrives density-normalized from `TouchDriver`).
    fn viewport_dp(&self) -> (f32, f32) {
        let (w, h) = self.size;
        (w as f32 / self.dpr, h as f32 / self.dpr)
    }

    /// Adopts the live window size (Round 7.6): viewport + both
    /// scene surfaces refit to `(w, h)`, commits replayed so the new
    /// surfaces' `live_nodes` match, then both arms repaint.
    /// Same-size calls are a no-op. Loud on any failure.
    /// `(w, h)` are device px; the host viewport divides by the DPR
    /// (Round 7.7 — dp viewport, device-px plans).
    fn refit(&mut self, w: u32, h: u32) -> Result<(), String> {
        let (w, h) = (w.max(1), h.max(1));
        if (w, h) == self.size {
            return Ok(());
        }
        self.size = (w, h);
        let (vw, vh) = self.viewport_dp();
        self.host.set_viewport(vw, vh);
        let old_c = self.csurf;
        self.csurf = self
            .cpu
            .create_surface(sink_desc(w, h))
            .map_err(|e| format!("cpu refit surface: {e:?}"))?;
        let _ = self.cpu.destroy_surface(old_c);
        let old_v = self.vsurf;
        self.vsurf = self
            .backend
            .create_surface(sink_desc(w, h))
            .map_err(|e| format!("vello refit surface: {e:?}"))?;
        let _ = self.backend.destroy_surface(old_v);
        for d in self.host.diffs_from(0) {
            self.cpu
                .commit(&d)
                .map_err(|e| format!("cpu refit commit: {e:?}"))?;
            self.backend
                .commit(&d)
                .map_err(|e| format!("vello refit commit: {e:?}"))?;
        }
        self.host.run_until_idle();
        self.repaint("refit")?;
        Ok(())
    }

    /// Rebuilds the full plan and repaints both arms (correctness
    /// over damage-minimality for the proof loop — batches are few).
    fn repaint(&mut self, what: &str) -> Result<(), String> {
        // Theme contract round: the build theme follows the host
        // mode (Android never enters Dark today — Light default —
        // but the plan must not hardcode the assumption).
        self.builder.set_theme_mode(self.host.theme().mode());
        let plan = self
            .host
            .with_retained_mut(|rec, styles| self.builder.build_full(rec, styles));
        self.cpu
            .paint(self.csurf, &plan)
            .map_err(|e| format!("cpu paint {what}: {e:?}"))?;
        self.backend
            .paint(self.vsurf, &plan)
            .map_err(|e| format!("vello paint {what}: {e:?}"))?;
        Ok(())
    }

    fn cpu_bytes(&self) -> Result<Vec<u8>, String> {
        straight_rgba(self.cpu.pixmap(self.csurf).ok_or("cpu pixmap missing")?)
    }
}

/// Initial pixel proof (Kitchen Sink oracle): CPU base, center-tap
/// interaction probe, offscreen GPU readback with the exact-size +
/// non-blank checks. Returns the oracle meta piece.
///
/// Unlike the retired toggle scene, a tap may or may not flip pixels
/// (it depends on what sits at the viewport center), so interaction
/// is RECORDED, never required — the hard gates are content: both
/// arms must render non-blank pixmaps at the live scene size.
fn run_pixel_proof(state: &mut SceneState, dir: &std::path::Path) -> Result<String, String> {
    let t_all = Instant::now();
    let (w, h) = state.size;
    // CPU arm, base scene.
    let t = Instant::now();
    state.repaint("base")?;
    let base = state.cpu_bytes()?;
    let cpu_base_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert_content(&base, w, h, SINK_BG, "cpu base")?;
    std::fs::write(dir.join("cpu_off.rgba"), &base).map_err(|e| format!("write off: {e}"))?;

    // Interaction probe: tap at the viewport center in dp
    // (Round 7.7 — intake is density-normalized, so the probe uses
    // the dp viewport, not device px; whatever control sits there
    // receives it through the shared pipeline).
    let (vw, vh) = state.viewport_dp();
    state
        .host
        .inject_input(oppa::InputEvent::pointer_down(vw / 2.0, vh / 2.0));
    state
        .host
        .inject_input(oppa::InputEvent::pointer_up(vw / 2.0, vh / 2.0));
    state.host.run_until_idle();
    let t = Instant::now();
    state.repaint("tapped")?;
    let tapped = state.cpu_bytes()?;
    let cpu_tapped_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert_content(&tapped, w, h, SINK_BG, "cpu tapped")?;
    let interacted = base != tapped;
    std::fs::write(dir.join("cpu_on.rgba"), &tapped).map_err(|e| format!("write on: {e}"))?;

    // GPU arm: GL-constrained device first (the M10
    // row); SwiftShader's GLES translator has no compute, so on
    // the emulator that refusal is expected and the Vulkan row is
    // attempted next. Exactly one path writes pixels; both attempts
    // are recorded.
    let t = Instant::now();
    let gl_err = match state.backend.ensure_gpu_gles() {
        Ok(a) => {
            let gpu_meta = finish_gpu_arm(
                state,
                dir,
                t,
                "gl",
                a,
                t_all,
                cpu_base_ms,
                cpu_tapped_ms,
                interacted,
            )?;
            return finish_damage_arm(state, dir, gpu_meta);
        }
        Err(e) => format!("{e:?}"),
    };
    let gl_device_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t = Instant::now();
    let adapter = state.backend.ensure_gpu_vulkan().map_err(|e| {
        format!("gl device refused [{gl_err} ({gl_device_ms:.1}ms)]; vulkan refused [{e:?}]")
    })?;
    let gpu_meta = finish_gpu_arm(
        state,
        dir,
        t,
        "vulkan",
        adapter,
        t_all,
        cpu_base_ms,
        cpu_tapped_ms,
        interacted,
    )?;
    finish_damage_arm(state, dir, gpu_meta)
}

/// Sustained damage-loop tail (Round 20.4, decision 327): after the
/// full-scene cold timings land, `DAMAGE_FRAMES` consecutive
/// center-tap flips run through settle → build → paint → readback
/// on the CPU arm, and the steady-state incremental record joins
/// the oracle/meta lines next to the cold numbers (plus
/// `damage.txt` beside `frameloop.txt`).
fn finish_damage_arm(
    state: &mut SceneState,
    dir: &std::path::Path,
    gpu_meta: String,
) -> Result<String, String> {
    let (w, h) = state.size;
    let (vw, vh) = state.viewport_dp();
    let SceneState {
        host,
        builder,
        cpu,
        csurf,
        ..
    } = state;
    let damage_record = frameloop::run_sustained_damage_loop(
        host,
        builder,
        cpu,
        *csurf,
        w as usize * h as usize * 4,
        dir,
        "cpu",
        |host| {
            host.inject_input(oppa::InputEvent::pointer_down(vw / 2.0, vh / 2.0));
            host.inject_input(oppa::InputEvent::pointer_up(vw / 2.0, vh / 2.0));
        },
    )?;
    Ok(format!("{gpu_meta}{damage_record}"))
}

/// Shared tail for whichever GPU path served: read back, write
/// pixels + meta (the scene is already painted tapped-side by the caller).
#[allow(clippy::too_many_arguments)]
fn finish_gpu_arm(
    state: &mut SceneState,
    dir: &std::path::Path,
    t_device: Instant,
    tag: &str,
    adapter: String,
    t_all: Instant,
    cpu_base_ms: f64,
    cpu_tapped_ms: f64,
    interacted: bool,
) -> Result<String, String> {
    let gl_device_ms = t_device.elapsed().as_secs_f64() * 1000.0;
    let (w, h) = state.size;
    // Frame-loop decomposition FIRST (its first full render is the
    // true first render — pipeline compilation included — so it
    // runs before the oracle read below).
    let frameloop_record = frameloop::run_frame_loop(
        &mut state.backend,
        state.vsurf,
        dir,
        w as usize * h as usize * 4,
        tag,
    )?;
    let t = Instant::now();
    let img = state
        .backend
        .render_pixels(state.vsurf)
        .map_err(|e| format!("{tag} readback: {e:?}"))?;
    let gl_read_ms = t.elapsed().as_secs_f64() * 1000.0;
    if img.width != w || img.height != h {
        return Err(format!("{tag} size mismatch {}x{}", img.width, img.height));
    }
    assert_content(&img.pixels, w, h, SINK_BG, &format!("{tag} readback"))?;
    std::fs::write(dir.join(format!("{tag}_on.rgba")), &img.pixels)
        .map_err(|e| format!("write {tag}: {e}"))?;

    let total_ms = t_all.elapsed().as_secs_f64() * 1000.0;
    let dpr = state.dpr;
    Ok(format!(
        "path={tag}\nadapter={adapter}\ncpu_off_ms={cpu_base_ms:.1}\ncpu_on_ms={cpu_tapped_ms:.1}\n\
         gpu_device_ms={gl_device_ms:.1}\ngpu_paint_ms=n/a\ngpu_read_ms={gl_read_ms:.1}\n\
         total_ms={total_ms:.1}\nbytes={}\ndpr={dpr:.3}\ncontent=nonblank interacted={interacted}\n{frameloop_record}\n",
        img.pixels.len()
    ))
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let boot_dir = app.internal_data_path().unwrap_or_else(|| "/tmp".into());
    // Phase marker: one line per stage (observability for hung
    // runs — `phase.txt` shows the last stage entered).
    let phase = |name: &str| {
        eprintln!("oppa-phase: {name}");
        let _ = std::fs::write(boot_dir.join("phase.txt"), format!("{name}\n"));
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Scoped storage first (Round 3.4): resolve files/cache
        // with fallback, round-trip both, and only then let any
        // proof output land (unverified dirs never receive writes).
        phase("dirs");
        let (dirs, dirs_record) = app_storage::resolve_and_validate(&app)?;
        let dir = dirs.files_dir.clone();
        phase("setup");
        let mut state = SceneState::setup()?;

        // Touch driver before the pixel proof (Round 7.7): the
        // config density re-bases the scene to dp (viewport dp,
        // device-px plans — the desktop Round 2.4 rule), so the
        // proof paints and probes at the real scale instead of
        // physical px (unreadably tiny on 400+ dpi panels, and
        // dp-px mismatched taps).
        phase("touch-new");
        let mut touch =
            touch::TouchDriver::new(&app, MW, MH).map_err(|e| format!("touch driver: {e}"))?;
        let density = touch.density();
        state.set_density(density)?;
        phase("pixels");
        let pixel_meta = run_pixel_proof(&mut state, &dir)?;
        // Oracle record lands immediately (a later present failure
        // must not take the timings with it — the phone proved it).
        std::fs::write(dir.join("oracle.txt"), &pixel_meta)
            .map_err(|e| format!("write oracle.txt: {e}"))?;

        // Composition feed #1 (pre-tap): M1 shapes through the
        // dispatch seam + policy into the shell IME log.
        phase("feed1");
        let feed1 = ime_validate::run_composition_feed(touch.shell_mut())?;

        // Text slice validation BEFORE the surface loop (it needs
        // no window — feeds, shapes, and the JNI font record land
        // even where the window never serves the scene size).
        phase("text");
        let text_record = text_validate::validate_text(&app, &dir)?;

        // Present + tap phase: the live window size is adopted first
        // (Round 7.6 — system-bar insets included), then the swapchain
        // presents the Kitchen Sink while `adb input` gestures drive
        // the shell's touch mapping: every batch injects pointer
        // events, runs idle, rebuilds the plan, commits both arms,
        // and re-presents on change. Split borrows: the step closure
        // owns host/cpu, the driver owns the GPU backend. Runs until
        // the budget elapses or the activity is destroyed (the final
        // stay-alive loop below holds the process until Destroy per
        // the android-activity contract).
        let mut tap_lines: Vec<String> = Vec::new();
        let taps_path = dir.join("taps.txt");
        let mut tap_log: Vec<String> = Vec::new();
        phase("window");
        let presented = match surface::wait_for_window(&app, &mut tap_log) {
            Err(e) => {
                // No window at all (headless emulator): state flips
                // are still CPU-proven through the headless tap phase
                // — on-screen proof stays open, everything else lands.
                eprintln!("oppa-android-app: {e}; headless tap phase");
                let SceneState {
                    host,
                    builder,
                    cpu,
                    csurf,
                    ..
                } = &mut state;
                drive_headless_taps(
                    &app,
                    host,
                    builder,
                    cpu,
                    *csurf,
                    &mut touch,
                    density,
                    &dir,
                    TAP_BUDGET_SECS,
                    &mut tap_log,
                )?
            }
            Ok((window, ww, wh)) => {
                state.refit(ww, wh)?;
                touch
                    .shell_mut()
                    .note_surface_changed(ww as f32 / density, wh as f32 / density);
                let SceneState {
                    host,
                    builder,
                    cpu,
                    csurf,
                    backend,
                    vsurf,
                    ..
                } = &mut state;
                phase("loop");
                let record = surface::drive_present_loop(
                    &app,
                    &window,
                    backend,
                    *vsurf,
                    ww,
                    wh,
                    &mut touch,
                    Some(TAP_BUDGET_SECS),
                    &mut tap_log,
                    |backend, cmds| {
                        apply_tap_batch(
                            host,
                            builder,
                            cpu,
                            *csurf,
                            Some((backend, *vsurf)),
                            cmds,
                            density,
                            &mut tap_lines,
                            &taps_path,
                        )
                    },
                )?;
                std::fs::write(&taps_path, tap_log.join("\n") + "\n")
                    .map_err(|e| format!("write taps.txt: {e}"))?;
                record
            }
        };

        // Composition feed #2 (post-tap) + IMM policy over JNI.
        phase("feed2");
        let feed2 = ime_validate::run_composition_feed(touch.shell_mut())?;
        phase("imm");
        let imm_record = ime_validate::imm_policy(&app)?;
        // Soft-keyboard bridge (Round 3.1): native entries
        // registered, queue drained (empty — no proxy traffic in
        // the proof loop), show/hide round-tripped verbatim.
        phase("bridge");
        let bridge_register = ime_bridge::ensure_ime_callbacks(&app)?;
        let bridge_drained = ime_bridge::drain_ime_queue_into(touch.shell_mut());
        let bridge_show = ime_bridge::show_keyboard(&app)?;
        let bridge_hide = ime_bridge::hide_keyboard(&app)?;
        let bridge_record =
            format!("{bridge_register} drained={bridge_drained} {bridge_show} {bridge_hide}");
        let ime_log: Vec<String> = touch
            .shell_mut()
            .take_ime_log()
            .iter()
            .map(|op| format!("{op:?}"))
            .collect();
        std::fs::write(
            dir.join("imm.txt"),
            format!(
                "feed1={feed1}\nfeed2={feed2}\n{imm_record}\nbridge={bridge_record}\nshell_ime_log={ime_log:?}\n"
            ),
        )
        .map_err(|e| format!("write imm.txt: {e}"))?;

        phase("done");
        // Proof banked early (Round 7.8): meta + DONE land BEFORE
        // the interactive phase below (same bytes the outer write
        // repeats) so host pull tooling keeps its timing — the bonus
        // UI phase that follows must never take the evidence with it.
        let meta =
            format!("{pixel_meta}presented={presented}\n{text_record}\nstorage={dirs_record}\n");
        std::fs::write(dir.join("meta.txt"), &meta).map_err(|e| format!("write meta.txt: {e}"))?;
        std::fs::write(dir.join("DONE"), "ok").map_err(|e| format!("write DONE: {e}"))?;

        // Interactive phase (Round 7.8): the proof is banked, now the
        // app stays a live UI until Destroy instead of going
        // input-dead after the tap budget (tapping a DONE-idle app
        // did nothing — the stay-alive drain ate it). Same step
        // machinery, unbounded budget; the GPU context re-ensures
        // once (~1 s stall, documented). Failures here are loud but
        // non-fatal: degrading bonus interactivity must not rewrite
        // the banked proof into `error.txt`.
        phase("interactive");
        if let Some(window) = app.native_window() {
            let (ww, wh) = state.size;
            let SceneState {
                host,
                builder,
                cpu,
                csurf,
                backend,
                vsurf,
                ..
            } = &mut state;
            match surface::drive_present_loop(
                &app,
                &window,
                backend,
                *vsurf,
                ww,
                wh,
                &mut touch,
                None,
                &mut tap_log,
                |backend, cmds| {
                    apply_tap_batch(
                        host,
                        builder,
                        cpu,
                        *csurf,
                        Some((backend, *vsurf)),
                        cmds,
                        density,
                        &mut tap_lines,
                        &taps_path,
                    )
                },
            ) {
                Ok(record) => tap_log.push(format!("interactive: {record}")),
                Err(e) => {
                    let note = format!("interactive aborted (proof banked, UI degraded): {e}");
                    eprintln!("oppa-android-app: {note}");
                    tap_log.push(note);
                }
            }
            let _ = std::fs::write(&taps_path, tap_log.join("\n") + "\n");
        } else {
            tap_log.push("interactive skipped (no window)".to_string());
        }

        Ok::<_, String>(meta)
    }));
    match result {
        Ok(Ok(meta)) => {
            let _ = std::fs::write(boot_dir.join("meta.txt"), meta);
            let _ = std::fs::write(boot_dir.join("DONE"), "ok");
        }
        Ok(Err(e)) => {
            let _ = std::fs::write(boot_dir.join("error.txt"), format!("workload: {e}"));
        }
        Err(_) => {
            let _ = std::fs::write(boot_dir.join("error.txt"), "workload panicked");
        }
    }
    // Stay alive for pulls until the system destroys us (never
    // `process::exit` — the android-activity contract). The input
    // queue keeps draining here: after DONE nothing else consumes
    // it, and one undispatched MotionEvent ANRs the app within 5 s
    // (observed on-device) — pump-and-discard keeps it flowing while
    // the Destroy watch below stays armed.
    let destroyed = std::cell::Cell::new(false);
    while !destroyed.get() {
        match app.input_events_iter() {
            Ok(mut iter) => loop {
                let more = iter.next(|_| android_activity::InputStatus::Handled);
                if !more {
                    break;
                }
            },
            Err(e) => eprintln!("oppa-android-app: stay-alive input drain: {e:?}"),
        }
        app.poll_events(Some(std::time::Duration::from_millis(200)), |event| {
            if matches!(event, PollEvent::Main(MainEvent::Destroy)) {
                destroyed.set(true);
            }
        });
    }
}
