// ---------------------------------------------------------------------------
// Text measure cache (inline on the retained node)
// ---------------------------------------------------------------------------

use super::*;

// ---------------------------------------------------------------------------
// Measure cache (inline on the retained node; lifecycle is the node's)
// ---------------------------------------------------------------------------

/// Cache key: re-shape iff the bytes or the resolved style changed. Flags
/// alone never re-shape. Weight rides the key (decision 239) so a bold
/// re-style never hits a regular-weight cache entry.
pub(crate) fn measure_key(
    text: &str,
    size_px: f32,
    weight: crate::text::FontWeight,
    cfg: &LayoutTextConfig,
) -> TextMeasureKey {
    TextMeasureKey {
        bytes_hash: fnv1a64(text.as_bytes()),
        size_bits: size_px.to_bits(),
        dpr_bits: cfg.device_pixel_ratio.to_bits(),
        family_hash: fnv1a64(cfg.family.as_bytes()),
        weight: weight.0,
    }
}

/// Rich cache key (Phase 36 PR3, decision 355): the joined bytes plus
/// every span's weight and ink — a span re-style re-shapes exactly
/// like a byte change (never a stale-span hit). Inks ride the key
/// (they do not move layout, but the key owns cache identity — a
/// cheaper miss than a second table).
pub(crate) fn measure_key_rich(
    joined: &str,
    spans: &[crate::vnode::TextSpan],
    size_px: f32,
    cfg: &LayoutTextConfig,
) -> TextMeasureKey {
    let mut buf: Vec<u8> = Vec::with_capacity(joined.len() + spans.len() * 8);
    buf.extend_from_slice(joined.as_bytes());
    for s in spans {
        buf.extend_from_slice(&s.weight.0.to_le_bytes());
        buf.extend_from_slice(&s.ink.map(|c| c.0).unwrap_or(u32::MAX).to_le_bytes());
        buf.push(0xFF);
    }
    TextMeasureKey {
        bytes_hash: fnv1a64(&buf),
        size_bits: size_px.to_bits(),
        dpr_bits: cfg.device_pixel_ratio.to_bits(),
        family_hash: fnv1a64(cfg.family.as_bytes()),
        weight: spans.first().map(|s| s.weight.0).unwrap_or(400),
    }
}

/// Light pre-pass for run gating: true iff some text leaf's cache key
/// misses (fresh node, changed bytes, or changed resolved size/weight —
/// the TEXT-only change surfaces here, not via flags). Weight inherits
/// through transparent `Text` wrappers exactly like the size does (the
/// measured bare-text leaf always carries hint `None` — see
/// [`LayoutCtx::layout_node`]).
pub(crate) fn needs_measure(rec: &Reconciler, cfg: &LayoutTextConfig, root: NodeId) -> bool {
    let mut stack = vec![(root, cfg.default_px, crate::text::FontWeight::NORMAL)];
    while let Some((id, inherited, inherited_weight)) = stack.pop() {
        let Some(n) = rec.get(id) else {
            continue;
        };
        if n.tag == Tag::Text {
            if let Some(text) = &n.text {
                if !text.is_empty() {
                    let size = resolve_text_px(n.text_hint, inherited, cfg);
                    let weight = resolve_text_weight(n.text_hint, inherited_weight);
                    let key = match &n.rich_text {
                        Some(spans) => measure_key_rich(text, spans, size, cfg),
                        None => measure_key(text, size, weight, cfg),
                    };
                    if n.measured.as_ref().is_none_or(|m| m.key != key) {
                        return true;
                    }
                }
            }
        }
        let inner = resolve_text_px(n.text_hint, inherited, cfg);
        let inner_weight = resolve_text_weight(n.text_hint, inherited_weight);
        for child in n.children.iter().rev() {
            stack.push((*child, inner, inner_weight));
        }
    }
    false
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct TextMeasureKey {
    pub(crate) bytes_hash: u64,
    pub(crate) size_bits: u32,
    pub(crate) dpr_bits: u32,
    pub(crate) family_hash: u64,
    pub(crate) weight: u16,
}

/// One text node's cached measurement: the shaped run plus its metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct MeasuredText {
    pub(crate) key: TextMeasureKey,
    pub shaped: ShapedRun,
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
}

// ---------------------------------------------------------------------------
// Stats (the wrap round-trip instrument: `nodes_shaped` counts `shape()`
// calls the engine performed this run)
// ---------------------------------------------------------------------------

/// Mask bits the FramePlan builder consumes (M4 `FRAME_MASK`): a box that
/// changed while carrying none of these is a position-only LAYOUT move
/// (finding F1) — the engine stamps PAINT on it (decision 104) so the
/// builder rebuilds it instead of replaying stale pixels.
pub(crate) const FRAME_DIRT: PassMask = PassMask::from_bits(
    PassMask::STRUCTURE.bits()
        | PassMask::STYLE.bits()
        | PassMask::PAINT.bits()
        | PassMask::TEXT.bits(),
);

/// Per-run engine accounting. `nodes_shaped` is the measured wrap
/// round-trip count: shapes performed, not lines laid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LayoutStats {
    pub nodes_visited: usize,
    pub nodes_shaped: usize,
    pub lines_laid: usize,
    pub boxes_changed: usize,
    pub boxes_dropped: bool,
    pub generation_bumped: bool,
    /// Boxes whose change carried no paint-relevant dirt and were
    /// therefore stamped PAINT engine-side (F1 resolution, decision
    /// 104). Measured per run; the builder test asserts the count.
    pub paint_stamped: usize,
    /// Flow passes over the tree (2 when Row fill-width redistribution
    /// re-flows fill children into their shares; else 1; 0 when skipped).
    pub layout_passes: u32,
    /// True when the dirty set was empty and nothing ran.
    pub empty: bool,
}
