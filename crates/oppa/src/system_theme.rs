//! OS light/dark source seam (Round 16.2, decision 315).
//!
//! The reactive theme system (decision 306) recolors every control
//! from one host-level signal — this module is how the OS drives
//! that signal: platform backends read the system theme at startup
//! and on change, and runners forward readings into
//! [`ComponentHost::set_theme`](crate::component::ComponentHost::set_theme).
//! Unknown/unreadable always maps to `None` (the app default
//! stands — a theme is never guessed, the dialog-seam precedent).

use std::collections::VecDeque;

use crate::style::ThemeMode;

/// OS theme reader (Round 16.2, decision 315). `!Send` like every
/// UI-thread type (backends call thread-affine OS APIs).
pub trait SystemThemeSource {
    /// Current OS theme, if the platform can read one (`None` =
    /// unknown or unreadable — the caller keeps the app default,
    /// never a guessed mode).
    fn system_theme(&mut self) -> Option<ThemeMode>;
}

/// Headless/test theme source: scripted readings in order
/// (exhausted scripts repeat the last reading — level-triggered,
/// like the dialog polls).
#[derive(Debug, Default)]
pub struct ScriptedThemeSource {
    script: VecDeque<ThemeMode>,
    last: Option<ThemeMode>,
}

impl ScriptedThemeSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues one reading per upcoming query.
    pub fn push_reading(&mut self, mode: ThemeMode) {
        self.script.push_back(mode);
    }
}

impl SystemThemeSource for ScriptedThemeSource {
    fn system_theme(&mut self) -> Option<ThemeMode> {
        if let Some(mode) = self.script.pop_front() {
            self.last = Some(mode);
        }
        self.last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_readings_settle_and_repeat() {
        let mut s = ScriptedThemeSource::new();
        assert_eq!(s.system_theme(), None, "unreadable until scripted");
        s.push_reading(ThemeMode::Dark);
        assert_eq!(s.system_theme(), Some(ThemeMode::Dark));
        assert_eq!(s.system_theme(), Some(ThemeMode::Dark), "level repeat");
        s.push_reading(ThemeMode::Light);
        assert_eq!(s.system_theme(), Some(ThemeMode::Light));
    }
}
