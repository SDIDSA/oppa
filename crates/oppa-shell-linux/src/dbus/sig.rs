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
    pub(super) fn align(&self) -> usize {
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
pub(super) fn parse_type(sig: &[u8]) -> Result<(Type, usize), String> {
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
pub(super) fn parse_sig(sig: &str) -> Result<Vec<Type>, String> {
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
