use super::*;

// ---------------------------------------------------------------------------
// 5. Verdict-(b) fields + external hole (one mechanism, two callers)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
pub(super) struct FieldProps {
    pub(super) value: String,
}

pub(super) fn render_field(_ctx: &Ctx, props: &FieldProps) -> VNode {
    TextField {
        text: Arc::from(props.value.as_str()),
        style: Text::title_small,
        label: Arc::from("Name"),
    }
    .into()
}

#[test]
fn text_field_renders_input_with_value_and_label() {
    let mut rig = Rig::new(
        "Field",
        FieldProps {
            value: "Ada".into(),
        },
        render_field,
        1.0,
    );
    let field = rig.scroll_target("field");
    let el = rig.backend.element(field).expect("field element").clone();
    assert_eq!(el.kind, oppa_dom::HtmlKind::Field);
    assert_eq!(el.text, "Ada", "value rides the text payload");
    assert!(el.children.is_empty(), "inputs are void");
    assert!(el
        .attrs
        .contains(&("aria-label".to_string(), "Name".to_string())));
    assert!(
        !el.attrs.iter().any(|(k, _)| k == "role"),
        "inputs are implicitly textbox: {:?}",
        el.attrs
    );
    // The value-carrying text child is absorbed (keyed, unmaterialized).
    assert_eq!(rig.backend.absorbed_count(), 1);
    let html = rig.backend.render_node(field);
    assert!(html.starts_with("<input type=\"text\""), "{html}");
    assert!(html.contains("value=\"Ada\""), "{html}");
    assert!(html.contains("aria-label=\"Name\""), "{html}");
    assert!(!html.contains("<span"), "no child spans inside the input");
    let _ = rig.commit_sync().expect("resync");
}

