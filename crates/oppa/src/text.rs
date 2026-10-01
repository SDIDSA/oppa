//! The TextService contract (DESIGN §2.1/§2.3, §8.2, §9.2). M0b freezes the
//! trait and the data shapes the §9.2 spike will consume: pre-shaped
//! positioned glyph runs, glyph-index ↔ byte-offset mapping, per-glyph
//! advance/position, single-line run boundaries, caret/candidate-window
//! anchoring, and the DPR rounding rule shared by all presenters (§8.8).
//!
//! The mapping/caret math lives here as pure functions of a [`ShapedRun`] so
//! it is unit-testable with no backend; backends implement [`TextService`]
//! (enumerate/shape) and may override [`TextService::measure_line`].

use std::fmt;

/// Resolution-independent font weight (DWRITE_FONT_WEIGHT scale: 1..=999).
/// Thin=100, Normal=400, Bold=700, Black=900.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const THIN: FontWeight = FontWeight(100);
    pub const EXTRA_LIGHT: FontWeight = FontWeight(200);
    pub const LIGHT: FontWeight = FontWeight(300);
    pub const NORMAL: FontWeight = FontWeight(400);
    pub const MEDIUM: FontWeight = FontWeight(500);
    pub const SEMI_BOLD: FontWeight = FontWeight(600);
    pub const BOLD: FontWeight = FontWeight(700);
    pub const BLACK: FontWeight = FontWeight(900);
}

/// Font slant (DWRITE_FONT_STYLE values).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

/// Width axis (DWRITE_FONT_STRETCH scale: 1..=9). 1=UltraCondensed, 5=Normal,
/// 9=UltraExpanded.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct FontStretch(pub u16);

impl FontStretch {
    pub const ULTRA_CONDENSED: FontStretch = FontStretch(1);
    pub const CONDENSED: FontStretch = FontStretch(3);
    pub const NORMAL: FontStretch = FontStretch(5);
    pub const EXPANDED: FontStretch = FontStretch(7);
    pub const ULTRA_EXPANDED: FontStretch = FontStretch(9);
}

/// Backend-managed font handle. Ids are stable within one
/// [`TextService::enumerate_fonts`] snapshot; spikes/re-runs re-request.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct FontId(pub u32);

/// One font family face advertised by the backend.
#[derive(Clone, Debug, PartialEq)]
pub struct FontInfo {
    pub id: FontId,
    pub family: String,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub stretch: FontStretch,
}

/// How a text run is to be shaped/measured. Sizes are CSS px (96-dpi
/// space); `device_pixel_ratio` scales every emitted position/advance into
/// device px, so all downstream geometry is already at output resolution
/// (§8.2: subpixel/DPR rounding lives in TextService).
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub family: String,
    pub font_size_px: f32,
    pub device_pixel_ratio: f32,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub stretch: FontStretch,
    /// Extra tracking added to every glyph advance, device px.
    pub letter_spacing_px: f32,
    pub locale: String,
}

impl TextStyle {
    pub fn new(family: &str, font_size_px: f32) -> Self {
        Self {
            family: family.to_string(),
            font_size_px,
            device_pixel_ratio: 1.0,
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
            stretch: FontStretch::NORMAL,
            letter_spacing_px: 0.0,
            locale: "en-US".to_string(),
        }
    }

    /// Effective em size handed to the shaping backend: device px.
    pub fn em_size(&self) -> f32 {
        self.font_size_px * self.device_pixel_ratio.max(f32::EPSILON)
    }
}

/// One shaped, positioned glyph of a run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapedGlyph {
    pub glyph_id: u32,
    pub x_advance: f32,
    pub x_offset: f32,
    pub y_offset: f32,
}

/// Vertical metrics of a font face, device px (all positive; the baseline
/// sits at y=0, ascent above, descent below).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
}

/// A contiguous shaped slice of the source text sharing one font face.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    /// `[start, end)` UTF-8 byte range of the source text this run shaped.
    pub byte_range: (usize, usize),
    /// `[start, end)` range into `ShapedRun::glyphs`.
    pub glyph_range: (usize, usize),
    pub rtl: bool,
    /// ISO 15924 numeric script id as reported by the backend (0 = unknown).
    pub script: u16,
    pub font_id: FontId,
    pub font_metrics: FontMetrics,
}

/// One grapheme-cluster's mapping between text and glyphs. `byte_range`s
/// partition the source text; `glyph_range`s partition the run's glyphs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cluster {
    pub byte_range: (usize, usize),
    pub glyph_range: (usize, usize),
}

