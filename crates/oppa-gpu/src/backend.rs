//! GPU backend: universal wgpu render pipelines without compute shaders.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use oppa::{
    BackendError, Caps, DiffOp, DrawOp, FontId, FramePlan, ImageId, NodeId, PaintStats,
    PresenterKind, RendererBackend, SurfaceDesc, SurfaceId, TreeDiff,
};
use wgpu::util::DeviceExt;

use crate::atlas::{FaceBytes, FaceTable, GlyphAtlas, GpuImage};
use crate::batch::{collect_batches, BatchCollector, DrawBatch};
use crate::oracle::RgbaImage;
use crate::pipeline::Pipelines;
use crate::vertex::ViewportUniform;

static GPU_LOCK: Mutex<()> = Mutex::new(());

pub fn gpu_caps() -> Caps {
    Caps {
        max_layers: 64,
        blur_backdrop: false,
        msaa: false,
        text_as_paths: false,
    }
}

pub struct PresentReport {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub cpu_ms: f64,
}

struct Surface {
    desc: SurfaceDesc,
    retained: Vec<DrawOp>,
    last_paint_frame: u64,
    presents: Vec<f64>,
}

pub struct GpuCtx {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub pipelines_rgba: Pipelines,
    pub pipelines_present: Option<Pipelines>,
    pub atlas: GlyphAtlas,
    pub adapter_info: String,
}

pub struct GpuBackend {
    next_surface: u64,
    surfaces: HashMap<SurfaceId, Surface>,
    live_nodes: HashSet<NodeId>,
    paints_total: u64,
    frame: u64,
    faces: FaceTable,
    images: HashMap<ImageId, (u32, u32, Vec<u8>)>,
    gpu_images: HashMap<ImageId, GpuImage>,
    gpu: Option<GpuCtx>,
    collector: BatchCollector,
}

impl GpuBackend {
    pub fn new() -> Self {
        Self {
            next_surface: 1,
            surfaces: HashMap::new(),
            live_nodes: HashSet::new(),
            paints_total: 0,
            frame: 0,
            faces: FaceTable::default(),
            images: HashMap::new(),
            gpu_images: HashMap::new(),
            gpu: None,
            collector: BatchCollector::new(),
        }
    }

    pub fn set_font_bytes(&mut self, bytes: Vec<u8>, index: u32) {
        self.faces.default = Some(FaceBytes::checked(bytes, index, "default"));
    }

    pub fn set_font_for(&mut self, id: FontId, bytes: Vec<u8>, index: u32) {
        self.faces
            .explicit
            .insert(id, FaceBytes::checked(bytes, index, "explicit"));
    }

    pub fn insert_image(&mut self, id: ImageId, width: u32, height: u32, rgba_straight: Vec<u8>) {
        if width == 0 || height == 0 {
            panic!(
                "oppa-gpu: insert_image {id:?}: zero size {width}x{height} — refused, never silent"
            );
        }
        let expect = width as usize * height as usize * 4;
        if rgba_straight.len() != expect {
            panic!(
                "oppa-gpu: insert_image {id:?}: {} bytes != {width}x{height}x4 ({expect}) — refused, never silent",
                rgba_straight.len()
            );
        }
        let mut premul = Vec::with_capacity(expect);
        for px in rgba_straight.chunks_exact(4) {
            let a = px[3];
            premul.push(premultiply_u8(px[0], a));
            premul.push(premultiply_u8(px[1], a));
            premul.push(premultiply_u8(px[2], a));
            premul.push(a);
        }

        if let Some(gpu) = &self.gpu {
            let gpu_img = GpuImage::new(
                &gpu.device,
                &gpu.queue,
                &gpu.pipelines_rgba.texture_bind_group_layout,
                &gpu.pipelines_rgba.image_sampler,
                width,
                height,
                &premul,
            );
            self.gpu_images.insert(id, gpu_img);
        }

        self.images.insert(id, (width, height, premul));
    }

