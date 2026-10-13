pub(crate) fn is_space_byte(b: u8) -> bool {
    b == b' ' || b == b'\t' || b == b'\n' || b == b'\r'
}

/// Strips a leading keyword (`const`) when it stands as a word.
pub(crate) fn strip_word<'a>(s: &'a str, word: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(word)?;
    if rest.is_empty()
        || !(rest.as_bytes()[0].is_ascii_alphanumeric() || rest.as_bytes()[0] == b'_')
    {
        Some(rest.trim_start())
    } else {
        None
    }
}

/// Matches the `<` opened at `open` (which must be `<`); returns the
/// index of its closer. Nests `<>`/`()`/`[]`/`{}` and skips string,
/// char, and lifetime-quoted spans so a `<` in disguise never
/// unbalances the scan.
pub(crate) fn match_angle(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => {
                // `>>` closes two (TokenStream display spaces them —
                // either spelling balances here).
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'(' | b'[' | b'{' => {
                if let Some(end) = match_group(bytes, i) {
                    i = end + 1;
                    continue;
                }
                return None;
            }
            b'"' => {
                i = skip_string(bytes, i);
                continue;
            }
            b'\'' => {
                i = skip_char_lifetime(bytes, i);
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Matches any of `(`/`[`/`{` opened at `open`; returns its closer.
pub(crate) fn match_group(bytes: &[u8], open: usize) -> Option<usize> {
    let (o, c) = match bytes[open] {
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
        b'{' => (b'{', b'}'),
        _ => return None,
    };
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        if bytes[i] == o {
            depth += 1;
        } else if bytes[i] == c {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        } else if bytes[i] == b'"' {
            i = skip_string(bytes, i);
            continue;
        } else if bytes[i] == b'\'' {
            i = skip_char_lifetime(bytes, i);
            continue;
        }
        i += 1;
    }
    None
}

pub(crate) fn skip_string(bytes: &[u8], open: usize) -> usize {
    let mut j = open + 1;
    while j < bytes.len() {
        if bytes[j] == b'\\' {
            j += 2;
            continue;
        }
        if bytes[j] == b'"' {
            return j + 1;
        }
        j += 1;
    }
    bytes.len()
}

/// Splits on top-level commas (nesting `<>`/`()`/`[]`/`{}` respected;
/// strings/chars skipped — a bound like `T: Fn(A, B)` never splits).
pub(crate) fn split_top_level(s: &str) -> Vec<String> {
    let bytes = s.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'<' | b'(' | b'[' | b'{' => depth += 1,
            b'>' | b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
            }
            b',' if depth == 0 => {
                parts.push(s[start..i].to_string());
                start = i + 1;
            }
            b'"' => {
                i = skip_string(bytes, i);
                continue;
            }
            b'\'' => {
                i = skip_char_lifetime(bytes, i);
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(s[start..].to_string());
    parts
}

pub(crate) fn is_word_at_bytes(haystack: &[u8], at: usize, word: &[u8]) -> bool {
    if at + word.len() > haystack.len() || &haystack[at..at + word.len()] != word {
        return false;
    }
    let before_ok = at == 0 || !is_ident_byte(haystack[at - 1]);
    let after_ok = at + word.len() == haystack.len() || !is_ident_byte(haystack[at + word.len()]);
    before_ok && after_ok
}

pub(crate) fn find_word(haystack: &[u8], word: &[u8]) -> Option<usize> {
    if word.is_empty() || haystack.len() < word.len() {
        return None;
    }
    let mut i = 0;
    while i + word.len() <= haystack.len() {
        if &haystack[i..i + word.len()] == word {
            let before_ok = i == 0 || !is_ident_byte(haystack[i - 1]);
            let after_ok =
                i + word.len() == haystack.len() || !is_ident_byte(haystack[i + word.len()]);
            if before_ok && after_ok {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

pub(crate) fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Matches the brace opened at `open` (which must be `{`); returns the
/// index of its closer. Operates on noise-stripped text only.
pub(crate) fn match_brace(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            depth += 1;
        } else if bytes[i] == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Skips a char literal or lifetime starting at the `'` at `q`.
pub(crate) fn skip_char_lifetime(b: &[u8], q: usize) -> usize {
    if q + 2 < b.len() && b[q + 1] != b'\\' && b[q + 2] == b'\'' {
        return q + 3;
    }
    if q + 3 < b.len() && b[q + 1] == b'\\' && b[q + 3] == b'\'' {
        return q + 4;
    }
    q + 1
}
