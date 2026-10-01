//! Linux demo (v1 remainder, Gap 4): one window, one toggle
//! scene — CPU fallback paints through softbuffer while the Linux
//! text slice measures the corpus.
//!
//! Run where a display server exists (WSLg/X11/Wayland):
//! `cargo run -p oppa-shell-linux --example linux_demo`.
//! Prints the run record (faces, shapes, presents, timings) and
//! exits 0; any failure is loud (nonzero exit + reason).
//!
//! Acceptance: window opens, text measures, CPU fallback paints.

use std::time::{Duration, Instant};

use oppa::text::{TextService, TextStyle};
use oppa::{Color, ComponentHost, Ctx, RendererBackend, Semantics, Style, VNode};
use oppa_cpu::{CpuBackend, FramePlanBuilder};
use oppa_shell_linux::{pack_rgba_to_xrgb, ShellWindow};
use oppa_shell_linux::{translate, LinuxShell, ShellConfig};
use oppa_text_linux::LinuxTextService;

const W: f32 = 800.0;
const H: f32 = 600.0;

fn scene(ctx: &Ctx, _props: &()) -> VNode {
    let is_on = ctx.signal(false);
    let bg = if is_on.get() {
        Color(0x44_44_44)
    } else {
        Color(0x55_55_55)
    };
    let s = is_on.clone();
    oppa::Div("screen")
        .style(Style::new().size(W, H).bg(Color(0xFF_FF_FF)))
        .child(
            oppa::Div("toggle")
                .style(Style::new().size(44, 24).bg(bg))
                .semantics(Semantics::switch().checked(is_on.get()).label("Wi-Fi"))
                .on_press(move || s.set(!s.get()))
                .build(),
        )
}

/// Straightens a tiny-skia pixmap to raw RGBA8 (the m10 conversion).
fn straight_rgba(px: &tiny_skia::Pixmap) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(px.width() as usize * px.height() as usize * 4);
    for p in px.pixels() {
        if p.alpha() != 255 {
            return Err("non-opaque pixel".to_string());
        }
        out.push(p.red());
        out.push(p.green());
        out.push(p.blue());
        out.push(255);
    }
    Ok(out)
}

struct Demo {
    host: ComponentHost,
    builder: FramePlanBuilder,
    cpu: CpuBackend,
    csurf: oppa::SurfaceId,
    shell: LinuxShell,
    cursor_dp: (f32, f32),
    rgba: Vec<u8>,
    window: Option<ShellWindow>,
    frames: u32,
    presents: u32,
    started: Instant,
    text_record: String,
}

impl Demo {
    fn new() -> Result<Self, String> {
        // Text measures first (loud when the system has no fonts).
        let t = Instant::now();
        let (texts, skipped) =
            LinuxTextService::system().map_err(|e| format!("linux fonts: {e}"))?;
        if !skipped.is_empty() {
            return Err(format!("skipped faces: {skipped:?}"));
        }
        let families = texts.families().to_vec();
        let family = ["DejaVu Sans", "Ubuntu", "Noto Sans"]
            .into_iter()
            .find(|f| families.iter().any(|g| g == f))
            .ok_or("no latin family on system")?;
        let corpus = ["Hi", "Hello world", "W"];
        let mut shapes = Vec::new();
        for text in corpus {
            let run = texts
                .shape(text, &TextStyle::new(family, 16.0))
                .map_err(|e| format!("shape {text:?}: {e:?}"))?;
            if run.glyphs.is_empty() || run.total_advance <= 0.0 {
                return Err(format!("degenerate shape for {text:?}"));
            }
            shapes.push(format!(
                "{text}={}g/{:.1}px",
                run.glyphs.len(),
                run.total_advance
            ));
        }
        let text_ms = t.elapsed().as_secs_f64() * 1000.0;
        let text_record = format!(
            "text family={family} faces={} shapes=[{}] skipped=0 load_shape_ms={text_ms:.0}",
            texts.enumerate_fonts().len(),
            shapes.join(" "),
        );

        // Scene + CPU paint (the fallback arm).
        let t = Instant::now();
        let host = ComponentHost::new();
        host.set_viewport(W, H);
        host.mount("LinuxScene", (), scene);
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        let mut cpu = CpuBackend::new();
        let csurf = cpu
            .create_surface(oppa::SurfaceDesc {
                width_px: W as u32,
                height_px: H as u32,
                background: Color(0xFF_FF_FF),
            })
            .map_err(|e| format!("cpu surface: {e:?}"))?;
        for d in host.diffs_from(0) {
            cpu.commit(&d).map_err(|e| format!("cpu commit: {e:?}"))?;
        }
        let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
        cpu.paint(csurf, &plan)
            .map_err(|e| format!("cpu paint: {e:?}"))?;
        let rgba = straight_rgba(cpu.pixmap(csurf).ok_or("cpu pixmap missing")?)?;
        // The pack path is validated here too (loud on mismatch).
        let words = pack_rgba_to_xrgb(&rgba, W as u32, H as u32)?;
        let paint_ms = t.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "linux-demo: {text_record} cpu_paint_ms={paint_ms:.0} words={}",
            words.len()
        );
        Ok(Self {
            host,
            builder,
            cpu,
            csurf,
            // Shell intake starts unconfigured (density follows
            // the real window scale at open); input flows once a
            // window exists.
            shell: LinuxShell::new(ShellConfig {
                title: "oppa".to_string(),
                density: 1.0,
                width_dp: W,
                height_dp: H,
            }),
            cursor_dp: (0.0, 0.0),
            rgba,
            window: None,
            frames: 0,
            presents: 0,
            started: Instant::now(),
            text_record,
        })
    }

    /// Injects one winit event batch: translate → shell → host →
    /// repaint. Returns true when the scene changed (re-present).
    fn drive_input(&mut self, event: &winit::event::WindowEvent) -> bool {
        use oppa::shell::PlatformShell;
        let density = self.shell.density();
        for ev in translate(event, &mut self.cursor_dp, density) {
            self.shell.push_event(ev);
        }
        let _ = self.shell.pump_events();
        let modifiers = self.shell.modifiers();
        let mut changed = false;
        for cmd in self.shell.take_cmds() {
            // Char has no `InputEvent` mapping by design (it types via
            // the runner's focused session; this demo has no fields) —
            // skip explicitly, never through `to_input_event` (loud).
            if matches!(cmd, oppa_shell_linux::LinuxCmd::Char { .. }) {
                continue;
            }
            self.host.inject_input(cmd.to_input_event(modifiers));
            changed = true;
        }
        for err in self.shell.take_errors() {
            eprintln!("linux-demo: shell drain: {err:?}");
        }
        if !changed {
            return false;
        }
        self.host.run_until_idle();
        let plan = self
            .host
            .with_retained_mut(|rec, styles| self.builder.build_full(rec, styles));
        if self.cpu.paint(self.csurf, &plan).is_err() {
            return false;
        }
        match self.cpu.pixmap(self.csurf) {
            Some(px) => match straight_rgba(px) {
                Ok(rgba) => {
                    self.rgba = rgba;
                    true
                }
                Err(_) => false,
            },
            None => false,
        }
    }
}

