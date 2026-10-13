use super::*;

#[test]
fn routed_keys_map_unmapped_count() {
    use winit::keyboard::KeyCode;
    let mut shell = shell();
    shell.push_event(LinuxEvent::KeyDown {
        code: KeyCode::Escape,
    });
    shell.push_event(LinuxEvent::KeyUp {
        code: KeyCode::Escape,
    });
    // Shortcut letters route since decision 246 (the runner acts
    // only while ctrl is held); F1 stays the unmapped probe.
    shell.push_event(LinuxEvent::KeyDown {
        code: KeyCode::KeyA,
    });
    shell.push_event(LinuxEvent::KeyDown { code: KeyCode::F1 });
    let (cmds, errors) = pump(&mut shell);
    assert!(errors.is_empty());
    assert_eq!(
        cmds,
        vec![
            LinuxCmd::Key {
                code: oppa::input::keys::ESCAPE,
                pressed: true
            },
            LinuxCmd::Key {
                code: oppa::input::keys::ESCAPE,
                pressed: false
            },
            LinuxCmd::Key {
                code: oppa::input::keys::A,
                pressed: true
            },
        ]
    );
    assert_eq!(shell.stats.ignored_keys, 1);
}

#[test]
fn keycode_table_covers_routed_set() {
    use winit::keyboard::KeyCode;
    assert_eq!(
        keycode_to_framework(KeyCode::Escape),
        Some(oppa::input::keys::ESCAPE)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::Enter),
        Some(oppa::input::keys::ENTER)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::NumpadEnter),
        Some(oppa::input::keys::ENTER)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::Space),
        Some(oppa::input::keys::SPACE)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::Tab),
        Some(oppa::input::keys::TAB)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::Backspace),
        Some(oppa::input::keys::BACKSPACE)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::Delete),
        Some(oppa::input::keys::DELETE)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::KeyA),
        Some(oppa::input::keys::A)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::KeyC),
        Some(oppa::input::keys::C)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::KeyV),
        Some(oppa::input::keys::V)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::KeyX),
        Some(oppa::input::keys::X)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::KeyY),
        Some(oppa::input::keys::Y)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::KeyZ),
        Some(oppa::input::keys::Z)
    );
    // Round 5.3 arrows ride the routed set (physical codes).
    assert_eq!(
        keycode_to_framework(KeyCode::ArrowLeft),
        Some(oppa::input::keys::LEFT)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::ArrowUp),
        Some(oppa::input::keys::UP)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::ArrowRight),
        Some(oppa::input::keys::RIGHT)
    );
    assert_eq!(
        keycode_to_framework(KeyCode::ArrowDown),
        Some(oppa::input::keys::DOWN)
    );
    assert_eq!(keycode_to_framework(KeyCode::KeyQ), None);
    assert_eq!(keycode_to_framework(KeyCode::F1), None);
}

#[test]
fn modifiers_state_samples() {
    use winit::keyboard::ModifiersState;
    let m = translate_modifiers(ModifiersState::SHIFT);
    assert!(m.shift && !m.ctrl && !m.alt && !m.meta);
    let m = translate_modifiers(ModifiersState::empty());
    assert_eq!(m, Modifiers::NONE);
}
