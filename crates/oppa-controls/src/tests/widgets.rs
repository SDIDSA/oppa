use super::*;
use oppa::input::{keys, InputEvent, KeyState};
use oppa::{
    find_retained_by_debug, ComponentHost, Ctx, CursorIcon, Div, Props, SharedString, Signal, Text,
    VNode,
};
use std::rc::Rc;

// ------------------------------------------------------------------
// Phase 38b (decisions 368–370): Tree, Splitter, DatePicker
// ------------------------------------------------------------------

fn tree_nodes() -> Vec<TreeNode> {
    vec![
        TreeNode::root("a", "Alpha"),
        TreeNode::child("a1", "Alpha-One", "a"),
        TreeNode::child("a2", "Alpha-Two", "a"),
        TreeNode::root("b", "Beta"),
    ]
}

#[derive(Clone)]
struct TreeCase {
    props: TreeProps,
}
impl Props for TreeCase {}

fn tree_render(ctx: &Ctx, p: &TreeCase) -> VNode {
    ctx.child("oppa::Tree", 7, &p.props, Tree)
}

/// Mounted tree + its controlled signals (one harness shape for
/// the Phase 38b tree tests — the tuple type rides an alias so
/// the signature stays under the complexity lint).
type TreeHarness = (
    ComponentHost,
    oppa::Collection<TreeNode>,
    Signal<Vec<SharedString>>,
    Signal<Option<SharedString>>,
);

fn mount_tree() -> TreeHarness {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let rt = host.runtime();
    let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:tree"));
    coll.ingest(tree_nodes());
    let expanded = rt.signal(Vec::<SharedString>::new());
    let selected = rt.signal(None::<SharedString>);
    let props = TreeProps {
        nodes: coll.clone(),
        expanded: expanded.clone(),
        selected: selected.clone(),
        label: SharedString::from("Files"),
        width: 280.0,
        height: 200.0,
        row_height: 28.0,
        overscan: 2,
        enabled: true,
        debug: SharedString::from("tree"),
    };
    host.mount("Tree", TreeCase { props }, tree_render);
    host.run_until_idle();
    (host, coll, expanded, selected)
}

/// G12: the flatten shows roots only when collapsed, children
/// when expanded (pure); the toggle is symmetric.
#[test]
fn tree_visible_flattens_and_toggles() {
    let rows: Vec<oppa::Row<TreeNode>> = tree_nodes()
        .into_iter()
        .enumerate()
        .map(|(i, value)| oppa::Row {
            id: oppa::RowId(i as u64),
            value,
        })
        .collect();
    let flat = tree_visible(&rows, &[]);
    assert_eq!(
        flat.iter()
            .map(|(r, d)| (r.value.id.to_string(), *d))
            .collect::<Vec<_>>(),
        vec![("a".to_string(), 0), ("b".to_string(), 0)],
        "collapsed shows roots only"
    );
    let open = vec![SharedString::from("a")];
    let flat = tree_visible(&rows, &open);
    assert_eq!(
        flat.iter()
            .map(|(r, d)| (r.value.id.to_string(), *d))
            .collect::<Vec<_>>(),
        vec![
            ("a".to_string(), 0),
            ("a1".to_string(), 1),
            ("a2".to_string(), 1),
            ("b".to_string(), 0),
        ],
        "expanded interleaves children in commit order"
    );
    let shut = tree_toggled(&open, &SharedString::from("a"));
    assert!(shut.is_empty(), "toggle is symmetric");
    let reopened = tree_toggled(&shut, &SharedString::from("b"));
    assert_eq!(reopened, vec![SharedString::from("b")]);
}