    pub fn remove_image(&mut self, id: ImageId) -> bool {
        self.gpu_images.remove(&id);
        self.images.remove(&id).is_some()
    }

    /// True when the backend holds pre-decoded pixels for `id`.
    pub fn has_image(&self, id: ImageId) -> bool {
        self.images.contains_key(&id)
    }

    pub fn insert_cached(&mut self, cache: &oppa::ImageCache, id: ImageId) {
        let Some((width, height, rgba)) = cache.pixels_of(id) else {
            panic!(
                "oppa-gpu: insert_cached {id:?}: no pre-decoded pixels in the cache — deposit with insert_pixels first"
            );
        };
        self.insert_image(id, width, height, rgba);
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

    pub fn advance_frame(&mut self) {
        self.frame += 1;
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

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

    pub fn adapter(&self) -> Option<&wgpu::Adapter> {
        self.gpu.as_ref().map(|g| &g.adapter)
    }

    pub fn device(&self) -> Option<&wgpu::Device> {
        self.gpu.as_ref().map(|g| &g.device)
    }

    pub fn queue(&self) -> Option<&wgpu::Queue> {
        self.gpu.as_ref().map(|g| &g.queue)
    }

    pub fn ensure_gpu(&mut self) -> Result<String, BackendError> {
        if let Some(gpu) = &self.gpu {
            return Ok(gpu.adapter_info.clone());
        }
        let (device, queue, adapter, info) =
            pollster::block_on(init_headless_gpu()).map_err(BackendError::UnsupportedOp)?;
        self.install_headless(device, queue, adapter, info)
    }

    /// Ensures a GLES device (decision 388: the Android on-device
    /// proof keeps its GL-first row on the GPU rasterizer —
    /// SwiftShader's GLES translator has no compute, so on the
    /// emulator that refusal is expected and the Vulkan row is
    /// attempted next). Same loud contract as [`ensure_gpu`](Self::ensure_gpu).
    pub fn ensure_gpu_gles(&mut self) -> Result<String, BackendError> {
        if let Some(gpu) = &self.gpu {
            return Ok(gpu.adapter_info.clone());
        }
        let (device, queue, adapter, info) = pollster::block_on(init_headless_gpu_on(
            wgpu::Backends::GL,
            wgpu::PowerPreference::LowPower,
        ))
        .map_err(BackendError::UnsupportedOp)?;
        self.install_headless(device, queue, adapter, info)
    }

    /// Ensures a Vulkan device (decision 388: the second Android
    /// on-device row, same loud contract as [`ensure_gpu`](Self::ensure_gpu)).
    pub fn ensure_gpu_vulkan(&mut self) -> Result<String, BackendError> {
        if let Some(gpu) = &self.gpu {
            return Ok(gpu.adapter_info.clone());
        }
        let (device, queue, adapter, info) = pollster::block_on(init_headless_gpu_on(
            wgpu::Backends::VULKAN,
            wgpu::PowerPreference::HighPerformance,
        ))
        .map_err(BackendError::UnsupportedOp)?;
        self.install_headless(device, queue, adapter, info)
    }

    /// Installs a headless device context (pipelines + atlas +
    /// re-uploaded static images), shared by every `ensure_*` arm.
    fn install_headless(
        &mut self,
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter: wgpu::Adapter,
        info: String,
    ) -> Result<String, BackendError> {
        let pipelines_rgba = Pipelines::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let atlas = GlyphAtlas::new(&device, &pipelines_rgba.texture_bind_group_layout);

        let mut gpu_images = HashMap::new();
        for (id, (w, h, premul)) in &self.images {
            let img = GpuImage::new(
                &device,
                &queue,
                &pipelines_rgba.texture_bind_group_layout,
                &pipelines_rgba.image_sampler,
                *w,
                *h,
                premul,
            );
            gpu_images.insert(*id, img);
        }
        self.gpu_images = gpu_images;

        self.gpu = Some(GpuCtx {
            adapter,
            device,
            queue,
            pipelines_rgba,
            pipelines_present: None,
            atlas,
            adapter_info: info.clone(),
        });
        Ok(info)
    }

    pub fn ensure_gpu_for_surface(
        &mut self,
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<String, BackendError> {
        let (device, queue, adapter, info) =
            pollster::block_on(init_gpu_for_surface(instance, surface))
                .map_err(BackendError::UnsupportedOp)?;
        let pipelines_rgba = Pipelines::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let atlas = GlyphAtlas::new(&device, &pipelines_rgba.texture_bind_group_layout);

        let mut gpu_images = HashMap::new();
        for (id, (w, h, premul)) in &self.images {
            let img = GpuImage::new(
                &device,
                &queue,
                &pipelines_rgba.texture_bind_group_layout,
                &pipelines_rgba.image_sampler,
                *w,
                *h,
                premul,
            );
            gpu_images.insert(*id, img);
        }
        self.gpu_images = gpu_images;

        self.gpu = Some(GpuCtx {
            adapter,
            device,
            queue,
            pipelines_rgba,
            pipelines_present: None,
            atlas,
            adapter_info: info.clone(),
        });
        Ok(info)
    }

    pub fn configure_surface(
        &mut self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
        present_mode: wgpu::PresentMode,
    ) -> Result<wgpu::TextureFormat, BackendError> {
        let gpu = self.gpu.as_mut().ok_or_else(|| {
            BackendError::UnsupportedOp(
                "no GPU context: call ensure_gpu_for_surface first".to_string(),
            )
        })?;
        let capabilities = surface.get_capabilities(&gpu.adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
                )
            })
            .ok_or_else(|| {
                BackendError::UnsupportedOp(format!(
                    "surface offers no Rgba8/Bgra8 format (offered: {:?})",
                    capabilities.formats
                ))
            })?;

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

        if format != wgpu::TextureFormat::Rgba8Unorm {
            gpu.pipelines_present = Some(Pipelines::new(&gpu.device, format));
        }

        Ok(format)
    }

    pub fn render_pixels(&mut self, surface: SurfaceId) -> Result<RgbaImage, BackendError> {
        let _guard = GPU_LOCK
            .lock()
            .map_err(|e| BackendError::UnsupportedOp(format!("GPU lock poisoned: {e}")))?;

        let desc = self
            .surfaces
            .get(&surface)
            .ok_or(BackendError::UnknownSurface(surface))?
            .desc;

        if desc.width_px == 0 || desc.height_px == 0 {
            return Err(BackendError::BadSurface("zero-size surface".to_string()));
        }

        self.ensure_gpu()?;
        let gpu = self.gpu.as_mut().expect("ensured above");
        let s = self.surfaces.get(&surface).expect("checked above");

        // Collect batches
        collect_batches(
            &s.retained,
            desc.width_px,
            desc.height_px,
            &self.faces,
            &mut gpu.atlas,
            &gpu.queue,
            &self.gpu_images,
            &mut self.collector,
        )
        .map_err(BackendError::UnsupportedOp)?;

        // Offscreen texture to render into
        let (w, h) = (desc.width_px, desc.height_px);
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oppa_gpu_readback_tex"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        execute_render(
            &gpu.device,
            &gpu.queue,
            &gpu.pipelines_rgba,
            &target_view,
            &gpu.atlas,
            &self.gpu_images,
            &self.collector,
            w,
            h,
            desc.background,
        );

        readback_texture(&gpu.device, &gpu.queue, &texture, w, h)
    }

