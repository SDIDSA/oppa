use super::*;

#[test]
fn wheel_routes_to_positioned_scroll_cmd() {
    // Decision 250 retires the ignore: wheel classifies to a
    // positioned Scroll (the runner hit-tests the target — the
    // shell still fabricates no NodeId). Test shell runs density
    // 2 (10dp → 20px, like the touch test above).
    let mut shell = shell();
    shell.push_event(LinuxEvent::Wheel {
        x_dp: 10.0,
        y_dp: 12.0,
        dx: 0.0,
        dy: 30.0,
    });
    let (cmds, _) = pump(&mut shell);
    assert_eq!(
        cmds,
        vec![LinuxCmd::Scroll {
            x: 20.0,
            y: 24.0,
            dx: 0.0,
            dy: 60.0
        }]
    );
}

#[test]
fn shift_wheel_reroutes_vertical_as_horizontal() {
    // Phase 36 PR2b (decision 354): a Shift-held purely-vertical
    // tick arrives as `dx` (native convention); device-native
    // `dx` rides untouched under Shift.
    let mut shell = shell();
    shell.push_event(LinuxEvent::ModifiersChanged {
        shift: true,
        ctrl: false,
        alt: false,
        meta: false,
    });
    shell.push_event(LinuxEvent::Wheel {
        x_dp: 10.0,
        y_dp: 12.0,
        dx: 0.0,
        dy: 30.0,
    });
    shell.push_event(LinuxEvent::Wheel {
        x_dp: 10.0,
        y_dp: 12.0,
        dx: 6.0,
        dy: 30.0,
    });
    let (cmds, _) = pump(&mut shell);
    assert_eq!(
        cmds,
        vec![
            LinuxCmd::Scroll {
                x: 20.0,
                y: 24.0,
                dx: 60.0,
                dy: 0.0
            },
            LinuxCmd::Scroll {
                x: 20.0,
                y: 24.0,
                dx: 12.0,
                dy: 60.0
            },
        ]
    );
}
