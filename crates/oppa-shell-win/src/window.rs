//! Runtime window chrome (Round 16.3, decision 316): title,
//! min/max track sizes, and borderless fullscreen over the live
//! HWND, behind [`WindowControl`](oppa::WindowControl).
//!
//! Best-effort presentational hints (failures absorb silently —
//! the loop proves forwarding headlessly; the OS application is
//! reviewed, never a surprise hang): a dead HWND (post-destroy)
//! simply ignores calls. Min/max sizes land in the shared table
//! the wndproc's `WM_GETMINMAXINFO` arm enforces; fullscreen
//! snapshots style + rect and restores both on exit.

use std::cell::RefCell;
use std::rc::Rc;

use oppa::{WindowControl, WindowIcon};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, DeleteObject, GetMonitorInfoW, MonitorFromWindow, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, GetWindowLongW, GetWindowRect, SendMessageW, SetWindowLongW,
    SetWindowPos, SetWindowTextW, GWL_EXSTYLE, GWL_STYLE, HWND_TOP, ICON_BIG, ICON_SMALL,
    SWP_FRAMECHANGED, SWP_NOZORDER, SWP_SHOWWINDOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_SETICON,
    WS_POPUP, WS_VISIBLE,
};
use windows_core::PCWSTR;

use super::win::{ShellShared, Win32Shell};

/// Pre-fullscreen window chrome (restored verbatim on exit).
struct FullscreenSaved {
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    rect: RECT,
}

/// Live-HWND chrome control (the runner installs one bound to its
/// shell — min/max flow through the shared table the wndproc
/// reads, everything else calls the HWND directly).
pub struct Win32WindowControl {
    hwnd: HWND,
    shared: Rc<RefCell<ShellShared>>,
    fullscreen_saved: RefCell<Option<FullscreenSaved>>,
    /// Icons this control owns (created via `CreateIconIndirect` for
    /// the live `WM_SETICON` pair — destroyed on replace and on
    /// drop, so reinstalls never leak GDI handles).
    icons: RefCell<Vec<windows::Win32::UI::WindowsAndMessaging::HICON>>,
}

impl Win32WindowControl {
    /// Binds the live shell (clones its HWND + shared table).
    pub fn of_shell(shell: &Win32Shell) -> Self {
        Self {
            hwnd: shell.hwnd(),
            shared: shell.shared.clone(),
            fullscreen_saved: RefCell::new(None),
            icons: RefCell::new(Vec::new()),
        }
    }

    /// Owned-icon count (test observability — installs own exactly
    /// the live pair, resets own none).
    #[cfg(test)]
    fn owned_icon_count(&self) -> usize {
        self.icons.borrow().len()
    }
}

/// Builds one `HICON` from a validated [`WindowIcon`] (Round 20.3,
/// decision 326): RGBA → BGRA swap into a 32bpp color bitmap + an
/// empty monochrome mask through `CreateIconIndirect`. `None` on any
/// GDI failure (the caller keeps the previous icon and names the
/// failure — a failed build never half-applies).
fn icon_from_rgba(icon: &WindowIcon) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    use windows::core::BOOL;
    use windows::Win32::UI::WindowsAndMessaging::ICONINFO;
    let (w, h) = (icon.width() as i32, icon.height() as i32);
    let mut bgra = Vec::with_capacity(icon.rgba().len());
    for px in icon.rgba().chunks_exact(4) {
        bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    unsafe {
        // windows 0.62 returns bare handles here (null on failure —
        // the `is_invalid` shape, same as the IMM contexts in
        // `win.rs`), not `Result`s.
        let hbm_color = CreateBitmap(w, h, 1, 32, Some(bgra.as_ptr() as *const _));
        if hbm_color.is_invalid() {
            eprintln!("oppa-shell-win: icon color bitmap refused — keeping previous");
            return None;
        }
        let hbm_mask = CreateBitmap(w, h, 1, 1, None);
        if hbm_mask.is_invalid() {
            eprintln!("oppa-shell-win: icon mask bitmap refused — keeping previous");
            let _ = DeleteObject(hbm_color.into());
            return None;
        }
        let info = ICONINFO {
            fIcon: BOOL(1),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: hbm_mask,
            hbmColor: hbm_color,
        };
        let icon_handle = match CreateIconIndirect(&info) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("oppa-shell-win: CreateIconIndirect refused [{e:?}] — keeping previous");
                let _ = DeleteObject(hbm_color.into());
                let _ = DeleteObject(hbm_mask.into());
                return None;
            }
        };
        // The icon copies both bitmaps — free the sources.
        let _ = DeleteObject(hbm_color.into());
        let _ = DeleteObject(hbm_mask.into());
        Some(icon_handle)
    }
}

impl Drop for Win32WindowControl {
    fn drop(&mut self) {
        for owned in self.icons.borrow_mut().drain(..) {
            let _ = unsafe { DestroyIcon(owned) };
        }
    }
}

impl WindowControl for Win32WindowControl {
    fn set_title(&self, title: &str) {
        let wide: Vec<u16> = title.encode_utf16().chain([0]).collect();
        unsafe {
            let _ = SetWindowTextW(self.hwnd, PCWSTR::from_raw(wide.as_ptr()));
        }
    }

