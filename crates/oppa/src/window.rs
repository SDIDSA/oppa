//! Runtime window-chrome seam (Round 16.3, decision 316).
//!
//! Title, min/max sizes, and fullscreen change while the app runs
//! — through runner-installed handles, never statics: the desktop
//! loop forwards its window calls into an installed
//! [`WindowControl`], and headless loops with no control stay quiet
//! no-ops (window chrome is a presentational hint like `set_cursor`
//! — advisory, never a wiring bug). The close veto rides beside
//! it: runners consult the loop-installed handler on `WM_CLOSE` /
//! `CloseRequested` and suppress exit when it refuses, so
//! unsaved-changes flows can mount instead of dying.

use std::cell::RefCell;
use std::rc::Rc;

/// Runtime window icon (Round 20.3, decision 326): an RGBA8
/// pixmap (row-major, top-left first) for the OS window chrome
/// (taskbar / title bar / alt-tab). Constructible only through
/// [`WindowIcon::new`] — zero dimensions and buffer-length
/// mismatches are loud `Err`s there, never a silently dropped or
/// misread icon; backends receive only validated icons.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowIcon {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

impl WindowIcon {
    /// Validates loudly: `width > 0`, `height > 0`, and
    /// `rgba.len() == width * height * 4`.
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err(format!(
                "window icon refused: dimensions must be nonzero, got {width}x{height}"
            ));
        }
        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(format!(
                "window icon refused: {} bytes != {width}x{height}x4 ({expected})",
                rgba.len()
            ));
        }
        Ok(Self {
            rgba,
            width,
            height,
        })
    }

    /// Raw RGBA8 bytes (row-major).
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

/// Runtime window chrome (Round 16.3, decision 316). `&self`
/// methods (shared-handle friendly — backends hold OS handles or
/// `Rc`-shared state, never `&mut` exclusivity). Best-effort
/// presentational hints: a dead or not-yet-open window absorbs
/// calls silently (documented per backend, never a surprise
/// hang — dialogs block, chrome never does).
pub trait WindowControl {
    /// Retitles the window.
    fn set_title(&self, title: &str);
    /// Constrains the resizable floor (`None` clears it back to
    /// the OS default).
    fn set_min_size(&self, size: Option<(u32, u32)>);
    /// Constrains the resizable ceiling (`None` clears it back to
    /// the OS default).
    fn set_max_size(&self, size: Option<(u32, u32)>);
    /// Toggles borderless fullscreen (state-preserving: exit
    /// restores the pre-fullscreen style and rect).
    fn set_fullscreen(&self, fullscreen: bool);
    /// Installs the runtime window icon (`None` restores the OS
    /// default). Receives only [`WindowIcon::new`]-validated icons —
    /// a backend never re-validates dimensions, it converts.
    fn set_icon(&self, icon: Option<WindowIcon>);
}

/// One recorded chrome call (headless observability — tests prove
/// each loop method forwards exactly).
#[derive(Clone, Debug, PartialEq)]
pub enum WindowCall {
    SetTitle(String),
    SetMinSize(Option<(u32, u32)>),
    SetMaxSize(Option<(u32, u32)>),
    SetFullscreen(bool),
    SetIcon(Option<WindowIcon>),
}

/// Headless/test window control: records every call in order (no
/// window to drive — forwarding proofs, never behavior proofs).
/// Clones share one record (install a clone, read through the
/// original — the loop owns its handle, the test keeps its own).
#[derive(Clone, Debug, Default)]
pub struct ScriptedWindowControl {
    calls: Rc<RefCell<Vec<WindowCall>>>,
}

impl ScriptedWindowControl {
    pub fn new() -> Self {
        Self::default()
    }

    /// Recorded calls so far, in order.
    pub fn calls(&self) -> Vec<WindowCall> {
        self.calls.borrow().clone()
    }
}

impl WindowControl for ScriptedWindowControl {
    fn set_title(&self, title: &str) {
        self.calls
            .borrow_mut()
            .push(WindowCall::SetTitle(title.to_string()));
    }

    fn set_min_size(&self, size: Option<(u32, u32)>) {
        self.calls.borrow_mut().push(WindowCall::SetMinSize(size));
    }

    fn set_max_size(&self, size: Option<(u32, u32)>) {
        self.calls.borrow_mut().push(WindowCall::SetMaxSize(size));
    }

    fn set_fullscreen(&self, fullscreen: bool) {
        self.calls
            .borrow_mut()
            .push(WindowCall::SetFullscreen(fullscreen));
    }

    fn set_icon(&self, icon: Option<WindowIcon>) {
        self.calls.borrow_mut().push(WindowCall::SetIcon(icon));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_control_records_every_call() {
        let c = ScriptedWindowControl::new();
        c.set_title("Hi");
        c.set_min_size(Some((100, 80)));
        c.set_max_size(None);
        c.set_fullscreen(true);
        let icon = WindowIcon::new(vec![255; 2 * 2 * 4], 2, 2).expect("valid icon builds");
        c.set_icon(Some(icon.clone()));
        c.set_icon(None);
        assert_eq!(
            c.calls(),
            vec![
                WindowCall::SetTitle("Hi".to_string()),
                WindowCall::SetMinSize(Some((100, 80))),
                WindowCall::SetMaxSize(None),
                WindowCall::SetFullscreen(true),
                WindowCall::SetIcon(Some(icon)),
                WindowCall::SetIcon(None),
            ]
        );
    }

    #[test]
    fn window_icon_new_refuses_bad_shapes_loudly() {
        assert!(WindowIcon::new(vec![0; 16], 2, 2).is_ok());
        assert!(WindowIcon::new(vec![], 0, 2).is_err(), "zero width refused");
        assert!(
            WindowIcon::new(vec![], 2, 0).is_err(),
            "zero height refused"
        );
        assert!(
            WindowIcon::new(vec![0; 15], 2, 2).is_err(),
            "short buffer refused"
        );
        assert!(
            WindowIcon::new(vec![0; 17], 2, 2).is_err(),
            "long buffer refused"
        );
        assert!(
            WindowIcon::new(vec![0; 16], 2, 2)
                .expect("valid")
                .rgba()
                .len()
                == 16,
            "accessors expose the validated pixmap"
        );
    }
}
