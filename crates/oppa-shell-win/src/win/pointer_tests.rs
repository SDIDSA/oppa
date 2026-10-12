use super::*;
use oppa::shell::PlatformShell;

fn hidden_shell() -> Win32Shell {
    // Deterministic modifiers (decision 417): the proc samples live
    // Shift/Ctrl per message — pin both released so no assertion can
    // depend on the machine's keyboard.
    super::set_key_override(VK_SHIFT.0, Some(false));
    super::set_key_override(VK_CONTROL.0, Some(false));
    Win32Shell::new(ShellConfig {
        title: "pointer-test".to_string(),
        width: 200,
        height: 150,
        record_messages: false,
        suppress_os_composition_window: true,
        visible: false,
        style: oppa::WindowStyle::Decorated,
    })
    .expect("hidden test window builds")
}

/// Round 8.3 (decision 299): every cursor shape loads through the
/// OS loader, `set_cursor` stores the runner's resolution, and a
/// real `WM_SETCURSOR` through the proc applies it and claims the
/// message (nonzero — the class arrow never overrides us).
#[test]
fn cursor_shapes_load_store_and_apply_on_setcursor() {
    use oppa::CursorIcon;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SETCURSOR};
    for cursor in [
        CursorIcon::Default,
        CursorIcon::Pointer,
        CursorIcon::Text,
        CursorIcon::Crosshair,
        CursorIcon::Move,
        CursorIcon::NotAllowed,
        CursorIcon::ColResize,
        CursorIcon::RowResize,
    ] {
        let handle = unsafe { LoadCursorW(None, cursor_idc(cursor)) }
            .unwrap_or_else(|_| panic!("stock cursor loads for {cursor:?}"));
        assert!(!handle.0.is_null(), "non-null handle for {cursor:?}");
    }
    let mut shell = hidden_shell();
    assert_eq!(shell.shared.borrow().current_cursor, None);
    shell.set_cursor(CursorIcon::Text);
    assert_eq!(shell.shared.borrow().current_cursor, Some(CursorIcon::Text));
    let ret = unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_SETCURSOR,
            Some(WPARAM(shell.hwnd().0 as usize)),
            Some(LPARAM(1)),
        )
    };
    assert_eq!(ret.0, 1, "SETCURSOR claims the message");
}

/// Resize-cursor fix-up: a `WM_SETCURSOR` over a sizing border
/// (`HTRIGHT`) is NOT claimed — it delegates to `DefWindowProcW`,
/// which owns the resize arrows. Claiming it pinned the
/// framework cursor over the borders (resizing worked, the
/// affordance never showed).
#[test]
fn setcursor_delegates_sizing_borders_to_defwindowproc() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{DefWindowProcW, SendMessageW, WM_SETCURSOR};
    let shell = hidden_shell();
    // HTRIGHT = 11 in the low word (hit-test), no mouse message
    // in the high word — DefWindowProc keys the arrow off the
    // hit-test alone.
    let lparam = LPARAM(11);
    let wparam = WPARAM(shell.hwnd().0 as usize);
    let ret = unsafe { SendMessageW(shell.hwnd(), WM_SETCURSOR, Some(wparam), Some(lparam)) };
    let direct = unsafe { DefWindowProcW(shell.hwnd(), WM_SETCURSOR, wparam, lparam) };
    assert_eq!(
        ret.0, direct.0,
        "border SETCURSOR delegates (got {ret:?}, DefWindowProc gives {direct:?})"
    );
}

