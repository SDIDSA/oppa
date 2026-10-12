use super::*;
use oppa::ime::ImeCompositionHandler;
use oppa::input::InputEvent;
use oppa::{ComponentHost, SharedString};

#[test]
fn text_input_renders_value_and_semantics() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("Ada"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    let id = node_by_debug(&host, "text-input");
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::TextField);
    assert_eq!(sem.label.as_deref(), Some("Name"));
    assert!(!sem.disabled);
    let b = host.committed_box(id).expect("hit box");
    assert!(
        approx(b.w, 200.0) && approx(b.h, 32.0),
        "default 200x32, got {}x{}",
        b.w,
        b.h
    );
    assert!(
        host.tab_order().contains(&id),
        "enabled input is tabbable (press owner, decision 96)"
    );
    press_node(&host, id);
    assert_eq!(
        host.focused_node(),
        Some(id),
        "press focuses through the M5 router"
    );
}

#[test]
fn text_input_placeholder_shows_when_empty() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    host.mount(
        "TI",
        TextInputProps::new("Name", value.clone()).placeholder("Enter name"),
        TextInput,
    );
    host.run_until_idle();
    // Empty: the outer carries the role; the placeholder is a
    // presentational span, never a bindable field.
    assert_eq!(host.text_fields().len(), 1);
    assert!(
        approx(lined_content_w(&host), 87.5),
        "placeholder 10x8.75 displayed"
    );
    value.set(SharedString::from("Ada"));
    host.run_until_idle();
    assert_eq!(host.text_fields().len(), 2, "outer + value leaf");
    assert!(
        approx(lined_content_w(&host), 26.25),
        "value 3x8.75 displayed"
    );
    let id = node_by_debug(&host, "text-input");
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!(sem.label.as_deref(), Some("Name"), "label stable");
}

/// Decision 461: live composition renders inline (the preedit
/// used to vanish until commit — soft-keyboard taps showed
/// nothing). The epoch subscription re-renders the field around
/// the display composite; idle trees are untouched.
#[test]
fn text_input_live_composition_renders_inline() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    host.mount(
        "TI",
        TextInputProps::new("Name", value.clone()).placeholder("Enter name"),
        TextInput,
    );
    host.run_until_idle();
    assert_eq!(host.text_fields().len(), 1, "empty: placeholder span only");
    let id = node_by_debug(&host, "text-input");
    press_node(&host, id);
    let mut session = host.focused_field_session().expect("session");
    session.composition_started(0);
    session.composition_updated("ok", 2);
    host.run_until_idle();
    assert_eq!(
        host.text_fields().len(),
        2,
        "composing leaf renders inline, not the placeholder"
    );
    assert!(
        approx(lined_content_w(&host), 17.5),
        "composing 2x8.75 displayed"
    );
    // Committing keeps the text through the normal funnel.
    session.composition_committed("ok");
    host.run_until_idle();
    assert!(
        approx(lined_content_w(&host), 17.5),
        "committed 2x8.75 displayed"
    );
}

/// Decision 293 (OQ-SINK-1 fixed): typing into an empty+
/// placeholder `TextInput` feeds the value through the session
/// fallback (the field leaf only exists once the value is
/// non-empty — the outer resolves to the instance session).
/// On DOM the same contract renders `value=""` with a native
/// `placeholder` attribute, so the browser only ever sends
/// typed text.
#[test]
fn text_input_placeholder_typing_feeds_value() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    host.mount(
        "TI",
        TextInputProps::new("Name", value.clone()).placeholder("Enter name"),
        TextInput,
    );
    host.run_until_idle();
    let id = node_by_debug(&host, "text-input");
    host.inject_input(InputEvent::text(id, "Ada"));
    host.run_until_idle();
    assert_eq!(
        value.get(),
        SharedString::from("Ada"),
        "typed text feeds the value even under a placeholder"
    );
}

/// Decision 293: the fallback also covers the value leaf of a
/// non-empty field (same session resolution through the leaf's
/// press owner — the web U8 channel for every field, not just
/// placeholder ones).
#[test]
fn text_input_leaf_typing_feeds_value_without_bind_call() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("Al"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    // No `bind_edit_session` anywhere (the fallback self-wires).
    let field = node_by_debug(&host, "field");
    host.inject_input(InputEvent::text(field, "Ada"));
    host.run_until_idle();
    assert_eq!(
        value.get(),
        SharedString::from("Ada"),
        "leaf text feeds the value with no app-layer bind"
    );
}

