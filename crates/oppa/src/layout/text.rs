use super::{check_constrain_width, LaidCluster, LaidGlyph, LaidLine, LaidRun};
use crate::text::{FontId, ShapedRun};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Inline text: wrap + BiDi visual ordering + ellipsis (pure over ShapedRun)
// ---------------------------------------------------------------------------

/// Shaper-local font identity for the run covering `byte_start` (M7,
/// decision 110): clusters partition the text and runs partition the
/// text, so every cluster start belongs to exactly one run; the
/// unreachable fallback is `FontId(0)` (never a silent substitution —
/// the id rides the op for diagnostics either way).
fn run_font_of(shaped: &ShapedRun, byte_start: usize) -> FontId {
    shaped
        .runs
        .iter()
        .find(|r| byte_start >= r.byte_range.0 && byte_start < r.byte_range.1)
        .map(|r| r.font_id)
        .unwrap_or(FontId(0))
}

/// Logical cluster geometry: cumulative advances in source order.
fn logical_widths(shaped: &ShapedRun) -> Vec<f32> {
    let mut widths = Vec::with_capacity(shaped.clusters.len());
    for c in &shaped.clusters {
        let w: f32 = shaped.glyphs
            [c.glyph_range.0.min(shaped.glyphs.len())..c.glyph_range.1.min(shaped.glyphs.len())]
            .iter()
            .map(|g| g.x_advance)
            .sum();
        widths.push(w);
    }
    widths
}

/// Embedding level of a cluster for visual ordering (UBA-lite, decision
/// 76): LTR runs read level 0; RTL runs read level 1, except ASCII digits
/// (EN) inside an RTL run, which form an LTR island at level 2 (UBA I2:
/// `EN` at an odd level goes up one). Neutrals ride their run's level (the
/// N1 grouping the backend's run analysis already encodes: the space
/// between Latin and Arabic reads LTR, the one between Arabic and digits
/// reads RTL). Arabic-Indic digits stay level 1 (documented limit — AN
/// handling is future work); paragraph base direction is LTR (v1 bound).
fn cluster_level(shaped: &ShapedRun, text: &str, cluster_idx: usize) -> u8 {
    let Some(cluster) = shaped.clusters.get(cluster_idx) else {
        return 0;
    };
    let rtl = shaped
        .runs
        .iter()
        .find(|r| cluster.byte_range.0 >= r.byte_range.0 && cluster.byte_range.0 < r.byte_range.1)
        .map(|r| r.rtl)
        .unwrap_or(false);
    if !rtl {
        return 0;
    }
    let is_ascii_digit = text
        .get(cluster.byte_range.0..cluster.byte_range.1)
        .map(|s| s.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or(false);
    if is_ascii_digit {
        2
    } else {
        1
    }
}

/// One cluster placed in visual order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrderedCluster {
    /// Index into the [`ShapedRun`]'s cluster list (logical identity).
    pub index: usize,
    /// Visual x relative to the line origin (device px, subpixel).
    pub x: f32,
    pub width: f32,
    /// Segment direction (odd embedding level). Digit islands in RTL runs
    /// read false (they lay out LTR); caret edges follow this, not the
    /// run flag.
    pub rtl: bool,
}

