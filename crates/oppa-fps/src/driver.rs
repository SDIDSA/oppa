//! Native winit event-loop driver (Windows, Linux, Android).
//! The window is created hidden in `resumed`, render state (GPU
//! with loud CPU fallback) builds against it, a warmup frame
//! presents, then the window shows. `Resized` refits swapchain +
//! scene to the live client size (the uncapped-blank class:
//! reconfiguring to a stale size never displays);
//! `RedrawRequested` steps the FPS core and presents;
//! `about_to_wait` re-arms redraw for the uncapped loop. Suspend
//! drops window + render state (Android); resume rebuilds them
//! around the surviving scene core.
//!
//! Web runs its own rAF driver (`web.rs`): winit's web backend
//! trips a create-time borrow panic here, and WebGPU bring-up is
//! async anyway. Shared scene + CPU present helpers live in
//! `app.rs`.

use std::cell::{Cell, RefCell};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

use oppa::{Color, RendererBackend, SurfaceDesc, SurfaceId};
use oppa_cpu::CpuBackend;
use oppa_vello::VelloBackend;

use crate::app::{
    atlas_pair, build_cpu_scene, layout_service, present_cpu_frame, FpsCore, DEFAULT_H, DEFAULT_W,
};
use crate::clock::now_secs;

/// Platform log: stderr on native (this driver is native-only;
/// the Web driver logs through `web::console_log`).
pub fn log_line(s: &str) {
    eprintln!("{s}");
}

#[cfg(not(target_arch = "wasm32"))]
fn cache_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA")
            .ok()
            .map(|b| std::path::PathBuf::from(b).join("oppa-fps"))
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var("HOME")
            .ok()
            .map(|b| std::path::PathBuf::from(b).join(".cache/oppa-fps"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

/// Ordered GPU backend attempts per target (name, wgpu backends).
/// Desktop probes Vulkan first (measured: Vello bring-up ~2s vs
/// ~6-16s Dx12, and only Vulkan persists pipeline-cache data in
/// wgpu-hal 29); single-backend targets try once.
fn backend_attempts() -> Vec<(&'static str, wgpu::Backends)> {
    #[cfg(target_os = "windows")]
    {
        vec![
            ("vulkan", wgpu::Backends::VULKAN),
            ("dx12", wgpu::Backends::DX12),
        ]
    }
    #[cfg(target_os = "linux")]
    {
        vec![("vulkan", wgpu::Backends::VULKAN)]
    }
    #[cfg(target_os = "android")]
    {
        vec![("vulkan", wgpu::Backends::VULKAN)]
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "android")))]
    {
        vec![("vulkan", wgpu::Backends::VULKAN)]
    }
}

struct GpuState {
    surface: wgpu::Surface<'static>,
    backend: Rc<RefCell<VelloBackend>>,
    surface_id: Rc<Cell<SurfaceId>>,
    size: (u32, u32),
    mode: wgpu::PresentMode,
    name: String,
    outdated_streak: u32,
    outdated_total: u64,
    last_storm_warn: f64,
    last_stage_log: f64,
}

struct CpuState {
    backend: Rc<RefCell<CpuBackend>>,
    surface_id: SurfaceId,
    _context: softbuffer::Context<Arc<Window>>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    size: (u32, u32),
    last_stage_log: f64,
}

enum RenderState {
    #[cfg(not(target_arch = "wasm32"))]
    Gpu(GpuState),
    Cpu(CpuState),
}

/// The cross-platform FPS application: scene core + render state
/// under a winit event loop. Built by the entries with two text
/// services over the same bundled bytes (layout + app-side).
pub struct FpsDriver {
    core: FpsCore,
    window: Option<Arc<Window>>,
    render: Option<RenderState>,
    last: f64,
}

impl FpsDriver {
    pub fn new(frozen: Option<String>) -> Self {
        let core = FpsCore::new(
            layout_service(),
            layout_service(),
            (DEFAULT_W, DEFAULT_H),
            frozen,
        );
        Self {
            core,
            window: None,
            render: None,
            last: now_secs(),
        }
    }

    /// Picks the present mode: uncapped Immediate first (the
    /// throughput leg), then Mailbox, then vsync Fifo. Loud either
    /// way, with the offered list for blank-window diagnosis.
    #[cfg(not(target_arch = "wasm32"))]
    fn pick_mode(
        surface: &wgpu::Surface<'_>,
        backend: &RefCell<VelloBackend>,
    ) -> wgpu::PresentMode {
        let offered = backend
            .borrow()
            .gpu_adapter()
            .map(|a| surface.get_capabilities(a).present_modes)
            .unwrap_or_default();
        log_line(&format!("oppa-fps: offered present modes: {offered:?}"));
        for mode in [
            wgpu::PresentMode::Immediate,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::Fifo,
        ] {
            if offered.contains(&mode) {
                log_line(&format!("oppa-fps: present mode {mode:?}"));
                return mode;
            }
        }
        log_line("oppa-fps: no preferred mode offered, Fifo");
        wgpu::PresentMode::Fifo
    }