    /// Renders one frame without readback and returns its wall ms
    /// (decision 388: the frame-loop rig's bare leg, ported from the
    /// retired Vello `render_noread` — same GPU work + queue stall,
    /// no copy/map; the first call includes pipeline compilation,
    /// steady calls do not).
    pub fn render_noread(&mut self, surface: SurfaceId) -> Result<f64, BackendError> {
        let t = web_time::Instant::now();
        let _guard = GPU_LOCK
            .lock()
            .map_err(|e| BackendError::UnsupportedOp(format!("GPU lock poisoned: {e}")))?;

        let desc = self
            .surfaces
            .get(&surface)
            .ok_or(BackendError::UnknownSurface(surface))?
            .desc;

        if desc.width_px == 0 || desc.height_px == 0 {
            return Err(BackendError::BadSurface("zero-size surface".to_string()));
        }

        self.ensure_gpu()?;
        let gpu = self.gpu.as_mut().expect("ensured above");
        let s = self.surfaces.get(&surface).expect("checked above");

        // Collect batches
        collect_batches(
            &s.retained,
            desc.width_px,
            desc.height_px,
            &self.faces,
            &mut gpu.atlas,
            &gpu.queue,
            &self.gpu_images,
            &mut self.collector,
        )
        .map_err(BackendError::UnsupportedOp)?;

        // Transient target (never copied back).
        let (w, h) = (desc.width_px, desc.height_px);
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oppa_gpu_noread_tex"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let target_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        execute_render(
            &gpu.device,
            &gpu.queue,
            &gpu.pipelines_rgba,
            &target_view,
            &gpu.atlas,
            &self.gpu_images,
            &self.collector,
            w,
            h,
            desc.background,
        );