/// Round 9.1 (decision 300): a zero bound returns promptly with no
/// message (timer path — never a hang), and a posted message wakes
/// an indefinite wait (no added latency on input).
#[test]
fn wait_times_out_promptly_and_wakes_on_posted_message() {
    let mut shell = hidden_shell();
    // Drain first: this thread's queue may hold traffic from
    // earlier tests' windows (same-thread pooling) — the wait
    // must see a quiet queue to prove the timer bound.
    shell.process_os_messages();
    assert_eq!(
        shell.wait_for_input(Some(0)),
        WaitOutcome::Timeout,
        "quiet queue hits the timer bound"
    );
    // Posted-message wake, cross-thread: the waiter blocks
    // indefinitely on its own window while this thread posts.
    let (tx, rx) = std::sync::mpsc::channel();
    let (posted_tx, posted_rx) = std::sync::mpsc::channel();
    std::thread::scope(|s| {
        s.spawn(move || {
            let mut waiter = hidden_shell();
            // HWND is not `Send` — the raw value crosses as an
            // integer and is re-wrapped on this side only.
            tx.send(waiter.shared.borrow().hwnd.0 as isize)
                .expect("hwnd reaches the poster");
            // Drain first: a fresh window's thread queue carries
            // creation traffic that would wake the wait
            // immediately (the same pooled-queue lesson as
            // above) — and drop the window early if it did.
            waiter.process_os_messages();
            assert_eq!(
                waiter.wait_for_input(None),
                WaitOutcome::Input,
                "posted message wakes the indefinite wait"
            );
            // Keep `waiter` alive until the poster finishes posting so its HWND isn't destroyed early
            let _ = posted_rx.recv();
        });
        let hwnd_raw = rx.recv().expect("waiter opens its window");
        // The waiter may not be blocked yet — harmless: an already-
        // queued message still wakes `MsgWaitForMultipleObjectsEx`
        // immediately (`MWMO_INPUTAVAILABLE`).
        std::thread::sleep(std::time::Duration::from_millis(50));
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_USER};
        unsafe {
            PostMessageW(
                Some(HWND(hwnd_raw as *mut core::ffi::c_void)),
                WM_USER,
                WPARAM(0),
                LPARAM(0),
            )
            .expect("post wakes the waiter");
        }
        let _ = posted_tx.send(());
    });
}

/// Round 7.19 (decision 294): queued pointer events pump to the
/// discrete pipeline feed in order — parity with the Linux and
/// Android shells' PointerDown/Move/Up commands.
#[test]
fn pump_maps_discrete_pointer_cmds_in_order() {
    let mut shell = hidden_shell();
    {
        let mut s = shell.shared.borrow_mut();
        s.queue.push_back(ShellEvent::PointerDown {
            button: oppa::PointerButton::Primary,
            dbl_click: false,
            shift: false,
            x: 10,
            y: 20,
        });
        s.queue.push_back(ShellEvent::PointerMove {
            x: 30,
            y: 20,
            left_down: true,
        });
        s.queue.push_back(ShellEvent::PointerMove {
            x: 40,
            y: 25,
            left_down: false,
        });
        s.queue.push_back(ShellEvent::PointerUp {
            button: oppa::PointerButton::Primary,
            x: 40,
            y: 25,
            shift: false,
        });
    }
    let kinds: Vec<EventKind> = shell.pump_events().iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            EventKind::Press,
            EventKind::PointerMove,
            EventKind::PointerMove,
            EventKind::Release,
        ]
    );
    assert_eq!(
        shell.take_cmds(),
        vec![
            Cmd::PointerDown {
                x: 10.0,
                y: 20.0,
                shift: false,
                button: oppa::PointerButton::Primary,
            },
            Cmd::PointerMove { x: 30.0, y: 20.0 },
            Cmd::PointerMove { x: 40.0, y: 25.0 },
            Cmd::PointerUp {
                x: 40.0,
                y: 25.0,
                shift: false,
                button: oppa::PointerButton::Primary,
            },
        ]
    );
}

