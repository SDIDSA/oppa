//! M9 reload product loop + fuzzer gate (§8.4 + §9.6 matrix).
//!
//! The stated precondition for freezing any renderer (Vello/DOM/CPU):
//! a hot-reload swap must never corrupt state, tear the tree, leak a
//! retired generation's data, or race a completing async task against
//! an unload. Everything since M0 (generational checks, stable-symbol
//! handler registry, RELOAD drain-before-unload ordering,
//! cancel-at-reload) was built so this fuzzer has something real to
//! fail against.
//!
//! Five scenarios, each its own test with its own iteration count and
//! timing-variance strategy (stated per test — "adversarial timing"
//! here means seeded xorshift interleavings of workload ops, clock
//! advances, burst sizes, task sleeps, and swap points, not repeated
//! identical runs). Every scenario asserts the same mechanical
//! property, not just absence of a crash:
//!
//! > **No generational slot is ever touched after its generation
//! > retires; no retired-generation task/worker result is ever
//! > applied; no cancelled future is ever resumed.**
//!
//! Mechanically that is: (1) post-swap snapshots carry no
//! outgoing-generation props (`reload_snapshot` generation check —
//! the release-profile twin of the harness's debug
//! `assert_no_outgoing_props`); (2) worker/task counters partition
//! exactly-once (`applied + discarded + dropped == submitted`);
//! (3) the transition evaluator's live nodes all resolve to live
//! retained styles (no interpolation against a retired `NodeId`);
//! (4) retired scratch slots probed directly stay retired
//! (`try_get` is `Err`, `get` panics — the `m9_generational_proof`
//! test proves the check fires, so the fuzzer's silence is not a
//! vacuous pass).
//!
//! Clocks: `ComponentHost::new()` (system clock) everywhere, stated.
//! Interpolations wall-settle inside `run_until_idle`; the fuzz
//! transition duration is 20 ms (not the 120 ms showcase number) to
//! bound wall time — the §9.4 stamp semantics do not depend on the
//! duration. Scroll rows keep the showcase 120 ms (stamped commits
//! never arm the animation, so they cost no wall time).
//!
//! Swap paths, mixed and counted per test: direct `reload_to`
//! (swap-then-drain: queued inputs drain post-swap) and the RELOAD
//! hook (`request_swap` + frames: INPUT drains pre-swap, asserting
//! the §9.1 INPUT→RELOAD ordering in the phase log). Exactly-once
//! input accounting is asserted under both.
//!
//! Honesty notes (coverage limits, stated not hidden):
//!
//! - IME: this fuzzer proves the **core-side** composition session
//!   (content/caret/buffer signals + the `dispatch_ime_event` seam)
//!   survives mid-composition swaps. It does NOT fuzz the TSF/TIP
//!   machinery (`oppa-shell-win`): that path needs a real OS IME and
//!   cannot run headless. Checked instead: the shell crate holds no
//!   reactive handles at all (no `Signal`/`GenArena`/`NodeId`/
//!   `Runtime` — grep-verified), so there is no generational surface
//!   there for this property to bite; the real-IME evidence remains
//!   the two hands-off PASS runs (`spike/results/ime_manual.json`,
//!   locked #28). Recorded as a platform-track residual, not a gate
//!   input.
//! - Real dylib swaps are proven by `real_dylib.rs` (one genuine
//!   adopt-across-unload per run); the fuzz matrix runs over
//!   `StaticSource` because per-iteration cdylib rebuilds would turn
//!   1,000+ swaps into hours. Same protocol, same drain/adopt/evict
//!   code path (`HotRegistry::reload_to`).

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use oppa::{
    dispatch_ime_event, find_retained_by_debug, ComponentHost, Ctx, Div, Ease, ImeCompositionEvent,
    ImeCompositionHandler, InputEvent, KeyState, MsExt, Props, ScrollOffset, Semantics,
    SharedString, Signal, Store, Style, Text, Transition, VNode,
};
use oppa_macros::{component, component_manifest, Props};
use oppa_reload::{HotRegistry, StaticSource};

// ---------------------------------------------------------------------------
// Shared RNG + seed handling (deterministic xorshift, no `rand` dep)
// ---------------------------------------------------------------------------

