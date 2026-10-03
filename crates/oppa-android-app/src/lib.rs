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
//! `CpuBackend`, `GpuBackend`, `AndroidShell` classification
//! into the shared `InputEvent` pipeline, `oppa-text-android`
//! shaping, core `dispatch_ime_event`. The visible image goes
//! through `wgpu::Surface` from the `NativeWindow` (`surface.rs`:
//! scene to intermediate texture, blit, present). Text rides the
//! device's own `/system/fonts` through `AndroidTextService`
//! (layout family `Roboto`), with every face injected into both
//! backends — otherwise gpu refuses text runs loudly at paint. The
//! retained scene is `SceneCore` (`oppa-app`) — host, plan builder,
//! CPU + GPU paint arms, theme/DPR/resize contract, and GPU bring-up
//! are all the unified engine now; this shell contributes only the
//! device glue (lifecycle, touches, swapchain, JNI).
//!
//! Output files (internal data dir): `cpu_off.rgba`,
//! `cpu_on.rgba`, `{gl,vulkan}_on.rgba` (raw RGBA8 row-major,
//! live-window WxH — exactly one GPU path writes, GL first then Vulkan),
//! `shapes.txt` (reference-corpus shaping), `fonts_jni.txt`
//! (platform font list), `taps.txt` (per-batch input/density oracle
//! lines first, run-log summary appended below — decision 399),
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
use oppa::{Color, ComponentHost, Ctx, DebugOverlayConfig, Props, VNode};
use oppa_text_android::AndroidTextService;

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
/// Scene background is the host theme's page `background` (desktop
/// `frame.rs` rule — Light white, Dark near-black), never a fixed
/// color: the blank-vs-rendered distinction this sacrifices (Light
/// white is the OS default window background too) is covered by the
/// `assert_content` loud guard in the pixel proof and every tap
/// batch: a uniform pixmap fails the run instead of passing
/// silently. The guard always compares against the surfaces'
/// current bg (threaded through, never this module's assumption).
/// Layout family on-device (must exist in `/system/fonts`; the
/// fallback chain covers the rest).
const SINK_FAMILY: &str = "Roboto";

/// Build flags (decision 393) — both ON in this benchmark build:
/// `DEBUG_BADGE` arms the FPS overlay before mount,
/// `BENCHMARK_PHASE` runs the bounded present phase uncapped
/// (settle → repaint → present every frame, badge clock per
/// present). The unbounded interactive phase always stays
/// event-paced Fifo, flags or not.
const DEBUG_BADGE: bool = true;
const BENCHMARK_PHASE: bool = true;

