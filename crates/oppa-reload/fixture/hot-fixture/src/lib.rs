//! Hot-swap fixture: one editable component across two builds.
//!
//! Component function names are PascalCase by design (the locked §4
//! authoring surface) — hence the file-level allow.
//!
//! - Default build (v1): `Counter` + `OldOnly`.
//! - `--features v2`: `Counter` (edited body — inserted signal) + `NewOnly`.
//!
//! `CounterProps` keeps its layout across both builds (same crate name +
//! module path ⇒ same `type_name`), so props adopt; bodies differ, so
//! site keys shift. `OldOnly` vanishes (eviction); `NewOnly` appears
//! (manifest-scan discovery). The generated dylib glue is framework code
//! and exempt from `#[hot_crate]` (which scans annotated user files).

#![allow(non_snake_case)]

use oppa::{Ctx, Div, SharedString, Signal, VNode};
use oppa_macros::{component, Props};

#[derive(Clone, Props)]
pub struct CounterProps {
    pub label: SharedString,
    pub initial: u32,
    pub counter: Signal<u32>,
    pub probe: Signal<u32>,
}

#[derive(Clone, Props)]
pub struct EmptyProps;

#[cfg(not(feature = "v2"))]
#[component]
pub fn Counter(ctx: &Ctx, props: &CounterProps) -> VNode {
    let local = ctx.signal(props.initial);
    props.probe.set(local.get() + props.counter.get());
    let _ = props.label.clone();
    Div("counter").build()
}

#[cfg(feature = "v2")]
#[component]
pub fn Counter(ctx: &Ctx, props: &CounterProps) -> VNode {
    // Edited body (v2): inserted line shifts `local`'s site → re-seed.
    let _epoch = ctx.signal(0u32);
    let local = ctx.signal(props.initial);
    props.probe.set(local.get() + props.counter.get());
    let _ = props.label.clone();
    Div("counter").build()
}

#[cfg(not(feature = "v2"))]
#[component]
pub fn OldOnly(ctx: &Ctx, _props: &EmptyProps) -> VNode {
    let _epoch = ctx.signal(0u32);
    Div("old").build()
}

#[cfg(feature = "v2")]
#[component]
pub fn NewOnly(ctx: &Ctx, _props: &EmptyProps) -> VNode {
    let _epoch = ctx.signal(2u32);
    Div("new").build()
}

#[cfg(not(feature = "v2"))]
oppa_macros::component_manifest![export, Counter(CounterProps), OldOnly(EmptyProps)];

#[cfg(feature = "v2")]
oppa_macros::component_manifest![export, Counter(CounterProps), NewOnly(EmptyProps)];
