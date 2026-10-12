use super::*;
use oppa::{
    find_retained_by_debug, Color, Column, ComponentHost, Ctx, Div, InputEvent, Props, Px,
    SharedString, Signal, Text, VNode,
};
use std::rc::Rc;

fn route_home(_ctx: &Ctx) -> VNode {
    Div("route-home").build()
}

fn route_settings(_ctx: &Ctx) -> VNode {
    Div("route-settings").build()
}

/// G22: the Phase 36 leaves render as controls (label wrappers
/// announce, bare leaves stay byte-identical); NavHost switches
/// on the stack and empties quietly past unknown names.
#[test]
fn g22_wrappers_render_leaves_and_route() {
    // RichText labeled (wrapper announces) + bare (mounts).
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    host.mount(
        "RT",
        RichTextViewProps {
            spans: vec![oppa::TextSpan::new("hi")],
            style: Text::body_secondary,
            label: Some(SharedString::from("Greeting")),
            debug: SharedString::from("rich"),
        },
        RichTextView,
    );
    host.run_until_idle();
    let wrap = node_by_debug(&host, "rich");
    let sem = host.retained_semantics(wrap).expect("semantics");
    assert_eq!(sem.label.as_deref(), Some("Greeting"));
    let host_bare = ComponentHost::new();
    host_bare.set_text_service(Box::new(FakeText));
    host_bare.mount(
        "RTB",
        RichTextViewProps {
            spans: vec![oppa::TextSpan::new("hi")],
            style: Text::body_secondary,
            label: None,
            debug: SharedString::from("rich-bare"),
        },
        RichTextView,
    );
    host_bare.run_until_idle();
    assert!(
        find_retained_by_debug(&host_bare, "rich-bare").is_empty(),
        "bare leaves carry no wrapper"
    );
    // Image: the `img` leaf + alt announcement.
    let host2 = ComponentHost::new();
    host2.mount(
        "IMG",
        ImageViewProps {
            image: oppa::ImageId(7),
            size: 48.0,
            radius: 4.0,
            alt: SharedString::from("Logo"),
        },
        ImageView,
    );
    host2.run_until_idle();
    let img = node_by_debug(&host2, "img");
    let sem2 = host2.retained_semantics(img).expect("semantics");
    assert_eq!(sem2.label.as_deref(), Some("Logo"));
    // Canvas: the named leaf mounts.
    let host3 = ComponentHost::new();
    host3.mount(
        "CV",
        CanvasViewProps {
            ops: vec![oppa::CanvasOp::Rect {
                x: Px::of(2.0),
                y: Px::of(2.0),
                w: Px::of(20.0),
                h: Px::of(12.0),
                color: Color(0x22_66_CC),
            }],
            width: 64.0,
            height: 32.0,
            label: None,
            debug: SharedString::from("plot"),
        },
        CanvasView,
    );
    host3.run_until_idle();
    assert_eq!(find_retained_by_debug(&host3, "plot").len(), 1);
    // NavHost: current route renders, pushes switch, unknown
    // names empty quietly (the unmatched-Tabs precedent).
    let host4 = ComponentHost::new();
    let stack = host4.runtime().signal(oppa::NavStack::new(
        oppa::Route::new("home").expect("route"),
    ));
    let routes = vec![
        RouteView {
            name: SharedString::from("home"),
            content: Rc::new(route_home),
        },
        RouteView {
            name: SharedString::from("settings"),
            content: Rc::new(route_settings),
        },
    ];
    host4.mount(
        "NAV",
        NavHostProps {
            stack: stack.clone(),
            routes: routes.clone(),
        },
        NavHost,
    );
    host4.run_until_idle();
    assert_eq!(find_retained_by_debug(&host4, "route-home").len(), 1);
    let mut moved = stack.get();
    moved.push(oppa::Route::new("settings").expect("route"));
    stack.set(moved);
    host4.run_until_idle();
    assert!(find_retained_by_debug(&host4, "route-home").is_empty());
    assert_eq!(find_retained_by_debug(&host4, "route-settings").len(), 1);
    let mut lost = stack.get();
    lost.push(oppa::Route::new("void").expect("route"));
    stack.set(lost);
    host4.run_until_idle();
    assert_eq!(find_retained_by_debug(&host4, "nav-empty").len(), 1);
}

// ------------------------------------------------------------------
// Phase 39a (decisions 377–379): DOM → framework value loop
// ------------------------------------------------------------------

#[derive(Clone)]
struct LoopCase {
    props: TextInputProps,
    count: Signal<u32>,
}
impl Props for LoopCase {}

