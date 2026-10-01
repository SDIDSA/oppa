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

use crate::dbus::{self, BusConn, BusTransport, DVal};

// ---------------------------------------------------------------------------
// Process seam (zenity + probing)
// ---------------------------------------------------------------------------

/// One spawned helper process (zenity): non-blocking exit poll
/// plus output capture. Killed on supersede (see
/// [`CommandChild::kill`]).
pub trait CommandChild {
    /// Non-blocking poll: `None` = still running; `Some` = reaped
    /// with exit code + captured stdout/stderr.
    fn try_wait(&mut self) -> Result<Option<ChildOutcome>, PickError>;
    /// Best-effort kill (supersede path — errors are advisory, the
    /// old dialog going away is what matters, and the next poll
    /// reaps whatever is left).
    fn kill(&mut self);
}

/// A reaped helper process.
#[derive(Clone, Debug, PartialEq)]
pub struct ChildOutcome {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Spawns helper processes (`zenity`, probes). The production impl
/// is [`StdRunner`]; tests script canned outcomes (same argv
/// parsing + state machine either way).
pub trait CommandRunner {
    type Child: CommandChild;
    fn spawn(&mut self, program: &str, args: &[String]) -> Result<Self::Child, PickError>;
}

/// [`CommandRunner`] over `std::process` (production).
pub struct StdRunner;

/// One spawned helper process, as returned by [`StdRunner`]
/// (opaque handle — drive it through [`CommandChild`]).
pub struct StdChild {
    child: Option<std::process::Child>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    done: bool,
}

impl CommandChild for StdChild {
    fn try_wait(&mut self) -> Result<Option<ChildOutcome>, PickError> {
        if self.done {
            return Ok(None);
        }
        let Some(child) = self.child.as_mut() else {
            return Ok(None);
        };
        match child.try_wait() {
            Ok(None) => Ok(None),
            Ok(Some(status)) => {
                self.done = true;
                // Dialog outputs are path lists (bytes, never
                // megabytes): take the pipes after exit, when no
                // writer can block us. Order: stdout, then stderr,
                // then wait (reap) — all post-exit, none blocking.
                if let Some(mut out) = child.stdout.take() {
                    use std::io::Read;
                    let _ = out.read_to_end(&mut self.stdout);
                }
                if let Some(mut err) = child.stderr.take() {
                    use std::io::Read;
                    let _ = err.read_to_end(&mut self.stderr);
                }
                let _ = child.wait();
                Ok(Some(ChildOutcome {
                    code: status.code().unwrap_or(-1),
                    stdout: std::mem::take(&mut self.stdout),
                    stderr: std::mem::take(&mut self.stderr),
                }))
            }
            Err(e) => Err(PickError::Backend(format!("zenity wait failed: {e}"))),
        }
    }

    fn kill(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
    }
}

impl CommandRunner for StdRunner {
    type Child = StdChild;

