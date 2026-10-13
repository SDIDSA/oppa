// ---------------------------------------------------------------------------
// Command dispatch (Cmd -> framework input)
// ---------------------------------------------------------------------------

use super::ime::{post_input_ime, WinImeMapper};
use super::window::{key_down, sync_cursor};
use super::*;

/// One pumped command's outcome: exit requests unwind the loop
/// (clean exit 0); damage re-presents.
pub(crate) struct DriveOut {
    pub(crate) exit: bool,
    pub(crate) damaged: bool,
}

/// Maps one Win32 shell command into framework input. Discrete
/// pointer commands step straight into the reactive router
/// (Round 7.19, decision 294 — parity with Linux/Android):
/// Down focuses + presses + captures, Moves drive `on_drag` through
/// the capture owner (slider track math reads the capture position),
/// Up classifies the tap / drag-release, Cancel trips the global
/// release. Keys pass their virtual-key code through (the
/// framework key space matches Win32 VKs for the routed set —
/// verified in `input::keys`); Backspace/Delete route to the focused
/// session (consumed, never also injected); Ctrl+letter editing
/// shortcuts are consumed inside [`DesktopLoop::step`] (same
/// never-injected rule); wheel ticks hit-test their scroll target
/// through [`DesktopLoop::scroll_at`]; printable `WM_CHAR`s type into
/// it unless the IME mapper is holding them as commit echo (see
/// [`WinImeMapper`]). Legacy `Click` still synthesizes a tap (kept
/// for backward compatibility — the shell no longer emits it now
/// that Down/Up step discretely); legacy `Drag` is a quiet no-op
/// (the 1D from/to collapse cannot address the router — Moves carry
/// the drag now). Click variants the pipeline cannot express
/// (double/shift field clicks) collapse to a plain tap — stated, not
/// silent.
pub(crate) fn drive_cmd(
    loop_: &mut DesktopLoop,
    shell: &mut Win32Shell,
    ime: &mut WinImeMapper,
    cmd: Cmd,
) -> Result<DriveOut, String> {
    let quiet = DriveOut {
        exit: false,
        damaged: false,
    };
    match cmd {
        Cmd::PointerDown {
            x,
            y,
            shift,
            button,
        } => {
            // Round 8.2: shift rides into the router's tap modifiers
            // (Shift+Click extends field selections end to end).
            // Round 9.2: the OS button rides into the tap taxonomy
            // (secondary taps dispatch menu events, never presses).
            let modifiers = if shift {
                Modifiers::shift()
            } else {
                Modifiers::NONE
            };
            let damage = loop_.step(InputEvent::Pointer {
                id: Some(0),
                action: oppa::input::PointerAction::Down { button },
                x,
                y,
                modifiers,
            })?;
            // A press may move field focus: re-anchor + re-sync the
            // candidate machinery to the newly focused session.
            post_input_ime(loop_, shell);
            // Round 8.3: Down sets hover too — publish its cursor.
            sync_cursor(loop_, shell);
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::PointerMove { x, y } => {
            let damage = loop_.step(InputEvent::pointer_move(x, y))?;
            // Round 8.3: hover moved — publish the hovered style.
            sync_cursor(loop_, shell);
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::PointerUp {
            x,
            y,
            shift,
            button,
        } => {
            // Round 8.2: release shift rides with the press (the tap
            // modifiers the field Shift+Click arm reads). Round 9.2:
            // the release button pairs with the Down by pointer id.
            let modifiers = if shift {
                Modifiers::shift()
            } else {
                Modifiers::NONE
            };
            let damage = loop_.step(InputEvent::Pointer {
                id: Some(0),
                action: oppa::input::PointerAction::Up { button },
                x,
                y,
                modifiers,
            })?;
            // A release may move field focus (tap): re-anchor +
            // re-sync like a press.
            post_input_ime(loop_, shell);
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::PointerCancel => {
            let damage = loop_.step(InputEvent::pointer_cancel())?;
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::Click { x, y, .. } => {
            let (x, y) = (x as f32, y as f32);
            let damage = loop_.step(InputEvent::pointer_down(x, y))?;
            let damage = damage + loop_.step(InputEvent::pointer_up(x, y))?;
            // A click may move field focus: re-anchor + re-sync the
            // candidate machinery to the newly focused session.
            post_input_ime(loop_, shell);
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::Key { vk, shift, ctrl } => {
            // Fresh user intent closes the commit-echo window (a stale
            // swallow count must never eat real typing — the echo of a
            // commit always arrives without an interleaving key).
            ime.clear_swallow();
            if loop_.escape_exits(vk, true) {
                return Ok(DriveOut {
                    exit: true,
                    damaged: false,
                });
            }
            // Editing keys are consumed by the focused session (never
            // also injected — a future field `Key` handler must not
            // see them twice).
            if vk == oppa::input::keys::BACKSPACE {
                let damage = loop_.backspace()?;
                post_input_ime(loop_, shell);
                return Ok(DriveOut {
                    exit: false,
                    damaged: damage > 0,
                });
            }
            if vk == oppa::input::keys::DELETE {
                let damage = loop_.delete_forward()?;
                post_input_ime(loop_, shell);
                return Ok(DriveOut {
                    exit: false,
                    damaged: damage > 0,
                });
            }
            // Alt rides live state, not `Cmd` (decision 246): the
            // `step` interception refuses ctrl+alt (AltGr types real
            // chars on many layouts), and the injected modifiers
            // should stop dropping alt in the same stroke.
            use windows::Win32::UI::Input::KeyboardAndMouse::VK_MENU;
            let damage = loop_.step(InputEvent::Key {
                code: vk,
                modifiers: Modifiers {
                    shift,
                    ctrl,
                    alt: key_down(VK_MENU.0),
                    ..Modifiers::NONE
                },
                state: KeyState::Pressed,
                repeat: false,
            })?;
            // Arrows (and any key) may move the composition caret.
            post_input_ime(loop_, shell);
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::Char { ch } => {
            // Control chars (< 0x20) are shortcut shadows (Backspace,
            // Enter, Escape arrive via `KeyDown`) — skipped per the
            // spike's WM_CHAR rule; `type_text` guards the rest.
            if (ch as u32) < 0x20 {
                return Ok(quiet);
            }
            // Commit echo (the IME delivering the committed text a
            // second time as WM_CHARs) is swallowed so committed text
            // is never inserted twice — the spike's swallow rule.
            if ime.swallow_char(ch) {
                return Ok(quiet);
            }
            let damage = loop_.type_text(&ch.to_string())?;
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::Scroll { x, y, dx, dy } => {
            let damage = loop_.scroll_at(x, y, dx, dy)?;
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::Drag { .. } => Ok(quiet),
        Cmd::FocusChanged(focused) => {
            if !focused {
                // External focus management (locked #27): a real focus
                // change commits any active IME composition.
                loop_.host().notify_edit_focus_lost();
            }
            // The TSF document manager follows window focus.
            shell.tsf_note_focus(focused);
            Ok(quiet)
        }
        Cmd::DpiChanged { w, h, dpi, .. } => {
            // Monitor DPI change (Round 2.4, OQ-G10-2): the HWND
            // repositioning to the suggested rect happens in
            // `run_windows` (`SetWindowPos` — outside the pipeline);
            // here the pipeline re-bases DPR first (CSS stable, so
            // the resize below usually no-ops) and repaints crisply
            // at the new scale. x/y ride the Cmd for the runner.
            loop_.set_device_pixel_ratio(oppa::dpr_from_dpi(dpi))?;
            // Negative suggested sizes are OS garbage — clamp before
            // the u32 cast (wrapping would surface as billions).
            loop_.resize(w.max(0) as u32, h.max(0) as u32)?;
            let damage = loop_.repaint()?;
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::SystemThemeChanged => {
            // OS light/dark flip (Round 16.2, decision 315):
            // re-query the installed source into the reactive theme
            // signal (themed bodies recolor in place); the sync
            // repaints itself exactly when the mode moved.
            let changed = loop_.sync_system_theme()?;
            Ok(DriveOut {
                exit: false,
                damaged: changed,
            })
        }
        Cmd::CloseRequested => {
            // Close veto (Round 16.3, decision 316): the app-level
            // handler decides — approved closes destroy through the
            // normal DestroyWindow→WM_DESTROY→quit flow (the pump
            // exits on the quit message); vetoed closes keep the
            // window running so a modal can mount on the next frame.
            if loop_.close_requested() {
                shell.destroy_window();
            }
            Ok(quiet)
        }
    }
}
