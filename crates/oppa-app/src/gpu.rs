// ---------------------------------------------------------------------------
// GPU surfaces, present modes, and construction (delegating to SceneCore)
// ---------------------------------------------------------------------------

use super::*;

impl DesktopLoop {
    /// Creates the loop over a fresh [`SceneCore`] (the platform-free
    /// engine — the unified Android/desktop pipeline, decision 397):
    /// viewport + text service + layout config + both paint surfaces.
    /// Loud on any failure. Starts CPU-only; runners enable GPU
    /// against the live window (see `enable_gpu`). Ticks the system
    /// clock (see [`DesktopLoop::with_clock`] for the deterministic
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
    /// identical to [`DesktopLoop::new`].
    pub fn with_clock(
        width_px: u32,
        height_px: u32,
        service: Box<dyn TextService>,
        family: &str,
        clock: Rc<dyn Clock>,
    ) -> Result<Self, String> {
        let scene = SceneCore::with_clock(width_px, height_px, service, family, clock)?;
        Ok(Self {
            scene,
            clipboard: Box::new(InMemoryClipboard::new()),
            save_dialog: None,
            folder_dialog: None,
            open_dialog: None,
            theme_source: None,
            window_control: None,
            close_handler: None,
            poll_hook: None,
            continuous_repaint: false,
            window_style: WindowStyle::Decorated,
        })
    }

    /// Which backend the platform glue presents (`Cpu` until a
    /// runner enables GPU; runners flip back on loud GPU failure).
    pub fn renderer_kind(&self) -> RendererKind {
        self.scene.renderer_kind()
    }

    /// True once a runner enabled hardware presentation (any GPU
    /// variant — the only GPU presenter since decision 388).
    pub fn is_gpu(&self) -> bool {
        self.scene.is_gpu()
    }

