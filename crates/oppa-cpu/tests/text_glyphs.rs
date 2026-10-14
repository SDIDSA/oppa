//! CPU glyph rasterization (decision 200): runs with a registered
//! face render real ab_glyph coverage; faceless runs keep legacy
//! bars. Uses the vendored DejaVu anchor by relative include (no new
//! binaries — see `oppa-text-rustybuzz/test-fonts/LICENSE-DejaVu.txt`).
//! Non-ASCII surface is absent here by construction (ASCII corpus).

use ab_glyph::{Font, FontRef};
use oppa::{
    Color, DrawOp, FontId, FontRun, FramePlan, NodeId, PaintXf, PlacedGlyph, RendererBackend,
    SurfaceDesc, Transform,
};
use oppa_cpu::CpuBackend;

const DEJAVU: &[u8] = include_bytes!("../../oppa-text-rustybuzz/test-fonts/DejaVuSans.ttf");
const FID: FontId = FontId(7);

/// "Hi" at 16px: H + i cells, one explicit run.
fn hi_plan() -> FramePlan {
    hi_plan_ink(Color(0x00_00_00))
}

fn hi_plan_ink(ink: Color) -> FramePlan {
    let font = FontRef::try_from_slice(DEJAVU).expect("dejavu parses");
    let h = font.glyph_id('H').0 as u32;
    let i = font.glyph_id('i').0 as u32;
    FramePlan {
        ops: vec![DrawOp::Text {
            node: NodeId::new(1, 0),
            x: 10.0,
            y: 10.0,
            line_height: 20.0,
            baseline: 16.0,
            em_size: 16.0,
            glyphs: vec![
                PlacedGlyph {
                    glyph_id: h,
                    x: 0.0,
                    advance: 12.0,
                },
                PlacedGlyph {
                    glyph_id: i,
                    x: 12.0,
                    advance: 6.0,
                },
            ],
            fonts: vec![FontRun {
                glyph_range: (0, 2),
                family: "DejaVu Sans".to_string(),
                font_id: FID,
            }],
            ink,
            opacity: 1.0,
            xf: PaintXf::identity(),
        }],
        full_repaint: true,
        ..Default::default()
    }
}

fn paint_hi_ink(ink: Color) -> Vec<u8> {
    let mut be = CpuBackend::new();
    be.set_font_for(FID, DEJAVU.into(), 0);
    let s = be
        .create_surface(SurfaceDesc {
            width_px: 60,
            height_px: 40,
            background: Color(0xFF_FF_FF),
        })
        .expect("surface");
    let plan = hi_plan_ink(ink);
    be.paint(s, &plan).expect("paint");
    be.pixmap(s).expect("pixmap").data().to_vec()
}

fn paint_hi(faced: bool) -> Vec<u8> {
    let mut be = CpuBackend::new();
    if faced {
        be.set_font_for(FID, DEJAVU.into(), 0);
    }
    let s = be
        .create_surface(SurfaceDesc {
            width_px: 60,
            height_px: 40,
            background: Color(0xFF_FF_FF),
        })
        .expect("surface");
    be.paint(s, &hi_plan()).expect("paint");
    be.pixmap(s).expect("pixmap").data().to_vec()
}

fn inked_positions(px: &[u8]) -> Vec<(u32, u32)> {
    px.chunks_exact(4)
        .enumerate()
        .filter(|(_, c)| c[0] < 128)
        .map(|(i, _)| ((i % 60) as u32, (i / 60) as u32))
        .collect()
}

#[test]
fn faced_run_renders_glyph_shapes_not_bars() {
    let inked = inked_positions(&paint_hi(true));
    // Real coverage: dozens of inked pixels, far fewer than the two
    // full advance cells (12x20 + 6x20 = 360).
    assert!(inked.len() > 20, "glyph coverage present: {}", inked.len());
    assert!(
        inked.len() < 200,
        "narrower than full cells: {}",
        inked.len()
    );
    let (min_x, max_x, min_y, max_y) = inked
        .iter()
        .fold((u32::MAX, 0, u32::MAX, 0), |(a, b, c, d), &(x, y)| {
            (a.min(x), b.max(x), c.min(y), d.max(y))
        });
    // Positioned at the op origin: x in [10, 10+18), cap top above the
    // baseline row (10 + 16 = 26), feet on it.
    assert!(min_x >= 10 && max_x < 30, "x {min_x}..{max_x}");
    assert!(min_y < 26, "ascends above baseline: {min_y}");
    assert!(max_y <= 26, "feet on the baseline: {max_y}");
    // Bottom-right of the H advance cell (21, 29) is blank in the
    // glyph but inked in the bar — the crisp discriminator.
    let at = |x: u32, y: u32| paint_hi(true)[(y as usize * 60 + x as usize) * 4];
    assert!(at(21, 29) > 200, "cell corner blank under glyphs");
}

