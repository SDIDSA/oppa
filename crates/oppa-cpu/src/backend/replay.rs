//! FramePlan replay: validate loudly, splice retained ops, raster every op in plan order.

use std::collections::HashMap;

use oppa::{BackendError, DrawOp, FontId, FontRun, ImageId, PaintXf};

use tiny_skia::{FillRule, Pixmap, PixmapPaint, Transform};

use crate::path::build_path;

use super::raster::{
    clip_mask, fill_disc, fill_rounded, fill_rounded_corners, intersect_rect,
    paint_box_blurred_shadow, paint_of, rasterize_glyphs, rect_of, surface_bg,
};
use super::{FaceTable, Surface};

/// Residual paint affine as a tiny-skia transform (`from_row` takes
/// `(sx, ky, kx, sy, tx, ty)` — the column order, not the row order).
fn ts_xf(xf: PaintXf) -> Transform {
    Transform::from_row(xf.a, xf.c, xf.b, xf.d, xf.tx, xf.ty)
}

/// Axis-aligned baked bounds of a detour op (the manual helpers all
/// paint axis-aligned, so the temp layer covers exactly this box plus
/// a 4px fringe for AA/outline overshoot).
fn detour_bounds(op: &DrawOp) -> Option<(f32, f32, f32, f32)> {
    const PAD: f32 = 4.0;
    let (x, y, w, h) = match op {
        DrawOp::RRect { x, y, w, h, .. } => (*x, *y, *w, *h),
        DrawOp::Circle { cx, cy, r, .. } => (cx - r, cy - r, 2.0 * r, 2.0 * r),
        DrawOp::Shadow {
            x,
            y,
            w,
            h,
            dx,
            dy,
            blur_radius,
            ..
        } => {
            // Blurred shadows spread the box-blur kernel past the box
            // (same `2k+1` margin the direct path spreads).
            let k = blur_radius.ceil().max(1.0) as i32;
            let m = (k * 2 + 1) as f32;
            (x + dx - m, y + dy - m, w + 2.0 * m, h + 2.0 * m)
        }
        DrawOp::Text {
            x,
            y,
            glyphs,
            line_height,
            ..
        } => {
            if glyphs.is_empty() {
                return None;
            }
            let mut min_x = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            for g in glyphs {
                min_x = min_x.min(g.x);
                max_x = max_x.max(g.x + g.advance);
            }
            (x + min_x, *y, (max_x - min_x).max(0.0), *line_height)
        }
        _ => return None,
    };
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some((x - PAD, y - PAD, w + 2.0 * PAD, h + 2.0 * PAD))
}

