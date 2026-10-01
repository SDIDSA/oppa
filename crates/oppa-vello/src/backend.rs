//! Vello backend: per-surface scenes + retained-op replay + present ledger.
//!
//! Raster discipline mirrors the CPU backend (retained-op replay, empty-plan
//! skip) so the cross-backend oracle compares two independent
//! implementations of one discipline. Present cadence is caller-owned (the
//! compositor is us): [`VelloBackend::present_at`](VelloBackend) ledgers
//! per-surface paint frames and present timestamps off the injected clock;
//! GPU submission happens only on presents of non-empty staged scenes.

use std::collections::{HashMap, HashSet};

use oppa::{
    BackendError, Caps, DrawOp, FramePlan, ImageId, NodeId, PaintStats, PresenterKind,
    RendererBackend, SurfaceDesc, SurfaceId, TreeDiff,
};

use crate::atlas::GlyphAtlas;
use crate::encoder::{encode_plan, EncodeStats};
use crate::oracle::RgbaImage;

/// The Vello capability declaration (see crate docs for the per-field
/// rationale — blur degrades contractually, AA is always on, text is real
/// outlines).
pub fn vello_caps() -> Caps {
    Caps {
        max_layers: 64,
        blur_backdrop: false,
        msaa: true,
        text_as_paths: true,
    }
}

struct Surface {
    desc: SurfaceDesc,
    scene: vello::Scene,
    /// Retained display list (same splice discipline as the CPU backend:
    /// incremental paints replace the dirty nodes' ops; full repaints
    /// replace wholesale; every paint re-encodes the whole list).
    retained: Vec<DrawOp>,
    last_stats: EncodeStats,
    /// Frame index of the latest paint on this surface (skew ledger).
    last_paint_frame: u64,
    /// Timestamps presented (vsync-cadence ledger, injected-clock secs).
    presents: Vec<f64>,
    /// Staged GPU work units since the last [`VelloBackend::take_gpu_work`].
    staged_work: u64,
}

pub struct VelloBackend {
    next_surface: u64,
    surfaces: HashMap<SurfaceId, Surface>,
    live_nodes: HashSet<NodeId>,
    paints_total: u64,
    /// Monotonic frame counter, advanced by the caller per frame (the
    /// test loop advances it once per vsync tick — the compositor is us).
    frame: u64,
    atlas: GlyphAtlas,
    gpu: Option<GpuCtx>,
    /// Decoded image registry (OQ-G8-1 closed here — mirrors the CPU
    /// backend's `images` table, but peniko takes straight alpha so
    /// no premultiply happens: `oppa-image` output deposits
    /// verbatim). Missing ids refuse loudly at encode — pending
    /// images never stage placeholders.
    images: HashMap<ImageId, vello::peniko::ImageData>,
}

/// Live GPU context (created on demand; headless encoding needs none).
struct GpuCtx {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: vello::Renderer,
    adapter_info: String,
    /// Cached blit pipeline per swapchain format (creating a
    /// `TextureBlitter` per present costs milliseconds on weak
    /// drivers — profiled 3.6–5.2ms on Adreno 650 vs 0.2–0.5ms
    /// on RTX; the format is fixed per configure, so one slot).
    blitter: Option<(wgpu::TextureFormat, wgpu::util::TextureBlitter)>,
    /// Reused frame targets (the WebGPU-memory-leak fix): the
    /// present path allocated a fresh intermediate texture every
    /// frame and the web loop never ran device maintenance, so
    /// VRAM grew ~330MB/s at 165fps until the browser purged
    /// (8GB sawtooth). Both textures are recreated only when the
    /// scene size changes. `src` feeds Vello (storage) and the
    /// blit (texture); `dst` is the bench scratch (render
    /// attachment — the swapchain frame serves that role on the
    /// present path).
    frame_tex: Option<(u32, u32, wgpu::Texture, wgpu::Texture)>,
}

/// Report for one swapchain present (the app records it into
/// `meta.txt` next to the oracle bytes).
#[derive(Clone, Debug)]
pub struct PresentReport {
    pub width: u32,
    pub height: u32,
    pub format: String,
    /// CPU wall for acquire + render + blit + submit + the present
    /// call (the GPU work itself is pipelined - this is the
    /// frame's CPU cost, not its GPU cost).
    pub cpu_ms: f64,
    /// Stage walls (sum ≈ `cpu_ms`; measured with the portable
    /// clock so the breakdown works on every target, wasm
    /// included). Added for per-platform bottleneck ID — the
    /// per-frame `TextureBlitter::new` + intermediate texture
    /// below are the prime suspects on weak CPUs.
    pub acquire_ms: f64,
    pub render_ms: f64,
    pub blit_setup_ms: f64,
    pub blit_submit_ms: f64,
    pub present_ms: f64,
    /// Target prep: surface view + per-frame intermediate texture
    /// alloc (also a per-frame cost suspect, like the blitter).
    pub target_ms: f64,
}

