use super::*;

// ---------------------------------------------------------------------------
// 2. StyleId → CSS rule identity (no inline-style spam)
// ---------------------------------------------------------------------------

#[test]
fn same_style_one_rule_changed_style_minimal_churn() {
    let mut rig = Rig::new("Count", CountProps { n: 2, tint: false }, render_count, 1.0);
    // Wrap + 2 cells: the cells share one rule, the wrap another.
    assert_eq!(rig.sheet.rule_count(), 2, "wrap rule + cell rule");
    let churn = rig.sheet.churn();
    let wrap = rig.scroll_target("wrap");
    let wrap_class = rig.backend.element(wrap).expect("wrap").classes.clone();
    let cell = oppa::find_retained_by_debug(&rig.host, "cell")[0];
    let cell_class = rig.backend.element(cell).expect("cell").classes.clone();
    // Re-sync without changes: zero churn, zero touches.
    let touched = rig.commit_sync().expect("sync");
    assert_eq!(touched, 0, "settled frame touches nothing");
    assert_eq!(rig.sheet.churn(), churn, "no rule churn on resync");

    // A real style change through props (both cells retint): exactly
    // one new rule, the wrap's rule untouched.
    rig.set_props(CountProps { n: 2, tint: true });
    let touched = rig.commit_sync().expect("sync");
    assert!(touched > 0, "retinted cells re-derive");
    assert_eq!(rig.sheet.churn(), churn + 1, "exactly one new rule");
    assert_eq!(rig.sheet.rule_count(), 3);
    assert_eq!(
        rig.backend.element(wrap).expect("wrap").classes,
        wrap_class,
        "unrelated nodes keep their rule"
    );
    assert_ne!(
        rig.backend.element(cell).expect("cell").classes,
        cell_class,
        "changed nodes move rules"
    );
}

#[test]
fn static_style_never_leaks_inline() {
    let mut rig = Rig::new("Card", CardProps { title: "Hi".into() }, render_card, 1.0);
    rig.host.set_text_service(Box::new(FakeText));
    rig.host.run_until_idle();
    rig.commit_sync().expect("sync");
    let card = rig.scroll_target("card");
    let el = rig.backend.element(card).expect("card").clone();
    // Static style rides the shared class...
    assert_eq!(el.classes.len(), 1);
    let decl = rig
        .sheet
        .decl_of_class(&el.classes[0])
        .expect("rule for the class");
    assert!(decl.contains("background:#444444;"), "{decl}");
    assert!(decl.contains("border-radius:6px;"), "{decl}");
    // ...never inline (geometry only).
    assert!(
        !el.inline_geom.contains("background"),
        "no inline-style spam: {}",
        el.inline_geom
    );
    let html = rig.backend.render_node(card);
    let style_attr = html
        .split("style=\"")
        .nth(1)
        .expect("inline geometry")
        .split('"')
        .next()
        .unwrap_or("");
    // The card is the root: relative container, never absolute offsets.
    assert!(style_attr.contains("position:relative"), "{style_attr}");
    assert!(style_attr.contains("width:100px"), "{style_attr}");
    assert!(style_attr.contains("height:40px"), "{style_attr}");
    assert!(!style_attr.contains("background"), "{style_attr}");
    assert!(!style_attr.contains("border-radius"), "{style_attr}");
    // The stylesheet carries the statics exactly once.
    let sheet = rig.sheet.render();
    assert_eq!(sheet.matches("background:#444444;").count(), 1, "{sheet}");
}
