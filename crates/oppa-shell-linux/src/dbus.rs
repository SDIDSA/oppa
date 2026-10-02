//! Minimal D-Bus session-bus client (Round 2.3, OQ-G12-1): exactly
//! what the FileChooser portal needs — connect + AUTH + Hello, one
//! `OpenFile` method call shape, `Response` signal awaiting — over a
//! transport-generic stream, with zero new crates.
//!
//! Scope discipline (stated, not silent): little-endian marshal,
//! `unix:` + `tcp:` addresses, and the type subset the portal uses
//! (`b/u/y/s/o/g/ay`, arrays, structs, dict entries, variants).
//! Anything else refuses loudly with the offending signature or
//! byte offset. Method replies parse against caller-known
//! signatures; signals carry theirs. Every wait is bounded
//! (callers pass the deadline — the bus daemon is local, but a dead
//! one must refuse, never hang the UI thread).
//!
//! Wire reference: the D-Bus specification (message format +
//! type system) and the
//! [FileChooser portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html)
//! (`OpenFile(ssa{sv})→o`, `filters a(sa(us))`, `current_folder
//! ay`, results `uris as`).

mod msg;
mod parse;
mod sig;
mod transport;
mod value;

#[cfg(test)]
mod tests;

pub(crate) use msg::{
    build_method_call, parse_body, parse_message, InMsg, MSG_ERROR, MSG_METHOD_RETURN, MSG_SIGNAL,
};
pub(crate) use sig::Type;
#[cfg(unix)]
pub(crate) use transport::{connect_abstract, connect_unix};
pub(crate) use transport::{connect_tcp, default_address, BusAddr, BusConn, BusTransport};
pub(crate) use value::{marshal_value, DVal};