#[test]
fn text_input_disabled_is_handlerless() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(SharedString::from("Ada"));
    host.mount(
        "TIDis",
        TextInputProps::new("Name", value.clone()).disabled(),
        TextInput,
    );
    host.run_until_idle();
    let id = node_by_debug(&host, "text-input");
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
    assert_eq!(
        value.get(),
        SharedString::from("Ada"),
        "press on disabled never edits"
    );
    assert_eq!(host.focused_node(), None, "press never focuses");
}

#[test]
fn text_input_live_typing_updates_value() {
    use oppa_app::DesktopLoop;
    // Full runner path, headless: real `TextInput` + real
    // `DesktopLoop` — press to focus, type, backspace.
    let mut loop_ = DesktopLoop::new(500, 600, Box::new(FakeText), "Test").expect("headless loop");
    let value = loop_.host().runtime().signal(SharedString::from(""));
    loop_.mount(
        "TI",
        TextInputProps::new("Name", value.clone()).placeholder("Enter name"),
        TextInput,
    );
    let id = node_by_debug(loop_.host(), "text-input");
    let b = loop_.host().committed_box(id).expect("hit box");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    loop_
        .step(oppa::InputEvent::pointer_down(cx, cy))
        .expect("down steps");
    loop_
        .step(oppa::InputEvent::pointer_up(cx, cy))
        .expect("up steps");
    assert_eq!(loop_.host().focused_node(), Some(id));
    assert!(loop_.type_text("Hi").expect("types") > 0);
    assert_eq!(value.get(), SharedString::from("Hi"));
    loop_.backspace().expect("backspaces");
    assert_eq!(value.get(), SharedString::from("H"));
    loop_.type_text("!").expect("types");
    assert_eq!(value.get(), SharedString::from("H!"));
}

#[test]
fn text_input_edit_session_binds() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(SharedString::from(""));
    let handle = host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    let sessions = host.edit_sessions_for(handle.root_instance());
    assert_eq!(sessions.len(), 1, "one session keyed to the instance");
    sessions[0].insert("hi");
    host.run_until_idle();
    assert_eq!(
        value.get(),
        SharedString::from("hi"),
        "programmatic feed lands in the value signal"
    );
    // ...and the session feeds the field leaf through the U8 bind path.
    let field = node_by_debug(&host, "field");
    host.bind_edit_session(field, &sessions[0]);
    host.inject_input(InputEvent::text(field, "yo"));
    host.run_until_idle();
    assert_eq!(value.get(), SharedString::from("yo"));
}

/// Decision 478: a set icon renders a fixed leading slot inside
/// the field box — same outer box, icon vertically centered —
/// while focus, typing, and tab order behave exactly as without
/// one (the icon is decorative and handlerless).
#[test]
fn text_input_icon_renders_slot_and_still_types() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    let mut rgba = Vec::with_capacity(4 * 4 * 4);
    for _ in 0..16 {
        rgba.extend_from_slice(&[255, 0, 0, 255]);
    }
    let image = host.images().insert_pixels("field-icon", 4, 4, rgba);
    host.mount(
        "TIIcon",
        TextInputProps::new("Name", value.clone())
            .icon(image)
            .icon_size(20.0),
        TextInput,
    );
    host.run_until_idle();
    // Same outer box as the iconless default.
    let outer = host
        .committed_box(node_by_debug(&host, "text-input"))
        .expect("box");
    assert!(
        approx(outer.w, 200.0) && approx(outer.h, 32.0),
        "outer box unchanged, got {}x{}",
        outer.w,
        outer.h
    );
    // Fixed 20px slot, vertically centered in the box.
    let icon = host
        .committed_box(node_by_debug(&host, "text-input-icon"))
        .expect("icon slot laid out");
    assert!(
        approx(icon.w, 20.0) && approx(icon.h, 20.0),
        "icon slot 20x20, got {}x{}",
        icon.w,
        icon.h
    );
    assert!(
        approx(icon.y + icon.h / 2.0, outer.y + outer.h / 2.0),
        "icon vertically centered, got {} vs {}",
        icon.y + icon.h / 2.0,
        outer.y + outer.h / 2.0
    );
    assert!(
        icon.x >= outer.x && icon.x + icon.w <= outer.x + outer.w,
        "icon inside the box"
    );
    // Focus, typing, and tab order are untouched.
    let id = node_by_debug(&host, "text-input");
    assert!(
        host.tab_order().contains(&id),
        "outer stays the single tab stop"
    );
    assert!(
        !host
            .tab_order()
            .contains(&node_by_debug(&host, "text-input-icon")),
        "icon adds no tab stop of its own"
    );
    press_node(&host, id);
    assert_eq!(host.focused_node(), Some(id), "press focuses");
    host.inject_input(InputEvent::text(id, "Ada"));
    host.run_until_idle();
    assert_eq!(
        value.get(),
        SharedString::from("Ada"),
        "typing feeds the value with an icon set"
    );
}
