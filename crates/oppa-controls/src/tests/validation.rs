use super::*;
use oppa::input::{keys, InputEvent, KeyState};
use oppa::{find_retained_by_debug, ComponentHost, Ctx, Props, SharedString, Text, VNode};

// ------------------------------------------------------------------
// Phase 38a (decisions 366–367): form validation + slider keys
// ------------------------------------------------------------------

#[derive(Clone)]
struct ValidationFieldCase {
    props: TextInputProps,
}
impl Props for ValidationFieldCase {}

fn validation_field_render(ctx: &Ctx, p: &ValidationFieldCase) -> VNode {
    ctx.child("oppa::TextInput", 7, &p.props, TextInput)
}

/// G7: invalid/required/error_message announce through Semantics;
/// the footer stacks under the field only when a message is
/// present (valid fields keep the exact tree — no wrapper).
#[test]
fn validation_marks_announce_and_footer_stacks() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from(""));
    let props = TextInputProps {
        label: SharedString::from("Age"),
        value: value.clone(),
        placeholder: None,
        placeholder_ink: None,
        enabled: true,
        width: 200.0,
        height: 32.0,
        style: Text::body_secondary,
        debug: SharedString::from("text-input"),
        on_change: None,
        masked: false,
        invalid: true,
        required: true,
        error_message: Some(SharedString::from("err-age")),
        helper_text: Some(SharedString::from("Years, digits only")),
        borderless: false,
        icon: None,
        icon_size: 16.0,
        focused: None,
    };
    host.mount(
        "Val",
        ValidationFieldCase { props },
        validation_field_render,
    );
    host.run_until_idle();
    let id = node_by_debug(&host, "text-input");
    let sem = host.retained_semantics(id).expect("semantics");
    assert!(sem.invalid, "invalid announces");
    assert!(sem.required, "required announces");
    assert_eq!(
        sem.error_message.as_deref(),
        Some("err-age"),
        "error node identity announces"
    );
    // Error wins over helper while invalid; the wrapper stacks it.
    assert_eq!(
        find_retained_by_debug(&host, "text-input-field").len(),
        1,
        "footer wrapper stacks under the field"
    );
    // Helper-only (valid): footer renders, marks stay off.
    let host2 = ComponentHost::new();
    host2.set_text_service(Box::new(FakeText));
    let value2 = host2.runtime().signal(SharedString::from(""));
    let props2 = TextInputProps {
        label: SharedString::from("Age"),
        value: value2.clone(),
        placeholder: None,
        placeholder_ink: None,
        enabled: true,
        width: 200.0,
        height: 32.0,
        style: Text::body_secondary,
        debug: SharedString::from("text-input"),
        on_change: None,
        masked: false,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: Some(SharedString::from("Years, digits only")),
        borderless: false,
        icon: None,
        icon_size: 16.0,
        focused: None,
    };
    host2.mount(
        "ValHelp",
        ValidationFieldCase { props: props2 },
        validation_field_render,
    );
    host2.run_until_idle();
    let id2 = node_by_debug(&host2, "text-input");
    let sem2 = host2.retained_semantics(id2).expect("semantics");
    assert!(!sem2.invalid);
    assert!(!sem2.required);
    assert_eq!(sem2.error_message, None);
    assert_eq!(
        find_retained_by_debug(&host2, "text-input-field").len(),
        1,
        "helper-only still stacks the caption"
    );
    // Plain (no message): no wrapper — the pre-38a tree exactly.
    let host3 = ComponentHost::new();
    host3.set_text_service(Box::new(FakeText));
    let value3 = host3.runtime().signal(SharedString::from(""));
    host3.mount(
        "ValPlain",
        ValidationFieldCase {
            props: TextInputProps::new("Age", value3),
        },
        validation_field_render,
    );
    host3.run_until_idle();
    assert!(
        find_retained_by_debug(&host3, "text-input-field").is_empty(),
        "valid fields keep the exact tree"
    );
}

