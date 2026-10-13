//! Payload-leaf emitters: vector paths and retained canvas specs (paint rides the payload).

use oppa::{DrawOp, FramePlan, LayoutBox, NodeId, PaintXf, Reconciler, Style};

/// Vector emission (decision 291): one [`DrawOp::Path`] per path
/// node — the committed box origin/size plus the retained
/// [`PathSpec`](oppa::PathSpec). Style paint fields (`bg`, rings,
/// gradients, shadows, shape flags, ink) never apply to paths: path
/// paint rides `data`/`fill`/`stroke` only, so any of them set is an
/// authoring bug and refuses loudly here (no backend can diverge on
/// it). Payload gaps (no spec, blank data, neither fill nor stroke,
/// bad stroke width) refuse loudly too — [`Path::build`](oppa::Path)
/// already enforces these for component-built nodes; this arm covers
/// hand-built `Tag::Path` elements with the same loudness.
pub(super) fn emit_path_op(
    plan: &mut FramePlan,
    rec: &Reconciler,
    id: NodeId,
    b: &LayoutBox,
    style: &Style,
    opacity: f32,
) {
    if style.bg.is_some() {
        panic!(
            "plan: bg on a Path node {id:?} — path paint rides data/fill/stroke, never style bg"
        );
    }
    if style.border.is_some() || style.border_edges.is_some() {
        panic!("plan: border on a Path node {id:?} — path paint rides data/fill/stroke, never style rings");
    }
    if style.bg_gradient.is_some() {
        panic!("plan: bg_gradient on a Path node {id:?} — path paint rides data/fill/stroke, never style gradients");
    }
    if style.bg_radial.is_some() {
        panic!("plan: bg_radial on a Path node {id:?} — path paint rides data/fill/stroke, never style gradients");
    }
    if style.shadow.is_some() {
        panic!("plan: shadow on a Path node {id:?} — path paint rides data/fill/stroke, never style shadows");
    }
    if style.circle || style.has_any_radius() {
        panic!("plan: radius/circle on a Path node {id:?} — paths carry their own geometry, never style shape flags");
    }
    if style.ink.is_some() {
        panic!(
            "plan: ink on a Path node {id:?} — path paint rides data/fill/stroke, never style ink"
        );
    }
    let Some(spec) = rec.get(id).and_then(|n| n.path.clone()) else {
        panic!("plan: Tag::Path node {id:?} without a path payload — Path nodes carry PathSpec (hand-built Path elements must set it)");
    };
    if spec.data.trim().is_empty() {
        panic!("plan: Tag::Path node {id:?} has blank path data — refused, never a silent no-op");
    }
    if spec.fill.is_none() && spec.stroke.is_none() {
        panic!("plan: Tag::Path node {id:?} has neither fill nor stroke — refused, never a silent no-op");
    }
    if let Some(s) = spec.stroke {
        if !s.width.is_finite() {
            panic!(
                "plan: Tag::Path node {id:?} stroke width is non-finite ({}) — NaN/Inf never paints silently",
                s.width
            );
        }
        if s.width < 0.0 {
            panic!(
                "plan: Tag::Path node {id:?} stroke width is negative ({}) — negative widths never paint silently",
                s.width
            );
        }
    }
    plan.ops.push(DrawOp::Path {
        node: id,
        x: b.x,
        y: b.y,
        width: b.w,
        height: b.h,
        data: spec.data,
        fill: spec.fill,
        stroke: spec.stroke,
        opacity,
        xf: PaintXf::identity(),
    });
}

/// Canvas shape emission (Phase 36 PR4, decision 358): style paint
/// fields never apply to a canvas (paint rides the spec — the path
/// rule, same loud refusals); the spec lowers to shape ops at the
/// committed box origin. Text ops paint nothing here — their laid
/// lines flow through the shared text arm below (per-op ink already
/// rides the runs, so builders split paint without new `DrawOp`s).
/// Payload gaps refuse loudly (the path arm's rule — [`Canvas::build`]
/// already enforces these for component-built nodes).
pub(super) fn emit_canvas_ops(
    plan: &mut FramePlan,
    rec: &Reconciler,
    id: NodeId,
    b: &LayoutBox,
    style: &Style,
    opacity: f32,
) {
    if style.bg.is_some() {
        panic!("plan: bg on a Canvas node {id:?} — canvas paint rides the spec, never style bg");
    }
    if style.border.is_some() || style.border_edges.is_some() {
        panic!(
            "plan: border on a Canvas node {id:?} — canvas paint rides the spec, never style rings"
        );
    }
    if style.bg_gradient.is_some() {
        panic!("plan: bg_gradient on a Canvas node {id:?} — canvas paint rides the spec, never style gradients");
    }
    if style.bg_radial.is_some() {
        panic!("plan: bg_radial on a Canvas node {id:?} — canvas paint rides the spec, never style gradients");
    }
    if style.shadow.is_some() {
        panic!("plan: shadow on a Canvas node {id:?} — canvas paint rides the spec, never style shadows");
    }
    if style.circle || style.has_any_radius() {
        panic!("plan: radius/circle on a Canvas node {id:?} — canvas carries its own geometry, never style shape flags");
    }
    if style.ink.is_some() {
        panic!(
            "plan: ink on a Canvas node {id:?} — canvas text carries per-op ink, never style ink"
        );
    }
    let Some(spec) = rec.get(id).and_then(|n| n.canvas.clone()) else {
        panic!("plan: Tag::Canvas node {id:?} without a canvas payload — Canvas nodes carry CanvasSpec (hand-built Canvas elements must set it)");
    };
    for op in &spec.ops {
        match op {
            oppa::CanvasOp::Rect { x, y, w, h, color } => {
                plan.ops.push(DrawOp::Rect {
                    node: id,
                    x: b.x + x.get(),
                    y: b.y + y.get(),
                    w: w.get(),
                    h: h.get(),
                    color: *color,
                    opacity,
                    xf: PaintXf::identity(),
                });
            }
            oppa::CanvasOp::RRect {
                x,
                y,
                w,
                h,
                radius,
                color,
            } => {
                plan.ops.push(DrawOp::RRect {
                    node: id,
                    x: b.x + x.get(),
                    y: b.y + y.get(),
                    w: w.get(),
                    h: h.get(),
                    radius: radius.get(),
                    radii: None,
                    color: *color,
                    opacity,
                    xf: PaintXf::identity(),
                });
            }
            oppa::CanvasOp::Path { data, fill, stroke } => {
                if data.trim().is_empty() {
                    panic!("plan: Canvas node {id:?} has blank path data — refused, never a silent no-op");
                }
                if fill.is_none() && stroke.is_none() {
                    panic!("plan: Canvas node {id:?} has neither fill nor stroke — refused, never a silent no-op");
                }
                if let Some(s) = stroke {
                    if !s.width.is_finite() || s.width < 0.0 {
                        panic!(
                            "plan: Canvas node {id:?} stroke width ({}) is non-finite or negative — never paints silently",
                            s.width
                        );
                    }
                }
                plan.ops.push(DrawOp::Path {
                    node: id,
                    x: b.x,
                    y: b.y,
                    width: b.w,
                    height: b.h,
                    data: data.clone(),
                    fill: *fill,
                    stroke: *stroke,
                    opacity,
                    xf: PaintXf::identity(),
                });
            }
            oppa::CanvasOp::Text { .. } => {
                // Laid lines paint through the shared text arm below.
            }
        }
    }
}
