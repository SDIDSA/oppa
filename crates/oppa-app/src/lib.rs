//! Desktop application runner (decision 242): window + event/paint
//! loop over a [`ComponentHost`] scene, so running a visual UI is one
//! call instead of ~150 lines of platform boilerplate.
//!
//! Shape (mirrors the proven `oppa-fps` split): this crate owns the
//! platform-free [`DesktopLoop`] — host, mount, input-inject, repaint
//! through the GPU-first dual engine (Round 7.4, decision 279:
//! [`VelloBackend`] hardware swapchains primary, [`CpuBackend`]
//! fallback) — plus the per-platform event/present glue
//! (`windows` / `linux` modules, one compiled per target). Text
//! shaping stays platform-native (DirectWrite on Windows, the Linux
//! system service on Linux); pixels ride the active backend (GPU
//! scene via `wgpu`, CPU pixmap via `rgba8` for GDI / softbuffer).
//!
//! Input scope (decision 243 adds typing, 246 the editing
//! shortcuts): pointer + keys + text. Typed characters, Backspace,
//! and Delete route to the focused field's session
//! (`focused_field_session` — no binding needed; the session owns the
//! value signal), as do the Ctrl+letter editing shortcuts
//! (select-all / undo / redo / copy / cut / paste), intercepted in
//! [`DesktopLoop::step`] so both platform arms share one path. IME
//! composition delivery is wired (Round 2.1, decision 256): the
//! Windows runner maps `WM_IME_*` snapshots and the Linux runner maps
//! winit `Ime` events into normalized [`ImeCompositionEvent`]s fed
//! through [`DesktopLoop::feed_ime`] (plus the preedit/commit/cancel
//! halves), and the candidate anchor rides
//! [`DesktopLoop::ime_anchor`] after every IME step.

use std::path::PathBuf;
use std::rc::Rc;

use oppa::input::keys;
use oppa::text::TextService;
use oppa::{
    dispatch_ime_event, Clipboard, Clock, ComponentHost, Ctx, FileDialogOptions, FolderDialog,
    FolderDialogOptions, FramePlan, ImeCompositionEvent, InMemoryClipboard, InputEvent, KeyState,
    LayoutTextConfig, Modifiers, MountHandle, PasteOutcome, Props, RendererBackend, SaveFileDialog,
    SurfaceDesc, SurfaceId, SystemClock, SystemThemeSource, ThemeTokens, VNode, WindowControl,
    WindowIcon,
};
use oppa_cpu::{CpuBackend, FramePlanBuilder};
use oppa_gpu::CustomGpuBackend;
use oppa_vello::VelloBackend;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

/// Window launch parameters.
#[derive(Clone, Debug)]
pub struct WindowOptions {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl WindowOptions {
    pub fn new(title: &str, width: u32, height: u32) -> Self {
        Self {
            title: title.to_string(),
            width: width.max(1),
            height: height.max(1),
        }
    }
}

/// Launches a desktop app: opens the window, mounts `component`, and
/// runs the event/paint loop until the window closes (or Escape exits
/// at root). Clean return is exit code 0; every failure is a loud
/// `Err` (never a silent blank window).
///
/// `component` is a plain `fn` pointer (not a generic closure): the
/// mount contract takes a `fn`, so a generic `Fn` bound could not be
/// passed through — stated, not silent.
///
/// Needs loop ownership (close veto, custom dialogs, theme source)?
/// Use [`run_desktop_with`] — this is exactly that with a no-op hook.
pub fn run_desktop<P: Props>(
    options: WindowOptions,
    props: P,
    component: fn(&Ctx, &P) -> VNode,
) -> Result<(), String> {
    run_desktop_with(options, props, component, |_| {})
}

/// Launches a desktop app with loop ownership (Round 25.3, decision
/// 338): like [`run_desktop`], but `configure` runs against the live
/// [`DesktopLoop`] after mount and before the event pump — install a
/// close handler (`set_close_handler`), swap dialog backends, or
/// override the theme source there. Runs after the platform's own
/// backend installs (clipboard, native dialogs, OS theme), so app
/// configuration wins; runs after mount, so `loop_.host()` signals
/// already exist for handler closures to capture.
pub fn run_desktop_with<P: Props>(
    options: WindowOptions,
    props: P,
    component: fn(&Ctx, &P) -> VNode,
    configure: impl FnOnce(&mut DesktopLoop),
) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows::run_windows(options, props, component, configure)
    }
    #[cfg(target_os = "linux")]
    {
        linux::run_linux(options, props, component, configure)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (options, props, component, configure);
        Err("run_desktop: no desktop shell on this target (Windows/Linux only)".to_string())
    }
}

/// Platform-free scene driver: host + dual paint surfaces + repaint
/// (Round 7.4, decision 279 — the `oppa-fps` GPU-first pattern:
/// [`VelloBackend`] hardware scene primary, [`CpuBackend`] pixmap
/// fallback). The platform glue owns the window (and the `wgpu`
/// swapchain surface when GPU is up) and feeds shell input through
/// [`DesktopLoop::step`]; everything here runs headless (the unit
/// tests below prove init, mount, input, damage, and pixels without
/// a window).
/// Ctrl held without alt or meta: the editing-shortcut modifier
/// shape (decision 246 — the AltGr guard, see
/// [`DesktopLoop::step`]). Shared with the Linux runner's `Char`
/// skip so both arms use one predicate.
pub(crate) fn is_edit_shortcut_modifier(modifiers: Modifiers) -> bool {
    modifiers.ctrl && !modifiers.alt && !modifiers.meta
}

/// The six intercepted codes (decision 246).
fn is_edit_shortcut_code(code: u32) -> bool {
    matches!(
        code,
        keys::A | keys::Z | keys::Y | keys::C | keys::X | keys::V
    )
}

/// Which scene rasterizer the loop presents (Round 7.4, decision
/// 279 — mirrors `oppa-fps::driver::RenderState`): hardware Vello
/// swapchains primary, CPU pixmap fallback. The loop always owns
/// both scene surfaces; `active` selects which one the platform
/// glue presents (GPU via `present_gpu`, CPU via `rgba8` for GDI /
/// softbuffer). Headless loops stay [`RendererKind::Cpu`] until a
/// runner enables GPU against a live window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RendererKind {
    Cpu,
    Gpu,
    CustomGpu,
}

/// Parses one `OPPA_RENDERER` value (pure — headless-testable):
/// `cpu` forces the softbuffer/GDI path, `gpu` requests hardware.
/// Anything else (including `None`) is auto (GPU-first with loud
/// CPU fallback). Case-insensitive; surrounding whitespace ignored.
pub fn parse_renderer_override(value: Option<&str>) -> Option<RendererKind> {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("cpu") => Some(RendererKind::Cpu),
        Some("gpu") => Some(RendererKind::Gpu),
        Some("custom_gpu") | Some("custom-gpu") => Some(RendererKind::CustomGpu),
        _ => None,
    }
}

