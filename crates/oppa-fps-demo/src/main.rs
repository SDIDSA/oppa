//! oppa-fps-demo: a white desktop window with black centered text
//! showing the FPS the renderer is managing, in real time.
//!
//! Product-path wiring only: `Win32Shell` window → `ComponentHost`
//! scene (one centered `Text` leaf) → `DWriteTextService` measure →
//! Vello GPU encode → real swapchain present. The FPS number is the
//! measured present-loop rate (EMA of frame dt, refreshed at 4 Hz),
//! not an injected clock. Windows-only (shell + DirectWrite slices).

#[cfg(windows)]
mod app {
    use oppa::text::{TextService, TextStyle};
    use oppa::{
        Color, ComponentHost, Ctx, Div, LayoutTextConfig, RendererBackend, SharedString, Style,
        SurfaceDesc, SurfaceId, Text, VNode,
    };
    use oppa_cpu::{install_paint_hook, CpuBackend};
    use oppa_macros::{component, Props};
    use oppa_shell_win::{ShellConfig, Win32Shell};
    use oppa_text_dwrite::DWriteTextService;
    use oppa_vello::{install_vello_paint_hook_shared, VelloBackend};
    use raw_window_handle::{
        RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
    };
    use std::cell::{Cell, RefCell};
    use std::num::NonZeroIsize;
    use std::rc::Rc;
    use std::time::{Duration, Instant};
    use wgpu::SurfaceTargetUnsafe;
    use windows::Win32::Foundation::HWND;

    const WIN_W: f32 = 800.0;
    const WIN_H: f32 = 600.0;
    const FONT_PX: f32 = 64.0;

    #[derive(Clone, Props)]
    struct FpsProps {
        label: SharedString,
        x: f32,
        y: f32,
    }

    #[component]
    fn FpsApp(ctx: &Ctx, props: &FpsProps) -> VNode {
        let _ = ctx;
        Div("screen")
            .style(Style::new().size(WIN_W, WIN_H).bg(Color(0xFF_FF_FF)))
            .child(
                Div("center")
                    .style(
                        Style::new()
                            .x(props.x)
                            .absolute_y(props.y)
                            .ink(Color(0x00_00_00)),
                    )
                    .child(
                        Text {
                            text: props.label.clone(),
                            style: Text::title_small,
                        }
                        .into(),
                    ),
            )
    }

