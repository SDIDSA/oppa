use super::*;

// ---------------------------------------------------------------------------
// 1. Static scene → PNG on disk, hand-computed pixel spot checks.
// ---------------------------------------------------------------------------

#[test]
fn static_scene_png_spot_checks() {
    let mut rig = Rig::new("Hi");
    let plan = rig.build_incremental();
    // Mount dirties everything: card RRect + one text line = 2 ops.
    assert_eq!(plan.ops.len(), 2, "ops: {plan:?}");
    assert!(matches!(plan.ops[0], DrawOp::RRect { .. }), "card bg first");
    assert!(matches!(plan.ops[1], DrawOp::Text { .. }), "text second");
    assert_eq!(plan.damage.len(), 3, "card + wrapper + leaf boxes");
    assert_eq!(plan.stats.subtrees_skipped, 0);

    rig.commit_all();
    let stats = rig.paint(&plan);
    assert!(!stats.skipped_empty);
    assert!(stats.ops_executed > 0);

    // Hand-computed spots (FakeText 16px: adv 10, line 16; div 100—40 r6).
    assert_eq!(rig.pixel(110, 50), white(), "surface bg outside card");
    assert_eq!(rig.pixel(4, 30), gray(), "div padding below text");
    assert_eq!(rig.pixel(50, 0), gray(), "div top band inside radius");
    assert_eq!(rig.pixel(13, 8), ink(), "first glyph cell center");
    assert_eq!(rig.pixel(23, 8), ink(), "second glyph cell center");
    assert_eq!(rig.pixel(0, 0), white(), "rounded corner cutout");
    assert_eq!(rig.pixel(33, 8), gray(), "past the text, still div bg");

    // PNG on disk (the §3 artifact).
    let path = std::env::temp_dir().join("m4_static.png");
    rig.backend.save_png(rig.surface, &path).expect("save png");
    let meta = std::fs::metadata(&path).expect("png on disk");
    assert!(meta.len() > 100, "non-trivial png");
    println!("m4 static png: {} ({} bytes)", path.display(), meta.len());

    // StyleId→rule sharing stays (locked #8): equal payloads, one rule.
    let (a, b) = rig.host.with_retained_mut(|rec, _| {
        let ids = oppa_cpu::builder::node_styles(rec);
        assert_eq!(ids.len(), rec.retained_ids().len());
        (ids.len(), rec.retained_ids().len())
    });
    assert_eq!(a, b);
}