fn seed_of(test: &str, fallback: u64) -> u64 {
    let seed: u64 = std::env::var("OPPA_FUZZ_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(fallback);
    eprintln!("m9[{test}] seed: {seed:#018x} (override with OPPA_FUZZ_SEED)");
    seed
}

fn xorshift(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

// ---------------------------------------------------------------------------
// Shared domain types (one definition — v1/v2 manifests share Props types,
// so adopt checks pass and only body call-sites shift)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ContactId(u64);

#[derive(Clone, PartialEq, Debug)]
struct Contact {
    display_name: Arc<str>,
    status: Arc<str>,
}

fn seed_contacts(n: usize) -> (Vec<ContactId>, HashMap<ContactId, Contact>) {
    let mut ids = Vec::with_capacity(n);
    let mut values = HashMap::new();
    for i in 0..n {
        let id = ContactId(i as u64);
        ids.push(id);
        values.insert(
            id,
            Contact {
                display_name: Arc::from(format!("Contact {i:03}")),
                status: Arc::from(if i % 3 == 0 { "online" } else { "offline" }),
            },
        );
    }
    (ids, values)
}

const ROW_H: f32 = 56.0;
const VIEWPORT_H: f32 = 600.0;
const OVER: usize = 4;

fn slot_count(viewport_h: f32, row_h: f32, over: usize) -> usize {
    (viewport_h / row_h).ceil() as usize + 1 + 2 * over
}

fn window_first(offset_px: f32, row_h: f32, over: usize, n_items: usize, n_slots: usize) -> usize {
    let first = (offset_px / row_h).floor().max(0.0) as usize;
    let first = first.saturating_sub(over);
    first.min(n_items.saturating_sub(n_slots))
}

fn cell_bg(item: Option<ContactId>, selected: bool) -> Option<oppa::Color> {
    if selected {
        return Some(oppa::Color(0x88_88_88));
    }
    let id = item?;
    Some(if id.0 % 2 == 0 {
        oppa::Color(0x22_22_22)
    } else {
        oppa::Color(0x33_33_33)
    })
}

// ---------------------------------------------------------------------------
// Scenario 1 — reload mid-scroll (slot-keyed virtualized list mid-gesture)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ListProps {
    store: Store<ContactId, Contact>,
    viewport_h: f32,
    row_h: f32,
    over: usize,
}

impl Props for ListProps {}

#[derive(Clone)]
struct RowProps {
    store: Store<ContactId, Contact>,
    item: oppa::Memo<Option<ContactId>>,
    row_h: f32,
}

impl Props for RowProps {}

fn list_body(ctx: &Ctx, props: &ListProps, epoch: bool) -> VNode {
    if epoch {
        let _e = ctx.signal(0u32);
    }
    let offset = ctx.scroll_offset();
    let n = props.store.len();
    let k = slot_count(props.viewport_h, props.row_h, props.over);
    let row_h = props.row_h;
    let over = props.over;
    let first = window_first(offset.get(), row_h, over, n, k);
    let items: Vec<oppa::Memo<Option<ContactId>>> = (0..k)
        .map(|slot| {
            let store = props.store.clone();
            let offset = offset.clone();
            ctx.binding(move || {
                let f = window_first(offset.get(), row_h, over, n, k);
                store.get(f + slot)
            })
        })
        .collect();
    oppa::ScrollArea("list")
        .content_size(n as f32 * row_h)
        .style(Style::new().h(props.viewport_h).fill_width())
        .on_scroll(|| {})
        .children(items.into_iter().enumerate().map(|(slot, item)| {
            let row = first + slot;
            let row_props = RowProps {
                store: props.store.clone(),
                item,
                row_h: props.row_h,
            };
            oppa::Row("slot")
                .style(
                    Style::new()
                        .absolute_y(row as f32 * props.row_h)
                        .h(props.row_h)
                        .fill_width(),
                )
                .key(slot as u64)
                .child(ctx.child("Row", slot as u64, &row_props, row_body_for(epoch)))
        }))
}

// Function-pointer indirection so v1/v2 lists render through their own row
// body (child code follows the parent revision — the product-loop shape).
fn row_body_for(epoch: bool) -> fn(&Ctx, &RowProps) -> VNode {
    if epoch {
        list_v2::Row
    } else {
        list_v1::Row
    }
}

fn row_body(ctx: &Ctx, props: &RowProps, epoch: bool) -> VNode {
    if epoch {
        let _e = ctx.signal(1u32);
    }
    let contact = ctx.memo({
        let store = props.store.clone();
        let item = props.item.clone();
        move || {
            item.read()
                .and_then(|id| store.lookup(&id))
                .unwrap_or_else(|| Contact {
                    display_name: Arc::from(""),
                    status: Arc::from(""),
                })
        }
    });
    let item_id = props.item.read();
    let sel: Signal<bool> = ctx.keyed_state(item_id.map(|id| id.0).unwrap_or(u64::MAX), || false);
    // Showcase 120 ms transition: every recycle is a real bg delta, and
    // the binding-edge stamp must suppress all of it (created delta 0).
    let style = Style::new()
        .h(props.row_h)
        .fill_width()
        .pad_x(12)
        .bg(cell_bg(item_id, sel.get()))
        .transition(Transition::new(120.ms(), Ease::Out));
    let item = props.item.clone();
    let rt = ctx.runtime();
    oppa::Row("cell")
        .style(style)
        .semantics(
            oppa::Semantics::list_item()
                .selected(sel.get())
                .label(&contact.read().display_name),
        )
        .on_press(move || {
            if let Some(id) = item.read() {
                let flag: Signal<bool> = rt.keyed_state(id.0, || false);
                flag.set(!flag.get());
            }
        })
        .children([
            Text {
                text: contact.read().display_name.clone(),
                style: Text::title_small,
            }
            .into(),
            Text {
                text: contact.read().status.clone(),
                style: Text::body_secondary,
            }
            .into(),
        ])
}

mod list_v1 {
    use super::*;

    #[component]
    pub fn List(ctx: &Ctx, props: &ListProps) -> VNode {
        list_body(ctx, props, false)
    }

    #[component]
    pub fn Row(ctx: &Ctx, props: &RowProps) -> VNode {
        row_body(ctx, props, false)
    }

    component_manifest![List(ListProps), Row(RowProps)];
}

mod list_v2 {
    use super::*;

    #[component]
    pub fn List(ctx: &Ctx, props: &ListProps) -> VNode {
        // Edited body: inserted signal shifts later sites → re-seed (§5.1).
        list_body(ctx, props, true)
    }

    #[component]
    pub fn Row(ctx: &Ctx, props: &RowProps) -> VNode {
        row_body(ctx, props, true)
    }

    component_manifest![List(ListProps), Row(RowProps)];
}

/// Asserts the release-profile half of the generational property: no live
/// instance holds props tagged with any retired generation. (The harness
/// also calls the debug-only `assert_no_outgoing_props`.)
fn assert_no_retired_props(host: &ComponentHost, retired: &[oppa::HotGeneration], ctx: &str) {
    for snap in host.reload_snapshot() {
        if let Some(gen) = snap.props_generation {
            assert!(
                !retired.contains(&gen),
                "{ctx}: retired props survived (instance {} gen {gen:?})",
                snap.instance
            );
        }
    }
}

/// Asserts the evaluator never interpolates against a retired `NodeId`:
/// every live interpolation targets a node with a live retained style.
fn assert_evaluator_live_nodes(host: &ComponentHost, ctx: &str) {
    host.with_evaluator(|e| {
        for node in e.live_nodes() {
            assert!(
                host.retained_style(node).is_some(),
                "{ctx}: interpolator targets retired node {node:?}"
            );
        }
    });
}

#[test]
fn m9_reload_mid_scroll() {
    let mut rng = seed_of("mid-scroll", 0x5011_0001);
    const N_ITEMS: usize = 200;
    const ITERS: usize = 250;

    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let (ids, values) = seed_contacts(N_ITEMS);
    let store = Store::new(&rt, ids, values);
    host.set_keyed_capacity(N_ITEMS + 64);
    let k = slot_count(VIEWPORT_H, ROW_H, OVER);
    let handle = host.mount(
        "List",
        ListProps {
            store: store.clone(),
            viewport_h: VIEWPORT_H,
            row_h: ROW_H,
            over: OVER,
        },
        list_v1::List,
    );
    host.run_until_idle();
    let root = handle.root_instance();
    let offset: ScrollOffset = host.instance_scroll(root).expect("scroll handle");
    let list_node = find_retained_by_debug(&host, "list")[0];
    host.bind_scroll(list_node, offset.clone());

    let mut reg = HotRegistry::new(host.clone());
    reg.install(Box::new(StaticSource::new(
        "list-v1",
        list_v1::__oppa_manifest_descs(),
    )));
    // Hook registry for same-frame INPUT→RELOAD swaps (phase-ordering arm).
    let hook_reg = Rc::new(RefCell::new(HotRegistry::new(host.clone())));
    hook_reg.borrow_mut().install(Box::new(StaticSource::new(
        "list-v1-hook",
        list_v1::__oppa_manifest_descs(),
    )));
    HotRegistry::arm(&hook_reg);

    let mut retired: Vec<oppa::HotGeneration> = Vec::new();
    let mut use_v2 = false;
    let mut swaps = 0u32;
    let mut hook_swaps = 0u32;
    let mut direct_swaps = 0u32;
    let mut swaps_mid_sweep = 0u32;
    let mut input_fed = 0u32;
    let mut direct_fed = 0u32;
    let mut nonvacuous_stamped = 0u32;
    let mut offset_px = 0.0f32;
    let max_offset = N_ITEMS as f32 * ROW_H - VIEWPORT_H;
    let mut last_scroll_iter = 0usize;

    for i in 0..ITERS {
        let op = xorshift(&mut rng) % 10;
        match op {
            // 0-4: scroll step — mixed TIME-style direct writes and
            // INPUT-fed Scroll events (window-lag arm: sub-row straddles
            // and full-window jumps both occur).
            0..=4 => {
                let r = xorshift(&mut rng);
                let step = match r % 4 {
                    0 => (r % 7) as f32 + 0.5,           // sub-row straddle
                    1 => ROW_H * ((r % 5) as f32 + 1.0), // whole rows
                    2 => (r % 300) as f32,               // arbitrary jump
                    _ => -(offset_px / 2.0),             // fling back toward top
                };
                offset_px = (offset_px + step).clamp(0.0, max_offset);
                // Stamp instruments, measured around the TICK (the swap
                // re-run below changes nothing, so it stamps nothing —
                // measuring there would be vacuous by construction).
                let created_before = host.with_evaluator(|e| e.created());
                let suppressed_before = host.with_evaluator(|e| e.suppressed());
                if r.is_multiple_of(2) {
                    // INPUT-fed (browser-scroll mapping): routed dy
                    // accumulates at the INPUT boundary.
                    let cur = offset.get();
                    let dy = offset_px - cur;
                    host.inject_input(InputEvent::Scroll {
                        target: list_node,
                        dx: 0.0,
                        dy,
                    });
                    input_fed += 1;
                } else {
                    // TIME-physics style direct write.
                    offset.set(offset_px);
                    direct_fed += 1;
                }
                host.run_until_idle();
                last_scroll_iter = i;
                // Payoff trace, every tick: zero structure ops.
                let tail = host.last_diff().expect("scroll commits");
                assert_eq!(
                    tail.structure_ops(),
                    0,
                    "iter {i}: scroll tick tore structure ({})",
                    tail.structure_ops()
                );
                // Binding-edge ticks create zero interpolators; ticks
                // that rebind count suppressions (non-vacuous stamp).
                let created_after = host.with_evaluator(|e| e.created());
                let suppressed_after = host.with_evaluator(|e| e.suppressed());
                assert_eq!(
                    created_after, created_before,
                    "iter {i}: scroll tick interpolated a rebind"
                );
                if suppressed_after > suppressed_before {
                    nonvacuous_stamped += 1;
                }
            }
            // 5: rare selection flip (unstamped real change — creates a
            // live 120 ms interpolation; wall-settles inside run_until).
            5 => {
                if xorshift(&mut rng).is_multiple_of(4) {
                    let id = ContactId(xorshift(&mut rng) % N_ITEMS as u64);
                    let flag: Signal<bool> = rt.keyed_state(id.0, || false);
                    flag.set(!flag.get());
                    host.run_until_idle();
                } else {
                    host.run_until_idle();
                }
            }
            // 6: rare store mutation (coarse invalidation — all rows
            // re-derive; text deltas jump by lock, never interpolate).
            6 => {
                if xorshift(&mut rng).is_multiple_of(8) {
                    let (mut ids2, mut vals) = (Vec::new(), HashMap::new());
                    for j in 0..N_ITEMS {
                        let id = ContactId(j as u64);
                        ids2.push(id);
                        vals.insert(
                            id,
                            Contact {
                                display_name: Arc::from(format!("Contact {j:03}")),
                                status: Arc::from(if j % 2 == 0 { "busy" } else { "away" }),
                            },
                        );
                    }
                    store.set(ids2, vals);
                    host.run_until_idle();
                }
            }
            // 7-8: swap — direct or hook, at a random point mid-sweep.
            7 | 8 => {
                // Swap re-runs change no values, so they must create no
                // interpolators either (measured here); the stamp itself
                // is measured around scroll ticks above.
                let created_before = host.with_evaluator(|e| e.created());
                let suppressed_before = host.with_evaluator(|e| e.suppressed());
                let _ = suppressed_before;
                use_v2 = !use_v2 || xorshift(&mut rng).is_multiple_of(2);
                let slots_before: Vec<oppa::NodeId> = {
                    let mut v = find_retained_by_debug(&host, "slot");
                    v.sort();
                    v
                };
                assert_eq!(slots_before.len(), k, "slot count drifted pre-swap");
                if xorshift(&mut rng).is_multiple_of(3) {
                    // Hook path: same-frame INPUT→RELOAD ordering. Drain
                    // the phase log first so the ordering check below
                    // reads THIS swap's frame, not stale history.
                    let _ = rt.take_phase_log();
                    let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                        Box::new(StaticSource::new(
                            "list-v2",
                            list_v2::__oppa_manifest_descs(),
                        ))
                    } else {
                        Box::new(StaticSource::new(
                            "list-v1",
                            list_v1::__oppa_manifest_descs(),
                        ))
                    };
                    HotRegistry::request_swap(&hook_reg, src);
                    host.run_until_idle();
                    let rep = hook_reg.borrow();
                    let rep = rep.last_report().expect("hook swap ran");
                    assert!(rep.ok(), "hook swap evicted: {:?}", rep.evicted);
                    retired.push(rep.outgoing);
                    hook_swaps += 1;
                    // Phase ordering: INPUT before RELOAD in the swap frame.
                    let log = rt.take_phase_log();
                    let inp = log.iter().position(|p| *p == oppa::Phase::Input);
                    let rel = log.iter().position(|p| *p == oppa::Phase::Reload);
                    if let (Some(a), Some(b)) = (inp, rel) {
                        assert!(a < b, "RELOAD ran before INPUT ({log:?})");
                    }
                } else {
                    let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                        Box::new(StaticSource::new(
                            "list-v2",
                            list_v2::__oppa_manifest_descs(),
                        ))
                    } else {
                        Box::new(StaticSource::new(
                            "list-v1",
                            list_v1::__oppa_manifest_descs(),
                        ))
                    };
                    let rep = reg.reload_to(src);
                    assert!(rep.ok(), "direct swap evicted: {:?}", rep.evicted);
                    retired.push(rep.outgoing);
                    direct_swaps += 1;
                }
                swaps += 1;
                if i.saturating_sub(last_scroll_iter) <= 2 {
                    swaps_mid_sweep += 1;
                }
                host.run_until_idle();
                // Slot identity survives the swap (keys are slots).
                let mut slots_after = find_retained_by_debug(&host, "slot");
                slots_after.sort();
                assert_eq!(slots_before, slots_after, "iter {i}: slot ids tore");
                // Stamped sweep: swap re-runs create zero interpolators.
                let created_after = host.with_evaluator(|e| e.created());
                assert_eq!(
                    created_after, created_before,
                    "iter {i}: swap created interpolators on stamped commits"
                );
                assert_no_retired_props(&host, &retired, "mid-scroll");
                assert_evaluator_live_nodes(&host, "mid-scroll");
            }
            // 9: settle + invariant sweep.
            _ => {
                host.run_until_idle();
                assert_no_retired_props(&host, &retired, "mid-scroll");
                assert_evaluator_live_nodes(&host, "mid-scroll");
            }
        }
    }

    // Adversarial-proof counters: the fuzzer must actually have swapped
    // mid-sweep through both paths and seen real stamps — else clean.
    eprintln!(
        "m9[mid-scroll] iters={ITERS} swaps={swaps} (hook={hook_swaps} direct={direct_swaps}) \
         mid_sweep={swaps_mid_sweep} input_fed={input_fed} direct_fed={direct_fed} \
         nonvacuous_stamped={nonvacuous_stamped} slots={k}"
    );
    assert!(swaps >= 15, "too few swaps to stress mid-scroll");
    assert!(hook_swaps >= 3, "hook path never exercised mid-scroll");
    assert!(direct_swaps >= 3, "direct path never exercised mid-scroll");
    assert!(
        swaps_mid_sweep >= 5,
        "no swap landed mid-sweep — timing vacuous"
    );
    assert!(input_fed >= 10, "INPUT-fed scroll never exercised");
    assert!(direct_fed >= 10, "TIME-style scroll never exercised");
    assert!(
        nonvacuous_stamped >= 3,
        "stamp never fired non-vacuously — suppression claim vacuous"
    );
}

