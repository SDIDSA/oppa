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

use std::io::{Read, Write};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Signature model (single complete type at a time)
// ---------------------------------------------------------------------------

/// One parsed D-Bus type (alignment per the spec: `y`=1, `n`/`q`=2,
/// `b`/`i`/`u`/`s`/`o`/`g`=4 — note `g` aligns 1 for its length
/// byte but the *signature string itself* is length-prefixed u8,
/// `x`/`t`/`d`/structs/dicts=8, arrays follow their element).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Type {
    Byte,
    Bool,
    U16,
    I16,
    U32,
    I32,
    U64,
    Str,
    Path,
    Sig,
    Array(Box<Type>),
    Struct(Vec<Type>),
    Variant,
}

impl Type {
    fn align(&self) -> usize {
        match self {
            Type::Byte => 1,
            Type::U16 | Type::I16 => 2,
            Type::Bool | Type::U32 | Type::I32 | Type::Str | Type::Path => 4,
            Type::Sig | Type::Variant => 1,
            Type::U64 => 8,
            Type::Array(_) => 4,
            Type::Struct(_) => 8,
        }
    }
}

/// Parses one complete type from the front of `sig`, returning the
/// type plus the consumed length. Dict entries (`{…}`) read as
/// two-element structs (same wire format — key-basicness is the
/// caller's contract, like the portal's `a{sv}`).
fn parse_type(sig: &[u8]) -> Result<(Type, usize), String> {
    let (&c, rest) = sig.split_first().ok_or("empty signature")?;
    let simple = match c {
        b'y' => Some(Type::Byte),
        b'b' => Some(Type::Bool),
        b'n' => Some(Type::I16),
        b'q' => Some(Type::U16),
        b'i' => Some(Type::I32),
        b'u' => Some(Type::U32),
        b'x' | b't' => Some(Type::U64),
        b's' => Some(Type::Str),
        b'o' => Some(Type::Path),
        b'g' => Some(Type::Sig),
        b'v' => Some(Type::Variant),
        _ => None,
    };
    if let Some(t) = simple {
        return Ok((t, 1));
    }
    match c {
        b'a' => {
            let (inner, n) = parse_type(rest)?;
            Ok((Type::Array(Box::new(inner)), 1 + n))
        }
        b'(' | b'r' => {
            let mut items = Vec::new();
            let mut off = 0;
            loop {
                let (&d, _) = rest[off..]
                    .split_first()
                    .ok_or("unterminated struct signature")?;
                if d == b')' || d == b'}' {
                    off += 1;
                    break;
                }
                let (t, n) = parse_type(&rest[off..])?;
                items.push(t);
                off += n;
            }
            Ok((Type::Struct(items), 1 + off))
        }
        b'{' => {
            let (k, n) = parse_type(rest)?;
            let (v, m) = parse_type(&rest[n..])?;
            let (&end, _) = rest[n + m..]
                .split_first()
                .ok_or("unterminated dict signature")?;
            if end != b'}' {
                return Err("dict entry must close with }".to_string());
            }
            let _ = k;
            Ok((Type::Struct(vec![k, v]), 2 + n + m))
        }
        _ => Err(format!("unsupported signature byte {c:#04x}")),
    }
}

