use super::*;
use oppa::{
    find_retained_by_debug, Column, ComponentHost, Ctx, Div, Props, SharedString, Signal, Style,
    Text, VNode,
};
use std::rc::Rc;

fn radio_props(label: &str, selected: bool, fired: &Signal<bool>) -> RadioProps {
    let set = fired.clone();
    RadioProps {
        label: SharedString::from(label),
        selected,
        enabled: true,
        on_select: action(move || set.set(true)),
    }
}

#[test]
fn radio_renders_selected_dot_and_semantics() {
    for (selected, want_dot) in [(true, true), (false, false)] {
        let host = ComponentHost::new();
        let fired = host.runtime().signal(false);
        host.mount("R", radio_props("Pro", selected, &fired), Radio);
        host.run_until_idle();
        let id = node_by_debug(&host, "radio");
        let sem = host.retained_semantics(id).expect("semantics");
        assert_eq!(sem.role, oppa::Role::RadioButton);
        assert_eq!(sem.selected, Some(selected));
        assert_eq!(sem.label.as_deref(), Some("Pro"));
        assert!(!sem.disabled);
        let ib = host
            .committed_box(node_by_debug(&host, "radio-indicator"))
            .expect("indicator box");
        assert!(
            approx(ib.w, 18.0) && approx(ib.h, 18.0),
            "indicator 18x18, got {}x{}",
            ib.w,
            ib.h
        );
        let dots = find_retained_by_debug(&host, "radio-dot");
        assert_eq!(!dots.is_empty(), want_dot, "dot iff selected");
        if want_dot {
            let db = host.committed_box(dots[0]).expect("dot box");
            assert!(
                approx(db.w, 8.0) && approx(db.h, 8.0),
                "dot 8x8, got {}x{}",
                db.w,
                db.h
            );
            // (18 - 8) / 2 = 5: exact centering via out-of-flow offsets.
            assert!(
                approx(db.x, ib.x + 5.0) && approx(db.y, ib.y + 5.0),
                "dot centered, got ({}, {}) in ({}, {})",
                db.x,
                db.y,
                ib.x,
                ib.y
            );
        }
    }
}

#[test]
fn radio_press_fires_select() {
    let host = ComponentHost::new();
    let fired = host.runtime().signal(false);
    host.mount("R", radio_props("Pro", false, &fired), Radio);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "radio"));
    assert!(fired.get(), "press selects");
}

#[test]
fn radio_disabled_is_handlerless() {
    let host = ComponentHost::new();
    let fired = host.runtime().signal(false);
    let set = fired.clone();
    let props = RadioProps {
        label: SharedString::from("Pro"),
        selected: false,
        enabled: false,
        on_select: action(move || set.set(true)),
    };
    host.mount("RDis", props, Radio);
    host.run_until_idle();
    let id = node_by_debug(&host, "radio");
    let sem = host.retained_semantics(id).expect("semantics");
    assert!(sem.disabled, "disabled flag on the role");
    assert!(
        host.retained_handlers(id).is_empty(),
        "disabled carries no handler — refusal is structural"
    );
    assert!(
        !host.tab_order().contains(&id),
        "disabled leaves the tab order (decision 96)"
    );
    press_node(&host, id);
    assert!(!fired.get(), "press on disabled never selects");
}

#[test]
fn radio_group_selects_exclusively() {
    let host = ComponentHost::new();
    let selected = host.runtime().signal(SharedString::from("Free"));
    let props = RadioGroupProps {
        options: vec![
            RadioOption {
                value: SharedString::from("Free"),
                label: SharedString::from("Free"),
            },
            RadioOption {
                value: SharedString::from("Pro"),
                label: SharedString::from("Pro"),
            },
        ],
        selected: selected.clone(),
        enabled: true,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
    };
    host.mount("G", props, RadioGroup::<SharedString>);
    host.run_until_idle();
    let radios = || find_retained_by_debug(&host, "radio");
    assert_eq!(radios().len(), 2);
    let sem = |id| host.retained_semantics(id).expect("semantics");
    assert_eq!(sem(radios()[0]).selected, Some(true));
    assert_eq!(sem(radios()[1]).selected, Some(false));
    press_node(&host, radios()[1]);
    assert_eq!(selected.get(), SharedString::from("Pro"));
    assert_eq!(sem(radios()[0]).selected, Some(false));
    assert_eq!(sem(radios()[1]).selected, Some(true));
}