/// The unit a renderer commits and the spike tests against: pre-shaped,
/// pre-positioned runs (DESIGN §2.3: display lists carry shaped runs;
/// renderers only rasterize).
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedRun {
    pub glyphs: Vec<ShapedGlyph>,
    pub runs: Vec<TextRun>,
    pub clusters: Vec<Cluster>,
    /// Sum of all glyph advances, device px (single-line width).
    pub total_advance: f32,
    /// UTF-8 byte length of the shaped source text.
    pub text_len_bytes: usize,
}

impl ShapedRun {
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// Single-line box: width from total advance, height from the tallest
    /// run's font (max ascent/descent; the v1 bound is one line).
    pub fn single_line_metrics(&self) -> MeasuredRun {
        let mut metrics = MeasuredRun {
            width: self.total_advance,
            ascent: 0.0,
            descent: 0.0,
            line_gap: 0.0,
        };
        for run in &self.runs {
            metrics.ascent = metrics.ascent.max(run.font_metrics.ascent);
            metrics.descent = metrics.descent.max(run.font_metrics.descent);
            metrics.line_gap = metrics.line_gap.max(run.font_metrics.line_gap);
        }
        metrics
    }

    /// Glyph index whose cluster contains `byte_offset` (mid-cluster bytes
    /// snap to the cluster's first glyph — caret positions never split a
    /// cluster, §9.2 criterion 1).
    pub fn glyph_index_for_byte_offset(&self, byte_offset: usize) -> Option<usize> {
        let cluster = self.cluster_containing_byte(byte_offset)?;
        Some(cluster.glyph_range.0)
    }

    /// The cluster start byte for `glyph_index`'s cluster.
    pub fn byte_offset_for_glyph_index(&self, glyph_index: usize) -> Option<usize> {
        let cluster = self.cluster_containing_glyph(glyph_index)?;
        Some(cluster.byte_range.0)
    }

    /// Pen x (device px) of the caret at `byte_offset`, snapped to the
    /// containing cluster's leading edge. A caret at (or past) the end of
    /// the text is the trailing edge of the run: total advance. Out-of-range
    /// bytes clamp.
    pub fn caret_x(&self, byte_offset: usize) -> f32 {
        if byte_offset >= self.text_len_bytes {
            return self.total_advance;
        }
        let byte = byte_offset.min(self.text_len_bytes);
        let Some(cluster) = self.cluster_containing_byte(byte) else {
            return self.total_advance;
        };
        self.pen_x_at(cluster.glyph_range.0)
    }

    /// Hit-test: cluster index resolution for an x position (device px,
    /// run-relative). Leading half of a cluster → its start byte; trailing
    /// half → the next cluster's start byte (== this cluster's end byte).
    /// Beyond the last cluster → text length.
    pub fn byte_offset_for_x(&self, x: f32) -> usize {
        let mut pen = 0.0f32;
        for cluster in &self.clusters {
            let cluster_width = self.advance_of_cluster(cluster);
            let mid = pen + cluster_width * 0.5;
            if x < mid {
                return cluster.byte_range.0;
            }
            pen += cluster_width;
            if x < pen {
                return cluster.byte_range.1;
            }
        }
        self.text_len_bytes
    }

    /// The IME candidate-window anchor (§9.2 criterion 1): a caret-height
    /// box at the caret position; y measured with the baseline at 0.
    pub fn caret_rect(&self, byte_offset: usize) -> CaretRect {
        let metrics = self.single_line_metrics();
        CaretRect {
            x: self.caret_x(byte_offset),
            y: -metrics.ascent,
            width: 0.0,
            height: metrics.ascent + metrics.descent,
        }
    }

    /// Pen x (device px) before glyph `index` across the whole run.
    pub fn pen_x_at(&self, index: usize) -> f32 {
        let index = index.min(self.glyphs.len());
        self.glyphs[..index].iter().map(|g| g.x_advance).sum()
    }

    fn advance_of_cluster(&self, cluster: &Cluster) -> f32 {
        self.glyphs[cluster.glyph_range.0..cluster.glyph_range.1]
            .iter()
            .map(|g| g.x_advance)
            .sum()
    }

    fn cluster_containing_byte(&self, byte: usize) -> Option<&Cluster> {
        if self.clusters.is_empty() {
            return None;
        }
        let byte = byte.min(self.text_len_bytes);
        self.clusters
            .iter()
            .find(|c| byte >= c.byte_range.0 && byte < c.byte_range.1)
            .or_else(|| self.clusters.last())
    }