/// Serializes GPU readbacks across parallel test threads (concurrent
/// Vulkan/DX12 device use hangs the driver — the M6 oracle fix; the
/// encode path stays lock-free).
static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl VelloBackend {
    pub fn new() -> Self {
        Self {
            next_surface: 1,
            surfaces: HashMap::new(),
            live_nodes: HashSet::new(),
            paints_total: 0,
            frame: 0,
            atlas: GlyphAtlas::new(),
            gpu: None,
            images: HashMap::new(),
        }
    }

    /// Injects the surface's default font face (M6 single-face path;
    /// per-run faces refine it via [`set_font_for`](Self::set_font_for)).
    pub fn set_font_bytes(&mut self, bytes: Vec<u8>, index: u32) {
        self.atlas.set_font_bytes(bytes, index);
    }

    /// Injects the face for one shaper-reported font id (M7, decision
    /// 110 — ends the single-face bound; fallback runs encode against
    /// their own face).
    pub fn set_font_for(&mut self, id: oppa::FontId, bytes: Vec<u8>, index: u32) {
        self.atlas.set_font_for(id, bytes, index);
    }

    /// Deposits decoded pixels for `id` (OQ-G8-1): straight-alpha
    /// `RGBA8` (the `oppa-image` output shape) is validated
    /// (`rgba.len() == w*h*4`, non-zero, loudly) and stored verbatim
    /// — peniko's `ImageAlphaType::Alpha` IS straight alpha, so unlike
    /// the CPU backend nothing converts. Replaces any previous
    /// deposit. Bytes are validated here (loud), never at encode —
    /// the `set_font_bytes` rule applied to images.
    pub fn insert_image(&mut self, id: ImageId, width: u32, height: u32, rgba_straight: Vec<u8>) {
        if width == 0 || height == 0 {
            panic!("oppa-vello: insert_image {id:?}: zero size {width}x{height} — refused, never silent");
        }
        let expect = width as usize * height as usize * 4;
        if rgba_straight.len() != expect {
            panic!(
                "oppa-vello: insert_image {id:?}: {} bytes != {width}x{height}x4 ({expect}) — refused, never silent",
                rgba_straight.len()
            );
        }
        self.images.insert(
            id,
            vello::peniko::ImageData {
                data: rgba_straight.into(),
                format: vello::peniko::ImageFormat::Rgba8,
                alpha_type: vello::peniko::ImageAlphaType::Alpha,
                width,
                height,
            },
        );
    }

    /// Drops a deposited image (returns false when absent — removal is
    /// idempotent teardown, not a refusal).
    pub fn remove_image(&mut self, id: ImageId) -> bool {
        self.images.remove(&id).is_some()
    }

    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    pub fn paints_total(&self) -> u64 {
        self.paints_total
    }

    pub fn live_node_count(&self) -> usize {
        self.live_nodes.len()
    }

    pub fn retained_op_count(&self, surface: SurfaceId) -> Option<usize> {
        self.surfaces.get(&surface).map(|s| s.retained.len())
    }

    pub fn last_encode_stats(&self, surface: SurfaceId) -> Option<EncodeStats> {
        self.surfaces.get(&surface).map(|s| s.last_stats)
    }

    /// Staged work units accumulated since the last call (the
    /// unchanged-surface skip test asserts this is 0 on static frames).
    pub fn take_gpu_work(&mut self) -> u64 {
        let mut total = 0;
        for s in self.surfaces.values_mut() {
            total += s.staged_work;
            s.staged_work = 0;
        }
        total
    }

    /// Advances the compositor frame counter (one vsync tick).
    pub fn advance_frame(&mut self) {
        self.frame += 1;
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Records a present of `surface` at injected-clock `now_secs`.
    /// Only meaningful after a paint (no scene → no-op counted
    /// separately from staged work).
    pub fn present_at(&mut self, surface: SurfaceId, now_secs: f64) -> Result<(), BackendError> {
        let s = self
            .surfaces
            .get_mut(&surface)
            .ok_or(BackendError::UnknownSurface(surface))?;
        s.presents.push(now_secs);
        Ok(())
    }

    pub fn presents(&self, surface: SurfaceId) -> Option<&[f64]> {
        self.surfaces.get(&surface).map(|s| s.presents.as_slice())
    }

    /// Per-surface skew in frames: how many compositor frames apart the
    /// two surfaces' latest paints are (locked #21: ≤1 observable and
    /// bounded). Unknown surfaces are a loud error, never a zero.
    pub fn skew_frames(&self, a: SurfaceId, b: SurfaceId) -> Result<u64, BackendError> {
        let fa = self
            .surfaces
            .get(&a)
            .ok_or(BackendError::UnknownSurface(a))?
            .last_paint_frame;
        let fb = self
            .surfaces
            .get(&b)
            .ok_or(BackendError::UnknownSurface(b))?
            .last_paint_frame;
        Ok(fa.abs_diff(fb))
    }

    /// Probes adapter availability without creating a device (the driver
    /// matrix's hardware row: primary first, software fallback on
    /// request — recorded, never assumed).
    pub fn probe_adapter(force_fallback: bool) -> Result<String, String> {
        pollster::block_on(probe_adapter_async(force_fallback))
    }

    /// Probes the GLES 3.1-class row specifically (M10): a GL-only
    /// instance, so the returned adapter — when one exists — is the
    /// path Vello would take on weak mobile GPUs, not a Vulkan/DX12
    /// adapter wearing a GLES label. `Ok` carries the same info
    /// string as [`probe_adapter`](Self::probe_adapter); `Err`
    /// carries the reason (no GL backend compiled in, no GL driver
    /// on the box, no device). Never panics — the M10 row test
    /// records whichever outcome, and an `Err` keeps the GLES-GPU
    /// pixel row open rather than failing the suite on hardware the
    /// CI box does not have.
    pub fn probe_gles_adapter() -> Result<String, String> {
        pollster::block_on(probe_gles_adapter_async())
    }

    /// Ensures a live GPU context, requesting a real adapter (loud when
    /// none exists — the pixel oracle requires hardware, never silently
    /// degrades to a second software rasterizer).
    pub fn ensure_gpu(&mut self) -> Result<String, BackendError> {
        if let Some(gpu) = &self.gpu {
            return Ok(gpu.adapter_info.clone());
        }
        let info = pollster::block_on(init_gpu()).map_err(BackendError::UnsupportedOp)?;
        let (device, queue, adapter, adapter_info) = info;
        let renderer = vello::Renderer::new(&device, vello::RendererOptions::default())
            .map_err(|e| BackendError::UnsupportedOp(format!("vello Renderer::new: {e}")))?;
        self.gpu = Some(GpuCtx {
            adapter,
            device,
            queue,
            renderer,
            adapter_info: adapter_info.clone(),
            blitter: None,
            frame_tex: None,
        });
        Ok(adapter_info)
    }

    /// Ensures a live GPU context on the GLES row (M10): GL-only
    /// instance with a LowPower preference — the device Vello would
    /// get on weak mobile hardware. Same loud contract as
    /// [`ensure_gpu`](Self::ensure_gpu): `Err` when the row cannot
    /// serve (no GL driver, device request refused, or Vello
    /// rejects the GL device). A second call reuses the context
    /// regardless of which `ensure_*` created it (one live context
    /// per backend — the row and the desktop path never mix).
    ///
    /// Shader init is serial on this row (round 6.1, release
    /// verification): GL backends share one context, and wgpu-hal's
    /// WGL/EGL guard panics past a 1s lock wait — Vello's default
    /// multi-threaded init piles every worker onto that single
    /// lock and trips it deterministically in release timing.
    /// `Some(1)` is Vello's own documented remedy for
    /// shared-context platforms (macOS default); pixels are
    /// unaffected (init-time threading only — the oracle asserts
    /// the same standard).
    pub fn ensure_gpu_gles(&mut self) -> Result<String, BackendError> {
        if let Some(gpu) = &self.gpu {
            return Ok(gpu.adapter_info.clone());
        }
        let info = pollster::block_on(init_gpu_on(
            wgpu::Backends::GL,
            wgpu::PowerPreference::LowPower,
        ))
        .map_err(BackendError::UnsupportedOp)?;
        let (device, queue, adapter, adapter_info) = info;
        let options = vello::RendererOptions {
            num_init_threads: std::num::NonZeroUsize::new(1),
            ..vello::RendererOptions::default()
        };
        let renderer = vello::Renderer::new(&device, options)
            .map_err(|e| BackendError::UnsupportedOp(format!("vello Renderer::new on GL: {e}")))?;
        self.gpu = Some(GpuCtx {
            adapter,
            device,
            queue,
            renderer,
            adapter_info: adapter_info.clone(),
            blitter: None,
            frame_tex: None,
        });
        Ok(adapter_info)
    }

    /// Ensures a live GPU context on the Vulkan row: Vulkan-only
    /// instance (M10 gap closure — SwiftShader exposes Vulkan on
    /// the emulator where its GLES translator has no compute).
    /// Same loud contract as [`ensure_gpu`](Self::ensure_gpu).
    pub fn ensure_gpu_vulkan(&mut self) -> Result<String, BackendError> {
        if let Some(gpu) = &self.gpu {
            return Ok(gpu.adapter_info.clone());
        }
        let info = pollster::block_on(init_gpu_on(
            wgpu::Backends::VULKAN,
            wgpu::PowerPreference::HighPerformance,
        ))
        .map_err(BackendError::UnsupportedOp)?;
        let (device, queue, adapter, adapter_info) = info;
        let renderer =
            vello::Renderer::new(&device, vello::RendererOptions::default()).map_err(|e| {
                BackendError::UnsupportedOp(format!("vello Renderer::new on Vulkan: {e}"))
            })?;
        self.gpu = Some(GpuCtx {
            adapter,
            device,
            queue,
            renderer,
            adapter_info: adapter_info.clone(),
            blitter: None,
            frame_tex: None,
        });
        Ok(adapter_info)
    }

    /// Ensures a live GPU context on an adapter compatible with
    /// `surface` (the swapchain path: the caller creates the
    /// `wgpu::Instance` + `wgpu::Surface` from the platform window
    /// and hands both here). The context is always (re)created from
    /// the given instance — never reused across instances, because a
    /// headless adapter from another instance can report a surface
    /// "supported" while its device cannot see it (observed as a
    /// `Surface does not exist` panic in wgpu-core storage). Scenes
    /// are per-surface and survive the swap; only the device-side
    /// context is re-created. Same loud contract as the other
    /// `ensure_*`: `Err` when no compatible adapter exists or the
    /// device/renderer refuses it.
    pub fn ensure_gpu_for_surface(
        &mut self,
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<String, BackendError> {
        // Always bind the surface's own instance (see the method
        // docs — cross-instance reuse panics inside wgpu-core).
        let info = pollster::block_on(init_gpu_for_surface(instance, surface))
            .map_err(BackendError::UnsupportedOp)?;
        let (device, queue, adapter, adapter_info) = info;
        let renderer =
            vello::Renderer::new(&device, vello::RendererOptions::default()).map_err(|e| {
                BackendError::UnsupportedOp(format!("vello Renderer::new for surface: {e}"))
            })?;
        self.gpu = Some(GpuCtx {
            adapter,
            device,
            queue,
            renderer,
            adapter_info: adapter_info.clone(),
            blitter: None,
            frame_tex: None,
        });
        Ok(adapter_info)
    }

    /// Ensures a live GPU context on an adapter compatible with
    /// `surface`, awaiting adapter/device requests instead of
    /// blocking (the Web path: JS-promise adapter/device requests
    /// cannot `block_on` in a sync resume — that hangs the tab).
    /// Same loud contract as the sync twin; additive, old paths
    /// untouched.
    pub async fn ensure_gpu_for_surface_async(
        &mut self,
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<String, BackendError> {
        let info = init_gpu_for_surface(instance, surface)
            .await
            .map_err(BackendError::UnsupportedOp)?;
        let (device, queue, adapter, adapter_info) = info;
        let renderer =
            vello::Renderer::new(&device, vello::RendererOptions::default()).map_err(|e| {
                BackendError::UnsupportedOp(format!("vello Renderer::new for surface: {e}"))
            })?;
        self.gpu = Some(GpuCtx {
            adapter,
            device,
            queue,
            renderer,
            adapter_info: adapter_info.clone(),
            blitter: None,
            frame_tex: None,
        });
        Ok(adapter_info)
    }

    /// Ensures a GPU context like
    /// [`ensure_gpu_for_surface`](Self::ensure_gpu_for_surface), with a
    /// disk-backed pipeline-cache round-trip (decision 199):
    /// `cache_data` — bytes previously returned here (e.g. loaded from
    /// disk) — seeds pipeline creation; the returned bytes
    /// (`PipelineCache::get_data` after the renderer builds) are what
    /// the caller stores. Where the backend offers no persistence the
    /// saved half is `None` and behavior equals the uncached path
    /// (degraded, never failed). Stale data (driver/adapter drift) is
    /// rejected by wgpu validation (`fallback: true`), never an error.
    pub fn ensure_gpu_for_surface_with_cache(
        &mut self,
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
        cache_data: Option<Vec<u8>>,
    ) -> Result<(String, Option<Vec<u8>>), BackendError> {
        use wgpu::Features;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(surface),
            force_fallback_adapter: false,
        }))
        .map_err(|e| {
            BackendError::UnsupportedOp(format!("no surface-compatible GPU adapter: {e}"))
        })?;
        let info = adapter.get_info();
        let adapter_info = format!(
            "{} backend={:?} driver={} device={}",
            info.name, info.backend, info.driver, info.device
        );
        let mut desc = wgpu::DeviceDescriptor::default();
        let cachable = adapter.features().contains(Features::PIPELINE_CACHE);
        if cachable {
            desc.required_features = Features::PIPELINE_CACHE;
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&desc)).map_err(|e| {
            BackendError::UnsupportedOp(format!("surface GPU device request failed: {e}"))
        })?;
        let pipeline_cache = if cachable {
            // SAFETY: `cache_data` originates from our own `get_data`
            // output (written to disk by the caller, read back here);
            // `fallback: true` degrades to uncached creation when the
            // driver rejects it instead of failing.
            Some(unsafe {
                device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                    label: Some("oppa-vello-pipeline-cache"),
                    data: cache_data.as_deref(),
                    fallback: true,
                })
            })
        } else {
            None
        };
        let renderer = vello::Renderer::new(
            &device,
            vello::RendererOptions {
                pipeline_cache: pipeline_cache.clone(),
                ..Default::default()
            },
        )
        .map_err(|e| {
            BackendError::UnsupportedOp(format!("vello Renderer::new for surface: {e}"))
        })?;
        let saved = pipeline_cache.as_ref().and_then(|c| c.get_data());
        self.gpu = Some(GpuCtx {
            adapter,
            device,
            queue,
            renderer,
            adapter_info: adapter_info.clone(),
            blitter: None,
            frame_tex: None,
        });
        Ok((adapter_info, saved))
    }

    /// The live adapter, if a GPU context was ensured (the swapchain
    /// caller queries surface capabilities off it before configuring
    /// the surface). `None` until any `ensure_*` succeeds.
    pub fn gpu_adapter(&self) -> Option<&wgpu::Adapter> {
        self.gpu.as_ref().map(|g| &g.adapter)
    }

    /// Renders the surface's current scene through the real swapchain:
    /// scene → intermediate `Rgba8Unorm` texture (Vello's compute
    /// path cannot bind the surface texture directly) → blit to the
    /// surface's current texture → `present`. This is on-screen
    /// presentation, not the software lock/post blit and not the
    /// offscreen readback. Requires a context from
    /// [`ensure_gpu_for_surface`](Self::ensure_gpu_for_surface);
    /// a foreign context is a loud error, never a silent re-ensure
    /// against the wrong adapter.
    pub fn present_surface(
        &mut self,
        surface_id: SurfaceId,
        target: &wgpu::Surface<'_>,
    ) -> Result<PresentReport, BackendError> {
        let t = web_time::Instant::now();
        let _guard = GPU_LOCK
            .lock()
            .map_err(|e| BackendError::UnsupportedOp(format!("GPU test lock poisoned: {e}")))?;
        let desc = self
            .surfaces
            .get(&surface_id)
            .ok_or(BackendError::UnknownSurface(surface_id))?
            .desc;
        if desc.width_px == 0 || desc.height_px == 0 {
            return Err(BackendError::BadSurface("zero-size surface".to_string()));
        }
        if self.gpu.is_none() {
            return Err(BackendError::UnsupportedOp(
                "no GPU context: call ensure_gpu_for_surface first".to_string(),
            ));
        }
        let frame = match target.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => {
                return Err(BackendError::UnsupportedOp(format!(
                    "surface acquire not drawable: {other:?}"
                )));
            }
        };
        let acquire_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t_target = web_time::Instant::now();
        let dst_format = frame.texture.format();
        let dst_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let (w, h) = (desc.width_px, desc.height_px);
        let gpu = self.gpu.as_mut().expect("checked above");
        let s = self.surfaces.get(&surface_id).expect("checked above");
        let (frame_src, _) = frame_targets(gpu, w, h);
        let src_view = frame_src.create_view(&wgpu::TextureViewDescriptor::default());
        let target_ms = t_target.elapsed().as_secs_f64() * 1000.0;
        let t_render = web_time::Instant::now();
        gpu.renderer
            .render_to_texture(
                &gpu.device,
                &gpu.queue,
                &s.scene,
                &src_view,
                &vello::RenderParams {
                    base_color: vello_color(desc.background),
                    width: w,
                    height: h,
                    antialiasing_method: vello::AaConfig::Area,
                },
            )
            .map_err(|e| {
                BackendError::UnsupportedOp(format!("vello present render failed: {e}"))
            })?;
        let render_ms = t_render.elapsed().as_secs_f64() * 1000.0;
        let (blit_setup_ms, blit_submit_ms) = blit_cached(gpu, &src_view, &dst_view, dst_format);
        let t_present = web_time::Instant::now();
        frame.present();
        let present_ms = t_present.elapsed().as_secs_f64() * 1000.0;
        // Drive deferred destruction (dropped views/buffers): the
        // web loop has no other maintenance tick, and without this
        // per-frame transients accumulate in VRAM (the 8GB
        // sawtooth). Non-blocking; loud on device trouble like the
        // readback path's wait-poll.
        gpu.device.poll(wgpu::PollType::Poll).map_err(|e| {
            BackendError::UnsupportedOp(format!("GPU maintenance poll failed: {e:?}"))
        })?;
        Ok(PresentReport {
            width: w,
            height: h,
            format: format!("{dst_format:?}"),
            cpu_ms: t.elapsed().as_secs_f64() * 1000.0,
            acquire_ms,
            render_ms,
            blit_setup_ms,
            blit_submit_ms,
            present_ms,
            target_ms,
        })
    }

    /// Renders one frame into a scratch target and submits, without
    /// acquiring or presenting (throughput probe for vsync-gated
    /// targets: the browser composites at panel rate no matter how
    /// fast we submit, so uncapped throughput is measured here,
    /// not in the rAF loop). No `poll` — submission is
    /// fire-and-forget, which is also what keeps this
    /// deadlock-free on wasm. Returns CPU wall ms. Targets come
    /// from the shared frame cache (a one-shot bench must not
    /// spike transient VRAM either).
    pub fn render_submit_only(&mut self, surface_id: SurfaceId) -> Result<f64, BackendError> {
        let t = web_time::Instant::now();
        let _guard = GPU_LOCK
            .lock()
            .map_err(|e| BackendError::UnsupportedOp(format!("GPU test lock poisoned: {e}")))?;
        let desc = self
            .surfaces
            .get(&surface_id)
            .ok_or(BackendError::UnknownSurface(surface_id))?
            .desc;
        if desc.width_px == 0 || desc.height_px == 0 {
            return Err(BackendError::BadSurface("zero-size surface".to_string()));
        }
        if self.gpu.is_none() {
            return Err(BackendError::UnsupportedOp(
                "no GPU context: call ensure_gpu_for_surface first".to_string(),
            ));
        }
        let (w, h) = (desc.width_px, desc.height_px);
        let gpu = self.gpu.as_mut().expect("checked above");
        let s = self.surfaces.get(&surface_id).expect("checked above");
        let (frame_src, frame_dst) = frame_targets(gpu, w, h);
        let src_view = frame_src.create_view(&wgpu::TextureViewDescriptor::default());
        let dst_view = frame_dst.create_view(&wgpu::TextureViewDescriptor::default());
        gpu.renderer
            .render_to_texture(
                &gpu.device,
                &gpu.queue,
                &s.scene,
                &src_view,
                &vello::RenderParams {
                    base_color: vello_color(desc.background),
                    width: w,
                    height: h,
                    antialiasing_method: vello::AaConfig::Area,
                },
            )
            .map_err(|e| BackendError::UnsupportedOp(format!("vello bench render failed: {e}")))?;
        let _ = blit_cached(gpu, &src_view, &dst_view, wgpu::TextureFormat::Rgba8Unorm);
        // Same maintenance as the present path (a 1000-iteration
        // bench must not spike transient VRAM either).
        gpu.device.poll(wgpu::PollType::Poll).map_err(|e| {
            BackendError::UnsupportedOp(format!("GPU maintenance poll failed: {e:?}"))
        })?;
        Ok(t.elapsed().as_secs_f64() * 1000.0)
    }

    /// Configures `surface` for presentation of the given scene size:    /// format is the first non-sRGB 8-bit format the surface offers
    /// (`Rgba8Unorm`/`Bgra8Unorm` — the blit's straight-copy pair,
    /// same choice as `vello::util`), `Fifo` present mode, auto
    /// alpha. Requires a context from
    /// [`ensure_gpu_for_surface`](Self::ensure_gpu_for_surface).
    /// Returns the chosen format for the run record.
    pub fn configure_surface(
        &self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
    ) -> Result<wgpu::TextureFormat, BackendError> {
        self.configure_surface_with_present_mode(surface, width, height, wgpu::PresentMode::Fifo)
    }

    /// Like [`configure_surface`](Self::configure_surface) with an
    /// explicit present mode (decision 201): `Immediate` unpaces
    /// presentation (tearing allowed) for raw-throughput measurement.
    /// Format choice, validation, and latency are unchanged — only the
    /// mode differs.
    pub fn configure_surface_with_present_mode(
        &self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
        present_mode: wgpu::PresentMode,
    ) -> Result<wgpu::TextureFormat, BackendError> {
        let gpu = self.gpu.as_ref().ok_or_else(|| {
            BackendError::UnsupportedOp(
                "no GPU context: call ensure_gpu_for_surface first".to_string(),
            )
        })?;
        let capabilities = surface.get_capabilities(&gpu.adapter);
        let format = capabilities.formats.iter().copied().find(|f| {
            matches!(
                f,
                wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
            )
        });
        let format = match format {
            Some(f) => f,
            None => {
                // Loud diagnostics (§9): name exactly what the surface
                // offered — the fix decision (sRGB twins? alpha modes?
                // empty capabilities?) depends on the real list.
                return Err(BackendError::UnsupportedOp(format!(
                    "surface offers no Rgba8/Bgra8 format (offered: {:?}, alpha: {:?})",
                    capabilities.formats, capabilities.alpha_modes,
                )));
            }
        };
        surface.configure(
            &gpu.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width,
                height,
                present_mode,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
            },
        );
        Ok(format)
    }

    /// Renders the surface's current scene to an RGBA8 image via the GPU
    /// (the oracle's Vello arm). Requires hardware (see `ensure_gpu`).
    pub fn render_pixels(&mut self, surface: SurfaceId) -> Result<RgbaImage, BackendError> {
        let _guard = GPU_LOCK
            .lock()
            .map_err(|e| BackendError::UnsupportedOp(format!("GPU test lock poisoned: {e}")))?;
        let desc = self
            .surfaces
            .get(&surface)
            .ok_or(BackendError::UnknownSurface(surface))?
            .desc;
        if desc.width_px == 0 || desc.height_px == 0 {
            return Err(BackendError::BadSurface("zero-size surface".to_string()));
        }
        let info = self.ensure_gpu()?;
        let _ = info;
        let gpu = self.gpu.as_mut().expect("ensured above");
        let s = self.surfaces.get(&surface).expect("checked above");
        readback(
            &gpu.device,
            &gpu.queue,
            &mut gpu.renderer,
            &s.scene,
            &s.desc,
        )
    }

    /// Renders the surface's scene to a throwaway intermediate
    /// texture and waits the queue dry — the same GPU work as
    /// `render_pixels` minus the copy/map/readback. Returns the
    /// CPU wall (encode + submit + stall). The frame-loop harness
    /// subtracts this from the full-readback cost to isolate the
    /// readback overhead; the first call includes pipeline
    /// compilation, steady calls do not (reported separately).
    pub fn render_noread(&mut self, surface: SurfaceId) -> Result<f64, BackendError> {
        let t = web_time::Instant::now();
        let _guard = GPU_LOCK
            .lock()
            .map_err(|e| BackendError::UnsupportedOp(format!("GPU test lock poisoned: {e}")))?;
        let desc = self
            .surfaces
            .get(&surface)
            .ok_or(BackendError::UnknownSurface(surface))?
            .desc;
        if desc.width_px == 0 || desc.height_px == 0 {
            return Err(BackendError::BadSurface("zero-size surface".to_string()));
        }
        let info = self.ensure_gpu()?;
        let _ = info;
        let gpu = self.gpu.as_mut().expect("ensured above");
        let s = self.surfaces.get(&surface).expect("checked above");
        let (w, h) = (desc.width_px, desc.height_px);
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oppa-vello-noread"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        gpu.renderer
            .render_to_texture(
                &gpu.device,
                &gpu.queue,
                &s.scene,
                &view,
                &vello::RenderParams {
                    base_color: vello_color(desc.background),
                    width: w,
                    height: h,
                    antialiasing_method: vello::AaConfig::Area,
                },
            )
            .map_err(|e| BackendError::UnsupportedOp(format!("vello render failed: {e}")))?;
        // `render_to_texture` submits internally; the poll waits the
        // queue dry (same stall as the readback path, minus the
        // copy/map).
        gpu.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| BackendError::UnsupportedOp(format!("GPU poll failed: {e:?}")))?;
        Ok(t.elapsed().as_secs_f64() * 1000.0)
    }
}

