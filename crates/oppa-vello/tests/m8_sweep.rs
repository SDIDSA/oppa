//! M8 acceptance (Vello arm): the virtualized sweep on the GPU presenter.
//!
//! Headless tests (plan discipline, encode stats, evaluated colors)
//! run everywhere; pixel rows need a real GPU and are `#[cfg(windows)]`
//! (the M6 gating rule — loud hardware requirement, never a silent
//! software fallback).
//!
//! Geometry-only rows (bg rects + 120 ms transitions, no text leaves):
//! text-as-cells vs outlines differs by design (M6 F3), so the
//! cross-rasterizer EXACT compare — the phantom-flash signal — runs
//! where exactness holds (M6 strict-geometry 0/0 precedent). Text churn
//! through the same slot machinery is covered by the CPU sweep; the
//! evaluated-plan colors here prove the stamp path end-to-end on Vello.

#![allow(non_snake_case)]

use std::collections::HashMap;
use std::rc::Rc;

use oppa::{
    find_retained_by_debug, ComponentHost, Ctx, Ease, Memo, MsExt, Props, ScrollOffset, Signal,
    Store, Style, Transition, VNode,
};
use oppa_cpu::FramePlanBuilder;
use oppa_vello::VelloBackend;

// ---------------------------------------------------------------------------
// Harness (same slot/binding machinery, geometry-only rows)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ContactId(u64);

const ROW_H: f32 = 56.0;
const VIEWPORT_H: f32 = 600.0;
const VW: f32 = 800.0;
const OVER: usize = 4;

const BASE_EVEN: oppa::Color = oppa::Color(0x22_22_22);
const BASE_ODD: oppa::Color = oppa::Color(0x33_33_33);
const SELECTION_BG: oppa::Color = oppa::Color(0x88_88_88);
const SURFACE_BG: oppa::Color = oppa::Color(0xFF_FF_FF);

fn window_first(offset_px: f32, row_h: f32, over: usize, n_items: usize, n_slots: usize) -> usize {
    let first = (offset_px / row_h).floor().max(0.0) as usize;
    first
        .saturating_sub(over)
        .min(n_items.saturating_sub(n_slots))
}

fn slot_count(viewport_h: f32, row_h: f32, over: usize) -> usize {
    // Visible + 1 straddle + overscan both sides (M8 decision 127).
    (viewport_h / row_h).ceil() as usize + 1 + 2 * over
}

#[derive(Clone)]
struct ListProps {
    store: Store<ContactId, String>,
    over: usize,
}

impl Props for ListProps {}

#[derive(Clone)]
struct RowProps {
    item: Memo<Option<ContactId>>,
    row_h: f32,
}

impl Props for RowProps {}

fn seed_store(rt: &oppa::Runtime, n: usize) -> Store<ContactId, String> {
    let mut ids = Vec::with_capacity(n);
    let mut values = HashMap::new();
    for i in 0..n {
        let id = ContactId(i as u64);
        ids.push(id);
        values.insert(id, format!("Contact {i:03}"));
    }
    Store::new(rt, ids, values)
}

fn ContactList(ctx: &Ctx, props: &ListProps) -> VNode {
    let offset = ctx.scroll_offset();
    let n = props.store.len();
    let k = slot_count(VIEWPORT_H, ROW_H, props.over);
    let over = props.over;
    let first = window_first(offset.get(), ROW_H, over, n, k);
    let items: Vec<Memo<Option<ContactId>>> = (0..k)
        .map(|slot| {
            let store = props.store.clone();
            let offset = offset.clone();
            ctx.binding(move || {
                let f = window_first(offset.get(), ROW_H, over, n, k);
                store.get(f + slot)
            })
        })
        .collect();
    oppa::ScrollArea("list")
        .content_size(n as f32 * ROW_H)
        .style(Style::new().h(VIEWPORT_H).fill_width())
        .on_scroll(|| {})
        .children(items.into_iter().enumerate().map(|(slot, item)| {
            let row = first + slot;
            let row_props = RowProps { item, row_h: ROW_H };
            oppa::Row("slot")
                .style(
                    Style::new()
                        .absolute_y(row as f32 * ROW_H)
                        .h(ROW_H)
                        .fill_width(),
                )
                .key(slot as u64)
                .child(ctx.child("ContactRow", slot as u64, &row_props, ContactRow))
        }))
}

fn ContactRow(ctx: &Ctx, props: &RowProps) -> VNode {
    let item_id = props.item.read();
    let sel_flag: Signal<bool> =
        ctx.keyed_state(item_id.map(|id| id.0).unwrap_or(u64::MAX), || false);
    let bg = if sel_flag.get() {
        Some(SELECTION_BG)
    } else {
        item_id.map(|id| if id.0 % 2 == 0 { BASE_EVEN } else { BASE_ODD })
    };
    // Geometry-only (no Text leaves — headless Vello refuses text with
    // no font bytes; the flash signal is the bg, which is exact across
    // rasterizers by the M6 strict-geometry precedent).
    oppa::Row("cell")
        .style(
            Style::new()
                .h(props.row_h)
                .fill_width()
                .bg(bg)
                .transition(Transition::new(120.ms(), Ease::Out)),
        )
        .build()
}

