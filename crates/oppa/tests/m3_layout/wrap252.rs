use super::*;

// ---------------------------------------------------------------------------
// Round 1.1 (decision 252): constrained widths wrap text and expand the
// container height; NaN/negative widths fail loudly, never silently.
// ---------------------------------------------------------------------------

#[test]
fn wrapped_text_expands_auto_container_height_with_exact_lines() {
    use oppa::layout::{layout_text, layout_text_with_breaks};
    // Fake metrics mirror FakeText at 14px/dpr1: adv 8.75, line 14 + gap 1.75.
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Auto-height Div, explicit w=25: "abcdefgh" (8 x 8.75 = 70) wraps to
    // lines of 2 -> 4 lines; height expands to 4*14 + 3*1.75 = 61.25.
    // (Width comes from the fixed outer; the auto-height inner proves
    // the intrinsic-height expansion — `Style` has no w-only builder.)
    let tree: VNode = Div("root").child(
        Div("outer").style(Style::new().size(25, 500)).child(
            Div("t")
                .style(Style::new().fill_width())
                .child(VNode::Text(Arc::from("abcdefgh"))),
        ),
    );
    rig.layout(tree);
    let leaf = rig.only("text");
    let lb = LayoutLedger::committed(&rig.rec, leaf).expect("leaf");
    assert_eq!(lb.lines.len(), 4, "lines of 2 at w=25");
    for (i, line) in lb.lines.iter().enumerate() {
        assert!(
            approx(line.y, i as f32 * 15.75),
            "line {i} y = {}, want {}",
            line.y,
            i as f32 * 15.75
        );
        assert!(approx(line.height, 14.0), "line {i} h = {}", line.height);
        assert!(approx(line.width, 17.5), "line {i} w = {}", line.width);
    }
    assert!(approx(lb.h, 61.25), "leaf h expands, got {}", lb.h);
    let t = LayoutLedger::committed(&rig.rec, rig.only("t")).expect("container");
    assert!(approx(t.w, 25.0), "container w, got {}", t.w);
    assert!(
        approx(t.h, 61.25),
        "container h expands to wrapped text, got {}",
        t.h
    );
    assert!(
        approx(t.content_h, 61.25),
        "content_h tracks wrapped text, got {}",
        t.content_h
    );
    // The pure wrap entry points accept the same constraint story:
    // infinite = intrinsic single line; NaN/negative panic loudly.
    let run = oppa::ShapedRun {
        glyphs: vec![ShapedGlyph {
            glyph_id: 0,
            x_advance: 10.0,
            x_offset: 0.0,
            y_offset: 0.0,
        }],
        runs: vec![TextRun {
            byte_range: (0, 1),
            glyph_range: (0, 1),
            rtl: false,
            script: 0,
            font_id: FontId(0),
            font_metrics: FontMetrics {
                ascent: 12.0,
                descent: 4.0,
                line_gap: 2.0,
            },
        }],
        clusters: vec![Cluster {
            byte_range: (0, 1),
            glyph_range: (0, 1),
        }],
        total_advance: 10.0,
        text_len_bytes: 1,
    };
    let one = layout_text(&run, "a", f32::INFINITY, None, 12.0, 4.0, 2.0);
    assert_eq!(one.len(), 1, "infinite avail stays single-line");
    let one_b = layout_text_with_breaks(&run, "a", f32::INFINITY, None, 12.0, 4.0, 2.0, &[]);
    assert_eq!(one_b.len(), 1, "infinite avail stays single-line (breaks)");
}

