//! M8 acceptance: virtualization + transition evaluator (§9.4 stamp end-to-end).
//!
//! The full §4.2 payoff trace asserted against the real reconciler: the
//! recycled ContactList/ContactRow with slot keys, the TIME transition
//! evaluator honoring the binding-edge stamp, and the window-lag
//! compensation under a scripted offset sweep.
//!
//! Mechanical notes (same class as M2's D1–D6, stated not smuggled):
//!
//! - Rows carry no `Img` (async decode is explicitly out of round scope —
//!   backends refuse `RImg` loudly, so the sweep rows are bg + text only).
//! - Selection lives in `keyed_state` keyed by item identity (the brief's
//!   requirement); the host's keyed capacity is sized to the list
//!   (`set_keyed_capacity` — the M8 answer to M2 decision 50's deferred
//!   per-list namespacing: one global LRU + explicit per-host sizing).
//! - Slots track the overscanned window (`window_first`): slot identity
//!   (keys) stays fixed while positions follow the window — structure
//!   stays 0, position changes are LAYOUT-only updates (M8 decision 119).
//! - Zebra striping (bg alternates by item index) makes every recycle a
//!   real bg delta: the phantom-flash stress is structural, not arranged.
//!
//! What lives where: this file proves the core trace (mount/scale,
//! zero-structure sweep, sub-row dedup, selection identity, per-slot flag
//! attribution, TIME-vs-INPUT feed equivalence, TIME evaluator
//! end-to-end off the injected clock, stamped zero-interpolator counts).
//! Pixels live in `oppa-cpu` (`m8_sweep`: oracle frame-by-frame exact +
//! repaint bound) and `oppa-vello` (`m8_sweep`: tol-banded rows); the DOM
//! mapping lives in `oppa-dom` (`m8_transitions`).

#![allow(non_snake_case)]

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use oppa::{
    find_retained_by_debug, ComponentHost, Ctx, Ease, InputEvent, Memo, MsExt, Props, ScrollOffset,
    Signal, Store, Style, Text, Transition, VNode,
};

// ---------------------------------------------------------------------------
// Shared fakes: contacts, palette
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ContactId(u64);

#[derive(Clone, PartialEq, Debug)]
struct Contact {
    display_name: Arc<str>,
    status: Arc<str>,
}

impl Contact {
    fn placeholder() -> Self {
        Self {
            display_name: Arc::from(""),
            status: Arc::from(""),
        }
    }
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
const N_ITEMS: usize = 1000;

const BASE_EVEN: oppa::Color = oppa::Color(0x22_22_22);
const BASE_ODD: oppa::Color = oppa::Color(0x33_33_33);
const SELECTION_BG: oppa::Color = oppa::Color(0x88_88_88);
const HOVER_BG: oppa::Color = oppa::Color(0x44_44_44);
const PRESS_BG: oppa::Color = oppa::Color(0x55_55_55);
const FOCUS_RING: oppa::Color = oppa::Color(0xAA_BB_CC);

/// First visible window row under the overscan (pure — shared by the
/// component and the tests so both compute the same window).
fn window_first(offset_px: f32, row_h: f32, over: usize, n_items: usize, n_slots: usize) -> usize {
    let first = (offset_px / row_h).floor().max(0.0) as usize;
    let first = first.saturating_sub(over);
    let max_first = n_items.saturating_sub(n_slots);
    first.min(max_first)
}

fn slot_count(viewport_h: f32, row_h: f32, over: usize) -> usize {
    // Visible rows + 1 straddle margin (a sub-row offset straddles one
    // extra row) + overscan both sides (M8 decision 127 — the sweep
    // caught the margin missing: row 15 visible with only rows 0..14
    // covered at a straddling offset near max velocity).
    (viewport_h / row_h).ceil() as usize + 1 + 2 * over
}

/// Cell background priority: pressed > selected > hover > zebra base.
/// `None` item (out-of-range slot) paints no bg.
fn cell_bg(
    item: Option<ContactId>,
    selected: bool,
    hovered: bool,
    pressed: bool,
) -> Option<oppa::Color> {
    if pressed {
        return Some(PRESS_BG);
    }
    if selected {
        return Some(SELECTION_BG);
    }
    if hovered {
        return Some(HOVER_BG);
    }
    let id = item?;
    Some(if id.0 % 2 == 0 { BASE_EVEN } else { BASE_ODD })
}

// ---------------------------------------------------------------------------
// §4.2 ContactList / ContactRow port (M8 harness)
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
    item: Memo<Option<ContactId>>,
    row_h: f32,
}

