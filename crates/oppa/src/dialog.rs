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
//! and zenity shapes here (OQ-G12-1..3 follow-ups — decision 423 adds
//! the bytes open seam [`FileOpener`]/[`PickedFile`] for SAF intents
//! and Web pickers; save-bytes stays a follow-up);
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

/// App-facing pick backend (decision 429 — the unification): any
/// [`FileDialog`] serves path results, any [`FileOpener`] serves
/// byte results, through one method pair. Blanket-implemented for
/// both backend traits (including behind `dyn`), so runners keep
/// owning bare backends while app code drives sessions.
pub trait PickBackend<R> {
    fn pick_request(&mut self, options: FilePickerOptions);
    fn pick_poll(&mut self) -> Option<Result<R, PickError>>;
}

impl<T: FileDialog + ?Sized> PickBackend<Vec<PathBuf>> for T {
    fn pick_request(&mut self, options: FilePickerOptions) {
        FileDialog::request_open(self, options);
    }

    fn pick_poll(&mut self) -> Option<Result<Vec<PathBuf>, PickError>> {
        FileDialog::poll_open(self)
    }
}

impl<T: FileOpener + ?Sized> PickBackend<Vec<PickedFile>> for T {
    fn pick_request(&mut self, options: FilePickerOptions) {
        FileOpener::request_bytes(self, options);
    }

    fn pick_poll(&mut self) -> Option<Result<Vec<PickedFile>, PickError>> {
        FileOpener::poll_bytes(self)
    }
}

/// One app tick against any pick backend (decision 429):
/// poll-first, request-if-idle, consume-once. A press first drains
/// any settlement (never requesting over settled data — the old
/// request-then-poll order discarded an async settlement before
/// reading it, so the path caption never left `(none)`), requests
/// only when idle, and consumes each settlement exactly once (the
/// backend's level-triggered echo is never surfaced twice — the
/// session only surfaces polls made while its own request is
/// outstanding). Blocking backends settle inside the same press;
/// async backends settle across presses — one call shape, one
/// observable sequence, every platform.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PickSession {
    outstanding: bool,
}

/// Outcome of one [`PickSession::press`]: either the dialog is still
/// open, or one settlement, consumed.
#[derive(Clone, Debug, PartialEq)]
pub enum PickPress<R> {
    StillOpen,
    Settled(Result<R, PickError>),
}

impl PickSession {
    pub fn new() -> Self {
        Self::default()
    }

    /// True while a request fired through this session awaits its
    /// settlement (observability for spinners/tests — never a second
    /// request channel).
    pub fn is_outstanding(&self) -> bool {
        self.outstanding
    }

    /// Drain-only tick (decision 431 — the auto-update half): if a
    /// request fired through this session is still outstanding,
    /// polls the backend once and consumes at most one settlement;
    /// otherwise returns `None` without touching the backend (never
    /// requests — safe to call every loop iteration). App loops tick
    /// this so an async settlement surfaces without another press;
    /// user presses still go through [`PickSession::press`].
    pub fn poll<R, B: PickBackend<R> + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Option<Result<R, PickError>> {
        if !self.outstanding {
            return None;
        }
        match backend.pick_poll() {
            None => None,
            Some(result) => {
                self.outstanding = false;
                Some(result)
            }
        }
    }

    /// Runs one app tick: returns a settlement awaiting consumption
    /// without requesting; otherwise fires `options` and returns
    /// whatever settled synchronously (`StillOpen` while async-open).
    /// A second tick while open re-polls (never double-fires — one
    /// dialog per press series, every platform).
    pub fn press<R, B: PickBackend<R> + ?Sized>(
        &mut self,
        backend: &mut B,
        options: FilePickerOptions,
    ) -> PickPress<R> {
        if !self.outstanding {
            backend.pick_request(options);
            self.outstanding = true;
        }
        match self.poll(backend) {
            None => PickPress::StillOpen,
            Some(result) => PickPress::Settled(result),
        }
    }
}

/// Open-row outcome (decision 429 — the shared vocabulary): every
/// platform renders the same path line + status caption from one
/// [`PickPress`]. Dismissal and refusals leave the controlled path
/// untouched (dismissal is data — decision 230, never a clear).
#[derive(Clone, Debug, PartialEq)]
pub enum OpenOutcome {
    StillOpen,
    Dismissed,
    Picked { first: String, count: usize },
    Failed(PickError),
}

