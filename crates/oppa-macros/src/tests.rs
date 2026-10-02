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
    let (export, entries) = parse_manifest("Toggle(ToggleProps), ContactRow(RowProps)").unwrap();
    assert!(!export);
    assert_eq!(
        entries,
        vec![
            ManifestEntry {
                component: "Toggle".to_string(),
                props: "ToggleProps".to_string(),
                canonical: "Toggle".to_string(),
                tag: "Toggle".to_string(),
            },
            ManifestEntry {
                component: "ContactRow".to_string(),
                props: "RowProps".to_string(),
                canonical: "ContactRow".to_string(),
                tag: "ContactRow".to_string(),
            },
        ]
    );
}

#[test]
fn manifest_parses_monomorphized_generics() {
    let (export, entries) =
        parse_manifest("List::<i32>(ListProps::<i32>), Map::<String, Vec<u8>>(MapProps)").unwrap();
    assert!(!export);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].component, "List::<i32>");
    assert_eq!(entries[0].props, "ListProps::<i32>");
    assert_eq!(entries[0].canonical, "List<i32>");
    assert_eq!(entries[0].tag, "List__i32");
    assert_eq!(entries[1].tag, "Map__String__Vec_u8");
    let expanded = expand_manifest(export, &entries);
    assert!(expanded.contains("__oppa_render_List__i32"));
    assert!(expanded.contains("__oppa_drain_List__i32"));
    assert!(expanded.contains("__oppa_adopt_List__i32"));
    assert!(expanded.contains("SymbolHash::of(\"List<i32>\")"));
    assert!(expanded.contains("List::<i32>(ctx, props.get::<ListProps::<i32>>())"));
}

#[test]
fn manifest_rejects_bare_generics_loudly() {
    // Bare type parameters refuse at macro expansion (never a
    // silent unbound expansion failing pages later at rustc).
    assert!(parse_manifest("List<T>(ListProps<T>)").is_err());
    assert!(parse_manifest("List::<T>(ListProps::<T>)").is_err());
    // The shorthand without props names the explicit form.
    assert!(parse_manifest("List::<i32>").is_err());
    // Distinct monomorphizations coexist (no duplicate).
    assert!(parse_manifest("List::<i32>(ListProps::<i32>), List::<u8>(ListProps::<u8>)").is_ok());
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
