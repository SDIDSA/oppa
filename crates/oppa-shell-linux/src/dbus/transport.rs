use super::*;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Transport (generic stream + AUTH + Hello)
// ---------------------------------------------------------------------------

/// A D-Bus byte stream with read deadlines (Unix socket in
/// production, loopback TCP in tests — same framing either way).
pub(crate) trait BusTransport: Read + Write {
    fn set_read_deadline(&mut self, timeout: Option<Duration>) -> std::io::Result<()>;
}

impl BusTransport for std::net::TcpStream {
    fn set_read_deadline(&mut self, timeout: Option<Duration>) -> std::io::Result<()> {
        self.set_read_timeout(timeout)
    }
}

#[cfg(unix)]
impl BusTransport for std::os::unix::net::UnixStream {
    fn set_read_deadline(&mut self, timeout: Option<Duration>) -> std::io::Result<()> {
        self.set_read_timeout(timeout)
    }
}

/// Parsed session-bus address (only the transports we speak).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BusAddr {
    UnixPath(String),
    #[cfg(unix)]
    UnixAbstract(Vec<u8>),
    Tcp {
        host: String,
        port: u16,
    },
}

/// Parses `DBUS_SESSION_BUS_ADDRESS` values (first `;`-separated
/// entry wins; `guid=` suffixes ignored). Only `unix:` (path /
/// abstract) and `tcp:` parse — anything else refuses loudly.
pub(crate) fn parse_address(addr: &str) -> Result<BusAddr, String> {
    let first = addr.split(';').next().unwrap_or("").trim();
    let (transport, params) = first
        .split_once(':')
        .ok_or(format!("bus address without transport: {addr:?}"))?;
    let mut path: Option<String> = None;
    let mut abstract_name: Option<String> = None;
    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;
    for kv in params.split(',') {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        match (transport, k) {
            ("unix", "path") => path = Some(v.to_string()),
            ("unix", "abstract") => abstract_name = Some(v.to_string()),
            ("tcp", "host") => host = Some(v.to_string()),
            ("tcp", "port") => {
                port = Some(
                    v.parse::<u16>()
                        .map_err(|_| format!("bad tcp port {v:?}"))?,
                );
            }
            _ => {}
        }
    }
    match transport {
        "unix" => {
            if let Some(p) = path {
                Ok(BusAddr::UnixPath(p))
            } else if let Some(a) = abstract_name {
                #[cfg(unix)]
                {
                    Ok(BusAddr::UnixAbstract(a.into_bytes()))
                }
                #[cfg(not(unix))]
                {
                    let _ = a;
                    Err("abstract sockets need unix".to_string())
                }
            } else {
                Err(format!("unix bus address without path/abstract: {addr:?}"))
            }
        }
        "tcp" => match (host, port) {
            (Some(h), Some(p)) => Ok(BusAddr::Tcp { host: h, port: p }),
            _ => Err(format!("tcp bus address needs host+port: {addr:?}")),
        },
        other => Err(format!("unsupported bus transport {other:?}")),
    }
}

/// Default session-bus address (env first, then the per-uid
/// runtime path — the standard resolution, no guessing).
pub(crate) fn default_address() -> Result<BusAddr, String> {
    if let Ok(env) = std::env::var("DBUS_SESSION_BUS_ADDRESS") {
        if !env.trim().is_empty() {
            return parse_address(&env);
        }
    }
    #[cfg(unix)]
    {
        let uid = unsafe { libc::getuid() };
        Ok(BusAddr::UnixPath(format!("/run/user/{uid}/bus")))
    }
    #[cfg(not(unix))]
    {
        Err("no DBUS_SESSION_BUS_ADDRESS and no unix runtime dir".to_string())
    }
}

/// AUTH identity hex (EXTERNAL mechanism — the daemon confirms
/// against the socket peer credentials).
fn auth_identity() -> String {
    #[cfg(unix)]
    {
        let uid = unsafe { libc::getuid() };
        format!("{uid:x}")
    }
    #[cfg(not(unix))]
    {
        String::new()
    }
}

/// A connected, authed, Hello'd bus with a serial counter.
pub(crate) struct BusConn<T: BusTransport> {
    stream: T,
    sender: String,
    serial: u32,
}

