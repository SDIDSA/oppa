//! Shape emitters: fills, rings, bands, gradients, shadows (M4/M5 paint vocabulary).

use oppa::{DrawOp, FramePlan, LayoutBox, NodeId, PaintXf, Style, Tag};

use super::resolve::{lerp_color, resolve_radii};

/// One background fill at `(b.x + dx, b.y + dy, w, h)` in `color`,
/// honoring the style's circle/radius shape (the border ring's two
/// fills share this path; the inner fill shrinks the radius by the
/// ring width, floored at zero).
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_shape(
    plan: &mut FramePlan,
    style: &Style,
    b: &LayoutBox,
    id: NodeId,
    tag: Tag,
    color: oppa::Color,
    opacity: f32,
    dpr: f32,
    dx: f32,
    dy: f32,
    w: f32,
    h: f32,
) {
    let _ = tag;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let (x, y) = (b.x + dx, b.y + dy);
    if style.circle {
        plan.ops.push(DrawOp::Circle {
            node: id,
            cx: x + w * 0.5,
            cy: y + h * 0.5,
            r: w.min(h) * 0.5,
            color,
            opacity,
            xf: PaintXf::identity(),
        });
    } else if let Some(radii) = resolve_radii(style, w, h, dpr) {
        // Round 11.1: the inset ring keeps per-corner shapes (each
        // corner shrinks by the inset, like the uniform rule below).
        let inset = dx.min(dy);
        let radii = radii.map(|r| (r - inset).max(0.0));
        if radii.iter().any(|r| *r > 0.0) {
            plan.ops.push(DrawOp::RRect {
                node: id,
                x,
                y,
                w,
                h,
                radius: 0.0,
                radii: Some(radii),
                color,
                opacity,
                xf: PaintXf::identity(),
            });
        } else {
            plan.ops.push(DrawOp::Rect {
                node: id,
                x,
                y,
                w,
                h,
                color,
                opacity,
                xf: PaintXf::identity(),
            });
        }
    } else if let Some(r) = style.radius {
        let radius = (r.get() * dpr - dx.min(dy)).max(0.0);
        if radius > 0.0 {
            plan.ops.push(DrawOp::RRect {
                node: id,
                x,
                y,
                w,
                h,
                radius,
                radii: None,
                color,
                opacity,
                xf: PaintXf::identity(),
            });
        } else {
            plan.ops.push(DrawOp::Rect {
                node: id,
                x,
                y,
                w,
                h,
                color,
                opacity,
                xf: PaintXf::identity(),
            });
        }
    } else {
        plan.ops.push(DrawOp::Rect {
            node: id,
            x,
            y,
            w,
            h,
            color,
            opacity,
            xf: PaintXf::identity(),
        });
    }
}

/// Two-stop radial gradient as 1-device-px concentric solid-circle
/// steps (Round 27.2, decision 507): `n = round(r)` discs, largest
/// first in the rim color down to a ~1px center disc — the linear
/// strip doctrine applied radially (shared builder code over the
/// same solid-circle path the oracles prove exact, so all backends
/// agree by construction; native interpolation stays a follow-up).
/// Colors lerp in sRGB rounded to the nearest step; the rim lands
/// exactly on `r` and the center exactly on `from` (no drift).
#[allow(clippy::too_many_arguments)] // one call site; a params struct buys nothing
pub(super) fn emit_radial(
    plan: &mut FramePlan,
    id: NodeId,
    x: f32,
    y: f32,
    g: oppa::RadialGradient,
    opacity: f32,
) {
    let r = g.r.get();
    if r <= 0.0 {
        return;
    }
    let (cx, cy) = (x + g.cx.get(), y + g.cy.get());
    let n = r.round().max(1.0) as usize;
    for k in 0..n {
        // Descending radii: the k-th disc covers everything inside
        // `rad`, painted over the larger (rim-closer) ones.
        let rad = r * (n - k) as f32 / n as f32;
        if rad <= 0.0 {
            continue;
        }
        let color = lerp_color(g.from, g.to, rad / r);
        plan.ops.push(DrawOp::Circle {
            node: id,
            cx,
            cy,
            r: rad,
            color,
            opacity,
            xf: PaintXf::identity(),
        });
    }
}
/// Two-stop linear gradient as 1-device-px solid strips (Round 1.3,
/// decision 254): `n = round(span)` strips along the axis, strip colors
/// lerped in sRGB and rounded to the nearest step. Shared builder code
/// over the same solid-rect path the strict oracle proves exact, so all
/// backends agree by construction; native interpolation stays a
/// follow-up. The final edge is exact (no drift sliver).
#[allow(clippy::too_many_arguments)] // one call site; a params struct buys nothing
pub(super) fn emit_gradient(
    plan: &mut FramePlan,
    id: NodeId,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    g: oppa::LinearGradient,
    opacity: f32,
) {
    let span = if g.horizontal { w } else { h };
    if span <= 0.0 || w <= 0.0 || h <= 0.0 {
        return;
    }
    let n = span.round().max(1.0) as usize;
    let base = if g.horizontal { x } else { y };
    let mut prev = base;
    for k in 0..n {
        let edge = if k + 1 == n {
            base + span
        } else {
            base + span * (k + 1) as f32 / n as f32
        };
        let color = lerp_color(g.from, g.to, (k as f32 + 0.5) / n as f32);
        let (sx, sy, sw, sh) = if g.horizontal {
            (prev, y, edge - prev, h)
        } else {
            (x, prev, w, edge - prev)
        };
        if edge > prev {
            plan.ops.push(DrawOp::Rect {
                node: id,
                x: sx,
                y: sy,
                w: sw,
                h: sh,
                color,
                opacity,
                xf: PaintXf::identity(),
            });
        }
        prev = edge;
    }
}

