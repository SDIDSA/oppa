//! File-picker seam (G12 — decisions 230–231, file picker only;
//! Round 16.1, decision 314, adds save + folder pickers).
//!
//! Desktop integration beyond this file does not exist yet
//! (multi-window, menus, drag-and-drop, tray — each a named
//! OQ-G12-x, not silent gaps). The picker leads because it pairs
//! with the G5 stores (picked paths feed `FsSandbox`) and G8 images
//! (picked bytes feed `decode_image`).
//!
//! Design (see decisions 230–231; rationale in git history):
//!
//! - **Request/poll completion (230).** `request_open` shows the
//!   dialog (or queues it); `poll_open` returns `None` while it is
//!   still open (async shells — web picker promises, portals) and
//!   `Some` once settled. Blocking shells (Win32 modal) settle
//!   before `request_open` returns — the modal loop is standard OS
//!   behavior, documented, never a surprise hang. Same shape as the
//!   G3 clipboard reads (decision 209), applied to dialogs.
//! - **Dismissal is data.** `Ok(vec![])` = dismissed (query outcome,
//!   like the clipboard's empty-`Ok(None)`); `Err` is refusal/
//!   failure only. Callers never guess. Save/folder pickers settle
//!   `Ok(None)` on dismissal (decision 314 — one path, not a vec).
//! - **Level-triggered last result.** Polling with no outstanding
//!   request re-returns the last settled result (the `InputEvent::
//!   Text` self-heal precedent — a drop self-heals on the next
//!   poll).
//! - **Shell seam (231).**
//!   [`PlatformShell::file_dialog`](crate::shell::PlatformShell::file_dialog)
//!   returns `None` by default (pre-G12 shells compile untouched,
//!   refuse loudly through `Unsupported`); Win32 wires the real
//!   `GetOpenFileNameW` dialog this round.
//! - **Blocking save/folder (314).** [`SaveFileDialog::save`] and
//!   [`FolderDialog::pick`] block the caller until the user picks
//!   or dismisses (native modal on Windows; a bounded poll loop
//!   over request/poll on portal/zenity Linux). Callers that must
//!   not block keep the request/poll shape — the blocking pair is
//!   a convenience, never the only path.
//!
//! Out of scope: Linux/Android/Web save backends beyond the portal
//! and zenity shapes here (OQ-G12-1..3 follow-ups);
//! drag-and-drop-into-fields (needs the G11 pointer model extended
//! to OS drops — OQ-G12-4).

use std::collections::VecDeque;
use std::fmt;
use std::path::PathBuf;

/// Picker failure (loud by construction).
#[derive(Clone, Debug, PartialEq)]
pub enum PickError {
    /// This shell has no picker backend yet (default seam state).
    Unsupported(&'static str),
    /// The OS call failed; the string is the backend's message.
    Backend(String),
}

impl fmt::Display for PickError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PickError::Unsupported(who) => {
                write!(
                    f,
                    "{who} has no file-picker backend — refusal, never silent"
                )
            }
            PickError::Backend(msg) => write!(f, "file picker failed: {msg}"),
        }
    }
}

impl std::error::Error for PickError {}

/// One filter entry: display name + semicolon-separated patterns
/// (`("PNG images", "*.png")`, `("All files", "*.*")`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FileFilter {
    pub name: String,
    pub patterns: Vec<String>,
}

/// Open-dialog options (plain data — shells translate).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FilePickerOptions {
    pub title: String,
    pub filters: Vec<FileFilter>,
    /// Allow multi-select (results carry every path).
    pub multiple: bool,
    /// Suggested start directory (hint — shells may ignore).
    pub initial_dir: Option<PathBuf>,
}

/// Save-dialog options (Round 16.1, decision 314 — plain data,
/// shells translate; the `FilePickerOptions` twin for the save
/// path, minus multi-select — saves pick exactly one destination).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FileDialogOptions {
    pub title: String,
    pub filters: Vec<FileFilter>,
    /// Suggested file name (`"report.txt"` — shells prefill it).
    pub default_name: String,
    /// Suggested start directory (hint — shells may ignore).
    pub initial_dir: Option<PathBuf>,
}

