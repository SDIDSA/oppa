//! FramePlan → `vello::Scene` encoding (M6 item 1: full DrawOp coverage).
//!
//! The encoder walks the plan's ops with the same raster discipline as the
//! CPU replay (`backend.rs` in `oppa-cpu`): an alpha stack multiplied into
//! every fill, a LIFO clip/layer stack with layers-first merged pops, and
//! images painted from the backend's registry (unregistered ids refuse
//! loudly before anything stages — OQ-G8-1 closed, pending images never
//! stage placeholders). It shares no code with the CPU backend — same
//! discipline, second implementation, so the cross-backend oracle compares
//! two independent rasterizers.
//!
//! Stated approximations (see crate docs):
//!
//! - Shadows blur natively (Phase 36 PR4, decision 356): radius 0
//!   stays the offset solid (contractual degradation, identical on
//!   both backends); radius > 0 encodes a gaussian blurred rounded
//!   rect (std_dev ≈ radius/2), tol-banded against the CPU box-blur,
//!   never pixel-exact.
//! - Vector paths (decision 291): parsed via `kurbo::BezPath::from_svg`
//!   (native — the CPU backend owns its own parser because
//!   `tiny-skia-path` 0.12 ships no `from_svg`), translated by the
//!   committed box origin, filled `NonZero` + stroked round/round
//!   (the CPU backend's fixed caps, so both agree by construction).
//!   Invalid data fails the encode loudly (no partial scenes).
//! - Text: one `draw_glyphs` run per [`FontRun`](oppa::FontRun) (M7,
//!   decision 110), glyph-run origin at `y + baseline`, `font_size =
//!   em_size` (the exact em size — ends the M6 `= line_height`
//!   approximation), `hint(false)`; x positions and advances are
//!   subpixel-exact, never rounded. Hand-built plans with an empty
//!   `fonts` vec encode as one implicit `FontId(0)` run (back-compat;
//!   builder-emitted plans always carry the segmentation).
//! - `PushLayer{opacity}` becomes a viewport-clipped scene opacity layer
//!   (isolated group); the CPU multiplies per-op alpha in place. Opacity
//!   is honored on both; overlapping translucent content inside a layer
//!   may differ by the isolation (the layer test's tolerance).

use oppa::{BackendError, DrawOp, FramePlan, ImageId, PlacedGlyph, SurfaceDesc};
use vello::kurbo::{Affine, Circle, Rect, RoundedRect, RoundedRectRadii};
use vello::peniko::{BlendMode, Brush, Compose, Fill, ImageBrushRef, ImageData, Mix};

use crate::atlas::GlyphAtlas;

/// Per-encode accounting: the staged-GPU-work instrument (an empty plan
/// stages nothing — static ≈ 0 GPU work).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct EncodeStats {
    /// Shape fills staged (incl. the surface-bg fill and shadow fills).
    pub shapes_encoded: usize,
    /// Glyph runs staged via `draw_glyphs`.
    pub glyph_runs: usize,
    /// Individual glyphs staged.
    pub glyphs: usize,
}

impl EncodeStats {
    /// Staged work units (the skip tests assert this is 0 on empty plans).
    pub fn work_units(&self) -> u64 {
        self.shapes_encoded as u64 + self.glyphs as u64
    }
}

fn peniko_color(c: oppa::Color, opacity: f32) -> vello::peniko::Color {
    let a = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    vello::peniko::Color::from_rgba8(
        ((c.0 >> 16) & 0xFF) as u8,
        ((c.0 >> 8) & 0xFF) as u8,
        (c.0 & 0xFF) as u8,
        a,
    )
}

fn rect_of(x: f32, y: f32, w: f32, h: f32) -> Option<Rect> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some(Rect::new(
        x as f64,
        y as f64,
        (x + w) as f64,
        (y + h) as f64,
    ))
}

fn fill_rect_shape(
    scene: &mut vello::Scene,
    rect: (f32, f32, f32, f32),
    radius: f32,
    brush: &Brush,
) -> bool {
    let (x, y, w, h) = rect;
    if w <= 0.0 || h <= 0.0 {
        return false;
    }
    let r = radius.min(w * 0.5).min(h * 0.5);
    if r <= 0.0 {
        if let Some(rc) = rect_of(x, y, w, h) {
            scene.fill(Fill::NonZero, Affine::IDENTITY, brush, None, &rc);
            return true;
        }
        return false;
    }
    let rr = RoundedRect::new(x as f64, y as f64, (x + w) as f64, (y + h) as f64, r as f64);
    scene.fill(Fill::NonZero, Affine::IDENTITY, brush, None, &rr);
    true
}

