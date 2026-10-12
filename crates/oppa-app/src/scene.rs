// ---------------------------------------------------------------------------
// SceneCore — the unified platform-free app-loop engine (facade)
// ---------------------------------------------------------------------------
//
// One composition of the shared loop rule that both the `DesktopLoop`
// runners (Windows GDI/GPU, Linux winit/softbuffer) and the Android
// runner (`oppa-android-app`) drive:
//
// - the shared [`SceneSession`] (one implementation of the loop rule:
//   diff commit replay into presenters, overlay publish, theme/page-bg
//   contract with the refit trigger, plan build, image-sync trigger,
//   resize/DPR rules, step ladder, plan/caret telemetry), plus
// - the pixel [`RgbPresenter`] adapter:
//   BOTH paint arms (CPU pixmap + GPU scene twin) with their
//   surfaces, GPU bring-up state (active renderer, recorded API,
//   present mode, current-mode negotiation), and the pixel readout.
//
// Platform glue keeps ONLY the window/event pump and its own recorder
// for delivers: shell commands, IME snapshots, dialogs, clipboard...
// every loop must contribute its own piece (lifecycle events, window
// resizing hooks...) and delegate everything above to [`SceneCore`] so
// a pipeline fix or feature lands once for all loops.
//
// History note: these bodies moved verbatim from `frame.rs` /
// `resize.rs` / `gpu.rs` (all one loop's logic); error wording ("desktop
// loop: ...") is preserved load-bearing — tests grep it.

use super::*;
use crate::presenter_rgb::RgbPresenter;
use oppa_cpu::FramePlanBuilder;

/// Platform-free pixel scene engine — see the module docs.
pub struct SceneCore {
    /// The shared loop rule (also the web DOM loop's engine).
    pub(crate) session: SceneSession,
    /// The pixel present adapter (CPU arm + GPU twin, surfaces,
    /// present machinery, readout).
    pub(crate) rgb: RgbPresenter,
}

impl SceneCore {
    /// Creates the host (viewport + text service + layout config) and
    /// both paint surfaces (CPU pixmap + GPU scene twin). Loud on
    /// any failure. Starts CPU-only; runners enable GPU against the
    /// live window (see `enable_gpu`). Ticks the system clock (live
    /// loops — see `SceneCore::with_clock` for the deterministic
    /// harness clock).
    pub fn new(
        width_px: u32,
        height_px: u32,
        service: Box<dyn TextService>,
        family: &str,
    ) -> Result<Self, String> {
        Self::with_clock(
            width_px,
            height_px,
            service,
            family,
            Rc::new(SystemClock::new()),
        )
    }

    /// Clock-injected construction (Round 15.2, decision 313 — the
    /// headless blink rig: a `MockClock` owner advances time
    /// explicitly, so flip demand proves without sleeps). Otherwise
    /// identical to [`SceneCore::new`].
    pub fn with_clock(
        width_px: u32,
        height_px: u32,
        service: Box<dyn TextService>,
        family: &str,
        clock: Rc<dyn Clock>,
    ) -> Result<Self, String> {
        let (w, h) = (width_px.max(1), height_px.max(1));
        let host = ComponentHost::with_clock(clock);
        host.set_viewport(w as f32, h as f32);
        host.set_text_service(service);
        host.set_layout_config(LayoutTextConfig {
            family: family.to_string(),
            ..Default::default()
        });
        // Theme contract round: the page background seeds from the
        // host theme (a fresh host is Light — the pre-contract
        // white, pixel-identical) and clears both paint surfaces.
        let bg = host.theme_tokens().background;
        let rgb = RgbPresenter::new(w, h, bg)?;
        let session = SceneSession::from_parts(host, FramePlanBuilder::new(1.0), (w, h), 1.0);
        Ok(Self { session, rgb })
    }

    /// The host (mount records, recursive journal reads — shared by
    /// every runner and the testkit escape-hatch rule).
    pub fn host(&self) -> &ComponentHost {
        &self.session.host
    }

    /// Injects a raster face for one shaper font id (decision 200):
    /// without it the CPU backend draws advance-cell bars and the
    /// GPU twin refuses loudly at paint; with it, real outlines
    /// on both. Best-effort by contract (callers warn, never die,
    /// when the face is unreadable). Always lands in both backends
    /// so a later `enable_gpu` never paints tofu.
    pub fn set_font_for(
        &mut self,
        id: oppa::text::FontId,
        bytes: std::sync::Arc<[u8]>,
        index: u32,
    ) {
        self.rgb.set_font_for(id, bytes, index);
    }

