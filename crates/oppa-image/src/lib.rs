//! Image decode seam (G8 — decisions 222–223; SVG — decision 291).
//!
//! `RImg` refused loudly since M4 ("no decoded pixels in v1 — async
//! decode unscoped"). This crate scopes the sync half: format-sniffed
//! bytes in, straight-alpha `RGBA8` out. PNG ships; SVG ships via
//! `resvg` (no font stack — text-in-SVG refuses loudly, never tofu);
//! every other format refuses loudly by magic bytes (JPEG names its
//! follow-up, it is not silently covered).
//!
//! Design (see decisions 222–223; rationale in git history):
//!
//! - **Straight alpha out.** Decoders hand back unpremultiplied
//!   `RGBA8`; each backend premultiplies at insert with its own
//!   convention (the CPU backend matches tiny-skia's bit-for-bit —
//!   see `oppa-cpu`). The seam never guesses a backend's pixel
//!   layout. (SVG renders through a premultiplied pixmap internally
//!   and converts back on the way out — the round-trip rounding is
//!   stated below, not hidden.)
//! - **Dimensions before allocation.** Width/height come from the
//!   header (`read_info`) and are capped at [`MAX_DIMENSION`] before
//!   any pixel buffer is allocated — a corrupt header can never
//!   drive an OOM. SVG sizes (natural or targeted) check the same
//!   cap before the pixmap allocates.
//! - **Loud failures.** [`ImageError`] names empty input, unknown
//!   formats (with the sniffed reality — JPEG tells you it is
//!   OQ-G8-3), corrupt streams, oversize headers, and SVG shapes
//!   outside the no-font build (`<text>`, external images).
//! - **Async is a documented pattern, not a pump (OQ-G8-2).**
//!   `decode_*` are blocking `Send`-friendly pure functions: run
//!   them in `ctx.spawn` (native) and deposit via
//!   `CpuBackend::insert_image` on the UI thread. No framework
//!   decode pump exists yet.
//!
//! Out of scope: JPEG/GIF/WebP/AVIF (OQ-G8-3), EXIF orientation
//! (decoded pixels are stored as-stored — orientation is OQ-G8-4),
//! animated frames (first frame only... precisely: APNG refuses via
//! the animation chunk — loud, not first-frame-silent; see the
//! `acTL` check).

use std::fmt;

/// Decode failure (loud by construction).
#[derive(Clone, Debug, PartialEq)]
pub enum ImageError {
    Empty,
    /// First bytes matched no supported format. The string names what
    /// was sniffed (`"jpeg"`, `"gif"`, `"unknown"`, ...).
    UnsupportedFormat(String),
    TooLarge {
        width: u32,
        height: u32,
    },
    DecodeFailed(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::Empty => write!(f, "no image bytes"),
            ImageError::UnsupportedFormat(sniffed) => {
                write!(f, "unsupported image format: {sniffed}")
            }
            ImageError::TooLarge { width, height } => {
                write!(f, "image {width}x{height} exceeds {MAX_DIMENSION}px")
            }
            ImageError::DecodeFailed(msg) => write!(f, "image decode failed: {msg}"),
        }
    }
}

impl std::error::Error for ImageError {}

/// Decoded pixels: straight-alpha `RGBA8`, row-major, top-left first.
/// `Send`-friendly (owned) — decode off-thread, deposit on the UI
/// thread.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Maximum width/height in px (decision 222): a reasoned OOM bound
/// (8192²×4 = 256 MB worst case per image — large-wallpaper class),
/// not a derived law. Checked from the header before allocating.
pub const MAX_DIMENSION: u32 = 8192;

/// PNG magic (8 bytes, RFC 2083 §12.1).
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
/// JPEG SOI marker (OQ-G8-3 — named, not decoded).
const JPEG_MAGIC: &[u8] = b"\xFF\xD8\xFF";