/// Reads `OPPA_RENDERER` through [`parse_renderer_override`].
/// Unrecognized non-empty values log loudly once and resolve to
/// auto (never a silent reinterpretation).
pub fn renderer_override() -> Option<RendererKind> {
    let raw = std::env::var("OPPA_RENDERER").ok();
    let parsed = parse_renderer_override(raw.as_deref());
    if let Some(ref v) = raw {
        if !v.trim().is_empty() && parsed.is_none() {
            eprintln!(
                "oppa-app: OPPA_RENDERER={v:?} unrecognized (expected cpu|gpu) — auto (GPU-first, CPU fallback)"
            );
        }
    }
    parsed
}

/// True when the environment forces the CPU path
/// (`OPPA_RENDERER=cpu`): runners skip GPU bring-up entirely.
pub fn gpu_disabled_by_env() -> bool {
    renderer_override() == Some(RendererKind::Cpu)
}

/// True when the environment requests hardware
/// (`OPPA_RENDERER=gpu`): runners attempt GPU and fall back loudly
/// when it refuses (never a silent CPU substitution).
pub fn gpu_forced_by_env() -> bool {
    renderer_override() == Some(RendererKind::Gpu)
}

/// Poll hook callback for [`DesktopLoop`] pump iterations (Round 26.2, decision 342).
pub type DesktopPollHook = Box<dyn FnMut(&mut DesktopLoop)>;

pub struct DesktopLoop {
    host: ComponentHost,
    cpu: CpuBackend,
    surface: SurfaceId,
    /// Hardware scene twin (Round 7.4): same frame plans encode here
    /// whenever GPU is active (headless encoding needs no device —
    /// only `present_gpu` needs the swapchain context). Always
    /// created beside the CPU surface so enabling GPU never rebuilds
    /// the scene; CPU-only repaints leave it untouched (a Text run
    /// without an injected face would refuse loudly there while the
    /// CPU path still draws bars — the decision-200 contract).
    vello: VelloBackend,
    vello_surface: SurfaceId,
    custom_gpu: CustomGpuBackend,
    custom_gpu_surface: SurfaceId,
    /// Which backend the platform glue presents. CPU until a runner
    /// enables GPU against a live window; runners flip it back on
    /// loud GPU failure (fallback, never silent).
    active: RendererKind,
    /// Swapchain present mode once GPU is up (mirrors the fps
    /// driver's `GpuState.mode` — Immediate first, then Mailbox,
    /// then Fifo). `None` while CPU-only.
    gpu_mode: Option<wgpu::PresentMode>,
    builder: FramePlanBuilder,
    /// Paint surface size in device px (physical pixels).
    viewport: (u32, u32),
    /// Output scale: device px per CSS px (Round 2.4, OQ-G10-2).
    /// The host viewport, layout config, and builder all derive
    /// from (`viewport`, `dpr`) — never set one without the others
    /// (see `resize` / `set_device_pixel_ratio`).
    dpr: f32,
    /// The clipboard the editing shortcuts read and write (decision
    /// 246). Session-local memory by default (headless-correct — the
    /// unit tests below copy/paste with zero setup); the platform
    /// glue replaces it with the OS backend where one exists
    /// (`run_windows` installs the Win32 clipboard, `run_linux` the
    /// X11 one — copy/paste then work system-wide, stated in the
    /// round logs).
    clipboard: Box<dyn Clipboard>,
    /// The last committed frame plan (Round 15.2, decision 313 —
    /// test/presenter observability: proves which overlay `Rect`s a
    /// repaint committed; one retained plan, refreshed every
    /// repaint, never a growing history).
    last_plan: Option<FramePlan>,
    /// Caret overlay presence at the last repaint (Round 15.2,
    /// decision 313 — the blink demand: the flip writes no signals,
    /// so the tick path compares presence instead of waiting for
    /// dirt that never comes).
    last_caret_shown: bool,
    /// Save-dialog backend (Round 16.1, decision 314): `None` until
    /// a runner installs the OS backend (headless stays graceful —
    /// [`DesktopLoop::save_file_dialog`] answers `None` with no
    /// backend, never a panic).
    save_dialog: Option<Box<dyn SaveFileDialog>>,
    /// Folder-picker backend (Round 16.1, decision 314 — same
    /// install/graceful rule as the save slot).
    folder_dialog: Option<Box<dyn FolderDialog>>,
    /// OS theme reader (Round 16.2, decision 315): `None` until a
    /// runner installs the platform source (headless keeps the app
    /// default — [`DesktopLoop::sync_system_theme`] is a quiet
    /// no-op with no source, never a panic).
    theme_source: Option<Box<dyn SystemThemeSource>>,
    /// Runtime window chrome (Round 16.3, decision 316): `None`
    /// until a runner installs the platform control (headless
    /// title/size/fullscreen calls stay quiet no-ops —
    /// presentational hints, never wiring bugs).
    window_control: Option<Box<dyn WindowControl>>,
    /// App-level close veto (Round 16.3, decision 316): consulted
    /// on `WM_CLOSE` / `CloseRequested` — `false` suppresses exit
    /// so unsaved-changes flows can mount instead of dying.
    /// `None` closes (pre-16.3 behavior, unchanged).
    close_handler: Option<Rc<dyn Fn() -> bool>>,
    /// Runner-side poll hook (Round 26.2, decision 342): installed by
    /// the app through the `run_desktop_with` configure closure and
    /// invoked once per pump iteration with no runner borrows held
    /// (take-call-restore, so blocking dialog backends cannot trip
    /// the resize hook's borrow). Observes runner-visible signals
    /// (e.g. Task Studio's `exit_requested`), writes files through
    /// the dialog seams, then asks for close. Keep it quick and
    /// idempotent -- it runs every iteration, including modal ones.
    poll_hook: Option<DesktopPollHook>,
    /// Scene clear color (theme contract round): both paint surfaces
    /// clear to the host theme's `background`. Tracked so a theme
    /// toggle refits the surfaces instead of presenting a stale page
    /// (same one-refit path as resize/DPR, never two).
    page_bg: oppa::Color,
}

impl DesktopLoop {
    /// Creates the host (viewport + text service + layout config) and
    /// both paint surfaces (CPU pixmap + Vello scene twin). Loud on
    /// any failure. Starts CPU-only; runners enable GPU against the
    /// live window (see `enable_gpu`). Ticks the system clock (live
    /// loops — see [`DesktopLoop::with_clock`] for the deterministic
    /// harness clock).
    pub fn new(
        width_px: u32,
        height_px: u32,
        service: Box<dyn TextService>,
        family: &str,
    ) -> Result<Self, String> {
        Self::with_clock(
            width_px,
            height_px,
            service,
            family,
            Rc::new(SystemClock::new()),
        )
    }

