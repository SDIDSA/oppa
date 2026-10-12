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
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetSysColorBrush, InvalidateRect, UpdateWindow, COLOR_WINDOW, PAINTSTRUCT,
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
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCHITTEST, WM_PAINT, WM_QUIT,
    WM_RBUTTONDBLCLK, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS, WM_SETTINGCHANGE,
    WM_SIZE, WM_SYSKEYDOWN, WNDCLASSEXW, WNDCLASS_STYLES, WS_OVERLAPPEDWINDOW,
};

mod events;
mod shell_platform;

pub(crate) use events::{
    char_from_wparam, client_x, client_y, cmd_of, key_down, kind_of, read_composition, wheel_event,
};
pub use events::{Cmd, ImeMessage, MessageRecord, ShellEvent, HWHEEL_LINE_PX, WHEEL_LINE_PX};
pub(crate) use shell_platform::shell_wnd_proc;

/// Frameless hit-test answers (decision 482 — the `WM_NCHITTEST`
/// vocabulary for popup-chrome windows; plain `isize` so the pure
/// classifier below stays headless-testable, the proc casts).
pub(crate) const HTCLIENT_PX: isize = 1;
pub(crate) const HTCAPTION_PX: isize = 2;
pub(crate) const HTLEFT_PX: isize = 10;
pub(crate) const HTRIGHT_PX: isize = 11;
pub(crate) const HTTOP_PX: isize = 12;
pub(crate) const HTTOPLEFT_PX: isize = 13;
pub(crate) const HTTOPRIGHT_PX: isize = 14;
pub(crate) const HTBOTTOM_PX: isize = 15;
pub(crate) const HTBOTTOMLEFT_PX: isize = 16;
pub(crate) const HTBOTTOMRIGHT_PX: isize = 17;
/// Native caption-button answers (decision 485 — real `HT*BUTTON`
/// values so `DefWindowProcW` owns hover glow, press, and Win11 snap
/// layouts over the zones; plain `isize` like the frameless codes
/// above).
pub(crate) const HTMINBUTTON_PX: isize = 8;
pub(crate) const HTMAXBUTTON_PX: isize = 9;
pub(crate) const HTCLOSE_PX: isize = 20;

/// Pure native-caption matcher (decision 485): client point against
/// the published `(button, rect)` zones, in order — first hit wins
/// (zones never overlap by construction: they are disjoint button
/// boxes). `None` falls through to the frameless classifier below.
/// Pure (headless-testable — the proc supplies the live point +
/// the runner-published zones; see `caption_button_at_*`).
pub(crate) fn caption_button_at(
    x: f32,
    y: f32,
    zones: &[(oppa::CaptionButton, [f32; 4])],
) -> Option<isize> {
    caption_zone_at(x, y, zones).map(|button| match button {
        oppa::CaptionButton::Minimize => HTMINBUTTON_PX,
        oppa::CaptionButton::Maximize => HTMAXBUTTON_PX,
        oppa::CaptionButton::Close => HTCLOSE_PX,
    })
}

/// Pure native-caption zone lookup (decision 488): the slot whose
/// box holds the client point, in zone order. The click arms match
/// on this (self-performed actions); the hit-test arm maps it
/// through [`caption_button_at`].
pub(crate) fn caption_zone_at(
    x: f32,
    y: f32,
    zones: &[(oppa::CaptionButton, [f32; 4])],
) -> Option<oppa::CaptionButton> {
    zones.iter().find_map(|(button, r)| {
        (x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]).then_some(*button)
    })
}

/// Frameless resize-border thickness in client px (decision 482):
/// an 8px band on every edge answers the sizing codes so border
/// drags, corner drags, and snap-by-drag keep working without a
/// title bar. Fixed (never DPI-scaled — the band is an affordance
/// floor, not a metric; the OS scales the window, not the band).
pub(crate) const FRAMELESS_BORDER_PX: f32 = 8.0;

