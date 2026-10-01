//! M8 acceptance (CPU arm): the virtualized sweep image-diffed
//! frame-by-frame through the M4 oracle, plus the measured repaint bound
//! and the overscan experiment (DESIGN-sketch `+2` vs shipped `+4`).
//!
//! Same slot/binding/offset machinery as `oppa`'s `m8_virtualization`
//! (per-crate rig duplication is the workspace's established pattern —
//! `oppa` cannot depend on `oppa-cpu`), extended with `FakeText` layout
//! so text payloads churn through real pixels, and evaluated builds so
//! the TIME evaluator's values are what the oracle compares.
//!
//! What this proves (acceptance list):
//!
//! - Sweep ticks: zero structure ops per tick at scale (TreeDiff count).
//! - Repaint bounded per tick (builder stats + oracle history clean).
//! - Phantom-flash stress: 120 ms bg transitions under the sweep,
//!   stamped — flash count 0, interpolator count 0, oracle 0 every tick.
//! - Overscan measurement at +2 and +4 (decision 119's input).

#![allow(non_snake_case)]

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use oppa::{
    find_retained_by_debug, BackendError, ComponentHost, Ctx, Ease, Memo, MsExt, Props,
    ScrollOffset, Signal, Store, Style, SurfaceDesc, Text, Transition, VNode,
};
use oppa_cpu::{FramePlanBuilder, OracleSession};

// ---------------------------------------------------------------------------
// Fakes (M7 rig shape: uniform advance, em-scaled like a real backend)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FakeText;

impl oppa::TextService for FakeText {
    fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
        Vec::new()
    }

    fn shape(
        &self,
        text: &str,
        style: &oppa::TextStyle,
    ) -> Result<oppa::ShapedRun, oppa::TextError> {
        if text.is_empty() {
            return Err(oppa::TextError::EmptyText);
        }
        let em = style.font_size_px * style.device_pixel_ratio;
        let adv = em * 0.625;
        let metrics = oppa::FontMetrics {
            ascent: em * 0.75,
            descent: em * 0.25,
            line_gap: em * 0.125,
        };
        let mut glyphs = Vec::new();
        let mut clusters = Vec::new();
        for (k, (i, ch)) in text.char_indices().enumerate() {
            let len = ch.len_utf8();
            glyphs.push(oppa::ShapedGlyph {
                glyph_id: k as u32,
                x_advance: adv,
                x_offset: 0.0,
                y_offset: 0.0,
            });
            clusters.push(oppa::Cluster {
                byte_range: (i, i + len),
                glyph_range: (k, k + 1),
            });
        }
        Ok(oppa::ShapedRun {
            glyphs,
            runs: vec![oppa::TextRun {
                byte_range: (0, text.len()),
                glyph_range: (0, clusters.len()),
                rtl: false,
                script: 0,
                font_id: oppa::FontId(0),
                font_metrics: metrics,
            }],
            clusters,
            total_advance: adv * text.chars().count() as f32,
            text_len_bytes: text.len(),
        })
    }
}

// ---------------------------------------------------------------------------
// Contacts + harness (same machinery as the core M8 suite)
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
const VW: f32 = 800.0;

const BASE_EVEN: oppa::Color = oppa::Color(0x22_22_22);
const BASE_ODD: oppa::Color = oppa::Color(0x33_33_33);
const SELECTION_BG: oppa::Color = oppa::Color(0x88_88_88);
const SURFACE_BG: oppa::Color = oppa::Color(0xFF_FF_FF);

fn window_first(offset_px: f32, row_h: f32, over: usize, n_items: usize, n_slots: usize) -> usize {
    let first = (offset_px / row_h).floor().max(0.0) as usize;
    let first = first.saturating_sub(over);
    first.min(n_items.saturating_sub(n_slots))
}

fn slot_count(viewport_h: f32, row_h: f32, over: usize) -> usize {
    // Visible rows + 1 straddle margin + overscan both sides (M8
    // decision 127 — see the core suite for the caught margin).
    (viewport_h / row_h).ceil() as usize + 1 + 2 * over
}

