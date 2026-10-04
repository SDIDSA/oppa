#![cfg(windows)]

use super::*;

// ---------------------------------------------------------------------------
// 8. Windows-only: DirectWrite advances flow into the plan unmodified.
// ---------------------------------------------------------------------------

#[cfg(windows)]
#[test]
fn dwrite_advances_flow_unmodified_into_plan() {
    use oppa_text_dwrite::DWriteTextService;

    let svc = DWriteTextService::new().expect("dwrite service");
    let style = oppa::TextStyle {
        family: "Segoe UI".to_string(),
        font_size_px: 16.0,
        device_pixel_ratio: 1.0,
        weight: oppa::FontWeight::NORMAL,
        style: oppa::FontStyle::Normal,
        stretch: oppa::FontStretch::NORMAL,
        letter_spacing_px: 0.0,
        locale: "en-US".to_string(),
    };
    let shaped = svc.shape("Hi", &style).expect("shape Hi");
    assert_eq!(shaped.glyphs.len(), 2, "M0b: Hi is 2 glyphs");

    let host = ComponentHost::new();
    host.set_text_service(Box::new(svc));
    host.set_viewport(VW, VH);
    let _handle = host.mount(
        "card",
        CardProps {
            title: "Hi".to_string(),
        },
        render_card,
    );
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    let text_op = plan
        .ops
        .iter()
        .find_map(|o| match o {
            DrawOp::Text { glyphs, .. } => Some(glyphs),
            _ => None,
        })
        .expect("text op");
    assert_eq!(text_op.len(), shaped.glyphs.len());
    for (placed, shaped_g) in text_op.iter().zip(shaped.glyphs.iter()) {
        assert_eq!(
            placed.glyph_id, shaped_g.glyph_id,
            "glyph identity preserved"
        );
        assert_eq!(
            placed.advance, shaped_g.x_advance,
            "advance unmodified (never re-shaped)"
        );
    }
    println!(
        "dwrite Hi advances: {:?} total={}",
        shaped
            .glyphs
            .iter()
            .map(|g| g.x_advance)
            .collect::<Vec<_>>(),
        shaped.total_advance
    );

    // And it paints (block cells at shaped positions).
    let mut be = CpuBackend::new();
    let s = be
        .create_surface(oppa::SurfaceDesc {
            width_px: VW as u32,
            height_px: VH as u32,
            background: SURFACE_BG,
        })
        .expect("surface");
    for d in host.diffs_from(0) {
        be.commit(&d).expect("commit");
    }
    be.paint(s, &plan).expect("paint");
    assert_ne!(be.encode_png(s).expect("png").len(), 0);

    // Subpixel observation (M6 review starts here): the H cell ends at
    // 8+11.359375=19.359375, so pixel 19 is a partial-coverage AA fringe
    // strictly between ink and div bg — advances honored subpixel, no
    // re-rounding in the backend. Cells are solid (no outlines): the
    // documented gap Caps::text_as_paths=false names.
    let (r, g, b, a) = be.pixel_rgba(s, 19, 8).expect("fringe px");
    assert_eq!(a, 255);
    assert!(r > 17 && r < 68, "aa fringe r={r}");
    assert_eq!((g, b), (r, r), "gray-axis fringe");
}
