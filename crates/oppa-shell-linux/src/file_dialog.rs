//! Linux file-open dialog (Round 2.3, OQ-G12-1): XDG Desktop Portal
//! (`org.freedesktop.portal.FileChooser.OpenFile` over the minimal
//! [`crate::dbus`] client) with a `zenity` subprocess fallback,
//! behind the [`FileDialog`](oppa::FileDialog) request/poll seam.
//!
//! Backend order (probed once, cached): portal when the session bus
//! connects, else `zenity --version`, else loud
//! [`PickError::Unsupported`](oppa::PickError::Unsupported) on use
//! (a headless/foreign box has no picker to wire — refusal, never a
//! silent no-op). Single file, multi-file, and directory selection
//! all route through both backends (`directory` rides
//! `OpenFile(directory=true)` on the portal — no separate
//! `ChooseFolder` — and `--directory` on zenity; directory mode is
//! always single-select, stated). Round 16.1: saves ride
//! `SaveFile` (`current_name` prefill) on the portal and `--save`
//! on zenity, with blocking [`SaveFileDialog`](oppa::SaveFileDialog)
//! / [`FolderDialog`](oppa::FolderDialog) convenience impls over
//! the same request/poll machinery (5 ms bounded pumps —
//! documented blocking, never a surprise).
//!
//! Poll semantics (decision 230, preserved): `request_open`
//! supersedes any outstanding request (the portal worker closes the
//! stale dialog; a spawned zenity is killed — orphan dialogs never
//! linger); `poll_open` is non-blocking (`None` while open,
//! level-triggered last result after). Dismissal settles
//! `Ok(vec![])`; failures are loud `Backend`s.
//!
//! Process spawning goes through [`CommandRunner`] (real
//! [`StdRunner`] in production — also the future `flatpak-spawn`
//! seam — scripted stubs in tests; the argv/output/state logic is
//! identical either way, so headless runs prove the real paths).

use std::path::PathBuf;

use oppa::{
    FileDialog, FileDialogOptions, FilePickerOptions, FolderDialog, FolderDialogOptions, PickError,
    SaveFileDialog,
};

mod portal;
mod zenity;

pub(crate) use portal::{
    portal_reachable, OpenJob, PortalWorker, SaveJob, WorkerCmd, WorkerResult,
};
pub(crate) use zenity::{settle_zenity, zenity_present};
pub use zenity::{
    zenity_argv, zenity_save_argv, ChildOutcome, CommandChild, CommandRunner, StdChild, StdRunner,
};

// ---------------------------------------------------------------------------
// Dialog backend (portal worker xor zenity child, probe-cached)
// ---------------------------------------------------------------------------

/// Which backend a probe selected (cached after the first
/// `request_open` — probing twice would double-spawn).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Backend {
    Portal,
    Zenity,
}

struct PendingZenity<R: CommandRunner> {
    child: R::Child,
}

/// Linux file-open dialog (UI-thread use — owns its worker/child).
/// Generic over the [`CommandRunner`] so headless runs prove the
/// real argv/output/state paths (production uses [`StdRunner`]).
/// Round 16.1: the same struct also serves save + folder picks
/// (request_save_file/poll_save plus the blocking [`SaveFileDialog`]
/// / [`FolderDialog`] impls — one backend, every chooser kind).
pub struct LinuxFileDialog<R: CommandRunner = StdRunner> {
    runner: R,
    probe: Option<Option<Backend>>,
    generation: u64,
    worker: Option<PortalWorker>,
    pending_zenity: Option<PendingZenity<R>>,
    last: Option<Result<Vec<PathBuf>, PickError>>,
    pending_save_zenity: Option<PendingZenity<R>>,
    last_save: Option<Result<Option<PathBuf>, PickError>>,
}

impl<R: CommandRunner> LinuxFileDialog<R> {
    pub fn new(runner: R) -> Self {
        Self {
            runner,
            probe: None,
            generation: 0,
            worker: None,
            pending_zenity: None,
            last: None,
            pending_save_zenity: None,
            last_save: None,
        }
    }

    /// Opens a single/multi file dialog (see [`FileDialog`]).
    pub fn request_open_files(&mut self, options: FilePickerOptions) {
        self.request(false, options);
    }

    /// Opens a directory picker (Round 2.3: portal
    /// `OpenFile(directory=true)`, zenity `--directory` — always
    /// single-select, stated). Settles through the shared
    /// [`poll_open`](FileDialog::poll_open).
    pub fn request_open_dir(&mut self, options: FilePickerOptions) {
        self.request(true, options);
    }

