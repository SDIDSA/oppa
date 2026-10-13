use super::*;

// ---------------------------------------------------------------------------
// BiDi visual ordering on the Latin+Arabic+digits corpus shape
// ---------------------------------------------------------------------------

#[test]
fn bidi_visual_order_collapses_source_order_divergence() {
    let (fake, calls) = FakeText::bidi();
    let mut rig = Rig::new(fake, calls);
    // Corpus string (escapes per decision 45): "abc " + U+0645 U+0631 +
    // " 123". Fake: 10px/char at 16px em... default leaf is 14px →
    // 8.75px/char; force title size via hint for round numbers.
    let text = "abc \u{645}\u{631} 123";
    assert_eq!(text.len(), 12);
    let tree: VNode = Div("root").child(VNode::from(Text {
        text: Arc::from(text),
        style: Text::title_small,
    }));
    rig.layout(tree);
    let ids = find_ids(&rig.rec, "text");
    let line = ids
        .iter()
        .filter_map(|id| LayoutLedger::committed(&rig.rec, *id))
        .find(|b| !b.lines.is_empty())
        .expect("leaf box")
        .lines
        .into_iter()
        .next()
        .expect("one line");
    // 10 chars × 10px: width preserved by the reorder.
    assert!(approx(line.width, 100.0), "width = {}", line.width);
    // Visual order: LTR head, EN digit island (LTR unit), mirrored RTL
    // tail (space + Arabic pair). The fake mirrors the backend's
    // single-tail-run resolution, so the island path is exercised.
    let bytes: Vec<(usize, usize)> = line.clusters.iter().map(|c| c.byte_range).collect();
    assert_eq!(
        bytes,
        vec![
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 4),
            (9, 10),
            (10, 11),
            (11, 12),
            (8, 9),
            (6, 8),
            (4, 6)
        ],
        "island + mirrored tail"
    );
    // Forward-affinity carets: Arabic start (byte 4) sits at the mirrored
    // tail's right edge (100), not the source-order 40; the digit island
    // starts LTR at 40; the trailing caret follows the last logical char
    // (island '3' → 70), not the line width.
    assert!(
        approx(line.caret_x(4), 100.0),
        "visual caret = {}",
        line.caret_x(4)
    );
    assert!(approx(line.caret_x(8), 80.0));
    assert!(approx(line.caret_x(9), 40.0));
    assert!(
        approx(line.caret_x(12), 70.0),
        "trailing = {}",
        line.caret_x(12)
    );
}
