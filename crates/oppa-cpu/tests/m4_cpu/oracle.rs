use super::*;

// ---------------------------------------------------------------------------
// 3. Full-repaint-assert oracle: incremental output == full-repaint output.
// ---------------------------------------------------------------------------

#[test]
fn oracle_incremental_matches_full_repaint() {
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

    let desc = oppa::SurfaceDesc {
        width_px: VW as u32,
        height_px: VH as u32,
        background: SURFACE_BG,
    };
    let mut oracle = OracleSession::new(desc).expect("oracle");
    oracle.commit_all(&host.diffs_from(0)).expect("commit");

    // Static frame: incremental plan already carries every op.
    let incr = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    let full = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    assert!(full.full_repaint);
    assert_eq!(full.ops.len(), incr.ops.len());
    let diff = oracle.assert_paints(&incr, &full).expect("oracle paints");
    assert_eq!(diff, 0, "incremental == full repaint");

    // Text change with replayed history: paint P1 then P2 incrementally.
    handle.set_props(CardProps {
        title: "Hi!".to_string(),
    });
    host.run_until_idle();
    oracle.commit_all(&host.diffs_from(0)).expect("commit");
    let incr2 = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    assert_eq!(incr2.ops.len(), 1);
    let full2 = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let (s_incr, _) = oracle.surfaces();
    oracle.backend_mut().paint(s_incr, &incr2).expect("replay");
    let diff2 = oracle
        .assert_paints(&FramePlan::default(), &full2)
        .expect("oracle paints");
    assert_eq!(
        diff2, 0,
        "incremental history == full repaint after text change"
    );
}