/// G12: chevron expands, row press selects, arrows walk the
/// visible list, and roles announce Tree/TreeItem.
#[test]
fn tree_expands_selects_and_walks() {
    let (host, _coll, expanded, selected) = mount_tree();
    // Container announces the tree role.
    let root = node_by_debug(&host, "tree");
    let sem = host.retained_semantics(root).expect("semantics");
    assert_eq!(sem.role, oppa::Role::Tree);
    assert_eq!(sem.label.as_deref(), Some("Files"));
    // Collapsed: two rows, both TreeItem.
    assert_eq!(find_retained_by_debug(&host, "tree-row").len(), 2);
    // Chevron expands "a" (first chevron hit in commit order).
    let chevrons = find_retained_by_debug(&host, "tree-chevron-hit");
    assert_eq!(chevrons.len(), 2);
    press_node(&host, chevrons[0]);
    assert_eq!(expanded.get(), vec![SharedString::from("a")]);
    assert_eq!(find_retained_by_debug(&host, "tree-row").len(), 4);
    // Row press selects (rows render in visible order).
    let rows = find_retained_by_debug(&host, "tree-row");
    press_node(&host, rows[1]);
    assert_eq!(selected.get(), Some(SharedString::from("a1")));
    let sem1 = host.retained_semantics(rows[1]).expect("semantics");
    assert_eq!(sem1.role, oppa::Role::TreeItem);
    assert_eq!(sem1.selected, Some(true));
    // Down moves selection to the next visible row.
    host.inject_input(InputEvent::key(keys::DOWN, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(selected.get(), Some(SharedString::from("a2")));
    // Left on a leaf ascends to the parent.
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(selected.get(), Some(SharedString::from("a")));
    // Left on the open parent collapses it.
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert!(expanded.get().is_empty(), "Left collapses the open node");
    assert_eq!(find_retained_by_debug(&host, "tree-row").len(), 2);
    // Right re-opens, Enter-equivalent press selects.
    host.inject_input(InputEvent::key(keys::RIGHT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(expanded.get(), vec![SharedString::from("a")]);
}

/// G13: minima clamp loudly-checked bounds; arrows nudge.
#[test]
fn splitter_fraction_clamps_and_arrows_nudge() {
    assert_eq!(splitter_fraction(0.5, 400.0, 8.0, 40.0, 40.0), 0.5);
    assert_eq!(
        splitter_fraction(0.0, 400.0, 8.0, 40.0, 40.0),
        (40.0 + 4.0) / 400.0,
        "first minimum pins the low end"
    );
    assert_eq!(
        splitter_fraction(1.0, 400.0, 8.0, 40.0, 40.0),
        1.0 - (40.0 + 4.0) / 400.0,
        "second minimum pins the high end"
    );
    // Mounted arrows nudge through the funnel.
    let host = ComponentHost::new();
    let frac = host.runtime().signal(0.5f32);
    fn split_pane(_ctx: &Ctx) -> VNode {
        Div("pane").child(VNode::from(Text {
            text: SharedString::from("pane"),
            style: Text::body_secondary,
        }))
    }
    let props = SplitterProps {
        fraction: frac.clone(),
        axis: SplitterAxis::Vertical,
        width: 400.0,
        height: 300.0,
        divider_px: 8.0,
        min_first_px: 40.0,
        min_second_px: 40.0,
        enabled: true,
        debug: SharedString::from("split"),
        first: Rc::new(split_pane),
        second: Rc::new(split_pane),
        on_change: None,
    };
    host.mount("Split", props, Splitter);
    host.run_until_idle();
    let div = node_by_debug(&host, "split-divider");
    press_node(&host, div);
    assert_eq!(host.focused_node(), Some(div));
    host.inject_input(InputEvent::key(keys::RIGHT, KeyState::Pressed));
    host.run_until_idle();
    assert!(
        (frac.get() - 0.55).abs() < 1e-6,
        "Right nudges +0.05, got {}",
        frac.get()
    );
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert!(
        (frac.get() - 0.5).abs() < 1e-6,
        "Left nudges back, got {}",
        frac.get()
    );
    // Divider shows the resize cursor.
    let cursor = host.with_retained_mut(|rec, styles| {
        let n = rec.get(div).expect("live divider");
        styles.get(n.style).cloned().unwrap_or_default().cursor
    });
    assert_eq!(cursor, Some(CursorIcon::ColResize));
}

/// G13/G14 pure cores: leap rule, known weekday (2026-10-01 is
/// a Thursday), parse/format round-trip, month shift clamping.
#[test]
fn date_math_shapes() {
    assert!(is_leap(2024) && !is_leap(2025) && !is_leap(1900) && is_leap(2000));
    assert_eq!(days_in_month(2024, 2), 29);
    assert_eq!(days_in_month(2025, 2), 28);
    // 2026-10-01: Thursday → Monday0 == 3.
    assert_eq!(
        weekday_monday0(Date {
            year: 2026,
            month: 10,
            day: 1
        }),
        3
    );
    // Monday 2026-09-28 → 0.
    assert_eq!(
        weekday_monday0(Date {
            year: 2026,
            month: 9,
            day: 28
        }),
        0
    );
    let d = Date::new(2026, 10, 1);
    assert_eq!(format_date(d).to_string(), "2026-10-01");
    assert_eq!(parse_date("2026-10-01"), Some(d));
    assert_eq!(parse_date("2026-13-01"), None, "month 13 never parses");
    assert_eq!(parse_date("2026-02-30"), None, "Feb 30 never parses");
    assert_eq!(parse_date("10-01-2026"), None, "non-padded never parses");
    assert_eq!(parse_date("hello"), None);
    // Jan 31 shifted +1 clamps to Feb 28 (2025, non-leap).
    let jan31 = Date::new(2025, 1, 31);
    let feb = shift_month(jan31, 1, None, None).expect("in range");
    assert_eq!((feb.year, feb.month, feb.day), (2025, 2, 28));
    // Bound refusal: shifting past max is None (disable rule).
    let oct = Date::new(2026, 10, 15);
    assert_eq!(
        shift_month(
            oct,
            1,
            Some(Date::new(2026, 10, 1)),
            Some(Date::new(2026, 10, 31))
        ),
        None,
        "November lies fully past October max"
    );
    // Day steps pin to the bounds.
    assert_eq!(
        shift_days(
            Date::new(2026, 10, 31),
            1,
            Some(Date::new(2026, 10, 1)),
            Some(Date::new(2026, 10, 31))
        ),
        Date::new(2026, 10, 31),
        "stepping past max pins"
    );
}

/// G14: picking a day sets the value and closes; prev/next flip
/// the shown month; the text bridge parses; bounds refuse.
#[test]
fn date_picker_picks_flips_and_parses() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(Date::new(2026, 10, 1));
    let open = host.runtime().signal(false);
    let props = DatePickerProps {
        value: value.clone(),
        open: open.clone(),
        min: None,
        max: None,
        enabled: true,
        width: 240.0,
        label: SharedString::from("When"),
    };
    host.mount("DP", props, DatePicker);
    host.run_until_idle();
    // Closed: no popup in the tree.
    assert!(find_retained_by_debug(&host, "date-popup").is_empty());
    // The chevron half toggles (the box center belongs to the
    // text bridge — taps there focus the field, the knob→track
    // precedent: each half routes to its own press owner).
    press_node(&host, node_by_debug(&host, "date-chevron"));
    assert!(open.get());
    assert_eq!(find_retained_by_debug(&host, "date-popup").len(), 1);
    assert_eq!(find_retained_by_debug(&host, "date-grid").len(), 1);
    // Pick the 15th: value sets, popup closes.
    press_node(&host, node_by_debug(&host, "date-day-15"));
    assert_eq!(value.get(), Date::new(2026, 10, 15));
    assert!(!open.get(), "pick closes the popup");
    // Reopen via the chevron, flip to November (day preserved).
    press_node(&host, node_by_debug(&host, "date-chevron"));
    press_node(&host, node_by_debug(&host, "date-next"));
    assert_eq!(value.get(), Date::new(2026, 11, 15));
    // Text bridge: a valid spelling commits.
    press_node(&host, node_by_debug(&host, "date-text"));
    let sess = host.focused_field_session().expect("session");
    sess.select_all();
    sess.insert("2026-12-25");
    host.run_until_idle();
    assert_eq!(value.get(), Date::new(2026, 12, 25));
    // Invalid text never moves the value.
    let sess = host.focused_field_session().expect("session");
    sess.select_all();
    sess.insert("not-a-date");
    host.run_until_idle();
    assert_eq!(value.get(), Date::new(2026, 12, 25));
    // Bounds: days outside [min, max] are handlerless.
    let host2 = ComponentHost::new();
    host2.set_text_service(Box::new(FakeText));
    let value2 = host2.runtime().signal(Date::new(2026, 10, 10));
    let open2 = host2.runtime().signal(true);
    host2.mount(
        "DPB",
        DatePickerProps {
            value: value2.clone(),
            open: open2.clone(),
            min: Some(Date::new(2026, 10, 5)),
            max: Some(Date::new(2026, 10, 20)),
            enabled: true,
            width: 240.0,
            label: SharedString::from("When"),
        },
        DatePicker,
    );
    host2.run_until_idle();
    let day1 = node_by_debug(&host2, "date-day-1");
    assert!(
        host2.retained_handlers(day1).is_empty(),
        "out-of-bounds days carry no press"
    );
    press_node(&host2, node_by_debug(&host2, "date-day-10"));
    assert_eq!(value2.get(), Date::new(2026, 10, 10));
}