/// Sniffs `bytes` by magic and decodes. PNG and SVG decode;
/// everything else refuses loudly (JPEG/GIF/etc. name themselves in
/// the error).
pub fn decode_image(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    if bytes.is_empty() {
        return Err(ImageError::Empty);
    }
    if bytes.starts_with(PNG_MAGIC) {
        return decode_png(bytes);
    }
    if is_svg(bytes) {
        return decode_svg(bytes, None, None);
    }
    if bytes.starts_with(JPEG_MAGIC) {
        return Err(ImageError::UnsupportedFormat(
            "jpeg (OQ-G8-3 — needs a jpeg decoder dep)".to_string(),
        ));
    }
    if bytes.starts_with(b"GIF8") {
        return Err(ImageError::UnsupportedFormat("gif".to_string()));
    }
    if bytes.starts_with(b"RIFF") && bytes.len() > 11 && &bytes[8..12] == b"WEBP" {
        return Err(ImageError::UnsupportedFormat("webp".to_string()));
    }
    Err(ImageError::UnsupportedFormat("unknown".to_string()))
}

/// True when `bytes` look like SVG (decision 291): leading UTF-8
/// BOM + whitespace stripped, then a case-insensitive `<svg`, or an
/// `<?xml` prolog with `<svg` in the first 2 KB (prolog + doctype +
/// comments). Non-UTF-8 input is never SVG (SVG is UTF-8 by
/// definition) — it falls through to the `unknown` refusal, never a
/// silent guess.
pub fn is_svg(bytes: &[u8]) -> bool {
    let mut head = bytes;
    if head.starts_with(&[0xEF, 0xBB, 0xBF]) {
        head = &head[3..];
    }
    let mut i = 0;
    while i < head.len() && matches!(head[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    head = &head[i..];
    if head.len() >= 4 && head[..4].eq_ignore_ascii_case(b"<svg") {
        return true;
    }
    if head.len() >= 5 && head[..5].eq_ignore_ascii_case(b"<?xml") {
        let window = &bytes[..bytes.len().min(2048)];
        return window.windows(4).any(|w| w.eq_ignore_ascii_case(b"<svg"));
    }
    false
}

/// Decodes SVG bytes to straight-alpha `RGBA8` (decision 291) via
/// `resvg` (no font stack — see below).
///
/// `target_width`/`target_height` override the raster size: both
/// `Some` renders exactly there; one `Some` scales the other by the
/// SVG aspect (rounded, minimum 1); both `None` keeps the SVG's
/// natural size (rounded up from its viewBox, minimum 1). Every size
/// checks [`MAX_DIMENSION`] before the pixmap allocates (the G8
/// dims-before-alloc rule); zero/oversize targets fail loudly.
///
/// Loud refusals (never silent pixels):
///
/// - non-UTF-8 or unparseable SVG → `DecodeFailed` naming the cause;
/// - `<text…` content → `UnsupportedFormat` (the no-font build would
///   otherwise skip text silently — resvg renders no glyphs without
///   its font stack, so text-in-SVG is a named follow-up, not a gap);
/// - zero natural size or zero resolved target → `DecodeFailed`.
pub fn decode_svg(
    bytes: &[u8],
    target_width: Option<u32>,
    target_height: Option<u32>,
) -> Result<DecodedImage, ImageError> {
    if bytes.is_empty() {
        return Err(ImageError::Empty);
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|e| ImageError::DecodeFailed(format!("svg is not UTF-8: {e}")))?;
    if contains_svg_text(text) {
        return Err(ImageError::UnsupportedFormat(
            "svg <text> (needs resvg's font stack — text-in-SVG is a follow-up, never silent tofu)"
                .to_string(),
        ));
    }
    let tree = resvg::usvg::Tree::from_str(text, &resvg::usvg::Options::default())
        .map_err(|e| ImageError::DecodeFailed(format!("svg parse: {e:?}")))?;
    let natural = tree.size();
    if natural.width() <= 0.0 || natural.height() <= 0.0 {
        return Err(ImageError::DecodeFailed(format!(
            "svg has zero natural size {}x{} — refused",
            natural.width(),
            natural.height()
        )));
    }
    let (width, height) = resolve_svg_size(
        natural.width(),
        natural.height(),
        target_width,
        target_height,
    )?;
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ImageError::TooLarge { width, height });
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| ImageError::DecodeFailed("svg pixmap alloc failed".to_string()))?;
    let sx = width as f32 / natural.width();
    let sy = height as f32 / natural.height();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(sx, sy),
        &mut pixmap.as_mut(),
    );
    Ok(DecodedImage {
        width,
        height,
        rgba: unpremultiply(pixmap.data()),
    })
}

