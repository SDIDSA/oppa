//! M2b acceptance: the swap protocol over static (in-process) manifests.
//!
//! One mounted root per host (M2 reconciler is single-root). Components
//! write internal state into a props-carried probe signal each run — the
//! headless observability seam for keep-vs-reseed assertions.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use oppa::{ComponentHost, Ctx, Div, Event, EventKind, HandlerId, SharedString, Signal, VNode};
use oppa_macros::{component, component_manifest, Props};
use oppa_reload::{EvictReason, HotRegistry, StaticSource};

#[derive(Clone, Props)]
pub struct CounterProps {
    pub label: SharedString,
    pub initial: u32,
    /// Author-owned signal (controlled pattern): must survive swaps.
    pub counter: Signal<u32>,
    /// Body copies internal state here every run (observability).
    pub probe: Signal<u32>,
}

#[derive(Clone, Props)]
pub struct EmptyProps;

fn counter_props(host: &ComponentHost, initial: u32) -> (Signal<u32>, Signal<u32>, CounterProps) {
    let rt = host.runtime();
    let counter = rt.signal(10u32);
    let probe = rt.signal(0u32);
    let props = CounterProps {
        label: Arc::from("counter"),
        initial,
        counter: counter.clone(),
        probe: probe.clone(),
    };
    (counter, probe, props)
}

mod v1 {
    use super::*;

    #[component]
    pub fn Counter(ctx: &Ctx, props: &CounterProps) -> VNode {
        let local = ctx.signal(props.initial);
        props.probe.set(local.get() + props.counter.get());
        let _ = props.label.clone();
        Div("counter").build()
    }

    #[component]
    pub fn OldOnly(ctx: &Ctx, _props: &EmptyProps) -> VNode {
        let _epoch = ctx.signal(0u32);
        Div("old").build()
    }

    component_manifest![Counter(CounterProps), OldOnly(EmptyProps)];
}

mod v2 {
    use super::*;

    #[component]
    pub fn Counter(ctx: &Ctx, props: &CounterProps) -> VNode {
        // Inserted above `local`: every later call-site hash shifts, so
        // `local` re-seeds instead of shuffling (§5.1).
        let _epoch = ctx.signal(0u32);
        let local = ctx.signal(props.initial);
        props.probe.set(local.get() + props.counter.get());
        let _ = props.label.clone();
        Div("counter").build()
    }

    #[component]
    pub fn NewOnly(ctx: &Ctx, _props: &EmptyProps) -> VNode {
        let _epoch = ctx.signal(1u32);
        Div("new").build()
    }

    component_manifest![Counter(CounterProps), NewOnly(EmptyProps)];
}

mod v2b {
    use super::*;

    #[derive(Clone, Props)]
    pub struct CounterPropsV2 {
        pub label: SharedString,
    }

    // Same symbol ("Counter"), changed props layout → TypeMismatch.
    #[component]
    pub fn Counter(ctx: &Ctx, props: &CounterPropsV2) -> VNode {
        let _epoch = ctx.signal(0u32);
        let _ = props.label.clone();
        Div("counter").build()
    }

    component_manifest![Counter(CounterPropsV2)];
}

fn v1_source() -> Box<StaticSource> {
    Box::new(StaticSource::new("v1", v1::__oppa_manifest_descs()))
}

fn v2_source() -> Box<StaticSource> {
    Box::new(StaticSource::new("v2", v2::__oppa_manifest_descs()))
}

#[test]
fn identical_rescan_keeps_state() {
    let host = ComponentHost::new();
    let (counter, probe, props) = counter_props(&host, 7);
    let handle = host.mount("Counter", props, v1::Counter);
    host.run_until_idle();
    assert_eq!(probe.get(), 17); // local 7 + counter 10

    // Props update without reload: `local` keeps 7 (React semantics).
    handle.set_props(CounterProps {
        label: Arc::from("counter"),
        initial: 100,
        counter: counter.clone(),
        probe: probe.clone(),
    });
    host.run_until_idle();
    assert_eq!(probe.get(), 17);

    // Swap v1 → v1 (same code, fresh manifest object): nothing re-seeds.
    let mut reg = HotRegistry::new(host.clone());
    reg.install(v1_source());
    let report = reg.reload_to(v1_source());
    assert!(report.ok(), "unexpected evictions: {:?}", report.evicted);
    assert_eq!(report.drained, 1);
    assert_eq!(report.adopted, 1);
    assert_eq!(report.effects_rerun, 1);
    assert_eq!(probe.get(), 17); // local kept 7, not re-seeded to 100
    assert_eq!(report.incoming.bits(), report.outgoing.bits() + 1);
}

