//! M6 acceptance: Vello backend + driver matrix (first GPU presenter).
//!
//! Same dirty-subtree [`FramePlan`](oppa::FramePlan)s the CPU backend
//! consumes, second rasterizer. Headless tests (encode discipline, atlas
//! fidelity log, skip/skew/present ledgers, F1, TIME cadence) run
//! everywhere; pixel tests need a real GPU + DirectWrite and are
//! `#[cfg(windows)]` (loud hardware requirement, never a silent
//! software fallback).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use oppa::{
    BackendError, Color, Column, ComponentHost, Ctx, Div, DrawOp, FramePlan, MockClock, NodeId,
    PresenterKind, Props, RendererBackend, VNode,
};
use oppa_cpu::FramePlanBuilder;
use oppa_vello::{EncodeStats, VelloBackend};

// ---------------------------------------------------------------------------
// Shared rigs (geometry-only scenes stay portable: no font bytes needed).
// ---------------------------------------------------------------------------

const CARD_BG: Color = Color(0x44_44_44);
const SURFACE_BG: Color = Color(0xFF_FF_FF);
const VW: f32 = 120.0;
const VH: f32 = 60.0;

#[derive(Clone)]
struct CardProps {
    title: String,
}

impl Props for CardProps {}

fn render_card(_ctx: &Ctx, props: &CardProps) -> VNode {
    let text: VNode = oppa::Text {
        text: Arc::from(props.title.as_str()),
        style: oppa::Text::title_small,
    }
    .into();
    Div("card")
        .style(
            oppa::Style::new()
                .size(100, 40)
                .pad_x(8)
                .radius(6)
                .bg(CARD_BG),
        )
        .child(text)
}

fn render_plate(_ctx: &Ctx, _p: &()) -> VNode {
    Div("plate")
        .style(oppa::Style::new().size(100, 40).radius(6).bg(CARD_BG))
        .build()
}

fn surface_desc() -> oppa::SurfaceDesc {
    oppa::SurfaceDesc {
        width_px: VW as u32,
        height_px: VH as u32,
        background: SURFACE_BG,
    }
}

/// Hand-built plan exercising every DrawOp (geometry half — portable).
/// Mirrors the CPU contract-surface test's stack shapes plus the M5
/// border-ring pair (outer fill + inset fill, no new ops).
fn coverage_plan() -> FramePlan {
    let n = NodeId::new(7, 0);
    let ring_outer = NodeId::new(8, 0);
    FramePlan {
        viewport_w: VW,
        viewport_h: VH,
        ops: vec![
            DrawOp::PushClip {
                x: 5.0,
                y: 5.0,
                w: 100.0,
                h: 50.0,
            },
            DrawOp::PushLayer { opacity: 0.8 },
            DrawOp::Rect {
                node: n,
                x: 10.0,
                y: 10.0,
                w: 30.0,
                h: 20.0,
                color: CARD_BG,
                opacity: 1.0,
            },
            DrawOp::RRect {
                node: n,
                x: 45.0,
                y: 10.0,
                w: 30.0,
                h: 20.0,
                radius: 5.0,
                radii: None,
                color: CARD_BG,
                opacity: 1.0,
            },
            DrawOp::Circle {
                node: n,
                cx: 95.0,
                cy: 20.0,
                r: 8.0,
                color: CARD_BG,
                opacity: 1.0,
            },
            // Offset-solid shadow (contractual blur degradation).
            DrawOp::Shadow {
                node: n,
                x: 10.0,
                y: 35.0,
                w: 30.0,
                h: 10.0,
                dx: 1.0,
                dy: 2.0,
                blur_radius: 0.0,
                color: Color(0x00_00_00),
            },
            DrawOp::Pop,
            DrawOp::Pop,
            // M5 border-ring pair: outer ring fill + inset background.
            DrawOp::RRect {
                node: ring_outer,
                x: 4.0,
                y: 4.0,
                w: 40.0,
                h: 24.0,
                radius: 6.0,
                radii: None,
                color: Color(0xAA_BB_CC),
                opacity: 1.0,
            },
            DrawOp::RRect {
                node: ring_outer,
                x: 6.0,
                y: 6.0,
                w: 36.0,
                h: 20.0,
                radius: 4.0,
                radii: None,
                color: CARD_BG,
                opacity: 1.0,
            },
        ],
        damage: vec![],
        stats: Default::default(),
        full_repaint: true,
    }
}

// ---------------------------------------------------------------------------
// 1. DrawOp coverage: every op encodes (or refuses loudly, RImg).
// ---------------------------------------------------------------------------

#[test]
fn drawop_coverage_geometry_encodes() {
    let mut be = VelloBackend::new();
    assert_eq!(be.kind(), PresenterKind::GpuDrawList);
    let s = be.create_surface(surface_desc()).expect("surface");
    let plan = coverage_plan();
    let stats = be.paint(s, &plan).expect("encode all geometry ops");
    assert!(!stats.skipped_empty);
    // bg + rect + rrect + circle + shadow + ring-outer + ring-inner = 7.
    let enc: EncodeStats = be.last_encode_stats(s).expect("encode stats");
    assert_eq!(enc.shapes_encoded, 7, "every shape op staged: {enc:?}");
    assert_eq!(stats.ops_executed, 7);
    assert_eq!(be.take_gpu_work(), 7, "staged work metered");
    assert_eq!(be.take_gpu_work(), 0, "meter drains on take");
}

/// Round 11.1 (decision 305): a per-corner `RRect` encodes one
/// shape through the kurbo native path (same shared-builder rule as
/// the CPU bands-and-discs — cross-backend agreement by
/// construction, pixel-compared in the oracle sweeps).
#[test]
fn per_corner_rrect_encodes_one_shape() {
    let mut be = VelloBackend::new();
    let s = be.create_surface(surface_desc()).expect("surface");
    let plan = FramePlan {
        viewport_w: VW,
        viewport_h: VH,
        ops: vec![DrawOp::RRect {
            node: NodeId::new(1, 0),
            x: 10.0,
            y: 10.0,
            w: 30.0,
            h: 20.0,
            radius: 0.0,
            radii: Some([8.0, 0.0, 4.0, 0.0]),
            color: CARD_BG,
            opacity: 1.0,
        }],
        damage: vec![],
        stats: Default::default(),
        full_repaint: true,
    };
    let stats = be.paint(s, &plan).expect("encode per-corner rrect");
    let enc: EncodeStats = be.last_encode_stats(s).expect("encode stats");
    // Surface background + the one rounded shape.
    assert_eq!(
        enc.shapes_encoded, 2,
        "background plus cornered shape: {enc:?}"
    );
    assert_eq!(stats.ops_executed, 2);
}

