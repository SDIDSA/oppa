// ---------------------------------------------------------------------------
// GPU surfaces and present modes
// ---------------------------------------------------------------------------

use super::*;

impl DesktopLoop {
    /// Creates the host (viewport + text service + layout config) and
    /// both paint surfaces (CPU pixmap + Vello scene twin). Loud on
    /// any failure. Starts CPU-only; runners enable GPU against the
    /// live window (see `enable_gpu`). Ticks the system clock (live
    /// loops ??? see [`DesktopLoop::with_clock`] for the deterministic
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

    /// Clock-injected construction (Round 15.2, decision 313 ??? the
    /// headless blink rig: a `MockClock` owner advances time
    /// explicitly, so flip demand proves without sleeps). Otherwise
    /// identical to [`DesktopLoop::new`].
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
        // page background (a fresh host is Light ??? the pre-contract
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
        let mut vello = VelloBackend::new();
        let vello_surface = vello
            .create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background: page_bg,
            })
            .map_err(|e| format!("desktop loop: create vello surface: {e:?}"))?;
        let mut custom_gpu = CustomGpuBackend::new();
        let custom_gpu_surface = custom_gpu
            .create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background: page_bg,
            })
            .map_err(|e| format!("desktop loop: create custom_gpu surface: {e:?}"))?;
        Ok(Self {
            host,
            cpu,
            surface,
            vello,
            vello_surface,
            custom_gpu,
            custom_gpu_surface,
            active: RendererKind::Cpu,
            gpu_mode: None,
            builder: FramePlanBuilder::new(1.0),
            viewport: (w, h),
            dpr: 1.0,
            clipboard: Box::new(InMemoryClipboard::new()),
            last_plan: None,
            last_caret_shown: false,
            save_dialog: None,
            folder_dialog: None,
            theme_source: None,
            window_control: None,
            close_handler: None,
            poll_hook: None,
            page_bg,
        })
    }

    /// Which backend the platform glue presents (`Cpu` until a
    /// runner enables GPU; runners flip back on loud GPU failure).
    pub fn renderer_kind(&self) -> RendererKind {
        self.active
    }

    /// True once a runner enabled hardware presentation.
    pub fn is_gpu(&self) -> bool {
        self.active == RendererKind::Gpu || self.active == RendererKind::CustomGpu
    }

    /// Human name for logs (`gpu` / `custom_gpu` / `cpu`).
    pub fn renderer_name(&self) -> &'static str {
        match self.active {
            RendererKind::Gpu => "gpu",
            RendererKind::CustomGpu => "custom_gpu",
            RendererKind::Cpu => "cpu",
        }
    }

    /// Swapchain present mode once GPU is up (`None` while CPU-only).
    pub fn gpu_present_mode(&self) -> Option<wgpu::PresentMode> {
        self.gpu_mode
    }

    /// Vello scene surface (the GPU twin; painted whenever
    /// [`RendererKind::Gpu`] is active).
    pub fn vello_surface_id(&self) -> SurfaceId {
        self.vello_surface
    }

    /// Custom GPU surface (painted whenever
    /// [`RendererKind::CustomGpu`] is active).
    pub fn custom_gpu_surface_id(&self) -> SurfaceId {
        self.custom_gpu_surface
    }

    /// Custom GPU retained op count.
    pub fn custom_gpu_retained_op_count(&self) -> Option<usize> {
        self.custom_gpu.retained_op_count(self.custom_gpu_surface)
    }

    /// CPU pixmap surface (always painted ??? the GDI / softbuffer
    /// source and the GPU fallback pixels).
    pub fn cpu_surface_id(&self) -> SurfaceId {
        self.surface
    }

    /// Enables hardware presentation after the runner configured the
    /// swapchain (mirrors the fps driver's GPU arm): the next
    /// [`DesktopLoop::repaint`] paints both backends. Callers repaint
    /// immediately so the GPU twin catches up from its
    /// creation-time scene.
    pub fn enable_gpu(&mut self, mode: wgpu::PresentMode) {
        if renderer_override() == Some(RendererKind::CustomGpu) {
            self.active = RendererKind::CustomGpu;
        } else {
            self.active = RendererKind::Gpu;
        }
        self.gpu_mode = Some(mode);
    }

    /// Explicitly enables custom GPU rasterizer mode.
    pub fn enable_custom_gpu(&mut self, mode: wgpu::PresentMode) {
        self.active = RendererKind::CustomGpu;
        self.gpu_mode = Some(mode);
    }

    /// Drops back to the CPU pixmap (loud GPU failure path): later
    /// repaints skip the Vello twin again; its retained scene stays
    /// for a later re-enable (refit keeps sizes in lockstep).
    pub fn disable_gpu(&mut self) {
        self.active = RendererKind::Cpu;
        self.gpu_mode = None;
    }

    /// Ensures a live GPU context on an adapter compatible with
    /// `surface` (the swapchain path ??? the caller owns the
    /// `wgpu::Instance` + `wgpu::Surface` from the platform window
    /// and hands both here, mirroring
    /// `oppa-fps::driver::FpsDriver::build_gpu`). Disk-cache
    /// round-trip included (decision 199): `cache_data` seeds
    /// pipeline creation; the returned bytes are what the caller
    /// stores. Loud `Err` when no compatible adapter exists or the
    /// device/renderer refuses it ??? never a silent CPU substitution
    /// (the runner logs and falls back).
    pub fn ensure_gpu_for_surface_with_cache(
        &mut self,
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
        cache_data: Option<Vec<u8>>,
    ) -> Result<(String, Option<Vec<u8>>), String> {
        if renderer_override() == Some(RendererKind::CustomGpu) {
            let info = self
                .custom_gpu
                .ensure_gpu_for_surface(instance, surface)
                .map_err(|e| format!("desktop loop: ensure custom gpu for surface: {e:?}"))?;
            self.active = RendererKind::CustomGpu;
            Ok((format!("{info} (custom_gpu)"), None))
        } else {
            self.vello
                .ensure_gpu_for_surface_with_cache(instance, surface, cache_data)
                .map_err(|e| format!("desktop loop: ensure gpu for surface: {e:?}"))
        }
    }

    /// Configures `surface` for presentation of the current viewport
    /// (format = first non-sRGB 8-bit the surface offers; the fps
    /// choice). Requires a context from
    /// [`DesktopLoop::ensure_gpu_for_surface_with_cache`]. Returns
    /// the chosen format for the run record.
    pub fn configure_gpu_surface(
        &mut self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
        mode: wgpu::PresentMode,
    ) -> Result<wgpu::TextureFormat, String> {
        if self.active == RendererKind::CustomGpu {
            self.custom_gpu
                .configure_surface(surface, width, height, mode)
                .map_err(|e| format!("desktop loop: configure custom gpu surface: {e:?}"))
        } else {
            self.vello
                .configure_surface_with_present_mode(surface, width, height, mode)
                .map_err(|e| format!("desktop loop: configure gpu surface: {e:?}"))
        }
    }

    /// Picks the present mode: uncapped Immediate first, then
    /// Mailbox, then vsync Fifo (the fps-driver order ??? the
    /// throughput leg first). Loud either way, with the offered list
    /// for blank-window diagnosis.
    pub fn pick_gpu_mode(&self, surface: &wgpu::Surface<'_>) -> wgpu::PresentMode {
        let offered = if self.active == RendererKind::CustomGpu {
            self.custom_gpu
                .adapter()
                .map(|a| surface.get_capabilities(a).present_modes)
                .unwrap_or_default()
        } else {
            self.vello
                .gpu_adapter()
                .map(|a| surface.get_capabilities(a).present_modes)
                .unwrap_or_default()
        };
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

    /// Presents the active GPU backend through the real swapchain.
    /// Requires GPU mode (see `enable_gpu`); a CPU-only call is a loud `Err`.
    pub fn present_gpu(
        &mut self,
        surface: &wgpu::Surface<'_>,
    ) -> Result<oppa_vello::PresentReport, String> {
        if !self.is_gpu() {
            return Err("desktop loop: present_gpu while CPU-only".to_string());
        }
        if self.active == RendererKind::CustomGpu {
            let rep = self
                .custom_gpu
                .present_surface(self.custom_gpu_surface, surface)
                .map_err(|e| format!("desktop loop: present custom gpu: {e:?}"))?;
            Ok(oppa_vello::PresentReport {
                width: rep.width,
                height: rep.height,
                format: rep.format,
                cpu_ms: rep.cpu_ms,
                acquire_ms: 0.0,
                render_ms: rep.cpu_ms,
                blit_setup_ms: 0.0,
                blit_submit_ms: 0.0,
                present_ms: 0.0,
                target_ms: 0.0,
            })
        } else {
            self.vello
                .present_surface(self.vello_surface, surface)
                .map_err(|e| format!("desktop loop: present gpu: {e:?}"))
        }
    }
}
