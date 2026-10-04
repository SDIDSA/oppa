use super::*;

// ---------------------------------------------------------------------------
// Decision 237: vertical padding + flex alignment (exact coordinates)
// ---------------------------------------------------------------------------

#[test]
fn pad_y_div_auto_height_includes_padding() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Div pad_y 6, kid h=20 fills width: outer h = 20 + 12 = 32.
    let tree: VNode = Div("root").child(
        Div("pad")
            .style(Style::new().pad_y(6))
            .child(Div("kid").style(Style::new().h(20)).build()),
    );
    rig.layout(tree);
    let pad = LayoutLedger::committed(&rig.rec, rig.only("pad")).expect("pad");
    let kid = LayoutLedger::committed(&rig.rec, rig.only("kid")).expect("kid");
    assert!(approx(pad.x, 0.0), "pad.x = {}", pad.x);
    assert!(approx(pad.y, 0.0), "pad.y = {}", pad.y);
    assert!(approx(pad.w, 800.0), "pad fills block width, got {}", pad.w);
    assert!(approx(pad.h, 32.0), "20 + 2*6, got {}", pad.h);
    assert!(approx(pad.content_h, 20.0), "content = {}", pad.content_h);
    assert!(approx(kid.x, 0.0), "kid.x = {}", kid.x);
    assert!(approx(kid.y, 6.0), "kid.y = pad top, got {}", kid.y);
    assert!(approx(kid.w, 800.0), "kid fills, got {}", kid.w);
    assert!(approx(kid.h, 20.0), "kid.h = {}", kid.h);
}

#[test]
fn pad_y_row_auto_height_offsets_children() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Row auto height (fills block width 800), pad_y 4, gap 2:
    // max child 16 → h = 16 + 8 = 24; children at y = 4.
    let tree: VNode = Div("root").child(Row("r").style(Style::new().pad_y(4).gap(2)).children([
        Div("a").style(Style::new().size(60, 10)).build(),
        Div("b").style(Style::new().size(40, 16)).build(),
    ]));
    rig.layout(tree);
    let r = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("r");
    let a = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let b = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    assert!(approx(r.w, 800.0), "row fills, got {}", r.w);
    assert!(approx(r.h, 24.0), "16 + 2*4, got {}", r.h);
    assert!(approx(r.content_h, 16.0), "content = {}", r.content_h);
    assert!(approx(a.x, 0.0), "a.x = {}", a.x);
    assert!(approx(a.y, 4.0), "a.y = {}", a.y);
    assert!(approx(a.w, 60.0) && approx(a.h, 10.0));
    assert!(approx(b.x, 62.0), "60 + gap 2, got {}", b.x);
    assert!(approx(b.y, 4.0), "b.y = {}", b.y);
    assert!(approx(b.w, 40.0) && approx(b.h, 16.0));
}

#[test]
fn pad_y_column_auto_height_stacks_from_content() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Column fills block width, pad_y 5, gap 2: 10 + 2 + 8 = 20 content,
    // h = 20 + 10 = 30. First at y=5, second at 5+10+2=17.
    let tree: VNode =
        Div("root").child(Column::new().style(Style::new().pad_y(5).gap(2)).children([
            Div("a").style(Style::new().size(60, 10)).build(),
            Div("b").style(Style::new().size(40, 8)).build(),
        ]));
    rig.layout(tree);
    // Column::new() uses debug "column"; distinct kids carry the asserts.
    let a = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let b = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    assert!(approx(a.x, 0.0), "a.x = {}", a.x);
    assert!(approx(a.y, 5.0), "a.y = {}", a.y);
    assert!(approx(a.w, 60.0) && approx(a.h, 10.0));
    assert!(approx(b.x, 0.0), "b.x = {}", b.x);
    assert!(approx(b.y, 17.0), "b.y = {}", b.y);
    assert!(approx(b.w, 40.0) && approx(b.h, 8.0));
    let col_id = find_ids(&rig.rec, "column").pop().expect("column");
    let col = LayoutLedger::committed(&rig.rec, col_id).expect("col");
    assert!(approx(col.w, 800.0), "col fills, got {}", col.w);
    assert!(approx(col.h, 30.0), "20 + 2*5, got {}", col.h);
    assert!(approx(col.content_h, 20.0), "content = {}", col.content_h);
}