fn cell_bg(item: Option<ContactId>, selected: bool) -> Option<oppa::Color> {
    if selected {
        return Some(SELECTION_BG);
    }
    let id = item?;
    Some(if id.0 % 2 == 0 { BASE_EVEN } else { BASE_ODD })
}

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
    let sel_flag: Signal<bool> =
        ctx.keyed_state(item_id.map(|id| id.0).unwrap_or(u64::MAX), || false);
    let bg = cell_bg(item_id, sel_flag.get());
    oppa::Row("cell")
        .style(
            Style::new()
                .h(props.row_h)
                .fill_width()
                .pad_x(12)
                .bg(bg)
                .transition(Transition::new(120.ms(), Ease::Out)),
        )
        .semantics(
            oppa::Semantics::list_item()
                .selected(sel_flag.get())
                .label(&contact.read().display_name),
        )
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

struct Harness {
    host: ComponentHost,
    offset: ScrollOffset,
    k: usize,
    n: usize,
    over: usize,
    clock: Rc<oppa::MockClock>,
    builder: FramePlanBuilder,
    oracle: OracleSession,
    seen: usize,
}

fn mount_harness(n_items: usize, over: usize) -> Harness {
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(VW, VIEWPORT_H);
    host.set_text_service(Box::new(FakeText));
    let rt = host.runtime();
    let (ids, values) = seed_contacts(n_items);
    let store = Store::new(&rt, ids, values);
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
    let oracle = OracleSession::new(SurfaceDesc {
        width_px: VW as u32,
        height_px: VIEWPORT_H as u32,
        background: SURFACE_BG,
    })
    .expect("oracle surfaces");
    let seen = host.diff_count();
    Harness {
        host,
        offset,
        k,
        n: n_items,
        over,
        clock,
        builder: FramePlanBuilder::new(1.0),
        oracle,
        seen,
    }
}

const TICK: f64 = 1.0 / 60.0;

fn pump(h: &Harness, dt: f64) {
    run_bounded(h);
    h.clock.advance(dt);
    run_bounded(h);
    assert!(!h.host.runtime().has_demand(), "pump({dt}s) must settle");
}

fn run_bounded(h: &Harness) {
    for _ in 0..100 {
        if !h.host.run_once() {
            break;
        }
    }
}

/// One sweep tick through both oracle arms at the same `now`.
/// Returns (structure_ops, ops_emitted, damage_rects, cells_repainted,
/// oracle_diff, flashes).
#[allow(clippy::too_many_arguments)]
fn sweep_tick(h: &mut Harness) -> Result<(usize, usize, usize, usize, usize, usize), BackendError> {
    let now = h.clock.get();
    let diffs: Vec<oppa::TreeDiff> = h.host.diffs_from(h.seen);
    let structure: usize = diffs.iter().map(|d| d.structure_ops()).sum();
    h.oracle.commit_all(&diffs)?;
    h.seen = h.host.diff_count();
    let (incr, full) = h.host.with_retained_mut(|rec, styles| {
        h.host.with_evaluator(|ev| {
            (
                h.builder.build_incremental_evaluated(rec, styles, ev, now),
                h.builder.build_full_evaluated(rec, styles, ev, now),
            )
        })
    });
    let cells: HashSet<oppa::NodeId> = find_retained_by_debug(&h.host, "cell")
        .into_iter()
        .collect();
    let repainted: HashSet<oppa::NodeId> = incr
        .ops
        .iter()
        .filter_map(|op| op.node())
        .filter(|id| cells.contains(id))
        .collect();
    // Phantom-flash probe at the evaluated level: every repainted
    // cell's painted bg equals its retained target.
    let mut flashes = 0usize;
    h.host.with_retained_mut(|rec, styles| {
        h.host.with_evaluator(|ev| {
            for id in &repainted {
                let style_bg = rec
                    .get(*id)
                    .and_then(|n| styles.get(n.style))
                    .and_then(|s| s.bg);
                if ev.resolve_bg(*id, style_bg, now) != style_bg {
                    flashes += 1;
                }
            }
        });
    });
    let d = h.oracle.assert_paints(&incr, &full)?;
    Ok((
        structure,
        incr.stats.ops_emitted,
        incr.damage.len(),
        repainted.len(),
        d,
        flashes,
    ))
}

// ---------------------------------------------------------------------------
// The sweep: frame-by-frame oracle + repaint bound (over = 4, N = 300)
// ---------------------------------------------------------------------------

