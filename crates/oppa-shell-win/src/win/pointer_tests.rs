use super::*;
use oppa::shell::PlatformShell;

fn hidden_shell() -> Win32Shell {
    // Deterministic modifiers (decision 417): the proc samples live
    // Shift/Ctrl per message — pin both released so no assertion can
    // depend on the machine's keyboard.
    super::set_key_override(VK_SHIFT.0, Some(false));
    super::set_key_override(VK_CONTROL.0, Some(false));
    hidden_shell_with_style(oppa::WindowStyle::Decorated)
}

fn hidden_shell_with_style(style: oppa::WindowStyle) -> Win32Shell {
    super::set_key_override(VK_SHIFT.0, Some(false));
    super::set_key_override(VK_CONTROL.0, Some(false));
    Win32Shell::new(ShellConfig {
        title: "style-test".to_string(),
        width: 200,
        height: 150,
        record_messages: false,
        suppress_os_composition_window: true,
        visible: false,
        style,
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

/// Decision 484 (the winit borderless architecture — found live:
/// a bare `WS_POPUP` silently dropped Aero snap/tiling with the
/// thick frame): frameless windows keep the overlapped frame
/// styles (`WS_CAPTION | WS_THICKFRAME | WS_SYSMENU` + min/max
/// boxes) so the OS keeps owning snap, tiling, shadows, and
/// animations — only the `WM_NCCALCSIZE` visuals go.
#[test]
fn frameless_window_keeps_frame_styles_for_snap() {
    use ::windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, GWL_STYLE, WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU,
        WS_THICKFRAME,
    };
    for style in [
        oppa::WindowStyle::Undecorated,
        oppa::WindowStyle::Transparent,
        oppa::WindowStyle::Extended,
    ] {
        let shell = hidden_shell_with_style(style);
        let bits = unsafe { GetWindowLongW(shell.hwnd(), GWL_STYLE) as u32 };
        for flag in [
            WS_CAPTION.0,
            WS_THICKFRAME.0,
            WS_SYSMENU.0,
            WS_MINIMIZEBOX.0,
            WS_MAXIMIZEBOX.0,
        ] {
            assert_ne!(
                bits & flag,
                0,
                "{style:?} keeps frame bit {flag:#x} (got {bits:#x})"
            );
        }
    }
}

/// Decision 484: transparent windows enable DWM per-pixel
/// composition at creation (winit's empty-blur-region recipe) — the
/// call succeeds against the live compositor, and the proc skips
/// GDI erasure (the `COLOR_WINDOW` white brush would paint an opaque
/// sheet under the alpha-composited flip content — the Stylebook
/// white-background report).
#[test]
fn transparent_window_composites_and_skips_gdi_erase() {
    use ::windows::Win32::Foundation::{LPARAM, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_ERASEBKGND};
    let shell = hidden_shell_with_style(oppa::WindowStyle::Transparent);
    assert!(
        super::enable_transparent_blur(shell.hwnd()).is_ok(),
        "DWM blur-behind engages on the live compositor"
    );
    let erased = unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_ERASEBKGND,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        )
        .0
    };
    assert_eq!(
        erased, 1,
        "transparent windows claim erasure (paint nothing)"
    );
}

/// Decision 489: every frameless window extends the 1px glass frame
/// (native border line + drop shadow — the `WM_NCCALCSIZE` strip
/// takes them with the visuals). Proven against a hidden window on
/// the live compositor (same environment bound as creation).
#[test]
fn frameless_window_extends_glass_frame() {
    for style in [
        oppa::WindowStyle::Undecorated,
        oppa::WindowStyle::Transparent,
        oppa::WindowStyle::Extended,
    ] {
        let shell = hidden_shell_with_style(style);
        assert!(
            super::extend_glass_frame(shell.hwnd()).is_ok(),
            "{style:?} takes the glass frame"
        );
    }
}

