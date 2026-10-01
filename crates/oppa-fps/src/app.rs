//! Shared FPS app core: one `ComponentHost` scene (white root +
//! centered text leaf), FPS EMA, and live viewport tracking. No
//! platform code here — the winit driver (`driver.rs`) feeds window
//! size and redraw ticks; entries (`main.rs`, `web.rs`) supply the
//! text service and font bytes.
//!
//! Text is identical on every platform: shaping/measuring rides the
//! bundled DejaVu face through `RustybuzzService`, and both
//! rasterizers (Vello atlas, CPU ab_glyph) inject the same bytes.

use std::cell::RefCell;
use std::rc::Rc;

use oppa::text::{FontId, TextService, TextStyle};
use oppa::{
    Color, ComponentHost, Ctx, Div, LayoutTextConfig, MountHandle, RendererBackend, SharedString,
    Style, SurfaceDesc, SurfaceId, Text, VNode,
};
use oppa_cpu::{install_paint_hook, CpuBackend};
use oppa_macros::{component, Props};
use oppa_text_rustybuzz::RustybuzzService;

use crate::clock::{now_secs, AppClock};
use oppa_fonts::{DEJAVU_SANS, DEJAVU_SANS_FAMILY};

/// Default window size (also the pre-show size on every platform).
pub const DEFAULT_W: u32 = 800;
/// Default window size (also the pre-show size on every platform).
pub const DEFAULT_H: u32 = 600;
/// Text size, device px at dpr 1.
pub const FONT_PX: f32 = 64.0;

#[derive(Clone, Props)]
struct FpsProps {
    label: SharedString,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[component]
fn FpsApp(ctx: &Ctx, props: &FpsProps) -> VNode {
    let _ = ctx;
    Div("screen")
        .style(Style::new().size(props.w, props.h).bg(Color(0xFF_FF_FF)))
        .child(
            Div("center")
                .style(
                    Style::new()
                        .x(props.x)
                        .absolute_y(props.y)
                        .ink(Color(0x00_00_00)),
                )
                .child(
                    Text {
                        text: props.label.clone(),
                        style: Text::title_small,
                    }
                    .into(),
                ),
        )
}

/// Measures `label` and centers it in a `w`×`h` box (the live
/// viewport — never creation-time constants, or a resize leaves
/// the text off-center).
fn centered_props(
    svc: &RustybuzzService,
    style: &TextStyle,
    label: &str,
    w: f32,
    h: f32,
) -> FpsProps {
    let run = svc.shape(label, style).expect("shape fps label");
    let m = svc.measure_line(&run);
    FpsProps {
        label: SharedString::from(label),
        x: (w - run.total_advance) / 2.0,
        y: (h - (m.ascent + m.descent)) / 2.0,
        w,
        h,
    }
}

/// Builds the layout-side text service: the bundled face, no
/// system font directory (none exists on Web; sets differ per OS).
/// Deterministic: same bytes in the same order, every platform.
pub fn layout_service() -> RustybuzzService {
    RustybuzzService::from_bytes_with_chain(&[("DejaVuSans.ttf", DEJAVU_SANS)], &[])
        .expect("bundled DejaVu loads")
        .0
}

/// (font id, bytes, collection index) for atlas injection: probe
/// the service the way the scene shapes, then resolve the face.
/// Both rasterizers inject the same pair.
pub fn atlas_pair(svc: &RustybuzzService) -> (FontId, Vec<u8>, u32) {
    let style = TextStyle::new(DEJAVU_SANS_FAMILY, FONT_PX);
    let probe = svc.shape("FPS: 60", &style).expect("probe shape");
    let fid = probe.runs[0].font_id;
    let (bytes, index) = svc.face_bytes(fid).expect("bundled face bytes");
    (fid, bytes.to_vec(), index)
}

/// The platform-independent FPS scene: host, text leaf, EMA, and
/// viewport. The driver calls `set_size` on resize and `step` per
/// redraw; `frame` runs one scheduler pass.
pub struct FpsCore {
    host: ComponentHost,
    handle: MountHandle<FpsProps>,
    svc: RustybuzzService,
    style: TextStyle,
    size: (u32, u32),
    ema_dt: f64,
    current_label: String,
    frozen: Option<String>,
    last_label_at: f64,
}

impl FpsCore {
    /// Mounts the scene at `size` with `layout_svc` driving layout
    /// and `app_svc` driving app-side measure/probes (two instances
    /// over the same bytes — deterministic ids, no shared borrow).
    pub fn new(
        layout_svc: RustybuzzService,
        app_svc: RustybuzzService,
        size: (u32, u32),
        frozen: Option<String>,
    ) -> Self {
        let host = ComponentHost::with_clock(Rc::new(AppClock));
        host.set_viewport(size.0 as f32, size.1 as f32);
        host.set_text_service(Box::new(layout_svc));
        host.set_layout_config(LayoutTextConfig {
            family: DEJAVU_SANS_FAMILY.to_string(),
            title_px: FONT_PX,
            ..Default::default()
        });
        let style = TextStyle::new(DEJAVU_SANS_FAMILY, FONT_PX);
        let start = frozen.clone().unwrap_or_else(|| "FPS: --".to_string());
        let handle = host.mount(
            "FpsApp",
            centered_props(&app_svc, &style, &start, size.0 as f32, size.1 as f32),
            FpsApp,
        );
        host.run_until_idle();
        Self {
            host,
            handle,
            svc: app_svc,
            style,
            size,
            ema_dt: 1.0 / 60.0,
            current_label: start,
            frozen,
            last_label_at: now_secs(),
        }
    }

