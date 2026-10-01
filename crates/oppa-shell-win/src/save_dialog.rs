//! Win32 save + folder dialogs (Round 16.1, decision 314): COM
//! `IFileSaveDialog` (Explorer-style save, modal) and
//! `IFileOpenDialog` with `FOS_PICKFOLDERS` (folder pick), behind
//! the blocking [`SaveFileDialog`](oppa::SaveFileDialog) /
//! [`FolderDialog`](oppa::FolderDialog) seams.
//!
//! Blocking by OS design (the modal loop runs inside `Show` —
//! standard desktop behavior, documented, never a surprise hang):
//! both methods settle before returning. Dismissal settles
//! `Ok(None)` (the cancel HRESULT — decision-230 dismissal-is-data,
//! never a failure); real failures are loud `Backend`s. Requires
//! STA COM on the calling thread (the `run_windows` runner
//! `CoInitializeEx`s it; a bare thread gets a loud `Backend`
//! naming the missing apartment, never a silent hang).

use std::path::{Path, PathBuf};

use oppa::{
    FileDialogOptions, FileFilter, FolderDialog, FolderDialogOptions, PickError, SaveFileDialog,
};
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, IBindCtx, CLSCTX_ALL};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, IFileDialog, IFileOpenDialog, IFileSaveDialog, IShellItem,
    SHCreateItemFromParsingName, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};
use windows_core::{Interface, HRESULT, PCWSTR};

/// `ERROR_CANCELLED` as the `HRESULT` a dismissed dialog reports
/// (pure — headless-tested through [`is_dismissal`]).
fn cancel_hr() -> HRESULT {
    HRESULT::from_win32(ERROR_CANCELLED.0)
}

/// True when `e` is user dismissal (pure — headless-tested).
pub fn is_dismissal(e: &windows_core::Error) -> bool {
    e.code() == cancel_hr()
}

/// NUL-terminated UTF-16 for COM string params (pure —
/// headless-tested).
pub fn wide_nul(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Builds COM filter specs from framework filters: owned
/// `(name, spec)` wide pairs (`spec` joins patterns with `;` —
/// the `COMDLG_FILTERSPEC` shape, borrowed by the call below while
/// these owners live). Empty filters select `*.*` (the
/// commdlg-fallback precedent). Pure (headless-tested).
pub fn build_filter_specs(filters: &[FileFilter]) -> Vec<(Vec<u16>, Vec<u16>)> {
    if filters.is_empty() {
        return vec![(wide_nul("All files"), wide_nul("*.*"))];
    }
    filters
        .iter()
        .map(|f| (wide_nul(&f.name), wide_nul(&f.patterns.join(";"))))
        .collect()
}

/// Default save extension from the first glob pattern (`"*.png"`
/// → `"png"`; `"*.*"`, non-globs, and empty lists → `None`, so the
/// dialog keeps the typed name verbatim instead of appending junk).
/// Pure (headless-tested).
pub fn default_ext_from_filters(filters: &[FileFilter]) -> Option<String> {
    let pattern = filters.first()?.patterns.first()?;
    let ext = pattern.strip_prefix("*.")?;
    if ext.is_empty() || ext.contains(['*', '?']) {
        return None;
    }
    Some(ext.to_string())
}

/// `IShellItem` for a default folder (best-effort — the options
/// `initial_dir` is a hint shells may ignore, so creation failure
/// here falls back to the dialog default instead of refusing).
fn default_folder_item(dir: &Path) -> Result<IShellItem, windows_core::Error> {
    let wide = wide_nul(&dir.to_string_lossy());
    unsafe {
        SHCreateItemFromParsingName::<PCWSTR, Option<&IBindCtx>, IShellItem>(
            PCWSTR::from_raw(wide.as_ptr()),
            None,
        )
    }
}

/// Local path behind an `IShellItem` (`SIGDN_FILESYSPATH` —
/// filesystem paths only, never virtual items). Frees the COM
/// string (leak-free by rule).
fn item_display_path(item: &IShellItem) -> Result<PathBuf, PickError> {
    let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }
        .map_err(|e| PickError::Backend(format!("dialog result display name: {e:?}")))?;
    let path = String::from_utf16_lossy(unsafe { name.as_wide() });
    unsafe { CoTaskMemFree(Some(name.as_ptr() as *const _)) };
    Ok(PathBuf::from(path))
}

