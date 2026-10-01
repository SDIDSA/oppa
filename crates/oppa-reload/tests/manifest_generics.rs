//! Phase 37a (decision 360): monomorphized generic manifests compile
//! and run end to end. One generic component over generic props,
//! two monomorphizations in one static manifest — distinct symbols,
//! distinct props types, distinct renders. This is the crate's
//! consolidated generics binary — the reload-gate files stay
//! untouched.

#![allow(non_snake_case)]

use oppa::{ComponentHost, Ctx, Div, Props, SharedString, VNode};
use oppa_macros::{component_manifest, Props as PropsDerive};
use oppa_reload::StaticSource;

#[derive(Clone, PropsDerive)]
pub struct ItemProps<T> {
    pub label: SharedString,
    pub value: T,
}

fn Item<T: Clone + 'static>(ctx: &Ctx, props: &ItemProps<T>) -> VNode {
    let _ = ctx.signal(0u32);
    // The monomorphized type name proves which instantiation ran.
    Div("item").child(VNode::from(oppa::Text::new(format!(
        "{}:{}",
        props.label,
        std::any::type_name::<T>()
    ))))
}

mod mono {
    use super::*;

    component_manifest![
        Item::<i32>(ItemProps::<i32>),
        Item::<String>(ItemProps::<String>)
    ];

    pub fn source() -> Box<StaticSource> {
        Box::new(StaticSource::new("mono", __oppa_manifest_descs()))
    }
}

#[test]
fn generic_manifest_monomorphizes_with_distinct_symbols() {
    let descs = mono::__oppa_manifest_descs();
    assert_eq!(descs.len(), 2, "one entry per monomorphization");
    assert_eq!(
        format!("{}", descs[0].symbol),
        format!("{}", oppa::hash::SymbolHash::of("Item<i32>"))
    );
    assert_eq!(
        format!("{}", descs[1].symbol),
        format!("{}", oppa::hash::SymbolHash::of("Item<String>"))
    );
    assert_eq!(descs[0].props_type, std::any::type_name::<ItemProps<i32>>());
    assert_eq!(
        descs[1].props_type,
        std::any::type_name::<ItemProps<String>>()
    );
    let _ = mono::source();
}

#[derive(Clone)]
struct RootProps;
impl Props for RootProps {}

fn root_scene(ctx: &Ctx, _: &RootProps) -> VNode {
    let _ = ctx.signal(0u32);
    Div("root").children([
        ctx.child_keyed(
            1,
            &ItemProps {
                label: SharedString::from("a"),
                value: 7i32,
            },
            Item::<i32>,
        ),
        ctx.child_keyed(
            2,
            &ItemProps {
                label: SharedString::from("b"),
                value: "seven".to_string(),
            },
            Item::<String>,
        ),
    ])
}

#[test]
fn generic_instantiations_render_through_keyed_children() {
    // Both monomorphizations mount side by side (keyed, no manual
    // strings) and render their own type names.
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.mount("Root", RootProps, root_scene);
    host.run_until_idle();
    let items = oppa::find_retained_by_debug(&host, "item");
    assert_eq!(items.len(), 2, "both instantiations mount");
}
