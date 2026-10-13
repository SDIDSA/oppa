use super::*;
use oppa::input::{keys, InputEvent, KeyState};
use oppa::{
    find_retained_by_debug, ComponentHost, Ctx, Div, Props, SharedString, Signal, Style, VNode,
};

// ------------------------------------------------------------------
// Round 3.3 (decision 262): the dismiss-first chain over a live
// popup + field — composition cancels before focus clears
// (clearing first would commit it, locked #27), popups close at
// the author step, and an empty chain reports Unhandled for the
// runner (nav pop / exit — `nav.rs` BackPress).
// ------------------------------------------------------------------

#[derive(Clone)]
struct BackScene {
    open: Signal<bool>,
    value: Signal<SharedString>,
}
impl Props for BackScene {}

fn back_scene(ctx: &Ctx, p: &BackScene) -> VNode {
    // Field first: TAB focuses it even with the modal open
    // (depth-first tab order — no pointer needed, so the open
    // backdrop never eats the focus press).
    Div("back-root").children([
        ctx.child(
            "back-field",
            1,
            &TextInputProps::new("Name", p.value.clone()),
            TextInput,
        ),
        ctx.child(
            "back-modal",
            2,
            &ModalProps::new("Delete file?", p.open.clone()),
            Modal,
        ),
    ])
}

fn back_harness() -> (ComponentHost, Signal<bool>, Signal<SharedString>) {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    let value = host.runtime().signal(SharedString::from("Ada"));
    host.mount(
        "Back",
        BackScene {
            open: open.clone(),
            value: value.clone(),
        },
        back_scene,
    );
    host.run_until_idle();
    assert!(
        !find_retained_by_debug(&host, "modal-card").is_empty(),
        "popup mounted open"
    );
    // Keyboard focus (the field is first in tab order).
    host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
    host.run_until_idle();
    let field = node_by_debug(&host, "text-input");
    assert_eq!(host.focused_node(), Some(field), "TAB focuses the field");
    assert!(host.focused_field_session().is_some(), "session live");
    (host, open, value)
}

fn start_composition(host: &ComponentHost) {
    use oppa::ime::{dispatch_ime_event, ImeCompositionEvent};
    let mut sess = host
        .focused_field_session()
        .expect("focused session composes");
    dispatch_ime_event(
        &mut sess,
        &ImeCompositionEvent::CompositionStarted { start_byte: 3 },
    );
    dispatch_ime_event(
        &mut sess,
        &ImeCompositionEvent::CompositionUpdated {
            composition: "Adax".to_string(),
            caret_byte: 4,
        },
    );
    assert!(sess.is_composing(), "composition active");
}

#[test]
fn back_chain_cancels_then_blurs_then_exits() {
    let (host, open, value) = back_harness();
    start_composition(&host);
    // Step 1: composition cancels (reverts — value untouched,
    // popup stays, focus stays for the next press).
    assert_eq!(host.handle_back(), oppa::BackOutcome::CompositionCancelled);
    host.run_until_idle();
    assert_eq!(&*value.get(), "Ada", "cancel reverts, never commits");
    assert!(open.get(), "popup survives the composition step");
    assert!(host.focused_node().is_some(), "focus survives too");
    assert!(
        host.focused_field_session()
            .is_none_or(|s| !s.is_composing()),
        "nothing composing anymore"
    );
    // Step 2 (author popup step — the host cannot close what it
    // does not own, so the author/runner flips `open` first, as
    // the BackPress contract orders it).
    open.set(false);
    host.run_until_idle();
    assert!(
        find_retained_by_debug(&host, "modal-card").is_empty(),
        "author step dismisses the popup"
    );
    // Step 3: focus clears.
    assert_eq!(host.handle_back(), oppa::BackOutcome::FocusCleared);
    host.run_until_idle();
    assert_eq!(host.focused_node(), None);
    // Step 4: empty chain reports Unhandled (the runner exits).
    assert_eq!(host.handle_back(), oppa::BackOutcome::Unhandled);
}