impl<T: BusTransport> BusConn<T> {
    /// Reads one `\r\n`-terminated line (AUTH handshake phase).
    fn read_line(&mut self, what: &str, timeout: Duration) -> Result<String, String> {
        self.stream
            .set_read_deadline(Some(timeout))
            .map_err(|e| format!("{what}: deadline arm failed: {e}"))?;
        let mut line = Vec::new();
        let start = Instant::now();
        loop {
            if start.elapsed() > timeout + Duration::from_secs(1) {
                return Err(format!("{what}: line wait timed out"));
            }
            let mut one = [0u8; 1];
            match self.stream.read_exact(&mut one) {
                Ok(()) => {
                    line.push(one[0]);
                    if line.len() >= 2 && line[line.len() - 2..] == *b"\r\n" {
                        line.truncate(line.len() - 2);
                        break;
                    }
                    if line.len() > 4096 {
                        return Err(format!("{what}: line too long"));
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    return Err(format!("{what}: timed out ({e})"));
                }
                Err(e) => return Err(format!("{what}: read failed: {e}")),
            }
        }
        String::from_utf8(line).map_err(|e| format!("{what}: non-UTF8 line: {e}"))
    }

    fn write_all(&mut self, what: &str, bytes: &[u8]) -> Result<(), String> {
        self.stream
            .write_all(bytes)
            .map_err(|e| format!("{what}: write failed: {e}"))?;
        self.stream
            .flush()
            .map_err(|e| format!("{what}: flush failed: {e}"))?;
        Ok(())
    }

    /// Reads exactly `n` bytes (framing reads — bounded by the
    /// caller-supplied deadline via the stream timeout).
    pub fn read_exact(
        &mut self,
        what: &str,
        buf: &mut [u8],
        timeout: Duration,
    ) -> Result<(), String> {
        self.stream
            .set_read_deadline(Some(timeout))
            .map_err(|e| format!("{what}: deadline arm failed: {e}"))?;
        self.stream.read_exact(buf).map_err(|e| match e.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
                format!("{what}: timed out")
            }
            _ => format!("{what}: read failed: {e}"),
        })
    }

    /// Reads one full message (header → fields → body, sizes from
    /// the wire — never more than the message itself).
    pub fn read_message(&mut self, what: &str, timeout: Duration) -> Result<InMsg, String> {
        let mut head = [0u8; 16];
        self.read_exact(what, &mut head, timeout)?;
        // Fields length lives at bytes 12..16 (both orders agree on
        // the offsets; the length itself parses after order detect —
        // re-parse cheaply: field length is order-sensitive, so peek
        // via parse_message on a growing buffer is simpler. Instead:
        // read the fields length by completing through parse in two
        // steps is over-engineering — read a bounded chunk and parse.
        let mut buf = head.to_vec();
        // Parse progressively: 16 bytes give the order + field
        // length only after full header parse, which needs the
        // fields... circular, so: detect order from byte 0, read
        // the u32 at 12..16 in that order, then complete.
        let le = match buf[0] {
            b'l' => true,
            b'B' => false,
            o => return Err(format!("{what}: bad byte order {o:#04x}")),
        };
        let fl = {
            let b = [buf[12], buf[13], buf[14], buf[15]];
            (if le {
                u32::from_le_bytes(b)
            } else {
                u32::from_be_bytes(b)
            }) as usize
        };
        if fl > 1024 * 1024 {
            return Err(format!("{what}: absurd fields length {fl}"));
        }
        let fields_end = 16 + fl;
        buf.resize(fields_end, 0);
        self.read_exact(what, &mut buf[16..], timeout)?;
        // Body length needs the header parse — parse header-only by
        // reusing parse_message on fields + zero body... simpler:
        // body length = total - body_start, but total is unknown
        // until we know body_len, which lives... actually body_len
        // is header bytes 4..8 (fixed offset!). Read it directly.
        let body_len = {
            let b = [buf[4], buf[5], buf[6], buf[7]];
            (if le {
                u32::from_le_bytes(b)
            } else {
                u32::from_be_bytes(b)
            }) as usize
        };
        if body_len > 128 * 1024 * 1024 {
            return Err(format!("{what}: absurd body length {body_len}"));
        }
        let body_start = fields_end + (8 - fields_end % 8) % 8;
        buf.resize(body_start + body_len, 0);
        self.read_exact(what, &mut buf[fields_end..], timeout)?;
        let (msg, _) = parse_message(&buf)?;
        Ok(msg)
    }

    /// Sends one AUTH line + BEGIN handshake over an already-open
    /// stream, then Hello — returning the daemon-assigned sender
    /// name. `timeout` bounds the daemon round-trips (local bus:
    /// milliseconds; dead bus refuses instead of hanging).
    pub fn handshake(stream: T, timeout: Duration) -> Result<Self, String> {
        let mut conn = Self {
            stream,
            sender: String::new(),
            serial: 0,
        };
        conn.write_all(
            "auth",
            format!("AUTH EXTERNAL {}\r\n", auth_identity()).as_bytes(),
        )?;
        let line = conn.read_line("auth", timeout)?;
        if !line.starts_with("OK") {
            return Err(format!("bus AUTH refused: {line:?}"));
        }
        conn.write_all("begin", b"BEGIN\r\n")?;
        let mut stashed = Vec::new();
        let reply = conn.call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "Hello",
            "",
            &[],
            &mut stashed,
            timeout,
        )?;
        if !stashed.is_empty() {
            return Err("bus sent signals before Hello completed".to_string());
        }
        let vals = parse_body(&reply.body, &reply.sig).map_err(|e| format!("hello: {e}"))?;
        let [DVal::Str(name)] = vals.as_slice() else {
            return Err(format!("hello: unexpected body {vals:?}"));
        };
        conn.sender = name.clone();
        Ok(conn)
    }

    /// One method call (skips interleaved signals into `stashed`
    /// for the caller to drain — signals are never dropped
    /// silently). Returns the reply message. Nine parameters because
    /// a D-Bus call is nine things (route ×4, body ×2, stash,
    /// deadline) — a params struct buys nothing at one call shape.
    #[allow(clippy::too_many_arguments)]
    pub fn call(
        &mut self,
        dest: &str,
        path: &str,
        iface: &str,
        member: &str,
        body_sig: &str,
        body: &[DVal],
        stashed: &mut Vec<InMsg>,
        timeout: Duration,
    ) -> Result<InMsg, String> {
        self.serial += 1;
        if self.serial == 0 {
            self.serial = 1;
        }
        let bytes = build_method_call(self.serial, dest, path, iface, member, body_sig, body)?;
        self.write_all(member, &bytes)?;
        loop {
            let msg = self.read_message(member, timeout)?;
            match msg.mtype {
                MSG_METHOD_RETURN if msg.reply_to == Some(self.serial) => return Ok(msg),
                MSG_ERROR if msg.reply_to == Some(self.serial) => {
                    let detail = msg
                        .error_name
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());
                    return Err(format!("{member}: bus error {detail}"));
                }
                MSG_SIGNAL => stashed.push(msg),
                _ => {}
            }
        }
    }

    pub fn sender(&self) -> &str {
        &self.sender
    }
}