/// Resolves the raster size: exact target when both are given, aspect
/// scale (rounded, minimum 1) when one is given, natural size
/// (rounded up, minimum 1) otherwise. Zero explicit targets fail
/// loudly (a 0-px raster is an authoring bug, never an empty image).
fn resolve_svg_size(
    natural_w: f32,
    natural_h: f32,
    target_width: Option<u32>,
    target_height: Option<u32>,
) -> Result<(u32, u32), ImageError> {
    match (target_width, target_height) {
        (Some(0), _) | (_, Some(0)) => Err(ImageError::DecodeFailed(
            "svg target size is zero — refused, never an empty image".to_string(),
        )),
        (Some(w), Some(h)) => Ok((w, h)),
        (Some(w), None) => {
            let h = ((w as f32 * natural_h / natural_w).round() as u32).max(1);
            Ok((w, h))
        }
        (None, Some(h)) => {
            let w = ((h as f32 * natural_w / natural_h).round() as u32).max(1);
            Ok((w, h))
        }
        (None, None) => Ok((natural_w.ceil() as u32, natural_h.ceil() as u32)),
    }
}

/// True when the SVG source carries a `<text` element (case-insensitive,
/// attribute-tolerant — matches `<text`, `<text `, `<text>`, `<text/`).
/// A byte scan, not XML parsing: it over-matches inside comments
/// (refusing a commented-out text element is the loud-safe direction —
/// a false refusal names itself, a missed one would render tofu).
fn contains_svg_text(source: &str) -> bool {
    let bytes = source.as_bytes();
    for (i, w) in bytes.windows(5).enumerate() {
        if w.eq_ignore_ascii_case(b"<text") {
            let next = bytes.get(i + 5);
            if matches!(
                next,
                None | Some(b' ')
                    | Some(b'\t')
                    | Some(b'\n')
                    | Some(b'\r')
                    | Some(b'>')
                    | Some(b'/')
            ) {
                return true;
            }
        }
    }
    false
}