    fn request(&mut self, directory: bool, options: FilePickerOptions) {
        // Supersede: kill/close whatever is outstanding first (no
        // orphan dialogs, no stale generations).
        self.abandon_pending();
        self.generation += 1;
        let gen = self.generation;
        match self.backend() {
            Some(Backend::Portal) => {
                if !self.ensure_worker() {
                    self.last = Some(Err(PickError::Backend(
                        "portal worker would not start".to_string(),
                    )));
                    return;
                }
                let worker = self.worker.as_ref().expect("ensured above");
                if worker
                    .tx
                    .send(WorkerCmd::Open(OpenJob {
                        options,
                        directory,
                        generation: gen,
                    }))
                    .is_err()
                {
                    // Worker died mid-flight — drop it (the next
                    // request respawns) and fail this one loudly.
                    self.worker = None;
                    self.last = Some(Err(PickError::Backend("portal worker died".to_string())));
                }
            }
            Some(Backend::Zenity) => {
                let argv = zenity_argv(&options, directory);
                match self.runner.spawn("zenity", &argv) {
                    Ok(child) => {
                        self.pending_zenity = Some(PendingZenity { child });
                    }
                    Err(e) => {
                        self.last = Some(Err(e));
                    }
                }
            }
            None => {
                self.last = Some(Err(PickError::Unsupported("LinuxFileDialog")));
            }
        }
    }

    /// Opens a save dialog (Round 16.1: portal `SaveFile` with
    /// `current_name`, zenity `--save` — single destination).
    /// Settles through [`poll_save`](Self::poll_save).
    pub fn request_save_file(&mut self, options: FileDialogOptions) {
        // Supersede: kill/close whatever is outstanding first (no
        // orphan dialogs, no stale generations — the open rule,
        // applied across kinds: only one chooser lives at a time).
        self.abandon_pending();
        self.generation += 1;
        let gen = self.generation;
        match self.backend() {
            Some(Backend::Portal) => {
                if !self.ensure_worker() {
                    self.last_save = Some(Err(PickError::Backend(
                        "portal worker would not start".to_string(),
                    )));
                    return;
                }
                let worker = self.worker.as_ref().expect("ensured above");
                if worker
                    .tx
                    .send(WorkerCmd::Save(SaveJob {
                        options,
                        generation: gen,
                    }))
                    .is_err()
                {
                    // Worker died mid-flight — drop it (the next
                    // request respawns) and fail this one loudly.
                    self.worker = None;
                    self.last_save =
                        Some(Err(PickError::Backend("portal worker died".to_string())));
                }
            }
            Some(Backend::Zenity) => {
                let argv = zenity_save_argv(&options);
                match self.runner.spawn("zenity", &argv) {
                    Ok(child) => {
                        self.pending_save_zenity = Some(PendingZenity { child });
                    }
                    Err(e) => {
                        self.last_save = Some(Err(e));
                    }
                }
            }
            None => {
                self.last_save = Some(Err(PickError::Unsupported("LinuxFileDialog")));
            }
        }
    }

    /// Polls the save request: `None` = still open (both backends);
    /// `Some(Ok(path))` = picked, `Some(Ok(None))` = dismissed,
    /// `Some(Err(e))` = refused/failed loudly. Level-triggered last
    /// result after settling (the open rule).
    pub fn poll_save(&mut self) -> Option<Result<Option<PathBuf>, PickError>> {
        self.drain_worker();
        if let Some(mut pending) = self.pending_save_zenity.take() {
            match pending.child.try_wait() {
                Ok(None) => {
                    self.pending_save_zenity = Some(pending);
                    return None;
                }
                Ok(Some(outcome)) => {
                    self.last_save = Some(settle_zenity(&outcome).and_then(single_result));
                }
                Err(e) => {
                    self.last_save = Some(Err(e));
                }
            }
        }
        self.last_save.clone()
    }

    /// Drops outstanding work (a spawned zenity is killed — best
    /// effort, never blocking; the portal path supersedes by sending
    /// the next Open, which closes the stale dialog worker-side, so
    /// there is nothing to drop here for it).
    fn abandon_pending(&mut self) {
        if let Some(mut pending) = self.pending_zenity.take() {
            pending.child.kill();
            // Reap whatever is left so no zombie lingers (poll is
            // non-blocking — a live child just reports None).
            let _ = pending.child.try_wait();
        }
        // Save zenity slot supersedes the same way (open and save
        // never live together — one chooser at a time).
        if let Some(mut pending) = self.pending_save_zenity.take() {
            pending.child.kill();
            let _ = pending.child.try_wait();
        }
    }

    /// Probes backends once (portal bus, else zenity binary) and
    /// caches the decision.
    fn backend(&mut self) -> Option<Backend> {
        if let Some(cached) = self.probe {
            return cached;
        }
        let found = if portal_reachable() {
            Some(Backend::Portal)
        } else if zenity_present(&mut self.runner) {
            Some(Backend::Zenity)
        } else {
            None
        };
        self.probe = Some(found);
        found
    }