#[test]
fn field_value_update_resyncs_without_structure() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount(
        "Field",
        FieldProps {
            value: "Ada".into(),
        },
        render_field,
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

    handle.set_props(FieldProps {
        value: "Ada Lovelace".into(),
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
    assert!(stats.touched > 0, "the input re-derived");
    let field = oppa::find_retained_by_debug(&host, "field")[0];
    let html = backend.render_node(field);
    assert!(html.contains("value=\"Ada Lovelace\""), "{html}");
}

/// Round 8.2 (decision 298): dragging across a field paints DOM
/// highlight divs from the same session range the rasterizers turn
/// into `Rect` ops (one shared `selection_rects` rule); collapsing
/// the selection removes them with no structural churn.
#[test]
fn selection_highlight_divs_track_session_range() {
    use oppa_controls::{TextInput, TextInputProps};
    use oppa_dom::HtmlKind;
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.set_text_service(Box::new(FakeText));
    let value = host
        .runtime()
        .signal(oppa::SharedString::from("hello world"));
    host.mount("TI", TextInputProps::new("Name", value), TextInput);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    let commit_sync = |backend: &mut DomBackend, sheet: &mut StyleSheet| {
        use oppa::RendererBackend;
        for d in host.diffs_from(0) {
            backend.commit(&d).expect("commit");
        }
        backend.set_selection(host.focused_selection_paint());
        backend.with_sync(&host, sheet).expect("sync");
    };
    commit_sync(&mut backend, &mut sheet);
    let id = oppa::find_retained_by_debug(&host, "text-input")[0];
    assert_eq!(
        backend.element(id).map(|e| e.kind.clone()),
        Some(HtmlKind::Field),
        "the field Div carries the verdict-(b) input shape"
    );
    assert!(
        !backend.render_node(id).contains("class=\"sel\""),
        "collapsed selection paints no highlight"
    );
    // Drag byte 2→8 (8.75px/char FakeText, leading-half probes).
    let origin_x = host.text_origin_under(id).expect("text origin");
    let y = host.committed_box(id).expect("hit box").y + 8.0;
    let ptr = |action, x: f32| InputEvent::Pointer {
        id: Some(0),
        action,
        x,
        y,
        modifiers: oppa::Modifiers::NONE,
    };
    use oppa::{PointerAction, PointerButton};
    let primary = PointerButton::Primary;
    host.inject_input(ptr(
        PointerAction::Down { button: primary },
        origin_x + 2.0 * 8.75 + 2.0,
    ));
    host.inject_input(ptr(PointerAction::Move, origin_x + 5.0 * 8.75 + 2.0));
    host.inject_input(ptr(PointerAction::Move, origin_x + 8.0 * 8.75 + 2.0));
    host.inject_input(ptr(
        PointerAction::Up { button: primary },
        origin_x + 8.0 * 8.75 + 2.0,
    ));
    host.run_until_idle();
    assert_eq!(
        host.focused_field_session().expect("session").selection(),
        (2, 8)
    );
    for d in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.set_selection(host.focused_selection_paint());
    backend.with_sync(&host, &mut sheet).expect("sync");
    let html = backend.render_node(id);
    assert!(
        html.contains("class=\"sel\""),
        "highlight div renders: {html}"
    );
    assert!(html.contains("background:#b3d7ff"), "themed fill: {html}");
    let el = backend.element(id).expect("field element");
    assert_eq!(el.sel_rects.len(), 1, "one line, one rect");
    assert!(
        (el.sel_rects[0][2] - el.sel_rects[0][0] - 52.5).abs() < 0.01,
        "width pins cluster edges 17.5→70.0, got {:?}",
        el.sel_rects[0]
    );
}

/// Round 22.2 (decision 332): a multi-line `TextArea` selection
/// spans one highlight div per covered hard line (the same shared
/// `selection_rects` rule the rasterizers turn into `Rect` ops),
/// and the area emits `<textarea>`.
#[test]
fn textarea_multiline_selection_spans_lines() {
    use oppa_controls::{TextArea, TextAreaProps};
    let host = ComponentHost::new();
    host.set_viewport(300.0, 200.0);
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(oppa::SharedString::from("ab\ncd"));
    host.mount("TA", TextAreaProps::new("Notes", value), TextArea);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    let commit_sync = |backend: &mut DomBackend, sheet: &mut StyleSheet| {
        use oppa::RendererBackend;
        for d in host.diffs_from(0) {
            backend.commit(&d).expect("commit");
        }
        backend.set_selection(host.focused_selection_paint());
        backend.with_sync(&host, sheet).expect("sync");
    };
    commit_sync(&mut backend, &mut sheet);
    let id = oppa::find_retained_by_debug(&host, "text-area")[0];
    let html = backend.render_node(id);
    assert!(html.contains("<textarea"), "area shape: {html}");
    assert!(
        html.contains(">ab\ncd</textarea>"),
        "multiline content rides along: {html}"
    );
    // Focus (tap the area center) then select all across both lines.
    let b = host.committed_box(id).expect("hit box");
    host.inject_input(InputEvent::pointer_down(b.x + b.w / 2.0, b.y + b.h / 2.0));
    host.inject_input(InputEvent::pointer_up(b.x + b.w / 2.0, b.y + b.h / 2.0));
    host.run_until_idle();
    host.focused_field_session().expect("session").select_all();
    host.run_until_idle();
    commit_sync(&mut backend, &mut sheet);
    let el = backend.element(id).expect("area element");
    assert_eq!(
        el.sel_rects.len(),
        2,
        "one rect per line, got {:?}",
        el.sel_rects
    );
    assert!(
        el.sel_rects[1][1] > el.sel_rects[0][1],
        "rects stack in line order, got {:?}",
        el.sel_rects
    );
}

/// Round 15.1 (decision 312): the focused field derives a synced
/// `caret` bar div at the active cluster's leading edge (2 CSS px
/// wide at DPR 1, in the field's text ink); the blink half-cycle
/// removes it — synced visibility, no CSS animation, one host clock.
#[test]
fn caret_bar_div_tracks_focus_and_blink_phase() {
    use oppa::{PointerAction, PointerButton};
    use oppa_controls::{TextInput, TextInputProps};
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(200.0, 120.0);
    host.set_text_service(Box::new(FakeText));
    let value = host
        .runtime()
        .signal(oppa::SharedString::from("hello world"));
    host.mount("TI", TextInputProps::new("Name", value), TextInput);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    // Tap byte 0 → focus + collapsed caret, blink reset to visible.
    let id = oppa::find_retained_by_debug(&host, "text-input")[0];
    let origin_x = host.text_origin_under(id).expect("text origin");
    let y = host.committed_box(id).expect("hit box").y + 8.0;
    let ptr = |action, x: f32| InputEvent::Pointer {
        id: Some(0),
        action,
        x,
        y,
        modifiers: oppa::Modifiers::NONE,
    };
    let primary = PointerButton::Primary;
    host.inject_input(ptr(PointerAction::Down { button: primary }, origin_x + 2.0));
    host.inject_input(ptr(PointerAction::Up { button: primary }, origin_x + 2.0));
    host.run_until_idle();
    assert_eq!(
        host.focused_field_session().expect("session").caret(),
        0,
        "tap parks the caret at byte 0"
    );
    {
        use oppa::RendererBackend;
        for d in host.diffs_from(0) {
            backend.commit(&d).expect("commit");
        }
    }
    backend.set_caret(host.focused_caret_paint());
    backend.with_sync(&host, &mut sheet).expect("sync");
    let html = backend.render_node(id);
    assert!(
        html.contains("class=\"sel caret\""),
        "caret div renders trailing the input: {html}"
    );
    let el = backend.element(id).expect("field element");
    let r = el.caret_rect.expect("caret rect while visible");
    assert!(
        ((r[2] - r[0]) - 2.0).abs() < 0.01,
        "2 CSS px wide at DPR 1, got {r:?}"
    );
    assert!(r[3] > r[1], "positive bar height, got {r:?}");
    assert!(!el.caret_ink.is_empty(), "ink snapshots with the rect");
    assert!(
        html.contains(&format!("background:{};", el.caret_ink)),
        "bar paints in the field ink: {html}"
    );
    // Blink half-cycle: the host hides the caret, the re-sync drops
    // the div (no stale bar survives the hidden phase).
    clock.advance(0.6);
    assert!(
        host.focused_caret_paint().is_none(),
        "hidden half-cycle paints nothing"
    );
    backend.set_caret(host.focused_caret_paint());
    backend.with_sync(&host, &mut sheet).expect("sync");
    let html = backend.render_node(id);
    assert!(
        !html.contains("caret"),
        "hidden caret leaves no div: {html}"
    );
    assert!(
        backend
            .element(id)
            .expect("field element")
            .caret_rect
            .is_none(),
        "rect clears with the phase"
    );
}

/// Decision 293 (OQ-SINK-1 fix): an empty+placeholder control
/// renders `value=""` with a native `placeholder` attribute — the
/// placeholder is never the value, so the browser only ever sends
/// typed text. Setting a value swaps to the value with no
/// placeholder attribute (and no structural churn — the leaf shape
/// is stable, only payloads re-derive).
#[test]
fn placeholder_renders_attribute_not_value() {
    use oppa_controls::{TextInput, TextInputProps};
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let value = host.runtime().signal(oppa::SharedString::from(""));
    host.mount(
        "TI",
        TextInputProps::new("Name", value.clone()).placeholder("Enter name"),
        TextInput,
    );
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let id = oppa::find_retained_by_debug(&host, "text-input")[0];
    let html = backend.render_node(id);
    assert!(html.starts_with("<input type=\"text\""), "{html}");
    assert!(html.contains("value=\"\""), "empty value, {html}");
    assert!(
        html.contains("placeholder=\"Enter name\""),
        "native hint, {html}"
    );
    assert!(
        !html.contains("value=\"Enter name\""),
        "placeholder never leaks into the value, {html}"
    );
    // A value swaps the payload (leaf replaces the span, same tag —
    // Update-only) and drops the attribute.
    value.set(oppa::SharedString::from("Ada"));
    host.run_until_idle();
    let seen = host.diff_count() - 1;
    for d in host.diffs_from(seen) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let html = backend.render_node(id);
    assert!(html.contains("value=\"Ada\""), "{html}");
    assert!(
        !html.contains("placeholder="),
        "no hint with a value, {html}"
    );
}