// ---------------------------------------------------------------------------
// Scenario 2 — reload mid-transition (in-flight interpolation at swap)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct FlipProps {
    label: SharedString,
    /// Author-owned toggle (survives swaps — not a ctx local).
    flag: Signal<bool>,
    probe: Signal<u32>,
}

mod flip_v1 {
    use super::*;

    #[component]
    pub fn Flip(ctx: &Ctx, props: &FlipProps) -> VNode {
        let local = ctx.signal(0u32);
        let bg = if props.flag.get() {
            oppa::Color(0x11_11_11)
        } else {
            oppa::Color(0x22_22_22)
        };
        props
            .probe
            .set(local.get() + if props.flag.get() { 100 } else { 0 });
        let _ = props.label.clone();
        Div("flip")
            .style(
                Style::new()
                    .size(44, 24)
                    .bg(bg)
                    // Fuzz duration (stated): 20 ms wall-settle, not the
                    // 120 ms showcase — stamp semantics are duration-free.
                    .transition(Transition::new(20.ms(), Ease::Out)),
            )
            .on_press({
                let flag = props.flag.clone();
                move || flag.set(!flag.get())
            })
            .build()
    }

    component_manifest![Flip(FlipProps)];
}

mod flip_v2 {
    use super::*;

    #[component]
    pub fn Flip(ctx: &Ctx, props: &FlipProps) -> VNode {
        let _epoch = ctx.signal(0u32);
        let local = ctx.signal(0u32);
        let bg = if props.flag.get() {
            oppa::Color(0x11_11_11)
        } else {
            oppa::Color(0x22_22_22)
        };
        props
            .probe
            .set(local.get() + if props.flag.get() { 100 } else { 0 });
        let _ = props.label.clone();
        Div("flip")
            .style(
                Style::new()
                    .size(44, 24)
                    .bg(bg)
                    .transition(Transition::new(20.ms(), Ease::Out)),
            )
            .on_press({
                let flag = props.flag.clone();
                move || flag.set(!flag.get())
            })
            .build()
    }