        gpu.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| BackendError::UnsupportedOp(format!("GPU stall poll failed: {e:?}")))?;
        Ok(t.elapsed().as_secs_f64() * 1000.0)
    }

    pub fn present_surface(
        &mut self,
        surface_id: SurfaceId,
        target: &wgpu::Surface<'_>,
    ) -> Result<PresentReport, BackendError> {
        let t_start = web_time::Instant::now();
        let _guard = GPU_LOCK
            .lock()
            .map_err(|e| BackendError::UnsupportedOp(format!("GPU lock poisoned: {e}")))?;

        let desc = self
            .surfaces
            .get(&surface_id)
            .ok_or(BackendError::UnknownSurface(surface_id))?
            .desc;

        if desc.width_px == 0 || desc.height_px == 0 {
            return Err(BackendError::BadSurface("zero-size surface".to_string()));
        }

        let gpu = self.gpu.as_mut().ok_or_else(|| {
            BackendError::UnsupportedOp(
                "no GPU context: call ensure_gpu_for_surface first".to_string(),
            )
        })?;

        let frame = match target.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => {
                return Err(BackendError::UnsupportedOp(format!(
                    "surface acquire not drawable: {other:?}"
                )));
            }
        };

        let dst_format = frame.texture.format();
        let dst_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let pipelines = if dst_format == wgpu::TextureFormat::Rgba8Unorm {
            &gpu.pipelines_rgba
        } else {
            gpu.pipelines_present
                .as_ref()
                .unwrap_or(&gpu.pipelines_rgba)
        };

        let s = self.surfaces.get(&surface_id).expect("checked above");
        collect_batches(
            &s.retained,
            desc.width_px,
            desc.height_px,
            &self.faces,
            &mut gpu.atlas,
            &gpu.queue,
            &self.gpu_images,
            &mut self.collector,
        )
        .map_err(BackendError::UnsupportedOp)?;

        execute_render(
            &gpu.device,
            &gpu.queue,
            pipelines,
            &dst_view,
            &gpu.atlas,
            &self.gpu_images,
            &self.collector,
            desc.width_px,
            desc.height_px,
            desc.background,
        );

        frame.present();

        gpu.device.poll(wgpu::PollType::Poll).map_err(|e| {
            BackendError::UnsupportedOp(format!("GPU maintenance poll failed: {e:?}"))
        })?;

        Ok(PresentReport {
            width: desc.width_px,
            height: desc.height_px,
            format: format!("{dst_format:?}"),
            cpu_ms: t_start.elapsed().as_secs_f64() * 1000.0,
        })
    }
}

