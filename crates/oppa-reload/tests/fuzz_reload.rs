//! M2b fuzzer v1 (§8.4 headless identity churn + §9.6 task/message path).
//!
//! Deterministic seeded xorshift (no `rand` dependency): fixed default
//! seed, `OPPA_FUZZ_SEED` override, seed printed first. Model-based: the
//! fuzzer predicts the probe value through keeps/reseeds and asserts it
//! after every run, and proves exactly-once task semantics end to end
//! (mailbox increments == applied submits; retired submits never run).

#![allow(non_snake_case)]

use std::sync::Arc;

use oppa::{ComponentHost, Ctx, Div, SharedString, Signal, VNode};
use oppa_macros::{component, component_manifest, Props};
use oppa_reload::{HotRegistry, StaticSource};

#[derive(Clone, Props)]
pub struct PingProps {
    pub label: SharedString,
    pub initial: u32,
    pub counter: Signal<u32>,
    pub probe: Signal<u32>,
}

mod v1 {
    use super::*;

    #[component]
    pub fn Ping(ctx: &Ctx, props: &PingProps) -> VNode {
        let local = ctx.signal(props.initial);
        props.probe.set(local.get() + props.counter.get());
        let _ = props.label.clone();
        Div("ping").build()
    }

    #[component]
    pub fn Bye(ctx: &Ctx, _props: &PingProps) -> VNode {
        let _epoch = ctx.signal(0u32);
        Div("bye").build()
    }

    component_manifest![Ping(PingProps), Bye(PingProps)];
}

mod v2 {
    use super::*;

    #[component]
    pub fn Ping(ctx: &Ctx, props: &PingProps) -> VNode {
        // Edited body: inserted line shifts `local`'s site → re-seed.
        let _epoch = ctx.signal(0u32);
        let local = ctx.signal(props.initial);
        props.probe.set(local.get() + props.counter.get());
        let _ = props.label.clone();
        Div("ping").build()
    }

    #[component]
    pub fn Hi(ctx: &Ctx, _props: &PingProps) -> VNode {
        let _epoch = ctx.signal(1u32);
        Div("hi").build()
    }

    component_manifest![Ping(PingProps), Hi(PingProps)];
}