    component_manifest![Flip(FlipProps)];
}

#[test]
fn m9_reload_mid_transition() {
    let mut rng = seed_of("mid-transition", 0x5011_0002);
    const ITERS: usize = 250;

    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let flag = rt.signal(false);
    let probe = rt.signal(0u32);
    let mk = |label: &str| FlipProps {
        label: Arc::from(label),
        flag: flag.clone(),
        probe: probe.clone(),
    };
    host.mount("Flip", mk("flip"), flip_v1::Flip);
    host.run_until_idle();

    let mut reg = HotRegistry::new(host.clone());
    reg.install(Box::new(StaticSource::new(
        "flip-v1",
        flip_v1::__oppa_manifest_descs(),
    )));

    let mut retired: Vec<oppa::HotGeneration> = Vec::new();
    let mut use_v2 = false;
    let mut swaps = 0u32;
    let mut swaps_with_live = 0u32;
    let mut flips = 0u32;
    let mut settles_exact = 0u32;

    for i in 0..ITERS {
        let op = xorshift(&mut rng) % 6;
        match op {
            // 0-1: flip the toggle (unstamped real change → one live
            // interpolation), then sometimes swap in the SAME iteration
            // while it is still live — deterministically mid-transition.
            // NOTE: `run_until_idle` wall-settles the 20 ms interpolation
            // (system clock), so liveness is observed after a single
            // `run_once`, never after a full settle; `created` (monotonic)
            // is the creation proof.
            0 | 1 => {
                let created_before = host.with_evaluator(|e| e.created());
                flag.set(!flag.get());
                flips += 1;
                assert!(host.run_once(), "flip frame had no demand");
                let live_now = host.with_evaluator(|e| e.active_count());
                assert!(live_now > 0, "iter {i}: flip created no interpolator");
                assert_eq!(
                    host.with_evaluator(|e| e.created()),
                    created_before + 1,
                    "iter {i}: flip must create exactly one bg interpolator"
                );
                assert_evaluator_live_nodes(&host, "mid-transition flip");
                if xorshift(&mut rng).is_multiple_of(2) {
                    use_v2 = !use_v2;
                    let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                        Box::new(StaticSource::new(
                            "flip-v2",
                            flip_v2::__oppa_manifest_descs(),
                        ))
                    } else {
                        Box::new(StaticSource::new(
                            "flip-v1",
                            flip_v1::__oppa_manifest_descs(),
                        ))
                    };
                    let live_at_swap = host.with_evaluator(|e| e.active_count());
                    let rep = reg.reload_to(src);
                    assert!(rep.ok(), "swap evicted: {:?}", rep.evicted);
                    retired.push(rep.outgoing);
                    swaps += 1;
                    if live_at_swap > 0 {
                        swaps_with_live += 1;
                    }
                    // Live nodes still resolve post-swap (no retired
                    // generation's style values under interpolation).
                    assert_evaluator_live_nodes(&host, "mid-transition swap");
                    assert_no_retired_props(&host, &retired, "mid-transition");
                }
                // Wall-settle then exact-settle assert.
                host.run_until_idle();
                let settled = host.with_evaluator(|e| e.active_count());
                assert_eq!(settled, 0, "iter {i}: interpolation never settled");
                let expect = if flag.get() { 100 } else { 0 };
                assert_eq!(probe.get(), expect, "iter {i}: probe diverged");
                settles_exact += 1;
            }
            // 2: swap at rest (evaluator-target survival, no live interp).
            2 => {
                use_v2 = !use_v2;
                let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                    Box::new(StaticSource::new(
                        "flip-v2",
                        flip_v2::__oppa_manifest_descs(),
                    ))
                } else {
                    Box::new(StaticSource::new(
                        "flip-v1",
                        flip_v1::__oppa_manifest_descs(),
                    ))
                };
                let rep = reg.reload_to(src);
                assert!(rep.ok(), "swap evicted: {:?}", rep.evicted);
                retired.push(rep.outgoing);
                swaps += 1;
                host.run_until_idle();
                let expect = if flag.get() { 100 } else { 0 };
                assert_eq!(probe.get(), expect);
                assert_no_retired_props(&host, &retired, "mid-transition");
                assert_evaluator_live_nodes(&host, "mid-transition");
            }
            // 3-5: settle + invariants.
            _ => {
                host.run_until_idle();
                let expect = if flag.get() { 100 } else { 0 };
                assert_eq!(probe.get(), expect);
                assert_no_retired_props(&host, &retired, "mid-transition");
                assert_evaluator_live_nodes(&host, "mid-transition");
            }
        }
    }

    eprintln!(
        "m9[mid-transition] iters={ITERS} flips={flips} swaps={swaps} \
         swaps_with_live={swaps_with_live} settles_exact={settles_exact}"
    );
    assert!(flips >= 30, "too few flips to stress interpolation");
    assert!(swaps >= 15, "too few swaps");
    assert!(
        swaps_with_live >= 10,
        "no swap landed with a live interpolator — mid-transition timing vacuous"
    );
}

