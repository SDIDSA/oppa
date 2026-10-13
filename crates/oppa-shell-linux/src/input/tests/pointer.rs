use super::*;

use oppa::InputEvent;

#[test]
fn click_maps_dp_to_px() {
    let mut shell = shell();
    shell.push_event(LinuxEvent::MouseDown {
        button: oppa::PointerButton::Primary,
        x_dp: 10.0,
        y_dp: 12.0,
    });
    shell.push_event(LinuxEvent::MouseUp {
        button: oppa::PointerButton::Primary,
        x_dp: 10.0,
        y_dp: 12.0,
    });
    let (cmds, errors) = pump(&mut shell);
    assert!(errors.is_empty());
    assert_eq!(
        cmds,
        vec![
            LinuxCmd::PointerDown {
                id: 0,
                button: oppa::PointerButton::Primary,
                x: 20.0,
                y: 24.0
            },
            LinuxCmd::PointerUp {
                id: 0,
                button: oppa::PointerButton::Primary,
                x: 20.0,
                y: 24.0
            },
        ]
    );
    assert_eq!(shell.stats.mouse, 2);
}

/// Round 9.2 (decision 301): right/middle classify to
/// secondary/auxiliary (left stays primary); the pipeline event
/// carries the taxonomy into the router's tap dispatch.
#[test]
fn mouse_buttons_classify_to_taxonomy() {
    let mut shell = shell();
    for button in [
        oppa::PointerButton::Secondary,
        oppa::PointerButton::Auxiliary,
    ] {
        shell.push_event(LinuxEvent::MouseDown {
            button,
            x_dp: 10.0,
            y_dp: 12.0,
        });
        shell.push_event(LinuxEvent::MouseUp {
            button,
            x_dp: 10.0,
            y_dp: 12.0,
        });
    }
    let (cmds, errors) = pump(&mut shell);
    assert!(errors.is_empty());
    assert_eq!(
        cmds,
        vec![
            LinuxCmd::PointerDown {
                id: 0,
                button: oppa::PointerButton::Secondary,
                x: 20.0,
                y: 24.0
            },
            LinuxCmd::PointerUp {
                id: 0,
                button: oppa::PointerButton::Secondary,
                x: 20.0,
                y: 24.0
            },
            LinuxCmd::PointerDown {
                id: 0,
                button: oppa::PointerButton::Auxiliary,
                x: 20.0,
                y: 24.0
            },
            LinuxCmd::PointerUp {
                id: 0,
                button: oppa::PointerButton::Auxiliary,
                x: 20.0,
                y: 24.0
            },
        ]
    );
    // The pipeline event carries the button into the router.
    let ev = LinuxCmd::PointerDown {
        id: 0,
        button: oppa::PointerButton::Secondary,
        x: 20.0,
        y: 24.0,
    }
    .to_input_event(Modifiers::NONE);
    assert!(matches!(
        ev,
        InputEvent::Pointer {
            action: oppa::PointerAction::Down {
                button: oppa::PointerButton::Secondary
            },
            ..
        }
    ));
}

#[test]
fn second_touch_routes_with_its_own_id() {
    // G11 retires the v1 refusal: index 1 routes as pointer id 1
    // (the router holds one capture per id — decision 227).
    let mut shell = shell();
    shell.push_event(LinuxEvent::TouchDown {
        index: 1,
        x_dp: 5.0,
        y_dp: 5.0,
    });
    shell.push_event(LinuxEvent::TouchMove {
        index: 1,
        x_dp: 6.0,
        y_dp: 6.0,
    });
    shell.push_event(LinuxEvent::TouchUp {
        index: 1,
        x_dp: 6.0,
        y_dp: 6.0,
    });
    let (cmds, errors) = pump(&mut shell);
    assert!(errors.is_empty(), "nothing refuses anymore");
    assert_eq!(
        cmds,
        vec![
            LinuxCmd::PointerDown {
                id: 1,
                button: oppa::PointerButton::Primary,
                x: 10.0,
                y: 10.0
            },
            LinuxCmd::PointerMove {
                id: 1,
                x: 12.0,
                y: 12.0
            },
            LinuxCmd::PointerUp {
                id: 1,
                button: oppa::PointerButton::Primary,
                x: 12.0,
                y: 12.0
            },
        ]
    );
    assert_eq!(shell.stats.touches, 3);
}
#[test]
fn touch_up_needs_no_position() {
    // Lift carries the last known position (a lift at (0,0)
    // would misroute to the origin — the spurious-flip class).
    let mut shell = shell();
    shell.push_event(LinuxEvent::TouchDown {
        index: 0,
        x_dp: 10.0,
        y_dp: 12.0,
    });
    shell.push_event(LinuxEvent::TouchUp {
        index: 0,
        x_dp: 10.0,
        y_dp: 12.0,
    });
    let (cmds, _) = pump(&mut shell);
    assert_eq!(
        cmds,
        vec![
            LinuxCmd::PointerDown {
                id: 0,
                button: oppa::PointerButton::Primary,
                x: 20.0,
                y: 24.0
            },
            LinuxCmd::PointerUp {
                id: 0,
                button: oppa::PointerButton::Primary,
                x: 20.0,
                y: 24.0
            },
        ]
    );
}