fn xorshift(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

const MAIL_KEY: u64 = 0xF00D;

#[test]
fn fuzz_reload_churn() {
    let seed: u64 = std::env::var("OPPA_FUZZ_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x9E3779B97F4A7C15);
    eprintln!("fuzz seed: {seed:#018x} (override with OPPA_FUZZ_SEED)");
    let mut rng = seed;

    let host = ComponentHost::new();
    let rt = host.runtime();
    let counter = rt.signal(0u32);
    let probe = rt.signal(0u32);
    let mk_props = |initial: u32| PingProps {
        label: Arc::from("ping"),
        initial,
        counter: counter.clone(),
        probe: probe.clone(),
    };
    let handle = host.mount("Ping", mk_props(1), v1::Ping);
    host.run_until_idle();

    let mut reg = HotRegistry::new(host.clone());
    reg.install(Box::new(StaticSource::new(
        "v1",
        v1::__oppa_manifest_descs(),
    )));

    // Model state: `local[manifest]` is the body's `local` signal value
    // last seen under that manifest version. First visits seed from the
    // current initial (fresh site keys); revisits revive the stored value
    // (keys persist in the instance map — reverting an edit revives its
    // state, the keyed-storage consequence of §5.1). Props updates never
    // touch either slot.
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Active {
        V1,
        V2,
    }
    let mut active = Active::V1;
    let mut local = [1u32, 0u32];
    let mut visited = [true, false];
    let mut initial = 1u32;
    let mut counter_val = 0u32;
    let mut spawned_tasks = 0u64;
    let mut direct_submits = 0u64;
    let mut swaps = 0u32;
    let mut last_outgoing = oppa::HotGeneration(0);

    let descs = |a: Active| match a {
        Active::V1 => StaticSource::new("v1", v1::__oppa_manifest_descs()),
        Active::V2 => StaticSource::new("v2", v2::__oppa_manifest_descs()),
    };

    for i in 0..150 {
        let op = xorshift(&mut rng) % 6;
        match op {
            // 0: run frames, assert the model-predicted probe.
            0 => {
                host.run_until_idle();
                assert_eq!(
                    probe.get(),
                    local[active as usize] + counter_val,
                    "probe diverged from the keep/reseed model"
                );
            }
            // 1: props update (`local` keeps its value — not a swap).
            1 => {
                initial = (xorshift(&mut rng) % 50) as u32;
                handle.set_props(mk_props(initial));
                host.run_until_idle();
                assert_eq!(probe.get(), local[active as usize] + counter_val);
            }
            // 2: counter write (author-owned signal — always live).
            2 => {
                counter_val = (xorshift(&mut rng) % 50) as u32;
                counter.set(counter_val);
                host.run_until_idle();
                assert_eq!(probe.get(), local[active as usize] + counter_val);
            }
            // 3: swap (maybe to the same manifest — rescan path).
            3 => {
                let next = if xorshift(&mut rng).is_multiple_of(2) {
                    active
                } else if active == Active::V1 {
                    Active::V2
                } else {
                    Active::V1
                };
                let report = reg.reload_to(Box::new(descs(next)));
                assert!(report.ok(), "fuzz swap evicted: {:?}", report.evicted);
                assert_eq!(report.drained, 1);
                assert_eq!(report.adopted, 1);
                swaps += 1;
                last_outgoing = report.outgoing;
                if next != active {
                    // First visit seeds from current initial; revisits
                    // revive the stored per-manifest value.
                    if !visited[next as usize] {
                        local[next as usize] = initial;
                        visited[next as usize] = true;
                    }
                    active = next;
                }
                host.run_until_idle();
                assert_eq!(probe.get(), local[active as usize] + counter_val);
            }
            // 4: spawn a task (increment mailbox on apply).
            4 => {
                spawned_tasks += 1;
                rt.spawn_task(move |scope| {
                    scope.submit(move |rt| {
                        rt.keyed_state::<u32>(MAIL_KEY, || 0).update(|v| v + 1);
                    });
                });
            }
            // 5: direct worker submit (same accounting).
            _ => {
                direct_submits += 1;
                rt.worker_submit(move |rt| {
                    rt.keyed_state::<u32>(MAIL_KEY, || 0).update(|v| v + 1);
                });
                host.run_until_idle();
            }
        }
        eprintln!(
            "iter {i}: op={op} probe={} model_local={:?} counter={counter_val} initial={initial}",
            probe.get(),
            local
        );
    }

    // Quiesce: every spawned task reaches a terminal state…
    let mut waited = 0;
    loop {
        let stats = rt.stats();
        if stats.tasks_done + stats.tasks_dropped == spawned_tasks {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        waited += 1;
        assert!(waited < 10_000, "fuzz tasks never resolved");
    }
    host.run_until_idle();

    // …then exactly-once holds end to end: every applied submit
    // incremented once; retired submits never ran. Dropped pre-run tasks
    // never submit, so they stand outside the worker accounting.
    let stats = rt.stats();
    let submitted = spawned_tasks + direct_submits;
    assert_eq!(
        stats.worker_applied + stats.worker_discarded + stats.tasks_dropped,
        submitted
    );
    assert_eq!(
        rt.keyed_state::<u32>(MAIL_KEY, || 0).get(),
        stats.worker_applied as u32
    );
    // Soundness: no retired-generation props anywhere (also asserted in
    // the harness on debug builds — belt and suspenders).
    if swaps > 0 {
        for snap in host.reload_snapshot() {
            if let Some(gen) = snap.props_generation {
                assert_ne!(gen, last_outgoing, "retired props survived");
            }
        }
    }
}
