use super::*;

// ---------------------------------------------------------------------------
// 2. FramePlan minimality: empty on no-change; exactly the subtree on text change.
// ---------------------------------------------------------------------------

#[test]
fn frameplan_minimality_static_zero_text_subtree() {
    let mut rig = Rig::new("Hi");
    let p1 = rig.build_incremental();
    rig.commit_all();
    rig.paint(&p1);
    let before = rig.png_bytes();

    // Second commit with no changes → empty plan (static ≈ 0 CPU).
    let p2 = rig.build_incremental();
    assert!(p2.is_empty(), "no-change plan must be empty: {p2:?}");
    assert_eq!(p2.stats.subtrees_skipped, 3, "whole tree skipped");
    assert_eq!(p2.stats.nodes_visited, 1, "root probed, children skipped");
    let st = rig.paint(&p2);
    assert!(st.skipped_empty, "empty plan touches nothing");
    assert_eq!(rig.png_bytes(), before, "surface byte-identical");

    // Text-only change rebuilds exactly its subtree ops (props-driven;
    // covered precisely in text_only_change_rebuilds_exactly_its_subtree).
    let p3 = rig.build_incremental();
    assert!(p3.is_empty(), "still empty with no props change");
}

// Text-change minimality needs the MountHandle: separate focused test.

#[test]
fn text_only_change_rebuilds_exactly_its_subtree() {
    let (fake, _) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    host.set_viewport(VW, VH);
    let handle = host.mount(
        "card",
        CardProps {
            title: "Hi".to_string(),
        },
        render_card,
    );
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let p1 = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    assert_eq!(p1.ops.len(), 2);

    handle.set_props(CardProps {
        title: "Hi!".to_string(),
    });
    host.run_until_idle();
    let p2 = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    // Exactly the text leaf's op (rebuilt), nothing else (skipped).
    assert_eq!(p2.ops.len(), 1, "one subtree op: {p2:?}");
    assert!(matches!(p2.ops[0], DrawOp::Text { .. }));
    // F1 closed (M6 decision 104): the text wrapper's content extent also
    // changed with LAYOUT-only dirt, so the engine stamps PAINT on it —
    // damage covers wrapper + leaf (2), while ops stay 1 (the wrapper
    // emits no fill of its own).
    assert_eq!(p2.damage.len(), 2);
    assert_eq!(
        p2.stats.subtrees_skipped, 0,
        "open chain, no skipped subtree"
    );
    assert_eq!(p2.stats.nodes_visited, 3);
    // Damage payoff numbers for the report: rebuilt 1 op vs full 2.
    assert_eq!(p1.ops.len(), 2, "full static plan is 2 ops");

    // Third build with no further change → empty again.
    let p3 = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    assert!(p3.is_empty());
}
