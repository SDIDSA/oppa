//! Android input classification (M10): MotionEvent / key intake into
//! the shared [`InputEvent`](oppa::InputEvent) pipeline.
//!
//! The shape mirrors `oppa-shell-win`'s `ShellEvent`/`Cmd` split on
//! purpose: the shell classifies OS intake into payload-shaped values,
//! and the framework-owned host router (`ComponentHost::route_input`
//! via `inject_input`) owns all routing. Both shells produce
//! `InputEvent`; neither shell routes. That is the shared contract,
//! and `tests/android_contract.rs` proves it is not forked
//! per-platform (same Toggle, same assertions as M5's `m5_input`).
//!
//! v1 bounds, same as the framework's (no fork):
//!
//! - Every pointer index routes with its own id (G11 — the v1
//!   index-0-only refusal is retired; the framework holds one
//!   capture per id).
//! - Coordinates arrive in dp and scale by `density` (device px =
//!   dp × density) at the classification boundary, so hit-testing
//!   sees the same space as every other shell.
//! - Keys are ambient (decision 96): every keycode classifies to a
//!   `Key` event and the router quietly ignores what nothing serves.
//!   `BACK` maps to `ESCAPE` (dismiss on both platforms —
//!   classification, not new semantics).

use oppa::input::{keys, PointerAction, PointerButton};
use oppa::{InputEvent, KeyState, Modifiers};

/// Android keycodes used by the classifier (`android.view.KeyEvent`).
pub mod keycodes {
    /// System back (mapped to dismiss).
    pub const BACK: u32 = 4;
    /// D-pad up (mapped to Up, round 5.3).
    pub const DPAD_UP: u32 = 19;
    /// D-pad down (mapped to Down, round 5.3).
    pub const DPAD_DOWN: u32 = 20;
    /// D-pad left (mapped to Left, round 5.3).
    pub const DPAD_LEFT: u32 = 21;
    /// D-pad right (mapped to Right, round 5.3).
    pub const DPAD_RIGHT: u32 = 22;
    /// Tab.
    pub const TAB: u32 = 61;
    /// Enter.
    pub const ENTER: u32 = 66;
    /// Backspace (mapped to the framework Backspace — runners
    /// route it to the focused field's session, decision 243).
    pub const DEL: u32 = 67;
    /// Forward delete (mapped to the framework Delete — same
    /// runner routing as Backspace).
    pub const FORWARD_DEL: u32 = 112;
    /// Space.
    pub const SPACE: u32 = 62;
    /// Escape.
    pub const ESCAPE: u32 = 111;
    /// Letter A (decision 442 — Android letters are contiguous
    /// 29–54, mirroring the contiguous framework 0x41–0x5A, so
    /// ctrl-combos route; plain presses commit text through the
    /// decision-460 hardware fallback instead of router silence).
    pub const A: u32 = 29;
    /// Letter Z (range end, see `A`).
    pub const Z: u32 = 54;
    /// Digit 0 (decision 460 — Android digits are contiguous 7–16,
    /// mirroring the framework 0x30–0x39; without this they pass
    /// through raw and collide with control codes — `KEYCODE_1`
    /// (8) reads as framework Backspace and deletes).
    pub const DIGIT_0: u32 = 7;
    /// Digit 9 (range end, see `DIGIT_0`).
    pub const DIGIT_9: u32 = 16;
    /// Text-navigation Home (mapped to the framework Home — runners
    /// route it like the arrows, decision 442). Distinct from
    /// `KEYCODE_HOME` (3, system home — never intercepted).
    pub const MOVE_HOME: u32 = 122;
    /// Text-navigation End (same routing as `MOVE_HOME`).
    pub const MOVE_END: u32 = 123;
}

/// One classified Android intake event. Motion coordinates are dp;
/// `to_input_event` scales them by the shell density.
#[derive(Clone, Debug, PartialEq)]
pub enum AndroidEvent {
    /// `ACTION_DOWN` / `ACTION_POINTER_DOWN` (`pointer_index` is the
    /// MotionEvent pointer index — every index routes, G11).
    MotionDown {
        pointer_index: u32,
        x_dp: f32,
        y_dp: f32,
    },
    /// `ACTION_MOVE`.
    MotionMove {
        pointer_index: u32,
        x_dp: f32,
        y_dp: f32,
    },
    /// `ACTION_UP`.
    MotionUp {
        pointer_index: u32,
        x_dp: f32,
        y_dp: f32,
    },
    /// `ACTION_CANCEL` (gesture preempted — never dispatches).
    MotionCancel,
    /// `ACTION_DOWN` key (keycode from [`keycodes`], modifiers
    /// sampled at event time — decision 442, the desktop
    /// `step_session_nav` matrix needs shift/ctrl on-device).
    KeyDown { keycode: u32, modifiers: Modifiers },
    /// `ACTION_UP` key (modifiers sampled like `KeyDown` — release
    /// is not a second edit, but the router sees the true state).
    KeyUp { keycode: u32, modifiers: Modifiers },
    /// Soft-keyboard text commit (Round 3.1): the JNI text callback's
    /// payload (`InputConnection.commitText`) — full committed
    /// string, caret follows it (replacement selection is applied by
    /// the session insert over the current selection).
    CommitText { text: String },
    /// Soft-keyboard surrounding-text delete (Round 3.1):
    /// `deleteSurroundingText` char counts (many keyboards send this
    /// instead of key events for backspace).
    DeleteSurrounding { before_chars: u32, after_chars: u32 },
    /// Soft-keyboard composition update (decision 438): the
    /// `InputConnection.setComposingText` payload — the live
    /// composing string (caret: end of text, see the runner's
    /// preedit feed). Without this every composing keystroke died in
    /// the proxy's hidden buffer and single-letter taps never
    /// reached the session.
    ComposeText { text: String },
    /// Soft-keyboard finish-composing (decision 461): the
    /// `InputConnection.finishComposingText` signal — no payload.
    /// The runner commits the session's open buffer with its own
    /// text (quiet no-op with no open composition).
    FinishComposing,
    /// Window focus gain/loss.
    FocusChanged(bool),
}

