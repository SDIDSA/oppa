//! Compile-time authoring lints (M2b: §8.1 re-seed lint support, §9.6
//! state lint).
//!
//! [`check_no_ambient_state`] is `const`-compatible so `#[hot_crate]` can
//! run it inside `const _: () = assert!(...)` — a hot crate holding
//! `static` / `thread_local!` / `OnceCell` state fails the *build*, not a
//! review checklist. Anything that must survive a swap lives core-side in
//! reactive storage (lock #25); hot crates hold no surviving state.
//!
//! Scanner limits, stated (a lint is a backstop, not a proof):
//!
//! - Operates file-at-a-time: `#[hot_crate]` checks the annotated file or
//!   module. `#[path]`-remapped modules are rejected loudly.
//! - `use ...;` statements and `#[...]` attributes are skipped (importing
//!   `thread_local` is not holding state).
//! - Comment, string, char, byte-string, and raw-string contents are
//!   skipped. Lifetimes (`'a`) are not char literals (lookahead heuristic).

/// Clean verdict of [`check_no_ambient_state`].
pub const CLEAN: u32 = u32::MAX;

/// Returns [`CLEAN`] if `src` holds no ambient hot-crate state, else the
/// byte offset of the first violating keyword. Keywords (word-boundary
/// matched): `static`, `thread_local`, `OnceCell`, `LazyLock`,
/// `lazy_static`.
pub const fn check_no_ambient_state(src: &str) -> u32 {
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        let b = bytes[i];
        // Line comment: skip to newline.
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            i += 2;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Block comment: skip nested to close.
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
            let mut depth = 1usize;
            i += 2;
            while i < len && depth > 0 {
                if bytes[i] == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        // Byte raw string `br"..."`, `br#"..."#`: same skip as raw.
        if b == b'b'
            && i + 2 < len
            && bytes[i + 1] == b'r'
            && (bytes[i + 2] == b'"' || bytes[i + 2] == b'#')
        {
            let mut hashes = 0usize;
            let mut j = i + 2;
            while j < len && bytes[j] == b'#' {
                hashes += 1;
                j += 1;
            }
            if j < len && bytes[j] == b'"' {
                i = skip_raw_string(bytes, len, j + 1, hashes);
                continue;
            }
        }
        // Raw string `r"..."`, `r#"..."#`, ...: skip to matching close.
        if b == b'r' && i + 1 < len && (bytes[i + 1] == b'"' || bytes[i + 1] == b'#') {
            let mut hashes = 0usize;
            let mut j = i + 1;
            while j < len && bytes[j] == b'#' {
                hashes += 1;
                j += 1;
            }
            if j < len && bytes[j] == b'"' {
                j += 1;
                i = skip_raw_string(bytes, len, j, hashes);
                continue;
            }
        }
        // Ordinary / byte string: skip with escapes.
        if b == b'"' || (b == b'b' && i + 1 < len && bytes[i + 1] == b'"') {
            i = skip_quoted(bytes, len, if b == b'"' { i } else { i + 1 }, b'"');
            continue;
        }
        // Char / byte-char literal vs. lifetime: `'x'` / `'\n'` scan;
        // anything else is a lifetime — advance one.
        if b == b'\'' || (b == b'b' && i + 1 < len && bytes[i + 1] == b'\'') {
            let q = if b == b'\'' { i } else { i + 1 };
            i = skip_char_or_lifetime(bytes, len, q);
            continue;
        }
        // Attribute `#[...]` / `#![...]`: skip balanced brackets.
        if b == b'#' && i + 1 < len && bytes[i + 1] == b'[' {
            i = skip_balanced(bytes, len, i + 1, b'[', b']');
            continue;
        }
        // Identifier run: keyword check with word boundaries.
        if is_ident_start(b) {
            let start = i;
            while i < len && is_ident_char(bytes[i]) {
                i += 1;
            }
            let word_len = i - start;
            if is_banned(bytes, start, word_len) {
                // `use ...;` imports are not state — skip the statement.
                if is_use_statement(bytes, start) {
                    while i < len && bytes[i] != b';' {
                        i += 1;
                    }
                    continue;
                }
                return start as u32;
            }
            continue;
        }
        i += 1;
    }
    CLEAN
}