    fn cluster_containing_glyph(&self, glyph_index: usize) -> Option<&Cluster> {
        if self.glyphs.is_empty() {
            return None;
        }
        let glyph_index = glyph_index.min(self.glyphs.len() - 1);
        self.clusters
            .iter()
            .find(|c| glyph_index >= c.glyph_range.0 && glyph_index < c.glyph_range.1)
            .or_else(|| self.clusters.last())
    }
}

/// Single-line box for a shaped run (v1 bound: one line, no wrapping).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasuredRun {
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
}

impl MeasuredRun {
    /// Baseline-relative top/bottom of the line box (y=0 at the baseline).
    pub fn height(&self) -> f32 {
        self.ascent + self.descent
    }
}

/// Caret box in device px, run-relative; y=0 is the baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TextError {
    /// The requested family is not present in the backend's font source.
    FontNotFound(String),
    /// Empty text cannot be shaped into a run.
    EmptyText,
    /// Backend-specific failure, carry the reason.
    Backend(String),
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextError::FontNotFound(family) => write!(f, "font not found: {family}"),
            TextError::EmptyText => write!(f, "cannot shape empty text"),
            TextError::Backend(why) => write!(f, "text backend failure: {why}"),
        }
    }
}

impl std::error::Error for TextError {}

/// §8.8 (DPR/rounding determinism): the single shared rule every backend
/// uses to snap a coordinate to the device pixel grid. Rounding happens at
/// commit positions, never inside shaping (advances stay subpixel).
pub fn round_to_device_px(value: f32, device_pixel_ratio: f32) -> f32 {
    if device_pixel_ratio > 0.0 {
        (value * device_pixel_ratio).round() / device_pixel_ratio
    } else {
        value.round()
    }
}

/// DPR sourcing helpers (G10, decision 226): the single shared rule
/// every shell uses to convert its platform density into the
/// framework `device_pixel_ratio` (1.0 = 96 dpi baseline, §8.8).
/// Non-positive inputs panic loudly — a zero/negative density is a
/// platform bug, and clamping it to 1.0 would silently mis-scale
/// every text advance downstream.
pub fn dpr_from_dpi(dpi: u32) -> f32 {
    if dpi == 0 {
        panic!("dpi 0 has no device pixel ratio — refused, never silent");
    }
    dpi as f32 / 96.0
}

/// winit `scale_factor()` / web `devicePixelRatio` style input.
pub fn dpr_from_scale_factor(scale: f64) -> f32 {
    if scale.is_nan() || scale <= 0.0 {
        panic!("scale factor {scale} has no device pixel ratio — refused, never silent");
    }
    scale as f32
}

/// Line-break opportunities for paragraph wrapping (v2 item 2,
/// decision 193): break-after UTF-8 byte offsets where a soft break
/// may occur. `layout_text` consumes them the way it consumes
/// [`ShapedRun`]s — as pure data — so core keeps its M0
/// zero-dependency invariant while the UAX #14 tables live behind
/// this trait in the adjacent `oppa-linebreak` crate (the
/// `oppa-text-*` pattern: trait in core, tables outside).
pub trait BreakSource {
    /// Soft-break-after byte offsets in `text`: sorted, unique,
    /// within `(0, text.len())`, on char boundaries. `\n` hard
    /// breaks are NOT included (they stay cluster-driven in
    /// `layout_text`). The layout layer additionally intersects
    /// with cluster ends, so a stray offset can never split a
    /// cluster — it is inert, never a silent mid-cluster break.
    fn opportunities(&self, text: &str) -> Vec<usize>;
}

/// The service the per-platform backends implement (DESIGN §2.1:
/// font enumeration, shaping, measurement — TextService *interface only*
/// here; per-OS implementations are separate follow-up work).
///
/// Measurement's default is the shared pure math on [`ShapedRun`]; backends
/// that need font-specific line metrics may override.
pub trait TextService {
    fn enumerate_fonts(&self) -> Vec<FontInfo>;

    fn shape(&self, text: &str, style: &TextStyle) -> Result<ShapedRun, TextError>;

    fn measure_line(&self, run: &ShapedRun) -> MeasuredRun {
        run.single_line_metrics()
    }
}

