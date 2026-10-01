//! The real Win32 window shell (M1 remainder). One window, one purpose:
//! host an editable field and receive REAL input (mouse/keyboard/IME) —
//! not the spike's scripted feed. The window proc is wired to the M0
//! [`PlatformShell`] trait (`pump_events`, `set_ime` from M0b); the OS IME
//! messages are snapshotted at message time (`ImmGetCompositionString`
//! reads must happen at message time — the composition state can move
//! after the message) and routed through the *existing* pipeline: the
//! host-provided mapper constructs the normalized [`ImeCompositionEvent`]s
//! and dispatches them via `dispatch_ime_event` → the session's
//! `ImeCompositionHandler` (the same seam the spike proved with scripted
//! input; here the source is a real IME).
//!
//! Deliberately not here (round scope): menus, multi-window, resize polish,
//! theming, widget model. This is the vehicle for the manual real-IME
//! pass, nothing more.

use super::tsf::{TsfBridge, TsfStatus};
use super::ImeState;
use oppa::handlers::HandlerId;
use oppa::ime::ImeOps;
use oppa::shell::{Event, EventKind, PlatformShell};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetSysColorBrush, InvalidateRect, ScreenToClient, UpdateWindow,
    COLOR_WINDOW, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemServices::MK_LBUTTON;
use windows::Win32::UI::Input::Ime::{
    ImmGetCompositionStringW, ImmGetContext, ImmGetConversionStatus, ImmGetOpenStatus,
    ImmReleaseContext, ImmSetCandidateWindow, ImmSetCompositionWindow, CANDIDATEFORM,
    CFS_CANDIDATEPOS, CFS_POINT, COMPOSITIONFORM, GCS_COMPATTR, GCS_COMPSTR, GCS_CURSORPOS,
    GCS_DELTASTART, GCS_RESULTSTR, IME_COMPOSITION_STRING, IME_CONVERSION_MODE, IME_SENTENCE_MODE,
    ISC_SHOWUICOMPOSITIONWINDOW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, GetKeyboardLayout, VK_CONTROL, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::MINMAXINFO;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetForegroundWindow, LoadCursorW, PeekMessageW, PostQuitMessage, RegisterClassExW, SetCursor,
    SetForegroundWindow, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
    IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_IBEAM, IDC_NO, IDC_SIZEALL, IDC_SIZENS, IDC_SIZEWE, MSG,
    PM_REMOVE, SHOW_WINDOW_CMD, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CAPTURECHANGED, WM_CHAR,
    WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_GETMINMAXINFO, WM_IME_COMPOSITION,
    WM_IME_ENDCOMPOSITION, WM_IME_NOTIFY, WM_IME_SETCONTEXT, WM_IME_STARTCOMPOSITION, WM_KEYDOWN,
    WM_KILLFOCUS, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_QUIT, WM_RBUTTONDBLCLK,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS, WM_SETTINGCHANGE, WM_SIZE,
    WM_SYSKEYDOWN, WNDCLASSEXW, WNDCLASS_STYLES, WS_OVERLAPPEDWINDOW,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

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
fn wheel_event(x: i32, y: i32, delta: i16, shift: bool) -> ShellEvent {
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

#[derive(Clone, Debug, PartialEq)]
pub struct ShellConfig {
    pub title: String,
    pub width: i32,
    pub height: i32,
    /// Arm full message recording from launch.
    pub record_messages: bool,
    /// Suppress the OS-drawn composition window (the host draws the
    /// composition inline); the OS candidate UI stays, anchored by our
    /// `ImmSetCandidateWindow` calls.
    pub suppress_os_composition_window: bool,
    /// Show the window at creation (default true — all existing
    /// behavior). `false` keeps it hidden until [`Win32Shell::show`]:
    /// long GPU bring-up (Vello shader compile) then happens with no
    /// window at all instead of a blank unresponsive one.
    pub visible: bool,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            title: "oppa shell".to_string(),
            width: 640,
            height: 200,
            record_messages: false,
            suppress_os_composition_window: true,
            visible: true,
        }
    }
}