/// Per-corner rounded fill (Round 11.1, decision 305): the kurbo
/// native — `RoundedRect::from_rect` with the builder-clamped CSS
/// radii (same shared rule as the CPU bands-and-discs, so both
/// agree by construction). All-zero falls back to the plain rect,
/// like the uniform path above.
fn fill_rounded_corners_shape(
    scene: &mut vello::Scene,
    rect: (f32, f32, f32, f32),
    radii: [f32; 4],
    brush: &Brush,
) -> bool {
    let (x, y, w, h) = rect;
    if w <= 0.0 || h <= 0.0 {
        return false;
    }
    if radii.iter().all(|r| *r <= 0.0) {
        if let Some(rc) = rect_of(x, y, w, h) {
            scene.fill(Fill::NonZero, Affine::IDENTITY, brush, None, &rc);
            return true;
        }
        return false;
    }
    let rr = RoundedRect::from_rect(
        Rect::new(x as f64, y as f64, (x + w) as f64, (y + h) as f64),
        RoundedRectRadii::new(
            radii[0] as f64,
            radii[1] as f64,
            radii[2] as f64,
            radii[3] as f64,
        ),
    );
    scene.fill(Fill::NonZero, Affine::IDENTITY, brush, None, &rr);
    true
}

/// Encodes `plan` into `scene` (reset first by the caller when replaying).
/// Returns the per-encode stats. Fails loudly on unregistered `RImg`
/// ids (pending images never stage placeholders) and on Text ops with
/// no injected font — before staging anything.
pub fn encode_plan(
    scene: &mut vello::Scene,
    plan: &FramePlan,
    surface: &SurfaceDesc,
    atlas: &mut GlyphAtlas,
    images: &std::collections::HashMap<ImageId, ImageData>,
) -> Result<EncodeStats, BackendError> {
    // Validate loudly before touching the scene: no partial encodes.
    for op in &plan.ops {
        if let DrawOp::RImg { node, image, .. } = op {
            if !images.contains_key(image) {
                return Err(BackendError::UnsupportedOp(format!(
                    "RImg on {node:?} (image {image:?}): pixels not registered — \
                     decode via oppa-image and insert_image first (no decoded pixels \
                     stage placeholders, never silently)"
                )));
            }
        }
        if let DrawOp::Text { node, glyphs, .. } = op {
            if !glyphs.is_empty() && !atlas.has_font() {
                return Err(BackendError::UnsupportedOp(format!(
                    "Text on {node:?}: no font bytes injected — single-face atlas is empty (v1 bound, finding F3)"
                )));
            }
        }
    }
    let mut stats = EncodeStats::default();
    let (sw, sh) = (surface.width_px as f32, surface.height_px as f32);
    // The log reflects the latest encode only: paints replay the whole
    // retained list, so stale placements from earlier encodes would lie.
    atlas.clear_log();

    // Surface background (mirrors the CPU replay's pixmap fill).
    let bg = Brush::Solid(peniko_color(surface.background, 1.0));
    if let Some(rc) = rect_of(0.0, 0.0, sw, sh) {
        scene.fill(Fill::NonZero, Affine::IDENTITY, &bg, None, &rc);
        stats.shapes_encoded += 1;
    }

    // Layer opacity lives scene-side (push_layer carries it); the brush
    // alpha stack stays at 1.0 so per-op opacity applies exactly once.
    let alpha_stack: Vec<f32> = vec![1.0];
    let mut layer_depth = 0usize;
    let mut scene_depth = 0usize;
    for op in &plan.ops {
        let alpha = alpha_stack.iter().product::<f32>();
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
                    let brush = Brush::Solid(peniko_color(*color, a));
                    if fill_rect_shape(scene, (*x, *y, *w, *h), 0.0, &brush) {
                        stats.shapes_encoded += 1;
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
                    let brush = Brush::Solid(peniko_color(*color, a));
                    // Round 11.1: per-corner radii encode natively
                    // (uniform keeps the exact pre-11.1 path above).
                    let staged = match radii {
                        Some(rs) => {
                            fill_rounded_corners_shape(scene, (*x, *y, *w, *h), *rs, &brush)
                        }
                        None => fill_rect_shape(scene, (*x, *y, *w, *h), *radius, &brush),
                    };
                    if staged {
                        stats.shapes_encoded += 1;
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
                    let brush = Brush::Solid(peniko_color(*color, a));
                    let c = Circle::new((*cx as f64, *cy as f64), *r as f64);
                    scene.fill(Fill::NonZero, Affine::IDENTITY, &brush, None, &c);
                    stats.shapes_encoded += 1;
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
                    // Offset solid (contractual degradation, same as CPU).
                    let brush = Brush::Solid(peniko_color(*color, alpha));
                    if fill_rect_shape(scene, (x + dx, y + dy, *w, *h), 0.0, &brush) {
                        stats.shapes_encoded += 1;
                    }
                } else {
                    // Gaussian blurred rect (Phase 36 PR4, decision 356):
                    // std_dev ≈ blur/2 (the CSS-blur approximation —
                    // tol-banded against the CPU box-blur, never exact).
                    if !blur_radius.is_finite() {
                        panic!(
                            "vello: shadow blur_radius non-finite ({blur_radius}) — never encodes silently"
                        );
                    }
                    let Some(rc) = rect_of(x + dx, y + dy, *w, *h) else {
                        continue;
                    };
                    scene.draw_blurred_rounded_rect(
                        Affine::IDENTITY,
                        rc,
                        peniko_color(*color, alpha),
                        0.0,
                        (*blur_radius as f64 / 2.0).max(0.0),
                    );
                    stats.shapes_encoded += 1;
                }
            }
            DrawOp::Text {
                x,
                y,
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
                    encode_text_runs(
                        scene, atlas, *x, *y, *em_size, *baseline, glyphs, fonts, *ink, a,
                        &mut stats,
                    )?;
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
                    let bez = vello::kurbo::BezPath::from_svg(data).map_err(|e| {
                        BackendError::UnsupportedOp(format!(
                            "Path: invalid SVG path data ({e}) — refused, never mis-staged"
                        ))
                    })?;
                    let t = Affine::translate((*x as f64, *y as f64));
                    if let Some(c) = fill {
                        let brush = Brush::Solid(peniko_color(*c, a));
                        scene.fill(Fill::NonZero, t, &brush, None, &bez);
                        stats.shapes_encoded += 1;
                    }
                    if let Some(s) = stroke {
                        if !s.width.is_finite() || s.width < 0.0 {
                            return Err(BackendError::UnsupportedOp(format!(
                                "Path: stroke width {} is non-finite/negative — refused, never mis-staged",
                                s.width
                            )));
                        }
                        // Zero-width strokes skip (kurbo would hairline —
                        // never silently).
                        if s.width > 0.0 {
                            let st = vello::kurbo::Stroke::new(s.width as f64)
                                .with_caps(vello::kurbo::Cap::Round)
                                .with_join(vello::kurbo::Join::Round);
                            let brush = Brush::Solid(peniko_color(s.color, a));
                            scene.stroke(&st, t, &brush, None, &bez);
                            stats.shapes_encoded += 1;
                        }
                    }
                }
            }
            DrawOp::RImg {
                x, y, w, h, image, ..
            } => {
                let a = alpha;
                if a > 0.0 && *w > 0.0 && *h > 0.0 {
                    let img = images
                        .get(image)
                        .expect("validated above — unregistered images never stage");
                    let (iw, ih) = (img.width as f64, img.height as f64);
                    if iw > 0.0 && ih > 0.0 {
                        // Peniko takes straight alpha (the oppa-image
                        // output shape — no conversion, unlike the CPU
                        // premultiply). Source→dest scale with the op
                        // offset as translation; layer alpha rides the
                        // brush (same citizenship as every other op).
                        // (`&ImageData` converts to the ref brush the
                        // scene takes — no clone, the registry owns.)
                        let brush = ImageBrushRef::from(img).with_alpha(a.clamp(0.0, 1.0));
                        let t = Affine::new([
                            *w as f64 / iw,
                            0.0,
                            0.0,
                            *h as f64 / ih,
                            *x as f64,
                            *y as f64,
                        ]);
                        scene.draw_image(brush, t);
                        stats.shapes_encoded += 1;
                    }
                }
            }
            DrawOp::PushClip { x, y, w, h } => {
                if let Some(rc) = rect_of(*x, *y, *w, *h) {
                    scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &rc);
                    scene_depth += 1;
                }
            }
            DrawOp::PushLayer { opacity } => {
                // Viewport-clipped opacity layer (isolated group, SrcOver).
                // The scene layer carries the opacity — the brush alpha
                // must NOT also multiply it (double application would read
                // 0.8 as 0.64; the M6 geometry-oracle fix).
                let viewport = Rect::new(0.0, 0.0, sw as f64, sh as f64);
                scene.push_layer(
                    Fill::NonZero,
                    BlendMode::new(Mix::Normal, Compose::SrcOver),
                    opacity.clamp(0.0, 1.0),
                    Affine::IDENTITY,
                    &viewport,
                );
                scene_depth += 1;
                layer_depth += 1;
            }
            DrawOp::Pop => {
                // Merged-pop discipline mirrors the CPU replay exactly
                // (layers-first): builder-emitted plans never mix clips
                // and layers in one subtree, so this is exact there.
                if layer_depth > 0 {
                    layer_depth -= 1;
                    if scene_depth > 0 {
                        scene.pop_layer();
                        scene_depth -= 1;
                    }
                } else if scene_depth > 0 {
                    scene.pop_layer();
                    scene_depth -= 1;
                }
            }
        }
    }
    Ok(stats)
}

