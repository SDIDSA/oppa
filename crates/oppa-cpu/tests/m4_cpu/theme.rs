use super::*;

// ---------------------------------------------------------------------------
// Theme contract round (decision 323): the build theme owns the
// default text ink — explicit `Style::ink` wins in both modes.
// ---------------------------------------------------------------------------

fn ink_app(_ctx: &Ctx, _p: &()) -> VNode {
    Div("wrap")
        .style(Style::new().size(120, 60))
        .child(VNode::from(Text::new("hi")))
}

fn text_inks(plan: &FramePlan) -> Vec<Color> {
    plan.ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { ink, .. } => Some(*ink),
            _ => None,
        })
        .collect()
}

/// Unset builds paint exactly the pre-contract plan (Light's
/// `text_primary` IS the contract `INK`), and publishing Dark
/// re-inks every default to the dark `text_primary` — one shared
/// rule for CPU, Vello, and the plan-consuming tests.
#[test]
fn build_theme_owns_the_default_text_ink() {
    let (fake, _) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    host.set_viewport(VW, VH);
    host.mount("ink", (), ink_app);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    assert_eq!(builder.theme_mode(), ThemeMode::Light, "unset is Light");
    let light = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let inks = text_inks(&light);
    assert!(!inks.is_empty(), "the scene carries text");
    assert!(
        inks.iter().all(|i| *i == INK),
        "Light defaults are the contract ink: {inks:?}"
    );
    assert_eq!(
        INK,
        ThemeTokens::light().text_primary,
        "the no-churn identity the contract rests on"
    );
    builder.set_theme_mode(ThemeMode::Dark);
    assert_eq!(builder.theme_mode(), ThemeMode::Dark);
    let dark = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let inks = text_inks(&dark);
    assert!(
        inks.iter().all(|i| *i == ThemeTokens::dark().text_primary),
        "Dark defaults follow the theme: {inks:?}"
    );
    assert_ne!(
        ThemeTokens::dark().text_primary,
        INK,
        "the test spans two inks"
    );
}

fn inked_app(_ctx: &Ctx, _p: &()) -> VNode {
    Div("wrap")
        .style(Style::new().size(120, 60).ink(Color(0x12_34_56)))
        .child(VNode::from(Text::new("hi")))
}

/// An explicit `Style::ink` override wins in both modes (the theme
/// owns only the default — author ink is never re-derived).
#[test]
fn explicit_ink_wins_in_both_themes() {
    let (fake, _) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    host.set_viewport(VW, VH);
    host.mount("inked", (), inked_app);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        builder.set_theme_mode(mode);
        let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
        let inks = text_inks(&plan);
        assert_eq!(inks, vec![Color(0x12_34_56)], "{mode:?} keeps author ink");
    }
}