    /// The frame engine: replays every new diff into the paint
    /// adapters (the CPU pixmap and the GPU twin — cheap
    /// bookkeeping), publishes the focused selection/caret as
    /// build-scoped overlays, follows the theme contract (build theme
    /// and page background with presenter refit on toggle), builds ONE
    /// full plan, syncs static-image pixel deposits (CPU always, GPU
    /// twin when active), and paints the arms. The full loop rule +
    /// per-adapter paints live on
    /// [`SceneSession::repaint`]; pixels encode into the
    /// CPU pixmap (skipped while opted out — decision 473) plus the
    /// custom twin whenever GPU is active.
    /// Returns the plan's damage-rect count; pixels failures are
    /// loud `Err`s.
    pub fn repaint(&mut self) -> Result<usize, String> {
        self.session.repaint(&mut self.rgb)
    }

    /// The last committed frame plan (Round 15.2, decision 313 —
    /// proves which overlay `Rect`s a repaint committed; `None`
    /// before the first repaint, never a live borrow).
    pub fn last_plan(&self) -> Option<FramePlan> {
        self.session.last_plan()
    }

    /// Seconds until the focused caret's next blink flip, if any
    /// (Round 15.2, decision 313 — the runner wake query): `Some`
    /// while a collapsed caret is focused (wake then to repaint the
    /// flip); `None` otherwise. Pure over the host.
    pub fn blink_tick_in_secs(&self) -> Option<f64> {
        self.session.host.caret_blink_in_secs()
    }

    /// True when the caret overlay's presence flipped since the last
    /// repaint (Round 15.2, decision 313 — the blink demand). Pure
    /// over the host state above.
    pub fn caret_overlay_dirty(&self) -> bool {
        self.session.caret_overlay_dirty()
    }

    /// Settles and repaints iff the caret overlay flipped (Round
    /// 15.2, decision 313 — the timer-tick path runners call when
    /// no input arrived): returns `true` when it repainted, `false`
    /// when settled-quiet (never a spin — a quiet poll schedules
    /// nothing). Paint failures are loud `Err`s like `repaint`.
    pub fn poll_blink(&mut self) -> Result<bool, String> {
        self.session.poll_blink(&mut self.rgb)
    }

    /// Earliest component-timer due instant in host-clock ms (Round
    /// 21.1, decision 328 — the wake-query twin of
    /// `blink_tick_in_secs`). Pure over the host.
    pub fn next_timer_due_ms(&self) -> Option<f64> {
        let now = self.session.host.now_ms();
        self.session.host.next_timer_due_ms(now)
    }

    /// Fires due component timers, settling + repainting when
    /// anything fired (Round 21.1, decision 328 — the timer-tick
    /// path runners call on wake): returns the fired count. Zero
    /// when nothing was due (quiet). Paint failures are loud `Err`s.
    pub fn tick_timers(&mut self) -> Result<usize, String> {
        self.session.tick_timers(&mut self.rgb)
    }

    /// Tracks a resize: refits the viewport and rebuilds the paint
    /// surface (positions derive from the viewport, so refit alone
    /// leaves the scene stale). `width_px`/`height_px` are device px; the host
    /// viewport divides by the DPR (identity at 1.0 — the only
    /// previously reachable state). Layout invalidation rides
    /// `set_viewport` (decision 248 — the trailing settle actually
    /// re-runs layout now). Same-size calls are a no-op. Loud on
    /// failure.
    pub fn resize(&mut self, width_px: u32, height_px: u32) -> Result<(), String> {
        self.session.resize(&mut self.rgb, width_px, height_px)
    }

    /// Current output scale (device px per CSS px).
    pub fn dpr(&self) -> f32 {
        self.session.dpr()
    }

    /// Viewport in device px.
    pub fn viewport(&self) -> (u32, u32) {
        self.session.viewport()
    }