impl Default for GpuBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RendererBackend for GpuBackend {
    fn kind(&self) -> PresenterKind {
        PresenterKind::GpuDrawList
    }

    fn caps(&self) -> Caps {
        gpu_caps()
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
        self.surfaces.insert(
            id,
            Surface {
                desc,
                retained: Vec::new(),
                last_paint_frame: self.frame,
                presents: Vec::new(),
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

        // Validate ops before touching pixels
        for op in &plan.ops {
            if let DrawOp::RImg { node, image, .. } = op {
                if !self.images.contains_key(image) {
                    return Err(BackendError::UnsupportedOp(format!(
                        "RImg on {node:?} (image {image:?}): pixels not registered — \
                         decode via oppa-image and insert_image first"
                    )));
                }
            }
            if let DrawOp::Path { node, data, .. } = op {
                if let Err(why) = oppa_cpu::path::build_path(data) {
                    return Err(BackendError::UnsupportedOp(format!(
                        "Path on {node:?}: invalid SVG path data ({why}) — refused, never mis-painted"
                    )));
                }
            }
        }

        let s = self.surfaces.get_mut(&surface).expect("checked above");
        s.last_paint_frame = self.frame;
        if plan.full_repaint {
            s.retained = plan.ops.clone();
        } else {
            oppa_cpu::splice_retained(&mut s.retained, &plan.ops);
        }

        let executed = s.retained.len();
        Ok(PaintStats {
            ops_executed: executed,
            paints: self.paints_total,
            skipped_empty: false,
        })
    }
}

fn premultiply_u8(c: u8, a: u8) -> u8 {
    let prod = u32::from(c) * u32::from(a) + 128;
    ((prod + (prod >> 8)) >> 8) as u8
}

fn wgpu_color(c: oppa::Color) -> wgpu::Color {
    wgpu::Color {
        r: ((c.0 >> 16) & 0xFF) as f64 / 255.0,
        g: ((c.0 >> 8) & 0xFF) as f64 / 255.0,
        b: (c.0 & 0xFF) as f64 / 255.0,
        a: 1.0,
    }
}

async fn init_headless_gpu() -> Result<(wgpu::Device, wgpu::Queue, wgpu::Adapter, String), String> {
    init_headless_gpu_on(
        wgpu::Backends::all(),
        wgpu::PowerPreference::HighPerformance,
    )
    .await
}

async fn init_headless_gpu_on(
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
        .map_err(|e| format!("no GPU adapter: {e}"))?;
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

#[allow(clippy::too_many_arguments)]
fn execute_render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipelines: &Pipelines,
    target_view: &wgpu::TextureView,
    atlas: &GlyphAtlas,
    gpu_images: &HashMap<ImageId, GpuImage>,
    collector: &BatchCollector,
    w: u32,
    h: u32,
    background: oppa::Color,
) {
    let viewport_uniform = ViewportUniform {
        viewport_size: [w as f32, h as f32],
        _padding: [0.0, 0.0],
    };
    let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("oppa_viewport_uniform"),
        contents: bytemuck::bytes_of(&viewport_uniform),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("oppa_viewport_bg"),
        layout: &pipelines.uniform_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buf.as_entire_binding(),
        }],
    });

    let quad_buf = if !collector.quads.is_empty() {
        Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("oppa_quad_instance_buf"),
                contents: bytemuck::cast_slice(&collector.quads),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        )
    } else {
        None
    };

    let glyph_buf = if !collector.glyphs.is_empty() {
        Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("oppa_glyph_instance_buf"),
                contents: bytemuck::cast_slice(&collector.glyphs),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        )
    } else {
        None
    };

    let image_buf = if !collector.images.is_empty() {
        Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("oppa_image_instance_buf"),
                contents: bytemuck::cast_slice(&collector.images),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        )
    } else {
        None
    };

    let path_vbo = if !collector.path_vertices.is_empty() {
        Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("oppa_path_vbo"),
                contents: bytemuck::cast_slice(&collector.path_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        )
    } else {
        None
    };

    let path_ibo = if !collector.path_indices.is_empty() {
        Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("oppa_path_ibo"),
                contents: bytemuck::cast_slice(&collector.path_indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
        )
    } else {
        None
    };

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("oppa_render_encoder"),
    });

    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("oppa_main_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu_color(background)),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        for batch in &collector.batches {
            let sc = batch.scissor();
            if sc.2 == 0 || sc.3 == 0 {
                continue;
            }
            rpass.set_scissor_rect(sc.0, sc.1, sc.2, sc.3);

            match batch {
                DrawBatch::Quad { instances, .. } => {
                    if let Some(q_buf) = &quad_buf {
                        rpass.set_pipeline(&pipelines.quad_pipeline);
                        rpass.set_bind_group(0, &uniform_bind_group, &[]);
                        rpass.set_vertex_buffer(0, pipelines.unit_quad_vbo.slice(..));
                        rpass.set_vertex_buffer(1, q_buf.slice(..));
                        rpass.set_index_buffer(
                            pipelines.unit_quad_ibo.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        rpass.draw_indexed(0..6, 0, instances.clone());
                    }
                }
                DrawBatch::Glyph { instances, .. } => {
                    if let Some(g_buf) = &glyph_buf {
                        rpass.set_pipeline(&pipelines.glyph_pipeline);
                        rpass.set_bind_group(0, &uniform_bind_group, &[]);
                        rpass.set_bind_group(1, &atlas.bind_group, &[]);
                        rpass.set_vertex_buffer(0, pipelines.unit_quad_vbo.slice(..));
                        rpass.set_vertex_buffer(1, g_buf.slice(..));
                        rpass.set_index_buffer(
                            pipelines.unit_quad_ibo.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        rpass.draw_indexed(0..6, 0, instances.clone());
                    }
                }
                DrawBatch::Image {
                    instances,
                    image_id,
                    ..
                } => {
                    if let (Some(i_buf), Some(gpu_img)) = (&image_buf, gpu_images.get(image_id)) {
                        rpass.set_pipeline(&pipelines.image_pipeline);
                        rpass.set_bind_group(0, &uniform_bind_group, &[]);
                        rpass.set_bind_group(1, &gpu_img.bind_group, &[]);
                        rpass.set_vertex_buffer(0, pipelines.unit_quad_vbo.slice(..));
                        rpass.set_vertex_buffer(1, i_buf.slice(..));
                        rpass.set_index_buffer(
                            pipelines.unit_quad_ibo.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        rpass.draw_indexed(0..6, 0, instances.clone());
                    }
                }
                DrawBatch::Path { indices, .. } => {
                    if let (Some(v_buf), Some(i_buf)) = (&path_vbo, &path_ibo) {
                        rpass.set_pipeline(&pipelines.path_pipeline);
                        rpass.set_bind_group(0, &uniform_bind_group, &[]);
                        rpass.set_vertex_buffer(0, v_buf.slice(..));
                        rpass.set_index_buffer(i_buf.slice(..), wgpu::IndexFormat::Uint32);
                        rpass.draw_indexed(indices.clone(), 0, 0..1);
                    }
                }
            }
        }
    }

    queue.submit([encoder.finish()]);
}

fn readback_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    w: u32,
    h: u32,
) -> Result<RgbaImage, BackendError> {
    let bytes_per_row_unpadded = w * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let bytes_per_row = bytes_per_row_unpadded.div_ceil(align) * align;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("oppa_readback_buf"),
        size: (bytes_per_row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
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
        wgpu::Extent3d {
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