/// Temp-layer detour for rotated manual-helper ops (decision 517):
/// the op paints axis-aligned into a scratch layer at a shifted
/// origin (clips/AA fringe included by the bounds pad — no clip
/// applies inside the temp), then the layer blits through the
/// residual with the target-space clip mask. Returns false when
/// nothing paints (degenerate bounds or layer alloc failure — the
/// direct path's quiet rule, never a partial paint).
fn paint_rotated(
    target: &mut Pixmap,
    op: &DrawOp,
    xf: PaintXf,
    faces: &FaceTable,
    images: &std::collections::HashMap<ImageId, Pixmap>,
    alpha: f32,
    mask: Option<&tiny_skia::Mask>,
) -> bool {
    let _ = images;
    let Some((ox, oy, ow, oh)) = detour_bounds(op) else {
        return false;
    };
    let (tw, th) = (ow.ceil() as u32, oh.ceil() as u32);
    if tw == 0 || th == 0 {
        return false;
    }
    let Some(mut layer) = Pixmap::new(tw, th) else {
        return false;
    };
    // Shift the op to the temp origin (glyph cells stay relative —
    // only the op origin moves, like the ST bake).
    let shift = |v: f32, o: f32| v - o;
    match op {
        DrawOp::RRect {
            x,
            y,
            w,
            h,
            radius,
            radii,
            color,
            opacity,
            ..
        } => {
            let a = alpha * opacity;
            if a <= 0.0 {
                return false;
            }
            let paint = paint_of(*color, a);
            let r = (shift(*x, ox), shift(*y, oy), *w, *h);
            match radii {
                Some(rs) => fill_rounded_corners(&mut layer, r, *rs, &paint, None),
                None => fill_rounded(&mut layer, r, *radius, &paint, None),
            }
        }
        DrawOp::Circle {
            cx,
            cy,
            r,
            color,
            opacity,
            ..
        } => {
            let a = alpha * opacity;
            if a <= 0.0 || *r <= 0.0 {
                return false;
            }
            fill_disc(
                &mut layer,
                shift(*cx, ox),
                shift(*cy, oy),
                *r,
                &paint_of(*color, a),
                None,
            );
        }
        DrawOp::Shadow {
            x,
            y,
            w,
            h,
            dx,
            dy,
            blur_radius,
            color,
            ..
        } => {
            if alpha <= 0.0 || *w <= 0.0 || *h <= 0.0 {
                return false;
            }
            if *blur_radius <= 0.0 {
                if let Some(rc) = rect_of(shift(x + dx, ox), shift(y + dy, oy), *w, *h) {
                    layer.fill_rect(rc, &paint_of(*color, alpha), Transform::identity(), None);
                }
            } else {
                if !blur_radius.is_finite() {
                    panic!(
                        "cpu: shadow blur_radius non-finite ({blur_radius}) — never paints silently"
                    );
                }
                if !paint_box_blurred_shadow(
                    &mut layer,
                    shift(x + dx, ox),
                    shift(y + dy, oy),
                    *w,
                    *h,
                    *blur_radius,
                    *color,
                    alpha,
                    None,
                ) {
                    return false;
                }
            }
        }
        DrawOp::Text {
            x,
            y,
            line_height,
            baseline,
            em_size,
            glyphs,
            fonts,
            ink,
            opacity,
            ..
        } => {
            let a = alpha * opacity;
            if a <= 0.0 || glyphs.is_empty() {
                return false;
            }
            let implicit = [FontRun {
                glyph_range: (0, glyphs.len()),
                family: String::new(),
                font_id: FontId(0),
            }];
            let runs: &[FontRun] = if fonts.is_empty() { &implicit } else { fonts };
            let (sx, sy) = (shift(*x, ox), shift(*y, oy));
            for run in runs {
                let (lo, hi) = (
                    run.glyph_range.0.min(glyphs.len()),
                    run.glyph_range.1.min(glyphs.len()),
                );
                if lo >= hi {
                    continue;
                }
                let cells = &glyphs[lo..hi];
                match faces.face_for(run.font_id) {
                    Some(face) => {
                        rasterize_glyphs(
                            &mut layer,
                            face,
                            cells,
                            sx,
                            sy,
                            *baseline,
                            *em_size,
                            *ink,
                            a,
                            &[],
                        );
                    }
                    None => {
                        let paint = paint_of(*ink, a);
                        for g in cells {
                            if g.advance <= 0.0 {
                                continue;
                            }
                            if let Some(rc) = rect_of(sx + g.x, sy, g.advance, *line_height) {
                                layer.fill_rect(rc, &paint, Transform::identity(), None);
                            }
                        }
                    }
                }
            }
        }
        _ => return false,
    }
    // Blit: temp-local → target through the residual about the temp
    // origin (drawn at 0,0 — the full map rides the transform, so no
    // rounding moves rotated pixels).
    let blit = Transform::from_row(
        xf.a,
        xf.c,
        xf.b,
        xf.d,
        xf.a * ox + xf.b * oy + xf.tx,
        xf.c * ox + xf.d * oy + xf.ty,
    );
    target.draw_pixmap(
        0,
        0,
        layer.as_ref(),
        &PixmapPaint {
            opacity: 1.0,
            ..PixmapPaint::default()
        },
        blit,
        mask,
    );
    true
}
pub(super) fn replay(
    surface: &mut Surface,
    ops: &[DrawOp],
    faces: &FaceTable,
    images: &HashMap<ImageId, Pixmap>,
) -> Result<usize, BackendError> {
    // Validate loudly before touching pixels: no partial paints.
    // Registered images paint; unregistered ids refuse (pending images
    // never paint placeholders — G8, decision 223). Path data parses
    // here too (decision 291 — invalid vectors fail the whole paint,
    // never mis-paint half a frame).
    for op in ops {
        if let DrawOp::RImg { node, image, .. } = op {
            if !images.contains_key(image) {
                return Err(BackendError::UnsupportedOp(format!(
                    "RImg on {node:?} (image {image:?}): pixels not registered — \
                     decode via oppa-image and insert_image first (no decoded pixels \
                     paint placeholders, never silently)"
                )));
            }
        }
        if let DrawOp::Path { node, data, .. } = op {
            if let Err(why) = build_path(data) {
                return Err(BackendError::UnsupportedOp(format!(
                    "Path on {node:?}: invalid SVG path data ({why}) — refused, never mis-painted"
                )));
            }
        }
    }
    surface.pixmap.fill(surface_bg(surface.desc));
    let (sw, sh) = (surface.desc.width_px, surface.desc.height_px);
    let mut executed = 0usize;
    let mut alpha_stack: Vec<f32> = vec![1.0];
    // Clip stack as geometric rects (PushClip/Pop are LIFO-disciplined by
    // the builder: ScrollArea viewports wrap their subtrees). The effective
    // clip is rebuilt into a mask per painted op — M4-scale cheap, M6+
    // optimization to cache it.
    let mut clip_stack: Vec<(f32, f32, f32, f32)> = Vec::new();
    // Merged pop order (documented approximation, exact for
    // builder-emitted plans which never mix clips and layers in one
    // subtree): layers pop before clips.
    let mut layer_depth = 0usize;
    for op in ops {
        let alpha = alpha_stack.iter().product::<f32>();
        let eff = clip_stack
            .iter()
            .copied()
            .reduce(intersect_rect)
            .unwrap_or((0.0, 0.0, sw as f32, sh as f32));
        let full_surface = eff.0 <= 0.0 && eff.1 <= 0.0 && eff.2 >= sw as f32 && eff.3 >= sh as f32;
        // Borrow dance: the mask (if any) is built fresh per op.
        let mask = if full_surface {
            None
        } else {
            clip_mask(sw, sh, eff.0, eff.1, eff.2, eff.3)
        };
        let mask_ref = mask.as_ref();
        match op {
            DrawOp::Rect {
                x,
                y,
                w,
                h,
                color,
                opacity,
                ..
            } => {
                let a = alpha * opacity;
                if a > 0.0 {
                    let xf = op.xf();
                    let t = if xf.is_identity() {
                        Transform::identity()
                    } else {
                        ts_xf(xf)
                    };
                    if let Some(rc) = rect_of(*x, *y, *w, *h) {
                        surface
                            .pixmap
                            .fill_rect(rc, &paint_of(*color, a), t, mask_ref);
                        executed += 1;
                    }
                }
            }
            DrawOp::RRect {
                x,
                y,
                w,
                h,
                radius,
                radii,
                color,
                opacity,
                ..
            } => {
                let a = alpha * opacity;
                if a > 0.0 {
                    let xf = op.xf();
                    if xf.is_identity() {
                        // Round 11.1: per-corner radii generalize the
                        // bands-and-discs rule (each corner disc takes its
                        // own radius; bands span the edge maxima — the
                        // uniform shape is the all-equal special case,
                        // pixel-identical to before).
                        match radii {
                            Some(rs) => fill_rounded_corners(
                                &mut surface.pixmap,
                                (*x, *y, *w, *h),
                                *rs,
                                &paint_of(*color, a),
                                mask_ref,
                            ),
                            None => fill_rounded(
                                &mut surface.pixmap,
                                (*x, *y, *w, *h),
                                *radius,
                                &paint_of(*color, a),
                                mask_ref,
                            ),
                        }
                        executed += 1;
                    } else if paint_rotated(
                        &mut surface.pixmap,
                        op,
                        xf,
                        faces,
                        images,
                        alpha,
                        mask_ref,
                    ) {
                        executed += 1;
                    }
                }
            }
            DrawOp::Circle {
                cx,
                cy,
                r,
                color,
                opacity,
                ..
            } => {
                let a = alpha * opacity;
                if a > 0.0 && *r > 0.0 {
                    let xf = op.xf();
                    if xf.is_identity() {
                        fill_disc(
                            &mut surface.pixmap,
                            *cx,
                            *cy,
                            *r,
                            &paint_of(*color, a),
                            mask_ref,
                        );
                        executed += 1;
                    } else if paint_rotated(
                        &mut surface.pixmap,
                        op,
                        xf,
                        faces,
                        images,
                        alpha,
                        mask_ref,
                    ) {
                        executed += 1;
                    }
                }
            }
            DrawOp::Shadow {
                x,
                y,
                w,
                h,
                dx,
                dy,
                blur_radius,
                color,
                ..
            } => {
                if *blur_radius <= 0.0 {
                    // Offset solid (the pre-PR4 shape, pixel-exact).
                    let xf = op.xf();
                    let t = if xf.is_identity() {
                        Transform::identity()
                    } else {
                        ts_xf(xf)
                    };
                    if let Some(rc) = rect_of(x + dx, y + dy, *w, *h) {
                        surface
                            .pixmap
                            .fill_rect(rc, &paint_of(*color, alpha), t, mask_ref);
                        executed += 1;
                    }
                } else {
                    // Box-blurred shadow (Phase 36 PR4, decision 356):
                    // solid rect into a layer pixmap, two separable box
                    // passes (horizontal + vertical), composited back.
                    // Radius in device px; non-finite refuses loudly.
                    // Rotated shadows ride the temp-layer detour (the
                    // blur kernel is axis-aligned — decision 517).
                    if !blur_radius.is_finite() {
                        panic!(
                            "cpu: shadow blur_radius non-finite ({blur_radius}) — never paints silently"
                        );
                    }
                    let xf = op.xf();
                    if xf.is_identity() {
                        if paint_box_blurred_shadow(
                            &mut surface.pixmap,
                            x + dx,
                            y + dy,
                            *w,
                            *h,
                            *blur_radius,
                            *color,
                            alpha,
                            mask_ref,
                        ) {
                            executed += 1;
                        }
                    } else if paint_rotated(
                        &mut surface.pixmap,
                        op,
                        xf,
                        faces,
                        images,
                        alpha,
                        mask_ref,
                    ) {
                        executed += 1;
                    }
                }
            }
            DrawOp::Text {
                x,
                y,
                line_height,
                baseline,
                em_size,
                glyphs,
                fonts,
                ink,
                opacity,
                ..
            } => {
                let a = alpha * opacity;
                if a > 0.0 && !glyphs.is_empty() {
                    // Rotated text rides the temp-layer detour (glyph
                    // coverage is axis-aligned — decision 517).
                    let xf = op.xf();
                    if !xf.is_identity() {
                        if paint_rotated(
                            &mut surface.pixmap,
                            op,
                            xf,
                            faces,
                            images,
                            alpha,
                            mask_ref,
                        ) {
                            executed += 1;
                        }
                    } else {
                        // Run segmentation mirrors the Vello encoder: empty
                        // `fonts` (hand-built plans) is one implicit run
                        // over all cells.
                        let implicit = [FontRun {
                            glyph_range: (0, glyphs.len()),
                            family: String::new(),
                            font_id: FontId(0),
                        }];
                        let runs: &[FontRun] = if fonts.is_empty() { &implicit } else { fonts };
                        for run in runs {
                            let (lo, hi) = (
                                run.glyph_range.0.min(glyphs.len()),
                                run.glyph_range.1.min(glyphs.len()),
                            );
                            if lo >= hi {
                                continue;
                            }
                            let cells = &glyphs[lo..hi];
                            match faces.face_for(run.font_id) {
                                Some(face) => {
                                    executed += rasterize_glyphs(
                                        &mut surface.pixmap,
                                        face,
                                        cells,
                                        *x,
                                        *y,
                                        *baseline,
                                        *em_size,
                                        *ink,
                                        a,
                                        &clip_stack,
                                    );
                                }
                                // No usable face: legacy advance-cell bars
                                // (all pre-existing oracle behavior preserved).
                                None => {
                                    let paint = paint_of(*ink, a);
                                    for g in cells {
                                        if g.advance <= 0.0 {
                                            continue;
                                        }
                                        if let Some(rc) =
                                            rect_of(x + g.x, *y, g.advance, *line_height)
                                        {
                                            surface.pixmap.fill_rect(
                                                rc,
                                                &paint,
                                                Transform::identity(),
                                                mask_ref,
                                            );
                                            executed += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            DrawOp::Path {
                x,
                y,
                data,
                fill,
                stroke,
                opacity,
                ..
            } => {
                let a = alpha * opacity;
                if a > 0.0 {
                    // Validated above — unparseable data never reaches paint.
                    let path = build_path(data)
                        .expect("validated above — invalid paths never reach paint");
                    // Rotation composes outside the box offset
                    // (decision 517): `xf ∘ translate(x, y)` as one
                    // row — identity keeps the exact old transform.
                    let xf = op.xf();
                    let t = if xf.is_identity() {
                        Transform::from_translate(*x, *y)
                    } else {
                        Transform::from_row(
                            xf.a,
                            xf.c,
                            xf.b,
                            xf.d,
                            xf.a * x + xf.b * y + xf.tx,
                            xf.c * x + xf.d * y + xf.ty,
                        )
                    };
                    if let Some(fill_color) = fill {
                        surface.pixmap.fill_path(
                            &path,
                            &paint_of(*fill_color, a),
                            FillRule::Winding,
                            t,
                            mask_ref,
                        );
                        executed += 1;
                    }
                    if let Some(s) = stroke {
                        // Zero-width strokes skip: tiny-skia strokes a
                        // width-0 path as a hairline, which would be a
                        // silent surprise (negative widths refuse at
                        // plan build, so only zero reaches here).
                        if s.width > 0.0 {
                            let ts_stroke = tiny_skia::Stroke {
                                width: s.width,
                                miter_limit: 4.0,
                                line_cap: tiny_skia::LineCap::Round,
                                line_join: tiny_skia::LineJoin::Round,
                                dash: None,
                            };
                            surface.pixmap.stroke_path(
                                &path,
                                &paint_of(s.color, a),
                                &ts_stroke,
                                t,
                                mask_ref,
                            );
                            executed += 1;
                        }
                    }
                }
            }
            DrawOp::RImg {
                x, y, w, h, image, ..
            } => {
                let a = alpha_stack.iter().product::<f32>();
                if a > 0.0 && *w > 0.0 && *h > 0.0 {
                    let pixmap = images
                        .get(image)
                        .expect("validated above — unregistered images never reach paint");
                    let (iw, ih) = (pixmap.width() as f32, pixmap.height() as f32);
                    if iw > 0.0 && ih > 0.0 {
                        // Source→dest scale (from_row: [sx 0 0 sy 0 0] —
                        // the (x, y) offset rides draw_pixmap's own args).
                        // Rotated images fold the offset into the row
                        // (decision 517): `xf ∘ translate(x, y) ∘ scale`
                        // drawn at the origin — identity keeps the exact
                        // old rounded-position path.
                        let xf = op.xf();
                        let (dx, dy, t) = if xf.is_identity() {
                            (
                                x.round() as i32,
                                y.round() as i32,
                                Transform::from_row(*w / iw, 0.0, 0.0, *h / ih, 0.0, 0.0),
                            )
                        } else {
                            let (sx, sy) = (*w / iw, *h / ih);
                            (
                                0,
                                0,
                                Transform::from_row(
                                    xf.a * sx,
                                    xf.c * sx,
                                    xf.b * sy,
                                    xf.d * sy,
                                    xf.a * x + xf.b * y + xf.tx,
                                    xf.c * x + xf.d * y + xf.ty,
                                ),
                            )
                        };
                        surface.pixmap.draw_pixmap(
                            dx,
                            dy,
                            pixmap.as_ref(),
                            &PixmapPaint {
                                opacity: a.clamp(0.0, 1.0),
                                ..PixmapPaint::default()
                            },
                            t,
                            mask_ref,
                        );
                        executed += 1;
                    }
                }
            }
            DrawOp::PushClip { x, y, w, h } => {
                clip_stack.push((*x, *y, *w, *h));
            }
            DrawOp::PushLayer { opacity } => {
                alpha_stack.push(opacity.clamp(0.0, 1.0));
                layer_depth += 1;
            }
            DrawOp::Pop => {
                if layer_depth > 0 {
                    alpha_stack.pop();
                    layer_depth -= 1;
                } else if !clip_stack.is_empty() {
                    clip_stack.pop();
                }
            }
        }
    }
    Ok(executed)
}
