use super::parse::Cursor;
use super::sig::parse_sig;
use super::*;

// ---------------------------------------------------------------------------
// Messages (framing + header fields we route on)
// ---------------------------------------------------------------------------

pub(crate) const MSG_METHOD_CALL: u8 = 1;
pub(crate) const MSG_METHOD_RETURN: u8 = 2;
pub(crate) const MSG_ERROR: u8 = 3;
pub(crate) const MSG_SIGNAL: u8 = 4;

/// Header field codes we emit/route on (sender included — the
/// Response matcher keys on path, but the sender names the peer).
pub(super) const F_PATH: u8 = 1;
pub(super) const F_INTERFACE: u8 = 2;
pub(super) const F_MEMBER: u8 = 3;
pub(super) const F_ERROR_NAME: u8 = 4;
pub(super) const F_REPLY_SERIAL: u8 = 5;
pub(super) const F_DESTINATION: u8 = 6;
pub(super) const F_SIGNATURE: u8 = 7;
pub(super) const F_SENDER: u8 = 8;

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
pub(super) fn pad8(buf: &mut Vec<u8>, base: usize) {
    while !(base + buf.len()).is_multiple_of(8) {
        buf.push(0);
    }
}

pub(super) fn field(buf: &mut Vec<u8>, base: usize, code: u8, v: &DVal) -> Result<(), String> {
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
