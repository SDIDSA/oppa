#![cfg(windows)]

use super::*;
use crate::windows::{drive_cmd, WinImeMapper};
use oppa_shell_win::{Cmd, ShellConfig, Win32Shell};

fn hidden_shell() -> Win32Shell {
    Win32Shell::new(ShellConfig {
        title: "dpi-test".to_string(),
        width: 200,
        height: 150,
        record_messages: false,
        suppress_os_composition_window: true,
        visible: false,
    })
    .expect("hidden test window builds")
}

#[test]
fn wndproc_dpi_changed_snapshots_rect_and_dpi() {
    use ::windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_DPICHANGED};
    use oppa::shell::PlatformShell;
    let mut shell = hidden_shell();
    let rect = RECT {
        left: 10,
        top: 20,
        right: 810,
        bottom: 620,
    };
    let dpi = 192u32;
    unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_DPICHANGED,
            Some(WPARAM(((dpi << 16) | dpi) as usize)),
            Some(LPARAM(&rect as *const RECT as isize)),
        );
    }
    let _ = shell.pump_events();
    assert_eq!(
        shell.take_cmds(),
        vec![Cmd::DpiChanged {
            x: 10,
            y: 20,
            w: 800,
            h: 600,
            dpi: 192,
        }],
        "suggested rect + x-axis DPI snapshot at message time"
    );
}

#[test]
fn drive_cmd_dpi_changed_rebases_and_repaints() {
    let (mut loop_, _) = field_harness();
    let mut shell = hidden_shell();
    let mut ime = WinImeMapper::new();
    let out = drive_cmd(
        &mut loop_,
        &mut shell,
        &mut ime,
        Cmd::DpiChanged {
            x: 0,
            y: 0,
            w: 400,
            h: 300,
            dpi: 192,
        },
    )
    .expect("drives");
    assert!(out.damaged, "repaint follows the re-base");
    assert_eq!(loop_.dpr(), 2.0, "192 dpi re-bases to 2.0");
    assert_eq!(loop_.viewport(), (400, 300));
    let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
        .into_iter()
        .next()
        .expect("field box");
    let box_ = loop_.host().committed_box(field).expect("field box");
    assert_eq!((box_.w, box_.h), (400.0, 64.0), "crisp at 2x: {box_:?}");
    assert_eq!(loop_.rgba8().expect("pixels").len(), 400 * 300 * 4);
}
