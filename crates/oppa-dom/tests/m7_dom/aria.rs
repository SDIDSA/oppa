use super::*;

// ---------------------------------------------------------------------------
// 4. ARIA mapping incl. SemanticsDiff parity (M5 switch through the DOM)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
pub(super) struct SwitchProps {
    pub(super) label: String,
    pub(super) on: bool,
}

pub(super) fn render_switch(_ctx: &Ctx, props: &SwitchProps) -> VNode {
    Div("track")
        .style(Style::new().size(44, 24).radius(12).bg(CARD_BG))
        .semantics(
            Semantics::switch()
                .checked(props.on)
                .label(props.label.as_str()),
        )
        .on_press(|| {})
        .child(
            Div("knob")
                .style(Style::new().size(18, 18).circle().x(3.0))
                .build(),
        )
}

#[test]
fn aria_switch_parity_with_m5_payload() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount(
        "Switch",
        SwitchProps {
            label: "Wi-Fi".into(),
            on: true,
        },
        render_switch,
    );
    host.run_until_idle();
    let mut snapshot = SemanticsSnapshot::new();
    let diff = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snapshot));
    assert_eq!(diff.upserted.len(), 1, "the M5 switch payload, one upsert");
    assert!(diff.removed.is_empty());
    let entry = diff.upserted[0].clone();
    assert_eq!(entry.semantics.checked, Some(true));

    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    backend.with_sync(&host, &mut sheet).expect("sync");
    let track = oppa::find_retained_by_debug(&host, "track")[0];
    let el = backend.element(track).expect("track element").clone();
    // Same source, two readers: aria == the diff's payload, bounds ==
    // the diff's bounds.
    assert_eq!(el.attrs, aria_attrs(&entry.semantics));
    assert!(el
        .attrs
        .contains(&("role".to_string(), "switch".to_string())));
    assert!(el
        .attrs
        .contains(&("aria-checked".to_string(), "true".to_string())));
    assert!(el
        .attrs
        .contains(&("aria-label".to_string(), "Wi-Fi".to_string())));
    // The track is the root (offsets are relative-container implicit):
    // bounds parity reads width/height + the zero origin.
    assert_eq!((entry.x, entry.y), (0.0, 0.0));
    let geom = &el.inline_geom;
    assert!(geom.contains("position:relative"), "{geom}");
    assert!(geom.contains(&format!("width:{}px", entry.w)), "{geom}");
    assert!(geom.contains(&format!("height:{}px", entry.h)), "{geom}");
    let html = backend.render_node(track);
    assert!(html.contains("role=\"switch\""), "{html}");
    assert!(html.contains("aria-checked=\"true\""), "{html}");
}
