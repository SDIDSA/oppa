// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

use super::*;

/// Pixels per vertical wheel notch (Round 24.2): one `WM_MOUSEWHEEL`
/// notch is `WHEEL_DELTA` (120) units, and one notch moves one line =
/// 120 px — exactly the horizontal arm's effective scale (decision
/// 325). Sign is applied at conversion (`ShellEvent::Wheel` →
/// `Cmd::Scroll`): Win32 negative delta is wheel-down, which grows the
/// framework scroll offset (positive `dy`, matching the vertical
/// `ScrollArea` sign convention).
pub const WHEEL_LINE_PX: f32 = 120.0;

/// Pixels per horizontal tilt-wheel notch (Round 20.2, decision 325):
/// one `WM_MOUSEHWHEEL` notch is `WHEEL_DELTA` (120) units, and one
/// notch moves one line = 120 px — exactly the vertical arm's
/// effective scale (Round 24.2 converts one notch to
/// [`WHEEL_LINE_PX`]), so tilt and roll move content at the same
/// rate. Sign is applied at conversion
/// (`ShellEvent::HWheel` → `Cmd::Scroll`): Win32 positive delta is
/// tilt-right, which scrolls content left (negative `dx`, matching
/// the vertical `ScrollArea` sign convention where positive `dy`
/// grows the offset).
pub const HWHEEL_LINE_PX: f32 = 120.0;

/// One real input event from the window proc, with the payload the M0
/// `Event` enum cannot carry yet (positions, keys). IME messages carry
/// their snapshot data (see [`ImeMessage`]).
#[derive(Clone, Debug, PartialEq)]
pub enum ShellEvent {
    /// Button press (`dbl_click` from `WM_*BUTTONDBLCLK`), client
    /// coords; modifier state read at event time. Round 9.2: all
    /// three buttons classify here (left/primary, right/secondary,
    /// middle/auxiliary) — the router owns the taxonomy.
    PointerDown {
        button: oppa::PointerButton,
        dbl_click: bool,
        shift: bool,
        x: i32,
        y: i32,
    },
    PointerUp {
        button: oppa::PointerButton,
        x: i32,
        y: i32,
        /// Shift held at release (Round 8.2 — Shift+Click extends the
        /// field selection through the router's tap modifiers).
        shift: bool,
    },
    PointerMove {
        x: i32,
        y: i32,
        left_down: bool,
    },
    /// Mouse capture lost while the button was logically down
    /// (`WM_CAPTURECHANGED` with an in-flight drag — an external
    /// steal such as Alt+Tab, never our own `ReleaseCapture` on
    /// button-up, which clears the in-flight flag first). Maps to
    /// [`Cmd::PointerCancel`] so the router releases capture,
    /// long-press arms, and pressed flags instead of sticking them.
    PointerCancel,
    /// WM_KEYDOWN / WM_SYSKEYDOWN; modifiers read at event time.
    KeyDown {
        vk: u32,
        shift: bool,
        ctrl: bool,
    },
    /// WM_MOUSEWHEEL (decision 250): client coords converted at
    /// message time (wheel position arrives in *screen* coords,
    /// unlike the button messages) + signed delta in WHEEL_DELTA
    /// units. Raw integer layer like every sibling — `Cmd` carries
    /// the converted floats. Shift-held ticks arrive here as
    /// [`ShellEvent::HWheel`] instead (Phase 36 PR2b — the proc
    /// routes on the sampled Shift state, same message).
    Wheel {
        x: i32,
        y: i32,
        delta: i16,
    },
    /// WM_MOUSEHWHEEL (Round 20.2, decision 325): same shape as
    /// [`ShellEvent::Wheel`] but the horizontal tilt wheel —
    /// positive delta is tilt-right. `Cmd` converts to `dx` (with
    /// `dy: 0.0`) at the [`Cmd::Scroll`] documented scale+sign, so
    /// the raw layer stays delta-faithful like its sibling.
    HWheel {
        x: i32,
        y: i32,
        delta: i16,
    },
    /// WM_CHAR (only keys the IME does not consume reach this).
    Char {
        ch: char,
    },
    /// WM_SETFOCUS (true) / WM_KILLFOCUS (false).
    FocusChanged(bool),
    /// WM_DPICHANGED (Round 2.4, OQ-G10-2): suggested window rect
    /// (screen coords, incl. frame) + new DPI, snapshotted at
    /// message time (the RECT dies with the call).
    DpiChanged {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        dpi: u32,
    },
    /// WM_SETTINGCHANGE naming `"ImmersiveColorSet"` (Round 16.2,
    /// decision 315 — the OS light/dark flip; the section name is
    /// classified at message time, the runner re-queries the
    /// registry when this drains).
    SystemThemeChanged,
    /// A real OS IME message, snapshotted at message time.
    Ime(ImeMessage),
    /// WM_CLOSE (Round 16.3, decision 316 — queued, never
    /// destroyed inline: the runner approves through the loop's
    /// close handler and destroys explicitly).
    CloseRequested,
}