/// Payload-shaped command, drained by the host loop in event order
/// (the `Cmd` half of the shell split — device px, ready to inject).
/// Pointer commands carry the MotionEvent pointer index as the
/// framework pointer id (G11 — one capture per id in the router).
/// Window focus intents are deliberately absent: the shell records
/// `has_focus` itself (IME policy reads it) and focus routing stays
/// click-driven in the framework router — no fabricated targets.
/// Text commands are runner-matched (the loop inserts into the
/// focused session — `to_input_event` refuses them loudly, the
/// desktop `Char` precedent): `Clone` but not `Copy` (commits own
/// their string — callers `.cloned()` batch slices, see the app).
#[derive(Clone, Debug, PartialEq)]
pub enum AndroidCmd {
    PointerDown {
        id: u32,
        x: f32,
        y: f32,
    },
    PointerMove {
        id: u32,
        x: f32,
        y: f32,
    },
    PointerUp {
        id: u32,
        x: f32,
        y: f32,
    },
    PointerCancel,
    Key {
        code: u32,
        pressed: bool,
        modifiers: Modifiers,
    },
    /// Soft-keyboard commit for the focused session (Round 3.1).
    CommitText {
        text: String,
    },
    /// Soft-keyboard surrounding delete for the focused session.
    DeleteSurrounding {
        before_chars: u32,
        after_chars: u32,
    },
    /// Soft-keyboard composition for the focused session (decision
    /// 438) — runner-matched into the preedit feed, never injected.
    ComposeText {
        text: String,
    },
    /// Soft-keyboard finish-composing for the focused session
    /// (decision 461) — runner-matched into a buffer-text commit,
    /// never injected.
    FinishComposing,
}

impl AndroidCmd {
    /// The shared-pipeline event (what the host loop `inject_input`s —
    /// the exact constructors every shell feeds the router).
    pub fn to_input_event(self) -> InputEvent {
        match self {
            AndroidCmd::PointerDown { id, x, y } => InputEvent::Pointer {
                id: Some(id),
                // Round 9.2: touch is always primary (buttons are a
                // mouse taxonomy — touch contacts never classify).
                action: PointerAction::Down {
                    button: PointerButton::Primary,
                },
                x,
                y,
                modifiers: Modifiers::NONE,
            },
            AndroidCmd::PointerMove { id, x, y } => InputEvent::Pointer {
                id: Some(id),
                action: PointerAction::Move,
                x,
                y,
                modifiers: Modifiers::NONE,
            },
            AndroidCmd::PointerUp { id, x, y } => InputEvent::Pointer {
                id: Some(id),
                action: PointerAction::Up {
                    button: PointerButton::Primary,
                },
                x,
                y,
                modifiers: Modifiers::NONE,
            },
            AndroidCmd::PointerCancel => InputEvent::Pointer {
                id: None,
                action: PointerAction::Cancel,
                x: f32::NAN,
                y: f32::NAN,
                modifiers: Modifiers::NONE,
            },
            AndroidCmd::Key {
                code,
                pressed,
                modifiers,
            } => InputEvent::Key {
                code,
                modifiers,
                state: if pressed {
                    KeyState::Pressed
                } else {
                    KeyState::Released
                },
                repeat: false,
            },
            AndroidCmd::CommitText { text } => panic!(
                "AndroidCmd::CommitText({text:?}) reached to_input_event — commits carry no InputEvent mapping by design; \
                 the runner must match CommitText first and insert it via the focused session"
            ),
            AndroidCmd::DeleteSurrounding { .. } => panic!(
                "AndroidCmd::DeleteSurrounding reached to_input_event — surrounding deletes carry no InputEvent mapping by design; \
                 the runner must match DeleteSurrounding first and delete through the focused session"
            ),
            AndroidCmd::ComposeText { .. } => panic!(
                "AndroidCmd::ComposeText reached to_input_event — compositions carry no InputEvent mapping by design; \
                 the runner must match ComposeText first and feed preedit through the focused session"
            ),
            AndroidCmd::FinishComposing => panic!(
                "AndroidCmd::FinishComposing reached to_input_event — finish signals carry no InputEvent mapping by design; \
                 the runner must match FinishComposing first and commit the open buffer through the focused session"
            ),
        }
    }
}

