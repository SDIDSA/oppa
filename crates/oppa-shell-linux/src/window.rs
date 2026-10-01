//! Runtime window chrome (Round 16.3, decision 316): title,
//! min/max sizes, and fullscreen over the live winit window,
//! behind [`WindowControl`](oppa::WindowControl).
//!
//! Desired state is stored on every call and applied to the live
//! window when one is attached (the runner attaches on open, so
//! pre-open calls still land — nothing is lost before the window
//! exists). No window attached absorbs calls silently
//! (best-effort presentational hints, documented per the seam).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use oppa::{WindowControl, WindowIcon};

/// Desired chrome (stored always, applied on attach + on set).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DesiredWindow {
    title: Option<String>,
    min: Option<(u32, u32)>,
    max: Option<(u32, u32)>,
    fullscreen: bool,
    icon: Option<WindowIcon>,
}

/// Live-window chrome control (clones share one record — the loop
/// owns one behind the trait object, the runner keeps one to
/// attach the window on open).
#[derive(Clone, Debug, Default)]
pub struct LinuxWindowControl {
    inner: Rc<RefCell<Inner>>,
}

#[derive(Debug, Default)]
struct Inner {
    window: Option<Arc<winit::window::Window>>,
    desired: DesiredWindow,
}

impl LinuxWindowControl {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attaches the live window (runner calls on open): applies
    /// every desired value stored so far, then tracks live.
    pub fn attach(&self, window: Arc<winit::window::Window>) {
        let mut inner = self.inner.borrow_mut();
        inner.window = Some(window.clone());
        apply(&window, &inner.desired);
    }

    /// Desired state (test observability — headless proofs that
    /// pre-open calls land; the OS application itself needs a
    /// window and is reviewed, never faked).
    #[cfg(test)]
    pub(crate) fn desired(&self) -> DesiredWindow {
        self.inner.borrow().desired.clone()
    }
}

/// Pushes desired chrome into a live window (one rule for attach
/// and set — attach replays everything, sets replay their own
/// write through the same path).
fn apply(window: &winit::window::Window, desired: &DesiredWindow) {
    use winit::dpi::PhysicalSize;
    if let Some(title) = &desired.title {
        window.set_title(title);
    }
    window.set_min_inner_size(desired.min.map(|(w, h)| PhysicalSize::new(w, h)));
    window.set_max_inner_size(desired.max.map(|(w, h)| PhysicalSize::new(w, h)));
    window.set_fullscreen(if desired.fullscreen {
        Some(winit::window::Fullscreen::Borderless(None))
    } else {
        None
    });
    apply_icon(window, &desired.icon);
}

/// Pushes a validated [`WindowIcon`] into a live winit window
/// (Round 20.3, decision 326 — one rule for attach and set: `None`
/// restores the OS default). Dimensions are already validated by
/// [`WindowIcon::new`], so a `from_rgba` refusal is impossible by
/// construction — it names itself loudly and keeps the previous
/// icon instead of clearing it.
fn apply_icon(window: &winit::window::Window, icon: &Option<WindowIcon>) {
    match icon {
        None => window.set_window_icon(None),
        Some(spec) => {
            match winit::window::Icon::from_rgba(spec.rgba().to_vec(), spec.width(), spec.height())
            {
                Ok(built) => window.set_window_icon(Some(built)),
                Err(e) => {
                    eprintln!("oppa-shell-linux: window icon refused [{e:?}] — keeping previous")
                }
            }
        }
    }
}

impl WindowControl for LinuxWindowControl {
    fn set_title(&self, title: &str) {
        let mut inner = self.inner.borrow_mut();
        inner.desired.title = Some(title.to_string());
        if let Some(window) = inner.window.as_ref() {
            window.set_title(title);
        }
    }

    fn set_min_size(&self, size: Option<(u32, u32)>) {
        use winit::dpi::PhysicalSize;
        let mut inner = self.inner.borrow_mut();
        inner.desired.min = size;
        if let Some(window) = inner.window.as_ref() {
            window.set_min_inner_size(size.map(|(w, h)| PhysicalSize::new(w, h)));
        }
    }

    fn set_max_size(&self, size: Option<(u32, u32)>) {
        use winit::dpi::PhysicalSize;
        let mut inner = self.inner.borrow_mut();
        inner.desired.max = size;
        if let Some(window) = inner.window.as_ref() {
            window.set_max_inner_size(size.map(|(w, h)| PhysicalSize::new(w, h)));
        }
    }

    fn set_fullscreen(&self, fullscreen: bool) {
        let mut inner = self.inner.borrow_mut();
        inner.desired.fullscreen = fullscreen;
        if let Some(window) = inner.window.as_ref() {
            window.set_fullscreen(if fullscreen {
                Some(winit::window::Fullscreen::Borderless(None))
            } else {
                None
            });
        }
    }

    fn set_icon(&self, icon: Option<WindowIcon>) {
        let mut inner = self.inner.borrow_mut();
        inner.desired.icon = icon.clone();
        if let Some(window) = inner.window.as_ref() {
            apply_icon(window, &icon);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_open_calls_store_desired() {
        let c = LinuxWindowControl::new();
        assert_eq!(c.desired(), DesiredWindow::default(), "starts clear");
        c.set_title("Hi");
        c.set_min_size(Some((100, 80)));
        c.set_max_size(Some((400, 300)));
        c.set_fullscreen(true);
        let icon = WindowIcon::new(vec![9; 4 * 4 * 4], 4, 4).expect("valid icon builds");
        c.set_icon(Some(icon.clone()));
        assert_eq!(
            c.desired(),
            DesiredWindow {
                title: Some("Hi".to_string()),
                min: Some((100, 80)),
                max: Some((400, 300)),
                fullscreen: true,
                icon: Some(icon),
            },
            "pre-open calls land instead of dropping"
        );
        // Clones share the record (loop/runner split).
        assert_eq!(c.clone().desired(), c.desired());
        c.set_fullscreen(false);
        assert!(!c.desired().fullscreen);
        // Reset clears back to the OS default.
        c.set_icon(None);
        assert_eq!(c.desired().icon, None);
    }

    /// Round 20.3 (decision 326): a validated icon converts into a
    /// live winit icon (the `apply_icon` conversion winit itself
    /// accepts — `from_rgba` refuses nothing `WindowIcon::new`
    /// allowed through).
    #[test]
    fn validated_icon_converts_to_winit_icon() {
        let spec = WindowIcon::new(vec![9; 4 * 4 * 4], 4, 4).expect("valid icon builds");
        assert!(
            winit::window::Icon::from_rgba(spec.rgba().to_vec(), spec.width(), spec.height())
                .is_ok(),
            "validated dims convert"
        );
    }
}