    /// Builds GPU render state against `window`, or returns the
    /// loud reason to fall back to CPU.
    #[cfg(not(target_arch = "wasm32"))]
    fn build_gpu(
        window: &Arc<Window>,
        display: raw_window_handle::RawDisplayHandle,
        core: &FpsCore,
        size: (u32, u32),
    ) -> Result<GpuState, String> {
        let raw_window = window
            .window_handle()
            .map_err(|e| format!("window handle: {e:?}"))?
            .as_raw();
        let backend = Rc::new(RefCell::new(VelloBackend::new()));
        for (name, backends) in backend_attempts() {
            // One leaked instance per attempt (the surface borrows
            // it; the winner lives for the process).
            let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
            desc.backends = backends;
            let instance: &'static wgpu::Instance = Box::leak(Box::new(wgpu::Instance::new(desc)));
            let target = wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(display),
                raw_window_handle: raw_window,
            };
            let surface = match unsafe { instance.create_surface_unsafe(target) } {
                Ok(s) => s,
                Err(e) => {
                    log_line(&format!("oppa-fps: {name} surface failed: {e}"));
                    continue;
                }
            };
            let cache_path =
                cache_dir().map(|d| d.join(format!("vello-pipeline-cache-{name}.bin")));
            let cache_data = cache_path.as_ref().and_then(|p| std::fs::read(p).ok());
            log_line(&format!(
                "oppa-fps: {name} pipeline cache {}",
                cache_data
                    .as_ref()
                    .map(|d| format!("{} bytes loaded (warm)", d.len()))
                    .unwrap_or_else(|| "absent (cold)".to_string())
            ));
            let ensured = backend
                .borrow_mut()
                .ensure_gpu_for_surface_with_cache(instance, &surface, cache_data);
            match ensured {
                Ok((adapter, saved)) => {
                    log_line(&format!("oppa-fps: {name} {adapter}"));
                    if let (Some(path), Some(data)) = (cache_path, saved) {
                        if let Some(parent) = path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        match std::fs::write(&path, &data) {
                            Ok(()) => log_line(&format!(
                                "oppa-fps: pipeline cache {} bytes saved",
                                data.len()
                            )),
                            Err(e) => {
                                log_line(&format!("oppa-fps: pipeline cache save failed: {e}"))
                            }
                        }
                    }
                    let mode = Self::pick_mode(&surface, &backend);
                    backend
                        .borrow()
                        .configure_surface_with_present_mode(&surface, size.0, size.1, mode)
                        .map_err(|e| format!("configure surface: {e}"))?;
                    let surface_id = Rc::new(Cell::new(
                        backend
                            .borrow_mut()
                            .create_surface(SurfaceDesc {
                                width_px: size.0,
                                height_px: size.1,
                                background: Color(0xFF_FF_FF),
                            })
                            .map_err(|e| format!("create surface: {e}"))?,
                    ));
                    oppa_vello::install_vello_paint_hook_shared(
                        core.host(),
                        backend.clone(),
                        surface_id.clone(),
                        1.0,
                        Rc::new(Cell::new(0)),
                        Rc::new(Cell::new(0)),
                    );
                    let (fid, bytes, index) = atlas_pair(core.text_service());
                    backend.borrow_mut().set_font_for(fid, bytes, index);
                    return Ok(GpuState {
                        surface,
                        backend,
                        surface_id,
                        size,
                        mode,
                        name: name.to_string(),
                        outdated_streak: 0,
                        outdated_total: 0,
                        last_storm_warn: now_secs(),
                        last_stage_log: now_secs(),
                    });
                }
                Err(e) => {
                    log_line(&format!("oppa-fps: {name} gpu failed: {e}"));
                }
            }
        }
        Err("no working GPU backend".to_string())
    }

    /// Picks GPU-or-CPU render state. Native tries GPU first
    /// with loud CPU fallback.
    fn build_render(
        &self,
        window: &Arc<Window>,
        display: raw_window_handle::RawDisplayHandle,
        size: (u32, u32),
    ) -> RenderState {
        match Self::build_gpu(window, display, &self.core, size) {
            Ok(gpu) => {
                log_line(&format!(
                    "oppa-fps: GPU path ({} / {:?})",
                    gpu.name, gpu.mode
                ));
                RenderState::Gpu(gpu)
            }
            Err(e) => {
                log_line(&format!("oppa-fps: {e}; CPU fallback"));
                RenderState::Cpu(Self::build_cpu(window, &self.core, size).expect("CPU fallback"))
            }
        }
    }

    /// Builds CPU render state (tiny-skia pixmap + softbuffer) on
    /// the shared scene (see `app::build_cpu_scene`).
    fn build_cpu(
        window: &Arc<Window>,
        core: &FpsCore,
        size: (u32, u32),
    ) -> Result<CpuState, String> {
        let scene = build_cpu_scene(core, size)?;
        let _context = softbuffer::Context::new(window.clone())
            .map_err(|e| format!("softbuffer context: {e:?}"))?;
        let mut surface = softbuffer::Surface::new(&_context, window.clone())
            .map_err(|e| format!("softbuffer surface: {e:?}"))?;
        surface
            .resize(
                NonZeroU32::new(size.0).ok_or("zero width")?,
                NonZeroU32::new(size.1).ok_or("zero height")?,
            )
            .map_err(|e| format!("softbuffer resize: {e:?}"))?;
        Ok(CpuState {
            backend: scene.backend,
            surface_id: scene.surface_id,
            _context,
            surface,
            size,
            last_stage_log: now_secs(),
        })
    }

    /// (Re)creates window + render state (first resume and every
    /// Android resume). GPU first, CPU fallback — loud either way.
    fn setup(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title("FPS")
            .with_inner_size(PhysicalSize::new(DEFAULT_W, DEFAULT_H))
            .with_visible(false);
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let size = window.inner_size();
        let size = (size.width.max(1), size.height.max(1));
        self.core.set_size(size.0, size.1);
        let display = event_loop
            .owned_display_handle()
            .display_handle()
            .expect("display handle")
            .as_raw();
        self.render = Some(self.build_render(&window, display, size));
        // Warmup frame before first show: the window appears with
        // content, never blank.
        self.core.frame();
        self.present();
        window.set_visible(true);
        self.window = Some(window);
        self.last = now_secs();
    }

    /// Tears down window + render state (Android suspend). The
    /// scene core survives; resume rebuilds around it.
    fn teardown(&mut self) {
        self.render = None;
        self.window = None;
    }

    /// Refits render state to a new client size (the resize path:
    /// reconfigure to the LIVE size, never a stale constant).
    fn on_resize(&mut self, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        self.core.set_size(w, h);
        match self.render.as_mut() {
            #[cfg(not(target_arch = "wasm32"))]
            Some(RenderState::Gpu(gpu)) => {
                if (w, h) == gpu.size {
                    return;
                }
                gpu.size = (w, h);
                let _ = gpu.backend.borrow().configure_surface_with_present_mode(
                    &gpu.surface,
                    w,
                    h,
                    gpu.mode,
                );
                let old = gpu.surface_id.get();
                let new_id = gpu
                    .backend
                    .borrow_mut()
                    .create_surface(SurfaceDesc {
                        width_px: w,
                        height_px: h,
                        background: Color(0xFF_FF_FF),
                    })
                    .expect("resize: create surface");
                gpu.surface_id.set(new_id);
                let _ = gpu.backend.borrow_mut().destroy_surface(old);
                gpu.outdated_streak = 0;
            }
            Some(RenderState::Cpu(cpu)) => {
                if (w, h) == cpu.size {
                    return;
                }
                cpu.size = (w, h);
                cpu.surface
                    .resize(
                        NonZeroU32::new(w).ok_or("zero width").expect("resize"),
                        NonZeroU32::new(h).ok_or("zero height").expect("resize"),
                    )
                    .expect("softbuffer resize");
                let old = cpu.surface_id;
                let new_id = cpu
                    .backend
                    .borrow_mut()
                    .create_surface(SurfaceDesc {
                        width_px: w,
                        height_px: h,
                        background: Color(0xFF_FF_FF),
                    })
                    .expect("resize: create cpu surface");
                cpu.surface_id = new_id;
                let _ = cpu.backend.borrow_mut().destroy_surface(old);
            }
            None => {}
        }
        // Repaint at the new size immediately (the next redraw
        // would do it anyway; this keeps resize feedback instant).
        self.core.frame();
        self.present();
    }

    /// One frame: step the FPS core, run the scheduler, present.
    /// Returns false when the loop must stop (loud present error).
    fn redraw(&mut self) -> bool {
        let now = now_secs();
        let dt = now - self.last;
        self.last = now;
        self.core.step(dt, now);
        self.core.frame();
        self.present()
    }

    /// Presents the current scene. True = keep running.
    fn present(&mut self) -> bool {
        match self.render.as_mut() {
            #[cfg(not(target_arch = "wasm32"))]
            Some(RenderState::Gpu(gpu)) => {
                match gpu
                    .backend
                    .borrow_mut()
                    .present_surface(gpu.surface_id.get(), &gpu.surface)
                {
                    Ok(rep) => {
                        gpu.outdated_streak = 0;
                        // Stage profile every ~5s (per-platform
                        // bottleneck ID — magnitudes, not noise).
                        let now = now_secs();
                        if now - gpu.last_stage_log >= 5.0 {
                            gpu.last_stage_log = now;
                            log_line(&format!(
                                "oppa-fps stages ms: acquire={:.2} target={:.2} render={:.2} blit_setup={:.2} blit_submit={:.2} present={:.2} cpu={:.2}",
                                rep.acquire_ms,
                                rep.target_ms,
                                rep.render_ms,
                                rep.blit_setup_ms,
                                rep.blit_submit_ms,
                                rep.present_ms,
                                rep.cpu_ms
                            ));
                        }
                        true
                    }
                    Err(e) if format!("{e}").contains("Outdated") && gpu.outdated_streak < 600 => {
                        gpu.outdated_streak += 1;
                        gpu.outdated_total += 1;
                        // Yield briefly before reconfiguring (a
                        // resize storm is otherwise reconfigured
                        // every uncapped frame).
                        #[cfg(not(target_arch = "wasm32"))]
                        if gpu.outdated_streak > 3 {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                        let (w, h) = gpu.size;
                        let _ = gpu.backend.borrow().configure_surface_with_present_mode(
                            &gpu.surface,
                            w,
                            h,
                            gpu.mode,
                        );
                        if now_secs() - gpu.last_storm_warn >= 5.0 {
                            gpu.last_storm_warn = now_secs();
                            log_line(&format!(
                                "oppa-fps: Outdated storm ({} reconfigures on {}/{:?} at {w}x{h}): swapchain never matches the window",
                                gpu.outdated_total, gpu.name, gpu.mode
                            ));
                        }
                        true
                    }
                    Err(e) => {
                        log_line(&format!("oppa-fps: present failed: {e}"));
                        false
                    }
                }
            }
            Some(RenderState::Cpu(cpu)) => {
                let t0 = now_secs();
                match present_cpu_frame(&cpu.backend, cpu.surface_id, &mut cpu.surface, cpu.size) {
                    Ok(()) => {
                        let now = now_secs();
                        if now - cpu.last_stage_log >= 5.0 {
                            cpu.last_stage_log = now;
                            log_line(&format!(
                                "oppa-fps stages ms: cpu_present={:.2}",
                                (now - t0) * 1000.0
                            ));
                        }
                        true
                    }
                    Err(e) => {
                        log_line(&format!("oppa-fps: cpu present failed: {e}"));
                        false
                    }
                }
            }
            None => true,
        }
    }
}