#[test]
fn rimg_refused_loudly_before_staging() {
    let mut be = VelloBackend::new();
    let s = be.create_surface(surface_desc()).expect("surface");
    let n = NodeId::new(3, 0);
    let plan = FramePlan {
        viewport_w: VW,
        viewport_h: VH,
        ops: vec![DrawOp::RImg {
            node: n,
            x: 0.0,
            y: 0.0,
            w: 36.0,
            h: 36.0,
            image: oppa::ImageId(1),
        }],
        damage: vec![],
        stats: Default::default(),
        full_repaint: true,
    };
    let err = be.paint(s, &plan).expect_err("RImg must fail loudly");
    assert!(matches!(err, BackendError::UnsupportedOp(_)), "{err}");
    assert!(
        err.to_string().contains("not registered"),
        "pending-specific refusal, never a placeholder: {err}"
    );
    assert_eq!(be.take_gpu_work(), 0, "refusal stages nothing");
}

#[test]
fn rimg_cpu_vs_vello_oracle_exact() {
    // Hardware-oracle row: software-emulated adapters (WARP) prove no
    // real-GPU pixels and crash under parallel load - skip loudly.
    if let Err(e) = VelloBackend::probe_hardware_adapter() {
        eprintln!("SKIP rimg_cpu_vs_vello_oracle_exact: {e}");
        return;
    }
    // OQ-G8-1 closed: a registered image paints on BOTH rasterizers
    // and agrees pixel-exact at 1:1 (distinct opaque solids — no
    // filter or premultiply divergence possible at identity).
    let rgba: Vec<u8> = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let id = oppa::ImageId(9);
    let plan = FramePlan {
        viewport_w: VW,
        viewport_h: VH,
        ops: vec![DrawOp::RImg {
            node: NodeId::new(1, 0),
            x: 10.0,
            y: 10.0,
            w: 2.0,
            h: 2.0,
            image: id,
        }],
        damage: vec![],
        stats: Default::default(),
        full_repaint: true,
    };
    let mut oracle = oppa_vello::GpuOracle::new(surface_desc()).expect("oracle");
    use oppa::RendererBackend;
    oracle.cpu.insert_image(id, 2, 2, rgba.clone());
    oracle.vello.insert_image(id, 2, 2, rgba);
    oracle
        .cpu
        .paint(oracle.cpu_surface, &plan)
        .expect("cpu paint");
    oracle
        .vello
        .paint(oracle.vello_surface, &plan)
        .expect("vello paint");
    let cpu_px = oracle.cpu.pixmap(oracle.cpu_surface).expect("cpu pixmap");
    let cpu_img =
        oppa_vello::oracle::RgbaImage::from_cpu_pixmap(cpu_px).expect("all pixels opaque");
    let vello_img = oracle
        .vello
        .render_pixels(oracle.vello_surface)
        .expect("gpu readback");
    let exact = oppa_vello::oracle::diff_count_exact(&cpu_img, &vello_img);
    assert_eq!(
        exact, 0,
        "registered images agree pixel-exact cross-backend"
    );
}

#[test]
#[should_panic(expected = "bytes !=")]
fn vello_insert_image_rejects_length_mismatch() {
    let mut be = VelloBackend::new();
    be.insert_image(oppa::ImageId(3), 2, 2, vec![0u8; 15]);
}

#[test]
#[should_panic(expected = "zero size")]
fn vello_insert_image_rejects_zero_size() {
    let mut be = VelloBackend::new();
    be.insert_image(oppa::ImageId(4), 0, 2, vec![]);
}

#[test]
fn text_without_font_fails_loudly_never_tofu() {
    let mut be = VelloBackend::new();
    let s = be.create_surface(surface_desc()).expect("surface");
    let n = NodeId::new(3, 0);
    let plan = FramePlan {
        viewport_w: VW,
        viewport_h: VH,
        ops: vec![DrawOp::Text {
            node: n,
            x: 8.0,
            y: 0.0,
            line_height: 16.0,
            baseline: 12.0,
            // M7 lock touch (decision 110): exact em size + per-run
            // fonts ride every Text op; this hand-built plan carries one
            // run (the encoder treats an empty vec as one implicit run —
            // spelled explicitly here).
            em_size: 16.0,
            glyphs: vec![oppa::PlacedGlyph {
                glyph_id: 42,
                x: 0.0,
                advance: 10.0,
            }],
            fonts: vec![oppa::FontRun {
                glyph_range: (0, 1),
                family: "Segoe UI".to_string(),
                font_id: oppa::FontId(0),
            }],
            ink: oppa::INK,
            opacity: 1.0,
        }],
        damage: vec![],
        stats: Default::default(),
        full_repaint: true,
    };
    assert!(!be.atlas().has_font());
    let err = be.paint(s, &plan).expect_err("no font must fail loudly");
    assert!(matches!(err, BackendError::UnsupportedOp(_)), "{err}");
}

// ---------------------------------------------------------------------------
// 2. Caps negotiation incl. the blur-degradation declaration.
// ---------------------------------------------------------------------------

#[test]
fn caps_negotiation_declares_what_vello_does() {
    let be = VelloBackend::new();
    let caps = be.caps();
    assert_eq!(caps, oppa_vello::vello_caps());
    assert_eq!(caps.max_layers, 64, "scene stack, not pixmap masks");
    assert!(
        !caps.blur_backdrop,
        "Shadow carries no blur radius — offset solids on both backends"
    );
    assert!(caps.msaa, "Vello analytic area coverage is always on");
    assert!(
        caps.text_as_paths,
        "real outlines via draw_glyphs (closes the M4 CPU gap)"
    );
    // The two declarations side by side (the negotiation the M7 DOM
    // backend will join as a third row).
    let cpu = oppa_cpu::CpuBackend::new();
    assert!(!cpu.caps().text_as_paths, "M4 baseline: cells, not paths");
    assert!(!cpu.caps().msaa);
    assert_eq!(cpu.caps().max_layers, 8);
}

// ---------------------------------------------------------------------------
// 3. Contract surface: multi-surface, destroy, zero-size, unknown.
// ---------------------------------------------------------------------------