/// Maps one `WM_MOUSEWHEEL` tick to its queued event (Phase 36 PR2b,
/// decision 354): a Shift-held tick rolls horizontally (the native
/// convention — browsers and listviews do the same) and queues as
/// `HWheel` with the same delta, so the Cmd layer converts
/// scale+sign exactly like the tilt wheel; otherwise `Wheel`. Pure
/// over the sampled Shift state (the proc samples it at message
/// time) — headless-testable without synthesizing key state.
pub(crate) fn wheel_event(x: i32, y: i32, delta: i16, shift: bool) -> ShellEvent {
    if shift {
        ShellEvent::HWheel { x, y, delta }
    } else {
        ShellEvent::Wheel { x, y, delta }
    }
}

/// Payload-shaped commands, drained by the registered field handler in
/// event order. Some shell events carry no command (their M0 kind is
/// still dispatched).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    Click {
        dbl_click: bool,
        shift: bool,
        x: i32,
        y: i32,
    },
    Drag {
        from_x: f32,
        to_x: f32,
    },
    /// Discrete pointer press at client px (Round 7.19, decision
    /// 294): the button-down pipeline feed — the router's pointer
    /// Down (focus + press + capture owner), so sliders drag and
    /// buttons hold their pressed state on Windows like they already
    /// do on Linux/Android. `shift` rides to the router's tap
    /// modifiers (Round 8.2 — Shift+Click extends field selections);
    /// `button` rides the tap taxonomy (Round 9.2 — secondary taps
    /// dispatch menu events, never presses); the `dbl_click` OS flag
    /// stays informational (multi-click chains synthesize uniformly
    /// router-side).
    PointerDown {
        x: f32,
        y: f32,
        shift: bool,
        button: oppa::PointerButton,
    },
    /// Discrete pointer move at client px: every `WM_MOUSEMOVE`
    /// feeds the router's pointer Move (drag notification +
    /// capture-owner dispatch).
    PointerMove {
        x: f32,
        y: f32,
    },
    /// Discrete pointer release at client px: button-up feeds the
    /// router's pointer Up (tap / drag-release classification).
    /// `shift` rides to the tap modifiers (Round 8.2); `button`
    /// pairs with the Down by pointer id (Round 9.2).
    PointerUp {
        x: f32,
        y: f32,
        shift: bool,
        button: oppa::PointerButton,
    },
    /// Capture lost mid-drag (`WM_CAPTURECHANGED` with an in-flight
    /// button): the router's global tripwire — releases every
    /// capture, arm, and pressed flag, never a stuck press.
    PointerCancel,
    Key {
        vk: u32,
        shift: bool,
        ctrl: bool,
    },
    /// Wheel scroll at client px (decision 250, corrected Round 24.2):
    /// `dy` arrives in `Cmd::Scroll` as converted px — wheel-down is
    /// positive at [`WHEEL_LINE_PX`] per notch — while the queued
    /// [`ShellEvent::Wheel`] keeps the raw signed `WHEEL_DELTA` units.
    /// `dx` carries the Round 20.2 horizontal tilt conversion
    /// (`WM_MOUSEHWHEEL` — see [`HWHEEL_LINE_PX`]), 0.0 for vertical
    /// wheel ticks.
    Scroll {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
    Char {
        ch: char,
    },
    FocusChanged(bool),
    /// Monitor DPI change (Round 2.4): suggested window rect (x/y
    /// apply at the HWND in the runner via `SetWindowPos`; the
    /// pipeline consumes w/h/dpi) + new DPI.
    DpiChanged {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        dpi: u32,
    },
    /// OS light/dark flip (Round 16.2, decision 315 — the runner
    /// re-queries the registry into the reactive theme signal).
    SystemThemeChanged,
    /// Window close request (Round 16.3, decision 316 — the runner
    /// consults the loop's close veto: approved closes destroy
    /// through the normal flow, vetoed closes keep running).
    CloseRequested,
}

