//! Live intake (v1 close-out): winit events into the shared
//! `InputEvent` pipeline — the Linux counterpart of
//! `oppa-shell-android`'s classification, feeding the same router
//! Win32 feeds.
//!
//! Mapping rules (stated; the shell owns classification):
//!
//! - Button Down/Up classify by OS button (Round 9.2, decision
//!   301): left = primary, right = secondary, middle = auxiliary;
//!   further buttons stay counted and ignored
//!   (`LinuxEvent::OtherButton`). `MouseInput` carries no position,
//!   so the shell tracks the cursor from `CursorMoved` (clicks
//!   before any move read (0,0) — documented).
//! - TouchDown/Up/Move map by index to pointer ids (G11 — every
//!   finger routes; the v1 index-0-only refusal is retired, same as
//!   Android).
//! - Keys map by physical code through `keycode_to_framework`
//!   (Escape/Enter/Space/Tab/Backspace/Delete — the framework-routed
//!   set); anything else is counted and ignored (same as Android's
//!   repeat rule).
//! - Committed text arrives as `Char` events: `KeyboardInput.text`
//!   per pressed key (control chars incl. DEL never become events —
//!   they ride `Key`); winit `Ime` composition arrives as `ImePreedit`
//!   (non-empty preedit text + byte-wise cursor) / `ImeCommit` /
//!   `ImeCancel` commands (Round 2.1, decision 256 — the window
//!   enables IME, so commits arrive; empty preedits stay unmapped
//!   because winit sends one right before every `Commit`, and
//!   `Enabled` carries nothing to route). Char commands route
//!   through the runner's focused session, IME commands through its
//!   `feed_ime_*` halves — never through `to_input_event`.
//! - Modifiers are sampled from `ModifiersChanged` into the
//!   commands (shells sample at event time, per contract).
//! - Wheel carries the last-known cursor dp (decision 250 —
//!   `MouseWheel` itself carries none, like button events) and
//!   classifies to a positioned `Scroll` command; the runner
//!   hit-tests the target (decision 100 lives there now — the
//!   shell never fabricates a `NodeId`). `LineDelta` arrives in
//!   lines (×30 dp per the desktop wheel contract);
//!   `PixelDelta` arrives in physical px (density-normalized to dp
//!   like every pointer coordinate).
//! - `translate` (winit types in, `LinuxEvent` out) is compiled
//!   everywhere but mechanically untested, except the `Ime` arms:
//!   `WindowEvent::Ime` needs no live `DeviceId`, so those are
//!   covered like everything below. Other `WindowEvent` values need
//!   a live `DeviceId`, so no harness can construct them;
//!   `translate` only repackages fields (reviewed). Everything below
//!   it — the event table and the keycode table — is unit-tested
//!   like the Android contract.

use oppa::ime::ImeOps;
use oppa::shell::PlatformShell;
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

/// The Linux shell: intake queue + classification + IME log +
/// file dialog (Round 2.3 — owned here for the
/// [`PlatformShell::file_dialog`](oppa::shell::PlatformShell::file_dialog)
/// seam, mirroring `Win32Shell`'s ownership).
pub struct LinuxShell {
    density: f32,
    width_dp: f32,
    height_dp: f32,
    modifiers: Modifiers,
    queue: Vec<LinuxEvent>,
    cmds: Vec<LinuxCmd>,
    errors: Vec<LinuxShellError>,
    ime_log: Vec<ImeOps>,
    pub stats: LinuxIntakeStats,
    dialog: super::file_dialog::LinuxFileDialog<super::file_dialog::StdRunner>,
}

impl LinuxShell {
    pub fn new(config: ShellConfig) -> Self {
        Self {
            density: config.density,
            width_dp: config.width_dp,
            height_dp: config.height_dp,
            modifiers: Modifiers::NONE,
            queue: Vec::new(),
            cmds: Vec::new(),
            errors: Vec::new(),
            ime_log: Vec::new(),
            stats: LinuxIntakeStats::default(),
            dialog: super::file_dialog::LinuxFileDialog::new(super::file_dialog::StdRunner),
        }
    }

    pub fn density(&self) -> f32 {
        self.density
    }