/// Decision 490 probe (locks the native-resize design): the OS does
/// NOT route points just outside a frameless rect back to it
/// (`WindowFromPoint` — the feed of `WM_NCHITTEST` delivery), so no
/// invisible border ring exists to receive — outside pickup needs an
/// owned transparent margin ring (window bigger than the scene with
/// a glassed NC ring: NCCALCSIZE insets, per-resize blur regions —
/// major surgery, explicitly deferred, never silently assumed).
/// Inside-edge points keep answering through the proc below with
/// genuine OS frame metrics (decision 490 def-deferral).
#[test]
fn outside_points_do_not_route_to_frameless_window() {
    use ::windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, WindowFromPoint};
    use windows::Win32::Foundation::{POINT, RECT};
    let shell = hidden_shell_with_style(oppa::WindowStyle::Extended);
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(shell.hwnd(), &mut rect);
    }
    // A point far outside can never be ours (robust on every
    // machine, no other window can own it... it owns nothing — the
    // assertion is only that IT ISN'T US).
    let far = unsafe {
        WindowFromPoint(POINT {
            x: rect.right + 200,
            y: rect.bottom + 200,
        })
    };
    assert_ne!(far, shell.hwnd(), "far points never route to the window");
    // Near-edge points report for the record (informational only —
    // DWM versions may differ, so no assertion pins them).
    for (x, y, edge) in [
        (rect.left - 3, (rect.top + rect.bottom) / 2, "left"),
        (rect.right + 3, (rect.top + rect.bottom) / 2, "right"),
        ((rect.left + rect.right) / 2, rect.top - 3, "top"),
        ((rect.left + rect.right) / 2, rect.bottom + 3, "bottom"),
    ] {
        let hit = unsafe { WindowFromPoint(POINT { x, y }) };
        eprintln!(
            "oppa-shell-win: outside probe {edge}: routed={}",
            hit == shell.hwnd()
        );
    }
}

/// Decision 484: the maximized frameless clamp pins the proposed
/// rect to the work origin + work size (the winit `WM_NCCALCSIZE`
/// mirror — no decoration offsets survive the strip, so a
/// maximized borderless window owns exactly the work area, never
/// the taskbar).
#[test]
fn clamp_rect_to_work_pins_maximized_rect() {
    use windows::Win32::Foundation::RECT;
    let clamped = super::clamp_rect_to_work(
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        },
    );
    assert_eq!(
        (clamped.left, clamped.top, clamped.right, clamped.bottom),
        (0, 0, 1920, 1040),
        "maximized rect owns the work area only"
    );
}

/// Decision 485: the pure caption matcher maps each zone to its
/// `HT*BUTTON` code in order (first hit wins — zones never overlap
/// by construction) and misses fall through (`None` — the frameless
/// classifier below owns the point).
#[test]
fn caption_button_at_maps_zones_in_order() {
    use oppa::CaptionButton;
    let zones = vec![
        (CaptionButton::Minimize, [154.0, 0.0, 46.0, 40.0]),
        (CaptionButton::Maximize, [200.0, 0.0, 46.0, 40.0]),
        (CaptionButton::Close, [246.0, 0.0, 46.0, 40.0]),
    ];
    assert_eq!(
        super::caption_button_at(160.0, 20.0, &zones),
        Some(HTMINBUTTON_PX),
        "minimize zone answers HTMINBUTTON"
    );
    assert_eq!(
        super::caption_button_at(223.0, 20.0, &zones),
        Some(HTMAXBUTTON_PX),
        "maximize zone answers HTMAXBUTTON"
    );
    assert_eq!(
        super::caption_button_at(269.0, 20.0, &zones),
        Some(HTCLOSE_PX),
        "close zone answers HTCLOSE"
    );
    assert_eq!(
        super::caption_button_at(100.0, 20.0, &zones),
        None,
        "bar chrome falls through to caption/drag handling"
    );
    assert_eq!(
        super::caption_button_at(160.0, 20.0, &[]),
        None,
        "no zones means no codes (custom/off)"
    );
}

/// Decision 485, live OS proof: a real extended window with
/// published caption zones answers `WM_NCHITTEST` with the `HT*BUTTON`
/// codes through the proc — ahead of the drag region beneath them —
/// so `DefWindowProcW` owns hover glow, press, and snap layouts.
#[test]
fn caption_zones_answer_htbutton_codes_through_proc() {
    use ::windows::Win32::Foundation::{LPARAM, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, SendMessageW, WM_NCHITTEST};
    use oppa::CaptionButton;
    use windows::Win32::Foundation::RECT;
    let shell = hidden_shell_with_style(oppa::WindowStyle::Extended);
    // Three 46x40 zones packed at the top-right of the 200x150
    // client area (the runner publishes these from the retained
    // button boxes — here asserted verbatim through the proc).
    shell.set_caption_buttons(vec![
        (CaptionButton::Minimize, [62.0, 0.0, 46.0, 40.0]),
        (CaptionButton::Maximize, [108.0, 0.0, 46.0, 40.0]),
        (CaptionButton::Close, [154.0, 0.0, 46.0, 40.0]),
    ]);
    assert_eq!(shell.caption_zones().len(), 3, "zones publish");
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(shell.hwnd(), &mut rect);
    }
    let hit = |cx: i32, cy: i32| unsafe {
        let (sx, sy) = (rect.left + cx, rect.top + cy);
        SendMessageW(
            shell.hwnd(),
            WM_NCHITTEST,
            Some(WPARAM(0)),
            Some(LPARAM(((sy << 16) | (sx & 0xFFFF)) as isize)),
        )
        .0
    };
    assert_eq!(hit(85, 20), HTMINBUTTON_PX, "minimize zone codes");
    assert_eq!(hit(131, 20), HTMAXBUTTON_PX, "maximize zone codes");
    assert_eq!(hit(177, 20), HTCLOSE_PX, "close zone codes");
    assert_eq!(hit(30, 100), HTCLIENT_PX, "away from zones stays client");
}