/// One Text op → one `draw_glyphs` run per [`FontRun`](oppa::FontRun).
/// Positions come from the plan's pre-positioned cells (never
/// re-shaped); the atlas only supplies the font bytes. M7 rule
/// (decision 110): glyph-run origin at the baseline (`y + baseline`),
/// `font_size = em_size` (exact), `hint(false)`; x stays subpixel.
/// Face selection per run is explicit-id → default → loud refusal (the
/// atlas owns the order; `None` here names the missing id).
#[allow(clippy::too_many_arguments)]
fn encode_text_runs(
    scene: &mut vello::Scene,
    atlas: &mut GlyphAtlas,
    x: f32,
    y: f32,
    em_size: f32,
    baseline: f32,
    glyphs: &[PlacedGlyph],
    fonts: &[oppa::FontRun],
    ink: oppa::Color,
    alpha: f32,
    stats: &mut EncodeStats,
) -> Result<(), BackendError> {
    // Hand-built plans predate the segmentation (empty `fonts`):
    // one implicit run over the whole cell list.
    let implicit = [oppa::FontRun {
        glyph_range: (0, glyphs.len()),
        family: String::new(),
        font_id: oppa::FontId(0),
    }];
    let runs: &[oppa::FontRun] = if fonts.is_empty() { &implicit } else { fonts };
    let brush = Brush::Solid(peniko_color(ink, alpha));
    for run in runs {
        let (lo, hi) = (
            run.glyph_range.0.min(glyphs.len()),
            run.glyph_range.1.min(glyphs.len()),
        );
        if lo >= hi {
            continue;
        }
        let cells = &glyphs[lo..hi];
        let Some(font) = atlas.face_for(run.font_id).cloned() else {
            return Err(BackendError::UnsupportedOp(format!(
                "Text with no usable font face (run {:?}, family {:?}): \
                 no explicit face and no default — inject via set_font_bytes/set_font_for",
                run.font_id, run.family,
            )));
        };
        atlas.log_run(x, cells, run.font_id);
        // Positions are box-relative (`PlacedGlyph.x` is already the
        // visual pen position); the translate carries the box origin,
        // so the scene x of each glyph is exactly `x + g.x` — the
        // logged value. The y translate carries the baseline (Vello
        // glyph y=0 means baseline).
        let positioned = cells.iter().map(|g| vello::Glyph {
            id: g.glyph_id,
            x: g.x,
            y: 0.0,
        });
        scene
            .draw_glyphs(&font)
            .transform(Affine::translate((x as f64, (y + baseline) as f64)))
            .font_size(em_size)
            .hint(false)
            .brush(brush)
            .draw(Fill::NonZero, positioned);
        stats.glyph_runs += 1;
        stats.glyphs += cells.len();
    }
    Ok(())
}
