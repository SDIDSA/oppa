use super::sig::parse_type;
use super::*;

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