#[test]
fn body_edit_reseeds_shifted_sites_but_signals_survive() {
    let host = ComponentHost::new();
    let (counter, probe, props) = counter_props(&host, 7);
    let handle = host.mount("Counter", props, v1::Counter);
    host.run_until_idle();
    assert_eq!(probe.get(), 17);

    counter.set(41);
    host.run_until_idle();
    assert_eq!(probe.get(), 48); // local 7 + counter 41

    // Props update pre-swap: local keeps 7 → probe stays 48.
    handle.set_props(CounterProps {
        label: Arc::from("counter"),
        initial: 100,
        counter: counter.clone(),
        probe: probe.clone(),
    });
    host.run_until_idle();
    assert_eq!(probe.get(), 48);

    // Swap to the edited body: `local`'s site hash shifted → re-seeds to
    // initial (100). The author-owned `counter` signal survives untouched.
    let mut reg = HotRegistry::new(host.clone());
    reg.install(v1_source());
    let report = reg.reload_to(v2_source());
    assert!(report.ok(), "unexpected evictions: {:?}", report.evicted);
    assert_eq!(probe.get(), 141); // re-seeded 100 + surviving 41
    assert_eq!(counter.get(), 41);
}

#[test]
fn removed_symbol_evicts_loudly_without_panicking() {
    let host = ComponentHost::new();
    host.mount("OldOnly", EmptyProps, v1::OldOnly);
    host.run_until_idle();

    let mut reg = HotRegistry::new(host.clone());
    reg.install(v1_source());
    let report = reg.reload_to(v2_source());
    assert!(!report.ok());
    assert_eq!(report.evicted.len(), 1);
    assert_eq!(report.evicted[0].reason, EvictReason::UnknownSymbol);
    assert_eq!(report.adopted, 0);
    assert_eq!(report.effects_rerun, 0); // retired effect never re-runs
    assert!(host.reload_snapshot().is_empty()); // instance gone
                                                // Post-swap frames run clean (no ghost re-runs into missing props).
    host.run_until_idle();
}

#[test]
fn changed_props_layout_evicts_on_type_mismatch() {
    let host = ComponentHost::new();
    let (_counter, _probe, props) = counter_props(&host, 7);
    host.mount("Counter", props, v1::Counter);
    host.run_until_idle();

    let mut reg = HotRegistry::new(host.clone());
    reg.install(v1_source());
    let report = reg.reload_to(Box::new(StaticSource::new(
        "v2b",
        v2b::__oppa_manifest_descs(),
    )));
    assert!(!report.ok());
    assert_eq!(report.evicted.len(), 1);
    assert_eq!(report.evicted[0].reason, EvictReason::TypeMismatch);
}

#[test]
fn handlers_reresolve_through_reruns() {
    let host = ComponentHost::new();
    let (_counter, _probe, props) = counter_props(&host, 7);
    host.mount("Counter", props, v1::Counter);
    host.run_until_idle();

    let rt = host.runtime();
    let fired = Rc::new(RefCell::new(Vec::new()));
    let id = HandlerId::from_symbol("app.flip");
    let fired_old = fired.clone();
    rt.register_handler(id, move || fired_old.borrow_mut().push("old"));
    rt.dispatch(Event {
        kind: EventKind::Press,
        handler: id,
    });
    assert_eq!(*fired.borrow(), vec!["old"]);

    let mut reg = HotRegistry::new(host.clone());
    reg.install(v1_source());
    let report = reg.reload_to(v1_source());
    assert!(report.ok());

    // New code re-registers the ids it serves (simulated here by the new
    // app revision); dispatch resolves to the NEW closure — no dispatch
    // ever saw a half-swapped table (all inside RELOAD).
    let fired_new = fired.clone();
    rt.register_handler(id, move || fired_new.borrow_mut().push("new"));
    rt.dispatch(Event {
        kind: EventKind::Press,
        handler: id,
    });
    assert_eq!(*fired.borrow(), vec!["old", "new"]);
}

/// Core-side mailbox keys for task results. Tasks cannot capture signals
/// (`!Send` — the capture discipline), so they rendezvous with the UI
/// thread through `keyed_state` (§9.6 authoring pattern: incremental
/// writes through core-side handles).
const TASK_KEY: u64 = 0x7A51;
const TASK_KEY_OLD: u64 = 0x7A52;