    /// Human name for logs (one per `OPPA_RENDERER` value).
    pub fn renderer_name(&self) -> &'static str {
        self.scene.renderer_name()
    }

    /// Badge label for the debug overlay's presenter slot
    /// (`[Gpu/Vulkan]`, `[Gpu/DX12]`, `[Gpu/GL]`, `[CPU]`) — the
    /// engine derives it from the active renderer plus the recorded
    /// `wgpu` backend (never a hardcoded label).
    pub fn presenter_badge_name(&self) -> &'static str {
        self.scene.presenter_badge_name()
    }

    /// Recorded winning `wgpu` backend underneath the GPU presenter
    /// (`None` until a runner records one at bring-up).
    pub fn gpu_api(&self) -> Option<GpuApi> {
        self.scene.gpu_api()
    }

    /// Records the winning `wgpu` backend after bring-up (runners
    /// call this with the attempt name that succeeded, before
    /// publishing [`DesktopLoop::presenter_badge_name`]).
    pub fn set_gpu_api(&mut self, api: GpuApi) {
        self.scene.set_gpu_api(api);
    }

    /// Swapchain present mode once GPU is up (`None` while CPU-only).
    pub fn gpu_present_mode(&self) -> Option<wgpu::PresentMode> {
        self.scene.gpu_present_mode()
    }

    /// GPU surface (painted whenever a GPU variant is active).
    pub fn gpu_surface_id(&self) -> SurfaceId {
        self.scene.gpu_surface_id()
    }

    /// GPU twin retained op count.
    pub fn gpu_retained_op_count(&self) -> Option<usize> {
        self.scene.gpu_retained_op_count()
    }

    /// CPU pixmap surface (always painted — the GDI / softbuffer
    /// source and the GPU fallback pixels).
    pub fn cpu_surface_id(&self) -> SurfaceId {
        self.scene.cpu_surface_id()
    }

    /// Enables hardware presentation after the runner configured the
    /// swapchain: the next [`DesktopLoop::repaint`] paints the GPU
    /// twin. Callers repaint immediately so the twin catches up from
    /// its creation-time scene.
    pub fn enable_gpu(&mut self, mode: wgpu::PresentMode) {
        self.scene.enable_gpu(mode);
    }

    /// Drops back to the CPU pixmap (loud GPU failure path): later
    /// repaints skip the GPU twin again; its retained scene stays
    /// for a later re-enable (refit keeps sizes in lockstep). Repaints
    /// once immediately when the CPU paint was opted out (decision
    /// 436 catch-up) — loud `Err`, never silent old pixels.
    pub fn disable_gpu(&mut self) -> Result<(), String> {
        self.scene.disable_gpu()
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
        self.scene.ensure_gpu_for_surface(instance, surface)
    }

    /// Configures `surface` for presentation of the current viewport
    /// (format = first non-sRGB 8-bit the surface offers).
    /// Requires a context from
    /// [`DesktopLoop::ensure_gpu_for_surface`]. Returns
    /// the chosen format for the run record.
    pub fn configure_gpu_surface(
        &mut self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
        mode: wgpu::PresentMode,
    ) -> Result<wgpu::TextureFormat, String> {
        self.scene
            .configure_gpu_surface(surface, width, height, mode)
    }

    /// Configures `surface` with an explicit composite-alpha mode
    /// (decision 482 — `PreMultiplied` for
    /// `WindowStyle::Transparent`, paired with
    /// [`DesktopLoop::set_transparent_clear`]).
    pub fn configure_gpu_surface_alpha(
        &mut self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
        mode: wgpu::PresentMode,
        alpha_mode: wgpu::CompositeAlphaMode,
    ) -> Result<wgpu::TextureFormat, String> {
        self.scene
            .configure_gpu_surface_alpha(surface, width, height, mode, alpha_mode)
    }

    /// Swapchain alpha for the loop's window style (decision 482):
    /// `PreMultiplied` for `WindowStyle::Transparent`, `Auto`
    /// otherwise. Bring-up and every reconfigure path go through
    /// [`DesktopLoop::configure_gpu_surface_styled`] so a
    /// transparent window never reverts to opaque on resize or
    /// outdated retries.
    pub fn surface_alpha_mode(&self) -> wgpu::CompositeAlphaMode {
        if self.window_style.is_transparent() {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            wgpu::CompositeAlphaMode::Auto
        }
    }

    /// Configures `surface` for the loop's window style (decision
    /// 482 — [`DesktopLoop::surface_alpha_mode`]; pair Transparent
    /// with [`DesktopLoop::set_transparent_clear`]).
    pub fn configure_gpu_surface_styled(
        &mut self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
        mode: wgpu::PresentMode,
    ) -> Result<wgpu::TextureFormat, String> {
        let alpha = self.surface_alpha_mode();
        self.configure_gpu_surface_alpha(surface, width, height, mode, alpha)
    }

    /// Arms (`true`) or clears the transparent frame clear (decision
    /// 482 — the `WindowStyle::Transparent` leg).
    pub fn set_transparent_clear(&mut self, transparent: bool) {
        self.scene.set_transparent_clear(transparent);
    }

    /// Whether frames clear transparent (decision 482).
    pub fn transparent_clear(&self) -> bool {
        self.scene.transparent_clear()
    }

    /// Picks the present mode, vsync-first (product default —
    /// benchmarks take the uncapped row). Loud either way, with the
    /// offered list for blank-window diagnosis.
    pub fn pick_gpu_mode(&self, surface: &wgpu::Surface<'_>) -> wgpu::PresentMode {
        self.scene.pick_gpu_mode(surface)
    }

    /// Presents the custom GPU backend through the real swapchain.
    /// Requires GPU mode (see `enable_gpu`); a CPU-only call is a loud `Err`.
    pub fn present_gpu(
        &mut self,
        surface: &wgpu::Surface<'_>,
    ) -> Result<oppa_gpu::PresentReport, String> {
        self.scene.present_gpu(surface)
    }
}