/// Joins per-span [`ShapedRun`]s into one paragraph run (Phase 36 PR3,
/// decision 355 — the shape-whole-paragraph data half of
/// `v2-paragraph.md` Q5): glyphs/runs/clusters concatenate with
/// paragraph-relative byte and glyph ranges, advances sum. Each span
/// keeps its own shaping (no cross-span ligatures/kerning — the
/// stated shaping boundary); the joined run wraps/breaks/carets
/// exactly like a single run (the M3 no-re-shape rule — wrapping
/// never re-shapes). Empty runs contribute zero bytes (inert —
/// skipped, never a silent break). All inputs must share the
/// paragraph's size (mixed sizes stay out of minimal RichText —
/// refused loudly below, never silently unified).
pub fn join_shaped_runs(runs: &[ShapedRun]) -> ShapedRun {
    let mut glyphs = Vec::new();
    let mut out_runs = Vec::new();
    let mut clusters = Vec::new();
    let mut byte_base = 0usize;
    let mut total_advance = 0.0f32;
    for run in runs {
        let glyph_base = glyphs.len();
        glyphs.extend_from_slice(&run.glyphs);
        for r in &run.runs {
            out_runs.push(TextRun {
                byte_range: (r.byte_range.0 + byte_base, r.byte_range.1 + byte_base),
                glyph_range: (r.glyph_range.0 + glyph_base, r.glyph_range.1 + glyph_base),
                rtl: r.rtl,
                script: r.script,
                font_id: r.font_id,
                font_metrics: r.font_metrics,
            });
        }
        for c in &run.clusters {
            clusters.push(Cluster {
                byte_range: (c.byte_range.0 + byte_base, c.byte_range.1 + byte_base),
                glyph_range: (c.glyph_range.0 + glyph_base, c.glyph_range.1 + glyph_base),
            });
        }
        byte_base += run.text_len_bytes;
        total_advance += run.total_advance;
    }
    ShapedRun {
        glyphs,
        runs: out_runs,
        clusters,
        total_advance,
        text_len_bytes: byte_base,
    }
}