impl Default for VelloBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns clones of the reused (src, dst) frame targets,
/// allocating them only when absent or when the scene size
/// changed (see `GpuCtx::frame_tex`). Allocating per frame
/// leaks VRAM on targets without implicit maintenance (web:
/// ~330MB/s at 165fps, 8GB sawtooth). Clones are cheap
/// Arc-backed handles; owning them frees the caller to
/// reborrow `gpu` mutably for render/blit below.
/// 0 = src (Vello storage + blit texture),
/// 1 = bench scratch (render attachment).
fn frame_targets(gpu: &mut GpuCtx, w: u32, h: u32) -> (wgpu::Texture, wgpu::Texture) {
    let fresh = !matches!(&gpu.frame_tex, Some((cw, ch, _, _)) if *cw == w && *ch == h);
    if fresh {
        let extent = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let src = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oppa-vello-frame-src"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let dst = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oppa-vello-frame-dst"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        gpu.frame_tex = Some((w, h, src, dst));
    }
    let (_, _, src, dst) = gpu.frame_tex.as_ref().expect("just ensured");
    (src.clone(), dst.clone())
}

/// Copies `src` into `dst` through the per-format cached blitter
/// (see `GpuCtx::blitter`): one creation per format switch,
/// ~free hits afterwards. Returns (setup_ms, submit_ms).
/// Shared by the swapchain present and the throughput probe.
fn blit_cached(
    gpu: &mut GpuCtx,
    src_view: &wgpu::TextureView,
    dst_view: &wgpu::TextureView,
    dst_format: wgpu::TextureFormat,
) -> (f64, f64) {
    let t_blit = web_time::Instant::now();
    let blitter: &wgpu::util::TextureBlitter = match &mut gpu.blitter {
        Some((fmt, b)) if *fmt == dst_format => b,
        slot => {
            &slot
                .insert((
                    dst_format,
                    wgpu::util::TextureBlitter::new(&gpu.device, dst_format),
                ))
                .1
        }
    };
    let blit_setup_ms = t_blit.elapsed().as_secs_f64() * 1000.0;
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    blitter.copy(&gpu.device, &mut encoder, src_view, dst_view);
    gpu.queue.submit([encoder.finish()]);
    let blit_submit_ms = t_blit.elapsed().as_secs_f64() * 1000.0 - blit_setup_ms;
    (blit_setup_ms, blit_submit_ms)
}