#[test]
fn faceless_run_keeps_legacy_bars() {
    // No face registered: the exact pre-decision-200 behavior —
    // advance cells fully inked, including the glyph-blank corner.
    let px = paint_hi(false);
    let at = |x: u32, y: u32| px[(y as usize * 60 + x as usize) * 4];
    assert_eq!(at(21, 29), 0, "H cell corner inked (bar)");
    assert_eq!(at(11, 11), 0, "H cell top-left inked (bar)");
    assert_eq!(at(0, 0), 255, "background untouched");
}

#[test]
fn raster_is_deterministic() {
    assert_eq!(paint_hi(true), paint_hi(true));
}

/// Decision 517: real glyph coverage rotates through the temp-layer
/// detour — the "Hi" run turns 90° about its box center (19, 20), so
/// the unrotated darkest pixel vacates its footprint (background)
/// while its rotated image inks (axis-aligned coverage in-temp, then
/// the residual blit — count preserved in band, bbox moved).
#[test]
fn rotated_run_moves_real_coverage_through_detour() {
    let plain = paint_hi(true);
    let (di, _) = plain
        .chunks_exact(4)
        .enumerate()
        .min_by_key(|(_, c)| c[0])
        .expect("inked pixels");
    let (dx, dy) = ((di % 60) as f32, (di / 60) as f32);
    // 90° image about the run-box center (19, 20).
    let (ix, iy) = (19.0 - (dy - 20.0), 20.0 + (dx - 19.0));
    let mut plan = hi_plan();
    plan.ops.iter_mut().for_each(|op| {
        op.set_xf(Transform::rotate(90).to_full_affine(19.0, 20.0));
    });
    let mut be = CpuBackend::new();
    be.set_font_for(FID, DEJAVU.into(), 0);
    let s = be
        .create_surface(SurfaceDesc {
            width_px: 60,
            height_px: 40,
            background: Color(0xFF_FF_FF),
        })
        .expect("surface");
    be.paint(s, &plan).expect("paint");
    let px = be.pixmap(s).expect("pixmap").data().to_vec();
    let at = |x: u32, y: u32| px[(y as usize * 60 + x as usize) * 4];
    assert!(
        at(ix as u32, iy as u32) < 160,
        "rotated image of the darkest pixel inks ({ix}, {iy}), got {}",
        at(ix as u32, iy as u32)
    );
    assert!(
        at(dx as u32, dy as u32) > 200,
        "old footprint vacates ({dx}, {dy}), got {}",
        at(dx as u32, dy as u32)
    );
    let inked = inked_positions(&px);
    let (a, b, c, d) = inked
        .iter()
        .fold((u32::MAX, 0, u32::MAX, 0), |(a, b, c, d), &(x, y)| {
            (a.min(x), b.max(x), c.min(y), d.max(y))
        });
    assert!(
        a >= 11 && b <= 26 && c >= 11 && d <= 27,
        "ink bbox moved with the turn, got x {a}..{b} y {c}..{d}"
    );
    let plain_n = inked_positions(&plain).len();
    assert!(
        (plain_n / 2..=plain_n * 2).contains(&inked.len()),
        "coverage count preserved in band ({} vs {plain_n})",
        inked.len()
    );
}

#[test]
fn translucent_ink_blends_by_its_alpha() {
    // Darkest pixel of the opaque render = full-coverage glyph core.
    let solid = paint_hi_ink(Color(0x00_00_00));
    let (core, _) = solid
        .chunks_exact(4)
        .enumerate()
        .min_by_key(|(_, c)| c[0])
        .expect("inked pixels");
    let (cx, cy) = ((core % 60) as u32, (core / 60) as u32);
    assert!(
        solid[(cy as usize * 60 + cx as usize) * 4] < 64,
        "sampled a solid glyph core"
    );
    // Same pixel at 50% ink alpha over white: ~127 either way.
    let half = paint_hi_ink(Color(0x80_00_00_00));
    let v = half[(cy as usize * 60 + cx as usize) * 4];
    assert!(
        (100..=160).contains(&v),
        "half-alpha ink blends, got {v} at ({cx}, {cy})"
    );
    // And 24-bit white ink is a no-op over a white surface (the
    // alpha rule leaves 24-bit colors fully opaque by construction).
    let clear = paint_hi_ink(Color(0x00_FF_FF_FF));
    let v = clear[(cy as usize * 60 + cx as usize) * 4];
    assert!(v > 200, "white-on-white untouched, got {v}");
}

#[test]
#[should_panic(expected = "font bytes rejected")]
fn invalid_face_bytes_fail_loud_at_registration() {
    let mut be = CpuBackend::new();
    be.set_font_for(FID, std::sync::Arc::<[u8]>::from([0, 1, 2, 3]), 0);
}
