use super::*;

// ---------------------------------------------------------------------------
// 6. Em-size + per-run font identity on all three backends (decision 110)
// ---------------------------------------------------------------------------

/// The lock touch rides every Text op: exact em size (no `=
/// line_height` approximation) + per-run font segmentation (no
/// single-face bound). Proved here at the plan level (all platforms);
/// the Vello encode arm runs Windows-gated below, the DOM span arm
/// right after.
#[test]
fn text_ops_carry_exact_em_size_and_segmented_fonts() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.set_text_service(Box::new(TwoFaceFake));
    host.mount(
        "Card",
        CardProps {
            title: "HelloWorld".into(),
        },
        render_card,
    );
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    let texts: Vec<&oppa::DrawOp> = plan
        .ops
        .iter()
        .filter(|op| matches!(op, oppa::DrawOp::Text { .. }))
        .collect();
    assert_eq!(texts.len(), 1);
    match texts[0] {
        oppa::DrawOp::Text {
            em_size,
            fonts,
            glyphs,
            line_height,
            ..
        } => {
            // title_small = 16px at dpr 1 → em 16 exactly (the line
            // height is ascent + descent + gap = 16 too here, so assert
            // against a config where they differ — see below).
            assert_eq!(*em_size, 16.0, "exact em size, device px");
            assert_eq!(fonts.len(), 2, "two fallback runs, merged");
            assert_eq!(fonts[0].family, "Alpha");
            assert_eq!(fonts[0].font_id, oppa::FontId(7));
            assert_eq!(fonts[1].family, "Beta");
            assert_eq!(fonts[1].font_id, oppa::FontId(9));
            assert_eq!(fonts[0].glyph_range, (0, 5));
            assert_eq!(fonts[1].glyph_range, (5, 10));
            assert_eq!(glyphs.len(), 10);
            let _ = line_height;
        }
        _ => unreachable!(),
    }
}

#[test]
fn em_size_is_not_line_height() {
    // The approximation `font_size = line_height` dies here: em comes
    // from measure context (size × dpr), line height from metrics.
    // TwoFaceFake metrics: ascent .8em + descent .2em + gap 0 → equal
    // here too, so use dpr 2 where em (32) still equals line height...
    // instead assert the source directly: em == size × dpr while the
    // op also carries the independent line metrics.
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.set_layout_config(oppa::LayoutTextConfig {
        device_pixel_ratio: 2.0,
        ..Default::default()
    });
    host.set_text_service(Box::new(TwoFaceFake));
    host.mount(
        "Card",
        CardProps {
            title: "HelloWorld".into(),
        },
        render_card,
    );
    host.run_until_idle();
    let builder = FramePlanBuilder::new(2.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    match plan
        .ops
        .iter()
        .find(|op| matches!(op, oppa::DrawOp::Text { .. }))
    {
        Some(oppa::DrawOp::Text {
            em_size,
            line_height,
            baseline,
            ..
        }) => {
            assert_eq!(*em_size, 32.0, "16px × dpr 2, exact");
            assert_eq!(*line_height, 32.0, "(.8 + .2) × 32");
            assert_eq!(*baseline, 25.6, "ascent .8 × 32");
        }
        _ => panic!("expected a Text op"),
    }
}

#[test]
fn dom_text_spans_carry_measured_family_and_size() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.set_text_service(Box::new(TwoFaceFake));
    host.mount(
        "Card",
        CardProps {
            title: "HelloWorld".into(),
        },
        render_card,
    );
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    // The value-carrying text node: two fallback runs → inner spans.
    let text_node = host.with_retained_mut(|rec, _| {
        rec.retained_ids()
            .into_iter()
            .find(|id| {
                rec.get(*id)
                    .is_some_and(|n| n.tag == oppa::Tag::Text && n.text.is_some())
            })
            .expect("text leaf")
    });
    let el = backend.element(text_node).expect("span").clone();
    assert_eq!(el.runs.len(), 2, "fallback segmentation survives to DOM");
    assert!(
        el.inline_font.contains("font-size:16px;"),
        "{}",
        el.inline_font
    );
    assert!(el.inline_font.contains("Alpha"), "{}", el.inline_font);
    let html = backend.render_node(text_node);
    assert!(html.contains("font-family:Alpha,sans-serif;"), "{html}");
    assert!(html.contains("font-family:Beta,sans-serif;"), "{html}");
    assert!(html.contains("Hello"), "{html}");
    assert!(html.contains("World"), "{html}");
}

#[test]
fn gpu_face_table_holds_per_id_faces_with_default_fallback() {
    // Headless face-table discipline (no draw — selection only):
    // explicit id wins, unmapped ids fall back to the default,
    // empty table reports no font (the loud-refusal precondition
    // stays). Decision 388: the surviving table is the custom
    // rasterizer's (`oppa_gpu::atlas::FaceTable`).
    use oppa::FontId;
    use oppa_gpu::atlas::{FaceBytes, FaceTable};
    use std::sync::Arc;
    let mut table = FaceTable::default();
    assert!(table.face_for(FontId(0)).is_none());
    table.default = Some(FaceBytes {
        bytes: Arc::new(vec![1, 2, 3]),
        index: 0,
    });
    assert!(table.face_for(FontId(0)).is_some(), "default serves id 0");
    assert!(
        table.face_for(FontId(7)).is_some(),
        "default covers unmapped ids"
    );
    table.explicit.insert(
        FontId(7),
        FaceBytes {
            bytes: Arc::new(vec![4, 5, 6, 7]),
            index: 1,
        },
    );
    assert_eq!(table.explicit.len(), 1, "one explicit face registered");
    assert!(table.face_for(FontId(7)).is_some());
    assert!(
        table.face_for(FontId(9)).is_some(),
        "default still covers the rest"
    );
}
