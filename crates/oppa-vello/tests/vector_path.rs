//! Vector paths on Vello (decision 291): `DrawOp::Path` encodes
//! through `kurbo::BezPath::from_svg` (native — the CPU backend owns
//! its own parser), translated by the box origin, filled `NonZero`
//! plus stroked round/round.
//!
//! Encode-only tests run headless (no GPU); the cross-backend pixel
//! test rides the oracle like every M6 row (same hardware bar,
//! stated, not new).

use oppa::{Color, DrawOp, FramePlan, NodeId, PlanStats, RendererBackend, StrokeDesc, SurfaceDesc};
use oppa_vello::{GpuOracle, VelloBackend};

fn tri_plan() -> FramePlan {
    FramePlan {
        viewport_w: 60.0,
        viewport_h: 60.0,
        ops: vec![DrawOp::Path {
            node: NodeId::new(1, 0),
            x: 0.0,
            y: 0.0,
            width: 60.0,
            height: 60.0,
            data: "M 5 5 L 55 5 L 30 55 Z".into(),
            fill: Some(Color(0xFF_00_00)),
            stroke: None,
            opacity: 1.0,
        }],
        damage: vec![],
        stats: PlanStats::default(),
        full_repaint: true,
    }
}

fn surface_60() -> SurfaceDesc {
    SurfaceDesc {
        width_px: 60,
        height_px: 60,
        background: Color(0xFF_FF_FF),
    }
}

/// Fill + stroke stage two shapes; the stats prove staged work
/// (mirrors the zero-work skip rule — a path stages, emptiness does
/// not).
#[test]
fn path_fill_and_stroke_encode_two_shapes() {
    let mut vello = VelloBackend::new();
    let surf = vello.create_surface(surface_60()).expect("surface builds");
    let plan = FramePlan {
        viewport_w: 20.0,
        viewport_h: 20.0,
        ops: vec![DrawOp::Path {
            node: NodeId::new(1, 0),
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
            data: "M 4.5 10.5 L 8.5 14.5 L 15.5 6".into(),
            fill: Some(Color(0x22_66_CC)),
            stroke: Some(StrokeDesc {
                color: Color(0xFF_FF_FF),
                width: 2.5,
            }),
            opacity: 1.0,
        }],
        damage: vec![],
        stats: PlanStats::default(),
        full_repaint: true,
    };
    let stats = vello.paint(surf, &plan).expect("encodes");
    assert!(!stats.skipped_empty);
    let last = vello.last_encode_stats(surf).expect("encode stats");
    // Background + fill + stroke (the encoder always stages the
    // surface bg first — see `encode_plan`).
    assert_eq!(
        last.shapes_encoded, 3,
        "bg + fill + stroke stage three shapes"
    );
}

/// Invalid data fails the encode loudly (no partial scene — the
/// surface keeps proving it by encoding cleanly after).
#[test]
fn invalid_path_data_fails_encode_loudly() {
    let mut vello = VelloBackend::new();
    let surf = vello.create_surface(surface_60()).expect("surface builds");
    let plan = FramePlan {
        viewport_w: 10.0,
        viewport_h: 10.0,
        ops: vec![DrawOp::Path {
            node: NodeId::new(1, 0),
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            data: "M 0 0 X 1 1".into(),
            fill: Some(Color(0xFF_00_00)),
            stroke: None,
            opacity: 1.0,
        }],
        damage: vec![],
        stats: PlanStats::default(),
        full_repaint: true,
    };
    let err = vello.paint(surf, &plan).expect_err("invalid data refuses");
    assert!(
        err.to_string().contains("invalid SVG path data"),
        "names the cause, got {err}"
    );
}

/// Zero-width strokes stage nothing (kurbo would hairline — never
/// silently); a fill-only sibling still stages its one shape.
#[test]
fn zero_width_stroke_stages_nothing() {
    let mut vello = VelloBackend::new();
    let surf = vello.create_surface(surface_60()).expect("surface builds");
    let stroked = FramePlan {
        viewport_w: 10.0,
        viewport_h: 10.0,
        ops: vec![DrawOp::Path {
            node: NodeId::new(1, 0),
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            data: "M 1 1 L 9 9".into(),
            fill: None,
            stroke: Some(StrokeDesc {
                color: Color(0xFF_00_00),
                width: 0.0,
            }),
            opacity: 1.0,
        }],
        damage: vec![],
        stats: PlanStats::default(),
        full_repaint: true,
    };
    vello.paint(surf, &stroked).expect("encodes");
    assert_eq!(
        vello.last_encode_stats(surf).expect("stats").shapes_encoded,
        1,
        "zero-width stroke stages nothing (surface bg only)"
    );
    vello.paint(surf, &tri_plan()).expect("encodes");
    assert_eq!(
        vello.last_encode_stats(surf).expect("stats").shapes_encoded,
        2,
        "fill-only triangle stages bg + its shape"
    );
}

/// The brief's cross-backend paint test: one triangle plan paints on
/// both rasterizers — deep-interior pixels read exact red and far
/// corners read surface-white on each. (Cross-backend *exact-0* is
/// not asserted: two independent AA rasterizers share interior and
/// exterior, never fringe bits — stated.)
#[test]
fn vector_path_renders_on_cpu_and_vello() {
    // Hardware-oracle row: software-emulated adapters (WARP) prove no
    // real-GPU pixels and crash under parallel load - skip loudly.
    if let Err(e) = VelloBackend::probe_hardware_adapter() {
        eprintln!("SKIP vector_path_renders_on_cpu_and_vello: {e}");
        return;
    }
    const RED: (u8, u8, u8, u8) = (255, 0, 0, 255);
    const WHITE: (u8, u8, u8, u8) = (255, 255, 255, 255);
    let mut oracle = GpuOracle::new(surface_60()).expect("oracle builds");
    let plan = tri_plan();
    oracle
        .cpu
        .paint(oracle.cpu_surface, &plan)
        .expect("cpu paints");
    oracle
        .vello
        .paint(oracle.vello_surface, &plan)
        .expect("vello paints");
    let cpu_img = oppa_vello::oracle::RgbaImage::from_cpu_pixmap(
        oracle.cpu.pixmap(oracle.cpu_surface).expect("cpu pixmap"),
    )
    .expect("opaque test surface converts");
    let vello_img = oracle
        .vello
        .render_pixels(oracle.vello_surface)
        .expect("gpu readback");
    for (name, img) in [("cpu", &cpu_img), ("vello", &vello_img)] {
        assert_eq!(
            img.pixel(30, 22),
            Some(RED),
            "{name}: deep-interior pixel is exact fill"
        );
        for (x, y) in [(2, 58), (57, 57), (2, 2), (57, 2)] {
            assert_eq!(
                img.pixel(x, y),
                Some(WHITE),
                "{name}: exterior pixel ({x}, {y}) stays surface-white"
            );
        }
    }
}
