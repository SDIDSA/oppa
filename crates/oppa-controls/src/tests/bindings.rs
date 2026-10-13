use super::*;
use oppa::input::{keys, InputEvent, KeyState};
use oppa::{ComponentHost, SharedString, Text};

#[test]
fn toggle_and_checkbox_report_flips() {
    let host = ComponentHost::new();
    let (notify, seen) = blowing::<bool>();
    let on = host.runtime().signal(false);
    host.mount(
        "Tgl",
        ToggleCase {
            props: ToggleProps {
                label: SharedString::from("Wi-Fi"),
                on: on.clone(),
                enabled: true,
                on_change: Some(notify),
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
        },
        toggle_render,
    );
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "toggle"));
    assert!(on.get());
    assert_eq!(*seen.borrow(), vec![true], "flip reports the new value");
    // Checkbox mirrors (mark + semantics already proven — the
    // notification path is the assertion here).
    let host2 = ComponentHost::new();
    let (notify2, seen2) = blowing::<bool>();
    let checked = host2.runtime().signal(false);
    host2.mount(
        "Chk",
        CheckCase {
            props: CheckboxProps {
                label: SharedString::from("T&C"),
                checked: checked.clone(),
                enabled: true,
                on_change: Some(notify2),
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
        check_render,
    );
    host2.run_until_idle();
    press_node(&host2, node_by_debug(&host2, "checkbox"));
    assert!(checked.get());
    assert_eq!(*seen2.borrow(), vec![true]);
}

#[test]
fn slider_reports_every_path() {
    // Controlled slider with notification: step, drag, and
    // arrow paths all report through the single funnel.
    let host = ComponentHost::new();
    let value = host.runtime().signal(50.0f32);
    let (notify, seen) = blowing::<f32>();
    let props = SliderProps {
        label: SharedString::from("Volume"),
        value: value.clone(),
        min: 0.0,
        max: 100.0,
        step: 10.0,
        enabled: true,
        on_change: Some(notify),
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
    };
    host.mount("Sld", SliderCase { props }, slider_render);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "step-inc"));
    assert_eq!(*seen.borrow(), vec![60.0], "step reports");
    // Drag path reports the snapped value (track-owned press
    // point — buttons own their own presses).
    let (_x0, y0, tx, tw) = press_track(&host);
    host.inject_input(InputEvent::pointer_move(tx + tw * 0.8, y0));
    host.run_until_idle();
    assert_eq!(*seen.borrow(), vec![60.0, 80.0], "drag reports");
    host.inject_input(InputEvent::pointer_up(tx + tw * 0.8, y0));
    host.run_until_idle();
    // Arrow path reports too (the drag press focused the track —
    // cancel/up keep focus, so no TAB needed).
    let track = node_by_debug(&host, "slider");
    assert_eq!(host.focused_node(), Some(track));
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(*seen.borrow(), vec![60.0, 80.0, 70.0], "arrow reports");
}

#[test]
fn uncontrolled_toggle_checkbox_slider_advance_and_report() {
    // Toggle: press flips internal state + reports.
    let host = ComponentHost::new();
    let (notify, seen) = blowing::<bool>();
    host.mount(
        "UT",
        UncontrolledToggleProps {
            label: SharedString::from("Wi-Fi"),
            initial: false,
            enabled: true,
            on_change: Some(notify),
            invalid: false,
            required: false,
            error_message: None,
            helper_text: None,
        },
        UncontrolledToggle,
    );
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "toggle"));
    assert_eq!(*seen.borrow(), vec![true], "uncontrolled flip reports");
    press_node(&host, node_by_debug(&host, "toggle"));
    assert_eq!(*seen.borrow(), vec![true, false], "second flip reports too");
    // Slider: step reports the snapped value.
    let host2 = ComponentHost::new();
    let (notify2, seen2) = blowing::<f32>();
    host2.mount(
        "US",
        UncontrolledSliderProps {
            label: SharedString::from("Volume"),
            initial: 50.0,
            min: 0.0,
            max: 100.0,
            step: 10.0,
            enabled: true,
            on_change: Some(notify2),
            invalid: false,
            required: false,
            error_message: None,
            helper_text: None,
        },
        UncontrolledSlider,
    );
    host2.run_until_idle();
    press_node(&host2, node_by_debug(&host2, "step-inc"));
    assert_eq!(*seen2.borrow(), vec![60.0]);
    // Checkbox: press reports.
    let host3 = ComponentHost::new();
    let (notify3, seen3) = blowing::<bool>();
    host3.mount(
        "UC",
        UncontrolledCheckboxProps {
            label: SharedString::from("T&C"),
            initial: false,
            enabled: true,
            on_change: Some(notify3),
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
        UncontrolledCheckbox,
    );
    host3.run_until_idle();
    press_node(&host3, node_by_debug(&host3, "checkbox"));
    assert_eq!(*seen3.borrow(), vec![true]);
}

#[test]
fn uncontrolled_text_reports_committed_edits() {
    // Typing through the focused session reports each commit
    // (the session funnel — insert here, IME commits and undo
    // ride the same path core-side).
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let (notify, seen) = blowing::<SharedString>();
    host.mount(
        "UTI",
        UncontrolledTextInputProps {
            label: SharedString::from("Name"),
            initial: SharedString::from(""),
            placeholder: None,
            placeholder_ink: None,
            enabled: true,
            width: 200.0,
            height: 32.0,
            style: Text::body_secondary,
            debug: SharedString::from("text-input"),
            on_change: Some(notify),
            masked: false,
            invalid: false,
            required: false,
            error_message: None,
            helper_text: None,
            icon: None,
            icon_size: 16.0,
            focused: None,
        },
        UncontrolledTextInput,
    );
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "text-input"));
    let sess = host.focused_field_session().expect("session");
    sess.insert("hi");
    host.run_until_idle();
    assert_eq!(
        seen.borrow()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        vec!["hi".to_string()],
        "committed edit reports"
    );
    // TextArea mirrors through the same funnel.
    let host2 = ComponentHost::new();
    host2.set_text_service(Box::new(FakeText));
    let (notify2, seen2) = blowing::<SharedString>();
    host2.mount(
        "UTA",
        UncontrolledTextAreaProps {
            label: SharedString::from("Notes"),
            initial: SharedString::from(""),
            placeholder: None,
            enabled: true,
            width: 200.0,
            style: Text::body_secondary,
            debug: SharedString::from("text-area"),
            on_change: Some(notify2),
            invalid: false,
            required: false,
            error_message: None,
            helper_text: None,
            focused: None,
        },
        UncontrolledTextArea,
    );
    host2.run_until_idle();
    press_node(&host2, node_by_debug(&host2, "text-area"));
    let sess2 = host2.focused_field_session().expect("session");
    sess2.insert("a\nb");
    host2.run_until_idle();
    assert_eq!(
        seen2
            .borrow()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        vec!["a\nb".to_string()]
    );
}
