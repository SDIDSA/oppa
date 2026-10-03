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
        let bg = ThemeTokens::of(host.theme().mode()).background;
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
    pub fn set_font_for(&mut self, id: oppa::text::FontId, bytes: Vec<u8>, index: u32) {
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
    /// CPU pixmap always plus the custom twin whenever GPU is active.
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

    /// CPU pixmap surface (always painted — the GDI / softbuffer
    /// source and the GPU fallback pixels).
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
    /// for a later re-enable (refit keeps sizes in lockstep).
    pub fn disable_gpu(&mut self) {
        self.rgb.active = RendererKind::Cpu;
        self.rgb.gpu_mode = None;
        self.rgb.gpu_api = None;
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
        self.rgb
            .gpu_backend
            .configure_surface(surface, width, height, mode)
            .map_err(|e| format!("desktop loop: configure custom gpu surface: {e:?}"))
    }

    /// Picks the present mode: uncapped Immediate first, then
    /// Mailbox, then vsync Fifo (throughput leg first). Loud either
    /// way, with the offered list for blank-window diagnosis.
    /// The desktop `pick_gpu_mode` and the Android `pick_uncapped_mode`
    /// are this one method now.
    pub fn pick_gpu_mode(&self, surface: &wgpu::Surface<'_>) -> wgpu::PresentMode {
        let offered = self
            .rgb
            .gpu_backend
            .adapter()
            .map(|a| surface.get_capabilities(a).present_modes)
            .unwrap_or_default();
        eprintln!("oppa-app: offered present modes: {offered:?}");
        for mode in [
            wgpu::PresentMode::Immediate,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::Fifo,
        ] {
            if offered.contains(&mode) {
                eprintln!("oppa-app: present mode {mode:?}");
                return mode;
            }
        }
        eprintln!("oppa-app: no preferred mode offered, Fifo");
        wgpu::PresentMode::Fifo
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

    /// Reads the retained benchmark machinery out of the engine
    /// (Round 20.4, decision 327 — the `frameloop` damage loop
    /// predates [`SceneCore`] and still drives
    /// `ComponentHost` / `FramePlanBuilder` / `CpuBackend`
    /// directly): the caller gets the SAME pieces this engine uses —
    /// its own builder, CPU surface, diff cursor — so the benchmark
    /// measures this exact scene, not a copy. The surface id is the
    /// current CPU pixmap surface; refit re-homes it, so callers must
    /// re-read it after any resize/DPR/theme change. Nothing else
    /// should reach into the engine this way: everything above
    /// delegates through the engine methods.
    pub fn parts_mut(
        &mut self,
    ) -> (
        &mut ComponentHost,
        &mut FramePlanBuilder,
        &mut CpuBackend,
        SurfaceId,
    ) {
        (
            &mut self.session.host,
            &mut self.session.builder,
            &mut self.rgb.cpu,
            self.rgb.surface,
        )
    }

    /// Mutable GPU twin for offscreen render/readback benchmarking
    /// (`render_pixels` / `render_noread` — no swapchain context).
    /// The engine otherwise paints it; keep runs out if at all
    /// possible.
    pub fn gpu_backend_mut(&mut self) -> &mut GpuBackend {
        &mut self.rgb.gpu_backend
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
