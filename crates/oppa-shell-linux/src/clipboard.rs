//! Linux system clipboard (Round 2.2, OQ-G3-1): the X11 CLIPBOARD
//! selection through `x11rb` (pure Rust, already in the tree — no new
//! packages, no C headers, same dlopen story as the rest of this
//! shell).
//!
//! Ownership model (ICCCM selections, stated bounds):
//!
//! - Writes take selection ownership with our hidden window and keep
//!   the text in `owned_text`. Reads first verify ownership (one
//!   round-trip, catches inter-pump takeovers): ours → instant,
//!   theirs → requestor path with INCR receive.
//! - Another app pasting our text sends `SelectionRequestEvent` to our
//!   window; [`LinuxClipboard::service`] (the [`Clipboard::service`]
//!   hook, pumped from `run_linux`'s idle path) answers `TARGETS` /
//!   `UTF8_STRING` / refusal, with an INCR-send state machine past
//!   [`INCR_THRESHOLD`]. While we own the selection the runner wakes
//!   periodically so a peer's paste never hangs on us.
//! - Payloads over [`INCR_THRESHOLD`] chunk at [`INCR_CHUNK`]
//!   (reasoned bounds: far under the X11 max-request size, big
//!   enough that field copies never chunk).
//! - Served targets are `TARGETS` + `UTF8_STRING` only (`STRING`,
//!   `TIMESTAMP`, `MULTIPLE` refuse with an empty reply —
//!   ICCCM-legal, stated); received `STRING` maps losslessly through
//!   latin-1, anything else refuses loudly. Only the CLIPBOARD
//!   selection (middle-click PRIMARY is a follow-up). Wayland-native
//!   compositors are reached through XWayland bridging (stated).
//! - Every wait is bounded ([`SELECTION_TIMEOUT`] per wait — a dead
//!   owner is a loud `Backend` error, never a hung UI thread).
//! - No display → the constructors fail loudly as
//!   `Backend("x11 connect …")` (clipboard is display-bound; the
//!   runner keeps the session-local clipboard with a one-time note
//!   — the TSF best-effort precedent, decision 256).
//!
//! [`Clipboard::service`]: oppa::Clipboard::service

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use oppa::{Clipboard, ClipboardError};
use std::fmt::Debug;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// Largest single-shot selection payload (bytes) before INCR
/// chunking takes over (far under the X11 max-request size, big
/// enough that field copies never chunk — stated, not silent).
pub const INCR_THRESHOLD: usize = 128 * 1024;
/// INCR chunk size (bytes) in both directions (under the
/// max-request size with wide margin).
pub const INCR_CHUNK: usize = 64 * 1024;
/// Bound per blocking wait for a peer (dead owners refuse loudly
/// instead of hanging the UI thread — stated, not silent).
pub const SELECTION_TIMEOUT: Duration = Duration::from_secs(1);
/// Idle services polled before an abandoned INCR send is dropped
/// (~10 s at the runner's 100 ms idle cadence — a stuck requestor
/// must not wedge our state forever).
const INCR_SEND_TICKS: u32 = 100;

/// Interned selection atoms (all interned at connect — none of
/// these are core-predefined, so no `AtomEnum` guessing).
#[derive(Clone, Copy, Debug)]
struct Atoms {
    clipboard: Atom,
    utf8: Atom,
    targets: Atom,
    incr: Atom,
    string: Atom,
}

/// One live X connection: the hidden owner/requestor window plus
/// atoms plus any in-flight INCR send.
struct LiveConn {
    conn: RustConnection,
    window: Window,
    atoms: Atoms,
    incr_send: Option<IncrSend>,
}

/// An in-flight INCR send to a peer (multi-`service()` state: the
/// requestor deletes the property per chunk, we answer chunk by
/// chunk until the zero terminator — chunks pre-split with
/// [`split_incr`], so the send and receive paths share one shape).
struct IncrSend {
    requestor: Window,
    property: Atom,
    chunks: VecDeque<Vec<u8>>,
    ticks: u32,
}

/// Linux system clipboard backend (UI-thread use — one connection
/// per instance; construct one and share nothing).
pub struct LinuxClipboard {
    display: Option<String>,
    conn: Option<LiveConn>,
    owned_text: Option<String>,
    outstanding: bool,
}