#[test]
fn esc_key_rides_the_same_chain() {
    // The router's ESC arm runs `handle_back` (desktop parity —
    // one chain everywhere): first ESC cancels, second blurs.
    let (host, _open, value) = back_harness();
    start_composition(&host);
    host.inject_input(InputEvent::key(keys::ESCAPE, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(&*value.get(), "Ada", "ESC cancels the composition");
    assert!(host.focused_node().is_some(), "focus survives step one");
    host.inject_input(InputEvent::key(keys::ESCAPE, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(host.focused_node(), None, "second ESC blurs");
    // Repeat ESC repeats are ignored (no double blur, no exit
    // from the router — exit stays runner-side).
    host.inject_input(InputEvent::Key {
        code: keys::ESCAPE,
        modifiers: oppa::Modifiers::NONE,
        state: KeyState::Pressed,
        repeat: true,
    });
    host.run_until_idle();
    assert_eq!(host.handle_back(), oppa::BackOutcome::Unhandled);
}

// ------------------------------------------------------------------
// Round 5.1 (decision 268): TextArea — the multi-line sibling.
// ------------------------------------------------------------------

#[test]
fn text_area_renders_area_role_and_focuses() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hi"));
    host.mount("TA", TextAreaProps::new("Notes", value.clone()), TextArea);
    host.run_until_idle();
    let id = node_by_debug(&host, "text-area");
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::TextArea);
    assert_eq!(sem.label.as_deref(), Some("Notes"));
    assert!(!sem.disabled);
    assert!(
        host.tab_order().contains(&id),
        "enabled area is tabbable (press owner, decision 96)"
    );
    press_node(&host, id);
    assert_eq!(
        host.focused_node(),
        Some(id),
        "press focuses through the M5 router"
    );
    assert!(host.focused_field_session().is_some(), "session live");
}

#[test]
fn text_area_auto_heights_with_lines() {
    // Explicit width, content-driven height: three lines stand
    // taller than one (same width — no horizontal growth). The
    // area sits in a sized parent (areas live inside scenes,
    // never as roots — roots fill the viewport by decision 74).
    let one = area_box_height("one");
    let three = area_box_height("one\ntwo\nthree");
    assert!(
        three > one,
        "three lines ({three}) stand taller than one ({one})"
    );
    #[derive(Clone)]
    struct WrapProps {
        value: Signal<SharedString>,
    }
    impl Props for WrapProps {}
    fn wrap_scene(ctx: &Ctx, p: &WrapProps) -> VNode {
        Div("wrap")
            .style(Style::new().size(300, 400))
            .child(ctx.child(
                "area",
                1,
                &TextAreaProps::new("Notes", p.value.clone()),
                TextArea,
            ))
    }
    fn area_box_height(value: &str) -> f32 {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let v = host.runtime().signal(SharedString::from(value));
        host.mount("Wrap", WrapProps { value: v }, wrap_scene);
        host.run_until_idle();
        let b = host
            .committed_box(node_by_debug(&host, "text-area"))
            .expect("area box");
        assert!(approx(b.w, 200.0), "explicit width holds, got {}", b.w);
        b.h
    }
}

#[test]
fn text_area_placeholder_and_disabled_mirror_input() {
    // Empty + placeholder: presentational span, never the value.
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    host.mount(
        "TA",
        TextAreaProps::new("Notes", value.clone()).placeholder("Write here"),
        TextArea,
    );
    host.run_until_idle();
    assert_eq!(host.text_fields().len(), 1, "area counts as a field");
    // Disabled: handlerless, untabbable, press never focuses.
    let host2 = ComponentHost::new();
    host2.set_text_service(Box::new(FakeText));
    let value2 = host2.runtime().signal(SharedString::from("x"));
    let mut props = TextAreaProps::new("Notes", value2);
    props.enabled = false;
    host2.mount("TA", props, TextArea);
    host2.run_until_idle();
    let id = node_by_debug(&host2, "text-area");
    assert!(host2.retained_semantics(id).expect("semantics").disabled);
    assert!(host2.retained_handlers(id).is_empty());
    assert!(!host2.tab_order().contains(&id));
    press_node(&host2, id);
    assert_eq!(host2.focused_node(), None, "press never focuses");
}

#[test]
fn text_area_session_types_multiline() {
    // Programmatic multi-line feed lands in the value (the
    // router ENTER path is core-proven in m5_input).
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    host.mount("TA", TextAreaProps::new("Notes", value.clone()), TextArea);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "text-area"));
    let sess = host.focused_field_session().expect("session");
    sess.insert("a\nb");
    host.run_until_idle();
    assert_eq!(&*value.get(), "a\nb");
}