/// One raw IME message's data, snapshotted at message time. `comp` is the
/// GCS_COMPSTR string (UTF-16 → UTF-8), `attrs` the GCS_COMPATTR bytes,
/// `cursor_pos`/`delta_start` the GCS_CURSORPOS/GCS_DELTASTART values as
/// returned by the API, `result` the GCS_RESULTSTR string on commit.
#[derive(Clone, Debug, PartialEq)]
pub enum ImeMessage {
    StartComposition,
    Composition {
        gcs_flags: u32,
        comp: Option<String>,
        attrs: Option<Vec<u8>>,
        cursor_pos: i32,
        delta_start: i32,
        result: Option<String>,
    },
    EndComposition,
    Notify {
        code: u32,
        param: usize,
    },
    SetContext {
        f_select: bool,
        data: usize,
    },
}

/// One recorded Win32 message (the manual pass's raw archaeology log).
#[derive(Clone, Copy, Debug)]
pub struct MessageRecord {
    pub seq: u32,
    pub msg: u32,
    pub wparam: usize,
    pub lparam: isize,
}

pub(crate) fn key_down(vk: u16) -> bool {
    unsafe { GetKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

pub(crate) fn client_x(lparam: LPARAM) -> i32 {
    (lparam.0 as u32 & 0xFFFF) as u16 as i16 as i32
}

pub(crate) fn client_y(lparam: LPARAM) -> i32 {
    ((lparam.0 as u32 >> 16) & 0xFFFF) as u16 as i16 as i32
}

/// WM_CHAR packs the char in wParam; UTF-32 code points arrive directly,
/// UTF-16 surrogates as half a pair (0xD800..=0xDFFF) — dropped here
/// (debug-grade; the pass exercises no astral *typed* input).
pub(crate) fn char_from_wparam(wparam: WPARAM) -> Option<char> {
    let code = (wparam.0 & 0xFFFF) as u32;
    if (0xD800..=0xDFFF).contains(&code) {
        None
    } else {
        char::from_u32(code)
    }
}

/// Reads the composition snapshot the GCS flags in lParam name, via the
/// real IMM API (`ImmGetCompositionStringW`). Reads at message time; the
/// composition state can move after the message is handled.
pub(crate) unsafe fn read_composition(hwnd: HWND, flags: u32) -> ImeMessage {
    let himc = ImmGetContext(hwnd);
    if himc.is_invalid() {
        return ImeMessage::Composition {
            gcs_flags: flags,
            comp: None,
            attrs: None,
            cursor_pos: -1,
            delta_start: -1,
            result: None,
        };
    }
    let read_string = |which: IME_COMPOSITION_STRING| -> Option<String> {
        let len = ImmGetCompositionStringW(himc, which, None, 0);
        if len < 0 {
            return None;
        }
        let mut buf = vec![0u8; len as usize + 2];
        let got =
            ImmGetCompositionStringW(himc, which, Some(buf.as_mut_ptr().cast()), buf.len() as u32);
        if got < 0 {
            return None;
        }
        let units = (got as usize / 2).min(len as usize / 2);
        let mut u16s = Vec::with_capacity(units);
        for i in 0..units {
            u16s.push(u16::from_le_bytes([buf[i * 2], buf[i * 2 + 1]]));
        }
        Some(String::from_utf16_lossy(&u16s))
    };
    let comp = if flags & GCS_COMPSTR.0 != 0 {
        read_string(GCS_COMPSTR)
    } else {
        None
    };
    let result = if flags & GCS_RESULTSTR.0 != 0 {
        read_string(GCS_RESULTSTR)
    } else {
        None
    };
    let attrs = if flags & GCS_COMPATTR.0 != 0 {
        let len = ImmGetCompositionStringW(himc, GCS_COMPATTR, None, 0);
        if len >= 0 {
            let mut buf = vec![0u8; len as usize];
            let got = ImmGetCompositionStringW(
                himc,
                GCS_COMPATTR,
                Some(buf.as_mut_ptr().cast()),
                buf.len() as u32,
            );
            if got >= 0 {
                buf.truncate(got as usize);
                Some(buf)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    let cursor_pos = if flags & GCS_CURSORPOS.0 != 0 {
        ImmGetCompositionStringW(himc, GCS_CURSORPOS, None, 0)
    } else {
        -1
    };
    let delta_start = if flags & GCS_DELTASTART.0 != 0 {
        ImmGetCompositionStringW(himc, GCS_DELTASTART, None, 0)
    } else {
        -1
    };
    let _ = ImmReleaseContext(hwnd, himc);
    ImeMessage::Composition {
        gcs_flags: flags,
        comp,
        attrs,
        cursor_pos,
        delta_start,
        result,
    }
}

/// The command a shell event maps to, if any. M0's `Event` enum carries
/// only kind + handler; the payload channel is the cmds queue, drained
/// 1:1 by the registered field handler in event order. `pending_drag`
/// tracks the press x of an in-flight drag (client px) — the
/// `WM_CAPTURECHANGED` tripwire (an external steal while down).
///
/// Pointer events map twice by design (Round 7.19, decision 294): the
/// discrete `Cmd::Pointer{Down,Move,Up}` feed drives the reactive
/// router (press/capture/drag parity with Linux/Android), while the
/// legacy `Cmd::Click` still carries the dbl_click/shift variants the
/// pipeline cannot express. `Cmd::Drag` is no longer emitted (the 1D
/// from/to collapse cannot address the router); the variant stays for
/// backward compatibility.
pub(crate) fn cmd_of(ev: &ShellEvent, pending_drag: &mut Option<i32>) -> Option<Cmd> {
    match ev {
        ShellEvent::PointerDown {
            x,
            y,
            shift,
            button,
            ..
        } => {
            *pending_drag = Some(*x);
            // Discrete feed: the router's pointer Down (focus +
            // press + capture owner). The legacy `Cmd::Click` tap is
            // no longer emitted here — its synthetic down+up would
            // double-drive the router now that `drive_cmd` steps
            // PointerDown for real. The `Click`/`Drag` variants stay
            // constructible for backward compatibility.
            Some(Cmd::PointerDown {
                x: *x as f32,
                y: *y as f32,
                shift: *shift,
                button: *button,
            })
        }
        ShellEvent::PointerUp {
            x,
            y,
            shift,
            button,
        } => {
            *pending_drag = None;
            Some(Cmd::PointerUp {
                x: *x as f32,
                y: *y as f32,
                shift: *shift,
                button: *button,
            })
        }
        ShellEvent::PointerMove { x, y, .. } => Some(Cmd::PointerMove {
            x: *x as f32,
            y: *y as f32,
        }),
        ShellEvent::PointerCancel => {
            *pending_drag = None;
            Some(Cmd::PointerCancel)
        }
        ShellEvent::KeyDown { vk, shift, ctrl } => Some(Cmd::Key {
            vk: *vk,
            shift: *shift,
            ctrl: *ctrl,
        }),
        ShellEvent::Wheel { x, y, delta } => Some(Cmd::Scroll {
            x: *x as f32,
            y: *y as f32,
            dx: 0.0,
            // Win32 wheel-down is negative and scrolls content up:
            // dy = -(delta / 120) * LINE_PX (Round 24.2 — one notch
            // moves one line at the horizontal arm's scale).
            dy: -(*delta as f32 / 120.0) * WHEEL_LINE_PX,
        }),
        ShellEvent::HWheel { x, y, delta } => Some(Cmd::Scroll {
            x: *x as f32,
            y: *y as f32,
            // Win32 tilt-right is positive and scrolls content left:
            // dx = -(delta / 120) * LINE_PX (decision 325 — one notch
            // moves one line at the vertical arm's scale).
            dx: -(*delta as f32 / 120.0) * HWHEEL_LINE_PX,
            dy: 0.0,
        }),
        ShellEvent::Char { ch } => Some(Cmd::Char { ch: *ch }),
        ShellEvent::FocusChanged(f) => Some(Cmd::FocusChanged(*f)),
        ShellEvent::DpiChanged { x, y, w, h, dpi } => Some(Cmd::DpiChanged {
            x: *x,
            y: *y,
            w: *w,
            h: *h,
            dpi: *dpi,
        }),
        ShellEvent::SystemThemeChanged => Some(Cmd::SystemThemeChanged),
        ShellEvent::CloseRequested => Some(Cmd::CloseRequested),
        ShellEvent::Ime(_) => None,
    }
}

/// The M0 event kind a shell event classifies as (the trait's normalized
/// stream; payloads ride the cmds queue).
pub(crate) fn kind_of(ev: &ShellEvent) -> EventKind {
    match ev {
        ShellEvent::PointerDown { .. } => EventKind::Press,
        ShellEvent::PointerUp { .. } => EventKind::Release,
        ShellEvent::PointerMove { .. } => EventKind::PointerMove,
        ShellEvent::PointerCancel => EventKind::Release,
        ShellEvent::KeyDown { .. } | ShellEvent::Char { .. } => EventKind::Key,
        ShellEvent::Wheel { .. } | ShellEvent::HWheel { .. } => EventKind::Scroll,
        ShellEvent::FocusChanged(true) => EventKind::Focus,
        ShellEvent::FocusChanged(false) => EventKind::Blur,
        ShellEvent::DpiChanged { .. } => EventKind::DpiChanged,
        ShellEvent::SystemThemeChanged => EventKind::SystemTheme,
        ShellEvent::CloseRequested => EventKind::CloseRequested,
        ShellEvent::Ime(_) => EventKind::Ime,
    }
}