impl OpenOutcome {
    /// Maps one consumed press onto the row outcome (the path signal
    /// takes `path_text`, the caption takes `status`).
    pub fn of(press: PickPress<Vec<PathBuf>>) -> Self {
        match press {
            PickPress::StillOpen => OpenOutcome::StillOpen,
            PickPress::Settled(Ok(paths)) if paths.is_empty() => OpenOutcome::Dismissed,
            PickPress::Settled(Ok(paths)) => OpenOutcome::Picked {
                first: paths[0].to_string_lossy().into_owned(),
                count: paths.len(),
            },
            PickPress::Settled(Err(e)) => OpenOutcome::Failed(e),
        }
    }

    /// The picked path display text (`None` renders `(none)`; only
    /// `Picked` carries one — everything else leaves the controlled
    /// path alone).
    pub fn path_text(&self) -> Option<&str> {
        match self {
            OpenOutcome::Picked { first, .. } => Some(first),
            _ => None,
        }
    }

    /// The file name portion of a pick (decision 430 — the display
    /// line: `tapme.txt`, never the overflowing absolute path).
    /// `None` unless `Picked` with a non-empty file name.
    pub fn file_name(&self) -> Option<&str> {
        match self {
            OpenOutcome::Picked { first, .. } => std::path::Path::new(first)
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|n| !n.is_empty()),
            _ => None,
        }
    }

    /// The parent directory portion of a pick (decision 430 — the
    /// wrapping second line). `None` unless `Picked` with a
    /// non-empty parent (bare file names render one line).
    pub fn dir_name(&self) -> Option<&str> {
        match self {
            OpenOutcome::Picked { first, .. } => std::path::Path::new(first)
                .parent()
                .and_then(|d| d.to_str())
                .filter(|d| !d.is_empty()),
            _ => None,
        }
    }

    /// The status caption, byte-identical on every platform (decision
    /// 430: single picks name the file, not the absolute path — the
    /// caption stays one short line while the row below carries the
    /// wrapped directory).
    pub fn status(&self) -> String {
        match self {
            OpenOutcome::StillOpen => "still open…".to_string(),
            OpenOutcome::Dismissed => "dismissed".to_string(),
            OpenOutcome::Picked { first, count } => {
                if *count > 1 {
                    format!("picked {count} files (first shown)")
                } else {
                    format!("picked {}", self.file_name().unwrap_or(first.as_str()))
                }
            }
            OpenOutcome::Failed(e) => format!("picker failed: {e}"),
        }
    }
}
/// Per-loop pick drain (decision 431 — the auto-update half): app
/// loops tick [`PickRegistry::poll_all`] once per iteration so an
/// async settlement surfaces without another press. Each entry pairs
/// one shared [`PickSession`] with its backend plus the callback
/// that publishes the settlement (usually into component signals).
/// Entries are keyed by session+backend identity with latest-wins
/// callbacks — renders re-track every pass harmlessly (the closure
/// captures equivalent signal handles), and one backend never
/// grows two entries. `!Send` like every UI-thread type (backends
/// call thread-affine OS APIs; callbacks set component signals).
#[derive(Default)]
pub struct PickRegistry {
    entries: Vec<RegistryEntry>,
}

struct RegistryEntry {
    session: std::rc::Rc<std::cell::RefCell<PickSession>>,
    backend: std::rc::Rc<std::cell::RefCell<dyn FileDialog>>,
    on_settle: std::rc::Rc<dyn Fn(OpenOutcome)>,
}