impl std::fmt::Debug for LinuxClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinuxClipboard")
            .field("display", &self.display)
            .field("connected", &self.conn.is_some())
            .field("owned", &self.owned_text.is_some())
            .finish_non_exhaustive()
    }
}

impl LinuxClipboard {
    /// Connects to the session display (`$DISPLAY`) and takes no
    /// selection yet. Loud `Backend` without an X display (see the
    /// module docs — clipboard is display-bound).
    pub fn new() -> Result<Self, ClipboardError> {
        Self::connect(None)
    }

    /// Connects to a named display (remote-display seam — and the
    /// deterministic no-display probe the headless tests use, since
    /// an invalid name fails everywhere without touching process
    /// environment).
    pub fn with_display(display: &str) -> Result<Self, ClipboardError> {
        Self::connect(Some(display.to_string()))
    }

    fn connect(display: Option<String>) -> Result<Self, ClipboardError> {
        let (conn, screen_num) = x11rb::connect(display.as_deref()).map_err(|e| {
            ClipboardError::Backend(format!("x11 connect (no X display at {display:?}): {e:?}"))
        })?;
        let screen = &conn.setup().roots[screen_num];
        let window = conn.generate_id().map_err(conn_err)?;
        conn.create_window(
            0,
            window,
            screen.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new()
                .override_redirect(1)
                .event_mask(EventMask::PROPERTY_CHANGE),
        )
        .map_err(conn_err)?
        .check()
        .map_err(conn_err)?;
        // One flush for all five interns (single round-trip bulk).
        let c_clipboard = conn.intern_atom(false, b"CLIPBOARD").map_err(conn_err)?;
        let c_utf8 = conn.intern_atom(false, b"UTF8_STRING").map_err(conn_err)?;
        let c_targets = conn.intern_atom(false, b"TARGETS").map_err(conn_err)?;
        let c_incr = conn.intern_atom(false, b"INCR").map_err(conn_err)?;
        let c_string = conn.intern_atom(false, b"STRING").map_err(conn_err)?;
        let atoms = Atoms {
            clipboard: c_clipboard.reply().map_err(conn_err)?.atom,
            utf8: c_utf8.reply().map_err(conn_err)?.atom,
            targets: c_targets.reply().map_err(conn_err)?.atom,
            incr: c_incr.reply().map_err(conn_err)?.atom,
            string: c_string.reply().map_err(conn_err)?.atom,
        };
        conn.flush().map_err(conn_err)?;
        Ok(Self {
            display,
            conn: Some(LiveConn {
                conn,
                window,
                atoms,
                incr_send: None,
            }),
            owned_text: None,
            outstanding: false,
        })
    }

    /// Runs `f` on the live connection, (re)connecting after a
    /// transport error (an X error usually means a dead connection —
    /// retrying the next op is the transient-friendly shape;
    /// persistent failure surfaces per op, loudly). A reconnect
    /// drops ownership claims (the server cleared them with the dead
    /// connection — never claim text we no longer own). Closures map
    /// x11rb errors explicitly (no inference guessing across the
    /// crate's error enums).
    fn with_live<T>(
        &mut self,
        f: impl FnOnce(&mut LiveConn) -> Result<T, ClipboardError>,
    ) -> Result<T, ClipboardError> {
        if self.conn.is_none() {
            let display = self.display.clone();
            let fresh = Self::connect(display)?;
            self.conn = Some(fresh.conn.expect("fresh connect is live"));
            self.owned_text = None;
        }
        let live = self.conn.as_mut().expect("connected above");
        f(live).inspect_err(|_| {
            self.drop_conn();
        })
    }

    /// Drops the connection after a transport error (the next op
    /// reconnects — see [`LinuxClipboard::with_live`]).
    fn drop_conn(&mut self) {
        self.conn = None;
        self.owned_text = None;
    }

