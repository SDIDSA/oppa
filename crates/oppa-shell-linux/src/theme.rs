//! Linux OS theme source (Round 16.2, decision 315): the
//! `org.freedesktop.appearance color-scheme` setting over the
//! minimal [`crate::dbus`] client, plus a `SettingChanged` watcher
//! thread, behind [`SystemThemeSource`](oppa::SystemThemeSource).
//!
//! Portal spec values: `0` = no preference, `1` = prefer dark,
//! `2` = prefer light. No-preference maps to `None` (the app
//! default stands — never a guessed mode). No bus, no portal, or a
//! missing key maps to `None` the same way (a headless box has no
//! theme to read — refusal-shaped, never a silent Light).
//!
//! Live updates ride [`LinuxThemeWatcher`] (own bus, own thread —
//! the file-dialog worker precedent): it pushes one unit per
//! appearance/color-scheme change and the runner re-queries
//! through the installed source, so there is exactly one
//! application path, never two.

use oppa::{SystemThemeSource, ThemeMode};

use crate::dbus::{self, DVal};

/// Portal `color-scheme` value → mode (pure — headless-tested):
/// `1` reads dark, `2` reads light, anything else (including the
/// `0` no-preference) reads unknown.
pub fn color_scheme_to_theme(value: u32) -> Option<ThemeMode> {
    match value {
        1 => Some(ThemeMode::Dark),
        2 => Some(ThemeMode::Light),
        _ => None,
    }
}