// ---------------------------------------------------------------------------
// Scenario 3 — reload mid-IME-composition (core-side session survival)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct ImeProps {
    label: SharedString,
    content: Signal<String>,
    caret: Signal<usize>,
    comp: Signal<Option<String>>,
    ime_hits: Signal<u32>,
}

/// Minimal core-side composition session over the normalized seam: the
/// shape the spike's `EditingSession` proves at full fidelity. Handler
/// writes are signal writes (core-side, residence-safe); nothing here
/// touches TSF machinery (see module honesty notes).
/// Clamps a byte index down to a char boundary (the fuzz writer is
/// byte-oriented while content goes multibyte after the first commit —
/// test-side simplicity, not engine behavior; the spike session steps
/// by cluster).
fn floor_char(s: &str, mut idx: usize) -> usize {
    idx = idx.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

struct SessionWriter {
    content: Signal<String>,
    caret: Signal<usize>,
    comp: Signal<Option<String>>,
    log: Vec<String>,
}

impl ImeCompositionHandler for SessionWriter {
    fn composition_started(&mut self, start_byte: usize) {
        self.log.push(format!("started@{start_byte}"));
        self.caret.set(start_byte);
        self.comp.set(Some(String::new()));
    }
    fn composition_updated(&mut self, composition: &str, caret_byte: usize) {
        self.log
            .push(format!("updated[{}]@{caret_byte}", composition.len()));
        self.caret.set(caret_byte);
        self.comp.set(Some(composition.to_string()));
    }
    fn composition_committed(&mut self, committed: &str) {
        self.log.push(format!("committed[{}]", committed.len()));
        let mut cur = self.content.get();
        let at = floor_char(&cur, self.caret.get().min(cur.len()));
        cur.insert_str(at, committed);
        self.content.set(cur);
        self.caret.set(at + committed.len());
        self.comp.set(None);
    }
    fn composition_cancelled(&mut self) {
        self.log.push("cancelled".to_string());
        self.comp.set(None);
    }
    fn delete_range(&mut self, range: (usize, usize)) {
        self.log.push(format!("deleted({}-{})", range.0, range.1));
        let mut cur = self.content.get();
        let (a, b) = (
            floor_char(&cur, range.0.min(cur.len())),
            floor_char(&cur, range.1.min(cur.len())),
        );
        if a < b {
            cur.drain(a..b);
            self.content.set(cur);
            self.caret.set(a);
        }
    }
}

mod ime_v1 {
    use super::*;

    #[component]
    pub fn Field(ctx: &Ctx, props: &ImeProps) -> VNode {
        let local = ctx.signal(0u32);
        let _ = (local.get(), props.caret.get());
        let shown = match props.comp.get() {
            Some(c) => format!("{}|{c}", props.content.get()),
            None => props.content.get(),
        };
        let _ = props.label.clone();
        Div("field")
            .style(Style::new().size(200, 24))
            .on_ime({
                let hits = props.ime_hits.clone();
                move || hits.set(hits.get() + 1)
            })
            .child(
                Text {
                    text: Arc::from(shown),
                    style: Text::title_small,
                }
                .into(),
            )
    }

    component_manifest![Field(ImeProps)];
}

mod ime_v2 {
    use super::*;

    #[component]
    pub fn Field(ctx: &Ctx, props: &ImeProps) -> VNode {
        let _epoch = ctx.signal(0u32);
        let local = ctx.signal(0u32);
        let _ = (local.get(), props.caret.get());
        let shown = match props.comp.get() {
            Some(c) => format!("{}|{c}", props.content.get()),
            None => props.content.get(),
        };
        let _ = props.label.clone();
        Div("field")
            .style(Style::new().size(200, 24))
            .on_ime({
                let hits = props.ime_hits.clone();
                move || hits.set(hits.get() + 1)
            })
            .child(
                Text {
                    text: Arc::from(shown),
                    style: Text::title_small,
                }
                .into(),
            )
    }

    component_manifest![Field(ImeProps)];
}

#[test]
fn m9_reload_mid_ime() {
    let mut rng = seed_of("mid-ime", 0x5011_0003);
    const ITERS: usize = 200;

    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let content = rt.signal(String::from("Hello"));
    let caret = rt.signal(5usize);
    let comp = rt.signal(None::<String>);
    let ime_hits = rt.signal(0u32);
    host.mount(
        "Field",
        ImeProps {
            label: Arc::from("field"),
            content: content.clone(),
            caret: caret.clone(),
            comp: comp.clone(),
            ime_hits: ime_hits.clone(),
        },
        ime_v1::Field,
    );
    host.run_until_idle();
    let field_node = find_retained_by_debug(&host, "field")[0];

    let mut reg = HotRegistry::new(host.clone());
    reg.install(Box::new(StaticSource::new(
        "ime-v1",
        ime_v1::__oppa_manifest_descs(),
    )));

    let mut writer = SessionWriter {
        content: content.clone(),
        caret: caret.clone(),
        comp: comp.clone(),
        log: Vec::new(),
    };
    let mut retired: Vec<oppa::HotGeneration> = Vec::new();
    let mut use_v2 = false;
    let mut swaps = 0u32;
    let mut swaps_mid_comp = 0u32;
    let mut sequences = 0u32;
    let mut events_fed = 0u64;

    // Scripted composition alphabet (zh candidate commit, ja
    // kana→candidate, cancel-mid, delete-range re-anchor — the M1
    // scenario shapes, headless through the normalized seam).
    for i in 0..ITERS {
        let op = xorshift(&mut rng) % 8;
        match op {
            // 0-3: run one composition sequence; swap mid-sequence
            // (after Started/Updated, before Commit/Cancel) with p≈1/2.
            0..=3 => {
                sequences += 1;
                let start = (xorshift(&mut rng) % 6) as usize;
                let seq: Vec<ImeCompositionEvent> = match xorshift(&mut rng) % 4 {
                    0 => vec![
                        ImeCompositionEvent::CompositionStarted { start_byte: start },
                        ImeCompositionEvent::CompositionUpdated {
                            composition: "ni".to_string(),
                            caret_byte: start + 2,
                        },
                        ImeCompositionEvent::CompositionUpdated {
                            composition: "nihao".to_string(),
                            caret_byte: start + 5,
                        },
                        ImeCompositionEvent::CompositionCommitted {
                            committed: "你好".to_string(),
                        },
                    ],
                    1 => vec![
                        ImeCompositionEvent::CompositionStarted { start_byte: start },
                        ImeCompositionEvent::CompositionUpdated {
                            composition: "konnitiha".to_string(),
                            caret_byte: start + 9,
                        },
                        ImeCompositionEvent::CompositionCommitted {
                            committed: "こんにちは".to_string(),
                        },
                    ],
                    2 => vec![
                        ImeCompositionEvent::CompositionStarted { start_byte: start },
                        ImeCompositionEvent::CompositionUpdated {
                            composition: "ka".to_string(),
                            caret_byte: start + 2,
                        },
                        ImeCompositionEvent::CompositionCancelled,
                    ],
                    _ => vec![
                        ImeCompositionEvent::DeleteRange { range: (0, start) },
                        ImeCompositionEvent::CompositionStarted { start_byte: 0 },
                        ImeCompositionEvent::CompositionUpdated {
                            composition: "o".to_string(),
                            caret_byte: 1,
                        },
                        ImeCompositionEvent::CompositionCommitted {
                            committed: "o".to_string(),
                        },
                    ],
                };
                let mid = seq.len() / 2;
                for (k, ev) in seq.iter().enumerate() {
                    dispatch_ime_event(&mut writer, ev);
                    events_fed += 1;
                    if k + 1 == mid && xorshift(&mut rng).is_multiple_of(2) {
                        // Mid-composition swap: the buffer must survive.
                        let buf_before = comp.get();
                        let log_before = writer.log.len();
                        use_v2 = !use_v2;
                        let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                            Box::new(StaticSource::new("ime-v2", ime_v2::__oppa_manifest_descs()))
                        } else {
                            Box::new(StaticSource::new("ime-v1", ime_v1::__oppa_manifest_descs()))
                        };
                        let rep = reg.reload_to(src);
                        assert!(rep.ok(), "swap evicted: {:?}", rep.evicted);
                        retired.push(rep.outgoing);
                        swaps += 1;
                        swaps_mid_comp += 1;
                        host.run_until_idle();
                        assert_eq!(
                            comp.get(),
                            buf_before,
                            "iter {i}: composition buffer lost across swap"
                        );
                        assert_eq!(
                            writer.log.len(),
                            log_before,
                            "iter {i}: canonical stream disturbed by swap"
                        );
                        assert_no_retired_props(&host, &retired, "mid-ime");
                    }
                }
                host.run_until_idle();
                // Session readable post-sequence (no retired touch).
                let _ = (content.get(), caret.get(), comp.get());
                assert_no_retired_props(&host, &retired, "mid-ime");
            }
            // 4: route an Ime input event at the field (kind-routing
            // survives swaps — handler re-registration via re-runs).
            4 => {
                let before = ime_hits.get();
                host.inject_input(InputEvent::Ime { target: field_node });
                host.run_until_idle();
                assert_eq!(ime_hits.get(), before + 1, "iter {i}: Ime route lost");
            }
            // 5: swap at rest.
            5 => {
                use_v2 = !use_v2;
                let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                    Box::new(StaticSource::new("ime-v2", ime_v2::__oppa_manifest_descs()))
                } else {
                    Box::new(StaticSource::new("ime-v1", ime_v1::__oppa_manifest_descs()))
                };
                let rep = reg.reload_to(src);
                assert!(rep.ok(), "swap evicted: {:?}", rep.evicted);
                retired.push(rep.outgoing);
                swaps += 1;
                host.run_until_idle();
                assert_no_retired_props(&host, &retired, "mid-ime");
            }
            // 6-7: settle + invariants.
            _ => {
                host.run_until_idle();
                let _ = (content.get(), caret.get(), comp.get());
                assert_no_retired_props(&host, &retired, "mid-ime");
            }
        }
    }

    eprintln!(
        "m9[mid-ime] iters={ITERS} sequences={sequences} events={events_fed} \
         swaps={swaps} mid_composition={swaps_mid_comp}"
    );
    assert!(sequences >= 40, "too few composition sequences");
    assert!(swaps >= 15, "too few swaps");
    assert!(
        swaps_mid_comp >= 10,
        "no swap landed mid-composition — hardest-case timing vacuous"
    );
    assert!(events_fed >= 200, "too few composition events fed");
}

