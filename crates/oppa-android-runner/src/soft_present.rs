//! CPU software present over JNI-free NDK (decision 433): the
//! Android leg of the presenter's terminal CPU row.
//!
//! The engine paints the CPU pixmap every frame regardless of the
//! active presenter ([`SceneCore::rgba8`] — straight RGBA8,
//! row-major, opaque-only), so a GPU-less window still has pixels:
//! this module posts them through the native-window lock
//! (`lock` → row copy → drop posts via `unlockAndPost`). No Java,
//! no JNI — the `ndk` crate owns the calls (the `nativewindow`
//! feature, additive on the existing `rwh_06`).
//!
//! Loud failures over silent frames (the runner doctrine): geometry
//! lock, format, and size mismatches are `Err`s, never a stretched,
//! truncated, or unposted frame. A failed present fails the loop
//! like a failed GPU present — the next generation re-walks the
//! ladder, so a transient refusal heals back to GPU on its own.

use android_activity::ndk::native_window::NativeWindow;

use oppa_app::SceneCore;

/// Locks the window's buffers to the scene size in RGBA8 (once per
/// window generation — the loop calls this when it adopts the live
/// size). Any refusal is a loud `Err` (a window that will not take
/// RGBA buffers cannot show the pixmap — never a silent black
/// frame).
pub fn ensure_software_window(window: &NativeWindow, w: u32, h: u32) -> Result<(), String> {
    window
        .set_buffers_geometry(
            w as i32,
            h as i32,
            Some(
                android_activity::ndk::hardware_buffer_format::HardwareBufferFormat::R8G8B8A8_UNORM,
            ),
        )
        .map_err(|e| format!("software window geometry {w}x{h}: {e}"))
}

/// Presents one CPU frame (decision 433): the engine `rgba8`
/// pixmap straight into the locked window buffer, row by row
/// through the guard's visible lines (stride padding stays
/// untouched — never a whole-buffer copy past the visible width).
/// Dropping the guard posts. Loud `Err` on pixmap, lock, format,
/// or size mismatch — a refused frame fails like a refused GPU
/// present (the runner propagates, the next generation re-walks).
pub fn present_software(scene: &SceneCore, window: &NativeWindow) -> Result<(), String> {
    let (vw, vh) = scene.viewport();
    let rgba = scene.rgba8()?;
    let expected = vw as usize * vh as usize * 4;
    if rgba.len() != expected {
        return Err(format!(
            "software present pixmap {} bytes != {vw}x{vh}x4 ({expected})",
            rgba.len()
        ));
    }
    let mut guard = window
        .lock(None)
        .map_err(|e| format!("software window lock: {e}"))?;
    if guard.format()
        != android_activity::ndk::hardware_buffer_format::HardwareBufferFormat::R8G8B8A8_UNORM
    {
        return Err(format!(
            "software window format {:?} != R8G8B8A8_UNORM",
            guard.format()
        ));
    }
    if guard.width() != vw as usize || guard.height() != vh as usize {
        return Err(format!(
            "software window {}x{} != scene {vw}x{vh}",
            guard.width(),
            guard.height()
        ));
    }
    let lines = guard
        .lines()
        .ok_or_else(|| "software window has no writable lines".to_string())?;
    for (dst, src) in lines.zip(rgba.chunks_exact(vw as usize * 4)) {
        for (pixel, byte) in dst.iter_mut().zip(src.iter()) {
            pixel.write(*byte);
        }
    }
    // Guard drop posts (`unlockAndPost`) — the present commits
    // here, never before the last row lands.
    Ok(())
}
