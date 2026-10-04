use super::*;

/// Round 2.1 (decision 256): winit `Ime` events need no live
/// `DeviceId`, so `translate` is covered here — the one crack in
/// the "mechanically untested" wall.
#[test]
fn ime_translate_maps_preedit_commit_and_disabled() {
    use winit::event::{Ime, WindowEvent};
    let mut cursor = (0.0f32, 0.0f32);
    // Non-empty preedit with a byte-wise cursor routes.
    assert_eq!(
        translate(
            &WindowEvent::Ime(Ime::Preedit("nihao".to_string(), Some((5, 5)))),
            &mut cursor,
            1.0,
        ),
        vec![LinuxEvent::ImePreedit {
            text: "nihao".to_string(),
            cursor: Some((5, 5)),
        }]
    );
    // Empty preedit stays unmapped (winit sends one before every
    // commit — the commit below carries the text).
    assert_eq!(
        translate(
            &WindowEvent::Ime(Ime::Preedit(String::new(), None)),
            &mut cursor,
            1.0,
        ),
        Vec::<LinuxEvent>::new()
    );
    assert_eq!(
        translate(
            &WindowEvent::Ime(Ime::Commit("你好".to_string())),
            &mut cursor,
            1.0,
        ),
        vec![LinuxEvent::ImeCommit {
            text: "你好".to_string(),
        }]
    );
    assert_eq!(
        translate(&WindowEvent::Ime(Ime::Disabled), &mut cursor, 1.0),
        vec![LinuxEvent::ImeDisabled]
    );
    assert_eq!(
        translate(&WindowEvent::Ime(Ime::Enabled), &mut cursor, 1.0),
        Vec::<LinuxEvent>::new(),
        "Enabled carries nothing to route"
    );
}

#[test]
fn ime_events_classify_to_runner_matched_commands() {
    let mut shell = shell();
    shell.push_event(LinuxEvent::ImePreedit {
        text: "ni".to_string(),
        cursor: Some((2, 2)),
    });
    shell.push_event(LinuxEvent::ImeCommit {
        text: "你".to_string(),
    });
    shell.push_event(LinuxEvent::ImeDisabled);
    let (cmds, errors) = pump(&mut shell);
    assert!(errors.is_empty());
    assert_eq!(
        cmds,
        vec![
            LinuxCmd::ImePreedit {
                text: "ni".to_string(),
                cursor: Some((2, 2)),
            },
            LinuxCmd::ImeCommit {
                text: "你".to_string(),
            },
            LinuxCmd::ImeCancel,
        ]
    );
    assert_eq!(shell.stats.ime, 3);
}

#[test]
#[should_panic(expected = "must match ImePreedit first")]
fn ime_preedit_refuses_pipeline_mapping_loudly() {
    LinuxCmd::ImePreedit {
        text: "ni".to_string(),
        cursor: None,
    }
    .to_input_event(Modifiers::NONE);
}

#[test]
#[should_panic(expected = "must match ImeCommit first")]
fn ime_commit_refuses_pipeline_mapping_loudly() {
    LinuxCmd::ImeCommit {
        text: "你".to_string(),
    }
    .to_input_event(Modifiers::NONE);
}

#[test]
#[should_panic(expected = "must match ImeCancel first")]
fn ime_cancel_refuses_pipeline_mapping_loudly() {
    LinuxCmd::ImeCancel.to_input_event(Modifiers::NONE);
}
