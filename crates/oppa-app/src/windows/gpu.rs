// ---------------------------------------------------------------------------
// GPU swapchain bring-up (Windows)
// ---------------------------------------------------------------------------

use super::*;

/// Live hardware present target (Round 7.4): the `wgpu` swapchain
/// surface for the runner's HWND plus the negotiated mode and backend
/// name for logs. The scene lives in [`DesktopLoop`]'s Vello twin;
/// this is only the swapchain half (mirrors
/// `oppa-fps::driver::GpuState` minus the scene). GDI `blit_rgba`
/// stays as the loud CPU fallback.
pub(crate) struct WindowsGpu {
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) mode: wgpu::PresentMode,
    pub(crate) name: String,
}

/// Ordered GPU backend attempts per target (mirrors the fps driver:
/// Windows probes Vulkan first — measured Vello bring-up ~2s vs
/// ~6-16s Dx12, and only Vulkan persists pipeline-cache data in
/// wgpu-hal 29 — then DX12).
fn backend_attempts() -> Vec<(&'static str, wgpu::Backends)> {
    vec![
        ("vulkan", wgpu::Backends::VULKAN),
        ("dx12", wgpu::Backends::DX12),
    ]
}

fn cache_dir() -> Option<std::path::PathBuf> {
    std::env::var("LOCALAPPDATA")
        .ok()
        .map(|b| std::path::PathBuf::from(b).join("oppa-app"))
}

/// Builds the GPU swapchain against `hwnd`, or returns the loud
/// reason to fall back to GDI (mirrors
/// `oppa-fps::driver::FpsDriver::build_gpu`).
pub(crate) fn build_gpu_windows(
    hwnd: windows::Win32::Foundation::HWND,
    loop_: &mut DesktopLoop,
    size: (u32, u32),
) -> Result<WindowsGpu, String> {
    let (w, h) = (size.0.max(1), size.1.max(1));
    let hwnd_val = hwnd.0 as isize;
    // Round 19.8: the fps-demo hinstance shape re-ported (19.7 proved it
    // serves Vulkan end to end, then reverted it for the empty-capabilities
    // reconfigure below). Vulkan requires the real module handle on the
    // raw window handle; DX12 tolerates None.
    let mut win_handle = Win32WindowHandle::new(
        NonZeroIsize::new(hwnd_val).ok_or_else(|| "null HWND for gpu surface".to_string())?,
    );
    win_handle.hinstance = unsafe {
        windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
            .ok()
            .and_then(|h| NonZeroIsize::new(h.0 as isize))
    };
    if win_handle.hinstance.is_none() {
        eprintln!("oppa-app: GetModuleHandleW failed; Vulkan surface may refuse (DX12 unaffected)");
    }
    let raw_window = RawWindowHandle::Win32(win_handle);
    let raw_display = RawDisplayHandle::Windows(WindowsDisplayHandle::new());
    for (name, backends) in backend_attempts() {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = backends;
        let instance: &'static wgpu::Instance = Box::leak(Box::new(wgpu::Instance::new(desc)));
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(raw_display),
            raw_window_handle: raw_window,
        };
        let surface = match unsafe { instance.create_surface_unsafe(target) } {
            Ok(s) => s,
            Err(e) => {
                eprintln!("oppa-app: {name} surface failed: {e}");
                continue;
            }
        };
        let cache_path = cache_dir().map(|d| d.join(format!("vello-pipeline-cache-{name}.bin")));
        let cache_data = cache_path.as_ref().and_then(|p| std::fs::read(p).ok());
        eprintln!(
            "oppa-app: {name} pipeline cache {}",
            cache_data
                .as_ref()
                .map(|d| format!("{} bytes loaded (warm)", d.len()))
                .unwrap_or_else(|| "absent (cold)".to_string())
        );
        match loop_.ensure_gpu_for_surface_with_cache(instance, &surface, cache_data) {
            Ok((adapter, saved)) => {
                eprintln!("oppa-app: {name} {adapter}");
                if let (Some(path), Some(data)) = (cache_path, saved) {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    match std::fs::write(&path, &data) {
                        Ok(()) => {
                            eprintln!("oppa-app: pipeline cache {} bytes saved", data.len())
                        }
                        Err(e) => eprintln!("oppa-app: pipeline cache save failed: {e}"),
                    }
                }
                let mode = loop_.pick_gpu_mode(&surface);
                loop_
                    .configure_gpu_surface(&surface, w, h, mode)
                    .map_err(|e| format!("configure surface: {e}"))?;
                loop_.enable_gpu(mode);
                return Ok(WindowsGpu {
                    surface,
                    mode,
                    name: name.to_string(),
                });
            }
            Err(e) => {
                eprintln!("oppa-app: {name} gpu failed: {e}");
            }
        }
    }
    Err("no working GPU backend".to_string())
}

/// Reconfigures the GPU swapchain to the loop's live viewport (Round
/// 7.4 — resize/DPR arms call this; Round 19.8: a reconfigure failure
/// retries once before disabling GPU loudly and keeping GDI, never a
/// silent blank. The retry is insurance against a transient surface
/// refusal; 19.7's empty-capabilities read (`offered: []`) root-caused
/// to a destroyed window at teardown (see `hwnd_alive`) — not a live
/// transient — so a repeated refusal still disables; the 7.4
/// loud-fallback design is unchanged, only given one second chance
/// with both attempts on stderr).
pub(crate) fn reconfigure_gpu(loop_: &mut DesktopLoop, gpu: &WindowsGpu) {
    if !loop_.is_gpu() {
        return;
    }
    let (w, h) = loop_.viewport();
    if let Err(e) = loop_.configure_gpu_surface(&gpu.surface, w, h, gpu.mode) {
        eprintln!(
            "oppa-app: gpu reconfigure failed ({w}x{h} {mode:?}: {e}); retrying once",
            mode = gpu.mode
        );
        if let Err(e2) = loop_.configure_gpu_surface(&gpu.surface, w, h, gpu.mode) {
            eprintln!("oppa-app: gpu reconfigure retry failed: {e2}; CPU fallback");
            loop_.disable_gpu();
        } else {
            eprintln!("oppa-app: gpu reconfigure retry succeeded ({w}x{h})");
        }
    }
}
