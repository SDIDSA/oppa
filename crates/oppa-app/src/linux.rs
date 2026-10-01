//! Linux event/present glue (decision 242, Round 7.4 decision 279):
//! winit window + `LinuxShell` intake + system text service + GPU-first
//! paint (Vello hardware swapchains primary, softbuffer CPU fallback).
//! Structure mirrors the proven `linux_demo` example
//! (translate → shell → `take_cmds` → `to_input_event` → inject →
//! repaint → redraw) minus its diagnostics/bounded-run harness, with
//! the swapchain bring-up mirrored from `oppa-fps::driver` (Vulkan
//! attempt, pipeline-cache round-trip, Immediate/Mailbox/Fifo pick).
//! Under Wayland the GPU path creates hardware swapchains
//! (`zwp_linux_dmabuf_v1`) running natively on the compositor
//! (including WSLg) without touching Weston's software SHM
//! `libpixman` bug; the CPU fallback stays on softbuffer.
//!
//! Density policy (Round 2.4 — replaces the old fixed-1.0 rule):
//! the shell + loop follow the window's live scale factor
//! (`ScaleFactorChanged` → density + DPR + surface re-base, cursor
//! rescaled). Intake positions stay density-normalized throughout
//! (uniform with the Windows arm — physical px only ever appear at
//! the surface/window boundary).

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use oppa::shell::PlatformShell;
use oppa::{Ctx, InputEvent, KeyState, Props, VNode};
use oppa_shell_linux::{
    translate, LinuxClipboard, LinuxCmd, LinuxFileDialog, LinuxShell, LinuxSystemTheme,
    LinuxThemeWatcher, LinuxWindowControl, ShellConfig, ShellWindow, StdRunner,
};
use oppa_text_linux::LinuxTextService;

use crate::{gpu_disabled_by_env, is_edit_shortcut_modifier, DesktopLoop, WindowOptions};

/// Live hardware present target (Round 7.4): the `wgpu` swapchain
/// surface for the runner's window plus the negotiated mode and the
/// backend name for logs. The scene itself lives in
/// [`DesktopLoop`]'s Vello twin; this is only the swapchain half
/// (mirrors `oppa-fps::driver::GpuState` minus the scene, which the
/// loop owns). The `ShellWindow` softbuffer stays alive beside it as
/// the loud CPU fallback.
struct LinuxGpu {
    surface: wgpu::Surface<'static>,
    mode: wgpu::PresentMode,
    name: String,
}

/// Ordered GPU backend attempts (mirrors the fps driver: desktop
/// probes Vulkan first — measured Vello bring-up ~2s vs ~6-16s Dx12
/// on Windows, and only Vulkan persists pipeline-cache data in
/// wgpu-hal 29; Linux tries the single Vulkan row).
fn backend_attempts() -> Vec<(&'static str, wgpu::Backends)> {
    vec![("vulkan", wgpu::Backends::VULKAN)]
}

fn cache_dir() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|b| std::path::PathBuf::from(b).join(".cache/oppa-app"))
}

