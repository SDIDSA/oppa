//! M3 layout engine: framework-owned, written once (locked #6, ADR-0004,
//! BUILD-ORDER M3). Renderers never compute layout; this engine produces one
//! [`LayoutBox`] per retained node (x, y, w, h, content size, shaped text
//! runs) and renderers rasterize what it positioned.
//!
//! v1 scope (constraints spec, locked #6): flexbox subset (Row/Column with
//! `gap`/`pad_x`/`pad_y`/`fill_width`/`fill_height`/`margin_x`/`margin_y`,
//! `align_items` cross-axis + `justify_content` main-axis, Decisions
//! 237/249; `flex_wrap` Row line-breaking, decision 253) + block-lite (Div: full-width
//! vertical stack;
//! Stack: overlay; ScrollArea: explicit viewport + `content_size` spacer) +
//! Portal viewport-anchored overlay layer (decision 255) +
//! absolute positioning (per-axis `.x` / `.absolute_y` overrides) + inline
//! text runs (width-driven wrap, `\n` hard breaks, UBA-lite BiDi visual
//! ordering over the `rtl` run metadata, optional-v1 ellipsis behind a
//! config flag). Grid and variable-height rows are v2 and have no code
//! paths here.
//!
//! Measure↔layout protocol: the engine consumes [`TextService`] (`shape` +
//! `measure_line` via [`ShapedRun`] math) and caches one [`MeasuredText`]
//! per text node inline on the retained node, keyed by (text bytes, resolved
//! size, family, dpr). Wrap re-flows over cached advances, so wrapping never
//! re-shapes. The engine consumes only its own LAYOUT flag (a text-hint
//! change sets it — hint resizes measurement); TEXT/STRUCTURE flags persist
//! for their future M4 consumers, and text changes surface as cache misses,
//! so flag dirt alone never re-shapes.
//!
//! Feedback wiring (§3-spec/layout/constraints.md): settled boxes are read
//! through [`LayoutLedger::settled`], which tracks a generation signal. The
//! engine publishes (bumps the generation) in the LAYOUT phase — after
//! EFFECTS ran — so effects reading settled metrics observe previous-frame
//! values and re-run in the next frame's EFFECTS. Effects must not write
//! what layout reads (retained styles/text, the boxes, the generation):
//! the write handles are framework-owned and not exposed; a feedback loop
//! through app signals is user logic and is documented, not enforced.
//!
//! DPR rounding (coordinate-system spec, §8.8): [`round_to_device_px`] at
//! commit positions (box x/y) only; advances, widths, and line geometry stay
//! subpixel.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::arena::NodeId;
use crate::hash::fnv1a64;
use crate::interner::Interner;
use crate::pass_mask::PassMask;
use crate::reactive::{untrack, Runtime, Signal};
use crate::reconciler::Reconciler;
use crate::style::{AlignItems, Color, FlexWrap, GridTrack, JustifyContent, Style};
use crate::text::{
    round_to_device_px, BreakSource, FontId, ShapedRun, TextError, TextService, TextStyle,
};
use crate::vnode::{Tag, TextClass};

pub mod text;
pub(crate) mod containers;
pub(crate) mod engine;
pub(crate) mod flex;
pub(crate) mod flex_vertical;
pub(crate) mod flex_wrap;
pub(crate) mod grid;
pub(crate) mod leaf;
pub(crate) mod measure;
pub(crate) mod node;
pub(crate) mod primitives;
pub(crate) mod scrollbar;
pub use text::*;
pub use engine::{LayoutEngine, LayoutLedger};
pub use measure::{LayoutStats, MeasuredText};
pub use primitives::{LaidCluster, LaidGlyph, LaidLine, LaidRun, LayoutBox, LayoutTextConfig};
pub use scrollbar::{
    scrollbar_max_offset, scrollbar_thumb, scrollbar_thumb_x, ScrollbarThumb, ScrollbarThumbX,
    SCROLLBAR_HIT_PX, SCROLLBAR_MIN_THUMB_PX, SCROLLBAR_TRACK_PX,
};
pub(crate) use engine::{LayoutCtx, Margin, Pad, Size};
pub(crate) use measure::{FRAME_DIRT, measure_key, measure_key_rich, needs_measure};
#[cfg(test)]
mod tests;

/// Loud style validation (Decision 237: loud failures over silent
/// degradation). NaN/infinite in any layout-relevant field panics; negative
/// sizes/pads/gaps/content floors panic (offsets `x`/`absolute_y` may be
/// negative but must still be finite). Called per node at layout entry.
fn check_layout_px(field: &str, v: f32, allow_negative: bool) {
    if !v.is_finite() {
        panic!("layout: style.{field} is non-finite ({v}) — NaN/Inf never lays out silently");
    }
    if !allow_negative && v < 0.0 {
        panic!("layout: style.{field} is negative ({v}) — negative sizes/pads/gaps never lay out silently");
    }
}