    /// Re-bases intake classification on a new monitor scale (Round
    /// 2.4, OQ-G10-2 — the runner calls this on `ScaleFactorChanged`
    /// before any same-batch positions classify). Non-finite or
    /// non-positive scales panic loudly (the shell divides by this
    /// on every pointer event — a zero/NaN density would silently
    /// mis-scale the world).
    pub fn set_density(&mut self, density: f32) {
        if !density.is_finite() || density <= 0.0 {
            panic!("linux shell: density {density} refused — scales are finite and positive");
        }
        self.density = density;
    }

    /// Live device pixel ratio (G10, decision 226): the winit
    /// `scale_factor()` the demo installs as `density` (dp-to-px is
    /// already density-driven everywhere in this shell — surface size,
    /// intake classification). One name for the framework DPR across
    /// shells; the value is identical to `density()` by construction.
    pub fn device_pixel_ratio(&self) -> f32 {
        self.density
    }

    pub fn push_event(&mut self, ev: LinuxEvent) {
        self.queue.push(ev);
    }

    /// Surface tracking (the demo calls this on resize).
    pub fn note_surface_changed(&mut self, width_dp: f32, height_dp: f32) {
        self.width_dp = width_dp;
        self.height_dp = height_dp;
    }

    pub fn surface_size_px(&self) -> (u32, u32) {
        (
            (self.width_dp * self.density).round() as u32,
            (self.height_dp * self.density).round() as u32,
        )
    }

    fn classify(&mut self, ev: LinuxEvent) {
        let px = |v: f32| v * self.density;
        match ev {
            LinuxEvent::MouseDown { button, x_dp, y_dp } => {
                self.stats.mouse += 1;
                self.cmds.push(LinuxCmd::PointerDown {
                    id: 0,
                    button,
                    x: px(x_dp),
                    y: px(y_dp),
                });
            }
            LinuxEvent::MouseUp { button, x_dp, y_dp } => {
                self.stats.mouse += 1;
                self.cmds.push(LinuxCmd::PointerUp {
                    id: 0,
                    button,
                    x: px(x_dp),
                    y: px(y_dp),
                });
            }
            LinuxEvent::MouseMove { x_dp, y_dp } => {
                self.stats.mouse += 1;
                self.cmds.push(LinuxCmd::PointerMove {
                    id: 0,
                    x: px(x_dp),
                    y: px(y_dp),
                });
            }
            LinuxEvent::Wheel { x_dp, y_dp, dx, dy } => {
                self.stats.mouse += 1;
                self.cmds.push(LinuxCmd::Scroll {
                    x: px(x_dp),
                    y: px(y_dp),
                    dx: px(dx),
                    dy: px(dy),
                });
            }
            LinuxEvent::KeyDown { code } => {
                self.stats.keys += 1;
                match keycode_to_framework(code) {
                    Some(mapped) => self.cmds.push(LinuxCmd::Key {
                        code: mapped,
                        pressed: true,
                    }),
                    None => self.stats.ignored_keys += 1,
                }
            }
            LinuxEvent::KeyUp { code } => {
                self.stats.keys += 1;
                match keycode_to_framework(code) {
                    Some(mapped) => self.cmds.push(LinuxCmd::Key {
                        code: mapped,
                        pressed: false,
                    }),
                    None => self.stats.ignored_keys += 1,
                }
            }
            LinuxEvent::ModifiersChanged {
                shift,
                ctrl,
                alt,
                meta,
            } => {
                self.modifiers = Modifiers {
                    shift,
                    ctrl,
                    alt,
                    meta,
                };
            }
            LinuxEvent::TouchDown { index, x_dp, y_dp } => {
                // G11: every touch index routes with its own pointer id
                // (the v1 index-0-only refusal is retired — the router
                // holds one capture per id, decision 227). Round 9.2:
                // touch is always primary (buttons are a mouse
                // taxonomy — contacts never classify).
                self.stats.touches += 1;
                self.cmds.push(LinuxCmd::PointerDown {
                    id: index,
                    button: oppa::PointerButton::Primary,
                    x: px(x_dp),
                    y: px(y_dp),
                });
            }
            LinuxEvent::TouchUp { index, x_dp, y_dp } => {
                self.stats.touches += 1;
                self.cmds.push(LinuxCmd::PointerUp {
                    id: index,
                    button: oppa::PointerButton::Primary,
                    x: px(x_dp),
                    y: px(y_dp),
                });
            }
            LinuxEvent::TouchMove { index, x_dp, y_dp } => {
                self.stats.touches += 1;
                self.cmds.push(LinuxCmd::PointerMove {
                    id: index,
                    x: px(x_dp),
                    y: px(y_dp),
                });
            }
            LinuxEvent::Char { ch } => {
                self.stats.chars += 1;
                self.cmds.push(LinuxCmd::Char { ch });
            }
            LinuxEvent::ImePreedit { text, cursor } => {
                self.stats.ime += 1;
                self.cmds.push(LinuxCmd::ImePreedit { text, cursor });
            }
            LinuxEvent::ImeCommit { text } => {
                self.stats.ime += 1;
                self.cmds.push(LinuxCmd::ImeCommit { text });
            }
            LinuxEvent::ImeDisabled => {
                self.stats.ime += 1;
                self.cmds.push(LinuxCmd::ImeCancel);
            }
            LinuxEvent::Density { scale } => {
                self.stats.density += 1;
                self.cmds.push(LinuxCmd::Density { scale });
            }
            LinuxEvent::OtherButton => {
                self.stats.ignored_buttons += 1;
            }
            LinuxEvent::Cancel => {
                self.cmds.push(LinuxCmd::PointerCancel);
            }
        }
    }

