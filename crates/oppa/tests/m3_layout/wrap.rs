use super::*;

// ---------------------------------------------------------------------------
// Wrap re-flows over cached advances (the measured round-trip count)
// ---------------------------------------------------------------------------

#[test]
fn wrap_reflows_without_reshape() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Title 16px → 10px/char; explicit w=25 → lines of 2 → 4 lines.
    let tree = || {
        Div("root").child(
            Div("t")
                .style(Style::new().size(25, 100))
                .child(VNode::Text(Arc::from("abcdefgh"))),
        )
    };
    let s1 = rig.layout(tree());
    assert_eq!(s1.nodes_shaped, 1, "one shape for the text");
    // Lines live on the text leaf (containers carry none).
    let leaf = rig.only("text");
    let l1 = LayoutLedger::committed(&rig.rec, leaf).expect("leaf");
    assert_eq!(l1.lines.len(), 4, "lines of 2 at w=25");
    // Re-run with w=30 (layout-affecting style change, same text): re-wrap
    // to lines of 3 with zero new shapes.
    let tree2 = Div("root").child(
        Div("t")
            .style(Style::new().size(30, 100))
            .child(VNode::Text(Arc::from("abcdefgh"))),
    );
    let s2 = rig.relayout(tree2);
    assert_eq!(rig.shape_calls(), 1, "re-wrap shapes nothing");
    assert_eq!(s2.nodes_shaped, 0);
    let l2 = LayoutLedger::committed(&rig.rec, leaf).expect("leaf2");
    assert_eq!(
        l2.lines.len(),
        3,
        "lines of 3 at w=30, got {}",
        l2.lines.len()
    );
}

// ---------------------------------------------------------------------------
// Ellipsis (optional-v1, behind the config flag)
// ---------------------------------------------------------------------------

#[test]
fn ellipsis_truncates_single_line_when_enabled() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    rig.ledger.set_config(LayoutTextConfig {
        ellipsis: true,
        ..Default::default()
    });
    let tree: VNode = Div("root").child(
        Div("t")
            .style(Style::new().size(25, 100))
            .child(VNode::Text(Arc::from("abcdefgh"))),
    );
    rig.layout(tree);
    // Default 14px → 8.75px/char; budget 25-8.75=16.25 → 1 cluster + marker.
    let b = LayoutLedger::committed(&rig.rec, rig.only("text")).expect("leaf");
    assert_eq!(b.lines.len(), 1, "ellipsis forces single-line");
    assert!(
        b.lines[0].clusters.iter().any(|c| c.ellipsis),
        "marker present"
    );
    // One text shape + one amortized ellipsis-glyph shape.
    assert_eq!(rig.shape_calls(), 2, "shapes = {}", rig.shape_calls());
}
