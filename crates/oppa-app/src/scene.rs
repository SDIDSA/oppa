// ---------------------------------------------------------------------------
// SceneCore — the unified platform-free app-loop engine
// ---------------------------------------------------------------------------
//
// One implementation of the pixel presentation pipeline that both the
// `DesktopLoop` runners (Windows GDI/GPU, Linux winit/softbuffer) and
// the Android runner (`oppa-android-app`) drive. It owns:
//
// - the `ComponentHost` scene,
// - the `FramePlanBuilder` (selection/caret/theme-tracking),
// - BOTH paint arms (CPU pixmap + GPU scene twin) with their surfaces,
// - the diff cursor (commits always land in both backends),
// - the theme contract (build theme + page background + surface refit),
// - the resize/DPR rebase rules,
// - GPU bring-up state (active renderer, recorded API, present mode,
//   present-mode negotiation),
// - the last committed plan + caret-overlay presence (frame telemetry).
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

/// Platform-free pixel scene engine — see the module docs.
pub struct SceneCore {
    pub(crate) host: ComponentHost,
    pub(crate) cpu: CpuBackend,
    pub(crate) surface: SurfaceId,
    /// Hardware scene twin: same frame plans encode here whenever
    /// GPU is active (headless encoding needs no device — only
    /// `present_gpu` needs the swapchain context). Always created
    /// beside the CPU surface so enabling GPU never rebuilds the
    /// scene; CPU-only repaints leave it untouched (a Text run
    /// without an injected face would refuse loudly there while the
    /// CPU path still draws bars — the decision-200 contract).
    pub(crate) gpu_backend: GpuBackend,
    pub(crate) gpu_surface: SurfaceId,
    /// Which backend the platform glue presents. CPU until a runner
    /// enables GPU against a live window; runners flip it back on
    /// loud GPU failure (fallback, never silent).
    pub(crate) active: RendererKind,
    /// Winning `wgpu` backend recorded at bring-up (see
    /// [`GpuApi`]). `None` until a runner records one.
    pub(crate) gpu_api: Option<GpuApi>,
    /// Swapchain present mode once GPU is up (Immediate first, then
    /// Mailbox, then Fifo). `None` while CPU-only.
    pub(crate) gpu_mode: Option<wgpu::PresentMode>,
    pub(crate) builder: FramePlanBuilder,
    /// Paint surface size in device px (physical pixels).
    pub(crate) viewport: (u32, u32),
    /// Output scale: device px per CSS px (Round 2.4, OQ-G10-2).
    /// The host viewport, layout config, and builder all derive
    /// from (`viewport`, `dpr`) — never set one without the others
    /// (see `resize` / `set_device_pixel_ratio`).
    pub(crate) dpr: f32,
    /// Scene clear color (theme contract round): both paint surfaces
    /// clear to the host theme's `background`. Tracked so a theme
    /// toggle refits the surfaces instead of presenting a stale page
    /// (same one-refit path as resize/DPR, never two).
    pub(crate) page_bg: oppa::Color,
    /// The last committed frame plan (Round 15.2, decision 313 —
    /// test/presenter observability: proves which overlay `Rect`s a
    /// repaint committed; one retained plan, refreshed every
    /// repaint, never a growing history).
    last_plan: Option<FramePlan>,
    /// Caret overlay presence at the last repaint (Round 15.2,
    /// decision 313 — the blink demand: the flip writes no signals,
    /// so the tick path compares presence instead of waiting for
    /// dirt that never comes).
    last_caret_shown: bool,
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
        // Theme contract round: surfaces clear to the host theme's
        // page background (a fresh host is Light — the pre-contract
        // white, pixel-identical).
        let page_bg = ThemeTokens::of(host.theme().mode()).background;
        let mut cpu = CpuBackend::new();
        let surface = cpu
            .create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background: page_bg,
            })
            .map_err(|e| format!("desktop loop: create cpu surface: {e:?}"))?;
        let mut gpu_backend = GpuBackend::new();
        let gpu_surface = gpu_backend
            .create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background: page_bg,
            })
            .map_err(|e| format!("desktop loop: create gpu surface: {e:?}"))?;
        Ok(Self {
            host,
            cpu,
            surface,
            gpu_backend,
            gpu_surface,
            active: RendererKind::Cpu,
            gpu_api: None,
            gpu_mode: None,
            builder: FramePlanBuilder::new(1.0),
            viewport: (w, h),
            dpr: 1.0,
            page_bg,
            last_plan: None,
            last_caret_shown: false,
        })
    }

    /// The host (mount records, recursive journal reads — shared by
    /// every runner and the testkit escape-hatch rule).
    pub fn host(&self) -> &ComponentHost {
        &self.host
    }

    /// Injects a raster face for one shaper font id (decision 200):
    /// without it the CPU backend draws advance-cell bars and the
    /// GPU twin refuses loudly at paint; with it, real outlines
    /// on both. Best-effort by contract (callers warn, never die,
    /// when the face is unreadable). Always lands in both backends
    /// so a later `enable_gpu` never paints tofu.
    pub fn set_font_for(&mut self, id: oppa::text::FontId, bytes: Vec<u8>, index: u32) {
        self.cpu.set_font_for(id, bytes.clone(), index);
        self.gpu_backend.set_font_for(id, bytes, index);
    }

    /// The frame engine: replays every new diff into BOTH backends
    /// (cheap bookkeeping), publishes the focused selection/caret as
    /// build-scoped overlays, follows the theme contract (build theme
    /// and page background with surface refit on toggle), builds ONE
    /// full plan, syncs static-image pixel deposits (CPU always, GPU
    /// twin when active), and paints both arms. Pixels encode into the
    /// CPU pixmap always (the GDI / softbuffer source and the GPU
    /// fallback) plus into the custom twin whenever GPU is active.
    /// CPU-only repaints skip custom so a faceless test shaper never
    /// trips the custom loud-no-face rule while the CPU path still
    /// draws bars. Returns the plan's damage-rect count; pixels
    /// failures are loud `Err`s.
    pub fn repaint(&mut self) -> Result<usize, String> {
        for d in self.host.diffs_from(0) {
            self.cpu
                .commit(&d)
                .map_err(|e| format!("desktop loop: cpu commit: {e:?}"))?;
            self.gpu_backend
                .commit(&d)
                .map_err(|e| format!("desktop loop: gpu commit: {e:?}"))?;
        }
        // Round 15.2 (decision 313): the text overlays ride every
        // committed frame — the shared build-scoped rule from the
        // paint hooks (selection + caret resolved outside the
        // retained borrow). Before this, live presented frames
        // committed with both overlays `None`: zero selection rects
        // and zero carets on GPU and GDI alike.
        self.builder
            .set_selection(self.host.focused_selection_paint());
        self.builder.set_caret(self.host.focused_caret_paint());
        // Theme contract round: the build theme rides the same
        // per-frame publish rule (default text ink follows the host
        // mode), and a toggled page background refits the surfaces
        // before painting (same one-refit path as resize/DPR —
        // steady state is a color compare, never surface churn).
        let mode = self.host.theme().mode();
        self.builder.set_theme_mode(mode);
        let want_bg = ThemeTokens::of(mode).background;
        if want_bg != self.page_bg {
            self.page_bg = want_bg;
            self.refit_surface()?;
        }
        let plan = self
            .host
            .with_retained_mut(|rec, styles| self.builder.build_full(rec, styles));
        self.last_caret_shown = self.builder.caret().is_some();
        self.last_plan = Some(plan.clone());
        let damage = plan.damage.len();
        let images = self.host.images();
        for op in &plan.ops {
            if let oppa::DrawOp::RImg { image, .. } = op {
                if !self.cpu.has_image(*image) && images.pixels_of(*image).is_some() {
                    self.cpu.insert_cached(&images, *image);
                }
                if self.is_gpu()
                    && !self.gpu_backend.has_image(*image)
                    && images.pixels_of(*image).is_some()
                {
                    self.gpu_backend.insert_cached(&images, *image);
                }
            }
        }
        self.cpu
            .paint(self.surface, &plan)
            .map_err(|e| format!("desktop loop: cpu paint: {e:?}"))?;
        if self.is_gpu() {
            self.gpu_backend
                .paint(self.gpu_surface, &plan)
                .map_err(|e| format!("desktop loop: gpu paint: {e:?}"))?;
        }
        Ok(damage)
    }

    /// The last committed frame plan (Round 15.2, decision 313 —
    /// proves which overlay `Rect`s a repaint committed; `None`
    /// before the first repaint, never a live borrow).
    pub fn last_plan(&self) -> Option<FramePlan> {
        self.last_plan.clone()
    }

    /// Seconds until the focused caret's next blink flip, if any
    /// (Round 15.2, decision 313 — the runner wake query): `Some`
    /// while a collapsed caret is focused (wake then to repaint the
    /// flip); `None` otherwise. Pure over the host.
    pub fn blink_tick_in_secs(&self) -> Option<f64> {
        self.host.caret_blink_in_secs()
    }

    /// True when the caret overlay's presence flipped since the last
    /// repaint (Round 15.2, decision 313 — the blink demand). Pure
    /// over the host state above.
    pub fn caret_overlay_dirty(&self) -> bool {
        self.host.focused_caret_paint().is_some() != self.last_caret_shown
    }

    /// Settles and repaints iff the caret overlay flipped (Round
    /// 15.2, decision 313 — the timer-tick path runners call when
    /// no input arrived): returns `true` when it repainted, `false`
    /// when settled-quiet (never a spin — a quiet poll schedules
    /// nothing). Paint failures are loud `Err`s like `repaint`.
    pub fn poll_blink(&mut self) -> Result<bool, String> {
        self.host.run_until_idle();
        if self.caret_overlay_dirty() {
            self.repaint()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Earliest component-timer due instant in host-clock ms (Round
    /// 21.1, decision 328 — the wake-query twin of
    /// `blink_tick_in_secs`). Pure over the host.
    pub fn next_timer_due_ms(&self) -> Option<f64> {
        let now = self.host.now_ms();
        self.host.next_timer_due_ms(now)
    }

    /// Fires due component timers, settling + repainting when
    /// anything fired (Round 21.1, decision 328 — the timer-tick
    /// path runners call on wake): returns the fired count. Zero
    /// when nothing was due (quiet). Paint failures are loud `Err`s.
    pub fn tick_timers(&mut self) -> Result<usize, String> {
        let now = self.host.now_ms();
        let fired = self.host.tick_timers(now);
        if fired > 0 {
            self.host.run_until_idle();
            self.repaint()?;
        }
        Ok(fired)
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
        let (w, h) = (width_px.max(1), height_px.max(1));
        if (w, h) == self.viewport {
            return Ok(());
        }
        self.viewport = (w, h);
        self.host
            .set_viewport(w as f32 / self.dpr, h as f32 / self.dpr);
        self.refit_surface()?;
        self.host.run_until_idle();
        Ok(())
    }

    /// Current output scale (device px per CSS px).
    pub fn dpr(&self) -> f32 {
        self.dpr
    }

    /// Viewport in device px.
    pub fn viewport(&self) -> (u32, u32) {
        self.viewport
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
        if !dpr.is_finite() || dpr <= 0.0 {
            panic!(
                "desktop loop: device pixel ratio {dpr} refused — scales are finite and positive"
            );
        }
        if dpr == self.dpr {
            return Ok(());
        }
        // CSS size is viewport_px / old_dpr (exact at 1.0, the only
        // previously reachable state — no drift by construction).
        let css_w = self.viewport.0 as f32 / self.dpr;
        let css_h = self.viewport.1 as f32 / self.dpr;
        self.dpr = dpr;
        let mut config = self.host.layout_config();
        config.device_pixel_ratio = dpr;
        self.host.set_layout_config(config);
        self.builder.set_dpr(dpr);
        self.host.set_viewport(css_w, css_h);
        let w = (css_w * dpr).round().max(1.0) as u32;
        let h = (css_h * dpr).round().max(1.0) as u32;
        self.viewport = (w, h);
        self.refit_surface()?;
        self.host.run_until_idle();
        Ok(())
    }

    /// Recreates BOTH paint surfaces at the tracked viewport size
    /// with the tracked page background and replays the scene
    /// commits onto them (the Android `rebase_surfaces` rule and the
    /// desktop `refit_surface` rule are this one path now — a theme
    /// toggle and a resize can never diverge again). The GPU twin
    /// refits even while CPU-only so a later `enable_gpu` never
    /// presents a stale size.
    pub(crate) fn refit_surface(&mut self) -> Result<(), String> {
        let (w, h) = self.viewport;
        let bg = self.page_bg;
        let old = self.surface;
        let next = self
            .cpu
            .create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background: bg,
            })
            .map_err(|e| format!("desktop loop: recreate cpu surface: {e:?}"))?;
        self.surface = next;
        let _ = self.cpu.destroy_surface(old);
        let old_gpu = self.gpu_surface;
        let next_gpu = self
            .gpu_backend
            .create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background: bg,
            })
            .map_err(|e| format!("desktop loop: recreate gpu surface: {e:?}"))?;
        self.gpu_surface = next_gpu;
        let _ = self.gpu_backend.destroy_surface(old_gpu);
        // Replay all commits from zero so the fresh surfaces'
        // `live_nodes` match the scene (the Android `rebase_surfaces`
        // rule — the desktop runners had no callers of the replay
        // because their first repaint after refit replays naturally;
        // replaying here keeps every driver correct with the same
        // method, including offscreen flows that paint without a
        // settle between resize and paint).
        for d in self.host.diffs_from(0) {
            self.cpu
                .commit(&d)
                .map_err(|e| format!("desktop loop: rebase cpu commit: {e:?}"))?;
            self.gpu_backend
                .commit(&d)
                .map_err(|e| format!("desktop loop: rebase gpu commit: {e:?}"))?;
        }
        Ok(())
    }

    /// Straight RGBA8 row-major pixels of the last paint (opaque
    /// scenes only — a non-opaque pixel is a loud `Err`, the m10
    /// straightening rule, formerly the Android `straight_rgba`.
    /// Present glue converts onward per platform (BGRA for GDI,
    /// XRGB words for softbuffer).
    pub fn rgba8(&self) -> Result<Vec<u8>, String> {
        let px = self
            .cpu
            .pixmap(self.surface)
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
        self.active
    }

    /// True once a runner enabled hardware presentation (any GPU
    /// variant — the only GPU presenter since decision 388).
    pub fn is_gpu(&self) -> bool {
        self.active.is_gpu()
    }

    /// Human name for logs (one per `OPPA_RENDERER` value).
    pub fn renderer_name(&self) -> &'static str {
        match self.active {
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
            return match self.gpu_api {
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
        self.gpu_api
    }

    /// Records the winning `wgpu` backend after bring-up (runners
    /// call this with the attempt name that succeeded, before
    /// publishing [`SceneCore::presenter_badge_name`]).
    pub fn set_gpu_api(&mut self, api: GpuApi) {
        self.gpu_api = Some(api);
    }

    /// Swapchain present mode once GPU is up (`None` while CPU-only).
    pub fn gpu_present_mode(&self) -> Option<wgpu::PresentMode> {
        self.gpu_mode
    }

    /// GPU surface (painted whenever a GPU variant is active).
    pub fn gpu_surface_id(&self) -> SurfaceId {
        self.gpu_surface
    }

    /// GPU twin retained op count.
    pub fn gpu_retained_op_count(&self) -> Option<usize> {
        self.gpu_backend.retained_op_count(self.gpu_surface)
    }

    /// CPU retained op count (test symmetry with the GPU twin).
    pub fn cpu_retained_op_count(&self) -> Option<usize> {
        self.cpu.retained_op_count(self.surface)
    }

    /// CPU pixmap surface (always painted — the GDI / softbuffer
    /// source and the GPU fallback pixels).
    pub fn cpu_surface_id(&self) -> SurfaceId {
        self.surface
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
        self.active = renderer_override().unwrap_or(RendererKind::GpuAuto);
        self.gpu_mode = Some(mode);
    }

    /// Enables hardware presentation with an explicit renderer kind
    /// (no environment consultation): the Android runner's bring-up
    /// shape — zygote-inherited env must not decide the arm there.
    pub fn enable_gpu_pinned(&mut self, kind: RendererKind, mode: wgpu::PresentMode) {
        self.active = kind;
        self.gpu_mode = Some(mode);
    }

    /// Drops back to the CPU pixmap (loud GPU failure path): later
    /// repaints skip the GPU twin again; its retained scene stays
    /// for a later re-enable (refit keeps sizes in lockstep).
    pub fn disable_gpu(&mut self) {
        self.active = RendererKind::Cpu;
        self.gpu_mode = None;
        self.gpu_api = None;
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
            .gpu_backend
            .ensure_gpu_for_surface(instance, surface)
            .map_err(|e| format!("desktop loop: ensure gpu for surface: {e:?}"))?;
        self.active = renderer_override().unwrap_or(RendererKind::GpuAuto);
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
            .gpu_backend
            .ensure_gpu_for_surface(instance, surface)
            .map_err(|e| format!("desktop loop: ensure pinned gpu for surface: {e:?}"))?;
        self.active = kind;
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
        self.gpu_backend
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
        self.gpu_backend
            .present_surface(self.gpu_surface, surface)
            .map_err(|e| format!("desktop loop: present custom gpu: {e:?}"))
    }

    /// Notes one presented frame for the debug overlay (runners call
    /// this after every present, both modes): the meter clock ticks
    /// on present cadence and the badge text refreshes at most 4Hz
    /// through a throttled root re-run (see
    /// [`ComponentHost::note_overlay_frame`](oppa::ComponentHost::note_overlay_frame)).
    /// Quiet no-op unless the overlay is enabled.
    pub fn note_frame_presented(&self) {
        self.host.note_overlay_frame();
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
            &mut self.host,
            &mut self.builder,
            &mut self.cpu,
            self.surface,
        )
    }

    /// Mutable GPU twin for offscreen render/readback benchmarking
    /// (`render_pixels` / `render_noread` — no swapchain context).
    /// The engine otherwise paints it; keep runs out if at all
    /// possible.
    pub fn gpu_backend_mut(&mut self) -> &mut GpuBackend {
        &mut self.gpu_backend
    }

    /// Mounts the root component and settles the first frame.
    pub fn mount<P: Props>(
        &self,
        name: &str,
        props: P,
        render: fn(&Ctx, &P) -> VNode,
    ) -> MountHandle<P> {
        let handle = self.host.mount(name, props, render);
        self.host.run_until_idle();
        handle
    }
}
