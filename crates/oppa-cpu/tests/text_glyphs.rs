//! CPU glyph rasterization (decision 200): runs with a registered
//! face render real ab_glyph coverage; faceless runs keep legacy
//! bars. Uses the vendored DejaVu anchor by relative include (no new
//! binaries — see `oppa-text-rustybuzz/test-fonts/LICENSE-DejaVu.txt`).
//! Non-ASCII surface is absent here by construction (ASCII corpus).

use ab_glyph::{Font, FontRef};
use oppa::{
    Color, DrawOp, FontId, FontRun, FramePlan, NodeId, PlacedGlyph, RendererBackend, SurfaceDesc,
};
use oppa_cpu::CpuBackend;

const DEJAVU: &[u8] = include_bytes!("../../oppa-text-rustybuzz/test-fonts/DejaVuSans.ttf");
const FID: FontId = FontId(7);

/// "Hi" at 16px: H + i cells, one explicit run.
fn hi_plan() -> FramePlan {
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
            ink: Color(0x00_00_00),
            opacity: 1.0,
        }],
        full_repaint: true,
        ..Default::default()
    }
}

fn paint_hi(faced: bool) -> Vec<u8> {
    let mut be = CpuBackend::new();
    if faced {
        be.set_font_for(FID, DEJAVU.to_vec(), 0);
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

#[test]
#[should_panic(expected = "font bytes rejected")]
fn invalid_face_bytes_fail_loud_at_registration() {
    let mut be = CpuBackend::new();
    be.set_font_for(FID, vec![0, 1, 2, 3], 0);
}