/// Null hwnd renders as no owner (headless/default shells carry
/// none — `Show` still runs modal to the thread).
fn owner_or_none(owner: HWND) -> Option<HWND> {
    if owner.is_invalid() {
        None
    } else {
        Some(owner)
    }
}

/// Win32 save-dialog backend (UI-thread use — modal on the caller).
pub struct Win32SaveDialog {
    owner: HWND,
}

impl Win32SaveDialog {
    pub fn new(owner: HWND) -> Self {
        Self { owner }
    }

    fn run_modal(&self, options: &FileDialogOptions) -> Result<Option<PathBuf>, PickError> {
        let dialog: IFileSaveDialog =
            unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL) }.map_err(|e| {
                PickError::Backend(format!("save dialog CoCreateInstance (STA COM up?): {e:?}"))
            })?;
        // Owners outlive the call (specs borrow them through `Show`).
        let pairs = build_filter_specs(&options.filters);
        let specs: Vec<COMDLG_FILTERSPEC> = pairs
            .iter()
            .map(|(name, spec)| COMDLG_FILTERSPEC {
                pszName: PCWSTR::from_raw(name.as_ptr()),
                pszSpec: PCWSTR::from_raw(spec.as_ptr()),
            })
            .collect();
        let title = wide_nul(&options.title);
        let name = wide_nul(&options.default_name);
        let ext = default_ext_from_filters(&options.filters).map(|e| wide_nul(&e));
        // `SetFileTypes`/`SetTitle`/folder/`Show`/result ride the
        // `IFileDialog` base (queried once — one QI, never per call).
        let base: IFileDialog = dialog
            .cast()
            .map_err(|e| PickError::Backend(format!("save dialog IFileDialog QI: {e:?}")))?;
        unsafe {
            base.SetFileTypes(&specs)
                .map_err(|e| PickError::Backend(format!("save dialog SetFileTypes: {e:?}")))?;
            if title.len() > 1 {
                base.SetTitle(PCWSTR::from_raw(title.as_ptr()))
                    .map_err(|e| PickError::Backend(format!("save dialog SetTitle: {e:?}")))?;
            }
            if name.len() > 1 {
                dialog
                    .SetFileName(PCWSTR::from_raw(name.as_ptr()))
                    .map_err(|e| PickError::Backend(format!("save dialog SetFileName: {e:?}")))?;
            }
            if let Some(ext) = &ext {
                dialog
                    .SetDefaultExtension(PCWSTR::from_raw(ext.as_ptr()))
                    .map_err(|e| {
                        PickError::Backend(format!("save dialog SetDefaultExtension: {e:?}"))
                    })?;
            }
            if let Some(dir) = &options.initial_dir {
                if let Ok(item) = default_folder_item(dir) {
                    let _ = base.SetDefaultFolder(&item);
                }
            }
            match base.Show(owner_or_none(self.owner)) {
                Ok(()) => {}
                Err(e) if is_dismissal(&e) => return Ok(None),
                Err(e) => {
                    return Err(PickError::Backend(format!("save dialog Show: {e:?}")));
                }
            }
            let item = base
                .GetResult()
                .map_err(|e| PickError::Backend(format!("save dialog GetResult: {e:?}")))?;
            Ok(Some(item_display_path(&item)?))
        }
    }
}

impl Default for Win32SaveDialog {
    fn default() -> Self {
        Self::new(HWND::default())
    }
}

impl SaveFileDialog for Win32SaveDialog {
    fn save(&mut self, options: FileDialogOptions) -> Result<Option<PathBuf>, PickError> {
        // Modal: settles before returning (documented blocking).
        self.run_modal(&options)
    }
}

/// Win32 folder-picker backend (UI-thread use — modal on the caller).
pub struct Win32FolderDialog {
    owner: HWND,
}

impl Win32FolderDialog {
    pub fn new(owner: HWND) -> Self {
        Self { owner }
    }

