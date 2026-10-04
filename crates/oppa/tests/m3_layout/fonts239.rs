use super::*;

// ---------------------------------------------------------------------------
// Decision 239: arbitrary font sizes + weights (exact coordinates)
// ---------------------------------------------------------------------------

#[test]
fn custom_size_scales_geometry_exactly() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Fake advance = em * 0.625, line box = em * (0.75 + 0.25) = em.
    // title_small 16px "ab" → 20 x 16; custom 32px "ab" → 40 x 32.
    let tree: VNode = Div("root").child(Row("r").children([
        VNode::from(Text {
            text: Arc::from("ab"),
            style: Text::title_small,
        }),
        VNode::from(Text::new("ab").size(32).build()),
    ]));
    rig.layout(tree);
    let ids = find_ids(&rig.rec, "text");
    let mut wh: Vec<(f32, f32)> = ids
        .iter()
        .filter_map(|id| LayoutLedger::committed(&rig.rec, *id))
        .filter(|b| !b.lines.is_empty())
        .map(|b| (b.w, b.h))
        .collect();
    wh.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert_eq!(wh.len(), 2);
    assert!(
        approx(wh[0].0, 20.0) && approx(wh[0].1, 16.0),
        "title 2x10, h 16, got {:?}",
        wh[0]
    );
    assert!(
        approx(wh[1].0, 40.0) && approx(wh[1].1, 32.0),
        "custom32 2x20, h 32, got {:?}",
        wh[1]
    );
}

#[test]
fn text_builder_default_matches_body_secondary() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Untouched builder == BodySecondary: body 14px → 8.75px/char.
    let tree: VNode = Div("root").child(Row("r").children([
        VNode::from(Text {
            text: Arc::from("ab"),
            style: Text::body_secondary,
        }),
        VNode::from(Text::new("ab").build()),
    ]));
    rig.layout(tree);
    let ids = find_ids(&rig.rec, "text");
    let mut widths: Vec<f32> = ids
        .iter()
        .filter_map(|id| LayoutLedger::committed(&rig.rec, *id))
        .filter(|b| !b.lines.is_empty())
        .map(|b| b.w)
        .collect();
    widths.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(widths.len(), 2);
    assert!(approx(widths[0], 17.5), "body, got {}", widths[0]);
    assert!(
        approx(widths[1], 17.5),
        "builder default, got {}",
        widths[1]
    );
}

#[test]
fn bold_weight_reaches_the_shaper() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode =
        Div("root").child(Row("r").child(VNode::from(Text::new("ab").size(16).bold().build())));
    rig.layout(tree);
    assert_eq!(*rig.fake.weights.borrow(), vec![FontWeight::BOLD]);
    // Weight carries no geometry in the fake (as in real backends, where
    // only the shaper's advances change): 16px → 20 x 16 like title.
    // (Wrapper + leaf share the "text" label — the leaf holds lines.)
    let leaf = find_ids(&rig.rec, "text")
        .into_iter()
        .filter_map(|id| LayoutLedger::committed(&rig.rec, id))
        .find(|b| !b.lines.is_empty())
        .expect("leaf box");
    assert!(approx(leaf.w, 20.0), "w = {}", leaf.w);
    assert!(approx(leaf.h, 16.0), "h = {}", leaf.h);
}

#[test]
fn weight_change_remasures_not_aliases() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree = |style| {
        Div("root").child(Row("r").child(VNode::from(Text {
            text: Arc::from("alpha"),
            style,
        })))
    };
    let s1 = rig.layout(tree(Text::new("alpha").size(16).bold().build().style));
    assert_eq!(s1.nodes_shaped, 1);
    // Same bytes, same size, different weight: the key misses — exactly
    // one re-shape, never a bold-cache alias for the regular style.
    let s2 = rig.relayout(tree(Text::new("alpha").size(16).build().style));
    assert_eq!(s2.nodes_shaped, 1, "weight flip re-shapes once");
    assert_eq!(rig.shape_calls(), 2);
    assert_eq!(
        *rig.fake.weights.borrow(),
        vec![FontWeight::BOLD, FontWeight::NORMAL]
    );
}

#[test]
#[should_panic(expected = "size_px is 0")]
fn custom_zero_size_rejects_loudly() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(Row("r").child(VNode::from(Text {
        text: Arc::from("hi"),
        style: TextClass::Custom {
            size_px: 0,
            weight: FontWeight::NORMAL,
        },
    })));
    rig.layout(tree);
}
