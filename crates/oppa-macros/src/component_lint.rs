use super::scan::{is_ident_byte, match_brace, skip_char_lifetime};

// ---------------------------------------------------------------------------
// #[component] call-site lint — check_component_body (unit-tested below)
// ---------------------------------------------------------------------------

/// State-creation methods keyed by call-site hash (§5.1): invoking one
/// inside a nested `fn` or a closure collapses every invocation to one key.
const SITE_KEYED_METHODS: [&str; 3] = ["signal", "memo", "binding"];

/// Lints a stringified `#[component]` item. Returns one message per
/// hazard; empty means clean. Explicitly NOT a rewrite — the item passes
/// through untouched so `#[track_caller]` keeps working.
///
/// Flagged (run-varying execution count ⇒ silent shuffle, §5.1):
///
/// - creation calls inside nested `fn` items (one base key shared across
///   every external call site — order-fragile);
/// - creation calls inside `if` / `match` / `for` / `while` / `loop`
///   bodies (conditional/dynamic execution — arm taken or trip count
///   varies per run).
///
/// Spared by design: creation calls in loop-*combinator* bodies
/// (`.map(|slot| ...)` — the locked §4.2 virtualization pattern: one
/// base key per closure line, ordinals disambiguate prefix-stable
/// iterations, internal edits re-seed by line change). Dynamic-bound
/// combinators (`.map` over `0..store.len()`) are order-fragile on bound
/// change — accepted and documented (suffix revive/seed semantics), not
/// linted, since linting them would forbid the locked example.
///
/// Limits, stated: creation calls in `if`/`while` *conditions* (as
/// opposed to bodies) are not scanned; the reseed behavior tests
/// backstop the lint.
pub(crate) fn check_component_body(src: &str) -> Vec<String> {
    let mut lints = Vec::new();
    let open = match src.find('{') {
        Some(i) => i,
        None => {
            return vec!["#[component] requires a function body (§4)".to_string()];
        }
    };
    let close = match src.rfind('}') {
        Some(i) => i,
        None => {
            return vec!["#[component] requires a function body (§4)".to_string()];
        }
    };
    if close <= open {
        return vec!["#[component] requires a function body (§4)".to_string()];
    }
    let body = &src[open..=close];
    let clean = strip_noise(body);
    let bytes = clean.as_bytes();
    let conditional = conditional_regions(bytes);
    for (s, e) in &conditional {
        if let Some(method) = find_ctx_creation(&clean[*s..*e]) {
            lints.push(format!(
                "ctx.{method}() inside a conditional/loop body executes a run-varying number of times — later sites shuffle instead of re-seeding (§5.1); lift the creation call into unconditional body position (keyed_state with explicit keys covers data-driven cases)"
            ));
        }
    }
    let nested = nested_fn_regions(bytes);
    for (s, e) in &nested {
        if in_ranges(*s, &conditional) {
            continue; // already reported under the conditional rule
        }
        if let Some(method) = find_ctx_creation(&clean[*s..*e]) {
            lints.push(format!(
                "ctx.{method}() inside a nested `fn` item shares one base call-site key across every external call site — order-fragile shuffle instead of re-seed (§5.1); lift the creation call into the component body"
            ));
        }
    }
    lints
}

