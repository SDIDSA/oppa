use super::*;
use oppa::shell::PlatformShell;

fn hidden_shell() -> Win32Shell {
    Win32Shell::new(ShellConfig {
        title: "resize-test".to_string(),
        width: 200,
        height: 150,
        record_messages: false,
        suppress_os_composition_window: true,
        visible: false,
        style: oppa::WindowStyle::Decorated,
    })
    .expect("hidden test window builds")
}

fn send_size(shell: &Win32Shell, size_type: usize, w: i32, h: i32) {
    use ::windows::Win32::Foundation::{LPARAM, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SIZE};
    unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_SIZE,
            Some(WPARAM(size_type)),
            Some(LPARAM(((h << 16) | w) as isize)),
        );
    }
}

/// Round 7.20 (decision 295): a registered resize callback fires
/// synchronously from `WM_SIZE` with the exact client size, while
/// minimized and zero sizes stay quiet (no surface exists for
/// them).
#[test]
fn resize_callback_fires_on_size_with_exact_px() {
    let mut shell = hidden_shell();
    let seen: Rc<RefCell<Vec<(u32, u32)>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let seen = seen.clone();
        shell.set_resize_callback(Rc::new(move |w, h| seen.borrow_mut().push((w, h))));
    }
    // SIZE_RESTORED (0): the live-drag step shape.
    send_size(&shell, 0, 800, 600);
    // SIZE_MAXIMIZED (2): a jump, not a drag — still a resize.
    send_size(&shell, 2, 1024, 768);
    assert_eq!(*seen.borrow(), vec![(800, 600), (1024, 768)]);
    // SIZE_MINIMIZED (1): suppressed — nothing to paint into.
    send_size(&shell, 1, 800, 600);
    // Zero sizes: suppressed (no surface exists for them).
    send_size(&shell, 0, 0, 600);
    send_size(&shell, 0, 800, 0);
    assert_eq!(
        *seen.borrow(),
        vec![(800, 600), (1024, 768)],
        "minimized/zero sizes never reach the hook"
    );
    // The pump queue is untouched: WM_SIZE is a synchronous hook,
    // never a queued command.
    let _ = shell.pump_events();
    assert!(shell.take_cmds().is_empty());
}

fn send_sizemove(shell: &Win32Shell, msg: u32) {
    use ::windows::Win32::Foundation::{LPARAM, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::SendMessageW;
    unsafe {
        SendMessageW(shell.hwnd(), msg, Some(WPARAM(0)), Some(LPARAM(0)));
    }
}

/// Decision 498: `WM_SIZE` steps fire the hook (throttling lives
/// runner-side at ~30Hz) and `WM_EXITSIZEMOVE` fires it once more
/// with the final live size, so release lands crisp even when
/// trailing drag steps were throttled away.
#[test]
fn sizing_steps_fire_with_exit_catch_up() {
    use ::windows::Win32::UI::WindowsAndMessaging::WM_EXITSIZEMOVE;
    let mut shell = hidden_shell();
    let seen: Rc<RefCell<Vec<(u32, u32)>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let seen = seen.clone();
        shell.set_resize_callback(Rc::new(move |w, h| seen.borrow_mut().push((w, h))));
    }
    // Drag steps fire (the runner throttles the expensive ones).
    send_size(&shell, 0, 190, 150);
    send_size(&shell, 0, 180, 140);
    assert_eq!(*seen.borrow(), vec![(190, 150), (180, 140)]);
    // Release fires once more with the live client size (queried
    // live — the 200x150 creation dims are outer, so the client is
    // smaller by the OS frame).
    send_sizemove(&shell, WM_EXITSIZEMOVE);
    let live = {
        use ::windows::Win32::Foundation::RECT;
        use ::windows::Win32::UI::WindowsAndMessaging::GetClientRect;
        let mut client = RECT::default();
        unsafe {
            let _ = GetClientRect(shell.hwnd(), &mut client);
        }
        (
            (client.right - client.left) as u32,
            (client.bottom - client.top) as u32,
        )
    };
    assert_eq!(*seen.borrow(), vec![(190, 150), (180, 140), live]);
}