/// Shell-side errors: G11 retired the only variant (`MultiTouch` —
/// every pointer index now routes). Retained as an empty type so
/// `take_errors` keeps its shape; it is never constructed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellError {}

/// Framework keycode for an Android keycode (the classifier half of
/// decision 96 — unknown codes pass through and the router stays
/// quiet; only BACK is remapped, to dismiss; round 5.3 maps the
/// D-pad to the arrow kinds).
pub fn classify_keycode(keycode: u32) -> u32 {
    match keycode {
        keycodes::TAB => keys::TAB,
        keycodes::ENTER => keys::ENTER,
        keycodes::SPACE => keys::SPACE,
        keycodes::BACK | keycodes::ESCAPE => keys::ESCAPE,
        keycodes::DPAD_LEFT => keys::LEFT,
        keycodes::DPAD_UP => keys::UP,
        keycodes::DPAD_RIGHT => keys::RIGHT,
        keycodes::DPAD_DOWN => keys::DOWN,
        keycodes::DEL => keys::BACKSPACE,
        keycodes::FORWARD_DEL => keys::DELETE,
        keycodes::MOVE_HOME => keys::HOME,
        keycodes::MOVE_END => keys::END,
        // Letters map to the contiguous framework VKs (decision
        // 442 — only ctrl-held combinations route as actions; plain
        // presses commit through the decision-460 text fallback).
        code @ keycodes::A..=keycodes::Z => keys::A + (code - keycodes::A),
        // Digits map to the contiguous framework 0x30–0x39
        // (decision 460 — same rule as letters; see the
        // `keycodes::DIGIT_0` note).
        code @ keycodes::DIGIT_0..=keycodes::DIGIT_9 => 0x30 + (code - keycodes::DIGIT_0),
        other => other,
    }
}

impl AndroidEvent {
    /// Classifies intake into a command (`density` scales dp → px).
    /// Returns `Ok(None)` for intake handled shell-side without a
    /// command (window focus — recorded in `has_focus`, never
    /// injected as a fabricated target). The `Err` arm is retained
    /// for shell-side refusals (nothing refuses in G11 — every
    /// pointer routes).
    pub fn classify(self, density: f32) -> Result<Option<AndroidCmd>, ShellError> {
        match self {
            AndroidEvent::MotionDown {
                pointer_index,
                x_dp,
                y_dp,
            } => Ok(Some(AndroidCmd::PointerDown {
                id: pointer_index,
                x: x_dp * density,
                y: y_dp * density,
            })),
            AndroidEvent::MotionMove {
                pointer_index,
                x_dp,
                y_dp,
            } => Ok(Some(AndroidCmd::PointerMove {
                id: pointer_index,
                x: x_dp * density,
                y: y_dp * density,
            })),
            AndroidEvent::MotionUp {
                pointer_index,
                x_dp,
                y_dp,
            } => Ok(Some(AndroidCmd::PointerUp {
                id: pointer_index,
                x: x_dp * density,
                y: y_dp * density,
            })),
            AndroidEvent::MotionCancel => Ok(Some(AndroidCmd::PointerCancel)),
            AndroidEvent::KeyDown { keycode, modifiers } => Ok(Some(AndroidCmd::Key {
                code: classify_keycode(keycode),
                pressed: true,
                modifiers,
            })),
            AndroidEvent::KeyUp { keycode, modifiers } => Ok(Some(AndroidCmd::Key {
                code: classify_keycode(keycode),
                pressed: false,
                modifiers,
            })),
            // Soft-keyboard text (Round 3.1): density-independent
            // (no coordinates) — straight through to the
            // runner-matched commands.
            AndroidEvent::CommitText { text } => Ok(Some(AndroidCmd::CommitText { text })),
            AndroidEvent::DeleteSurrounding {
                before_chars,
                after_chars,
            } => Ok(Some(AndroidCmd::DeleteSurrounding {
                before_chars,
                after_chars,
            })),
            // Soft-keyboard composition (decision 438): same
            // density-independent straight-through as commits.
            AndroidEvent::ComposeText { text } => Ok(Some(AndroidCmd::ComposeText { text })),
            // Soft-keyboard finish-composing (decision 461): same
            // straight-through (payload-free).
            AndroidEvent::FinishComposing => Ok(Some(AndroidCmd::FinishComposing)),
            // Focus intents stay shell-side (the shell cannot name a
            // retained node): the shell records `has_focus`, the loop
            // reads it — no command is produced.
            AndroidEvent::FocusChanged(_) => Ok(None),
        }
    }
}