/// Replaces comments and string/char literals with spaces (length-stable)
/// so structural scanning never trips on program text in disguise.
pub(crate) fn strip_noise(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = vec![b' '; b.len()];
    let mut i = 0;
    while i < b.len() {
        // Line comment.
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
        // Raw strings, optional b/c/br/cr prefixes.
        if let Some(after) = raw_open(b, i) {
            i = skip_raw(b, after);
            continue;
        }
        // Ordinary strings, optional b/c prefix.
        if b[i] == b'"' || ((b[i] == b'b' || b[i] == b'c') && i + 1 < b.len() && b[i + 1] == b'"') {
            let mut j = if b[i] == b'"' { i + 1 } else { i + 2 };
            while j < b.len() {
                if b[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if b[j] == b'"' {
                    j += 1;
                    break;
                }
                j += 1;
            }
            i = j;
            continue;
        }
        // Char literal vs. lifetime.
        if b[i] == b'\'' || (b[i] == b'b' && i + 1 < b.len() && b[i + 1] == b'\'') {
            let q = if b[i] == b'\'' { i } else { i + 1 };
            i = skip_char_lifetime(b, q);
            continue;
        }
        // Attribute #[...]: blank it (doc strings would false-positive),
        // skipping strings nested inside.
        if b[i] == b'#' && i + 1 < b.len() && b[i + 1] == b'[' {
            let mut j = i + 2;
            let mut depth = 1usize;
            while j < b.len() && depth > 0 {
                if b[j] == b'"' {
                    let mut k = j + 1;
                    while k < b.len() {
                        if b[k] == b'\\' {
                            k += 2;
                            continue;
                        }
                        if b[k] == b'"' {
                            k += 1;
                            break;
                        }
                        k += 1;
                    }
                    j = k;
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
        out[i] = b[i];
        i += 1;
    }
    String::from_utf8(out).expect("noise stripping is byte-preserving ASCII")
}

/// If a raw-string opens at `i` (with optional `b`/`br`/`c`/`cr` prefix),
/// returns the index just past the opening quote.
pub(crate) fn raw_open(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i;
    if b[j] == b'b' || b[j] == b'c' {
        j += 1;
    }
    if j >= b.len() || b[j] != b'r' {
        // Not a raw string (`b"`/`c"` fall through to the plain-string path).
        return None;
    }
    j += 1;
    while j < b.len() && b[j] == b'#' {
        j += 1;
    }
    if j < b.len() && b[j] == b'"' {
        return Some(j + 1);
    }
    None
}

pub(crate) fn skip_raw(b: &[u8], mut j: usize) -> usize {
    // Re-derive the hash count from the opener: caller passed the index
    // past the quote, so walk back over `"` then `#`s.
    let mut hashes = 0usize;
    let mut k = j - 1;
    debug_assert_eq!(b[k], b'"');
    while k > 0 && b[k - 1] == b'#' {
        hashes += 1;
        k -= 1;
    }
    while j < b.len() {
        if b[j] == b'"' {
            let mut h = 0usize;
            while h < hashes && j + 1 + h < b.len() && b[j + 1 + h] == b'#' {
                h += 1;
            }
            if h == hashes {
                return j + 1 + hashes;
            }
        }
        j += 1;
    }
    b.len()
}

/// Byte ranges of nested `fn` item bodies (operates on noise-stripped
/// text; `fn`-pointer *types* like `fn()` are skipped, not flagged).
pub(crate) fn nested_fn_regions(clean: &[u8]) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    let mut i = 0;
    while i < clean.len() {
        if is_word_at(clean, i, b"fn") {
            let mut j = i + 2;
            while j < clean.len()
                && (clean[j] == b' ' || clean[j] == b'\t' || clean[j] == b'\n' || clean[j] == b'\r')
            {
                j += 1;
            }
            if j < clean.len() && clean[j] == b'(' {
                i = j + 1; // `fn()` type — not an item.
                continue;
            }
            // Item name (must be an ident; otherwise give up on this one).
            let name_start = j;
            while j < clean.len() && (clean[j].is_ascii_alphanumeric() || clean[j] == b'_') {
                j += 1;
            }
            if j == name_start {
                i += 2;
                continue;
            }
            // Scan to the body `{` (params/return/where hold no braces).
            while j < clean.len() && clean[j] != b'{' && clean[j] != b';' {
                j += 1;
            }
            if j < clean.len() && clean[j] == b'{' {
                if let Some(end) = match_brace(clean, j) {
                    regions.push((i, end + 1));
                    i = end + 1;
                    continue;
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }
    regions
}

pub(crate) fn is_word_at(haystack: &[u8], at: usize, word: &[u8]) -> bool {
    if at + word.len() > haystack.len() || &haystack[at..at + word.len()] != word {
        return false;
    }
    let before_ok = at == 0 || !is_ident_byte(haystack[at - 1]);
    let after_ok = at + word.len() == haystack.len() || !is_ident_byte(haystack[at + word.len()]);
    before_ok && after_ok
}

/// Byte ranges of `if` / `match` / `for` / `while` / `loop` bodies
/// (operates on noise-stripped text). For each keyword, the first `{` at
/// paren-depth 0 opens the region; matching continues over immediately
/// following `{` blocks (covers `} else {` chains and struct-literal
/// scrutinees before match arms). Conditions are never scanned — only
/// bodies (see the lint's stated limits).
pub(crate) fn conditional_regions(clean: &[u8]) -> Vec<(usize, usize)> {
    const KEYWORDS: [&[u8]; 5] = [b"if", b"match", b"for", b"while", b"loop"];
    let mut regions = Vec::new();
    let mut i = 0;
    while i < clean.len() {
        let mut keyword = false;
        let mut after = i;
        for kw in KEYWORDS {
            if is_word_at(clean, i, kw) {
                keyword = true;
                after = i + kw.len();
                break;
            }
        }
        if !keyword {
            i += 1;
            continue;
        }
        // Scan to the first `{` at paren-depth 0 (abort on `;`/`}` —
        // guards against non-block keyword lookalikes).
        let mut j = after;
        let mut depth = 0usize;
        let mut body_at = None;
        while j < clean.len() {
            match clean[j] {
                b'(' => depth += 1,
                b')' => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                b'{' if depth == 0 => {
                    body_at = Some(j);
                    break;
                }
                b';' | b'}' if depth == 0 => break,
                _ => {}
            }
            j += 1;
        }
        let mut at = match body_at {
            Some(at) => at,
            None => {
                i += 1;
                continue;
            }
        };
        // Match this block and any immediately following `{` blocks.
        loop {
            match match_brace(clean, at) {
                Some(end) => {
                    regions.push((at, end + 1));
                    let mut k = end + 1;
                    while k < clean.len()
                        && (clean[k] == b' '
                            || clean[k] == b'\t'
                            || clean[k] == b'\n'
                            || clean[k] == b'\r')
                    {
                        k += 1;
                    }
                    if k < clean.len() && clean[k] == b'{' {
                        at = k;
                        continue;
                    }
                    i = end + 1;
                    break;
                }
                None => {
                    i = at + 1;
                    break;
                }
            }
        }
    }
    regions
}

pub(crate) fn in_ranges(i: usize, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|(s, e)| i >= *s && i < *e)
}

/// Finds `ctx.signal(|memo(|binding(` (whitespace-tolerant); returns the
/// method name.
pub(crate) fn find_ctx_creation(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if is_word_at(b, i, b"ctx") {
            let mut j = i + 3;
            while j < b.len() && (b[j] == b' ' || b[j] == b'\t' || b[j] == b'\n' || b[j] == b'\r') {
                j += 1;
            }
            if j < b.len() && b[j] == b'.' {
                j += 1;
                while j < b.len()
                    && (b[j] == b' ' || b[j] == b'\t' || b[j] == b'\n' || b[j] == b'\r')
                {
                    j += 1;
                }
                for method in SITE_KEYED_METHODS {
                    if is_word_at(b, j, method.as_bytes()) {
                        let mut k = j + method.len();
                        while k < b.len()
                            && (b[k] == b' ' || b[k] == b'\t' || b[k] == b'\n' || b[k] == b'\r')
                        {
                            k += 1;
                        }
                        if k < b.len() && b[k] == b'(' {
                            return Some(method.to_string());
                        }
                    }
                }
            }
        }
        i += 1;
    }
    None
}