    /// Re-bases the loop on a new device pixel ratio keeping the CSS
    /// viewport stable (Round 2.4, OQ-G10-2 — the runners call this
    /// on DPI/scale changes): layout config + builder follow, the
    /// host viewport rescales to CSS, the surface refits to device
    /// px, and layout re-runs (measure keys carry the DPR, so text
    /// re-shapes; the `set_viewport` dirt from decision 248 reflows
    /// boxes — nothing stale survives at the new scale). Same-value
    /// calls are a no-op. Non-finite or non-positive scales panic
    /// loudly (a scale is never zero/NaN — shells map raw OS values
    /// through `dpr_from_dpi`/`dpr_from_scale_factor` first).
    pub fn set_device_pixel_ratio(&mut self, dpr: f32) -> Result<(), String> {
        self.session.set_device_pixel_ratio(&mut self.rgb, dpr)
    }

    /// Straight RGBA8 row-major pixels of the last paint (opaque
    /// scenes only — a non-opaque pixel is a loud `Err`, the m10
    /// straightening rule, formerly the Android `straight_rgba`.
    /// Present glue converts onward per platform (BGRA for GDI,
    /// XRGB words for softbuffer).
    pub fn rgba8(&self) -> Result<Vec<u8>, String> {
        let px = self
            .rgb
            .cpu
            .pixmap(self.rgb.surface)
            .ok_or_else(|| "desktop loop: cpu pixmap missing".to_string())?;
        if px.width() == 0 || px.height() == 0 {
            return Err("desktop loop: zero-size pixmap".to_string());
        }
        let mut out = Vec::with_capacity(px.width() as usize * px.height() as usize * 4);
        for p in px.pixels() {
            if p.alpha() != 255 {
                return Err("desktop loop: non-opaque pixel".to_string());
            }
            out.push(p.red());
            out.push(p.green());
            out.push(p.blue());
            out.push(255);
        }
        Ok(out)
    }

    /// Which backend the platform glue presents (`Cpu` until a
    /// runner enables GPU; runners flip back on loud GPU failure).
    pub fn renderer_kind(&self) -> RendererKind {
        self.rgb.active
    }

    /// True once a runner enabled hardware presentation (any GPU
    /// variant — the only GPU presenter since decision 388).
    pub fn is_gpu(&self) -> bool {
        self.rgb.active.is_gpu()
    }

