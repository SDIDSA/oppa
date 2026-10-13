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

use oppa::Modifiers;

mod events;
mod shell;
#[cfg(test)]
mod tests;

pub use events::{LinuxCmd, LinuxEvent, LinuxIntakeStats, LinuxShellError, ShellConfig};
pub use shell::LinuxShell;

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
