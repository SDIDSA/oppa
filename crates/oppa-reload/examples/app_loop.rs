//! App-shaped hot-reload loop (G14 — the example `oppa-reload`
//! previously lacked): boot a `ComponentHost`, install the v1
//! manifest, drive frames, swap to v2, and watch author-owned state
//! survive while the report narrates the swap.
//!
//! Run: `cargo run -p oppa-reload --example app_loop`.
//!
//! This example swaps `StaticSource` manifests (no dylib build —
//! the loop is identical; `DylibSource` only changes where the
//! manifest bytes come from, proven by the `real_dylib` test).
//! Android stays restart-only (locked #16) — this loop is the
//! desktop shape.

// Component functions are CamelCase by framework convention (the
// `#[component] fn Name` surface — same allow as the test suite).
#![allow(non_snake_case)]

use oppa::{ComponentHost, Ctx, SharedString, Signal, VNode};
use oppa_macros::{component, component_manifest, Props};
use oppa_reload::{HotRegistry, StaticSource};

#[derive(Clone, Props)]
pub struct CounterProps {
    pub label: SharedString,
    pub counter: Signal<u32>,
}

mod v1 {
    use super::*;

    #[component]
    pub fn Counter(ctx: &Ctx, props: &CounterProps) -> VNode {
        let local = ctx.signal(0u32);
        let _ = local.get();
        oppa::Div("counter").child(VNode::from(oppa::Text {
            text: SharedString::from(props.label.as_ref()),
            style: oppa::Text::body_secondary,
        }))
    }

    component_manifest![Counter(CounterProps)];
}

mod v2 {
    use super::*;

    #[component]
    pub fn Counter(ctx: &Ctx, props: &CounterProps) -> VNode {
        // One body edit above the session site (the re-seed rule,
        // section 5.1 — later sites re-seed, never shuffle).
        let banner = ctx.signal(true);
        let _ = banner.get();
        let local = ctx.signal(0u32);
        let _ = local.get();
        let _ = props.counter.get();
        oppa::Div("counter").child(VNode::from(oppa::Text {
            text: SharedString::from(props.label.as_ref()),
            style: oppa::Text::body_secondary,
        }))
    }

    component_manifest![Counter(CounterProps)];
}

fn main() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let counter = host.runtime().signal(41u32);
    let props = CounterProps {
        label: SharedString::from("counter"),
        counter: counter.clone(),
    };

    // 1. Boot on v1 code (typed mount — what an app does first).
    let _handle = host.mount("Counter", props, v1::Counter);
    host.run_until_idle();
    println!("boot: counter = {}", counter.get());
    println!("boot: retained nodes = {}", host.retained_count());

    // 2. Install the v1 manifest (harness learns the symbols).
    let mut reg = HotRegistry::new(host.clone());
    reg.install(Box::new(StaticSource::new(
        "v1",
        v1::__oppa_manifest_descs(),
    )));
    println!("install: symbols = {:?}", reg.current_symbols().len());

    // 3. Author edits code; the watcher would rescan here. Swap to v2.
    counter.set(42);
    let report = reg.reload_to(Box::new(StaticSource::new(
        "v2",
        v2::__oppa_manifest_descs(),
    )));
    host.run_until_idle();
    println!("reload ok: {}", report.ok());
    println!("reload evicted: {}", report.evicted.len());
    println!("after reload: counter = {}", counter.get());
    println!("after reload: retained nodes = {}", host.retained_count());

    assert!(report.ok(), "swap must succeed");
    assert_eq!(counter.get(), 42, "author-owned state survives");
    println!("app_loop: PASS — state survived the swap, nothing evicted");
}