/// Pure frameless `WM_NCHITTEST` classifier (decision 482): client
/// point `(x, y)` in a `w`×`h` client area maps to a hit-test code.
/// Maximized windows never resize (caption-or-client only — a
/// maximized sizing answer would let the OS un-maximize into a
/// half-applied rect). Borders win over drag regions (a full-bleed
/// header keeps its top 8px resizable — edge resizing stays
/// functional under `Extended`), drag regions win over client.
/// Pure (headless-testable — the proc supplies the live rect + the
/// published regions; see `frameless_hit_test_*`).
pub(crate) fn frameless_hit_test(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    maximized: bool,
    drag_rects: &[[f32; 4]],
) -> isize {
    if maximized {
        return if in_rects(x, y, drag_rects) {
            HTCAPTION_PX
        } else {
            HTCLIENT_PX
        };
    }
    let b = FRAMELESS_BORDER_PX;
    let left = x < b;
    let right = x >= w - b;
    let top = y < b;
    let bottom = y >= h - b;
    if top && left {
        return HTTOPLEFT_PX;
    }
    if top && right {
        return HTTOPRIGHT_PX;
    }
    if bottom && left {
        return HTBOTTOMLEFT_PX;
    }
    if bottom && right {
        return HTBOTTOMRIGHT_PX;
    }
    if left {
        return HTLEFT_PX;
    }
    if right {
        return HTRIGHT_PX;
    }
    if top {
        return HTTOP_PX;
    }
    if bottom {
        return HTBOTTOM_PX;
    }
    if in_rects(x, y, drag_rects) {
        HTCAPTION_PX
    } else {
        HTCLIENT_PX
    }
}

fn in_rects(x: f32, y: f32, rects: &[[f32; 4]]) -> bool {
    rects
        .iter()
        .any(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3])
}

/// Enables DWM per-pixel transparency (decision 484 — winit's exact
/// recipe, see `platform_impl/windows/window.rs::on_create`): blur
/// behind with an EMPTY region (`CreateRectRgn(0, 0, -1, -1)`) makes
/// the redirection surface fully transparent, so the runner's
/// premultiplied-alpha swapchain composites per-pixel alpha instead
/// of an opaque sheet. Pure OS call (headless-testable against a
/// hidden window — needs DWM running, the same environment bound as
/// window creation itself). A refusal is a loud `Err` (the shell
/// logs it and keeps an opaque window — composition is a
/// presentational backstop, never a creation failure).
pub(crate) fn enable_transparent_blur(hwnd: HWND) -> Result<(), String> {
    use windows::core::BOOL;
    use windows::Win32::Graphics::Dwm::{
        DwmEnableBlurBehindWindow, DWM_BB_BLURREGION, DWM_BB_ENABLE, DWM_BLURBEHIND,
    };
    use windows::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject};
    unsafe {
        let region = CreateRectRgn(0, 0, -1, -1);
        if region.is_invalid() {
            return Err("CreateRectRgn refused the empty blur region".to_string());
        }
        let bb = DWM_BLURBEHIND {
            dwFlags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
            fEnable: BOOL::from(true),
            hRgnBlur: region,
            fTransitionOnMaximized: BOOL::from(false),
        };
        let hr = DwmEnableBlurBehindWindow(hwnd, &bb);
        let _ = DeleteObject(region.into());
        if hr.is_err() {
            return Err(format!("DwmEnableBlurBehindWindow refused: {hr:?}"));
        }
        Ok(())
    }
}

/// Extends the DWM glass frame 1px into the client area (decision
/// 489 — the standard borderless recipe for native borders and drop
/// shadows: stripping `WM_NCCALCSIZE` takes the frame visuals with
/// it, so without this a frameless window is a flat edgeless rect).
/// The 1px glass edge hides under opaque scene pixels (only the
/// border line + outside shadow show); on `Transparent` it reads as
/// a faint edge defining the floating shape. Corners stay
/// OS-rounded, colors OS-owned (the gist's no-control rule). Pure OS
/// call (same headless test bound as the blur helper). A refusal is
/// a loud `Err` (the shell logs it and keeps the shadowless window —
/// presentational backstop, never a creation failure).
pub(crate) fn extend_glass_frame(hwnd: HWND) -> Result<(), String> {
    use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
    use windows::Win32::UI::Controls::MARGINS;
    let margins = MARGINS {
        cxLeftWidth: 1,
        cxRightWidth: 1,
        cyTopHeight: 1,
        cyBottomHeight: 1,
    };
    unsafe {
        let hr = DwmExtendFrameIntoClientArea(hwnd, &margins);
        if hr.is_err() {
            return Err(format!("DwmExtendFrameIntoClientArea refused: {hr:?}"));
        }
        Ok(())
    }
}

/// Clamps a maximized borderless rect into the monitor work area
/// (decision 484 — the winit `WM_NCCALCSIZE` mirror: without
/// decorations to offset it, a maximized frameless window would
/// span the full monitor including the taskbar). Pure
/// (headless-testable — the proc supplies the live work rect).
pub(crate) fn clamp_rect_to_work(
    mut rect: windows::Win32::Foundation::RECT,
    work: windows::Win32::Foundation::RECT,
) -> windows::Win32::Foundation::RECT {
    rect.left = work.left;
    rect.top = work.top;
    rect.right = work.left + (work.right - work.left);
    rect.bottom = work.top + (work.bottom - work.top);
    rect
}

