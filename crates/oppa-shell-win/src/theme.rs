//! Windows OS theme source (Round 16.2, decision 315): the
//! `AppsUseLightTheme` registry value for the startup query plus
//! `WM_SETTINGCHANGE` with `lParam == "ImmersiveColorSet"` for live
//! updates, behind the [`SystemThemeSource`](oppa::SystemThemeSource)
//! seam.
//!
//! Unknown/unreadable always maps to `None` (a missing value, a
//! non-DWORD, or a denied key keeps the app default — a theme is
//! never guessed, the dialog-seam precedent).

use oppa::{SystemThemeSource, ThemeMode};
use windows::Win32::Foundation::{ERROR_SUCCESS, LPARAM};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
    REG_VALUE_TYPE,
};
use windows_core::PCWSTR;

use super::save_dialog::wide_nul;

/// Registry path holding the apps theme flag.
const PERSONALIZE_SUBKEY: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";
/// Value name: `0` = dark, nonzero = light.
const APPS_LIGHT_VALUE: &str = "AppsUseLightTheme";

/// Maps a registry DWORD to a mode (pure — headless-tested):
/// `0` reads dark, any other value reads light, a missing value
/// reads unknown (`None` — never a guessed mode).
pub fn theme_mode_from_registry_dword(value: Option<u32>) -> Option<ThemeMode> {
    match value {
        None => None,
        Some(0) => Some(ThemeMode::Dark),
        Some(_) => Some(ThemeMode::Light),
    }
}

/// Reads the raw `AppsUseLightTheme` DWORD, if present and shaped
/// right (any registry failure — missing key, denied read, wrong
/// type — is `None`, never a loud error: unreadable means unknown).
pub fn read_apps_use_light_theme() -> Option<u32> {
    let subkey = wide_nul(PERSONALIZE_SUBKEY);
    let name = wide_nul(APPS_LIGHT_VALUE);
    unsafe {
        let mut key = HKEY::default();
        let open = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(subkey.as_ptr()),
            None,
            KEY_READ,
            &mut key,
        );
        if open != ERROR_SUCCESS {
            return None;
        }
        let mut kind = REG_VALUE_TYPE::default();
        let mut data: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        let queried = RegQueryValueExW(
            key,
            PCWSTR::from_raw(name.as_ptr()),
            None,
            Some(&mut kind),
            Some(&mut data as *mut u32 as *mut u8),
            Some(&mut len),
        );
        let _ = RegCloseKey(key);
        if queried != ERROR_SUCCESS || kind != REG_DWORD {
            return None;
        }
        Some(data)
    }
}

/// True when a `WM_SETTINGCHANGE` `lParam` names the
/// `"ImmersiveColorSet"` section (pure over the pointer —
/// headless-tested by pointing at local wide strings; null reads
/// false, never a fault).
///
/// # Safety
///
/// `lparam` must be null or point to a valid NUL-terminated UTF-16
/// section name for the duration of the call (the wndproc contract
/// — the OS owns that string through dispatch).
pub unsafe fn is_immersive_color_set(lparam: LPARAM) -> bool {
    if lparam.0 == 0 {
        return false;
    }
    let mut len = 0usize;
    let ptr = lparam.0 as *const u16;
    // Section names are short — bound the scan (a missing NUL
    // refuses instead of running the page).
    while len < 256 {
        if unsafe { *ptr.add(len) } == 0 {
            break;
        }
        len += 1;
    }
    if len >= 256 {
        return false;
    }
    let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len) });
    text == "ImmersiveColorSet"
}

/// Current OS theme from the registry (`None` when unknown —
/// headless/test shells and denied keys keep the app default).
pub fn system_theme() -> Option<ThemeMode> {
    theme_mode_from_registry_dword(read_apps_use_light_theme())
}

/// Stateless registry reader (the runner installs one instance;
/// every query re-reads — no cached staleness by construction).
pub struct Win32SystemTheme;

impl SystemThemeSource for Win32SystemTheme {
    fn system_theme(&mut self) -> Option<ThemeMode> {
        system_theme()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_dwords_map_to_modes() {
        assert_eq!(theme_mode_from_registry_dword(None), None);
        assert_eq!(
            theme_mode_from_registry_dword(Some(0)),
            Some(ThemeMode::Dark)
        );
        assert_eq!(
            theme_mode_from_registry_dword(Some(1)),
            Some(ThemeMode::Light)
        );
        assert_eq!(
            theme_mode_from_registry_dword(Some(0xFFFF_FFFF)),
            Some(ThemeMode::Light),
            "any nonzero reads light"
        );
    }

    #[test]
    fn immersive_section_matches_by_pointer() {
        let section = wide_nul("ImmersiveColorSet");
        assert!(unsafe { is_immersive_color_set(LPARAM(section.as_ptr() as isize)) });
        let other = wide_nul("WindowsThemeElement");
        assert!(!unsafe { is_immersive_color_set(LPARAM(other.as_ptr() as isize)) });
        assert!(
            !unsafe { is_immersive_color_set(LPARAM(0)) },
            "null reads false"
        );
        let empty = wide_nul("");
        assert!(!unsafe { is_immersive_color_set(LPARAM(empty.as_ptr() as isize)) });
    }

    #[test]
    fn live_registry_read_never_panics() {
        // Whatever this box reports (dark / light / unknown) must
        // arrive without panic — the query is advisory, not a gate.
        let _ = read_apps_use_light_theme();
        let _ = system_theme();
    }
}
