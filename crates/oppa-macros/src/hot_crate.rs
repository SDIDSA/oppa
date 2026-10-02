use super::scan::{find_word, match_brace};

/// String-level core of `#[hot_crate]` (unit-testable without the
/// proc-macro bridge).
pub(crate) fn expand_hot_crate(src: &str) -> String {
    match parse_mod_target(src) {
        Err(msg) => format!("{src}\ncompile_error!({msg:?});"),
        Ok(ModTarget::File(name)) => {
            let check = format!(
                "#[cfg(not(test))]\nconst _: () = assert!(::oppa::lint::check_no_ambient_state(::std::include_str!(::std::concat!(::std::env!(\"CARGO_MANIFEST_DIR\"), \"/src/{name}.rs\"))) == ::oppa::lint::CLEAN, \"hot crate module `{name}` holds ambient state (static / thread_local! / OnceCell / LazyLock) — surviving state must live core-side in reactive storage (lock #25)\");"
            );
            format!("{src}\n{check}")
        }
        Ok(ModTarget::Inline(body)) => {
            let lit = escape_into_string_literal(&body);
            let check = format!(
                "#[cfg(not(test))]\nconst _: () = assert!(::oppa::lint::check_no_ambient_state(\"{lit}\") == ::oppa::lint::CLEAN, \"hot crate inline module holds ambient state (static / thread_local! / OnceCell / LazyLock) — surviving state must live core-side in reactive storage (lock #25)\");"
            );
            format!("{src}\n{check}")
        }
    }
}

pub(crate) enum ModTarget {
    File(String),
    Inline(String),
}

pub(crate) fn parse_mod_target(src: &str) -> Result<ModTarget, String> {
    if src.contains("#[path") {
        return Err(
            "`#[hot_crate]` does not scan `#[path]`-remapped modules — use standard src/<name>.rs layout or an inline module".to_string(),
        );
    }
    let bytes = src.as_bytes();
    let at = find_word(bytes, b"mod").ok_or_else(|| {
        "`#[hot_crate]` attaches to `mod name;` or `mod name { ... }` items".to_string()
    })?;
    let mut i = at + 3;
    while i < bytes.len()
        && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n' || bytes[i] == b'\r')
    {
        i += 1;
    }
    let name_start = i;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    let name = src[name_start..i].to_string();
    if name.is_empty() {
        return Err("`#[hot_crate]` could not find the module name".to_string());
    }
    while i < bytes.len()
        && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n' || bytes[i] == b'\r')
    {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b';' {
        return Ok(ModTarget::File(name));
    }
    if i < bytes.len() && bytes[i] == b'{' {
        if let Some(end) = match_brace(bytes, i) {
            return Ok(ModTarget::Inline(src[i..=end].to_string()));
        }
        return Err(format!("`mod {name}` has unbalanced braces"));
    }
    Err(format!(
        "`#[hot_crate]` on `mod {name}`: expected `;` or `{{ ... }}`"
    ))
}

pub(crate) fn escape_into_string_literal(body: &str) -> String {
    let mut lit = String::with_capacity(body.len() + 2);
    for ch in body.chars() {
        match ch {
            '\\' => lit.push_str("\\\\"),
            '"' => lit.push_str("\\\""),
            '\n' => lit.push_str("\\n"),
            '\r' => lit.push_str("\\r"),
            '\t' => lit.push_str("\\t"),
            _ => lit.push(ch),
        }
    }
    lit
}