    fn read_now(&mut self) -> Result<Option<String>, ClipboardError> {
        // Owned fast path (ownership verified — catches takeovers
        // between service pumps without any conversion traffic).
        let (us, owner) = self.with_live(|live| {
            let owner = live
                .conn
                .get_selection_owner(live.atoms.clipboard)
                .map_err(conn_err)?
                .reply()
                .map_err(conn_err)?
                .owner;
            Ok((live.window, owner))
        })?;
        if owner == us {
            if let Some(text) = &self.owned_text {
                return Ok(if text.is_empty() {
                    None
                } else {
                    Some(text.clone())
                });
            }
        } else {
            self.owned_text = None;
        }
        // Requestor path (with INCR receive for large pastes).
        let bytes = self.request_selection()?;
        decode_selection(&bytes.data, bytes.utf8)
    }

    /// Converts the CLIPBOARD selection into our window (blocking
    /// with [`SELECTION_TIMEOUT`] per wait — a dead owner refuses
    /// loudly instead of hanging the UI thread).
    fn request_selection(&mut self) -> Result<SelectedBytes, ClipboardError> {
        let (window, atoms) = self.with_live(|live| Ok((live.window, live.atoms)))?;
        self.with_live(|live| {
            live.conn
                .convert_selection(
                    window,
                    atoms.clipboard,
                    atoms.utf8,
                    atoms.utf8,
                    x11rb::CURRENT_TIME,
                )
                .map_err(conn_err)?
                .check()
                .map_err(conn_err)?;
            live.conn.flush().map_err(conn_err)?;
            Ok(())
        })?;
        loop {
            match wait_event(self, "convert selection")? {
                Event::SelectionNotify(n)
                    if n.requestor == window && n.selection == atoms.clipboard =>
                {
                    if n.property == x11rb::NONE {
                        // Conversion refused (e.g. owner serves no
                        // UTF-8 target) — nothing pastable, not an
                        // error (stated).
                        return Ok(SelectedBytes {
                            data: Vec::new(),
                            utf8: true,
                        });
                    }
                    return self.read_property(n.property);
                }
                _ => {}
            }
        }
    }

    /// Reads our window's property fully: offset loop for non-INCR
    /// large replies (final delete is the requestor cleanup), INCR
    /// handshake for chunked ones (each chunk read deleted, per the
    /// protocol — the delete signals readiness for the next).
    fn read_property(&mut self, property: Atom) -> Result<SelectedBytes, ClipboardError> {
        let (window, atoms) = self.with_live(|live| Ok((live.window, live.atoms)))?;
        let first = self.with_live(|live| {
            let rep = live
                .conn
                .get_property(
                    false,
                    window,
                    property,
                    0u32,
                    0,
                    (INCR_THRESHOLD / 4 + 1024) as u32,
                )
                .map_err(conn_err)?
                .reply()
                .map_err(conn_err)?;
            Ok(rep)
        })?;
        if first.type_ == atoms.incr {
            // INCR receive: delete the type property to signal
            // readiness, then chunks arrive as PropertyNotify until
            // the zero chunk.
            self.with_live(|live| {
                live.conn
                    .delete_property(window, property)
                    .map_err(conn_err)?
                    .check()
                    .map_err(conn_err)?;
                Ok(())
            })?;
            let mut data = Vec::new();
            loop {
                match wait_event(self, "incr chunk")? {
                    Event::PropertyNotify(p) if p.window == window && p.atom == property => {
                        let chunk = self.with_live(|live| {
                            let rep = live
                                .conn
                                .get_property(
                                    true,
                                    window,
                                    property,
                                    0u32,
                                    0,
                                    (INCR_CHUNK / 4 + 1024) as u32,
                                )
                                .map_err(conn_err)?
                                .reply()
                                .map_err(conn_err)?;
                            Ok(rep)
                        })?;
                        if chunk.value.is_empty() {
                            break;
                        }
                        data.extend_from_slice(&chunk.value);
                    }
                    _ => {}
                }
            }
            return Ok(SelectedBytes { data, utf8: true });
        }
        let mut data = first.value;
        let mut bytes_after = first.bytes_after;
        while bytes_after > 0 {
            let offset = data.len().div_ceil(4) as u32;
            let rep = self.with_live(|live| {
                let rep = live
                    .conn
                    .get_property(
                        false,
                        window,
                        property,
                        0u32,
                        offset,
                        bytes_after.div_ceil(4) + 1024,
                    )
                    .map_err(conn_err)?
                    .reply()
                    .map_err(conn_err)?;
                Ok(rep)
            })?;
            data.extend_from_slice(&rep.value);
            bytes_after = rep.bytes_after;
        }
        self.with_live(|live| {
            live.conn
                .delete_property(window, property)
                .map_err(conn_err)?
                .check()
                .map_err(conn_err)?;
            Ok(())
        })?;
        let utf8 = first.type_ == atoms.utf8;
        if first.type_ != atoms.utf8 && first.type_ != atoms.string && first.type_ != 0 {
            return Err(ClipboardError::Backend(format!(
                "selection arrived in an unexpected type {} — refusing, never laundering",
                first.type_
            )));
        }
        Ok(SelectedBytes { data, utf8 })
    }

