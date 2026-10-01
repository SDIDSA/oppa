//! CPU-vs-Vello oracle (M6: the M4 image-diff substrate extended across
//! rasterizers — same plan on both backends, diffed in pixels).
//!
//! Per-pixel exactness is NOT assumed across rasterizers (different
//! coverage ramps: tiny-skia vs Vello analytic AA), so the oracle counts
//! two numbers: exact differing pixels and tolerance-banded differing
//! pixels (every channel within ±`tol`). Geometry plans are expected at
//! ~0 on both counts; text plans are position-compared via ink columns
//! (outlines vs cells differ by design — finding F3's shape half).

/// One RGBA8 image (Vello readback shape; CPU pixmaps convert into this).
#[derive(Clone, PartialEq, Debug)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA8, `width*height*4` bytes, straight (unpremultiplied)
    /// color.
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// Converts a tiny-skia pixmap (premultiplied) to straight RGBA8.
    /// All M6 oracle scenes paint over an opaque background, so every
    /// pixel is opaque and premultiplied == straight — the conversion
    /// asserts that (a non-opaque pixel is a loud `None`, never a
    /// silently unpremultiplied compare).
    pub fn from_cpu_pixmap(px: &tiny_skia::Pixmap) -> Option<Self> {
        let mut pixels = Vec::with_capacity(px.width() as usize * px.height() as usize * 4);
        for p in px.pixels() {
            if p.alpha() != 255 {
                return None;
            }
            pixels.push(p.red());
            pixels.push(p.green());
            pixels.push(p.blue());
            pixels.push(255);
        }
        Some(Self {
            width: px.width(),
            height: px.height(),
            pixels,
        })
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<(u8, u8, u8, u8)> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = ((y * self.width + x) * 4) as usize;
        Some((
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ))
    }
}

/// Exact differing-pixel count (`usize::MAX` on size mismatch — never
/// silent equality).
pub fn diff_count_exact(a: &RgbaImage, b: &RgbaImage) -> usize {
    if a.width != b.width || a.height != b.height || a.pixels.len() != b.pixels.len() {
        return usize::MAX;
    }
    a.pixels
        .iter()
        .zip(b.pixels.iter())
        .filter(|(x, y)| x != y)
        .count()
}

/// Tolerance-banded differing-pixel count: a pixel differs only when some
/// channel is more than `tol` apart. `tol` covers the AA coverage-ramp
/// difference between rasterizers (stated per test, never assumed zero).
pub fn diff_count_tol(a: &RgbaImage, b: &RgbaImage, tol: u8) -> usize {
    if a.width != b.width || a.height != b.height || a.pixels.len() != b.pixels.len() {
        return usize::MAX;
    }
    a.pixels
        .chunks_exact(4)
        .zip(b.pixels.chunks_exact(4))
        .filter(|(x, y)| x.iter().zip(y.iter()).any(|(c, d)| c.abs_diff(*d) > tol))
        .count()
}

/// Per-column ink presence: for each x, whether any pixel in the column is
/// darker than `threshold` on all channels (text-position comparison that
/// is shape-blind by construction — outlines and cells share columns iff
/// advances agree).
pub fn ink_columns(img: &RgbaImage, threshold: u8) -> Vec<bool> {
    let mut out = vec![false; img.width as usize];
    for x in 0..img.width {
        for y in 0..img.height {
            if let Some((r, g, b, _)) = img.pixel(x, y) {
                if r < threshold && g < threshold && b < threshold {
                    out[x as usize] = true;
                    break;
                }
            }
        }
    }
    out
}

/// Symmetric difference of two ink-column sets (columns inked on exactly
/// one side).
pub fn ink_column_diff(a: &[bool], b: &[bool]) -> usize {
    a.iter().zip(b.iter()).filter(|(x, y)| x != y).count()
}

/// The GPU oracle session: one CPU backend surface + one Vello surface of
/// the same description, committed with the same diff history.
pub struct GpuOracle {
    pub cpu: oppa_cpu::CpuBackend,
    pub vello: super::VelloBackend,
    pub cpu_surface: oppa::SurfaceId,
    pub vello_surface: oppa::SurfaceId,
}

impl GpuOracle {
    pub fn new(desc: oppa::SurfaceDesc) -> Result<Self, oppa::BackendError> {
        use oppa::RendererBackend;
        let mut cpu = oppa_cpu::CpuBackend::new();
        let cpu_surface = cpu.create_surface(desc)?;
        let mut vello = super::VelloBackend::new();
        let vello_surface = vello.create_surface(desc)?;
        Ok(Self {
            cpu,
            vello,
            cpu_surface,
            vello_surface,
        })
    }

    pub fn commit_all(&mut self, diffs: &[oppa::TreeDiff]) -> Result<(), oppa::BackendError> {
        use oppa::RendererBackend;
        for d in diffs {
            self.cpu.commit(d)?;
            self.vello.commit(d)?;
        }
        Ok(())
    }
}