/// Folder-picker options (Round 16.1, decision 314 — plain data,
/// shells translate; always single-select, stated).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FolderDialogOptions {
    pub title: String,
    /// Suggested start directory (hint — shells may ignore).
    pub initial_dir: Option<PathBuf>,
}

/// File-open dialog backend (decision 230). `!Send` like every
/// UI-thread type (backends call thread-affine OS APIs).
pub trait FileDialog {
    /// Shows (or queues) the dialog for `options`, replacing any
    /// outstanding request (a second request supersedes — stated,
    /// never queued silently).
    fn request_open(&mut self, options: FilePickerOptions);

    /// Polls the request: `None` = still open (async shells only);
    /// `Some(Ok(paths))` = settled (`[]` = dismissed);
    /// `Some(Err(e))` = refused/failed loudly.
    fn poll_open(&mut self) -> Option<Result<Vec<PathBuf>, PickError>>;
}

/// Headless/test dialog: scripted responses in order (each
/// `request_open` arms the next; exhausted scripts settle empty =
/// dismissed). Polls re-return the last result (level-triggered).
#[derive(Debug, Default)]
pub struct ScriptedDialog {
    script: VecDeque<Vec<PathBuf>>,
    last: Vec<PathBuf>,
    outstanding: bool,
}

impl ScriptedDialog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues one response per upcoming request.
    pub fn push_response(&mut self, paths: Vec<PathBuf>) {
        self.script.push_back(paths);
    }
}

impl FileDialog for ScriptedDialog {
    fn request_open(&mut self, _options: FilePickerOptions) {
        self.last = self.script.pop_front().unwrap_or_default();
        self.outstanding = true;
    }

    fn poll_open(&mut self) -> Option<Result<Vec<PathBuf>, PickError>> {
        self.outstanding = false;
        Some(Ok(self.last.clone()))
    }
}

/// Blocking save-dialog backend (Round 16.1, decision 314).
/// `!Send` like every UI-thread type (backends call thread-affine
/// OS APIs). Dismissal settles `Ok(None)` (decision-230
/// dismissal-is-data, one path); `Err` is refusal/failure only.
pub trait SaveFileDialog {
    /// Shows the dialog and blocks until the user picks or
    /// dismisses (native modal on Windows; a bounded poll loop on
    /// portal/zenity Linux — documented per backend, never a
    /// surprise hang).
    fn save(&mut self, options: FileDialogOptions) -> Result<Option<PathBuf>, PickError>;
}

/// Blocking folder-picker backend (Round 16.1, decision 314 —
/// same blocking contract as [`SaveFileDialog`]; always one
/// directory, never multi-select).
pub trait FolderDialog {
    /// Shows the picker and blocks until the user picks or
    /// dismisses.
    fn pick(&mut self, options: FolderDialogOptions) -> Result<Option<PathBuf>, PickError>;
}

/// Headless/test save dialog: scripted responses in order
/// (exhausted scripts settle dismissed = `Ok(None)`), recording
/// every options value so tests prove filters, default names, and
/// start dirs reached the backend.
#[derive(Debug, Default)]
pub struct ScriptedSaveDialog {
    script: VecDeque<Option<PathBuf>>,
    last_options: Vec<FileDialogOptions>,
}

impl ScriptedSaveDialog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues one response per upcoming save (`None` = dismissed).
    pub fn push_response(&mut self, path: Option<PathBuf>) {
        self.script.push_back(path);
    }

    /// Options values received so far, in order.
    pub fn seen_options(&self) -> &[FileDialogOptions] {
        &self.last_options
    }
}