/// Connects + handshakes a Unix-socket bus (production path).
#[cfg(unix)]
pub(crate) fn connect_unix(
    path: &str,
    timeout: Duration,
) -> Result<BusConn<std::os::unix::net::UnixStream>, String> {
    let stream = std::os::unix::net::UnixStream::connect(path)
        .map_err(|e| format!("bus socket {path:?}: {e}"))?;
    BusConn::handshake(stream, timeout)
}

/// Connects + handshakes an abstract-socket bus (abstract
/// namespaces are Linux-only — other unix targets refuse loudly).
#[cfg(unix)]
pub(crate) fn connect_abstract(
    name: &[u8],
    timeout: Duration,
) -> Result<BusConn<std::os::unix::net::UnixStream>, String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::linux::net::SocketAddrExt as _;
        let addr = std::os::unix::net::SocketAddr::from_abstract_name(name)
            .map_err(|e| format!("abstract bus name: {e}"))?;
        let stream = std::os::unix::net::UnixStream::connect_addr(&addr)
            .map_err(|e| format!("abstract bus connect: {e}"))?;
        BusConn::handshake(stream, timeout)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (name, timeout);
        Err("abstract bus sockets need Linux".to_string())
    }
}

/// Connects + handshakes a TCP bus (tests use loopback pairs; rare
/// production `tcp:` addresses share the path).
pub(crate) fn connect_tcp(
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<BusConn<std::net::TcpStream>, String> {
    let stream = std::net::TcpStream::connect((host, port))
        .map_err(|e| format!("tcp bus {host}:{port}: {e}"))?;
    BusConn::handshake(stream, timeout)
}
