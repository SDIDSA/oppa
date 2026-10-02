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
