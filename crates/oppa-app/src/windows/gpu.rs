// ---------------------------------------------------------------------------
// GPU swapchain bring-up (Windows)
// ---------------------------------------------------------------------------

use super::*;

/// Live hardware present target: the `wgpu` swapchain surface for
/// the runner's HWND plus the negotiated mode and backend name for
/// logs. The scene lives in [`DesktopLoop`]'s GPU twin; this is
/// only the swapchain half. GDI `blit_rgba` stays as the loud CPU
/// fallback.
pub(crate) struct WindowsGpu {
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) mode: wgpu::PresentMode,
    pub(crate) name: String,
}

/// Ordered GPU backend attempts: Windows probes Vulkan first, then
/// DX12, then GL (decision 390 — a DX12-dead but GL-alive box, e.g.
/// pre-Win10, crippled VMs, corrupt D3D12 stacks, still gets
/// hardware; healthy machines never reach past Vulkan). An explicit
/// `OPPA_RENDERER=gpu-vulkan` / `gpu-dx12` / `gpu-gl` pins the
/// single backend instead (never a silent substitution when the
/// user named one).
fn backend_attempts() -> Vec<(&'static str, wgpu::Backends)> {
    match crate::renderer_override() {
        Some(crate::RendererKind::GpuDx12) => vec![("dx12", wgpu::Backends::DX12)],
        Some(crate::RendererKind::GpuVulkan) => vec![("vulkan", wgpu::Backends::VULKAN)],
        Some(crate::RendererKind::GpuGl) => vec![("gl", wgpu::Backends::GL)],
        _ => vec![
            ("vulkan", wgpu::Backends::VULKAN),
            ("dx12", wgpu::Backends::DX12),
            ("gl", wgpu::Backends::GL),
        ],
    }
}

/// Builds the GPU swapchain against `hwnd`, or returns the loud
/// reason to fall back to GDI.
pub(crate) fn build_gpu_windows(
    hwnd: windows::Win32::Foundation::HWND,
    loop_: &mut DesktopLoop,
    size: (u32, u32),
) -> Result<WindowsGpu, String> {
    let (w, h) = (size.0.max(1), size.1.max(1));
    let hwnd_val = hwnd.0 as isize;
    // Vulkan requires the real module handle on the raw window
    // handle; DX12 tolerates None.
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
        match loop_.ensure_gpu_for_surface(instance, &surface) {
            Ok(adapter) => {
                eprintln!("oppa-app: {name} {adapter}");
                let mode = loop_.pick_gpu_mode(&surface);
                loop_
                    .configure_gpu_surface(&surface, w, h, mode)
                    .map_err(|e| format!("configure surface: {e}"))?;
                loop_.enable_gpu(mode);
                // Record the winning backend for the debug badge
                // (decision 387): a closed row vocabulary, so an
                // unknown name refuses loudly instead of guessing.
                match crate::GpuApi::from_row_name(name) {
                    Some(api) => loop_.set_gpu_api(api),
                    None => {
                        return Err(format!("winning gpu backend {name:?} is not a known row"));
                    }
                }
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