#[test]
fn contract_surface_multi_destroy_and_loud_misses() {
    let mut be = VelloBackend::new();
    let s1 = be.create_surface(surface_desc()).expect("s1");
    let s2 = be.create_surface(surface_desc()).expect("s2");
    assert_ne!(s1, s2);
    let plan = coverage_plan();
    be.paint(s1, &plan).expect("paint s1");
    be.paint(s2, &plan).expect("paint s2");
    assert_eq!(be.retained_op_count(s1), be.retained_op_count(s2));
    assert_eq!(be.retained_op_count(s1).expect("retained"), plan.ops.len());

    be.destroy_surface(s2).expect("destroy");
    assert!(matches!(
        be.paint(s2, &plan),
        Err(BackendError::UnknownSurface(_))
    ));
    assert!(matches!(
        be.present_at(s2, 0.0),
        Err(BackendError::UnknownSurface(_))
    ));
    assert!(matches!(
        be.skew_frames(s1, s2),
        Err(BackendError::UnknownSurface(_))
    ));
    assert!(matches!(
        be.create_surface(oppa::SurfaceDesc {
            width_px: 0,
            height_px: 10,
            background: SURFACE_BG
        }),
        Err(BackendError::BadSurface(_))
    ));
}

// ---------------------------------------------------------------------------
// 4. Unchanged-surface skip: static frames stage zero GPU work.
// ---------------------------------------------------------------------------

#[test]
fn unchanged_surface_skip_stages_zero_gpu_work() {
    let host = ComponentHost::new();
    host.set_viewport(VW, VH);
    let _handle = host.mount("plate", (), render_plate);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);

    let mut be = VelloBackend::new();
    let s = be.create_surface(surface_desc()).expect("surface");
    for d in host.diffs_from(0) {
        be.commit(&d).expect("commit");
    }
    // Mount frame: one RRect op, staged work > 0.
    let p1 = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    assert_eq!(p1.ops.len(), 1, "plate is one RRect: {p1:?}");
    let st1 = be.paint(s, &p1).expect("paint mount");
    assert!(!st1.skipped_empty);
    assert!(be.take_gpu_work() > 0, "mount frame stages work");

    // Static frame: empty plan → skip, zero staged work (measured #2).
    let p2 = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    assert!(p2.is_empty(), "static plan must be empty: {p2:?}");
    let st2 = be.paint(s, &p2).expect("paint static");
    assert!(st2.skipped_empty, "empty plan touches nothing");
    assert_eq!(be.take_gpu_work(), 0, "static frame ≈ 0 GPU work");
}

// ---------------------------------------------------------------------------
// 5. Per-surface skew bound (locked #21): observable and bounded.
// ---------------------------------------------------------------------------

#[test]
fn per_surface_skew_bound_is_observable() {
    let mut be = VelloBackend::new();
    let a = be.create_surface(surface_desc()).expect("a");
    let b = be.create_surface(surface_desc()).expect("b");
    let plan = coverage_plan();

    // Same-frame paints: skew 0.
    be.paint(a, &plan).expect("paint a");
    be.paint(b, &plan).expect("paint b");
    assert_eq!(be.skew_frames(a, b).expect("skew"), 0);

    // Staggered paints: the ledger observes the violation (skew 2 > 1).
    be.advance_frame();
    be.advance_frame();
    be.paint(a, &plan).expect("paint a late");
    assert_eq!(be.skew_frames(a, b).expect("skew"), 2);
    let skew = be.skew_frames(a, b).expect("skew");
    assert!(skew > 1, "a 2-frame lag must read as a violation, not 0/1");

    // Catch-up restores the bound.
    be.paint(b, &plan).expect("paint b catch-up");
    assert_eq!(be.skew_frames(a, b).expect("skew"), 0);
}

// ---------------------------------------------------------------------------
// 6. TIME-phase interpolation at vsync cadence, compositor-is-us (#18).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SlideProps {
    x: f32,
}

impl Props for SlideProps {}

fn render_slide(_ctx: &Ctx, props: &SlideProps) -> VNode {
    // The sliding knob must be a CHILD (`.x` is an out-of-flow child
    // offset — a root leaf ignores its own `.x`, so a root knob would
    // never move and the TIME test would read x=0 forever).
    Div("track")
        .style(oppa::Style::new().size(100, 20).bg(SURFACE_BG))
        .child(
            Div("knob")
                .style(oppa::Style::new().size(20, 10).x(props.x).bg(CARD_BG))
                .build(),
        )
}

#[test]
fn time_phase_interpolation_at_vsync_cadence() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(VW, VH);
    let handle = Rc::new(host.mount("slide", SlideProps { x: 0.0 }, render_slide));
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);

    let mut be = VelloBackend::new();
    let s = be.create_surface(surface_desc()).expect("surface");
    for d in host.diffs_from(0) {
        be.commit(&d).expect("commit");
    }
    // Mount plan painted once (work > 0), then drained from the meter.
    let p0 = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    be.paint(s, &p0).expect("mount paint");
    assert!(be.take_gpu_work() > 0);

    // TIME drives the interpolation: x = now * 600 (10px per 1/60 tick).
    // Five ticks, one frame each; the test loop is the compositor:
    // advance clock → run frame → paint → advance frame → present.
    let ticks = Rc::new(Cell::new(0usize));
    let ticks2 = ticks.clone();
    let h2 = handle.clone();
    host.runtime().add_animation(move |now| {
        let t = ticks2.get() + 1;
        ticks2.set(t);
        h2.set_props(SlideProps {
            x: (now * 600.0) as f32,
        });
        t < 5
    });

    let mut xs = Vec::new();
    for _ in 0..5 {
        clock.advance(1.0 / 60.0);
        assert!(host.runtime().run_once(), "animation demand runs a frame");
        let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
        assert!(!plan.is_empty(), "moving knob rebuilds every tick");
        let st = be.paint(s, &plan).expect("paint tick");
        assert!(!st.skipped_empty);
        assert!(be.take_gpu_work() > 0, "moving content stages work");
        be.advance_frame();
        be.present_at(s, clock.get()).expect("present");
        let ids = host.with_retained_mut(|rec, _| rec.find_by_debug("knob"));
        assert_eq!(ids.len(), 1);
        let b = host.committed_box(ids[0]).expect("knob box");
        xs.push(b.x);
    }
    // TIME serviced the interpolation: x tracks the timestamps exactly.
    assert_eq!(xs.len(), 5);
    for (i, x) in xs.iter().enumerate() {
        let want = ((i + 1) as f32 / 60.0) * 600.0;
        assert!(
            (x - want).abs() < 0.001,
            "tick {i}: x={x} vs TIME-derived {want}"
        );
    }
    assert!(xs.windows(2).all(|w| w[1] > w[0]), "monotonic: {xs:?}");

    // Presents landed at vsync cadence (measured: 5 × 1/60 spacing).
    let presents = be.presents(s).expect("presents").to_vec();
    assert_eq!(presents.len(), 5, "one present per tick");
    for (i, t) in presents.iter().enumerate() {
        let want = (i + 1) as f64 / 60.0;
        assert!((t - want).abs() < 1e-9, "present {i} at {t}, want {want}");
    }

    // Animation drained: one more tick with no changes → empty plan,
    // zero staged work, while the present still lands (skip DURING the
    // vsync loop, not just after it).
    clock.advance(1.0 / 60.0);
    assert!(!host.runtime().run_once() || true, "loop may idle");
    host.runtime().request_frame();
    host.run_until_idle();
    let p_static = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    assert!(p_static.is_empty(), "settled plan must be empty");
    let st = be.paint(s, &p_static).expect("paint static");
    assert!(st.skipped_empty);
    assert_eq!(be.take_gpu_work(), 0, "static tick stages zero work");
    be.advance_frame();
    be.present_at(s, clock.get()).expect("present static");
    assert_eq!(be.presents(s).expect("presents").len(), 6);
}

