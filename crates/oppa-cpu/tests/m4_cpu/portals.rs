use super::*;

// ---------------------------------------------------------------------------
// Round 1.4 (decision 255): overlay portals emit last (top z-layer)
// even when the portal precedes app content in the tree.
// ---------------------------------------------------------------------------

fn render_portal_first(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").children([
        Portal("p").child(
            Div("over")
                .style(Style::new().size(30, 10).bg(Color(0xCC_00_00)))
                .build(),
        ),
        Div("under")
            .style(Style::new().size(30, 10).bg(Color(0x00_00_CC)))
            .build(),
    ])
}

#[test]
fn portal_ops_emit_last_in_tree_order() {
    let host = ComponentHost::new();
    host.set_viewport(VW, VH);
    let _handle = host.mount("layers", (), render_portal_first);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    assert_eq!(plan.ops.len(), 2, "one fill per layer: {plan:?}");
    // Tree order is portal-first, but the portal fill paints last.
    assert!(
        matches!(
            plan.ops[0],
            DrawOp::Rect {
                color: Color(0x00_00_CC),
                ..
            }
        ),
        "app fill first: {:?}",
        plan.ops[0]
    );
    assert!(
        matches!(
            plan.ops[1],
            DrawOp::Rect {
                color: Color(0xCC_00_00),
                ..
            }
        ),
        "portal fill last (top z-layer): {:?}",
        plan.ops[1]
    );
    // A second full build re-emits both (nothing drained, nothing lost).
    let plan2 = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    assert_eq!(plan2.ops.len(), 2, "phase split loses nothing: {plan2:?}");
}
