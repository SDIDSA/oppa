use super::*;

use super::fields_basic::{render_field, FieldProps};

/// Round 5.1 (decision 268): multi-line fields render real
/// `<textarea>` elements (content-carrying, absorbed children,
/// value updates re-derive without structure ops).
#[derive(Clone, Props)]
struct AreaProps {
    value: String,
}

fn render_area(_ctx: &Ctx, props: &AreaProps) -> VNode {
    oppa::TextArea {
        text: Arc::from(props.value.as_str()),
        style: Text::title_small,
        label: Arc::from("Notes"),
    }
    .into()
}

#[test]
fn text_area_renders_textarea_with_content() {
    let mut rig = Rig::new(
        "Area",
        AreaProps {
            value: "line one\nline two".into(),
        },
        render_area,
        1.0,
    );
    let area = rig.scroll_target("area");
    let el = rig.backend.element(area).expect("area element").clone();
    assert_eq!(el.kind, oppa_dom::HtmlKind::Area);
    assert_eq!(el.text, "line one\nline two", "content rides the payload");
    assert!(el.children.is_empty(), "areas absorb like inputs");
    assert!(el
        .attrs
        .contains(&("aria-label".to_string(), "Notes".to_string())));
    let html = rig.backend.render_node(area);
    assert!(html.starts_with("<textarea"), "{html}");
    assert!(html.contains(">line one\nline two</textarea>"), "{html}");
    let _ = rig.commit_sync().expect("resync");
}

#[test]
fn area_value_update_resyncs_without_structure() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount(
        "Area",
        AreaProps {
            value: "one".into(),
        },
        render_area,
    );
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let mutations = backend.mutations();
    handle.set_props(AreaProps {
        value: "one\ntwo".into(),
    });
    host.run_until_idle();
    let diff = host.last_diff().expect("diff");
    assert_eq!(diff.structure_ops(), 0, "value change is Update-only");
    let seen = host.diff_count() - 1;
    for d in host.diffs_from(seen) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    let stats = backend.with_sync(&host, &mut sheet).expect("sync");
    assert_eq!(backend.mutations(), mutations, "no DOM structure ops");
    assert!(stats.touched > 0, "the area re-derived");
    let area = oppa::find_retained_by_debug(&host, "area")[0];
    let html = backend.render_node(area);
    assert!(html.contains(">one\ntwo</textarea>"), "{html}");
}

#[test]
fn node_for_pid_resolves_the_rendered_field() {
    let mut rig = Rig::new(
        "Field",
        FieldProps {
            value: "Ada".into(),
        },
        render_field,
        1.0,
    );
    let field = rig.scroll_target("field");
    let html = rig.backend.render_node(field);
    // The pid the page reports back is the rendered data-pid.
    let pid = html
        .split("data-pid=\"")
        .nth(1)
        .expect("pid attr")
        .split('"')
        .next()
        .expect("pid end");
    assert_eq!(rig.backend.node_for_pid(pid), Some(field));
    assert_eq!(rig.backend.node_for_pid("9999-9999"), None);
    let _ = rig.commit_sync().expect("resync");
}

fn render_wrapped_field(_ctx: &Ctx, props: &FieldProps) -> VNode {
    oppa::Div("wrap").child(
        TextField {
            text: Arc::from(props.value.as_str()),
            style: Text::title_small,
            label: Arc::from("Name"),
        }
        .into(),
    )
}

#[test]
fn zero_field_omits_geometry_for_browser_sizing() {
    // Decision 189: the Rig installs no TextService, so the field
    // commits 0x0 — the backend must not echo that back as 0px
    // (an invisible, unfocusable input). Position stays, dims go.
    let mut rig = Rig::new(
        "Wrap",
        FieldProps { value: "".into() },
        render_wrapped_field,
        1.0,
    );
    let field = rig.scroll_target("field");
    let html = rig.backend.render_node(field);
    assert!(html.starts_with("<input type=\"text\""), "{html}");
    assert!(
        html.contains("position:absolute;left:0px;top:0px;"),
        "{html}"
    );
    assert!(!html.contains("width:"), "{html}");
    assert!(!html.contains("height:"), "{html}");
    assert!(html.contains("value=\"\""), "{html}");
    let _ = rig.commit_sync().expect("resync");
}

#[test]
fn external_hole_renders_marker_with_box_and_children() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Hole", (), render_hole);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let hole = oppa::find_retained_by_debug(&host, "player")[0];
    let el = backend.element(hole).expect("hole element").clone();
    assert_eq!(el.kind, oppa_dom::HtmlKind::External(7));
    assert_eq!(el.children.len(), 1, "framework children pass through");
    let html = backend.render_node(hole);
    assert!(html.contains("data-external=\"custom:7\""), "{html}");
    assert!(html.contains("position:absolute"), "box preserved: {html}");
    assert!(html.contains("caption"), "child content intact: {html}");
}

fn render_hole(_ctx: &Ctx, _props: &()) -> VNode {
    let caption: VNode = Text {
        text: Arc::from("caption"),
        style: Text::body_secondary,
    }
    .into();
    Div("stage")
        .style(Style::new().size(160, 90).bg(CARD_BG))
        .child(
            Custom("player", 7)
                .style(Style::new().size(160, 90))
                .child(caption),
        )
}