/// Per-edge inset bands: top/bottom span the full width, left/right sit
/// between them (no double-painted corners — sequential src-over would
/// still agree cross-backend, but non-overlap keeps single-backend
/// pixels obvious too). Bands clamp to the box, never overflow it.
pub(super) fn emit_edge_bands(
    plan: &mut FramePlan,
    id: NodeId,
    b: &LayoutBox,
    edges: [f32; 4],
    color: oppa::Color,
    opacity: f32,
) {
    let [top, right, bottom, left] = edges;
    let top = top.min(b.h);
    let bottom = bottom.min(b.h);
    let left = left.min(b.w);
    let right = right.min(b.w);
    if top > 0.0 {
        plan.ops.push(DrawOp::Rect {
            node: id,
            x: b.x,
            y: b.y,
            w: b.w,
            h: top,
            color,
            opacity,
            xf: PaintXf::identity(),
        });
    }
    if bottom > 0.0 {
        plan.ops.push(DrawOp::Rect {
            node: id,
            x: b.x,
            y: b.y + b.h - bottom,
            w: b.w,
            h: bottom,
            color,
            opacity,
            xf: PaintXf::identity(),
        });
    }
    let mid_h = (b.h - top - bottom).max(0.0);
    if mid_h > 0.0 {
        if left > 0.0 {
            plan.ops.push(DrawOp::Rect {
                node: id,
                x: b.x,
                y: b.y + top,
                w: left,
                h: mid_h,
                color,
                opacity,
                xf: PaintXf::identity(),
            });
        }
        if right > 0.0 {
            plan.ops.push(DrawOp::Rect {
                node: id,
                x: b.x + b.w - right,
                y: b.y + top,
                w: right,
                h: mid_h,
                color,
                opacity,
                xf: PaintXf::identity(),
            });
        }
    }
}

/// Shadow emission (Phase 36 PR4, decision 356 — supersedes the
/// Round-1.3 stepped expansion for `blur > 0`): one [`DrawOp::Shadow`]
/// carrying the device-px radius — Vello paints it gaussian,
/// the CPU backend box-blurs it, CSS takes it as `box-shadow` blur.
/// `blur == 0` keeps the shipped offset solid. Blur validity is
/// checked by the caller (and re-checked here — belt-and-braces).
pub(super) fn emit_shadow(
    plan: &mut FramePlan,
    id: NodeId,
    b: &LayoutBox,
    shadow: &oppa::Shadow,
    dpr: f32,
) {
    let blur_dev = shadow.blur.get() * dpr;
    if !blur_dev.is_finite() || blur_dev < 0.0 {
        panic!(
            "plan: shadow blur is non-finite or negative ({blur_dev}) — NaN/negative blur never paints silently"
        );
    }
    // Phase 36 PR4 (decision 356): native blur everywhere — one
    // `Shadow` op carrying the radius (Vello gaussian, CPU box-blur,
    // CSS box-shadow). `blur == 0` keeps the offset solid (the
    // pre-PR4 shape, pixel-exact on every backend).
    plan.ops.push(DrawOp::Shadow {
        node: id,
        x: b.x,
        y: b.y,
        w: b.w,
        h: b.h,
        dx: shadow.x.get() * dpr,
        dy: shadow.y.get() * dpr,
        blur_radius: blur_dev,
        color: shadow.color,
        xf: PaintXf::identity(),
    });
}