/// Premultiplied pixmap bytes → straight-alpha `RGBA8` (the seam's
/// output shape). Fully transparent pixels decode to `(0, 0, 0, 0)`
/// (straight alpha has no color under zero alpha — the information
/// is already gone, stated); otherwise each channel un-premultiplies
/// with round-half-up (`(p·255 + a/2) / a`). Opaque pixels round-trip
/// exactly; translucent ones may drift ±1 (premultiply quantization —
/// stated, asserted in the tests).
fn unpremultiply(premul: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(premul.len());
    for px in premul.chunks_exact(4) {
        let a = px[3];
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let un = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
            out.extend_from_slice(&[un(px[0]), un(px[1]), un(px[2]), a]);
        }
    }
    out
}
/// Decodes PNG bytes to straight-alpha `RGBA8` (8-bit RGB/RGBA via
/// `normalize_to_color8`; animated `acTL` refuses loudly — never a
/// silent first frame).
pub fn decode_png(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    if bytes.is_empty() {
        return Err(ImageError::Empty);
    }
    if !bytes.starts_with(PNG_MAGIC) {
        return Err(ImageError::UnsupportedFormat("unknown".to_string()));
    }
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| ImageError::DecodeFailed(format!("png header: {e}")))?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ImageError::TooLarge { width, height });
    }
    if reader.info().frame_control.is_some() {
        return Err(ImageError::UnsupportedFormat(
            "animated png (OQ-G8-3 family)".to_string(),
        ));
    }
    let color = reader.info().color_type;
    let mut buf =
        vec![
            0u8;
            reader
                .output_buffer_size()
                .ok_or_else(|| ImageError::DecodeFailed("png buffer size overflow".to_string()))?
        ];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| ImageError::DecodeFailed(format!("png pixels: {e}")))?;
    debug_assert_eq!((info.width, info.height), (width, height));
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity(buf.len() / 3 * 4);
            for px in buf.chunks_exact(3) {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            rgba
        }
        other => {
            let _ = color;
            return Err(ImageError::DecodeFailed(format!(
                "unexpected png output color {other:?} after normalize_to_color8"
            )));
        }
    };
    Ok(DecodedImage {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes straight-alpha RGBA8 to PNG bytes (test fixture foundry
    /// — no binaries in the tree).
    fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .expect("png header writes")
            .write_image_data(rgba)
            .expect("png pixels write");
        out
    }

    #[test]
    fn png_round_trips_exact_pixels() {
        // 2x2 distinct opaque: red green / blue white.
        let rgba: Vec<u8> = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let bytes = encode_png(2, 2, &rgba);
        let img = decode_png(&bytes).expect("decodes");
        assert_eq!((img.width, img.height), (2, 2));
        assert_eq!(img.rgba, rgba, "lossless round-trips byte-exact");
        // The sniffing entry agrees.
        assert_eq!(decode_image(&bytes).expect("sniffs png"), img);
    }

    #[test]
    fn png_rgb_gains_opaque_alpha() {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .expect("header")
            .write_image_data(&[10, 20, 30])
            .expect("pixels");
        let img = decode_png(&out).expect("decodes");
        assert_eq!(img.rgba, vec![10, 20, 30, 255]);
    }

    #[test]
    fn refusals_are_loud_and_named() {
        assert_eq!(decode_image(&[]), Err(ImageError::Empty));
        assert_eq!(decode_png(&[]), Err(ImageError::Empty));
        assert_eq!(decode_svg(&[], None, None), Err(ImageError::Empty));
        assert_eq!(
            decode_image(&[0, 1, 2, 3]),
            Err(ImageError::UnsupportedFormat("unknown".to_string()))
        );
        assert_eq!(
            decode_image(b"\xFF\xD8\xFF rest"),
            Err(ImageError::UnsupportedFormat(
                "jpeg (OQ-G8-3 — needs a jpeg decoder dep)".to_string()
            ))
        );
        assert_eq!(
            decode_image(b"GIF89a..."),
            Err(ImageError::UnsupportedFormat("gif".to_string()))
        );
        // Truncated PNG: magic ok, stream corrupt.
        assert!(matches!(
            decode_png(b"\x89PNG\r\n\x1a\njunk"),
            Err(ImageError::DecodeFailed(_))
        ));
        // Non-PNG into decode_png.
        assert_eq!(
            decode_png(b"hello world, not png"),
            Err(ImageError::UnsupportedFormat("unknown".to_string()))
        );
    }

    /// Minimal SVG fixture foundry (no binaries in the tree): a
    /// full-bleed rect, so every pixel is covered with no AA fringe
    /// to argue about.
    fn svg_rect(w: u32, h: u32, fill: &str) -> Vec<u8> {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\">\
             <rect width=\"{w}\" height=\"{h}\" fill=\"{fill}\"/></svg>"
        )
        .into_bytes()
    }

    /// Decision 291: SVG bytes sniff, rasterize to straight-alpha
    /// RGBA8, and feed the `Img`/`insert_image` shape (opaque pixels
    /// round-trip exactly).
    #[test]
    fn svg_rect_decodes_to_exact_pixels() {
        let bytes = svg_rect(4, 4, "red");
        assert!(is_svg(&bytes), "sniffer sees the svg");
        let img = decode_svg(&bytes, None, None).expect("decodes");
        assert_eq!((img.width, img.height), (4, 4));
        assert_eq!(img.rgba.len(), 4 * 4 * 4);
        for px in img.rgba.chunks_exact(4) {
            assert_eq!(px, &[255, 0, 0, 255], "opaque red round-trips exactly");
        }
        // The sniffing entry agrees (natural size, no targets).
        assert_eq!(decode_image(&bytes).expect("sniffs svg"), img);
    }

    #[test]
    fn svg_sniffer_covers_prologs_and_case() {
        assert!(is_svg(b"<svg xmlns='http://www.w3.org/2000/svg'/>"));
        assert!(is_svg(b"  \n\t<svg width='1' height='1'/>"));
        assert!(is_svg(
            b"<?xml version=\"1.0\"?><svg width='1' height='1'/>"
        ));
        assert!(is_svg(b"<?xml version=\"1.0\"?>\n<!-- c -->\n<svg/>"));
        assert!(
            is_svg(b"<SVG width='1' height='1'/>"),
            "case-insensitive tag"
        );
        assert!(
            is_svg(&[0xEF, 0xBB, 0xBF, b'<', b's', b'v', b'g']),
            "BOM stripped"
        );
        assert!(!is_svg(b"hello world, not svg"));
        assert!(!is_svg(b"\x89PNG\r\n\x1a\njunk"), "png is not svg");
        assert!(!is_svg(&[0xFF, 0xD8, 0xFF]), "non-UTF-8 is never svg");
    }

    #[test]
    fn svg_targets_scale_and_cap_loudly() {
        let bytes = svg_rect(16, 8, "blue");
        // Exact target.
        let img = decode_svg(&bytes, Some(8), Some(4)).expect("exact target");
        assert_eq!((img.width, img.height), (8, 4));
        // One-sided target keeps the 2:1 aspect (rounded, min 1).
        let img = decode_svg(&bytes, Some(8), None).expect("width target");
        assert_eq!((img.width, img.height), (8, 4));
        let img = decode_svg(&bytes, None, Some(4)).expect("height target");
        assert_eq!((img.width, img.height), (8, 4));
        // Zero targets refuse (never an empty image).
        assert!(matches!(
            decode_svg(&bytes, Some(0), None),
            Err(ImageError::DecodeFailed(_))
        ));
        // Oversize targets hit the dims-before-alloc cap.
        assert_eq!(
            decode_svg(&bytes, Some(MAX_DIMENSION + 1), Some(4)),
            Err(ImageError::TooLarge {
                width: MAX_DIMENSION + 1,
                height: 4
            })
        );
    }

    #[test]
    fn svg_text_and_garbage_refuse_loudly() {
        // No font stack in this build: text would render as nothing,
        // so it refuses by name instead (never silent tofu).
        let text_svg = b"<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'>\
            <text x='1' y='6'>hi</text></svg>";
        assert!(is_svg(text_svg));
        assert!(matches!(
            decode_svg(text_svg, None, None),
            Err(ImageError::UnsupportedFormat(msg)) if msg.contains("<text>")
        ));
        // Malformed XML fails with the cause, never a partial image.
        assert!(matches!(
            decode_svg(b"<svg><rect", None, None),
            Err(ImageError::DecodeFailed(_))
        ));
        // Non-UTF-8 fails (SVG is UTF-8 by definition).
        assert!(matches!(
            decode_svg(&[0x3C, 0x73, 0xFF, 0x3E], None, None),
            Err(ImageError::DecodeFailed(_))
        ));
    }

    #[test]
    fn svg_half_alpha_rounds_within_one() {
        let bytes = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"2\" height=\"2\">\
             <rect width=\"2\" height=\"2\" fill=\"red\" fill-opacity=\"0.5\"/></svg>"
            .as_bytes();
        let img = decode_svg(bytes, None, None).expect("decodes");
        for px in img.rgba.chunks_exact(4) {
            assert_eq!(px[3], 128, "alpha survives premultiply exactly");
            assert!(
                px[0] >= 254,
                "red un-premultiplies within ±1 (stated quantization), got {px:?}"
            );
            assert_eq!((px[1], px[2]), (0, 0));
        }
    }
}
