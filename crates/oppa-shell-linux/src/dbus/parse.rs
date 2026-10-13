use super::sig::parse_type;
use super::*;

// ---------------------------------------------------------------------------
// Parsing (cursor over a byte slice — truncation refuses loudly)
// ---------------------------------------------------------------------------

pub(super) struct Cursor<'a> {
    pub(super) buf: &'a [u8],
    pub(super) off: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn need(&self, n: usize) -> Result<(), String> {
        if self.off + n > self.buf.len() {
            return Err(format!(
                "truncated message at offset {} (need {n})",
                self.off
            ));
        }
        Ok(())
    }

    pub(super) fn align(&mut self, base: usize, align: usize) {
        while !(base + self.off).is_multiple_of(align) {
            self.off += 1;
        }
    }

    pub(super) fn u8v(&mut self) -> Result<u8, String> {
        self.need(1)?;
        let v = self.buf[self.off];
        self.off += 1;
        Ok(v)
    }

    pub(super) fn u16v(&mut self, base: usize) -> Result<u16, String> {
        self.align(base, 2);
        self.need(2)?;
        let v = u16::from_le_bytes([self.buf[self.off], self.buf[self.off + 1]]);
        self.off += 2;
        Ok(v)
    }

    pub(super) fn u32v(&mut self, base: usize) -> Result<u32, String> {
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

    pub(super) fn strv(&mut self, base: usize) -> Result<String, String> {
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

    pub(super) fn sigv(&mut self) -> Result<String, String> {
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
    pub(super) fn value(&mut self, base: usize, ty: &Type) -> Result<DVal, String> {
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