/// Test-only modifier pin (decision 417): when set, [`key_down`]
/// answers from this table instead of the live `GetKeyState`, so
/// hidden-window pointer tests are deterministic no matter what the
/// machine's keyboard is doing (the documented live-Shift flake class:
/// a Shift held anywhere on the box flips `shift: false` assertions
/// and reroutes wheel ticks horizontally mid-suite). Production never
/// writes here — the table starts empty (everything live).
static KEY_OVERRIDE: std::sync::Mutex<[Option<bool>; 256]> = std::sync::Mutex::new([None; 256]);

/// Pin one virtual key for tests (`None` releases back to live).
/// Test-only by convention: production code must never call this.
pub fn set_key_override(vk: u16, down: Option<bool>) {
    if let Some(slot) = KEY_OVERRIDE.lock().unwrap().get_mut(vk as usize) {
        *slot = down;
    }
}

pub(crate) fn key_override(vk: u16) -> Option<bool> {
    KEY_OVERRIDE
        .lock()
        .unwrap()
        .get(vk as usize)
        .copied()
        .flatten()
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
    /// OS window chrome (decisions 482–484): every style creates a
    /// standard `WS_OVERLAPPEDWINDOW` (caption + thick frame + system
    /// menu stay set so the OS keeps owning snap, tiling, shadows,
    /// and animations); frameless styles strip only the non-client
    /// *visuals* in the wndproc (`WM_NCCALCSIZE`, maximized work-area
    /// clamp) and answer border/caption hit-tests (`WM_NCHITTEST`,
    /// published drag regions) — see the creation comment.
    pub style: oppa::WindowStyle,
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
            style: oppa::WindowStyle::Decorated,
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
    /// OS window chrome (decisions 482–484): copied from
    /// [`ShellConfig::style`] at creation. Frameless windows keep
    /// the overlapped frame styles (snap/tiling/shadows survive)
    /// while the wndproc strips the non-client visuals
    /// (`WM_NCCALCSIZE` + maximized work-area clamp) and answers
    /// border + caption hit-tests (`WM_NCHITTEST`).
    pub style: oppa::WindowStyle,
    /// `Extended`-mode drag regions in client px (`[x, y, w, h]`,
    /// decision 482): published by the runner from
    /// `oppa::window_drag_rects` after every repaint (the bar's
    /// committed boxes minus press-owning children). The
    /// `WM_NCHITTEST` arm answers `HTCAPTION` inside them so the OS
    /// drags the window; nested controls keep their own hit-tests
    /// because their boxes were subtracted before publish. Empty =
    /// no caption region (undecorated windows stay fully client).
    pub drag_rects: Vec<[f32; 4]>,
    /// Native caption zones (decision 485): the `HeaderBar` caption
    /// button boxes published by the runner (Windows `Extended` +
    /// `native_caption_buttons` only) so the `WM_NCHITTEST` arm
    /// answers the `HT*BUTTON` codes ahead of drag regions and
    /// borders — the OS owns hover glow, press, and snap layouts
    /// over them. Empty = custom/off (the app's bridged buttons
    /// keep their presses).
    pub caption_buttons: Vec<(oppa::CaptionButton, [f32; 4])>,
    /// Hovered native caption zone (decision 486): refreshed on
    /// every `WM_NCHITTEST` (zone hit or miss) and cleared on
    /// `WM_NCMOUSELEAVE`, so the runner can paint the in-scene hover
    /// glow DWM never draws on a stripped frame. `None` means no
    /// hover (or no zones — custom/off never hovers).
    pub hover_caption: Option<oppa::CaptionButton>,
    /// Title-bar box in client px (decision 491): published by the
    /// runner from `oppa::title_bar_rect` so the
    /// `WM_GETTITLEBARINFOEX` arm reports real custom-frame geometry
    /// (screen-converted at answer time) instead of phantom standard
    /// metrics. `None` means no bar (default handling keeps the OS
    /// metrics, byte-identical).
    pub title_bar_rect: Option<[f32; 4]>,
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
            style: cfg.style,
            drag_rects: Vec::new(),
            caption_buttons: Vec::new(),
            hover_caption: None,
            title_bar_rect: None,
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
            // Decisions 482–484 (the winit borderless architecture):
            // EVERY style creates the standard overlapped window
            // (`WS_OVERLAPPEDWINDOW`: caption + thick frame + system
            // menu + min/max boxes stay set — the OS keeps owning
            // Aero snap, tiling, Win+arrow keys, the taskbar system
            // menu, DWM shadows, and minimize/maximize animations).
            // Framelessness lives in the message handling, not the
            // creation flags: `WM_NCCALCSIZE` strips the non-client
            // visuals and `WM_NCHITTEST` answers border/caption hits,
            // so frameless never means unmanaged (a bare `WS_POPUP`
            // was tried first — it silently dropped snap/tiling with
            // the thick frame, found live in Stylebook Extended).
            // Transparent additionally needs DWM per-pixel composition
            // (`enable_transparent_blur` below) plus the runner-owned
            // premultiplied-alpha swapchain + alpha-0 clear.
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

            // Decision 484 (winit's exact recipe): transparent
            // windows enable DWM per-pixel composition with an EMPTY
            // blur region — the redirection surface goes fully
            // transparent so the premultiplied-alpha swapchain shows
            // per-pixel alpha. Loud on refusal but never fatal (the
            // window stays fully usable, falling back to opaque).
            if cfg.style.is_transparent() {
                if let Err(e) = enable_transparent_blur(hwnd) {
                    eprintln!(
                        "oppa-shell-win: transparent composition unavailable: {e} (opaque window)"
                    );
                }
            }

            // Decision 489: every frameless window extends the 1px
            // glass frame (native border line + drop shadow — the
            // `WM_NCCALCSIZE` strip takes them with the visuals).
            // Loud on refusal but never fatal (a shadowless window
            // stays fully usable).
            if cfg.style.is_frameless() {
                if let Err(e) = extend_glass_frame(hwnd) {
                    eprintln!("oppa-shell-win: glass frame unavailable: {e} (no border/shadow)");
                }
            }

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

    /// OS window chrome selected at creation (decision 482).
    pub fn window_style(&self) -> oppa::WindowStyle {
        self.shared.borrow().style
    }

    /// Publishes the `Extended`-mode drag regions in client px
    /// (`[x, y, w, h]`, decision 482 — the runner recomputes these
    /// from `oppa::window_drag_rects` after every repaint and calls
    /// here; the `WM_NCHITTEST` arm answers `HTCAPTION` inside them).
    /// Empty clears the caption region (undecorated windows stay
    /// fully client).
    pub fn set_drag_regions(&self, rects: Vec<[f32; 4]>) {
        self.shared.borrow_mut().drag_rects = rects;
    }

    /// Published drag regions, in order (test observability — the
    /// OS application of them needs a live `WM_NCHITTEST`, reviewed,
    /// never faked; the pure classifier below proves the mapping).
    #[cfg(test)]
    pub(crate) fn drag_regions(&self) -> Vec<[f32; 4]> {
        self.shared.borrow().drag_rects.clone()
    }

    /// Publishes the native caption zones (decision 485 — the runner
    /// recomputes these from `oppa::native_caption_zones` alongside
    /// the drag regions; the `WM_NCHITTEST` arm answers the
    /// `HT*BUTTON` codes over them ahead of everything else).
    /// Empty clears the zones (custom/off — bridged buttons keep
    /// their presses).
    pub fn set_caption_buttons(&self, zones: Vec<(oppa::CaptionButton, [f32; 4])>) {
        self.shared.borrow_mut().caption_buttons = zones;
    }

    /// Published caption zones, in order (test observability — same
    /// live-proc bound as [`Win32Shell::drag_regions`]).
    #[cfg(test)]
    pub(crate) fn caption_zones(&self) -> Vec<(oppa::CaptionButton, [f32; 4])> {
        self.shared.borrow().caption_buttons.clone()
    }

    /// Hovered native caption zone, if any (decision 486 — drained
    /// by the Windows runner per pump iteration into the host hover
    /// signal the `HeaderBar` paints its glow from).
    pub fn hover_caption(&self) -> Option<oppa::CaptionButton> {
        self.shared.borrow().hover_caption
    }

    /// Publishes the title-bar box in client px (decision 491 — the
    /// runner republishes from `oppa::title_bar_rect` alongside the
    /// drag regions; the `WM_GETTITLEBARINFOEX` arm screen-converts
    /// it at answer time). `None` clears back to default handling.
    pub fn set_title_bar_rect(&self, rect: Option<[f32; 4]>) {
        self.shared.borrow_mut().title_bar_rect = rect;
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

pub(crate) fn log_ime(shared: &Rc<RefCell<ShellShared>>, msg: ImeMessage) {
    let mut s = shared.borrow_mut();
    s.seq += 1;
    let seq = s.seq;
    s.ime_log.push(format!("[ime {:04}] {:?}", seq, msg));
    s.queue.push_back(ShellEvent::Ime(msg));
}

#[cfg(test)]
mod pointer_tests;
#[cfg(test)]
mod resize_tests;