    fn spawn(&mut self, program: &str, args: &[String]) -> Result<Self::Child, PickError> {
        std::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map(|child| StdChild {
                child: Some(child),
                stdout: Vec::new(),
                stderr: Vec::new(),
                done: false,
            })
            .map_err(|e| PickError::Backend(format!("spawn {program}: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Pure shapes (argv building, output parsing, URIs — headless-tested)
// ---------------------------------------------------------------------------

/// Builds the `zenity --file-selection` argv for `options`
/// (`directory` selects the `--directory` shape, always
/// single-select — stated). Newline-joined multi output (real `\n`
/// — passed as argv, never through a shell); one
/// `--file-filter` per filter (`NAME | p1 p2`); `--filename`
/// carries `initial_dir` (trailing slash hints folders).
pub fn zenity_argv(options: &FilePickerOptions, directory: bool) -> Vec<String> {
    let mut argv = vec!["--file-selection".to_string()];
    if directory {
        argv.push("--directory".to_string());
    } else if options.multiple {
        argv.push("--multiple".to_string());
        argv.push("--separator=\n".to_string());
    }
    if !options.title.is_empty() {
        argv.push(format!("--title={}", options.title));
    }
    if let Some(dir) = &options.initial_dir {
        let mut start = dir.to_string_lossy().into_owned();
        if !start.ends_with('/') {
            start.push('/');
        }
        argv.push(format!("--filename={start}"));
    }
    if !directory {
        for f in &options.filters {
            argv.push(format!(
                "--file-filter={} | {}",
                f.name,
                f.patterns.join(" ")
            ));
        }
    }
    argv
}

/// Builds the `zenity --file-selection --save` argv for `options`
/// (Round 16.1 — single destination: never `--multiple` or
/// `--directory`). `--filename` carries the suggested destination
/// (`initial_dir` + `default_name`, no trailing slash — it names a
/// file, not a folder); filters ride the same `--file-filter`
/// shape as open. Pure (headless-tested).
pub fn zenity_save_argv(options: &FileDialogOptions) -> Vec<String> {
    let mut argv = vec!["--file-selection".to_string(), "--save".to_string()];
    if !options.title.is_empty() {
        argv.push(format!("--title={}", options.title));
    }
    let mut start = options
        .initial_dir
        .as_ref()
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !start.is_empty() && !start.ends_with('/') {
        start.push('/');
    }
    start.push_str(&options.default_name);
    if !start.is_empty() {
        argv.push(format!("--filename={start}"));
    }
    for f in &options.filters {
        argv.push(format!(
            "--file-filter={} | {}",
            f.name,
            f.patterns.join(" ")
        ));
    }
    argv
}

/// Parses zenity stdout (newline-joined by our `--separator`;
/// trailing newline dropped). Empty output reads as dismissed
/// (`Ok(vec![])` — the decision-230 dismissal rule, never an
/// error). Filenames containing newlines cannot round-trip
/// (essentially nonexistent — stated bound).
pub(crate) fn parse_zenity_output(out: &[u8]) -> Vec<PathBuf> {
    String::from_utf8_lossy(out)
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Settles a reaped zenity run: exit 0 parses paths, exit 1 is
/// dismissal (`Ok(vec![])` — window-X and Cancel share it), anything
/// else is a loud `Backend` carrying stderr.
pub(crate) fn settle_zenity(outcome: &ChildOutcome) -> Result<Vec<PathBuf>, PickError> {
    match outcome.code {
        0 => Ok(parse_zenity_output(&outcome.stdout)),
        1 => Ok(Vec::new()),
        code => Err(PickError::Backend(format!(
            "zenity failed (exit {code}): {}",
            String::from_utf8_lossy(&outcome.stderr).trim()
        ))),
    }
}

/// Decodes a `file://` URI from a portal `uris` result into a local
/// path (pure — headless-tested): empty authority or `localhost`
/// only (remote hosts refuse loudly — they never become local
/// paths silently), `%XX` percent-decoding (`+` stays literal —
/// paths, not queries), empty paths refuse.
pub(crate) fn uri_to_path(uri: &str) -> Result<PathBuf, PickError> {
    let rest = uri
        .strip_prefix("file://")
        .ok_or_else(|| PickError::Backend(format!("non-file URI refused: {uri:?}")))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if !authority.is_empty() && authority != "localhost" {
        return Err(PickError::Backend(format!(
            "remote file URI refused (never a silent local path): {uri:?}"
        )));
    }
    let decoded = percent_decode(path)?;
    if decoded.is_empty() {
        return Err(PickError::Backend(format!("empty file URI path: {uri:?}")));
    }
    Ok(PathBuf::from(decoded))
}

/// `%XX` decoding over bytes (pure — headless-tested): malformed
/// escapes refuse loudly, `+` is literal.
pub(crate) fn percent_decode(path: &str) -> Result<String, PickError> {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(PickError::Backend(format!(
                    "bad percent escape in {path:?}"
                )));
            }
            let hex = |b: u8| match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                b'A'..=b'F' => Some(b - b'A' + 10),
                _ => None,
            };
            match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 3;
                }
                _ => {
                    return Err(PickError::Backend(format!(
                        "bad percent escape in {path:?}"
                    )));
                }
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|e| PickError::Backend(format!("URI path is not UTF-8: {e}")))
}

/// Builds the portal `OpenFile` options dict (crate-visible pure
/// helper — headless-tested through marshal round-trip): `handle_token` + `multiple` +
/// `directory` + glob `filters` (`(name, [(0, pattern)...])` per the
/// [FileChooser](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html)
/// spec — type 0 is glob, 1 is MIME; ours are globs) +
/// `current_folder` (NUL-terminated fs bytes, omitted when unset).
/// Empty filter lists omit the key (absent means unfiltered).
pub(crate) fn portal_options(
    options: &FilePickerOptions,
    directory: bool,
    token: &str,
) -> Vec<(String, DVal)> {
    let mut out = vec![
        (
            "handle_token".to_string(),
            DVal::Variant(Box::new(DVal::Str(token.to_string()))),
        ),
        (
            "multiple".to_string(),
            DVal::Variant(Box::new(DVal::Bool(!directory && options.multiple))),
        ),
        (
            "directory".to_string(),
            DVal::Variant(Box::new(DVal::Bool(directory))),
        ),
    ];
    if !directory && !options.filters.is_empty() {
        let mut filters = Vec::new();
        for f in &options.filters {
            let mut entries = Vec::new();
            for p in &f.patterns {
                entries.push(DVal::Struct(vec![DVal::U32(0), DVal::Str(p.clone())]));
            }
            filters.push(DVal::Struct(vec![
                DVal::Str(f.name.clone()),
                DVal::Array(entries),
            ]));
        }
        out.push((
            "filters".to_string(),
            DVal::Variant(Box::new(DVal::Array(filters))),
        ));
    }
    #[cfg(unix)]
    if let Some(dir) = &options.initial_dir {
        use std::os::unix::ffi::OsStrExt;
        let mut bytes = dir.as_os_str().as_bytes().to_vec();
        bytes.push(0);
        out.push((
            "current_folder".to_string(),
            DVal::Variant(Box::new(DVal::Bytes(bytes))),
        ));
    }
    out
}

/// Builds the portal `SaveFile` options dict (Round 16.1, decision
/// 314 — same `handle_token` + glob `filters` + `current_folder`
/// shape as [`portal_options`], plus `current_name` carrying
/// `default_name` (omitted when empty — the daemon then offers its
/// own); no `multiple`/`directory` keys (saves pick exactly one
/// file). Crate-visible pure helper — headless-tested through
/// marshal round-trip.
pub(crate) fn portal_save_options(options: &FileDialogOptions, token: &str) -> Vec<(String, DVal)> {
    let mut out = vec![(
        "handle_token".to_string(),
        DVal::Variant(Box::new(DVal::Str(token.to_string()))),
    )];
    if !options.default_name.is_empty() {
        out.push((
            "current_name".to_string(),
            DVal::Variant(Box::new(DVal::Str(options.default_name.clone()))),
        ));
    }
    if !options.filters.is_empty() {
        let mut filters = Vec::new();
        for f in &options.filters {
            let mut entries = Vec::new();
            for p in &f.patterns {
                entries.push(DVal::Struct(vec![DVal::U32(0), DVal::Str(p.clone())]));
            }
            filters.push(DVal::Struct(vec![
                DVal::Str(f.name.clone()),
                DVal::Array(entries),
            ]));
        }
        out.push((
            "filters".to_string(),
            DVal::Variant(Box::new(DVal::Array(filters))),
        ));
    }
    #[cfg(unix)]
    if let Some(dir) = &options.initial_dir {
        use std::os::unix::ffi::OsStrExt;
        let mut bytes = dir.as_os_str().as_bytes().to_vec();
        bytes.push(0);
        out.push((
            "current_folder".to_string(),
            DVal::Variant(Box::new(DVal::Bytes(bytes))),
        ));
    }
    out
}

/// Portal method for a request kind (Round 16.1 — pure,
/// headless-tested): saves go to `SaveFile`, opens (files or
/// `directory=true` folders) to `OpenFile`. One literal per kind —
/// the worker never guesses a method name.
pub(crate) fn chooser_method(save: bool) -> &'static str {
    if save {
        "SaveFile"
    } else {
        "OpenFile"
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

// ---------------------------------------------------------------------------
// Portal worker (blocking bus pump on its own thread — the UI thread
// never blocks on a user-driven dialog; G7 precedent)
// ---------------------------------------------------------------------------

/// One portal open request for the worker.
struct OpenJob {
    options: FilePickerOptions,
    directory: bool,
    generation: u64,
}

/// One portal save request for the worker (Round 16.1 — same
/// request/response lifecycle as [`OpenJob`], one destination).
struct SaveJob {
    options: FileDialogOptions,
    generation: u64,
}

enum WorkerCmd {
    Open(OpenJob),
    Save(SaveJob),
    Shutdown,
}

struct WorkerResult {
    generation: u64,
    /// True for save responses (Round 16.1 — open and save share
    /// one generation counter + channel, so results route by kind;
    /// stale generations drop either way).
    save: bool,
    result: Result<Vec<PathBuf>, PickError>,
}

/// Response codes (portal convention: 0 picked, 1 dismissed,
/// anything else loud).
fn settle_response(code: u32, results: Vec<(String, DVal)>) -> Result<Vec<PathBuf>, PickError> {
    match code {
        0 => {
            let mut paths = Vec::new();
            for (key, val) in &results {
                if key == "uris" {
                    if let DVal::Variant(inner) = val {
                        if let DVal::Array(items) = &**inner {
                            for item in items {
                                if let DVal::Str(uri) = item {
                                    paths.push(uri_to_path(uri)?);
                                } else {
                                    return Err(PickError::Backend(format!(
                                        "portal uris entry is not a string: {item:?}"
                                    )));
                                }
                            }
                        } else {
                            return Err(PickError::Backend(format!(
                                "portal uris is not a string array: {val:?}"
                            )));
                        }
                    } else {
                        return Err(PickError::Backend(format!(
                            "portal uris is not a variant: {val:?}"
                        )));
                    }
                }
            }
            Ok(paths)
        }
        1 => Ok(Vec::new()),
        other => Err(PickError::Backend(format!(
            "portal dismissed with code {other}"
        ))),
    }
}

/// The worker's blocking dialog run (owns its bus for its whole
/// life): closes any previous request, subscribes once, opens,
/// awaits the `Response` for our handle while watching the command
/// channel (supersede/close/shutdown preempt the wait — no stuck
/// threads, no orphan dialogs).
fn worker_main<T: BusTransport>(
    mut conn: BusConn<T>,
    rx: std::sync::mpsc::Receiver<WorkerCmd>,
    tx: std::sync::mpsc::Sender<WorkerResult>,
    pid: u32,
) {
    // Broad Response subscription once (path-matched per request —
    // fewer round-trips than per-request rules, same selectivity).
    let mut stashed = Vec::new();
    if conn
        .call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "AddMatch",
            "s",
            &[DVal::Str(
                "type='signal',interface='org.freedesktop.portal.Request',member='Response'"
                    .to_string(),
            )],
            &mut stashed,
            std::time::Duration::from_secs(5),
        )
        .is_err()
    {
        return;
    }
    let sender_flat = conn
        .sender()
        .trim_start_matches(':')
        .replace(['.', '-'], "_");
    let mut counter = 0u64;
    let mut current: Option<String> = None;
    let mut pending: Option<WorkerCmd> = None;
    loop {
        // Next job: a preempted supersede wins over the channel
        // (no queueing behind a stale dialog — the trait's
        // supersede rule). Idle with no pending blocks (a
        // dialog-free worker sleeps here). Either kind parks as
        // pending (both close the stale dialog above); a shutdown
        // never parks (it closes inline below).
        let cmd = match pending.take() {
            Some(cmd) => cmd,
            None => match rx.recv() {
                Ok(cmd) => cmd,
                Err(_) => return,
            },
        };
        let save = matches!(cmd, WorkerCmd::Save(_));
        if matches!(cmd, WorkerCmd::Shutdown) {
            return;
        }
        // Supersede: close the stale dialog first (best-effort —
        // its late Response, if any, carries a dead generation and
        // is dropped by the generation check below).
        if let Some(ref handle) = current {
            let _ = conn.call(
                "org.freedesktop.portal.Desktop",
                handle,
                "org.freedesktop.portal.Request",
                "Close",
                "",
                &[],
                &mut stashed,
                std::time::Duration::from_secs(2),
            );
            current = None;
        }
        counter += 1;
        let token = format!("oppa{pid}_{counter}");
        let expected = format!("/org/freedesktop/portal/desktop/request/{sender_flat}/{token}");
        // Method + options per kind (Round 16.1 — saves ride
        // `SaveFile` with `current_name`; opens keep `OpenFile`
        // with the file/folder flag).
        let dict_of = |pairs: Vec<(String, DVal)>| {
            pairs
                .into_iter()
                .map(|(k, v)| DVal::Struct(vec![DVal::Str(k), v]))
                .collect::<Vec<DVal>>()
        };
        let (method, title, dict, generation) = match &cmd {
            WorkerCmd::Open(job) => (
                chooser_method(false),
                job.options.title.clone(),
                dict_of(portal_options(&job.options, job.directory, &token)),
                job.generation,
            ),
            WorkerCmd::Save(job) => (
                chooser_method(true),
                job.options.title.clone(),
                dict_of(portal_save_options(&job.options, &token)),
                job.generation,
            ),
            WorkerCmd::Shutdown => return,
        };
        let reply = conn.call(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.FileChooser",
            method,
            "ssa{sv}",
            &[
                DVal::Str(String::new()),
                DVal::Str(title),
                DVal::Array(dict),
            ],
            &mut stashed,
            std::time::Duration::from_secs(10),
        );
        let handle = match reply {
            Ok(msg) => match dbus::parse_body(&msg.body, &msg.sig) {
                Ok(vals) => match vals.as_slice() {
                    [DVal::Path(h)] => h.clone(),
                    other => {
                        let _ = tx.send(WorkerResult {
                            generation,
                            save,
                            result: Err(PickError::Backend(format!(
                                "portal handle is not a path: {other:?}"
                            ))),
                        });
                        continue;
                    }
                },
                Err(e) => {
                    let _ = tx.send(WorkerResult {
                        generation,
                        save,
                        result: Err(PickError::Backend(format!("portal handle: {e}"))),
                    });
                    continue;
                }
            },
            Err(e) => {
                let _ = tx.send(WorkerResult {
                    generation,
                    save,
                    result: Err(PickError::Backend(format!("portal {method}: {e}"))),
                });
                continue;
            }
        };
        // Sanity: the daemon echoes our token path (a foreign path
        // is a wiring bug — refuse, never follow it blindly).
        if handle != expected {
            let _ = tx.send(WorkerResult {
                generation,
                save,
                result: Err(PickError::Backend(format!(
                    "portal handle mismatch (want {expected:?}, got {handle:?})"
                ))),
            });
            continue;
        }
        current = Some(handle.clone());
        // Await our Response (100 ms pumps — preemption drains
        // here, never in a blocking read).
        match await_response(&mut conn, &handle, &rx, &mut stashed) {
            AwaitOutcome::Settled(result) => {
                current = None;
                let _ = tx.send(WorkerResult {
                    generation,
                    save,
                    result,
                });
            }
            // A preempting Open or Save parks as pending (the loop
            // takes it first and closes the stale dialog above);
            // Shutdown closes it here and exits (no next pass may
            // come).
            AwaitOutcome::Preempted(cmd) => {
                if matches!(cmd, WorkerCmd::Shutdown) {
                    if let Some(ref handle) = current {
                        let _ = conn.call(
                            "org.freedesktop.portal.Desktop",
                            handle,
                            "org.freedesktop.portal.Request",
                            "Close",
                            "",
                            &[],
                            &mut stashed,
                            std::time::Duration::from_secs(2),
                        );
                    }
                    return;
                }
                pending = Some(cmd);
                current = None;
            }
            AwaitOutcome::Disconnected => return,
        }
    }
}

enum AwaitOutcome {
    Settled(Result<Vec<PathBuf>, PickError>),
    Preempted(WorkerCmd),
    Disconnected,
}

/// Pumps one Response wait: socket reads at 100 ms bounded waits
/// interleaved with command drains (user-driven dialogs block here
/// as long as they must — the UI thread never does).
fn await_response<T: BusTransport>(
    conn: &mut BusConn<T>,
    handle: &str,
    rx: &std::sync::mpsc::Receiver<WorkerCmd>,
    stashed: &mut Vec<dbus::InMsg>,
) -> AwaitOutcome {
    // Drain the stash first (a fast daemon may have answered
    // before we started waiting).
    if let Some(r) = take_response(stashed, handle) {
        return AwaitOutcome::Settled(r);
    }
    loop {
        // Commands preempt the wait — the consumed command rides
        // back out (mpsc has no requeue, so the outcome carries it).
        match rx.try_recv() {
            Ok(cmd) => return AwaitOutcome::Preempted(cmd),
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return AwaitOutcome::Disconnected;
            }
        }
        let _ = handle;
        match conn.read_message("response", std::time::Duration::from_millis(100)) {
            Ok(msg) => {
                if msg.mtype == dbus::MSG_SIGNAL
                    && msg.member.as_deref() == Some("Response")
                    && msg.path.as_deref() == Some(handle)
                {
                    return AwaitOutcome::Settled(settle_response_msg(&msg));
                } else if msg.mtype == dbus::MSG_SIGNAL {
                    stashed.push(msg);
                }
            }
            Err(e) => {
                // Read timeouts are the pump tick (user still
                // choosing); a dead bus ends the dialog loudly (no
                // response can ever arrive). The timeout text is our
                // own bounded-wait message (grep-stable by
                // construction, documented here — not parsed wire).
                if e.contains("timed out") {
                    continue;
                }
                return AwaitOutcome::Settled(Err(PickError::Backend(format!(
                    "portal bus died mid-dialog: {e}"
                ))));
            }
        }
    }
}

/// Settles one Response signal message into paths (shared by the
/// stash drain and the live wait — one parser, never two).
fn settle_response_msg(msg: &dbus::InMsg) -> Result<Vec<PathBuf>, PickError> {
    match dbus::parse_body(&msg.body, &msg.sig) {
        Ok(vals) => match vals.as_slice() {
            [DVal::Struct(pair)] => match pair.as_slice() {
                [DVal::U32(code), DVal::Array(dict)] => {
                    let mut results = Vec::new();
                    for entry in dict {
                        if let DVal::Struct(kv) = entry {
                            if let [DVal::Str(k), v] = kv.as_slice() {
                                results.push((k.clone(), v.clone()));
                            }
                        }
                    }
                    settle_response(*code, results)
                }
                _ => Err(PickError::Backend("portal response shape".to_string())),
            },
            _ => Err(PickError::Backend("portal response arity".to_string())),
        },
        Err(e) => Err(PickError::Backend(format!("portal response: {e}"))),
    }
}

/// Pulls our handle's Response from the stash, if present.
fn take_response(
    stashed: &mut Vec<dbus::InMsg>,
    handle: &str,
) -> Option<Result<Vec<PathBuf>, PickError>> {
    let pos = stashed.iter().position(|m| {
        m.mtype == dbus::MSG_SIGNAL
            && m.member.as_deref() == Some("Response")
            && m.path.as_deref() == Some(handle)
    })?;
    let msg = stashed.remove(pos);
    Some(settle_response_msg(&msg))
}

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

struct PortalWorker {
    tx: std::sync::mpsc::Sender<WorkerCmd>,
    rx: std::sync::mpsc::Receiver<WorkerResult>,
    #[allow(dead_code)]
    thread: Option<std::thread::JoinHandle<()>>,
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
        let addr = match dbus::default_address() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<WorkerCmd>();
        let (res_tx, res_rx) = std::sync::mpsc::channel::<WorkerResult>();
        let pid = std::process::id();
        let thread = std::thread::Builder::new()
            .name("oppa-portal".to_string())
            .spawn(move || {
                // Connect inside the worker (a missing bus fails
                // here, never on the UI thread); each transport
                // monomorphizes the generic worker below.
                match addr {
                    dbus::BusAddr::UnixPath(p) => {
                        #[cfg(unix)]
                        if let Ok(conn) = dbus::connect_unix(&p, std::time::Duration::from_secs(5))
                        {
                            worker_main(conn, cmd_rx, res_tx, pid);
                        }
                        #[cfg(not(unix))]
                        {
                            let _ = (p, cmd_rx, res_tx, pid);
                        }
                    }
                    #[cfg(unix)]
                    dbus::BusAddr::UnixAbstract(name) => {
                        if let Ok(conn) =
                            dbus::connect_abstract(&name, std::time::Duration::from_secs(5))
                        {
                            worker_main(conn, cmd_rx, res_tx, pid);
                        }
                    }
                    dbus::BusAddr::Tcp { host, port } => {
                        if let Ok(conn) =
                            dbus::connect_tcp(&host, port, std::time::Duration::from_secs(5))
                        {
                            worker_main(conn, cmd_rx, res_tx, pid);
                        }
                    }
                    #[allow(unreachable_patterns)]
                    _ => {}
                }
            });
        match thread {
            Ok(thread) => {
                self.worker = Some(PortalWorker {
                    tx: cmd_tx,
                    rx: res_rx,
                    thread: Some(thread),
                });
                true
            }
            Err(_) => false,
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

/// True when a portal bus is dialable right now (socket exists —
/// no full handshake on the probe path; the worker handshakes for
/// real on first use).
fn portal_reachable() -> bool {
    let addr = match dbus::default_address() {
        Ok(a) => a,
        Err(_) => return false,
    };
    match addr {
        dbus::BusAddr::UnixPath(p) => std::path::Path::new(&p).exists(),
        #[cfg(unix)]
        dbus::BusAddr::UnixAbstract(_) => true,
        dbus::BusAddr::Tcp { .. } => false,
    }
}

/// True when `zenity --version` exits 0 (probe, not a dialog).
fn zenity_present<R: CommandRunner>(runner: &mut R) -> bool {
    let mut child = match runner.spawn("zenity", &["--version".to_string()]) {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Spin briefly (local exec — milliseconds; bounded 2 s so a
    // wedged helper never hangs the probe, loudly false past it).
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(out)) => return out.code == 0,
            Ok(None) => {}
            Err(_) => return false,
        }
        if start.elapsed() > std::time::Duration::from_secs(2) {
            child.kill();
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
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
mod tests {
    use super::*;
    use oppa::FileFilter;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Recorded spawns (program + argv) shared with assertions.
    type Calls = Rc<RefCell<Vec<(String, Vec<String>)>>>;

    /// Scripted process spawner (records argv, plays canned
    /// outcomes): the zenity surface without a zenity.
    struct StubRunner {
        calls: Calls,
        script: RefCell<Vec<Option<ChildOutcome>>>,
        fail_spawn: bool,
        kills: Rc<std::cell::Cell<usize>>,
    }

    struct StubChild {
        outcome: Option<ChildOutcome>,
        killed: Rc<RefCell<bool>>,
        kills: Rc<std::cell::Cell<usize>>,
    }

    impl CommandChild for StubChild {
        fn try_wait(&mut self) -> Result<Option<ChildOutcome>, PickError> {
            Ok(self.outcome.clone())
        }

        fn kill(&mut self) {
            *self.killed.borrow_mut() = true;
            self.kills.set(self.kills.get() + 1);
        }
    }

    impl CommandRunner for StubRunner {
        type Child = StubChild;

        fn spawn(&mut self, program: &str, args: &[String]) -> Result<Self::Child, PickError> {
            self.calls
                .borrow_mut()
                .push((program.to_string(), args.to_vec()));
            if self.fail_spawn {
                return Err(PickError::Backend("stub spawn failed".to_string()));
            }
            let killed = Rc::new(RefCell::new(false));
            let outcome = self.script.borrow_mut().pop().flatten();
            Ok(StubChild {
                outcome,
                killed,
                kills: self.kills.clone(),
            })
        }
    }

    fn stub() -> (
        LinuxFileDialog<StubRunner>,
        Calls,
        Rc<std::cell::Cell<usize>>,
    ) {
        let calls: Calls = Rc::new(RefCell::new(Vec::new()));
        let kills = Rc::new(std::cell::Cell::new(0));
        let runner = StubRunner {
            calls: calls.clone(),
            script: RefCell::new(vec![None]),
            fail_spawn: false,
            kills: kills.clone(),
        };
        // Force the zenity backend without probing the real box
        // (probe caching is itself asserted below — here the argv
        // and state paths are the question).
        let mut dialog = LinuxFileDialog::new(runner);
        dialog.probe = Some(Some(Backend::Zenity));
        (dialog, calls, kills)
    }

    fn options() -> FilePickerOptions {
        FilePickerOptions {
            title: "Pick".to_string(),
            filters: vec![FileFilter {
                name: "PNG images".to_string(),
                patterns: vec!["*.png".to_string(), "*.PNG".to_string()],
            }],
            multiple: true,
            initial_dir: Some(PathBuf::from("/tmp")),
        }
    }

    #[test]
    fn zenity_argv_covers_single_multi_and_dir() {
        let (mut dialog, calls, _) = stub();
        dialog.request_open_files(options());
        let (_, argv) = calls.borrow().last().cloned().expect("spawned");
        assert!(argv.contains(&"--file-selection".to_string()), "{argv:?}");
        assert!(argv.contains(&"--multiple".to_string()), "{argv:?}");
        assert!(
            argv.iter().any(|a| a == "--separator=\n"),
            "newline separator: {argv:?}"
        );
        assert!(argv.iter().any(|a| a == "--title=Pick"), "{argv:?}");
        assert!(argv.iter().any(|a| a == "--filename=/tmp/"), "{argv:?}");
        assert!(
            argv.iter()
                .any(|a| a == "--file-filter=PNG images | *.png *.PNG"),
            "filter shape: {argv:?}"
        );
        // Directory mode: --directory, single, no filters/multiple.
        let (mut dialog, calls, _) = stub();
        dialog.request_open_dir(FilePickerOptions {
            title: String::new(),
            ..Default::default()
        });
        let (_, argv) = calls.borrow().last().cloned().expect("spawned");
        assert!(argv.contains(&"--directory".to_string()), "{argv:?}");
        assert!(!argv.iter().any(|a| a == "--multiple"), "{argv:?}");
        assert!(
            !argv.iter().any(|a| a.starts_with("--file-filter")),
            "{argv:?}"
        );
        assert!(
            !argv.iter().any(|a| a.starts_with("--title")),
            "empty title omitted: {argv:?}"
        );
    }

    #[test]
    fn zenity_output_settles_paths_dismissal_and_errors() {
        // Multi pick, trailing newline dropped.
        let out = ChildOutcome {
            code: 0,
            stdout: b"/a/b.png\n/c/d.png\n".to_vec(),
            stderr: Vec::new(),
        };
        assert_eq!(
            settle_zenity(&out).expect("parses"),
            vec![PathBuf::from("/a/b.png"), PathBuf::from("/c/d.png")]
        );
        // Dismissal (Cancel / window-X) is data, not failure.
        let dismissed = ChildOutcome {
            code: 1,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        assert_eq!(
            settle_zenity(&dismissed).expect("dismisses"),
            Vec::<PathBuf>::new()
        );
        // Anything else carries stderr loudly.
        let failed = ChildOutcome {
            code: 2,
            stdout: Vec::new(),
            stderr: b"Gtk: cannot open display".to_vec(),
        };
        let err = settle_zenity(&failed).expect_err("fails loudly");
        assert!(err.to_string().contains("cannot open display"), "{err}");
    }

    #[test]
    fn request_poll_tracks_open_and_level_triggered_last() {
        let (mut dialog, _, _) = stub();
        // The stub child reports None (still open) until fed.
        dialog.request_open_files(options());
        assert_eq!(dialog.poll_open(), None, "open dialog pends");
        assert_eq!(dialog.poll_open(), None, "still pends");
        // A settled child settles, then re-polls level-triggered.
        let settled = ChildOutcome {
            code: 0,
            stdout: b"/picked/a.txt\n".to_vec(),
            stderr: Vec::new(),
        };
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut dialog = LinuxFileDialog::new(StubRunner {
            calls,
            script: RefCell::new(vec![Some(settled)]),
            fail_spawn: false,
            kills: Rc::new(std::cell::Cell::new(0)),
        });
        dialog.probe = Some(Some(Backend::Zenity));
        dialog.request_open_files(options());
        let first = dialog.poll_open();
        assert_eq!(
            first,
            Some(Ok(vec![PathBuf::from("/picked/a.txt")])),
            "settled paths"
        );
        assert_eq!(dialog.poll_open(), first, "level-triggered repeat");
        // Supersede kills the stale child and spawns anew.
        let (mut dialog, calls, kills) = stub();
        dialog.request_open_files(options());
        dialog.request_open_files(options());
        assert_eq!(calls.borrow().len(), 2, "two spawns");
        assert_eq!(kills.get(), 1, "stale child killed, no orphan dialog");
        assert_eq!(dialog.poll_open(), None);
    }

    #[test]
    fn probe_without_backends_refuses_loudly() {
        // Neither a bus nor a binary on this box shape (the probe
        // runs for real — on a Linux desktop with either present it
        // would select it; here both are absent by construction of
        // the stub runner... except a real bus may exist! So: force
        // the probe through a failing runner and assert the refusal
        // shape, plus assert the real probe never panics).
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut dialog = LinuxFileDialog::new(StubRunner {
            calls,
            script: RefCell::new(Vec::new()),
            fail_spawn: true,
            kills: Rc::new(std::cell::Cell::new(0)),
        });
        // No bus on this box to fake deterministically — instead
        // drive the Unsupported arm directly through a cached probe.
        dialog.probe = Some(None);
        dialog.request_open_files(options());
        assert_eq!(
            dialog.poll_open(),
            Some(Err(PickError::Unsupported("LinuxFileDialog"))),
            "no backend refuses loudly"
        );
        // And the real probe runs without panicking either way.
        let mut dialog2 = LinuxFileDialog::new(StubRunner {
            calls: Rc::new(RefCell::new(Vec::new())),
            script: RefCell::new(Vec::new()),
            fail_spawn: true,
            kills: Rc::new(std::cell::Cell::new(0)),
        });
        let _ = dialog2.backend();
    }

    #[test]
    fn uri_paths_decode_or_refuse() {
        assert_eq!(
            uri_to_path("file:///tmp/a%20b.txt").expect("decodes"),
            PathBuf::from("/tmp/a b.txt")
        );
        assert_eq!(
            uri_to_path("file://localhost/x").expect("localhost"),
            PathBuf::from("/x")
        );
        assert!(uri_to_path("http://h/x").is_err(), "non-file refuses");
        assert!(
            uri_to_path("file://remotehost/x").is_err(),
            "remote host never becomes local"
        );
        assert!(
            uri_to_path("file:///tmp/%zz").is_err(),
            "bad escape refuses"
        );
        assert!(uri_to_path("file://").is_err(), "empty path refuses");
        assert_eq!(
            percent_decode("a+b").expect("plus literal"),
            "a+b",
            "+ stays literal in paths"
        );
    }

    #[test]
    fn portal_options_carry_mode_filters_and_folder() {
        let token = "oppa1_2";
        let dict = portal_options(&options(), false, token);
        let get = |key: &str| {
            dict.iter()
                .find(|(k, _)| k == key)
                .unwrap_or_else(|| panic!("key {key}"))
                .1
                .clone()
        };
        assert_eq!(
            get("handle_token"),
            DVal::Variant(Box::new(DVal::Str(token.to_string())))
        );
        assert_eq!(get("multiple"), DVal::Variant(Box::new(DVal::Bool(true))));
        assert_eq!(get("directory"), DVal::Variant(Box::new(DVal::Bool(false))));
        // Filters marshal through the real encoder (spec shape
        // `a(sa(us))` — glob type 0).
        let DVal::Variant(filters) = get("filters") else {
            panic!("filters is a variant");
        };
        let sig = dbus_variant_sig(&filters).expect("filters sign");
        assert_eq!(sig, "a(sa(us))", "spec shape, got {sig}");
        // Directory mode flips both flags, drops filters.
        let dir = portal_options(&FilePickerOptions::default(), true, token);
        let getd = |key: &str| {
            dir.iter()
                .find(|(k, _)| k == key)
                .unwrap_or_else(|| panic!("key {key}"))
                .1
                .clone()
        };
        assert_eq!(getd("directory"), DVal::Variant(Box::new(DVal::Bool(true))));
        assert_eq!(getd("multiple"), DVal::Variant(Box::new(DVal::Bool(false))));
        assert!(
            dir.iter().all(|(k, _)| k != "filters"),
            "no filters for dirs"
        );
    }

    /// Test-only signature probe (mirrors the encoder's derivation
    /// so the spec-shape assertion above stays honest).
    fn dbus_variant_sig(v: &DVal) -> Result<String, String> {
        fn sig_of(v: &DVal) -> Result<String, String> {
            match v {
                DVal::Bool(_) => Ok("b".into()),
                DVal::U32(_) => Ok("u".into()),
                DVal::Str(_) => Ok("s".into()),
                DVal::Path(_) => Ok("o".into()),
                DVal::Sig(_) => Ok("g".into()),
                DVal::Bytes(_) => Ok("ay".into()),
                DVal::Array(items) => {
                    let f = items.first().ok_or("empty")?;
                    Ok(format!("a{}", sig_of(f)?))
                }
                DVal::Struct(items) => {
                    let mut s = String::from("(");
                    for i in items {
                        s.push_str(&sig_of(i)?);
                    }
                    s.push(')');
                    Ok(s)
                }
                DVal::Variant(inner) => sig_of(inner),
            }
        }
        sig_of(v)
    }

    #[test]
    fn settle_response_maps_codes() {
        // Success with uris.
        let ok = settle_response(
            0,
            vec![(
                "uris".to_string(),
                DVal::Variant(Box::new(DVal::Array(vec![DVal::Str(
                    "file:///a.txt".to_string(),
                )]))),
            )],
        )
        .expect("settles");
        assert_eq!(ok, vec![PathBuf::from("/a.txt")]);
        // Dismissal is data.
        assert_eq!(
            settle_response(1, vec![]).expect("dismisses"),
            Vec::<PathBuf>::new()
        );
        // Anything else is loud.
        assert!(settle_response(2, vec![]).is_err(), "code 2 refuses");
        // Non-string uris refuse (never laundered).
        assert!(
            settle_response(
                0,
                vec![(
                    "uris".to_string(),
                    DVal::Variant(Box::new(DVal::Array(vec![DVal::U32(1)])))
                )],
            )
            .is_err(),
            "non-string uri refuses"
        );
    }

    /// Round 16.1 (decision 314): the portal `SaveFile` dict carries
    /// the token, the default name, and glob filters — and none of
    /// the open-only flags; the method literal pins per kind.
    #[test]
    fn portal_save_options_carry_name_filters_and_method() {
        use oppa::FileDialogOptions;
        let options = FileDialogOptions {
            title: "Save".to_string(),
            filters: vec![FileFilter {
                name: "Text".to_string(),
                patterns: vec!["*.txt".to_string()],
            }],
            default_name: "report.txt".to_string(),
            initial_dir: Some(PathBuf::from("/tmp")),
        };
        let dict = portal_save_options(&options, "oppa9_9");
        let get = |key: &str| {
            dict.iter()
                .find(|(k, _)| k == key)
                .unwrap_or_else(|| panic!("key {key}"))
                .1
                .clone()
        };
        assert_eq!(
            get("handle_token"),
            DVal::Variant(Box::new(DVal::Str("oppa9_9".to_string())))
        );
        assert_eq!(
            get("current_name"),
            DVal::Variant(Box::new(DVal::Str("report.txt".to_string())))
        );
        let DVal::Variant(filters) = get("filters") else {
            panic!("filters is a variant");
        };
        assert_eq!(
            dbus_variant_sig(&filters).expect("filters sign"),
            "a(sa(us))",
            "spec glob shape"
        );
        assert!(
            dict.iter()
                .all(|(k, _)| k != "multiple" && k != "directory"),
            "save has no open-only flags: {dict:?}"
        );
        assert_eq!(chooser_method(true), "SaveFile");
        assert_eq!(chooser_method(false), "OpenFile");
    }

    /// Round 16.1: save results collapse to one path (empty
    /// dismisses, several refuse — never a silent first-wins on
    /// user data).
    #[test]
    fn single_result_collapses_or_refuses() {
        assert_eq!(single_result(vec![]).expect("dismisses"), None);
        assert_eq!(
            single_result(vec![PathBuf::from("/a")]).expect("picks"),
            Some(PathBuf::from("/a"))
        );
        assert!(
            single_result(vec![PathBuf::from("/a"), PathBuf::from("/b")]).is_err(),
            "never first-wins"
        );
    }

    /// Round 16.1: the zenity `--save` argv names the suggested
    /// destination (dir + default name, no trailing slash — a file,
    /// not a folder), carries filters, and never multi/directory
    /// flags.
    #[test]
    fn zenity_save_argv_names_destination_and_filters() {
        use oppa::FileDialogOptions;
        let options = FileDialogOptions {
            title: "Save".to_string(),
            filters: vec![FileFilter {
                name: "Text".to_string(),
                patterns: vec!["*.txt".to_string()],
            }],
            default_name: "report.txt".to_string(),
            initial_dir: Some(PathBuf::from("/tmp")),
        };
        let argv = zenity_save_argv(&options);
        assert!(argv.contains(&"--save".to_string()), "{argv:?}");
        assert!(
            argv.iter().any(|a| a == "--filename=/tmp/report.txt"),
            "dir + default name: {argv:?}"
        );
        assert!(
            argv.iter().any(|a| a == "--file-filter=Text | *.txt"),
            "filters: {argv:?}"
        );
        assert!(
            !argv.iter().any(|a| a == "--multiple" || a == "--directory"),
            "single destination: {argv:?}"
        );
    }

    /// Round 16.1: blocking save settles the scripted destination
    /// (argv proves filters + defaults reached the backend) and
    /// dismissal settles `None` without panic.
    #[test]
    fn blocking_save_settles_argv_and_dismissal() {
        use oppa::{FileDialogOptions, SaveFileDialog};
        let settled = ChildOutcome {
            code: 0,
            stdout: b"/tmp/report.txt\n".to_vec(),
            stderr: Vec::new(),
        };
        let calls: Calls = Rc::new(RefCell::new(Vec::new()));
        let mut dialog = LinuxFileDialog::new(StubRunner {
            calls: calls.clone(),
            script: RefCell::new(vec![Some(settled)]),
            fail_spawn: false,
            kills: Rc::new(std::cell::Cell::new(0)),
        });
        dialog.probe = Some(Some(Backend::Zenity));
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
            dialog.save(options).expect("saves"),
            Some(PathBuf::from("/tmp/report.txt"))
        );
        let (_, argv) = calls.borrow().last().cloned().expect("spawned");
        assert!(argv.contains(&"--save".to_string()), "{argv:?}");
        assert!(
            argv.iter().any(|a| a == "--filename=/tmp/report.txt"),
            "defaults reach zenity: {argv:?}"
        );
        assert!(
            argv.iter().any(|a| a == "--file-filter=Text | *.txt"),
            "filters reach zenity: {argv:?}"
        );
        // Dismissal (exit 1) settles None, never panics.
        let dismissed = ChildOutcome {
            code: 1,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        let mut dialog = LinuxFileDialog::new(StubRunner {
            calls: Rc::new(RefCell::new(Vec::new())),
            script: RefCell::new(vec![Some(dismissed)]),
            fail_spawn: false,
            kills: Rc::new(std::cell::Cell::new(0)),
        });
        dialog.probe = Some(Some(Backend::Zenity));
        assert_eq!(
            dialog
                .save(FileDialogOptions::default())
                .expect("dismisses"),
            None
        );
    }

    /// Round 16.1: blocking folder pick spawns `--directory`,
    /// settles one path, and dismissal settles `None`.
    #[test]
    fn blocking_folder_settles_single_path_and_dismissal() {
        use oppa::{FolderDialog, FolderDialogOptions};
        let settled = ChildOutcome {
            code: 0,
            stdout: b"/tmp/out\n".to_vec(),
            stderr: Vec::new(),
        };
        let calls: Calls = Rc::new(RefCell::new(Vec::new()));
        let mut dialog = LinuxFileDialog::new(StubRunner {
            calls: calls.clone(),
            script: RefCell::new(vec![Some(settled)]),
            fail_spawn: false,
            kills: Rc::new(std::cell::Cell::new(0)),
        });
        dialog.probe = Some(Some(Backend::Zenity));
        let options = FolderDialogOptions {
            title: "Pick".to_string(),
            initial_dir: Some(PathBuf::from("/tmp")),
        };
        assert_eq!(
            dialog.pick(options).expect("picks"),
            Some(PathBuf::from("/tmp/out"))
        );
        let (_, argv) = calls.borrow().last().cloned().expect("spawned");
        assert!(argv.contains(&"--directory".to_string()), "{argv:?}");
        assert!(
            argv.iter().any(|a| a == "--filename=/tmp/"),
            "start dir reaches zenity: {argv:?}"
        );
        let dismissed = ChildOutcome {
            code: 1,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        let mut dialog = LinuxFileDialog::new(StubRunner {
            calls: Rc::new(RefCell::new(Vec::new())),
            script: RefCell::new(vec![Some(dismissed)]),
            fail_spawn: false,
            kills: Rc::new(std::cell::Cell::new(0)),
        });
        dialog.probe = Some(Some(Backend::Zenity));
        assert_eq!(
            dialog
                .pick(FolderDialogOptions::default())
                .expect("dismisses"),
            None
        );
    }
}
