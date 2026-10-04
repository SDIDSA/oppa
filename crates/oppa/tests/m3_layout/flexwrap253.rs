use super::*;

// ---------------------------------------------------------------------------
// Round 1.2 (decision 253): Row flex-wrap line-breaking with exact
// container bounds; NoWrap default preserved; non-Row Wrap loud.
// ---------------------------------------------------------------------------

#[test]
fn row_wrap_breaks_into_lines_with_exact_geometry() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Outer 100 wide; Row fills it (auto height) with gap 4 and Wrap.
    // Three 60-wide children: each breaks alone (60+4+60 > 100).
    // Lines h 10/12/8; extent_h = 10+12+8+2*4 = 38.
    let tree: VNode = Div("root").child(
        Div("outer").style(Style::new().size(100, 400)).child(
            Row("r")
                .style(Style::new().fill_width().gap(4).flex_wrap(FlexWrap::Wrap))
                .children([
                    Div("a").style(Style::new().size(60, 10)).build(),
                    Div("b").style(Style::new().size(60, 12)).build(),
                    Div("c").style(Style::new().size(60, 8)).build(),
                ]),
        ),
    );
    rig.layout(tree);
    let ba = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let bb = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    let bc = LayoutLedger::committed(&rig.rec, rig.only("c")).expect("c");
    let br = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("row");
    assert!(approx(ba.x, 0.0) && approx(ba.y, 0.0), "a at origin");
    assert!(
        approx(bb.x, 0.0) && approx(bb.y, 14.0),
        "b line 2, got {}/{}",
        bb.x,
        bb.y
    );
    assert!(
        approx(bc.x, 0.0) && approx(bc.y, 30.0),
        "c line 3, got {}/{}",
        bc.x,
        bc.y
    );
    assert!(approx(br.w, 100.0), "row w, got {}", br.w);
    assert!(approx(br.h, 38.0), "row h expands to 3 lines, got {}", br.h);
    assert!(
        approx(br.content_w, 60.0),
        "content max line, got {}",
        br.content_w
    );
    assert!(
        approx(br.content_h, 38.0),
        "content_h, got {}",
        br.content_h
    );
}

#[test]
fn row_wrap_multi_child_lines_center_cross_axis_per_line() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // 100-wide, gap 4, Center: [40x10, 40x16] share line 1 (84 wide),
    // 40x10 breaks to line 2. Line heights 16/10; a centers at y=3.
    let tree: VNode = Div("root").child(
        Div("outer").style(Style::new().size(100, 400)).child(
            Row("r")
                .style(
                    Style::new()
                        .fill_width()
                        .gap(4)
                        .flex_wrap(FlexWrap::Wrap)
                        .align_items(AlignItems::Center),
                )
                .children([
                    Div("a").style(Style::new().size(40, 10)).build(),
                    Div("b").style(Style::new().size(40, 16)).build(),
                    Div("c").style(Style::new().size(40, 10)).build(),
                ]),
        ),
    );
    rig.layout(tree);
    let ba = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let bb = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    let bc = LayoutLedger::committed(&rig.rec, rig.only("c")).expect("c");
    let br = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("row");
    assert!(
        approx(ba.x, 0.0) && approx(ba.y, 3.0),
        "a centered, got {}/{}",
        ba.x,
        ba.y
    );
    assert!(
        approx(bb.x, 44.0) && approx(bb.y, 0.0),
        "b line top, got {}/{}",
        bb.x,
        bb.y
    );
    assert!(
        approx(bc.x, 0.0) && approx(bc.y, 20.0),
        "c line 2, got {}/{}",
        bc.x,
        bc.y
    );
    assert!(approx(br.h, 30.0), "row h = 16+10+4, got {}", br.h);
    assert!(
        approx(br.content_w, 84.0),
        "content max line, got {}",
        br.content_w
    );
}

#[test]
fn row_wrap_fill_takes_its_line_remainder() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // 100-wide, gap 4: fixed 60x10 + fill (empty Div, h 0).
    // Fill joins line 1 (60+4 <= 100) and takes 100-60-4 = 36.
    let tree: VNode = Div("root").child(
        Div("outer").style(Style::new().size(100, 400)).child(
            Row("r")
                .style(Style::new().fill_width().gap(4).flex_wrap(FlexWrap::Wrap))
                .children([
                    Div("fixed").style(Style::new().size(60, 10)).build(),
                    Div("f").style(Style::new().fill_width()).build(),
                ]),
        ),
    );
    rig.layout(tree);
    let bf = LayoutLedger::committed(&rig.rec, rig.only("f")).expect("f");
    let br = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("row");
    assert!(approx(bf.w, 36.0), "fill share of line, got {}", bf.w);
    assert!(approx(bf.x, 64.0), "fill x after gap, got {}", bf.x);
    assert!(approx(br.h, 10.0), "single line h, got {}", br.h);
    assert!(
        approx(br.content_w, 100.0),
        "line fills width, got {}",
        br.content_w
    );
}

#[test]
fn row_nowrap_default_overflows_single_line_under_constraint() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // No flex_wrap set: two 60-wide children in a 100x50 row stay on one
    // line and overflow (the shipped single-line flex, byte-identical).
    let tree: VNode =
        Div("root").child(Row("r").style(Style::new().size(100, 50).gap(4)).children([
            Div("a").style(Style::new().size(60, 10)).build(),
            Div("b").style(Style::new().size(60, 10)).build(),
        ]));
    rig.layout(tree);
    let ba = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let bb = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    let br = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("row");
    assert!(approx(ba.y, bb.y), "single line, got {}/{}", ba.y, bb.y);
    assert!(approx(bb.x, 64.0), "b placed past overflow, got {}", bb.x);
    assert!(
        approx(br.w, 100.0) && approx(br.h, 50.0),
        "explicit box kept"
    );
    assert!(
        approx(br.content_w, 124.0),
        "overflow extent, got {}",
        br.content_w
    );
}

#[test]
#[should_panic(expected = "only Row wraps")]
fn column_wrap_refuses_loudly() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(
        Column::new()
            .style(Style::new().flex_wrap(FlexWrap::Wrap))
            .children([Div("a").style(Style::new().size(10, 10)).build()]),
    );
    rig.layout(tree);
}
