//! Display list walker and batch collector.

use std::collections::HashMap;
use std::ops::Range;

use oppa::{Color, DrawOp, FontRun, ImageId};

use crate::atlas::{FaceTable, GlyphAtlas, GpuImage};
use crate::path::tessellate_svg_path;
use crate::vertex::{PathVertex, QuadInstance, TexturedInstance};

#[derive(Clone, Debug)]
pub enum DrawBatch {
    Quad {
        scissor: (u32, u32, u32, u32),
        instances: Range<u32>,
    },
    Glyph {
        scissor: (u32, u32, u32, u32),
        instances: Range<u32>,
    },
    Image {
        scissor: (u32, u32, u32, u32),
        instances: Range<u32>,
        image_id: ImageId,
    },
    Path {
        scissor: (u32, u32, u32, u32),
        indices: Range<u32>,
    },
}

impl DrawBatch {
    pub fn scissor(&self) -> (u32, u32, u32, u32) {
        match self {
            Self::Quad { scissor, .. }
            | Self::Glyph { scissor, .. }
            | Self::Image { scissor, .. }
            | Self::Path { scissor, .. } => *scissor,
        }
    }
}

pub struct BatchCollector {
    pub batches: Vec<DrawBatch>,
    pub quads: Vec<QuadInstance>,
    pub glyphs: Vec<TexturedInstance>,
    pub images: Vec<TexturedInstance>,
    pub path_vertices: Vec<PathVertex>,
    pub path_indices: Vec<u32>,
    pub executed_ops: usize,
}

impl BatchCollector {
    pub fn new() -> Self {
        Self {
            batches: Vec::new(),
            quads: Vec::new(),
            glyphs: Vec::new(),
            images: Vec::new(),
            path_vertices: Vec::new(),
            path_indices: Vec::new(),
            executed_ops: 0,
        }
    }

    pub fn clear(&mut self) {
        self.batches.clear();
        self.quads.clear();
        self.glyphs.clear();
        self.images.clear();
        self.path_vertices.clear();
        self.path_indices.clear();
        self.executed_ops = 0;
    }
}

impl Default for BatchCollector {
    fn default() -> Self {
        Self::new()
    }
}

fn to_rgba_f32(c: Color, opacity: f32) -> [f32; 4] {
    // Style opacity multiplies the color's own alpha (24-bit colors
    // ride full opacity — see `Color::alpha_u8`).
    let a = opacity.clamp(0.0, 1.0) * f32::from(c.alpha_u8()) / 255.0;
    let r = ((c.0 >> 16) & 0xFF) as f32 / 255.0;
    let g = ((c.0 >> 8) & 0xFF) as f32 / 255.0;
    let b = (c.0 & 0xFF) as f32 / 255.0;
    [r, g, b, a]
}

fn intersect_rect(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    let x = a.0.max(b.0);
    let y = a.1.max(b.1);
    let r = (a.0 + a.2).min(b.0 + b.2);
    let b2 = (a.1 + a.3).min(b.1 + b.3);
    (x, y, (r - x).max(0.0), (b2 - y).max(0.0))
}

fn to_scissor(eff: (f32, f32, f32, f32), sw: u32, sh: u32) -> (u32, u32, u32, u32) {
    let sx = eff.0.floor().max(0.0) as u32;
    let sy = eff.1.floor().max(0.0) as u32;
    let right = (eff.0 + eff.2).ceil().min(sw as f32) as u32;
    let bottom = (eff.1 + eff.3).ceil().min(sh as f32) as u32;
    let sw_box = right.saturating_sub(sx);
    let sh_box = bottom.saturating_sub(sy);
    (sx.min(sw), sy.min(sh), sw_box, sh_box)
}

/// Residual paint affine as instance attributes (decision 517):
/// identity for ST-only chains (the shader multiplies through, so
/// unrotated instances land on the exact old pixels).
fn xf_attrs(op: &DrawOp) -> ([f32; 4], [f32; 2]) {
    let xf = op.xf();
    ([xf.a, xf.b, xf.c, xf.d], [xf.tx, xf.ty])
}