impl PickRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tracks one open-backend drain: the session and backend are
    /// shared handles (the control and the sink drive the same
    /// pair), the callback publishes each settlement exactly once.
    /// Re-tracking the same pair replaces the callback (renders
    /// re-track freely — latest wins, never duplicates).
    pub fn track_open(
        &mut self,
        session: &std::rc::Rc<std::cell::RefCell<PickSession>>,
        backend: &std::rc::Rc<std::cell::RefCell<dyn FileDialog>>,
        on_settle: std::rc::Rc<dyn Fn(OpenOutcome)>,
    ) {
        if let Some(entry) = self.entries.iter_mut().find(|e| {
            std::rc::Rc::ptr_eq(&e.session, session) && std::rc::Rc::ptr_eq(&e.backend, backend)
        }) {
            entry.on_settle = on_settle;
            return;
        }
        self.entries.push(RegistryEntry {
            session: session.clone(),
            backend: backend.clone(),
            on_settle,
        });
    }

    /// Drains every tracked settlement once: polls each outstanding
    /// session (never requests) and publishes settlements through
    /// their callbacks. Returns the settlement count (loops treat
    /// non-zero as activity — step + present that iteration, so the
    /// fresh caption reaches the screen). Pure queries when nothing
    /// is outstanding (no backend traffic, never a dialog).
    pub fn poll_all(&mut self) -> usize {
        let mut count = 0usize;
        for entry in &mut self.entries {
            // Guards drop before the callback runs (callbacks set
            // component signals — never under backend locks).
            let outcome = {
                let mut session = entry.session.borrow_mut();
                if !session.is_outstanding() {
                    None
                } else {
                    let mut backend = entry.backend.borrow_mut();
                    session
                        .poll(&mut *backend)
                        .map(|result| OpenOutcome::of(PickPress::Settled(result)))
                }
            };
            if let Some(outcome) = outcome {
                count += 1;
                (entry.on_settle)(outcome);
            }
        }
        count
    }

    /// Tracked entry count (tests/diagnostics — never a capacity).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no entry is tracked.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

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

/// One picked file by bytes (decision 423 — the Android/Web seam):
/// `name` is the display name (file name only, never a filesystem
/// path — SAF yields `content://` URIs, browsers yield `File`
/// blobs); `bytes` are the file contents for `decode_image` /
/// `FsSandbox` consumers. Empty `bytes` with a non-empty `name`
/// is a valid empty file; dismissal is `Ok(vec![])`, never an
/// empty-name entry.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PickedFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

impl PickedFile {
    pub fn new(name: String, bytes: Vec<u8>) -> Self {
        Self { name, bytes }
    }
}

/// File-open-by-bytes backend (decision 423 — async by design for
/// Android SAF intents and Web `<input>`/File System Access
/// promises): same request/poll shape as [`FileDialog`], but results
/// carry bytes instead of filesystem paths. Blocking shells settle
/// before `request_bytes` returns; async shells return `None` from
/// `poll_bytes` while open. `!Send` like every UI-thread type.
pub trait FileOpener {
    /// Shows (or queues) the dialog for `options`, replacing any
    /// outstanding request (a second request supersedes — stated,
    /// never queued silently).
    fn request_bytes(&mut self, options: FilePickerOptions);

    /// Polls the request: `None` = still open (async shells only);
    /// `Some(Ok(files))` = settled (`[]` = dismissed);
    /// `Some(Err(e))` = refused/failed loudly. Level-triggered last
    /// result after settling (the [`FileDialog`] rule).
    fn poll_bytes(&mut self) -> Option<Result<Vec<PickedFile>, PickError>>;
}

/// Headless/test bytes dialog: scripted responses in order (each
/// `request_bytes` arms the next; exhausted scripts settle empty =
/// dismissed). Polls re-return the last result (level-triggered).
#[derive(Debug, Default)]
pub struct ScriptedOpener {
    script: VecDeque<Vec<PickedFile>>,
    last: Vec<PickedFile>,
    outstanding: bool,
}

impl ScriptedOpener {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues one response per upcoming request.
    pub fn push_response(&mut self, files: Vec<PickedFile>) {
        self.script.push_back(files);
    }
}

impl FileOpener for ScriptedOpener {
    fn request_bytes(&mut self, _options: FilePickerOptions) {
        self.last = self.script.pop_front().unwrap_or_default();
        self.outstanding = true;
    }

    fn poll_bytes(&mut self) -> Option<Result<Vec<PickedFile>, PickError>> {
        self.outstanding = false;
        Some(Ok(self.last.clone()))
    }
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

