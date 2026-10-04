use super::*;

use super::aria::{render_switch, SwitchProps};

// ---------------------------------------------------------------------------
// G13 catalog end-to-end: app-authored controls (oppa-controls,
// through ctx.child) through retained semantics into real DOM
// attributes — the screen-reader-visible surface per leg.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct CatalogProps {
    checked: oppa::Signal<bool>,
    on: oppa::Signal<bool>,
    vol: oppa::Signal<f32>,
}

impl Props for CatalogProps {}

fn render_catalog(ctx: &Ctx, p: &CatalogProps) -> VNode {
    use oppa_controls::{
        Button, ButtonProps, Checkbox, CheckboxProps, Slider, SliderProps, Toggle, ToggleProps,
    };
    Div("catalog").children([
        ctx.child("oppa::Button", 1, &ButtonProps::new("OK", || {}), Button),
        ctx.child(
            "oppa::Checkbox",
            2,
            &CheckboxProps {
                label: oppa::SharedString::from("T&C"),
                checked: p.checked.clone(),
                enabled: true,
                on_change: None,
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
            Checkbox,
        ),
        ctx.child(
            "oppa::Toggle",
            3,
            &ToggleProps {
                label: oppa::SharedString::from("Wi-Fi"),
                on: p.on.clone(),
                enabled: true,
                on_change: None,
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
            Toggle,
        ),
        ctx.child(
            "oppa::Slider",
            4,
            &SliderProps {
                label: oppa::SharedString::from("Volume"),
                value: p.vol.clone(),
                min: 0.0,
                max: 100.0,
                step: 10.0,
                enabled: true,
                on_change: None,
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
            Slider,
        ),
    ])
}

#[test]
fn aria_catalog_controls_end_to_end() {
    let host = ComponentHost::new();
    host.set_viewport(300.0, 200.0);
    let rt = host.runtime();
    host.mount(
        "Catalog",
        CatalogProps {
            checked: rt.signal(false),
            on: rt.signal(true),
            vol: rt.signal(50.0f32),
        },
        render_catalog,
    );
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    backend.with_sync(&host, &mut sheet).expect("sync");
    let html_of = |debug: &str| {
        let id = oppa::find_retained_by_debug(&host, debug)[0];
        backend.render_node(id)
    };
    let b = html_of("button");
    assert!(b.contains("role=\"button\""), "{b}");
    assert!(b.contains("aria-label=\"OK\""), "{b}");
    let c = html_of("checkbox");
    assert!(c.contains("role=\"checkbox\""), "{c}");
    assert!(c.contains("aria-checked=\"false\""), "{c}");
    let t = html_of("toggle");
    assert!(t.contains("role=\"switch\""), "{t}");
    assert!(t.contains("aria-checked=\"true\""), "{t}");
    let s = html_of("slider");
    assert!(s.contains("role=\"slider\""), "{s}");
    assert!(s.contains("aria-valuetext=\"50 percent\""), "{s}");
}

#[derive(Clone, Props)]
struct MaybeSwitchProps {
    show: bool,
}

fn render_maybe_switch(_ctx: &Ctx, props: &MaybeSwitchProps) -> VNode {
    if props.show {
        render_switch(
            _ctx,
            &SwitchProps {
                label: "Wi-Fi".into(),
                on: true,
            },
        )
    } else {
        Div("plate")
            .style(Style::new().size(44, 24).bg(CARD_BG))
            .build()
    }
}

#[test]
fn aria_removal_drops_the_element() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount(
        "Maybe",
        MaybeSwitchProps { show: true },
        render_maybe_switch,
    );
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let mut snapshot = SemanticsSnapshot::new();
    let d1 = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snapshot));
    assert_eq!(d1.upserted.len(), 1);
    let track = oppa::find_retained_by_debug(&host, "track")[0];
    assert!(backend.element(track).is_some());

    handle.set_props(MaybeSwitchProps { show: false });
    host.run_until_idle();
    let diff = host.last_diff().expect("diff");
    assert!(
        diff.ops
            .iter()
            .any(|op| matches!(op, oppa::DiffOp::Remove { .. })),
        "switch subtree removed: {:?}",
        diff.ops
    );
    let seen = host.diff_count() - 1;
    for d in host.diffs_from(seen) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    // The root survives as the plate (same Div tag → Update, not
    // Replace); the SWITCH PAYLOAD is what leaves.
    let plate = backend.element(track).expect("root survives as plate");
    assert!(
        !plate.attrs.iter().any(|(k, _)| k == "role"),
        "role gone: {:?}",
        plate.attrs
    );
    let d2 = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snapshot));
    assert_eq!(d2.removed, vec![track], "payload removal == diff removal");
}
