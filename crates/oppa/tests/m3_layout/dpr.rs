use super::*;

// ---------------------------------------------------------------------------
// Shared DPR rounding (commit positions only)
// ---------------------------------------------------------------------------

#[test]
fn dpr_rounding_applies_to_positions_not_extents() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    rig.ledger.set_config(LayoutTextConfig {
        device_pixel_ratio: 2.0,
        ..Default::default()
    });
    // x=3.3 CSS px → 6.6 device → snaps to 6.5; w=10.33 → 20.66 kept.
    let tree: VNode = Div("root").child(Div("k").style(Style::new().size(10.33, 5).x(3.3)).build());
    rig.layout(tree);
    let b = LayoutLedger::committed(&rig.rec, rig.only("k")).expect("k");
    assert!(approx(b.x, 6.5), "snapped commit position, got {}", b.x);
    assert!(
        (b.w - 20.66).abs() < 1e-2,
        "subpixel extent kept, got {}",
        b.w
    );
}