/// G7 across the catalog: Checkbox, Toggle, Select, and
/// RadioGroup carry the same four marks (validators stay
/// app-side — the payload only announces).
#[test]
fn checkbox_toggle_select_radiogroup_carry_validation() {
    // Checkbox.
    let host = ComponentHost::new();
    let checked = host.runtime().signal(false);
    host.mount(
        "ChkV",
        CheckCase {
            props: CheckboxProps {
                label: SharedString::from("T&C"),
                checked: checked.clone(),
                enabled: true,
                on_change: None,
                invalid: true,
                required: true,
                error_message: Some(SharedString::from("err-tc")),
                helper_text: None,
                box_px: 20.0,
                box_radius: 4.0,
                label_style: oppa::Text::body_secondary,
                label_ink: None,
                check_ink: None,
                check_scale: 1.0,
            },
        },
        check_render,
    );
    host.run_until_idle();
    let sem = host
        .retained_semantics(node_by_debug(&host, "checkbox"))
        .expect("semantics");
    assert!(sem.invalid && sem.required);
    assert_eq!(sem.error_message.as_deref(), Some("err-tc"));
    assert_eq!(find_retained_by_debug(&host, "checkbox-field").len(), 1);
    // Toggle.
    let host2 = ComponentHost::new();
    let on = host2.runtime().signal(false);
    host2.mount(
        "TglV",
        ToggleCase {
            props: ToggleProps {
                label: SharedString::from("Wi-Fi"),
                on: on.clone(),
                enabled: true,
                on_change: None,
                invalid: true,
                required: false,
                error_message: Some(SharedString::from("err-wifi")),
                helper_text: None,
            },
        },
        toggle_render,
    );
    host2.run_until_idle();
    let sem2 = host2
        .retained_semantics(node_by_debug(&host2, "toggle"))
        .expect("semantics");
    assert!(sem2.invalid);
    assert_eq!(sem2.error_message.as_deref(), Some("err-wifi"));
    assert_eq!(find_retained_by_debug(&host2, "toggle-field").len(), 1);
    // Select.
    let host3 = ComponentHost::new();
    let sel = host3.runtime().signal("a".to_string());
    let open = host3.runtime().signal(false);
    let mut sp = SelectProps::new(
        vec![SelectItem {
            value: "a".to_string(),
            label: SharedString::from("Alpha"),
        }],
        sel,
        open,
    );
    sp.invalid = true;
    sp.required = true;
    sp.error_message = Some(SharedString::from("err-sel"));
    host3.mount("SelV", sp, Select::<String>);
    host3.run_until_idle();
    let sem3 = host3
        .retained_semantics(node_by_debug(&host3, "select-box"))
        .expect("semantics");
    assert!(sem3.invalid && sem3.required);
    assert_eq!(sem3.error_message.as_deref(), Some("err-sel"));
    assert_eq!(find_retained_by_debug(&host3, "select-field").len(), 1);
    // RadioGroup.
    let host4 = ComponentHost::new();
    let gsel = host4.runtime().signal(SharedString::from("Free"));
    host4.mount(
        "GV",
        RadioGroupProps {
            options: vec![RadioOption {
                value: SharedString::from("Free"),
                label: SharedString::from("Free"),
            }],
            selected: gsel,
            enabled: true,
            invalid: true,
            required: true,
            error_message: Some(SharedString::from("err-plan")),
            helper_text: None,
        },
        RadioGroup::<SharedString>,
    );
    host4.run_until_idle();
    assert_eq!(
        find_retained_by_debug(&host4, "radio-group-field").len(),
        1,
        "group footer stacks under the options"
    );
}

/// Decision 367: the numeric range announces through value_num /
/// min_value / max_value (G18) next to the human value_text; Home
/// jumps to min, End to max (held arrows keep repeat-stepping).
#[test]
fn slider_home_end_jump_and_range_announces() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(50.0f32);
    let props = SliderProps {
        label: SharedString::from("Volume"),
        value: value.clone(),
        min: 0.0,
        max: 100.0,
        step: 10.0,
        enabled: true,
        on_change: None,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
    };
    host.mount("SldHE", SliderCase { props }, slider_render);
    host.run_until_idle();
    let track = node_by_debug(&host, "slider");
    let sem = host.retained_semantics(track).expect("semantics");
    assert_eq!(sem.value_text.as_deref(), Some("50 percent"));
    assert_eq!(sem.value_num, Some(oppa::Num::of(50.0)));
    assert_eq!(sem.min_value, Some(oppa::Num::of(0.0)));
    assert_eq!(sem.max_value, Some(oppa::Num::of(100.0)));
    // Focus the track (press owns capture + focus), then jump.
    press_node(&host, track);
    assert_eq!(host.focused_node(), Some(track));
    host.inject_input(InputEvent::key(keys::HOME, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(value.get(), 0.0, "Home jumps to min");
    host.inject_input(InputEvent::key(keys::END, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(value.get(), 100.0, "End jumps to max");
    // Arrows still step from the jumped ends.
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(value.get(), 90.0, "Left steps down from max");
}
