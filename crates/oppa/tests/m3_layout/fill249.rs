use super::*;

// ---------------------------------------------------------------------------
// Decision 249: fill_height + margins
// ---------------------------------------------------------------------------

#[test]
fn column_fill_height_splits_remaining_height() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Column h=300, gap 10, two fill children (intrinsic 0 — the
    // explicit height belongs on the container, never the fill
    // children, or the share would never size them):
    // remainder = 300 - 0 - 10 = 290 → 145 each.
    let tree: VNode = Div("root").child(
        Column::new()
            .style(Style::new().size(200, 300))
            .gap(10)
            .children([
                Div("f1").style(Style::new().fill_height()).build(),
                Div("f2").style(Style::new().fill_height()).build(),
            ]),
    );
    rig.layout(tree);
    let b1 = LayoutLedger::committed(&rig.rec, rig.only("f1")).expect("f1");
    let b2 = LayoutLedger::committed(&rig.rec, rig.only("f2")).expect("f2");
    assert!(approx(b1.h, 145.0), "f1 share, got {}", b1.h);
    assert!(approx(b2.h, 145.0), "f2 share, got {}", b2.h);
    assert!(approx(b1.y, 0.0), "f1 at top, got {}", b1.y);
    assert!(approx(b2.y, 145.0 + 10.0), "gap 10 after f1, got {}", b2.y);
}

#[test]
fn row_fill_height_cross_axis() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Row h=100, pad_y 10 → content height 80; the fill child
    // (intrinsic 0) grows to exactly the content height at the
    // content top (align defaults to Start).
    let tree: VNode = Div("root").child(
        Row("r")
            .style(Style::new().size(200, 100).pad_y(10))
            .children([Div("f").style(Style::new().fill_height()).build()]),
    );
    rig.layout(tree);
    let bf = LayoutLedger::committed(&rig.rec, rig.only("f")).expect("f");
    assert!(
        approx(bf.h, 80.0),
        "fill spans content height, got {}",
        bf.h
    );
    assert!(approx(bf.y, 10.0), "content top, got {}", bf.y);
    let br = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("r");
    assert!(approx(br.h, 100.0), "row keeps explicit h, got {}", br.h);
}

#[test]
fn margins_offset_child_and_expand_container() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Auto Div + one 96x32 child with margin_x 15 / margin_y 10:
    // the child sits at (15, 10) and the container grows by both
    // margins on each axis (126 x 52). The box rides a Row so it
    // sizes intrinsically — as a direct block child it would fill
    // the root width (block-lite), hiding the extent math.
    let tree: VNode = Div("root").child(
        Row("r").child(
            Div("box").child(
                Div("kid")
                    .style(Style::new().size(96, 32).margin(15, 10))
                    .build(),
            ),
        ),
    );
    rig.layout(tree);
    let bk = LayoutLedger::committed(&rig.rec, rig.only("kid")).expect("kid");
    assert!(approx(bk.x, 15.0), "margin_x offsets x, got {}", bk.x);
    assert!(approx(bk.y, 10.0), "margin_y offsets y, got {}", bk.y);
    let bb = LayoutLedger::committed(&rig.rec, rig.only("box")).expect("box");
    assert!(approx(bb.w, 126.0), "width carries margins, got {}", bb.w);
    assert!(approx(bb.h, 52.0), "height carries margins, got {}", bb.h);
}