/// An external capture steal while a drag is in flight trips the
/// cancel command (the router's global release); our own
/// button-up release stays quiet.
#[test]
fn right_and_middle_buttons_classify_without_primary_aliasing() {
    use ::windows::Win32::Foundation::{LPARAM, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{
        SendMessageW, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP,
    };
    // Round 9.2 (decision 301): real button messages classify
    // into the tap taxonomy (right = secondary, middle =
    // auxiliary) instead of aliasing primary.
    let mut shell = hidden_shell();
    unsafe {
        // lparam packs client y in the high word, x in the low.
        let at = |x: i32, y: i32| Some(LPARAM(((y << 16) | x) as isize));
        SendMessageW(shell.hwnd(), WM_RBUTTONDOWN, Some(WPARAM(0)), at(10, 20));
        SendMessageW(shell.hwnd(), WM_RBUTTONUP, Some(WPARAM(0)), at(10, 20));
        SendMessageW(shell.hwnd(), WM_MBUTTONDOWN, Some(WPARAM(0)), at(30, 40));
        SendMessageW(shell.hwnd(), WM_MBUTTONUP, Some(WPARAM(0)), at(30, 40));
    }
    let _ = shell.pump_events();
    assert_eq!(
        shell.take_cmds(),
        vec![
            Cmd::PointerDown {
                x: 10.0,
                y: 20.0,
                shift: false,
                button: oppa::PointerButton::Secondary,
            },
            Cmd::PointerUp {
                x: 10.0,
                y: 20.0,
                shift: false,
                button: oppa::PointerButton::Secondary,
            },
            Cmd::PointerDown {
                x: 30.0,
                y: 40.0,
                shift: false,
                button: oppa::PointerButton::Auxiliary,
            },
            Cmd::PointerUp {
                x: 30.0,
                y: 40.0,
                shift: false,
                button: oppa::PointerButton::Auxiliary,
            },
        ]
    );
}

/// An external capture steal while a drag is in flight trips the
/// cancel command (the router's global release); our own
/// button-up release stays quiet.
#[test]
fn capture_changed_trips_cancel_only_while_down() {
    use ::windows::Win32::Foundation::{LPARAM, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{
        SendMessageW, WM_CAPTURECHANGED, WM_LBUTTONUP,
    };

    // External steal mid-drag: Down pumped (in-flight flag set),
    // then a real WM_CAPTURECHANGED through the window proc.
    let mut shell = hidden_shell();
    shell
        .shared
        .borrow_mut()
        .queue
        .push_back(ShellEvent::PointerDown {
            button: oppa::PointerButton::Primary,
            dbl_click: false,
            shift: false,
            x: 10,
            y: 20,
        });
    let _ = shell.pump_events();
    assert_eq!(
        shell.take_cmds(),
        vec![Cmd::PointerDown {
            x: 10.0,
            y: 20.0,
            shift: false,
            button: oppa::PointerButton::Primary,
        }]
    );
    unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_CAPTURECHANGED,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        );
    }
    let _ = shell.pump_events();
    assert_eq!(
        shell.take_cmds(),
        vec![Cmd::PointerCancel],
        "external steal mid-drag cancels"
    );

    // Our own button-up release: a real WM_LBUTTONUP through the
    // proc queues the release AND pre-clears the in-flight flag,
    // so the release's own WM_CAPTURECHANGED stays quiet.
    let mut shell = hidden_shell();
    unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_LBUTTONUP,
            Some(WPARAM(0)),
            Some(LPARAM((25 << 16) | 40)),
        );
        SendMessageW(
            shell.hwnd(),
            WM_CAPTURECHANGED,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        );
    }
    let _ = shell.pump_events();
    assert_eq!(
        shell.take_cmds(),
        vec![Cmd::PointerUp {
            x: 40.0,
            y: 25.0,
            shift: false,
            button: oppa::PointerButton::Primary,
        }],
        "own release carries no cancel echo"
    );

    // No drag in flight at all: capture-changed is a quiet no-op.
    let mut shell = hidden_shell();
    unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_CAPTURECHANGED,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        );
    }
    let _ = shell.pump_events();
    assert!(
        shell.take_cmds().is_empty(),
        "capture-changed with nothing down emits nothing"
    );
}

/// Round 16.2 (decision 315): a real `WM_SETTINGCHANGE` naming
/// "ImmersiveColorSet" drains as a theme-change command; any
/// other section stays quiet.
#[test]
fn setting_change_immersive_maps_to_theme_command() {
    use ::windows::Win32::Foundation::{LPARAM, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SETTINGCHANGE};
    let mut shell = hidden_shell();
    let section: Vec<u16> = "ImmersiveColorSet".encode_utf16().chain([0]).collect();
    unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_SETTINGCHANGE,
            Some(WPARAM(0)),
            Some(LPARAM(section.as_ptr() as isize)),
        );
    }
    let _ = shell.pump_events();
    assert_eq!(shell.take_cmds(), vec![Cmd::SystemThemeChanged]);
    // Other sections never queue.
    let mut shell = hidden_shell();
    let other: Vec<u16> = "WindowsThemeElement".encode_utf16().chain([0]).collect();
    unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_SETTINGCHANGE,
            Some(WPARAM(0)),
            Some(LPARAM(other.as_ptr() as isize)),
        );
    }
    let _ = shell.pump_events();
    assert!(
        shell.take_cmds().is_empty(),
        "non-theme sections stay quiet"
    );
}