#[allow(clippy::too_many_arguments)]
pub fn collect_batches(
    ops: &[DrawOp],
    viewport_width: u32,
    viewport_height: u32,
    faces: &FaceTable,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    gpu_images: &HashMap<ImageId, GpuImage>,
    collector: &mut BatchCollector,
) -> Result<(), String> {
    collector.clear();

    let (sw, sh) = (viewport_width, viewport_height);
    let mut alpha_stack: Vec<f32> = vec![1.0];
    let mut clip_stack: Vec<(f32, f32, f32, f32)> = Vec::new();
    let mut layer_depth = 0usize;

    for op in ops {
        let alpha = alpha_stack.iter().product::<f32>();
        let eff = clip_stack
            .iter()
            .copied()
            .reduce(intersect_rect)
            .unwrap_or((0.0, 0.0, sw as f32, sh as f32));

        let scissor = to_scissor(eff, sw, sh);
        if scissor.2 == 0 || scissor.3 == 0 {
            // Invisible under empty clip; still update stacks
            match op {
                DrawOp::PushClip { x, y, w, h } => clip_stack.push((*x, *y, *w, *h)),
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
                _ => {}
            }
            continue;
        }

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
                if a > 0.0 && *w > 0.0 && *h > 0.0 {
                    let (xf_abcd, xf_t) = xf_attrs(op);
                    let instance = QuadInstance {
                        rect: [*x, *y, *w, *h],
                        color: to_rgba_f32(*color, a),
                        radii: [0.0, 0.0, 0.0, 0.0],
                        params: [0.0, 0.0, 0.0, 0.0],
                        xf_abcd,
                        xf_t,
                    };
                    push_quad(collector, scissor, instance);
                    collector.executed_ops += 1;
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
                if a > 0.0 && *w > 0.0 && *h > 0.0 {
                    let r_box = radii.unwrap_or([*radius, *radius, *radius, *radius]);
                    let (xf_abcd, xf_t) = xf_attrs(op);
                    let instance = QuadInstance {
                        rect: [*x, *y, *w, *h],
                        color: to_rgba_f32(*color, a),
                        radii: r_box,
                        params: [0.0, 0.0, 0.0, 0.0],
                        xf_abcd,
                        xf_t,
                    };
                    push_quad(collector, scissor, instance);
                    collector.executed_ops += 1;
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
                    let (xf_abcd, xf_t) = xf_attrs(op);
                    let instance = QuadInstance {
                        rect: [cx - r, cy - r, 2.0 * r, 2.0 * r],
                        color: to_rgba_f32(*color, a),
                        radii: [0.0, 0.0, 0.0, 0.0],
                        params: [1.0, 0.0, 0.0, 0.0],
                        xf_abcd,
                        xf_t,
                    };
                    push_quad(collector, scissor, instance);
                    collector.executed_ops += 1;
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
                if alpha > 0.0 && *w > 0.0 && *h > 0.0 {
                    if *blur_radius <= 0.0 {
                        let (xf_abcd, xf_t) = xf_attrs(op);
                        let instance = QuadInstance {
                            rect: [x + dx, y + dy, *w, *h],
                            color: to_rgba_f32(*color, alpha),
                            radii: [0.0, 0.0, 0.0, 0.0],
                            params: [0.0, 0.0, 0.0, 0.0],
                            xf_abcd,
                            xf_t,
                        };
                        push_quad(collector, scissor, instance);
                    } else {
                        let margin = (3.0 * blur_radius).ceil();
                        let (xf_abcd, xf_t) = xf_attrs(op);
                        let instance = QuadInstance {
                            rect: [
                                x + dx - margin,
                                y + dy - margin,
                                w + 2.0 * margin,
                                h + 2.0 * margin,
                            ],
                            color: to_rgba_f32(*color, alpha),
                            radii: [0.0, 0.0, 0.0, 0.0],
                            params: [2.0, *blur_radius, *dx, *dy],
                            xf_abcd,
                            xf_t,
                        };
                        push_quad(collector, scissor, instance);
                    }
                    collector.executed_ops += 1;
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
                    let implicit = [FontRun {
                        glyph_range: (0, glyphs.len()),
                        family: String::new(),
                        font_id: oppa::FontId(0),
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
                                let (xf_abcd, xf_t) = xf_attrs(op);
                                for g in cells {
                                    if let Some(cached) = atlas.get_or_insert(
                                        queue,
                                        face,
                                        run.font_id,
                                        g.glyph_id,
                                        *em_size,
                                    ) {
                                        let gx = x + g.x + cached.offset[0];
                                        let gy = y + baseline + cached.offset[1];
                                        let instance = TexturedInstance {
                                            rect: [gx, gy, cached.size[0], cached.size[1]],
                                            uv: cached.uv,
                                            color: to_rgba_f32(*ink, a),
                                            xf_abcd,
                                            xf_t,
                                        };
                                        push_glyph(collector, scissor, instance);
                                    }
                                }
                            }
                            None => {
                                // Fallback: legacy advance-cell bars
                                for g in cells {
                                    if g.advance > 0.0 {
                                        let (xf_abcd, xf_t) = xf_attrs(op);
                                        let instance = QuadInstance {
                                            rect: [x + g.x, *y, g.advance, *line_height],
                                            color: to_rgba_f32(*ink, a),
                                            radii: [0.0, 0.0, 0.0, 0.0],
                                            params: [0.0, 0.0, 0.0, 0.0],
                                            xf_abcd,
                                            xf_t,
                                        };
                                        push_quad(collector, scissor, instance);
                                    }
                                }
                            }
                        }
                    }
                    collector.executed_ops += 1;
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
                    let fill_col = fill.map(|c| to_rgba_f32(c, a));
                    let stroke_spec = stroke.map(|s| (to_rgba_f32(s.color, a), s.width));

                    let start_idx = collector.path_indices.len() as u32;
                    // Paths rotate on the CPU (decision 517): the
                    // tessellated vertices carry the residual, so the
                    // path pipeline needs no layout change.
                    let painted = tessellate_svg_path(
                        data,
                        (*x, *y),
                        op.xf(),
                        fill_col,
                        stroke_spec,
                        &mut collector.path_vertices,
                        &mut collector.path_indices,
                    )?;

                    if painted {
                        let end_idx = collector.path_indices.len() as u32;
                        if end_idx > start_idx {
                            collector.batches.push(DrawBatch::Path {
                                scissor,
                                indices: start_idx..end_idx,
                            });
                            collector.executed_ops += 1;
                        }
                    }
                }
            }
            DrawOp::RImg {
                x, y, w, h, image, ..
            } => {
                if alpha > 0.0 && *w > 0.0 && *h > 0.0 {
                    if !gpu_images.contains_key(image) {
                        return Err(format!(
                            "RImg {image:?}: image texture not registered — refused, never silent"
                        ));
                    }
                    let (xf_abcd, xf_t) = xf_attrs(op);
                    let instance = TexturedInstance {
                        rect: [*x, *y, *w, *h],
                        uv: [0.0, 0.0, 1.0, 1.0],
                        color: [1.0, 1.0, 1.0, alpha],
                        xf_abcd,
                        xf_t,
                    };
                    push_image(collector, scissor, *image, instance);
                    collector.executed_ops += 1;
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

    Ok(())
}

fn push_quad(
    collector: &mut BatchCollector,
    scissor: (u32, u32, u32, u32),
    instance: QuadInstance,
) {
    let idx = collector.quads.len() as u32;
    collector.quads.push(instance);

    if let Some(DrawBatch::Quad {
        scissor: s,
        instances,
    }) = collector.batches.last_mut()
    {
        if *s == scissor && instances.end == idx {
            instances.end = idx + 1;
            return;
        }
    }

    collector.batches.push(DrawBatch::Quad {
        scissor,
        instances: idx..idx + 1,
    });
}

fn push_glyph(
    collector: &mut BatchCollector,
    scissor: (u32, u32, u32, u32),
    instance: TexturedInstance,
) {
    let idx = collector.glyphs.len() as u32;
    collector.glyphs.push(instance);

    if let Some(DrawBatch::Glyph {
        scissor: s,
        instances,
    }) = collector.batches.last_mut()
    {
        if *s == scissor && instances.end == idx {
            instances.end = idx + 1;
            return;
        }
    }

    collector.batches.push(DrawBatch::Glyph {
        scissor,
        instances: idx..idx + 1,
    });
}

fn push_image(
    collector: &mut BatchCollector,
    scissor: (u32, u32, u32, u32),
    image_id: ImageId,
    instance: TexturedInstance,
) {
    let idx = collector.images.len() as u32;
    collector.images.push(instance);

    if let Some(DrawBatch::Image {
        scissor: s,
        instances,
        image_id: id,
    }) = collector.batches.last_mut()
    {
        if *s == scissor && *id == image_id && instances.end == idx {
            instances.end = idx + 1;
            return;
        }
    }

    collector.batches.push(DrawBatch::Image {
        scissor,
        instances: idx..idx + 1,
        image_id,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_multiplies_opacity_by_color_alpha() {
        // Opaque colors ride opacity alone (every pre-alpha scene).
        let solid = to_rgba_f32(Color(0xFF_6E_A0_FF), 1.0);
        assert_eq!(solid, [110.0 / 255.0, 160.0 / 255.0, 1.0, 1.0]);
        // 24-bit literals stay fully opaque (the codebase-wide shape).
        let short = to_rgba_f32(Color(0x22_66_CC), 0.5);
        assert_eq!(short[3], 0.5);
        // Translucent colors multiply through (the placeholder path).
        let dim = to_rgba_f32(Color(0x59_FF_FF_FF), 1.0);
        assert!((dim[3] - 0x59 as f32 / 255.0).abs() < 1e-6, "{dim:?}");
        assert_eq!(&dim[0..3], &[1.0, 1.0, 1.0]);
        let dimmed = to_rgba_f32(Color(0x59_FF_FF_FF), 0.5);
        assert!(
            (dimmed[3] - 0.5 * 0x59 as f32 / 255.0).abs() < 1e-6,
            "{dimmed:?}"
        );
    }
}
