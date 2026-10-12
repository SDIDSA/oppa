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
/// DX12, then GL — see [`WINDOWS_GPU_ORDER`](crate::WINDOWS_GPU_ORDER)
/// (decision 390). An explicit `OPPA_RENDERER=gpu-vulkan` /
/// `gpu-dx12` / `gpu-gl` pins the single backend instead (never a
/// silent substitution when the user named one) — the pin match
/// lives once in [`presenter_rows`](crate::presenter_rows).
struct WindowsSurfaceFactory {
    raw_window_handle: RawWindowHandle,
    raw_display_handle: RawDisplayHandle,
}

impl WindowsSurfaceFactory {
    /// Snapshots the HWND into raw handles once (Vulkan requires the
    /// real module handle; DX12 tolerates `None` — a failed lookup
    /// warns once here instead of once per row).
    fn new(hwnd_val: NonZeroIsize) -> Self {
        let mut win_handle = Win32WindowHandle::new(hwnd_val);
        win_handle.hinstance = unsafe {
            windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .ok()
                .and_then(|h| NonZeroIsize::new(h.0 as isize))
        };
        if win_handle.hinstance.is_none() {
            eprintln!(
                "oppa-app: GetModuleHandleW failed; Vulkan surface may refuse (DX12 unaffected)"
            );
        }
        Self {
            raw_window_handle: RawWindowHandle::Win32(win_handle),
            raw_display_handle: RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
        }
    }
}

impl<'a> crate::SurfaceFactory<'a> for WindowsSurfaceFactory {
    fn make_surface(
        &mut self,
        backends: wgpu::Backends,
    ) -> Result<crate::BuiltSurface<'a>, String> {
        // One leaked instance per attempt (the surface borrows it;
        // the winner lives for the process).
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = backends;
        let instance: &'static wgpu::Instance = Box::leak(Box::new(wgpu::Instance::new(desc)));
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(self.raw_display_handle),
            raw_window_handle: self.raw_window_handle,
        };
        let surface =
            unsafe { instance.create_surface_unsafe(target) }.map_err(|e| format!("{e}"))?;
        Ok(crate::BuiltSurface { instance, surface })
    }
}

/// Builds the GPU swapchain against `hwnd` through the shared
/// presenter ladder (see [`WINDOWS_GPU_ORDER`](crate::WINDOWS_GPU_ORDER),
/// closed with the terminal CPU row), or returns the loud reason to
/// fall back to the GDI blit leg.
pub(crate) fn build_gpu_windows(
    hwnd: windows::Win32::Foundation::HWND,
    loop_: &mut DesktopLoop,
    size: (u32, u32),
) -> Result<WindowsGpu, String> {
    let (w, h) = (size.0.max(1), size.1.max(1));
    let rows = crate::presenter_rows(&crate::WINDOWS_GPU_ORDER, true);
    let hwnd_val = NonZeroIsize::new(hwnd.0 as isize)
        .ok_or_else(|| "null HWND for gpu surface".to_string())?;
    let mut factory = WindowsSurfaceFactory::new(hwnd_val);
    // Styled (decision 482 — transparent windows negotiate a
    // premultiplied-alpha swapchain; a surface refusing the mode
    // aborts loudly into the CPU fallback, never a silent opaque
    // window). Read once: the loop owns the style, the closure owns
    // only the scene.
    let alpha = loop_.surface_alpha_mode();
    let mut attempt = move |scene: &mut crate::SceneCore,
                            row: crate::GpuRow,
                            instance: &wgpu::Instance,
                            surface: &wgpu::Surface<'_>|
          -> Result<crate::GpuApplied, crate::RowFail> {
        let adapter = scene
            .ensure_gpu_for_surface(instance, surface)
            .map_err(|e| crate::RowFail::Next(format!("{} gpu failed: {e}", row.name)))?;
        eprintln!("oppa-app: {} {adapter}", row.name);
        let mode = scene.pick_gpu_mode(surface);
        let format = scene
            .configure_gpu_surface_alpha(surface, w, h, mode, alpha)
            .map_err(|e| crate::RowFail::Abort(format!("configure surface: {e}")))?;
        scene.enable_gpu(mode);
        // Decision 473: nobody reads the CPU pixmap while the
        // swapchain serves — skip the redundant full-scene software
        // raster (the Android per-generation precedent). Transient
        // present failures refresh it once via `refresh_cpu_pixmap`
        // without leaving GPU mode; `disable_gpu` re-arms it on the
        // loud-failure fallback.
        scene.set_cpu_paint(false);
        // Record the winning backend for the debug badge
        // (decision 387): a closed row vocabulary, so an unknown
        // name refuses loudly instead of guessing.
        match crate::GpuApi::from_row_name(row.name) {
            Some(api) => scene.set_gpu_api(api),
            None => {
                return Err(crate::RowFail::Abort(format!(
                    "winning gpu backend {:?} is not a known row",
                    row.name
                )));
            }
        }
        Ok(crate::GpuApplied {
            adapter,
            mode,
            format,
        })
    };
    match crate::walk_presenter_rows(&rows, &mut factory, &mut attempt, loop_.scene_mut()) {
        Ok(crate::LadderOutcome::Gpu(up)) => Ok(WindowsGpu {
            surface: up.surface,
            mode: up.mode,
            name: up.name.to_string(),
        }),
        // The ladder logs the refusal line; the CPU wording stays
        // here (the runner owns its blit leg).
        Ok(crate::LadderOutcome::Cpu) => Err("no working GPU backend".to_string()),
        Err(e) => Err(e.to_string()),
    }
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
    // Styled (decision 482): resizes re-apply the loop's alpha mode
    // so a transparent window never reverts to opaque mid-session.
    if let Err(e) = loop_.configure_gpu_surface_styled(&gpu.surface, w, h, gpu.mode) {
        eprintln!(
            "oppa-app: gpu reconfigure failed ({w}x{h} {mode:?}: {e}); retrying once",
            mode = gpu.mode
        );
        if let Err(e2) = loop_.configure_gpu_surface_styled(&gpu.surface, w, h, gpu.mode) {
            eprintln!("oppa-app: gpu reconfigure retry failed: {e2}; CPU fallback");
            if let Err(e) = loop_.disable_gpu() {
                eprintln!("oppa-app: cpu catch-up repaint failed: {e}");
            }
        } else {
            eprintln!("oppa-app: gpu reconfigure retry succeeded ({w}x{h})");
        }
    }
}
