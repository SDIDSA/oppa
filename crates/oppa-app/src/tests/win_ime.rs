#![cfg(windows)]

use super::*;
use crate::windows::ime::utf16_offset_to_byte;
use crate::windows::{drive_cmd, drive_ime, WinImeMapper};
use oppa::EditSession;
use oppa_shell_win::{ImeMessage, ShellConfig, ShellEvent, Win32Shell};
use std::rc::Rc;

fn mapper() -> WinImeMapper {
    WinImeMapper::new()
}

fn session_of(loop_: &DesktopLoop) -> EditSession {
    loop_.host().focused_field_session().expect("session")
}

fn hidden_shell() -> Win32Shell {
    Win32Shell::new(ShellConfig {
        title: "ime-test".to_string(),
        width: 200,
        height: 150,
        record_messages: false,
        suppress_os_composition_window: true,
        visible: false,
        style: oppa::WindowStyle::Decorated,
    })
    .expect("hidden test window builds")
}

#[test]
fn utf16_offsets_map_to_byte_boundaries() {
    assert_eq!(utf16_offset_to_byte("nihao", 5), 5);
    assert_eq!(utf16_offset_to_byte("nihao", 99), 5);
    // "héllo": é is 2 bytes / 1 unit — unit 2 lands on byte 3.
    assert_eq!(utf16_offset_to_byte("héllo", 2), 3);
    // Surrogate pair: 2 units, 4 bytes — mid-pair floors in.
    assert_eq!(utf16_offset_to_byte("a👍b", 2), 1);
    assert_eq!(utf16_offset_to_byte("a👍b", 3), 5);
}

#[test]
fn mapper_start_update_commit_cancel_sequence() {
    let (mut loop_, value) = field_harness();
    focus_field(&mut loop_);
    let mut ime = mapper();
    let s = session_of(&loop_);
    // Start over a collapsed caret: single Started.
    let evs = ime.map_message(&ImeMessage::StartComposition, &s);
    assert_eq!(
        evs,
        vec![oppa::ImeCompositionEvent::CompositionStarted { start_byte: 0 }]
    );
    loop_.feed_ime(&evs).expect("feeds");
    // Update while composing: bare Updated.
    let s = session_of(&loop_);
    let evs = ime.map_message(
        &ImeMessage::Composition {
            gcs_flags: 0,
            comp: Some("nihao".to_string()),
            attrs: None,
            cursor_pos: 5,
            delta_start: 0,
            result: None,
        },
        &s,
    );
    assert_eq!(
        evs,
        vec![oppa::ImeCompositionEvent::CompositionUpdated {
            composition: "nihao".to_string(),
            caret_byte: 5,
        }]
    );
    // Result commits and arms the echo swallow (UTF-16 units).
    let s = session_of(&loop_);
    let evs = ime.map_message(
        &ImeMessage::Composition {
            gcs_flags: 0,
            comp: None,
            attrs: None,
            cursor_pos: 0,
            delta_start: 0,
            result: Some("你好".to_string()),
        },
        &s,
    );
    assert_eq!(
        evs,
        vec![oppa::ImeCompositionEvent::CompositionCommitted {
            committed: "你好".to_string(),
        }]
    );
    assert_eq!(ime.swallow_pending, 2, "two BMP units held");
    loop_.feed_ime(&evs).expect("feeds");
    assert_eq!(value.get(), SharedString::from("你好"));
    // End after the commit: nothing to cancel.
    let s = session_of(&loop_);
    assert!(ime.map_message(&ImeMessage::EndComposition, &s).is_empty());
    // A fresh update-cancel round trips through cancel.
    let s = session_of(&loop_);
    let start = ime.map_message(&ImeMessage::StartComposition, &s);
    loop_.feed_ime(&start).expect("feeds");
    let s = session_of(&loop_);
    let evs = ime.map_message(&ImeMessage::EndComposition, &s);
    assert_eq!(evs, vec![oppa::ImeCompositionEvent::CompositionCancelled]);
    loop_.feed_ime(&evs).expect("feeds");
    assert_eq!(
        value.get(),
        SharedString::from("你好"),
        "cancel keeps commits"
    );
}

#[test]
fn mapper_start_replaces_the_selection_atomically() {
    let (mut loop_, value) = field_harness();
    focus_field(&mut loop_);
    loop_.type_text("hello").expect("types");
    let s = session_of(&loop_);
    s.select_all();
    let mut ime = mapper();
    let evs = ime.map_message(&ImeMessage::StartComposition, &s);
    assert_eq!(
        evs,
        vec![
            oppa::ImeCompositionEvent::CompositionStarted { start_byte: 0 },
            oppa::ImeCompositionEvent::DeleteRange { range: (0, 5) },
        ],
        "started-before-delete (pre-deletion undo snapshot)"
    );
    loop_.feed_ime(&evs).expect("feeds");
    loop_.feed_ime_commit("你好").expect("feeds");
    assert_eq!(value.get(), SharedString::from("你好"));
    session_of(&loop_).undo();
    assert_eq!(
        value.get(),
        SharedString::from("hello"),
        "undo restores pre-selection"
    );
}

