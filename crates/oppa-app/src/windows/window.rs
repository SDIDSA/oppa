// ---------------------------------------------------------------------------
// Window helpers (probes, cursor, sizing, DPI)
// ---------------------------------------------------------------------------

use super::*;

/// Live async key state (the shell samples shift/ctrl at proc time;
/// alt is not carried by `Cmd`, so the runner samples it here at
/// dispatch — the pump runs every ~8ms while shortcut keys are held.
/// Same bit test as the shell's own `key_down`).
pub(crate) fn key_down(vk: u16) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
    unsafe { GetKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

/// Active-work wait bound in ms (Round 9.1, decision 300): the pre-9.1
/// poll granularity, now a deadline bound instead of a spin — holds
/// fire, worker outboxes drain, and transition tails retire within
/// this of becoming ready. Settled loops block indefinitely (near-0%
/// CPU), so this never sets the idle cadence, only the active one.
const ACTIVE_WAIT_MS: u32 = 8;

/// Wait horizon for the event-driven loop (Round 9.1 — pure,
/// headless-testable): `None` blocks until the next OS message
/// (settled: no frame demand, no live interpolations, no worker
/// traffic, no armed holds); `Some(ACTIVE_WAIT_MS)` ticks while
/// background work is live. Long-press arms never create frame
/// demand by design (G11 — a held finger neither spins the loop nor
/// hangs it), so they are polled explicitly here; flings likewise
/// (Round 10.2 — momentum ticks explicitly, never self-demand).
/// Round 15.2 (decision 313): a focused caret flips without input,
/// so the horizon also wakes at the next flip (ceiled to whole ms —
/// a due flip wakes in 1ms, repaints, and re-arms ~500ms out, never
/// a spin); the earliest live deadline wins.
pub(crate) fn wait_timeout_ms(host: &oppa::ComponentHost) -> Option<u32> {
    let background = host.runtime().has_demand()
        || host.longpress_armed_count() > 0
        || host.with_evaluator(|ev| ev.active_count()) > 0
        || host.fling_count() > 0;
    let blink_ms = host
        .caret_blink_in_secs()
        .map(|s| (s * 1000.0).ceil().max(1.0) as u32);
    // Round 21.1 (decision 328): component timers join the horizon
    // (ceiled to whole ms like the blink arm — a due timer wakes in
    // 1ms, fires, and re-arms at its cadence, never a spin).
    let now_ms = host.now_ms();
    let timer_ms = host
        .next_timer_due_ms(now_ms)
        .map(|due| ((due - now_ms).max(0.0).ceil().max(1.0)) as u32);
    // Earliest live deadline wins (any arm absent drops out).
    let mut horizon = None;
    for ms in [background.then_some(ACTIVE_WAIT_MS), blink_ms, timer_ms]
        .into_iter()
        .flatten()
    {
        horizon = Some(horizon.map_or(ms, |h: u32| h.min(ms)));
    }
    horizon
}

/// Publishes the hovered style's cursor to the shell (Round 8.3,
/// decision 299): hover resolution lives core-side
/// (`hover_cursor`); the shell owns the OS cursor. Unstyled hover
/// falls back to the arrow — leaving a field restores it.
pub(crate) fn sync_cursor(loop_: &DesktopLoop, shell: &mut Win32Shell) {
    shell.set_cursor(
        loop_
            .host()
            .hover_cursor()
            .unwrap_or(oppa::CursorIcon::Default),
    );
}

/// Grows the frame so the client area is exactly the scene size
/// (`CreateWindowExW` takes outer dims — without this the client is
/// smaller than the pixmap and every present clips; decision 202 in
/// the fps-demo, demo-side policy copied here).
pub(crate) fn fit_client_to_scene(hwnd: windows::Win32::Foundation::HWND, w: u32, h: u32) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRect, GetWindowLongW, SetWindowPos, GWL_STYLE, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOZORDER, WINDOW_STYLE,
    };
    unsafe {
        let style = WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32);
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: w as i32,
            bottom: h as i32,
        };
        let _ = AdjustWindowRect(&mut rc, style, false);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            rc.right - rc.left,
            rc.bottom - rc.top,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Whether the window still exists (Round 19.8 root-cause guard:
/// 19.7's empty-capabilities reconfigure + GetDC FATAL were one
/// teardown race — `WM_CLOSE` destroys the HWND synchronously inside
/// `drive_cmd`, and the loop-bottom poll then read the dead window
/// as 1x1 and reconfigured + presented into it. A dead window exits
/// the loop cleanly; a live-window failure stays loud FATAL).
pub(crate) fn hwnd_alive(hwnd: windows::Win32::Foundation::HWND) -> bool {
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)).as_bool() }
}

pub(crate) fn live_client_size(hwnd: windows::Win32::Foundation::HWND) -> (u32, u32) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
    unsafe {
        let mut cr = RECT::default();
        let _ = GetClientRect(hwnd, &mut cr);
        (
            (cr.right - cr.left).max(1) as u32,
            (cr.bottom - cr.top).max(1) as u32,
        )
    }
}

/// Raw (unclamped) client dimensions in px — negative/zero when the
/// window has no drawable area (minimized or destroyed windows read
/// zeros from `GetClientRect`).
pub(crate) fn raw_client_size(hwnd: windows::Win32::Foundation::HWND) -> (i32, i32) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
    unsafe {
        let mut cr = RECT::default();
        let _ = GetClientRect(hwnd, &mut cr);
        (cr.right - cr.left, cr.bottom - cr.top)
    }
}