fn tabs_props(active: Signal<String>) -> TabsProps<String> {
    // Same panel debug for both tabs (position-matched same-tag
    // children recycle in place by design — M8 — so debug labels
    // would go stale; the contents differ in measured width
    // instead, which proves which view is mounted).
    let mk = |value: &str, body: &'static str| TabItem {
        value: value.to_string(),
        label: SharedString::from(value),
        content: Rc::new(move |_| {
            Div("panel").child(VNode::from(Text {
                text: SharedString::from(body),
                style: Text::body_secondary,
            }))
        }),
    };
    TabsProps {
        tabs: vec![mk("a", "Alpha"), mk("b", "Beta-2-X")],
        active,
        enabled: true,
    }
}

fn panel_content_w(host: &ComponentHost) -> f32 {
    let panel = node_by_debug(host, "tab-panel");
    host.committed_box(panel).expect("panel box").content_w
}

#[test]
fn tabs_renders_tab_list_and_active_panel() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let active = host.runtime().signal("a".to_string());
    host.mount("T", tabs_props(active), Tabs::<String>);
    host.run_until_idle();
    let bar = node_by_debug(&host, "tab-bar");
    let sem = host.retained_semantics(bar).expect("semantics");
    assert_eq!(sem.role, oppa::Role::TabList);
    let items = find_retained_by_debug(&host, "tab-item");
    assert_eq!(items.len(), 2);
    let sem = |id| host.retained_semantics(id).expect("semantics");
    assert_eq!(sem(items[0]).role, oppa::Role::Tab);
    assert_eq!(sem(items[0]).selected, Some(true));
    assert_eq!(sem(items[0]).label.as_deref(), Some("a"));
    assert_eq!(sem(items[1]).selected, Some(false));
    // "Alpha": 5 chars x 8.75 — the active view is mounted.
    assert!(
        approx(panel_content_w(&host), 43.75),
        "panel shows Alpha, w={}",
        panel_content_w(&host)
    );
}

#[test]
fn tabs_click_switches_active_panel() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let active = host.runtime().signal("a".to_string());
    host.mount("T", tabs_props(active.clone()), Tabs::<String>);
    host.run_until_idle();
    let items = || find_retained_by_debug(&host, "tab-item");
    press_node(&host, items()[1]);
    assert_eq!(active.get(), "b".to_string());
    // "Beta-2-X": 8 chars x 8.75 — the view swapped in place.
    assert!(
        approx(panel_content_w(&host), 70.0),
        "panel shows Beta-2-X, w={}",
        panel_content_w(&host)
    );
    let sem = |id| host.retained_semantics(id).expect("semantics");
    assert_eq!(sem(items()[0]).selected, Some(false));
    assert_eq!(sem(items()[1]).selected, Some(true));
}

fn select_props(selected: Signal<String>, open: Signal<bool>) -> SelectProps<String> {
    SelectProps::new(
        vec![
            SelectItem {
                value: "a".to_string(),
                label: SharedString::from("Alpha"),
            },
            SelectItem {
                value: "b".to_string(),
                label: SharedString::from("Beta"),
            },
            SelectItem {
                value: "c".to_string(),
                label: SharedString::from("Gamma"),
            },
        ],
        selected,
        open,
    )
}

#[test]
fn select_renders_combobox_with_current_label() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let selected = host.runtime().signal("b".to_string());
    let open = host.runtime().signal(false);
    host.mount("S", select_props(selected, open), Select::<String>);
    host.run_until_idle();
    let box_id = node_by_debug(&host, "select-box");
    let sem = host.retained_semantics(box_id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::ComboBox);
    assert_eq!(sem.label.as_deref(), Some("Beta"));
    assert!(!sem.disabled);
    // Closed: no list, no options.
    assert!(find_retained_by_debug(&host, "select-list").is_empty());
    assert!(find_retained_by_debug(&host, "select-option").is_empty());
}

