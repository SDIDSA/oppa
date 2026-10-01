//! Authoring macros for the oppa component model (M2) + hot-reload
//! authoring surface (M2b).
//!
//! - `#[component]` marks `fn Name(ctx: &Ctx, props: Props) -> VNode` as a
//!   component. Pass-through (no rewrite), so call-site/source-hash keying
//!   keeps working on plain stable Rust — plus the M2b call-site lint: a
//!   `ctx.signal/memo/binding` creation call inside a nested `fn` item or
//!   a conditional/loop body is a `compile_error` (run-varying execution
//!   count would silently turn re-seed into shuffle, §5.1). Loop-
//!   *combinator* bodies (`.map(|slot| ...)` — the locked §4.2 pattern)
//!   are spared by design.
//! - `#[derive(Props)]` implements the marker trait
//!   `oppa::component::Props` (`Clone + 'static`).
//! - `component_manifest![Name(Props), ...]` generates the stable
//!   `oppa_component_manifest()` export + per-component render/drain/adopt
//!   glue for the M2b dylib swap (`oppa::reload` ABI).
//! - `#[hot_crate]` on `mod` items enforces the §9.6 state lint at compile
//!   time (ambient `static`/`thread_local!`/`OnceCell` state fails the
//!   build via `oppa::lint::check_no_ambient_state`).
//!
//! Zero dependencies by design (the `oppa` core crate is std-only).

use proc_macro::TokenStream;

/// Marks a component function. Pass-through (no rewrite), so the examples
/// in DESIGN §4 carry the attribute literally and the `#[track_caller]`-
/// based call-site keying in `Ctx` keeps working — plus the M2b call-site
/// lint (§8.1): state-creation calls in nested `fn` items or
/// conditional/loop bodies are a compile error, not a silent shuffle.
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_src = item.to_string();
    if !attr.is_empty() {
        let err = "compile_error!(\"#[component] takes no arguments\");";
        return format!("{item_src}\n{err}").parse().unwrap();
    }
    let lints = check_component_body(&item_src);
    if lints.is_empty() {
        return item;
    }
    let mut out = item_src;
    for lint in lints {
        out.push_str(&format!("\ncompile_error!({lint:?});"));
    }
    out.parse().unwrap()
}

/// Derives the `Props` marker trait (`Clone + 'static` bound, enforced at
/// the impl site so a non-clone props struct fails loudly here, not deep
/// inside the reconciler).
///
/// Plain structs emit the direct impl; generic structs emit the bounded
/// impl (Round 14.1, decision 311) — each type param gains
/// `Clone + 'static`, each lifetime gains `'static`, the original
/// `where` clause is preserved verbatim:
///
/// ```ignore
/// #[derive(Props)]
/// struct ListProps<T> { items: Vec<T> }
/// // impl<T> ::oppa::component::Props for ListProps<T> where T: Clone + 'static {}
/// ```
#[proc_macro_derive(Props)]
pub fn derive_props(input: TokenStream) -> TokenStream {
    let src = input.to_string();
    match parse_props_struct(&src) {
        Ok(expansion) => expansion.parse().unwrap(),
        Err(msg) => format!("compile_error!({msg:?});").parse().unwrap(),
    }
}