/// The field event handler id the M0 registry dispatches to.
pub const FIELD_EVENT: HandlerId =
    HandlerId::from_hash(oppa::hash::SymbolHash::of("spike.field.event"));

/// Outcome of [`Win32Shell::wait_for_input`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WaitOutcome {
    /// An OS message is ready (queue it via `process_os_messages`).
    Input,
    /// The finite bound elapsed with no message (tick background work).
    Timeout,
}

/// Win32 system cursor for a framework [`CursorIcon`](oppa::CursorIcon)
/// (Round 8.3, decision 299 — every variant names its stock cursor;
/// `Default` is the class arrow). Pure (headless-testable — the
/// `LoadCursorW` call itself runs at `WM_SETCURSOR` time).
fn cursor_idc(cursor: oppa::CursorIcon) -> windows::core::PCWSTR {
    match cursor {
        oppa::CursorIcon::Default => IDC_ARROW,
        oppa::CursorIcon::Pointer => IDC_HAND,
        oppa::CursorIcon::Text => IDC_IBEAM,
        oppa::CursorIcon::Crosshair => IDC_CROSS,
        oppa::CursorIcon::Move => IDC_SIZEALL,
        oppa::CursorIcon::NotAllowed => IDC_NO,
        oppa::CursorIcon::ColResize => IDC_SIZEWE,
        oppa::CursorIcon::RowResize => IDC_SIZENS,
    }
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

/// The host's IME mapper sink: one call per snapshotted real IME message.
pub type ImeSink = Rc<dyn Fn(&ImeMessage)>;

pub struct ShellShared {
    pub hwnd: HWND,
    pub cfg: ShellConfig,
    pub queue: VecDeque<ShellEvent>,
    pub cmds: VecDeque<Cmd>,
    pub msg_log: Vec<MessageRecord>,
    pub ime_log: Vec<String>,
    pub seq: u32,
    pub recording: bool,
    pub ime_callback: Option<ImeSink>,
    /// Press x of an in-flight pointer drag (client px), if any.
    pub pending_drag: Option<i32>,
    /// Live-resize hook (Round 7.20, decision 295): the runner
    /// installs this to reflow + repaint + re-present synchronously
    /// from `WM_SIZE` — inside the OS modal sizing loop, where the
    /// outer pump never runs and the content would otherwise freeze
    /// under DWM bitmap-stretching until release.
    pub resize_callback: Option<Rc<dyn Fn(u32, u32)>>,
    /// The anchoring log: every caret rect passed through `set_ime`
    /// (client px), in order — the candidate-window anchoring path's
    /// observable record.
    pub anchored_rects: Vec<[f32; 4]>,
    /// Current pointer cursor (Round 8.3, decision 299): published by
    /// the runner through `set_cursor`, applied at the next
    /// `WM_SETCURSOR` (the OS queries per mouse move — storing here
    /// instead of calling `SetCursor` eagerly, which the next
    /// `DefWindowProcW` pass would reset to the class cursor).
    /// `None` = the class arrow (untouched since launch).
    pub current_cursor: Option<oppa::CursorIcon>,
    /// Runtime resizable floor in client px (Round 16.3, decision
    /// 316): published by the runner through the window control,
    /// enforced at the next `WM_GETMINMAXINFO` (`None` = the OS
    /// default, untouched since launch).
    pub min_size: Option<(u32, u32)>,
    /// Runtime resizable ceiling in client px (Round 16.3, decision
    /// 316 — same publish/enforce rule as the floor).
    pub max_size: Option<(u32, u32)>,
}

pub struct Win32Shell {
    pub shared: Rc<RefCell<ShellShared>>,
    tsf: Option<TsfBridge>,
    clipboard: super::clipboard::Win32Clipboard,
    file_dialog: super::file_dialog::Win32FileDialog,
}

thread_local! {
    static SHELL_RC: RefCell<Option<Rc<RefCell<ShellShared>>>> = const { RefCell::new(None) };
}

impl Win32Shell {
    pub fn new(cfg: ShellConfig) -> Result<Self, String> {
        let shared = Rc::new(RefCell::new(ShellShared {
            hwnd: HWND::default(),
            cfg: cfg.clone(),
            queue: VecDeque::new(),
            cmds: VecDeque::new(),
            msg_log: Vec::new(),
            ime_log: Vec::new(),
            seq: 0,
            recording: cfg.record_messages,
            ime_callback: None,
            pending_drag: None,
            resize_callback: None,
            anchored_rects: Vec::new(),
            current_cursor: None,
            min_size: None,
            max_size: None,
        }));

        SHELL_RC.with(|rc| *rc.borrow_mut() = Some(shared.clone()));

        unsafe {
            let hinstance = HINSTANCE::from(GetModuleHandleW(None).unwrap());
            let class_name = w!("oppa_shell_window");
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: WNDCLASS_STYLES(CS_HREDRAW.0 | CS_VREDRAW.0),
                lpfnWndProc: Some(shell_wnd_proc),
                hInstance: hinstance,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                hbrBackground: GetSysColorBrush(COLOR_WINDOW),
                lpszClassName: class_name,
                ..Default::default()
            };
            RegisterClassExW(&wc);

            let mut title = cfg.title.encode_utf16().collect::<Vec<_>>();
            title.push(0);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name,
                windows::core::PCWSTR(title.as_ptr()),
                WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                cfg.width,
                cfg.height,
                None,
                None,
                Some(hinstance),
                None,
            )
            .unwrap_or_default();
            if hwnd.0.is_null() {
                return Err("CreateWindowExW failed".to_string());
            }
            shared.borrow_mut().hwnd = hwnd;

            if cfg.visible {
                let _ = ShowWindow(hwnd, SHOW_WINDOW_CMD(5));
                let _ = UpdateWindow(hwnd);
            }
        }

        // Copy the HWND out before construction (the struct takes
        // `shared` by move — borrowing it inside the literal would
        // collide with the move).
        let hwnd = shared.borrow().hwnd;
        Ok(Self {
            shared,
            tsf: None,
            clipboard: super::clipboard::Win32Clipboard::new(),
            file_dialog: super::file_dialog::Win32FileDialog::new(hwnd),
        })
    }

    /// Shows a window created with `visible: false` (same command as
    /// the creation-time show). Present one frame first so the window
    /// appears with content, never blank. The HWND is copied out
    /// before the call: `ShowWindow` dispatches synchronously into our
    /// wndproc, which borrows the same table (reentrant borrow = panic).
    pub fn show(&self) {
        let hwnd = self.shared.borrow().hwnd;
        unsafe {
            let _ = ShowWindow(hwnd, SHOW_WINDOW_CMD(5));
        }
    }

    /// TSF-aware association for the shell window (this round's fix).
    /// Activates `ITfThreadMgr`, creates + pushes a document-manager
    /// context backed by the real text store, associates it with the
    /// window and sets focus, advises the edit/composition sinks, and
    /// declares the `IS_TEXT` input scope. `init_text`/`init_sel` seed
    /// the store with the settled field state. Returns the bridge log
    /// lines for the pass's environment record (failures carry HRESULTs).
    /// On failure the error string + partial log are returned and no
    /// bridge is stored.
    pub fn enable_tsf(
        &mut self,
        init_text: &str,
        init_sel: (usize, usize),
    ) -> Result<Vec<String>, (String, Vec<String>)> {
        let hwnd = self.shared.borrow().hwnd;
        match TsfBridge::activate(hwnd, &self.shared, init_text, init_sel) {
            Ok(bridge) => {
                let mut bridge = bridge;
                let log = bridge.take_log();
                self.tsf = Some(bridge);
                Ok(log)
            }
            Err((e, log)) => Err((e, log)),
        }
    }

    /// Mirror settled session state into the TSF store (skipped while a
    /// TIP composition owns it). Returns the log line, or `None` when
    /// TSF was never enabled. Mirrored into the IME log like the
    /// reassert path.
    pub fn tsf_sync_external(&mut self, text: &str, sel: (usize, usize)) -> Option<String> {
        let line = self.tsf.as_mut().map(|b| b.sync_external(text, sel));
        if let Some(ref l) = line {
            self.shared.borrow_mut().ime_log.push(l.clone());
        }
        line
    }

    /// Drain the text store's per-call trace for the step notes.
    pub fn tsf_take_store_log(&mut self) -> Vec<String> {
        self.tsf
            .as_mut()
            .map(|b| b.take_store_log())
            .unwrap_or_default()
    }

    /// Re-assert TSF focus while the window is foreground (per pass step).
    /// Returns the log line, or `None` when TSF was never enabled. The
    /// line is also mirrored into the shell IME log (the raw record).
    pub fn tsf_reassert_focus(&mut self) -> Option<String> {
        let line = self.tsf.as_mut().map(|b| b.reassert_focus());
        if let Some(ref l) = line {
            self.shared.borrow_mut().ime_log.push(l.clone());
        }
        line
    }

    /// Note a focus change for the TSF document manager. Mirrored into
    /// the IME log like the reassert path.
    pub fn tsf_note_focus(&mut self, focused: bool) -> Option<String> {
        let line = self.tsf.as_mut().map(|b| b.note_focus(focused));
        if let Some(ref l) = line {
            self.shared.borrow_mut().ime_log.push(l.clone());
        }
        line
    }

    pub fn tsf_status(&self) -> Option<TsfStatus> {
        self.tsf.as_ref().map(|b| b.status())
    }

    pub fn tsf_enabled(&self) -> bool {
        self.tsf.is_some()
    }

    pub fn hwnd(&self) -> HWND {
        self.shared.borrow().hwnd
    }

    /// Destroys the window (Round 16.3, decision 316 — the approved
    /// close path: the runner calls this only after the loop's
    /// close handler approves; `WM_DESTROY` → quit then exits
    /// through the normal pump, never an abrupt kill).
    pub fn destroy_window(&self) {
        let hwnd = self.shared.borrow().hwnd;
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }

    pub fn set_ime_callback(&mut self, cb: Rc<dyn Fn(&ImeMessage)>) {
        self.shared.borrow_mut().ime_callback = Some(cb);
    }

    /// Installs the live-resize hook (Round 7.20, decision 295):
    /// fired synchronously from `WM_SIZE` with the new client size
    /// in px — including inside the modal sizing loop, where the
    /// outer pump is trapped in `DefWindowProcW` and never runs.
    pub fn set_resize_callback(&mut self, cb: Rc<dyn Fn(u32, u32)>) {
        self.shared.borrow_mut().resize_callback = Some(cb);
    }

    /// Drains the OS message queue (translate + dispatch). Real input
    /// enters the internal queue via the window proc. Returns `true`
    /// when WM_QUIT was retrieved (the loop should exit).
    pub fn process_os_messages(&mut self) -> bool {
        let mut quit = false;
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    quit = true;
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        quit
    }

    pub fn take_cmds(&mut self) -> Vec<Cmd> {
        self.shared.borrow_mut().cmds.drain(..).collect()
    }

    pub fn take_message_log(&mut self) -> Vec<MessageRecord> {
        std::mem::take(&mut self.shared.borrow_mut().msg_log)
    }

    pub fn take_ime_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.shared.borrow_mut().ime_log)
    }

    /// The anchoring log: every caret rect passed through `set_ime`
    /// (client px), in order — the candidate-window anchoring path's
    /// observable record.
    pub fn take_anchored_rects(&mut self) -> Vec<[f32; 4]> {
        std::mem::take(&mut self.shared.borrow_mut().anchored_rects)
    }

    /// Record a shell-side marker (driver step boundary) into the log.
    pub fn log_marker(&mut self, text: &str) {
        let mut shared = self.shared.borrow_mut();
        shared.seq += 1;
        let seq = shared.seq;
        shared.msg_log.push(MessageRecord {
            seq,
            msg: u32::MAX,
            wparam: 0,
            lparam: 0,
        });
        shared.ime_log.push(format!("# {text}"));
    }

    pub fn is_foreground(&self) -> bool {
        unsafe { GetForegroundWindow() == self.shared.borrow().hwnd }
    }

    /// The live IME state (open status, conversion/sentence mode, active
    /// HKL) — the pass logs this per step; the raw record of the IME's
    /// engagement at each point.
    pub fn ime_status(&self) -> ImeState {
        let hwnd = self.shared.borrow().hwnd;
        unsafe {
            let himc = ImmGetContext(hwnd);
            if himc.is_invalid() {
                return ImeState::default();
            }
            let mut state = ImeState {
                context_open: ImmGetOpenStatus(himc).as_bool(),
                ..ImeState::default()
            };
            let mut conv = IME_CONVERSION_MODE::default();
            let mut sent = IME_SENTENCE_MODE::default();
            if ImmGetConversionStatus(himc, Some(&mut conv), Some(&mut sent)).as_bool() {
                state.conversion_mode = conv.0;
                state.sentence_mode = sent.0;
            }
            let _ = ImmReleaseContext(hwnd, himc);
            state.active_hkl = GetKeyboardLayout(0).0 as usize;
            state
        }
    }

    pub fn focus_window(&mut self) -> bool {
        // Copy the HWND out first: SetForegroundWindow dispatches
        // synchronously (IMM/TSF re-enter the proc via SendMessage while it
        // runs), so holding the shared borrow across the call panics.
        let hwnd = self.shared.borrow().hwnd;
        unsafe { SetForegroundWindow(hwnd).as_bool() }
    }

    pub fn client_size(&self) -> (i32, i32) {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(self.shared.borrow().hwnd, &mut rect);
        }
        (rect.right - rect.left, rect.bottom - rect.top)
    }

    /// Live device pixel ratio for the shell window (G10, decision
    /// 226): per-monitor DPI via `GetDpiForWindow` over the 96 baseline
    /// (shared `dpr_from_dpi` rule — one conversion everywhere). A 0
    /// reading (failed call) falls back to 1.0: the window visibly
    /// exists, so 96 is the honest default, not a silent rescale —
    /// recorded in the ime log for the pass record.
    pub fn device_pixel_ratio(&mut self) -> f32 {
        let hwnd = self.shared.borrow().hwnd;
        let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) };
        if dpi == 0 {
            self.shared
                .borrow_mut()
                .ime_log
                .push("GetDpiForWindow returned 0 — dpr falls back to 1.0".to_string());
            return 1.0;
        }
        oppa::dpr_from_dpi(dpi)
    }

    pub fn invalidate(&mut self) {
        unsafe {
            let _ = InvalidateRect(Some(self.shared.borrow().hwnd), None, false);
        }
    }

    /// Blocks until an OS message arrives or `timeout_ms` elapses
    /// (Round 9.1, decision 300 — the event-driven wait replacing the
    /// 8ms poll): `None` blocks indefinitely (settled/idle — the loop
    /// drops to near-0% CPU, waking only on input); `Some(ms)` arms a
    /// one-shot high-resolution waitable timer (live transitions,
    /// worker traffic, or armed holds — bounded latency at the old
    /// poll granularity, then the caller ticks background work).
    /// Already-queued messages wake immediately
    /// (`MWMO_INPUTAVAILABLE` — no added latency either way), as do
    /// sent/posted messages from any thread (worker completion posts).
    /// A failed wait is a loud `Timeout` plus a stderr line: the loop
    /// retries with a re-derived bound (persistent failure degrades to
    /// polling, never a hang and never a spin).
    pub fn wait_for_input(&self, timeout_ms: Option<u32>) -> WaitOutcome {
        use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0};
        use windows::Win32::System::Threading::{CreateWaitableTimerW, SetWaitableTimer, INFINITE};
        use windows::Win32::UI::WindowsAndMessaging::{
            MsgWaitForMultipleObjectsEx, MWMO_INPUTAVAILABLE, QS_ALLINPUT,
        };
        // The message queue lives on this thread (the runner thread
        // owns the HWND) — waiting anywhere else would wait on the
        // wrong queue, so this takes `&self` by construction, never a
        // handle or an HWND.
        unsafe {
            match timeout_ms {
                None => {
                    let r = MsgWaitForMultipleObjectsEx(
                        None,
                        INFINITE,
                        QS_ALLINPUT,
                        MWMO_INPUTAVAILABLE,
                    );
                    if r == WAIT_FAILED {
                        eprintln!("oppa-shell-win: wait_for_input failed — retrying");
                        WaitOutcome::Timeout
                    } else {
                        // INFINITE never times out: any other outcome
                        // (WAIT_OBJECT_0) is queue input.
                        WaitOutcome::Input
                    }
                }
                Some(ms) => {
                    let timer =
                        match CreateWaitableTimerW(None, false, windows::core::PCWSTR::null()) {
                            Ok(h) => h,
                            Err(e) => {
                                eprintln!("oppa-shell-win: wait timer failed ({e:?}) — retrying");
                                return WaitOutcome::Timeout;
                            }
                        };
                    // Relative due, negative 100ns units.
                    let due: i64 = -(ms as i64) * 10_000;
                    if SetWaitableTimer(timer, &due, 0, None, None, false).is_err() {
                        eprintln!("oppa-shell-win: wait timer arm failed — retrying");
                        let _ = CloseHandle(timer);
                        return WaitOutcome::Timeout;
                    }
                    let handles = [HANDLE(timer.0)];
                    let r = MsgWaitForMultipleObjectsEx(
                        Some(&handles),
                        INFINITE,
                        QS_ALLINPUT,
                        MWMO_INPUTAVAILABLE,
                    );
                    let _ = CloseHandle(timer);
                    if r == WAIT_OBJECT_0 {
                        WaitOutcome::Timeout
                    } else if r == WAIT_FAILED {
                        eprintln!("oppa-shell-win: wait_for_input failed — retrying");
                        WaitOutcome::Timeout
                    } else {
                        // WAIT_OBJECT_0 + 1: the queue, not the timer.
                        WaitOutcome::Input
                    }
                }
            }
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

unsafe extern "system" fn shell_wnd_proc(
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
            if unsafe { super::theme::is_immersive_color_set(lparam) } {
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

pub(crate) fn log_ime(shared: &Rc<RefCell<ShellShared>>, msg: ImeMessage) {
    let mut s = shared.borrow_mut();
    s.seq += 1;
    let seq = s.seq;
    s.ime_log.push(format!("[ime {:04}] {:?}", seq, msg));
    s.queue.push_back(ShellEvent::Ime(msg));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn key_down(vk: u16) -> bool {
    unsafe { GetKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

fn client_x(lparam: LPARAM) -> i32 {
    (lparam.0 as u32 & 0xFFFF) as u16 as i16 as i32
}

fn client_y(lparam: LPARAM) -> i32 {
    ((lparam.0 as u32 >> 16) & 0xFFFF) as u16 as i16 as i32
}

/// WM_CHAR packs the char in wParam; UTF-32 code points arrive directly,
/// UTF-16 surrogates as half a pair (0xD800..=0xDFFF) — dropped here
/// (debug-grade; the pass exercises no astral *typed* input).
fn char_from_wparam(wparam: WPARAM) -> Option<char> {
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
unsafe fn read_composition(hwnd: HWND, flags: u32) -> ImeMessage {
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
fn cmd_of(ev: &ShellEvent, pending_drag: &mut Option<i32>) -> Option<Cmd> {
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
fn kind_of(ev: &ShellEvent) -> EventKind {
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

#[cfg(test)]
mod pointer_tests {
    use super::*;
    use oppa::shell::PlatformShell;

    fn hidden_shell() -> Win32Shell {
        Win32Shell::new(ShellConfig {
            title: "pointer-test".to_string(),
            width: 200,
            height: 150,
            record_messages: false,
            suppress_os_composition_window: true,
            visible: false,
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
}

#[cfg(test)]
mod resize_tests {
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
}