    fn write_now(&mut self, text: &str) -> Result<(), ClipboardError> {
        if text.is_empty() {
            return self.clear_now();
        }
        let window = self.with_live(|live| {
            live.conn
                .set_selection_owner(window_of(live), live.atoms.clipboard, x11rb::CURRENT_TIME)
                .map_err(conn_err)?
                .check()
                .map_err(conn_err)?;
            live.conn.flush().map_err(conn_err)?;
            Ok(live.window)
        })?;
        let owner = self.with_live(|live| {
            let owner = live
                .conn
                .get_selection_owner(live.atoms.clipboard)
                .map_err(conn_err)?
                .reply()
                .map_err(conn_err)?
                .owner;
            Ok(owner)
        })?;
        if owner != window {
            self.owned_text = None;
            return Err(ClipboardError::Backend(
                "selection ownership taken before verify — another owner raced us".to_string(),
            ));
        }
        self.owned_text = Some(text.to_string());
        Ok(())
    }

    fn clear_now(&mut self) -> Result<(), ClipboardError> {
        self.with_live(|live| {
            live.conn
                .set_selection_owner(x11rb::NONE, live.atoms.clipboard, x11rb::CURRENT_TIME)
                .map_err(conn_err)?
                .check()
                .map_err(conn_err)?;
            live.conn.flush().map_err(conn_err)?;
            Ok(())
        })?;
        self.owned_text = None;
        Ok(())
    }

    /// Answers one `SelectionRequestEvent` for our text (single-shot
    /// reply, INCR start, or ICCCM-legal refusal).
    fn serve_request(&mut self, req: SelectionRequestEvent) {
        let text = match &self.owned_text {
            Some(t) if !t.is_empty() => t.clone(),
            _ => {
                self.reply_none(&req);
                return;
            }
        };
        let Some((utf8, targets)) = self.conn.as_ref().map(|l| (l.atoms.utf8, l.atoms.targets))
        else {
            return;
        };
        if req.target == targets {
            // TARGETS → the served conversions.
            let mut list = Vec::with_capacity(8);
            list.extend_from_slice(&utf8.to_ne_bytes());
            list.extend_from_slice(&targets.to_ne_bytes());
            self.reply_bytes(&req, targets, &list);
        } else if req.target == utf8 {
            let bytes = text.as_bytes();
            if bytes.len() > INCR_THRESHOLD {
                self.reply_incr(&req, bytes.to_vec());
            } else {
                self.reply_bytes(&req, utf8, bytes);
            }
        } else {
            // STRING / TIMESTAMP / MULTIPLE / anything else: refuse
            // (empty reply — ICCCM-legal, stated bound).
            self.reply_none(&req);
        }
    }

    fn reply_bytes(&mut self, req: &SelectionRequestEvent, type_: Atom, data: &[u8]) {
        let Some(live) = self.conn.as_mut() else {
            return;
        };
        let stored = live
            .conn
            .change_property8(PropMode::REPLACE, req.requestor, req.property, type_, data)
            .is_ok()
            && live.conn.flush().is_ok();
        // An empty property reads as refused requestor-side, so a
        // failed store still terminates honestly.
        let property = if stored { req.property } else { x11rb::NONE };
        send_notify(live, req, property);
    }

    fn reply_none(&mut self, req: &SelectionRequestEvent) {
        if let Some(live) = self.conn.as_mut() {
            send_notify(live, req, x11rb::NONE);
        }
    }