    fn set_min_size(&self, size: Option<(u32, u32)>) {
        self.shared.borrow_mut().min_size = size;
    }
    fn set_max_size(&self, size: Option<(u32, u32)>) {
        self.shared.borrow_mut().max_size = size;
    }

    fn set_fullscreen(&self, fullscreen: bool) {
        let mut saved = self.fullscreen_saved.borrow_mut();
        if fullscreen == saved.is_some() {
            return;
        }
        let hwnd = self.hwnd;
        if fullscreen {
            // Snapshot first (a failed snapshot aborts before any
            // chrome moves — half-applied fullscreen never shows).
            let prev = unsafe {
                let style = WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32);
                let ex_style = WINDOW_EX_STYLE(GetWindowLongW(hwnd, GWL_EXSTYLE) as u32);
                let mut rect = RECT::default();
                if GetWindowRect(hwnd, &mut rect).is_err() {
                    return;
                }
                FullscreenSaved {
                    style,
                    ex_style,
                    rect,
                }
            };
            let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !unsafe { GetMonitorInfoW(monitor, &mut info).as_bool() } {
                return;
            }
            *saved = Some(prev);
            unsafe {
                let _ = SetWindowLongW(hwnd, GWL_STYLE, (WS_POPUP.0 | WS_VISIBLE.0) as i32);
                let r = info.rcMonitor;
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOP),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_FRAMECHANGED | SWP_SHOWWINDOW,
                );
            }
        } else if let Some(prev) = saved.take() {
            unsafe {
                let _ = SetWindowLongW(hwnd, GWL_STYLE, prev.style.0 as i32);
                let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, prev.ex_style.0 as i32);
                let r = prev.rect;
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_FRAMECHANGED | SWP_NOZORDER | SWP_SHOWWINDOW,
                );
            }
        }
    }

    fn set_icon(&self, icon: Option<WindowIcon>) {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        // `ICON_SMALL`/`ICON_BIG` are u32 in windows 0.62 (WPARAM
        // takes usize — the `as` cast, same as every flag word here).
        let small = WPARAM(ICON_SMALL as usize);
        let big = WPARAM(ICON_BIG as usize);
        // Build first (a failed build keeps the previous icon — the
        // builder names the failure itself, so this stays quiet).
        let owned = match icon.as_ref() {
            Some(spec) => {
                let (Some(small_icon), Some(big_icon)) =
                    (icon_from_rgba(spec), icon_from_rgba(spec))
                else {
                    return;
                };
                unsafe {
                    SendMessageW(
                        self.hwnd,
                        WM_SETICON,
                        Some(small),
                        Some(LPARAM(small_icon.0 as isize)),
                    );
                    SendMessageW(
                        self.hwnd,
                        WM_SETICON,
                        Some(big),
                        Some(LPARAM(big_icon.0 as isize)),
                    );
                }
                vec![small_icon, big_icon]
            }
            // `None` restores the OS default (NULL icon pair).
            None => {
                unsafe {
                    SendMessageW(self.hwnd, WM_SETICON, Some(small), Some(LPARAM(0)));
                    SendMessageW(self.hwnd, WM_SETICON, Some(big), Some(LPARAM(0)));
                }
                Vec::new()
            }
        };
        // The window owns its copy from here — destroy everything
        // this control previously owned (replace never leaks).
        let mut icons = self.icons.borrow_mut();
        for old in icons.drain(..) {
            let _ = unsafe { DestroyIcon(old) };
        }
        *icons = owned;
    }
}

#[cfg(test)]
mod tests {
    use super::super::win::{ShellConfig, Win32Shell};
    use super::*;

    fn hidden_shell() -> Win32Shell {
        Win32Shell::new(ShellConfig {
            title: "icon-test".to_string(),
            width: 200,
            height: 150,
            record_messages: false,
            suppress_os_composition_window: true,
            visible: false,
        })
        .expect("hidden test window builds")
    }

    fn red_icon() -> WindowIcon {
        // 4x4 opaque red - valid by construction (4*4*4 bytes).
        WindowIcon::new([255u8, 0, 0, 255].repeat(16), 4, 4).expect("valid icon builds")
    }

    /// Round 20.3 (decision 326): the RGBA builder produces a live
    /// HICON through real GDI (headless-safe - no window needed).
    #[test]
    fn icon_builder_produces_live_hicon() {
        let handle = icon_from_rgba(&red_icon()).expect("GDI builds the icon");
        assert!(!handle.0.is_null(), "non-null HICON");
        let _ = unsafe { DestroyIcon(handle) };
    }

    /// Round 20.3 (decision 326): installing an icon owns exactly
    /// the live SMALL+BIG pair on a real hidden window, replacing
    /// keeps exactly two (no handle leak), and `None` resets to the
    /// OS default owning none.
    #[test]
    fn set_icon_owns_pair_replace_keeps_two_reset_owns_none() {
        let shell = hidden_shell();
        let control = Win32WindowControl::of_shell(&shell);
        assert_eq!(control.owned_icon_count(), 0, "starts owning nothing");
        control.set_icon(Some(red_icon()));
        assert_eq!(control.owned_icon_count(), 2, "install owns SMALL+BIG");
        control.set_icon(Some(red_icon()));
        assert_eq!(control.owned_icon_count(), 2, "replace destroys first");
        control.set_icon(None);
        assert_eq!(control.owned_icon_count(), 0, "reset restores default");
    }
}