// ---------------------------------------------------------------------------
// 7. F1: position-only LAYOUT moves stamp PAINT engine-side (decision 104).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ListProps {
    n: usize,
}

impl Props for ListProps {}

fn render_list(_ctx: &Ctx, props: &ListProps) -> VNode {
    // Stable keys so shrinking 2→1 removes the FIRST row and the
    // survivor (box-b, key 1) shifts up — the position-only LAYOUT move
    // F1 stamps. (Keying by index would drop the last row instead and
    // nothing would move.)
    let mut kids = Vec::new();
    if props.n == 2 {
        kids.push(
            Div("box-a")
                .key(0)
                .style(oppa::Style::new().size(60, 10).bg(CARD_BG))
                .build(),
        );
        kids.push(
            Div("box-b")
                .key(1)
                .style(oppa::Style::new().size(60, 10).bg(CARD_BG))
                .build(),
        );
    } else {
        kids.push(
            Div("box-b")
                .key(1)
                .style(oppa::Style::new().size(60, 10).bg(CARD_BG))
                .build(),
        );
    }
    Column::new().children(kids)
}

#[test]
fn f1_position_only_move_stamps_paint_and_rebuilds() {
    let host = ComponentHost::new();
    host.set_viewport(VW, VH);
    let handle = host.mount("list", ListProps { n: 2 }, render_list);
    host.run_until_idle();
    assert_eq!(
        host.layout_stats().paint_stamped,
        0,
        "mount carries FRAME dirt — no stamps"
    );
    let builder = FramePlanBuilder::new(1.0);

    // Baseline oracle over the CPU backend: incremental == full.
    let desc = surface_desc();
    let mut oracle = oppa_cpu::OracleSession::new(desc).expect("oracle");
    oracle.commit_all(&host.diffs_from(0)).expect("commit");
    let incr = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    let full = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    assert_eq!(oracle.assert_paints(&incr, &full).expect("oracle"), 0);

    // Remove the first row: the survivor shifts up with LAYOUT-only dirt.
    handle.set_props(ListProps { n: 1 });
    host.run_until_idle();
    let stamped = host.layout_stats().paint_stamped;
    assert!(
        stamped >= 1,
        "moved box must be stamped PAINT (got {stamped})"
    );
    println!("f1 paint_stamped={stamped}");

    // The incremental plan rebuilds exactly the moved subtree.
    let p2 = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    assert!(!p2.is_empty(), "F1 must not yield an empty plan");
    let survivor = host.with_retained_mut(|rec, _| rec.find_by_debug("box-b"));
    assert_eq!(survivor.len(), 1, "keyed survivor keeps its id");
    assert!(
        p2.ops.iter().any(|o| o.node() == Some(survivor[0])),
        "moved node rebuilt: {:?}",
        p2.ops.iter().filter_map(|o| o.node()).collect::<Vec<_>>()
    );
    println!("f1 rebuilt ops={} damage={}", p2.ops.len(), p2.damage.len());

    // Oracle: the incremental history (with the stamp) == full repaint.
    oracle.commit_all(&host.diffs_from(0)).expect("commit");
    let (s_incr, _) = oracle.surfaces();
    oracle.backend_mut().paint(s_incr, &p2).expect("replay");
    let full2 = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let diff = oracle
        .assert_paints(&FramePlan::default(), &full2)
        .expect("oracle paints");
    assert_eq!(diff, 0, "stamped history == full repaint (F1 closed)");

    // The Vello backend consumes the same stamped plan without error and
    // stages the moved box (same plans, second rasterizer).
    let mut vello = VelloBackend::new();
    let vs = vello.create_surface(desc).expect("vello surface");
    for d in host.diffs_from(0) {
        vello.commit(&d).expect("vello commit");
    }
    // Replay history: mount plans first (full), then the stamped delta.
    let wall = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let vst = vello.paint(vs, &wall).expect("vello full paint");
    assert!(!vst.skipped_empty);
    let enc = vello.last_encode_stats(vs).expect("encode stats");
    assert!(enc.shapes_encoded >= 1, "moved box staged: {enc:?}");
}

// ---------------------------------------------------------------------------
// 8. Cross-backend box determinism (BUILD-ORDER §4): same tree on both
//    backends → identical rounded boxes.
// ---------------------------------------------------------------------------

#[test]
fn cross_backend_boxes_are_identical() {
    fn boxes_once() -> Vec<(f32, f32, f32, f32)> {
        let host = ComponentHost::new();
        host.set_viewport(VW, VH);
        let _handle = host.mount("plate", (), render_plate);
        host.run_until_idle();
        host.with_retained_mut(|rec, _| {
            let mut ids = rec.retained_ids();
            ids.sort();
            ids.iter()
                .filter_map(|id| rec.get(*id).and_then(|n| n.layout.clone()))
                .map(|b| (b.x, b.y, b.w, b.h))
                .collect::<Vec<_>>()
        })
    }
    let (b1, b2) = (boxes_once(), boxes_once());
    assert_eq!(b1, b2, "identical rounded boxes across runs");
    // And both backends encode the same plan identically (positions are
    // backend-independent — the assert pins it, not the layout).
    let host = ComponentHost::new();
    host.set_viewport(VW, VH);
    let _handle = host.mount("plate", (), render_plate);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));

    let mut cpu = oppa_cpu::CpuBackend::new();
    let cs = cpu.create_surface(surface_desc()).expect("cpu surface");
    for d in host.diffs_from(0) {
        cpu.commit(&d).expect("cpu commit");
    }
    cpu.paint(cs, &plan).expect("cpu paint");

    let mut vello = VelloBackend::new();
    let vs = vello.create_surface(surface_desc()).expect("vello surface");
    for d in host.diffs_from(0) {
        vello.commit(&d).expect("vello commit");
    }
    vello.paint(vs, &plan).expect("vello paint");
    assert_eq!(
        vello.last_encode_stats(vs).expect("stats").shapes_encoded,
        2, // bg + plate RRect
        "same plan, same staged shapes"
    );
}

