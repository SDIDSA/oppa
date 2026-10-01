//! Win32 file-open dialog (G12, decision 231): `GetOpenFileNameW`
//! (Explorer-style, modal) behind the [`FileDialog`](oppa::FileDialog)
//! request/poll seam.
//!
//! Blocking by OS design (the modal loop runs inside
//! `GetOpenFileNameW` — standard desktop behavior, documented, never
//! a surprise hang): `request_open` settles before it returns, and
//! polls re-return the last result (level-triggered, like the
//! scripted double). Dismissal settles empty (`Ok(vec![])`); real
//! failures surface through `CommDlgExtendedError` (a bare FALSE
//! with code 0 is the dismiss, never a silent error).

use std::path::PathBuf;

use oppa::{FileDialog, FileFilter, FilePickerOptions, PickError};
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, OFN_ALLOWMULTISELECT, OFN_DONTADDTORECENT,
    OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_core::{PCWSTR, PWSTR};

/// 64K u16 file buffer (multi-select needs room — Explorer packs
/// dir + N files double-NUL-terminated here).
const FILE_BUFFER_LEN: usize = 65536;

/// Win32 open-dialog backend (UI-thread use — modal on the caller).
pub struct Win32FileDialog {
    owner: HWND,
    last: Result<Vec<PathBuf>, PickError>,
}

impl Win32FileDialog {
    pub fn new(owner: HWND) -> Self {
        Self {
            owner,
            last: Ok(Vec::new()),
        }
    }

    fn run_modal(&self, options: &FilePickerOptions) -> Result<Vec<PathBuf>, PickError> {
        let filter = build_filter_string(&options.filters);
        let title_nul: Vec<u16> = options.title.encode_utf16().chain([0]).collect();
        let initial_nul: Vec<u16> = options
            .initial_dir
            .as_ref()
            .map(|p| p.as_os_str().encode_wide().chain([0]).collect())
            .unwrap_or_default();
        let mut file_buf = vec![0u16; FILE_BUFFER_LEN];
        let mut ofn = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: self.owner,
            lpstrFilter: PCWSTR::from_raw(filter.as_ptr()),
            lpstrFile: PWSTR::from_raw(file_buf.as_mut_ptr()),
            nMaxFile: FILE_BUFFER_LEN as u32,
            lpstrTitle: if title_nul.len() <= 1 {
                PCWSTR::null()
            } else {
                PCWSTR::from_raw(title_nul.as_ptr())
            },
            lpstrInitialDir: if initial_nul.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR::from_raw(initial_nul.as_ptr())
            },
            nFilterIndex: 1,
            Flags: OFN_EXPLORER
                | OFN_FILEMUSTEXIST
                | OFN_PATHMUSTEXIST
                | OFN_DONTADDTORECENT
                | if options.multiple {
                    OFN_ALLOWMULTISELECT
                } else {
                    OFN_EXPLORER
                },
            ..Default::default()
        };
        let ok = unsafe { GetOpenFileNameW(&mut ofn) };
        if ok.as_bool() {
            Ok(parse_dialog_result(&file_buf))
        } else {
            let code = unsafe { CommDlgExtendedError() };
            if code.0 == 0 {
                Ok(Vec::new())
            } else {
                Err(PickError::Backend(format!(
                    "GetOpenFileNameW failed (code {})",
                    code.0
                )))
            }
        }
    }
}

/// Builds the Win32 double-NUL filter string (`"desc\0*.png\0…\0\0"`;
/// empty filters select `*.*`). Pure (headless-tested).
pub fn build_filter_string(filters: &[FileFilter]) -> Vec<u16> {
    fn push(out: &mut Vec<u16>, s: &str) {
        out.extend(s.encode_utf16());
        out.push(0);
    }
    let mut out = Vec::new();
    if filters.is_empty() {
        push(&mut out, "All files");
        push(&mut out, "*.*");
    } else {
        for f in filters {
            push(&mut out, &f.name);
            push(&mut out, &f.patterns.join(";"));
        }
    }
    out.push(0);
    out
}

/// Parses the Explorer result buffer: one NUL-terminated full path,
/// or dir + N files double-NUL-terminated. Pure (headless-tested).
pub fn parse_dialog_result(buf: &[u16]) -> Vec<PathBuf> {
    let mut parts: Vec<String> = Vec::new();
    for chunk in buf.split(|c| *c == 0) {
        if chunk.is_empty() {
            break;
        }
        parts.push(String::from_utf16_lossy(chunk));
    }
    if parts.is_empty() {
        return Vec::new();
    }
    if parts.len() == 1 {
        return vec![PathBuf::from(&parts[0])];
    }
    let dir = PathBuf::from(&parts[0]);
    parts[1..].iter().map(|f| dir.join(f)).collect()
}

impl Default for Win32FileDialog {
    fn default() -> Self {
        Self::new(HWND::default())
    }
}

impl FileDialog for Win32FileDialog {
    fn request_open(&mut self, options: FilePickerOptions) {
        // Modal: settles before returning (documented blocking).
        // Failures keep their identity into the poll (a Backend error
        // never degrades into a dismissal-empty).
        self.last = self.run_modal(&options);
    }

    fn poll_open(&mut self) -> Option<Result<Vec<PathBuf>, PickError>> {
        Some(self.last.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_string_is_double_nul_terminated() {
        let filters = vec![
            FileFilter {
                name: "PNG images".to_string(),
                patterns: vec!["*.png".to_string()],
            },
            FileFilter {
                name: "All files".to_string(),
                patterns: vec!["*.*".to_string()],
            },
        ];
        let wide = build_filter_string(&filters);
        let text = String::from_utf16_lossy(&wide);
        let chars: Vec<char> = text.chars().collect();
        assert_eq!(
            chars,
            "PNG images\0*.png\0All files\0*.*\0\0"
                .chars()
                .collect::<Vec<char>>()
        );
        // Empty filters fall back to *.* (never an empty filter box).
        let fallback = build_filter_string(&[]);
        assert_eq!(String::from_utf16_lossy(&fallback), "All files\0*.*\0\0");
    }

    #[test]
    fn result_parses_single_and_multi() {
        let single: Vec<u16> = "C:\\tmp\\a.png\0\0".encode_utf16().collect();
        assert_eq!(
            parse_dialog_result(&single),
            vec![PathBuf::from("C:\\tmp\\a.png")]
        );
        let multi: Vec<u16> = "C:\\tmp\0a.png\0b.png\0\0".encode_utf16().collect();
        assert_eq!(
            parse_dialog_result(&multi),
            vec![
                PathBuf::from("C:\\tmp\\a.png"),
                PathBuf::from("C:\\tmp\\b.png"),
            ]
        );
        assert!(parse_dialog_result(&[0, 0]).is_empty());
    }
}
