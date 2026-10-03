// ---------------------------------------------------------------------------
// RgbPresenter — the pixel adapter of the shared SceneSession loop
// ---------------------------------------------------------------------------
//
// Owns ONLY the pixel-present legs the web DOM backend deliberately does
// not carry (locked #1): the CPU pixmap arm + GPU twin pair, their
// surfaces, GPU bring-up/present-mode state, and the pixel readout. The
// loop logic itself (commit replay, overlay/theme publish, plan build,
// image-sync trigger, step ladder, telemetry) lives in
// [`super::session::SceneSession`]; the bodies here moved verbatim from
// the decision-397 `SceneCore` — error wording ("desktop loop: ...") is
// preserved load-bearing — tests grep it.

use super::*;
use oppa::{FramePlan, ImageCache};
use oppa_cpu::CpuBackend;
use oppa_gpu::GpuBackend;

/// Pixel presenter adapter (CPU pixmap arm + GPU scene twin): serves
/// the desktop runners and the Android runner through [`SceneCore`]'s
/// facades.
pub(crate) struct RgbPresenter {
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
    /// The last synced image cache (the static-image deposit source
    /// the present step scans the plan against).
    images: ImageCache,
}

impl RgbPresenter {
    /// Creates both paint surfaces (CPU pixmap + GPU scene twin).
    /// Loud on any failure. Starts CPU-only; runners enable GPU
    /// against the live window (see `SceneCore::enable_gpu`).
    pub(crate) fn new(
        width_px: u32,
        height_px: u32,
        background: oppa::Color,
    ) -> Result<Self, String> {
        let (w, h) = (width_px.max(1), height_px.max(1));
        let mut cpu = CpuBackend::new();
        let surface = cpu
            .create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background,
            })
            .map_err(|e| format!("desktop loop: create cpu surface: {e:?}"))?;
        let mut gpu_backend = GpuBackend::new();
        let gpu_surface = gpu_backend
            .create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background,
            })
            .map_err(|e| format!("desktop loop: create gpu surface: {e:?}"))?;
        Ok(Self {
            cpu,
            surface,
            gpu_backend,
            gpu_surface,
            active: RendererKind::Cpu,
            gpu_api: None,
            gpu_mode: None,
            images: ImageCache::default(),
        })
    }

    /// Injects a raster face for one shaper font id (decision 200):
    /// without it the CPU backend draws advance-cell bars and the
    /// GPU twin refuses loudly at paint; with it, real outlines
    /// on both. Best-effort by contract (callers warn, never die,
    /// when the face is unreadable). Always lands in both backends
    /// so a later `enable_gpu` never paints tofu.
    pub(crate) fn set_font_for(&mut self, id: oppa::text::FontId, bytes: Vec<u8>, index: u32) {
        self.cpu.set_font_for(id, bytes.clone(), index);
        self.gpu_backend.set_font_for(id, bytes, index);
    }

    /// Recreates BOTH paint surfaces at the given viewport size with
    /// the tracked page background and replays the scene commits
    /// onto them (the Android `rebase_surfaces` rule and the
    /// desktop `refit_surface` rule are this one path — a theme
    /// toggle and a resize can never diverge again). The GPU twin
    /// refits even while CPU-only so a later `enable_gpu` never
    /// presents a stale size.
    fn refit_surfaces(
        &mut self,
        host: &ComponentHost,
        width_px: u32,
        height_px: u32,
        bg: oppa::Color,
    ) -> Result<(), String> {
        let (w, h) = (width_px.max(1), height_px.max(1));
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
        self.rebase_commits(host)
    }

    /// Replays ALL commits from zero into both arms (the refit
    /// rebase rule — the diff-cursor policy of this adapter is
    /// "everything, every paint"). Shared by the trait commit and
    /// the refit rebase.
    fn rebase_commits(&mut self, host: &ComponentHost) -> Result<(), String> {
        for d in host.diffs_from(0) {
            self.cpu
                .commit(&d)
                .map_err(|e| format!("desktop loop: cpu commit: {e:?}"))?;
            self.gpu_backend
                .commit(&d)
                .map_err(|e| format!("desktop loop: gpu commit: {e:?}"))?;
        }
        Ok(())
    }
}

impl ScenePresenter for RgbPresenter {
    /// Replays every scene diff into BOTH backends (cheap
    /// bookkeeping): fresh surfaces always present the whole scene.
    fn commit(&mut self, host: &ComponentHost) -> Result<(), String> {
        self.rebase_commits(host)
    }

    fn set_selection(&mut self, _sel: Option<oppa::SelectionPaint>) {
        // The pixel arms stamp overlays through the shared plan
        // (the session publishes to the builder); nothing extra to
        // land on the surfaces.
    }

    fn set_caret(&mut self, _caret: Option<oppa::CaretPaint>) {
        // Same plan-stamped rule as the selection above.
    }

    fn set_theme_mode(&mut self, _mode: oppa::ThemeMode) -> Result<(), String> {
        // The page-background chase happens through the session's
        // refit trigger; the build theme rides the plan builder.
        Ok(())
    }

    fn set_images(&mut self, cache: &ImageCache) {
        self.images = cache.clone();
    }

    fn refit(
        &mut self,
        host: &ComponentHost,
        width_px: u32,
        height_px: u32,
        background: oppa::Color,
    ) -> Result<(), String> {
        self.refit_surfaces(host, width_px, height_px, background)
    }

    /// Paints BOTH arms: pixels encode into the CPU pixmap always
    /// (the GDI / softbuffer source and the GPU fallback) plus into
    /// the custom twin whenever GPU is active. CPU-only repaints
    /// skip custom so a faceless test shaper never trips the custom
    /// loud-no-face rule while the CPU path still draws bars. Static
    /// images deposit into the active arm(s) first. Pixels failures
    /// are loud `Err`s. Returns the plan's damage-rect count.
    fn paint(&mut self, _host: &ComponentHost, plan: &FramePlan) -> Result<usize, String> {
        let images = &self.images;
        for op in &plan.ops {
            if let oppa::DrawOp::RImg { image, .. } = op {
                if !self.cpu.has_image(*image) && images.pixels_of(*image).is_some() {
                    self.cpu.insert_cached(images, *image);
                }
                if self.active.is_gpu()
                    && !self.gpu_backend.has_image(*image)
                    && images.pixels_of(*image).is_some()
                {
                    self.gpu_backend.insert_cached(images, *image);
                }
            }
        }
        self.cpu
            .paint(self.surface, plan)
            .map_err(|e| format!("desktop loop: cpu paint: {e:?}"))?;
        if self.active.is_gpu() {
            self.gpu_backend
                .paint(self.gpu_surface, plan)
                .map_err(|e| format!("desktop loop: gpu paint: {e:?}"))?;
        }
        Ok(plan.damage.len())
    }
}
