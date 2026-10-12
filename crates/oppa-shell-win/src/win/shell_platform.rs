// ---------------------------------------------------------------------------
// PlatformShell trait implementation (event pump + dispatch)
// ---------------------------------------------------------------------------

use super::*;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, ScreenToClient, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, IsZoomed, ShowWindow, NCCALCSIZE_PARAMS, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
    WM_ERASEBKGND, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN, WM_NCMOUSELEAVE,
};

/// Publishes the hovered caption zone (decision 486): maps the
/// hit-test point through the zones and stores the live hover (`None`
/// on a miss — every `WM_NCHITTEST` refreshes, so the in-scene glow
/// tracks the cursor exactly while it is over the window). Arms
/// `TrackMouseEvent(TME_LEAVE | TME_NONCLIENT)` on entry so leaving
/// the window clears through `WM_NCMOUSELEAVE` below (re-armed per
/// entry — steady hovering never re-arms).
fn publish_caption_hover(
    shared: &Rc<RefCell<ShellShared>>,
    x: f32,
    y: f32,
    zones: &[(oppa::CaptionButton, [f32; 4])],
    hwnd: HWND,
) {
    let hover = zones.iter().find_map(|(button, r)| {
        (x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]).then_some(*button)
    });
    let mut s = shared.borrow_mut();
    if s.hover_caption != hover {
        s.hover_caption = hover;
        if hover.is_some() {
            let mut track = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE | TME_NONCLIENT,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = unsafe { TrackMouseEvent(&mut track) };
        }
    }
}

/// Live monitor work area for `hwnd` (decision 484): the maximized
/// frameless clamp keeps the taskbar visible. `None` when the
/// monitor query fails (the `WM_NCCALCSIZE` arm then keeps the
/// proposed rect — a missing clamp never misplaces the window).
fn monitor_work_rect(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            Some(info.rcWork)
        } else {
            None
        }
    }
}

impl PlatformShell for Win32Shell {
    fn pump_events(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        loop {
            let next = {
                let mut shared = self.shared.borrow_mut();
                shared.queue.pop_front()
            };
            let Some(ev) = next else { break };
            match ev {
                ShellEvent::Ime(msg) => {
                    // THE SEAM: real OS IME message → normalized event →
                    // dispatch_ime_event → the host's ImeCompositionHandler.
                    let cb = self.shared.borrow().ime_callback.clone();
                    if let Some(cb) = cb {
                        cb(&msg);
                    }
                }
                other => {
                    let mut shared = self.shared.borrow_mut();
                    if let Some(cmd) = cmd_of(&other, &mut shared.pending_drag) {
                        shared.cmds.push_back(cmd);
                    }
                    out.push(Event {
                        kind: kind_of(&other),
                        handler: FIELD_EVENT,
                    });
                }
            }
        }
        out
    }

    /// Candidate-window anchoring, driven by the M0b contract surface:
    /// `ImeOps::SetCaretRect` (device-px, run-relative per
    /// `ShapedRun::caret_rect`) → `ImmSetCompositionWindow` (CFS_POINT) +
    /// `ImmSetCandidateWindow` (CFS_CANDIDATEPOS), both in client coords
    /// (the host glue converts run-relative → client px before calling).
    /// Show/hide ops are the real IME's policy; logged only.
    fn set_ime(&mut self, ops: ImeOps) {
        match ops {
            ImeOps::SetCaretRect {
                x,
                y,
                width,
                height,
            } => {
                // Copy the HWND out first (same hazard as focus_window:
                // IMM calls can synchronously re-enter the proc).
                let hwnd = self.shared.borrow().hwnd;
                let himc = unsafe { ImmGetContext(hwnd) };
                if himc.is_invalid() {
                    return;
                }
                unsafe {
                    let comp = COMPOSITIONFORM {
                        dwStyle: CFS_POINT,
                        ptCurrentPos: POINT {
                            x: x as i32,
                            y: y as i32,
                        },
                        rcArea: RECT::default(),
                    };
                    let _ = ImmSetCompositionWindow(himc, &comp);
                    let cand = CANDIDATEFORM {
                        dwIndex: 0,
                        dwStyle: CFS_CANDIDATEPOS,
                        ptCurrentPos: POINT {
                            x: x as i32,
                            y: y as i32,
                        },
                        rcArea: RECT::default(),
                    };
                    let _ = ImmSetCandidateWindow(himc, &cand);
                    let _ = ImmReleaseContext(hwnd, himc);
                }
                self.shared
                    .borrow_mut()
                    .anchored_rects
                    .push([x, y, width, height]);
            }
            other => {
                self.shared
                    .borrow_mut()
                    .ime_log
                    .push(format!("set_ime op (no-op in real-IME wiring): {other:?}"));
            }
        }
    }