// ---------------------------------------------------------------------------
// Scenario 4 — reload mid-input-burst (INPUT drain vs RELOAD ordering)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct BurstProps {
    label: SharedString,
    hits: Signal<u32>,
}

mod burst_v1 {
    use super::*;

    #[component]
    pub fn Target(ctx: &Ctx, props: &BurstProps) -> VNode {
        let local = ctx.signal(0u32);
        let _ = local.get();
        let _ = props.label.clone();
        Div("tgt")
            .style(Style::new().size(44, 24))
            .semantics(Semantics::switch().checked(false).label(&props.label))
            .on_press({
                let hits = props.hits.clone();
                move || hits.set(hits.get() + 1)
            })
            .build()
    }

    component_manifest![Target(BurstProps)];
}

mod burst_v2 {
    use super::*;

    #[component]
    pub fn Target(ctx: &Ctx, props: &BurstProps) -> VNode {
        let _epoch = ctx.signal(0u32);
        let local = ctx.signal(0u32);
        let _ = local.get();
        let _ = props.label.clone();
        Div("tgt")
            .style(Style::new().size(44, 24))
            .semantics(Semantics::switch().checked(false).label(&props.label))
            .on_press({
                let hits = props.hits.clone();
                move || hits.set(hits.get() + 1)
            })
            .build()
    }

    component_manifest![Target(BurstProps)];
}

