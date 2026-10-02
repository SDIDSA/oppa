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

mod frame;
mod gpu;
mod resize;
mod step;
mod text_input;
mod wiring;

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