impl SaveFileDialog for ScriptedSaveDialog {
    fn save(&mut self, options: FileDialogOptions) -> Result<Option<PathBuf>, PickError> {
        self.last_options.push(options);
        Ok(self.script.pop_front().unwrap_or(None))
    }
}

/// Headless/test folder picker: scripted responses in order
/// (exhausted scripts settle dismissed = `Ok(None)`), recording
/// every options value so tests prove titles and start dirs
/// reached the backend.
#[derive(Debug, Default)]
pub struct ScriptedFolderDialog {
    script: VecDeque<Option<PathBuf>>,
    last_options: Vec<FolderDialogOptions>,
}

impl ScriptedFolderDialog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues one response per upcoming pick (`None` = dismissed).
    pub fn push_response(&mut self, path: Option<PathBuf>) {
        self.script.push_back(path);
    }

    /// Options values received so far, in order.
    pub fn seen_options(&self) -> &[FolderDialogOptions] {
        &self.last_options
    }
}

impl FolderDialog for ScriptedFolderDialog {
    fn pick(&mut self, options: FolderDialogOptions) -> Result<Option<PathBuf>, PickError> {
        self.last_options.push(options);
        Ok(self.script.pop_front().unwrap_or(None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::PlatformShell;

    #[test]
    fn scripted_round_trip_and_dismissal() {
        let mut d = ScriptedDialog::new();
        d.push_response(vec![PathBuf::from("/tmp/a.png")]);
        d.request_open(FilePickerOptions::default());
        assert_eq!(d.poll_open(), Some(Ok(vec![PathBuf::from("/tmp/a.png")])));
        // Exhausted script = dismissed (empty, not an error).
        d.request_open(FilePickerOptions::default());
        assert_eq!(d.poll_open(), Some(Ok(vec![])));
        // Level-triggered: re-poll repeats the last result.
        assert_eq!(d.poll_open(), Some(Ok(vec![])));
    }

    /// Round 16.1 (decision 314): scripted save/folder doubles
    /// settle scripted picks, dismiss on exhaustion, and record the
    /// options that reached them (filters, default names, dirs).
    #[test]
    fn scripted_save_and_folder_record_options() {
        let mut d = ScriptedSaveDialog::new();
        d.push_response(Some(PathBuf::from("/tmp/report.txt")));
        let options = FileDialogOptions {
            title: "Save".to_string(),
            filters: vec![FileFilter {
                name: "Text".to_string(),
                patterns: vec!["*.txt".to_string()],
            }],
            default_name: "report.txt".to_string(),
            initial_dir: Some(PathBuf::from("/tmp")),
        };
        assert_eq!(
            d.save(options.clone()).expect("saves"),
            Some(PathBuf::from("/tmp/report.txt"))
        );
        assert_eq!(d.seen_options(), &[options]);
        // Exhausted script = dismissed (None, not an error).
        assert_eq!(
            d.save(FileDialogOptions::default()).expect("dismisses"),
            None
        );

        let mut f = ScriptedFolderDialog::new();
        f.push_response(Some(PathBuf::from("/tmp/out")));
        let foptions = FolderDialogOptions {
            title: "Pick".to_string(),
            initial_dir: Some(PathBuf::from("/tmp")),
        };
        assert_eq!(
            f.pick(foptions.clone()).expect("picks"),
            Some(PathBuf::from("/tmp/out"))
        );
        assert_eq!(f.seen_options(), &[foptions]);
        assert_eq!(
            f.pick(FolderDialogOptions::default()).expect("dismisses"),
            None
        );
    }

    /// The additive seam holds: shells that predate G12 compile
    /// untouched and refuse loudly through `None`.
    struct AncientShell;

    impl PlatformShell for AncientShell {
        fn pump_events(&mut self) -> Vec<crate::shell::Event> {
            Vec::new()
        }
    }

    #[test]
    fn default_seam_refuses_loudly() {
        let mut shell = AncientShell;
        assert!(
            shell.file_dialog().is_none(),
            "pre-G12 shells get None by default"
        );
    }
}