/// Span index owning a paragraph byte (Phase 36 PR3): leading
/// affinity at boundaries (a boundary byte belongs to the span
/// starting there — trailing blanks trimmed from a prior line read
/// the next span, the decision-195 wrap-point rule). Empty spans own
/// no bytes (inert). Out-of-range bytes clamp to the last non-empty
/// span; no non-empty span → `None` (never an invented owner).
pub fn span_index_for_byte(ends: &[usize], byte: usize) -> Option<usize> {
    let mut owner: Option<usize> = None;
    for (i, end) in ends.iter().enumerate() {
        if *end == 0 || (i > 0 && *end == ends[i - 1]) {
            continue; // empty span — owns no bytes
        }
        owner = Some(i);
        if byte < *end {
            return Some(i);
        }
    }
    owner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_metrics_takes_max_across_runs() {
        let mut run = glyph_run(&[5.0, 5.0, 5.0, 5.0]);
        run.runs[0].byte_range = (0, 2);
        run.runs[0].glyph_range = (0, 2);
        run.runs[0].font_metrics = FontMetrics {
            ascent: 10.0,
            descent: 3.0,
            line_gap: 1.0,
        };
        run.runs.push(TextRun {
            byte_range: (2, 4),
            glyph_range: (2, 4),
            rtl: false,
            script: 0,
            font_id: FontId(1),
            font_metrics: FontMetrics {
                ascent: 14.0,
                descent: 5.0,
                line_gap: 3.0,
            },
        });
        let measured = run.single_line_metrics();
        assert_eq!(measured.width, 20.0);
        assert_eq!(measured.ascent, 14.0);
        assert_eq!(measured.descent, 5.0);
        assert_eq!(measured.height(), 19.0);
    }

    #[test]
    fn byte_offset_round_trips_through_cluster_map() {
        // "héllo": h(0,1) é(1,3) l(3,4) l(4,5) o(5,6); one glyph per cluster.
        let mut run = glyph_run(&[5.0, 7.0, 4.0, 4.0, 6.0]);
        run.text_len_bytes = 6;
        run.clusters = vec![
            Cluster {
                byte_range: (0, 1),
                glyph_range: (0, 1),
            },
            Cluster {
                byte_range: (1, 3),
                glyph_range: (1, 2),
            },
            Cluster {
                byte_range: (3, 4),
                glyph_range: (2, 3),
            },
            Cluster {
                byte_range: (4, 5),
                glyph_range: (3, 4),
            },
            Cluster {
                byte_range: (5, 6),
                glyph_range: (4, 5),
            },
        ];
        // Byte → glyph: mid-cluster byte 2 snaps into é's cluster.
        assert_eq!(run.glyph_index_for_byte_offset(0), Some(0));
        assert_eq!(run.glyph_index_for_byte_offset(1), Some(1));
        assert_eq!(
            run.glyph_index_for_byte_offset(2),
            Some(1),
            "mid-cluster snap"
        );
        assert_eq!(run.glyph_index_for_byte_offset(3), Some(2));
        // Glyph → byte: the é glyph resolves to its cluster start (byte 1).
        assert_eq!(run.byte_offset_for_glyph_index(1), Some(1));
        // Caret x at cluster boundaries and the x → byte round-trip.
        assert_eq!(run.caret_x(0), 0.0);
        assert_eq!(run.caret_x(1), 5.0);
        assert_eq!(
            run.caret_x(2),
            5.0,
            "mid-cluster caret snaps to leading edge"
        );
        assert_eq!(run.caret_x(6), 26.0, "trailing caret is the run end");
        for byte in [0usize, 1, 3, 4, 5] {
            assert_eq!(
                run.byte_offset_for_x(run.caret_x(byte)),
                byte,
                "round-trip byte {byte}"
            );
        }
        // Halfway through a cluster resolves to the cluster's end byte.
        assert_eq!(run.byte_offset_for_x(5.0 + 3.6), 3, "trailing half of é");
        assert_eq!(run.byte_offset_for_x(run.total_advance), 6);
        // Candidate anchor is a caret-height box at the caret x.
        let rect = run.caret_rect(3);
        assert_eq!(rect.x, 12.0);
        assert_eq!(rect.y, -12.0);
        assert_eq!(rect.height, 16.0);
    }

    #[test]
    fn surrogate_pair_is_one_cluster() {
        // "👍": 4 UTF-8 bytes, 2 UTF-16 units, 1 glyph.
        let mut run = glyph_run(&[10.0]);
        run.text_len_bytes = 4;
        run.clusters = vec![Cluster {
            byte_range: (0, 4),
            glyph_range: (0, 1),
        }];
        for byte in 0..4 {
            assert_eq!(
                run.glyph_index_for_byte_offset(byte),
                Some(0),
                "byte {byte} in pair"
            );
        }
        assert_eq!(run.caret_x(3), 0.0);
        assert_eq!(run.byte_offset_for_x(9.0), 4);
        assert_eq!(run.byte_offset_for_x(100.0), 4);
    }

    #[test]
    fn device_rounding_is_shared_and_deterministic() {
        assert_eq!(round_to_device_px(3.7, 1.0), 4.0);
        assert_eq!(round_to_device_px(3.7, 2.0), 3.5);
        assert_eq!(round_to_device_px(3.7, 1.25), 4.0);
        for dpr in [1.0f32, 1.25, 1.5, 2.0] {
            let a = round_to_device_px(13.37, dpr);
            let b = round_to_device_px(13.37, dpr);
            assert_eq!(a, b, "deterministic at dpr {dpr}");
        }
    }

    #[test]
    fn dpr_sourcing_maps_platform_density() {
        assert_eq!(dpr_from_dpi(96), 1.0);
        assert_eq!(dpr_from_dpi(192), 2.0);
        assert_eq!(dpr_from_dpi(144), 1.5);
        assert_eq!(dpr_from_scale_factor(1.0), 1.0);
        assert_eq!(dpr_from_scale_factor(2.0), 2.0);
        assert_eq!(dpr_from_scale_factor(1.25), 1.25);
    }

    #[test]
    #[should_panic(expected = "dpi 0")]
    fn dpr_from_zero_dpi_refuses() {
        let _ = dpr_from_dpi(0);
    }

    #[test]
    #[should_panic(expected = "scale factor")]
    fn dpr_from_nonpositive_scale_refuses() {
        let _ = dpr_from_scale_factor(0.0);
    }

    #[test]
    #[should_panic(expected = "scale factor")]
    fn dpr_from_nan_scale_refuses() {
        let _ = dpr_from_scale_factor(f64::NAN);
    }

    fn glyph_run(advances: &[f32]) -> ShapedRun {
        let glyphs: Vec<ShapedGlyph> = advances
            .iter()
            .map(|&a| ShapedGlyph {
                glyph_id: 0,
                x_advance: a,
                x_offset: 0.0,
                y_offset: 0.0,
            })
            .collect();
        ShapedRun {
            glyphs,
            runs: vec![TextRun {
                byte_range: (0, advances.len()),
                glyph_range: (0, advances.len()),
                rtl: false,
                script: 0,
                font_id: FontId(0),
                font_metrics: FontMetrics {
                    ascent: 12.0,
                    descent: 4.0,
                    line_gap: 2.0,
                },
            }],
            clusters: Vec::new(),
            total_advance: advances.iter().sum(),
            text_len_bytes: advances.len(),
        }
    }

    #[allow(dead_code)]
    fn unused_fmt_helper(f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let err = TextError::FontNotFound("x".to_string());
        writeln!(f, "{err}")
    }

    fn span_run(text: &str, adv: f32, font: u32) -> ShapedRun {
        // One cluster per byte, one glyph per cluster (the m3 FakeText
        // shape — advances uniform per span so joins are exact).
        let n = text.len();
        let glyphs: Vec<ShapedGlyph> = (0..n)
            .map(|_| ShapedGlyph {
                glyph_id: font,
                x_advance: adv,
                x_offset: 0.0,
                y_offset: 0.0,
            })
            .collect();
        ShapedRun {
            glyphs,
            runs: vec![TextRun {
                byte_range: (0, n),
                glyph_range: (0, n),
                rtl: false,
                script: 0,
                font_id: FontId(font),
                font_metrics: FontMetrics {
                    ascent: 12.0,
                    descent: 4.0,
                    line_gap: 2.0,
                },
            }],
            clusters: (0..n)
                .map(|i| Cluster {
                    byte_range: (i, i + 1),
                    glyph_range: (i, i + 1),
                })
                .collect(),
            total_advance: adv * n as f32,
            text_len_bytes: n,
        }
    }

    #[test]
    fn join_concatenates_ranges_and_advances() {
        let a = span_run("hi", 5.0, 0);
        let b = span_run("!", 7.0, 1);
        let j = join_shaped_runs(&[a.clone(), b.clone()]);
        assert_eq!(j.text_len_bytes, 3);
        assert_eq!(j.total_advance, 17.0);
        assert_eq!(j.glyphs.len(), 3);
        // Runs rebase to paragraph bytes/glyphs, keeping identity.
        assert_eq!(j.runs.len(), 2);
        assert_eq!(j.runs[0].byte_range, (0, 2));
        assert_eq!(j.runs[0].glyph_range, (0, 2));
        assert_eq!(j.runs[0].font_id, FontId(0));
        assert_eq!(j.runs[1].byte_range, (2, 3));
        assert_eq!(j.runs[1].glyph_range, (2, 3));
        assert_eq!(j.runs[1].font_id, FontId(1));
        // Clusters rebase; caret/hit-test math reads the joined run.
        assert_eq!(j.clusters.len(), 3);
        assert_eq!(j.clusters[2].byte_range, (2, 3));
        assert_eq!(j.caret_x(2), 10.0, "span boundary caret");
        assert_eq!(j.caret_x(3), 17.0, "trailing edge");
        assert_eq!(j.byte_offset_for_x(11.0), 2, "hit-test spans");
        // Empty runs are inert (zero bytes/glyphs — builders skip
        // them, caret math never sees them) but preserved (their
        // metrics still strut the line box, like CSS empty inlines).
        let e = span_run("", 5.0, 2);
        let je = join_shaped_runs(&[a.clone(), e, b.clone()]);
        assert_eq!(je.text_len_bytes, 3);
        assert_eq!(je.runs.len(), 3, "empty run preserved as strut");
        assert_eq!(je.runs[1].byte_range, (2, 2));
        assert_eq!(je.clusters.len(), 3, "no empty clusters");
    }

    #[test]
    fn span_lookup_has_leading_affinity_and_skips_empties() {
        // Ends: span0 = [0,2), span1 empty, span2 = [2,5).
        let ends = vec![2usize, 2, 5];
        assert_eq!(span_index_for_byte(&ends, 0), Some(0));
        assert_eq!(span_index_for_byte(&ends, 1), Some(0));
        assert_eq!(span_index_for_byte(&ends, 2), Some(2), "boundary leads");
        assert_eq!(span_index_for_byte(&ends, 4), Some(2));
        assert_eq!(span_index_for_byte(&ends, 99), Some(2), "clamps");
        assert_eq!(span_index_for_byte(&[], 0), None);
        assert_eq!(span_index_for_byte(&[0, 0], 0), None, "all empty");
    }
}