async fn probe_adapter_async(force_fallback: bool) -> Result<String, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: force_fallback,
        })
        .await
        .map_err(|e| {
            format!("no GPU adapter (fallback={force_fallback}) for the Vello pixel oracle: {e}")
        })?;
    let info = adapter.get_info();
    Ok(format!(
        "{} backend={:?} driver={} device={}",
        info.name, info.backend, info.driver, info.device
    ))
}

async fn probe_gles_adapter_async() -> Result<String, String> {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::GL;
    let instance = wgpu::Instance::new(desc);
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .map_err(|e| format!("no GLES adapter for the weakest-hardware row: {e}"))?;
    let info = adapter.get_info();
    Ok(format!(
        "{} backend={:?} driver={} device={}",
        info.name, info.backend, info.driver, info.device
    ))
}

async fn init_gpu() -> Result<(wgpu::Device, wgpu::Queue, wgpu::Adapter, String), String> {
    init_gpu_on(
        wgpu::InstanceDescriptor::new_without_display_handle().backends,
        wgpu::PowerPreference::HighPerformance,
    )
    .await
}

async fn init_gpu_on(
    backends: wgpu::Backends,
    power: wgpu::PowerPreference,
) -> Result<(wgpu::Device, wgpu::Queue, wgpu::Adapter, String), String> {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = backends;
    let instance = wgpu::Instance::new(desc);
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: power,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .map_err(|e| {
            format!("no GPU adapter (backends={backends:?}) for the Vello pixel oracle: {e}")
        })?;
    let info = adapter.get_info();
    let info_str = format!(
        "{} backend={:?} driver={} device={}",
        info.name, info.backend, info.driver, info.device
    );
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .map_err(|e| format!("GPU device request failed: {e}"))?;
    Ok((device, queue, adapter, info_str))
}