#[test]
fn m9_reload_mid_input_burst() {
    let mut rng = seed_of("mid-input-burst", 0x5011_0004);
    const ITERS: usize = 200;

    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let hits = rt.signal(0u32);
    host.mount(
        "Target",
        BurstProps {
            label: Arc::from("tgt"),
            hits: hits.clone(),
        },
        burst_v1::Target,
    );
    host.run_until_idle();
    // Sanity: the hit point lands on the target headless (explicit size
    // → committed box with no text service).
    assert!(host.hit_test(10.0, 12.0).is_some(), "burst rig misses");

    let hook_reg = Rc::new(RefCell::new(HotRegistry::new(host.clone())));
    hook_reg.borrow_mut().install(Box::new(StaticSource::new(
        "burst-v1",
        burst_v1::__oppa_manifest_descs(),
    )));
    HotRegistry::arm(&hook_reg);
    let mut direct_reg = HotRegistry::new(host.clone());
    direct_reg.install(Box::new(StaticSource::new(
        "burst-v1-direct",
        burst_v1::__oppa_manifest_descs(),
    )));

    let mut retired: Vec<oppa::HotGeneration> = Vec::new();
    let mut use_v2 = false;
    let mut hook_swaps = 0u32;
    let mut direct_swaps = 0u32;
    let mut bursts = 0u32;
    let mut events_injected = 0u64;
    let mut ordered_frames = 0u32;

    for i in 0..ITERS {
        let op = xorshift(&mut rng) % 6;
        match op {
            // 0-3: burst of 5-40 pointer Down/Up pairs (+ quiet keys and
            // Tabs that must NOT dispatch), then a swap — hook
            // (INPUT→RELOAD same frame) or direct (swap-then-drain).
            0..=3 => {
                let pairs = 5 + (xorshift(&mut rng) % 36) as usize;
                for _ in 0..pairs {
                    host.inject_input(InputEvent::pointer_down(10.0, 12.0));
                    host.inject_input(InputEvent::pointer_up(10.0, 12.0));
                    events_injected += 2;
                }
                // Ambient keys: quiet no-ops (never dispatch).
                let quiets = (xorshift(&mut rng) % 6) as usize;
                for _ in 0..quiets {
                    host.inject_input(InputEvent::key(0x41, KeyState::Pressed));
                    events_injected += 1;
                }
                if xorshift(&mut rng).is_multiple_of(2) {
                    host.inject_input(InputEvent::key(oppa::input::keys::TAB, KeyState::Pressed));
                    events_injected += 1;
                }
                let before = hits.get();
                bursts += 1;
                use_v2 = !use_v2 || xorshift(&mut rng).is_multiple_of(2);
                if xorshift(&mut rng).is_multiple_of(2) {
                    // Hook path: burst drains at INPUT, swap at RELOAD,
                    // same frame — assert the ordering mechanically.
                    // Pre-drain the log so the check reads this frame.
                    let _ = rt.take_phase_log();
                    let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                        Box::new(StaticSource::new(
                            "burst-v2",
                            burst_v2::__oppa_manifest_descs(),
                        ))
                    } else {
                        Box::new(StaticSource::new(
                            "burst-v1",
                            burst_v1::__oppa_manifest_descs(),
                        ))
                    };
                    HotRegistry::request_swap(&hook_reg, src);
                    assert!(host.run_once(), "burst+swap frame had no demand");
                    let log = rt.take_phase_log();
                    let inp = log.iter().position(|p| *p == oppa::Phase::Input);
                    let rel = log.iter().position(|p| *p == oppa::Phase::Reload);
                    match (inp, rel) {
                        (Some(a), Some(b)) => {
                            assert!(a < b, "iter {i}: RELOAD before INPUT ({log:?})");
                            ordered_frames += 1;
                        }
                        _ => panic!("iter {i}: swap frame missing INPUT/RELOAD ({log:?})"),
                    }
                    host.run_until_idle();
                    let rep = hook_reg.borrow();
                    let rep = rep.last_report().expect("hook swap ran");
                    assert!(rep.ok(), "hook swap evicted: {:?}", rep.evicted);
                    retired.push(rep.outgoing);
                    hook_swaps += 1;
                } else {
                    // Direct path: swap with the burst still queued —
                    // inputs drain post-swap through the new registry.
                    let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                        Box::new(StaticSource::new(
                            "burst-v2",
                            burst_v2::__oppa_manifest_descs(),
                        ))
                    } else {
                        Box::new(StaticSource::new(
                            "burst-v1",
                            burst_v1::__oppa_manifest_descs(),
                        ))
                    };
                    let rep = direct_reg.reload_to(src);
                    assert!(rep.ok(), "direct swap evicted: {:?}", rep.evicted);
                    retired.push(rep.outgoing);
                    direct_swaps += 1;
                    host.run_until_idle();
                }
                // Exactly-once under burst load, both orders: every Down/Up
                // pair dispatched exactly once; quiet keys never did.
                assert_eq!(
                    hits.get(),
                    before + pairs as u32,
                    "iter {i}: burst lost or duplicated under reload"
                );
                assert_no_retired_props(&host, &retired, "input-burst");
            }
            // 4: swap at rest.
            4 => {
                use_v2 = !use_v2;
                let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                    Box::new(StaticSource::new(
                        "burst-v2",
                        burst_v2::__oppa_manifest_descs(),
                    ))
                } else {
                    Box::new(StaticSource::new(
                        "burst-v1",
                        burst_v1::__oppa_manifest_descs(),
                    ))
                };
                let rep = direct_reg.reload_to(src);
                assert!(rep.ok(), "swap evicted: {:?}", rep.evicted);
                retired.push(rep.outgoing);
                direct_swaps += 1;
                host.run_until_idle();
                assert_no_retired_props(&host, &retired, "input-burst");
            }
            // 5: settle.
            _ => {
                host.run_until_idle();
                assert_no_retired_props(&host, &retired, "input-burst");
            }
        }
    }

    eprintln!(
        "m9[input-burst] iters={ITERS} bursts={bursts} events={events_injected} \
         hook={hook_swaps} direct={direct_swaps} ordered_frames={ordered_frames}"
    );
    assert!(bursts >= 60, "too few bursts to stress the drain");
    assert!(hook_swaps >= 10, "hook path never exercised under burst");
    assert!(
        direct_swaps >= 10,
        "direct path never exercised under burst"
    );
    assert_eq!(
        ordered_frames, hook_swaps,
        "every hook frame must show INPUT→RELOAD ordering"
    );
}

// ---------------------------------------------------------------------------
// Scenario 5 — reload with in-flight async tasks (§9.6 residence rule)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct TaskProps {
    label: SharedString,
}

mod task_v1 {
    use super::*;

    #[component]
    pub fn WorkerRoot(ctx: &Ctx, props: &TaskProps) -> VNode {
        let local = ctx.signal(0u32);
        let _ = local.get();
        let _ = props.label.clone();
        Div("worker").build()
    }

    component_manifest![WorkerRoot(TaskProps)];
}

mod task_v2 {
    use super::*;

    #[component]
    pub fn WorkerRoot(ctx: &Ctx, props: &TaskProps) -> VNode {
        let _epoch = ctx.signal(0u32);
        let local = ctx.signal(0u32);
        let _ = local.get();
        let _ = props.label.clone();
        Div("worker").build()
    }

    component_manifest![WorkerRoot(TaskProps)];
}

/// Mailbox key for the task test lives inside the test body (`KEY`).

