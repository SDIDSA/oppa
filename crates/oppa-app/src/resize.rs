// ---------------------------------------------------------------------------
// Resize, refit, pixel readout (delegating to SceneCore)
// ---------------------------------------------------------------------------

use super::*;

impl DesktopLoop {
    /// Tracks a resize: refits the viewport and rebuilds the paint
    /// surface (positions derive from the viewport, so refit alone
    /// leaves the scene stale). Device px in; the engine divides by
    /// the DPR for the host viewport. Same-size calls are a no-op.
    /// Loud on failure.
    pub fn resize(&mut self, width_px: u32, height_px: u32) -> Result<(), String> {
        self.scene.resize(width_px, height_px)
    }

    /// Straight RGBA8 row-major pixels of the last paint (opaque
    /// scenes only — a non-opaque pixel is a loud `Err`, the m10
    /// straightening rule). Present glue converts onward per
    /// platform (BGRA for GDI, XRGB words for softbuffer).
    pub fn rgba8(&self) -> Result<Vec<u8>, String> {
        self.scene.rgba8()
    }
}