    /// Human name for logs (one per `OPPA_RENDERER` value).
    pub fn renderer_name(&self) -> &'static str {
        match self.rgb.active {
            RendererKind::Gpu => "gpu",
            RendererKind::GpuAuto => "gpu-auto",
            RendererKind::GpuVulkan => "gpu-vulkan",
            RendererKind::GpuDx12 => "gpu-dx12",
            RendererKind::GpuGl => "gpu-gl",
            RendererKind::Cpu => "cpu",
        }
    }

    /// Badge label for the debug overlay's presenter slot
    /// (`[Gpu/Vulkan]`, `[Gpu/DX12]`, `[Gpu/GL]`, `[CPU]`): derived
    /// from the *active* renderer plus the recorded `wgpu` backend
    /// after bring-up, so the label can never disagree with the
    /// pixels. Runners set it from this after bring-up instead of
    /// naming outcomes. A GPU presenter with no recorded backend
    /// (e.g. a headless `enable_gpu` with no swapchain) reads the
    /// bare presenter (`Gpu`).
    pub fn presenter_badge_name(&self) -> &'static str {
        if self.is_gpu() {
            return match self.rgb.gpu_api {
                Some(GpuApi::Vulkan) => "Gpu/Vulkan",
                Some(GpuApi::Dx12) => "Gpu/DX12",
                Some(GpuApi::Gl) => "Gpu/GL",
                None => "Gpu",
            };
        }
        "CPU"
    }

    /// Recorded winning `wgpu` backend underneath the GPU presenter
    /// (`None` until a runner records one at bring-up).
    pub fn gpu_api(&self) -> Option<GpuApi> {
        self.rgb.gpu_api
    }

    /// Records the winning `wgpu` backend after bring-up (runners
    /// call this with the attempt name that succeeded, before
    /// publishing [`SceneCore::presenter_badge_name`]).
    pub fn set_gpu_api(&mut self, api: GpuApi) {
        self.rgb.gpu_api = Some(api);
    }

    /// Swapchain present mode once GPU is up (`None` while CPU-only).
    pub fn gpu_present_mode(&self) -> Option<wgpu::PresentMode> {
        self.rgb.gpu_mode
    }

    /// GPU surface (painted whenever a GPU variant is active).
    pub fn gpu_surface_id(&self) -> SurfaceId {
        self.rgb.gpu_surface
    }

    /// GPU twin retained op count.
    pub fn gpu_retained_op_count(&self) -> Option<usize> {
        self.rgb.gpu_backend.retained_op_count(self.rgb.gpu_surface)
    }

    /// CPU retained op count (test symmetry with the GPU twin).
    pub fn cpu_retained_op_count(&self) -> Option<usize> {
        self.rgb.cpu.retained_op_count(self.rgb.surface)
    }

    /// Total CPU paints executed (decision 436 test seam: proves the
    /// opt-out actually skips `CpuBackend::paint` instead of merely
    /// producing identical pixels).
    pub fn cpu_paints_total(&self) -> u64 {
        self.rgb.cpu.paints_total()
    }

    /// CPU pixmap surface (the GDI / softbuffer source and the GPU
    /// fallback pixels; stale while the paint is opted out — readers
    /// on the fallback path go through
    /// [`refresh_cpu_pixmap`](Self::refresh_cpu_pixmap)).
    pub fn cpu_surface_id(&self) -> SurfaceId {
        self.rgb.surface
    }

    /// Enables hardware presentation after the runner configured the
    /// swapchain: the next [`SceneCore::repaint`] paints the GPU
    /// twin. Callers repaint immediately so the twin catches up from
    /// its creation-time scene.
    pub fn enable_gpu(&mut self, mode: wgpu::PresentMode) {
        // The bring-up filtered the wgpu backends from this same
        // override, so the active variant records what was actually
        // attempted (never a silent substitution). Unset resolves
        // to the custom automatic default (decision 388).
        self.rgb.active = renderer_override().unwrap_or(RendererKind::GpuAuto);
        self.rgb.gpu_mode = Some(mode);
    }

    /// Enables hardware presentation with an explicit renderer kind
    /// (no environment consultation): the Android runner's bring-up
    /// shape — zygote-inherited env must not decide the arm there.
    pub fn enable_gpu_pinned(&mut self, kind: RendererKind, mode: wgpu::PresentMode) {
        self.rgb.active = kind;
        self.rgb.gpu_mode = Some(mode);
    }

    /// Drops back to the CPU pixmap (loud GPU failure path): later
    /// repaints skip the GPU twin again; its retained scene stays
    /// for a later re-enable (refit keeps sizes in lockstep). When
    /// the CPU paint was opted out while GPU served (decision 436),
    /// repaints once immediately so the fallback never serves silent
    /// old pixels — loud `Err` on failure, like every repaint.
    pub fn disable_gpu(&mut self) -> Result<(), String> {
        self.rgb.active = RendererKind::Cpu;
        self.rgb.gpu_mode = None;
        self.rgb.gpu_api = None;
        if !self.rgb.cpu_paint {
            self.rgb.set_cpu_paint(true);
            self.session.repaint(&mut self.rgb)?;
        }
        Ok(())
    }

    /// Opts the CPU pixmap paint in or out (decision 436, extended
    /// to desktop GPU runs by decision 473): GPU-serving runners
    /// disable it per bring-up (nobody reads the pixmap while the
    /// swapchain serves, and the full-scene software raster dominates
    /// tap cost — measured unoptimized on top); CPU-only runs and
    /// the proof harness keep the default on, and
    /// [`disable_gpu`](Self::disable_gpu) re-arms it on the
    /// loud-failure fallback. See [`refresh_cpu_pixmap`](Self::refresh_cpu_pixmap)
    /// for the transient-failure rule (fresh blit without leaving
    /// GPU mode).
    pub fn set_cpu_paint(&mut self, on: bool) {
        self.rgb.set_cpu_paint(on);
    }

    /// Repaints the CPU pixmap once without leaving GPU mode
    /// (decision 473 — transient present-failure fallback): when the
    /// CPU paint was opted out while GPU serves, re-arms it,
    /// repaints, and re-opts out, so the fallback blit that follows
    /// reads fresh pixels while the swapchain stays primary (the
    /// next frame still retries GPU per the runner contract).
    /// No-op when the CPU paint is already on (CPU-only runs never
    /// pay it). Loud `Err` on repaint failure (never a silent stale
    /// blit).
    pub fn refresh_cpu_pixmap(&mut self) -> Result<(), String> {
        if self.rgb.cpu_paint {
            return Ok(());
        }
        self.rgb.set_cpu_paint(true);
        let r = self.repaint();
        self.rgb.set_cpu_paint(false);
        r.map(|_| ())
    }

    /// Ensures a live GPU context on an adapter compatible with
    /// `surface` (the swapchain path — the caller owns the
    /// `wgpu::Instance` + `wgpu::Surface` from the platform window
    /// and hands both here). Loud `Err` when no compatible adapter
    /// exists or the device/renderer refuses it — never a silent CPU
    /// substitution (the runner logs and falls back).
    pub fn ensure_gpu_for_surface(
        &mut self,
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<String, String> {
        // Decision 388: the GPU rasterizer is the only GPU
        // presenter. A forced-CPU override reaching here is a
        // runner bug — refuse loudly, never present GPU silently.
        if renderer_override() == Some(RendererKind::Cpu) {
            return Err("desktop loop: ensure gpu for surface while OPPA_RENDERER=cpu".to_string());
        }
        let info = self
            .rgb
            .gpu_backend
            .ensure_gpu_for_surface(instance, surface)
            .map_err(|e| format!("desktop loop: ensure gpu for surface: {e:?}"))?;
        self.rgb.active = renderer_override().unwrap_or(RendererKind::GpuAuto);
        Ok(format!("{info} ({})", self.renderer_name()))
    }

    /// The GPU arm's bring-up without the desktop env override (the
    /// Android runner shape — see [`SceneCore::enable_gpu_pinned`]
    /// for why): identical to [`SceneCore::ensure_gpu_for_surface`]
    /// but the active renderer is pinned to `kind`, never read from
    /// the zygote-inherited environment.
    pub fn ensure_gpu_for_surface_pinned(
        &mut self,
        kind: RendererKind,
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<String, String> {
        let info = self
            .rgb
            .gpu_backend
            .ensure_gpu_for_surface(instance, surface)
            .map_err(|e| format!("desktop loop: ensure pinned gpu for surface: {e:?}"))?;
        self.rgb.active = kind;
        Ok(info)
    }

    /// Configures `surface` for presentation of the current viewport
    /// (format = first non-sRGB 8-bit the surface offers).
    /// Requires a context from
    /// [`SceneCore::ensure_gpu_for_surface`]. Returns
    /// the chosen format for the run record.
    pub fn configure_gpu_surface(
        &mut self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
        mode: wgpu::PresentMode,
    ) -> Result<wgpu::TextureFormat, String> {
        self.configure_gpu_surface_alpha(
            surface,
            width,
            height,
            mode,
            wgpu::CompositeAlphaMode::Auto,
        )
    }

    /// Configures `surface` with an explicit composite-alpha mode
    /// (decision 482): `Auto` for every opaque window style,
    /// `PreMultiplied` for `WindowStyle::Transparent` (pair with
    /// [`SceneCore::set_transparent_clear`] — the swapchain carries
    /// per-pixel alpha only when both agree). A mode the surface
    /// refuses is a loud `Err` (the runner falls back loudly, never
    /// a silent opaque window).
    pub fn configure_gpu_surface_alpha(
        &mut self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
        mode: wgpu::PresentMode,
        alpha_mode: wgpu::CompositeAlphaMode,
    ) -> Result<wgpu::TextureFormat, String> {
        self.rgb
            .gpu_backend
            .configure_surface_alpha(surface, width, height, mode, alpha_mode)
            .map_err(|e| format!("desktop loop: configure custom gpu surface: {e:?}"))
    }

    /// Arms (`true`) or clears the transparent frame clear (decision
    /// 482 — the `WindowStyle::Transparent` leg: frames clear to
    /// `alpha = 0` instead of the page background; pair with a
    /// premultiplied-alpha swapchain, see
    /// [`SceneCore::configure_gpu_surface_alpha`]).
    pub fn set_transparent_clear(&mut self, transparent: bool) {
        self.rgb.gpu_backend.set_transparent_clear(transparent);
    }

    /// Whether frames clear transparent (decision 482 — armed by
    /// the runners for `WindowStyle::Transparent` only).
    pub fn transparent_clear(&self) -> bool {
        self.rgb.gpu_backend.transparent_clear()
    }

    /// Picks the present mode, vsync-first: Fifo, then Mailbox,
    /// then uncapped Immediate (an event-paced UI has no business
    /// presenting uncapped — tearing plus wasted GPU for nothing;
    /// Immediate is the last resort, never the default). Loud either
    /// way, with the offered list for blank-window diagnosis.
    /// Benchmarks take [`SceneCore::pick_uncapped_mode`] instead.
    pub fn pick_gpu_mode(&self, surface: &wgpu::Surface<'_>) -> wgpu::PresentMode {
        let offered = self
            .rgb
            .gpu_backend
            .adapter()
            .map(|a| surface.get_capabilities(a).present_modes)
            .unwrap_or_default();
        let mode = prefer_present_mode(&offered, false);
        eprintln!("oppa-app: offered present modes: {offered:?}");
        eprintln!("oppa-app: present mode {mode:?}");
        mode
    }

    /// Picks the present mode, throughput-first: uncapped Immediate,
    /// then Mailbox, then vsync Fifo. Benchmark/continuous-repaint
    /// only — product loops take [`SceneCore::pick_gpu_mode`].
    pub fn pick_uncapped_mode(&self, surface: &wgpu::Surface<'_>) -> wgpu::PresentMode {
        let offered = self
            .rgb
            .gpu_backend
            .adapter()
            .map(|a| surface.get_capabilities(a).present_modes)
            .unwrap_or_default();
        let mode = prefer_present_mode(&offered, true);
        eprintln!("oppa-app: offered present modes: {offered:?}");
        eprintln!("oppa-app: present mode {mode:?} (uncapped)");
        mode
    }

    /// Presents the custom GPU backend through the real swapchain.
    /// Requires GPU mode (see `enable_gpu`); a CPU-only call is a loud `Err`.
    pub fn present_gpu(
        &mut self,
        surface: &wgpu::Surface<'_>,
    ) -> Result<oppa_gpu::PresentReport, String> {
        if !self.is_gpu() {
            return Err("desktop loop: present_gpu while CPU-only".to_string());
        }
        self.rgb
            .gpu_backend
            .present_surface(self.rgb.gpu_surface, surface)
            .map_err(|e| format!("desktop loop: present custom gpu: {e:?}"))
    }

    /// Notes one presented frame for the debug overlay (runners call
    /// this after every present, both modes): the meter clock ticks
    /// on present cadence and the badge text refreshes at most 4Hz
    /// through a throttled root re-run (see
    /// [`ComponentHost::note_overlay_frame`](oppa::ComponentHost::note_overlay_frame)).
    /// Quiet no-op unless the overlay is enabled.
    pub fn note_frame_presented(&self) {
        self.session.host.note_overlay_frame();
    }

    /// Benchmark-harness seam (oracle-owned, not product — decision
    /// 400: the `frameloop` damage/frame loops measure through these
    /// narrow per-op methods instead of borrowing the engine's
    /// pieces, so no caller reaches into the engine through
    /// accessors anymore): brings up a headless GLES context for the
    /// offscreen proof rows (no swapchain — only `present_gpu` needs
    /// one). Loud `Err` when no GLES device serves — never a silent
    /// CPU substitution (the runner logs and falls through to Vulkan).
    pub fn ensure_benchmark_gpu_gles(&mut self) -> Result<String, String> {
        self.rgb
            .gpu_backend
            .ensure_gpu_gles()
            .map_err(|e| format!("desktop loop: benchmark gpu gles: {e:?}"))
    }

    /// Benchmark-harness seam (same rule): the Vulkan offscreen row.
    pub fn ensure_benchmark_gpu_vulkan(&mut self) -> Result<String, String> {
        self.rgb
            .gpu_backend
            .ensure_gpu_vulkan()
            .map_err(|e| format!("desktop loop: benchmark gpu vulkan: {e:?}"))
    }

    /// Benchmark-harness seam (same rule): one offscreen GPU frame
    /// with readback off the engine's LIVE GPU surface (a theme-toggle
    /// refit re-homes it — the id is re-read every call, so callers
    /// keep no surface id across calls to go stale).
    pub fn benchmark_gpu_pixels(&mut self) -> Result<oppa_gpu::RgbaImage, String> {
        let surf = self.rgb.gpu_surface;
        self.rgb
            .gpu_backend
            .render_pixels(surf)
            .map_err(|e| format!("desktop loop: benchmark gpu pixels: {e:?}"))
    }

    /// Benchmark-harness seam (same rule): one offscreen GPU frame
    /// without readback (same GPU work + queue stall, no copy/map —
    /// the `frameloop` bare leg). Returns its wall ms. Live surface
    /// rule as above.
    pub fn benchmark_gpu_noread(&mut self) -> Result<f64, String> {
        let surf = self.rgb.gpu_surface;
        self.rgb
            .gpu_backend
            .render_noread(surf)
            .map_err(|e| format!("desktop loop: benchmark gpu noread: {e:?}"))
    }

    /// Benchmark-harness seam (same rule): one incremental CPU damage
    /// frame through THIS engine's own pieces — `flip` against the
    /// host, settle, one full plan build, CPU-arm paint — returning
    /// the pixmap readback byte length (the byte-drift guard input).
    /// No diff commit rides along (the plan paints directly — the same
    /// settle → build → paint → readback the damage loop always
    /// measured), no theme/image publish (steady-state frames flip one
    /// control, never the chrome). The live CPU surface is re-read
    /// every call for the same re-homing rule as the GPU legs above.
    pub fn benchmark_cpu_frame(
        &mut self,
        flip: impl FnOnce(&mut ComponentHost),
    ) -> Result<usize, String> {
        flip(&mut self.session.host);
        self.session.host.run_until_idle();
        let plan = self
            .session
            .host
            .with_retained_mut(|rec, styles| self.session.builder.build_full(rec, styles));
        let surf = self.rgb.surface;
        self.rgb
            .cpu
            .paint(surf, &plan)
            .map_err(|e| format!("desktop loop: benchmark cpu paint: {e:?}"))?;
        let px = self
            .rgb
            .cpu
            .pixmap(surf)
            .ok_or_else(|| "desktop loop: benchmark cpu pixmap missing".to_string())?;
        Ok(px.pixels().len() * 4)
    }

    /// Mounts the root component and settles the first frame.
    pub fn mount<P: Props>(
        &self,
        name: &str,
        props: P,
        render: fn(&Ctx, &P) -> VNode,
    ) -> MountHandle<P> {
        let handle = self.session.host.mount(name, props, render);
        self.session.host.run_until_idle();
        handle
    }
}

/// Preference order over offered present modes (pure for tests):
/// vsync-first (`Fifo → Mailbox → Immediate`) for product loops,
/// throughput-first reversed for benchmarks. Empty offers fall to
/// Fifo (every backend serves vsync — a missing Fifo is a driver
/// fact, and tearing beats black).
fn prefer_present_mode(offered: &[wgpu::PresentMode], uncapped: bool) -> wgpu::PresentMode {
    use wgpu::PresentMode::{Fifo, Immediate, Mailbox};
    let order = if uncapped {
        [Immediate, Mailbox, Fifo]
    } else {
        [Fifo, Mailbox, Immediate]
    };
    order
        .into_iter()
        .find(|mode| offered.contains(mode))
        .unwrap_or(Fifo)
}

#[cfg(test)]
mod tests {
    use super::prefer_present_mode;
    use wgpu::PresentMode::{Fifo, Immediate, Mailbox};

    #[test]
    fn product_prefers_vsync_first() {
        // Full offer: Fifo wins (no tearing, no wasted GPU).
        assert_eq!(
            prefer_present_mode(&[Immediate, Mailbox, Fifo], false),
            Fifo
        );
        // Fifo missing: Mailbox still vsyncs (never Immediate while
        // a vsync row exists).
        assert_eq!(prefer_present_mode(&[Immediate, Mailbox], false), Mailbox);
        // Immediate-only box: tearing beats black.
        assert_eq!(prefer_present_mode(&[Immediate], false), Immediate);
        // Empty offer: Fifo fallback (every backend serves vsync).
        assert_eq!(prefer_present_mode(&[], false), Fifo);
    }

    #[test]
    fn benchmark_prefers_throughput_first() {
        // Full offer: uncapped Immediate wins (benchmark cadence).
        assert_eq!(
            prefer_present_mode(&[Fifo, Mailbox, Immediate], true),
            Immediate
        );
        // Immediate missing: Mailbox before Fifo.
        assert_eq!(prefer_present_mode(&[Fifo, Mailbox], true), Mailbox);
        // Fifo-only box: still Fifo.
        assert_eq!(prefer_present_mode(&[Fifo], true), Fifo);
    }
}
