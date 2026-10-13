use super::scan::{
    find_word, is_ident_byte, is_space_byte, is_word_at_bytes, match_angle, skip_char_lifetime,
    skip_string, split_top_level, strip_word,
};

/// Parses `pub struct Name<...>(...) where ... { ... }` headers at
/// string level (the crate's zero-dependency rule — same scanner
/// class as the lint helpers below). Rejects enums/unions and
/// lowercase names loudly, like before.
pub(crate) fn parse_props_struct(src: &str) -> Result<String, String> {
    // Attributes ride the derive input (`#[doc = "..."]`, sibling
    // derives) — blank them first (length-stable, so positions hold)
    // or program text in disguise ("struct" inside a doc comment)
    // poisons the keyword scan.
    let clean = blank_attributes(src);
    let bytes = clean.as_bytes();
    let at = find_word(bytes, b"struct").ok_or_else(|| {
        "#[derive(Props)] only supports `struct Name { ... }` (found no struct item)".to_string()
    })?;
    let mut i = at + 6;
    while i < bytes.len() && is_space_byte(bytes[i]) {
        i += 1;
    }
    let name_start = i;
    while i < bytes.len() && is_ident_byte(bytes[i]) {
        i += 1;
    }
    let name = src[name_start..i].to_string();
    if name.is_empty() || !name.chars().next().is_some_and(|c| c.is_uppercase()) {
        return Err("#[derive(Props)] needs an Uppercase struct name".to_string());
    }
    if name == "Vec" {
        return Err("#[derive(Props)] needs an Uppercase struct name".to_string());
    }
    while i < bytes.len() && is_space_byte(bytes[i]) {
        i += 1;
    }
    // Generic params: `<...>` with nesting (bounds carry `Fn(A, B)`,
    // arrays, and deeper generics — depth-aware, never naive).
    // `has_params` distinguishes `struct Name` from `struct Name<>`
    // (the latter is a loud error, never a silent plain impl).
    let mut params = String::new();
    let mut has_params = false;
    if i < bytes.len() && bytes[i] == b'<' {
        let close = match_angle(bytes, i)
            .ok_or_else(|| format!("struct {name} has unbalanced `<...>` params"))?;
        params = src[i + 1..close].trim().to_string();
        has_params = true;
        i = close + 1;
        while i < bytes.len() && is_space_byte(bytes[i]) {
            i += 1;
        }
    }
    // Optional `where ...` up to the body `{` (or `;` for unit
    // structs — a generic unit struct is legal Rust).
    let mut where_clause = String::new();
    if is_word_at_bytes(bytes, i, b"where") {
        let mut j = i + 5;
        let mut depth = 0usize;
        while j < bytes.len() {
            match bytes[j] {
                b'(' | b'[' => depth += 1,
                b')' | b']' => {
                    depth = depth.saturating_sub(1);
                }
                b'{' | b';' if depth == 0 => break,
                _ => {}
            }
            j += 1;
        }
        where_clause = src[i + 5..j].trim().to_string();
    }
    if !has_params {
        return Ok(format!("impl ::oppa::component::Props for {name} {{}}"));
    }
    // Classify params (top-level commas only): lifetimes gain
    // `'a: 'static`, consts ride free, types gain `Clone + 'static`.
    let mut args = Vec::new();
    let mut bounds = Vec::new();
    for param in split_top_level(&params) {
        let param = param.trim();
        if param.is_empty() {
            continue;
        }
        if let Some(body) = param.strip_prefix('\'') {
            let end = body
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map(|e| e + 1)
                .unwrap_or(param.len());
            let lt = param[..end].to_string();
            args.push(lt.clone());
            bounds.push(format!("{lt}: 'static"));
        } else if strip_word(param, "const").is_some() {
            let rest = strip_word(param, "const").unwrap_or_default();
            let end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            args.push(rest[..end].to_string());
        } else {
            let end = param
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(param.len());
            let ty = param[..end].to_string();
            if ty.is_empty() {
                return Err(format!("struct {name} has an unparsable param `{param}`"));
            }
            args.push(ty.clone());
            bounds.push(format!("{ty}: Clone + 'static"));
        }
    }
    if args.is_empty() {
        return Err(format!("struct {name} has empty `<>` params"));
    }
    let mut where_part = where_clause;
    for b in &bounds {
        if !where_part.is_empty() {
            where_part.push_str(", ");
        }
        where_part.push_str(b);
    }
    Ok(format!(
        "impl<{params}> ::oppa::component::Props for {name}<{args}> where {where_part} {{}}",
        params = params,
        args = args.join(", "),
        where_part = where_part
    ))
}

/// Blanks attributes and comments with spaces (length-stable —
/// keyword scans run on the output, slicing on the input).
/// Stringified derive inputs keep raw `///` doc lines (not just
/// `#[doc]`), so both comment forms go — plus `#[...]` (the
/// sibling-derive + doc-attr form). String/char literals stay
/// (only program text outside them is scannable).
/// Stringified inputs space the opener (`# [doc = ...]`), so
/// whitespace between `#` and `[` is skipped.
pub(crate) fn blank_attributes(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = vec![b' '; b.len()];
    let mut i = 0;
    while i < b.len() {
        // Strings/chars first: a `//` or `#` inside one is program
        // text in disguise, never a comment/attribute opener.
        if b[i] == b'"' {
            i = skip_string(b, i);
            continue;
        }
        if b[i] == b'\'' {
            i = skip_char_lifetime(b, i);
            continue;
        }
        // Line comment to end of line.
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            i += 2;
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Block comment (nested).
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            let mut depth = 1usize;
            i += 2;
            while i < b.len() && depth > 0 {
                if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        if b[i] == b'#' {
            let mut j = i + 1;
            while j < b.len() && is_space_byte(b[j]) {
                j += 1;
            }
            if j < b.len() && b[j] == b'[' {
                j += 1;
                let mut depth = 1usize;
                while j < b.len() && depth > 0 {
                    if b[j] == b'"' {
                        j = skip_string(b, j);
                        continue;
                    }
                    if b[j] == b'\'' {
                        j = skip_char_lifetime(b, j);
                        continue;
                    }
                    if b[j] == b'[' {
                        depth += 1;
                    } else if b[j] == b']' {
                        depth -= 1;
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
        }
        out[i] = b[i];
        i += 1;
    }
    String::from_utf8(out).expect("attribute blanking is byte-preserving ASCII")
}