/// Builds the GPU swapchain against `window`, or returns the loud
/// reason to fall back to CPU (mirrors
/// `oppa-fps::driver::FpsDriver::build_gpu`: one leaked instance per
/// attempt, pipeline-cache round-trip, present-mode pick, surface
/// configure, loop enable).
fn build_gpu(
    window: &winit::window::Window,
    display: raw_window_handle::RawDisplayHandle,
    loop_: &mut DesktopLoop,
    size: (u32, u32),
) -> Result<LinuxGpu, String> {
    let (w, h) = (size.0.max(1), size.1.max(1));
    for (name, backends) in backend_attempts() {
        let raw_window = window
            .window_handle()
            .map_err(|e| format!("window handle: {e:?}"))?
            .as_raw();
        // One leaked instance per attempt (the surface borrows it;
        // the winner lives for the process — the fps rule).
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
                return Ok(LinuxGpu {
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

/// Presents one CPU frame through softbuffer (the fallback half):
/// pixmap → XRGB words → surface. Transient failures log loudly and
/// re-arm a redraw (the Round 7.3 rule — a transient present error
/// must not kill the app); the caller decides fatality.
fn cpu_present_to_window(loop_: &DesktopLoop, window: &mut ShellWindow) {
    match loop_.rgba8() {
        Ok(rgba) => {
            if let Err(e) = window.present(&rgba) {
                eprintln!("oppa-app: present: {e}");
                window.window().request_redraw();
            }
        }
        Err(e) => {
            eprintln!("oppa-app: pixels: {e}");
        }
    }
}

/// Presents through the GPU swapchain with loud CPU fallback (Round
/// 7.4): `Outdated` reconfigures and retries once (the fps storm
/// rule, single-shot here — sustained storms reconfigure per frame
/// rather than sleeping the event loop); any other failure logs the
/// exact error and falls back to softbuffer for this frame (the next
/// frame retries GPU — no silent permanent downgrade).
fn gpu_present_or_cpu(
    loop_: &mut DesktopLoop,
    gpu: &LinuxGpu,
    window: &mut ShellWindow,
    size: (u32, u32),
) {
    match loop_.present_gpu(&gpu.surface) {
        Ok(_) => {}
        Err(e) if e.contains("Outdated") => {
            eprintln!("oppa-app: gpu present outdated ({e}); reconfiguring");
            match loop_.configure_gpu_surface(&gpu.surface, size.0, size.1, gpu.mode) {
                Ok(_) => {
                    if let Err(e2) = loop_.present_gpu(&gpu.surface) {
                        eprintln!("oppa-app: gpu retry failed: {e2}; CPU fallback");
                        cpu_present_to_window(loop_, window);
                    }
                }
                Err(ce) => {
                    eprintln!("oppa-app: gpu reconfigure failed: {ce}; CPU fallback");
                    cpu_present_to_window(loop_, window);
                }
            }
        }
        Err(e) => {
            eprintln!("oppa-app: gpu present failed: {e}; CPU fallback");
            cpu_present_to_window(loop_, window);
        }
    }
}

struct LinuxRunner {
    loop_: DesktopLoop,
    shell: LinuxShell,
    cursor_dp: (f32, f32),
    window: Option<ShellWindow>,
    gpu: Option<LinuxGpu>,
    title: String,
    /// OS theme watcher (Round 16.2, decision 315 — `None` until
    /// the runner starts it; the drain in `about_to_wait`
    /// re-queries through the installed source).
    theme_watcher: Option<LinuxThemeWatcher>,
    /// Runtime window chrome (Round 16.3, decision 316 — shares
    /// state with the loop-installed control; attached to the live
    /// window on open so pre-open calls still land).
    window_control: LinuxWindowControl,
}

impl LinuxRunner {
    /// Types one char into the focused session; `true` when it
    /// damaged (re-present). Backend failures exit loudly (a dead
    /// paint loop must not spin silently).
    fn type_text_fatal(&mut self, ch: char) -> bool {
        self.loop_.type_text(&ch.to_string()).unwrap_or_else(|e| {
            eprintln!("oppa-app: FATAL type: {e}");
            std::process::exit(1);
        }) > 0
    }

    fn backspace_fatal(&mut self) -> bool {
        self.loop_.backspace().unwrap_or_else(|e| {
            eprintln!("oppa-app: FATAL backspace: {e}");
            std::process::exit(1);
        }) > 0
    }

    fn delete_forward_fatal(&mut self) -> bool {
        self.loop_.delete_forward().unwrap_or_else(|e| {
            eprintln!("oppa-app: FATAL delete: {e}");
            std::process::exit(1);
        }) > 0
    }

    /// Routes one wheel tick to its scroll target; `true` when it
    /// damaged (re-present). Same fatal policy as the editing arms.
    fn scroll_fatal(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> bool {
        self.loop_.scroll_at(x, y, dx, dy).unwrap_or_else(|e| {
            eprintln!("oppa-app: FATAL scroll: {e}");
            std::process::exit(1);
        }) > 0
    }

    /// Monitor scale change into density + DPR + surface (Round
    /// 2.4, decision 259); `true` when it damaged (re-present).
    /// Same fatal policy as the editing arms. Physical size comes
    /// from the live window (the scale event carries none we use —
    /// `Resized` owns size, the writer stays untouched).
    fn density_fatal(&mut self, scale: f32) -> bool {
        let old = self.shell.density();
        self.shell.set_density(scale);
        // The tracked cursor was divided by the old density at move
        // time — rescale it so pre-move clicks don't misroute (the
        // old density is provably positive: the shell refuses
        // anything else at both boundaries).
        if old != scale {
            let k = scale / old;
            self.cursor_dp.0 *= k;
            self.cursor_dp.1 *= k;
        }
        self.loop_
            .set_device_pixel_ratio(scale)
            .unwrap_or_else(|e| {
                eprintln!("oppa-app: FATAL density dpr: {e}");
                std::process::exit(1);
            });
        let (w, h) = self
            .window
            .as_ref()
            .map(|window| window.size())
            .unwrap_or_else(|| self.loop_.viewport());
        if (w, h) != self.loop_.viewport() {
            self.loop_.resize(w, h).unwrap_or_else(|e| {
                eprintln!("oppa-app: FATAL density resize: {e}");
                std::process::exit(1);
            });
        }
        let damaged = self.loop_.repaint().unwrap_or_else(|e| {
            eprintln!("oppa-app: FATAL density repaint: {e}");
            std::process::exit(1);
        }) > 0;
        self.reconfigure_gpu_fatal();
        damaged
    }

    /// Reconfigures the GPU swapchain to the loop's live viewport
    /// (Round 7.4 — resize/DPR arms call this after refitting the
    /// scene; a reconfigure failure disables GPU loudly and keeps
    /// the CPU fallback, never a silent blank).
    fn reconfigure_gpu_fatal(&mut self) {
        if !self.loop_.is_gpu() {
            return;
        }
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let (w, h) = self.loop_.viewport();
        if let Err(e) = self
            .loop_
            .configure_gpu_surface(&gpu.surface, w, h, gpu.mode)
        {
            eprintln!("oppa-app: gpu reconfigure failed: {e}; CPU fallback");
            self.loop_.disable_gpu();
        }
    }

    /// IME preedit into the focused session (Round 2.1, decision
    /// 256); `true` when it damaged. Same fatal policy as the
    /// editing arms. The candidate anchor refreshes on every step.
    fn ime_preedit_fatal(&mut self, text: &str, caret: Option<usize>) -> bool {
        let damaged = self
            .loop_
            .feed_ime_preedit(text, caret)
            .unwrap_or_else(|e| {
                eprintln!("oppa-app: FATAL ime preedit: {e}");
                std::process::exit(1);
            })
            > 0;
        self.anchor_ime();
        damaged
    }

    /// IME commit into the focused session (Round 2.1); `true` when
    /// it damaged. Same fatal policy and anchor refresh as preedit.
    fn ime_commit_fatal(&mut self, text: &str) -> bool {
        let damaged = self.loop_.feed_ime_commit(text).unwrap_or_else(|e| {
            eprintln!("oppa-app: FATAL ime commit: {e}");
            std::process::exit(1);
        }) > 0;
        self.anchor_ime();
        damaged
    }

    /// IME cancel for the focused session (Round 2.1); `true` when
    /// it damaged. Same fatal policy and anchor refresh as preedit.
    fn ime_cancel_fatal(&mut self) -> bool {
        let damaged = self.loop_.feed_ime_cancel().unwrap_or_else(|e| {
            eprintln!("oppa-app: FATAL ime cancel: {e}");
            std::process::exit(1);
        }) > 0;
        self.anchor_ime();
        damaged
    }

    /// Candidate anchor after an IME step (Round 2.1): the host caret
    /// rect becomes the winit candidate cursor area (physical px —
    /// density is 1.0, so device px are physical px) plus the shell
    /// IME log record (diagnostics trail).
    fn anchor_ime(&mut self) {
        if let Some([x, y, w, h]) = self.loop_.ime_anchor() {
            if let Some(window) = self.window.as_ref() {
                window.window().set_ime_cursor_area(
                    winit::dpi::Position::Physical(winit::dpi::PhysicalPosition::new(
                        x as i32, y as i32,
                    )),
                    winit::dpi::Size::Physical(winit::dpi::PhysicalSize::new(
                        w.max(0.0) as u32,
                        h.max(0.0) as u32,
                    )),
                );
            }
            self.shell.set_ime(oppa::ime::ImeOps::SetCaretRect {
                x,
                y,
                width: w,
                height: h,
            });
        }
    }

    /// Escape-at-root exits; everything else flows to the pipeline.
    /// Returns true when the caller should exit the event loop.
    /// (The winit arm needs the `ActiveEventLoop` to exit, so the
    /// check lives here rather than in the shared core.)
    fn host_inject_escape_or_input(&mut self, ev: InputEvent) -> bool {
        if let InputEvent::Key {
            code,
            state: KeyState::Pressed,
            ..
        } = &ev
        {
            if self.loop_.escape_exits(*code, true) {
                return true;
            }
        }
        // Repaint errors are loud through `repaint` below; inject
        // itself settles inline.
        self.loop_.host().inject_input(ev);
        false
    }
}

impl winit::application::ApplicationHandler for LinuxRunner {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let (w, h) = self.loop_.viewport();
        let window = ShellWindow::open(event_loop, &self.title, w, h).unwrap_or_else(|e| {
            eprintln!("oppa-app: FATAL window open: {e}");
            std::process::exit(1);
        });
        // IME enablement (Round 2.1, decision 256): without this
        // winit never delivers Preedit/Commit (the old "the window
        // never enables IME" bound — now closed).
        window.window().set_ime_allowed(true);
        // Runtime window chrome (Round 16.3): attach the live window
        // so pre-open title/size calls stored in the shared control
        // apply now, then track live.
        self.window_control.attach(window.window_arc());
        self.window = Some(window);
        // GPU-first bring-up (Round 7.4, decision 279): attempt
        // hardware swapchains against the live window; any failure
        // logs loudly and keeps the softbuffer CPU path (never a
        // silent blank, never a fatal exit for a missing GPU).
        if gpu_disabled_by_env() {
            eprintln!("oppa-app: OPPA_RENDERER=cpu — CPU path (softbuffer)");
            if let Some(window) = self.window.as_ref() {
                window.window().request_redraw();
            }
        } else {
            match event_loop
                .owned_display_handle()
                .display_handle()
                .map(|h| h.as_raw())
            {
                Ok(display) => {
                    let size = self
                        .window
                        .as_ref()
                        .map(|w| w.size())
                        .unwrap_or_else(|| self.loop_.viewport());
                    let window_ref = self.window.as_ref().expect("window just opened").window();
                    match build_gpu(window_ref, display, &mut self.loop_, size) {
                        Ok(gpu) => {
                            eprintln!("oppa-app: GPU path ({} / {:?})", gpu.name, gpu.mode);
                            self.gpu = Some(gpu);
                            // Sync the Vello twin before the first
                            // present (creation-time scene is stale
                            // once GPU enables both-backend paints).
                            if let Err(e) = self.loop_.repaint() {
                                eprintln!("oppa-app: FATAL gpu warmup repaint: {e}");
                                std::process::exit(1);
                            }
                            if let Some(window) = self.window.as_ref() {
                                window.window().request_redraw();
                            }
                        }
                        Err(e) => {
                            eprintln!("oppa-app: {e}; CPU fallback");
                            if let Some(window) = self.window.as_ref() {
                                window.window().request_redraw();
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("oppa-app: display handle: {e:?}; CPU fallback");
                    if let Some(window) = self.window.as_ref() {
                        window.window().request_redraw();
                    }
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        use winit::event::WindowEvent;
        match event {
            // Close veto (Round 16.3, decision 316): the app-level
            // handler decides — a refusal keeps the event loop
            // running instead of exiting.
            WindowEvent::CloseRequested => {
                if self.loop_.close_requested() {
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(size) => {
                let (w, h) = (size.width.max(1), size.height.max(1));
                if let Some(window) = self.window.as_mut() {
                    window.resize(w, h).unwrap_or_else(|e| {
                        eprintln!("oppa-app: FATAL resize: {e}");
                        std::process::exit(1);
                    });
                }
                let density = self.shell.density();
                self.shell
                    .note_surface_changed(w as f32 / density, h as f32 / density);
                if let Err(e) = self.loop_.resize(w, h) {
                    eprintln!("oppa-app: FATAL loop resize: {e}");
                    std::process::exit(1);
                }
                if let Err(e) = self.loop_.repaint() {
                    eprintln!("oppa-app: FATAL repaint: {e}");
                    std::process::exit(1);
                }
                // GPU swapchain follows the live size (Round 7.4):
                // reconfigure to the LIVE size, never a stale
                // constant (the uncapped-blank class).
                self.reconfigure_gpu_fatal();
                if let Some(window) = self.window.as_ref() {
                    window.window().request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                // Wayland configure negotiation (Round 7.3): the
                // compositor may settle a different size than
                // requested — sync surface + scene before presenting.
                let live = self
                    .window
                    .as_ref()
                    .map(|w| w.size())
                    .unwrap_or_else(|| self.loop_.viewport());
                let (w, h) = (live.0.max(1), live.1.max(1));
                if (w, h) != self.loop_.viewport() {
                    if let Some(window) = self.window.as_mut() {
                        if let Err(e) = window.resize(w, h) {
                            eprintln!("oppa-app: window resize: {e}");
                        }
                    }
                    let density = self.shell.density();
                    self.shell
                        .note_surface_changed(w as f32 / density, h as f32 / density);
                    if let Err(e) = self.loop_.resize(w, h) {
                        eprintln!("oppa-app: loop resize: {e}");
                    }
                    if let Err(e) = self.loop_.repaint() {
                        eprintln!("oppa-app: repaint: {e}");
                    }
                    self.reconfigure_gpu_fatal();
                }
                // Present on the active backend (Round 7.4): GPU
                // swapchain primary with loud softbuffer fallback.
                if self.loop_.is_gpu() {
                    let size = self.loop_.viewport();
                    // Disjoint field borrows (loop + gpu + window)
                    // stay separate — no whole-`self` call while the
                    // window is held.
                    match (self.gpu.as_ref(), self.window.as_mut()) {
                        (Some(gpu), Some(window)) => {
                            let loop_ = &mut self.loop_;
                            gpu_present_or_cpu(loop_, gpu, window, size);
                        }
                        _ => {
                            eprintln!("oppa-app: gpu active without surface; CPU fallback");
                            if let Some(window) = self.window.as_mut() {
                                cpu_present_to_window(&self.loop_, window);
                            }
                        }
                    }
                } else if let Some(window) = self.window.as_mut() {
                    cpu_present_to_window(&self.loop_, window);
                }
            }
            ev => {
                if self.window.is_none() {
                    return;
                }
                // Escape-at-root is checked per command inside the
                // drive (it needs the loop's focus state); a true
                // result exits the loop.
                let density = self.shell.density();
                let mut exit = false;
                for shell_ev in translate(&ev, &mut self.cursor_dp, density) {
                    self.shell.push_event(shell_ev);
                }
                let _ = self.shell.pump_events();
                let modifiers = self.shell.modifiers();
                let mut changed = false;
                for cmd in self.shell.take_cmds() {
                    // Monitor scale (Round 2.4, decision 259):
                    // runner-matched like Char/Scroll/Ime (panics in
                    // `to_input_event` by design) — density, DPR, and
                    // surface re-base together, then the repaint below
                    // presents crisply. The tracked cursor rescales
                    // (it was divided by the old density at move
                    // time); later commands in this batch still
                    // classified under the old density is accepted
                    // (scale events are rare — stated, not silent).
                    if let LinuxCmd::Density { scale } = &cmd {
                        changed |= self.density_fatal(*scale);
                        continue;
                    }
                    // IME composition (Round 2.1, decision 256):
                    // runner-matched like Char/Scroll (all three panic
                    // in `to_input_event` by design) — preedit/commit/
                    // cancel feed the focused session, then the
                    // candidate anchor refreshes. Borrow patterns
                    // throughout (the shell enums are Clone, not Copy
                    // — see `LinuxEvent`).
                    if let LinuxCmd::ImePreedit { text, cursor } = &cmd {
                        changed |= self.ime_preedit_fatal(text, cursor.as_ref().map(|c| c.1));
                        continue;
                    }
                    if let LinuxCmd::ImeCommit { text } = &cmd {
                        changed |= self.ime_commit_fatal(text);
                        continue;
                    }
                    if matches!(cmd, LinuxCmd::ImeCancel) {
                        changed |= self.ime_cancel_fatal();
                        continue;
                    }
                    // Char carries no `InputEvent` mapping (loud panic
                    // in `to_input_event`) — matched first and typed
                    // into the focused session. Editing keys are
                    // consumed the same way (never also injected);
                    // Ctrl+letter shortcut Keys likewise route through
                    // `step`'s interception (decision 246).
                    if let LinuxCmd::Char { ch } = &cmd {
                        // Shortcut shadows carry no text (decision
                        // 246): under the editing modifiers the Key
                        // arm runs the shortcut, so typing this `Char`
                        // would double-enter (Windows never gets here
                        // — its shadows are control chars the `Char`
                        // filter drops). Same predicate as the `step`
                        // interception; one acknowledged race: a
                        // `ModifiersChanged` landing after the press
                        // slips one char through (winit orders it
                        // first in practice).
                        if is_edit_shortcut_modifier(modifiers) {
                            continue;
                        }
                        changed |= self.type_text_fatal(*ch);
                        continue;
                    }
                    // Wheel scroll carries its position (decision 250):
                    // hit-test dispatch through the loop, never the
                    // generic `to_input_event` path (which refuses it
                    // loudly by design — same class as `Char` above).
                    if let LinuxCmd::Scroll { x, y, dx, dy } = &cmd {
                        changed |= self.scroll_fatal(*x, *y, *dx, *dy);
                        continue;
                    }
                    if let LinuxCmd::Key { code, pressed } = &cmd {
                        if *pressed && *code == oppa::input::keys::BACKSPACE {
                            changed |= self.backspace_fatal();
                            continue;
                        }
                        if *pressed && *code == oppa::input::keys::DELETE {
                            changed |= self.delete_forward_fatal();
                            continue;
                        }
                    }
                    let event = cmd.to_input_event(modifiers);
                    if self.host_inject_escape_or_input(event) {
                        exit = true;
                        break;
                    }
                    changed = true;
                }
                for err in self.shell.take_errors() {
                    eprintln!("oppa-app: shell drain: {err:?}");
                }
                if exit {
                    event_loop.exit();
                    return;
                }
                if changed {
                    self.loop_.host().run_until_idle();
                    if self.loop_.repaint().is_err() {
                        // `repaint` already formats the reason; a dead
                        // paint loop must not spin silently.
                        eprintln!("oppa-app: FATAL repaint failed");
                        std::process::exit(1);
                    }
                    if let Some(window) = self.window.as_ref() {
                        // Round 8.3: hover moved — publish the hovered
                        // style (unstyled falls back to the arrow).
                        window.set_cursor(
                            self.loop_
                                .host()
                                .hover_cursor()
                                .unwrap_or(oppa::CursorIcon::Default),
                        );
                        window.window().request_redraw();
                    }
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        // Round 16.2 (decision 315): OS theme flips arrive on the
        // watcher thread — drain here and re-query through the
        // installed source (one application path, never two). Fatal
        // repaints exit exactly like the blink path above.
        if self.theme_watcher.as_mut().is_some_and(|w| w.poll_change()) {
            match self.loop_.sync_system_theme() {
                Ok(true) => {
                    if let Some(window) = self.window.as_ref() {
                        window.window().request_redraw();
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    eprintln!("oppa-app: FATAL theme repaint failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        // Round 15.2 (decision 313): caret blink — the flip writes no
        // signals, so no input ever wakes it: repaint here when the
        // overlay flipped (and redraw-present it below) and wake at
        // the next flip. A fatal repaint exits exactly like the input
        // path's FATAL repaint (never a silent stuck bar).
        match self.loop_.poll_blink() {
            Ok(true) => {
                if let Some(window) = self.window.as_ref() {
                    window.window().request_redraw();
                }
            }
            Ok(false) => {}
            Err(e) => {
                eprintln!("oppa-app: FATAL blink repaint failed: {e}");
                std::process::exit(1);
            }
        }
        // Round 21.1 (decision 328): component timers fire FIRST —
        // a callback may create blink demand below, so the wakes
        // compute after (redraw when fired; a fatal repaint exits
        // exactly like the blink path above).
        match self.loop_.tick_timers() {
            Ok(0) => {}
            Ok(_) => {
                if let Some(window) = self.window.as_ref() {
                    window.window().request_redraw();
                }
            }
            Err(e) => {
                eprintln!("oppa-app: FATAL timer repaint failed: {e}");
                std::process::exit(1);
            }
        }
        // Component-requested close + runner poll (Round 26.2,
        // decision 342 -- the Windows-pump twin): poll first (it may
        // write files and ask for close), then a drained request
        // re-enters the veto consult exactly like CloseRequested.
        // No runner borrows span the hook (take-call-restore inside).
        self.loop_.run_poll_hook();
        if self.loop_.host().take_close_request() && self.loop_.close_requested() {
            event_loop.exit();
        }
        // X11 selection serving (Round 2.2, OQ-G3-1): answer peer
        // requests without blocking; while our clipboard owns a
        // servable selection, wake periodically so another app's
        // paste never hangs on us (100 ms bounds the added latency;
        // idle otherwise — the loop stays event-driven). The blink
        // deadline joins in (earliest wake wins — a focused caret
        // blinks without input, an unfocused loop still sleeps).
        let now = std::time::Instant::now();
        let mut wake = None;
        if self.loop_.clipboard().service() {
            wake = Some(now + std::time::Duration::from_millis(100));
        }
        if let Some(secs) = self.loop_.blink_tick_in_secs() {
            let tick = now + std::time::Duration::from_secs_f64(secs.max(0.0));
            wake = Some(wake.map_or(tick, |w| w.min(tick)));
        }
        // The timer horizon joins last (fresh — callbacks above may
        // have re-armed): host-clock ms → Instant delta, earliest
        // wake wins with blink + clipboard service.
        if let Some(due_ms) = self.loop_.next_timer_due_ms() {
            let delta_ms = (due_ms - self.loop_.host().now_ms()).max(0.0);
            let tick = now + std::time::Duration::from_secs_f64(delta_ms / 1000.0);
            wake = Some(wake.map_or(tick, |w| w.min(tick)));
        }
        match wake {
            Some(t) => event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(t)),
            None => {
                // Idle until the next OS event (no timers: the loop is purely
                // event-driven — repaints happen on input, presents on
                // redraw requests).
                event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
            }
        }
    }
}

/// Detects whether running under WSLg (Windows Subsystem for Linux GUI).
fn is_wslg() -> bool {
    std::path::Path::new("/mnt/wslg").exists()
        || std::env::var("WSL_DISTRO_NAME").is_ok()
        || std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|s| s.to_ascii_lowercase().contains("microsoft"))
            .unwrap_or(false)
}

pub fn run_linux<P: Props>(
    options: WindowOptions,
    props: P,
    component: fn(&Ctx, &P) -> VNode,
    configure: impl FnOnce(&mut DesktopLoop),
) -> Result<(), String> {
    // Under WSLg, the Weston RDP-backend compositor segfaults in
    // libpixman-1 on sustained Wayland SHM presents (microsoft/wslg#1386,
    // decision 185), crashing the compositor and dropping client
    // connections with EPIPE / ExitFailure(1). Preferring X11 (via
    // Xwayland, which presents via internal EGL) completely avoids
    // the Weston SHM bug, stays 100% stable, and matches the X11
    // clipboard backend below.
    if is_wslg()
        && std::env::var("DISPLAY").is_err()
        && std::path::Path::new("/tmp/.X11-unix/X0").exists()
    {
        std::env::set_var("DISPLAY", ":0");
    }

    // System text service first (loud when the system has no fonts —
    // the demo's policy, kept: a window without text is a lie).
    let (texts, skipped) =
        LinuxTextService::system().map_err(|e| format!("oppa-app: linux fonts: {e}"))?;
    if !skipped.is_empty() {
        eprintln!("oppa-app: skipped font files: {skipped:?}");
    }
    let families = texts.families().to_vec();
    let family = ["DejaVu Sans", "Ubuntu", "Noto Sans"]
        .into_iter()
        .find(|f| families.iter().any(|g| g == f))
        .ok_or_else(|| "oppa-app: no latin family on system".to_string())?
        .to_string();

    let all_font_ids = texts.all_font_ids();
    let mut font_pairs = Vec::with_capacity(all_font_ids.len());
    for id in all_font_ids {
        if let Some((bytes, index)) = texts.face_bytes(id) {
            font_pairs.push((id, bytes.to_vec(), index));
        }
    }

    let mut runner = LinuxRunner {
        loop_: DesktopLoop::new(options.width, options.height, Box::new(texts), &family)?,
        shell: LinuxShell::new(ShellConfig {
            title: options.title.clone(),
            density: 1.0,
            width_dp: options.width as f32,
            height_dp: options.height as f32,
        }),
        cursor_dp: (0.0, 0.0),
        window: None,
        gpu: None,
        title: options.title.clone(),
        theme_watcher: None,
        window_control: LinuxWindowControl::new(),
    };

    // Inject raster faces for all system font ids (decision 278, closes
    // the decision-277 Linux follow-up; Round 7.4 lands them in BOTH
    // backends via `DesktopLoop::set_font_for`): bold text and chevrons
    // rasterize real glyph outlines on the CPU pixmap and on the Vello
    // twin instead of advance-cell bars / loud-no-face refusals.
    for (id, bytes, index) in font_pairs {
        runner.loop_.set_font_for(id, bytes, index);
    }

    // Real OS clipboard (Round 2.2, OQ-G3-1 — closes the decision-246
    // session-local bound): fails loudly without an X display, in
    // which case the loop's session-local clipboard stands in with a
    // one-time note (clipboard is display-bound; without X there is
    // no system clipboard to wire — the TSF best-effort precedent,
    // decision 256, stated not silent). The loop owns the single
    // stateful backend (X11 ownership cannot be dual-owned like the
    // stateless Win32 handle — no shell seam lending); the per-frame
    // peer serving rides `about_to_wait` above.
    match LinuxClipboard::new() {
        Ok(clip) => runner.loop_.set_clipboard(Box::new(clip)),
        Err(e) => eprintln!("oppa-app: linux clipboard unavailable: {e} (in-app only)"),
    }
    // Native save + folder pickers (Round 16.1, decision 314 —
    // portal `SaveFile` / `OpenFile(directory=true)` with zenity
    // fallback; one instance per slot — workers spawn lazily, so an
    // unused slot costs no thread).
    runner
        .loop_
        .set_save_dialog(Box::new(LinuxFileDialog::new(StdRunner)));
    runner
        .loop_
        .set_folder_dialog(Box::new(LinuxFileDialog::new(StdRunner)));
    // OS theme at startup (Round 16.2, decision 315): install the
    // portal reader and sync before mount so the first paint already
    // matches the system (unreadable stays a quiet no-op, never a
    // repaint spin); the watcher below covers live flips.
    runner.loop_.set_theme_source(Box::new(LinuxSystemTheme));
    let _ = runner.loop_.sync_system_theme()?;
    runner.theme_watcher = Some(LinuxThemeWatcher::new());
    // Runtime window chrome (Round 16.3, decision 316 — the loop
    // forwards into the shared control; the runner attaches the
    // live window on open below).
    runner
        .loop_
        .set_window_control(Box::new(runner.window_control.clone()));
    runner.loop_.mount("App", props, component);
    // App ownership (Round 25.3, decision 338): after mount (host
    // signals exist) and after the platform backend installs above
    // (app configuration wins), before the warmup paint below.
    configure(&mut runner.loop_);
    // Warmup paint before the window exists (first redraw presents
    // real content — the demo's post-configure rule).
    runner.loop_.repaint()?;

    let mut builder = winit::event_loop::EventLoop::builder();
    // WSLg routing (Round 7.3, still the CPU rule): Wayland SHM
    // presents crash Weston's RDP compositor in libpixman
    // (microsoft/wslg#1386), so default to X11 (Xwayland EGL) unless
    // the caller overrides. Round 7.4 GPU Wayland is stable by
    // construction (hardware dmabuf swapchains never touch SHM), so
    // an explicit `WINIT_UNIX_BACKEND=wayland` reaches the GPU path
    // above and presents stably — the verification matrix proves
    // both (`wayland` + GPU, `cpu` + X11).
    if is_wslg() && std::env::var("WINIT_UNIX_BACKEND").is_err() {
        use winit::platform::x11::EventLoopBuilderExtX11;
        builder.with_x11();
    }
    let event_loop = builder
        .build()
        .map_err(|e| format!("oppa-app: event loop: {e:?}"))?;
    event_loop
        .run_app(&mut runner)
        .map_err(|e| format!("oppa-app: event loop failed: {e:?}"))?;
    Ok(())
}