/// Whether the window is minimized (iconic) per the OS.
pub(crate) fn window_minimized(hwnd: windows::Win32::Foundation::HWND) -> bool {
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsIconic(hwnd).as_bool() }
}

/// Headless-testable resize-skip decision (Round 20.2, decision 325):
/// a minimized window, or one whose client area is zero/negative,
/// has no drawable area — resizing the surface to the clamped 1x1
/// is a pointless reconfigure (the 19.8 minimize-thrash note), so
/// the loop-bottom poll skips it and keeps the pre-minimize
/// viewport for a clean restore. Destroyed windows never reach this
/// (the `hwnd_alive` break above runs first — a dead window exits,
/// it does not skip).
pub(crate) fn should_skip_resize_for_client_area(raw_w: i32, raw_h: i32, iconic: bool) -> bool {
    iconic || raw_w <= 0 || raw_h <= 0
}

/// OS-bridging skip check for the loop-bottom poll: `IsIconic` or a
/// zero-area `GetClientRect`, read before any 1x1 clamping.
pub(crate) fn client_area_missing(hwnd: windows::Win32::Foundation::HWND) -> bool {
    let (w, h) = raw_client_size(hwnd);
    should_skip_resize_for_client_area(w, h, window_minimized(hwnd))
}

/// Applies the OS-suggested bounds on a DPI crossing (Round 2.4):
/// move + resize the window in one `SetWindowPos` (keeping z-order
/// and activation — a DPI move must never restack or steal focus).
/// Best-effort like every other HWND effect here (the pipeline
/// re-bases independently, so a failed move degrades to the live
/// size poll, never a silent blank).
pub(crate) fn move_window_to_suggested(
    hwnd: windows::Win32::Foundation::HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            x,
            y,
            w.max(1),
            h.max(1),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Blits the loop's latest paint to the window client DC (RGBA →
/// BGRA + `SetDIBitsToDevice`). Loud on any failure (a present
/// failure is never a silent blank frame). Same pattern as the
/// fps-demo's `present_cpu` (the proven GDI path).
pub(crate) fn blit_rgba(
    loop_: &DesktopLoop,
    hwnd: windows::Win32::Foundation::HWND,
) -> Result<(), String> {
    use windows::Win32::Graphics::Gdi::{
        GetDC, ReleaseDC, SetDIBitsToDevice, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        RGBQUAD,
    };
    let rgba = loop_.rgba8()?;
    let (w, h) = loop_.viewport();
    if rgba.len() != w as usize * h as usize * 4 {
        return Err(format!("oppa-app: pixels {} != {w}x{h}x4", rgba.len()));
    }
    let mut bgra = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD {
            rgbBlue: 0,
            rgbGreen: 0,
            rgbRed: 0,
            rgbReserved: 0,
        }],
    };
    unsafe {
        let dc = GetDC(Some(hwnd));
        if dc.is_invalid() {
            // Round 19.8: the mid-loop fallback died here in 19.7 with a
            // bare "GetDC failed" (a never-exercised-on-GPU-boxes path).
            // Name the OS reason and the window's liveness — the fix
            // round's evidence depends on it.
            let code = windows::Win32::Foundation::GetLastError();
            let alive = windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)).as_bool();
            return Err(format!(
                "oppa-app: GetDC failed (last_error={code:?}, is_window={alive})"
            ));
        }
        let rows = SetDIBitsToDevice(
            dc,
            0,
            0,
            w,
            h,
            0,
            0,
            0,
            h,
            bgra.as_ptr() as *const core::ffi::c_void,
            &bmi,
            DIB_RGB_COLORS,
        );
        ReleaseDC(Some(hwnd), dc);
        if rows != h as i32 {
            return Err(format!(
                "oppa-app: SetDIBitsToDevice painted {rows} of {h} rows"
            ));
        }
    }
    Ok(())
}

/// Process DPI-awareness declaration (Round 6.2, decision 273):
/// without an explicit declaration Windows runs Win32 processes in
/// System-DPI unaware mode — bitmap-stretching the window on
/// High-DPI screens and never dispatching `WM_DPICHANGED` — which
/// leaves the Round-2.4 live-DPI engine dormant. Declares
/// Per-Monitor V2 first; where V2 is unavailable (older Windows 10
/// builds) falls back to per-monitor v1. Both OS calls refuse when
/// awareness is already declared (manifest or a previous call), so
/// the refusal arm is an expected status, never a panic — logged on
/// stderr per the loud-failure rule.
///
/// Must run before `CreateWindowExW` (called in [`run_windows`]
/// ahead of `Win32Shell::new`): awareness declared after window
/// creation does not apply to that window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DpiAwareness {
    /// Per-Monitor V2 declared — true `WM_DPICHANGED` delivery.
    PerMonitorV2,
    /// V2 unavailable; per-monitor v1 declared instead.
    PerMonitorV1,
    /// Both declarations refused (manifest-declared or already set).
    Unavailable,
}

pub fn ensure_dpi_awareness() -> DpiAwareness {
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwareness, SetProcessDpiAwarenessContext,
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, PROCESS_PER_MONITOR_DPI_AWARE,
    };
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_ok() {
            return DpiAwareness::PerMonitorV2;
        }
        // V2 unavailable on older Windows 10 builds — per-monitor
        // v1 still opts out of bitmap scaling.
        if SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE).is_ok() {
            return DpiAwareness::PerMonitorV1;
        }
    }
    eprintln!(
        "oppa-app: DPI awareness declaration refused (already declared via manifest or an \
         earlier call?) — the OS may bitmap-scale this window instead of sending WM_DPICHANGED"
    );
    DpiAwareness::Unavailable
}