/// Requests a surface-compatible adapter from the caller's instance
/// (the swapchain path — the instance owns the surface, so it comes
/// from the platform glue, not from a headless constructor here).
async fn init_gpu_for_surface(
    instance: &wgpu::Instance,
    surface: &wgpu::Surface<'_>,
) -> Result<(wgpu::Device, wgpu::Queue, wgpu::Adapter, String), String> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(surface),
            force_fallback_adapter: false,
        })
        .await
        .map_err(|e| format!("no surface-compatible GPU adapter: {e}"))?;
    let info = adapter.get_info();
    let info_str = format!(
        "{} backend={:?} driver={} device={}",
        info.name, info.backend, info.driver, info.device
    );
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .map_err(|e| format!("surface GPU device request failed: {e}"))?;
    Ok((device, queue, adapter, info_str))
}

/// Renders `scene` headlessly into an Rgba8Unorm texture and reads it back
/// (padded-row copy, unpadded on return).
fn readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut vello::Renderer,
    scene: &vello::Scene,
    desc: &SurfaceDesc,
) -> Result<RgbaImage, BackendError> {
    use wgpu::{BufferDescriptor, BufferUsages, Extent3d, TextureDescriptor, TextureDimension};
    let (w, h) = (desc.width_px, desc.height_px);
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("oppa-vello-readback"),
        size: Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    renderer
        .render_to_texture(
            device,
            queue,
            scene,
            &view,
            &vello::RenderParams {
                base_color: vello_color(desc.background),
                width: w,
                height: h,
                antialiasing_method: vello::AaConfig::Area,
            },
        )
        .map_err(|e| BackendError::UnsupportedOp(format!("vello render failed: {e}")))?;
    let bytes_per_row_unpadded = w * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let bytes_per_row = bytes_per_row_unpadded.div_ceil(align) * align;
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("oppa-vello-readback-buf"),
        size: (bytes_per_row * h) as u64,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(h),
            },
        },
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| BackendError::UnsupportedOp(format!("GPU readback poll failed: {e:?}")))?;
    let data = slice.get_mapped_range().to_vec();
    buffer.unmap();
    // Unpad rows.
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let start = (y * bytes_per_row) as usize;
        pixels.extend_from_slice(&data[start..start + bytes_per_row_unpadded as usize]);
    }
    Ok(RgbaImage {
        width: w,
        height: h,
        pixels,
    })
}

