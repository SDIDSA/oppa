use super::*;

// ---------------------------------------------------------------------------
// Dirty-subtree discipline
// ---------------------------------------------------------------------------

#[test]
fn dirty_discipline_style_only_skips_text_remeasure() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree = |bg: u32| {
        Div("root").children([
            Div("a")
                .style(Style::new().size(100, 20).bg(oppa::Color(bg)))
                .child(VNode::Text(Arc::from("alpha"))),
            Div("b")
                .style(Style::new().size(100, 20))
                .child(VNode::Text(Arc::from("beta"))),
        ])
    };
    let s1 = rig.layout(tree(1));
    assert_eq!(s1.nodes_shaped, 2);
    // Style-only change (bg is not in the layout-affecting subset):
    // the run is skipped entirely — zero shapes, empty stats.
    let s2 = rig.relayout(tree(2));
    assert!(s2.empty, "no LAYOUT dirt → run skipped");
    assert_eq!(s2.nodes_shaped, 0);
    assert_eq!(rig.shape_calls(), 2);
    // Layout-affecting change with unchanged text: re-layout, still zero
    // shapes (measure cache keyed by bytes+style, not flags).
    let tree3 = Div("root").children([
        Div("a")
            .style(Style::new().size(120, 20).bg(oppa::Color(2)))
            .child(VNode::Text(Arc::from("alpha"))),
        Div("b")
            .style(Style::new().size(100, 20))
            .child(VNode::Text(Arc::from("beta"))),
    ]);
    let s3 = rig.relayout(tree3);
    assert!(!s3.empty);
    assert_eq!(s3.nodes_shaped, 0, "cache absorbs flag dirt");
    assert_eq!(rig.shape_calls(), 2);
    // Text change (TEXT dirt, no LAYOUT flag — decision 69): exactly the
    // dirty subtree re-measures — one shape.
    let tree4 = Div("root").children([
        Div("a")
            .style(Style::new().size(120, 20).bg(oppa::Color(2)))
            .child(VNode::Text(Arc::from("alpha!"))),
        Div("b")
            .style(Style::new().size(100, 20))
            .child(VNode::Text(Arc::from("beta"))),
    ]);
    let s4 = rig.relayout(tree4);
    assert_eq!(s4.nodes_shaped, 1, "exactly the changed leaf");
    assert_eq!(rig.shape_calls(), 3);
}