    /// Starts an INCR send (the requestor deletes the INCR property
    /// per chunk; [`LinuxClipboard::service`] drives the rest).
    fn reply_incr(&mut self, req: &SelectionRequestEvent, data: Vec<u8>) {
        let Some(live) = self.conn.as_mut() else {
            return;
        };
        let size = (data.len() as u32).to_ne_bytes();
        let started = live
            .conn
            .change_property8(
                PropMode::REPLACE,
                req.requestor,
                req.property,
                live.atoms.incr,
                &size,
            )
            .is_ok()
            && live
                .conn
                .change_window_attributes(
                    req.requestor,
                    &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
                )
                .is_ok()
            && live.conn.flush().is_ok();
        if started {
            send_notify(live, req, req.property);
            live.incr_send = Some(IncrSend {
                requestor: req.requestor,
                property: req.property,
                chunks: split_incr(&data).into_iter().map(|c| c.to_vec()).collect(),
                ticks: 0,
            });
        } else {
            send_notify(live, req, x11rb::NONE);
        }
    }

    /// Drives an in-flight INCR send (called from [`service`]): when
    /// the requestor deleted the property, the next chunk goes out;
    /// an exhausted queue sends the zero terminator and ends the send.
    fn drive_incr_send(&mut self) {
        let step = {
            let Some(live) = self.conn.as_mut() else {
                return;
            };
            let Some(send) = live.incr_send.as_mut() else {
                return;
            };
            send.ticks += 1;
            if send.ticks > INCR_SEND_TICKS {
                // Stuck requestor — drop the send, keep ownership.
                live.incr_send = None;
                return;
            }
            // Present property means the requestor hasn't consumed
            // yet; absent means it deleted (ready for the next).
            let present = live
                .conn
                .get_property(false, send.requestor, send.property, 0u32, 0, 0)
                .ok()
                .and_then(|c| c.reply().ok())
                .is_some_and(|r| r.bytes_after > 0 || !r.value.is_empty());
            if present {
                return;
            }
            let requestor = send.requestor;
            let property = send.property;
            match send.chunks.pop_front() {
                Some(chunk) => (requestor, property, chunk),
                None => {
                    live.incr_send = None;
                    (requestor, property, Vec::new())
                }
            }
        };
        let (requestor, property, chunk) = step;
        let utf8 = self.conn.as_ref().map(|l| l.atoms.utf8).unwrap_or(0);
        if let Some(live) = self.conn.as_mut() {
            let _ =
                live.conn
                    .change_property8(PropMode::REPLACE, requestor, property, utf8, &chunk);
            let _ = live.conn.flush();
        }
    }
}

/// Window of a live connection (avoids borrowing the whole struct
/// inside [`with_live`](LinuxClipboard::with_live) closures).
fn window_of(live: &LiveConn) -> Window {
    live.window
}

/// Converted selection bytes plus whether they arrived as UTF-8
/// (`STRING` targets map through latin-1 instead).
struct SelectedBytes {
    data: Vec<u8>,
    utf8: bool,
}

/// Sends a `SelectionNotify` reply through the real serializer
/// (no hand-packing across x11rb versions).
fn send_notify(live: &mut LiveConn, req: &SelectionRequestEvent, property: Atom) {
    let ev = SelectionNotifyEvent {
        response_type: 31,
        sequence: 0,
        time: x11rb::CURRENT_TIME,
        requestor: req.requestor,
        selection: req.selection,
        target: req.target,
        property,
    };
    if live
        .conn
        .send_event(false, req.requestor, EventMask::default(), ev)
        .is_err()
    {
        return;
    }
    let _ = live.conn.flush();
}