    /// Win32 clipboard backend (G3, decision 210): the shell owns one
    /// [`Win32Clipboard`](super::clipboard::Win32Clipboard) and lends it
    /// through the [`PlatformShell`] seam.
    fn clipboard(&mut self) -> Option<&mut dyn oppa::Clipboard> {
        Some(&mut self.clipboard)
    }

    /// Win32 file-open dialog (G12, decision 231): the shell owns one
    /// [`Win32FileDialog`](super::file_dialog::Win32FileDialog) and
    /// lends it through the [`PlatformShell`] seam.
    fn file_dialog(&mut self) -> Option<&mut dyn oppa::FileDialog> {
        Some(&mut self.file_dialog)
    }

    /// Pointer cursor (Round 8.3, decision 299): stores the runner's
    /// hover resolution; the `WM_SETCURSOR` arm applies it (the OS
    /// queries per mouse move — see the field docs).
    fn set_cursor(&mut self, cursor: oppa::CursorIcon) {
        self.shared.borrow_mut().current_cursor = Some(cursor);
    }
}

// ---------------------------------------------------------------------------
// Window proc
// ---------------------------------------------------------------------------

pub(crate) unsafe extern "system" fn shell_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let shared = SHELL_RC.with(|rc| rc.borrow().as_ref().map(Rc::clone));
    let Some(shared) = shared else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };

    // Record every message that reaches the proc while recording is armed.
    {
        let mut s = shared.borrow_mut();
        if s.recording {
            s.seq += 1;
            let seq = s.seq;
            s.msg_log.push(MessageRecord {
                seq,
                msg,
                wparam: wparam.0,
                lparam: lparam.0,
            });
        }
    }

    let shift_down = key_down(VK_SHIFT.0);
    let ctrl_down = key_down(VK_CONTROL.0);
    let x = client_x(lparam);
    let y = client_y(lparam);

    match msg {
        WM_IME_SETCONTEXT => {
            log_ime(
                &shared,
                ImeMessage::SetContext {
                    f_select: wparam.0 != 0,
                    data: lparam.0 as usize,
                },
            );
            // Keep the OS candidate UI; suppress the OS composition window
            // (the host draws the composition inline). DefWindowProc sees
            // the modified show-mask.
            let suppressed = if shared.borrow().cfg.suppress_os_composition_window {
                ((lparam.0 as usize) & !(ISC_SHOWUICOMPOSITIONWINDOW as usize)) as isize
            } else {
                lparam.0
            };
            DefWindowProcW(hwnd, msg, wparam, LPARAM(suppressed))
        }
        WM_IME_STARTCOMPOSITION => {
            log_ime(&shared, ImeMessage::StartComposition);
            LRESULT(0)
        }
        WM_IME_COMPOSITION => {
            // Snapshot NOW (message time): composition state can move after.
            let flags = lparam.0 as u32;
            let snapshot = read_composition(hwnd, flags);
            log_ime(&shared, snapshot);
            LRESULT(0)
        }
        WM_IME_ENDCOMPOSITION => {
            log_ime(&shared, ImeMessage::EndComposition);
            LRESULT(0)
        }
        WM_IME_NOTIFY => {
            log_ime(
                &shared,
                ImeMessage::Notify {
                    code: wparam.0 as u32,
                    param: lparam.0 as usize,
                },
            );
            // Default handling drives the OS candidate window machinery.
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_NCCALCSIZE => {
            // Decisions 482–484: frameless windows strip the
            // non-client visuals (title bar + borders) — return 0
            // for both the `wParam`-TRUE DWM frame negotiation and
            // the FALSE shape. The creation style stays overlapped
            // (decision 484 — the OS keeps owning snap, tiling,
            // shadows, and animations); only the visuals go. A
            // maximized frameless window additionally clamps to the
            // monitor work area (the winit mirror — without
            // decorations to offset it, the proposed rect would span
            // the taskbar). Decorated windows keep the OS frame
            // (default handling, byte-identical).
            if shared.borrow().style.is_frameless() {
                if wparam.0 != 0 && unsafe { IsZoomed(hwnd).as_bool() } {
                    if let Some(work) = monitor_work_rect(hwnd) {
                        let params = unsafe { &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS) };
                        params.rgrc[0] = clamp_rect_to_work(params.rgrc[0], work);
                    }
                }
                LRESULT(0)
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_ERASEBKGND => {
            // Decision 484: transparent windows never GDI-erase — the
            // class brush is `COLOR_WINDOW` white, and erasing would
            // paint an opaque sheet under the alpha-composited flip
            // content (the Stylebook white-background report). The
            // swapchain owns every pixel, so claiming erasure paints
            // nothing. All other styles keep default handling.
            if shared.borrow().style.is_transparent() {
                LRESULT(1)
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_NCHITTEST => {
            // Decisions 482–486: frameless hit-testing — native
            // caption zones first (`HT*BUTTON`, so the OS owns press
            // and snap layouts over the `HeaderBar` buttons; hover
            // glow paints in-scene from the published hover state
            // below, since DWM paints no glow on a stripped frame),
            // then the pure `frameless_hit_test` classifier over the
            // live client rect plus the runner-published drag regions
            // (OS drag on `HTCAPTION`, edge/corner resize, snap), so
            // a title bar is never needed for full window management.
            // Decorated windows keep the default (byte-identical).
            if shared.borrow().style.is_frameless() {
                // Screen point packs as i16 pairs in lParam.
                let sx = ((lparam.0 & 0xFFFF) as u16) as i16 as i32;
                let sy = (((lparam.0 >> 16) & 0xFFFF) as u16) as i16 as i32;
                let mut pt = POINT { x: sx, y: sy };
                let _ = ScreenToClient(hwnd, &mut pt);
                let zones = shared.borrow().caption_buttons.clone();
                // Decision 486 hover publish (every hit-test
                // refreshes, zone hits and misses alike, so the
                // in-scene glow tracks the cursor exactly; leaving
                // the window clears through `WM_NCMOUSELEAVE` below).
                publish_caption_hover(&shared, pt.x as f32, pt.y as f32, &zones, hwnd);
                if let Some(code) = caption_button_at(pt.x as f32, pt.y as f32, &zones) {
                    return LRESULT(code);
                }
                let mut client = RECT::default();
                let _ = GetClientRect(hwnd, &mut client);
                let maximized = IsZoomed(hwnd).as_bool();
                let drag = shared.borrow().drag_rects.clone();
                LRESULT(frameless_hit_test(
                    pt.x as f32,
                    pt.y as f32,
                    (client.right - client.left) as f32,
                    (client.bottom - client.top) as f32,
                    maximized,
                    &drag,
                ))
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_NCLBUTTONDOWN => {
            // Decision 488 (found live: hover works, clicks don't —
            // `DefWindowProc` button tracking needs real caption
            // metrics that don't exist on a stripped frame, so it
            // drops the press): the shell performs caption clicks
            // itself over the published zones (the Chrome shape — OS
            // codes for hover/snap, self-performed actions).
            // Minimize/maximize run the same `ShowWindow` calls the
            // caption bridge uses (the decision-483 reentrancy guard
            // covers the synchronous `WM_SIZE`); close queues
            // `CloseRequested` so the loop veto consults exactly like
            // `WM_CLOSE`. Anything else keeps default handling
            // (caption double-click zoom, border behavior).
            if shared.borrow().style.is_frameless() {
                let sx = ((lparam.0 & 0xFFFF) as u16) as i16 as i32;
                let sy = (((lparam.0 >> 16) & 0xFFFF) as u16) as i16 as i32;
                let mut pt = POINT { x: sx, y: sy };
                let _ = ScreenToClient(hwnd, &mut pt);
                let zones = shared.borrow().caption_buttons.clone();
                match caption_zone_at(pt.x as f32, pt.y as f32, &zones) {
                    Some(oppa::CaptionButton::Minimize) => unsafe {
                        let _ = ShowWindow(hwnd, SW_MINIMIZE);
                        return LRESULT(0);
                    },
                    Some(oppa::CaptionButton::Maximize) => unsafe {
                        let maximized = IsZoomed(hwnd).as_bool();
                        let _ = ShowWindow(hwnd, if maximized { SW_RESTORE } else { SW_MAXIMIZE });
                        return LRESULT(0);
                    },
                    Some(oppa::CaptionButton::Close) => {
                        shared
                            .borrow_mut()
                            .queue
                            .push_back(ShellEvent::CloseRequested);
                        return LRESULT(0);
                    }
                    None => {}
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_NCLBUTTONDBLCLK => {
            // Decision 488: a double-click over a caption zone follows
            // a single-click that already acted — swallow it so the
            // action doesn't fire twice (no maximize-flicker).
            // Elsewhere keeps default handling (caption
            // double-click zoom).
            if shared.borrow().style.is_frameless() {
                let sx = ((lparam.0 & 0xFFFF) as u16) as i16 as i32;
                let sy = (((lparam.0 >> 16) & 0xFFFF) as u16) as i16 as i32;
                let mut pt = POINT { x: sx, y: sy };
                let _ = ScreenToClient(hwnd, &mut pt);
                let zones = shared.borrow().caption_buttons.clone();
                if caption_zone_at(pt.x as f32, pt.y as f32, &zones).is_some() {
                    return LRESULT(0);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_NCMOUSELEAVE => {
            // Decision 486: the cursor left the window — clear a live
            // caption hover so no in-scene glow sticks (the NCHITTEST
            // arm re-arms tracking on the next hover entry).
            shared.borrow_mut().hover_caption = None;
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_SETCURSOR => {
            // Round 8.3 (decision 299): the runner publishes the
            // hovered style's cursor through `set_cursor`; the OS
            // queries here per mouse move, so this applies the stored
            // shape now (eager `SetCursor` calls would die at the next
            // `DefWindowProcW` reset). Returning nonzero claims the
            // message — the class arrow never overrides us.
            //
            // Fix-up (resize cursors): the claim covers the client
            // area only (`HTCLIENT`). Border hits (sizing edges and
            // corners) fall through to `DefWindowProcW`, which owns
            // the resize arrows — claiming them would pin the
            // framework cursor over the borders and hide the
            // resize affordance while resizing still works.
            const HTCLIENT: isize = 1;
            if (lparam.0 & 0xFFFF) != HTCLIENT {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let cursor = shared
                .borrow()
                .current_cursor
                .unwrap_or(oppa::CursorIcon::Default);
            if let Ok(handle) = LoadCursorW(None, cursor_idc(cursor)) {
                let _ = SetCursor(Some(handle));
            }
            LRESULT(1)
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN | WM_RBUTTONDBLCLK | WM_MBUTTONDOWN => {
            // Round 9.2: all three buttons classify (the `*DBLCLK`
            // flag stays informational — multi-click chains
            // synthesize uniformly router-side; the class never
            // enabled `CS_DBLCLKS`, so doubles arrive as DOWN pairs
            // anyway — stated, not silent).
            let dbl_click = msg == WM_LBUTTONDBLCLK || msg == WM_RBUTTONDBLCLK;
            let button = match msg {
                WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => oppa::PointerButton::Secondary,
                WM_MBUTTONDOWN => oppa::PointerButton::Auxiliary,
                _ => oppa::PointerButton::Primary,
            };
            // Mouse capture (Round 7.19, decision 294): drags keep
            // streaming `WM_MOUSEMOVE` even when the cursor leaves
            // the client area (slider drags to the ends, press
            // slop-gates) instead of dropping events at the border.
            // Best-effort like every other HWND effect here.
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
            shared
                .borrow_mut()
                .queue
                .push_back(ShellEvent::PointerDown {
                    button,
                    dbl_click,
                    shift: shift_down,
                    x,
                    y,
                });
            LRESULT(0)
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
            let button = match msg {
                WM_RBUTTONUP => oppa::PointerButton::Secondary,
                WM_MBUTTONUP => oppa::PointerButton::Auxiliary,
                _ => oppa::PointerButton::Primary,
            };
            {
                let mut s = shared.borrow_mut();
                s.queue.push_back(ShellEvent::PointerUp {
                    button,
                    x,
                    y,
                    shift: shift_down,
                });
                // Clear the in-flight flag BEFORE releasing: the
                // release itself sends `WM_CAPTURECHANGED`, and that
                // handler must stay quiet for our own release (it
                // only fires on external steals while down).
                s.pending_drag = None;
            }
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            // External capture steal while a button was down (Alt+Tab,
            // a modal loop, another window's SetCapture): the router's
            // tripwire, so pressed flags never stick. Quiet when no
            // drag was in flight — notably right after our own
            // `WM_LBUTTONUP` release, which pre-cleared the flag.
            let mut s = shared.borrow_mut();
            if s.pending_drag.take().is_some() {
                s.queue.push_back(ShellEvent::PointerCancel);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let left_down = wparam.0 & (MK_LBUTTON.0 as usize) != 0;
            shared
                .borrow_mut()
                .queue
                .push_back(ShellEvent::PointerMove { x, y, left_down });
            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            shared.borrow_mut().queue.push_back(ShellEvent::KeyDown {
                vk: wparam.0 as u32,
                shift: shift_down,
                ctrl: ctrl_down,
            });
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            // Delta rides the high word (signed 16-bit); the
            // position packs screen (not client) coords — convert at
            // message time, before anything can move the window.
            // Shift+wheel rolls horizontally (see `wheel_event`).
            let delta = ((wparam.0 >> 16) as u16) as i16;
            let mut pt = POINT {
                x: client_x(lparam),
                y: client_y(lparam),
            };
            let _ = ScreenToClient(hwnd, &mut pt);
            shared
                .borrow_mut()
                .queue
                .push_back(wheel_event(pt.x, pt.y, delta, shift_down));
            LRESULT(0)
        }
        WM_MOUSEHWHEEL => {
            // Horizontal tilt wheel (Round 20.2, decision 325): same
            // packing as WM_MOUSEWHEEL (signed high-word delta,
            // screen-coords position converted at message time) —
            // queued as HWheel so the Cmd layer converts scale+sign.
            let delta = ((wparam.0 >> 16) as u16) as i16;
            let mut pt = POINT {
                x: client_x(lparam),
                y: client_y(lparam),
            };
            let _ = ScreenToClient(hwnd, &mut pt);
            shared.borrow_mut().queue.push_back(ShellEvent::HWheel {
                x: pt.x,
                y: pt.y,
                delta,
            });
            LRESULT(0)
        }
        WM_CHAR => {
            if let Some(ch) = char_from_wparam(wparam) {
                shared.borrow_mut().queue.push_back(ShellEvent::Char { ch });
            }
            LRESULT(0)
        }
        WM_SETFOCUS => {
            shared
                .borrow_mut()
                .queue
                .push_back(ShellEvent::FocusChanged(true));
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_KILLFOCUS => {
            shared
                .borrow_mut()
                .queue
                .push_back(ShellEvent::FocusChanged(false));
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_DPICHANGED => {
            // Monitor DPI change (Round 2.4, OQ-G10-2): snapshot the
            // suggested rect (screen coords, incl. frame — the RECT
            // dies with the call) and the x-axis DPI (monitors
            // report square pixels; a split DPI uses x — stated).
            // The runner applies the rect (`SetWindowPos`) and
            // re-bases the pipeline (DPR + resize + repaint).
            let rect = unsafe { *(lparam.0 as *const RECT) };
            let dpi = (wparam.0 & 0xFFFF) as u32;
            shared.borrow_mut().queue.push_back(ShellEvent::DpiChanged {
                x: rect.left,
                y: rect.top,
                w: rect.right - rect.left,
                h: rect.bottom - rect.top,
                dpi,
            });
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_SETTINGCHANGE => {
            // OS theme flip (Round 16.2, decision 315): `lParam`
            // names the changed section — only "ImmersiveColorSet"
            // queues (every other section stays a DefWindowProcW
            // no-op; the runner re-queries the registry on drain).
            if unsafe { crate::theme::is_immersive_color_set(lparam) } {
                shared
                    .borrow_mut()
                    .queue
                    .push_back(ShellEvent::SystemThemeChanged);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_GETMINMAXINFO => {
            // Runtime min/max sizes (Round 16.3, decision 316):
            // clamp only the sides the app constrained — unset
            // sides keep the OS defaults (never shrink an
            // unconstrained side to zero). Returning 0 (not
            // `DefWindowProcW`) is the documented contract here.
            let mmi = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
            let s = shared.borrow();
            if let Some((w, h)) = s.min_size {
                mmi.ptMinTrackSize.x = mmi.ptMinTrackSize.x.max(w as i32);
                mmi.ptMinTrackSize.y = mmi.ptMinTrackSize.y.max(h as i32);
            }
            if let Some((w, h)) = s.max_size {
                mmi.ptMaxTrackSize.x = mmi.ptMaxTrackSize.x.min(w as i32);
                mmi.ptMaxTrackSize.y = mmi.ptMaxTrackSize.y.min(h as i32);
            }
            LRESULT(0)
        }
        WM_SIZE => {
            // Live resize inside the modal sizing loop (Round 7.20,
            // decision 295): while the border drag traps the thread
            // in `DefWindowProcW`, the outer pump never runs — so the
            // runner's resize hook runs HERE, synchronously, on every
            // size step (reflow + repaint + swapchain reconfigure +
            // present, zero DWM stretching). `lParam` packs the new
            // client size exactly like the mouse messages (see
            // `client_x` / `client_y`); minimized (1 =
            // SIZE_MINIMIZED) and zero sizes never reach the hook (no
            // surface exists for them). `DefWindowProcW` still runs
            // so the OS sizing state updates normally.
            let w = client_x(lparam);
            let h = client_y(lparam);
            let minimized = wparam.0 == 1;
            if !minimized && w > 0 && h > 0 {
                let cb = shared.borrow().resize_callback.clone();
                if let Some(cb) = cb {
                    cb(w as u32, h as u32);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_PAINT => {
            // Debug-grade: no GDI painting (the wgpu surface covers the
            // client area); just validate.
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_CLOSE => {
            // Close veto (Round 16.3, decision 316): queue instead
            // of destroying — the runner approves through the
            // loop's close handler (`destroy_window` below) and a
            // vetoed close leaves the window running, so an
            // unsaved-changes modal can mount instead of dying.
            // (Never `DefWindowProcW` here — its default destroys.)
            shared
                .borrow_mut()
                .queue
                .push_back(ShellEvent::CloseRequested);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