#[test]
fn sweep_oracle_clean_and_repaint_bounded_per_tick() {
    let mut h = mount_harness(300, 4);
    assert_eq!(h.k, 20, "11 visible + 1 straddle + 8 overscan");
    // Arm selection crossings (real deltas under the stamp).
    for id in [7u64, 150, 280] {
        h.host.runtime().keyed_state::<bool>(id, || false).set(true);
    }
    pump(&h, 1.0);
    // The selection arm's (correct, unstamped) interpolations are the
    // baseline — the sweep must add zero.
    let created0 = h.host.with_evaluator(|e| e.created());

    let mut max_ops = 0usize;
    let mut max_damage = 0usize;
    let mut max_cells = 0usize;
    let mut moving = 0usize;
    let mut prev_first = window_first(0.0, ROW_H, h.over, h.n, h.k);
    for step in 1..=(h.n - h.k) {
        h.offset.set(step as f32 * ROW_H);
        pump(&h, TICK);
        let first = window_first(step as f32 * ROW_H, ROW_H, h.over, h.n, h.k);
        let (structure, ops, damage, cells, diff, flashes) =
            sweep_tick(&mut h).expect("sweep tick");
        assert_eq!(diff, 0, "step {step}: incremental == full repaint");
        assert_eq!(flashes, 0, "step {step}: phantom-flash count 0");
        if first != prev_first {
            assert_eq!(structure, 0, "step {step}: zero structure ops");
            moving += 1;
            max_ops = max_ops.max(ops);
            max_damage = max_damage.max(damage);
            max_cells = max_cells.max(cells);
            // The §4.2 unit is CELLS, not ops: ~30-cell bound, measured.
            assert!(
                cells <= 30,
                "step {step}: repainted cells {cells} exceed the ~30-cell bound"
            );
        }
        prev_first = first;
    }
    assert_eq!(moving, h.n - h.k - h.over, "every window move swept");
    eprintln!(
        "M8 CPU sweep over=4: max_ops={max_ops} max_damage={max_damage} max_cells={max_cells}"
    );
    // Measured bound, pinned (see ROUNDS.md M8 entry for the derivation):
    // per-slot ~3 ops (bg + 2 text lines) + ScrollArea clip pair.
    assert!(
        max_ops <= 4 * h.k + 8,
        "repaint ops bound: {max_ops} (K={})",
        h.k
    );
    assert!(
        max_damage <= 6 * h.k + 8,
        "damage bound (per-slot 6 nodes: slot + cell + 2 text elements + \
         2 text leaves): {max_damage} (K={})",
        h.k
    );
    assert_eq!(
        h.host.with_evaluator(|e| e.created() - created0),
        0,
        "zero interpolators across the pixel sweep"
    );
    assert!(
        h.host.with_evaluator(|e| e.suppressed()) > 0,
        "non-vacuous: stamps honored"
    );
    let _ = Cell::new(());
}

// ---------------------------------------------------------------------------
// Overscan experiment: the same sweep at +2 and +4
// ---------------------------------------------------------------------------

#[test]
fn overscan_two_vs_four_measured() {
    // Narrower, faster experiment (N = 120): the question is the
    // per-tick bound at each constant, not full-list coverage (proven
    // above at N = 300).
    for over in [2usize, 4usize] {
        let mut h = mount_harness(120, over);
        let mut max_ops = 0usize;
        let mut max_cells = 0usize;
        let mut prev_first = window_first(0.0, ROW_H, over, h.n, h.k);
        let mut moving = 0usize;
        for step in 1..=(h.n - h.k) {
            h.offset.set(step as f32 * ROW_H);
            pump(&h, TICK);
            let first = window_first(step as f32 * ROW_H, ROW_H, over, h.n, h.k);
            let (structure, ops, _, cells, diff, flashes) = sweep_tick(&mut h).expect("sweep tick");
            assert_eq!(diff, 0, "over={over} step {step}: oracle clean");
            assert_eq!(flashes, 0, "over={over} step {step}: no flashes");
            if first != prev_first {
                assert_eq!(structure, 0, "over={over} step {step}: zero structure");
                moving += 1;
                max_ops = max_ops.max(ops);
                max_cells = max_cells.max(cells);
            }
            prev_first = first;
        }
        eprintln!(
            "M8 overscan over={over}: K={} moving={moving} max_ops={max_ops} max_cells={max_cells}",
            h.k
        );
        assert_eq!(
            h.host.with_evaluator(|e| e.created()),
            0,
            "over={over}: zero interpolators"
        );
    }
}