fn loop_render(ctx: &Ctx, p: &LoopCase) -> VNode {
    Column::new().gap(8).children([
        ctx.child("oppa::LoopField", 1, &p.props, TextInput),
        VNode::from(Text {
            text: SharedString::from(format!("ticks {}", p.count.get())),
            style: Text::body_secondary,
        }),
    ])
}

/// U8 value loop (decision 377): DOM `input` events arrive as
/// `InputEvent::Text` and set the controlled signal exactly per
/// keystroke (counted, not inferred), reporting through
/// `on_change` like native commits; an unrelated tick
/// re-renders around the field with value, caret, and node
/// identity intact and no echo; programmatic sets never report
/// (no loop — `on_change` is the commit path, not the set path).
#[test]
fn value_loop_feeds_report_and_ticks_preserve() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    let count = host.runtime().signal(0u32);
    let (notify, seen) = blowing::<SharedString>();
    let props = TextInputProps {
        on_change: Some(notify),
        ..TextInputProps::new("Name", value.clone())
    };
    host.mount(
        "Loop",
        LoopCase {
            props,
            count: count.clone(),
        },
        loop_render,
    );
    host.run_until_idle();
    // Focus first (typing implies browser focus; the feed
    // itself never moves framework focus).
    let field = node_by_debug(&host, "text-input");
    press_node(&host, field);
    assert_eq!(host.focused_node(), Some(field));
    // Keystroke 1: the DOM value feeds exactly + reports.
    host.inject_input(InputEvent::text(field, "h"));
    host.run_until_idle();
    assert_eq!(value.get().to_string(), "h");
    assert_eq!(
        seen.borrow()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        vec!["h".to_string()],
        "platform commits report like native inserts"
    );
    assert_eq!(
        host.focused_field_session().expect("session").caret(),
        1,
        "feed caret collapses to the end"
    );
    // Keystroke 2: full values stay exact (level-triggered —
    // each event carries the whole current value).
    host.inject_input(InputEvent::text(field, "hi"));
    host.run_until_idle();
    assert_eq!(value.get().to_string(), "hi");
    assert_eq!(
        seen.borrow()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        vec!["h".to_string(), "hi".to_string()],
        "counted per step, never inferred"
    );
    // Unrelated tick: re-renders around the field — value,
    // caret, and node identity intact, no echo.
    count.set(1);
    host.run_until_idle();
    assert_eq!(value.get().to_string(), "hi");
    assert_eq!(
        host.focused_field_session().expect("session").caret(),
        2,
        "ticks never move the caret"
    );
    assert_eq!(
        node_by_debug(&host, "text-input"),
        field,
        "no remount across ticks"
    );
    assert_eq!(seen.borrow().len(), 2, "ticks never echo");
    // Programmatic sets flow to the value without reporting
    // (no loop back into `on_change`).
    value.set(SharedString::from("HELLO"));
    host.run_until_idle();
    assert_eq!(value.get().to_string(), "HELLO");
    assert_eq!(seen.borrow().len(), 2, "sets never report");
}

// ------------------------------------------------------------------
// Phase 39b (decision 380): reference-app coverage
// ------------------------------------------------------------------

/// The sink's Views tab mounts every Phase 38b–38c control
/// (Tree, Toolbar, Menubar, RichText, Canvas, Image, NavHost)
/// plus the Form validation/DatePicker and Layout Grid/Splitter
/// legs on their own tabs.
#[test]
fn kitchen_sink_views_tab_mounts_phase38_controls() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    host.mount("Sink", KitchenSinkProps::default(), KitchenSinkApp);
    host.run_until_idle();
    // Five tabs now (Form/Layout/Overlays/Platform/Views).
    let tabs = find_retained_by_debug(&host, "tab-item");
    assert_eq!(tabs.len(), 5);
    press_node(&host, tabs[4]);
    for debug in [
        "sink-tree",
        "toolbar-cell-0",
        "menubar-bar",
        "sink-rich",
        "sink-plot",
        "img",
        "sink-views-home",
    ] {
        assert_eq!(
            find_retained_by_debug(&host, debug).len(),
            1,
            "{debug} mounts on the Views tab"
        );
    }
    // Form leg: the invalid email + date box mount.
    press_node(&host, tabs[0]);
    assert_eq!(find_retained_by_debug(&host, "sink-email-field").len(), 1);
    assert_eq!(find_retained_by_debug(&host, "date").len(), 1);
    // Layout leg: the grid + splitter mount.
    press_node(&host, tabs[1]);
    assert_eq!(find_retained_by_debug(&host, "sink::Layout::Grid").len(), 1);
    assert_eq!(find_retained_by_debug(&host, "sink-split").len(), 1);
}
