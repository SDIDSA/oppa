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
    let lints = component_lint::check_component_body(&item_src);
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
    match props::parse_props_struct(&src) {
        Ok(expansion) => expansion.parse().unwrap(),
        Err(msg) => format!("compile_error!({msg:?});").parse().unwrap(),
    }
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
/// Monomorphized generics (Phase 37a, decision 360): name the concrete
/// args on both sides —
///
/// ```ignore
/// oppa_macros::component_manifest![List::<i32>(ListProps::<i32>)];
/// ```
///
/// — and the macro emits one entry per monomorphization (mangled
/// wrapper fns, canonical `Name<Args>` symbol). Bare type parameters
/// (`List<T>`, `List<T>(ListProps<T>)`, single-letter args) refuse
/// loudly at macro expansion: monomorphize with concrete args, never
/// a silent unbound expansion (rustc would fail pages later).
///
/// Each entry's `render` wrapper calls the same-named component function in
/// scope; `drain_props`/`adopt_props` move the concrete props value across
/// the unload boundary as a thin pointer with a type-name check (see
/// `oppa::reload`). Duplicate components are a compile error.
#[proc_macro]
pub fn component_manifest(input: TokenStream) -> TokenStream {
    let src = input.to_string();
    match manifest::parse_manifest(&src) {
        Ok((export, entries)) => {
            let out = manifest::expand_manifest(export, &entries);
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
    hot_crate::expand_hot_crate(&src).parse().unwrap()
}

mod component_lint;
mod hot_crate;
mod manifest;
mod props;
mod scan;

#[cfg(test)]
mod tests;
