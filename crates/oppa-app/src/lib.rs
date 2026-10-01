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
}

/// Parses one `OPPA_RENDERER` value (pure — headless-testable):
/// `cpu` forces the softbuffer/GDI path, `gpu` requests hardware.
/// Anything else (including `None`) is auto (GPU-first with loud
/// CPU fallback). Case-insensitive; surrounding whitespace ignored.
pub fn parse_renderer_override(value: Option<&str>) -> Option<RendererKind> {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("cpu") => Some(RendererKind::Cpu),
        Some("gpu") => Some(RendererKind::Gpu),
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
    poll_hook: Option<Box<dyn FnMut(&mut DesktopLoop)>>,
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
        Ok(Self {
            host,
            cpu,
            surface,
            vello,
            vello_surface,
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
        self.active == RendererKind::Gpu
    }

    /// Human name for logs (`gpu` / `cpu`).
    pub fn renderer_name(&self) -> &'static str {
        match self.active {
            RendererKind::Gpu => "gpu",
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

    /// CPU pixmap surface (always painted — the GDI / softbuffer
    /// source and the GPU fallback pixels).
    pub fn cpu_surface_id(&self) -> SurfaceId {
        self.surface
    }

    /// Enables hardware presentation after the runner configured the
    /// swapchain (mirrors the fps driver's GPU arm): the next
    /// [`DesktopLoop::repaint`] paints both backends. Callers repaint
    /// immediately so the Vello twin catches up from its
    /// creation-time scene.
    pub fn enable_gpu(&mut self, mode: wgpu::PresentMode) {
        self.active = RendererKind::Gpu;
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
        self.vello
            .ensure_gpu_for_surface_with_cache(instance, surface, cache_data)
            .map_err(|e| format!("desktop loop: ensure gpu for surface: {e:?}"))
    }

    /// Configures `surface` for presentation of the current viewport
    /// (format = first non-sRGB 8-bit the surface offers; the fps
    /// choice). Requires a context from
    /// [`DesktopLoop::ensure_gpu_for_surface_with_cache`]. Returns
    /// the chosen format for the run record.
    pub fn configure_gpu_surface(
        &self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
        mode: wgpu::PresentMode,
    ) -> Result<wgpu::TextureFormat, String> {
        self.vello
            .configure_surface_with_present_mode(surface, width, height, mode)
            .map_err(|e| format!("desktop loop: configure gpu surface: {e:?}"))
    }

    /// Picks the present mode: uncapped Immediate first, then
    /// Mailbox, then vsync Fifo (the fps-driver order — the
    /// throughput leg first). Loud either way, with the offered list
    /// for blank-window diagnosis.
    pub fn pick_gpu_mode(&self, surface: &wgpu::Surface<'_>) -> wgpu::PresentMode {
        let offered = self
            .vello
            .gpu_adapter()
            .map(|a| surface.get_capabilities(a).present_modes)
            .unwrap_or_default();
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

    /// Presents the Vello twin through the real swapchain (scene →
    /// intermediate texture → blit → `present`). Requires GPU mode
    /// (see `enable_gpu`); a CPU-only call is a loud `Err`, never a
    /// silent no-op. Swapchain `Outdated` is returned verbatim so the
    /// runner can reconfigure and retry (the fps storm rule).
    pub fn present_gpu(
        &mut self,
        surface: &wgpu::Surface<'_>,
    ) -> Result<oppa_vello::PresentReport, String> {
        if self.active != RendererKind::Gpu {
            return Err("desktop loop: present_gpu while CPU-only".to_string());
        }
        self.vello
            .present_surface(self.vello_surface, surface)
            .map_err(|e| format!("desktop loop: present gpu: {e:?}"))
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
    pub fn set_poll_hook(&mut self, hook: Option<Box<dyn FnMut(&mut DesktopLoop)>>) {
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
        self.cpu
            .paint(self.surface, &plan)
            .map_err(|e| format!("desktop loop: cpu paint: {e:?}"))?;
        if self.active == RendererKind::Gpu {
            self.vello
                .paint(self.vello_surface, &plan)
                .map_err(|e| format!("desktop loop: vello paint: {e:?}"))?;
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
        self.vello.set_font_for(id, bytes, index);
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
mod tests {
    use super::*;
    use oppa::{
        Cluster, Color, Div, DrawOp, FontId, FontMetrics, JustifyContent, Row, ShapedGlyph,
        ShapedRun, SharedString, Signal, Style, Text, TextError, TextRun, ThemeMode, ThemeTokens,
    };

    /// Uniform-advance fake shaper (body 14px → 8.75px/char).
    struct FakeText;

    impl TextService for FakeText {
        fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
            Vec::new()
        }

        fn shape(&self, text: &str, style: &oppa::text::TextStyle) -> Result<ShapedRun, TextError> {
            if text.is_empty() {
                return Err(TextError::EmptyText);
            }
            let em = style.font_size_px * style.device_pixel_ratio;
            let adv = em * 0.625;
            let metrics = FontMetrics {
                ascent: em * 0.75,
                descent: em * 0.25,
                line_gap: em * 0.125,
            };
            let mut glyphs = Vec::new();
            let mut clusters = Vec::new();
            for (k, (i, ch)) in text.char_indices().enumerate() {
                let len = ch.len_utf8();
                glyphs.push(ShapedGlyph {
                    glyph_id: k as u32,
                    x_advance: adv,
                    x_offset: 0.0,
                    y_offset: 0.0,
                });
                clusters.push(Cluster {
                    byte_range: (i, i + len),
                    glyph_range: (glyphs.len() - 1, glyphs.len()),
                });
            }
            Ok(ShapedRun {
                total_advance: adv * glyphs.len() as f32,
                text_len_bytes: text.len(),
                runs: vec![TextRun {
                    byte_range: (0, text.len()),
                    glyph_range: (0, glyphs.len()),
                    rtl: false,
                    script: 0,
                    font_id: FontId(0),
                    font_metrics: metrics,
                }],
                glyphs,
                clusters,
            })
        }
    }

    #[derive(Clone)]
    struct FlipProps {
        on: Signal<bool>,
    }
    impl Props for FlipProps {}

    fn flip_app(_ctx: &Ctx, props: &FlipProps) -> VNode {
        let on = props.on.clone();
        let bg = if on.get() {
            Color(0x22_66_CC)
        } else {
            Color(0x88_88_88)
        };
        Div("screen")
            .style(Style::new().size(200, 150).bg(Color(0xFF_FF_FF)))
            .child(
                Div("flip")
                    .style(Style::new().size(96, 32).bg(bg))
                    .semantics(oppa::Semantics::button().label("Flip"))
                    .on_press(move || on.set(!on.get()))
                    .child(VNode::from(Text {
                        text: oppa::SharedString::from("Flip"),
                        style: Text::body_secondary,
                    })),
            )
    }

    fn harness() -> (DesktopLoop, Signal<bool>) {
        let loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        let on = loop_.host().runtime().signal(false);
        loop_.mount("Flip", FlipProps { on: on.clone() }, flip_app);
        (loop_, on)
    }

    #[test]
    fn loop_mounts_and_paints_pixels() {
        let (mut loop_, _) = harness();
        let damage = loop_.repaint().expect("repaint works");
        assert!(damage > 0, "first paint damages");
        let rgba = loop_.rgba8().expect("pixels read back");
        assert_eq!(rgba.len(), 200 * 150 * 4);
        // White scene background present (opaque).
        assert!(rgba.chunks_exact(4).any(|p| p == [255, 255, 255, 255]));
    }

    /// Theme contract round (decision 323): toggling Dark repaints
    /// the page background and re-inks the default text — the hybrid
    /// from the field (dark inputs, black text, white page) came
    /// from neither following the host theme. The probe app carries
    /// no root background (transparent — the page shows through, the
    /// way the real sink root does).
    #[test]
    fn loop_dark_theme_repaints_page_and_default_ink() {
        #[derive(Clone)]
        struct BareProps;
        impl Props for BareProps {}
        fn bare_app(_ctx: &Ctx, _p: &BareProps) -> VNode {
            VNode::from(Text {
                text: SharedString::from("hi"),
                style: Text::body_secondary,
            })
        }
        let mut loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        loop_.mount("Bare", BareProps, bare_app);
        loop_.repaint().expect("light paint works");
        loop_.host().set_theme(ThemeMode::Dark);
        loop_.host().run_until_idle();
        loop_.repaint().expect("dark repaint works");
        // Page background follows the theme (opaque near-black).
        let dark_bg = ThemeTokens::dark().background;
        let dark_px = [
            ((dark_bg.0 >> 16) & 0xFF) as u8,
            ((dark_bg.0 >> 8) & 0xFF) as u8,
            (dark_bg.0 & 0xFF) as u8,
            255,
        ];
        let rgba = loop_.rgba8().expect("pixels read back");
        assert!(
            rgba.chunks_exact(4).any(|p| p == dark_px),
            "dark page background paints"
        );
        assert!(
            !rgba.chunks_exact(4).any(|p| p == [255, 255, 255, 255]),
            "no Light page white survives the toggle"
        );
        // Default text ink follows the theme (the probe carries no
        // explicit ink).
        let dark_ink = ThemeTokens::dark().text_primary;
        let inks: Vec<Color> = loop_
            .last_plan()
            .expect("plan committed")
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Text { ink, .. } => Some(*ink),
                _ => None,
            })
            .collect();
        assert!(!inks.is_empty(), "the scene carries text");
        assert!(
            inks.iter().all(|i| *i == dark_ink),
            "dark defaults follow the theme: {inks:?}"
        );
        // Toggling back restores the Light page exactly (one refit
        // per toggle, steady state churn-free).
        loop_.host().set_theme(ThemeMode::Light);
        loop_.host().run_until_idle();
        loop_.repaint().expect("light repaint works");
        let rgba = loop_.rgba8().expect("pixels read back");
        assert!(rgba.chunks_exact(4).any(|p| p == [255, 255, 255, 255]));
    }

    #[test]
    fn loop_simulated_press_flips_and_damages() {
        let (mut loop_, on) = harness();
        let id = oppa::find_retained_by_debug(loop_.host(), "flip")
            .into_iter()
            .next()
            .expect("flip node");
        let b = loop_.host().committed_box(id).expect("hit box");
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        let damage = loop_
            .step(InputEvent::pointer_down(cx, cy))
            .expect("down steps");
        let damage = damage
            + loop_
                .step(InputEvent::pointer_up(cx, cy))
                .expect("up steps");
        assert!(on.get(), "simulated press flips the signal");
        assert!(damage > 0, "the flip repaints damage");
    }

    #[test]
    fn escape_exits_only_at_root() {
        let (mut loop_, _) = harness();
        assert!(
            loop_.escape_exits(oppa::input::keys::ESCAPE, true),
            "nothing focused: Escape exits"
        );
        assert!(
            !loop_.escape_exits(oppa::input::keys::ENTER, true),
            "other keys never exit"
        );
        assert!(
            !loop_.escape_exits(oppa::input::keys::ESCAPE, false),
            "releases never exit"
        );
        // Focus the button: Escape now routes (clears focus) instead.
        let id = oppa::find_retained_by_debug(loop_.host(), "flip")
            .into_iter()
            .next()
            .expect("flip node");
        let b = loop_.host().committed_box(id).expect("hit box");
        loop_
            .step(InputEvent::pointer_down(b.x + 1.0, b.y + 1.0))
            .expect("down steps");
        loop_
            .step(InputEvent::pointer_up(b.x + 1.0, b.y + 1.0))
            .expect("up steps");
        // The press flipped focus onto the button (press owner).
        assert_eq!(loop_.host().focused_node(), Some(id));
        assert!(
            !loop_.escape_exits(oppa::input::keys::ESCAPE, true),
            "focused Escape routes, never exits"
        );
    }

    /// Minimal TextInput-shaped tree (decision 240's shape, local so
    /// this crate never depends on the controls catalog): focusable
    /// outer with its own session over the value signal.
    #[derive(Clone)]
    struct FieldProps {
        value: Signal<SharedString>,
    }
    impl Props for FieldProps {}

    fn field_app(ctx: &Ctx, props: &FieldProps) -> VNode {
        let session = ctx.edit_session(props.value.clone());
        Div("field-box")
            .style(Style::new().size(200, 32).bg(Color(0xFF_FF_FF)))
            .semantics(oppa::Semantics::text_field().label("Name"))
            .on_press(move || session.caret_to_end())
            .child(VNode::from(oppa::TextField {
                text: props.value.get(),
                style: Text::body_secondary,
                label: SharedString::from("Name"),
            }))
    }

    fn field_harness() -> (DesktopLoop, Signal<SharedString>) {
        let loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        let value = loop_.host().runtime().signal(SharedString::from(""));
        loop_.mount(
            "Field",
            FieldProps {
                value: value.clone(),
            },
            field_app,
        );
        (loop_, value)
    }

    fn focus_field(loop_: &mut DesktopLoop) {
        let id = oppa::find_retained_by_debug(loop_.host(), "field-box")
            .into_iter()
            .next()
            .expect("field box");
        let b = loop_.host().committed_box(id).expect("hit box");
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        loop_
            .step(InputEvent::pointer_down(cx, cy))
            .expect("down steps");
        loop_
            .step(InputEvent::pointer_up(cx, cy))
            .expect("up steps");
        assert_eq!(loop_.host().focused_node(), Some(id));
    }

    /// Minimal TextArea-shaped tree (Round 22.2 — local like
    /// `field_app`, with a multi-line session published the way
    /// `TextArea` publishes it).
    #[derive(Clone)]
    struct AreaProps {
        value: Signal<SharedString>,
    }
    impl Props for AreaProps {}

    fn area_app(ctx: &Ctx, props: &AreaProps) -> VNode {
        let session = ctx.edit_session(props.value.clone());
        session.set_wrap_width(Some(184.0));
        Div("area-box")
            .style(Style::new().size(200, 96).bg(Color(0xFF_FF_FF)))
            .semantics(oppa::Semantics::text_area().label("Notes"))
            .on_press(move || session.caret_to_end())
            .child(VNode::from(oppa::TextArea {
                text: props.value.get(),
                style: Text::body_secondary,
                label: SharedString::from("Notes"),
            }))
    }

    fn area_harness() -> (DesktopLoop, Signal<SharedString>) {
        let loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        let value = loop_.host().runtime().signal(SharedString::from(""));
        loop_.mount(
            "Area",
            AreaProps {
                value: value.clone(),
            },
            area_app,
        );
        (loop_, value)
    }

    fn focus_area(loop_: &mut DesktopLoop) {
        let id = oppa::find_retained_by_debug(loop_.host(), "area-box")
            .into_iter()
            .next()
            .expect("area box");
        let b = loop_.host().committed_box(id).expect("hit box");
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        loop_
            .step(InputEvent::pointer_down(cx, cy))
            .expect("down steps");
        loop_
            .step(InputEvent::pointer_up(cx, cy))
            .expect("up steps");
        assert_eq!(loop_.host().focused_node(), Some(id));
    }

    #[test]
    fn text_field_receives_typed_characters() {
        let (mut loop_, value) = field_harness();
        // Unfocused typing lands nowhere (quiet miss, router
        // precedent) — the signal is untouched.
        assert_eq!(loop_.type_text("Hi").expect("types"), 0);
        assert_eq!(value.get(), SharedString::from(""));
        focus_field(&mut loop_);
        assert!(loop_.type_text("Hi").expect("types") > 0);
        assert_eq!(value.get(), SharedString::from("Hi"));
        // Control chars never reach the session (shortcut shadows
        // ride `Key`, per the spike's WM_CHAR rule).
        assert_eq!(loop_.type_text("\x08\r").expect("types"), 0);
        assert_eq!(value.get(), SharedString::from("Hi"));
    }

    /// One pressed key with explicit modifiers (Round 22.1 — the
    /// exact shape runners feed [`DesktopLoop::step`] for Shift/Ctrl
    /// navigation).
    fn nav_key(code: u32, shift: bool, ctrl: bool) -> InputEvent {
        InputEvent::Key {
            code,
            modifiers: Modifiers {
                shift,
                ctrl,
                ..Modifiers::NONE
            },
            state: KeyState::Pressed,
            repeat: false,
        }
    }

    fn session_of(loop_: &DesktopLoop) -> oppa::EditSession {
        loop_
            .host()
            .focused_field_session()
            .expect("focused session")
    }

    /// Round 22.2 (decision 332): ArrowUp/Down travel hard lines
    /// with column affinity in a focused area, Shift+Up/Down
    /// extends, and single-line fields keep flowing Up/Down to the
    /// router (no session move, no consume).
    #[test]
    fn area_arrows_travel_hard_lines_and_single_line_ignores() {
        let (mut loop_, value) = area_harness();
        focus_area(&mut loop_);
        loop_.type_text("ab").expect("types");
        loop_
            .step(InputEvent::key(keys::ENTER, KeyState::Pressed))
            .expect("newline steps");
        loop_.type_text("cd").expect("types");
        assert_eq!(value.get(), SharedString::from("ab\ncd"));
        assert_eq!(session_of(&loop_).caret(), 5);
        loop_.step(nav_key(keys::UP, false, false)).expect("steps");
        assert_eq!(session_of(&loop_).caret(), 2, "Up onto the short line end");
        loop_
            .step(nav_key(keys::DOWN, false, false))
            .expect("steps");
        assert_eq!(session_of(&loop_).caret(), 5, "Down restores the column");
        loop_.step(nav_key(keys::UP, true, false)).expect("steps");
        assert_eq!(
            session_of(&loop_).selection(),
            (2, 5),
            "Shift+Up spans to the line above"
        );
        // Wrapped travel: 25 more chars wrap line 1 at 21 columns
        // (184px / 8.75px FakeText advances).
        loop_
            .step(nav_key(keys::DOWN, false, false))
            .expect("steps");
        loop_.type_text(&"e".repeat(25)).expect("types");
        assert_eq!(session_of(&loop_).caret(), 30);
        loop_.step(nav_key(keys::UP, false, false)).expect("steps");
        assert_eq!(
            session_of(&loop_).caret(),
            9,
            "Up keeps the column across wraps"
        );
        loop_
            .step(nav_key(keys::DOWN, false, false))
            .expect("steps");
        assert_eq!(session_of(&loop_).caret(), 30, "Down round-trips it");
        // Single-line fields ignore Up/Down (router-quiet, session
        // untouched).
        let (mut flat, _) = field_harness();
        focus_field(&mut flat);
        flat.type_text("hi").expect("types");
        flat.step(nav_key(keys::UP, false, false)).expect("steps");
        assert_eq!(
            flat.host()
                .focused_field_session()
                .expect("session")
                .caret(),
            2,
            "single-line Up moves nothing"
        );
    }

    /// Round 22.2 (decision 332): a multi-line selection commits
    /// one highlight rect per covered line (CPU plan + Vello twin
    /// retain both), and collapsing paints the line-aware caret
    /// bar on the caret's own line.
    #[test]
    fn area_multiline_selection_paints_per_line_rects() {
        let (mut loop_, _) = area_harness();
        focus_area(&mut loop_);
        loop_.type_text("ab").expect("types");
        loop_
            .step(InputEvent::key(keys::ENTER, KeyState::Pressed))
            .expect("newline steps");
        loop_.type_text("cd").expect("types");
        // Select all across both hard lines (Shift+Up from the end).
        loop_.step(nav_key(keys::UP, true, false)).expect("steps");
        loop_.step(nav_key(keys::UP, true, false)).expect("steps");
        assert_eq!(session_of(&loop_).selection(), (0, 5));
        let plan = loop_.last_plan().expect("selection repaints");
        let sels: Vec<(f32, f32, f32, f32)> = plan
            .ops
            .iter()
            .filter_map(|op| match op {
                oppa::DrawOp::Rect {
                    x, y, w, h, color, ..
                } if *color == oppa::SELECTION_FILL => Some((*x, *y, *w, *h)),
                _ => None,
            })
            .collect();
        assert_eq!(sels.len(), 2, "one rect per line, got {sels:?}");
        assert!(
            sels[1].1 > sels[0].1,
            "rects stack in line order, got {sels:?}"
        );
        // The CPU backend retains the multiline paint (the Vello
        // twin consumes the identical plan — CPU-only loops leave
        // it untouched by design, and the 7.4 GPU tests prove twin
        // encoding on text-free scenes; Vello refuses uninjected
        // faces loudly at paint, so no text paint runs headless).
        assert!(
            loop_.cpu_retained_op_count().unwrap_or(0) > 0,
            "CPU backend retains the selection paint"
        );
        // Collapse to the tail: the caret bar paints on line 1.
        loop_
            .step(nav_key(keys::DOWN, false, false))
            .expect("steps");
        assert_eq!(session_of(&loop_).selection(), (5, 5));
        let plan = loop_.last_plan().expect("caret repaints");
        let bars: Vec<(f32, f32, f32, f32)> = plan
            .ops
            .iter()
            .filter_map(|op| match op {
                oppa::DrawOp::Rect {
                    x, y, w, h, color, ..
                } if (*w - oppa::CARET_WIDTH_PX).abs() < 1e-3 && *color == oppa::INK => {
                    Some((*x, *y, *w, *h))
                }
                _ => None,
            })
            .collect();
        assert_eq!(bars.len(), 1, "one caret bar, got {bars:?}");
        assert!(
            (bars[0].1 - sels[1].1).abs() < 1e-3,
            "caret sits on the second line, bar {bars:?} vs rect {sels:?}"
        );
    }

    /// Round 22.1 (decision 331): Shift+arrows extend from the
    /// caret, Shift+Home/End span to the edges.
    #[test]
    fn shift_navigation_extends_selection() {
        let (mut loop_, _) = field_harness();
        // Unfocused: flows to the router, session untouched.
        loop_
            .step(nav_key(keys::RIGHT, true, false))
            .expect("steps");
        focus_field(&mut loop_);
        loop_.type_text("hello").expect("types");
        loop_.step(nav_key(keys::LEFT, true, false)).expect("steps");
        assert_eq!(session_of(&loop_).selection(), (4, 5));
        loop_.step(nav_key(keys::LEFT, true, false)).expect("steps");
        assert_eq!(session_of(&loop_).selection(), (3, 5));
        loop_.step(nav_key(keys::HOME, true, false)).expect("steps");
        assert_eq!(session_of(&loop_).selection(), (0, 5));
        assert_eq!(session_of(&loop_).caret(), 0);
        // The anchor persists: Shift+End walks back to it,
        // collapsing the selection onto the anchor.
        loop_.step(nav_key(keys::END, true, false)).expect("steps");
        assert_eq!(session_of(&loop_).selection(), (5, 5));
        assert_eq!(session_of(&loop_).caret(), 5);
    }

    /// Round 22.1: Ctrl+arrows word-step, Ctrl+Shift+arrows extend
    /// by word, plain arrows step by character.
    #[test]
    fn ctrl_arrows_word_step_and_plain_step() {
        let (mut loop_, _) = field_harness();
        focus_field(&mut loop_);
        loop_.type_text("hello world").expect("types");
        assert_eq!(session_of(&loop_).caret(), 11);
        loop_
            .step(nav_key(keys::LEFT, false, false))
            .expect("steps");
        assert_eq!(session_of(&loop_).caret(), 10, "plain steps a char");
        loop_.step(nav_key(keys::LEFT, false, true)).expect("steps");
        assert_eq!(
            session_of(&loop_).caret(),
            6,
            "word-steps to the word start"
        );
        loop_.step(nav_key(keys::LEFT, false, true)).expect("steps");
        assert_eq!(
            session_of(&loop_).caret(),
            0,
            "word start skips back a word"
        );
        loop_.step(nav_key(keys::LEFT, false, true)).expect("steps");
        assert_eq!(session_of(&loop_).caret(), 0, "pins at the start");
        loop_.step(nav_key(keys::RIGHT, true, true)).expect("steps");
        assert_eq!(session_of(&loop_).selection(), (0, 5), "word-extends");
        loop_
            .step(nav_key(keys::HOME, false, false))
            .expect("steps");
        assert_eq!(session_of(&loop_).caret(), 0, "Home jumps");
        loop_.step(nav_key(keys::END, false, false)).expect("steps");
        assert_eq!(session_of(&loop_).caret(), 11, "End jumps");
    }

    /// Round 22.1: copy/cut/paste round-trip through the loop
    /// clipboard, and masked fields refuse exfiltration (Ctrl+C/X
    /// no-op, content intact).
    #[test]
    fn clipboard_round_trip_and_masked_refusal() {
        let (mut loop_, _) = field_harness();
        focus_field(&mut loop_);
        loop_.type_text("hello").expect("types");
        loop_.step(ctrl_key(keys::A)).expect("steps");
        loop_.step(ctrl_key(keys::C)).expect("steps");
        assert_eq!(
            loop_.clipboard().read_text_now().expect("reads"),
            Some("hello".to_string()),
            "copy lands on the clipboard"
        );
        loop_.step(ctrl_key(keys::X)).expect("steps");
        assert_eq!(
            session_of(&loop_).content_text(),
            "",
            "cut deletes the selection"
        );
        loop_.step(ctrl_key(keys::V)).expect("steps");
        assert_eq!(
            session_of(&loop_).content_text(),
            "hello",
            "paste replaces the selection and advances"
        );
        // Masked: copy and cut refuse, content intact, clipboard
        // untouched.
        session_of(&loop_).set_masked(true);
        loop_.step(ctrl_key(keys::A)).expect("steps");
        loop_.clipboard().write_text("sentinel").expect("writes");
        loop_.step(ctrl_key(keys::C)).expect("steps");
        assert_eq!(
            loop_.clipboard().read_text_now().expect("reads"),
            Some("sentinel".to_string()),
            "masked copy exfiltrates nothing"
        );
        loop_.step(ctrl_key(keys::X)).expect("steps");
        assert_eq!(
            session_of(&loop_).content_text(),
            "hello",
            "masked cut deletes nothing"
        );
        assert_eq!(
            loop_.clipboard().read_text_now().expect("reads"),
            Some("sentinel".to_string()),
            "masked cut writes nothing"
        );
    }

    /// One pressed Ctrl+letter key event (decision 246): the exact
    /// shape both platform runners feed [`DesktopLoop::step`].
    fn ctrl_key(code: u32) -> InputEvent {
        InputEvent::Key {
            code,
            modifiers: Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
            state: KeyState::Pressed,
            repeat: false,
        }
    }

    #[test]
    fn ctrl_a_selects_all_and_ctrl_c_copies() {
        let (mut loop_, value) = field_harness();
        // Unfocused shortcuts miss quietly (router precedent).
        assert_eq!(loop_.step(ctrl_key(keys::A)).expect("steps"), 0);
        focus_field(&mut loop_);
        loop_.type_text("hello").expect("types");
        // Select-all then copy: the clipboard proves the selection
        // spanned the whole field (a collapsed caret would copy
        // nothing and leave the clipboard empty).
        loop_.step(ctrl_key(keys::A)).expect("steps");
        loop_.step(ctrl_key(keys::C)).expect("steps");
        assert_eq!(value.get(), SharedString::from("hello"));
        assert_eq!(
            loop_.clipboard().read_text_now().expect("reads"),
            Some("hello".to_string())
        );
        // Cut removes the selection and keeps the clipboard text.
        loop_.step(ctrl_key(keys::X)).expect("steps");
        assert_eq!(value.get(), SharedString::from(""));
        assert_eq!(
            loop_.clipboard().read_text_now().expect("reads"),
            Some("hello".to_string())
        );
    }

    #[test]
    fn ctrl_v_pastes_clipboard_text() {
        let (mut loop_, value) = field_harness();
        focus_field(&mut loop_);
        loop_
            .clipboard()
            .write_text("pasted")
            .expect("clipboard writes");
        loop_.step(ctrl_key(keys::V)).expect("steps");
        assert_eq!(value.get(), SharedString::from("pasted"));
        // Pasting over a selection replaces it (session `insert`
        // rule, not append).
        loop_.step(ctrl_key(keys::A)).expect("steps");
        loop_.clipboard().write_text("over").expect("writes");
        loop_.step(ctrl_key(keys::V)).expect("steps");
        assert_eq!(value.get(), SharedString::from("over"));
    }

    #[test]
    fn ctrl_z_undos_and_ctrl_y_redos() {
        let (mut loop_, value) = field_harness();
        focus_field(&mut loop_);
        loop_.type_text("ab").expect("types");
        loop_.backspace().expect("backspaces");
        assert_eq!(value.get(), SharedString::from("a"));
        // Two discrete undo levels: the backspace entry, then the
        // coalesced insert run.
        loop_.step(ctrl_key(keys::Z)).expect("steps");
        assert_eq!(value.get(), SharedString::from("ab"));
        loop_.step(ctrl_key(keys::Z)).expect("steps");
        assert_eq!(value.get(), SharedString::from(""));
        // ... and back through the same two levels.
        loop_.step(ctrl_key(keys::Y)).expect("steps");
        assert_eq!(value.get(), SharedString::from("ab"));
        loop_.step(ctrl_key(keys::Y)).expect("steps");
        assert_eq!(value.get(), SharedString::from("a"));
    }

    /// Clock-injected field rig (Round 15.2 — the headless blink
    /// harness: a `MockClock` owner advances time explicitly, so flip
    /// demand proves without sleeps).
    fn clock_harness() -> (DesktopLoop, Signal<SharedString>, Rc<oppa::MockClock>) {
        let clock = Rc::new(oppa::MockClock::new());
        let loop_ = DesktopLoop::with_clock(200, 150, Box::new(FakeText), "Test", clock.clone())
            .expect("headless loop builds");
        let value = loop_.host().runtime().signal(SharedString::from(""));
        loop_.mount(
            "Field",
            FieldProps {
                value: value.clone(),
            },
            field_app,
        );
        (loop_, value, clock)
    }

    /// Round 15.2 (decision 313): selecting text in a focused field
    /// commits themed selection `Rect`s in the loop's display list —
    /// live frames ran `build_full` with the overlay `None`, so GPU
    /// and GDI presented zero rects.
    #[test]
    fn desktop_repaint_commits_selection_rects() {
        let (mut loop_, _) = field_harness();
        focus_field(&mut loop_);
        loop_.type_text("hello world").expect("types");
        // Select-all through the session (collapsed caret → open
        // range), then commit exactly like the input path does.
        loop_
            .host()
            .focused_field_session()
            .expect("session")
            .select_all();
        loop_.repaint().expect("repaints");
        let plan = loop_.last_plan().expect("committed plan");
        let sels: Vec<(f32, f32, f32, f32)> = plan
            .ops
            .iter()
            .filter_map(|op| match op {
                oppa::DrawOp::Rect {
                    x, y, w, h, color, ..
                } if *color == oppa::SELECTION_FILL => Some((*x, *y, *w, *h)),
                _ => None,
            })
            .collect();
        assert_eq!(sels.len(), 1, "one line, one highlight, got {sels:?}");
        let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
            .into_iter()
            .next()
            .expect("field box");
        let origin_x = loop_.host().text_origin_under(field).expect("text origin");
        // 11 FakeText advances at 8.75px from the text origin.
        assert!(
            (sels[0].0 - origin_x).abs() < 1e-3 && (sels[0].2 - 96.25).abs() < 1e-2,
            "highlight pins the selected bytes, got {:?} vs origin {origin_x}",
            sels[0]
        );
        // One overlay at a time: the open range hides the caret bar.
        assert!(
            !plan.ops.iter().any(|op| matches!(op,
                oppa::DrawOp::Rect { w, color, .. }
                    if (*w - oppa::CARET_WIDTH_PX).abs() < 1e-3 && *color == oppa::INK)),
            "selected range paints no caret bar"
        );
    }

    /// Round 15.2 (decision 313): a focused collapsed caret commits
    /// its 2px bar in the loop's display list (the same zero-bar gap
    /// as the selection — one fix covers both overlays).
    #[test]
    fn desktop_repaint_commits_caret_bar() {
        let (mut loop_, _) = field_harness();
        focus_field(&mut loop_);
        // The tap reset the blink phase, and typing resets it again —
        // the bar is solid-visible at commit time (no clock needed
        // for the position pin).
        loop_.type_text("hello").expect("types");
        let plan = loop_.last_plan().expect("type_text repaints");
        let bars: Vec<(f32, f32, f32, f32)> = plan
            .ops
            .iter()
            .filter_map(|op| match op {
                oppa::DrawOp::Rect {
                    x, y, w, h, color, ..
                } if (*w - oppa::CARET_WIDTH_PX).abs() < 1e-3 && *color == oppa::INK => {
                    Some((*x, *y, *w, *h))
                }
                _ => None,
            })
            .collect();
        assert_eq!(bars.len(), 1, "exactly one caret bar, got {bars:?}");
        let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
            .into_iter()
            .next()
            .expect("field box");
        let origin_x = loop_.host().text_origin_under(field).expect("text origin");
        // Trailing caret after five FakeText advances.
        assert!(
            (bars[0].0 - (origin_x + 5.0 * 8.75)).abs() < 1e-2,
            "bar at the trailing edge, got {:?} vs {}",
            bars[0],
            origin_x + 43.75
        );
        assert!(bars[0].3 > 0.0, "positive bar height");
    }

    /// Round 15.2 (decision 313): the blink flip demands a repaint
    /// with no input — `caret_overlay_dirty` goes true on the hidden
    /// half-cycle, `poll_blink` commits it (the bar leaves the plan),
    /// and the tick horizon points at the next flip.
    #[test]
    fn blink_flip_demands_and_poll_repaints() {
        let (mut loop_, _, clock) = clock_harness();
        focus_field(&mut loop_);
        loop_.type_text("hi").expect("types");
        assert!(
            !loop_.caret_overlay_dirty(),
            "settled: snapshot matches the committed overlay"
        );
        let wake = loop_
            .blink_tick_in_secs()
            .expect("focused caret arms a tick");
        assert!(
            wake > 0.0 && wake <= 0.5,
            "horizon within the visible half, got {wake}"
        );
        clock.advance(0.6);
        assert!(loop_.caret_overlay_dirty(), "hidden flip demands a repaint");
        assert!(loop_.poll_blink().expect("polls"), "poll commits the flip");
        let plan = loop_.last_plan().expect("committed plan");
        assert!(
            !plan.ops.iter().any(|op| matches!(op,
                oppa::DrawOp::Rect { w, color, .. }
                    if (*w - oppa::CARET_WIDTH_PX).abs() < 1e-3 && *color == oppa::INK)),
            "hidden phase commits no bar"
        );
        assert!(!loop_.caret_overlay_dirty(), "snapshot follows the flip");
        assert!(
            !loop_.poll_blink().expect("polls"),
            "settled poll stays quiet"
        );
        let next = loop_.blink_tick_in_secs().expect("next flip armed");
        assert!(
            next > 0.3 && next <= 0.5,
            "wake points at the visible flip, got {next}"
        );
    }

    /// A save backend that always fails (backend-error grace shape).
    struct FailingSave;
    impl oppa::SaveFileDialog for FailingSave {
        fn save(&mut self, _: oppa::FileDialogOptions) -> Result<Option<PathBuf>, oppa::PickError> {
            Err(oppa::PickError::Backend("boom".to_string()))
        }
    }

    /// Round 16.1 (decision 314): save prompts return the scripted
    /// destination in order (each call reaches the backend — never
    /// a cached answer); dismissal and missing backends settle
    /// `None` without panic, and backend errors degrade loudly to
    /// `None` instead of killing the app.
    #[test]
    fn save_file_dialog_returns_pick_and_graceful_none() {
        use oppa::{FileDialogOptions, FileFilter, ScriptedSaveDialog};
        let mut loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        // No backend installed: graceful None (headless/web rule).
        assert_eq!(loop_.save_file_dialog(FileDialogOptions::default()), None);
        let mut scripted = ScriptedSaveDialog::new();
        scripted.push_response(Some(PathBuf::from("/tmp/a.txt")));
        scripted.push_response(Some(PathBuf::from("/tmp/b.txt")));
        loop_.set_save_dialog(Box::new(scripted));
        let options = FileDialogOptions {
            title: "Save".to_string(),
            filters: vec![FileFilter {
                name: "Text".to_string(),
                patterns: vec!["*.txt".to_string()],
            }],
            default_name: "a.txt".to_string(),
            initial_dir: Some(PathBuf::from("/tmp")),
        };
        assert_eq!(
            loop_.save_file_dialog(options),
            Some(PathBuf::from("/tmp/a.txt"))
        );
        assert_eq!(
            loop_.save_file_dialog(FileDialogOptions::default()),
            Some(PathBuf::from("/tmp/b.txt")),
            "second call reaches the backend too"
        );
        // Exhausted script = dismissed.
        assert_eq!(loop_.save_file_dialog(FileDialogOptions::default()), None);
        // Backend failure degrades to None (loud on stderr, no panic).
        loop_.set_save_dialog(Box::new(FailingSave));
        assert_eq!(loop_.save_file_dialog(FileDialogOptions::default()), None);
    }

    /// Round 16.1 (decision 314): folder prompts return the scripted
    /// directory; dismissal and missing backends settle `None`
    /// without panic.
    #[test]
    fn pick_folder_dialog_returns_dir_and_graceful_none() {
        use oppa::{FolderDialogOptions, ScriptedFolderDialog};
        let mut loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        assert_eq!(
            loop_.pick_folder_dialog(FolderDialogOptions::default()),
            None
        );
        let mut scripted = ScriptedFolderDialog::new();
        scripted.push_response(Some(PathBuf::from("/tmp/out")));
        loop_.set_folder_dialog(Box::new(scripted));
        assert_eq!(
            loop_.pick_folder_dialog(FolderDialogOptions {
                title: "Pick".to_string(),
                initial_dir: Some(PathBuf::from("/tmp")),
            }),
            Some(PathBuf::from("/tmp/out"))
        );
        assert_eq!(
            loop_.pick_folder_dialog(FolderDialogOptions::default()),
            None
        );
    }

    /// Round 16.2 (decision 315): a scripted OS reading flips the
    /// reactive theme in place — `ctx.theme().mode()` follows, the
    /// themed fill recolors in the committed plan, and redundant
    /// readings stay quiet (no repaint spin).
    #[test]
    fn sync_system_theme_forwards_os_mode_and_recolors() {
        use oppa::{ScriptedThemeSource, ThemeMode, ThemeTokens};
        #[derive(Clone)]
        struct ThemeProps;
        impl Props for ThemeProps {}
        fn theme_app(ctx: &Ctx, _: &ThemeProps) -> VNode {
            Div("themed")
                .style(Style::new().size(200, 150).bg(ctx.theme().tokens().surface))
                .build()
        }
        let mut loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        loop_.mount("Theme", ThemeProps, theme_app);
        // No source installed: quiet no-op (headless default).
        assert!(!loop_.sync_system_theme().expect("syncs"));
        assert_eq!(loop_.host().theme().mode(), ThemeMode::Light);
        let mut source = ScriptedThemeSource::new();
        source.push_reading(ThemeMode::Dark);
        loop_.set_theme_source(Box::new(source));
        assert!(
            loop_.sync_system_theme().expect("syncs"),
            "dark reading applies"
        );
        assert_eq!(loop_.host().theme().mode(), ThemeMode::Dark);
        let plan = loop_.last_plan().expect("sync repaints");
        let dark = ThemeTokens::dark().surface;
        assert!(
            plan.ops.iter().any(|op| matches!(op,
                oppa::DrawOp::Rect { color, .. } if *color == dark)),
            "themed fill recolors in place"
        );
        // Exhausted script repeats Dark: same mode stays quiet.
        assert!(!loop_.sync_system_theme().expect("syncs"));
        assert_eq!(loop_.host().theme().mode(), ThemeMode::Dark);
    }

    /// Round 16.3 (decision 316): window-chrome calls forward into
    /// the installed control in order, stay quiet with none, and
    /// the close veto defaults to closing while a refusing handler
    /// suppresses.
    #[test]
    fn window_chrome_forwards_and_close_veto_decides() {
        use oppa::{ScriptedWindowControl, WindowCall, WindowIcon};
        use std::rc::Rc;
        let loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        // No control installed: every call is a quiet no-op.
        loop_.set_title("Hi");
        loop_.set_min_size(Some((100, 80)));
        loop_.set_max_size(Some((400, 300)));
        loop_.set_fullscreen(true);
        loop_.set_icon(None);
        // No handler installed: closes (pre-16.3 behavior).
        assert!(loop_.close_requested(), "default closes");
        // The scripted double shares its record across clones: the
        // loop owns one behind the trait object, the test reads
        // through its own.
        let control = ScriptedWindowControl::new();
        let mut loop_ = loop_;
        loop_.set_window_control(Box::new(control.clone()));
        loop_.set_title("Hi");
        loop_.set_min_size(Some((100, 80)));
        loop_.set_max_size(None);
        loop_.set_fullscreen(true);
        loop_.set_fullscreen(false);
        let icon = WindowIcon::new(vec![9; 4 * 4 * 4], 4, 4).expect("valid icon builds");
        loop_.set_icon(Some(icon.clone()));
        loop_.set_icon(None);
        assert_eq!(
            control.calls(),
            vec![
                WindowCall::SetTitle("Hi".to_string()),
                WindowCall::SetMinSize(Some((100, 80))),
                WindowCall::SetMaxSize(None),
                WindowCall::SetFullscreen(true),
                WindowCall::SetFullscreen(false),
                WindowCall::SetIcon(Some(icon)),
                WindowCall::SetIcon(None),
            ]
        );
        // Veto: refusing handler suppresses, approving closes.
        loop_.set_close_handler(Rc::new(|| false));
        assert!(!loop_.close_requested(), "refusal suppresses exit");
        loop_.set_close_handler(Rc::new(|| true));
        assert!(loop_.close_requested(), "approval closes");
    }

    /// Round 21.1 (decision 328): the loop fires due component
    /// timers through the wake query + tick path — the timeout flips
    /// the signal the body reads (fire → settle → repaint), the
    /// re-render re-arms per-render, and quiet ticks settle nothing.
    #[test]
    fn loop_tick_timers_fires_settles_and_repaints() {
        #[derive(Clone)]
        struct TimerCase {
            fired: Signal<bool>,
        }
        impl Props for TimerCase {}
        fn timer_app(ctx: &Ctx, props: &TimerCase) -> VNode {
            let f = props.fired.clone();
            let f2 = props.fired.clone();
            ctx.use_timeout(100.0, move || {
                f2.set(true);
            });
            Div("root").child(
                Div("box")
                    .style(Style::new().size(if f.get() { 10.0 } else { 20.0 }, 10.0))
                    .build(),
            )
        }
        let clock = Rc::new(oppa::MockClock::new());
        let mut loop_ =
            DesktopLoop::with_clock(200, 150, Box::new(FakeText), "Test", clock.clone())
                .expect("headless loop builds");
        let fired = loop_.host().runtime().signal(false);
        loop_.mount(
            "Timer",
            TimerCase {
                fired: fired.clone(),
            },
            timer_app,
        );
        loop_.repaint().expect("paints the base");
        assert_eq!(
            loop_.next_timer_due_ms(),
            Some(100.0),
            "armed timeout names its horizon"
        );
        clock.advance(0.05);
        assert_eq!(loop_.tick_timers().expect("ticks"), 0, "quiet before due");
        assert!(!fired.get());
        clock.advance(0.05);
        assert_eq!(loop_.tick_timers().expect("ticks"), 1, "fires at due");
        assert!(fired.get(), "callback flipped the signal");
        // The fire re-rendered (tracked read) and re-armed
        // per-render: the horizon moved exactly one period out.
        assert_eq!(
            loop_.next_timer_due_ms(),
            Some(200.0),
            "re-render re-arms per-render"
        );
        assert_eq!(
            loop_.tick_timers().expect("ticks"),
            0,
            "no immediate refire"
        );
    }

    /// Scroll scene (decision 250): a 120x100 `on_scroll` list at
    /// x=40 holding one item pinned at `-offset` (tracked read — the
    /// feed write re-runs the body, so scrolled content visibly
    /// moves and the repaint damages). The canonical shape:
    /// `ctx.scroll_offset()` + `.on_scroll` + `bind_scroll`.
    #[derive(Clone)]
    struct ScrollCase;
    impl Props for ScrollCase {}

    fn scroll_app(ctx: &Ctx, _props: &ScrollCase) -> VNode {
        let offset = ctx.scroll_offset();
        Div("root").child(
            Div("list")
                .style(Style::new().size(120, 100).x(40))
                .on_scroll(|| {})
                .child(
                    Div("item")
                        .style(
                            Style::new()
                                .size(120, 30)
                                .absolute_y(-offset.get())
                                .bg(Color(0x22_66_CC)),
                        )
                        .build(),
                ),
        )
    }

    #[test]
    fn desktop_loop_mouse_wheel_scrolls_target_and_ignores_miss() {
        let mut loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        let handle = loop_.mount("Scroll", ScrollCase, scroll_app);
        let root = handle.root_instance();
        let offset = loop_.host().instance_scroll(root).expect("scroll handle");
        let list = oppa::find_retained_by_debug(loop_.host(), "list")
            .into_iter()
            .next()
            .expect("list node");
        loop_.host().bind_scroll(list, offset.clone());
        // (50, 10) hits the item — walk-up resolves the list — the
        // feed accumulates +50 and the moved item damages.
        let damage = loop_.scroll_at(50.0, 10.0, 0.0, 50.0).expect("scrolls");
        assert!(damage > 0, "scrolled content repaints");
        assert_eq!(offset.get(), 50.0);
        let item = oppa::find_retained_by_debug(loop_.host(), "item")
            .into_iter()
            .next()
            .expect("item node");
        assert_eq!(loop_.host().committed_box(item).expect("item box").y, -50.0);
        // (10, 50) hits root-only dead space — quiet miss, feed kept.
        assert_eq!(loop_.scroll_at(10.0, 50.0, 0.0, 50.0).expect("misses"), 0);
        assert_eq!(offset.get(), 50.0);
    }

    /// Horizontal scroll scene (Round 9.3, decision 302): a 200×40
    /// `on_scroll` viewport holding a 300-wide strip (bound `[0,
    /// 100]`). `scroll_at` with `dx != 0` moves the bound horizontal
    /// feed — the brief's verification, end to end through hit-test
    /// resolution.
    #[derive(Clone)]
    struct HScrollCase;
    impl Props for HScrollCase {}

    fn h_scroll_app(ctx: &Ctx, _props: &HScrollCase) -> VNode {
        let x = ctx.scroll_x();
        Div("root").child(
            oppa::ScrollArea("hlist")
                .style(Style::new().size(200, 40).x(40))
                .on_scroll(|| {})
                .child(
                    Div("wide")
                        .style(Style::new().size(300, 20).x(x.get()))
                        .build(),
                ),
        )
    }

    #[test]
    fn scroll_at_moves_the_bound_horizontal_feed() {
        let mut loop_ =
            DesktopLoop::new(400, 300, Box::new(FakeText), "Test").expect("headless loop builds");
        let handle = loop_.mount("HScroll", HScrollCase, h_scroll_app);
        let root = handle.root_instance();
        let offset_x = loop_
            .host()
            .instance_scroll_x(root)
            .expect("scroll_x handle");
        let list = oppa::find_retained_by_debug(loop_.host(), "hlist")
            .into_iter()
            .next()
            .expect("list node");
        loop_.host().bind_scroll_x(list, offset_x.clone());
        // (50, 10) hits the strip — walk-up resolves the list — `dx`
        // accumulates and the moved strip damages.
        let damage = loop_.scroll_at(50.0, 10.0, 30.0, 0.0).expect("scrolls");
        assert!(damage > 0, "scrolled content repaints");
        assert_eq!(offset_x.get(), 30.0);
        // Past the end pins at content_w - w = 100.
        loop_.scroll_at(50.0, 10.0, 500.0, 0.0).expect("scrolls");
        assert_eq!(offset_x.get(), 100.0);
    }

    #[test]
    fn text_field_backspace_deletes_character() {
        let (mut loop_, value) = field_harness();
        focus_field(&mut loop_);
        loop_.type_text("abc").expect("types");
        assert_eq!(value.get(), SharedString::from("abc"));
        loop_.backspace().expect("backspaces");
        assert_eq!(value.get(), SharedString::from("ab"));
        // Delete-forward at the end is a session no-op (still "ab").
        loop_.delete_forward().expect("deletes");
        assert_eq!(value.get(), SharedString::from("ab"));
        // Unfocused edits miss quietly too.
        loop_.step(InputEvent::Focus { node: None }).expect("blurs");
        assert_eq!(loop_.backspace().expect("backspaces"), 0);
        assert_eq!(value.get(), SharedString::from("ab"));
    }

    /// Resize scene (decision 248): an unsized root fills the
    /// viewport (the layout root rule) with one fixed plate
    /// main-axis-centered in it — the plate's x derives from the
    /// viewport width every layout pass, so stale boxes after a
    /// resize are observable. Cross-axis centering is deliberately
    /// not asserted: the root-fill fix-up stretches the root box
    /// after children place (layout.rs root rule), so root children
    /// center vertically against content height during the pass —
    /// proven engine shape, not this round's question. (No
    /// `fill_height` exists — only `fill_width` — so the root fills
    /// by leaving the size unset, not by a fill pair.)
    #[derive(Clone)]
    struct ResizeProps;
    impl Props for ResizeProps {}

    fn resize_app(_ctx: &Ctx, _props: &ResizeProps) -> VNode {
        Row("root")
            .style(Style::new().justify_content(JustifyContent::Center))
            .child(
                Div("plate")
                    .style(Style::new().size(96, 32).bg(Color(0x22_66_CC)))
                    .build(),
            )
    }

    #[test]
    fn desktop_loop_resize_refits_surface_and_recomputes_layout() {
        let mut loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        loop_.mount("Resize", ResizeProps, resize_app);
        let root = oppa::find_retained_by_debug(loop_.host(), "root")
            .into_iter()
            .next()
            .expect("root node");
        let plate = oppa::find_retained_by_debug(loop_.host(), "plate")
            .into_iter()
            .next()
            .expect("plate node");
        let root_box = loop_.host().committed_box(root).expect("root box");
        assert_eq!((root_box.w, root_box.h), (200.0, 150.0));
        // (200-96)/2 — main-axis-centered in the initial viewport.
        let plate_box = loop_.host().committed_box(plate).expect("plate box");
        assert_eq!(plate_box.x, 52.0);
        assert_eq!(loop_.rgba8().expect("pixels").len(), 200 * 150 * 4);

        loop_.resize(400, 300).expect("resize works");
        loop_.repaint().expect("repaint works");
        assert_eq!(loop_.viewport(), (400, 300));
        assert_eq!(loop_.rgba8().expect("pixels").len(), 400 * 300 * 4);
        // Root refills the new viewport; the plate re-centers —
        // without decision-248 invalidation both would still read the
        // 200x150 boxes (no demand, gated-out layout).
        let root_box = loop_.host().committed_box(root).expect("root box");
        assert_eq!((root_box.w, root_box.h), (400.0, 300.0));
        let plate_box = loop_.host().committed_box(plate).expect("plate box");
        assert_eq!(plate_box.x, 152.0);
    }

    /// Round 2.1: normalized IME feeds ride the focused session with
    /// atomic undo, and the anchor tracks the composition caret.
    #[test]
    fn ime_preedit_commit_cycle_with_atomic_undo() {
        let (mut loop_, value) = field_harness();
        // Unfocused feeds miss quietly (the type_text precedent).
        assert_eq!(loop_.feed_ime_preedit("ni", Some(2)).expect("feeds"), 0);
        assert_eq!(loop_.feed_ime_commit("你").expect("feeds"), 0);
        assert_eq!(loop_.feed_ime_cancel().expect("feeds"), 0);
        assert_eq!(value.get(), SharedString::from(""));
        focus_field(&mut loop_);
        // Preedit overlays the composition — committed value untouched.
        loop_.feed_ime_preedit("nihao", Some(5)).expect("feeds");
        assert_eq!(value.get(), SharedString::from(""));
        let session = loop_.host().focused_field_session().expect("session");
        assert_eq!(session.composition_text(), "nihao");
        assert!(session.is_composing());
        // Commit applies as one undo unit (pre-composition snapshot).
        loop_.feed_ime_commit("你好").expect("feeds");
        assert_eq!(value.get(), SharedString::from("你好"));
        assert!(!session.is_composing());
        session.undo();
        assert_eq!(value.get(), SharedString::from(""));
    }

    #[test]
    fn ime_cancel_discards_preedit_and_empty_feeds_noop() {
        let (mut loop_, value) = field_harness();
        focus_field(&mut loop_);
        loop_.feed_ime_preedit("ka", Some(2)).expect("feeds");
        let session = loop_.host().focused_field_session().expect("session");
        assert!(session.is_composing());
        loop_.feed_ime_cancel().expect("feeds");
        assert_eq!(value.get(), SharedString::from(""));
        assert!(!session.is_composing());
        // A cancel with nothing composing, an empty preedit (the
        // winit pre-commit clear), and an empty commit all no-op.
        assert_eq!(loop_.feed_ime_cancel().expect("feeds"), 0);
        assert_eq!(loop_.feed_ime_preedit("", None).expect("feeds"), 0);
        assert_eq!(loop_.feed_ime_commit("").expect("feeds"), 0);
        assert!(!session.is_composing());
    }

    #[test]
    fn ime_anchor_tracks_the_composition_caret() {
        let (mut loop_, _) = field_harness();
        assert_eq!(loop_.ime_anchor(), None, "unfocused: no anchor");
        focus_field(&mut loop_);
        // FakeText at body 14px: 8.75px/char; the field-box leaf sits
        // at the origin, so the anchor x is the caret advance.
        loop_.feed_ime_preedit("nihao", Some(2)).expect("feeds");
        let near = loop_.ime_anchor().expect("anchor while composing");
        assert_eq!(near[0], 17.5, "caret x at 2 chars, got {near:?}");
        assert_eq!((near[1], near[3]), (0.0, 14.0), "line box, got {near:?}");
        loop_.feed_ime_preedit("nihao", Some(5)).expect("feeds");
        let far = loop_.ime_anchor().expect("anchor follows caret");
        assert_eq!(far[0], 43.75, "caret x at 5 chars, got {far:?}");
        assert!(far[0] > near[0], "anchor moves with the caret");
        // After commit the anchor sits at the trailing caret of the
        // committed text (2 FakeText glyphs x 8.75).
        loop_.feed_ime_commit("你好").expect("feeds");
        let done = loop_.ime_anchor().expect("anchor after commit");
        assert_eq!(done[0], 17.5, "trailing caret kept, got {done:?}");
    }

    /// Round 2.1 Windows half: snapshotted `WM_IME_*` messages through
    /// the app mapper into the focused session, with commit-echo
    /// swallow and candidate anchoring. Headless (scripted messages —
    /// the OS IME itself is the one input no harness can script).
    #[cfg(windows)]
    mod win_ime {
        use super::*;
        use crate::windows::{drive_cmd, drive_ime, utf16_offset_to_byte, WinImeMapper};
        use oppa::EditSession;
        use oppa_shell_win::{ImeMessage, ShellConfig, ShellEvent, Win32Shell};
        use std::rc::Rc;

        fn mapper() -> WinImeMapper {
            WinImeMapper::new()
        }

        fn session_of(loop_: &DesktopLoop) -> EditSession {
            loop_.host().focused_field_session().expect("session")
        }

        fn hidden_shell() -> Win32Shell {
            Win32Shell::new(ShellConfig {
                title: "ime-test".to_string(),
                width: 200,
                height: 150,
                record_messages: false,
                suppress_os_composition_window: true,
                visible: false,
            })
            .expect("hidden test window builds")
        }

        #[test]
        fn utf16_offsets_map_to_byte_boundaries() {
            assert_eq!(utf16_offset_to_byte("nihao", 5), 5);
            assert_eq!(utf16_offset_to_byte("nihao", 99), 5);
            // "héllo": é is 2 bytes / 1 unit — unit 2 lands on byte 3.
            assert_eq!(utf16_offset_to_byte("héllo", 2), 3);
            // Surrogate pair: 2 units, 4 bytes — mid-pair floors in.
            assert_eq!(utf16_offset_to_byte("a👍b", 2), 1);
            assert_eq!(utf16_offset_to_byte("a👍b", 3), 5);
        }

        #[test]
        fn mapper_start_update_commit_cancel_sequence() {
            let (mut loop_, value) = field_harness();
            focus_field(&mut loop_);
            let mut ime = mapper();
            let s = session_of(&loop_);
            // Start over a collapsed caret: single Started.
            let evs = ime.map_message(&ImeMessage::StartComposition, &s);
            assert_eq!(
                evs,
                vec![oppa::ImeCompositionEvent::CompositionStarted { start_byte: 0 }]
            );
            loop_.feed_ime(&evs).expect("feeds");
            // Update while composing: bare Updated.
            let s = session_of(&loop_);
            let evs = ime.map_message(
                &ImeMessage::Composition {
                    gcs_flags: 0,
                    comp: Some("nihao".to_string()),
                    attrs: None,
                    cursor_pos: 5,
                    delta_start: 0,
                    result: None,
                },
                &s,
            );
            assert_eq!(
                evs,
                vec![oppa::ImeCompositionEvent::CompositionUpdated {
                    composition: "nihao".to_string(),
                    caret_byte: 5,
                }]
            );
            // Result commits and arms the echo swallow (UTF-16 units).
            let s = session_of(&loop_);
            let evs = ime.map_message(
                &ImeMessage::Composition {
                    gcs_flags: 0,
                    comp: None,
                    attrs: None,
                    cursor_pos: 0,
                    delta_start: 0,
                    result: Some("你好".to_string()),
                },
                &s,
            );
            assert_eq!(
                evs,
                vec![oppa::ImeCompositionEvent::CompositionCommitted {
                    committed: "你好".to_string(),
                }]
            );
            assert_eq!(ime.swallow_pending, 2, "two BMP units held");
            loop_.feed_ime(&evs).expect("feeds");
            assert_eq!(value.get(), SharedString::from("你好"));
            // End after the commit: nothing to cancel.
            let s = session_of(&loop_);
            assert!(ime.map_message(&ImeMessage::EndComposition, &s).is_empty());
            // A fresh update-cancel round trips through cancel.
            let s = session_of(&loop_);
            let start = ime.map_message(&ImeMessage::StartComposition, &s);
            loop_.feed_ime(&start).expect("feeds");
            let s = session_of(&loop_);
            let evs = ime.map_message(&ImeMessage::EndComposition, &s);
            assert_eq!(evs, vec![oppa::ImeCompositionEvent::CompositionCancelled]);
            loop_.feed_ime(&evs).expect("feeds");
            assert_eq!(
                value.get(),
                SharedString::from("你好"),
                "cancel keeps commits"
            );
        }

        #[test]
        fn mapper_start_replaces_the_selection_atomically() {
            let (mut loop_, value) = field_harness();
            focus_field(&mut loop_);
            loop_.type_text("hello").expect("types");
            let s = session_of(&loop_);
            s.select_all();
            let mut ime = mapper();
            let evs = ime.map_message(&ImeMessage::StartComposition, &s);
            assert_eq!(
                evs,
                vec![
                    oppa::ImeCompositionEvent::CompositionStarted { start_byte: 0 },
                    oppa::ImeCompositionEvent::DeleteRange { range: (0, 5) },
                ],
                "started-before-delete (pre-deletion undo snapshot)"
            );
            loop_.feed_ime(&evs).expect("feeds");
            loop_.feed_ime_commit("你好").expect("feeds");
            assert_eq!(value.get(), SharedString::from("你好"));
            session_of(&loop_).undo();
            assert_eq!(
                value.get(),
                SharedString::from("hello"),
                "undo restores pre-selection"
            );
        }

        #[test]
        fn drive_cmd_swallows_commit_echo_but_passes_real_chars() {
            let (mut loop_, value) = field_harness();
            focus_field(&mut loop_);
            let mut shell = hidden_shell();
            let mut ime = mapper();
            // Commit "ab" (2 echo units) through the driver.
            let s = session_of(&loop_);
            let evs = ime.map_message(
                &ImeMessage::Composition {
                    gcs_flags: 0,
                    comp: None,
                    attrs: None,
                    cursor_pos: 0,
                    delta_start: 0,
                    result: Some("ab".to_string()),
                },
                &s,
            );
            loop_.feed_ime(&evs).expect("feeds");
            assert_eq!(value.get(), SharedString::from("ab"));
            // The two echo WM_CHARs die quietly.
            for ch in ['a', 'b'] {
                let out = drive_cmd(
                    &mut loop_,
                    &mut shell,
                    &mut ime,
                    oppa_shell_win::Cmd::Char { ch },
                )
                .expect("drives");
                assert!(!out.damaged, "echo paints nothing");
            }
            assert_eq!(value.get(), SharedString::from("ab"));
            assert_eq!(ime.swallow_pending, 0, "echo consumed");
            // A real keystroke clears nothing pending (already zero)
            // and its char inserts.
            drive_cmd(
                &mut loop_,
                &mut shell,
                &mut ime,
                oppa_shell_win::Cmd::Key {
                    vk: oppa::input::keys::A,
                    shift: false,
                    ctrl: false,
                },
            )
            .expect("drives");
            drive_cmd(
                &mut loop_,
                &mut shell,
                &mut ime,
                oppa_shell_win::Cmd::Char { ch: 'c' },
            )
            .expect("drives");
            assert_eq!(value.get(), SharedString::from("abc"));
        }

        #[test]
        fn drive_ime_end_to_end_feeds_and_anchors() {
            let (mut loop_, value) = field_harness();
            focus_field(&mut loop_);
            let mut shell = hidden_shell();
            let mut ime = mapper();
            let seq = [
                ImeMessage::StartComposition,
                ImeMessage::Composition {
                    gcs_flags: 0,
                    comp: Some("ni".to_string()),
                    attrs: None,
                    cursor_pos: 2,
                    delta_start: 0,
                    result: None,
                },
                ImeMessage::Composition {
                    gcs_flags: 0,
                    comp: None,
                    attrs: None,
                    cursor_pos: 0,
                    delta_start: 0,
                    result: Some("你".to_string()),
                },
                ImeMessage::EndComposition,
            ];
            let mut damaged = false;
            for msg in &seq {
                damaged |= drive_ime(&mut loop_, &mut shell, &mut ime, msg).expect("drives");
            }
            assert!(damaged, "composition damages");
            assert_eq!(value.get(), SharedString::from("你"));
            // The candidate anchor tracked the session (run-relative
            // rect at the trailing caret of one CJK char).
            let anchor = loop_.ime_anchor().expect("anchor after commit");
            assert_eq!(anchor[0], 8.75, "one FakeText advance, got {anchor:?}");
            let anchored = shell.take_anchored_rects();
            assert!(!anchored.is_empty(), "shell anchored the candidate");
            assert_eq!(
                anchored.last().unwrap()[0],
                anchor[0],
                "shell saw the live rect, got {anchored:?}"
            );
            session_of(&loop_).undo();
            assert_eq!(value.get(), SharedString::from(""));
        }

        #[test]
        fn pump_moves_queue_to_cmds_and_fires_the_ime_callback() {
            use oppa::shell::PlatformShell;
            let mut shell = hidden_shell();
            let seen: Rc<std::cell::RefCell<Vec<ImeMessage>>> =
                Rc::new(std::cell::RefCell::new(Vec::new()));
            {
                let seen = seen.clone();
                shell.set_ime_callback(Rc::new(move |msg| seen.borrow_mut().push(msg.clone())));
            }
            shell
                .shared
                .borrow_mut()
                .queue
                .push_back(ShellEvent::Char { ch: 'a' });
            shell
                .shared
                .borrow_mut()
                .queue
                .push_back(ShellEvent::Ime(ImeMessage::StartComposition));
            let _ = shell.pump_events();
            assert_eq!(
                *seen.borrow(),
                vec![ImeMessage::StartComposition],
                "callback fires for IME messages"
            );
            assert_eq!(
                shell.take_cmds(),
                vec![oppa_shell_win::Cmd::Char { ch: 'a' }],
                "chars pair to commands after the pump (the missing-pump fix)"
            );
        }
    }

    /// Round 2.4 (decision 259): DPR re-basing keeps CSS stable,
    /// doubles device geometry, and re-shapes text.
    #[test]
    fn set_device_pixel_ratio_rescales_device_keep_css() {
        let (mut loop_, _) = field_harness();
        // field_harness mounts 200x150 at dpr 1: field-box 200x32.
        let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
            .into_iter()
            .next()
            .expect("field box");
        let before = loop_.host().committed_box(field).expect("field box");
        assert_eq!((before.w, before.h), (200.0, 32.0));
        assert_eq!(loop_.dpr(), 1.0);
        loop_.set_device_pixel_ratio(2.0).expect("re-bases");
        assert_eq!(loop_.dpr(), 2.0);
        // CSS stable (200x150), device doubled: viewport() reads
        // device px, so it doubles too.
        assert_eq!(loop_.viewport(), (400, 300));
        let after = loop_.host().committed_box(field).expect("field box");
        assert_eq!((after.w, after.h), (400.0, 64.0), "device geometry doubles");
        assert_eq!(
            loop_.rgba8().expect("pixels").len(),
            400 * 300 * 4,
            "surface refits to device px"
        );
        // Text re-shaped at the new scale (measure keys carry the
        // DPR — a re-shape ran, not a stale reuse).
        assert!(
            loop_.host().layout_stats().nodes_shaped > 0,
            "text re-shapes at the new DPR"
        );
        // Same-value re-set is a no-op.
        loop_.set_device_pixel_ratio(2.0).expect("no-op");
        // Back down restores exactly.
        loop_.set_device_pixel_ratio(1.0).expect("re-bases");
        let back = loop_.host().committed_box(field).expect("field box");
        assert_eq!((back.w, back.h), (200.0, 32.0));
        assert_eq!(loop_.rgba8().expect("pixels").len(), 200 * 150 * 4);
    }

    #[test]
    fn resize_at_non_unity_dpr_divides_to_css() {
        let (mut loop_, _) = field_harness();
        loop_.set_device_pixel_ratio(2.0).expect("re-bases");
        loop_.resize(400, 300).expect("resize works");
        // 400x300 device px at dpr 2 = 200x150 CSS: the field keeps
        // its full-width row (block child of the root).
        let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
            .into_iter()
            .next()
            .expect("field box");
        let box_ = loop_.host().committed_box(field).expect("field box");
        assert_eq!((box_.w, box_.h), (400.0, 64.0));
        assert_eq!(loop_.viewport(), (400, 300));
    }

    #[test]
    #[should_panic(expected = "refused")]
    fn zero_dpr_refuses_loudly() {
        let (mut loop_, _) = field_harness();
        let _ = loop_.set_device_pixel_ratio(0.0);
    }

    #[test]
    #[should_panic(expected = "refused")]
    fn nan_dpr_refuses_loudly() {
        let (mut loop_, _) = field_harness();
        let _ = loop_.set_device_pixel_ratio(f32::NAN);
    }

    /// Round 7.4 (decision 279): `OPPA_RENDERER` parsing is pure and
    /// case-insensitive; anything unrecognized resolves to auto
    /// (GPU-first with loud CPU fallback — the wrapper logs).
    #[test]
    fn renderer_override_parses_cpu_gpu_auto() {
        assert_eq!(parse_renderer_override(None), None);
        assert_eq!(
            parse_renderer_override(Some("cpu")),
            Some(RendererKind::Cpu)
        );
        assert_eq!(
            parse_renderer_override(Some("CPU")),
            Some(RendererKind::Cpu)
        );
        assert_eq!(
            parse_renderer_override(Some("gpu")),
            Some(RendererKind::Gpu)
        );
        assert_eq!(
            parse_renderer_override(Some("  Gpu  ")),
            Some(RendererKind::Gpu)
        );
        assert_eq!(parse_renderer_override(Some("")), None);
        assert_eq!(parse_renderer_override(Some("vulkan")), None);
    }

    #[test]
    fn desktop_with_hook_configures_live_loop() {
        // The `run_desktop_with` hook contract, headlessly: the same
        // `FnOnce(&mut DesktopLoop)` closure shape the runners invoke
        // after mount installs a close veto that `close_requested`
        // consults (the cookbook's dirty-gated veto).
        let mut loop_ =
            DesktopLoop::new(400, 300, Box::new(FakeText), "Hook").expect("headless loop");
        let dirty = loop_.host().runtime().signal(true);
        let gate = dirty.clone();
        let configure = |loop_: &mut DesktopLoop| {
            loop_.set_close_handler(Rc::new(move || !gate.get()));
        };
        configure(&mut loop_);
        assert!(!loop_.close_requested(), "dirty vetoes close");
        dirty.set(false);
        assert!(loop_.close_requested(), "clean closes");
    }

    #[test]
    fn poll_hook_runs_restores_and_removes() {
        // Round 26.2 (decision 342): installed hooks run once per
        // `run_poll_hook` call against the live loop, survive the
        // call (take-call-restore), and `None` removes them.
        let mut loop_ =
            DesktopLoop::new(400, 300, Box::new(FakeText), "Hook").expect("headless loop");
        let fires = Rc::new(std::cell::Cell::new(0usize));
        let count = fires.clone();
        loop_.set_poll_hook(Some(Box::new(move |loop_: &mut DesktopLoop| {
            count.set(count.get() + 1);
            // The hook observes the live loop (viewport here;
            // Task Studio reads hooks signals the same way).
            let _ = loop_.viewport();
        })));
        loop_.run_poll_hook();
        loop_.run_poll_hook();
        assert_eq!(fires.get(), 2, "hook runs every invocation");
        loop_.set_poll_hook(None);
        loop_.run_poll_hook();
        assert_eq!(fires.get(), 2, "removal stops calls");
    }

    #[test]
    fn close_flag_drains_through_veto_consult() {
        // The runner contract, headlessly: a drained component
        // request consults the veto -- refused while dirty, exit
        // once clean (what both pumps do per iteration).
        let mut loop_ =
            DesktopLoop::new(400, 300, Box::new(FakeText), "Hook").expect("headless loop");
        let dirty = loop_.host().runtime().signal(true);
        let gate = dirty.clone();
        loop_.set_close_handler(Rc::new(move || !gate.get()));
        loop_.host().request_close();
        assert!(loop_.host().take_close_request(), "flag drains");
        assert!(!loop_.close_requested(), "dirty veto holds");
        dirty.set(false);
        loop_.host().request_close();
        assert!(loop_.host().take_close_request(), "flag drains again");
        assert!(loop_.close_requested(), "clean exits");
    }

    /// Round 7.4: headless loops start CPU-only with both scene
    /// surfaces live (Vello twin untouched until a runner enables
    /// GPU — a faceless shaper must never trip the Vello loud-no-face
    /// rule on the CPU path).
    #[test]
    fn loop_starts_cpu_only_with_both_surfaces() {
        let (mut loop_, _) = harness();
        assert_eq!(loop_.renderer_kind(), RendererKind::Cpu);
        assert!(!loop_.is_gpu());
        assert_eq!(loop_.renderer_name(), "cpu");
        assert_eq!(loop_.gpu_present_mode(), None);
        let damage = loop_.repaint().expect("repaints");
        assert!(damage > 0);
        assert!(loop_.cpu_retained_op_count().unwrap_or(0) > 0);
        assert_eq!(
            loop_.vello_retained_op_count(),
            Some(0),
            "CPU-only repaints leave the Vello twin untouched"
        );
        assert_eq!(loop_.rgba8().expect("pixels").len(), 200 * 150 * 4);
    }

    /// Text-free scene for GPU paint tests (a Text run without an
    /// injected face would refuse loudly on Vello by contract, while
    /// the CPU path still draws bars — so the dual test stays
    /// faceless by having no text at all).
    #[derive(Clone)]
    struct PlainProps;
    impl Props for PlainProps {}

    fn plain_app(_ctx: &Ctx, _props: &PlainProps) -> VNode {
        Div("root")
            .style(Style::new().size(200, 150).bg(Color(0xFF_FF_FF)))
            .child(
                Div("plate")
                    .style(Style::new().size(96, 32).bg(Color(0x22_66_CC)))
                    .build(),
            )
    }

    /// Round 7.4: enabling GPU paints the same frame plan into both
    /// backends (CPU pixmap stays the fallback source); disabling
    /// returns to CPU-only without losing the viewport.
    #[test]
    fn enable_gpu_paints_both_backends_then_disables() {
        let mut loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        loop_.mount("Plain", PlainProps, plain_app);
        loop_.enable_gpu(wgpu::PresentMode::Fifo);
        assert!(loop_.is_gpu());
        assert_eq!(loop_.renderer_name(), "gpu");
        assert_eq!(loop_.gpu_present_mode(), Some(wgpu::PresentMode::Fifo));
        let damage = loop_.repaint().expect("gpu repaint works");
        assert!(damage > 0);
        assert!(loop_.cpu_retained_op_count().unwrap_or(0) > 0);
        assert!(
            loop_.vello_retained_op_count().unwrap_or(0) > 0,
            "Vello twin encodes the same plan"
        );
        assert_eq!(loop_.rgba8().expect("pixels").len(), 200 * 150 * 4);
        loop_.disable_gpu();
        assert!(!loop_.is_gpu());
        assert_eq!(loop_.gpu_present_mode(), None);
        loop_.repaint().expect("cpu repaint after disable works");
    }

    /// Round 7.4: refit keeps both surfaces in lockstep (a later
    /// `enable_gpu` never presents a stale size).
    #[test]
    fn resize_refits_both_surfaces_in_lockstep() {
        let mut loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        loop_.mount("Plain", PlainProps, plain_app);
        loop_.enable_gpu(wgpu::PresentMode::Fifo);
        loop_.repaint().expect("paints");
        loop_.resize(400, 300).expect("resize works");
        assert_eq!(loop_.viewport(), (400, 300));
        loop_.repaint().expect("paints at the new size");
        assert_eq!(loop_.rgba8().expect("pixels").len(), 400 * 300 * 4);
        assert!(loop_.vello_retained_op_count().unwrap_or(0) > 0);
    }

    /// Round 2.4 Windows half (decision 259): a real
    /// `WM_DPICHANGED` through the window proc snapshots the
    /// suggested rect + DPI into a pipeline command, and driving it
    /// re-bases + resizes + repaints. Headless (hidden window + sent
    /// message — the OS crossing itself is the one input no harness
    /// can script).
    #[cfg(windows)]
    mod win_dpi {
        use super::*;
        use crate::windows::{drive_cmd, WinImeMapper};
        use oppa_shell_win::{Cmd, ShellConfig, Win32Shell};

        fn hidden_shell() -> Win32Shell {
            Win32Shell::new(ShellConfig {
                title: "dpi-test".to_string(),
                width: 200,
                height: 150,
                record_messages: false,
                suppress_os_composition_window: true,
                visible: false,
            })
            .expect("hidden test window builds")
        }

        #[test]
        fn wndproc_dpi_changed_snapshots_rect_and_dpi() {
            use ::windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
            use ::windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_DPICHANGED};
            use oppa::shell::PlatformShell;
            let mut shell = hidden_shell();
            let rect = RECT {
                left: 10,
                top: 20,
                right: 810,
                bottom: 620,
            };
            let dpi = 192u32;
            unsafe {
                SendMessageW(
                    shell.hwnd(),
                    WM_DPICHANGED,
                    Some(WPARAM(((dpi << 16) | dpi) as usize)),
                    Some(LPARAM(&rect as *const RECT as isize)),
                );
            }
            let _ = shell.pump_events();
            assert_eq!(
                shell.take_cmds(),
                vec![Cmd::DpiChanged {
                    x: 10,
                    y: 20,
                    w: 800,
                    h: 600,
                    dpi: 192,
                }],
                "suggested rect + x-axis DPI snapshot at message time"
            );
        }

        #[test]
        fn drive_cmd_dpi_changed_rebases_and_repaints() {
            let (mut loop_, _) = field_harness();
            let mut shell = hidden_shell();
            let mut ime = WinImeMapper::new();
            let out = drive_cmd(
                &mut loop_,
                &mut shell,
                &mut ime,
                Cmd::DpiChanged {
                    x: 0,
                    y: 0,
                    w: 400,
                    h: 300,
                    dpi: 192,
                },
            )
            .expect("drives");
            assert!(out.damaged, "repaint follows the re-base");
            assert_eq!(loop_.dpr(), 2.0, "192 dpi re-bases to 2.0");
            assert_eq!(loop_.viewport(), (400, 300));
            let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
                .into_iter()
                .next()
                .expect("field box");
            let box_ = loop_.host().committed_box(field).expect("field box");
            assert_eq!((box_.w, box_.h), (400.0, 64.0), "crisp at 2x: {box_:?}");
            assert_eq!(loop_.rgba8().expect("pixels").len(), 400 * 300 * 4);
        }
    }
}