/// Parses a full signature into top-level types (empty string = no
/// values — `AddMatch`/`Close` bodies).
fn parse_sig(sig: &str) -> Result<Vec<Type>, String> {
    let bytes = sig.as_bytes();
    let mut out = Vec::new();
    let mut off = 0;
    while off < bytes.len() {
        let (t, n) = parse_type(&bytes[off..])?;
        out.push(t);
        off += n;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Dynamic values (marshal + parse, signature-driven)
// ---------------------------------------------------------------------------

/// One D-Bus value (only the shapes the portal uses — anything
/// else is a loud construction error, never a silent encoding).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DVal {
    Bool(bool),
    U32(u32),
    Str(String),
    Path(String),
    /// A signature value (only valid where the spec demands type
    /// `g` — the SIGNATURE header field).
    Sig(String),
    Bytes(Vec<u8>),
    Array(Vec<DVal>),
    Struct(Vec<DVal>),
    Variant(Box<DVal>),
}

fn pad_to(buf: &mut Vec<u8>, base: usize, align: usize) {
    while !(base + buf.len()).is_multiple_of(align) {
        buf.push(0);
    }
}

fn marshal_u32(buf: &mut Vec<u8>, base: usize, v: u32) {
    pad_to(buf, base, 4);
    buf.extend_from_slice(&v.to_le_bytes());
}

fn marshal_str(buf: &mut Vec<u8>, base: usize, s: &str) {
    pad_to(buf, base, 4);
    marshal_u32(buf, base, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
    buf.push(0);
}

/// Marshals one value of the given type (little-endian; `base` is
/// the body's start offset for alignment). Type/value mismatches
/// are programmer bugs — loud errors, never silent encodings.
pub(crate) fn marshal_value(
    buf: &mut Vec<u8>,
    base: usize,
    ty: &Type,
    v: &DVal,
) -> Result<(), String> {
    match (ty, v) {
        (Type::Byte, DVal::U32(n)) if *n < 256 => buf.push(*n as u8),
        (Type::Bool, DVal::Bool(b)) => {
            pad_to(buf, base, 4);
            buf.extend_from_slice(&u32::from(*b).to_le_bytes());
        }
        (Type::U16, DVal::U32(n)) if *n < 65536 => {
            pad_to(buf, base, 2);
            buf.extend_from_slice(&(*n as u16).to_le_bytes());
        }
        (Type::U32, DVal::U32(n)) => marshal_u32(buf, base, *n),
        (Type::Str, DVal::Str(s)) | (Type::Path, DVal::Path(s)) | (Type::Path, DVal::Str(s)) => {
            marshal_str(buf, base, s)
        }
        (Type::Sig, DVal::Str(s)) | (Type::Sig, DVal::Sig(s)) => {
            if s.len() > 255 {
                return Err("signature too long".to_string());
            }
            buf.push(s.len() as u8);
            buf.extend_from_slice(s.as_bytes());
            buf.push(0);
        }
        (Type::Array(inner), DVal::Bytes(b)) if matches!(**inner, Type::Byte) => {
            pad_to(buf, base, 4);
            marshal_u32(buf, base, b.len() as u32);
            buf.extend_from_slice(b);
        }
        (Type::Array(inner), DVal::Array(items)) => {
            pad_to(buf, base, 4);
            let len_pos = base + buf.len();
            buf.extend_from_slice(&[0, 0, 0, 0]);
            // Elements start at the element alignment past the
            // length field; the length counts element bytes only
            // (never the alignment pad).
            let misalignment = (base + buf.len()) % inner.align();
            if misalignment != 0 {
                buf.extend(vec![0; inner.align() - misalignment]);
            }
            let start = buf.len();
            for item in items {
                if matches!(**inner, Type::Struct(_)) {
                    let m = (base + buf.len()) % 8;
                    if m != 0 {
                        buf.extend(vec![0; 8 - m]);
                    }
                }
                marshal_value(buf, base, inner, item)?;
            }
            let len = (buf.len() - start) as u32;
            buf[len_pos..len_pos + 4].copy_from_slice(&len.to_le_bytes());
        }
        (Type::Struct(fields), DVal::Struct(vals)) => {
            if fields.len() != vals.len() {
                return Err(format!(
                    "struct arity {} vs {} values",
                    fields.len(),
                    vals.len()
                ));
            }
            pad_to(buf, base, 8);
            for (f, v) in fields.iter().zip(vals.iter()) {
                marshal_value(buf, base, f, v)?;
            }
        }
        (Type::Variant, DVal::Variant(inner)) => {
            let sig = variant_sig(inner)?;
            marshal_value(buf, base, &Type::Sig, &DVal::Str(sig.clone()))?;
            let (t, _) = parse_type(sig.as_bytes())?;
            marshal_value(buf, base, &t, inner)?;
        }
        _ => {
            return Err(format!("marshal mismatch: {ty:?} vs {v:?}"));
        }
    }
    Ok(())
}

/// Signature for a variant payload, derived structurally (only the
/// shapes we emit — anything else is a loud construction error,
/// since a wrong signature silently mistargets the call). Empty
/// arrays carry no signature: callers omit the key instead (the
/// portal treats absent filter lists as none).
fn variant_sig(v: &DVal) -> Result<String, String> {
    fn sig_of(v: &DVal) -> Result<String, String> {
        match v {
            DVal::Bool(_) => Ok("b".to_string()),
            DVal::U32(_) => Ok("u".to_string()),
            DVal::Str(_) => Ok("s".to_string()),
            DVal::Path(_) => Ok("o".to_string()),
            DVal::Sig(_) => Ok("g".to_string()),
            DVal::Bytes(_) => Ok("ay".to_string()),
            DVal::Array(items) => {
                let first = items
                    .first()
                    .ok_or("empty array carries no signature — omit the key instead")?;
                Ok(format!("a{}", sig_of(first)?))
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

// ---------------------------------------------------------------------------
// Parsing (cursor over a byte slice — truncation refuses loudly)
// ---------------------------------------------------------------------------

struct Cursor<'a> {
    buf: &'a [u8],
    off: usize,
}

impl<'a> Cursor<'a> {
    fn need(&self, n: usize) -> Result<(), String> {
        if self.off + n > self.buf.len() {
            return Err(format!(
                "truncated message at offset {} (need {n})",
                self.off
            ));
        }
        Ok(())
    }

    fn align(&mut self, base: usize, align: usize) {
        while !(base + self.off).is_multiple_of(align) {
            self.off += 1;
        }
    }

    fn u8v(&mut self) -> Result<u8, String> {
        self.need(1)?;
        let v = self.buf[self.off];
        self.off += 1;
        Ok(v)
    }

    fn u16v(&mut self, base: usize) -> Result<u16, String> {
        self.align(base, 2);
        self.need(2)?;
        let v = u16::from_le_bytes([self.buf[self.off], self.buf[self.off + 1]]);
        self.off += 2;
        Ok(v)
    }

    fn u32v(&mut self, base: usize) -> Result<u32, String> {
        self.align(base, 4);
        self.need(4)?;
        let v = u32::from_le_bytes([
            self.buf[self.off],
            self.buf[self.off + 1],
            self.buf[self.off + 2],
            self.buf[self.off + 3],
        ]);
        self.off += 4;
        Ok(v)
    }

    fn strv(&mut self, base: usize) -> Result<String, String> {
        let len = self.u32v(base)? as usize;
        self.need(len + 1)?;
        let s = std::str::from_utf8(&self.buf[self.off..self.off + len])
            .map_err(|e| format!("invalid UTF-8 string: {e}"))?
            .to_string();
        self.off += len;
        if self.buf[self.off] != 0 {
            return Err("string missing NUL terminator".to_string());
        }
        self.off += 1;
        Ok(s)
    }

    fn sigv(&mut self) -> Result<String, String> {
        let len = self.u8v()? as usize;
        self.need(len + 1)?;
        let s = std::str::from_utf8(&self.buf[self.off..self.off + len])
            .map_err(|e| format!("invalid signature: {e}"))?
            .to_string();
        self.off += len;
        if self.buf[self.off] != 0 {
            return Err("signature missing NUL terminator".to_string());
        }
        self.off += 1;
        Ok(s)
    }

    /// Parses one value; `base` is the body start (alignment root).
    fn value(&mut self, base: usize, ty: &Type) -> Result<DVal, String> {
        match ty {
            Type::Byte => Ok(DVal::U32(self.u8v()? as u32)),
            Type::Bool => Ok(DVal::Bool(self.u32v(base)? != 0)),
            Type::U16 => Ok(DVal::U32(self.u16v(base)? as u32)),
            Type::U32 => Ok(DVal::U32(self.u32v(base)?)),
            Type::Str => Ok(DVal::Str(self.strv(base)?)),
            Type::Path => Ok(DVal::Path(self.strv(base)?)),
            Type::Sig => Ok(DVal::Str(self.sigv()?)),
            Type::I16 | Type::I32 | Type::U64 => Err(format!("type {ty:?} never arrives here")),
            Type::Array(inner) if matches!(**inner, Type::Byte) => {
                // Byte arrays ride as one slice (no per-element
                // cursor dance — `current_folder`-style payloads).
                let len = self.u32v(base)? as usize;
                self.need(len)?;
                let b = self.buf[self.off..self.off + len].to_vec();
                self.off += len;
                Ok(DVal::Bytes(b))
            }
            Type::Array(inner) => {
                let len = self.u32v(base)? as usize;
                let misalignment = (base + self.off) % inner.align();
                if misalignment != 0 {
                    self.off += inner.align() - misalignment;
                }
                let data_start = self.off;
                let mut items = Vec::new();
                while self.off - data_start < len {
                    if matches!(**inner, Type::Struct(_)) {
                        self.align(base, 8);
                    }
                    // Empty-element guard: a zero-width element
                    // would spin forever — the portal never sends
                    // any, so refuse loudly instead.
                    let before = self.off;
                    items.push(self.value(base, inner)?);
                    if self.off == before {
                        return Err("zero-width array element".to_string());
                    }
                }
                if self.off - data_start != len {
                    return Err(format!("array element overran its length ({})", len));
                }
                Ok(DVal::Array(items))
            }
            Type::Struct(fields) => {
                self.align(base, 8);
                let mut out = Vec::with_capacity(fields.len());
                for f in fields {
                    out.push(self.value(base, f)?);
                }
                Ok(DVal::Struct(out))
            }
            Type::Variant => {
                let sig = self.sigv()?;
                let (t, _) = parse_type(sig.as_bytes())?;
                Ok(DVal::Variant(Box::new(self.value(base, &t)?)))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Messages (framing + header fields we route on)
// ---------------------------------------------------------------------------

pub(crate) const MSG_METHOD_CALL: u8 = 1;
pub(crate) const MSG_METHOD_RETURN: u8 = 2;
pub(crate) const MSG_ERROR: u8 = 3;
pub(crate) const MSG_SIGNAL: u8 = 4;

/// Header field codes we emit/route on (sender included — the
/// Response matcher keys on path, but the sender names the peer).
const F_PATH: u8 = 1;
const F_INTERFACE: u8 = 2;
const F_MEMBER: u8 = 3;
const F_ERROR_NAME: u8 = 4;
const F_REPLY_SERIAL: u8 = 5;
const F_DESTINATION: u8 = 6;
const F_SIGNATURE: u8 = 7;
const F_SENDER: u8 = 8;

/// One parsed message (header fields of interest + raw body for
/// caller-known signature parsing).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InMsg {
    pub mtype: u8,
    pub serial: u32,
    pub reply_to: Option<u32>,
    pub sender: Option<String>,
    pub path: Option<String>,
    pub member: Option<String>,
    pub iface: Option<String>,
    pub error_name: Option<String>,
    pub sig: String,
    pub body: Vec<u8>,
}

/// Pads `buf` (absolute `base`) to 8 (message framing helper —
/// header fields, field arrays, and bodies all 8-align).
fn pad8(buf: &mut Vec<u8>, base: usize) {
    while !(base + buf.len()).is_multiple_of(8) {
        buf.push(0);
    }
}

fn field(buf: &mut Vec<u8>, base: usize, code: u8, v: &DVal) -> Result<(), String> {
    // Header fields are an array of 8-aligned structs: pad the entry
    // start (the first entry inherits the array's alignment).
    pad8(buf, base);
    buf.push(code);
    marshal_value(
        buf,
        base,
        &Type::Variant,
        &DVal::Variant(Box::new(v.clone())),
    )
}

/// Builds a complete method-call message (serial assigned by the
/// caller — nonzero client serials, per the spec).
pub(crate) fn build_method_call(
    serial: u32,
    dest: &str,
    path: &str,
    iface: &str,
    member: &str,
    body_sig: &str,
    body: &[DVal],
) -> Result<Vec<u8>, String> {
    let types = parse_sig(body_sig)?;
    if types.len() != body.len() {
        return Err(format!(
            "body arity {} vs {} values for {member}",
            types.len(),
            body.len()
        ));
    }
    // Body first (header needs its length).
    let mut body_bytes = Vec::new();
    for (t, v) in types.iter().zip(body.iter()) {
        marshal_value(&mut body_bytes, 0, t, v)?;
    }
    let mut msg = vec![b'l', MSG_METHOD_CALL, 0, 1];
    msg.extend_from_slice(&(body_bytes.len() as u32).to_le_bytes());
    msg.extend_from_slice(&serial.to_le_bytes());
    // Fields array: pad 12 → 16, entries 8-aligned.
    pad8(&mut msg, 0);
    let base = 0;
    field(&mut msg, base, F_PATH, &DVal::Path(path.to_string()))?;
    field(&mut msg, base, F_DESTINATION, &DVal::Str(dest.to_string()))?;
    field(&mut msg, base, F_INTERFACE, &DVal::Str(iface.to_string()))?;
    field(&mut msg, base, F_MEMBER, &DVal::Str(member.to_string()))?;
    if !body_sig.is_empty() {
        field(
            &mut msg,
            base,
            F_SIGNATURE,
            &DVal::Sig(body_sig.to_string()),
        )?;
    }
    // Fix the fields-array length prefix: fields started at 16.
    let mut framed = Vec::with_capacity(16 + msg.len());
    framed.extend_from_slice(&msg[..12]);
    framed.extend_from_slice(&((msg.len() - 16) as u32).to_le_bytes());
    framed.extend_from_slice(&msg[16..]);
    // Body starts 8-aligned past the fields.
    pad8(&mut framed, 0);
    framed.extend_from_slice(&body_bytes);
    Ok(framed)
}

/// Parses one message from the front of `buf`, returning the
/// message plus total bytes consumed (headers + body). Little- and
/// big-endian both parse (the order flag rides each message).
pub(crate) fn parse_message(buf: &[u8]) -> Result<(InMsg, usize), String> {
    if buf.len() < 16 {
        return Err(format!("header truncated ({} bytes)", buf.len()));
    }
    let le = match buf[0] {
        b'l' => true,
        b'B' => false,
        o => return Err(format!("bad byte order {o:#04x}")),
    };
    let u32at = |off: usize| -> Result<u32, String> {
        if off + 4 > buf.len() {
            return Err("header truncated".to_string());
        }
        let b = [buf[off], buf[off + 1], buf[off + 2], buf[off + 3]];
        Ok(if le {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    };
    if buf[3] != 1 {
        return Err(format!("unsupported protocol version {}", buf[3]));
    }
    if !le {
        return Err("big-endian peers unsupported (we marshal LE)".to_string());
    }
    let mtype = buf[1];
    let body_len = u32at(4)? as usize;
    let serial = u32at(8)?;
    let fields_len = u32at(12)? as usize;
    let mut cur = Cursor { buf, off: 16 };
    // Fields array elements are 8-aligned structs off the array
    // start (16): the cursor aligns each entry explicitly.
    let fields_end = 16 + fields_len;
    if buf.len() < fields_end {
        return Err("fields truncated".to_string());
    }
    let mut path = None;
    let mut iface = None;
    let mut member = None;
    let mut error_name = None;
    let mut reply_to = None;
    let mut sender = None;
    let mut sig = String::new();
    while cur.off < fields_end {
        cur.align(16, 8);
        let code = cur.u8v()?;
        // Each field is BYTE code + VARIANT (the variant's own
        // signature prefix is the type tag — no extra byte).
        let val = cur.value(16, &Type::Variant)?;
        let inner = match val {
            DVal::Variant(b) => *b,
            other => other,
        };
        let as_str = match &inner {
            DVal::Str(s) | DVal::Path(s) => Some(s.clone()),
            _ => None,
        };
        match code {
            F_PATH => path = as_str,
            F_INTERFACE => iface = as_str,
            F_MEMBER => member = as_str,
            F_ERROR_NAME => error_name = as_str,
            F_REPLY_SERIAL => {
                if let DVal::U32(n) = inner {
                    reply_to = Some(n);
                }
            }
            F_DESTINATION => {}
            F_SIGNATURE => sig = as_str.unwrap_or_default(),
            F_SENDER => sender = as_str,
            _ => {}
        }
    }
    let body_start = fields_end + (8 - fields_end % 8) % 8;
    if buf.len() < body_start + body_len {
        return Err(format!(
            "body truncated (have {}, need {})",
            buf.len(),
            body_start + body_len
        ));
    }
    Ok((
        InMsg {
            mtype,
            serial,
            reply_to,
            sender,
            path,
            member,
            iface,
            error_name,
            sig,
            body: buf[body_start..body_start + body_len].to_vec(),
        },
        body_start + body_len,
    ))
}

/// Parses a body against a known signature.
pub(crate) fn parse_body(body: &[u8], sig: &str) -> Result<Vec<DVal>, String> {
    let types = parse_sig(sig)?;
    let mut cur = Cursor { buf: body, off: 0 };
    let mut out = Vec::with_capacity(types.len());
    for t in &types {
        out.push(cur.value(0, t)?);
    }
    if cur.off != body.len() {
        return Err(format!(
            "trailing body bytes ({} of {})",
            body.len() - cur.off,
            body.len()
        ));
    }
    Ok(out)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sig_model_aligns_per_spec() {
        assert_eq!(parse_sig("ssa{sv}").expect("sig").len(), 3);
        let (t, n) = parse_type(b"a(sa(us))").expect("nested");
        assert_eq!(n, 9);
        assert!(matches!(t, Type::Array(_)));
        assert_eq!(Type::Struct(vec![]).align(), 8);
        assert_eq!(Type::U32.align(), 4);
        assert_eq!(Type::Byte.align(), 1);
    }

    #[test]
    fn values_round_trip_through_signatures() {
        let cases: Vec<(&str, DVal)> = vec![
            ("b", DVal::Bool(true)),
            ("u", DVal::U32(42)),
            ("s", DVal::Str("title".to_string())),
            ("o", DVal::Path("/a/b".to_string())),
            ("ay", DVal::Bytes(vec![1, 2, 3])),
            (
                "a{sv}",
                DVal::Array(vec![
                    DVal::Struct(vec![
                        DVal::Str("multiple".to_string()),
                        DVal::Variant(Box::new(DVal::Bool(true))),
                    ]),
                    DVal::Struct(vec![
                        DVal::Str("filters".to_string()),
                        DVal::Variant(Box::new(DVal::Array(vec![DVal::Struct(vec![
                            DVal::Str("Images".to_string()),
                            DVal::Array(vec![DVal::Struct(vec![
                                DVal::U32(0),
                                DVal::Str("*.png".to_string()),
                            ])]),
                        ])]))),
                    ]),
                ]),
            ),
            (
                "(ua{sv})",
                DVal::Struct(vec![
                    DVal::U32(0),
                    DVal::Array(vec![DVal::Struct(vec![
                        DVal::Str("uris".to_string()),
                        DVal::Variant(Box::new(DVal::Array(vec![DVal::Str(
                            "file:///tmp/a.txt".to_string(),
                        )]))),
                    ])]),
                ]),
            ),
        ];
        for (sig, val) in &cases {
            let types = parse_sig(sig).expect("static sig");
            assert_eq!(types.len(), 1);
            let mut buf = Vec::new();
            marshal_value(&mut buf, 0, &types[0], val)
                .unwrap_or_else(|e| panic!("marshal {sig}: {e}"));
            let back = parse_body(&buf, sig).unwrap_or_else(|e| panic!("parse {sig}: {e}"));
            assert_eq!(back, vec![val.clone()], "round-trip {sig}");
        }
    }

    #[test]
    fn method_call_frames_and_parses() {
        let bytes = build_method_call(
            7,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.FileChooser",
            "OpenFile",
            "ssa{sv}",
            &[
                DVal::Str(String::new()),
                DVal::Str("Pick".to_string()),
                DVal::Array(vec![]),
            ],
        )
        .expect("frames");
        let (msg, consumed) = parse_message(&bytes).expect("parses");
        assert_eq!(consumed, bytes.len(), "exact framing");
        assert_eq!(msg.mtype, MSG_METHOD_CALL);
        assert_eq!(msg.serial, 7);
        assert_eq!(msg.member.as_deref(), Some("OpenFile"));
        assert_eq!(msg.sig, "ssa{sv}");
        let body = parse_body(&msg.body, &msg.sig).expect("body parses");
        assert_eq!(body.len(), 3);
    }

    #[test]
    fn truncation_refuses_loudly() {
        assert!(parse_message(b"l\x01").is_err(), "short header");
        let bytes = build_method_call(1, "d", "/p", "i", "M", "s", &[DVal::Str("x".into())])
            .expect("frames");
        assert!(
            parse_message(&bytes[..bytes.len() - 3]).is_err(),
            "cut body"
        );
        assert!(parse_body(b"\x01\x00", "s").is_err(), "cut string");
        assert!(parse_sig("a(").is_err(), "open array");
        assert!(parse_sig("z").is_err(), "bad type byte");
    }

    #[test]
    fn address_forms_parse_or_refuse() {
        assert_eq!(
            parse_address("unix:path=/run/user/1000/bus;autolaunch:").expect("unix path"),
            BusAddr::UnixPath("/run/user/1000/bus".to_string())
        );
        assert_eq!(
            parse_address("unix:path=/run/user/1000/bus,guid=abc").expect("guid suffix"),
            BusAddr::UnixPath("/run/user/1000/bus".to_string())
        );
        assert_eq!(
            parse_address("tcp:host=127.0.0.1,port=99").expect("tcp"),
            BusAddr::Tcp {
                host: "127.0.0.1".to_string(),
                port: 99
            }
        );
        assert!(parse_address("tcp:host=x").is_err(), "tcp needs port");
        assert!(parse_address("nonce:foo=1").is_err(), "unknown transport");
        #[cfg(unix)]
        assert!(
            matches!(
                parse_address("unix:abstract=/tmp/x"),
                Ok(BusAddr::UnixAbstract(_))
            ),
            "abstract parses on unix"
        );
        #[cfg(not(unix))]
        assert!(
            parse_address("unix:abstract=/tmp/x").is_err(),
            "abstract refuses loudly off unix"
        );
        assert!(parse_address("").is_err(), "empty refuses");
    }

    /// Loopback D-Bus peer: AUTH-OKs, answers Hello/AddMatch/calls
    /// with canned replies, then emits one Response signal — the
    /// whole client flow against scripted bytes, no real bus.
    fn fake_peer(
        listener: std::net::TcpListener,
        handle: String,
        uris: Vec<String>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().expect("accept");
            s.set_read_timeout(Some(Duration::from_secs(10)))
                .expect("timeout");
            // AUTH line.
            let mut got = Vec::new();
            loop {
                let mut one = [0u8; 1];
                s.read_exact(&mut one).expect("auth byte");
                got.push(one[0]);
                if got.len() >= 2 && got[got.len() - 2..] == *b"\r\n" {
                    break;
                }
            }
            assert!(got.starts_with(b"AUTH EXTERNAL"), "auth shape");
            s.write_all(b"OK deadbeef\r\n").expect("ok");
            // BEGIN line.
            let mut got = Vec::new();
            loop {
                let mut one = [0u8; 1];
                s.read_exact(&mut one).expect("begin byte");
                got.push(one[0]);
                if got.len() >= 2 && got[got.len() - 2..] == *b"\r\n" {
                    break;
                }
            }
            // Framed calls: Hello, AddMatch, OpenFile — reply in kind.
            for expect_member in ["Hello", "AddMatch", "OpenFile"] {
                let msg = read_one(&mut s);
                assert_eq!(msg.member.as_deref(), Some(expect_member));
                let reply_sig;
                let reply_vals: Vec<DVal>;
                if expect_member == "Hello" {
                    reply_sig = "s";
                    reply_vals = vec![DVal::Str(":9.99".to_string())];
                } else if expect_member == "OpenFile" {
                    reply_sig = "o";
                    reply_vals = vec![DVal::Path(handle.clone())];
                } else {
                    reply_sig = "";
                    reply_vals = vec![];
                }
                s.write_all(&reply_msg(msg.serial, reply_sig, &reply_vals))
                    .expect("reply");
            }
            // The Response signal for the awaited handle.
            let mut uri_vals = Vec::new();
            for u in &uris {
                uri_vals.push(DVal::Str(u.clone()));
            }
            s.write_all(&signal_msg(
                &handle,
                "(ua{sv})",
                &[DVal::Struct(vec![
                    DVal::U32(0),
                    DVal::Array(vec![DVal::Struct(vec![
                        DVal::Str("uris".to_string()),
                        DVal::Variant(Box::new(DVal::Array(uri_vals))),
                    ])]),
                ])],
            ))
            .expect("signal");
            // Hold the socket briefly so the client read lands.
            std::thread::sleep(Duration::from_millis(200));
        })
    }

    fn read_exact_stream(s: &mut std::net::TcpStream, n: usize) -> Vec<u8> {
        let mut buf = vec![0u8; n];
        s.read_exact(&mut buf).expect("peer read");
        buf
    }

    fn read_one(s: &mut std::net::TcpStream) -> InMsg {
        let head = read_exact_stream(s, 16);
        let fl = u32::from_le_bytes([head[12], head[13], head[14], head[15]]) as usize;
        let rest = read_exact_stream(s, fl);
        let mut prefix = head;
        prefix.extend_from_slice(&rest);
        // Body length sits at bytes 4..8; the body itself starts
        // 8-aligned past the fields — the pad bytes between are
        // message bytes too, so they must be READ (not just
        // zero-filled locally), or every later message misaligns.
        let body_len = u32::from_le_bytes([prefix[4], prefix[5], prefix[6], prefix[7]]) as usize;
        let body_start = prefix.len() + (8 - prefix.len() % 8) % 8;
        let mut full = prefix;
        full.extend_from_slice(&read_exact_stream(s, body_start - full.len() + body_len));
        let (msg, _) = parse_message(&full).expect("peer parses client");
        msg
    }

    fn reply_msg(serial: u32, sig: &str, vals: &[DVal]) -> Vec<u8> {
        // Minimal METHOD_RETURN with REPLY_SERIAL (+ SIGNATURE).
        let types = parse_sig(sig).expect("static");
        let mut body = Vec::new();
        for (t, v) in types.iter().zip(vals.iter()) {
            marshal_value(&mut body, 0, t, v).expect("static");
        }
        let mut msg = vec![b'l', MSG_METHOD_RETURN, 0, 1];
        msg.extend_from_slice(&(body.len() as u32).to_le_bytes());
        msg.extend_from_slice(&99u32.to_le_bytes());
        while msg.len() % 8 != 0 {
            msg.push(0);
        }
        // Fields: REPLY_SERIAL + SIGNATURE when non-empty, built
        // with the production encoder (the helper must not drift
        // from it — that drift is exactly what this test pins).
        let mut fields = Vec::new();
        field(&mut fields, 16, F_REPLY_SERIAL, &DVal::U32(serial)).expect("static");
        if !sig.is_empty() {
            field(&mut fields, 16, F_SIGNATURE, &DVal::Sig(sig.to_string())).expect("static");
        }
        let mut framed = Vec::new();
        framed.extend_from_slice(&msg[..12]);
        framed.extend_from_slice(&(fields.len() as u32).to_le_bytes());
        framed.extend_from_slice(&fields);
        pad8(&mut framed, 0);
        framed.extend_from_slice(&body);
        framed
    }

    fn signal_msg(handle: &str, sig: &str, vals: &[DVal]) -> Vec<u8> {
        let types = parse_sig(sig).expect("static");
        let mut body = Vec::new();
        for (t, v) in types.iter().zip(vals.iter()) {
            marshal_value(&mut body, 0, t, v).expect("static");
        }
        let mut msg = vec![b'l', MSG_SIGNAL, 0, 1];
        msg.extend_from_slice(&(body.len() as u32).to_le_bytes());
        msg.extend_from_slice(&77u32.to_le_bytes());
        pad8(&mut msg, 0);
        let mut fields = Vec::new();
        // Built with the production encoder (absolute base 16 —
        // detached buffers misalign variants, which is exactly the
        // bug this helper once had).
        field(&mut fields, 16, F_PATH, &DVal::Path(handle.to_string())).expect("static");
        field(
            &mut fields,
            16,
            F_INTERFACE,
            &DVal::Str("org.freedesktop.portal.Request".to_string()),
        )
        .expect("static");
        field(
            &mut fields,
            16,
            F_MEMBER,
            &DVal::Str("Response".to_string()),
        )
        .expect("static");
        field(&mut fields, 16, F_SIGNATURE, &DVal::Sig(sig.to_string())).expect("static");
        let mut framed = Vec::new();
        framed.extend_from_slice(&msg[..12]);
        framed.extend_from_slice(&(fields.len() as u32).to_le_bytes());
        framed.extend_from_slice(&fields);
        pad8(&mut framed, 0);
        framed.extend_from_slice(&body);
        framed
    }

    /// Peer-constructed replies and signals parse through the same
    /// production parser (pins the fake peer honest — a malformed
    /// canned reply fails here, not in the loopback test).
    #[test]
    fn peer_reply_and_signal_frame_exactly() {
        let reply = reply_msg(41, "s", &[DVal::Str(":9.99".to_string())]);
        let (msg, consumed) = parse_message(&reply).expect("reply parses");
        assert_eq!(consumed, reply.len());
        assert_eq!(msg.mtype, MSG_METHOD_RETURN);
        assert_eq!(msg.reply_to, Some(41));
        assert_eq!(msg.sig, "s");
        let signal = signal_msg(
            "/org/freedesktop/portal/desktop/request/9_99/oppa0",
            "(ua{sv})",
            &[DVal::Struct(vec![DVal::U32(0), DVal::Array(vec![])])],
        );
        let (sig, consumed) = parse_message(&signal).expect("signal parses");
        assert_eq!(consumed, signal.len());
        assert_eq!(sig.mtype, MSG_SIGNAL);
        assert_eq!(sig.member.as_deref(), Some("Response"));
        assert_eq!(sig.sig, "(ua{sv})");
        assert_eq!(
            sig.path.as_deref(),
            Some("/org/freedesktop/portal/desktop/request/9_99/oppa0")
        );
    }

    #[test]
    fn hello_call_and_response_flow_over_loopback() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        let port = listener.local_addr().expect("port").port();
        let handle = "/org/freedesktop/portal/desktop/request/9_99/oppa0".to_string();
        let peer = fake_peer(
            listener,
            handle.clone(),
            vec!["file:///tmp/a%20b.txt".to_string()],
        );
        let stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let mut conn = BusConn::handshake(stream, Duration::from_secs(5)).expect("handshake");
        assert_eq!(conn.sender(), ":9.99");
        let mut stashed = Vec::new();
        // Empty-body call first (the AddMatch shape the dialog
        // layer subscribes with — serials advance, nothing stashes).
        conn.call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "AddMatch",
            "s",
            &[DVal::Str("type='signal'".to_string())],
            &mut stashed,
            Duration::from_secs(5),
        )
        .expect("addmatch replies");
        let reply = conn
            .call(
                "org.freedesktop.portal.Desktop",
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.FileChooser",
                "OpenFile",
                "ssa{sv}",
                &[
                    DVal::Str(String::new()),
                    DVal::Str("Pick".to_string()),
                    DVal::Array(vec![]),
                ],
                &mut stashed,
                Duration::from_secs(5),
            )
            .expect("open call replies");
        let vals = parse_body(&reply.body, &reply.sig).expect("handle parses");
        assert_eq!(vals, vec![DVal::Path(handle.clone())]);
        // The signal the peer emitted after the reply lands in the
        // stash on the next call… instead read it directly: one more
        // framed message is waiting.
        let sig = conn
            .read_message("signal", Duration::from_secs(5))
            .expect("signal arrives");
        assert_eq!(sig.mtype, MSG_SIGNAL);
        assert_eq!(sig.member.as_deref(), Some("Response"));
        assert_eq!(sig.path.as_deref(), Some(handle.as_str()));
        let body = parse_body(&sig.body, &sig.sig).expect("response parses");
        assert_eq!(
            body,
            vec![DVal::Struct(vec![
                DVal::U32(0),
                DVal::Array(vec![DVal::Struct(vec![
                    DVal::Str("uris".to_string()),
                    DVal::Variant(Box::new(DVal::Array(vec![DVal::Str(
                        "file:///tmp/a%20b.txt".to_string()
                    )]))),
                ])]),
            ])]
        );
        peer.join().expect("peer exits");
        assert!(stashed.is_empty(), "no stray signals swallowed");
    }
}
