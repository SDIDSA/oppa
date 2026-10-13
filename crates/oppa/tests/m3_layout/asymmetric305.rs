use super::*;

// ---------------------------------------------------------------------------
// Round 11.1 (decision 305): asymmetric pads/margins
// ---------------------------------------------------------------------------

#[test]
fn asymmetric_pad_shifts_content_per_side() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Div pad_left 10 / pad_top 4 / pad_right 2 / pad_bottom 6 with a
    // 20-high kid: outer h = 20 + 4 + 6 = 30, kid at (10, 4); the
    // block width fills 800, so the right pad shows in content_w.
    let tree: VNode = Div("root").child(
        Div("pad")
            .style(
                Style::new()
                    .pad_left(10)
                    .pad_top(4)
                    .pad_right(2)
                    .pad_bottom(6),
            )
            .child(Div("kid").style(Style::new().h(20)).build()),
    );
    rig.layout(tree);
    let pad = LayoutLedger::committed(&rig.rec, rig.only("pad")).expect("pad");
    let kid = LayoutLedger::committed(&rig.rec, rig.only("kid")).expect("kid");
    assert!(approx(pad.h, 30.0), "20 + 4 + 6, got {}", pad.h);
    assert!(approx(kid.x, 10.0), "left pad shifts x, got {}", kid.x);
    assert!(approx(kid.y, 4.0), "top pad shifts y, got {}", kid.y);
    assert!(approx(kid.w, 788.0), "800 - 10 - 2, got {}", kid.w);
}

#[test]
fn per_side_fields_refine_symmetric_shorthands() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // pad_x 8 + pad_left 2 → left 2, right 8; margin_y 10 +
    // margin_top 3 on the kid → y 3, container grows 3 + 10.
    let tree: VNode = Div("root").child(
        Row("r").child(
            Div("box").child(
                Div("kid")
                    .style(Style::new().size(96, 32).margin_y(10).margin_top(3))
                    .build(),
            ),
        ),
    );
    rig.layout(tree);
    let bk = LayoutLedger::committed(&rig.rec, rig.only("kid")).expect("kid");
    assert!(approx(bk.y, 3.0), "per-side margin wins, got {}", bk.y);
    let bb = LayoutLedger::committed(&rig.rec, rig.only("box")).expect("box");
    assert!(approx(bb.h, 45.0), "32 + 3 + 10, got {}", bb.h);
    // Pads: pad container with shorthand + override (the kid fills —
    // no explicit width — so the right pad shows in its extent).
    let tree2: VNode = Div("root").child(
        Div("pad")
            .style(Style::new().pad_x(8).pad_left(2))
            .child(Div("kid").style(Style::new().h(10)).build()),
    );
    rig.layout(tree2);
    let kid2 = LayoutLedger::committed(&rig.rec, rig.only("kid")).expect("kid");
    assert!(approx(kid2.x, 2.0), "left override wins, got {}", kid2.x);
    assert!(approx(kid2.w, 790.0), "800 - 2 - 8, got {}", kid2.w);
}

#[test]
#[should_panic(expected = "out of range")]
fn custom_bad_weight_rejects_loudly() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(Row("r").child(VNode::from(Text {
        text: Arc::from("hi"),
        style: TextClass::Custom {
            size_px: 16,
            weight: FontWeight(0),
        },
    })));
    rig.layout(tree);
}
