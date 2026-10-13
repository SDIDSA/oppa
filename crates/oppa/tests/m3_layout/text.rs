use super::*;

// ---------------------------------------------------------------------------
// Inline text: single-line measurement, hints, empty text
// ---------------------------------------------------------------------------

#[test]
fn text_single_line_measures_through_service() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Bare text leaf in a Row (intrinsic): default 14px → 8.75px/char.
    let tree: VNode = Div("root").child(Row("r").child(VNode::Text(Arc::from("hello"))));
    let stats = rig.layout(tree);
    assert_eq!(stats.nodes_shaped, 1);
    assert_eq!(rig.shape_calls(), 1);
    let id = rig.only("text");
    let b = LayoutLedger::committed(&rig.rec, id).expect("text box");
    assert!(approx(b.w, 43.75), "w = {}", b.w);
    assert!(approx(b.h, 14.0), "h = ascent+descent = {}", b.h);
    assert_eq!(b.lines.len(), 1);
    assert_eq!(b.lines[0].runs.len(), 5);
}

#[test]
fn text_hint_selects_size() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // title_small 16px → 10px/char; body_secondary 14px → 8.75px/char.
    // Row wrapper: leaves size intrinsically (a Div would full-width them).
    let tree: VNode = Div("root").child(Row("r").children([
        VNode::from(Text {
            text: Arc::from("ab"),
            style: Text::title_small,
        }),
        VNode::from(Text {
            text: Arc::from("ab"),
            style: Text::body_secondary,
        }),
    ]));
    rig.layout(tree);
    let ids = find_ids(&rig.rec, "text");
    // Two wrappers + two leaves share the debug label; leaves hold lines.
    let mut widths: Vec<f32> = ids
        .iter()
        .filter_map(|id| LayoutLedger::committed(&rig.rec, *id))
        .filter(|b| !b.lines.is_empty())
        .map(|b| b.w)
        .collect();
    widths.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(widths.len(), 2);
    assert!(approx(widths[0], 17.5), "body 2×8.75, got {}", widths[0]);
    assert!(approx(widths[1], 20.0), "title 2×10, got {}", widths[1]);
}

#[test]
fn empty_text_is_zero_without_shape_call() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(VNode::Text(Arc::from("")));
    let stats = rig.layout(tree);
    assert_eq!(stats.nodes_shaped, 0);
    assert_eq!(rig.shape_calls(), 0);
    let b = LayoutLedger::committed(&rig.rec, rig.only("text")).expect("box");
    // Block leaf spans the content width; the measurement is in content.
    assert!(approx(b.content_w, 0.0) && approx(b.content_h, 0.0));
    assert!(b.lines.is_empty());
}

#[test]
fn empty_field_measures_one_space() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Decision 189: the empty field payload measures as one space
    // (title_small 16px fake → 10px advance; line box 12+4=16) so
    // the box never collapses. Static empty text above stays zero.
    // Row wrapper: leaves size intrinsically (a Div would full-width
    // them — same rule as text_single_line_measures_through_service).
    let tree: VNode = Div("root").child(
        Row("r").child(
            TextField {
                text: Arc::from(""),
                style: Text::title_small,
                label: Arc::from("Name"),
            }
            .into(),
        ),
    );
    rig.layout(tree);
    let elem = LayoutLedger::committed(&rig.rec, rig.only("field")).expect("field box");
    assert!(approx(elem.w, 10.0), "w = {}", elem.w);
    assert!(approx(elem.h, 16.0), "h = {}", elem.h);
    let leaf = LayoutLedger::committed(&rig.rec, rig.only("text")).expect("payload box");
    assert!(!leaf.lines.is_empty(), "space lines committed");
    assert!(approx(leaf.w, 10.0), "leaf w = {}", leaf.w);
}
