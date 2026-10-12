//! Node dispatch: [`FramePlan`] ops for one dirty retained node (M4 damage discipline).

use oppa::{
    CaretPaint, DrawOp, FramePlan, HandlePaint, Interner, LayoutBox, NodeId, PlacedGlyph,
    Reconciler, SelectionPaint, Style, Tag, CARET_WIDTH_PX, HANDLE_KNOB_R_PX, HANDLE_WIDTH_PX,
};

use super::leaf::{emit_canvas_ops, emit_path_op};
use super::resolve::{opacity_of, resolve_edge_bands, resolve_ink, resolve_radii};
use super::shape::{emit_edge_bands, emit_gradient, emit_shadow, emit_shape};

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_node_ops(
    plan: &mut FramePlan,
    rec: &Reconciler,
    styles: &Interner<Style>,
    style: &Style,
    b: &LayoutBox,
    id: NodeId,
    tag: Tag,
    image: Option<oppa::ImageId>,
    dpr: f32,
    eval: Option<(&oppa::TransitionEvaluator, f64)>,
    selection: Option<SelectionPaint>,
    caret: Option<CaretPaint>,
    handles: Option<(HandlePaint, HandlePaint)>,
    tokens: oppa::ThemeTokens,
) {
    // M8 evaluated values: live interpolations paint mid-flight, stamped
    // recycles paint their jumped targets, everything else paints the
    // retained values (the overlay is identity off the evaluator's
    // tracked set).
    let (bg, opacity) = match eval {
        Some((e, now)) => (
            e.resolve_bg(id, style.bg, now),
            e.resolve_opacity(id, opacity_of(style), now),
        ),
        None => (style.bg, opacity_of(style)),
    };
    let mut style_evaluated;
    let style_ref: &Style = match eval {
        Some(_) => {
            style_evaluated = style.clone();
            style_evaluated.bg = bg;
            style_evaluated.opacity = Some(oppa::Px::of(opacity));
            &style_evaluated
        }
        None => style,
    };
    let opacity = opacity_of(style_ref);
    if opacity <= 0.0 {
        return;
    }
    if tag == Tag::Image {
        // Static images (Phase 36 PR4, decision 359): the op records
        // the intent with the retained id; backends serve one
        // `ImageCache` deposit (`insert_cached` on CPU/Vello, data URI
        // on DOM) and refuse undeposited ids loudly. (`None` — a
        // hand-built Image node without `Img` — keeps the legacy
        // dummy id, refused downstream just as loudly.)
        plan.ops.push(DrawOp::RImg {
            node: id,
            x: b.x,
            y: b.y,
            w: b.w,
            h: b.h,
            image: image.unwrap_or(oppa::ImageId(0)),
        });
        return;
    }
    // Vector leaf (decision 291): style paint fields never apply to a
    // path (its paint rides the payload) — any of them set is an
    // authoring bug and refuses loudly, so no backend can silently
    // diverge on it. The path op carries the committed box origin +
    // size with the retained payload.
    if tag == Tag::Path {
        emit_path_op(plan, rec, id, b, style_ref, opacity);
        return;
    }
    // Retained canvas (Phase 36 PR4, decision 358): style paint
    // fields never apply (paint rides the spec — the path rule), the
    // spec lowers to shape ops at the committed box origin, and the
    // laid text lines paint through the shared text arm below (the
    // node carries no text payload, so nothing double-paints).
    if tag == Tag::Canvas {
        emit_canvas_ops(plan, rec, id, b, style_ref, opacity);
    }
    if let Some(shadow) = &style_ref.shadow {
        emit_shadow(plan, id, b, shadow, dpr);
    }
    // Round 1.3 (decision 254): conflicting paint specs refuse loudly —
    // the builder cannot know which fill the author meant, so ambiguity
    // is a spec bug, never a precedence guess. Validated before the
    // opacity early-return so invisible subtrees stay loud too.
    if let Some(shadow) = &style_ref.shadow {
        let blur = shadow.blur.get();
        if !blur.is_finite() {
            panic!("plan: shadow blur is non-finite ({blur}) — NaN/Inf never paints silently");
        }
        if blur < 0.0 {
            panic!("plan: shadow blur is negative ({blur}) — negative blur never paints silently");
        }
    }
    if style_ref.border.is_some() && style_ref.border_edges.is_some() {
        panic!("plan: border + border_edges both set — ambiguous ring spec; set exactly one");
    }
    if style_ref.bg_gradient.is_some() && style_ref.bg.is_some() {
        panic!("plan: bg + bg_gradient both set — gradient replaces bg; set exactly one");
    }
    if style_ref.bg_gradient.is_some() && style_ref.has_any_radius() {
        panic!("plan: bg_gradient with radius/circle — sharp strips vs round shape (follow-up)");
    }
    // Inset border ring (M5 `Style::border`): an outer fill in the
    // border color plus an inset background fill, reusing the existing
    // shape ops (no new DrawOp, no backend change). Paint-only and
    // inset, so layout never moves. Round 1.3: `border_edges` paints
    // four sharp bands over the fill instead (validated + emitted
    // below); radius/circle with edges refuses loudly (sharp bands vs
    // round shape), while the uniform ring keeps honoring them.
    let border_w = style_ref.border.and_then(|bd| {
        if bd.color != oppa::Color::TRANSPARENT
            && bd.width.get() > 0.0
            && b.w > 2.0 * bd.width.get()
            && b.h > 2.0 * bd.width.get()
        {
            Some((bd.width.get() * dpr, bd.color))
        } else {
            None
        }
    });
    let edge_bands = style_ref
        .border_edges
        .and_then(|e| resolve_edge_bands(e, dpr));
    if edge_bands.is_some() && (style_ref.has_any_radius()) {
        panic!("plan: border_edges with radius/circle — sharp bands vs round shape (follow-up)");
    }
    if let Some(g) = style_ref.bg_gradient {
        if b.w > 0.0 && b.h > 0.0 {
            if let Some((w, border_color)) = border_w {
                // Uniform-ring outer + inset gradient (mirrors the solid
                // ring path below: outer fill, then the fill inset).
                emit_shape(
                    plan,
                    style_ref,
                    b,
                    id,
                    tag,
                    border_color,
                    opacity,
                    dpr,
                    0.0,
                    0.0,
                    b.w,
                    b.h,
                );
                emit_gradient(
                    plan,
                    id,
                    b.x + w,
                    b.y + w,
                    b.w - 2.0 * w,
                    b.h - 2.0 * w,
                    g,
                    opacity,
                );
            } else {
                emit_gradient(plan, id, b.x, b.y, b.w, b.h, g, opacity);
            }
        }
    } else if let Some(bg) = style_ref.bg {
        if bg != oppa::Color::TRANSPARENT && b.w > 0.0 && b.h > 0.0 {
            if let Some((w, border_color)) = border_w {
                emit_shape(
                    plan,
                    style_ref,
                    b,
                    id,
                    tag,
                    border_color,
                    opacity,
                    dpr,
                    0.0,
                    0.0,
                    b.w,
                    b.h,
                );
                emit_shape(
                    plan,
                    style_ref,
                    b,
                    id,
                    tag,
                    bg,
                    opacity,
                    dpr,
                    w,
                    w,
                    b.w - 2.0 * w,
                    b.h - 2.0 * w,
                );
            } else if style_ref.circle {
                plan.ops.push(DrawOp::Circle {
                    node: id,
                    cx: b.x + b.w * 0.5,
                    cy: b.y + b.h * 0.5,
                    r: b.w.min(b.h) * 0.5,
                    color: bg,
                    opacity,
                });
            } else if let Some(radii) = resolve_radii(style_ref, b.w, b.h, dpr) {
                // Round 11.1: per-corner radii ride `RRect.radii`
                // (the uniform `radius` below stays the resolved
                // shorthand for info — backends branch on `radii`).
                plan.ops.push(DrawOp::RRect {
                    node: id,
                    x: b.x,
                    y: b.y,
                    w: b.w,
                    h: b.h,
                    radius: style_ref.radius.map(|r| r.get() * dpr).unwrap_or(0.0),
                    radii: Some(radii),
                    color: bg,
                    opacity,
                });
            } else if let Some(r) = style_ref.radius {
                let radius = r.get() * dpr;
                if radius > 0.0 {
                    plan.ops.push(DrawOp::RRect {
                        node: id,
                        x: b.x,
                        y: b.y,
                        w: b.w,
                        h: b.h,
                        radius,
                        radii: None,
                        color: bg,
                        opacity,
                    });
                } else {
                    plan.ops.push(DrawOp::Rect {
                        node: id,
                        x: b.x,
                        y: b.y,
                        w: b.w,
                        h: b.h,
                        color: bg,
                        opacity,
                    });
                }
            } else {
                plan.ops.push(DrawOp::Rect {
                    node: id,
                    x: b.x,
                    y: b.y,
                    w: b.w,
                    h: b.h,
                    color: bg,
                    opacity,
                });
            }
        }
    }
    // Per-edge inset bands overlay the fill (inset like the ring, never
    // layout — bands wider than the box clamp to it, never overflow it).
    if let Some(([top, right, bottom, left], color)) = edge_bands {
        if b.w > 0.0 && b.h > 0.0 {
            emit_edge_bands(plan, id, b, [top, right, bottom, left], color, opacity);
        }
    }
    if tag == Tag::Text || tag == Tag::Canvas {
        // Round 8.2 (decision 298): the themed selection rectangle —
        // one `Rect` per overlapping laid line, emitted BEFORE the
        // line's `Text` op (background order) through the shared
        // `selection_rects` rule, so CPU and Vello agree by
        // construction and no backend carries selection logic.
        // Canvas nodes never own an editing session, so the
        // selection gate below stays shut for them (their laid text
        // lines still paint through the shared loop).
        if let Some(sel) = selection {
            if sel.range.0 != sel.range.1 && oppa::input::is_within(rec, id, sel.field) {
                for r in b.selection_rects(sel.range) {
                    let (x0, y0, x1, y1) = (r[0], r[1], r[2], r[3]);
                    if x1 > x0 && y1 > y0 {
                        plan.ops.push(DrawOp::Rect {
                            node: id,
                            x: x0,
                            y: y0,
                            w: x1 - x0,
                            h: y1 - y0,
                            color: oppa::SELECTION_FILL,
                            opacity,
                        });
                    }
                }
            }
        }
        // Ordered selected byte span for ink inversion below (`None`
        // when nothing selects here: collapsed, foreign field, or no
        // selection — the same gate as the highlight rects above, so
        // rects and inverted glyphs always agree).
        let sel_span: Option<(usize, usize)> = match selection {
            Some(sel)
                if sel.range.0 != sel.range.1 && oppa::input::is_within(rec, id, sel.field) =>
            {
                Some((sel.range.0.min(sel.range.1), sel.range.0.max(sel.range.1)))
            }
            _ => None,
        };
        for line in &b.lines {
            // Flatten runs into cells while recording the per-run font
            // identity (M7, decision 110): consecutive same-font runs
            // merge so fallback boundaries — not cluster boundaries —
            // are what the op carries. Phase 36 PR3 (decision 355)
            // splits at span-ink boundaries too (one op per ink run —
            // backends paint solid ink per op, so mixed-ink lines need
            // one op per ink; single-ink lines keep exactly one op —
            // existing scenes stay byte-identical). Decision 475 splits
            // at selection boundaries the same way: runs overlapping
            // the session range paint in SELECTION_INK (cluster
            // granularity — clusters never split, the locked shaping
            // rule — so the ink boundary always lands on a cluster
            // edge, exactly where the highlight rect does);
            // unselected runs keep their span/theme ink and
            // selection-free scenes keep byte-identical ops.
            let mut glyphs: Vec<PlacedGlyph> = Vec::new();
            let mut fonts: Vec<oppa::FontRun> = Vec::new();
            let mut ink_run: Option<oppa::Color> = None;
            let mut flush = |glyphs: &mut Vec<PlacedGlyph>,
                             fonts: &mut Vec<oppa::FontRun>,
                             ink_run: &mut Option<oppa::Color>| {
                if glyphs.is_empty() {
                    return;
                }
                // Glyph cells carry the layout's pre-positioned advances into
                // the backend; the backend fills cells, never re-shapes.
                // `baseline` rides along for the GPU backend (M6 decision
                // 105); the CPU backend ignores it, `em_size`/`fonts` alike
                // (cells unchanged).
                plan.ops.push(DrawOp::Text {
                    node: id,
                    x: b.x,
                    y: b.y + line.y,
                    line_height: line.height,
                    baseline: line.baseline,
                    em_size: line.em_size,
                    glyphs: std::mem::take(glyphs),
                    fonts: std::mem::take(fonts),
                    ink: ink_run
                        .take()
                        .unwrap_or_else(|| resolve_ink(rec, styles, id, style, &tokens)),
                    opacity,
                });
            };
            for run in &line.runs {
                if run.glyphs.is_empty() {
                    continue;
                }
                // Selection ink boundary: a run overlapping the live
                // range paints selected (any overlap selects the whole
                // cluster — clusters are atomic); otherwise it keeps
                // its span ink. `None` runs join the open op
                // (single-style fast path — no extra ops).
                let selected =
                    sel_span.is_some_and(|(lo, hi)| run.byte_range.0 < hi && run.byte_range.1 > lo);
                let eff_ink = if selected {
                    Some(oppa::SELECTION_INK)
                } else {
                    run.ink
                };
                if ink_run != eff_ink && !glyphs.is_empty() {
                    flush(&mut glyphs, &mut fonts, &mut ink_run);
                }
                if ink_run.is_none() {
                    ink_run = eff_ink;
                }
                let base = glyphs.len();
                glyphs.extend(run.glyphs.iter().map(|g| PlacedGlyph {
                    glyph_id: g.glyph_id,
                    x: g.x,
                    advance: g.x_advance,
                }));
                let span = (base, glyphs.len());
                match fonts.last_mut() {
                    Some(last) if last.family == run.family && last.font_id == run.font_id => {
                        last.glyph_range.1 = span.1
                    }
                    _ => fonts.push(oppa::FontRun {
                        glyph_range: span,
                        family: run.family.clone(),
                        font_id: run.font_id,
                    }),
                }
            }
            flush(&mut glyphs, &mut fonts, &mut ink_run);
        }
    }
    // Round 15.1 (decision 312): the focused caret bar — one
    // `CARET_WIDTH_PX`-wide vertical `Rect` in the field's text ink,
    // emitted exactly once on the field container itself (after its
    // text, so the bar paints above the glyphs). Degenerate heights
    // paint nothing (the host already refuses them; this is the
    // belt-and-braces, mirroring the selection's empty-overlap rule).
    if let Some(c) = caret {
        if c.field == id && c.h > 0.0 {
            plan.ops.push(DrawOp::Rect {
                node: id,
                x: c.x,
                y: c.y,
                w: CARET_WIDTH_PX,
                h: c.h,
                color: c.color,
                opacity,
            });
        }
    }
    // Decision 454: renderer-drawn selection handles — one bar plus
    // one grab knob per live edge, emitted exactly once on the field
    // container itself (after the caret, so handles paint topmost).
    // Degenerate heights paint nothing (the host already refuses
    // them; belt-and-braces like the caret above). `None` (collapsed
    // selections, unfocused fields) emits nothing — pre-454 plans are
    // byte-identical.
    if let Some((start, end)) = handles {
        for h in [start, end] {
            if h.field == id && h.h > 0.0 {
                plan.ops.push(DrawOp::Rect {
                    node: id,
                    x: h.x,
                    y: h.y,
                    w: HANDLE_WIDTH_PX,
                    h: h.h,
                    color: h.color,
                    opacity,
                });
                plan.ops.push(DrawOp::Circle {
                    node: id,
                    cx: h.x,
                    cy: h.y + h.h + HANDLE_KNOB_R_PX,
                    r: HANDLE_KNOB_R_PX,
                    color: h.color,
                    opacity,
                });
            }
        }
    }
}
