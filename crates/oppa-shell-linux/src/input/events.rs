//! Intake vocabulary: config, winit-shaped events, runner commands, stats.

use oppa::{InputEvent, KeyState, Modifiers};

/// Shell geometry (mirrors `AndroidShell`'s config).
pub struct ShellConfig {
    pub title: String,
    pub density: f32,
    pub width_dp: f32,
    pub height_dp: f32,
}

/// Shell intake: platform-shaped facts, density-independent.
/// Coordinates are dp (the shell classifies dp→px on output —
/// the same direction as `AndroidEvent`).
///
/// `ImePreedit`/`ImeCommit` carry owned text, so this enum (and
/// [`LinuxCmd`]) is `Clone` but not `Copy` — strings never copy by
/// words (Round 2.1; matches by value still move, matches the
/// runner takes by reference — see `linux.rs`).
#[derive(Clone, PartialEq, Debug)]
pub enum LinuxEvent {
    MouseDown {
        button: oppa::PointerButton,
        x_dp: f32,
        y_dp: f32,
    },
    MouseUp {
        button: oppa::PointerButton,
        x_dp: f32,
        y_dp: f32,
    },
    MouseMove {
        x_dp: f32,
        y_dp: f32,
    },
    /// Wheel tick at the last-known cursor dp (decision 250 —
    /// `MouseWheel` carries no position; `translate` attaches
    /// `cursor_dp`, same as button events). Deltas in dp (`LineDelta`
    /// already scaled ×30, `PixelDelta` density-normalized).
    Wheel {
        x_dp: f32,
        y_dp: f32,
        dx: f32,
        dy: f32,
    },
    KeyDown {
        code: winit::keyboard::KeyCode,
    },
    KeyUp {
        code: winit::keyboard::KeyCode,
    },
    ModifiersChanged {
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    },
    TouchDown {
        index: u32,
        x_dp: f32,
        y_dp: f32,
    },
    TouchUp {
        index: u32,
        x_dp: f32,
        y_dp: f32,
    },
    TouchMove {
        index: u32,
        x_dp: f32,
        y_dp: f32,
    },
    /// Committed text char (decision 243): from `KeyboardInput.text`
    /// on press (control chars filtered at translate). Routes through
    /// the runner's focused session — never a pipeline `InputEvent`.
    Char {
        ch: char,
    },
    /// IME preedit update (Round 2.1): non-empty preedit text with
    /// the byte-wise cursor (`None` hides the cursor — the runner
    /// defaults it to the text end). Empty preedits never become
    /// events (winit sends one before every commit).
    ImePreedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    /// IME commit (Round 2.1): final text for the focused session.
    ImeCommit {
        text: String,
    },
    /// IME disabled (Round 2.1): clear pending preedit (the runner
    /// cancels an open composition; `Enabled` maps to nothing).
    ImeDisabled,
    /// Monitor scale change (Round 2.4, OQ-G10-2): the winit
    /// `ScaleFactorChanged` factor as f32 (realistic scales are
    /// exactly representable; the loop validates loudly anyway).
    /// Routes through the runner (density + DPR + surface), never
    /// through `to_input_event`.
    Density {
        scale: f32,
    },
    OtherButton,
    Cancel,
}