#[test]
fn drive_cmd_swallows_commit_echo_but_passes_real_chars() {
    let (mut loop_, value) = field_harness();
    focus_field(&mut loop_);
    let mut shell = hidden_shell();
    let mut ime = mapper();
    // Commit "ab" (2 echo units) through the driver.
    let s = session_of(&loop_);
    let evs = ime.map_message(
        &ImeMessage::Composition {
            gcs_flags: 0,
            comp: None,
            attrs: None,
            cursor_pos: 0,
            delta_start: 0,
            result: Some("ab".to_string()),
        },
        &s,
    );
    loop_.feed_ime(&evs).expect("feeds");
    assert_eq!(value.get(), SharedString::from("ab"));
    // The two echo WM_CHARs die quietly.
    for ch in ['a', 'b'] {
        let out = drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            oppa_shell_win::Cmd::Char { ch },
        )
        .expect("drives");
        assert!(!out.damaged, "echo paints nothing");
    }
    assert_eq!(value.get(), SharedString::from("ab"));
    assert_eq!(ime.swallow_pending, 0, "echo consumed");
    // A real keystroke clears nothing pending (already zero)
    // and its char inserts.
    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        oppa_shell_win::Cmd::Key {
            vk: oppa::input::keys::A,
            shift: false,
            ctrl: false,
        },
    )
    .expect("drives");
    drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        oppa_shell_win::Cmd::Char { ch: 'c' },
    )
    .expect("drives");
    assert_eq!(value.get(), SharedString::from("abc"));
}

#[test]
fn drive_ime_end_to_end_feeds_and_anchors() {
    let (mut loop_, value) = field_harness();
    focus_field(&mut loop_);
    let mut shell = hidden_shell();
    let mut ime = mapper();
    let seq = [
        ImeMessage::StartComposition,
        ImeMessage::Composition {
            gcs_flags: 0,
            comp: Some("ni".to_string()),
            attrs: None,
            cursor_pos: 2,
            delta_start: 0,
            result: None,
        },
        ImeMessage::Composition {
            gcs_flags: 0,
            comp: None,
            attrs: None,
            cursor_pos: 0,
            delta_start: 0,
            result: Some("你".to_string()),
        },
        ImeMessage::EndComposition,
    ];
    let mut damaged = false;
    for msg in &seq {
        damaged |= drive_ime(&mut loop_, &mut shell, &mut ime, msg).expect("drives");
    }
    assert!(damaged, "composition damages");
    assert_eq!(value.get(), SharedString::from("你"));
    // The candidate anchor tracked the session (run-relative
    // rect at the trailing caret of one CJK char).
    let anchor = loop_.ime_anchor().expect("anchor after commit");
    assert_eq!(anchor[0], 8.75, "one FakeText advance, got {anchor:?}");
    let anchored = shell.take_anchored_rects();
    assert!(!anchored.is_empty(), "shell anchored the candidate");
    assert_eq!(
        anchored.last().unwrap()[0],
        anchor[0],
        "shell saw the live rect, got {anchored:?}"
    );
    session_of(&loop_).undo();
    assert_eq!(value.get(), SharedString::from(""));
}

#[test]
fn pump_moves_queue_to_cmds_and_fires_the_ime_callback() {
    use oppa::shell::PlatformShell;
    let mut shell = hidden_shell();
    let seen: Rc<std::cell::RefCell<Vec<ImeMessage>>> =
        Rc::new(std::cell::RefCell::new(Vec::new()));
    {
        let seen = seen.clone();
        shell.set_ime_callback(Rc::new(move |msg| seen.borrow_mut().push(msg.clone())));
    }
    shell
        .shared
        .borrow_mut()
        .queue
        .push_back(ShellEvent::Char { ch: 'a' });
    shell
        .shared
        .borrow_mut()
        .queue
        .push_back(ShellEvent::Ime(ImeMessage::StartComposition));
    let _ = shell.pump_events();
    assert_eq!(
        *seen.borrow(),
        vec![ImeMessage::StartComposition],
        "callback fires for IME messages"
    );
    assert_eq!(
        shell.take_cmds(),
        vec![oppa_shell_win::Cmd::Char { ch: 'a' }],
        "chars pair to commands after the pump (the missing-pump fix)"
    );
}
