//! M1 remainder — the minimal Windows window shell.
//!
//! Scope (per the round brief): a real Win32 window — enough to host one
//! editable text field and receive real input, nothing more. The window
//! proc is wired to the M0 [`PlatformShell`] trait (`pump_events`,
//! `set_ime` from M0b) and wires the real OS IME messages (`WM_IME_*`
//! through `ImmGetCompositionString`) onto the *existing* pipeline:
//! `dispatch_ime_event` → the host-provided `ImeCompositionHandler` sink —
//! the same seam the M1 spike proved with scripted input, now fed by a
//! real IME.
//!
//! Deliberately NOT here (round scope): menus, multi-window, resize
//! polish, theming, any widget model. This is the vehicle for the manual
//! real-IME pass, nothing more.

/// The live IME engagement of the window (open status, conversion and
/// sentence mode, the active HKL) — recorded per pass step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImeState {
    pub context_open: bool,
    pub conversion_mode: u32,
    pub sentence_mode: u32,
    pub active_hkl: usize,
}

#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod file_dialog;
#[cfg(windows)]
pub mod save_dialog;
#[cfg(windows)]
pub mod theme;
#[cfg(windows)]
pub mod tsf;
#[cfg(windows)]
pub mod win;
#[cfg(windows)]
pub mod window;

#[cfg(windows)]
pub use clipboard::Win32Clipboard;
#[cfg(windows)]
pub use file_dialog::{build_filter_string, parse_dialog_result, Win32FileDialog};
#[cfg(windows)]
pub use save_dialog::{
    build_filter_specs, default_ext_from_filters, is_dismissal, wide_nul, Win32FolderDialog,
    Win32SaveDialog,
};
#[cfg(windows)]
pub use theme::{
    is_immersive_color_set, read_apps_use_light_theme, theme_mode_from_registry_dword,
    Win32SystemTheme,
};
#[cfg(windows)]
pub use tsf::{TsfBridge, TsfStatus};
#[cfg(windows)]
pub use win::{
    Cmd, ImeMessage, MessageRecord, ShellConfig, ShellEvent, ShellShared, WaitOutcome, Win32Shell,
    FIELD_EVENT,
};
#[cfg(windows)]
pub use window::Win32WindowControl;