/// One-shot `Settings.Read("org.freedesktop.appearance",
/// "color-scheme")` over a throwaway bus connection (bounded —
/// connect and call deadlines, never an unbounded UI hang).
/// `None` on every failure path (no bus, no portal, bus error,
/// misshapen reply — unknown stays unknown).
pub fn read_color_scheme() -> Option<u32> {
    let addr = dbus::default_address().ok()?;
    // Connect like the file-dialog worker (each transport
    // monomorphizes the same query below).
    match addr {
        dbus::BusAddr::UnixPath(p) => {
            #[cfg(unix)]
            {
                let conn = dbus::connect_unix(&p, std::time::Duration::from_secs(3)).ok()?;
                query_color_scheme(conn)
            }
            #[cfg(not(unix))]
            {
                let _ = p;
                None
            }
        }
        #[cfg(unix)]
        dbus::BusAddr::UnixAbstract(name) => {
            let conn = dbus::connect_abstract(&name, std::time::Duration::from_secs(3)).ok()?;
            query_color_scheme(conn)
        }
        dbus::BusAddr::Tcp { host, port } => {
            let conn = dbus::connect_tcp(&host, port, std::time::Duration::from_secs(3)).ok()?;
            query_color_scheme(conn)
        }
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

/// Runs the `Settings.Read` call on a connected bus (the per-kind
/// reply shape: one variant holding the `u32`).
fn query_color_scheme<T: dbus::BusTransport>(mut conn: dbus::BusConn<T>) -> Option<u32> {
    let mut stashed = Vec::new();
    let reply = conn
        .call(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
            "Read",
            "ss",
            &[
                DVal::Str("org.freedesktop.appearance".to_string()),
                DVal::Str("color-scheme".to_string()),
            ],
            &mut stashed,
            std::time::Duration::from_secs(5),
        )
        .ok()?;
    let vals = dbus::parse_body(&reply.body, &reply.sig).ok()?;
    match vals.as_slice() {
        [DVal::Variant(inner)] => match inner.as_ref() {
            DVal::U32(v) => Some(*v),
            _ => None,
        },
        _ => None,
    }
}

/// Current OS theme from the portal (`None` when unknown).
pub fn system_theme() -> Option<ThemeMode> {
    color_scheme_to_theme(read_color_scheme()?)
}

/// Stateless portal reader (the runner installs one instance;
/// every query re-reads — no cached staleness by construction).
pub struct LinuxSystemTheme;

impl SystemThemeSource for LinuxSystemTheme {
    fn system_theme(&mut self) -> Option<ThemeMode> {
        system_theme()
    }
}

/// True when a parsed `SettingChanged` body announces an
/// appearance/color-scheme change (pure over `DVal` —
/// headless-tested with literals; the thread parses wire through
/// this, one parser, never two).
pub(crate) fn is_appearance_change(vals: &[DVal]) -> bool {
    match vals {
        [DVal::Str(ns), DVal::Str(key), DVal::Variant(_)] => {
            ns == "org.freedesktop.appearance" && key == "color-scheme"
        }
        _ => false,
    }
}

enum WatcherCmd {
    Shutdown,
}

/// Live theme-change watcher (the file-dialog worker precedent —
/// own bus, own thread, never the UI thread): subscribes to
/// `org.freedesktop.portal.Settings SettingChanged` once and
/// pushes one unit per appearance/color-scheme change. The runner
/// drains it ([`poll_change`](Self::poll_change)) and re-queries
/// through the installed source. No bus at spawn (or a bus that
/// dies) ends the thread quietly — the startup query already
/// covered the theme, and a dead watcher degrades to it.
pub struct LinuxThemeWatcher {
    rx: std::sync::mpsc::Receiver<()>,
    tx: std::sync::mpsc::Sender<WatcherCmd>,
    #[allow(dead_code)]
    thread: Option<std::thread::JoinHandle<()>>,
}

impl LinuxThemeWatcher {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<WatcherCmd>();
        let (res_tx, res_rx) = std::sync::mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("oppa-theme".to_string())
            .spawn(move || watcher_main(cmd_rx, res_tx))
            .ok();
        Self {
            rx: res_rx,
            tx: cmd_tx,
            thread,
        }
    }

    /// Non-blocking drain: `true` when the OS theme changed since
    /// the last drain (edge — callers re-query the source).
    pub fn poll_change(&mut self) -> bool {
        let mut changed = false;
        while self.rx.try_recv().is_ok() {
            changed = true;
        }
        changed
    }
}

impl Drop for LinuxThemeWatcher {
    /// Best-effort shutdown (never blocking, never joining — a live
    /// watcher exits on the command; a dead one is already gone).
    fn drop(&mut self) {
        let _ = self.tx.send(WatcherCmd::Shutdown);
    }
}

impl Default for LinuxThemeWatcher {
    /// Default watcher (spawns like [`new`](Self::new) — the
    /// `new_without_default` rule).
    fn default() -> Self {
        Self::new()
    }
}

/// Watcher thread body: subscribe once, then pump signals at
/// 100 ms interleaved with command drains (the portal await
/// precedent — user-driven dialogs block there as long as they
/// must; theme signals arrive whenever they arrive).
fn watcher_main(rx: std::sync::mpsc::Receiver<WatcherCmd>, tx: std::sync::mpsc::Sender<()>) {
    let addr = match dbus::default_address() {
        Ok(a) => a,
        Err(_) => return,
    };
    match addr {
        dbus::BusAddr::UnixPath(p) => {
            #[cfg(unix)]
            match dbus::connect_unix(&p, std::time::Duration::from_secs(5)) {
                Ok(conn) => watch_loop(conn, rx, tx),
                Err(_) => {}
            }
            #[cfg(not(unix))]
            {
                let _ = (p, rx, tx);
            }
        }
        #[cfg(unix)]
        dbus::BusAddr::UnixAbstract(name) => {
            match dbus::connect_abstract(&name, std::time::Duration::from_secs(5)) {
                Ok(conn) => watch_loop(conn, rx, tx),
                Err(_) => {}
            }
        }
        dbus::BusAddr::Tcp { host, port } => {
            if let Ok(conn) = dbus::connect_tcp(&host, port, std::time::Duration::from_secs(5)) {
                watch_loop(conn, rx, tx);
            }
        }
        #[allow(unreachable_patterns)]
        _ => {}
    }
}

/// Signal pump on a connected bus (transport-generic — one loop
/// for every address kind).
fn watch_loop<T: dbus::BusTransport>(
    mut conn: dbus::BusConn<T>,
    rx: std::sync::mpsc::Receiver<WatcherCmd>,
    tx: std::sync::mpsc::Sender<()>,
) {
    let mut stashed = Vec::new();
    if conn
        .call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "AddMatch",
            "s",
            &[DVal::Str(
                "type='signal',interface='org.freedesktop.portal.Settings',member='SettingChanged'"
                    .to_string(),
            )],
            &mut stashed,
            std::time::Duration::from_secs(5),
        )
        .is_err()
    {
        return;
    }
    loop {
        match rx.try_recv() {
            Ok(WatcherCmd::Shutdown) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
        match conn.read_message("theme", std::time::Duration::from_millis(100)) {
            Ok(msg) => {
                if msg.mtype == dbus::MSG_SIGNAL
                    && msg.iface.as_deref() == Some("org.freedesktop.portal.Settings")
                    && msg.member.as_deref() == Some("SettingChanged")
                {
                    if let Ok(vals) = dbus::parse_body(&msg.body, &msg.sig) {
                        if is_appearance_change(&vals) {
                            let _ = tx.send(());
                        }
                    }
                } else if msg.mtype == dbus::MSG_SIGNAL {
                    stashed.push(msg);
                }
            }
            Err(e) => {
                // Read timeouts are the pump tick (signals arrive
                // whenever they arrive); a dead bus ends the watch
                // quietly (the file-dialog worker precedent — the
                // startup query already covered the theme).
                if e.contains("timed out") {
                    continue;
                }
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_scheme_values_map_to_modes() {
        assert_eq!(
            color_scheme_to_theme(0),
            None,
            "no preference reads unknown"
        );
        assert_eq!(color_scheme_to_theme(1), Some(ThemeMode::Dark));
        assert_eq!(color_scheme_to_theme(2), Some(ThemeMode::Light));
        assert_eq!(
            color_scheme_to_theme(99),
            None,
            "future values read unknown"
        );
    }

    #[test]
    fn appearance_signals_match_namespace_and_key() {
        let hit = vec![
            DVal::Str("org.freedesktop.appearance".to_string()),
            DVal::Str("color-scheme".to_string()),
            DVal::Variant(Box::new(DVal::U32(1))),
        ];
        assert!(is_appearance_change(&hit));
        let wrong_key = vec![
            DVal::Str("org.freedesktop.appearance".to_string()),
            DVal::Str("accent-color".to_string()),
            DVal::Variant(Box::new(DVal::U32(1))),
        ];
        assert!(!is_appearance_change(&wrong_key));
        let wrong_ns = vec![
            DVal::Str("org.freedesktop.background".to_string()),
            DVal::Str("color-scheme".to_string()),
            DVal::Variant(Box::new(DVal::U32(1))),
        ];
        assert!(!is_appearance_change(&wrong_ns));
        assert!(!is_appearance_change(&[]), "empty never matches");
        assert!(!is_appearance_change(&hit[..2]), "short bodies never match");
    }

    #[test]
    fn watcher_without_a_bus_stays_quiet() {
        // No bus on the headless box (and a graceful degrade
        // wherever one is unreachable): spawns, polls false,
        // drops — all without panic.
        let mut watcher = LinuxThemeWatcher::new();
        assert!(!watcher.poll_change());
    }

    #[test]
    fn live_query_never_panics() {
        // Whatever this box reports (dark / light / unknown) must
        // arrive without panic — the query is advisory, not a gate
        // (the file-dialog probe precedent).
        let _ = read_color_scheme();
        let _ = system_theme();
    }
}
