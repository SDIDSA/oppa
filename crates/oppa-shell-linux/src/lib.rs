//! Linux window shell (v1 remainder, Gap 4): a winit window with
//! a softbuffer CPU present path.
//!
//! Stack rationale (stated): winit + softbuffer are pure-Rust
//! crates that reach X11/Wayland through runtime `dlopen` — no C
//! headers or `-dev` packages at build time, which is what makes
//! this shell buildable on a sudo-blocked image where the runtime
//! libs (`libX11`, `libwayland-client`, `libxkbcommon`) are already
//! present. No HarfBuzz/fontconfig linkage anywhere (text rides
//! `oppa-text-linux`, rustybuzz over the system font dir).
//!
//! Bounds (v1): window open, CPU pixmap present, surface sizing,
//! and input mapping (`input.rs`: winit events into the shared
//! `InputEvent` pipeline mirroring `oppa-shell-android`).
//! IME policy is the named follow-up, not smuggled in.

pub mod clipboard;
pub mod dbus;
pub mod file_dialog;
pub mod input;
pub mod theme;
pub mod window;

pub use clipboard::{LinuxClipboard, INCR_CHUNK, INCR_THRESHOLD, SELECTION_TIMEOUT};
pub use file_dialog::{zenity_argv, LinuxFileDialog, StdRunner};
pub use input::{
    keycode_to_framework, translate, translate_modifiers, LinuxCmd, LinuxEvent, LinuxIntakeStats,
    LinuxShell, LinuxShellError, ShellConfig,
};
pub use theme::{color_scheme_to_theme, LinuxSystemTheme, LinuxThemeWatcher};
pub use window::LinuxWindowControl;

use std::num::NonZeroU32;

