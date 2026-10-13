use super::*;
use oppa::input::InputEvent;
use oppa::{Color, Column, ComponentHost, Ctx, CursorIcon, Props, SharedString, VNode};

/// Round 8.3 (decision 299): hover resolves the hand over the
/// button (even over its label leaf — inherit rule), the I-beam
/// over the field, and the arrow with no hover; the retained
/// styles carry the cursors structurally.
#[derive(Clone)]
struct CursorCase {
    btn: ButtonProps,
    field: TextInputProps,
}
impl Props for CursorCase {}

fn cursor_render(ctx: &Ctx, p: &CursorCase) -> VNode {
    Column::new().children([
        ctx.child("oppa::Button", 1, &p.btn, Button),
        ctx.child("oppa::TextInput", 2, &p.field, TextInput),
    ])
}

fn hover_at(host: &ComponentHost, debug: &str) {
    let id = node_by_debug(host, debug);
    let b = host.committed_box(id).expect("control has a hit box");
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Move,
        x: b.x + b.w / 2.0,
        y: b.y + b.h / 2.0,
        modifiers: oppa::Modifiers::NONE,
    });
    host.run_until_idle();
}

fn style_cursor_of(host: &ComponentHost, debug: &str) -> Option<CursorIcon> {
    let id = node_by_debug(host, debug);
    host.with_retained_mut(|rec, styles| {
        let n = rec.get(id).expect("live node");
        styles.get(n.style).cloned().unwrap_or_default().cursor
    })
}

#[test]
fn hover_resolves_button_pointer_and_field_text() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    assert_eq!(host.hover_cursor(), None, "no hover yet");
    let value = host.runtime().signal(SharedString::from("Ada"));
    host.mount(
        "Cur",
        CursorCase {
            btn: ButtonProps::new("OK", || {}),
            field: TextInputProps::new("Name", value),
        },
        cursor_render,
    );
    host.run_until_idle();
    hover_at(&host, "button");
    assert_eq!(
        host.hover_cursor(),
        Some(CursorIcon::Pointer),
        "hand over the button (label inherit)"
    );
    hover_at(&host, "text-input");
    assert_eq!(
        host.hover_cursor(),
        Some(CursorIcon::Text),
        "I-beam over the field"
    );
    assert_eq!(style_cursor_of(&host, "button"), Some(CursorIcon::Pointer));
    assert_eq!(style_cursor_of(&host, "text-input"), Some(CursorIcon::Text));
}

/// Round 8.3 (decision 299): disabled controls keep the platform
/// arrow (inert controls never promise clicks); Toggle and
/// Checkbox show the hand like Button when enabled.
#[test]
fn disabled_controls_keep_the_platform_arrow() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("Ada"));
    host.mount(
        "CurDis",
        CursorCase {
            btn: ButtonProps::new("OK", || {}).disabled(),
            field: TextInputProps::new("Name", value).disabled(),
        },
        cursor_render,
    );
    host.run_until_idle();
    assert_eq!(style_cursor_of(&host, "button"), None);
    assert_eq!(style_cursor_of(&host, "text-input"), None);
    hover_at(&host, "button");
    assert_eq!(host.hover_cursor(), None, "disabled button keeps the arrow");

    let host2 = ComponentHost::new();
    host2.set_text_service(Box::new(FakeText));
    let on = host2.runtime().signal(false);
    let checked = host2.runtime().signal(false);
    host2.mount(
        "CurTC",
        CursorTCProps {
            toggle: ToggleProps {
                label: SharedString::from("Wi-Fi"),
                on: on.clone(),
                enabled: true,
                on_change: None,
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
            check: CheckboxProps {
                label: SharedString::from("T&C"),
                checked: checked.clone(),
                enabled: true,
                on_change: None,
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
                box_px: 20.0,
                box_radius: 4.0,
                label_style: oppa::Text::body_secondary,
                label_ink: None,
                check_ink: None,
                check_scale: 1.0,
            },
        },
        cursor_tc_render,
    );
    host2.run_until_idle();
    hover_at(&host2, "toggle");
    assert_eq!(host2.hover_cursor(), Some(CursorIcon::Pointer));
    hover_at(&host2, "checkbox");
    assert_eq!(host2.hover_cursor(), Some(CursorIcon::Pointer));
}