struct Harness {
    host: ComponentHost,
    offset: ScrollOffset,
    k: usize,
    n: usize,
    clock: Rc<oppa::MockClock>,
    builder: FramePlanBuilder,
    backend: VelloBackend,
    vello_surface: oppa::SurfaceId,
    seen: usize,
}

fn mount_harness(n_items: usize) -> Harness {
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(VW, VIEWPORT_H);
    let store = seed_store(&host.runtime(), n_items);
    host.set_keyed_capacity(n_items.max(64) + 64);
    let handle = host.mount("ContactList", ListProps { store, over: OVER }, ContactList);
    host.run_until_idle();
    let offset = host
        .instance_scroll(handle.root_instance())
        .expect("scroll");
    let list = find_retained_by_debug(&host, "list")[0];
    host.bind_scroll(list, offset.clone());
    let k = slot_count(VIEWPORT_H, ROW_H, OVER);
    let mut backend = VelloBackend::new();
    use oppa::RendererBackend;
    let vello_surface = backend
        .create_surface(oppa::SurfaceDesc {
            width_px: VW as u32,
            height_px: VIEWPORT_H as u32,
            background: SURFACE_BG,
        })
        .expect("surface");
    // The registry tracks from the mount commit (ticks only carry
    // Updates — without the mount Adds the registry would undercount
    // while paints still replay correctly).
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("mount commit");
    }
    let seen = host.diff_count();
    Harness {
        host,
        offset,
        k,
        n: n_items,
        clock,
        builder: FramePlanBuilder::new(1.0),
        backend,
        vello_surface,
        seen,
    }
}

const TICK: f64 = 1.0 / 60.0;

fn pump(h: &Harness, dt: f64) {
    for _ in 0..100 {
        if !h.host.run_once() {
            break;
        }
    }
    h.clock.advance(dt);
    for _ in 0..100 {
        if !h.host.run_once() {
            break;
        }
    }
    assert!(!h.host.runtime().has_demand(), "pump must settle");
}

/// One headless sweep tick: commit, evaluated plan, encode. Returns
/// (structure_ops, ops_emitted, staged_work_delta, flashes).
fn sweep_tick(h: &mut Harness) -> (usize, usize, u64, usize) {
    use oppa::RendererBackend;
    let now = h.clock.get();
    let diffs: Vec<oppa::TreeDiff> = h.host.diffs_from(h.seen);
    let structure: usize = diffs.iter().map(|d| d.structure_ops()).sum();
    for d in &diffs {
        h.backend.commit(d).expect("vello commit");
    }
    h.seen = h.host.diff_count();
    let plan = h.host.with_retained_mut(|rec, styles| {
        h.host
            .with_evaluator(|ev| h.builder.build_incremental_evaluated(rec, styles, ev, now))
    });
    // Plan-level flash probe: every bg op paints its retained target
    // (stamped values jump — no mid-flight colors on a stamped sweep).
    let mut flashes = 0usize;
    h.host.with_retained_mut(|rec, styles| {
        h.host.with_evaluator(|ev| {
            for op in &plan.ops {
                let (Some(id), Some(style)) = (
                    op.node(),
                    op.node()
                        .and_then(|n| rec.get(n))
                        .and_then(|n| styles.get(n.style)),
                ) else {
                    continue;
                };
                let painted = match op {
                    oppa::DrawOp::Rect { color, .. } | oppa::DrawOp::RRect { color, .. } => {
                        Some(*color)
                    }
                    _ => None,
                };
                if let Some(c) = painted {
                    if ev.resolve_bg(id, style.bg, now) != style.bg || Some(c) != style.bg {
                        flashes += 1;
                    }
                }
            }
        });
    });
    h.backend
        .paint(h.vello_surface, &plan)
        .expect("vello paint");
    let work = h.backend.take_gpu_work();
    (structure, plan.stats.ops_emitted, work, flashes)
}

// ---------------------------------------------------------------------------
// Headless sweep (all platforms): plan discipline + encode, no pixels
// ---------------------------------------------------------------------------