/// Packs raw RGBA8 row-major pixels into softbuffer's XRGB8888 words
/// (`0xFF_RR_GG_BB`, alpha forced opaque — the m10 oracle's
/// straightening rule, same as every other present path).
/// Pure (headless-testable); loud on length mismatch.
pub fn pack_rgba_to_xrgb(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u32>, String> {
    if rgba.len() != width as usize * height as usize * 4 {
        return Err(format!("rgba bytes {} != {width}x{height}x4", rgba.len()));
    }
    Ok(rgba
        .chunks_exact(4)
        .map(|p| u32::from_le_bytes([p[2], p[1], p[0], 255]))
        .collect())
}

/// Report for one present (recorded by the demo).
#[derive(Clone, Debug)]
pub struct PresentInfo {
    pub width: u32,
    pub height: u32,
    pub words: usize,
}

/// Maps a framework cursor to its winit shape (Round 8.3 — names
/// match 1:1 by construction; pure, headless-testable — the live
/// `Window` call stays in [`ShellWindow::set_cursor`], which needs an
/// event loop the unit harness cannot own, same bound as window open).
fn map_cursor(cursor: oppa::CursorIcon) -> winit::window::CursorIcon {
    use oppa::CursorIcon as C;
    use winit::window::CursorIcon as W;
    match cursor {
        C::Default => W::Default,
        C::Pointer => W::Pointer,
        C::Text => W::Text,
        C::Crosshair => W::Crosshair,
        C::Move => W::Move,
        C::NotAllowed => W::NotAllowed,
        C::ColResize => W::ColResize,
        C::RowResize => W::RowResize,
    }
}

/// An opened window with a CPU present surface. The winit event
/// loop stays caller-owned (demos/binaries drive it); this type
struct SoftbufferState {
    _context: softbuffer::Context<std::sync::Arc<winit::window::Window>>,
    surface: softbuffer::Surface<
        std::sync::Arc<winit::window::Window>,
        std::sync::Arc<winit::window::Window>,
    >,
}

/// An opened window with an optional CPU present surface (created on
/// demand on first CPU present, avoiding conflicting Wayland buffer
/// allocations when a hardware GPU swapchain is active). The winit event
/// loop stays caller-owned (demos/binaries drive it); this type
/// owns the window + lazy softbuffer context + surface.
pub struct ShellWindow {
    window: std::sync::Arc<winit::window::Window>,
    softbuffer: Option<SoftbufferState>,
}

impl ShellWindow {
    /// Opens a window on `event_loop` at `width`×`height`. The CPU
    /// softbuffer surface is deferred until `present` or explicit
    /// initialization so GPU swapchains don't conflict.
    pub fn open(
        event_loop: &winit::event_loop::ActiveEventLoop,
        title: &str,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        use winit::window::WindowAttributes;
        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title(title)
                    .with_inner_size(winit::dpi::PhysicalSize::new(width, height)),
            )
            .map_err(|e| format!("create window: {e:?}"))?;
        let window = std::sync::Arc::new(window);
        Ok(Self {
            window,
            softbuffer: None,
        })
    }

    /// Current inner size in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        let size = self.window.inner_size();
        (size.width, size.height)
    }

    /// Pointer cursor shape (Round 8.3, decision 299): applies the
    /// runner's hover resolution to the winit window immediately
    /// (winit owns cursor state — no deferred query like Win32's
    /// `WM_SETCURSOR`, so no stored state here).
    pub fn set_cursor(&self, cursor: oppa::CursorIcon) {
        self.window.set_cursor(map_cursor(cursor));
    }

    fn ensure_softbuffer(&mut self) -> Result<&mut SoftbufferState, String> {
        if self.softbuffer.is_none() {
            let context = softbuffer::Context::new(self.window.clone())
                .map_err(|e| format!("softbuffer context: {e:?}"))?;
            let mut surface = softbuffer::Surface::new(&context, self.window.clone())
                .map_err(|e| format!("softbuffer surface: {e:?}"))?;
            let (w, h) = self.size();
            surface
                .resize(
                    NonZeroU32::new(w.max(1)).ok_or("zero width")?,
                    NonZeroU32::new(h.max(1)).ok_or("zero height")?,
                )
                .map_err(|e| format!("surface resize: {e:?}"))?;
            self.softbuffer = Some(SoftbufferState {
                _context: context,
                surface,
            });
        }
        Ok(self.softbuffer.as_mut().expect("just initialized"))
    }

    /// Reconfigures the surface for a new size (if softbuffer has been
    /// initialized; otherwise a no-op since GPU swapchain owns sizing).
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        if let Some(sb) = self.softbuffer.as_mut() {
            sb.surface
                .resize(
                    NonZeroU32::new(width).ok_or("zero width")?,
                    NonZeroU32::new(height).ok_or("zero height")?,
                )
                .map_err(|e| format!("surface resize: {e:?}"))
        } else {
            Ok(())
        }
    }

    /// Presents raw RGBA8 scene pixels (must match the surface
    /// size — a mismatch is a loud error, never a silent scale).
    pub fn present(&mut self, rgba: &[u8]) -> Result<PresentInfo, String> {
        let (w, h) = self.size();
        let words = pack_rgba_to_xrgb(rgba, w, h)?;
        let sb = self.ensure_softbuffer()?;
        let mut buffer = sb
            .surface
            .buffer_mut()
            .map_err(|e| format!("buffer acquire: {e:?}"))?;
        buffer.copy_from_slice(&words);
        buffer.present().map_err(|e| format!("present: {e:?}"))?;
        Ok(PresentInfo {
            width: w,
            height: h,
            words: words.len(),
        })
    }

    /// The underlying window (demos drive the loop off it).
    pub fn window(&self) -> &winit::window::Window {
        &self.window
    }

    /// Shared ownership of the underlying window (Round 16.3 —
    /// the runner hands this to the window-chrome control on open
    /// so pre-open title/size calls still land).
    pub fn window_arc(&self) -> std::sync::Arc<winit::window::Window> {
        self.window.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_layout_is_xrgb_opaque() {
        // 2x1: red + white.
        let rgba = vec![255, 0, 0, 255, 255, 255, 255, 255];
        let words = pack_rgba_to_xrgb(&rgba, 2, 1).expect("packs");
        assert_eq!(words, vec![0xFFFF_0000, 0xFFFF_FFFF]);
    }

    #[test]
    fn pack_rejects_size_mismatch_loudly() {
        let rgba = vec![0u8; 3];
        assert!(pack_rgba_to_xrgb(&rgba, 1, 1).is_err());
    }

    // Window opening is proven by the `linux_demo` example run
    // (winit 0.30 requires the event loop on the main thread, which
    // the test harness's worker threads are not — an `#[ignore]`d
    // unit test could never run there either; the demo asserts
    // open + size + present and exits loud on failure).
    #[test]
    fn cursor_shapes_map_one_to_one() {
        use oppa::CursorIcon as C;
        use winit::window::CursorIcon as W;
        assert!(matches!(map_cursor(C::Default), W::Default));
        assert!(matches!(map_cursor(C::Pointer), W::Pointer));
        assert!(matches!(map_cursor(C::Text), W::Text));
        assert!(matches!(map_cursor(C::Crosshair), W::Crosshair));
        assert!(matches!(map_cursor(C::Move), W::Move));
        assert!(matches!(map_cursor(C::NotAllowed), W::NotAllowed));
        assert!(matches!(map_cursor(C::ColResize), W::ColResize));
        assert!(matches!(map_cursor(C::RowResize), W::RowResize));
    }
}
