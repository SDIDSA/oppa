//! Desktop application runner (decision 242): window + event/paint
//! loop over a [`ComponentHost`] scene, so running a visual UI is one
//! call instead of ~150 lines of platform boilerplate.
//!
//! Shape: this crate owns the platform-free [`DesktopLoop`] —
//! host, mount, input-inject, repaint through the GPU-first engine
//! (GPU rasterizer hardware swapchains primary, [`CpuBackend`]
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
    dispatch_ime_event, Clipboard, Clock, ComponentHost, Ctx, DebugOverlayConfig,
    FileDialogOptions, FolderDialog, FolderDialogOptions, FramePlan, ImeCompositionEvent,
    InMemoryClipboard, InputEvent, KeyState, LayoutTextConfig, Modifiers, MountHandle,
    PasteOutcome, Props, RendererBackend, SaveFileDialog, SurfaceDesc, SurfaceId, SystemClock,
    SystemThemeSource, ThemeTokens, VNode, WindowControl, WindowIcon,
};
use oppa_cpu::{CpuBackend, FramePlanBuilder};
use oppa_gpu::GpuBackend;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

mod frame;
mod gpu;
mod resize;
mod scene;
mod step;
mod text_input;
mod wiring;

/// The unified platform-free loop engine (decision 397): shared by
/// `DesktopLoop` and the Android runner.
pub use scene::SceneCore;

/// Window launch parameters.
#[derive(Clone, Debug)]
pub struct WindowOptions {
    pub title: String,
    pub width: u32,
    pub height: u32,
    /// Debug performance overlay configuration.
    pub debug_overlay: DebugOverlayConfig,
    /// Uncapped continuous repaint (benchmark mode): the runner polls
    /// with a 0ms wait horizon and repaints + presents every
    /// iteration instead of sleeping until input. Default `false`
    /// (event-driven, near-0% idle CPU). The
    /// `OPPA_CONTINUOUS_REPAINT=1` / `OPPA_BENCHMARK=1` environment
    /// flags force it on at runner startup (see
    /// [`continuous_repaint_override`]).
    pub continuous_repaint: bool,
}

impl WindowOptions {
    pub fn new(title: &str, width: u32, height: u32) -> Self {
        Self {
            title: title.to_string(),
            width: width.max(1),
            height: height.max(1),
            debug_overlay: DebugOverlayConfig::default(),
            continuous_repaint: false,
        }
    }

    /// Sets the debug performance overlay configuration.
    pub fn with_debug_overlay(mut self, config: DebugOverlayConfig) -> Self {
        self.debug_overlay = config;
        self
    }

    /// Convenience setter to enable or disable the overlay with all metrics.
    pub fn with_fps_counter(mut self, enabled: bool) -> Self {
        self.debug_overlay = if enabled {
            DebugOverlayConfig::all()
        } else {
            DebugOverlayConfig::disabled()
        };
        self
    }