/// Classified commands (mirror `AndroidCmd`). Pointer commands carry
/// the shell pointer id (mouse = 0, touch = touch index — G11,
/// decision 229); cancel is the global tripwire (no id). `Clone`
/// without `Copy` (see [`LinuxEvent`]).
#[derive(Clone, PartialEq, Debug)]
pub enum LinuxCmd {
    PointerDown {
        id: u32,
        button: oppa::PointerButton,
        x: f32,
        y: f32,
    },
    PointerUp {
        id: u32,
        button: oppa::PointerButton,
        x: f32,
        y: f32,
    },
    PointerMove {
        id: u32,
        x: f32,
        y: f32,
    },
    PointerCancel,
    Key {
        code: u32,
        pressed: bool,
    },
    /// Wheel scroll at px (decision 250): the runner hit-tests the
    /// target and dispatches through `DesktopLoop::scroll_at` — never
    /// through `to_input_event` (a scroll with no runner match is a
    /// wiring bug and panics loudly there, the `Char` precedent).
    /// `dy` in device px per tick-equivalent; `dx` rides along
    /// (v1 is vertical-only — the router ignores it, stated).
    Scroll {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
    /// Committed text char (decision 243): carries no `InputEvent`
    /// mapping by design — the runner matches it first and types it
    /// into the focused session (`DesktopLoop::type_text`). Reaching
    /// `to_input_event` is a wiring bug and panics loudly there.
    Char {
        ch: char,
    },
    /// IME preedit update (Round 2.1): the runner matches it first
    /// and feeds it via `DesktopLoop::feed_ime_preedit` (byte-wise
    /// cursor, end default). Reaching `to_input_event` panics like
    /// `Char` above.
    ImePreedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    /// IME commit (Round 2.1): the runner feeds it via
    /// `DesktopLoop::feed_ime_commit`. Same loud refusal below.
    ImeCommit {
        text: String,
    },
    /// IME cancel (Round 2.1): the runner feeds it via
    /// `DesktopLoop::feed_ime_cancel`. Same loud refusal below.
    ImeCancel,
    /// Monitor scale change (Round 2.4): the runner matches it
    /// first and re-bases density + DPR + surface (`DesktopLoop::
    /// set_device_pixel_ratio`). Same loud refusal below.
    Density {
        scale: f32,
    },
}

impl LinuxCmd {
    pub fn to_input_event(self, modifiers: Modifiers) -> InputEvent {
        match self {
            LinuxCmd::PointerDown { id, button, x, y } => InputEvent::Pointer {
                id: Some(id),
                action: oppa::PointerAction::Down { button },
                x,
                y,
                modifiers,
            },
            LinuxCmd::PointerUp { id, button, x, y } => InputEvent::Pointer {
                id: Some(id),
                action: oppa::PointerAction::Up { button },
                x,
                y,
                modifiers,
            },
            LinuxCmd::PointerMove { id, x, y } => InputEvent::Pointer {
                id: Some(id),
                action: oppa::PointerAction::Move,
                x,
                y,
                modifiers,
            },
            LinuxCmd::PointerCancel => InputEvent::Pointer {
                id: None,
                action: oppa::PointerAction::Cancel,
                x: f32::NAN,
                y: f32::NAN,
                modifiers,
            },
            LinuxCmd::Key { code, pressed } => InputEvent::Key {
                code,
                modifiers,
                state: if pressed {
                    KeyState::Pressed
                } else {
                    KeyState::Released
                },
                repeat: false,
            },
            LinuxCmd::Char { ch } => panic!(
                "LinuxCmd::Char({ch:?}) reached to_input_event — chars carry no InputEvent mapping by design; \
                 the runner must match Char first and type it via DesktopLoop::type_text"
            ),
            LinuxCmd::ImePreedit { .. } => panic!(
                "LinuxCmd::ImePreedit reached to_input_event — preedit carries no InputEvent mapping by design; \
                 the runner must match ImePreedit first and feed it via DesktopLoop::feed_ime_preedit"
            ),
            LinuxCmd::ImeCommit { .. } => panic!(
                "LinuxCmd::ImeCommit reached to_input_event — commits carry no InputEvent mapping by design; \
                 the runner must match ImeCommit first and feed it via DesktopLoop::feed_ime_commit"
            ),
            LinuxCmd::ImeCancel => panic!(
                "LinuxCmd::ImeCancel reached to_input_event — cancels carry no InputEvent mapping by design; \
                 the runner must match ImeCancel first and feed it via DesktopLoop::feed_ime_cancel"
            ),
            LinuxCmd::Density { .. } => panic!(
                "LinuxCmd::Density reached to_input_event — scale changes carry no InputEvent mapping by design; \
                 the runner must match Density first and re-base via DesktopLoop::set_device_pixel_ratio"
            ),
            LinuxCmd::Scroll { .. } => panic!(
                "LinuxCmd::Scroll reached to_input_event — scroll carries a runner-resolved target, not a pipeline event; \
                 the runner must match Scroll first and dispatch via DesktopLoop::scroll_at"
            ),
        }
    }
}

/// Shell errors: G11 retired the only variant (`MultiTouch` — every
/// pointer id now routes instead of refusing). Retained as an empty
/// type so `take_errors` keeps its shape; it is never constructed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LinuxShellError {}

/// Intake statistics for the run record (every ignored class is
/// counted — nothing vanishes silently).
#[derive(Default, Debug)]
pub struct LinuxIntakeStats {
    pub mouse: usize,
    pub keys: usize,
    pub touches: usize,
    pub chars: usize,
    pub ime: usize,
    pub density: usize,
    pub ignored_buttons: usize,
    pub ignored_keys: usize,
}