const fn skip_raw_string(bytes: &[u8], len: usize, mut j: usize, hashes: usize) -> usize {
    while j < len {
        if bytes[j] == b'"' {
            let mut k = 0usize;
            while k < hashes && j + 1 + k < len && bytes[j + 1 + k] == b'#' {
                k += 1;
            }
            if k == hashes {
                return j + 1 + hashes;
            }
        }
        j += 1;
    }
    len
}

const fn skip_quoted(bytes: &[u8], len: usize, quote_at: usize, quote: u8) -> usize {
    let mut j = quote_at + 1;
    while j < len {
        if bytes[j] == b'\\' {
            j += 2;
            continue;
        }
        if bytes[j] == quote {
            return j + 1;
        }
        j += 1;
    }
    len
}

const fn skip_char_or_lifetime(bytes: &[u8], len: usize, q: usize) -> usize {
    // `'x'`
    if q + 2 < len && bytes[q + 1] != b'\\' && bytes[q + 2] == b'\'' {
        return q + 3;
    }
    // `'\e'` (escape, one char)
    if q + 3 < len && bytes[q + 1] == b'\\' && bytes[q + 3] == b'\'' {
        return q + 4;
    }
    // Otherwise a lifetime like `'a` — advance past the quote only.
    q + 1
}

const fn skip_balanced(bytes: &[u8], len: usize, mut j: usize, open: u8, close: u8) -> usize {
    let mut depth = 1usize;
    // `j` points at the opening bracket.
    j += 1;
    while j < len && depth > 0 {
        if bytes[j] == open {
            depth += 1;
        } else if bytes[j] == close {
            depth -= 1;
        }
        j += 1;
    }
    j
}

const fn is_ident_start(b: u8) -> bool {
    (b >= b'a' && b <= b'z') || (b >= b'A' && b <= b'Z') || b == b'_'
}

const fn is_ident_char(b: u8) -> bool {
    is_ident_start(b) || (b >= b'0' && b <= b'9')
}

const fn word_eq(bytes: &[u8], start: usize, len: usize, word: &[u8]) -> bool {
    if len != word.len() {
        return false;
    }
    let mut k = 0usize;
    while k < len {
        if bytes[start + k] != word[k] {
            return false;
        }
        k += 1;
    }
    true
}

const fn is_banned(bytes: &[u8], start: usize, len: usize) -> bool {
    word_eq(bytes, start, len, b"static")
        || word_eq(bytes, start, len, b"thread_local")
        || word_eq(bytes, start, len, b"OnceCell")
        || word_eq(bytes, start, len, b"LazyLock")
        || word_eq(bytes, start, len, b"lazy_static")
}