/// Decision 291: the chevron is one vector `Path` leaf (12×8),
/// not a "▾" text glyph — the box's only text leaf is the label
/// ("Beta" = 4 × 8.75 = 35.0), so no symbol-font coverage decides
/// the control's shape.
#[test]
fn select_chevron_is_a_vector_not_a_glyph() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let selected = host.runtime().signal("b".to_string());
    let open = host.runtime().signal(false);
    host.mount("S", select_props(selected, open), Select::<String>);
    host.run_until_idle();
    let chevrons = find_retained_by_debug(&host, "select-chevron");
    assert_eq!(chevrons.len(), 1, "one vector chevron, got {chevrons:?}");
    let cb = host.committed_box(chevrons[0]).expect("chevron laid out");
    assert!(
        approx(cb.w, 12.0) && approx(cb.h, 8.0),
        "chevron 12x8, got {}x{}",
        cb.w,
        cb.h
    );
    // Only the label measures text (the chevron measures nothing).
    let mut ws: Vec<f32> = find_retained_by_debug(&host, "text")
        .into_iter()
        .filter_map(|id| host.committed_box(id))
        .filter(|b| !b.lines.is_empty())
        .map(|b| b.content_w)
        .collect();
    ws.sort_by(|a, b| a.partial_cmp(b).expect("finite widths"));
    assert_eq!(ws.len(), 1, "label only, got {ws:?}");
    assert!(approx(ws[0], 35.0), "Beta widths, got {ws:?}");
}

#[test]
fn select_open_pick_closes_and_sets_value() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let selected = host.runtime().signal("b".to_string());
    let open = host.runtime().signal(false);
    host.mount(
        "S",
        select_props(selected.clone(), open.clone()),
        Select::<String>,
    );
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "select-box"));
    assert!(open.get(), "box press opens the list");
    let options = || find_retained_by_debug(&host, "select-option");
    assert_eq!(options().len(), 3);
    // Options reuse list_item + selected (existing payloads).
    let sem = |id| host.retained_semantics(id).expect("semantics");
    assert_eq!(sem(options()[0]).role, oppa::Role::ListItem);
    assert_eq!(sem(options()[0]).label.as_deref(), Some("Alpha"));
    assert_eq!(sem(options()[0]).selected, Some(false));
    assert_eq!(sem(options()[1]).selected, Some(true));
    press_node(&host, options()[2]);
    assert_eq!(selected.get(), "c".to_string());
    assert!(!open.get(), "picking closes the list");
    assert!(options().is_empty(), "closed list unmounts");
    let box_sem = host
        .retained_semantics(node_by_debug(&host, "select-box"))
        .expect("semantics");
    assert_eq!(box_sem.label.as_deref(), Some("Gamma"));
}

#[test]
fn select_disabled_is_handlerless_and_keeps_label() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let selected = host.runtime().signal("a".to_string());
    let open = host.runtime().signal(false);
    let mut props = select_props(selected.clone(), open.clone());
    props.enabled = false;
    host.mount("S", props, Select::<String>);
    host.run_until_idle();
    let box_id = node_by_debug(&host, "select-box");
    let sem = host.retained_semantics(box_id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::ComboBox);
    assert!(sem.disabled);
    assert_eq!(sem.label.as_deref(), Some("Alpha"));
    press_node(&host, box_id);
    assert!(!open.get(), "disabled box never opens");
    assert!(
        !host.tab_order().contains(&box_id),
        "disabled leaves the tab order (decision 96)"
    );
    // The list still follows the signal truthfully when opened
    // externally — but its options are handlerless too.
    open.set(true);
    host.run_until_idle();
    let options = find_retained_by_debug(&host, "select-option");
    assert_eq!(options.len(), 3);
    press_node(&host, options[1]);
    assert_eq!(
        selected.get(),
        "a".to_string(),
        "disabled pick never selects"
    );
    assert!(open.get(), "disabled pick never closes");
}

#[derive(Clone)]
struct SelectFormProps {
    selected: Signal<String>,
    open: Signal<bool>,
}
impl Props for SelectFormProps {}

fn select_form_app(ctx: &Ctx, props: &SelectFormProps) -> VNode {
    Column::new().children([
        Div("form-top").style(Style::new().size(100, 10)).build(),
        ctx.child(
            "oppa::Select",
            1,
            &select_props(props.selected.clone(), props.open.clone()),
            Select::<String>,
        ),
        Div("form-trailer")
            .style(Style::new().size(100, 10))
            .build(),
    ])
}