/// Parses `pub struct Name<...>(...) where ... { ... }` headers at
/// string level (the crate's zero-dependency rule — same scanner
/// class as the lint helpers below). Rejects enums/unions and
/// lowercase names loudly, like before.
fn parse_props_struct(src: &str) -> Result<String, String> {
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
fn blank_attributes(src: &str) -> String {
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

fn is_space_byte(b: u8) -> bool {
    b == b' ' || b == b'\t' || b == b'\n' || b == b'\r'
}

/// Strips a leading keyword (`const`) when it stands as a word.
fn strip_word<'a>(s: &'a str, word: &str) -> Option<&'a str> {
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
fn match_angle(bytes: &[u8], open: usize) -> Option<usize> {
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
fn match_group(bytes: &[u8], open: usize) -> Option<usize> {
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

fn skip_string(bytes: &[u8], open: usize) -> usize {
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
fn split_top_level(s: &str) -> Vec<String> {
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

fn is_word_at_bytes(haystack: &[u8], at: usize, word: &[u8]) -> bool {
    if at + word.len() > haystack.len() || &haystack[at..at + word.len()] != word {
        return false;
    }
    let before_ok = at == 0 || !is_ident_byte(haystack[at - 1]);
    let after_ok = at + word.len() == haystack.len() || !is_ident_byte(haystack[at + word.len()]);
    before_ok && after_ok
}

// ---------------------------------------------------------------------------
// component_manifest! — the stable dylib export (M2b §5.1, lock #14)
// ---------------------------------------------------------------------------

/// Generates the stable `oppa_component_manifest()` export plus
/// per-component render/drain/adopt glue (`oppa::reload` ABI).
///
/// ```ignore
/// oppa_macros::component_manifest![Toggle(ToggleProps), ContactRow(RowProps)];
/// ```
///
/// Prefix the list with `export,` for the hot-crate dylib form, which
/// additionally emits the stable `#[no_mangle] oppa_component_manifest`
/// symbol the harness looks up:
///
/// ```ignore
/// oppa_macros::component_manifest![export, Toggle(ToggleProps)];
/// ```
///
/// Each entry's `render` wrapper calls the same-named component function in
/// scope; `drain_props`/`adopt_props` move the concrete props value across
/// the unload boundary as a thin pointer with a type-name check (see
/// `oppa::reload`). Duplicate component names are a compile error.
#[proc_macro]
pub fn component_manifest(input: TokenStream) -> TokenStream {
    let src = input.to_string();
    match parse_manifest(&src) {
        Ok((export, entries)) => {
            let out = expand_manifest(export, &entries);
            match out.parse() {
                Ok(tokens) => tokens,
                Err(e) => format!("compile_error!(\"manifest expansion failed to parse: {e}\");")
                    .parse()
                    .expect("compile_error! parses"),
            }
        }
        Err(msg) => format!("compile_error!({msg:?});").parse().unwrap(),
    }
}

fn parse_manifest(src: &str) -> Result<(bool, Vec<(String, String)>), String> {
    // Split top-level commas (props paths never nest in M2b scope).
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let bytes = src.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                if depth == 0 {
                    return Err("unbalanced `)` in component_manifest![...]".to_string());
                }
                depth -= 1;
            }
            b',' if depth == 0 => {
                parts.push(src[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if depth != 0 {
        return Err("unbalanced `(` in component_manifest![...]".to_string());
    }
    let tail = src[start..].trim().to_string();
    if !tail.is_empty() {
        parts.push(tail);
    }
    if parts.is_empty() {
        return Err("component_manifest![...] needs at least one `Name(Props)` entry".to_string());
    }
    let mut entries = Vec::new();
    let mut export = false;
    for (index, part) in parts.iter().enumerate() {
        // Optional leading `export,` marker: emit the stable `#[no_mangle]`
        // dylib symbol. Static (in-process) manifests omit it — two
        // `#[no_mangle] oppa_component_manifest` symbols would collide at
        // link time (e.g. v1 + v2 manifests in one test binary).
        if index == 0 && part == "export" {
            export = true;
            continue;
        }
        let open = part
            .find('(')
            .ok_or_else(|| format!("entry `{part}` is not `Name(Props)`"))?;
        let name = part[..open].trim().to_string();
        if !is_component_ident(&name) {
            return Err(format!(
                "component name `{name}` must be an Uppercase identifier"
            ));
        }
        let rest = part[open + 1..].trim();
        let props = rest
            .strip_suffix(')')
            .ok_or_else(|| format!("entry `{part}` is missing its closing `)`"))?
            .trim()
            .to_string();
        if !is_type_path(&props) {
            return Err(format!(
                "props `{props}` must be a plain type path (no generics in M2b)"
            ));
        }
        if entries.iter().any(|(n, _): &(String, String)| n == &name) {
            return Err(format!(
                "duplicate component `{name}` in component_manifest![...]"
            ));
        }
        entries.push((name, props));
    }
    if entries.is_empty() {
        return Err("component_manifest![...] needs at least one `Name(Props)` entry".to_string());
    }
    Ok((export, entries))
}

fn is_component_ident(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn is_type_path(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    // Plain paths only: idents separated by `::` (generics arrive later).
    if path.contains(['<', '>', '(', ')', '[', ']', '{', '}', ',', ';', '"']) {
        return false;
    }
    path.split("::").all(|seg| {
        let mut chars = seg.chars();
        match chars.next() {
            Some(c) if c.is_alphabetic() || c == '_' => {}
            _ => return false,
        }
        chars.all(|c| c.is_alphanumeric() || c == '_')
    })
}

fn expand_manifest(export: bool, entries: &[(String, String)]) -> String {
    let mut out = String::new();
    for (name, props) in entries {
        out.push_str(&format!(
            r##"fn __oppa_render_{name}(ctx: &::oppa::component::Ctx, props: &::oppa::component::OpaqueProps) -> ::oppa::vnode::VNode {{ {name}(ctx, props.get::<{props}>()) }}
unsafe extern "C" fn __oppa_drain_{name}(props: *const ::oppa::component::OpaqueProps) -> ::oppa::reload::DrainedProps {{ let value: Option<{props}> = (*props).try_get::<{props}>().cloned(); match value {{ Some(value) => {{ let boxed: Box<{props}> = Box::new(value); ::oppa::reload::DrainedProps {{ ptr: Box::into_raw(boxed) as *mut ::std::ffi::c_void }} }}, None => ::oppa::reload::DrainedProps {{ ptr: ::std::ptr::null_mut() }} }} }}
unsafe extern "C" fn __oppa_adopt_{name}(drained: ::oppa::reload::DrainedProps, into: ::oppa::worker::HotGeneration, expected: *const ::std::ffi::c_char) -> *mut ::oppa::component::OpaqueProps {{ let want = ::std::ffi::CStr::from_ptr(expected).to_string_lossy(); if want != ::std::any::type_name::<{props}>() {{ return ::std::ptr::null_mut(); }} let boxed: Box<{props}> = Box::from_raw(drained.ptr as *mut {props}); let owned: {props} = *boxed; let opaque = ::oppa::component::OpaqueProps::new(owned, into); Box::into_raw(Box::new(opaque)) }}
"##,
            name = name,
            props = props
        ));
    }
    // `type_name` is not const-stable, so the table builds once at first
    // use behind an `OnceLock` (never mutated after — the exported
    // pointer stays valid for the provider's lifetime).
    out.push_str("pub fn __oppa_manifest_descs() -> &'static [::oppa::reload::ComponentDesc] {\n    static CELL: ::std::sync::OnceLock<Vec<::oppa::reload::ComponentDesc>> = ::std::sync::OnceLock::new();\n    CELL.get_or_init(|| vec![\n");
    for (name, props) in entries {
        out.push_str(&format!(
            "        ::oppa::reload::ComponentDesc {{ symbol: ::oppa::hash::SymbolHash::of(\"{name}\"), props_type: ::std::any::type_name::<{props}>(), render: __oppa_render_{name}, drain_props: __oppa_drain_{name}, adopt_props: __oppa_adopt_{name} }},\n",
            name = name,
            props = props
        ));
    }
    out.push_str("    ])\n}\n");
    if export {
        out.push_str(
            "#[no_mangle]\npub extern \"C\" fn oppa_component_manifest() -> ::oppa::reload::ManifestView {\n    let descs = __oppa_manifest_descs();\n    ::oppa::reload::ManifestView { entries: descs.as_ptr(), len: descs.len() }\n}\n",
        );
    }
    out
}

// ---------------------------------------------------------------------------
// #[hot_crate] — the §9.6 state lint at compile time
// ---------------------------------------------------------------------------

/// Enforces the state-residence rule (§9.6, lock #25) at compile time:
/// attach to each `mod` item of a hot crate; ambient state
/// (`static` / `thread_local!` / `OnceCell` / `LazyLock`) fails the build
/// through `oppa::lint::check_no_ambient_state`.
///
/// - `#[hot_crate] mod foo;` checks `src/foo.rs` via `include_str!`
///   (standard layout; `#[path]`-remapped modules are rejected loudly).
/// - `#[hot_crate] mod foo { ... }` checks the inline body.
/// - The check is `#[cfg(not(test))]` — `#[cfg(test)]` code never ships
///   in the dylib, so it is exempt.
#[proc_macro_attribute]
pub fn hot_crate(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let src = item.to_string();
    expand_hot_crate(&src).parse().unwrap()
}

/// String-level core of `#[hot_crate]` (unit-testable without the
/// proc-macro bridge).
fn expand_hot_crate(src: &str) -> String {
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

enum ModTarget {
    File(String),
    Inline(String),
}

fn parse_mod_target(src: &str) -> Result<ModTarget, String> {
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

fn escape_into_string_literal(body: &str) -> String {
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

fn find_word(haystack: &[u8], word: &[u8]) -> Option<usize> {
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

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Matches the brace opened at `open` (which must be `{`); returns the
/// index of its closer. Operates on noise-stripped text only.
fn match_brace(bytes: &[u8], open: usize) -> Option<usize> {
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
fn check_component_body(src: &str) -> Vec<String> {
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
fn strip_noise(src: &str) -> String {
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
fn raw_open(b: &[u8], i: usize) -> Option<usize> {
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

fn skip_raw(b: &[u8], mut j: usize) -> usize {
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

fn skip_char_lifetime(b: &[u8], q: usize) -> usize {
    if q + 2 < b.len() && b[q + 1] != b'\\' && b[q + 2] == b'\'' {
        return q + 3;
    }
    if q + 3 < b.len() && b[q + 1] == b'\\' && b[q + 3] == b'\'' {
        return q + 4;
    }
    q + 1
}

/// Byte ranges of nested `fn` item bodies (operates on noise-stripped
/// text; `fn`-pointer *types* like `fn()` are skipped, not flagged).
fn nested_fn_regions(clean: &[u8]) -> Vec<(usize, usize)> {
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

fn is_word_at(haystack: &[u8], at: usize, word: &[u8]) -> bool {
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
fn conditional_regions(clean: &[u8]) -> Vec<(usize, usize)> {
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

fn in_ranges(i: usize, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|(s, e)| i >= *s && i < *e)
}

/// Finds `ctx.signal(|memo(|binding(` (whitespace-tolerant); returns the
/// method name.
fn find_ctx_creation(s: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn props_plain_struct_expansion_unchanged() {
        assert_eq!(
            parse_props_struct("pub struct ToggleProps { initial : bool }").unwrap(),
            "impl ::oppa::component::Props for ToggleProps {}"
        );
    }

    #[test]
    fn props_single_generic_gains_clone_static() {
        // Stringified derive input spaces every punct (TokenStream
        // Display) — the expansion normalizes through its own format.
        assert_eq!(
            parse_props_struct("pub struct ListProps < T > { items : Vec < T > }").unwrap(),
            "impl<T> ::oppa::component::Props for ListProps<T> where T: Clone + 'static {}"
        );
    }

    #[test]
    fn props_multi_lifetime_const_where_preserved() {
        assert_eq!(
            parse_props_struct(
                "pub struct TableProps < 'a , T : Display , const N : usize > where T : PartialEq { rows : & 'a [T] }"
            )
            .unwrap(),
            "impl<'a , T : Display , const N : usize> ::oppa::component::Props for TableProps<'a, T, N> where T : PartialEq, 'a: 'static, T: Clone + 'static {}"
        );
    }

    #[test]
    fn props_rejects_non_structs_loudly() {
        assert!(parse_props_struct("enum ToggleProps { On, Off }").is_err());
        assert!(parse_props_struct("pub struct toggleProps { x : u8 }").is_err());
        assert!(parse_props_struct("pub struct Broken < T { x : T }").is_err());
        assert!(parse_props_struct("pub struct Empty <> { x : u8 }").is_err());
    }

    #[test]
    fn props_doc_attributes_never_poison_the_scan() {
        // `#[doc]` text rides the derive input — a "struct" in
        // disguise there must not shadow the real keyword.
        assert_eq!(
            parse_props_struct(
                "# [doc = \"a generic props struct deriving the marker\"] struct ListProps < T > { items : Vec < T > }"
            )
            .unwrap(),
            "impl<T> ::oppa::component::Props for ListProps<T> where T: Clone + 'static {}"
        );
    }

    #[test]
    fn props_full_derive_attribute_form_parses() {
        // The real shape: sibling derive + stacked docs + trailing comma.
        let src = "# [derive (Clone , Props)] # [doc = \" The brief's shape, verbatim: a generic props struct deriving\"] # [doc = \" the marker.\"] struct ListProps < T > { items : Vec < T > , }";
        assert_eq!(
            parse_props_struct(src).unwrap(),
            "impl<T> ::oppa::component::Props for ListProps<T> where T: Clone + 'static {}"
        );
    }

    #[test]
    fn props_verbatim_round_docs_parse() {
        // Byte-faithful doc text from the failing call site
        // (em-dash, backticks, apostrophe included).
        let src = "# [derive (Clone , Props)] # [doc = \" The brief's shape, verbatim: a generic props struct deriving\"] # [doc = \" the marker (no manual impl \u{2014} the derive emits\"] # [doc = \" `impl<T: Clone + 'static> Props`).\"] struct ListProps < T > { items : Vec < T > , }";
        assert_eq!(
            parse_props_struct(src).unwrap(),
            "impl<T> ::oppa::component::Props for ListProps<T> where T: Clone + 'static {}"
        );
    }

    #[test]
    fn props_raw_doc_lines_never_poison_the_scan() {
        // Stringified inputs keep raw `///` lines (not just `#[doc]`)
        // — the exact failure that motivated blanking (decision 311:
        // program text in disguise never shadows the keyword).
        let src = "/// The brief's shape, verbatim: a generic props struct deriving\n/// the marker.\nstruct ListProps < T > { items : Vec < T > , }";
        assert_eq!(
            parse_props_struct(src).unwrap(),
            "impl<T> ::oppa::component::Props for ListProps<T> where T: Clone + 'static {}"
        );
    }

    #[test]
    fn manifest_parses_pair() {
        let (export, entries) =
            parse_manifest("Toggle(ToggleProps), ContactRow(RowProps)").unwrap();
        assert!(!export);
        assert_eq!(
            entries,
            vec![
                ("Toggle".to_string(), "ToggleProps".to_string()),
                ("ContactRow".to_string(), "RowProps".to_string()),
            ]
        );
    }

    #[test]
    fn manifest_rejects_bad_shapes() {
        assert!(parse_manifest("").is_err());
        assert!(parse_manifest("Toggle").is_err());
        assert!(parse_manifest("toggle(ToggleProps)").is_err());
        assert!(parse_manifest("Toggle(ToggleProps<T>)").is_err());
        assert!(parse_manifest("Toggle(ToggleProps), Toggle(Other)").is_err());
        assert!(parse_manifest("Toggle(ToggleProps").is_err());
    }

    #[test]
    fn manifest_expands_stable_export() {
        let (export, entries) = parse_manifest("export, Toggle(ToggleProps)").unwrap();
        assert!(export);
        let expanded = expand_manifest(export, &entries);
        assert!(expanded.contains("oppa_component_manifest"));
        assert!(expanded.contains("__oppa_render_Toggle"));
        assert!(expanded.contains("__oppa_drain_Toggle"));
        assert!(expanded.contains("__oppa_adopt_Toggle"));
        assert!(expanded.contains("SymbolHash::of(\"Toggle\")"));
    }

    #[test]
    fn manifest_static_form_omits_export_symbol() {
        let (export, entries) = parse_manifest("Toggle(ToggleProps)").unwrap();
        assert!(!export);
        let expanded = expand_manifest(export, &entries);
        assert!(!expanded.contains("no_mangle"));
        assert!(!expanded.contains("oppa_component_manifest()"));
        assert!(expanded.contains("__oppa_render_Toggle"));
    }

    #[test]
    fn body_toggle_shape_is_clean() {
        let src = r#"fn Toggle(ctx: &Ctx, props: &ToggleProps) -> VNode {
    let is_on = ctx.signal(props.initial);
    let m = ctx.memo(|| is_on.get());
    Div("track").on_press(move || is_on.set(!is_on.get())).child(Div("k").build())
}"#;
        assert!(check_component_body(src).is_empty());
    }

    #[test]
    fn body_nested_fn_is_flagged() {
        let src = r#"fn Bad(ctx: &Ctx, props: &P) -> VNode {
    fn helper(ctx: &Ctx) -> Signal<u32> { ctx.signal(0) }
    let s = helper(ctx);
    Div("x").build()
}"#;
        let lints = check_component_body(src);
        assert_eq!(lints.len(), 1);
        assert!(lints[0].contains("nested `fn`"));
    }

    #[test]
    fn body_map_closure_is_clean_by_design() {
        // The locked §4.2 virtualization pattern: one base key per closure
        // line, ordinals disambiguate prefix-stable iterations.
        let src = r#"fn Fine(ctx: &Ctx, props: &P) -> VNode {
    let rows: Vec<_> = (0..3).map(|slot| { let s = ctx.signal(slot); s.get() }).collect();
    Div("x").build()
}"#;
        assert!(check_component_body(src).is_empty());
    }

    #[test]
    fn body_conditional_creation_is_flagged() {
        let src = r#"fn Bad(ctx: &Ctx, props: &P) -> VNode {
    let s = if props.enabled { ctx.signal(1u32) } else { ctx.signal(0u32) };
    Div("x").build()
}"#;
        let lints = check_component_body(src);
        assert_eq!(lints.len(), 1);
        assert!(lints[0].contains("conditional/loop"));
    }

    #[test]
    fn body_loop_creation_is_flagged() {
        let src = r#"fn Bad(ctx: &Ctx, props: &P) -> VNode {
    for i in 0..props.n { let _ = ctx.signal(i); }
    Div("x").build()
}"#;
        let lints = check_component_body(src);
        assert_eq!(lints.len(), 1);
        assert!(lints[0].contains("conditional/loop"));
    }

    #[test]
    fn body_operators_and_docs_are_quiet() {
        let src = r#"fn Fine(ctx: &Ctx, props: &P) -> VNode {
    // ctx.signal(0) would be a creation call, but not here.
    /// docs mention ctx.memo() freely.
    let x = 0b1010 | 0b0101;
    let y = x == 1 || x == 2;
    let s = "ctx.signal(0)";
    Div("x").build()
}"#;
        assert!(check_component_body(src).is_empty());
    }

    #[test]
    fn hot_crate_file_module_emits_include_check() {
        let out = expand_hot_crate("mod components;");
        assert!(out.contains("include_str"));
        assert!(out.contains("/src/components.rs"));
    }

    #[test]
    fn hot_crate_rejects_path_modules() {
        let out = expand_hot_crate("#[path = \"x.rs\"] mod components;");
        assert!(out.contains("compile_error"));
    }

    #[test]
    fn hot_crate_inline_body_is_checked() {
        let out = expand_hot_crate("mod inline_c { static X: u8 = 0; }");
        assert!(out.contains("check_no_ambient_state"));
    }
}