/// Decision 491: `WM_GETTITLEBARINFOEX` reports the real custom-frame
/// geometry in screen coordinates (the OS-documented custom-titlebar
/// channel — DWM features and accessibility read real button bounds
/// here instead of phantom standard metrics). Slots per contract:
/// 0 = bar, 2 = min, 3 = max, 5 = close (focusable); help (4) is
/// absent — invisible with a zero rect, never a phantom affordance.
#[test]
fn title_bar_info_reports_real_geometry_in_screen_coords() {
    use ::windows::Win32::Foundation::LPARAM;
    use ::windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SendMessageW, TITLEBARINFOEX, WM_GETTITLEBARINFOEX,
    };
    use oppa::CaptionButton;
    use windows::Win32::Foundation::RECT;
    let shell = hidden_shell_with_style(oppa::WindowStyle::Extended);
    shell.set_title_bar_rect(Some([0.0, 0.0, 200.0, 40.0]));
    shell.set_caption_buttons(vec![
        (CaptionButton::Minimize, [62.0, 0.0, 46.0, 40.0]),
        (CaptionButton::Maximize, [108.0, 0.0, 46.0, 40.0]),
        (CaptionButton::Close, [154.0, 0.0, 46.0, 40.0]),
    ]);
    let mut origin = RECT::default();
    unsafe {
        let _ = GetWindowRect(shell.hwnd(), &mut origin);
    }
    let mut info = TITLEBARINFOEX::default();
    let handled = unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_GETTITLEBARINFOEX,
            Some(::windows::Win32::Foundation::WPARAM(0)),
            Some(LPARAM(&mut info as *mut TITLEBARINFOEX as isize)),
        )
        .0
    };
    assert_ne!(handled, 0, "custom geometry claims the message");
    let at = |r: [f32; 4]| RECT {
        left: origin.left + r[0] as i32,
        top: origin.top + r[1] as i32,
        right: origin.left + (r[0] + r[2]) as i32,
        bottom: origin.top + (r[1] + r[3]) as i32,
    };
    assert_eq!(
        info.rcTitleBar,
        at([0.0, 0.0, 200.0, 40.0]),
        "bar box converts"
    );
    assert_eq!(
        info.rgrect[2],
        at([62.0, 0.0, 46.0, 40.0]),
        "min slot converts"
    );
    assert_eq!(
        info.rgrect[3],
        at([108.0, 0.0, 46.0, 40.0]),
        "max slot converts"
    );
    assert_eq!(
        info.rgrect[5],
        at([154.0, 0.0, 46.0, 40.0]),
        "close slot converts"
    );
    assert_eq!(info.rgrect[4], RECT::default(), "help stays zeroed");
    assert_eq!(
        info.rgstate[4], 0x0000_8000,
        "help reports invisible, never focusable"
    );
    for i in [2usize, 3, 5] {
        assert_eq!(
            info.rgstate[i], 0x0010_0000,
            "present buttons report focusable"
        );
    }
}

/// Decision 486: every `WM_NCHITTEST` refreshes the published
/// caption hover (zone hits set it, anything else clears it) and
/// `WM_NCMOUSELEAVE` clears it on window exit — the runner drains
/// this into the host hover signal the `HeaderBar` glow paints
/// from.
#[test]
fn nchittest_publishes_caption_hover_and_leave_clears() {
    use ::windows::Win32::Foundation::{LPARAM, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SendMessageW, WM_NCHITTEST, WM_NCMOUSELEAVE,
    };
    use oppa::CaptionButton;
    use windows::Win32::Foundation::RECT;
    let shell = hidden_shell_with_style(oppa::WindowStyle::Extended);
    shell.set_caption_buttons(vec![
        (CaptionButton::Minimize, [62.0, 0.0, 46.0, 40.0]),
        (CaptionButton::Maximize, [108.0, 0.0, 46.0, 40.0]),
        (CaptionButton::Close, [154.0, 0.0, 46.0, 40.0]),
    ]);
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(shell.hwnd(), &mut rect);
    }
    let probe = |cx: i32, cy: i32| unsafe {
        let (sx, sy) = (rect.left + cx, rect.top + cy);
        SendMessageW(
            shell.hwnd(),
            WM_NCHITTEST,
            Some(WPARAM(0)),
            Some(LPARAM(((sy << 16) | (sx & 0xFFFF)) as isize)),
        )
    };
    assert_eq!(shell.hover_caption(), None, "boot hover is none");
    probe(131, 20);
    assert_eq!(
        shell.hover_caption(),
        Some(CaptionButton::Maximize),
        "zone hit publishes the hover"
    );
    probe(30, 100);
    assert_eq!(shell.hover_caption(), None, "client hit clears the hover");
    probe(177, 20);
    assert_eq!(
        shell.hover_caption(),
        Some(CaptionButton::Close),
        "close hover publishes"
    );
    unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_NCMOUSELEAVE,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        );
    }
    assert_eq!(
        shell.hover_caption(),
        None,
        "leaving the window clears the hover"
    );
}