    pub fn take_cmds(&mut self) -> Vec<LinuxCmd> {
        std::mem::take(&mut self.cmds)
    }

    pub fn take_errors(&mut self) -> Vec<LinuxShellError> {
        std::mem::take(&mut self.errors)
    }

    pub fn take_ime_log(&mut self) -> Vec<ImeOps> {
        std::mem::take(&mut self.ime_log)
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// One-line intake counters for the run record.
    pub fn stats_summary(&self) -> String {
        let s = &self.stats;
        format!(
            "intake=mouse:{} keys:{} touches:{} chars:{} ime:{} density:{} ignored_buttons:{} ignored_keys:{}",
            s.mouse,
            s.keys,
            s.touches,
            s.chars,
            s.ime,
            s.density,
            s.ignored_buttons,
            s.ignored_keys,
        )
    }
}

impl PlatformShell for LinuxShell {
    fn pump_events(&mut self) -> Vec<oppa::shell::Event> {
        let queue = std::mem::take(&mut self.queue);
        for ev in queue {
            self.classify(ev);
        }
        Vec::new()
    }

    fn set_ime(&mut self, ops: ImeOps) {
        self.ime_log.push(ops);
    }

    /// File-open dialog seam (Round 2.3, OQ-G12-1 — closes the
    /// decision-246 session-local gap for picking, mirroring the
    /// Win32 arm; backends probe lazily on first request).
    fn file_dialog(&mut self) -> Option<&mut dyn oppa::FileDialog> {
        Some(&mut self.dialog)
    }
}

/// Framework keycode for a physical key, if routed
/// (Escape/Enter/Space/Tab/Backspace/Delete — the framework-routed
/// set, decision 243 adds the editing pair — plus the six Ctrl+letter
/// editing shortcuts, decision 246, plus the four arrows, round 5.3:
/// by physical code, so layouts keep working; the runner only acts
/// on them while ctrl is held, and plain presses stay router-quiet
/// like every other unhandled key).
pub fn keycode_to_framework(code: winit::keyboard::KeyCode) -> Option<u32> {
    use winit::keyboard::KeyCode;
    match code {
        KeyCode::Escape => Some(oppa::input::keys::ESCAPE),
        KeyCode::Enter | KeyCode::NumpadEnter => Some(oppa::input::keys::ENTER),
        KeyCode::Space => Some(oppa::input::keys::SPACE),
        KeyCode::Tab => Some(oppa::input::keys::TAB),
        KeyCode::Backspace => Some(oppa::input::keys::BACKSPACE),
        KeyCode::Delete => Some(oppa::input::keys::DELETE),
        KeyCode::ArrowLeft => Some(oppa::input::keys::LEFT),
        KeyCode::ArrowUp => Some(oppa::input::keys::UP),
        KeyCode::ArrowRight => Some(oppa::input::keys::RIGHT),
        KeyCode::ArrowDown => Some(oppa::input::keys::DOWN),
        KeyCode::Home => Some(oppa::input::keys::HOME),
        KeyCode::End => Some(oppa::input::keys::END),
        KeyCode::KeyA => Some(oppa::input::keys::A),
        KeyCode::KeyC => Some(oppa::input::keys::C),
        KeyCode::KeyV => Some(oppa::input::keys::V),
        KeyCode::KeyX => Some(oppa::input::keys::X),
        KeyCode::KeyY => Some(oppa::input::keys::Y),
        KeyCode::KeyZ => Some(oppa::input::keys::Z),
        _ => None,
    }
}

/// Sampled modifiers from a winit modifiers state.
pub fn translate_modifiers(state: winit::keyboard::ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        ctrl: state.control_key(),
        alt: state.alt_key(),
        meta: state.super_key(),
    }
}

