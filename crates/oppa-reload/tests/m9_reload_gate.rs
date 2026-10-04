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

#[path = "m9_reload_gate/burst.rs"]
mod burst;
#[path = "m9_reload_gate/generational.rs"]
mod generational;
#[path = "m9_reload_gate/ime.rs"]
mod ime;
#[path = "m9_reload_gate/scroll.rs"]
mod scroll;
#[path = "m9_reload_gate/tasks.rs"]
mod tasks;
#[path = "m9_reload_gate/transition.rs"]
mod transition;