    /// Sets uncapped continuous repaint (benchmark mode): the runner
    /// polls with a 0ms wait horizon and repaints + presents every
    /// iteration. Default `false`. The `OPPA_CONTINUOUS_REPAINT=1` /
    /// `OPPA_BENCHMARK=1` environment flags force it on at runner
    /// startup regardless of this setting (see
    /// [`continuous_repaint_override`]).
    pub fn with_continuous_repaint(mut self, enabled: bool) -> Self {
        self.continuous_repaint = enabled;
        self
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

/// Platform-free scene driver: host + paint surfaces + repaint
/// (GPU rasterizer hardware swapchains primary, CPU pixmap
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

/// Which scene rasterizer the loop presents (decision 388: the
/// GPU rasterizer is the only GPU presenter — Vello is removed):
/// hardware custom swapchains primary, CPU pixmap fallback. The loop
/// always owns both paint surfaces; `active` selects which one the
/// platform glue presents (GPU via `present_gpu`, CPU via `rgba8`
/// for GDI / softbuffer). Headless loops stay [`RendererKind::Cpu`]
/// until a runner enables GPU against a live window.
/// `Gpu` / `GpuAuto` request the rasterizer with automatic backend
/// choice (Vulkan-first on Windows); `GpuVulkan` / `GpuDx12` /
/// `GpuGl` pin its wgpu backend explicitly.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RendererKind {
    Cpu,
    Gpu,
    GpuAuto,
    GpuVulkan,
    GpuDx12,
    GpuGl,
}

impl RendererKind {
    /// True for every GPU variant (automatic, alias, + backend
    /// pins) — i.e. everything but [`RendererKind::Cpu`].
    pub fn is_gpu(self) -> bool {
        !matches!(self, RendererKind::Cpu)
    }
}

/// Parses one `OPPA_RENDERER` value (pure — headless-testable):
/// `cpu` forces the softbuffer/GDI path, `gpu` / `gpu-auto`
/// request the rasterizer (automatic backend), `gpu-vulkan` /
/// `gpu-dx12` / `gpu-gl` request it pinned to that wgpu backend.
/// Retired spellings (`vello*`, `custom*`) are unrecognized (loud
/// auto, never a silent substitution — decisions 388–389).
/// Anything else (including `None`) is auto (GPU-first with loud
/// CPU fallback). Case-insensitive; surrounding whitespace ignored;
/// hyphen/underscore spellings both accepted.
pub fn parse_renderer_override(value: Option<&str>) -> Option<RendererKind> {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("cpu") => Some(RendererKind::Cpu),
        Some("gpu") => Some(RendererKind::Gpu),
        Some("gpu-auto") | Some("gpu_auto") => Some(RendererKind::GpuAuto),
        Some("gpu-vulkan") | Some("gpu_vulkan") => Some(RendererKind::GpuVulkan),
        Some("gpu-dx12") | Some("gpu_dx12") => Some(RendererKind::GpuDx12),
        Some("gpu-gl") | Some("gpu_gl") => Some(RendererKind::GpuGl),
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
                "oppa-app: OPPA_RENDERER={v:?} unrecognized (expected cpu|gpu|gpu-auto|gpu-vulkan|gpu-dx12|gpu-gl) — auto (GPU-first, CPU fallback)"
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
/// (`OPPA_RENDERER=gpu*`): runners attempt GPU and fall back loudly
/// when it refuses (never a silent CPU substitution).
pub fn gpu_forced_by_env() -> bool {
    matches!(renderer_override(), Some(kind) if kind.is_gpu())
}

/// The winning `wgpu` backend underneath a GPU presenter
/// (decision 387): recorded by the runner that won bring-up, so the
/// debug badge can name the actual API instead of a hardcoded
/// presenter label. `None` until a runner records one (headless
/// loops stay `None` — the badge then reads the bare presenter,
/// never a guess).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GpuApi {
    Vulkan,
    Dx12,
    Gl,
}

impl GpuApi {
    /// Badge/API label (`Vulkan` / `DX12` / `GL`).
    pub fn label(self) -> &'static str {
        match self {
            GpuApi::Vulkan => "Vulkan",
            GpuApi::Dx12 => "DX12",
            GpuApi::Gl => "GL",
        }
    }

    /// Bring-up row name (`vulkan` / `dx12` / `gl`, matching the
    /// `backend_attempts` labels the runners log).
    pub fn row_name(self) -> &'static str {
        match self {
            GpuApi::Vulkan => "vulkan",
            GpuApi::Dx12 => "dx12",
            GpuApi::Gl => "gl",
        }
    }

    /// Parses a bring-up row name (the inverse of
    /// [`GpuApi::row_name`]): `None` for anything else (callers
    /// fall back loudly, never guess).
    pub fn from_row_name(name: &str) -> Option<GpuApi> {
        match name {
            "vulkan" => Some(GpuApi::Vulkan),
            "dx12" => Some(GpuApi::Dx12),
            "gl" => Some(GpuApi::Gl),
            _ => None,
        }
    }
}