    /// Spawns the portal worker on first portal request (lazy — a
    /// headless box never pays for the thread; the worker connects
    /// inside its own thread so a missing bus never blocks the UI).
    fn ensure_worker(&mut self) -> bool {
        if self.worker.is_some() {
            return true;
        }
        match PortalWorker::spawn() {
            Some(worker) => {
                self.worker = Some(worker);
                true
            }
            None => false,
        }
    }

    /// Drains worker results (latest generation wins — stale
    /// generations from a closed dialog drop silently here because
    /// the worker already closed that dialog; the *result* is what
    /// is stale, never the screen). Open and save share one
    /// generation counter + channel, so results route by kind
    /// (Round 16.1 — only one chooser is ever outstanding, but the
    /// tag keeps a save result out of an open poll and vice versa).
    fn drain_worker(&mut self) {
        let mut latest: Option<WorkerResult> = None;
        if let Some(worker) = self.worker.as_ref() {
            while let Ok(r) = worker.rx.try_recv() {
                latest = Some(r);
            }
        }
        if let Some(r) = latest {
            if r.generation != self.generation {
                return;
            }
            if r.save {
                // Saves settle one path (the portal returns a single
                // uri — several refuse loudly in `single_result`).
                self.last_save = Some(r.result.and_then(single_result));
            } else {
                self.last = Some(r.result);
            }
        }
    }
}

/// Collapses a save/folder result to one path (Round 16.1 — pure,
/// headless-tested): empty dismisses (`Ok(None)` — the
/// decision-230 rule), one picks, several refuse loudly (never a
/// silent first-wins on user data).
pub(crate) fn single_result(paths: Vec<PathBuf>) -> Result<Option<PathBuf>, PickError> {
    match paths.len() {
        0 => Ok(None),
        1 => Ok(Some(paths.into_iter().next().expect("exactly one"))),
        n => Err(PickError::Backend(format!(
            "save/folder settled {n} paths — refusing, never first-wins"
        ))),
    }
}

impl<R: CommandRunner> Drop for LinuxFileDialog<R> {
    /// Best-effort worker shutdown (never blocking, never joining —
    /// a mid-dialog worker exits on the command; a detached thread
    /// outlives only to process exit, documented).
    fn drop(&mut self) {
        if let Some(worker) = self.worker.as_ref() {
            let _ = worker.tx.send(WorkerCmd::Shutdown);
        }
    }
}

impl<R: CommandRunner> FileDialog for LinuxFileDialog<R> {
    fn request_open(&mut self, options: FilePickerOptions) {
        self.request(false, options);
    }

    fn poll_open(&mut self) -> Option<Result<Vec<PathBuf>, PickError>> {
        // Worker results first (a settled portal beats a stale
        // zenity child — only one path is ever outstanding after
        // `abandon_pending`, so order is just determinism).
        self.drain_worker();
        if let Some(mut pending) = self.pending_zenity.take() {
            match pending.child.try_wait() {
                Ok(None) => {
                    self.pending_zenity = Some(pending);
                    return None;
                }
                Ok(Some(outcome)) => {
                    self.last = Some(settle_zenity(&outcome));
                }
                Err(e) => {
                    self.last = Some(Err(e));
                }
            }
        }
        self.last.clone()
    }
}

impl<R: CommandRunner> SaveFileDialog for LinuxFileDialog<R> {
    fn save(&mut self, options: FileDialogOptions) -> Result<Option<PathBuf>, PickError> {
        // Blocking convenience over request/poll (decision-314
        // contract): bounded 5 ms pumps until the user settles.
        // Async callers keep request_save_file/poll_save instead.
        self.request_save_file(options);
        loop {
            if let Some(settled) = self.poll_save() {
                return settled;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

impl<R: CommandRunner> FolderDialog for LinuxFileDialog<R> {
    fn pick(&mut self, options: FolderDialogOptions) -> Result<Option<PathBuf>, PickError> {
        // Folders ride the existing OpenFile(directory=true) /
        // zenity --directory machinery (Round 2.3), collapsed to
        // one path here (always single-select, stated).
        self.request_open_dir(FilePickerOptions {
            title: options.title,
            filters: Vec::new(),
            multiple: false,
            initial_dir: options.initial_dir,
        });
        loop {
            match self.poll_open() {
                None => std::thread::sleep(std::time::Duration::from_millis(5)),
                Some(Err(e)) => return Err(e),
                Some(Ok(paths)) => return single_result(paths),
            }
        }
    }
}

#[cfg(test)]
mod tests;
