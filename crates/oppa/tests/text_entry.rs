//! U8 acceptance: the DOM→framework text-value loop, core-side.
//!
//! `InputEvent::Text` carries a field's full current value into the
//! bound feed signal (decision 188: feed-only, no handler dispatch;
//! explicit `bind_text` feeds win; otherwise the target's owning
//! instance session feeds when it unambiguously exists — decision
//! 293, so session-owning controls self-wire; anything else is a
//! quiet no-op; feeds prune with removals).
//! Browser proof (typing + swap preservation) rides the web harness;
//! this file proves the framework primitives.

#![allow(non_snake_case)]

use oppa::{ComponentHost, Ctx, InputEvent, SharedString, Signal, Text, TextField, VNode};
use oppa_macros::{component, Props};

#[derive(Clone, Props)]
struct FieldProps {
    value: Signal<SharedString>,
}

#[component]
fn FieldApp(ctx: &Ctx, props: &FieldProps) -> VNode {
    let _ = ctx;
    TextField {
        text: props.value.get(),
        style: Text::title_small,
        label: SharedString::from("Name"),
    }
    .into()
}

fn mount_field() -> (ComponentHost, Signal<SharedString>) {
    let host = ComponentHost::new();
    let value = host.runtime().signal(SharedString::from(""));
    host.mount(
        "FieldApp",
        FieldProps {
            value: value.clone(),
        },
        FieldApp,
    );
    host.run_until_idle();
    (host, value)
}

#[test]
fn text_fields_query_finds_the_leaf() {
    let (host, _) = mount_field();
    let fields = host.text_fields();
    assert_eq!(fields.len(), 1, "one TextField leaf retained");
}

#[test]
fn text_event_sets_the_bound_feed_and_rerenders() {
    let (host, value) = mount_field();
    let fields = host.text_fields();
    host.bind_text(fields[0], value.clone());
    let diffs_before = host.diff_count();
    host.inject_input(InputEvent::text(fields[0], "Ada"));
    host.run_until_idle();
    assert_eq!(value.get(), SharedString::from("Ada"));
    assert_eq!(host.bound_text(fields[0]), Some(SharedString::from("Ada")));
    assert!(
        host.diff_count() > diffs_before,
        "the signal write re-ran the component"
    );
}

#[test]
fn text_values_are_level_triggered_last_write_wins() {
    let (host, value) = mount_field();
    let fields = host.text_fields();
    host.bind_text(fields[0], value.clone());
    host.inject_input(InputEvent::text(fields[0], "Ada"));
    host.inject_input(InputEvent::text(fields[0], "Ad"));
    host.run_until_idle();
    assert_eq!(value.get(), SharedString::from("Ad"));
}

#[test]
fn text_to_unbound_target_is_a_quiet_noop() {
    let (host, value) = mount_field();
    let fields = host.text_fields();
    // No bind: the keystroke lands nowhere, loudly nothing.
    host.inject_input(InputEvent::text(fields[0], "Ada"));
    host.run_until_idle();
    assert_eq!(value.get(), SharedString::from(""));
    assert_eq!(host.bound_text(fields[0]), None);
}

#[derive(Clone, Props)]
struct GatedProps {
    value: Signal<SharedString>,
    show: Signal<bool>,
}

#[component]
fn GatedApp(ctx: &Ctx, props: &GatedProps) -> VNode {
    let _ = ctx;
    let mut kids = Vec::new();
    if props.show.get() {
        kids.push(
            TextField {
                text: props.value.get(),
                style: Text::title_small,
                label: SharedString::from("Name"),
            }
            .into(),
        );
    }
    oppa::Div("root").children(kids)
}

#[test]
fn text_feed_prunes_with_field_removal() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(SharedString::from(""));
    let show = host.runtime().signal(true);
    host.mount(
        "GatedApp",
        GatedProps {
            value: value.clone(),
            show: show.clone(),
        },
        GatedApp,
    );
    host.run_until_idle();
    assert_eq!(host.text_fields().len(), 1);
    let target = host.text_fields()[0];
    host.bind_text(target, value.clone());
    host.inject_input(InputEvent::text(target, "Ada"));
    host.run_until_idle();
    assert_eq!(value.get(), SharedString::from("Ada"));
    show.set(false);
    host.run_until_idle();
    assert!(host.text_fields().is_empty(), "field retired");
    assert_eq!(host.bound_text(target), None, "feed pruned");
}
