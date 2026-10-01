//! Live touch/key intake (v1 remainder, Gap 3): android-activity
//! input events into the shared `InputEvent` pipeline through
//! `AndroidShell` classification — the same router Win32 feeds.
//!
//! Mapping rules (stated; the shell owns classification, this
//! module only translates activity events into shell intake):
//!
//! - `MotionAction::{Down, PointerDown}` become `MotionDown` carrying
//!   the MotionEvent pointer index (G11 — every finger routes with
//!   its own id; the v1 fabricated second-finger drain is retired).
//! - `MotionAction::{Up, PointerUp}` become `MotionUp` the same way.
//! - `Move` becomes `MotionMove` for the action-index pointer (batch
//!   unpacking of simultaneous multi-pointer moves is OQ — one
//!   pointer per event, stated).
//! - `Outside/Hover*/Scroll/Button*` are counted and ignored
//!   (documented v1 bounds — no hover/scroll/button model).
//! - `KeyAction::{Down, Up}` become `KeyDown/KeyUp` with the
//!   platform keycode (`u32::from`); `Multiple` (key repeat) maps
//!   to `KeyDown` — the shell `Key` command carries no repeat flag
//!   (stated; the router stays quiet on unbound keys either way).
//! - `TextEvent/TextAction` (GameActivity text-input API) are
//!   counted and ignored on NativeActivity.
//! - Coordinates arrive in physical px; the shell takes dp, so px
//!   are divided by the config density (`dpi/160`, loud `Err` when
//!   the config reports none).

use android_activity::AndroidApp;
use oppa_shell_android::events::AndroidCmd;
use oppa_shell_android::events::AndroidEvent;
use oppa_shell_android::shell::{AndroidShell, ShellConfig};

/// Intake statistics for the run record (every ignored class is
/// counted — nothing vanishes silently).
#[derive(Default, Debug)]
pub struct IntakeStats {
    pub motion: usize,
    pub keys: usize,
    pub ignored_outside_hover_scroll_button: usize,
    pub ignored_text_api: usize,
    pub ignored_unknown_action: usize,
}

pub struct TouchDriver {
    shell: AndroidShell,
    density: f32,
    pub stats: IntakeStats,
}

impl TouchDriver {
    /// Builds the driver off the live config density (dpi/160);
    /// `width_px`/`height_px` are the window pixels (dp derived).
    /// Loud `Err` when the config reports no density.
    pub fn new(app: &AndroidApp, width_px: f32, height_px: f32) -> Result<Self, String> {
        let dpi = app
            .config()
            .density()
            .ok_or_else(|| "config reports no density".to_string())?;
        let density = dpi as f32 / 160.0;
        Ok(Self {
            shell: AndroidShell::new(ShellConfig {
                title: "oppa".to_string(),
                density,
                width_dp: width_px / density,
                height_dp: height_px / density,
            }),
            density,
            stats: IntakeStats::default(),
        })
    }

    pub fn density(&self) -> f32 {
        self.density
    }

    /// One-line intake counters for the run record.
    pub fn stats_summary(&self) -> String {
        let s = &self.stats;
        format!(
            "intake=motion:{} keys:{} ignored_hover_scroll_button:{} ignored_text_api:{} ignored_unknown:{}",
            s.motion,
            s.keys,
            s.ignored_outside_hover_scroll_button,
            s.ignored_text_api,
            s.ignored_unknown_action,
        )
    }

    pub fn shell_mut(&mut self) -> &mut AndroidShell {
        &mut self.shell
    }

    /// Drains the activity input queue into shell intake, pumps the
    /// shell, and returns the classified commands in event order.
    pub fn pump_activity(&mut self, app: &AndroidApp) -> Result<Vec<AndroidCmd>, String> {
        use android_activity::input::{InputEvent as ActivityInput, KeyAction, MotionAction};
        use oppa::shell::PlatformShell;

        // NOTE: the activity's `InputEvent` type lives behind a
        // crate-private `activity_impl` module, so the match is
        // inline in the inferred closure (not a named helper) —
        // the bindings still carry the full public motion/key API.
        let density = self.density;
        let stats = &mut self.stats;
        let shell = &mut self.shell;
        let mut iter = app
            .input_events_iter()
            .map_err(|e| format!("input_events_iter: {e:?}"))?;
        loop {
            let more = iter.next(|event| {
                match event {
                    ActivityInput::MotionEvent(motion) => {
                        stats.motion += 1;
                        let index = motion.pointer_index();
                        let coords = (0..motion.pointer_count()).contains(&index).then(|| {
                            let p = motion.pointer_at_index(index);
                            (p.x() / density, p.y() / density)
                        });
                        let (x_dp, y_dp) = match coords {
                            Some(c) => c,
                            None => {
                                stats.ignored_unknown_action += 1;
                                return android_activity::InputStatus::Handled;
                            }
                        };
                        let ev = match motion.action() {
                            MotionAction::Down => AndroidEvent::MotionDown {
                                pointer_index: index as u32,
                                x_dp,
                                y_dp,
                            },
                            MotionAction::Up => AndroidEvent::MotionUp {
                                pointer_index: index as u32,
                                x_dp,
                                y_dp,
                            },
                            MotionAction::Move => AndroidEvent::MotionMove {
                                pointer_index: index as u32,
                                x_dp,
                                y_dp,
                            },
                            MotionAction::Cancel => AndroidEvent::MotionCancel,
                            MotionAction::PointerDown => AndroidEvent::MotionDown {
                                pointer_index: index as u32,
                                x_dp,
                                y_dp,
                            },
                            MotionAction::PointerUp => AndroidEvent::MotionUp {
                                pointer_index: index as u32,
                                x_dp,
                                y_dp,
                            },
                            _ => {
                                stats.ignored_outside_hover_scroll_button += 1;
                                return android_activity::InputStatus::Handled;
                            }
                        };
                        shell.push_event(ev);
                    }
                    ActivityInput::KeyEvent(key) => {
                        stats.keys += 1;
                        let code = u32::from(key.key_code());
                        match key.action() {
                            KeyAction::Down | KeyAction::Multiple => {
                                shell.push_event(AndroidEvent::KeyDown { keycode: code });
                            }
                            KeyAction::Up => {
                                shell.push_event(AndroidEvent::KeyUp { keycode: code });
                            }
                            _ => {
                                stats.ignored_unknown_action += 1;
                            }
                        }
                    }
                    ActivityInput::TextEvent(_) | ActivityInput::TextAction(_) => {
                        stats.ignored_text_api += 1;
                    }
                    _ => {
                        stats.ignored_unknown_action += 1;
                    }
                }
                android_activity::InputStatus::Handled
            });
            if !more {
                break;
            }
        }
        let _events = self.shell.pump_events();
        let _errors = self.shell.take_errors();
        Ok(self.shell.take_cmds())
    }
}
