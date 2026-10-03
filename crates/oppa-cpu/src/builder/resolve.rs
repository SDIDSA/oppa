//! Shared resolvers: style queries, ink inheritance, edge validation, backend-fidelity views.

use std::collections::HashMap;

use oppa::{Interner, NodeId, Reconciler, Style, ThemeMode, ThemeTokens};

/// Opacity of a style payload (v1: baked into every emitted op; the
/// `PushLayer` stack exists for GPU-future plans and is honored by the
/// backend, but this builder never emits it).
pub(super) fn opacity_of(style: &Style) -> f32 {
    style.opacity.map(|p| p.get()).unwrap_or(1.0)
}

/// Per-corner radii in device px, CSS order `[tl, tr, br, bl]`
/// (Round 11.1, decision 305): `Some` exactly when the style sets
/// per-corner radii (each resolved against the uniform shorthand,
/// then clamped to half the given box — the one shared rule every
/// backend paints, so corners agree by construction and can never
/// overflow the box). `None` is the uniform path (possibly no
/// rounding at all) — pre-11.1 plans byte-identical.
pub(super) fn resolve_radii(style: &Style, w: f32, h: f32, dpr: f32) -> Option<[f32; 4]> {
    let corners = style.corner_radii()?;
    let cap = (w.min(h) * 0.5).max(0.0);
    Some(corners.map(|c| (c.get() * dpr).min(cap).max(0.0)))
}

/// Effective text ink for a text node (M5 `Style::ink` + the theme
/// contract round): the node's own override, else the nearest
/// ancestor's, else the build theme's `text_primary` (Light's is the
/// contract [`oppa::render::INK`], so unset builds paint exactly the
/// pre-contract plan). Inheritance (not per-leaf authoring — the
/// `Text` leaf conversion carries no style slot) is the stated rule;
/// the walk is presenter-side reads only.
pub(super) fn resolve_ink(
    rec: &Reconciler,
    styles: &Interner<Style>,
    id: NodeId,
    style: &Style,
    theme: ThemeMode,
) -> oppa::Color {
    if let Some(ink) = style.ink {
        return ink;
    }
    let mut cur = rec.get(id).and_then(|n| n.parent);
    while let Some(c) = cur {
        if let Some(ink) = rec
            .get(c)
            .and_then(|n| styles.get(n.style))
            .and_then(|s| s.ink)
        {
            return ink;
        }
        cur = rec.get(c).and_then(|n| n.parent);
    }
    ThemeTokens::of(theme).text_primary
}

/// sRGB channel lerp rounded to the nearest step (`Color` is opaque
/// `0xRRGGBB` — alpha never interpolates).
pub(super) fn lerp_color(from: oppa::Color, to: oppa::Color, t: f32) -> oppa::Color {
    let channel = |shift: u32| {
        let a = ((from.0 >> shift) & 0xFF) as f32;
        let b = ((to.0 >> shift) & 0xFF) as f32;
        (a + (b - a) * t).round().clamp(0.0, 255.0) as u32
    };
    oppa::Color((channel(16) << 16) | (channel(8) << 8) | channel(0))
}

/// Validates per-edge border widths (NaN/negative refuse loudly) and
/// resolves them to device px. Returns None when nothing would paint
/// (transparent color or all-zero widths).
pub(super) fn resolve_edge_bands(
    e: oppa::BorderEdges,
    dpr: f32,
) -> Option<([f32; 4], oppa::Color)> {
    for (name, w) in [
        ("top", e.top),
        ("right", e.right),
        ("bottom", e.bottom),
        ("left", e.left),
    ] {
        let v = w.get();
        if !v.is_finite() {
            panic!("plan: border_edges.{name} is non-finite ({v}) — NaN/Inf never paints silently");
        }
        if v < 0.0 {
            panic!(
                "plan: border_edges.{name} is negative ({v}) — negative bands never paint silently"
            );
        }
    }
    let widths = [
        e.top.get() * dpr,
        e.right.get() * dpr,
        e.bottom.get() * dpr,
        e.left.get() * dpr,
    ];
    if e.color == oppa::Color::TRANSPARENT || widths.iter().all(|w| *w <= 0.0) {
        None
    } else {
        Some((widths, e.color))
    }
}

/// Resolved-style sharing check (locked #8 stays): equal payloads intern
/// to one id, so the builder resolves through the table per node.
pub fn style_rule_count(styles: &Interner<Style>) -> usize {
    styles.len()
}

/// NodeId→style-id view used by backend-fidelity tests (which node paints
/// under which shared rule).
pub fn node_styles(rec: &Reconciler) -> HashMap<NodeId, oppa::StyleId> {
    rec.retained_ids()
        .into_iter()
        .filter_map(|id| rec.get(id).map(|n| (id, n.style)))
        .collect()
}