impl winit::application::ApplicationHandler for Demo {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        // Open only — the first present waits for the compositor's
        // configure (Resized): committing before the ack is a
        // protocol violation the compositor punishes by dropping
        // the client (diagnostic finding on WSLg Weston).
        let window = ShellWindow::open(event_loop, "oppa — linux demo", W as u32, H as u32)
            .expect("window opens");
        // Shell density follows the real window scale (winit
        // reports it; intake dp math depends on it).
        let scale = window.window().scale_factor() as f32;
        let (pw, ph) = window.size();
        self.shell = LinuxShell::new(ShellConfig {
            title: "oppa".to_string(),
            density: scale,
            width_dp: pw as f32 / scale,
            height_dp: ph as f32 / scale,
        });
        eprintln!("linux-demo: window opened, awaiting configure");
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        use winit::event::WindowEvent;
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                // The compositor owns the final size (Wayland
                // configure); the surface must follow or presents
                // fail the protocol and the client is dropped.
                // Gated by OPPA_DEMO_RESIZE for the failure matrix
                // (unset = log only).
                eprintln!("linux-demo: resized to {}x{}", size.width, size.height);
                if std::env::var("OPPA_DEMO_RESIZE").as_deref() == Ok("1") {
                    if let Some(window) = self.window.as_mut() {
                        if let Err(e) = window.resize(size.width.max(1), size.height.max(1)) {
                            eprintln!("linux-demo: FATAL resize: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                // First paint is requested here (post-configure):
                // presenting inside `resumed` races the configure
                // ack, so the redraw (served after winit acks) is
                // the first legal commit point.
                if let Some(window) = self.window.as_ref() {
                    let density = self.shell.density();
                    self.shell.note_surface_changed(
                        size.width as f32 / density,
                        size.height as f32 / density,
                    );
                    window.window().request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(window) = self.window.as_mut() {
                    match window.present(&self.rgba) {
                        Ok(info) => {
                            self.presents += 1;
                            eprintln!(
                                "linux-demo: presented {}x{} (present #{})",
                                info.width, info.height, self.presents
                            );
                        }
                        Err(e) => {
                            eprintln!("linux-demo: FATAL present: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                self.frames += 1;
            }
            // Input flows through the shell mapping into the shared
            // pipeline; a changed scene repaints and re-presents.
            ev => {
                if self.window.is_some() && self.drive_input(&ev) {
                    if let Some(window) = self.window.as_ref() {
                        window.window().request_redraw();
                    }
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        // Bounded run: exit 0 after ~8 s. Sustained mode re-presents
        // on a 1 Hz timer (OPPA_DEMO_MODE=sustained); once-mode
        // idles after the first present. (Sustained runs die when
        // WSLg's Weston itself segfaults in libpixman — compositor
        // bug, not this client; see rounds.md usability entry. Not
        // acceptance.)
        let sustained = std::env::var("OPPA_DEMO_MODE").as_deref() == Ok("sustained");
        if self.started.elapsed() > Duration::from_secs(8) {
            eprintln!(
                "linux-demo: {} frames={} presents={} elapsed_s={:.1}",
                self.text_record,
                self.frames,
                self.presents,
                self.started.elapsed().as_secs_f64()
            );
            event_loop.exit();
            return;
        }
        if sustained {
            if let Some(window) = self.window.as_ref() {
                window.window().request_redraw();
            }
        }
        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
            Instant::now() + Duration::from_secs(1),
        ));
    }
}

fn main() {
    let event_loop = winit::event_loop::EventLoop::new().expect("event loop");
    let mut demo = Demo::new().unwrap_or_else(|e| {
        eprintln!("linux-demo: FATAL: {e}");
        std::process::exit(1);
    });
    event_loop.run_app(&mut demo).unwrap_or_else(|e| {
        eprintln!("linux-demo: event loop failed: {e:?}");
        std::process::exit(1);
    });
}