#[derive(Clone)]
struct CursorTCProps {
    toggle: ToggleProps,
    check: CheckboxProps,
}
impl Props for CursorTCProps {}

fn cursor_tc_render(ctx: &Ctx, p: &CursorTCProps) -> VNode {
    Column::new().children([
        ctx.child("oppa::Toggle", 1, &p.toggle, Toggle),
        ctx.child("oppa::Checkbox", 2, &p.check, Checkbox),
    ])
}

/// Round 11.2 (decision 306): toggling the theme signal recolors
/// Button, TextInput, Toggle, and Modal in place — same
/// instances (toggle state and field text survive), only colors
/// re-derive.
#[derive(Clone)]
struct ThemeCase {
    btn: ButtonProps,
    field: TextInputProps,
    toggle: ToggleProps,
    modal: ModalProps,
}
impl Props for ThemeCase {}

fn theme_render(ctx: &Ctx, p: &ThemeCase) -> VNode {
    Column::new().children([
        ctx.child("oppa::Button", 1, &p.btn, Button),
        ctx.child("oppa::TextInput", 2, &p.field, TextInput),
        ctx.child("oppa::Toggle", 3, &p.toggle, Toggle),
        ctx.child("oppa::Modal", 4, &p.modal, Modal),
    ])
}

fn style_bg_of(host: &ComponentHost, debug: &str) -> Option<Color> {
    let id = node_by_debug(host, debug);
    host.with_retained_mut(|rec, styles| {
        let n = rec.get(id).expect("live node");
        styles.get(n.style).cloned().unwrap_or_default().bg
    })
}

#[test]
fn theme_toggle_recolors_controls_without_remounting() {
    use oppa::{ThemeMode, ThemeTokens};
    let light = ThemeTokens::light();
    let dark = ThemeTokens::dark();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hi"));
    let on = host.runtime().signal(true);
    let open = host.runtime().signal(true);
    host.mount(
        "Theme",
        ThemeCase {
            btn: ButtonProps::new("OK", || {}),
            field: TextInputProps::new("Name", value.clone()),
            toggle: ToggleProps {
                label: SharedString::from("Wi-Fi"),
                on: on.clone(),
                enabled: true,
                on_change: None,
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
            modal: ModalProps::new("Delete file?", open.clone()),
        },
        theme_render,
    );
    host.run_until_idle();
    // Light reproduces the catalog palette exactly.
    assert_eq!(style_bg_of(&host, "button"), Some(light.primary));
    assert_eq!(style_bg_of(&host, "text-input"), Some(light.surface));
    assert_eq!(style_bg_of(&host, "modal-card"), Some(light.surface));
    assert_eq!(
        style_bg_of(&host, "toggle-track"),
        Some(light.primary),
        "on-state track reads primary"
    );
    // Toggle to dark: every themed control re-renders in place.
    host.set_theme(ThemeMode::Dark);
    host.run_until_idle();
    assert_eq!(style_bg_of(&host, "button"), Some(dark.primary));
    assert_eq!(style_bg_of(&host, "text-input"), Some(dark.surface));
    assert_eq!(style_bg_of(&host, "modal-card"), Some(dark.surface));
    assert_eq!(style_bg_of(&host, "toggle-track"), Some(dark.primary));
    // Instances survive: toggle state and field text untouched.
    assert!(on.get(), "toggle state survives the recolor");
    assert_eq!(value.get().to_string(), "hi", "field text survives");
    assert!(open.get(), "modal stays open");
    // And back: Light restores exactly.
    host.set_theme(ThemeMode::Light);
    host.run_until_idle();
    assert_eq!(style_bg_of(&host, "button"), Some(light.primary));
    assert_eq!(style_bg_of(&host, "modal-card"), Some(light.surface));
}