fn resolve_pad(style: &Style, dpr: f32) -> Pad {
    // Round 11.1: each set side wins over its symmetric shorthand
    // (CSS-shorthand rule — `pad_x`/`pad_y` stay the concise path).
    let px = style.pad_x.map(|p| p.get()).unwrap_or(0.0);
    let py = style.pad_y.map(|p| p.get()).unwrap_or(0.0);
    let top = style.pad_top.map(|p| p.get()).unwrap_or(py);
    let bottom = style.pad_bottom.map(|p| p.get()).unwrap_or(py);
    let left = style.pad_left.map(|p| p.get()).unwrap_or(px);
    let right = style.pad_right.map(|p| p.get()).unwrap_or(px);
    check_layout_px("pad_x", px, false);
    check_layout_px("pad_y", py, false);
    check_layout_px("pad_top", top, false);
    check_layout_px("pad_bottom", bottom, false);
    check_layout_px("pad_left", left, false);
    check_layout_px("pad_right", right, false);
    Pad {
        left: left * dpr,
        right: right * dpr,
        top: top * dpr,
        bottom: bottom * dpr,
    }
}

fn resolve_gap(style: &Style, dpr: f32) -> f32 {
    let g = style.gap.map(|p| p.get()).unwrap_or(0.0);
    check_layout_px("gap", g, false);
    g * dpr
}

/// Symmetric child margins (Decision 249): validated like pads
/// (non-negative finite — negative margins panic loudly, never
/// collapse silently) and scaled by DPR at the same site. Round
/// 11.1: each set side wins over its symmetric shorthand (same
/// CSS-shorthand rule as [`resolve_pad`]).
fn resolve_margin(style: &Style, dpr: f32) -> Margin {
    let mx = style.margin_x.map(|p| p.get()).unwrap_or(0.0);
    let my = style.margin_y.map(|p| p.get()).unwrap_or(0.0);
    let top = style.margin_top.map(|p| p.get()).unwrap_or(my);
    let bottom = style.margin_bottom.map(|p| p.get()).unwrap_or(my);
    let left = style.margin_left.map(|p| p.get()).unwrap_or(mx);
    let right = style.margin_right.map(|p| p.get()).unwrap_or(mx);
    check_layout_px("margin_x", mx, false);
    check_layout_px("margin_y", my, false);
    check_layout_px("margin_top", top, false);
    check_layout_px("margin_bottom", bottom, false);
    check_layout_px("margin_left", left, false);
    check_layout_px("margin_right", right, false);
    Margin {
        left: left * dpr,
        right: right * dpr,
        top: top * dpr,
        bottom: bottom * dpr,
    }
}

fn resolve_explicit(style: &Style, dpr: f32) -> (Option<f32>, Option<f32>) {
    let w = style.w.map(|p| {
        let v = p.get();
        check_layout_px("w", v, false);
        v * dpr
    });
    let h = style.h.map(|p| {
        let v = p.get();
        check_layout_px("h", v, false);
        v * dpr
    });
    if let Some(v) = w {
        if !v.is_finite() {
            panic!("layout: scaled style.w is non-finite ({v})");
        }
    }
    if let Some(v) = h {
        if !v.is_finite() {
            panic!("layout: scaled style.h is non-finite ({v})");
        }
    }
    (w, h)
}

fn resolve_offsets(style: &Style, dpr: f32) -> (Option<f32>, Option<f32>) {
    let x = style.x.map(|p| {
        let v = p.get();
        check_layout_px("x", v, true);
        v * dpr
    });
    let ay = style.absolute_y.map(|p| {
        let v = p.get();
        check_layout_px("absolute_y", v, true);
        v * dpr
    });
    (x, ay)
}

fn resolve_floor(style: &Style, dpr: f32) -> f32 {
    let c = style.content_size.map(|p| p.get()).unwrap_or(0.0);
    check_layout_px("content_size", c, false);
    c * dpr
}

/// Min/max clamp on the committed box (Phase 36 PR2a, decision 353):
/// resolved sizes clamp into `[min, max]`; an *explicit* `w`/`h`
/// outside the clamp refuses loudly (an authoring contradiction —
/// never a silent snap). A `min > max` contradiction refuses loudly
/// too. Runs in [`LayoutEngine::commit_box`](LayoutEngine)
/// so every arm (Row/Column/Grid/Stack/Scroll/portal/text)
/// honors it from one site.
fn resolve_clamp(style: &Style, dpr: f32) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
    let cvt = |name: &str, v: Option<crate::style::Px>| {
        v.map(|p| {
            let v = p.get();
            check_layout_px(name, v, false);
            v * dpr
        })
    };
    let min_w = cvt("min_w", style.min_w);
    let min_h = cvt("min_h", style.min_h);
    let max_w = cvt("max_w", style.max_w);
    let max_h = cvt("max_h", style.max_h);
    for (lo, hi, axis) in [(min_w, max_w, "width"), (min_h, max_h, "height")] {
        if let (Some(lo), Some(hi)) = (lo, hi) {
            if lo > hi {
                panic!("layout: min_{axis} {lo} exceeds max_{axis} {hi} — contradictory clamp, never a silent pick");
            }
        }
    }
    (min_w, min_h, max_w, max_h)
}