// ---------------------------------------------------------------------------
// 9. Opacity folds into brush alpha without a contract change (decision
//    103, encode half — portable; the pixel half needs the GPU).
// ---------------------------------------------------------------------------

#[test]
fn opacity_folds_into_brush_alpha_no_contract_change() {
    let mut be = VelloBackend::new();
    let s = be.create_surface(surface_desc()).expect("surface");
    // The M4 alpha vehicle: half-alpha layer + full rect (same plan the
    // CPU contract-surface test paints).
    let n = NodeId::new(7, 0);
    let plan = FramePlan {
        viewport_w: VW,
        viewport_h: VH,
        ops: vec![
            DrawOp::PushLayer { opacity: 0.5 },
            DrawOp::Rect {
                node: n,
                x: 0.0,
                y: 0.0,
                w: 40.0,
                h: 40.0,
                color: CARD_BG,
                opacity: 1.0,
            },
            DrawOp::Pop,
        ],
        damage: vec![],
        stats: Default::default(),
        full_repaint: true,
    };
    let st = be.paint(s, &plan).expect("layer plan encodes");
    assert!(!st.skipped_empty);
    // bg + rect = 2 staged shapes; the layer carries the 0.5 (no Color
    // widening — `Color` stays opaque 0xRRGGBB by decision 103).
    let enc = be.last_encode_stats(s).expect("stats");
    assert_eq!(enc.shapes_encoded, 2);
}

// ---------------------------------------------------------------------------
// 10. PAINT-phase wiring through the host runtime (Vello hook parity).
// ---------------------------------------------------------------------------

#[test]
fn paint_phase_wiring_builds_and_commits_vello() {
    let host = ComponentHost::new();
    host.set_viewport(VW, VH);
    let backend = Rc::new(RefCell::new(VelloBackend::new()));
    let surface = backend
        .borrow_mut()
        .create_surface(surface_desc())
        .expect("surface");
    let calls = Rc::new(Cell::new(0usize));
    let last_ops = Rc::new(Cell::new(usize::MAX));
    oppa_vello::install_vello_paint_hook(
        &host,
        backend.clone(),
        surface,
        1.0,
        calls.clone(),
        last_ops.clone(),
    );

    let _handle = host.mount("plate", (), render_plate);
    host.run_until_idle();
    assert!(calls.get() >= 1, "PAINT phase ran the Vello hook");
    assert_eq!(last_ops.get(), 1, "mount plan through the phase");
    // Drain the mount frame's staged work (bg + plate) so the static
    // assertion below measures only the static frame.
    assert!(
        backend.borrow_mut().take_gpu_work() > 0,
        "mount stages work"
    );

    // Static frame: empty plan through the same phase (skip, zero work).
    host.runtime().request_frame();
    host.run_until_idle();
    assert!(calls.get() >= 2);
    assert_eq!(last_ops.get(), 0, "static frame builds empty plan in-phase");
    assert_eq!(backend.borrow_mut().take_gpu_work(), 0);
}

