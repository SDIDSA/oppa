use std::path::PathBuf;

use oppa::{FileDialogOptions, FilePickerOptions, PickError};

use crate::dbus::{self, BusConn, BusTransport, DVal};

// ---------------------------------------------------------------------------
// Portal pure shapes (URIs, options dicts, methods — headless-tested)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Portal worker (blocking bus pump on its own thread — the UI thread
// never blocks on a user-driven dialog; G7 precedent)
// ---------------------------------------------------------------------------

/// One portal open request for the worker.
pub(crate) struct OpenJob {
    pub(crate) options: FilePickerOptions,
    pub(crate) directory: bool,
    pub(crate) generation: u64,
}

/// One portal save request for the worker (Round 16.1 — same
/// request/response lifecycle as [`OpenJob`], one destination).
pub(crate) struct SaveJob {
    pub(crate) options: FileDialogOptions,
    pub(crate) generation: u64,
}

pub(crate) enum WorkerCmd {
    Open(OpenJob),
    Save(SaveJob),
    Shutdown,
}

pub(crate) struct WorkerResult {
    pub(crate) generation: u64,
    /// True for save responses (Round 16.1 — open and save share
    /// one generation counter + channel, so results route by kind;
    /// stale generations drop either way).
    pub(crate) save: bool,
    pub(crate) result: Result<Vec<PathBuf>, PickError>,
}

/// Response codes (portal convention: 0 picked, 1 dismissed,
/// anything else loud).
pub(crate) fn settle_response(
    code: u32,
    results: Vec<(String, DVal)>,
) -> Result<Vec<PathBuf>, PickError> {
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

pub(crate) struct PortalWorker {
    pub(crate) tx: std::sync::mpsc::Sender<WorkerCmd>,
    pub(crate) rx: std::sync::mpsc::Receiver<WorkerResult>,
    #[allow(dead_code)]
    pub(crate) thread: Option<std::thread::JoinHandle<()>>,
}

impl PortalWorker {
    /// Spawns the portal worker thread (lazy — a headless box never pays
    /// for the thread; the worker connects inside its own thread so a
    /// missing bus never blocks the UI). `None` when no bus address
    /// exists or the thread would not start.
    pub(crate) fn spawn() -> Option<Self> {
        let addr = match dbus::default_address() {
            Ok(a) => a,
            Err(_) => return None,
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
            Ok(thread) => Some(PortalWorker {
                tx: cmd_tx,
                rx: res_rx,
                thread: Some(thread),
            }),
            Err(_) => None,
        }
    }
}

/// True when a portal bus is dialable right now (socket exists —
/// no full handshake on the probe path; the worker handshakes for
/// real on first use).
pub(crate) fn portal_reachable() -> bool {
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
