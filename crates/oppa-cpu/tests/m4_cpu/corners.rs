use super::*;

// ---------------------------------------------------------------------------
// Round 11.1 (decision 305): per-corner radii paint per vertex.
// ---------------------------------------------------------------------------

fn render_corners(_ctx: &Ctx, _p: &()) -> VNode {
    Div("card")
        .style(Style::new().size(100, 40).radius_tl(12).bg(CARD_BG))
        .build()
}

/// Per-corner radii ride `RRect.radii` (CSS order) and paint round
/// where set, sharp where not: the top-left cuts out, the other
/// three vertices stay square.
#[test]
fn per_corner_radii_paint_round_and_sharp_vertices() {
    let (fake, _) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    host.set_viewport(VW, VH);
    host.mount("card", (), render_corners);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    type Bounds = (f32, f32, f32, f32, Option<[f32; 4]>);
    let rects: Vec<Bounds> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::RRect {
                x, y, w, h, radii, ..
            } => Some((*x, *y, *w, *h, *radii)),
            _ => None,
        })
        .collect();
    assert_eq!(rects.len(), 1, "one rounded fill, got {rects:?}");
    assert_eq!(
        rects[0].4,
        Some([12.0, 0.0, 0.0, 0.0]),
        "CSS tl/tr/br/bl order"
    );

    let mut backend = CpuBackend::new();
    let surface = backend
        .create_surface(oppa::SurfaceDesc {
            width_px: VW as u32,
            height_px: VH as u32,
            background: SURFACE_BG,
        })
        .expect("surface");
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.paint(surface, &plan).expect("paint");
    let px = |x: u32, y: u32| backend.pixel_rgba(surface, x, y).expect("pixel");
    // Box at (0,0): TL arc center (12,12) r12 — (2,2) sits outside it.
    assert_eq!(px(0, 0), white(), "round TL cuts the corner");
    assert_eq!(px(2, 2), white(), "outside the TL arc");
    assert_eq!(px(12, 12), gray(), "inside the TL arc");
    assert_eq!(px(99, 0), gray(), "sharp TR stays square");
    assert_eq!(px(0, 39), gray(), "sharp BL stays square");
    assert_eq!(px(99, 39), gray(), "sharp BR stays square");
    assert_eq!(px(50, 20), gray(), "center fills");
}

// ---------------------------------------------------------------------------
// Decision 518: translucent rounded fills blend once (no pane-grid).
// The bands+discs pieces overlap each other, so translucent fills
// through the legacy path double-blend every overlap (the springs
// artifact — invisible at opacity 1, which is why no oracle caught
// it). Hand-built single-op plans, unit-direct.
// ---------------------------------------------------------------------------

struct PxRig {
    backend: CpuBackend,
    surface: oppa::SurfaceId,
}

fn paint_rrect(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    radii: Option<[f32; 4]>,
    opacity: f32,
) -> PxRig {
    let plan = FramePlan {
        viewport_w: VW,
        viewport_h: VH,
        ops: vec![DrawOp::RRect {
            node: NodeId::new(1, 0),
            x,
            y,
            w,
            h,
            radius,
            radii,
            color: Color(0x22_66_CC),
            opacity,
            xf: PaintXf::identity(),
        }],
        damage: vec![],
        stats: Default::default(),
        full_repaint: true,
    };
    let mut backend = CpuBackend::new();
    let surface = backend
        .create_surface(oppa::SurfaceDesc {
            width_px: VW as u32,
            height_px: VH as u32,
            background: Color(0xFF_FF_FF),
        })
        .expect("surface");
    backend.paint(surface, &plan).expect("paint");
    PxRig { backend, surface }
}

/// Uniform translucent rounded rect: center, edge-mid, and
/// top-mid read the SAME single-blend value (double-blended
/// overlaps would differ by tens), inside the analytic band.
#[test]
fn translucent_rrect_blends_uniformly() {
    let rig = paint_rrect(10.0, 10.0, 40.0, 20.0, 6.0, None, 0.5);
    let px = |x: u32, y: u32| rig.backend.pixel_rgba(rig.surface, x, y).expect("pixel");
    let (center, edge, top) = (px(30, 20), px(12, 20), px(30, 12));
    assert_eq!(edge, center, "edge band matches center (no double-blend)");
    assert_eq!(top, center, "top band matches center (no double-blend)");
    let (r, g, b, a) = center;
    assert_eq!(a, 255, "composites opaque over white");
    assert!(
        (143..=147).contains(&r) && (177..=182).contains(&g) && (228..=232).contains(&b),
        "single-blend 0.5 blue over white, got ({r}, {g}, {b})"
    );
}

/// Per-corner translucent rounded rect: the same uniformity across
/// a round corner interior, a sharp corner, and the center.
#[test]
fn translucent_corners_rrect_blends_uniformly() {
    let rig = paint_rrect(
        10.0,
        10.0,
        40.0,
        20.0,
        0.0,
        Some([10.0, 0.0, 10.0, 0.0]),
        0.5,
    );
    let px = |x: u32, y: u32| rig.backend.pixel_rgba(rig.surface, x, y).expect("pixel");
    let (center, round, sharp) = (px(30, 20), px(14, 14), px(46, 14));
    assert_eq!(round, center, "round corner matches center");
    assert_eq!(sharp, center, "sharp corner matches center");
}

/// Opaque control: the legacy path still paints the exact color
/// (the 518 branch keeps opaque fills byte-identical).
#[test]
fn opaque_rrect_still_paints_exact() {
    let rig = paint_rrect(10.0, 10.0, 40.0, 20.0, 6.0, None, 1.0);
    let px = |x: u32, y: u32| rig.backend.pixel_rgba(rig.surface, x, y).expect("pixel");
    assert_eq!(px(30, 20), (0x22, 0x66, 0xCC, 255));
    assert_eq!(px(12, 20), (0x22, 0x66, 0xCC, 255));
}