#[test]
fn m9_reload_inflight_tasks() {
    let mut rng = seed_of("inflight-tasks", 0x5011_0005);
    const ITERS: usize = 150;
    // Separate mailbox key per run would collide across suite runs in one
    // process (keyed_state is per-runtime, and runtimes are per-test —
    // no collision). Fixed key is fine.
    const KEY: u64 = 0x5A51;

    let host = ComponentHost::new();
    let rt = host.runtime();
    host.mount(
        "WorkerRoot",
        TaskProps {
            label: Arc::from("worker"),
        },
        task_v1::WorkerRoot,
    );
    host.run_until_idle();

    let mut reg = HotRegistry::new(host.clone());
    reg.install(Box::new(StaticSource::new(
        "task-v1",
        task_v1::__oppa_manifest_descs(),
    )));

    let mut retired: Vec<oppa::HotGeneration> = Vec::new();
    let mut use_v2 = false;
    let mut swaps = 0u32;
    let mut raced = 0u32;
    let mut spawned: u64 = 0;
    let mut settled_applied = 0u32;

    for i in 0..ITERS {
        let op = xorshift(&mut rng) % 6;
        match op {
            // 0-2: spawn 1-3 tasks with 0-3 ms sleeps, then swap
            // IMMEDIATELY without quiescing — pending tasks hit
            // cancel-at-RELOAD, running ones finish into a retired tag.
            0..=2 => {
                let n = 1 + xorshift(&mut rng) % 3;
                for _ in 0..n {
                    let sleep_ms = xorshift(&mut rng) % 4;
                    spawned += 1;
                    rt.spawn_task(move |scope| {
                        if sleep_ms > 0 {
                            std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
                        }
                        scope.submit(move |rt| {
                            rt.keyed_state::<u32>(KEY, || 0).update(|v| v + 1);
                        });
                    });
                }
                let mail_before: u32 = rt.keyed_state::<u32>(KEY, || 0).get();
                let dropped_before = rt.stats().tasks_dropped;
                let discarded_before = rt.stats().worker_discarded;
                use_v2 = !use_v2 || xorshift(&mut rng).is_multiple_of(2);
                let src: Box<dyn oppa_reload::ComponentSource> = if use_v2 {
                    Box::new(StaticSource::new(
                        "task-v2",
                        task_v2::__oppa_manifest_descs(),
                    ))
                } else {
                    Box::new(StaticSource::new(
                        "task-v1",
                        task_v1::__oppa_manifest_descs(),
                    ))
                };
                let rep = reg.reload_to(src);
                assert!(rep.ok(), "swap evicted: {:?}", rep.evicted);
                assert!(
                    rep.tasks_dropped as u64 <= n,
                    "iter {i}: dropped more than spawned"
                );
                retired.push(rep.outgoing);
                swaps += 1;
                // Quiesce the executor, then drain: every spawned task
                // reaches exactly one terminal state.
                let mut waited = 0;
                loop {
                    let s = rt.stats();
                    if s.tasks_done + s.tasks_dropped >= spawned {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    waited += 1;
                    assert!(waited < 10_000, "iter {i}: tasks never resolved");
                }
                host.run_until_idle();
                // Retired-generation results never apply: the mailbox is
                // untouched by the racing tasks.
                assert_eq!(
                    rt.keyed_state::<u32>(KEY, || 0).get(),
                    mail_before,
                    "iter {i}: retired task result applied"
                );
                let s = rt.stats();
                if s.tasks_dropped > dropped_before || s.worker_discarded > discarded_before {
                    raced += 1;
                }
                assert_no_retired_props(&host, &retired, "inflight-tasks");
            }
            // 3: spawn + settle WITHOUT swapping (applied path — the
            // mailbox works; retired-discards above are not vacuous).
            3 => {
                spawned += 1;
                rt.spawn_task(move |scope| {
                    scope.submit(move |rt| {
                        rt.keyed_state::<u32>(KEY, || 0).update(|v| v + 1);
                    });
                });
                let mut waited = 0;
                loop {
                    let s = rt.stats();
                    if s.tasks_done + s.tasks_dropped >= spawned {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    waited += 1;
                    assert!(waited < 10_000, "iter {i}: task never resolved");
                }
                let before: u32 = rt.keyed_state::<u32>(KEY, || 0).get();
                host.run_until_idle();
                assert_eq!(
                    rt.keyed_state::<u32>(KEY, || 0).get(),
                    before + 1,
                    "iter {i}: same-generation task did not apply"
                );
                settled_applied += 1;
            }
            // 4-5: settle + global accounting.
            _ => {
                host.run_until_idle();
                assert_no_retired_props(&host, &retired, "inflight-tasks");
            }
        }
    }

    // Quiesce + exactly-once end to end.
    let mut waited = 0;
    loop {
        let s = rt.stats();
        if s.tasks_done + s.tasks_dropped >= spawned {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        waited += 1;
        assert!(waited < 10_000, "tasks never resolved");
    }
    host.run_until_idle();
    let s = rt.stats();
    eprintln!(
        "m9[inflight-tasks] iters={ITERS} spawned={spawned} swaps={swaps} raced={raced} \
         applied_path={settled_applied} done={} dropped={} applied={} discarded={}",
        s.tasks_done, s.tasks_dropped, s.worker_applied, s.worker_discarded,
    );
    // Every retired task resolves exactly once (discard XOR drop); the
    // applied-path tasks prove the mailbox counts real applies.
    assert_eq!(s.tasks_done + s.tasks_dropped, spawned);
    assert_eq!(
        s.worker_applied, settled_applied as u64,
        "only the no-swap tasks may apply"
    );
    assert!(swaps >= 15, "too few swaps");
    assert!(raced >= 5, "cancel/discard race never hit — timing vacuous");
    assert!(settled_applied >= 5, "applied path never exercised");
}

// ---------------------------------------------------------------------------
// Mechanical proof: the generational check fires (fuzzer silence is earned)
// ---------------------------------------------------------------------------

/// Formats a caught panic payload (same shape as the arena unit tests).
fn panic_text(payload: &Box<dyn std::any::Any + Send>) -> String {
    (**payload)
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| (**payload).downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

#[test]
fn m9_generational_proof() {
    // GenArena: retired → Retired; reused → StaleGeneration; double
    // retire → AlreadyRetired; OOB → OutOfBounds; live get works.
    let mut arena: oppa::GenArena<u32> = oppa::GenArena::new();
    let a = arena.alloc(1u32);
    assert!(arena.is_alive(a));
    arena.retire(a).expect("retire live");
    assert!(!arena.is_alive(a));
    assert_eq!(
        arena.try_get(a),
        Err(oppa::SlotError::Retired { index: a.index() })
    );
    let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = arena.get(a);
    }))
    .unwrap_err();
    let text = panic_text(&payload);
    assert!(text.contains("retired"), "loud panic was: {text}");
    // Reuse bumps the generation: the old handle is stale, never an alias.
    let b = arena.alloc(2u32);
    assert_eq!(b.index(), a.index());
    assert_eq!(b.generation(), a.generation() + 1);
    assert!(matches!(
        arena.try_get(a),
        Err(oppa::SlotError::StaleGeneration { .. })
    ));
    assert_eq!(*arena.get(b), 2);
    // Double-retire and OOB are loud too.
    arena.retire(b).expect("retire b");
    assert!(matches!(
        arena.retire(b),
        Err(oppa::SlotError::AlreadyRetired { .. })
    ));
    assert!(matches!(
        arena.try_get(oppa::GenerationalId::new(9999, 0)),
        Err(oppa::SlotError::OutOfBounds { .. })
    ));

    // Runtime signal: retiring the slot makes reads panic loudly.
    let rt = ComponentHost::new().runtime();
    let sig = rt.signal(7u32);
    assert_eq!(sig.get(), 7);
    rt.retire_signal(&sig);
    let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = sig.get();
    }))
    .unwrap_err();
    let text = panic_text(&payload);
    assert!(
        text.contains("retired") || text.contains("stale"),
        "retired signal read must panic loudly, got: {text}"
    );

    // NodeId arena: same generation discipline as retained nodes.
    let mut nodes: oppa::NodeArena<u32> = oppa::NodeArena::new();
    let n = nodes.alloc(9);
    let id = oppa::NodeId::from_gen(n);
    assert_eq!(*nodes.get(id.gen()), 9);
    nodes.retire(id.gen()).expect("retire node");
    assert!(nodes.try_get(id.gen()).is_err());

    eprintln!("m9[generational-proof] retired/stale/OOB/double-retire all loud — checks fire");
}
