//! Vector path tessellation via lyon for DrawOp::Path.

use lyon_tessellation::math::point;
use lyon_tessellation::path::Builder;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, LineCap, LineJoin, StrokeOptions,
    StrokeTessellator, StrokeVertex, VertexBuffers,
};
use oppa_cpu::path::build_path;
use tiny_skia::PathSegment;

use crate::vertex::PathVertex;

/// Convert a tiny_skia::Path into a lyon::path::Path.
pub fn to_lyon_path(skia_path: &tiny_skia::Path) -> lyon_tessellation::path::Path {
    let mut builder = Builder::new();
    let mut in_subpath = false;
    for seg in skia_path.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                if in_subpath {
                    builder.end(false);
                }
                builder.begin(point(p.x, p.y));
                in_subpath = true;
            }
            PathSegment::LineTo(p) => {
                if !in_subpath {
                    builder.begin(point(0.0, 0.0));
                    in_subpath = true;
                }
                builder.line_to(point(p.x, p.y));
            }
            PathSegment::QuadTo(p0, p1) => {
                if !in_subpath {
                    builder.begin(point(0.0, 0.0));
                    in_subpath = true;
                }
                builder.quadratic_bezier_to(point(p0.x, p0.y), point(p1.x, p1.y));
            }
            PathSegment::CubicTo(p0, p1, p2) => {
                if !in_subpath {
                    builder.begin(point(0.0, 0.0));
                    in_subpath = true;
                }
                builder.cubic_bezier_to(point(p0.x, p0.y), point(p1.x, p1.y), point(p2.x, p2.y));
            }
            PathSegment::Close => {
                if in_subpath {
                    builder.end(true);
                    in_subpath = false;
                }
            }
        }
    }
    if in_subpath {
        builder.end(false);
    }
    builder.build()
}

/// Tessellates SVG path data into vertex and index buffers.
pub fn tessellate_svg_path(
    data: &str,
    offset: (f32, f32),
    xf: oppa::PaintXf,
    fill_color: Option<[f32; 4]>,
    stroke: Option<([f32; 4], f32)>,
    out_vertices: &mut Vec<PathVertex>,
    out_indices: &mut Vec<u32>,
) -> Result<bool, String> {
    let skia_path = build_path(data)?;
    let lyon_path = to_lyon_path(&skia_path);
    let mut painted = false;

    let base_vertex_idx = out_vertices.len() as u32;
    let mut buffers: VertexBuffers<PathVertex, u32> = VertexBuffers::new();
    // Residual rotation applies on the CPU (decision 517): offset
    // first (the committed box origin, like the unrotated path),
    // then the residual — identity skips the multiply exactly.
    let place = |px: f32, py: f32| -> [f32; 2] {
        let (qx, qy) = (px + offset.0, py + offset.1);
        if xf.is_identity() {
            [qx, qy]
        } else {
            let (rx, ry) = xf.map_point(qx, qy);
            [rx, ry]
        }
    };

    if let Some(col) = fill_color {
        let mut tessellator = FillTessellator::new();
        let options = FillOptions::default();
        tessellator
            .tessellate_path(
                &lyon_path,
                &options,
                &mut BuffersBuilder::new(&mut buffers, |vertex: FillVertex| {
                    let pos = vertex.position();
                    PathVertex {
                        pos: place(pos.x, pos.y),
                        color: col,
                    }
                }),
            )
            .map_err(|e| format!("path fill tessellation error: {e:?}"))?;
        painted = true;
    }

    if let Some((col, width)) = stroke {
        if width > 0.0 {
            let mut tessellator = StrokeTessellator::new();
            let options = StrokeOptions::default()
                .with_line_width(width)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round);
            tessellator
                .tessellate_path(
                    &lyon_path,
                    &options,
                    &mut BuffersBuilder::new(&mut buffers, |vertex: StrokeVertex| {
                        let pos = vertex.position();
                        PathVertex {
                            pos: place(pos.x, pos.y),
                            color: col,
                        }
                    }),
                )
                .map_err(|e| format!("path stroke tessellation error: {e:?}"))?;
            painted = true;
        }
    }

    for idx in buffers.indices {
        out_indices.push(base_vertex_idx + idx);
    }
    out_vertices.extend(buffers.vertices);

    Ok(painted)
}
