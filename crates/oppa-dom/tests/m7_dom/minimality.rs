use super::*;

// ---------------------------------------------------------------------------
// 1. TreeDiff → DOM mutation minimality
// ---------------------------------------------------------------------------

#[test]
fn mount_produces_div_span_tree_with_stable_classes() {
    let mut rig = Rig::new("Card", CardProps { title: "Hi".into() }, render_card, 1.0);
    let card = rig.scroll_target("card");
    let el = rig.backend.element(card).expect("card element");
    assert_eq!(el.kind, oppa_dom::HtmlKind::Block);
    assert_eq!(el.children.len(), 1, "card wraps one text node");
    let html = render_page("t", &rig.backend, &rig.sheet);
    assert!(html.contains("<div"), "{html}");
    assert!(html.contains("<span"), "{html}");
    assert!(html.contains("Hi"), "{html}");
    assert!(html.contains("data-pid"), "parity hooks on every element");
    let _ = rig.commit_sync().expect("resync");
}

#[test]
fn text_update_touches_one_element_no_structure() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Card", CardProps { title: "Hi".into() }, render_card);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    let mut seen = 0usize;
    let commit_sync = |backend: &mut DomBackend, sheet: &mut StyleSheet, seen: &mut usize| {
        use oppa::RendererBackend;
        for diff in host.diffs_from(*seen) {
            backend.commit(&diff).expect("commit");
        }
        *seen = host.diff_count();
        backend.with_sync(&host, sheet).expect("sync").touched
    };
    let first = commit_sync(&mut backend, &mut sheet, &mut seen);
    assert!(first > 0, "mount touches");
    let elements = backend.element_count();
    let mutations = backend.mutations();

    handle.set_props(CardProps { title: "Yo".into() });
    host.run_until_idle();
    let diff = host.last_diff().expect("diff");
    assert_eq!(diff.structure_ops(), 0, "text change is Update-only");
    let touched = commit_sync(&mut backend, &mut sheet, &mut seen);
    assert_eq!(touched, 1, "exactly the text span re-derives");
    assert_eq!(backend.element_count(), elements, "no element churn");
    assert_eq!(backend.mutations(), mutations, "no DOM structure ops");
    let html = render_page("t", &backend, &sheet);
    assert!(html.contains("Yo"), "{html}");
    assert!(!html.contains(">Hi<"), "stale text gone: {html}");
}

#[test]
fn reorder_moves_dom_children_without_rebuild() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Ord", OrderProps { reversed: false }, render_ordered);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for diff in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&diff).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let wrap = oppa::find_retained_by_debug(&host, "wrap")[0];
    let before: Vec<NodeId> = backend.element(wrap).expect("wrap").children.clone();
    assert_eq!(before.len(), 3);
    let mutations = backend.mutations();

    handle.set_props(OrderProps { reversed: true });
    host.run_until_idle();
    let diff = host.last_diff().expect("diff");
    // [1,2,3] → [3,2,1]: the middle slot never moves (keyed Moves
    // only, no Add/Remove).
    assert_eq!(diff.structure_ops(), 2, "two keyed Moves: {:?}", diff.ops);
    for d in host.diffs_from(host.diff_count() - 1) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let after: Vec<NodeId> = backend.element(wrap).expect("wrap").children.clone();
    assert_eq!(after.len(), 3);
    assert_eq!(after, before.into_iter().rev().collect::<Vec<_>>());
    assert_eq!(backend.element_count(), 4, "wrap + 3 cells, none rebuilt");
    assert_eq!(backend.mutations(), mutations + 2, "exactly the Moves");
}

#[test]
fn remove_drops_subtree() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Count", CountProps { n: 3, tint: false }, render_count);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for diff in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&diff).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    assert_eq!(backend.element_count(), 4);

    handle.set_props(CountProps { n: 1, tint: false });
    host.run_until_idle();
    for d in host.diffs_from(host.diff_count() - 1) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    assert_eq!(backend.element_count(), 2, "wrap + survivor");
    let wrap = oppa::find_retained_by_debug(&host, "wrap")[0];
    assert_eq!(backend.element(wrap).expect("wrap").children.len(), 1);
}

#[test]
fn scroll_tick_produces_zero_structure_ops_end_to_end() {
    let mut rig = Rig::new("Slots", SlotProps { n: 5 }, render_slots, 1.0);
    let list = rig.scroll_target("list");
    let offset = rig
        .host
        .instance_scroll(rig.handle_inst)
        .expect("instance scroll handle");
    rig.host.bind_scroll(list, offset.clone());
    let mutations = rig.backend.mutations();

    // The M8 payoff trace starts here: counted from TreeDiffs, not
    // assumed from slot-key stability.
    rig.host.inject_input(InputEvent::Scroll {
        target: list,
        dx: 0.0,
        dy: 20.0,
    });
    rig.host.run_until_idle();
    let diff = rig.host.last_diff().expect("scroll-tick diff");
    assert_eq!(
        diff.structure_ops(),
        0,
        "scroll tick is Update-only: {:?}",
        diff.ops
    );
    assert_eq!(offset.get(), 20.0, "INPUT-fed offset applied");
    let touched = rig.commit_sync().expect("sync");
    assert_eq!(
        rig.backend.mutations(),
        mutations,
        "zero DOM structure ops on the scroll tick"
    );
    assert!(touched > 0, "row values did re-derive ({touched})");
}
