// ---------------------------------------------------------------------------
// Resize, refit, pixel readout
// ---------------------------------------------------------------------------

use super::*;

impl DesktopLoop {
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
    pub(crate) fn refit_surface(&mut self) -> Result<(), String> {
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
}