/// Repackages one winit window event into shell intake, tracking
/// the cursor for button events (which carry none). Returns the
/// events to push (usually zero or one). Compiled everywhere;
/// mechanically untested (see module docs) — everything it feeds
/// is covered below.
pub fn translate(
    event: &winit::event::WindowEvent,
    cursor_dp: &mut (f32, f32),
    density: f32,
) -> Vec<LinuxEvent> {
    use winit::event::{ElementState, MouseButton, TouchPhase, WindowEvent};
    use winit::keyboard::PhysicalKey;
    match event {
        WindowEvent::CursorMoved { position, .. } => {
            cursor_dp.0 = position.x as f32 / density;
            cursor_dp.1 = position.y as f32 / density;
            vec![LinuxEvent::MouseMove {
                x_dp: cursor_dp.0,
                y_dp: cursor_dp.1,
            }]
        }
        WindowEvent::MouseInput { state, button, .. } => {
            // Round 9.2 (decision 301): left/right/middle classify
            // (primary/secondary/auxiliary); further buttons stay
            // counted and ignored (`OtherButton`).
            let button = match button {
                MouseButton::Left => oppa::PointerButton::Primary,
                MouseButton::Right => oppa::PointerButton::Secondary,
                MouseButton::Middle => oppa::PointerButton::Auxiliary,
                _ => return vec![LinuxEvent::OtherButton],
            };
            let (x_dp, y_dp) = *cursor_dp;
            match state {
                ElementState::Pressed => vec![LinuxEvent::MouseDown { button, x_dp, y_dp }],
                ElementState::Released => vec![LinuxEvent::MouseUp { button, x_dp, y_dp }],
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            use winit::event::MouseScrollDelta;
            let (dx, dy) = match delta {
                MouseScrollDelta::LineDelta(x, y) => (x * 30.0, y * 30.0),
                MouseScrollDelta::PixelDelta(p) => (p.x as f32 / density, p.y as f32 / density),
            };
            let (x_dp, y_dp) = *cursor_dp;
            vec![LinuxEvent::Wheel { x_dp, y_dp, dx, dy }]
        }
        WindowEvent::KeyboardInput { event, .. } => {
            let code = match event.physical_key {
                PhysicalKey::Code(code) => code,
                PhysicalKey::Unidentified(_) => return vec![],
            };
            let mut out = match event.state {
                ElementState::Pressed => vec![LinuxEvent::KeyDown { code }],
                ElementState::Released => vec![LinuxEvent::KeyUp { code }],
            };
            // Committed text rides the key press (decision 243):
            // control chars (Enter/Escape/Backspace shadows, DEL)
            // never become Char events — they ride `Key`.
            if matches!(event.state, ElementState::Pressed) {
                if let Some(text) = &event.text {
                    out.extend(
                        text.chars()
                            .filter(|c| !c.is_control())
                            .map(|ch| LinuxEvent::Char { ch }),
                    );
                }
            }
            out
        }
        // IME composition (Round 2.1, decision 256): the window
        // enables IME (the runner calls `set_ime_allowed`), so
        // preedits and commits arrive. Empty preedits stay unmapped —
        // winit sends one right before every `Commit`, and the commit
        // carries the text; `Enabled` carries nothing to route.
        WindowEvent::Ime(ime) => match ime {
            winit::event::Ime::Preedit(text, cursor) if !text.is_empty() => {
                vec![LinuxEvent::ImePreedit {
                    text: text.clone(),
                    cursor: *cursor,
                }]
            }
            winit::event::Ime::Commit(text) => vec![LinuxEvent::ImeCommit { text: text.clone() }],
            winit::event::Ime::Disabled => vec![LinuxEvent::ImeDisabled],
            _ => vec![],
        },
        // Monitor scale (Round 2.4, OQ-G10-2 — closes the old
        // "never enables / density-1.0" bound): the factor routes
        // runner-side (density + DPR + surface). The
        // `inner_size_writer` is intentionally untouched (we never
        // initiate resizes — the OS owns window geometry, `Resized`
        // carries size). This arm is review-only: the writer makes
        // the event unconstructible headless (the 2.1 `Ime`
        // exception does not extend here — stated).
        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
            vec![LinuxEvent::Density {
                scale: *scale_factor as f32,
            }]
        }
        WindowEvent::ModifiersChanged(modifiers) => {
            let m = translate_modifiers(modifiers.state());
            vec![LinuxEvent::ModifiersChanged {
                shift: m.shift,
                ctrl: m.ctrl,
                alt: m.alt,
                meta: m.meta,
            }]
        }
        WindowEvent::Touch(touch) => {
            let index = touch.id as u32;
            let x_dp = touch.location.x as f32 / density;
            let y_dp = touch.location.y as f32 / density;
            match touch.phase {
                TouchPhase::Started => vec![LinuxEvent::TouchDown { index, x_dp, y_dp }],
                TouchPhase::Moved => vec![LinuxEvent::TouchMove { index, x_dp, y_dp }],
                TouchPhase::Ended => vec![LinuxEvent::TouchUp { index, x_dp, y_dp }],
                TouchPhase::Cancelled => vec![LinuxEvent::Cancel],
            }
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell() -> LinuxShell {
        LinuxShell::new(ShellConfig {
            title: "t".to_string(),
            density: 2.0,
            width_dp: 400.0,
            height_dp: 300.0,
        })
    }

    fn pump(shell: &mut LinuxShell) -> (Vec<LinuxCmd>, Vec<LinuxShellError>) {
        use oppa::shell::PlatformShell;
        let _ = shell.pump_events();
        (shell.take_cmds(), shell.take_errors())
    }

    #[test]
    fn click_maps_dp_to_px() {
        let mut shell = shell();
        shell.push_event(LinuxEvent::MouseDown {
            button: oppa::PointerButton::Primary,
            x_dp: 10.0,
            y_dp: 12.0,
        });
        shell.push_event(LinuxEvent::MouseUp {
            button: oppa::PointerButton::Primary,
            x_dp: 10.0,
            y_dp: 12.0,
        });
        let (cmds, errors) = pump(&mut shell);
        assert!(errors.is_empty());
        assert_eq!(
            cmds,
            vec![
                LinuxCmd::PointerDown {
                    id: 0,
                    button: oppa::PointerButton::Primary,
                    x: 20.0,
                    y: 24.0
                },
                LinuxCmd::PointerUp {
                    id: 0,
                    button: oppa::PointerButton::Primary,
                    x: 20.0,
                    y: 24.0
                },
            ]
        );
        assert_eq!(shell.stats.mouse, 2);
    }

    /// Round 9.2 (decision 301): right/middle classify to
    /// secondary/auxiliary (left stays primary); the pipeline event
    /// carries the taxonomy into the router's tap dispatch.
    #[test]
    fn mouse_buttons_classify_to_taxonomy() {
        let mut shell = shell();
        for button in [
            oppa::PointerButton::Secondary,
            oppa::PointerButton::Auxiliary,
        ] {
            shell.push_event(LinuxEvent::MouseDown {
                button,
                x_dp: 10.0,
                y_dp: 12.0,
            });
            shell.push_event(LinuxEvent::MouseUp {
                button,
                x_dp: 10.0,
                y_dp: 12.0,
            });
        }
        let (cmds, errors) = pump(&mut shell);
        assert!(errors.is_empty());
        assert_eq!(
            cmds,
            vec![
                LinuxCmd::PointerDown {
                    id: 0,
                    button: oppa::PointerButton::Secondary,
                    x: 20.0,
                    y: 24.0
                },
                LinuxCmd::PointerUp {
                    id: 0,
                    button: oppa::PointerButton::Secondary,
                    x: 20.0,
                    y: 24.0
                },
                LinuxCmd::PointerDown {
                    id: 0,
                    button: oppa::PointerButton::Auxiliary,
                    x: 20.0,
                    y: 24.0
                },
                LinuxCmd::PointerUp {
                    id: 0,
                    button: oppa::PointerButton::Auxiliary,
                    x: 20.0,
                    y: 24.0
                },
            ]
        );
        // The pipeline event carries the button into the router.
        let ev = LinuxCmd::PointerDown {
            id: 0,
            button: oppa::PointerButton::Secondary,
            x: 20.0,
            y: 24.0,
        }
        .to_input_event(Modifiers::NONE);
        assert!(matches!(
            ev,
            InputEvent::Pointer {
                action: oppa::PointerAction::Down {
                    button: oppa::PointerButton::Secondary
                },
                ..
            }
        ));
    }

    #[test]
    fn second_touch_routes_with_its_own_id() {
        // G11 retires the v1 refusal: index 1 routes as pointer id 1
        // (the router holds one capture per id — decision 227).
        let mut shell = shell();
        shell.push_event(LinuxEvent::TouchDown {
            index: 1,
            x_dp: 5.0,
            y_dp: 5.0,
        });
        shell.push_event(LinuxEvent::TouchMove {
            index: 1,
            x_dp: 6.0,
            y_dp: 6.0,
        });
        shell.push_event(LinuxEvent::TouchUp {
            index: 1,
            x_dp: 6.0,
            y_dp: 6.0,
        });
        let (cmds, errors) = pump(&mut shell);
        assert!(errors.is_empty(), "nothing refuses anymore");
        assert_eq!(
            cmds,
            vec![
                LinuxCmd::PointerDown {
                    id: 1,
                    button: oppa::PointerButton::Primary,
                    x: 10.0,
                    y: 10.0
                },
                LinuxCmd::PointerMove {
                    id: 1,
                    x: 12.0,
                    y: 12.0
                },
                LinuxCmd::PointerUp {
                    id: 1,
                    button: oppa::PointerButton::Primary,
                    x: 12.0,
                    y: 12.0
                },
            ]
        );
        assert_eq!(shell.stats.touches, 3);
    }

    #[test]
    fn routed_keys_map_unmapped_count() {
        use winit::keyboard::KeyCode;
        let mut shell = shell();
        shell.push_event(LinuxEvent::KeyDown {
            code: KeyCode::Escape,
        });
        shell.push_event(LinuxEvent::KeyUp {
            code: KeyCode::Escape,
        });
        // Shortcut letters route since decision 246 (the runner acts
        // only while ctrl is held); F1 stays the unmapped probe.
        shell.push_event(LinuxEvent::KeyDown {
            code: KeyCode::KeyA,
        });
        shell.push_event(LinuxEvent::KeyDown { code: KeyCode::F1 });
        let (cmds, errors) = pump(&mut shell);
        assert!(errors.is_empty());
        assert_eq!(
            cmds,
            vec![
                LinuxCmd::Key {
                    code: oppa::input::keys::ESCAPE,
                    pressed: true
                },
                LinuxCmd::Key {
                    code: oppa::input::keys::ESCAPE,
                    pressed: false
                },
                LinuxCmd::Key {
                    code: oppa::input::keys::A,
                    pressed: true
                },
            ]
        );
        assert_eq!(shell.stats.ignored_keys, 1);
    }

    #[test]
    fn keycode_table_covers_routed_set() {
        use winit::keyboard::KeyCode;
        assert_eq!(
            keycode_to_framework(KeyCode::Escape),
            Some(oppa::input::keys::ESCAPE)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::Enter),
            Some(oppa::input::keys::ENTER)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::NumpadEnter),
            Some(oppa::input::keys::ENTER)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::Space),
            Some(oppa::input::keys::SPACE)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::Tab),
            Some(oppa::input::keys::TAB)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::Backspace),
            Some(oppa::input::keys::BACKSPACE)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::Delete),
            Some(oppa::input::keys::DELETE)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::KeyA),
            Some(oppa::input::keys::A)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::KeyC),
            Some(oppa::input::keys::C)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::KeyV),
            Some(oppa::input::keys::V)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::KeyX),
            Some(oppa::input::keys::X)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::KeyY),
            Some(oppa::input::keys::Y)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::KeyZ),
            Some(oppa::input::keys::Z)
        );
        // Round 5.3 arrows ride the routed set (physical codes).
        assert_eq!(
            keycode_to_framework(KeyCode::ArrowLeft),
            Some(oppa::input::keys::LEFT)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::ArrowUp),
            Some(oppa::input::keys::UP)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::ArrowRight),
            Some(oppa::input::keys::RIGHT)
        );
        assert_eq!(
            keycode_to_framework(KeyCode::ArrowDown),
            Some(oppa::input::keys::DOWN)
        );
        assert_eq!(keycode_to_framework(KeyCode::KeyQ), None);
        assert_eq!(keycode_to_framework(KeyCode::F1), None);
    }

    #[test]
    fn modifiers_state_samples() {
        use winit::keyboard::ModifiersState;
        let m = translate_modifiers(ModifiersState::SHIFT);
        assert!(m.shift && !m.ctrl && !m.alt && !m.meta);
        let m = translate_modifiers(ModifiersState::empty());
        assert_eq!(m, Modifiers::NONE);
    }

    #[test]
    fn wheel_routes_to_positioned_scroll_cmd() {
        // Decision 250 retires the ignore: wheel classifies to a
        // positioned Scroll (the runner hit-tests the target — the
        // shell still fabricates no NodeId). Test shell runs density
        // 2 (10dp → 20px, like the touch test above).
        let mut shell = shell();
        shell.push_event(LinuxEvent::Wheel {
            x_dp: 10.0,
            y_dp: 12.0,
            dx: 0.0,
            dy: 30.0,
        });
        let (cmds, _) = pump(&mut shell);
        assert_eq!(
            cmds,
            vec![LinuxCmd::Scroll {
                x: 20.0,
                y: 24.0,
                dx: 0.0,
                dy: 60.0
            }]
        );
    }

    #[test]
    fn touch_up_needs_no_position() {
        // Lift carries the last known position (a lift at (0,0)
        // would misroute to the origin — the spurious-flip class).
        let mut shell = shell();
        shell.push_event(LinuxEvent::TouchDown {
            index: 0,
            x_dp: 10.0,
            y_dp: 12.0,
        });
        shell.push_event(LinuxEvent::TouchUp {
            index: 0,
            x_dp: 10.0,
            y_dp: 12.0,
        });
        let (cmds, _) = pump(&mut shell);
        assert_eq!(
            cmds,
            vec![
                LinuxCmd::PointerDown {
                    id: 0,
                    button: oppa::PointerButton::Primary,
                    x: 20.0,
                    y: 24.0
                },
                LinuxCmd::PointerUp {
                    id: 0,
                    button: oppa::PointerButton::Primary,
                    x: 20.0,
                    y: 24.0
                },
            ]
        );
    }

    /// Round 2.1 (decision 256): winit `Ime` events need no live
    /// `DeviceId`, so `translate` is covered here — the one crack in
    /// the "mechanically untested" wall.
    #[test]
    fn ime_translate_maps_preedit_commit_and_disabled() {
        use winit::event::{Ime, WindowEvent};
        let mut cursor = (0.0f32, 0.0f32);
        // Non-empty preedit with a byte-wise cursor routes.
        assert_eq!(
            translate(
                &WindowEvent::Ime(Ime::Preedit("nihao".to_string(), Some((5, 5)))),
                &mut cursor,
                1.0,
            ),
            vec![LinuxEvent::ImePreedit {
                text: "nihao".to_string(),
                cursor: Some((5, 5)),
            }]
        );
        // Empty preedit stays unmapped (winit sends one before every
        // commit — the commit below carries the text).
        assert_eq!(
            translate(
                &WindowEvent::Ime(Ime::Preedit(String::new(), None)),
                &mut cursor,
                1.0,
            ),
            Vec::<LinuxEvent>::new()
        );
        assert_eq!(
            translate(
                &WindowEvent::Ime(Ime::Commit("你好".to_string())),
                &mut cursor,
                1.0,
            ),
            vec![LinuxEvent::ImeCommit {
                text: "你好".to_string(),
            }]
        );
        assert_eq!(
            translate(&WindowEvent::Ime(Ime::Disabled), &mut cursor, 1.0),
            vec![LinuxEvent::ImeDisabled]
        );
        assert_eq!(
            translate(&WindowEvent::Ime(Ime::Enabled), &mut cursor, 1.0),
            Vec::<LinuxEvent>::new(),
            "Enabled carries nothing to route"
        );
    }

    #[test]
    fn ime_events_classify_to_runner_matched_commands() {
        let mut shell = shell();
        shell.push_event(LinuxEvent::ImePreedit {
            text: "ni".to_string(),
            cursor: Some((2, 2)),
        });
        shell.push_event(LinuxEvent::ImeCommit {
            text: "你".to_string(),
        });
        shell.push_event(LinuxEvent::ImeDisabled);
        let (cmds, errors) = pump(&mut shell);
        assert!(errors.is_empty());
        assert_eq!(
            cmds,
            vec![
                LinuxCmd::ImePreedit {
                    text: "ni".to_string(),
                    cursor: Some((2, 2)),
                },
                LinuxCmd::ImeCommit {
                    text: "你".to_string(),
                },
                LinuxCmd::ImeCancel,
            ]
        );
        assert_eq!(shell.stats.ime, 3);
    }

    #[test]
    #[should_panic(expected = "must match ImePreedit first")]
    fn ime_preedit_refuses_pipeline_mapping_loudly() {
        LinuxCmd::ImePreedit {
            text: "ni".to_string(),
            cursor: None,
        }
        .to_input_event(Modifiers::NONE);
    }

    #[test]
    #[should_panic(expected = "must match ImeCommit first")]
    fn ime_commit_refuses_pipeline_mapping_loudly() {
        LinuxCmd::ImeCommit {
            text: "你".to_string(),
        }
        .to_input_event(Modifiers::NONE);
    }

    #[test]
    #[should_panic(expected = "must match ImeCancel first")]
    fn ime_cancel_refuses_pipeline_mapping_loudly() {
        LinuxCmd::ImeCancel.to_input_event(Modifiers::NONE);
    }

    /// Round 2.3 (decision 258): the shell lends its file dialog
    /// through the seam (Win32 parity — backends probe lazily, so
    /// construction stays infallible and headless).
    #[test]
    fn shell_file_dialog_seam_lends_some() {
        use oppa::shell::PlatformShell;
        let mut shell = shell();
        assert!(
            shell.file_dialog().is_some(),
            "LinuxShell lends its dialog (OQ-G12-1 closed)"
        );
    }

    /// Round 2.4 (decision 259): scale factors classify to
    /// runner-matched density commands (the `ScaleFactorChanged`
    /// event itself is unconstructible headless — the writer — so
    /// the event table below the translate wall is what is proven).
    #[test]
    fn density_classifies_and_counts() {
        let mut shell = shell();
        shell.push_event(LinuxEvent::Density { scale: 2.0 });
        let (cmds, errors) = pump(&mut shell);
        assert!(errors.is_empty());
        assert_eq!(cmds, vec![LinuxCmd::Density { scale: 2.0 }]);
        assert_eq!(shell.stats.density, 1);
    }

    #[test]
    fn density_rebases_and_refuses_garbage() {
        let mut shell = shell();
        shell.set_density(2.0);
        assert_eq!(shell.density(), 2.0);
    }

    #[test]
    #[should_panic(expected = "refused")]
    fn zero_density_refuses_loudly() {
        shell().set_density(0.0);
    }

    #[test]
    #[should_panic(expected = "refused")]
    fn nan_density_refuses_loudly() {
        shell().set_density(f32::NAN);
    }

    #[test]
    #[should_panic(expected = "must match Density first")]
    fn density_refuses_pipeline_mapping_loudly() {
        LinuxCmd::Density { scale: 2.0 }.to_input_event(Modifiers::NONE);
    }
}