#[test]
fn row_center_and_space_between_exact() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Row 300x50, pad 10/5, gap base 4, SpaceBetween + Center.
    // content 280x40, extent 40*3+4*2=128, leftover 152 → extra 76/gap.
    // xs: 10, 130, 250. Center ys: h10 → 20, h20 → 15.
    let tree: VNode = Div("root").child(
        Row("r")
            .style(
                Style::new()
                    .size(300, 50)
                    .pad_x(10)
                    .pad_y(5)
                    .gap(4)
                    .align_items(AlignItems::Center)
                    .justify_content(JustifyContent::SpaceBetween),
            )
            .children([
                Div("a").style(Style::new().size(40, 10)).build(),
                Div("b").style(Style::new().size(40, 20)).build(),
                Div("c").style(Style::new().size(40, 10)).build(),
            ]),
    );
    rig.layout(tree);
    let r = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("r");
    let a = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let b = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    let c = LayoutLedger::committed(&rig.rec, rig.only("c")).expect("c");
    assert!(approx(r.w, 300.0) && approx(r.h, 50.0));
    assert!(approx(r.x, 0.0) && approx(r.y, 0.0));
    assert!(approx(a.x, 10.0), "a.x = {}", a.x);
    assert!(approx(a.y, 20.0), "5 + (40-10)/2, got {}", a.y);
    assert!(approx(a.w, 40.0) && approx(a.h, 10.0));
    assert!(approx(b.x, 130.0), "10+40+80, got {}", b.x);
    assert!(approx(b.y, 15.0), "5 + (40-20)/2, got {}", b.y);
    assert!(approx(c.x, 250.0), "130+40+80, got {}", c.x);
    assert!(approx(c.y, 20.0), "c.y = {}", c.y);
}

#[test]
fn column_center_both_axes_exact() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Column 200x120, pad 10/5, gap 4, Center/Center.
    // content 180x110, extent_h 34, leftover 76 → start +38.
    // ys: 43, 67. xs: w60 → 70, w40 → 80.
    let tree: VNode = Div("root").child(
        Column::new()
            .style(
                Style::new()
                    .size(200, 120)
                    .pad_x(10)
                    .pad_y(5)
                    .gap(4)
                    .align_items(AlignItems::Center)
                    .justify_content(JustifyContent::Center),
            )
            .children([
                Div("a").style(Style::new().size(60, 20)).build(),
                Div("b").style(Style::new().size(40, 10)).build(),
            ]),
    );
    rig.layout(tree);
    let a = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let b = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    assert!(approx(a.x, 70.0), "10 + (180-60)/2, got {}", a.x);
    assert!(approx(a.y, 43.0), "5 + 38, got {}", a.y);
    assert!(approx(a.w, 60.0) && approx(a.h, 20.0));
    assert!(approx(b.x, 80.0), "10 + (180-40)/2, got {}", b.x);
    assert!(approx(b.y, 67.0), "43+20+4, got {}", b.y);
    assert!(approx(b.w, 40.0) && approx(b.h, 10.0));
}

#[test]
fn row_stretch_grows_unconstrained_keeps_fixed() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Row 200x60 Stretch: fixed 50x20 stays; w-only 30 grows 0 → 60.
    let stretch_style = Style {
        w: Some(Px::of(30.0)),
        ..Default::default()
    };
    let tree: VNode = Div("root").child(
        Row("r")
            .style(Style::new().size(200, 60).align_items(AlignItems::Stretch))
            .children([
                Div("fixed").style(Style::new().size(50, 20)).build(),
                Div("grow").style(stretch_style).build(),
            ]),
    );
    rig.layout(tree);
    let r = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("r");
    let f = LayoutLedger::committed(&rig.rec, rig.only("fixed")).expect("fixed");
    let g = LayoutLedger::committed(&rig.rec, rig.only("grow")).expect("grow");
    assert!(approx(r.w, 200.0) && approx(r.h, 60.0));
    assert!(approx(f.x, 0.0) && approx(f.y, 0.0));
    assert!(approx(f.w, 50.0) && approx(f.h, 20.0), "fixed keeps h");
    assert!(approx(g.x, 50.0), "g.x = {}", g.x);
    assert!(approx(g.y, 0.0), "g.y = {}", g.y);
    assert!(approx(g.w, 30.0), "w keeps, got {}", g.w);
    assert!(approx(g.h, 60.0), "h stretches to content 60, got {}", g.h);
}

#[test]
fn column_stretch_fills_unconstrained_width() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Column 120 wide Stretch: fixed 50x20 stays; h-only 25 fills to 120.
    let tree: VNode = Div("root").child(
        Column::new()
            .style(Style::new().size(120, 100).align_items(AlignItems::Stretch))
            .children([
                Div("fixed").style(Style::new().size(50, 20)).build(),
                Div("grow").style(Style::new().h(25)).build(),
            ]),
    );
    rig.layout(tree);
    let f = LayoutLedger::committed(&rig.rec, rig.only("fixed")).expect("fixed");
    let g = LayoutLedger::committed(&rig.rec, rig.only("grow")).expect("grow");
    assert!(approx(f.x, 0.0) && approx(f.y, 0.0));
    assert!(approx(f.w, 50.0) && approx(f.h, 20.0), "fixed keeps w");
    assert!(approx(g.x, 0.0), "g.x = {}", g.x);
    assert!(approx(g.y, 20.0), "g.y = {}", g.y);
    assert!(
        approx(g.w, 120.0),
        "w stretches to content 120, got {}",
        g.w
    );
    assert!(approx(g.h, 25.0), "h keeps, got {}", g.h);
}

#[test]
#[should_panic(expected = "non-finite")]
fn layout_nan_pad_rejects_loudly() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(
        Div("bad")
            .style(Style::new().pad_y(f32::NAN))
            .child(Div("kid").style(Style::new().size(10, 10)).build()),
    );
    rig.layout(tree);
}