/// Page background for a host (desktop `frame.rs` rule — the
/// surface clear follows the host theme mode).
fn themed_page_bg(host: &ComponentHost) -> Color {
    oppa::ThemeTokens::of(host.theme().mode()).background
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

/// Writes the taps oracle file (decision 399): per-batch
/// input/density records first, run-log summary lines appended below.
/// The end-of-run summary write used to overwrite the batch records
/// (the whole-file `tap_log` write clobbered the incremental
/// `tap_lines` writes); now both sections share one file with stable
/// per-line layouts and no new output path for host pulls. Either
/// section may be empty (empty batches stay oracle-silent, quiet runs
/// stay summary-silent) — never a missing trailing newline.
fn write_taps_file(
    taps_path: &std::path::Path,
    tap_lines: &[String],
    tap_log: &[String],
) -> Result<(), String> {
    let mut body = String::new();
    if !tap_lines.is_empty() {
        body.push_str(&tap_lines.join("\n"));
        body.push('\n');
    }
    if !tap_log.is_empty() {
        body.push_str(&tap_log.join("\n"));
        body.push('\n');
    }
    std::fs::write(taps_path, body).map_err(|e| format!("write taps.txt: {e}"))?;
    Ok(())
}

/// One tap batch: inject commands into the engine host, run idle,
/// repaint through the engine (the committed frame plans encode into
/// BOTH arms — the GPU twin is enabled from setup, so an offscreen
/// readback later proves its pixels), assert the repainted pixmap
/// still carries content (the blank-screen guard), and append the
/// taps record incrementally for non-empty batches (benchmark frames
/// call this with empty commands — they repaint + assert but leave
/// the taps record alone).
///
/// The engine's theme contract rides every repaint: a tapped theme
/// toggle refits the page background + surfaces before painting
/// (desktop `frame.rs` rule), so the guard below compares against
/// the live bg, never a stale assumption.
///
/// Returns `(changed, note, gpu_scene)`: `gpu_scene` is the CURRENT
/// GPU scene id — a theme rebase re-homes it inside the engine
/// repaint, and the present loop must present the returned id, never
/// a stale copy (presenting a destroyed surface aborts the loop and
/// the app freezes on its last frame).
fn apply_tap_batch(
    state: &mut SceneState,
    cmds: &[oppa_shell_android::events::AndroidCmd],
    density: f32,
    tap_lines: &mut Vec<String>,
    taps_path: &std::path::Path,
) -> Result<(bool, String, oppa::SurfaceId), String> {
    let scene = &mut state.scene;
    for cmd in cmds.iter().cloned() {
        scene.host().inject_input(cmd.to_input_event());
    }
    scene.host().run_until_idle();
    scene.repaint()?;
    let page_bg = themed_page_bg(scene.host());
    let (w, h) = scene.viewport();
    let bytes = scene.rgba8()?;
    assert_content(&bytes, w, h, page_bg, "tap repaint")?;
    let line = format!("cmds={cmds:?} density={density}");
    if !cmds.is_empty() {
        tap_lines.push(line.clone());
        let _ = std::fs::write(taps_path, tap_lines.join("\n") + "\n");
    }
    Ok((true, line, scene.gpu_surface_id()))
}

/// Headless tap phase (no window/surface): drains live input
/// through the shell mapping and repaints the scene per batch.
/// State flips are CPU-proven here; on-screen proof needs the
/// swapchain path (used when the window serves the scene size).
/// Runs the full budget so `adb input` gestures land.
fn drive_headless_taps(
    app: &AndroidApp,
    state: &mut SceneState,
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
                state
                    .scene
                    .host()
                    .set_lifecycle(oppa::shell::AppLifecycleState::Paused);
            }
            PollEvent::Main(MainEvent::Resume { .. }) => {
                paused = false;
                state
                    .scene
                    .host()
                    .set_lifecycle(oppa::shell::AppLifecycleState::Active);
            }
            PollEvent::Main(MainEvent::SaveState { .. }) => {
                state
                    .scene
                    .host()
                    .set_lifecycle(oppa::shell::AppLifecycleState::Suspended);
            }
            PollEvent::Main(MainEvent::Stop) => {
                paused = true;
                state
                    .scene
                    .host()
                    .set_lifecycle(oppa::shell::AppLifecycleState::Suspended);
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
        let (_, note, _) = apply_tap_batch(state, &cmds, density, &mut tap_lines, &taps_path)?;
        // No swapchain here, so no presents tick the badge clock —
        // note per batch so the badge in the CPU pixmaps stays live
        // at batch cadence instead of freezing at mount values.
        state.scene.note_frame_presented();
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
/// inject, run, repaint both arms, and read back. One field — the
/// unified engine [`SceneCore`](oppa_app::SceneCore) owns the host,
/// the plan builder, both paint arms (CPU pixmap + GPU scene twin)
/// with their surfaces, the diff cursor, the theme/DPR/resize
/// contract, and the GPU bring-up state; this shell adds nothing on
/// top (all device-specific behavior lives in the loop below).
pub(crate) struct SceneState {
    /// The unified loop engine (see `crates/oppa-app/src/scene.rs`).
    scene: oppa_app::SceneCore,
}

impl SceneState {
    fn setup() -> Result<Self, String> {
        Self::setup_with((), oppa_controls::KitchenSinkApp)
    }

    /// Pluggable scene setup (Round 6.4): the same engine bring-up
    /// over any root component (the headless-testable shape of
    /// [`mount_app`] above). The proof loop calls `setup()` (the
    /// Kitchen Sink); embedders pass their own root.
    ///
    /// Text rides the device's `/system/fonts` through
    /// `AndroidTextService` (layout family `Roboto`): collect faces
    /// first, move the service into the engine, inject every face
    /// into BOTH backends before the first paint — otherwise custom
    /// refuses text runs loudly and the CPU arm draws bars).
    pub fn setup_with<P: Props>(props: P, render: fn(&Ctx, &P) -> VNode) -> Result<Self, String> {
        let (texts, skipped) = AndroidTextService::from_dir(Path::new("/system/fonts"))
            .map_err(|e| format!("android fonts: {e}"))?;
        if !skipped.is_empty() {
            eprintln!("oppa-android-app: skipped font files: {skipped:?}");
        }
        // The service moves into the host inside `SceneCore::new`;
        // the family print reads it before that move.
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
        let mut scene =
            oppa_app::SceneCore::new(MW as u32, MH as u32, Box::new(texts), SINK_FAMILY)?;
        // Inject every face into BOTH backends before the first
        // paint (decision 200 — bars vs tofu, see the module doc).
        for (id, bytes, index) in font_pairs {
            scene.set_font_for(id, bytes, index);
        }
        // Benchmark build (decision 393): the debug FPS badge is
        // armed when `DEBUG_BADGE` is on — `OPPA_DEBUG_FPS` is
        // unsettable under NativeActivity (apps inherit the zygote
        // env), so the app configures the host programmatically
        // BEFORE mount and the overlay wrap lands in the first
        // commit both arms paint below. Bare `Gpu` until the
        // swapchain row wins (the present loop publishes `Gpu/GL` /
        // `Gpu/Vulkan` after bring-up — the desktop
        // `presenter_badge_name` vocabulary).
        if DEBUG_BADGE {
            scene.host().set_debug_overlay(DebugOverlayConfig::all());
            scene.host().set_presenter_name("Gpu");
        }
        mount_app(scene.host(), props, render);
        scene.host().run_until_idle();
        // The GPU arm is enabled from setup (not at the present
        // loop): every repaint — including the offscreen proof's —
        // paints the GPU scene twin, so the offscreen readback
        // proves real GPU pixels. The custom rasterizer needs no
        // device for encoding (only `present_gpu` needs the
        // swapchain context); the swapchain rows
        // (`ensure_gpu_for_surface_pinned`, adapter + format) come
        // at the present loop (`surface::drive_present_loop`).
        scene.enable_gpu_pinned(oppa_app::RendererKind::GpuAuto, wgpu::PresentMode::Fifo);
        Ok(Self { scene })
    }

    /// Re-bases the scene on the config density (Round 7.7 — the
    /// desktop Round 2.4 rule ported): the engine rescales the
    /// layout config + builder + host viewport to dp, refits the
    /// paint surface to device px, and re-runs layout (measure keys
    /// carry the DPR, so text re-shapes). Same-value calls are a
    /// no-op. Non-finite or non-positive scales panic loudly inside
    /// the engine (a scale is never zero/NaN — the driver maps
    /// `dpi/160` first).
    fn set_density(&mut self, dpr: f32) -> Result<(), String> {
        self.scene.set_device_pixel_ratio(dpr)
    }

    /// Viewport in dp (what the router hit-tests touch against —
    /// intake arrives density-normalized from `TouchDriver`).
    fn viewport_dp(&self) -> (f32, f32) {
        let (w, h) = self.scene.viewport();
        (w as f32 / self.scene.dpr(), h as f32 / self.scene.dpr())
    }

    /// Adopts the live window size (Round 7.6): the engine's resize
    /// refits the viewport and both paint surfaces and re-runs
    /// layout (commits replayed so the new surfaces' `live_nodes`
    /// match); the repaint repaints both arms. Same-size calls are
    /// a no-op. Loud on any failure. `(w, h)` are device px; the
    /// host viewport divides by the DPR (Round 7.7 — dp viewport,
    /// device-px plans).
    fn refit(&mut self, w: u32, h: u32) -> Result<(), String> {
        self.scene.resize(w.max(1), h.max(1))?;
        self.scene.repaint()?;
        Ok(())
    }

    /// Rebuilds the full plan and repaints both arms through the
    /// engine (correctness over damage-minimality for the proof
    /// loop — batches are few). `what` rides the error context
    /// (which phase refused) — the engine is the body otherwise.
    fn repaint(&mut self, what: &str) -> Result<(), String> {
        self.scene
            .repaint()
            .map_err(|e| format!("repaint {what}: {e}"))?;
        Ok(())
    }

    /// Straight RGBA8 pixels of the CPU pixmap (the engine's opaque
    /// readback rule — a non-opaque pixel is a loud `Err`).
    fn cpu_bytes(&self) -> Result<Vec<u8>, String> {
        self.scene.rgba8()
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
    let (w, h) = state.scene.viewport();
    let bg = themed_page_bg(state.scene.host());
    // CPU arm, base scene.
    let t = Instant::now();
    state.repaint("base")?;
    let base = state.cpu_bytes()?;
    let cpu_base_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert_content(&base, w, h, bg, "cpu base")?;
    std::fs::write(dir.join("cpu_off.rgba"), &base).map_err(|e| format!("write off: {e}"))?;

    // Interaction probe: tap at the viewport center in dp
    // (Round 7.7 — intake is density-normalized, so the probe uses
    // the dp viewport, not device px; whatever control sits there
    // receives it through the shared pipeline).
    let (vw, vh) = state.viewport_dp();
    state
        .scene
        .host()
        .inject_input(oppa::InputEvent::pointer_down(vw / 2.0, vh / 2.0));
    state
        .scene
        .host()
        .inject_input(oppa::InputEvent::pointer_up(vw / 2.0, vh / 2.0));
    state.scene.host().run_until_idle();
    let t = Instant::now();
    state.repaint("tapped")?;
    let tapped = state.cpu_bytes()?;
    let cpu_tapped_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert_content(&tapped, w, h, bg, "cpu tapped")?;
    let interacted = base != tapped;
    std::fs::write(dir.join("cpu_on.rgba"), &tapped).map_err(|e| format!("write on: {e}"))?;

    // GPU arm: GL first, then Vulkan (decision 388 — the custom
    // rasterizer needs no compute shaders, so unlike the retired
    // Vello arm the GLES row is expected to serve, including on
    // SwiftShader; a refusal still falls through to Vulkan loudly).
    // Exactly one path writes pixels; both attempts are recorded.
    let t = Instant::now();
    let gl_err = match state.scene.gpu_backend_mut().ensure_gpu_gles() {
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
    let adapter = state
        .scene
        .gpu_backend_mut()
        .ensure_gpu_vulkan()
        .map_err(|e| {
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
    let (w, h) = state.scene.viewport();
    let (vw, vh) = state.viewport_dp();
    // The damage loop predates the unified engine (Round 20.4,
    // decision 327) and keeps its own machinery; it measures THIS
    // scene by borrowing the engine's own host/builder/CPU arm and
    // CPU surface through `SceneCore::parts_mut`.
    let (host, builder, cpu, csurf) = state.scene.parts_mut();
    let damage_record = frameloop::run_sustained_damage_loop(
        host,
        builder,
        cpu,
        csurf,
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
    let (w, h) = state.scene.viewport();
    // Frame-loop decomposition FIRST (its first full render is the
    // true first render — pipeline compilation included — so it
    // runs before the oracle read below). It predates the unified
    // engine and keeps its own machinery; it measures THIS twin by
    // borrowing the engine's GPU backend (`SceneCore::gpu_backend_mut`)
    // against the live GPU surface id.
    let frameloop_record = {
        let gsurf = state.scene.gpu_surface_id();
        frameloop::run_frame_loop(
            state.scene.gpu_backend_mut(),
            gsurf,
            dir,
            w as usize * h as usize * 4,
            tag,
        )?
    };
    let t = Instant::now();
    let gsurf = state.scene.gpu_surface_id();
    let img = state
        .scene
        .gpu_backend_mut()
        .render_pixels(gsurf)
        .map_err(|e| format!("{tag} readback: {e:?}"))?;
    let gl_read_ms = t.elapsed().as_secs_f64() * 1000.0;
    if img.width != w || img.height != h {
        return Err(format!("{tag} size mismatch {}x{}", img.width, img.height));
    }
    assert_content(
        &img.pixels,
        w,
        h,
        themed_page_bg(state.scene.host()),
        &format!("{tag} readback"),
    )?;
    std::fs::write(dir.join(format!("{tag}_on.rgba")), &img.pixels)
        .map_err(|e| format!("write {tag}: {e}"))?;

    let total_ms = t_all.elapsed().as_secs_f64() * 1000.0;
    let dpr = state.scene.dpr();
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
        // and re-presents on change. The step runs through the
        // engine on the whole `SceneState` (the arms serialize, so
        // no split borrow is needed). Runs until
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
                drive_headless_taps(
                    &app,
                    &mut state,
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
                phase("loop");
                let record = surface::drive_present_loop(
                    &app,
                    &window,
                    &mut state,
                    ww,
                    wh,
                    &mut touch,
                    Some(TAP_BUDGET_SECS),
                    &mut tap_log,
                    BENCHMARK_PHASE,
                    density,
                    &mut tap_lines,
                    &taps_path,
                )?;
                write_taps_file(&taps_path, &tap_lines, &tap_log)?;
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
            let (ww, wh) = state.scene.viewport();
            // Interactive phase stays display-paced (decision 393):
            // the badge keeps ticking on presents, but no uncapped
            // spin past the bounded budget — bonus UI must not burn
            // the device until Destroy.
            let record_result = surface::drive_present_loop(
                &app,
                &window,
                &mut state,
                ww,
                wh,
                &mut touch,
                None,
                &mut tap_log,
                false,
                density,
                &mut tap_lines,
                &taps_path,
            );
            match record_result {
                Ok(record) => tap_log.push(format!("interactive: {record}")),
                Err(e) => {
                    let note = format!("interactive aborted (proof banked, UI degraded): {e}");
                    eprintln!("oppa-android-app: {note}");
                    tap_log.push(note);
                }
            }
            let _ = write_taps_file(&taps_path, &tap_lines, &tap_log);
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