    /// Presents a CPU pixmap through GDI (demo-only): RGBA → BGRA
    /// swap + `SetDIBitsToDevice` on the window client DC. Loud on
    /// any failure (a present failure is never a silent blank frame).
    fn present_cpu(backend: &RefCell<CpuBackend>, surface: SurfaceId, hwnd: HWND) {
        use windows::Win32::Graphics::Gdi::{
            GetDC, ReleaseDC, SetDIBitsToDevice, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
            DIB_RGB_COLORS, RGBQUAD,
        };
        let borrowed = backend.borrow();
        let pix = borrowed.pixmap(surface).expect("cpu surface pixmap");
        let (w, h) = (pix.width(), pix.height());
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for px in pix.data().chunks_exact(4) {
            bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }],
        };
        unsafe {
            let dc = GetDC(Some(hwnd));
            if dc.is_invalid() {
                panic!("oppa-fps-demo: GetDC failed");
            }
            let rows = SetDIBitsToDevice(
                dc,
                0,
                0,
                w,
                h,
                0,
                0,
                0,
                h,
                bgra.as_ptr() as *const core::ffi::c_void,
                &bmi,
                DIB_RGB_COLORS,
            );
            ReleaseDC(Some(hwnd), dc);
            if rows != h as i32 {
                panic!("oppa-fps-demo: SetDIBitsToDevice painted {rows} of {h} rows");
            }
        }
    }

    /// Measures `label` through DirectWrite and centers it in a
    /// `w`×`h` box (the live viewport — never the creation-time
    /// constants, or a resize leaves the text off-center).
    fn centered_props(
        dwrite: &DWriteTextService,
        style: &TextStyle,
        label: &str,
        w: f32,
        h: f32,
    ) -> FpsProps {
        let run = dwrite.shape(label, style).expect("shape fps label");
        let m = dwrite.measure_line(&run);
        FpsProps {
            label: SharedString::from(label),
            x: (w - run.total_advance) / 2.0,
            y: (h - (m.ascent + m.descent)) / 2.0,
        }
    }

    pub fn run() {
        let t0 = Instant::now();
        let stamp = |t: Instant, what: &str| {
            eprintln!(
                "oppa-fps-demo init: {what} at {:.0}ms",
                t.elapsed().as_millis()
            )
        };
        // Window, hidden: the ~10s GPU bring-up (Vello shader compile)
        // happens with no window at all instead of a blank
        // unresponsive one. The queue still pumps between stages.
        let mut shell = Win32Shell::new(ShellConfig {
            title: "FPS".to_string(),
            width: WIN_W as i32,
            height: WIN_H as i32,
            record_messages: false,
            suppress_os_composition_window: true,
            visible: false,
        })
        .expect("create window");
        stamp(t0, "window created (hidden)");

        // Framework host: measure through DirectWrite, 64px Segoe UI.
        let dwrite = Rc::new(DWriteTextService::new().expect("DirectWrite factory"));
        let host = ComponentHost::new();
        host.set_viewport(WIN_W, WIN_H);
        host.set_text_service(Box::new(
            DWriteTextService::new().expect("DirectWrite factory"),
        ));
        host.set_layout_config(LayoutTextConfig {
            family: "Segoe UI".to_string(),
            title_px: FONT_PX,
            ..Default::default()
        });
        let text_style = TextStyle::new("Segoe UI", FONT_PX);
        let handle = host.mount(
            "FpsApp",
            centered_props(&dwrite, &text_style, "FPS: --", WIN_W, WIN_H),
            FpsApp,
        );
        host.run_until_idle();

        // OPPA_FPS_LABEL freezes the readout (pixel comparisons across
        // backends): set once here, live updates skipped in the loops.
        let frozen_label: Option<String> = std::env::var("OPPA_FPS_LABEL").ok();
        if let Some(ref label) = frozen_label {
            handle.set_props(centered_props(&dwrite, &text_style, label, WIN_W, WIN_H));
        }

        // Backend select: cpu (tiny-skia ink bars + GDI present --
        // no GPU bring-up at all), dx12-only, or default
        // (Vulkan-first). One env var, read once.
        let mode = std::env::var("OPPA_FPS_BACKEND").unwrap_or_default();
        let hwnd = shell.hwnd();
        // Client-exact window sizing (decision 202): `CreateWindowExW`
        // takes outer dims, so without this the client area is
        // 784x561 under an 800x600 swapchain (DWM downscales every
        // frame). Grow the frame so the client is exactly the scene
        // size: swapchain, pixmap, and client pixels line up 1:1 in
        // every mode. Demo-side policy; the shell keeps its
        // outer-dims creation semantics for existing users.
        unsafe {
            use windows::Win32::Foundation::RECT;
            use windows::Win32::UI::WindowsAndMessaging::{
                AdjustWindowRect, GetClientRect, GetWindowLongW, SetWindowPos, GWL_STYLE,
                SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, WINDOW_STYLE,
            };
            let style = WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32);
            let mut rc = RECT {
                left: 0,
                top: 0,
                right: WIN_W as i32,
                bottom: WIN_H as i32,
            };
            let _ = AdjustWindowRect(&mut rc, style, false);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                rc.right - rc.left,
                rc.bottom - rc.top,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let mut cr = RECT::default();
            let _ = GetClientRect(hwnd, &mut cr);
            eprintln!(
                "oppa-fps-demo: client {}x{} (scene {}x{})",
                cr.right - cr.left,
                cr.bottom - cr.top,
                WIN_W as u32,
                WIN_H as u32
            );
        }
        if mode == "cpu" {
            eprintln!("oppa-fps-demo: cpu backend (startup comparison)");
            let cpu = Rc::new(RefCell::new(CpuBackend::new()));
            // Glyph faces (decision 200): without this the CPU backend
            // draws legacy advance-cell bars; with it, real outlines.
            let probe = dwrite.shape("FPS: 60", &text_style).expect("probe shape");
            let fid = probe.runs[0].font_id;
            if let Some((path, index)) = dwrite.font_file_source(fid) {
                match std::fs::read(&path) {
                    Ok(bytes) => cpu.borrow_mut().set_font_for(fid, bytes, index),
                    Err(e) => eprintln!("oppa-fps-demo: cpu face unreadable: {e} (bars)"),
                }
            } else {
                eprintln!("oppa-fps-demo: no cpu face (bars)");
            }
            let cpu_surface = cpu
                .borrow_mut()
                .create_surface(SurfaceDesc {
                    width_px: WIN_W as u32,
                    height_px: WIN_H as u32,
                    background: Color(0xFF_FF_FF),
                })
                .expect("create cpu surface");
            install_paint_hook(
                &host,
                cpu.clone(),
                cpu_surface,
                1.0,
                Rc::new(Cell::new(0)),
                Rc::new(Cell::new(0)),
            );
            stamp(t0, "cpu backend ready");
            if shell.process_os_messages() {
                return;
            }
            // Warmup frame before the window is shown, so it appears
            // with content, never blank.
            host.runtime().request_frame();
            host.run_until_idle();
            present_cpu(&cpu, cpu_surface, hwnd);
            stamp(t0, "first present done (window still hidden)");
            shell.show();
            stamp(t0, "window shown with content");

            // Frame loop: GDI present is synchronous and unpaced.
            let mut last = Instant::now();
            let mut ema_dt = 1.0 / 60.0;
            let mut last_label_at = Instant::now();
            loop {
                if shell.process_os_messages() {
                    break;
                }
                let now = Instant::now();
                let dt = (now - last).as_secs_f64().clamp(0.0, 0.25);
                last = now;
                ema_dt = ema_dt * 0.95 + dt * 0.05;
                if frozen_label.is_none()
                    && now.duration_since(last_label_at) >= Duration::from_millis(250)
                {
                    last_label_at = now;
                    let label = format!("FPS: {:.0}", 1.0 / ema_dt.max(1e-6));
                    handle.set_props(centered_props(&dwrite, &text_style, &label, WIN_W, WIN_H));
                }
                host.runtime().request_frame();
                host.run_until_idle();
                present_cpu(&cpu, cpu_surface, hwnd);
            }
            return;
        }

        // Vello backend: atlas needs the Segoe UI face bytes (the
        // `font_file_source` + fs-read file locator).
        let backend = Rc::new(RefCell::new(VelloBackend::new()));
        let probe = dwrite.shape("FPS: 60", &text_style).expect("probe shape");
        let fid = probe.runs[0].font_id;
        let (path, index) = dwrite
            .font_file_source(fid)
            .expect("font file for Segoe UI");
        let bytes = std::fs::read(&path).expect("read font file");
        backend.borrow_mut().set_font_bytes(bytes, index);
        stamp(t0, "host+mount+atlas ready");
        if shell.process_os_messages() {
            return;
        }

        // GPU backend preference (measured 2026-09-27 on this box:
        // vello Renderer::new ~1.9s on Vulkan vs ~10-16s on Dx12, and
        // only Vulkan persists pipeline-cache data in wgpu-hal 29).
        // Vulkan first, default backends (Dx12) as fallback.
        // OPPA_FPS_BACKEND=dx12 forces the default path (diagnostics).
        let force_dx12 = mode == "dx12";
        let cache_dir = std::env::var("LOCALAPPDATA")
            .map(|base| std::path::PathBuf::from(base).join("oppa-fps-demo"))
            .ok();
        let backend_choice: [(&str, Option<wgpu::Backends>); 2] = [
            ("vulkan", Some(wgpu::Backends::VULKAN)),
            ("dx12", Some(wgpu::Backends::DX12)),
        ];
        // OPPA_FPS_BACKEND=dx12 forces the Dx12-only path (diagnostics);
        // otherwise Vulkan first with Dx12 as fallback. ("default" as a
        // name is retired: wgpu's all-backends pick landed on Vulkan
        // here anyway, which made the old fallback label a lie.)
        let backend_choice: Vec<(&str, Option<wgpu::Backends>)> = if force_dx12 {
            eprintln!("oppa-fps-demo: OPPA_FPS_BACKEND=dx12, Dx12-only path");
            backend_choice[1..].to_vec()
        } else {
            backend_choice.to_vec()
        };
        let mut gpu: Option<(String, wgpu::Surface<'static>)> = None;
        for (name, backends) in backend_choice {
            // Swapchain surface from the window HWND (Fifo = vsync-paced).
            // The surface borrows its instance, so the winning instance
            // is intentionally leaked: one per process, genuinely
            // 'static, dropped never (sound: the borrow outlives all use).
            let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
            if let Some(b) = backends {
                desc.backends = b;
            }
            let try_instance: &'static wgpu::Instance =
                Box::leak(Box::new(wgpu::Instance::new(desc)));
            let mut window_handle = Win32WindowHandle::new(
                NonZeroIsize::new(shell.hwnd().0 as isize).expect("null hwnd"),
            );
            // Vulkan requires the real module handle (Dx12 tolerates
            // None, which is why the default path worked without it).
            let hinstance = unsafe {
                windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                    .expect("module handle")
            };
            window_handle.hinstance = NonZeroIsize::new(hinstance.0 as isize);
            let target = SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(RawDisplayHandle::Windows(WindowsDisplayHandle::new())),
                raw_window_handle: RawWindowHandle::Win32(window_handle),
            };
            let try_surface = match unsafe { try_instance.create_surface_unsafe(target) } {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("oppa-fps-demo: {name} surface failed: {e}");
                    continue;
                }
            };
            // Disk-backed pipeline cache, one file per backend name.
            let cache_path = cache_dir
                .as_ref()
                .map(|d| d.join(format!("vello-pipeline-cache-{name}.bin")));
            let cache_data = cache_path.as_ref().and_then(|p| std::fs::read(p).ok());
            eprintln!(
                "oppa-fps-demo: {name} pipeline cache {}",
                cache_data
                    .as_ref()
                    .map(|d| format!("{} bytes loaded (warm)", d.len()))
                    .unwrap_or_else(|| "absent (cold)".to_string())
            );
            match backend.borrow_mut().ensure_gpu_for_surface_with_cache(
                try_instance,
                &try_surface,
                cache_data,
            ) {
                Ok((adapter, saved)) => {
                    eprintln!("oppa-fps-demo: {name} {adapter}");
                    if let (Some(path), Some(data)) = (cache_path, saved) {
                        if let Some(parent) = path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        match std::fs::write(&path, &data) {
                            Ok(()) => eprintln!(
                                "oppa-fps-demo: pipeline cache {} bytes saved",
                                data.len()
                            ),
                            Err(e) => {
                                eprintln!("oppa-fps-demo: pipeline cache save failed: {e}")
                            }
                        }
                    }
                    gpu = Some((name.to_string(), try_surface));
                    break;
                }
                Err(e) => {
                    eprintln!("oppa-fps-demo: {name} gpu failed: {e}");
                }
            }
        }
        let (backend_name, surface) = gpu.expect("no working GPU backend");
        stamp(t0, "gpu device ready");
        if shell.process_os_messages() {
            return;
        }
        // OPPA_FPS_PRESENT=immediate|mailbox selects the swapchain
        // mode (raw-throughput legs); default is vsync-paced Fifo.
        // All modes display on both backends (decision 202); a blank
        // window with a healthy loop is a size mismatch (resize
        // without reconfigure), now tracked live in the loop below.
        let present_mode = match std::env::var("OPPA_FPS_PRESENT").as_deref() {
            Ok("immediate") => {
                eprintln!("oppa-fps-demo: present mode Immediate (uncapped)");
                wgpu::PresentMode::Immediate
            }
            Ok("mailbox") => {
                eprintln!("oppa-fps-demo: present mode Mailbox");
                wgpu::PresentMode::Mailbox
            }
            _ => wgpu::PresentMode::Fifo,
        };
        // Log the offered present modes once (diagnosing a blank
        // window starts here: an unoffered mode never displays).
        if let Some(adapter) = backend.borrow().gpu_adapter() {
            eprintln!(
                "oppa-fps-demo: offered present modes: {:?}",
                surface.get_capabilities(adapter).present_modes
            );
        }
        backend
            .borrow()
            .configure_surface_with_present_mode(&surface, WIN_W as u32, WIN_H as u32, present_mode)
            .expect("configure surface");
        let surface_id = Rc::new(Cell::new(
            backend
                .borrow_mut()
                .create_surface(SurfaceDesc {
                    width_px: WIN_W as u32,
                    height_px: WIN_H as u32,
                    background: Color(0xFF_FF_FF),
                })
                .expect("create surface"),
        ));
        install_vello_paint_hook_shared(
            &host,
            backend.clone(),
            surface_id.clone(),
            1.0,
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
        );

        // Warmup frame before the window is shown, so it appears
        // with content, never blank.
        host.runtime().request_frame();
        host.run_until_idle();
        backend.borrow_mut().advance_frame();
        backend
            .borrow_mut()
            .present_surface(surface_id.get(), &surface)
            .expect("warmup present");
        stamp(t0, "first present done (window still hidden)");
        shell.show();
        stamp(t0, "window shown with content");

        // Frame loop: request → run → present; dt EMA → label at 4 Hz.
        // An occluded/minimized window loses the swapchain (Outdated):
        // reconfigure and ride it out instead of dying (a real app
        // never exits because the user alt-tabbed). Other present
        // errors still break loudly. A sustained Outdated storm
        // means the swapchain never matches the window (decision
        // 202) — that is logged loudly, never a silent blank
        // window.
        let mut last = Instant::now();
        let mut ema_dt = 1.0 / 60.0;
        let mut last_label_at = Instant::now();
        let mut outdated_streak = 0u32;
        let mut outdated_total = 0u64;
        let mut last_storm_warn_at = Instant::now();
        // Current client size. The surface is always reconfigured to
        // THIS, not to the creation-time constants: after a user
        // resize the client no longer matches WIN_W×WIN_H, and
        // reconfiguring to the stale size rebuilds a swapchain DWM
        // can never present (blank window + Outdated forever).
        let mut surf_w = WIN_W as u32;
        let mut surf_h = WIN_H as u32;
        // Current label text for recentering: the label position is
        // derived from the text, so a resize must re-run
        // `centered_props` with the live viewport, not just refit it.
        let mut current_label = frozen_label
            .clone()
            .unwrap_or_else(|| "FPS: --".to_string());
        loop {
            if shell.process_os_messages() {
                break;
            }
            // Live client size (a resize is the usual reason acquire
            // reports Outdated). Reconfigure and rebuild the scene
            // surface whenever it changed.
            let mut cr = windows::Win32::Foundation::RECT::default();
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut cr);
            }
            let cw = (cr.right - cr.left).max(1) as u32;
            let ch = (cr.bottom - cr.top).max(1) as u32;
            if cw != surf_w || ch != surf_h {
                surf_w = cw;
                surf_h = ch;
                // Configure the swapchain to the new client size.
                let _ = backend.borrow().configure_surface_with_present_mode(
                    &surface,
                    surf_w,
                    surf_h,
                    present_mode,
                );
                // Create the new scene surface BEFORE publishing its
                // id (the next paint must never see a stale handle),
                // then drop the old one.
                let old = surface_id.get();
                let new_id = backend
                    .borrow_mut()
                    .create_surface(SurfaceDesc {
                        width_px: surf_w,
                        height_px: surf_h,
                        background: Color(0xFF_FF_FF),
                    })
                    .expect("resize: create surface");
                surface_id.set(new_id);
                let _ = backend.borrow_mut().destroy_surface(old);
                host.set_viewport(surf_w as f32, surf_h as f32);
                // Recenter the text in the new viewport (the position
                // was computed for the old size).
                handle.set_props(centered_props(
                    &dwrite,
                    &text_style,
                    &current_label,
                    surf_w as f32,
                    surf_h as f32,
                ));
                backend.borrow_mut().advance_frame();
                host.runtime().request_frame();
                host.run_until_idle();
                continue;
            }
            let now = Instant::now();
            let dt = (now - last).as_secs_f64().clamp(0.0, 0.25);
            last = now;
            ema_dt = ema_dt * 0.95 + dt * 0.05;
            if frozen_label.is_none()
                && now.duration_since(last_label_at) >= Duration::from_millis(250)
            {
                last_label_at = now;
                current_label = format!("FPS: {:.0}", 1.0 / ema_dt.max(1e-6));
                handle.set_props(centered_props(
                    &dwrite,
                    &text_style,
                    &current_label,
                    surf_w as f32,
                    surf_h as f32,
                ));
            }
            host.runtime().request_frame();
            host.run_until_idle();
            backend.borrow_mut().advance_frame();
            let presented = backend
                .borrow_mut()
                .present_surface(surface_id.get(), &surface);
            match presented {
                Ok(_) => {
                    outdated_streak = 0;
                }
                Err(e) if format!("{e}").contains("Outdated") && outdated_streak < 600 => {
                    outdated_streak += 1;
                    outdated_total += 1;
                    if outdated_streak > 3 {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    // Reconfigure to the LIVE client size (see above).
                    let _ = backend.borrow().configure_surface_with_present_mode(
                        &surface,
                        surf_w,
                        surf_h,
                        present_mode,
                    );
                    // Loud storm warning: a visible window that never
                    // stops reconfiguring is a real problem worth
                    // surfacing. Silent in healthy runs.
                    if now.duration_since(last_storm_warn_at) >= Duration::from_secs(5) {
                        last_storm_warn_at = Instant::now();
                        eprintln!(
                            "oppa-fps-demo: Outdated storm ({outdated_total} reconfigures on {backend_name}/{present_mode:?} at {surf_w}x{surf_h}): swapchain never matches the window (decision 202)"
                        );
                    }
                }
                Err(e) => {
                    eprintln!("oppa-fps-demo: present failed: {e}");
                    break;
                }
            }
        }
    }
}

#[cfg(windows)]
fn main() {
    app::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("oppa-fps-demo: Windows only (shell + DirectWrite slices)");
}
