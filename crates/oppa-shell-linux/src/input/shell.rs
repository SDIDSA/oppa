//! Intake shell: queue + classification + IME log + file-dialog seam.

use oppa::ime::ImeOps;
use oppa::shell::PlatformShell;
use oppa::Modifiers;

use super::events::{LinuxCmd, LinuxEvent, LinuxIntakeStats, LinuxShellError, ShellConfig};
use super::keycode_to_framework;

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
    dialog: crate::file_dialog::LinuxFileDialog<crate::file_dialog::StdRunner>,
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
            dialog: crate::file_dialog::LinuxFileDialog::new(crate::file_dialog::StdRunner),
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
                // Phase 36 PR2b (decision 354): Shift+wheel rolls
                // horizontally (the native convention, the Windows
                // `wheel_event` twin) — a Shift-held purely-vertical
                // tick reroutes as `dx`; device-native `dx` rides
                // untouched, and unshifted ticks pass through exactly.
                let (dx, dy) = if self.modifiers.shift && dx == 0.0 {
                    (dy, 0.0)
                } else {
                    (dx, dy)
                };
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