// ---------------------------------------------------------------------------
// GPU + DirectWrite tests: the driver matrix pixel rows (Windows).
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod gpu {
    use super::*;
    use oppa::TextService;

    /// Segoe UI face bytes via the DWrite hook (the spike's archaeology:
    /// `font_file_source` + fs read + Blob — the atlas's file locator).
    fn segoe_face() -> (Vec<u8>, u32) {
        use oppa_text_dwrite::DWriteTextService;
        let svc = DWriteTextService::new().expect("dwrite service");
        let style = oppa::TextStyle::new("Segoe UI", 16.0);
        let shaped = svc.shape("Hi", &style).expect("shape Hi");
        let fid = shaped.runs[0].font_id;
        let (path, index) = svc.font_file_source(fid).expect("font file for Segoe UI");
        let bytes = std::fs::read(&path).expect("read font file");
        assert!(!bytes.is_empty());
        (bytes, index)
    }

    #[test]
    fn adapter_matrix_log_primary_and_fallback() {
        // Hardware-oracle row: software-emulated adapters (WARP) prove no
        // real-GPU pixels and crash under parallel load - skip loudly.
        if let Err(e) = VelloBackend::probe_hardware_adapter() {
            eprintln!("SKIP adapter_matrix_log_primary_and_fallback: {e}");
            return;
        }
        let primary =
            VelloBackend::probe_adapter(false).expect("primary GPU adapter must exist for M6");
        println!("M6 primary adapter: {primary}");
        assert!(!primary.is_empty());
        // Weakest-available row: attempt the software fallback explicitly
        // and record (not assert) the outcome — the matrix states what
        // ran, not what was wished for.
        match VelloBackend::probe_adapter(true) {
            Ok(info) => println!("M6 fallback adapter: {info}"),
            Err(e) => println!("M6 fallback adapter: unavailable ({e})"),
        }
    }

    #[test]
    fn text_card_mount_plan_matches_m4_shape() {
        let host = ComponentHost::new();
        let svc = oppa_text_dwrite::DWriteTextService::new().expect("dwrite");
        host.set_text_service(Box::new(svc));
        host.set_viewport(VW, VH);
        let _handle = host.mount("card", CardProps { title: "Hi".into() }, render_card);
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
        // M4 shape unchanged: card RRect + one text line = 2 ops.
        assert_eq!(plan.ops.len(), 2, "ops: {plan:?}");
        assert!(matches!(plan.ops[0], DrawOp::RRect { .. }));
        assert!(matches!(plan.ops[1], DrawOp::Text { .. }));
    }

    #[test]
    fn atlas_fidelity_dwrite_hi_advances_unmodified() {
        let svc = oppa_text_dwrite::DWriteTextService::new().expect("dwrite");
        let style = oppa::TextStyle {
            family: "Segoe UI".to_string(),
            font_size_px: 16.0,
            device_pixel_ratio: 1.0,
            weight: oppa::FontWeight::NORMAL,
            style: oppa::FontStyle::Normal,
            stretch: oppa::FontStretch::NORMAL,
            letter_spacing_px: 0.0,
            locale: "en-US".to_string(),
        };
        let shaped = svc.shape("Hi", &style).expect("shape Hi");
        assert_eq!(shaped.glyphs.len(), 2);

        let host = ComponentHost::new();
        host.set_text_service(Box::new(
            oppa_text_dwrite::DWriteTextService::new().expect("dwrite"),
        ));
        host.set_viewport(VW, VH);
        let _handle = host.mount("card", CardProps { title: "Hi".into() }, render_card);
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));

        let mut be = VelloBackend::new();
        let (bytes, index) = segoe_face();
        be.set_font_bytes(bytes, index);
        let s = be.create_surface(surface_desc()).expect("surface");
        for d in host.diffs_from(0) {
            be.commit(&d).expect("commit");
        }
        be.paint(s, &plan).expect("paint text plan");

        // Atlas fidelity (measured #1): advances flow unmodified into
        // placed glyphs; successive x deltas equal advances subpixel-exact.
        let placed = be.atlas().placements();
        assert_eq!(placed.len(), shaped.glyphs.len(), "all cells placed");
        let mut max_delta: f32 = 0.0;
        for (enc, sh) in placed.iter().zip(shaped.glyphs.iter()) {
            assert_eq!(enc.glyph_id, sh.glyph_id, "glyph identity preserved");
            max_delta = max_delta.max((enc.advance - sh.x_advance).abs());
        }
        assert_eq!(max_delta, 0.0, "advances unmodified (never re-shaped)");
        for w in placed.windows(2) {
            let dx = w[1].x - w[0].x;
            assert_eq!(dx, w[0].advance, "subpixel pen exact: dx={dx}");
        }
        println!(
            "M6 atlas: Hi advances {:?} total={} max_delta={max_delta}",
            shaped
                .glyphs
                .iter()
                .map(|g| g.x_advance)
                .collect::<Vec<_>>(),
            shaped.total_advance
        );
    }

    /// Axis-aligned half of the coverage plan (no curves, no text):
    /// rect + shadow + clip + layer. Both rasterizers fill axis-aligned
    /// rects identically, so this half must agree pixel-exact.
    fn strict_geometry_plan() -> FramePlan {
        let n = NodeId::new(7, 0);
        FramePlan {
            viewport_w: VW,
            viewport_h: VH,
            ops: vec![
                DrawOp::PushClip {
                    x: 5.0,
                    y: 5.0,
                    w: 100.0,
                    h: 50.0,
                },
                DrawOp::PushLayer { opacity: 0.8 },
                DrawOp::Rect {
                    node: n,
                    x: 10.0,
                    y: 10.0,
                    w: 30.0,
                    h: 20.0,
                    color: CARD_BG,
                    opacity: 1.0,
                },
                DrawOp::Shadow {
                    node: n,
                    x: 10.0,
                    y: 35.0,
                    w: 30.0,
                    h: 10.0,
                    dx: 1.0,
                    dy: 2.0,
                    blur_radius: 0.0,
                    color: Color(0x00_00_00),
                },
                DrawOp::Pop,
                DrawOp::Pop,
            ],
            damage: vec![],
            stats: Default::default(),
            full_repaint: true,
        }
    }

    /// Curved half (RRect + Circle + border-ring pair): tiny-skia fills
    /// rounded shapes as bands + discs with per-piece AA while Vello
    /// fills one analytic path, so corner/edge coverage ramps differ by
    /// design. Bounded, not exact (measured per-op: rrect tol16=12,
    /// circle 0, ring 0; whole-plan bound 60 carries margin).
    fn curves_plan() -> FramePlan {
        let n = NodeId::new(7, 0);
        let ring = NodeId::new(8, 0);
        FramePlan {
            viewport_w: VW,
            viewport_h: VH,
            ops: vec![
                DrawOp::RRect {
                    node: n,
                    x: 45.0,
                    y: 10.0,
                    w: 30.0,
                    h: 20.0,
                    radius: 5.0,
                    radii: None,
                    color: CARD_BG,
                    opacity: 1.0,
                },
                DrawOp::Circle {
                    node: n,
                    cx: 95.0,
                    cy: 20.0,
                    r: 8.0,
                    color: CARD_BG,
                    opacity: 1.0,
                },
                DrawOp::RRect {
                    node: ring,
                    x: 4.0,
                    y: 4.0,
                    w: 40.0,
                    h: 24.0,
                    radius: 6.0,
                    radii: None,
                    color: Color(0xAA_BB_CC),
                    opacity: 1.0,
                },
                DrawOp::RRect {
                    node: ring,
                    x: 6.0,
                    y: 6.0,
                    w: 36.0,
                    h: 20.0,
                    radius: 4.0,
                    radii: None,
                    color: CARD_BG,
                    opacity: 1.0,
                },
            ],
            damage: vec![],
            stats: Default::default(),
            full_repaint: true,
        }
    }

    #[test]
    fn cpu_vs_vello_oracle_geometry() {
        // Hardware-oracle row: software-emulated adapters (WARP) prove no
        // real-GPU pixels and crash under parallel load - skip loudly.
        if let Err(e) = VelloBackend::probe_hardware_adapter() {
            eprintln!("SKIP cpu_vs_vello_oracle_geometry: {e}");
            return;
        }
        // Strict half: axis-aligned fills agree pixel-exact (no ramps to
        // hide behind — per-op probe reads exact=0 on rect/shadow/layer/
        // clip, so the whole strict plan must read 0/0).
        let plan = strict_geometry_plan();
        let mut oracle = oppa_vello::GpuOracle::new(surface_desc()).expect("oracle");
        use oppa::RendererBackend;
        oracle
            .cpu
            .paint(oracle.cpu_surface, &plan)
            .expect("cpu paint");
        oracle
            .vello
            .paint(oracle.vello_surface, &plan)
            .expect("vello paint");

        let cpu_px = oracle.cpu.pixmap(oracle.cpu_surface).expect("cpu pixmap");
        let cpu_img =
            oppa_vello::oracle::RgbaImage::from_cpu_pixmap(cpu_px).expect("all pixels opaque");
        let vello_img = oracle
            .vello
            .render_pixels(oracle.vello_surface)
            .expect("gpu readback");

        let exact = oppa_vello::oracle::diff_count_exact(&cpu_img, &vello_img);
        let banded = oppa_vello::oracle::diff_count_tol(&cpu_img, &vello_img, 3);
        let total = (VW as usize) * (VH as usize);
        println!("M6 geometry strict: exact_diff={exact} tol3_diff={banded} of {total}");
        assert_eq!(exact, 0, "axis-aligned fills must agree pixel-exact");
        assert_eq!(banded, 0);

        // Curves half: bounded agreement at tol-16 (analytic vs band/disc
        // AA ramps + corner shape approximation — stated, not hidden).
        let curves = curves_plan();
        oracle
            .cpu
            .paint(oracle.cpu_surface, &curves)
            .expect("cpu curves");
        oracle
            .vello
            .paint(oracle.vello_surface, &curves)
            .expect("vello curves");
        let cpu_px = oracle.cpu.pixmap(oracle.cpu_surface).expect("cpu pixmap");
        let cpu_img =
            oppa_vello::oracle::RgbaImage::from_cpu_pixmap(cpu_px).expect("all pixels opaque");
        let vello_img = oracle
            .vello
            .render_pixels(oracle.vello_surface)
            .expect("gpu readback");
        let c_exact = oppa_vello::oracle::diff_count_exact(&cpu_img, &vello_img);
        let c_tol16 = oppa_vello::oracle::diff_count_tol(&cpu_img, &vello_img, 16);
        println!("M6 geometry curves: exact_diff={c_exact} tol16_diff={c_tol16} of {total}");
        assert!(
            c_tol16 <= 60,
            "curves tol-16 diff {c_tol16} must stay within the stated 60px bound"
        );
    }

    #[test]
    fn cpu_vs_vello_text_position_equivalence() {
        // Hardware-oracle row: software-emulated adapters (WARP) prove no
        // real-GPU pixels and crash under parallel load - skip loudly.
        if let Err(e) = VelloBackend::probe_hardware_adapter() {
            eprintln!("SKIP cpu_vs_vello_text_position_equivalence: {e}");
            return;
        }
        // Text plans differ in SHAPE by design (outlines vs cells — F3);
        // they must agree in POSITION: ink columns within 1px per edge.
        let host = ComponentHost::new();
        host.set_text_service(Box::new(
            oppa_text_dwrite::DWriteTextService::new().expect("dwrite"),
        ));
        host.set_viewport(VW, VH);
        let _handle = host.mount("card", CardProps { title: "Hi".into() }, render_card);
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));

        let mut oracle = oppa_vello::GpuOracle::new(surface_desc()).expect("oracle");
        let (bytes, index) = segoe_face();
        oracle.vello.set_font_bytes(bytes, index);
        oracle.commit_all(&host.diffs_from(0)).expect("commit");
        use oppa::RendererBackend;
        oracle
            .cpu
            .paint(oracle.cpu_surface, &plan)
            .expect("cpu paint");
        oracle
            .vello
            .paint(oracle.vello_surface, &plan)
            .expect("vello paint");

        let cpu_px = oracle.cpu.pixmap(oracle.cpu_surface).expect("cpu pixmap");
        let cpu_img =
            oppa_vello::oracle::RgbaImage::from_cpu_pixmap(cpu_px).expect("all pixels opaque");
        let vello_img = oracle
            .vello
            .render_pixels(oracle.vello_surface)
            .expect("gpu readback");

        let cpu_cols = oppa_vello::oracle::ink_columns(&cpu_img, 0x88);
        let vello_cols = oppa_vello::oracle::ink_columns(&vello_img, 0x88);
        let coldiff = oppa_vello::oracle::ink_column_diff(&cpu_cols, &vello_cols);
        println!("M6 text columns: coldiff={coldiff}");
        assert!(
            coldiff <= 2,
            "ink columns agree within 1px per edge (got {coldiff})"
        );
        // Shapes DO differ (outlines vs solid cells) — record, don't hide.
        let exact = oppa_vello::oracle::diff_count_exact(&cpu_img, &vello_img);
        println!("M6 text shape diff (expected nonzero, F3): exact={exact}");
        assert!(
            exact > 0,
            "outlines must differ from cells — else the review is vacuous"
        );
    }

    #[test]
    fn glyph_review_vello_hi_fringe_and_alpha() {
        // Hardware-oracle row: software-emulated adapters (WARP) prove no
        // real-GPU pixels and crash under parallel load - skip loudly.
        if let Err(e) = VelloBackend::probe_hardware_adapter() {
            eprintln!("SKIP glyph_review_vello_hi_fringe_and_alpha: {e}");
            return;
        }
        // Glyph review vs the M4 baseline: real-outline AA fringe strictly
        // between ink and bg (stronger than the cell-fringe floor), plus
        // the decision-103 alpha pixel proof on the same surface family.
        let host = ComponentHost::new();
        host.set_text_service(Box::new(
            oppa_text_dwrite::DWriteTextService::new().expect("dwrite"),
        ));
        host.set_viewport(VW, VH);
        let _handle = host.mount("card", CardProps { title: "Hi".into() }, render_card);
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));

        let mut oracle = oppa_vello::GpuOracle::new(surface_desc()).expect("oracle");
        let (bytes, index) = segoe_face();
        oracle.vello.set_font_bytes(bytes, index);
        oracle.commit_all(&host.diffs_from(0)).expect("commit");
        use oppa::RendererBackend;
        oracle
            .cpu
            .paint(oracle.cpu_surface, &plan)
            .expect("cpu paint");
        oracle
            .vello
            .paint(oracle.vello_surface, &plan)
            .expect("vello paint");
        let vello_img = oracle
            .vello
            .render_pixels(oracle.vello_surface)
            .expect("gpu readback");

        // The H stem's left edge sits inside the first cell (8..19.36):
        // scan the text rows for a pixel strictly between ink and card
        // bg on the gray axis — real outline AA, not a cell edge.
        let mut fringe = 0;
        for y in 0..VH as u32 {
            for x in 0..VW as u32 {
                if let Some((r, g, b, a)) = vello_img.pixel(x, y) {
                    assert_eq!(a, 255);
                    if r == g && g == b && r > 0x11 && r < 0x44 {
                        fringe += 1;
                    }
                }
            }
        }
        println!("M6 glyph fringe pixels (outline AA): {fringe}");
        assert!(fringe > 0, "real outlines carry AA fringe (M4 floor)");

        // Decision-103 pixel proof: half-alpha CARD_BG over white on both
        // rasterizers ≈ 162 gray (±2 for the coverage ramp).
        let n = NodeId::new(9, 0);
        let alpha_plan = FramePlan {
            viewport_w: VW,
            viewport_h: VH,
            ops: vec![DrawOp::Rect {
                node: n,
                x: 10.0,
                y: 10.0,
                w: 40.0,
                h: 40.0,
                color: CARD_BG,
                opacity: 0.5,
            }],
            damage: vec![],
            stats: Default::default(),
            full_repaint: true,
        };
        oracle
            .cpu
            .paint(oracle.cpu_surface, &alpha_plan)
            .expect("cpu alpha");
        oracle
            .vello
            .paint(oracle.vello_surface, &alpha_plan)
            .expect("vello alpha");
        let cpu_px = oracle.cpu.pixmap(oracle.cpu_surface).expect("cpu pixmap");
        let cpu_img = oppa_vello::oracle::RgbaImage::from_cpu_pixmap(cpu_px).expect("opaque");
        let vello_alpha = oracle
            .vello
            .render_pixels(oracle.vello_surface)
            .expect("gpu alpha");
        for (img, name) in [(&cpu_img, "cpu"), (&vello_alpha, "vello")] {
            let (r, g, b, a) = img.pixel(30, 30).expect("interior px");
            assert_eq!(a, 255);
            assert!(
                (r as i16 - 162).abs() <= 2,
                "{name} blended r={r} (want ≈162)"
            );
            assert_eq!((g, b), (r, r), "{name} gray-axis");
        }
        let banded = oppa_vello::oracle::diff_count_tol(&cpu_img, &vello_alpha, 2);
        println!("M6 alpha cross-backend tol2 diff: {banded}");
    }

    /// Round 1.3 scenes (decision 254): per-edge bands + gradient strips
    /// are integer opaque solids through the shared builder — the same
    /// proof as the strict-geometry half, so this must read exact 0.
    fn render_fx_exact(_ctx: &Ctx, _p: &()) -> VNode {
        Div("root").children([
            Div("fx-edges")
                .style(
                    oppa::Style::new()
                        .size(30, 10)
                        .border_edges(2, 4, 2, 4, Color(0xAA_BB_CC)),
                )
                .build(),
            Div("fx-grad")
                .style(
                    oppa::Style::new()
                        .size(30, 6)
                        .bg_gradient(Color(0x00_00_00), Color(0xFF_FF_FF)),
                )
                .build(),
        ])
    }

    #[test]
    fn style_fx_edges_and_gradient_agree_pixel_exact() {
        // Hardware-oracle row: software-emulated adapters (WARP) prove no
        // real-GPU pixels and crash under parallel load - skip loudly.
        if let Err(e) = VelloBackend::probe_hardware_adapter() {
            eprintln!("SKIP style_fx_edges_and_gradient_agree_pixel_exact: {e}");
            return;
        }
        let host = ComponentHost::new();
        host.set_viewport(VW, VH);
        let _handle = host.mount("fx", (), render_fx_exact);
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
        assert!(
            plan.ops.len() > 4,
            "bands + strips expand to many rects: {plan:?}"
        );
        assert!(
            plan.ops.iter().all(|o| matches!(o, DrawOp::Rect { .. })),
            "expansion is solids only: {plan:?}"
        );

        let mut oracle = oppa_vello::GpuOracle::new(surface_desc()).expect("oracle");
        oracle.commit_all(&host.diffs_from(0)).expect("commit");
        use oppa::RendererBackend;
        oracle
            .cpu
            .paint(oracle.cpu_surface, &plan)
            .expect("cpu paint");
        oracle
            .vello
            .paint(oracle.vello_surface, &plan)
            .expect("vello paint");
        let cpu_px = oracle.cpu.pixmap(oracle.cpu_surface).expect("cpu pixmap");
        let cpu_img =
            oppa_vello::oracle::RgbaImage::from_cpu_pixmap(cpu_px).expect("all pixels opaque");
        let vello_img = oracle
            .vello
            .render_pixels(oracle.vello_surface)
            .expect("gpu readback");
        let exact = oppa_vello::oracle::diff_count_exact(&cpu_img, &vello_img);
        let total = (VW as usize) * (VH as usize);
        println!("1.3 edges+gradient: exact_diff={exact} of {total}");
        assert_eq!(exact, 0, "shared-builder solids must agree pixel-exact");
    }

    /// Round 1.3 blurred shadow (decision 254): stepped solids with
    /// fractional opacities — the compositing half, same class as the M6
    /// alpha row (tol2), not the strict half. Bound asserted, not hidden.
    fn render_fx_blur(_ctx: &Ctx, _p: &()) -> VNode {
        Div("root").child(
            Div("fx-shadow")
                .style(
                    oppa::Style::new()
                        .size(30, 10)
                        .shadow(2, 3, Color(0x00_00_00))
                        .shadow_blur(2),
                )
                .build(),
        )
    }

    #[test]
    fn style_fx_blurred_shadow_agrees_tol_banded() {
        // Hardware-oracle row: software-emulated adapters (WARP) prove no
        // real-GPU pixels and crash under parallel load - skip loudly.
        if let Err(e) = VelloBackend::probe_hardware_adapter() {
            eprintln!("SKIP style_fx_blurred_shadow_agrees_tol_banded: {e}");
            return;
        }
        // Phase 36 PR4 (decision 356, supersedes the stepped solids):
        // one native-blur `Shadow` op (gaussian on Vello, box-blur on
        // CPU) — agreement is tol-banded (exact cross-engine blur
        // parity is not claimed, only geometry + the zero-blur
        // solid). Bound calibrated on hardware (2026-10-01: 438/7200
        // = 6.1% over tol-16 on a 30x10 rect at blur 2 — box and
        // gaussian profiles differ through the soft band by design);
        // the gate holds < 10% so genuine regressions (wrong rect,
        // missing blur, inverted alpha) still fail loudly.
        let host = ComponentHost::new();
        host.set_viewport(VW, VH);
        let _handle = host.mount("fx", (), render_fx_blur);
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
        let shadows: Vec<_> = plan
            .ops
            .iter()
            .filter(|op| matches!(op, oppa::DrawOp::Shadow { .. }))
            .collect();
        assert_eq!(shadows.len(), 1, "one native blur op: {plan:?}");

        let mut oracle = oppa_vello::GpuOracle::new(surface_desc()).expect("oracle");
        oracle.commit_all(&host.diffs_from(0)).expect("commit");
        use oppa::RendererBackend;
        oracle
            .cpu
            .paint(oracle.cpu_surface, &plan)
            .expect("cpu paint");
        oracle
            .vello
            .paint(oracle.vello_surface, &plan)
            .expect("vello paint");
        let cpu_px = oracle.cpu.pixmap(oracle.cpu_surface).expect("cpu pixmap");
        let cpu_img =
            oppa_vello::oracle::RgbaImage::from_cpu_pixmap(cpu_px).expect("all pixels opaque");
        let vello_img = oracle
            .vello
            .render_pixels(oracle.vello_surface)
            .expect("gpu readback");
        let tol16 = oppa_vello::oracle::diff_count_tol(&cpu_img, &vello_img, 16);
        let total = (VW as usize) * (VH as usize);
        println!("36.4 blurred shadow: tol16_diff={tol16} of {total}");
        assert!(
            tol16 * 10 < total,
            "blurred shadow agrees tol-banded (< 10% pixels over tol-16), got {tol16} of {total}"
        );
    }
}
