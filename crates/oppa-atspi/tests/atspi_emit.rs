//! M10 AT-SPI emitter proof: the locked toggle and list-item
//! semantics (#3, M2's `Semantics::switch()`/`list_item()`) flow from
//! the retained tree through `compute_semantics_diff` into the
//! `AtspiTree`, where they emit the wire vocabulary and stay
//! queryable — not just "data exists in the retained tree."
//!
//! Boundary, stated: this is the emitter-layer proof. Live-bus
//! validation (an AT client reading us over D-Bus) needs Linux and
//! is open — see the crate docs.

#![allow(non_snake_case)]

use oppa::{
    compute_semantics_diff, find_retained_by_debug, ComponentHost, Ctx, Semantics,
    SemanticsSnapshot, Style, VNode,
};
use oppa_atspi::{AtspiEvent, AtspiTree};

#[derive(Clone)]
struct SwitchProps {
    label: String,
    initial: bool,
}

impl oppa::Props for SwitchProps {}

fn Switch(ctx: &Ctx, props: &SwitchProps) -> VNode {
    let is_on = ctx.signal(props.initial);
    let s = is_on.clone();
    oppa::Div("sw")
        .style(Style::new().size(44, 24))
        .semantics(Semantics::switch().checked(is_on.get()).label(&props.label))
        .on_press(move || s.set(!s.get()))
        .build()
}

#[derive(Clone)]
struct RowProps {
    name: String,
    selected: bool,
}

impl oppa::Props for RowProps {}

fn Row(ctx: &Ctx, props: &RowProps) -> VNode {
    let _ = ctx.signal(0u32);
    oppa::Div("row")
        .style(Style::new().size(200, 24))
        .semantics(
            Semantics::list_item()
                .selected(props.selected)
                .label(&props.name),
        )
        .on_press(|| {})
        .build()
}

#[derive(Clone)]
struct ListProps {
    rows: Vec<(String, bool)>,
}

impl oppa::Props for ListProps {}

fn List(ctx: &Ctx, props: &ListProps) -> VNode {
    let _ = ctx.signal(0u32);
    oppa::Div("list").children(props.rows.iter().enumerate().map(|(i, (name, sel))| {
        let rp = RowProps {
            name: name.clone(),
            selected: *sel,
        };
        oppa::Div("slot")
            .key(i as u64)
            .child(ctx.child("Row", i as u64, &rp, Row))
    }))
}

fn parent_map(host: &ComponentHost) -> impl Fn(oppa::NodeId) -> Option<oppa::NodeId> + '_ {
    move |id| host.with_retained_mut(|rec, _| rec.get(id).and_then(|n| n.parent))
}

#[test]
fn toggle_emits_and_stays_queryable() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 100.0);
    let handle = host.mount(
        "Switch",
        SwitchProps {
            label: "Wi-Fi".to_string(),
            initial: false,
        },
        Switch,
    );
    host.run_until_idle();

    let mut tree = AtspiTree::new();
    let mut snap = SemanticsSnapshot::new();
    let diff = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
    assert!(!diff.is_empty(), "mount must upsert the switch");
    tree.apply(&diff, &parent_map(&host));

    let sw = find_retained_by_debug(&host, "sw")[0];
    let node = tree.get(sw).expect("switch is queryable post-emit");
    assert_eq!(node.role, "toggle button");
    assert_eq!(node.name.as_deref(), Some("Wi-Fi"));
    assert!(node.states.contains(&"checkable"));
    assert!(!node.states.contains(&"checked"));
    assert!(node.states.contains(&"enabled"));
    let events = tree.take_events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            AtspiEvent::ChildrenAdded { child, .. } if *child == sw
        )),
        "mount announces the child: {events:?}"
    );

    // Flip through the framework press path (not a signal write):
    // the emitter must report state-changed, never re-add.
    host.inject_input(oppa::InputEvent::pointer_down(10.0, 12.0));
    host.inject_input(oppa::InputEvent::pointer_up(10.0, 12.0));
    host.run_until_idle();
    let diff2 = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
    tree.apply(&diff2, &parent_map(&host));
    let events = tree.take_events();
    assert_eq!(
        events,
        vec![AtspiEvent::StateChangedChecked { node: sw, on: true }],
        "flip emits exactly one state event: {events:?}"
    );
    assert!(tree.get(sw).expect("live").states.contains(&"checked"));
    let _ = handle;
}

#[test]
fn list_items_emit_with_selection_and_removal() {
    let host = ComponentHost::new();
    host.set_viewport(300.0, 200.0);
    let mk = |rows: Vec<(String, bool)>| ListProps { rows };
    let handle = host.mount(
        "List",
        mk(vec![("Ada".to_string(), false), ("Bob".to_string(), true)]),
        List,
    );
    host.run_until_idle();

    let mut tree = AtspiTree::new();
    let mut snap = SemanticsSnapshot::new();
    let diff = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
    tree.apply(&diff, &parent_map(&host));
    assert_eq!(tree.len(), 2, "both rows mirrored, nothing else");
    tree.take_events();

    let rows = find_retained_by_debug(&host, "row");
    assert_eq!(rows.len(), 2);
    let ada = rows
        .iter()
        .find(|id| tree.get(**id).and_then(|n| n.name.clone()).as_deref() == Some("Ada"))
        .expect("Ada queryable by name");
    let ada_node = tree.get(*ada).expect("Ada live");
    assert_eq!(ada_node.role, "list item");
    assert!(!ada_node.states.contains(&"selected"));

    // Remove Ada: slot 0 rebinds to Bob (identity persists — value
    // events only, never a re-add), slot 1 retires (one remove).
    handle.set_props(mk(vec![("Bob".to_string(), true)]));
    host.run_until_idle();
    let diff2 = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
    assert_eq!(diff2.removed.len(), 1, "one row retired");
    tree.apply(&diff2, &parent_map(&host));
    let events = tree.take_events();
    assert_eq!(events.len(), 3, "remove + rebound values: {events:?}");
    assert!(
        matches!(events[0], AtspiEvent::ChildrenRemoved { .. }),
        "removal announces once first: {events:?}"
    );
    // The rebound survivor announces its new values, same identity.
    let rebound = find_retained_by_debug(&host, "row");
    assert_eq!(rebound.len(), 1);
    let survivor = tree.get(rebound[0]).expect("survivor silent and live");
    assert_eq!(survivor.name.as_deref(), Some("Bob"));
    assert!(survivor.states.contains(&"selected"));
    assert!(events
        .iter()
        .any(|e| matches!(e, AtspiEvent::StateChangedSelected { on: true, .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, AtspiEvent::NameChanged { .. })));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AtspiEvent::ChildrenAdded { .. })),
        "rebind must not re-add: {events:?}"
    );
    assert_eq!(tree.len(), 1);
}