/// True if the identifier at `start` is the `use` keyword opening an
/// import statement (imports are not state).
const fn is_use_statement(bytes: &[u8], start: usize) -> bool {
    // The matched word must itself be `use`; callers only invoke this on
    // banned words, so reaching here with `use` is impossible — this
    // instead detects `pub use` / leading-`use` position by scanning back
    // past whitespace to the previous token. Simpler and sufficient: check
    // whether the line so far (back to newline/`;`/`{`/`}`) ends with the
    // `use` keyword — i.e. this word sits inside a use statement.
    let mut j = start;
    // Walk back over whitespace.
    while j > 0 && (bytes[j - 1] == b' ' || bytes[j - 1] == b'\t') {
        j -= 1;
    }
    // Walk back over a `::` path tail to the statement head.
    // Only handles the common `use a::b::Word;` shape: scan back to `use`.
    let mut k = j;
    while k > 0
        && bytes[k - 1] != b';'
        && bytes[k - 1] != b'\n'
        && bytes[k - 1] != b'{'
        && bytes[k - 1] != b'}'
    {
        k -= 1;
    }
    // `k..j` is the statement head region; check it starts with `use `.
    // Skip leading `pub`/`pub(...)` qualifiers crudely: look for `use`
    // as the first ident in the region.
    let mut m = k;
    while m < j && (bytes[m] == b' ' || bytes[m] == b'\t') {
        m += 1;
    }
    if m + 3 <= j && bytes[m] == b'u' && bytes[m + 1] == b's' && bytes[m + 2] == b'e' {
        let after = m + 3;
        if after >= j || bytes[after] == b' ' || bytes[after] == b'\t' || bytes[after] == b':' {
            return true;
        }
    }
    // `pub use ...`: skip a leading `pub` (+ optional `(...)`).
    if m + 3 <= j && bytes[m] == b'p' && bytes[m + 1] == b'u' && bytes[m + 2] == b'b' {
        m += 3;
        while m < j && (bytes[m] == b' ' || bytes[m] == b'\t') {
            m += 1;
        }
        if m < j && bytes[m] == b'(' {
            let mut depth = 1usize;
            m += 1;
            while m < j && depth > 0 {
                if bytes[m] == b'(' {
                    depth += 1;
                } else if bytes[m] == b')' {
                    depth -= 1;
                }
                m += 1;
            }
            while m < j && (bytes[m] == b' ' || bytes[m] == b'\t') {
                m += 1;
            }
        }
        if m + 3 <= j && bytes[m] == b'u' && bytes[m + 1] == b's' && bytes[m + 2] == b'e' {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_component_code_passes() {
        let src = r#"
use oppa::component::{Ctx, Props};
pub fn Toggle(ctx: &Ctx, props: &ToggleProps) -> VNode {
    let is_on = ctx.signal(props.initial); // state, not ambient
    Div("t").build()
}
"#;
        assert_eq!(check_no_ambient_state(src), CLEAN);
    }

    #[test]
    fn static_is_flagged_with_offset() {
        let src = "fn f() {}\nstatic CACHE: u32 = 0;\n";
        let off = check_no_ambient_state(src);
        assert_ne!(off, CLEAN);
        assert_eq!(&src[off as usize..off as usize + 6], "static");
    }

    #[test]
    fn thread_local_and_cells_flagged() {
        assert_ne!(
            check_no_ambient_state("thread_local!{ static X: u8 = 0; }"),
            CLEAN
        );
        assert_ne!(
            check_no_ambient_state("static X: OnceCell<u8> = OnceCell::new();"),
            CLEAN
        );
        assert_ne!(
            check_no_ambient_state("use LazyLock; static Y: LazyLock<u8> = foo();"),
            CLEAN
        );
        assert_ne!(
            check_no_ambient_state("lazy_static! { static Z: u8 = 1; }"),
            CLEAN
        );
    }

    #[test]
    fn imports_comments_strings_lifetimes_ignored() {
        let src = r#"
use std::thread_local;
use std::cell::OnceCell;
// static NOT_STATE: u8 = 0;
/* thread_local in a block comment */
/// docs mention `static` and "OnceCell" freely
fn f<'a>(x: &'a str) -> &'a str {
    let s = "static thread_local OnceCell";
    let r = "static in raw string";
    let c = 'x';
    x
}
"#;
        assert_eq!(check_no_ambient_state(src), CLEAN);
    }

    #[test]
    fn attributes_skipped() {
        assert_eq!(
            check_no_ambient_state("#[derive(Clone)]\nfn f() {}\n"),
            CLEAN
        );
    }

    #[test]
    fn raw_strings_skipped() {
        let src = r##"let r = r#"static"#, other = br#"thread_local"#; fn f() {}"##;
        assert_eq!(check_no_ambient_state(src), CLEAN);
    }
}