fn vello_color(c: oppa::Color) -> vello::peniko::Color {
    vello::peniko::Color::from_rgba8(
        ((c.0 >> 16) & 0xFF) as u8,
        ((c.0 >> 8) & 0xFF) as u8,
        (c.0 & 0xFF) as u8,
        255,
    )
}

impl RendererBackend for VelloBackend {
    fn kind(&self) -> PresenterKind {
        PresenterKind::GpuDrawList
    }

    fn caps(&self) -> Caps {
        vello_caps()
    }

    fn create_surface(&mut self, desc: SurfaceDesc) -> Result<SurfaceId, BackendError> {
        if desc.width_px == 0 || desc.height_px == 0 {
            return Err(BackendError::BadSurface(format!(
                "zero-size surface {}x{}",
                desc.width_px, desc.height_px
            )));
        }
        let id = SurfaceId(self.next_surface);
        self.next_surface += 1;
        let mut scene = vello::Scene::new();
        // Stage the background immediately so a first-frame readback is
        // never an empty scene (mirrors the CPU pixmap's bg fill).
        let bg = vello::peniko::Brush::Solid(vello_color(desc.background));
        let rc = vello::kurbo::Rect::new(0.0, 0.0, desc.width_px as f64, desc.height_px as f64);
        scene.fill(
            vello::peniko::Fill::NonZero,
            vello::kurbo::Affine::IDENTITY,
            &bg,
            None,
            &rc,
        );
        self.surfaces.insert(
            id,
            Surface {
                desc,
                scene,
                retained: Vec::new(),
                last_stats: EncodeStats::default(),
                last_paint_frame: self.frame,
                presents: Vec::new(),
                staged_work: 0,
            },
        );
        Ok(id)
    }

