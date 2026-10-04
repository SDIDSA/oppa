use super::*;

// ---------------------------------------------------------------------------
// Flex rows/columns, block-lite, padding/gap, fill_width
// ---------------------------------------------------------------------------

#[test]
fn flex_row_positions_sizes_gaps_padding() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(Row("r").style(Style::new().pad_x(10).gap(4)).children([
        Div("a").style(Style::new().size(100, 20)).build(),
        Div("b").style(Style::new().size(50, 30)).build(),
    ]));
    let stats = rig.layout(tree);
    assert!(!stats.empty);
    let a = rig.only("a");
    let b = rig.only("b");
    let r = rig.only("r");
    let ba = LayoutLedger::committed(&rig.rec, a).expect("box a");
    let bb = LayoutLedger::committed(&rig.rec, b).expect("box b");
    let br = LayoutLedger::committed(&rig.rec, r).expect("box r");
    // content_x = 0 + pad 10; a spans 10..110; gap 4; b at 114.
    assert!(approx(ba.x, 10.0), "a.x = {}", ba.x);
    assert!(approx(ba.w, 100.0));
    assert!(approx(bb.x, 114.0), "b.x = {}", bb.x);
    assert!(approx(bb.w, 50.0));
    assert!(approx(br.h, 30.0), "row h = max child, got {}", br.h);
    assert!(approx(br.w, 800.0), "row fills block width, got {}", br.w);
    assert!(
        approx(br.content_w, 154.0),
        "content 100+4+50, got {}",
        br.content_w
    );
}

#[test]
fn column_fill_width_and_intrinsic() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(Column::new().gap(2).children([
        Div("fixed").style(Style::new().size(60, 10)).build(),
        Div("fill").style(Style::new().h(12).fill_width()).build(),
    ]));
    rig.layout(tree);
    let f = rig.only("fixed");
    let w = rig.only("fill");
    let bf = LayoutLedger::committed(&rig.rec, f).expect("fixed");
    let bw = LayoutLedger::committed(&rig.rec, w).expect("fill");
    assert!(approx(bf.w, 60.0), "intrinsic kept, got {}", bf.w);
    assert!(
        approx(bw.w, 800.0),
        "fill spans content width, got {}",
        bw.w
    );
    assert!(approx(bw.y, bf.y + 10.0 + 2.0), "gap 2, got {}", bw.y);
}

#[test]
fn block_lite_div_children_fill_width() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(Div("kid").style(Style::new().h(10)).build());
    rig.layout(tree);
    let k = rig.only("kid");
    let bk = LayoutLedger::committed(&rig.rec, k).expect("kid");
    assert!(approx(bk.w, 800.0), "block child full width, got {}", bk.w);
}

#[test]
fn row_fill_children_split_remainder() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Row with explicit width 200, pad 10, one fixed 60 + two fills:
    // remainder = 200 - 20 - 60 - gaps(2×4) = 112 → 56 each.
    let tree: VNode = Div("root").child(
        Row("r")
            .style(Style::new().size(200, 40).pad_x(10).gap(4))
            .children([
                Div("fixed").style(Style::new().size(60, 10)).build(),
                Div("f1").style(Style::new().h(10).fill_width()).build(),
                Div("f2").style(Style::new().h(10).fill_width()).build(),
            ]),
    );
    let stats = rig.layout(tree);
    assert_eq!(stats.layout_passes, 2, "fill redistribution is pass 2");
    let b1 = LayoutLedger::committed(&rig.rec, rig.only("f1")).expect("f1");
    let b2 = LayoutLedger::committed(&rig.rec, rig.only("f2")).expect("f2");
    assert!(approx(b1.w, 56.0), "f1 share, got {}", b1.w);
    assert!(approx(b2.w, 56.0), "f2 share, got {}", b2.w);
    assert!(approx(b2.x, b1.x + 56.0 + 4.0), "gap between fills");
}
