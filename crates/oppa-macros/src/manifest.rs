pub(crate) fn parse_manifest(src: &str) -> Result<(bool, Vec<ManifestEntry>), String> {
    // Split top-level commas (paren AND angle depth tracked — generic
    // args carry commas inside `<>`).
    let mut parts = Vec::new();
    let mut paren = 0usize;
    let mut angle = 0usize;
    let mut start = 0usize;
    let bytes = src.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => paren += 1,
            b')' => {
                if paren == 0 {
                    return Err("unbalanced `)` in component_manifest![...]".to_string());
                }
                paren -= 1;
            }
            b'<' => angle += 1,
            b'>' => {
                if angle == 0 {
                    return Err("unbalanced `>` in component_manifest![...]".to_string());
                }
                angle -= 1;
            }
            b',' if paren == 0 && angle == 0 => {
                parts.push(src[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if paren != 0 {
        return Err("unbalanced `(` in component_manifest![...]".to_string());
    }
    if angle != 0 {
        return Err("unbalanced `<` in component_manifest![...]".to_string());
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
        entries.push(parse_entry(part)?);
    }
    if entries.is_empty() {
        return Err("component_manifest![...] needs at least one `Name(Props)` entry".to_string());
    }
    // Duplicate components (same canonical symbol) are a compile error.
    let mut seen = std::collections::HashSet::new();
    for e in &entries {
        if !seen.insert(e.canonical.clone()) {
            return Err(format!(
                "duplicate component `{}` in component_manifest![...]",
                e.canonical
            ));
        }
    }
    Ok((export, entries))
}

/// One manifest entry: the component path (possibly turbofished), the
/// props path, the canonical `Name<Args>` symbol, and the mangled
/// wrapper-fn tag.
#[derive(Debug, PartialEq)]
pub(crate) struct ManifestEntry {
    pub(crate) component: String,
    pub(crate) props: String,
    pub(crate) canonical: String,
    pub(crate) tag: String,
}

/// Splits one entry into `Name(::<>Args)(Props)` — the trailing
/// top-level `(...)` group is the props; everything before it is the
/// component path (plain ident or `Ident::<Args>`).
pub(crate) fn parse_entry(part: &str) -> Result<ManifestEntry, String> {
    // Find the trailing top-level paren group (parens inside `<>`,
    // e.g. fn-pointer args, never open a top-level group).
    let bytes = part.as_bytes();
    if !part.trim_end().ends_with(')') {
        return Err(format!(
            "entry `{part}` is not `Name(Props)` — monomorphized generics spell `Name::<A>(P<A>)`"
        ));
    }
    let mut paren = 0usize;
    let mut angle = 0usize;
    let mut open_at: Option<usize> = None;
    for (i, b) in bytes.iter().enumerate() {
        match b {
            b'(' if angle == 0 => {
                if paren == 0 {
                    open_at = Some(i);
                }
                paren += 1;
            }
            b')' if angle == 0 => {
                paren = paren.saturating_sub(1);
            }
            b'<' => angle += 1,
            b'>' => angle = angle.saturating_sub(1),
            _ => {}
        }
    }
    let open = open_at.ok_or_else(|| format!("entry `{part}` is not `Name(Props)`"))?;
    let name = part[..open].trim().to_string();
    let props = part[open + 1..part.len() - 1].trim().to_string();
    if props.is_empty() {
        return Err(format!(
            "entry `{part}` names no props — spell `Name(Props)`"
        ));
    }
    let (base, args) = split_turbofish(&name).ok_or_else(|| {
        format!("component name `{name}` must be an Uppercase identifier or `Name::<Args>`")
    })?;
    if !is_component_ident(&base) {
        return Err(format!(
            "component name `{base}` must be an Uppercase identifier"
        ));
    }
    // Bare type parameters refuse loudly (Phase 37a): single-letter
    // Uppercase args are params by universal convention (`T`, `P`);
    // concrete single-idents (`String`) and compound args pass (a
    // truly-bare multi-char param still fails loudly at rustc).
    for arg in &args {
        let arg = arg.trim();
        if arg.len() == 1 && arg.chars().all(|c| c.is_uppercase()) {
            return Err(format!(
                "component `{name}` takes bare type parameter `{arg}` — monomorphize with concrete args (`Name::<i32>(P<i32>)`), never a silent unbound expansion"
            ));
        }
    }
    if !args.is_empty() && !is_type_path_generic(&props) {
        return Err(format!(
            "props `{props}` must be a type path (generic components spell `Name::<A>(P<A>)`)"
        ));
    }
    if args.is_empty() && !is_type_path(&props) {
        return Err(format!(
            "props `{props}` must be a plain type path (no generics in M2b)"
        ));
    }
    let canonical = if args.is_empty() {
        base.clone()
    } else {
        format!("{}<{}>", base, args.join(","))
    };
    let tag = if args.is_empty() {
        base.clone()
    } else {
        format!(
            "{}__{}",
            base,
            args.iter()
                .map(|a| mangle_arg(a))
                .collect::<Vec<_>>()
                .join("__")
        )
    };
    if !tag.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(format!(
            "component `{name}` mangles to non-identifier `{tag}` — simplify the turbofish args"
        ));
    }
    let component = if args.is_empty() {
        base.clone()
    } else {
        format!("{}::<{}>", base, args.join(","))
    };
    Ok(ManifestEntry {
        component,
        props,
        canonical,
        tag,
    })
}

/// Splits `Name` or `Name::<A, B>` into base + args (`None` when the
/// turbofish is malformed).
pub(crate) fn split_turbofish(name: &str) -> Option<(String, Vec<String>)> {
    let Some(turbo) = name.find("::") else {
        return Some((name.to_string(), Vec::new()));
    };
    let (base, rest) = name.split_at(turbo);
    let args = rest.strip_prefix("::")?.trim();
    let inner = args.strip_prefix('<')?.strip_suffix('>')?.trim();
    if inner.is_empty() {
        return None;
    }
    // Top-level comma split (nested turbos nest).
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(inner[start..].trim().to_string());
    if out.iter().any(|a| a.is_empty()) {
        return None;
    }
    Some((base.to_string(), out))
}

/// Mangles one turbofish arg into identifier characters
/// (`i32` → `i32`, `Vec<u8>` → `Vec_u8`, `&str` → `str`).
pub(crate) fn mangle_arg(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut prev_underscore = true;
    for c in arg.chars() {
        if c.is_alphanumeric() {
            out.push(c);
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    out.trim_matches('_').to_string()
}

pub(crate) fn is_component_ident(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

pub(crate) fn is_type_path(path: &str) -> bool {
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

/// Generic-aware type path (Phase 37a): plain segments plus balanced
/// `<>` groups (nested turbos nest); parens/brackets/braces/quotes
/// stay out everywhere.
pub(crate) fn is_type_path_generic(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    if path.contains(['(', ')', '[', ']', '{', '}', ';', '"']) {
        return false;
    }
    // Top-level `::` split (angle depth tracked — `A::<B>` keeps its
    // turbofish with the segment; the `::` introducing `<` never
    // splits).
    let mut segments = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let bytes = path.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => depth = depth.saturating_sub(1),
            b':' if depth == 0 && i + 1 < bytes.len() && bytes[i + 1] == b':' => {
                if i + 2 < bytes.len() && bytes[i + 2] == b'<' {
                    i += 2;
                } else {
                    segments.push(path[start..i].trim());
                    start = i + 2;
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    segments.push(path[start..].trim());
    if segments.iter().any(|s| s.is_empty()) {
        return false;
    }
    segments.iter().all(|seg| {
        // Plain ident, or `Ident[::<]args>` with balanced, non-empty,
        // recursively-valid args.
        if let Some(turbo) = seg.find('<') {
            if !seg.ends_with('>') {
                return false;
            }
            let base = seg[..turbo].trim().trim_end_matches(':');
            if base.is_empty() || !is_bare_ident(base) {
                return false;
            }
            let inner = seg[turbo + 1..seg.len() - 1].trim();
            if inner.is_empty() {
                return false;
            }
            split_top_commas(inner)
                .map(|args| !args.is_empty() && args.iter().all(|a| is_type_path_generic(a)))
                .unwrap_or(false)
        } else {
            is_bare_ident(seg)
        }
    })
}

/// One identifier segment (letters/`_` start, alphanumerics/`_` rest).
pub(crate) fn is_bare_ident(seg: &str) -> bool {
    let mut chars = seg.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Top-level comma split (`None` on unbalanced `<>`).
pub(crate) fn split_top_commas(inner: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            ',' if depth == 0 => {
                out.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(inner[start..].trim().to_string());
    Some(out)
}

pub(crate) fn expand_manifest(export: bool, entries: &[ManifestEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        let (component, props, tag) = (&entry.component, &entry.props, &entry.tag);
        out.push_str(&format!(
            r##"fn __oppa_render_{tag}(ctx: &::oppa::component::Ctx, props: &::oppa::component::OpaqueProps) -> ::oppa::vnode::VNode {{ {component}(ctx, props.get::<{props}>()) }}
unsafe extern "C" fn __oppa_drain_{tag}(props: *const ::oppa::component::OpaqueProps) -> ::oppa::reload::DrainedProps {{ let value: Option<{props}> = (*props).try_get::<{props}>().cloned(); match value {{ Some(value) => {{ let boxed: Box<{props}> = Box::new(value); ::oppa::reload::DrainedProps {{ ptr: Box::into_raw(boxed) as *mut ::std::ffi::c_void }} }}, None => ::oppa::reload::DrainedProps {{ ptr: ::std::ptr::null_mut() }} }} }}
unsafe extern "C" fn __oppa_adopt_{tag}(drained: ::oppa::reload::DrainedProps, into: ::oppa::worker::HotGeneration, expected: *const ::std::ffi::c_char) -> *mut ::oppa::component::OpaqueProps {{ let want = ::std::ffi::CStr::from_ptr(expected).to_string_lossy(); if want != ::std::any::type_name::<{props}>() {{ return ::std::ptr::null_mut(); }} let boxed: Box<{props}> = Box::from_raw(drained.ptr as *mut {props}); let owned: {props} = *boxed; let opaque = ::oppa::component::OpaqueProps::new(owned, into); Box::into_raw(Box::new(opaque)) }}
"##,
            tag = tag,
            component = component,
            props = props
        ));
    }
    // `type_name` is not const-stable, so the table builds once at first
    // use behind an `OnceLock` (never mutated after — the exported
    // pointer stays valid for the provider's lifetime).
    out.push_str("pub fn __oppa_manifest_descs() -> &'static [::oppa::reload::ComponentDesc] {\n    static CELL: ::std::sync::OnceLock<Vec<::oppa::reload::ComponentDesc>> = ::std::sync::OnceLock::new();\n    CELL.get_or_init(|| vec![\n");
    for entry in entries {
        out.push_str(&format!(
            "        ::oppa::reload::ComponentDesc {{ symbol: ::oppa::hash::SymbolHash::of(\"{}\"), props_type: ::std::any::type_name::<{}>(), render: __oppa_render_{}, drain_props: __oppa_drain_{}, adopt_props: __oppa_adopt_{} }},\n",
            entry.canonical, entry.props, entry.tag, entry.tag, entry.tag
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