fn check_leftover(axis: &str, v: f32) {
    if !v.is_finite() {
        panic!("layout: {axis} leftover is non-finite ({v}) — NaN/Inf never lays out silently");
    }
}

/// Loud width-constraint validation (Round 1.1, decision 252): every
/// width the engine flows text into must be a usable constraint.
/// `None`/infinite = intrinsic (single-line, no constraint) and stays
/// legal; NaN is never a width (it previously read as infinite and laid
/// out a silent single line); negative widths never lay out silently.
pub(crate) fn check_constrain_width(name: &str, v: f32) {
    if v.is_nan() {
        panic!("layout: {name} is NaN — NaN widths never lay out silently");
    }
    if v < 0.0 {
        panic!("layout: {name} is negative ({v}) — negative widths never lay out silently");
    }
}

fn resolve_text_px(hint: Option<TextClass>, inherited: f32, cfg: &LayoutTextConfig) -> f32 {
    match hint {
        Some(TextClass::TitleSmall) => cfg.title_px,
        Some(TextClass::BodySecondary) => cfg.body_px,
        // Custom sizes are absolute CSS px (decision 239) — a zero size
        // is refused loudly, never laid out as a silent zero box.
        Some(TextClass::Custom { size_px, .. }) => {
            if size_px == 0 {
                panic!(
                    "layout: TextClass::Custom size_px is 0 — zero-size text never lays out silently"
                );
            }
            size_px as f32
        }
        None => inherited,
    }
}

/// Author-specified shaper weight (decision 239): the two tokens resolve
/// `NORMAL`; `Custom` carries its weight; hint-less text (including the
/// measured bare-text leaf inside a `Text` wrapper) inherits. Weights
/// outside the `1..=999` DWRITE scale are refused loudly — a backend must
/// never receive a weight it cannot interpret.
fn resolve_text_weight(
    hint: Option<TextClass>,
    inherited: crate::text::FontWeight,
) -> crate::text::FontWeight {
    match hint {
        Some(TextClass::Custom { weight, .. }) => {
            if weight.0 == 0 || weight.0 > 999 {
                panic!(
                    "layout: TextClass::Custom weight is out of range ({}) — expected 1..=999, never laid out silently",
                    weight.0
                );
            }
            weight
        }
        Some(_) => crate::text::FontWeight::NORMAL,
        None => inherited,
    }
}

/// Shapes each `'\n'`-separated paragraph whole and stitches the runs
/// into one (byte ranges rebased to the full text, glyph ranges
/// rebased to the stitched glyph vec, advances summed,
/// `text_len_bytes` covering the `'\n'` gaps). Returns the run plus
/// the piece count for `nodes_shaped`, or None when every piece is
/// empty (e.g. `"\n"` — the leaf then commits a zero box, exactly
/// like empty text). A piece-level failure panics with the same loud
/// measurement message as the single-shape path.
fn shape_paragraphs(
    service: &dyn TextService,
    text: &str,
    style: &TextStyle,
    family: &str,
    size_px: f32,
) -> Option<(ShapedRun, usize)> {
    let mut start = 0usize;
    let mut pieces: Vec<(usize, &str)> = Vec::new();
    for (i, _) in text.match_indices('\n') {
        pieces.push((start, &text[start..i]));
        start = i + 1;
    }
    pieces.push((start, &text[start..]));
    let mut glyphs: Vec<crate::text::ShapedGlyph> = Vec::new();
    let mut runs: Vec<crate::text::TextRun> = Vec::new();
    let mut clusters: Vec<crate::text::Cluster> = Vec::new();
    let mut total = 0.0f32;
    let mut count = 0usize;
    for (off, piece) in pieces {
        if piece.is_empty() {
            continue;
        }
        let shaped = match service.shape(piece, style) {
            Ok(r) => r,
            Err(TextError::EmptyText) => continue,
            Err(e) => panic!(
                "layout: text measurement failed (family {family:?}, {size_px}px): {e} — \
                 a shaping failure is a backend/config bug, never silent",
            ),
        };
        let g0 = glyphs.len();
        glyphs.extend(shaped.glyphs.iter().copied());
        for mut run in shaped.runs {
            run.byte_range.0 += off;
            run.byte_range.1 += off;
            run.glyph_range.0 += g0;
            run.glyph_range.1 += g0;
            runs.push(run);
        }
        for mut cluster in shaped.clusters {
            cluster.byte_range.0 += off;
            cluster.byte_range.1 += off;
            cluster.glyph_range.0 += g0;
            cluster.glyph_range.1 += g0;
            clusters.push(cluster);
        }
        total += shaped.total_advance;
        count += 1;
    }
    if count == 0 {
        return None;
    }
    Some((
        ShapedRun {
            glyphs,
            runs,
            clusters,
            total_advance: total,
            text_len_bytes: text.len(),
        },
        count,
    ))
}