/// Round 16.3 (decision 316): a real `WM_CLOSE` queues a close
/// request instead of destroying — the window stays alive until
/// the runner approves and destroys explicitly.
#[test]
fn close_queues_request_and_keeps_window_alive() {
    use ::windows::Win32::Foundation::{LPARAM, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{IsWindow, SendMessageW, WM_CLOSE};
    let mut shell = hidden_shell();
    let hwnd = shell.hwnd();
    unsafe {
        SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
    }
    let _ = shell.pump_events();
    assert_eq!(shell.take_cmds(), vec![Cmd::CloseRequested]);
    assert!(
        unsafe { IsWindow(Some(hwnd)).as_bool() },
        "veto-pending close destroys nothing"
    );
    // Explicit destroy ends it through the normal flow.
    shell.destroy_window();
    assert!(
        unsafe { !IsWindow(Some(hwnd)).as_bool() },
        "approved close destroys"
    );
}

/// Round 24.2: a real `WM_MOUSEWHEEL` through the proc dispatches
/// a vertical `Scroll` cmd — wheel-down (negative delta) converts
/// to positive `dy` at the documented scale with `dx == 0.0`, and
/// wheel-up mirrors it.
#[test]
fn wheel_dispatches_vertical_scroll_cmd_with_content_sign() {
    use ::windows::Win32::Foundation::{LPARAM, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_MOUSEWHEEL};
    use oppa::shell::EventKind;
    let mut shell = hidden_shell();
    unsafe {
        // wparam high word = signed delta (-120 = one notch down);
        // lparam packs screen y/x like WM_MOUSEHWHEEL.
        let wheel = |delta: i16| Some(WPARAM(((delta as u16 as u32) << 16) as usize));
        let at = |x: i32, y: i32| Some(LPARAM(((y << 16) | x) as isize));
        SendMessageW(shell.hwnd(), WM_MOUSEWHEEL, wheel(-120), at(10, 20));
        SendMessageW(shell.hwnd(), WM_MOUSEWHEEL, wheel(120), at(30, 40));
    }
    let kinds: Vec<EventKind> = shell.pump_events().iter().map(|e| e.kind).collect();
    assert_eq!(kinds, vec![EventKind::Scroll, EventKind::Scroll]);
    let cmds = shell.take_cmds();
    assert_eq!(cmds.len(), 2, "both wheel ticks dispatch");
    match cmds[0] {
        Cmd::Scroll { dx, dy, .. } => {
            assert_eq!(dy, WHEEL_LINE_PX, "wheel-down grows the offset");
            assert_eq!(dx, 0.0, "vertical tick carries no horizontal delta");
        }
        other => panic!("WHEEL must dispatch Scroll, got {other:?}"),
    }
    match cmds[1] {
        Cmd::Scroll { dx, dy, .. } => {
            assert_eq!(dy, -WHEEL_LINE_PX, "wheel-up mirrors wheel-down");
            assert_eq!(dx, 0.0);
        }
        other => panic!("WHEEL must dispatch Scroll, got {other:?}"),
    }
}

/// Phase 36 PR2b (decision 354): the `WM_MOUSEWHEEL` → event
/// mapping routes on Shift — plain ticks queue vertical `Wheel`,
/// Shift-held ticks queue `HWheel` with the same delta (the Cmd
/// layer converts them exactly like the tilt wheel). Pure mapping
/// over the proc-sampled Shift state (no synthesized key state).
#[test]
fn shift_wheel_routes_horizontal_without_shift_stays_vertical() {
    assert_eq!(
        super::wheel_event(10, 20, -120, false),
        ShellEvent::Wheel {
            x: 10,
            y: 20,
            delta: -120
        }
    );
    assert_eq!(
        super::wheel_event(10, 20, -120, true),
        ShellEvent::HWheel {
            x: 10,
            y: 20,
            delta: -120
        },
        "Shift+wheel rolls horizontally with the same delta"
    );
    assert_eq!(
        super::wheel_event(30, 40, 120, true),
        ShellEvent::HWheel {
            x: 30,
            y: 40,
            delta: 120
        }
    );
}

/// Round 20.2 (decision 325): a real `WM_MOUSEHWHEEL` through the
/// proc dispatches a horizontal `Scroll` cmd — tilt-right
/// (positive delta) converts to negative `dx` at the documented
/// scale with `dy == 0.0`, and tilt-left mirrors it.
#[test]
fn hwheel_dispatches_horizontal_scroll_cmd() {
    use ::windows::Win32::Foundation::{LPARAM, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_MOUSEHWHEEL};
    use oppa::shell::EventKind;
    let mut shell = hidden_shell();
    unsafe {
        // wparam high word = signed delta (120 = one notch);
        // lparam packs screen y/x like WM_MOUSEWHEEL.
        let wheel = |delta: i16| Some(WPARAM(((delta as u16 as u32) << 16) as usize));
        let at = |x: i32, y: i32| Some(LPARAM(((y << 16) | x) as isize));
        SendMessageW(shell.hwnd(), WM_MOUSEHWHEEL, wheel(120), at(10, 20));
        SendMessageW(shell.hwnd(), WM_MOUSEHWHEEL, wheel(-120), at(30, 40));
    }
    let kinds: Vec<EventKind> = shell.pump_events().iter().map(|e| e.kind).collect();
    assert_eq!(kinds, vec![EventKind::Scroll, EventKind::Scroll]);
    let cmds = shell.take_cmds();
    assert_eq!(cmds.len(), 2, "both tilt ticks dispatch");
    match cmds[0] {
        Cmd::Scroll { dx, dy, .. } => {
            assert_eq!(dx, -HWHEEL_LINE_PX, "tilt-right scrolls content left");
            assert_eq!(dy, 0.0, "horizontal tick carries no vertical delta");
        }
        other => panic!("HWHEEL must dispatch Scroll, got {other:?}"),
    }
    match cmds[1] {
        Cmd::Scroll { dx, dy, .. } => {
            assert_eq!(dx, HWHEEL_LINE_PX, "tilt-left mirrors tilt-right");
            assert_eq!(dy, 0.0);
        }
        other => panic!("HWHEEL must dispatch Scroll, got {other:?}"),
    }
}

/// Decision 482: the pure frameless classifier maps borders to the
/// sizing codes (corners first), drag regions to `HTCAPTION`, and
/// everything else to `HTCLIENT` — in a 200x150 client area with an
/// 8px band and no regions.
#[test]
fn frameless_classifier_maps_borders_caption_client() {
    let plain = frameless_hit_test(100.0, 75.0, 200.0, 150.0, false, &[]);
    assert_eq!(plain, HTCLIENT_PX, "interior is client");
    assert_eq!(
        frameless_hit_test(2.0, 75.0, 200.0, 150.0, false, &[]),
        HTLEFT_PX,
        "left band resizes"
    );
    assert_eq!(
        frameless_hit_test(198.0, 75.0, 200.0, 150.0, false, &[]),
        HTRIGHT_PX,
        "right band resizes"
    );
    assert_eq!(
        frameless_hit_test(100.0, 2.0, 200.0, 150.0, false, &[]),
        HTTOP_PX,
        "top band resizes"
    );
    assert_eq!(
        frameless_hit_test(100.0, 148.0, 200.0, 150.0, false, &[]),
        HTBOTTOM_PX,
        "bottom band resizes"
    );
    assert_eq!(
        frameless_hit_test(2.0, 2.0, 200.0, 150.0, false, &[]),
        HTTOPLEFT_PX,
        "corners win over edges"
    );
    assert_eq!(
        frameless_hit_test(198.0, 148.0, 200.0, 150.0, false, &[]),
        HTBOTTOMRIGHT_PX,
        "corners win over edges"
    );
}

/// Decision 482: drag regions answer `HTCAPTION`, borders beat a
/// full-bleed region (edge resizing stays functional under a header
/// spanning the width), and maximized windows never resize.
#[test]
fn frameless_classifier_caption_beats_client_border_beats_caption() {
    let bar = [[0.0, 0.0, 200.0, 32.0]];
    assert_eq!(
        frameless_hit_test(100.0, 16.0, 200.0, 150.0, false, &bar),
        HTCAPTION_PX,
        "bar chrome drags"
    );
    assert_eq!(
        frameless_hit_test(100.0, 100.0, 200.0, 150.0, false, &bar),
        HTCLIENT_PX,
        "below the bar stays client"
    );
    assert_eq!(
        frameless_hit_test(100.0, 2.0, 200.0, 150.0, false, &bar),
        HTTOP_PX,
        "top border resizes even under a full-bleed bar"
    );
    assert_eq!(
        frameless_hit_test(100.0, 16.0, 200.0, 150.0, true, &bar),
        HTCAPTION_PX,
        "maximized bar still drags"
    );
    assert_eq!(
        frameless_hit_test(2.0, 75.0, 200.0, 150.0, true, &[]),
        HTCLIENT_PX,
        "maximized borders never resize"
    );
}

/// Decision 482, live OS proof: a real frameless window answers
/// `WM_NCHITTEST` through the proc — a published drag region hits
/// `HTCAPTION`, the left border hits `HTLEFT`, and the interior
/// hits `HTCLIENT`. Screen points derive from the live window
/// rect (a hidden `WS_POPUP` still owns screen geometry).
#[test]
fn frameless_window_answers_nchittest_through_proc() {
    use ::windows::Win32::Foundation::{LPARAM, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetWindowRect, SendMessageW, WM_NCHITTEST,
    };
    use windows::Win32::Foundation::RECT;
    super::set_key_override(VK_SHIFT.0, Some(false));
    super::set_key_override(VK_CONTROL.0, Some(false));
    let shell = Win32Shell::new(ShellConfig {
        title: "frameless-test".to_string(),
        width: 200,
        height: 150,
        record_messages: false,
        suppress_os_composition_window: true,
        visible: false,
        style: oppa::WindowStyle::Undecorated,
    })
    .expect("frameless test window builds");
    assert_eq!(shell.window_style(), oppa::WindowStyle::Undecorated);
    shell.set_drag_regions(vec![[50.0, 20.0, 100.0, 24.0]]);
    assert_eq!(shell.drag_regions(), vec![[50.0, 20.0, 100.0, 24.0]]);
    let mut rect = RECT::default();
    let mut client = RECT::default();
    unsafe {
        let _ = GetWindowRect(shell.hwnd(), &mut rect);
        let _ = GetClientRect(shell.hwnd(), &mut client);
    }
    // `WM_NCCALCSIZE` strips the whole non-client area: the client
    // covers the window pixel for pixel (no title bar, no borders).
    assert_eq!(client.right - client.left, rect.right - rect.left);
    assert_eq!(client.bottom - client.top, rect.bottom - rect.top);
    let at = |cx: i32, cy: i32| {
        let (sx, sy) = (rect.left + cx, rect.top + cy);
        Some(LPARAM(((sy << 16) | (sx & 0xFFFF)) as isize))
    };
    let hit = |cx: i32, cy: i32| unsafe {
        SendMessageW(shell.hwnd(), WM_NCHITTEST, Some(WPARAM(0)), at(cx, cy)).0
    };
    // Client geometry may be smaller than the requested 200x150
    // (frame fitting is runner-owned) — probe relative points that
    // hold for any client area larger than the bands: border at
    // x=2, region interior, deep interior.
    assert_eq!(hit(100, 30), HTCAPTION_PX, "drag region captions");
    assert_eq!(hit(2, 75), HTLEFT_PX, "left border resizes");
    assert_eq!(hit(100, 100), HTCLIENT_PX, "deep interior is client");
}