// ------------------------------------------------------------------
// Round 5.2 (decision 269): focus trap — Tab inside an open
// dialog cycles its subtree (derived from the retained tree,
// no registration); closing restores the global order.
// ------------------------------------------------------------------

#[derive(Clone)]
struct TrapScene {
    open: Signal<bool>,
    value: Signal<SharedString>,
}
impl Props for TrapScene {}

fn trap_scene(ctx: &Ctx, p: &TrapScene) -> VNode {
    // Field first, then the modal (opt-out backdrop: handlerless,
    // so the tab order is exactly field, cancel, confirm).
    Div("trap-root").children([
        ctx.child(
            "trap-field",
            1,
            &TextInputProps::new("Name", p.value.clone()),
            TextInput,
        ),
        ctx.child(
            "trap-modal",
            2,
            &ModalProps::new("Delete file?", p.open.clone()).no_backdrop_dismiss(),
            Modal,
        ),
    ])
}

fn trap_harness() -> (ComponentHost, Signal<bool>) {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    let value = host.runtime().signal(SharedString::from(""));
    host.mount(
        "Trap",
        TrapScene {
            open: open.clone(),
            value,
        },
        trap_scene,
    );
    host.run_until_idle();
    assert!(
        !find_retained_by_debug(&host, "modal-card").is_empty(),
        "popup mounted open"
    );
    (host, open)
}

fn debug_of(host: &ComponentHost) -> String {
    host.with_retained_mut(|rec, _| {
        rec.get(host.focused_node().expect("focus"))
            .map(|n| n.debug.clone())
            .unwrap_or_default()
    })
}

#[test]
fn tab_cycles_inside_open_dialog() {
    let (host, _open) = trap_harness();
    // Global order first: field, then into the dialog.
    tab(&host, false);
    assert_eq!(debug_of(&host), "text-input", "first stop: field");
    tab(&host, false);
    assert_eq!(debug_of(&host), "modal-cancel", "enters the dialog");
    tab(&host, false);
    assert_eq!(debug_of(&host), "modal-confirm");
    // Trapped: next cycles back to cancel, never the field.
    tab(&host, false);
    assert_eq!(debug_of(&host), "modal-cancel", "trap cycles");
    tab(&host, false);
    assert_eq!(debug_of(&host), "modal-confirm");
    // Backward wraps within the trap too.
    tab(&host, true);
    assert_eq!(debug_of(&host), "modal-cancel", "shift+tab wraps");
}

#[test]
fn closing_dialog_restores_global_order() {
    let (host, open) = trap_harness();
    tab(&host, false);
    tab(&host, false);
    assert_eq!(debug_of(&host), "modal-cancel", "inside first");
    open.set(false);
    host.run_until_idle();
    assert!(
        find_retained_by_debug(&host, "modal-card").is_empty(),
        "dialog unmounts"
    );
    // Focus was inside: unmount clears it (1.4 rule), TAB
    // restarts the global order at the field.
    tab(&host, false);
    assert_eq!(debug_of(&host), "text-input", "global order restored");
}

// ------------------------------------------------------------------
// Round 5.4 (decision 271): on_change notification + uncontrolled
// companions — internal flips report payload-carrying values;
// self-managed signals advance identically to controlled ones.
// ------------------------------------------------------------------