    /// Tracks a resize: refits the viewport and recenters the text
    /// in the new box (position is derived from the text, so refit
    /// alone leaves it off-center).
    pub fn set_size(&mut self, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        if (w, h) == self.size {
            return;
        }
        self.size = (w, h);
        self.host.set_viewport(w as f32, h as f32);
        self.handle.set_props(centered_props(
            &self.svc,
            &self.style,
            &self.current_label,
            w as f32,
            h as f32,
        ));
    }

    /// EMA step + 4 Hz label refresh. The driver owns the single
    /// `last` timestamp and passes dt in.
    pub fn step(&mut self, dt: f64, now: f64) -> bool {
        self.ema_dt = self.ema_dt * 0.95 + dt.clamp(0.0, 0.25) * 0.05;
        if self.frozen.is_some() || now - self.last_label_at < 0.25 {
            return false;
        }
        self.last_label_at = now;
        self.refresh_label()
    }

    fn refresh_label(&mut self) -> bool {
        if self.frozen.is_some() {
            return false;
        }
        let label = format!("FPS: {:.0}", 1.0 / self.ema_dt.max(1e-6));
        if label == self.current_label {
            return false;
        }
        self.current_label = label.clone();
        let (w, h) = self.size;
        self.handle.set_props(centered_props(
            &self.svc,
            &self.style,
            &label,
            w as f32,
            h as f32,
        ));
        true
    }

    /// Runs one scheduler pass (request + idle).
    pub fn frame(&self) {
        self.host.runtime().request_frame();
        self.host.run_until_idle();
    }

    pub fn host(&self) -> &ComponentHost {
        &self.host
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    pub fn text_service(&self) -> &RustybuzzService {
        &self.svc
    }
}

/// Packs RGBA8 row-major pixels into softbuffer XRGB8888 words
/// (`0xFF_RR_GG_BB`, alpha forced opaque — the m10 straightening
/// rule). Pure (headless-testable); loud on length mismatch.
pub fn pack_rgba_to_xrgb(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u32>, String> {
    if rgba.len() != width as usize * height as usize * 4 {
        return Err(format!("rgba bytes {} != {width}x{height}x4", rgba.len()));
    }
    Ok(rgba
        .chunks_exact(4)
        .map(|p| u32::from_le_bytes([p[2], p[1], p[0], 255]))
        .collect())
}

/// CPU scene state: backend + pixmap surface, window-independent.
/// Both drivers (native winit, Web rAF) build their softbuffer
/// target around this.
pub struct CpuScene {
    pub backend: Rc<RefCell<CpuBackend>>,
    pub surface_id: SurfaceId,
}

/// Creates the CPU backend, injects the bundled face, creates the
/// scene surface, and wires the paint hook. Loud on any failure.
pub fn build_cpu_scene(core: &FpsCore, size: (u32, u32)) -> Result<CpuScene, String> {
    use std::cell::Cell;
    let backend = Rc::new(RefCell::new(CpuBackend::new()));
    let (fid, bytes, index) = atlas_pair(core.text_service());
    backend.borrow_mut().set_font_for(fid, bytes, index);
    let surface_id = backend
        .borrow_mut()
        .create_surface(SurfaceDesc {
            width_px: size.0,
            height_px: size.1,
            background: Color(0xFF_FF_FF),
        })
        .map_err(|e| format!("create cpu surface: {e}"))?;
    install_paint_hook(
        core.host(),
        backend.clone(),
        surface_id,
        1.0,
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
    );
    Ok(CpuScene {
        backend,
        surface_id,
    })
}

/// Presents one CPU frame: pixmap → XRGB words → softbuffer.
/// Loud `Err` on any size mismatch or backend failure (never a
/// silent blank/SIGSEGV-prone overrun: lengths are checked before
/// the copy). Takes the backend `Rc` (not `&CpuBackend`) so
/// callers behind `RefCell`/`RefMut` can clone it out first —
/// disjoint field borrows do not split through a deref.
pub fn present_cpu_frame<D, W>(
    backend: &Rc<RefCell<CpuBackend>>,
    surface_id: SurfaceId,
    sb: &mut softbuffer::Surface<D, W>,
    size: (u32, u32),
) -> Result<(), String>
where
    D: raw_window_handle::HasDisplayHandle,
    W: raw_window_handle::HasWindowHandle,
{
    let borrowed = backend.borrow();
    let pix = borrowed
        .pixmap(surface_id)
        .ok_or_else(|| "cpu surface pixmap missing".to_string())?;
    let (w, h) = (pix.width(), pix.height());
    if (w, h) != size {
        return Err(format!("pixmap {w}x{h} != surface {}x{}", size.0, size.1));
    }
    let words = pack_rgba_to_xrgb(pix.data(), w, h)?;
    drop(borrowed);
    let mut buffer = sb
        .buffer_mut()
        .map_err(|e| format!("buffer acquire: {e:?}"))?;
    if buffer.len() != words.len() {
        return Err(format!(
            "softbuffer {} != scene {}",
            buffer.len(),
            words.len()
        ));
    }
    buffer.copy_from_slice(&words);
    buffer.present().map_err(|e| format!("present: {e:?}"))?;
    Ok(())
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
}
