//! CPU-vs-GPU pixel oracle and image comparison.

/// One RGBA8 image (readback shape; CPU pixmaps convert into this).
#[derive(Clone, PartialEq, Debug)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA8, `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// Converts a tiny-skia pixmap (premultiplied) to straight RGBA8.
    /// All oracle scenes paint over an opaque background, so every
    /// pixel is opaque and premultiplied == straight.
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

/// Exact differing-pixel count.
pub fn diff_count_exact(a: &RgbaImage, b: &RgbaImage) -> usize {
    if a.width != b.width || a.height != b.height {
        return usize::MAX;
    }
    a.pixels
        .chunks_exact(4)
        .zip(b.pixels.chunks_exact(4))
        .filter(|(pa, pb)| pa != pb)
        .count()
}

/// Differing-pixel count under a per-channel tolerance band `tol`.
pub fn diff_count_tol(a: &RgbaImage, b: &RgbaImage, tol: u8) -> usize {
    if a.width != b.width || a.height != b.height {
        return usize::MAX;
    }
    a.pixels
        .chunks_exact(4)
        .zip(b.pixels.chunks_exact(4))
        .filter(|(pa, pb)| {
            pa[0].abs_diff(pb[0]) > tol
                || pa[1].abs_diff(pb[1]) > tol
                || pa[2].abs_diff(pb[2]) > tol
                || pa[3].abs_diff(pb[3]) > tol
        })
        .count()
}

/// Maximum per-channel diff across all pixels.
pub fn max_channel_diff(a: &RgbaImage, b: &RgbaImage) -> u8 {
    if a.width != b.width || a.height != b.height {
        return 255;
    }
    a.pixels
        .iter()
        .zip(b.pixels.iter())
        .map(|(pa, pb)| pa.abs_diff(*pb))
        .max()
        .unwrap_or(0)
}