impl ApplicationHandler for FpsDriver {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        log_line("oppa-fps: resumed, setting up window");
        self.setup(event_loop);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        log_line("oppa-fps: suspended, tearing down window");
        self.teardown();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                log_line("oppa-fps: close requested");
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.on_resize(size.width, size.height);
            }
            WindowEvent::RedrawRequested if !self.redraw() => {
                event_loop.exit();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Uncapped loop: a redraw every iteration (vsync modes pace
        // themselves).
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

/// Runs the example: builds the driver and pumps the winit loop.
/// `frozen` fixes the readout (pixel comparisons); None = live.
pub fn run(frozen: Option<String>) {
    let mut driver = FpsDriver::new(frozen);
    let event_loop = winit::event_loop::EventLoop::new().expect("event loop");
    event_loop.run_app(&mut driver).expect("event loop run");
}

/// Android run: the loop needs the `AndroidApp` from
/// `android_main` (winit cannot summon it). Same driver after
/// that — suspend/resume teardown applies here most of all.
#[cfg(target_os = "android")]
pub fn run_android(app: android_activity::AndroidApp, frozen: Option<String>) {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    let mut driver = FpsDriver::new(frozen);
    let event_loop = winit::event_loop::EventLoop::builder()
        .with_android_app(app)
        .build()
        .expect("event loop");
    event_loop.run_app(&mut driver).expect("event loop run");
}