impl Props for RowProps {}

fn ContactList(ctx: &Ctx, props: &ListProps) -> VNode {
    let offset = ctx.scroll_offset();
    let n = props.store.len();
    let k = slot_count(props.viewport_h, props.row_h, props.over);
    let row_h = props.row_h;
    let over = props.over;
    // Tracked read: the list re-runs when the offset moves (slot
    // positions follow the window; item bindings re-derive below).
    let first = window_first(offset.get(), row_h, over, n, k);
    let items: Vec<Memo<Option<ContactId>>> = (0..k)
        .map(|slot| {
            let store = props.store.clone();
            let offset = offset.clone();
            ctx.binding(move || {
                let f = window_first(offset.get(), row_h, over, n, k);
                store.get(f + slot)
            })
        })
        .collect();
    debug_assert_eq!(first, window_first(offset.get(), row_h, over, n, k));
    let _ = first;
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
                .child(ctx.child("ContactRow", slot as u64, &row_props, ContactRow))
        }))
}

fn ContactRow(ctx: &Ctx, props: &RowProps) -> VNode {
    // Re-derives whenever the slot re-binds OR the store mutates that id.
    let contact = ctx.memo({
        let store = props.store.clone();
        let item = props.item.clone();
        move || {
            item.read()
                .and_then(|id| store.lookup(&id))
                .unwrap_or_else(Contact::placeholder)
        }
    });
    // Per-instance selection: keyed by ITEM identity, not slot position
    // (§4.2 escape hatch — scroll out and back, selection survives).
    let item_id = props.item.read();
    let sel_flag: Signal<bool> =
        ctx.keyed_state(item_id.map(|id| id.0).unwrap_or(u64::MAX), || false);
    let hovered = ctx.hovered();
    let pressed = ctx.pressed();
    let focused = ctx.focused();

    let bg = cell_bg(item_id, sel_flag.get(), hovered.get(), pressed.get());
    // `fill_width`: the cell fills its slot (a zero-width cell would let
    // hit-testing fall through to the slot row, which carries no Press
    // handler — the M5 loud-miss rule would fire on every hover).
    let mut style = Style::new()
        .h(props.row_h)
        .fill_width()
        .pad_x(12)
        .bg(bg)
        .transition(Transition::new(120.ms(), Ease::Out));
    if focused.get() {
        style = style.border(2, FOCUS_RING);
    }
    let item = props.item.clone();
    let rt = ctx.runtime();
    oppa::Row("cell")
        .style(style)
        .semantics(
            oppa::Semantics::list_item()
                .selected(sel_flag.get())
                .label(&contact.read().display_name),
        )
        .on_press(move || {
            // Handlers capture signals, not item data (§4.2): the press
            // reads the slot's CURRENT item at dispatch, so a recycled
            // slot selects what it shows, not what it showed.
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

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Harness {
    host: ComponentHost,
    root: u64,
    offset: ScrollOffset,
    list: oppa::NodeId,
    k: usize,
    clock: Rc<oppa::MockClock>,
}

fn mount_list(n_items: usize, over: usize) -> (Harness, oppa::MountHandle<ListProps>) {
    // Injected clock throughout (deterministic TIME stamps; the
    // evaluator commits against this clock exactly as against the
    // system clock — M6 cadence seam).
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let rt = host.runtime();
    let (ids, values) = seed_contacts(n_items);
    let store = Store::new(&rt, ids, values);
    // Per-list keyed sizing (M8 decision on M2 decision 50's deferral):
    // the selection set is keyed by item identity across the whole list,
    // so capacity covers the list; unconfigured hosts keep the global 64.
    host.set_keyed_capacity(n_items.max(64) + 64);
    let handle = host.mount(
        "ContactList",
        ListProps {
            store,
            viewport_h: VIEWPORT_H,
            row_h: ROW_H,
            over,
        },
        ContactList,
    );
    host.run_until_idle();
    let root = handle.root_instance();
    let offset = host.instance_scroll(root).expect("scroll handle");
    let list = find_retained_by_debug(&host, "list")[0];
    host.bind_scroll(list, offset.clone());
    let k = slot_count(VIEWPORT_H, ROW_H, over);
    (
        Harness {
            host,
            root,
            offset,
            list,
            k,
            clock,
        },
        handle,
    )
}

/// Bounded frame pump (M8 TIME discipline): advances the injected clock
/// by `dt`, then runs frames until idle (cap 100 — always terminates,
/// unlike bare `run_until_idle` under a frozen clock with a live
/// interpolation, which would spin until wall-clock settle). Asserts
/// full idle afterwards, so a leaked interpolation trips loudly instead
/// of hanging CI.
///
/// Ordering matters: signal writes commit during frames, so frames run
/// first at the current time (committing at `t` — a fresh interpolation
/// is legitimately live here), then time advances, then frames run again
/// (settling at `t + dt`). Only the end state must be idle.
fn pump(h: &Harness, dt: f64) {
    run_bounded(h);
    h.clock.advance(dt);
    run_bounded(h);
    assert!(
        !h.host.runtime().has_demand(),
        "pump({dt}s) must settle: live interpolations or pending work remain"
    );
}

/// Runs up to 100 frames (always terminates, even with a live
/// interpolation on a frozen clock — the cap bounds the spin).
fn run_bounded(h: &Harness) {
    for _ in 0..100 {
        if !h.host.run_once() {
            break;
        }
    }
}

/// One vsync step (the sweep cadence).
const TICK: f64 = 1.0 / 60.0;

/// Slot-position cell node: the `cell` whose committed box sits at
/// content y = `row * ROW_H` (window-tracking slots move with the
/// window, so identity follows the box, not the index).
fn cell_at_row(h: &Harness, row: usize) -> oppa::NodeId {
    let cells = find_retained_by_debug(&h.host, "cell");
    let want = row as f32 * ROW_H;
    cells
        .into_iter()
        .find(|id| {
            h.host
                .committed_box(*id)
                .is_some_and(|b| (b.y - want).abs() < 0.01)
        })
        .unwrap_or_else(|| panic!("no cell at row {row} (y={want})"))
}

fn cell_bg_of(h: &Harness, id: oppa::NodeId) -> Option<oppa::Color> {
    let sid = h.host.retained_style(id).expect("cell style");
    h.host
        .with_retained_mut(|_, styles| styles.get(sid).expect("style").bg)
}

fn evaluated_cell_bg(h: &Harness, id: oppa::NodeId, now: f64) -> Option<oppa::Color> {
    h.host
        .with_evaluator(|e| e.resolve_bg(id, cell_bg_of(h, id), now))
}

fn selected_of(host: &ComponentHost, id: ContactId) -> bool {
    oppa::untrack(|| host.runtime().keyed_state::<bool>(id.0, || false).get())
}

// ---------------------------------------------------------------------------
// 1. Recycled list mounts (N items, K slots, slot-key stability)
// ---------------------------------------------------------------------------

#[test]
fn recycled_list_mounts_with_stable_slot_keys() {
    let (h, _) = mount_list(N_ITEMS, OVER);
    let slots = find_retained_by_debug(&h.host, "slot");
    assert_eq!(
        slots.len(),
        h.k,
        "K = visible + straddle + 2*overscan = {}",
        h.k
    );
    assert_eq!(h.k, 20, "11 visible + 1 straddle + 8 overscan at 600/56");
    let cells = find_retained_by_debug(&h.host, "cell");
    assert_eq!(cells.len(), h.k, "one cell per slot");
    let before: HashSet<oppa::NodeId> = slots.into_iter().collect();
    let retained_before = h.host.retained_count();

    // Scroll half the list: slot Nodes must be identical (recycling is
    // the reconciler emitting nothing — no pool, no teardown).
    h.offset.set(500.0 * ROW_H);
    pump(&h, TICK);
    let after: HashSet<oppa::NodeId> = find_retained_by_debug(&h.host, "slot")
        .into_iter()
        .collect();
    assert_eq!(before, after, "slot identity is the slot, not the item");
    assert_eq!(
        h.host.retained_count(),
        retained_before,
        "no create/destroy"
    );
    let d = h.host.last_diff().expect("scroll diff");
    assert_eq!(d.structure_ops(), 0, "scroll tick is Update-only");
    assert!(d.suppress_transitions, "binding edges stamp the tick");
    // Slot 0 now shows item `first`, positioned at its row.
    let first = window_first(500.0 * ROW_H, ROW_H, OVER, N_ITEMS, h.k);
    assert_eq!(first, 496);
    let top = cell_at_row(&h, first);
    assert_eq!(
        cell_bg_of(&h, top),
        Some(BASE_EVEN),
        "zebra follows the item"
    );
}

#[test]
fn tiny_list_placeholders_keep_slot_count() {
    let (h, _) = mount_list(5, OVER);
    assert_eq!(find_retained_by_debug(&h.host, "slot").len(), h.k);
    assert_eq!(find_retained_by_debug(&h.host, "cell").len(), h.k);
    // Out-of-range slots show the placeholder (no bg, empty label).
    let d = h.host.last_diff().expect("mount diff");
    assert!(d.structure_ops() > 0, "mount builds structure");
    h.offset.set(0.0);
    pump(&h, TICK);
    let tail = cell_at_row(&h, 5);
    assert_eq!(cell_bg_of(&h, tail), None, "placeholder paints no bg");
}

// ---------------------------------------------------------------------------
// 2. Sweep ticks: zero structure ops per tick at scale
// ---------------------------------------------------------------------------

#[test]
fn sweep_ticks_zero_structure_ops_per_tick_at_scale() {
    let (h, _) = mount_list(N_ITEMS, OVER);
    let max_first = N_ITEMS - h.k;
    let mut stamped_ticks = 0usize;
    let mut row_ticks = 0usize;
    let mut prev_first = window_first(0.0, ROW_H, OVER, N_ITEMS, h.k);
    // Scripted offset sweep: full list, one row per tick (fast scroll),
    // plus the endpoints — the M7 5-slot test becomes the full trace.
    // (Tick 0 is the mount state — setting the same offset commits at
    // most an empty diff, so the sweep starts at step 1.)
    for step in 1..=max_first {
        h.offset.set(step as f32 * ROW_H);
        pump(&h, TICK);
        // The leading overscan absorbs the first OVER rows: the window
        // (and every binding) only moves once floor(offset/row) clears
        // it — non-moving ticks commit at most an empty diff.
        let first = window_first(step as f32 * ROW_H, ROW_H, OVER, N_ITEMS, h.k);
        let d = h.host.last_diff().expect("tick diff");
        if first != prev_first {
            assert_eq!(d.structure_ops(), 0, "tick {step}: zero structure ops");
            assert!(
                d.suppress_transitions,
                "tick {step}: binding edges stamp every window move"
            );
            stamped_ticks += 1;
            // Interpolator count on binding-edge commits: zero — counted
            // in the evaluator, not inferred from jumped values.
            assert_eq!(
                h.host.with_evaluator(|e| e.active_count()),
                0,
                "tick {step}: no live interpolators under the stamp"
            );
            assert_eq!(
                h.host.with_evaluator(|e| e.created()),
                0,
                "tick {step}: zero interpolators created across the sweep"
            );
        } else {
            assert!(
                d.is_empty(),
                "tick {step}: unmoved window commits nothing: {:?}",
                d.ops
            );
        }
        prev_first = first;
        row_ticks += 1;
    }
    assert_eq!(row_ticks, max_first, "full-list sweep, every row");
    assert_eq!(
        stamped_ticks,
        max_first - OVER,
        "every window move stamps (leading overscan absorbs the first rows)"
    );
    assert!(
        h.host.with_evaluator(|e| e.suppressed()) > 0,
        "zebra deltas under the stamp must count suppression (non-vacuous)"
    );
}

#[test]
fn sub_row_offsets_commit_nothing_at_all() {
    let (h, _) = mount_list(N_ITEMS, OVER);
    let diffs = h.host.diff_count();
    // All inside row 0's band: the binding memo gate dedups (equal
    // values invalidate nothing — the reconciler doc's zero-ops-at-all:
    // commits may exist (the list body re-runs on the offset write) but
    // every one is empty).
    for px in [10.0, 20.0, 30.0, 40.0, 50.0] {
        h.offset.set(px);
        pump(&h, TICK);
    }
    for d in h.host.diffs_from(diffs) {
        assert!(d.is_empty(), "sub-row offsets yield zero ops: {:?}", d.ops);
    }
}

// ---------------------------------------------------------------------------
// 3. Per-instance selection across recycling (identity, not position)
// ---------------------------------------------------------------------------

#[test]
fn selection_follows_identity_across_recycle() {
    let (h, _) = mount_list(N_ITEMS, OVER);
    // Select item 5 through its keyed flag (what the press handler writes).
    h.host.runtime().keyed_state::<bool>(5, || false).set(true);
    // The selection change is real (unstamped) so it interpolates: settle
    // a full second before asserting the painted state.
    pump(&h, 1.0);
    let row5 = 5usize;
    assert_eq!(
        cell_bg_of(&h, cell_at_row(&h, row5)),
        Some(SELECTION_BG),
        "selected item paints the selection bg"
    );
    // Scroll out: item 5 leaves the window entirely.
    h.offset.set((N_ITEMS - h.k) as f32 * ROW_H);
    pump(&h, TICK);
    assert!(
        !find_retained_by_debug(&h.host, "cell")
            .iter()
            .any(|id| cell_bg_of(&h, *id) == Some(SELECTION_BG)),
        "no slot shows selection while item 5 is out of window"
    );
    // Scroll back: selection survives (identity, not position).
    h.offset.set(0.0);
    pump(&h, TICK);
    assert!(
        selected_of(&h.host, ContactId(5)),
        "flag survives the round trip"
    );
    assert_eq!(
        cell_bg_of(&h, cell_at_row(&h, row5)),
        Some(SELECTION_BG),
        "selection follows the item back into its (new) slot"
    );
}

// ---------------------------------------------------------------------------
// 4. Per-slot flag attribution (M8 definition + proof)
// ---------------------------------------------------------------------------
//
// Definition (M8, carried since M6): hover/press/focus are SLOT-scoped —
// stable position identity. A row renders its CURRENT item through the
// slot's flags; flags never follow an item to another slot, and press
// handlers read the slot's current binding at dispatch. This matches
// the DOM (the same element keeps :hover/focus across a rebind).

#[test]
fn per_slot_flags_attribute_to_the_current_item() {
    let (h, _) = mount_list(100, OVER);
    let slot0 = h.host.lookup_child(h.root, 0).expect("slot-0 instance");

    // Hover slot 0 (item 0, screen y center 28 → content y 28).
    h.host.inject_input(InputEvent::pointer_move(400.0, 28.0));
    // Hover is a real (unstamped) visual change: settle past the 120 ms
    // interpolation; the flag itself persists (position identity).
    pump(&h, 1.0);
    assert_eq!(
        h.host.debug_instance_flags(slot0),
        (true, false, false),
        "hover lands on the slot instance"
    );
    assert_eq!(
        cell_bg_of(&h, cell_at_row(&h, 0)),
        Some(HOVER_BG),
        "row 0 renders hover for item 0"
    );

    // Scroll five rows (INPUT-fed, like the Web seam): the leading
    // overscan absorbs the first four, so the window moves 0 → 1 and
    // slot 0 now shows item 1 under a stationary pointer (no new
    // pointer events fire — the flag stays with the slot).
    h.host.inject_input(InputEvent::Scroll {
        target: h.list,
        dx: 0.0,
        dy: 5.0 * ROW_H,
    });
    pump(&h, 1.0);
    assert_eq!(h.offset.get(), 5.0 * ROW_H, "INPUT feed applied the tick");
    assert_eq!(
        h.host.debug_instance_flags(slot0),
        (true, false, false),
        "hover stays with the slot (position identity)"
    );
    assert_eq!(
        cell_bg_of(&h, cell_at_row(&h, 1)),
        Some(HOVER_BG),
        "hover now attributes to the current item (item 1)"
    );
    // Item 0 shows hover nowhere else (flags never follow the item).
    let hover_cells = find_retained_by_debug(&h.host, "cell")
        .into_iter()
        .filter(|id| cell_bg_of(&h, *id) == Some(HOVER_BG))
        .count();
    assert_eq!(hover_cells, 1, "exactly the hovered slot is tinted");

    // Press on the slot selects the CURRENT item (item 1, not item 0).
    let y = h.host.committed_box(cell_at_row(&h, 1)).expect("box").y + 28.0;
    h.host.inject_input(InputEvent::pointer_down(400.0, y));
    h.host.inject_input(InputEvent::pointer_up(400.0, y));
    pump(&h, 1.0);
    assert!(selected_of(&h.host, ContactId(1)), "press selected item 1");
    assert!(!selected_of(&h.host, ContactId(0)), "item 0 untouched");
    assert!(
        !h.host.debug_instance_flags(slot0).1,
        "press clears (no stuck pressed)"
    );

    // Focus follows click and stays position-scoped across recycle.
    let focus_before = h.host.focused_node();
    assert!(focus_before.is_some(), "click focuses the slot owner");
    h.host.inject_input(InputEvent::Scroll {
        target: h.list,
        dx: 0.0,
        dy: ROW_H,
    });
    pump(&h, 1.0);
    assert_eq!(
        h.host.focused_node(),
        focus_before,
        "focus stays on the slot node while its item changes"
    );
    assert!(
        h.host.debug_instance_flags(slot0).2,
        "slot instance still focused"
    );
}

// ---------------------------------------------------------------------------
// 5. TIME feed vs INPUT feed equivalence (one signal, two mechanisms)
// ---------------------------------------------------------------------------

#[test]
fn time_physics_and_input_feed_converge_on_the_same_window() {
    let (a, _) = mount_list(200, OVER);
    let (b, _) = mount_list(200, OVER);
    // A: TIME-physics stand-in — direct signal writes (what GPU TIME
    // physics performs); B: INPUT-fed scroll events (the Web mapping).
    for step in [0usize, 3, 17, 42, 100, 181] {
        a.offset.set(step as f32 * ROW_H);
        pump(&a, TICK);
        b.host.inject_input(InputEvent::Scroll {
            target: b.list,
            dx: 0.0,
            dy: step as f32 * ROW_H - b.offset.get(),
        });
        pump(&b, TICK);
        assert_eq!(a.offset.get(), b.offset.get(), "step {step}: same offset");
        let da = a.host.last_diff().expect("a diff");
        let db = b.host.last_diff().expect("b diff");
        assert_eq!(da.structure_ops(), 0, "step {step}: A Update-only");
        assert_eq!(db.structure_ops(), 0, "step {step}: B Update-only");
        let name_a = text_of(&a, 0, 200);
        let name_b = text_of(&b, 0, 200);
        assert_eq!(name_a, name_b, "step {step}: same window head");
    }
}

/// Display name rendered in the slot at window position `slot`.
fn text_of(h: &Harness, slot: usize, n_items: usize) -> String {
    let first = window_first(h.offset.get(), ROW_H, OVER, n_items, h.k);
    let _ = slot;
    let cell = cell_at_row(h, first + slot);
    h.host
        .with_retained_mut(|rec, _| {
            rec.get(cell).and_then(|n| {
                n.children.iter().find_map(|c| {
                    rec.get(*c).and_then(|t| {
                        t.children.iter().find_map(|g| {
                            rec.get(*g)
                                .and_then(|leaf| leaf.text.as_deref().map(str::to_string))
                        })
                    })
                })
            })
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// 6. TIME evaluator end-to-end off the injected clock
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct CardProps {
    bg: oppa::Color,
}

impl Props for CardProps {}

fn render_card(_ctx: &Ctx, props: &CardProps) -> VNode {
    // No binding edge: a plain state change MUST interpolate (the
    // non-vacuous control for the stamped sweep's zero).
    oppa::Div("card")
        .style(
            Style::new()
                .size(100, 40)
                .bg(props.bg)
                .transition(Transition::new(120.ms(), Ease::Out)),
        )
        .build()
}

#[test]
fn time_evaluator_interpolates_120ms_bg_over_frames() {
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let handle = host.mount(
        "Card",
        CardProps {
            bg: oppa::Color(0x10_10_10),
        },
        render_card,
    );
    host.run_until_idle();
    let frames0 = host.runtime().stats().frames;

    handle.set_props(CardProps {
        bg: oppa::Color(0x90_90_90),
    });
    let card = find_retained_by_debug(&host, "card")[0];
    // Sample the ramp frame-by-frame at 60 Hz off the injected clock.
    // The host owns the TIME drive (registers the settle animation on
    // the creating commit), so single frames advance the interpolation.
    let mut ramp: Vec<u32> = Vec::new();
    for _ in 0..60 {
        clock.advance(1.0 / 60.0);
        host.run_once();
        let now = clock.get();
        let c = host.with_evaluator(|e| e.resolve_bg(card, None, now).expect("tracked"));
        ramp.push((c.0 >> 16) & 0xFF);
        if host.with_evaluator(|e| e.active_count()) == 0 {
            break;
        }
    }
    assert_eq!(
        host.with_evaluator(|e| e.created()),
        1,
        "one real change → one interpolator"
    );
    for w in ramp.windows(2) {
        assert!(w[1] >= w[0], "monotonic ramp: {ramp:?}");
    }
    assert_eq!(*ramp.last().expect("samples"), 0x90, "settles exact");
    let frames = host.runtime().stats().frames - frames0;
    // 120 ms at 60 Hz ≈ 7–8 frames (the change frame + the settle tail).
    assert!(
        (5..=12).contains(&frames),
        "frame count ≈ duration off the injected clock: {frames} frames, ramp {ramp:?}"
    );
    assert!(
        !host.runtime().has_demand(),
        "settled evaluator idles (no live interpolations)"
    );
}

// ---------------------------------------------------------------------------
// 7. Stamped recycle with live transitions: zero interpolators, jump
// ---------------------------------------------------------------------------

#[test]
fn stamped_sweep_creates_zero_interpolators_and_never_flashes() {
    let (h, _) = mount_list(300, OVER);
    // Arm selection crossings through the sweep (real deltas under the
    // stamp — the accepted v1 per-commit limit: they jump too).
    for id in [7u64, 150, 280] {
        h.host.runtime().keyed_state::<bool>(id, || false).set(true);
    }
    // Let the (correct, unstamped) selection interpolation settle past
    // its 120 ms before the sweep starts — the sweep probes stamped
    // recycling, not a coincident real change still mid-flight.
    // (No bare run_until_idle here: the live interpolation on a frozen
    // clock would spin it forever — pump advances time first.)
    pump(&h, 1.0);
    assert_eq!(
        h.host.with_evaluator(|e| e.active_count()),
        0,
        "selection interpolation settled before the sweep"
    );
    let created0 = h.host.with_evaluator(|e| e.created());
    let mut flashes = 0usize;
    let mut ticks = 0usize;
    for step in 0..(300 - h.k) {
        h.offset.set(step as f32 * ROW_H);
        pump(&h, TICK);
        ticks += 1;
        let now = h.host.runtime().now_secs();
        // Phantom-flash probe: every slot's evaluated bg equals its
        // retained target at every tick (values jump, nothing lingers).
        for cell in find_retained_by_debug(&h.host, "cell") {
            let painted = evaluated_cell_bg(&h, cell, now);
            let target = cell_bg_of(&h, cell);
            if painted != target {
                flashes += 1;
            }
        }
    }
    assert_eq!(ticks, 300 - h.k, "full sweep ran");
    assert_eq!(flashes, 0, "phantom-flash count is 0 across the sweep");
    assert_eq!(
        h.host.with_evaluator(|e| e.created() - created0),
        0,
        "zero interpolators on binding-edge commits"
    );
    // Selection crossings snapped instantly despite the stamp (jump =
    // applied, not dropped).
    assert!(selected_of(&h.host, ContactId(150)));
    h.offset.set(145.0 * ROW_H);
    pump(&h, TICK);
    let now = h.host.runtime().now_secs();
    let row150 = cell_at_row(&h, 150);
    assert_eq!(
        evaluated_cell_bg(&h, row150, now),
        Some(SELECTION_BG),
        "coincident selection jumps under the stamp (v1 limit, stated)"
    );
}

#[test]
fn untracked_rows_without_transition_snap_without_suppression() {
    // Counter-discriminator: no `transition` field → snaps that count
    // neither as created nor as suppressed.
    #[derive(Clone)]
    struct PlainProps {
        bg: oppa::Color,
    }
    impl Props for PlainProps {}
    fn render_plain(_ctx: &Ctx, props: &PlainProps) -> VNode {
        oppa::Div("plain")
            .style(Style::new().size(10, 10).bg(props.bg))
            .build()
    }
    let host = ComponentHost::new();
    let handle = host.mount("Plain", PlainProps { bg: oppa::Color(1) }, render_plain);
    host.run_until_idle();
    handle.set_props(PlainProps { bg: oppa::Color(2) });
    host.run_until_idle();
    host.with_evaluator(|e| {
        assert_eq!(e.created(), 0);
        assert_eq!(e.suppressed(), 0, "no stamp, no transition: plain snap");
    });
}
