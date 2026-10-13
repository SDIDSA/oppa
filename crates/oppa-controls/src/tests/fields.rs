use super::*;
use oppa::ime::ImeCompositionHandler;
use oppa::input::InputEvent;
use oppa::{ComponentHost, SharedString, Signal, Text};

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

#[test]
fn text_input_focus_signal_updates_on_focus_and_blur() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("Ada"));
    let focused = host.runtime().signal(false);
    host.mount(
        "TI",
        TextInputProps::new("Name", value.clone()).focused(focused.clone()),
        TextInput,
    );
    host.run_until_idle();
    assert!(!focused.get(), "initially unfocused");

    let id = node_by_debug(&host, "text-input");
    press_node(&host, id);
    host.run_until_idle();
    assert!(focused.get(), "focusing updates bound signal to true");

    // Blur
    host.inject_input(InputEvent::Focus { node: None });
    host.run_until_idle();
    assert!(!focused.get(), "blurring updates bound signal to false");
}

/// Decision 510: the chained `TextInputProps` builders set the same
/// fields the old post-construction mutation did (no silent default
/// survives the chain).
#[test]
fn text_input_builder_chain_sets_all_fields() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(SharedString::from(""));
    let focused = host.runtime().signal(false);
    let p = TextInputProps::new("Email", value.clone())
        .placeholder("you@clinic.com")
        .placeholder_ink(Color(0x4D_FF_FF_FF))
        .borderless(true)
        .style(oppa::TextClass::Custom {
            size_px: 16,
            weight: oppa::FontWeight::NORMAL,
        })
        .focused(focused.clone())
        .size(344.0, 20.0)
        .debug("sera-email")
        .required(true)
        .invalid(true)
        .error_message("Bad email")
        .helper_text("Help");
    assert_eq!((p.width, p.height), (344.0, 20.0));
    assert_eq!(p.debug.as_ref(), "sera-email");
    assert!(p.required && p.invalid && p.borderless);
    assert_eq!(p.error_message.as_deref(), Some("Bad email"));
    assert_eq!(p.helper_text.as_deref(), Some("Help"));
    assert_eq!(p.placeholder_ink, Some(Color(0x4D_FF_FF_FF)));
    assert!(p.focused.is_some(), "focus signal binds");
}

// ---------------------------------------------------------------------------
// Decision 510: `FieldChrome` composite input chrome
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ChromeCase {
    error: Signal<SharedString>,
    focused: bool,
}
impl oppa::Props for ChromeCase {}

fn chrome_render(ctx: &Ctx, p: &ChromeCase) -> VNode {
    let raw = p.error.get();
    let error = if raw.is_empty() { None } else { Some(raw) };
    FieldChrome::new("fc", "Email address")
        .label_ink(Color(0xFF_D0_D0_D0))
        .chrome_bg(Color(0xFF_17_18_1C))
        .edge(Color(0xFF_3A_3B_42))
        .edge_focused(Color(0xFF_54_8D_EB))
        .build(
            ctx,
            Div("fc-icon").style(Style::new().square(18)).build(),
            VNode::from(Text::new("x").size(16)),
            error,
            p.focused,
        )
}

fn mount_chrome(error: &str, focused: bool) -> (ComponentHost, Signal<SharedString>) {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    host.set_viewport(500.0, 400.0);
    let err = host.runtime().signal(SharedString::from(error));
    host.mount(
        "FC",
        ChromeCase {
            error: err.clone(),
            focused,
        },
        chrome_render,
    );
    host.run_until_idle();
    (host, err)
}

fn chrome_edge(host: &ComponentHost) -> Color {
    let id = node_by_debug(host, "fc");
    host.with_retained_mut(|rec, styles| {
        styles
            .get(rec.get(id).expect("live chrome").style)
            .cloned()
            .unwrap_or_default()
            .border
            .expect("chrome edge")
            .color
    })
}

/// The chrome owns the measured box (18px icon slot, 20px-tall
/// content band, 14px vertical pads): label insets 5px from the
/// chrome's left edge, the error slot reserves space idle and shows
/// the message on error, and the edge tracks focus/error/idle.
#[test]
fn field_chrome_owns_label_box_and_error_slot() {
    let (host, _) = mount_chrome("", false);
    // 18px icon slot, exactly one text line tall band.
    let slot = host
        .committed_box(node_by_debug(&host, "fc-slot"))
        .expect("slot");
    assert!(
        approx(slot.w, 18.0) && approx(slot.h, 20.0),
        "slot 18x20, got {}x{}",
        slot.w,
        slot.h
    );
    // Chrome pads the content band 14px top/bottom.
    let chrome = host
        .committed_box(node_by_debug(&host, "fc"))
        .expect("chrome");
    assert!(
        approx(chrome.h, slot.h + 28.0),
        "box reads content + pads, got {}",
        chrome.h
    );
    // Label insets 5px from the chrome's left edge (the pad shifts
    // the text, never the row — so the text leaf proves it).
    let text_ids = find_retained_by_debug(&host, "text");
    let label_id = host
        .with_retained_mut(|rec, _| {
            text_ids.into_iter().find(|id| {
                rec.get(*id)
                    .and_then(|n| n.text.clone())
                    .is_some_and(|t| &*t == "Email address")
            })
        })
        .expect("label leaf");
    let label = host.committed_box(label_id).expect("label box");
    assert!(
        approx(label.x - chrome.x, 5.0),
        "label insets 5px, got {}",
        label.x - chrome.x
    );
    // Idle edge is the default border.
    assert_eq!(chrome_edge(&host), Color(0xFF_3A_3B_42));
    // The reserved slot renders idle (form never moves).
    assert!(
        !find_retained_by_debug(&host, "fc-err").is_empty(),
        "reserved error slot renders idle"
    );
}

/// Focus blues the edge; an error reddens it and shows the message.
#[test]
fn field_chrome_edge_tracks_focus_and_error() {
    let (host, _) = mount_chrome("", true);
    assert_eq!(
        chrome_edge(&host),
        Color(0xFF_54_8D_EB),
        "focus blues the edge"
    );
    // Error state: message line renders under the box.
    let (host, err) = mount_chrome("", false);
    err.set(SharedString::from("Bad email"));
    host.run_until_idle();
    assert_eq!(
        chrome_edge(&host),
        host.theme_tokens().error,
        "error reddens the edge"
    );
    let err_ids = find_retained_by_debug(&host, "fc-err");
    assert_eq!(err_ids.len(), 1, "one error slot");
    let shown = host.with_retained_mut(|rec, _| {
        let mut stack = vec![err_ids[0]];
        while let Some(cur) = stack.pop() {
            if let Some(node) = rec.get(cur) {
                if let Some(ref text) = node.text {
                    if !text.trim().is_empty() {
                        return Some(text.clone());
                    }
                }
                stack.extend(node.children.iter().copied());
            }
        }
        None
    });
    assert_eq!(shown.as_deref(), Some("Bad email"), "message shows");
}