/// Decision 488: the shell performs caption clicks itself (live
/// `DefWindowProc` button tracking needs real caption metrics that
/// don't exist on a stripped frame, so it drops the press).
/// `WM_NCLBUTTONDOWN` over a zone minimizes / toggles maximize /
/// queues `CloseRequested` (the veto path); a double-click over a
/// zone is swallowed (the single-click already acted — no
/// maximize-flicker); a double-click on bare caption keeps the
/// default zoom toggle. Windows shown here are destroyed at the end
/// (a maximized test window must never outlive its test).
#[test]
fn caption_clicks_act_and_double_clicks_swallow() {
    use ::windows::Win32::Foundation::{LPARAM, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, IsIconic, IsZoomed, SendMessageW, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN,
    };
    use oppa::shell::PlatformShell;
    use oppa::CaptionButton;
    use windows::Win32::Foundation::{HWND, RECT};
    fn zones() -> Vec<(CaptionButton, [f32; 4])> {
        vec![
            (CaptionButton::Minimize, [62.0, 0.0, 46.0, 40.0]),
            (CaptionButton::Maximize, [108.0, 0.0, 46.0, 40.0]),
            (CaptionButton::Close, [154.0, 0.0, 46.0, 40.0]),
        ]
    }
    fn styled() -> Win32Shell {
        let shell = hidden_shell_with_style(oppa::WindowStyle::Extended);
        shell.set_caption_buttons(zones());
        shell
    }
    fn at(hwnd: HWND, cx: i32, cy: i32) -> LPARAM {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetWindowRect(hwnd, &mut rect);
        }
        LPARAM((((rect.top + cy) << 16) | ((rect.left + cx) & 0xFFFF)) as isize)
    }
    // Minimize acts (taskbar button cleaned by destroy below).
    let shell = styled();
    unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_NCLBUTTONDOWN,
            Some(WPARAM(HTMINBUTTON_PX as usize)),
            Some(at(shell.hwnd(), 85, 20)),
        );
        assert!(IsIconic(shell.hwnd()).as_bool(), "minimize zone minimizes");
    }
    shell.destroy_window();
    // Maximize toggles, and a following double-click swallows (no
    // flicker back).
    let shell = styled();
    unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_NCLBUTTONDOWN,
            Some(WPARAM(HTMAXBUTTON_PX as usize)),
            Some(at(shell.hwnd(), 131, 20)),
        );
        assert!(IsZoomed(shell.hwnd()).as_bool(), "maximize zone maximizes");
        SendMessageW(
            shell.hwnd(),
            WM_NCLBUTTONDBLCLK,
            Some(WPARAM(HTMAXBUTTON_PX as usize)),
            Some(at(shell.hwnd(), 131, 20)),
        );
        assert!(
            IsZoomed(shell.hwnd()).as_bool(),
            "double-click swallows (stays maximized)"
        );
        SendMessageW(
            shell.hwnd(),
            WM_NCLBUTTONDOWN,
            Some(WPARAM(HTMAXBUTTON_PX as usize)),
            Some(at(shell.hwnd(), 131, 20)),
        );
        assert!(!IsZoomed(shell.hwnd()).as_bool(), "second press restores");
    }
    shell.destroy_window();
    // Close queues the veto path (never destroys synchronously —
    // the runner approves through the loop close handler).
    let mut shell = styled();
    unsafe {
        SendMessageW(
            shell.hwnd(),
            WM_NCLBUTTONDOWN,
            Some(WPARAM(HTCLOSE_PX as usize)),
            Some(at(shell.hwnd(), 177, 20)),
        );
    }
    let kinds: Vec<oppa::EventKind> = shell.pump_events().iter().map(|e| e.kind).collect();
    assert!(
        kinds.contains(&oppa::EventKind::CloseRequested),
        "close zone queues the veto consult, got {kinds:?}"
    );
    shell.destroy_window();
}
