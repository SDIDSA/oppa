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