#[test]
fn row_fill_wrapper_text_wraps_into_share_expanding_row_height() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Outer 200 wide; Row fills it (auto height); fixed 60x10 + fill Div.
    // Share = 200 - 60 = 140 -> 20-char text (8.75/char) wraps 16 + 4;
    // fill h = 2*14 + 1.75 = 29.75; row h expands to it.
    let tree: VNode = Div("root").child(
        Div("outer").style(Style::new().size(200, 500)).child(
            Row("r").style(Style::new().fill_width()).children([
                Div("fixed").style(Style::new().size(60, 10)).build(),
                Div("f1")
                    .style(Style::new().fill_width())
                    .child(VNode::Text(Arc::from("abcdefghijklmnopqrst"))),
            ]),
        ),
    );
    rig.layout(tree);
    let leaf = rig.only("text");
    let lb = LayoutLedger::committed(&rig.rec, leaf).expect("leaf");
    assert_eq!(
        lb.lines.len(),
        2,
        "fill share wraps 16+4, got {}",
        lb.lines.len()
    );
    assert!(
        approx(lb.lines[0].width, 140.0),
        "line0 w = {}",
        lb.lines[0].width
    );
    assert!(
        approx(lb.lines[1].width, 35.0),
        "line1 w = {}",
        lb.lines[1].width
    );
    assert!(approx(lb.lines[1].y, 15.75), "line1 y = {}", lb.lines[1].y);
    let f1 = LayoutLedger::committed(&rig.rec, rig.only("f1")).expect("f1");
    assert!(approx(f1.w, 140.0), "fill share w, got {}", f1.w);
    assert!(approx(f1.h, 29.75), "fill h expands, got {}", f1.h);
    let r = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("row");
    assert!(
        approx(r.h, 29.75),
        "row h expands to wrapped fill child, got {}",
        r.h
    );
}

#[test]
#[should_panic(expected = "NaN")]
fn layout_text_nan_avail_panics_loudly() {
    use oppa::layout::layout_text;
    let run = oppa::ShapedRun {
        glyphs: vec![ShapedGlyph {
            glyph_id: 0,
            x_advance: 10.0,
            x_offset: 0.0,
            y_offset: 0.0,
        }],
        runs: vec![TextRun {
            byte_range: (0, 1),
            glyph_range: (0, 1),
            rtl: false,
            script: 0,
            font_id: FontId(0),
            font_metrics: FontMetrics {
                ascent: 12.0,
                descent: 4.0,
                line_gap: 2.0,
            },
        }],
        clusters: vec![Cluster {
            byte_range: (0, 1),
            glyph_range: (0, 1),
        }],
        total_advance: 10.0,
        text_len_bytes: 1,
    };
    let _ = layout_text(&run, "a", f32::NAN, None, 12.0, 4.0, 2.0);
}

#[test]
#[should_panic(expected = "negative")]
fn layout_text_negative_avail_panics_loudly() {
    use oppa::layout::layout_text_with_breaks;
    let run = oppa::ShapedRun {
        glyphs: vec![ShapedGlyph {
            glyph_id: 0,
            x_advance: 10.0,
            x_offset: 0.0,
            y_offset: 0.0,
        }],
        runs: vec![TextRun {
            byte_range: (0, 1),
            glyph_range: (0, 1),
            rtl: false,
            script: 0,
            font_id: FontId(0),
            font_metrics: FontMetrics {
                ascent: 12.0,
                descent: 4.0,
                line_gap: 2.0,
            },
        }],
        clusters: vec![Cluster {
            byte_range: (0, 1),
            glyph_range: (0, 1),
        }],
        total_advance: 10.0,
        text_len_bytes: 1,
    };
    let _ = layout_text_with_breaks(&run, "a", -5.0, None, 12.0, 4.0, 2.0, &[]);
}

#[test]
#[should_panic(expected = "NaN")]
fn ledger_nan_viewport_panics_loudly() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    rig.rec.reconcile(
        &rig.rt,
        &mut rig.styles,
        false,
        Div("root").child(Div("a").style(Style::new().size(10, 10)).build()),
    );
    let fake2 = rig.fake.clone();
    rig.ledger
        .run(&mut rig.rec, &rig.styles, Some(&fake2), f32::NAN, 600.0);
}