#[test]
fn tasks_apply_and_retired_results_discard() {
    // Applied path: same-generation submit lands through INPUT.
    let host = ComponentHost::new();
    let rt = host.runtime();
    rt.spawn_task(move |scope| {
        scope.submit(move |rt| {
            rt.keyed_state::<u32>(TASK_KEY, || 0).set(5);
        });
    });
    // Quiesce the worker thread first: `run_until_idle` can drain the
    // submit before the thread records `tasks_done` (the increment lands
    // microseconds after the body returns).
    let mut waited = 0;
    loop {
        let stats = rt.stats();
        if stats.tasks_done + stats.tasks_dropped == 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        waited += 1;
        assert!(waited < 10_000, "executor task never resolved");
    }
    host.run_until_idle();
    assert_eq!(rt.keyed_state::<u32>(TASK_KEY, || 0).get(), 5);
    assert_eq!(rt.stats().tasks_done, 1);
    assert_eq!(rt.stats().worker_applied, 1);

    // Discard path: a task released AFTER the swap carries the retired
    // generation — applied 0, discarded-or-dropped partition the task, on
    // every interleaving. Synchronization is completion-flag polling, NOT
    // a barrier: a dropped task never runs, so waiting on it would hang.
    let host2 = ComponentHost::new();
    let rt2 = host2.runtime();
    rt2.keyed_state::<u32>(TASK_KEY_OLD, || 0).set(0);
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let done_task = done.clone();
    rt2.spawn_task(move |scope| {
        std::thread::sleep(std::time::Duration::from_millis(50));
        scope.submit(move |rt| {
            rt.keyed_state::<u32>(TASK_KEY_OLD, || 0).set(99);
        });
        done_task.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let mut reg = HotRegistry::new(host2.clone());
    reg.install(v1_source());
    let report = reg.reload_to(v1_source());
    assert!(report.ok());
    // Wait for EITHER completion (ran with the retired tag) OR the
    // harness's drop — both are correct resolutions of the race.
    let mut waited = 0;
    loop {
        if done.load(std::sync::atomic::Ordering::SeqCst) || rt2.stats().tasks_dropped >= 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        waited += 1;
        assert!(waited < 10_000, "executor task never resolved");
    }
    host2.run_until_idle();
    assert_eq!(
        rt2.keyed_state::<u32>(TASK_KEY_OLD, || 0).get(),
        0,
        "retired-generation result must never apply"
    );
    assert_eq!(rt2.stats().worker_applied, 0);
    let stats = rt2.stats();
    // Exactly one resolution: ran-then-discarded, or dropped pre-run.
    assert_eq!(stats.tasks_dropped + stats.tasks_done, 1);
    assert_eq!(
        stats.worker_discarded + stats.tasks_dropped,
        1,
        "every retired task resolves exactly once (discard XOR drop)"
    );
    let _ = report;
}

#[test]
fn keyed_state_survives_swaps() {
    // Residence rule (§9.6): core-side keyed state holds no hot vtables,
    // so the harness never drains it — it survives untouched.
    let host = ComponentHost::new();
    let rt = host.runtime();
    rt.keyed_state::<u32>(9, || 0).set(77);
    let mut reg = HotRegistry::new(host.clone());
    reg.install(v1_source());
    let report = reg.reload_to(v1_source());
    assert!(report.ok());
    assert_eq!(rt.keyed_state::<u32>(9, || 0).get(), 77);
}

#[test]
fn hook_path_runs_swaps_inside_reload_phase() {
    let host = ComponentHost::new();
    let (_counter, probe, props) = counter_props(&host, 7);
    host.mount("Counter", props, v1::Counter);
    host.run_until_idle();
    assert_eq!(probe.get(), 17);

    let reg = Rc::new(RefCell::new(HotRegistry::new(host.clone())));
    reg.borrow_mut().install(v1_source());
    HotRegistry::arm(&reg);
    HotRegistry::request_swap(&reg, v2_source());
    host.run_until_idle();

    let adopted = reg.borrow().last_report().map(|r| r.adopted).unwrap_or(0);
    assert_eq!(adopted, 1);
    // v2 body re-seeds `local` to initial (7); counter untouched (10).
    assert_eq!(probe.get(), 17);
    // No pending swap left behind.
    assert!(reg.borrow().last_report().is_some());
}
