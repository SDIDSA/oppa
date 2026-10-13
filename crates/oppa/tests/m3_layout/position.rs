use super::*;

// ---------------------------------------------------------------------------
// Absolute positioning (.x / .absolute_y in scope, both §4 examples)
// ---------------------------------------------------------------------------

#[test]
fn absolute_x_offsets_within_parent_flow_kept() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // §4.1 knob shape: track 44×24, knob 18×18 at x=23.
    let tree: VNode = Div("root").child(
        Div("track")
            .style(Style::new().size(44, 24))
            .child(Div("knob").style(Style::new().size(18, 18).x(23)).build()),
    );
    rig.layout(tree);
    let track = LayoutLedger::committed(&rig.rec, rig.only("track")).expect("track");
    let knob = LayoutLedger::committed(&rig.rec, rig.only("knob")).expect("knob");
    assert!(
        approx(knob.x, track.x + 23.0),
        "knob.x = track.x + 23, got {}",
        knob.x
    );
    assert!(approx(knob.y, track.y), "y stays in flow, got {}", knob.y);
}

#[test]
fn absolute_y_pins_and_leaves_auto_height() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root")
        .child(Div("list").child(Div("slot").style(Style::new().h(24).absolute_y(48)).build()));
    rig.layout(tree);
    let list = LayoutLedger::committed(&rig.rec, rig.only("list")).expect("list");
    let slot = LayoutLedger::committed(&rig.rec, rig.only("slot")).expect("slot");
    assert!(approx(slot.y, list.y + 48.0), "slot pinned, got {}", slot.y);
    assert!(
        approx(list.h, 0.0),
        "absolute child grows nothing, got {}",
        list.h
    );
}

// ---------------------------------------------------------------------------
// ScrollArea + content_size (fixed-height virtualization geometry)
// ---------------------------------------------------------------------------

#[test]
fn scrollarea_content_size_and_slot_geometry() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(
        ScrollArea("list")
            .style(Style::new().size(300, 200).content_size(1000))
            .children((0..3).map(|i| {
                Row("slot")
                    .key(i)
                    .style(Style::new().h(40).absolute_y(i as f32 * 40.0))
                    .build()
            })),
    );
    rig.layout(tree);
    let area = LayoutLedger::committed(&rig.rec, rig.only("list")).expect("area");
    assert!(approx(area.w, 300.0));
    assert!(approx(area.h, 200.0));
    assert!(
        approx(area.content_h, 1000.0),
        "spacer extent, got {}",
        area.content_h
    );
    let slots = find_ids(&rig.rec, "slot");
    assert_eq!(slots.len(), 3);
    let mut ys: Vec<f32> = slots
        .iter()
        .map(|id| LayoutLedger::committed(&rig.rec, *id).expect("slot").y)
        .collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // Slot geometry is exactly the absolute_y offsets (§4.2 slot math).
    assert!(approx(ys[0], area.y + 0.0));
    assert!(approx(ys[1], area.y + 40.0));
    assert!(approx(ys[2], area.y + 80.0));
}