/// Bounded wait for the next X event (a dead peer refuses loudly
/// instead of hanging the UI thread).
fn wait_event(clip: &mut LinuxClipboard, what: &str) -> Result<Event, ClipboardError> {
    let deadline = Instant::now() + SELECTION_TIMEOUT;
    loop {
        let next = clip.with_live(|live| live.conn.poll_for_event().map_err(conn_err))?;
        if let Some(ev) = next {
            return Ok(ev);
        }
        if Instant::now() >= deadline {
            return Err(ClipboardError::Backend(format!(
                "{what}: selection owner timed out — refusing, never hanging"
            )));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn conn_err(e: impl Debug) -> ClipboardError {
    ClipboardError::Backend(format!("x11 transport: {e:?}"))
}

/// Decodes selection bytes into clipboard text (pure — headless
/// unit-tested): UTF-8 validated loudly (a corrupt owner is never
/// laundered), `STRING` targets mapped losslessly through latin-1
/// (bytes 0x00–0xFF are codepoints U+0000–U+00FF), empty payloads
/// read as `None` (the Win32 empty→None precedent).
fn decode_selection(bytes: &[u8], utf8: bool) -> Result<Option<String>, ClipboardError> {
    if bytes.is_empty() {
        return Ok(None);
    }
    if utf8 {
        let text = std::str::from_utf8(bytes).map_err(|e| {
            ClipboardError::Backend(format!(
                "selection UTF-8 invalid: {e} — refusing, never laundering"
            ))
        })?;
        Ok(if text.is_empty() {
            None
        } else {
            Some(text.to_string())
        })
    } else {
        let text: String = bytes.iter().map(|b| *b as char).collect();
        Ok(if text.is_empty() { None } else { Some(text) })
    }
}

/// Splits a payload into [`INCR_CHUNK`] slices (pure — headless
/// unit-tested; the send and receive paths share the shape).
fn split_incr(data: &[u8]) -> Vec<&[u8]> {
    if data.is_empty() {
        return Vec::new();
    }
    data.chunks(INCR_CHUNK).collect()
}

impl Clipboard for LinuxClipboard {
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.write_now(text)
    }

    fn clear(&mut self) -> Result<(), ClipboardError> {
        self.clear_now()
    }

    fn request_read(&mut self) {
        self.outstanding = true;
    }

    fn poll_read(&mut self) -> Option<Result<Option<String>, ClipboardError>> {
        // Sync backend: every poll settles (the outstanding flag only
        // exists so the poll contract matches the async shells').
        self.outstanding = false;
        Some(self.read_now())
    }

    /// Service peer selection requests without blocking (see the
    /// module docs): answers `SelectionRequestEvent`s for our text,
    /// drives INCR sends, and forgets ownership on `SelectionClear`.
    /// Returns true while we own a servable selection (the runner
    /// keeps idle wakeups then — a peer's paste never hangs on us).
    fn service(&mut self) -> bool {
        // Drain all pending events (non-blocking by construction).
        loop {
            let next = {
                let Some(live) = self.conn.as_mut() else {
                    return false;
                };
                match live.conn.poll_for_event() {
                    Ok(ev) => ev,
                    Err(_) => {
                        self.drop_conn();
                        return false;
                    }
                }
            };
            let Some(ev) = next else {
                break;
            };
            match ev {
                Event::SelectionClear(n) => {
                    if self.clipboard_atom() == n.selection {
                        self.owned_text = None;
                        self.abort_incr_send();
                    }
                }
                Event::SelectionRequest(req) if self.is_our_request(&req) => {
                    self.serve_request(req);
                }
                _ => {}
            }
        }
        let due = self.conn.as_ref().is_some_and(|l| l.incr_send.is_some());
        if due {
            self.drive_incr_send();
        }
        self.owned_text.is_some()
    }
}

impl LinuxClipboard {
    fn clipboard_atom(&self) -> Atom {
        self.conn.as_ref().map(|l| l.atoms.clipboard).unwrap_or(0)
    }

    fn is_our_request(&self, req: &SelectionRequestEvent) -> bool {
        let Some(live) = self.conn.as_ref() else {
            return false;
        };
        req.owner == live.window && req.selection == live.atoms.clipboard
    }

    fn abort_incr_send(&mut self) {
        if let Some(live) = self.conn.as_mut() {
            live.incr_send = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_utf8_round_trips_multibyte() {
        let text = "oppa-clipboard-probe-256 \u{fc}nicode \u{1f44d}";
        let bytes = text.as_bytes();
        assert_eq!(
            decode_selection(bytes, true).expect("valid UTF-8 decodes"),
            Some(text.to_string())
        );
        // CJK alongside Latin survives too.
        let cjk = "nihao-\u{4f60}\u{597d}";
        assert_eq!(
            decode_selection(cjk.as_bytes(), true).expect("cjk decodes"),
            Some(cjk.to_string())
        );
    }

    #[test]
    fn decode_rejects_invalid_utf8_loudly() {
        let bytes = b"\xff\xfe invalid \x80";
        let err = decode_selection(bytes, true).expect_err("corrupt owner refuses");
        assert!(
            matches!(err, ClipboardError::Backend(_)),
            "loud Backend, got {err:?}"
        );
    }

    #[test]
    fn decode_empty_is_none_and_latin1_maps() {
        assert_eq!(decode_selection(b"", true).expect("empty"), None);
        // 0xE9 is e-acute in latin-1 -> U+00E9 (lossless, never mojibake).
        assert_eq!(
            decode_selection(b"caf\xe9", false).expect("latin-1 maps"),
            Some("caf\u{e9}".to_string())
        );
    }

    #[test]
    fn incr_split_reassembles_exactly() {
        // 200 KB -> 4 chunks (64 KB x 3 + 8 KB), byte-exact round-trip.
        let data: Vec<u8> = (0..200 * 1024).map(|i| (i % 251) as u8).collect();
        assert!(data.len() > INCR_THRESHOLD, "test spans the threshold");
        let chunks = split_incr(&data);
        assert_eq!(chunks.len(), 4, "ceil(200/64) chunks");
        assert!(chunks.iter().all(|c| c.len() <= INCR_CHUNK));
        let mut rebuilt = Vec::new();
        for c in &chunks {
            rebuilt.extend_from_slice(c);
        }
        assert_eq!(rebuilt, data, "chunking is lossless");
        assert!(split_incr(b"").is_empty(), "empty never chunks");
    }

    #[test]
    fn invalid_display_fails_loudly_everywhere() {
        // No environment touched (process-global): an invalid display
        // name fails deterministically on every machine, display or
        // not — the loud-failure contract, headless-proven.
        let err = LinuxClipboard::with_display("oppa-invalid-display-xyz")
            .expect_err("no such display refuses");
        let msg = err.to_string();
        assert!(
            msg.contains("x11 connect"),
            "connect failure names itself, got: {msg}"
        );
    }

    /// Real-X round-trip (Linux-with-display only): saves the user's
    /// current clipboard, writes a marker, reads it back through the
    /// request/poll shape, then restores the original — the Win32
    /// probe's mirror. Without a display the constructor fails and
    /// the test skips loudly (a skip, not a pass over nothing —
    /// headless CI stays green while display runs verify).
    #[test]
    fn system_round_trip_with_restore() {
        let mut clip = match LinuxClipboard::new() {
            Ok(c) => c,
            Err(e) if e.to_string().contains("x11 connect") => {
                eprintln!("SKIP system_round_trip_with_restore: {e}");
                return;
            }
            Err(e) => panic!("unexpected clipboard failure: {e}"),
        };
        let saved = clip.read_text_now().expect("initial read refuses loudly");
        let marker = "oppa-clipboard-probe-256 \u{fc}nicode \u{1f44d}";
        clip.write_text(marker).expect("write refuses loudly");
        assert_eq!(
            clip.read_text_now().expect("reread refuses loudly"),
            Some(marker.to_string()),
            "X11 UTF8_STRING round-trips incl. multi-byte"
        );
        clip.request_read();
        assert_eq!(
            clip.poll_read(),
            Some(Ok(Some(marker.to_string()))),
            "sync backend settles on first poll"
        );
        // Empty writes clear (the Win32 parity rule).
        clip.write_text("").expect("empty writes");
        assert_eq!(
            clip.owned_text, None,
            "empty write clears internal ownership"
        );
        let after_clear = clip.read_text_now().expect("cleared reads");
        // Under a live clipboard manager (e.g. WSLg Xwayland bridge, Klipper),
        // releasing ownership causes the manager to immediately re-assert the
        // host/prior clipboard; otherwise the selection reads None. In either
        // case, our marker must no longer be owned.
        assert!(
            after_clear.is_none() || after_clear != Some(marker.to_string()),
            "empty write released marker"
        );
        match &saved {
            Some(original) => clip.write_text(original).expect("restore refuses loudly"),
            None => clip.clear().expect("restore-clear refuses loudly"),
        }
        assert_eq!(
            clip.read_text_now().expect("post-restore read"),
            saved,
            "user clipboard restored"
        );
    }
}