    /// Decision 423: the bytes seam round-trips scripted files,
    /// dismisses on exhaustion, and re-returns the last result
    /// level-triggered (the `FileDialog` rule, byte-carrying).
    #[test]
    fn scripted_opener_round_trip_and_dismissal() {
        let mut d = ScriptedOpener::new();
        d.push_response(vec![PickedFile::new("a.png".to_string(), vec![1, 2, 3])]);
        d.request_bytes(FilePickerOptions::default());
        assert_eq!(
            d.poll_bytes(),
            Some(Ok(vec![PickedFile::new(
                "a.png".to_string(),
                vec![1, 2, 3]
            )]))
        );
        // Exhausted script = dismissed (empty, not an error).
        d.request_bytes(FilePickerOptions::default());
        assert_eq!(d.poll_bytes(), Some(Ok(vec![])));
        // Level-triggered: re-poll repeats the last result.
        assert_eq!(d.poll_bytes(), Some(Ok(vec![])));
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

    /// Decision 429: a controllable async backend — requests pend
    /// until `resolve` settles them (the SAF/Web rendezvous shape:
    /// `None` while open, one settlement, stale generations
    /// discard). Backend law preserved (supersede + level echo);
    /// the session on top is what unifies.
    #[derive(Debug, Default)]
    struct AsyncFake {
        pending: Option<u64>,
        generation: u64,
        last: Option<Result<Vec<PathBuf>, PickError>>,
    }

    impl AsyncFake {
        fn resolve(&mut self, generation: u64, result: Result<Vec<PathBuf>, PickError>) {
            if self.pending == Some(generation) {
                self.pending = None;
                self.last = Some(result);
            }
        }
    }

    impl FileDialog for AsyncFake {
        fn request_open(&mut self, _options: FilePickerOptions) {
            self.generation += 1;
            self.pending = Some(self.generation);
        }

        fn poll_open(&mut self) -> Option<Result<Vec<PathBuf>, PickError>> {
            if self.pending.is_some() {
                None
            } else {
                self.last.clone()
            }
        }
    }

    /// Decision 429: one press series against an async backend —
    /// `StillOpen`, then the settlement consumed exactly once (never
    /// requested over, never echoed twice), then the next press
    /// re-requests. Same call shape settles synchronously on
    /// blocking backends (covered below).
    #[test]
    fn session_unifies_async_and_sync_presses() {
        use super::{PickPress, PickSession};
        let mut backend = AsyncFake::default();
        let mut session = PickSession::new();
        assert!(!session.is_outstanding());
        // Press 1: fires, still open.
        assert_eq!(
            session.press(&mut backend, FilePickerOptions::default()),
            PickPress::StillOpen
        );
        assert!(session.is_outstanding());
        // Press 2 while open: re-polls, never double-fires
        // (generation stays 1 — one dialog, every platform).
        assert_eq!(
            session.press(&mut backend, FilePickerOptions::default()),
            PickPress::StillOpen
        );
        assert_eq!(backend.generation, 1);
        // Resolve between presses: press 3 consumes WITHOUT
        // requesting (generation stays 1 — settled data is never
        // superseded).
        backend.resolve(1, Ok(vec![PathBuf::from("/tmp/picked.txt")]));
        assert_eq!(
            session.press(&mut backend, FilePickerOptions::default()),
            PickPress::Settled(Ok(vec![PathBuf::from("/tmp/picked.txt")]))
        );
        assert!(!session.is_outstanding());
        // Press 4: consumed echo is not re-surfaced — re-requests.
        assert_eq!(
            session.press(&mut backend, FilePickerOptions::default()),
            PickPress::StillOpen
        );
        assert_eq!(backend.generation, 2);
        // Sync shape: scripted backends settle inside the same press.
        let mut sync = ScriptedDialog::new();
        sync.push_response(vec![PathBuf::from("/tmp/a.png")]);
        let mut session2 = PickSession::new();
        assert_eq!(
            session2.press(&mut sync, FilePickerOptions::default()),
            PickPress::Settled(Ok(vec![PathBuf::from("/tmp/a.png")]))
        );
    }

    /// Decision 429: dismissal and refusal consume like picks
    /// (dismissal is data; errors retry on the next press), and the
    /// shared caption vocabulary renders every outcome.
    #[test]
    fn session_consumes_dismissal_and_refusal() {
        use super::{OpenOutcome, PickPress, PickSession};
        let mut backend = AsyncFake::default();
        let mut session = PickSession::new();
        assert_eq!(
            session.press(&mut backend, FilePickerOptions::default()),
            PickPress::StillOpen
        );
        backend.resolve(1, Ok(vec![]));
        let outcome = OpenOutcome::of(session.press(&mut backend, FilePickerOptions::default()));
        assert_eq!(outcome, OpenOutcome::Dismissed);
        assert_eq!(outcome.path_text(), None);
        assert_eq!(outcome.status(), "dismissed");
        // Refusal consumes, then the next press requests anew.
        assert_eq!(
            session.press(&mut backend, FilePickerOptions::default()),
            PickPress::StillOpen
        );
        backend.resolve(2, Err(PickError::Backend("gone".to_string())));
        let outcome = OpenOutcome::of(session.press(&mut backend, FilePickerOptions::default()));
        assert_eq!(
            outcome,
            OpenOutcome::Failed(PickError::Backend("gone".to_string()))
        );
        assert_eq!(outcome.status(), "picker failed: file picker failed: gone");
        // Caption vocabulary for the remaining outcomes.
        assert_eq!(OpenOutcome::StillOpen.status(), "still open…");
        let picked = OpenOutcome::Picked {
            first: "a.txt".to_string(),
            count: 1,
        };
        assert_eq!(picked.path_text(), Some("a.txt"));
        assert_eq!(picked.file_name(), Some("a.txt"));
        assert_eq!(picked.dir_name(), None);
        assert_eq!(picked.status(), "picked a.txt");
        // Decision 430: absolute paths split into a short name line
        // plus a wrapping directory line, and the caption names the
        // file, never the overflowing path.
        let abs = OpenOutcome::Picked {
            first: "/data/user/0/com.oppa.app/files/picked/tapme.txt".to_string(),
            count: 1,
        };
        assert_eq!(abs.file_name(), Some("tapme.txt"));
        assert_eq!(
            abs.dir_name(),
            Some("/data/user/0/com.oppa.app/files/picked")
        );
        assert_eq!(abs.status(), "picked tapme.txt");
        let multi = OpenOutcome::Picked {
            first: "a.txt".to_string(),
            count: 3,
        };
        assert_eq!(multi.status(), "picked 3 files (first shown)");
    }

    /// Decision 431: the registry drains outstanding sessions
    /// without requesting (loops tick it per iteration), publishes
    /// each settlement exactly once through its callback, and
    /// re-tracking never duplicates.
    #[test]
    fn registry_drains_without_requesting() {
        use super::{OpenOutcome, PickRegistry, PickSession};
        use std::cell::RefCell;
        use std::rc::Rc;
        let fake: Rc<RefCell<AsyncFake>> = Rc::new(RefCell::new(AsyncFake::default()));
        let backend: Rc<RefCell<dyn FileDialog>> = fake.clone();
        let session: Rc<RefCell<PickSession>> = Rc::new(RefCell::new(PickSession::new()));
        let seen: Rc<RefCell<Vec<OpenOutcome>>> = Rc::new(RefCell::new(Vec::new()));
        let mut registry = PickRegistry::new();
        assert!(registry.is_empty());
        let seen_cb = seen.clone();
        registry.track_open(
            &session,
            &backend,
            Rc::new(move |outcome| seen_cb.borrow_mut().push(outcome)),
        );
        assert_eq!(registry.len(), 1);
        // Idle session: pure no-op, backend untouched.
        assert_eq!(registry.poll_all(), 0);
        assert!(seen.borrow().is_empty());
        // Request through the session (the press path — gen 1),
        // then tick while open: drains nothing, fires nothing.
        session
            .borrow_mut()
            .press(&mut *backend.borrow_mut(), FilePickerOptions::default());
        assert_eq!(registry.poll_all(), 0);
        assert!(seen.borrow().is_empty());
        // Resolve between ticks: the next tick consumes + publishes
        // exactly once, and the session goes idle.
        fake.borrow_mut()
            .resolve(1, Ok(vec![PathBuf::from("/tmp/r.txt")]));
        assert_eq!(registry.poll_all(), 1);
        assert_eq!(
            seen.borrow().as_slice(),
            &[OpenOutcome::Picked {
                first: "/tmp/r.txt".to_string(),
                count: 1
            }]
        );
        assert!(!session.borrow().is_outstanding());
        // The settled echo never re-publishes on later ticks.
        assert_eq!(registry.poll_all(), 0);
        assert_eq!(seen.borrow().len(), 1);
        // Re-tracking the same pair replaces instead of duplicating.
        let seen_cb = seen.clone();
        registry.track_open(
            &session,
            &backend,
            Rc::new(move |outcome| seen_cb.borrow_mut().push(outcome)),
        );
        assert_eq!(registry.len(), 1);
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