#[test]
fn vello_sweep_plans_encode_with_zero_structure() {
    let mut h = mount_harness(150);
    assert_eq!(h.k, 20);
    for id in [7u64, 100, 140] {
        h.host.runtime().keyed_state::<bool>(id, || false).set(true);
    }
    pump(&h, 1.0);
    let created0 = h.host.with_evaluator(|e| e.created());

    let mut max_ops = 0usize;
    let mut max_work = 0u64;
    let mut moving = 0usize;
    let mut prev_first = window_first(0.0, ROW_H, OVER, h.n, h.k);
    for step in 1..=(h.n - h.k) {
        h.offset.set(step as f32 * ROW_H);
        pump(&h, TICK);
        let first = window_first(step as f32 * ROW_H, ROW_H, OVER, h.n, h.k);
        let (structure, ops, work, flashes) = sweep_tick(&mut h);
        assert_eq!(flashes, 0, "step {step}: plan colors == targets");
        if first != prev_first {
            assert_eq!(structure, 0, "step {step}: zero structure ops");
            moving += 1;
            max_ops = max_ops.max(ops);
            max_work = max_work.max(work);
            assert!(ops <= 3 * h.k + 8, "step {step}: encode ops bound ({ops})");
        }
        prev_first = first;
    }
    assert_eq!(moving, 150 - 20 - 4, "every window move swept");
    eprintln!("M8 Vello headless sweep: max_ops={max_ops} max_work={max_work}");
    assert_eq!(
        h.host.with_evaluator(|e| e.created() - created0),
        0,
        "zero interpolators on the Vello sweep"
    );
    assert_eq!(
        h.backend.live_node_count(),
        h.host.retained_count(),
        "backend registry tracks the retained tree"
    );
}

// ---------------------------------------------------------------------------
// Pixel rows (Windows + GPU): CPU-vs-Vello exact per tick, tol recorded
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod gpu {
    use super::*;
    use oppa::RendererBackend;
    use oppa_vello::oracle::{diff_count_exact, diff_count_tol, RgbaImage};

    #[test]
    fn vello_pixel_sweep_exact_per_tick() {
        // Hardware-oracle row: software-emulated adapters (WARP) prove no
        // real-GPU pixels and crash under parallel load - skip loudly.
        if let Err(e) = VelloBackend::probe_hardware_adapter() {
            eprintln!("SKIP vello_pixel_sweep_exact_per_tick: {e}");
            return;
        }
        let mut h = mount_harness(150);
        let desc = oppa::SurfaceDesc {
            width_px: VW as u32,
            height_px: VIEWPORT_H as u32,
            background: SURFACE_BG,
        };
        let mut cpu = oppa_cpu::CpuBackend::new();
        let cpu_surface = cpu.create_surface(desc).expect("cpu surface");
        for d in h.host.diffs_from(0) {
            cpu.commit(&d).expect("cpu mount commit");
        }
        let mut seen = h.seen;
        let mut max_exact = 0usize;
        let mut max_tol16 = 0usize;
        let mut moving = 0usize;
        let mut prev_first = window_first(0.0, ROW_H, OVER, h.n, h.k);
        for step in 1..=(h.n - h.k) {
            h.offset.set(step as f32 * ROW_H);
            pump(&h, TICK);
            let first = window_first(step as f32 * ROW_H, ROW_H, OVER, h.n, h.k);
            let now = h.clock.get();
            let diffs: Vec<oppa::TreeDiff> = h.host.diffs_from(seen);
            for d in &diffs {
                cpu.commit(d).expect("cpu commit");
                h.backend.commit(d).expect("vello commit");
            }
            seen = h.host.diff_count();
            let plan = h.host.with_retained_mut(|rec, styles| {
                h.host.with_evaluator(|ev| {
                    h.builder.build_incremental_evaluated(rec, styles, ev, now)
                })
            });
            cpu.paint(cpu_surface, &plan).expect("cpu paint");
            h.backend
                .paint(h.vello_surface, &plan)
                .expect("vello paint");
            let cpu_img = RgbaImage::from_cpu_pixmap(cpu.pixmap(cpu_surface).expect("cpu pixmap"))
                .expect("all pixels opaque");
            let vello_img = h
                .backend
                .render_pixels(h.vello_surface)
                .expect("gpu readback");
            let exact = diff_count_exact(&cpu_img, &vello_img);
            let tol16 = diff_count_tol(&cpu_img, &vello_img, 16);
            max_exact = max_exact.max(exact);
            max_tol16 = max_tol16.max(tol16);
            if first != prev_first {
                moving += 1;
                assert_eq!(
                    exact, 0,
                    "step {step}: strict-geometry pixels exact (bg flash would light up here)"
                );
            }
            prev_first = first;
        }
        assert_eq!(moving, 150 - 20 - 4, "every window move swept");
        eprintln!("M8 Vello pixel sweep: max_exact={max_exact} max_tol16={max_tol16}");
        assert_eq!(max_exact, 0, "CPU exact across the sweep");
        assert_eq!(max_tol16, 0, "tol-16 identical (rects have no AA ramp)");
    }
}