    /// Clock-injected construction (Round 15.2, decision 313 — the
    /// headless blink rig: a `MockClock` owner advances time
    /// explicitly, so flip demand proves without sleeps). Otherwise
    /// identical to [`DesktopLoop::new`].
    pub fn with_clock(
        width_px: u32,
        height_px: u32,
        service: Box<dyn TextService>,
        family: &str,
        clock: Rc<dyn Clock>,
    ) -> Result<Self, String> {
        let (w, h) = (width_px.max(1), height_px.max(1));
        let host = ComponentHost::with_clock(clock);
        host.set_viewport(w as f32, h as f32);
        host.set_text_service(service);
        host.set_layout_config(LayoutTextConfig {
            family: family.to_string(),
            ..Default::default()
        });
        // Theme contract round: surfaces clear to the host theme's
        // page background (a fresh host is Light — the pre-contract
        // white, pixel-identical).
        let page_bg = ThemeTokens::of(host.theme().mode()).background;
        let mut cpu = CpuBackend::new();
        let surface = cpu
            .create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background: page_bg,
            })
            .map_err(|e| format!("desktop loop: create cpu surface: {e:?}"))?;
        let mut vello = VelloBackend::new();
        let vello_surface = vello
            .create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background: page_bg,
            })
            .map_err(|e| format!("desktop loop: create vello surface: {e:?}"))?;
        let mut custom_gpu = CustomGpuBackend::new();
        let custom_gpu_surface = custom_gpu
            .create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background: page_bg,
            })
            .map_err(|e| format!("desktop loop: create custom_gpu surface: {e:?}"))?;
        Ok(Self {
            host,
            cpu,
            surface,
            vello,
            vello_surface,
            custom_gpu,
            custom_gpu_surface,
            active: RendererKind::Cpu,
            gpu_mode: None,
            builder: FramePlanBuilder::new(1.0),
            viewport: (w, h),
            dpr: 1.0,
            clipboard: Box::new(InMemoryClipboard::new()),
            last_plan: None,
            last_caret_shown: false,
            save_dialog: None,
            folder_dialog: None,
            theme_source: None,
            window_control: None,
            close_handler: None,
            poll_hook: None,
            page_bg,
        })
    }

    /// Which backend the platform glue presents (`Cpu` until a
    /// runner enables GPU; runners flip back on loud GPU failure).
    pub fn renderer_kind(&self) -> RendererKind {
        self.active
    }

    /// True once a runner enabled hardware presentation.
    pub fn is_gpu(&self) -> bool {
        self.active == RendererKind::Gpu || self.active == RendererKind::CustomGpu
    }

    /// Human name for logs (`gpu` / `custom_gpu` / `cpu`).
    pub fn renderer_name(&self) -> &'static str {
        match self.active {
            RendererKind::Gpu => "gpu",
            RendererKind::CustomGpu => "custom_gpu",
            RendererKind::Cpu => "cpu",
        }
    }

    /// Swapchain present mode once GPU is up (`None` while CPU-only).
    pub fn gpu_present_mode(&self) -> Option<wgpu::PresentMode> {
        self.gpu_mode
    }

    /// Vello scene surface (the GPU twin; painted whenever
    /// [`RendererKind::Gpu`] is active).
    pub fn vello_surface_id(&self) -> SurfaceId {
        self.vello_surface
    }

    /// Custom GPU surface (painted whenever
    /// [`RendererKind::CustomGpu`] is active).
    pub fn custom_gpu_surface_id(&self) -> SurfaceId {
        self.custom_gpu_surface
    }

    /// Custom GPU retained op count.
    pub fn custom_gpu_retained_op_count(&self) -> Option<usize> {
        self.custom_gpu.retained_op_count(self.custom_gpu_surface)
    }

    /// CPU pixmap surface (always painted — the GDI / softbuffer
    /// source and the GPU fallback pixels).
    pub fn cpu_surface_id(&self) -> SurfaceId {
        self.surface
    }

    /// Enables hardware presentation after the runner configured the
    /// swapchain (mirrors the fps driver's GPU arm): the next
    /// [`DesktopLoop::repaint`] paints both backends. Callers repaint
    /// immediately so the GPU twin catches up from its
    /// creation-time scene.
    pub fn enable_gpu(&mut self, mode: wgpu::PresentMode) {
        if renderer_override() == Some(RendererKind::CustomGpu) {
            self.active = RendererKind::CustomGpu;
        } else {
            self.active = RendererKind::Gpu;
        }
        self.gpu_mode = Some(mode);
    }

    /// Explicitly enables custom GPU rasterizer mode.
    pub fn enable_custom_gpu(&mut self, mode: wgpu::PresentMode) {
        self.active = RendererKind::CustomGpu;
        self.gpu_mode = Some(mode);
    }

    /// Drops back to the CPU pixmap (loud GPU failure path): later
    /// repaints skip the Vello twin again; its retained scene stays
    /// for a later re-enable (refit keeps sizes in lockstep).
    pub fn disable_gpu(&mut self) {
        self.active = RendererKind::Cpu;
        self.gpu_mode = None;
    }

    /// Ensures a live GPU context on an adapter compatible with
    /// `surface` (the swapchain path — the caller owns the
    /// `wgpu::Instance` + `wgpu::Surface` from the platform window
    /// and hands both here, mirroring
    /// `oppa-fps::driver::FpsDriver::build_gpu`). Disk-cache
    /// round-trip included (decision 199): `cache_data` seeds
    /// pipeline creation; the returned bytes are what the caller
    /// stores. Loud `Err` when no compatible adapter exists or the
    /// device/renderer refuses it — never a silent CPU substitution
    /// (the runner logs and falls back).
    pub fn ensure_gpu_for_surface_with_cache(
        &mut self,
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
        cache_data: Option<Vec<u8>>,
    ) -> Result<(String, Option<Vec<u8>>), String> {
        if renderer_override() == Some(RendererKind::CustomGpu) {
            let info = self
                .custom_gpu
                .ensure_gpu_for_surface(instance, surface)
                .map_err(|e| format!("desktop loop: ensure custom gpu for surface: {e:?}"))?;
            self.active = RendererKind::CustomGpu;
            Ok((format!("{info} (custom_gpu)"), None))
        } else {
            self.vello
                .ensure_gpu_for_surface_with_cache(instance, surface, cache_data)
                .map_err(|e| format!("desktop loop: ensure gpu for surface: {e:?}"))
        }
    }

    /// Configures `surface` for presentation of the current viewport
    /// (format = first non-sRGB 8-bit the surface offers; the fps
    /// choice). Requires a context from
    /// [`DesktopLoop::ensure_gpu_for_surface_with_cache`]. Returns
    /// the chosen format for the run record.
    pub fn configure_gpu_surface(
        &mut self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
        mode: wgpu::PresentMode,
    ) -> Result<wgpu::TextureFormat, String> {
        if self.active == RendererKind::CustomGpu {
            self.custom_gpu
                .configure_surface(surface, width, height, mode)
                .map_err(|e| format!("desktop loop: configure custom gpu surface: {e:?}"))
        } else {
            self.vello
                .configure_surface_with_present_mode(surface, width, height, mode)
                .map_err(|e| format!("desktop loop: configure gpu surface: {e:?}"))
        }
    }

    /// Picks the present mode: uncapped Immediate first, then
    /// Mailbox, then vsync Fifo (the fps-driver order — the
    /// throughput leg first). Loud either way, with the offered list
    /// for blank-window diagnosis.
    pub fn pick_gpu_mode(&self, surface: &wgpu::Surface<'_>) -> wgpu::PresentMode {
        let offered = if self.active == RendererKind::CustomGpu {
            self.custom_gpu
                .adapter()
                .map(|a| surface.get_capabilities(a).present_modes)
                .unwrap_or_default()
        } else {
            self.vello
                .gpu_adapter()
                .map(|a| surface.get_capabilities(a).present_modes)
                .unwrap_or_default()
        };
        eprintln!("oppa-app: offered present modes: {offered:?}");
        for mode in [
            wgpu::PresentMode::Immediate,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::Fifo,
        ] {
            if offered.contains(&mode) {
                eprintln!("oppa-app: present mode {mode:?}");
                return mode;
            }
        }
        eprintln!("oppa-app: no preferred mode offered, Fifo");
        wgpu::PresentMode::Fifo
    }

    /// Presents the active GPU backend through the real swapchain.
    /// Requires GPU mode (see `enable_gpu`); a CPU-only call is a loud `Err`.
    pub fn present_gpu(
        &mut self,
        surface: &wgpu::Surface<'_>,
    ) -> Result<oppa_vello::PresentReport, String> {
        if !self.is_gpu() {
            return Err("desktop loop: present_gpu while CPU-only".to_string());
        }
        if self.active == RendererKind::CustomGpu {
            let rep = self
                .custom_gpu
                .present_surface(self.custom_gpu_surface, surface)
                .map_err(|e| format!("desktop loop: present custom gpu: {e:?}"))?;
            Ok(oppa_vello::PresentReport {
                width: rep.width,
                height: rep.height,
                format: rep.format,
                cpu_ms: rep.cpu_ms,
                acquire_ms: 0.0,
                render_ms: rep.cpu_ms,
                blit_setup_ms: 0.0,
                blit_submit_ms: 0.0,
                present_ms: 0.0,
                target_ms: 0.0,
            })
        } else {
            self.vello
                .present_surface(self.vello_surface, surface)
                .map_err(|e| format!("desktop loop: present gpu: {e:?}"))
        }
    }

    /// Current output scale (device px per CSS px).
    pub fn dpr(&self) -> f32 {
        self.dpr
    }

    /// Re-bases the loop on a new device pixel ratio keeping the CSS
    /// viewport stable (Round 2.4, OQ-G10-2 — the Desktops call this
    /// on DPI/scale changes): layout config + builder follow, the
    /// host viewport rescales to CSS, the surface refits to device
    /// px, and layout re-runs (measure keys carry the DPR, so text
    /// re-shapes; the `set_viewport` dirt from decision 248 reflows
    /// boxes — nothing stale survives at the new scale). Same-value
    /// calls are a no-op. Non-finite or non-positive scales panic
    /// loudly (a scale is never zero/NaN — shells map raw OS values
    /// through `dpr_from_dpi`/`dpr_from_scale_factor` first).
    pub fn set_device_pixel_ratio(&mut self, dpr: f32) -> Result<(), String> {
        if !dpr.is_finite() || dpr <= 0.0 {
            panic!(
                "desktop loop: device pixel ratio {dpr} refused — scales are finite and positive"
            );
        }
        if dpr == self.dpr {
            return Ok(());
        }
        // CSS size is viewport_px / old_dpr (exact at 1.0, the only
        // previously reachable state — no drift by construction).
        let css_w = self.viewport.0 as f32 / self.dpr;
        let css_h = self.viewport.1 as f32 / self.dpr;
        self.dpr = dpr;
        let mut config = self.host.layout_config();
        config.device_pixel_ratio = dpr;
        self.host.set_layout_config(config);
        self.builder.set_dpr(dpr);
        self.host.set_viewport(css_w, css_h);
        let w = (css_w * dpr).round().max(1.0) as u32;
        let h = (css_h * dpr).round().max(1.0) as u32;
        self.viewport = (w, h);
        self.refit_surface()?;
        self.host.run_until_idle();
        Ok(())
    }

    /// Mounts the root component and settles the first frame.
    pub fn mount<P: Props>(
        &self,
        name: &str,
        props: P,
        render: fn(&Ctx, &P) -> VNode,
    ) -> MountHandle<P> {
        let handle = self.host.mount(name, props, render);
        self.host.run_until_idle();
        handle
    }

    /// Installs the shortcut clipboard (decision 246 — the platform
    /// glue calls this with the OS backend; see the field docs for
    /// the default).
    pub fn set_clipboard(&mut self, clipboard: Box<dyn Clipboard>) {
        self.clipboard = clipboard;
    }

    /// Borrows the shortcut clipboard (test read-back, embedder
    /// peeks — same backend the shortcuts use, never a copy).
    pub fn clipboard(&mut self) -> &mut dyn Clipboard {
        &mut *self.clipboard
    }

    /// Installs the save-dialog backend (Round 16.1, decision 314 —
    /// the platform glue calls this with the OS backend; see the
    /// field docs for the headless default).
    pub fn set_save_dialog(&mut self, dialog: Box<dyn SaveFileDialog>) {
        self.save_dialog = Some(dialog);
    }

    /// Installs the folder-picker backend (Round 16.1, decision 314
    /// — same glue rule as the save slot).
    pub fn set_folder_dialog(&mut self, dialog: Box<dyn FolderDialog>) {
        self.folder_dialog = Some(dialog);
    }

    /// Prompts the user for a save destination (Round 16.1, decision
    /// 314): blocks on the native modal dialog and returns the
    /// picked path, or `None` when dismissed — or when no backend is
    /// installed (headless/web stay graceful, never a panic).
    /// Backend failures log loudly to stderr and settle `None` (the
    /// clipboard-transient precedent — a wedged picker must not kill
    /// the app; callers needing the reason drive the backend
    /// directly).
    pub fn save_file_dialog(&mut self, options: FileDialogOptions) -> Option<PathBuf> {
        let dialog = self.save_dialog.as_mut()?;
        match dialog.save(options) {
            Ok(path) => path,
            Err(e) => {
                eprintln!("oppa-app: save dialog failed: {e}");
                None
            }
        }
    }

    /// Prompts the user for a destination directory (Round 16.1,
    /// decision 314 — same settle rules as
    /// [`DesktopLoop::save_file_dialog`]: picked path, dismissal
    /// `None`, graceful headless `None`, loud-stderr backend
    /// failures).
    pub fn pick_folder_dialog(&mut self, options: FolderDialogOptions) -> Option<PathBuf> {
        let dialog = self.folder_dialog.as_mut()?;
        match dialog.pick(options) {
            Ok(path) => path,
            Err(e) => {
                eprintln!("oppa-app: folder dialog failed: {e}");
                None
            }
        }
    }

    /// Installs the OS theme reader (Round 16.2, decision 315 —
    /// the platform glue calls this with the OS source; see the
    /// field docs for the headless default).
    pub fn set_theme_source(&mut self, source: Box<dyn SystemThemeSource>) {
        self.theme_source = Some(source);
    }

    /// Forwards the OS theme into the reactive theme signal (Round
    /// 16.2, decision 315 — runners call this at startup and on
    /// every system theme-change message): queries the installed
    /// source and, when it reads a mode the app does not already
    /// have, sets it (every themed body re-renders, no instance
    /// re-created), settles, and repaints. Returns `true` when the
    /// mode changed (the runner presents). Quiet `false` with no
    /// source, on unreadable sources, or when the reading matches
    /// (a redundant OS message never spins a repaint). Paint
    /// failures are loud `Err`s like `repaint`.
    pub fn sync_system_theme(&mut self) -> Result<bool, String> {
        let mode = match self.theme_source.as_mut() {
            Some(source) => source.system_theme(),
            None => None,
        };
        let Some(mode) = mode else {
            return Ok(false);
        };
        if self.host.theme().mode() == mode {
            return Ok(false);
        }
        self.host.set_theme(mode);
        self.host.run_until_idle();
        self.repaint()?;
        Ok(true)
    }

    /// Installs the runtime window-chrome control (Round 16.3,
    /// decision 316 — the platform glue calls this with the OS
    /// control; see the field docs for the headless default).
    pub fn set_window_control(&mut self, control: Box<dyn WindowControl>) {
        self.window_control = Some(control);
    }

    /// Retitles the window at runtime (Round 16.3, decision 316 —
    /// forwards into the installed control; quiet no-op with none,
    /// never a panic).
    pub fn set_title(&self, title: &str) {
        if let Some(control) = self.window_control.as_ref() {
            control.set_title(title);
        }
    }

    /// Constrains the resizable floor at runtime (`None` clears it
    /// — same forwarding/grace rules as
    /// [`DesktopLoop::set_title`]).
    pub fn set_min_size(&self, size: Option<(u32, u32)>) {
        if let Some(control) = self.window_control.as_ref() {
            control.set_min_size(size);
        }
    }

    /// Constrains the resizable ceiling at runtime (`None` clears
    /// it — same forwarding/grace rules as
    /// [`DesktopLoop::set_title`]).
    pub fn set_max_size(&self, size: Option<(u32, u32)>) {
        if let Some(control) = self.window_control.as_ref() {
            control.set_max_size(size);
        }
    }

    /// Toggles borderless fullscreen at runtime (state-preserving —
    /// same forwarding/grace rules as [`DesktopLoop::set_title`]).
    pub fn set_fullscreen(&self, fullscreen: bool) {
        if let Some(control) = self.window_control.as_ref() {
            control.set_fullscreen(fullscreen);
        }
    }

    /// Installs the runtime window icon (Round 20.3, decision 326 —
    /// `None` restores the OS default; same forwarding/grace rules
    /// as [`DesktopLoop::set_title`]).
    pub fn set_icon(&self, icon: Option<WindowIcon>) {
        if let Some(control) = self.window_control.as_ref() {
            control.set_icon(icon);
        }
    }

    /// Installs the app-level close veto (Round 16.3, decision 316
    /// — the brief's `host.set_close_handler`, homed on the loop:
    /// close is window lifecycle, and the runners consult it here).
    /// Returning `false` suppresses exit on `WM_CLOSE` /
    /// `CloseRequested` so unsaved-changes flows can mount instead
    /// of dying; `true` (and no handler at all) closes.
    pub fn set_close_handler(&mut self, handler: Rc<dyn Fn() -> bool>) {
        self.close_handler = Some(handler);
    }

    /// Whether a close request proceeds (Round 16.3, decision 316 —
    /// runners call this on `WM_CLOSE` / `CloseRequested`): the
    /// installed handler decides, `false` vetoing; no handler
    /// closes (pre-16.3 behavior, unchanged).
    pub fn close_requested(&self) -> bool {
        match self.close_handler.as_ref() {
            Some(handler) => handler(),
            None => true,
        }
    }

    /// Installs the runner-side poll hook (Round 26.2, decision 342 --
    /// the voted write bridge): runners invoke it once per pump
    /// iteration; `None` removes it. The hook observes runner-visible
    /// signals and drives runner-owned seams (dialogs, close) that
    /// component code cannot reach.
    pub fn set_poll_hook(&mut self, hook: Option<DesktopPollHook>) {
        self.poll_hook = hook;
    }

    /// Runs one poll-hook invocation (runner-side): take-call-restore,
    /// so a hook that blocks in a dialog backend cannot trip a
    /// runner borrow held across the call. No hook is a quiet no-op.
    pub fn run_poll_hook(&mut self) {
        if let Some(mut hook) = self.poll_hook.take() {
            hook(self);
            self.poll_hook = Some(hook);
        }
    }

    /// Injects one framework input event, settles, and repaints.
    /// Returns the repaint's damage-rect count (0 = nothing to
    /// present). Paint failures are loud `Err`s.
    ///
    /// Ctrl+letter editing shortcuts are consumed here (decision
    /// 246): one interception point for both platform arms,
    /// headless-testable through this method. Only pressed keys with
    /// ctrl held and neither alt nor meta (AltGr is ctrl+alt at the
    /// OS level and types real chars on many layouts — swallowing
    /// those as shortcuts would eat user text; Win-key combos never
    /// reach the app on Windows and stay router-quiet on Linux).
    /// Consumed shortcuts never reach `inject_input` (no duplicate
    /// handling, no character entry).
    pub fn step(&mut self, ev: InputEvent) -> Result<usize, String> {
        if let InputEvent::Key {
            code,
            modifiers,
            state,
            ..
        } = &ev
        {
            if *state == KeyState::Pressed
                && is_edit_shortcut_modifier(*modifiers)
                && is_edit_shortcut_code(*code)
            {
                return self.run_edit_shortcut(*code);
            }
            // Round 22.1 (decision 331): navigation keys drive the
            // focused session directly (plain/shift/ctrl word-step
            // and extension included) — consumed so controls never
            // see them twice; anything unmapped (or with no focused
            // field) flows to the router, so slider/scrollbar arrows
            // keep working.
            if *state == KeyState::Pressed
                && !modifiers.alt
                && !modifiers.meta
                && self.step_session_nav(*code, modifiers.shift, modifiers.ctrl)?
            {
                self.host.run_until_idle();
                return self.repaint();
            }
            // Round 22.2 (decision 332): vertical keys drive the
            // focused multi-line session (same consume contract —
            // single-line Up/Down still flows to the router).
            if *state == KeyState::Pressed
                && !modifiers.alt
                && !modifiers.meta
                && !modifiers.ctrl
                && self.step_session_vline(*code, modifiers.shift)?
            {
                self.host.run_until_idle();
                return self.repaint();
            }
        }
        self.host.inject_input(ev);
        self.host.run_until_idle();
        self.repaint()
    }

    /// Runs one editing shortcut against the focused field's session
    /// (decision 246): select-all / undo / redo / copy / cut / paste.
    /// Quiet `Ok(0)` when no field is focused (router precedent for
    /// unhandled keys) or when the op is a no-op (copy/cut with a
    /// collapsed caret, paste of an empty clipboard or
    /// mid-composition — the session spells those, see
    /// `PasteOutcome`); settle + repaint only when content or
    /// selection changed. Backend clipboard failures are loud on
    /// stderr but never fatal: a locked clipboard is a routine
    /// transient (the G3 contract says retry-next-frame), and killing
    /// the app over it would punish the user for another app's lock
    /// — stated, not silent.
    pub fn run_edit_shortcut(&mut self, code: u32) -> Result<usize, String> {
        let Some(session) = self.host.focused_field_session() else {
            return Ok(0);
        };
        let changed = match code {
            keys::A => {
                session.select_all();
                true
            }
            keys::Z => {
                session.undo();
                true
            }
            keys::Y => {
                session.redo();
                true
            }
            keys::C => match session.copy_selection_to(&mut *self.clipboard) {
                Ok(copied) => copied.is_some(),
                Err(e) => {
                    eprintln!("oppa-app: clipboard copy failed: {e}");
                    return Ok(0);
                }
            },
            keys::X => match session.cut_selection_to(&mut *self.clipboard) {
                Ok(cut) => cut.is_some(),
                Err(e) => {
                    eprintln!("oppa-app: clipboard cut failed: {e}");
                    return Ok(0);
                }
            },
            keys::V => match session.paste_from(&mut *self.clipboard) {
                Ok(PasteOutcome::Pasted(_)) => true,
                Ok(_) => false,
                Err(e) => {
                    eprintln!("oppa-app: clipboard paste failed: {e}");
                    return Ok(0);
                }
            },
            _ => return Ok(0),
        };
        if !changed {
            return Ok(0);
        }
        self.host.run_until_idle();
        self.repaint()
    }

    /// Drives one navigation key into the focused field's session
    /// (Round 22.1, decision 331): Left/Right/Home/End in
    /// plain/shift/ctrl/ctrl+shift combinations — char-step,
    /// extension, word-step, word-extension, and jump/extend-to-ends
    /// (the session spells the moves; masked fields navigate
    /// identically, only copy/cut refuse). Returns true when a move
    /// ran (the caller settles + repaints); false with no focused
    /// session or an unmapped key — the key then flows to the
    /// router untouched (slider/scrollbar/menu arrows keep their
    /// owners). Alt/Meta combinations never reach here (the `step`
    /// guard above keeps AltGr typing intact).
    fn step_session_nav(&mut self, code: u32, shift: bool, ctrl: bool) -> Result<bool, String> {
        use keys::{END, HOME, LEFT, RIGHT};
        use oppa::EditSession;
        let op: fn(&EditSession) = match (code, shift, ctrl) {
            (LEFT, false, false) => EditSession::caret_left,
            (RIGHT, false, false) => EditSession::caret_right,
            (LEFT, true, false) => EditSession::extend_left,
            (RIGHT, true, false) => EditSession::extend_right,
            (LEFT, false, true) => EditSession::word_left,
            (RIGHT, false, true) => EditSession::word_right,
            (LEFT, true, true) => EditSession::extend_word_left,
            (RIGHT, true, true) => EditSession::extend_word_right,
            (HOME, false, _) => EditSession::caret_to_start,
            (END, false, _) => EditSession::caret_to_end,
            (HOME, true, _) => EditSession::extend_to_start,
            (END, true, _) => EditSession::extend_to_end,
            _ => return Ok(false),
        };
        let Some(session) = self.host.focused_field_session() else {
            return Ok(false);
        };
        op(&session);
        Ok(true)
    }

    /// Drives one vertical key into the focused multi-line session
    /// (Round 22.2, decision 332 — ArrowUp/ArrowDown with optional
    /// Shift): visual-line travel with preferred-x affinity on
    /// `TextArea` sessions, router-quiet otherwise (single-line
    /// fields keep the 22.1 behavior — Up/Down flow to directional
    /// handlers — and slider/scrollbar/menu arrows keep their
    /// owners). Same consume/settle contract as `step_session_nav`.
    fn step_session_vline(&mut self, code: u32, shift: bool) -> Result<bool, String> {
        use keys::{DOWN, UP};
        use oppa::EditSession;
        let op: fn(&EditSession) = match (code, shift) {
            (UP, false) => EditSession::line_up,
            (DOWN, false) => EditSession::line_down,
            (UP, true) => EditSession::extend_line_up,
            (DOWN, true) => EditSession::extend_line_down,
            _ => return Ok(false),
        };
        let Some(session) = self.host.focused_field_session() else {
            return Ok(false);
        };
        if !session.is_multiline() {
            return Ok(false);
        }
        op(&session);
        Ok(true)
    }

    /// Settles and repaints unconditionally (warmup frames, resizes).
    /// Commits always land in both backends (cheap bookkeeping);
    /// pixels encode into the CPU pixmap always (the GDI /
    /// softbuffer source and the GPU fallback) plus into the Vello
    /// twin whenever GPU is active. CPU-only repaints skip Vello so
    /// a faceless test shaper never trips the Vello loud-no-face
    /// rule while the CPU path still draws bars.
    pub fn repaint(&mut self) -> Result<usize, String> {
        for d in self.host.diffs_from(0) {
            self.cpu
                .commit(&d)
                .map_err(|e| format!("desktop loop: cpu commit: {e:?}"))?;
            self.vello
                .commit(&d)
                .map_err(|e| format!("desktop loop: vello commit: {e:?}"))?;
            self.custom_gpu
                .commit(&d)
                .map_err(|e| format!("desktop loop: custom_gpu commit: {e:?}"))?;
        }
        // Round 15.2 (decision 313): the text overlays ride every
        // committed frame — the shared build-scoped rule from the
        // paint hooks (selection + caret resolved outside the
        // retained borrow). Before this, live presented frames
        // committed with both overlays `None`: zero selection rects
        // and zero carets on GPU and GDI alike.
        self.builder
            .set_selection(self.host.focused_selection_paint());
        self.builder.set_caret(self.host.focused_caret_paint());
        // Theme contract round: the build theme rides the same
        // per-frame publish rule (default text ink follows the host
        // mode), and a toggled page background refits the surfaces
        // before painting (same one-refit path as resize/DPR —
        // steady state is a color compare, never surface churn).
        let mode = self.host.theme().mode();
        self.builder.set_theme_mode(mode);
        let want_bg = ThemeTokens::of(mode).background;
        if want_bg != self.page_bg {
            self.page_bg = want_bg;
            self.refit_surface()?;
        }
        let plan = self
            .host
            .with_retained_mut(|rec, styles| self.builder.build_full(rec, styles));
        self.last_caret_shown = self.builder.caret().is_some();
        self.last_plan = Some(plan.clone());
        let damage = plan.damage.len();
        let images = self.host.images();
        for op in &plan.ops {
            if let oppa::DrawOp::RImg { image, .. } = op {
                if !self.cpu.has_image(*image) && images.pixels_of(*image).is_some() {
                    self.cpu.insert_cached(&images, *image);
                }
                if self.active == RendererKind::Gpu
                    && !self.vello.has_image(*image)
                    && images.pixels_of(*image).is_some()
                {
                    self.vello.insert_cached(&images, *image);
                } else if self.active == RendererKind::CustomGpu
                    && !self.custom_gpu.has_image(*image)
                    && images.pixels_of(*image).is_some()
                {
                    self.custom_gpu.insert_cached(&images, *image);
                }
            }
        }
        self.cpu
            .paint(self.surface, &plan)
            .map_err(|e| format!("desktop loop: cpu paint: {e:?}"))?;
        if self.active == RendererKind::Gpu {
            self.vello
                .paint(self.vello_surface, &plan)
                .map_err(|e| format!("desktop loop: vello paint: {e:?}"))?;
        } else if self.active == RendererKind::CustomGpu {
            self.custom_gpu
                .paint(self.custom_gpu_surface, &plan)
                .map_err(|e| format!("desktop loop: custom_gpu paint: {e:?}"))?;
        }
        Ok(damage)
    }

    /// The last committed frame plan (Round 15.2, decision 313 —
    /// proves which overlay `Rect`s a repaint committed; `None`
    /// before the first repaint, never a live borrow).
    pub fn last_plan(&self) -> Option<FramePlan> {
        self.last_plan.clone()
    }

    /// Seconds until the focused caret's next blink flip, if any
    /// (Round 15.2, decision 313 — the runner wake query): `Some`
    /// while a collapsed caret is focused (wake then to repaint the
    /// flip); `None` otherwise (unfocused, selected, session-less —
    /// the loop stays event-driven). Pure over the host —
    /// headless-tested.
    pub fn blink_tick_in_secs(&self) -> Option<f64> {
        self.host.caret_blink_in_secs()
    }

    /// True when the caret overlay's presence flipped since the last
    /// repaint (Round 15.2, decision 313 — the blink demand): the
    /// flip writes no signals and dirties nothing, so the tick path
    /// compares presence instead of waiting for demand that never
    /// comes. Pure over the host — headless-tested.
    pub fn caret_overlay_dirty(&self) -> bool {
        self.host.focused_caret_paint().is_some() != self.last_caret_shown
    }

    /// Settles and repaints iff the caret overlay flipped (Round
    /// 15.2, decision 313 — the timer-tick path runners call when
    /// no input arrived): returns `true` when it repainted, `false`
    /// when settled-quiet (never a spin — a quiet poll schedules
    /// nothing). Paint failures are loud `Err`s like `repaint`.
    pub fn poll_blink(&mut self) -> Result<bool, String> {
        self.host.run_until_idle();
        if self.caret_overlay_dirty() {
            self.repaint()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Earliest component-timer due instant in host-clock ms (Round
    /// 21.1, decision 328 — the wake-query twin of
    /// `blink_tick_in_secs`): `Some` while a component timer is
    /// armed (runners wake then to fire it); `None` with no timers
    /// or while lifecycle-suspended. Pure over the host —
    /// headless-tested.
    pub fn next_timer_due_ms(&self) -> Option<f64> {
        let now = self.host.now_ms();
        self.host.next_timer_due_ms(now)
    }

    /// Fires due component timers, settling + repainting when
    /// anything fired (Round 21.1, decision 328 — the timer-tick
    /// path runners call on wake): returns the fired count. Zero
    /// when nothing was due (quiet — never a spin: firing
    /// schedules only what callbacks write, and the horizon then
    /// moves past `now`). Paint failures are loud `Err`s.
    pub fn tick_timers(&mut self) -> Result<usize, String> {
        let now = self.host.now_ms();
        let fired = self.host.tick_timers(now);
        if fired > 0 {
            self.host.run_until_idle();
            self.repaint()?;
        }
        Ok(fired)
    }

    /// Tracks a resize: refits the viewport and rebuilds the paint
    /// surface (positions derive from the viewport, so refit alone
    /// leaves the scene stale — same rule as the fps core's
    /// `set_size`). `width_px`/`height_px` are device px; the host
    /// viewport divides by the DPR (identity at 1.0 — the only
    /// previously reachable state). Layout invalidation rides
    /// `set_viewport` (decision 248 — the trailing settle actually
    /// re-runs layout now). Same-size calls are a no-op. Loud on
    /// failure.
    pub fn resize(&mut self, width_px: u32, height_px: u32) -> Result<(), String> {
        let (w, h) = (width_px.max(1), height_px.max(1));
        if (w, h) == self.viewport {
            return Ok(());
        }
        self.viewport = (w, h);
        self.host
            .set_viewport(w as f32 / self.dpr, h as f32 / self.dpr);
        self.refit_surface()?;
        self.host.run_until_idle();
        Ok(())
    }

    /// Recreates both paint surfaces at the current viewport size
    /// (shared by `resize`, `set_device_pixel_ratio`, and theme
    /// toggles — one refit path, never two). The Vello twin refits
    /// even while CPU-only so a later `enable_gpu` never presents a
    /// stale size. Both surfaces clear to the tracked page
    /// background (theme contract round).
    fn refit_surface(&mut self) -> Result<(), String> {
        let (w, h) = self.viewport;
        let bg = self.page_bg;
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
        let old_vello = self.vello_surface;
        let next_vello = self
            .vello
            .create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background: bg,
            })
            .map_err(|e| format!("desktop loop: recreate vello surface: {e:?}"))?;
        self.vello_surface = next_vello;
        let _ = self.vello.destroy_surface(old_vello);
        let old_custom = self.custom_gpu_surface;
        let next_custom = self
            .custom_gpu
            .create_surface(SurfaceDesc {
                width_px: w,
                height_px: h,
                background: bg,
            })
            .map_err(|e| format!("desktop loop: recreate custom_gpu surface: {e:?}"))?;
        self.custom_gpu_surface = next_custom;
        let _ = self.custom_gpu.destroy_surface(old_custom);
        Ok(())
    }

    /// Straight RGBA8 row-major pixels of the last paint (opaque
    /// scenes only — a non-opaque pixel is a loud `Err`, the m10
    /// straightening rule). Present glue converts onward per
    /// platform (BGRA for GDI, XRGB words for softbuffer).
    pub fn rgba8(&self) -> Result<Vec<u8>, String> {
        let px = self
            .cpu
            .pixmap(self.surface)
            .ok_or_else(|| "desktop loop: cpu pixmap missing".to_string())?;
        if px.width() == 0 || px.height() == 0 {
            return Err("desktop loop: zero-size pixmap".to_string());
        }
        let mut out = Vec::with_capacity(px.width() as usize * px.height() as usize * 4);
        for p in px.pixels() {
            if p.alpha() != 255 {
                return Err("desktop loop: non-opaque pixel".to_string());
            }
            out.push(p.red());
            out.push(p.green());
            out.push(p.blue());
            out.push(255);
        }
        Ok(out)
    }

    /// The Escape-at-root exit rule (shared by both platform loops):
    /// a pressed Escape with nothing focused exits; anywhere else it
    /// flows to the router (which clears focus). Pure over the host
    /// — headless-tested below.
    pub fn escape_exits(&self, code: u32, pressed: bool) -> bool {
        pressed && code == oppa::input::keys::ESCAPE && self.host.focused_node().is_none()
    }

    /// Types `text` into the focused text field's session, if any
    /// (decision 243): caret-aware insert with undo coalescing, then
    /// settle + repaint. Returns the repaint's damage count; `Ok(0)`
    /// when no field is focused (quiet miss — router precedent for
    /// unhandled keys). Control chars are skipped (the spike's
    /// WM_CHAR rule: shortcut shadows like Enter/Escape arrive via
    /// `Key`, never as text) — an all-control input paints nothing.
    pub fn type_text(&mut self, text: &str) -> Result<usize, String> {
        let printable: String = text.chars().filter(|c| (*c as u32) >= 0x20).collect();
        if printable.is_empty() {
            return Ok(0);
        }
        let Some(session) = self.host.focused_field_session() else {
            return Ok(0);
        };
        session.insert(&printable);
        self.host.run_until_idle();
        self.repaint()
    }

    /// Backspace into the focused field's session (selection, else
    /// char before caret — unicode-correct, undo-coalesced). `Ok(0)`
    /// when no field is focused. Consumed by the runner glue (never
    /// also injected — a future field `Key` handler must not see it
    /// twice).
    pub fn backspace(&mut self) -> Result<usize, String> {
        let Some(session) = self.host.focused_field_session() else {
            return Ok(0);
        };
        session.backspace();
        self.host.run_until_idle();
        self.repaint()
    }

    /// Delete-forward into the focused field's session (selection,
    /// else char after caret). Same quiet-miss and consume rules as
    /// [`DesktopLoop::backspace`].
    pub fn delete_forward(&mut self) -> Result<usize, String> {
        let Some(session) = self.host.focused_field_session() else {
            return Ok(0);
        };
        session.delete_forward();
        self.host.run_until_idle();
        self.repaint()
    }

    /// Feeds normalized IME composition events into the focused
    /// field's session (Round 2.1, decision 256 — the Windows runner
    /// maps `WM_IME_*` snapshots and the Linux runner maps winit `Ime`
    /// events into these; see `feed_ime_preedit` / `feed_ime_commit` /
    /// `feed_ime_cancel` for the platform-shaped halves). Dispatches
    /// through the one `dispatch_ime_event` seam, then settles +
    /// repaints, returning the repaint's damage count. Quiet `Ok(0)`
    /// when no field is focused (the `type_text` precedent — an IME
    /// event with no focused field is a focus race, never a wiring
    /// bug). Runners pump the candidate anchor
    /// ([`DesktopLoop::ime_anchor`]) after every call.
    pub fn feed_ime(&mut self, events: &[ImeCompositionEvent]) -> Result<usize, String> {
        let Some(mut session) = self.host.focused_field_session() else {
            return Ok(0);
        };
        for ev in events {
            dispatch_ime_event(&mut session, ev);
        }
        self.host.run_until_idle();
        self.repaint()
    }

    /// Preedit feed (the Linux/winit half of Round 2.1): non-empty
    /// preedit text ensures an open composition — anchored at the
    /// selection start, replacing the selection exactly like the
    /// Windows mapper — and updates it with the byte-wise caret
    /// (winit units; the session floors to a char boundary). Empty
    /// preedit is a no-op (winit sends it right before `Commit`, and
    /// a lone clear must leave the composition open for the commit —
    /// stated, not silent). Returns repaint damage; quiet `Ok(0)`
    /// when unfocused.
    pub fn feed_ime_preedit(
        &mut self,
        text: &str,
        caret_byte: Option<usize>,
    ) -> Result<usize, String> {
        if text.is_empty() {
            return Ok(0);
        }
        let Some(session) = self.host.focused_field_session() else {
            return Ok(0);
        };
        let sel = session.selection();
        let mut events = Vec::new();
        if !session.is_composing() {
            events.push(ImeCompositionEvent::CompositionStarted { start_byte: sel.0 });
            if sel.0 != sel.1 {
                events.push(ImeCompositionEvent::DeleteRange { range: sel });
            }
        }
        let start = session.composition_start_byte().unwrap_or(sel.0);
        events.push(ImeCompositionEvent::CompositionUpdated {
            composition: text.to_string(),
            caret_byte: start + caret_byte.unwrap_or(text.len()),
        });
        self.feed_ime(&events)
    }

    /// Commit feed: replaces the open composition (or inserts cold —
    /// the session spells it) as one atomic undo unit. Empty commits
    /// and unfocused commits are quiet `Ok(0)` no-ops.
    pub fn feed_ime_commit(&mut self, text: &str) -> Result<usize, String> {
        if text.is_empty() {
            return Ok(0);
        }
        self.feed_ime(&[ImeCompositionEvent::CompositionCommitted {
            committed: text.to_string(),
        }])
    }

    /// Cancel feed: drops the open composition, caret back to anchor.
    /// Quiet `Ok(0)` when nothing composes or nothing is focused.
    pub fn feed_ime_cancel(&mut self) -> Result<usize, String> {
        let Some(session) = self.host.focused_field_session() else {
            return Ok(0);
        };
        if !session.is_composing() {
            return Ok(0);
        }
        self.feed_ime(&[ImeCompositionEvent::CompositionCancelled])
    }

    /// Candidate-window anchor for the focused field in client px
    /// (Round 2.1): the host-resolved caret rect — runners hand it to
    /// the OS candidate window after every IME step so the candidate
    /// follows the composition caret dynamically. `None` when
    /// unfocused or unshaped (quiet — anchoring is advisory).
    pub fn ime_anchor(&self) -> Option<[f32; 4]> {
        self.host.focused_ime_anchor()
    }

    /// Routes one wheel tick at device-px `(x, y)` to the scrollable
    /// under the cursor (decision 250): hit-test + walk-up resolve
    /// the target, the `Scroll` event injects (handler dispatch +
    /// explicit-or-self-wired feed accumulation in the router), then
    /// settle + repaint. Quiet `Ok(0)` over non-scrollable area (router
    /// precedent for unhandled keys — scrolling dead space is not a
    /// wiring bug). `dx` feeds the bound horizontal feed when the
    /// target opted in (`bind_scroll_x` — Round 9.3, decision 302),
    /// `dy` the vertical feed as before (explicit feeds win; otherwise
    /// the target's handler owner self-wires, Round 24.2).
    pub fn scroll_at(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> Result<usize, String> {
        let Some(target) = self.host.scroll_target_at(x, y) else {
            return Ok(0);
        };
        self.host
            .inject_input(InputEvent::Scroll { target, dx, dy });
        self.host.run_until_idle();
        self.repaint()
    }

    /// Injects a raster face for one shaper font id (decision 200):
    /// without it the CPU backend draws advance-cell bars and the
    /// Vello twin refuses loudly at paint; with it, real outlines on
    /// both. Best-effort by contract (callers warn, never die, when
    /// the face is unreadable). Always lands in both backends so a
    /// later `enable_gpu` never paints tofu.
    pub fn set_font_for(&mut self, id: oppa::text::FontId, bytes: Vec<u8>, index: u32) {
        self.cpu.set_font_for(id, bytes.clone(), index);
        self.vello.set_font_for(id, bytes.clone(), index);
        self.custom_gpu.set_font_for(id, bytes, index);
    }

    /// Vello twin observability for tests (how many ops the last GPU
    /// paint retained; `None` when the surface is gone — never
    /// happens outside teardown).
    pub fn vello_retained_op_count(&self) -> Option<usize> {
        self.vello.retained_op_count(self.vello_surface)
    }

    /// CPU retained op count (test symmetry with the Vello twin).
    pub fn cpu_retained_op_count(&self) -> Option<usize> {
        self.cpu.retained_op_count(self.surface)
    }

    pub fn host(&self) -> &ComponentHost {
        &self.host
    }

    pub fn viewport(&self) -> (u32, u32) {
        self.viewport
    }
}

#[cfg(test)]
mod tests;