/// UBA-lite visual ordering for one line's clusters (given as logical
/// indices): standard level-based reversal (highest level first, maximal
/// `level >= l` sequences reversed), then mirroring inside odd-level
/// segments. LTR text is untouched; RTL sequences mirror; digit islands
/// move as LTR units. Output is left-to-right with visual xs.
pub fn order_visual(shaped: &ShapedRun, text: &str, logical: &[usize]) -> Vec<OrderedCluster> {
    let widths = logical_widths(shaped);
    // Segments: consecutive clusters sharing an embedding level.
    let mut segments: Vec<(u8, Vec<usize>)> = Vec::new();
    for &ci in logical {
        let level = cluster_level(shaped, text, ci);
        match segments.last_mut() {
            Some((l, v)) if *l == level => v.push(ci),
            _ => segments.push((level, vec![ci])),
        }
    }
    let max_level = segments.iter().map(|(l, _)| *l).max().unwrap_or(0);
    for l in (1..=max_level).rev() {
        let mut i = 0;
        while i < segments.len() {
            if segments[i].0 < l {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < segments.len() && segments[j].0 >= l {
                j += 1;
            }
            segments[i..j].reverse();
            i = j;
        }
    }
    // Flatten: odd-level segments mirror their clusters.
    let mut visual: Vec<OrderedCluster> = Vec::with_capacity(logical.len());
    for (level, members) in &segments {
        let rtl = level % 2 == 1;
        if rtl {
            for &ci in members.iter().rev() {
                visual.push(OrderedCluster {
                    index: ci,
                    x: 0.0,
                    width: widths[ci],
                    rtl,
                });
            }
        } else {
            for &ci in members {
                visual.push(OrderedCluster {
                    index: ci,
                    x: 0.0,
                    width: widths[ci],
                    rtl,
                });
            }
        }
    }
    let mut x = 0.0f32;
    for o in &mut visual {
        o.x = x;
        x += o.width;
    }
    visual
}

/// Paragraph split: logical cluster indices cut at hard breaks.
/// Two cluster styles are accepted (decision 196): clusters COVERING
/// `'\n'` (backend-shaped newlines, e.g. fakes) are dropped and cut;
/// `'\n'` bytes in UNCOVERED gaps (stitched per-paragraph shaping --
/// real backends map no font for `'\n`, so the engine shapes each
/// `'\n`-paragraph whole and stitches) cut one paragraph per newline.
/// Clusters walk in byte order (sorted -- a no-op for logical-order
/// runs), so both styles compose.
fn split_paragraphs(shaped: &ShapedRun, text: &str) -> Vec<Vec<usize>> {
    let bytes = text.as_bytes();
    let mut order: Vec<usize> = (0..shaped.clusters.len()).collect();
    order.sort_by_key(|&ci| shaped.clusters[ci].byte_range.0);
    let mut paragraphs: Vec<Vec<usize>> = vec![Vec::new()];
    let mut cursor = 0usize;
    let cut_gap = |gap: &[u8], paragraphs: &mut Vec<Vec<usize>>| {
        for _ in gap.iter().filter(|&&b| b == b'\n') {
            paragraphs.push(Vec::new());
        }
    };
    for ci in order {
        let (s, e) = shaped.clusters[ci].byte_range;
        if s > cursor {
            if let Some(gap) = bytes.get(cursor..s.min(text.len())) {
                cut_gap(gap, &mut paragraphs);
            }
            cursor = cursor.max(s);
        }
        if bytes.get(s..e).is_some_and(|b| b.contains(&b'\n')) {
            paragraphs.push(Vec::new());
            cursor = cursor.max(e);
        } else {
            paragraphs.last_mut().expect("paragraph").push(ci);
            cursor = cursor.max(e);
        }
    }
    if cursor < text.len() {
        if let Some(gap) = bytes.get(cursor..) {
            cut_gap(gap, &mut paragraphs);
        }
    }
    paragraphs
}

/// Lay out shaped text into lines: `\n` hard breaks, greedy width-driven
/// wrap at cluster boundaries over cached advances (no re-shape), optional
/// single-line ellipsis truncation. Pure over the [`ShapedRun`].
///
/// This is the legacy path (no break source installed): it splits words
/// anywhere. Install a [`BreakSource`] on the engine to wrap at
/// opportunities only -- see [`layout_text_with_breaks`].
pub fn layout_text(
    shaped: &ShapedRun,
    text: &str,
    avail_w: f32,
    ellipsis_adv: Option<f32>,
    ascent: f32,
    descent: f32,
    line_gap: f32,
) -> Vec<LaidLine> {
    // Round 1.1 (decision 252): the wrap width is a constraint.
    // NaN previously read as infinite (silent single line);
    // negative wrapped every cluster alone (silent shredding).
    check_constrain_width("avail_w", avail_w);
    if shaped.clusters.is_empty() {
        return Vec::new();
    }
    let widths = logical_widths(shaped);
    let paragraphs = split_paragraphs(shaped, text);
    let finite = avail_w.is_finite();
    let mut line_of_clusters: Vec<Vec<usize>> = Vec::new();
    if let Some(e_adv) = ellipsis_adv {
        // Optional-v1 ellipsis: single line, truncate the first paragraph's
        // logical tail to `avail - ellipsis`, append the marker (decision 73).
        if finite {
            let budget = (avail_w - e_adv).max(0.0);
            let mut kept: Vec<usize> = Vec::new();
            let mut used = 0.0f32;
            for &ci in &paragraphs[0] {
                if used + widths[ci] > budget && !kept.is_empty() {
                    break;
                }
                kept.push(ci);
                used += widths[ci];
                if used >= budget {
                    break;
                }
            }
            line_of_clusters.push(kept);
        } else if !paragraphs.is_empty() {
            line_of_clusters.push(paragraphs[0].clone());
        }
    } else {
        for para in &paragraphs {
            if !finite {
                line_of_clusters.push(para.clone());
                continue;
            }
            // Greedy wrap: break before the first cluster that overflows a
            // non-empty line; an over-wide single cluster stands alone.
            let mut line: Vec<usize> = Vec::new();
            let mut used = 0.0f32;
            for &ci in para {
                if !line.is_empty() && used + widths[ci] > avail_w {
                    line_of_clusters.push(std::mem::take(&mut line));
                    used = 0.0;
                }
                line.push(ci);
                used += widths[ci];
            }
            line_of_clusters.push(line);
        }
    }
    emit_lines(
        shaped,
        text,
        &line_of_clusters,
        ellipsis_adv,
        finite,
        ascent,
        descent,
        line_gap,
    )
}

/// One visual line per logical cluster list: UBA-lite visual ordering,
/// run/cluster emission, optional ellipsis marker. Shared by the greedy
/// wrap path and the opportunity wrap path, so downstream geometry is
/// identical for identical line contents.
#[allow(clippy::too_many_arguments)]
fn emit_lines(
    shaped: &ShapedRun,
    text: &str,
    line_of_clusters: &[Vec<usize>],
    ellipsis_adv: Option<f32>,
    finite: bool,
    ascent: f32,
    descent: f32,
    line_gap: f32,
) -> Vec<LaidLine> {
    let line_h = ascent + descent;
    let mut lines: Vec<LaidLine> = Vec::new();
    let mut y = 0.0f32;
    for (li, logical) in line_of_clusters.iter().enumerate() {
        let ordered = order_visual(shaped, text, logical);
        let width: f32 = ordered.iter().map(|o| o.width).sum();
        // Runs: group the line's visual clusters by TextRun (visual order).
        let mut runs: Vec<LaidRun> = Vec::new();
        let mut clusters: Vec<LaidCluster> = Vec::new();
        // Marker clusters reference the text end; map them to no run.
        let ellipsis_here = ellipsis_adv.is_some() && li == 0;
        for o in &ordered {
            let c = &shaped.clusters[o.index];
            clusters.push(LaidCluster {
                byte_range: c.byte_range,
                x: o.x,
                width: o.width,
                rtl: o.rtl,
                ellipsis: false,
            });
            let mut pen = o.x;
            let glyphs_in: &[crate::text::ShapedGlyph] =
                &shaped.glyphs[c.glyph_range.0.min(shaped.glyphs.len())
                    ..c.glyph_range.1.min(shaped.glyphs.len())];
            let laid: Vec<LaidGlyph> = glyphs_in
                .iter()
                .map(|g| {
                    let laid = LaidGlyph {
                        glyph_id: g.glyph_id,
                        x: pen,
                        x_advance: g.x_advance,
                    };
                    pen += g.x_advance;
                    laid
                })
                .collect();
            // One LaidRun per cluster keeps run/cluster alignment exact for
            // v1 scripts (no cross-cluster run merging to misattribute).
            // Font identity comes from the source TextRun covering the
            // cluster's first byte (M7, decision 110); the resolvable
            // family name is filled post-pass by the layout caller.
            runs.push(LaidRun {
                byte_range: c.byte_range,
                rtl: o.rtl,
                glyphs: laid,
                font_id: run_font_of(shaped, c.byte_range.0),
                family: String::new(),
                // Span ink arrives post-pass (see `layout_text_leaf`)
                // — the pure order/wrap step owns no span table.
                ink: None,
            });
        }
        if ellipsis_here && finite {
            clusters.push(LaidCluster {
                byte_range: (shaped.text_len_bytes, shaped.text_len_bytes),
                x: width,
                width: ellipsis_adv.unwrap_or(0.0),
                rtl: false,
                ellipsis: true,
            });
            runs.push(LaidRun {
                byte_range: (shaped.text_len_bytes, shaped.text_len_bytes),
                rtl: false,
                glyphs: Vec::new(),
                font_id: FontId(0),
                family: String::new(),
                ink: None,
            });
        }
        let full_width = if ellipsis_here && finite {
            width + ellipsis_adv.unwrap_or(0.0)
        } else {
            width
        };
        lines.push(LaidLine {
            y,
            height: line_h,
            baseline: ascent,
            // Post-pass context (see `layout_text_leaf`): the pure
            // order/wrap step owns no measure context, so em size arrives
            // from the caller, not from here.
            em_size: 0.0,
            width: full_width,
            runs,
            clusters,
        });
        y += line_h + line_gap;
    }
    // Drop the trailing gap: box height is last line's bottom.
    let _ = line_gap;
    lines
}

/// Lay out shaped text into lines at break opportunities only (v2 item
/// 2, decisions 194-195): `\n` hard breaks plus opportunity-driven soft
/// wrapping over cached advances (no re-shape — the M3 rule stays).
/// Pure over the [`ShapedRun`] plus `breaks` (break-after byte offsets
/// from a [`BreakSource`]), consumed the way the run itself is.
///
/// Rules (decision 194), all pinned in the paragraph corpus:
///
/// - Breaks happen only at offsets that end a cluster — stray offsets
///   are inert, so a cluster is never split (locked shaping rule).
/// - Between consecutive opportunities, clusters form an atomic span:
///   a span that overflows a non-empty line starts the next line
///   whole; an over-wide span on an empty line pushes whole (it may
///   overflow) — a single over-wide cluster stands alone, the greedy
///   rule preserved.
/// - Trailing ASCII blanks (space/tab) at a soft break are trimmed —
///   they ride neither line. Paragraph-final blanks are kept (hard
///   breaks don't trim).
/// - Wrap-point forward affinity (decision 195): the break byte
///   belongs to the next line's leading caret (see
///   [`LayoutBox::caret_position`] and [`LaidLine::caret_x`]).
///
/// Optional-v1 ellipsis keeps the greedy single-line truncation
/// (opportunities govern wrapping, never truncation).
#[allow(clippy::too_many_arguments)]
pub fn layout_text_with_breaks(
    shaped: &ShapedRun,
    text: &str,
    avail_w: f32,
    ellipsis_adv: Option<f32>,
    ascent: f32,
    descent: f32,
    line_gap: f32,
    breaks: &[usize],
) -> Vec<LaidLine> {
    // Round 1.1 (decision 252): same constraint rule as the greedy
    // path — NaN/negative never wraps silently.
    check_constrain_width("avail_w", avail_w);
    if shaped.clusters.is_empty() {
        return Vec::new();
    }
    if ellipsis_adv.is_some() {
        return layout_text(
            shaped,
            text,
            avail_w,
            ellipsis_adv,
            ascent,
            descent,
            line_gap,
        );
    }
    let widths = logical_widths(shaped);
    let paragraphs = split_paragraphs(shaped, text);
    if !avail_w.is_finite() {
        let mut all: Vec<Vec<usize>> = Vec::with_capacity(paragraphs.len());
        for para in &paragraphs {
            all.push(para.clone());
        }
        return emit_lines(shaped, text, &all, None, true, ascent, descent, line_gap);
    }
    let allowed: HashSet<usize> = breaks.iter().copied().collect();
    let mut line_of_clusters: Vec<Vec<usize>> = Vec::new();
    for para in &paragraphs {
        wrap_paragraph(
            shaped,
            text,
            para,
            &widths,
            avail_w,
            &allowed,
            &mut line_of_clusters,
        );
    }
    emit_lines(
        shaped,
        text,
        &line_of_clusters,
        None,
        true,
        ascent,
        descent,
        line_gap,
    )
}

/// Greedy span packing for one paragraph: spans (clusters between
/// consecutive opportunities) are atomic; lines fill until the next
/// span overflows. Appends committed lines (soft breaks trimmed,
/// paragraph end untrimmed) to `out`.
fn wrap_paragraph(
    shaped: &ShapedRun,
    text: &str,
    para: &[usize],
    widths: &[f32],
    avail_w: f32,
    allowed: &HashSet<usize>,
    out: &mut Vec<Vec<usize>>,
) {
    if para.is_empty() {
        // Empty paragraph (adjacent `\n`s): one empty line, same as greedy.
        out.push(Vec::new());
        return;
    }
    // Span segmentation: cut after every cluster whose end is an
    // opportunity. Stray offsets cut nothing (never inside a cluster).
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    for (k, &ci) in para.iter().enumerate() {
        if allowed.contains(&shaped.clusters[ci].byte_range.1) {
            spans.push((start, k + 1));
            start = k + 1;
        }
    }
    if start < para.len() {
        spans.push((start, para.len()));
    }
    let mut line: Vec<usize> = Vec::new();
    let mut used = 0.0f32;
    for (a, b) in spans {
        let w: f32 = para[a..b].iter().map(|&ci| widths[ci]).sum();
        if !line.is_empty() && used + w > avail_w {
            out.push(trim_trailing_blanks(
                shaped,
                text,
                std::mem::take(&mut line),
            ));
            used = 0.0;
        }
        line.extend(para[a..b].iter().copied());
        used += w;
    }
    out.push(line);
}

/// Strips trailing ASCII-blank clusters (space/tab — the characters
/// UAX breaks after) from a soft-broken line. Whole clusters only,
/// never into a cluster; a non-blank tail (hyphens, CJK, NBSP) stays.
fn trim_trailing_blanks(shaped: &ShapedRun, text: &str, mut line: Vec<usize>) -> Vec<usize> {
    while let Some(&ci) = line.last() {
        let (s, e) = shaped.clusters[ci].byte_range;
        let blank = text
            .get(s..e)
            .is_some_and(|t| !t.is_empty() && t.chars().all(|c| c == ' ' || c == '\t'));
        if !blank {
            break;
        }
        line.pop();
    }
    line
}

// ---------------------------------------------------------------------------
// Portable unit tests (synthetic runs; no backend)
// ---------------------------------------------------------------------------