    fn destroy_surface(&mut self, id: SurfaceId) -> Result<(), BackendError> {
        self.surfaces
            .remove(&id)
            .map(|_| ())
            .ok_or(BackendError::UnknownSurface(id))
    }

    fn commit(&mut self, diff: &TreeDiff) -> Result<(), BackendError> {
        use oppa::DiffOp;
        for op in &diff.ops {
            match op {
                DiffOp::Add { id, .. } => {
                    self.live_nodes.insert(*id);
                }
                DiffOp::Remove { id } => {
                    self.live_nodes.remove(id);
                    for surface in self.surfaces.values_mut() {
                        surface.retained.retain(|o| o.node() != Some(*id));
                    }
                }
                DiffOp::Move { .. } | DiffOp::Update { .. } => {}
            }
        }
        Ok(())
    }

    fn paint(&mut self, surface: SurfaceId, plan: &FramePlan) -> Result<PaintStats, BackendError> {
        // Borrow split: atlas + frame are backend-level, surface is per-id.
        if !self.surfaces.contains_key(&surface) {
            return Err(BackendError::UnknownSurface(surface));
        }
        self.paints_total += 1;
        if plan.is_empty() {
            return Ok(PaintStats {
                ops_executed: 0,
                paints: self.paints_total,
                skipped_empty: true,
            });
        }
        if plan.full_repaint {
            let ops = plan.ops.clone();
            self.surfaces.get_mut(&surface).expect("checked").retained = ops;
        } else {
            // Incremental splice, order-preserving like the CPU backend
            // (M8 `oppa_cpu::splice_retained` — tail repaints must not
            // reorder backgrounds over foregrounds).
            let ops = plan.ops.clone();
            let s = self.surfaces.get_mut(&surface).expect("checked");
            oppa_cpu::splice_retained(&mut s.retained, &ops);
        }
        // Re-encode the whole retained list (replay discipline, like CPU).
        // Disjoint field borrows: the scene lives per-surface, the atlas
        // is backend-level.
        let ops = self
            .surfaces
            .get(&surface)
            .expect("checked")
            .retained
            .clone();
        let desc = self.surfaces.get(&surface).expect("checked").desc;
        let replay = FramePlan {
            viewport_w: plan.viewport_w,
            viewport_h: plan.viewport_h,
            ops,
            damage: Vec::new(),
            stats: Default::default(),
            full_repaint: true,
        };
        let executed = {
            let surfaces = &mut self.surfaces;
            let atlas = &mut self.atlas;
            let images = &self.images;
            let s = surfaces.get_mut(&surface).expect("checked");
            s.scene.reset();
            let stats = encode_plan(&mut s.scene, &replay, &desc, atlas, images)?;
            let executed = stats.shapes_encoded + stats.glyphs;
            s.last_stats = stats;
            s.staged_work += stats.work_units();
            s.last_paint_frame = self.frame;
            executed
        };
        Ok(PaintStats {
            ops_executed: executed,
            paints: self.paints_total,
            skipped_empty: false,
        })
    }
}