/// Round 7.21 (decision 296): opening the dropdown neither
/// grows the `Select` box past 32px nor shifts the trailing
/// sibling — the list rides an anchored portal under the box
/// (32px + 4px gap) at list width.
#[test]
fn select_open_neither_grows_nor_shifts_siblings() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let selected = host.runtime().signal("b".to_string());
    let open = host.runtime().signal(false);
    host.mount(
        "F",
        SelectFormProps {
            selected: selected.clone(),
            open: open.clone(),
        },
        select_form_app,
    );
    host.run_until_idle();
    let select_h = || {
        host.committed_box(node_by_debug(&host, "select"))
            .expect("select box")
            .h
    };
    let trailer_y = || {
        host.committed_box(node_by_debug(&host, "form-trailer"))
            .expect("trailer box")
            .y
    };
    assert!(approx(select_h(), 32.0), "closed box is 32px");
    let y_closed = trailer_y();
    open.set(true);
    host.run_until_idle();
    assert!(
        !find_retained_by_debug(&host, "select-list").is_empty(),
        "list mounts when open"
    );
    assert!(
        approx(select_h(), 32.0),
        "open keeps the 32px box, got {}",
        select_h()
    );
    assert!(
        approx(trailer_y(), y_closed),
        "trailer never shifts, {} vs {}",
        trailer_y(),
        y_closed
    );
    let popup = host
        .committed_box(node_by_debug(&host, "select-popup"))
        .expect("popup box");
    let sbox = host
        .committed_box(node_by_debug(&host, "select-box"))
        .expect("select box");
    assert!(
        approx(popup.x, sbox.x) && approx(popup.y, sbox.y + 36.0),
        "popup anchors under the box, got ({}, {}) vs box ({}, {})",
        popup.x,
        popup.y,
        sbox.x,
        sbox.y
    );
    assert!(
        approx(popup.w, 160.0),
        "popup keeps the list width, got {}",
        popup.w
    );
}

fn progress_width(host: &ComponentHost, value: f32) -> (f32, f32) {
    host.mount("P", ProgressBarProps::new(value), ProgressBar);
    host.run_until_idle();
    let track = host
        .committed_box(node_by_debug(host, "progressbar-track"))
        .expect("track box");
    let fill = host
        .committed_box(node_by_debug(host, "progressbar-fill"))
        .expect("fill box");
    (track.w, fill.w)
}

#[test]
fn progressbar_fill_scales_with_clamped_value() {
    // (value, expected fill width) on the default 160 track.
    for (value, want) in [
        (0.0, 0.0),
        (0.25, 40.0),
        (0.68, 108.8),
        (1.0, 160.0),
        // Boundaries clamp gracefully (quiet value normalization,
        // the slider-clamp class — never a refusal).
        (-0.5, 0.0),
        (1.5, 160.0),
    ] {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let (track_w, fill_w) = progress_width(&host, value);
        assert!(approx(track_w, 160.0), "track full width, got {track_w}");
        assert!(
            approx(fill_w, want),
            "value {value}: fill {fill_w}, want {want}"
        );
    }
}

#[test]
fn progressbar_role_and_percent_text() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let mut props = ProgressBarProps::new(0.68);
    props.label = Some(SharedString::from("Storage Quota"));
    host.mount("P", props, ProgressBar);
    host.run_until_idle();
    let sem = host
        .retained_semantics(node_by_debug(&host, "progressbar-track"))
        .expect("semantics");
    assert_eq!(sem.role, oppa::Role::ProgressBar);
    assert_eq!(sem.value_text.as_deref(), Some("68 percent"));
    assert_eq!(sem.label.as_deref(), Some("Storage Quota"));
}

#[test]
#[should_panic(expected = "ProgressBar value is NaN")]
fn progressbar_nan_refuses_loudly() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    host.mount("P", ProgressBarProps::new(f32::NAN), ProgressBar);
    host.run_until_idle();
}

#[test]
fn badge_mounts_with_label() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    host.mount(
        "B",
        BadgeProps::new("PRO").variant(BadgeVariant::Success),
        Badge,
    );
    host.run_until_idle();
    let id = node_by_debug(&host, "badge");
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::Generic);
    assert_eq!(sem.label.as_deref(), Some("PRO"));
    let b = host.committed_box(id).expect("badge box");
    assert!(approx(b.h, 24.0), "fixed pill height, got {}", b.h);
    assert!(b.w > 0.0, "chip sizes to its text, w={}", b.w);
}