/// Parses one continuous-repaint flag value (pure —
/// headless-testable): `1` / `true` (case-insensitive, surrounding
/// whitespace ignored) arms uncapped rendering; anything else
/// (including `None`) leaves the event-driven loop alone.
pub fn parse_continuous_repaint_override(value: Option<&str>) -> bool {
    matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("1") | Some("true")
    )
}

/// Reads the continuous-repaint environment override
/// (`OPPA_CONTINUOUS_REPAINT` or `OPPA_BENCHMARK`): either flag set
/// to `1` / `true` forces benchmark mode at runner startup,
/// regardless of [`WindowOptions::continuous_repaint`]. Unrecognized
/// non-empty values log loudly once and resolve to off (never a
/// silent reinterpretation — the `OPPA_RENDERER` precedent).
pub fn continuous_repaint_override() -> bool {
    let mut on = false;
    for var in ["OPPA_CONTINUOUS_REPAINT", "OPPA_BENCHMARK"] {
        let raw = std::env::var(var).ok();
        if parse_continuous_repaint_override(raw.as_deref()) {
            on = true;
        } else if let Some(v) = raw {
            if !v.trim().is_empty() {
                eprintln!(
                    "oppa-app: {var}={v:?} unrecognized (expected 1|true) — continuous repaint off"
                );
            }
        }
    }
    on
}

/// Poll hook callback for [`DesktopLoop`] pump iterations (Round 26.2, decision 342).
pub type DesktopPollHook = Box<dyn FnMut(&mut DesktopLoop)>;

pub struct DesktopLoop {
    /// The unified platform-free loop engine (decision 397): host +
    /// plan builder + both paint arms + resize/DPR/theme machinery +
    /// GPU bring-up state + frame telemetry. Everything here is
    /// shared with the Android runner verbatim — a bug fixed in
    /// `scene.rs` fixes desktop Windows/Linux and Android at once.
    scene: SceneCore,
    /// The clipboard the editing shortcuts read and write (decision
    /// 246). Session-local memory by default (headless-correct — the
    /// unit tests copy/paste with zero setup); the platform
    /// glue replaces it with the OS backend where one exists
    /// (`run_windows` installs the Win32 clipboard, `run_linux` the
    /// X11 one — copy/paste then work system-wide, stated in the
    /// round logs).
    clipboard: Box<dyn Clipboard>,
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
    /// Whether the loop repaints continuously (benchmark mode): the
    /// runner polls with a 0ms wait horizon and repaints + presents
    /// every iteration instead of sleeping until input. Default
    /// `false` (event-driven); set through
    /// [`WindowOptions::with_continuous_repaint`],
    /// [`DesktopLoop::set_continuous_repaint`], or the
    /// `OPPA_CONTINUOUS_REPAINT=1` / `OPPA_BENCHMARK=1` environment
    /// flags (see [`continuous_repaint_override`]).
    continuous_repaint: bool,
}

impl DesktopLoop {
    /// The unified loop engine (borrow for mount records, plan
    /// reads, GPU state — everything [`SceneCore`] owns).
    pub fn scene(&self) -> &SceneCore {
        &self.scene
    }

    /// Mutable engine access (tests flip renderer state directly —
    /// the old `active` field writes moved here with the field).
    #[cfg(test)]
    pub(crate) fn scene_mut(&mut self) -> &mut SceneCore {
        &mut self.scene
    }

    /// CPU retained op count (test symmetry with the GPU twin).
    pub fn cpu_retained_op_count(&self) -> Option<usize> {
        self.scene.cpu_retained_op_count()
    }

    pub fn host(&self) -> &ComponentHost {
        self.scene.host()
    }

    pub fn viewport(&self) -> (u32, u32) {
        self.scene.viewport()
    }
}

#[cfg(test)]
mod tests;