    fn run_modal(&self, options: &FolderDialogOptions) -> Result<Option<PathBuf>, PickError> {
        let dialog: IFileOpenDialog =
            unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL) }.map_err(|e| {
                PickError::Backend(format!(
                    "folder dialog CoCreateInstance (STA COM up?): {e:?}"
                ))
            })?;
        let title = wide_nul(&options.title);
        // Base `IFileDialog` carries options/title/folder/show/result
        // (queried once); the open dialog itself only picks folders.
        let base: IFileDialog = dialog
            .cast()
            .map_err(|e| PickError::Backend(format!("folder dialog IFileDialog QI: {e:?}")))?;
        unsafe {
            base.SetOptions(FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
                .map_err(|e| PickError::Backend(format!("folder dialog SetOptions: {e:?}")))?;
            if title.len() > 1 {
                base.SetTitle(PCWSTR::from_raw(title.as_ptr()))
                    .map_err(|e| PickError::Backend(format!("folder dialog SetTitle: {e:?}")))?;
            }
            if let Some(dir) = &options.initial_dir {
                if let Ok(item) = default_folder_item(dir) {
                    let _ = base.SetDefaultFolder(&item);
                }
            }
            match base.Show(owner_or_none(self.owner)) {
                Ok(()) => {}
                Err(e) if is_dismissal(&e) => return Ok(None),
                Err(e) => {
                    return Err(PickError::Backend(format!("folder dialog Show: {e:?}")));
                }
            }
            let item = base
                .GetResult()
                .map_err(|e| PickError::Backend(format!("folder dialog GetResult: {e:?}")))?;
            Ok(Some(item_display_path(&item)?))
        }
    }
}

impl Default for Win32FolderDialog {
    fn default() -> Self {
        Self::new(HWND::default())
    }
}

impl FolderDialog for Win32FolderDialog {
    fn pick(&mut self, options: FolderDialogOptions) -> Result<Option<PathBuf>, PickError> {
        // Modal: settles before returning (documented blocking).
        self.run_modal(&options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_specs_carry_name_and_joined_patterns() {
        let specs = build_filter_specs(&[
            FileFilter {
                name: "PNG images".to_string(),
                patterns: vec!["*.png".to_string(), "*.PNG".to_string()],
            },
            FileFilter {
                name: "All files".to_string(),
                patterns: vec!["*.*".to_string()],
            },
        ]);
        let text: Vec<(String, String)> = specs
            .iter()
            .map(|(n, s)| {
                (
                    String::from_utf16_lossy(&n[..n.len() - 1]),
                    String::from_utf16_lossy(&s[..s.len() - 1]),
                )
            })
            .collect();
        assert_eq!(
            text,
            vec![
                ("PNG images".to_string(), "*.png;*.PNG".to_string()),
                ("All files".to_string(), "*.*".to_string()),
            ]
        );
        // Empty filters fall back to *.* (never an empty type box).
        let fallback = build_filter_specs(&[]);
        assert_eq!(fallback.len(), 1);
        assert_eq!(
            String::from_utf16_lossy(&fallback[0].1[..fallback[0].1.len() - 1]),
            "*.*"
        );
    }

    #[test]
    fn default_ext_comes_from_the_first_glob() {
        let png = |p: &str| FileFilter {
            name: "x".to_string(),
            patterns: vec![p.to_string()],
        };
        assert_eq!(
            default_ext_from_filters(&[png("*.png")]),
            Some("png".to_string())
        );
        assert_eq!(default_ext_from_filters(&[png("*.*")]), None);
        assert_eq!(default_ext_from_filters(&[]), None);
        assert_eq!(default_ext_from_filters(&[png("notes*")]), None);
        assert_eq!(default_ext_from_filters(&[png("*.")]), None);
        // First filter wins (the dialog's selected-type rule).
        assert_eq!(
            default_ext_from_filters(&[png("*.txt"), png("*.md")]),
            Some("txt".to_string())
        );
    }

    #[test]
    fn cancel_maps_to_dismissal_nothing_else_does() {
        let cancel = windows_core::Error::new(cancel_hr(), "");
        assert!(is_dismissal(&cancel), "ERROR_CANCELLED dismisses");
        let other = windows_core::Error::new(HRESULT::from_win32(2), "");
        assert!(!is_dismissal(&other), "real failures stay loud");
    }

    #[test]
    fn wide_strings_carry_the_nul() {
        assert_eq!(wide_nul("ab"), vec![97, 98, 0]);
        assert_eq!(wide_nul(""), vec![0]);
    }
}
