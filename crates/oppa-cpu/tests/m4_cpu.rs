//! M4 acceptance: CPU backend + FramePlan builder + image-diff oracle.
//!
//! The first runnable — one static component through the whole pipe
//! (core→reconciler→layout→FramePlan→CPU backend→PNG) plus the
//! SemanticsDiff dump. Portable tests shape through a counting fake
//! `TextService` (uniform advance, byte-exact clusters); the Windows-only
//! test proves DirectWrite-shaped advances flow unmodified into the plan.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use oppa::{
    compute_semantics_diff, BackendError, Caps, Color, Column, ComponentHost, Ctx, Div, DrawOp,
    FontId, FontMetrics, FramePlan, NodeId, Portal, PresenterKind, Props, RendererBackend, Row,
    Semantics, SemanticsSnapshot, ShapedGlyph, ShapedRun, Style, Text, TextError, TextRun,
    TextService, ThemeMode, ThemeTokens, VNode, INK,
};
use oppa_cpu::{CpuBackend, FramePlanBuilder, OracleSession};

// ---------------------------------------------------------------------------
// FakeText: uniform advance, em-scaled like a real backend (M3 rig shape,
// no RTL ranges — M4 scenes are LTR).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FakeText {
    calls: Rc<Cell<usize>>,
}

impl FakeText {
    fn new() -> (Self, Rc<Cell<usize>>) {
        let calls = Rc::new(Cell::new(0));
        (
            Self {
                calls: calls.clone(),
            },
            calls,
        )
    }
}

impl TextService for FakeText {
    fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
        Vec::new()
    }

    fn shape(&self, text: &str, style: &oppa::TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        self.calls.set(self.calls.get() + 1);
        let em = style.font_size_px * style.device_pixel_ratio;
        let adv = em * 0.625;
        let metrics = FontMetrics {
            ascent: em * 0.75,
            descent: em * 0.25,
            line_gap: em * 0.125,
        };
        let mut glyphs = Vec::new();
        let mut clusters = Vec::new();
        for (k, (i, ch)) in text.char_indices().enumerate() {
            let len = ch.len_utf8();
            glyphs.push(ShapedGlyph {
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
        let total_advance = adv * glyphs.len() as f32;
        Ok(ShapedRun {
            glyphs,
            runs: vec![TextRun {
                byte_range: (0, text.len()),
                glyph_range: (0, clusters.len()),
                rtl: false,
                script: 0,
                font_id: FontId(0),
                font_metrics: metrics,
            }],
            clusters,
            total_advance,
            text_len_bytes: text.len(),
        })
    }
}

// ---------------------------------------------------------------------------
// Static scene: styled Div (padding/radius/background) + one text line.
// Fake metrics at title 16px/dpr1: adv 10px, ascent 12, descent 4,
// line height 16. Div 100—40 at (0,0); text box (8,0,84,16).
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
    let text: VNode = Text {
        text: Arc::from(props.title.as_str()),
        style: Text::title_small,
    }
    .into();
    Div("card")
        .style(Style::new().size(100, 40).pad_x(8).radius(6).bg(CARD_BG))
        .child(text)
}

struct Rig {
    host: ComponentHost,
    #[allow(dead_code)]
    handle: oppa::MountHandle<CardProps>,
    builder: FramePlanBuilder,
    backend: CpuBackend,
    surface: oppa::SurfaceId,
}

impl Rig {
    fn new(title: &str) -> Self {
        let (fake, _) = FakeText::new();
        let host = ComponentHost::new();
        host.set_text_service(Box::new(fake));
        host.set_viewport(VW, VH);
        let handle = host.mount(
            "card",
            CardProps {
                title: title.to_string(),
            },
            render_card,
        );
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        let mut backend = CpuBackend::new();
        let surface = backend
            .create_surface(oppa::SurfaceDesc {
                width_px: VW as u32,
                height_px: VH as u32,
                background: SURFACE_BG,
            })
            .expect("surface");
        Self {
            host,
            handle,
            builder,
            backend,
            surface,
        }
    }

    fn build_incremental(&mut self) -> FramePlan {
        self.host
            .with_retained_mut(|rec, styles| self.builder.build_incremental(rec, styles))
    }

    fn commit_all(&mut self) {
        let diffs = self.host.diffs_from(0);
        for d in &diffs {
            self.backend.commit(d).expect("commit");
        }
    }

    fn paint(&mut self, plan: &FramePlan) -> oppa::PaintStats {
        self.backend.paint(self.surface, plan).expect("paint")
    }

    fn png_bytes(&self) -> Vec<u8> {
        self.backend.encode_png(self.surface).expect("png")
    }

    fn pixel(&self, x: u32, y: u32) -> (u8, u8, u8, u8) {
        self.backend.pixel_rgba(self.surface, x, y).expect("pixel")
    }
}

fn gray() -> (u8, u8, u8, u8) {
    (0x44, 0x44, 0x44, 255)
}

fn white() -> (u8, u8, u8, u8) {
    (255, 255, 255, 255)
}

fn ink() -> (u8, u8, u8, u8) {
    (0x11, 0x11, 0x11, 255)
}

// ---------------------------------------------------------------------------
// 1. Static scene → PNG on disk, hand-computed pixel spot checks.
// ---------------------------------------------------------------------------

#[test]
fn static_scene_png_spot_checks() {
    let mut rig = Rig::new("Hi");
    let plan = rig.build_incremental();
    // Mount dirties everything: card RRect + one text line = 2 ops.
    assert_eq!(plan.ops.len(), 2, "ops: {plan:?}");
    assert!(matches!(plan.ops[0], DrawOp::RRect { .. }), "card bg first");
    assert!(matches!(plan.ops[1], DrawOp::Text { .. }), "text second");
    assert_eq!(plan.damage.len(), 3, "card + wrapper + leaf boxes");
    assert_eq!(plan.stats.subtrees_skipped, 0);

    rig.commit_all();
    let stats = rig.paint(&plan);
    assert!(!stats.skipped_empty);
    assert!(stats.ops_executed > 0);

    // Hand-computed spots (FakeText 16px: adv 10, line 16; div 100—40 r6).
    assert_eq!(rig.pixel(110, 50), white(), "surface bg outside card");
    assert_eq!(rig.pixel(4, 30), gray(), "div padding below text");
    assert_eq!(rig.pixel(50, 0), gray(), "div top band inside radius");
    assert_eq!(rig.pixel(13, 8), ink(), "first glyph cell center");
    assert_eq!(rig.pixel(23, 8), ink(), "second glyph cell center");
    assert_eq!(rig.pixel(0, 0), white(), "rounded corner cutout");
    assert_eq!(rig.pixel(33, 8), gray(), "past the text, still div bg");

    // PNG on disk (the §3 artifact).
    let path = std::env::temp_dir().join("m4_static.png");
    rig.backend.save_png(rig.surface, &path).expect("save png");
    let meta = std::fs::metadata(&path).expect("png on disk");
    assert!(meta.len() > 100, "non-trivial png");
    println!("m4 static png: {} ({} bytes)", path.display(), meta.len());

    // StyleId→rule sharing stays (locked #8): equal payloads, one rule.
    let (a, b) = rig.host.with_retained_mut(|rec, _| {
        let ids = oppa_cpu::builder::node_styles(rec);
        assert_eq!(ids.len(), rec.retained_ids().len());
        (ids.len(), rec.retained_ids().len())
    });
    assert_eq!(a, b);
}

// ---------------------------------------------------------------------------
// Round 11.1 (decision 305): per-corner radii paint per vertex.
// ---------------------------------------------------------------------------

fn render_corners(_ctx: &Ctx, _p: &()) -> VNode {
    Div("card")
        .style(Style::new().size(100, 40).radius_tl(12).bg(CARD_BG))
        .build()
}

/// Per-corner radii ride `RRect.radii` (CSS order) and paint round
/// where set, sharp where not: the top-left cuts out, the other
/// three vertices stay square.
#[test]
fn per_corner_radii_paint_round_and_sharp_vertices() {
    let (fake, _) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    host.set_viewport(VW, VH);
    host.mount("card", (), render_corners);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    type Bounds = (f32, f32, f32, f32, Option<[f32; 4]>);
    let rects: Vec<Bounds> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::RRect {
                x, y, w, h, radii, ..
            } => Some((*x, *y, *w, *h, *radii)),
            _ => None,
        })
        .collect();
    assert_eq!(rects.len(), 1, "one rounded fill, got {rects:?}");
    assert_eq!(
        rects[0].4,
        Some([12.0, 0.0, 0.0, 0.0]),
        "CSS tl/tr/br/bl order"
    );

    let mut backend = CpuBackend::new();
    let surface = backend
        .create_surface(oppa::SurfaceDesc {
            width_px: VW as u32,
            height_px: VH as u32,
            background: SURFACE_BG,
        })
        .expect("surface");
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.paint(surface, &plan).expect("paint");
    let px = |x: u32, y: u32| backend.pixel_rgba(surface, x, y).expect("pixel");
    // Box at (0,0): TL arc center (12,12) r12 — (2,2) sits outside it.
    assert_eq!(px(0, 0), white(), "round TL cuts the corner");
    assert_eq!(px(2, 2), white(), "outside the TL arc");
    assert_eq!(px(12, 12), gray(), "inside the TL arc");
    assert_eq!(px(99, 0), gray(), "sharp TR stays square");
    assert_eq!(px(0, 39), gray(), "sharp BL stays square");
    assert_eq!(px(99, 39), gray(), "sharp BR stays square");
    assert_eq!(px(50, 20), gray(), "center fills");
}

// ---------------------------------------------------------------------------
// 2. FramePlan minimality: empty on no-change; exactly the subtree on text change.
// ---------------------------------------------------------------------------

#[test]
fn frameplan_minimality_static_zero_text_subtree() {
    let mut rig = Rig::new("Hi");
    let p1 = rig.build_incremental();
    rig.commit_all();
    rig.paint(&p1);
    let before = rig.png_bytes();

    // Second commit with no changes → empty plan (static ≈ 0 CPU).
    let p2 = rig.build_incremental();
    assert!(p2.is_empty(), "no-change plan must be empty: {p2:?}");
    assert_eq!(p2.stats.subtrees_skipped, 3, "whole tree skipped");
    assert_eq!(p2.stats.nodes_visited, 1, "root probed, children skipped");
    let st = rig.paint(&p2);
    assert!(st.skipped_empty, "empty plan touches nothing");
    assert_eq!(rig.png_bytes(), before, "surface byte-identical");

    // Text-only change rebuilds exactly its subtree ops (props-driven;
    // covered precisely in text_only_change_rebuilds_exactly_its_subtree).
    let p3 = rig.build_incremental();
    assert!(p3.is_empty(), "still empty with no props change");
}

// Text-change minimality needs the MountHandle: separate focused test.

#[test]
fn text_only_change_rebuilds_exactly_its_subtree() {
    let (fake, _) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    host.set_viewport(VW, VH);
    let handle = host.mount(
        "card",
        CardProps {
            title: "Hi".to_string(),
        },
        render_card,
    );
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let p1 = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    assert_eq!(p1.ops.len(), 2);

    handle.set_props(CardProps {
        title: "Hi!".to_string(),
    });
    host.run_until_idle();
    let p2 = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    // Exactly the text leaf's op (rebuilt), nothing else (skipped).
    assert_eq!(p2.ops.len(), 1, "one subtree op: {p2:?}");
    assert!(matches!(p2.ops[0], DrawOp::Text { .. }));
    // F1 closed (M6 decision 104): the text wrapper's content extent also
    // changed with LAYOUT-only dirt, so the engine stamps PAINT on it —
    // damage covers wrapper + leaf (2), while ops stay 1 (the wrapper
    // emits no fill of its own).
    assert_eq!(p2.damage.len(), 2);
    assert_eq!(
        p2.stats.subtrees_skipped, 0,
        "open chain, no skipped subtree"
    );
    assert_eq!(p2.stats.nodes_visited, 3);
    // Damage payoff numbers for the report: rebuilt 1 op vs full 2.
    assert_eq!(p1.ops.len(), 2, "full static plan is 2 ops");

    // Third build with no further change → empty again.
    let p3 = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    assert!(p3.is_empty());
}

// ---------------------------------------------------------------------------
// 3. Full-repaint-assert oracle: incremental output == full-repaint output.
// ---------------------------------------------------------------------------

#[test]
fn oracle_incremental_matches_full_repaint() {
    let (fake, _) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    host.set_viewport(VW, VH);
    let handle = host.mount(
        "card",
        CardProps {
            title: "Hi".to_string(),
        },
        render_card,
    );
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);

    let desc = oppa::SurfaceDesc {
        width_px: VW as u32,
        height_px: VH as u32,
        background: SURFACE_BG,
    };
    let mut oracle = OracleSession::new(desc).expect("oracle");
    oracle.commit_all(&host.diffs_from(0)).expect("commit");

    // Static frame: incremental plan already carries every op.
    let incr = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    let full = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    assert!(full.full_repaint);
    assert_eq!(full.ops.len(), incr.ops.len());
    let diff = oracle.assert_paints(&incr, &full).expect("oracle paints");
    assert_eq!(diff, 0, "incremental == full repaint");

    // Text change with replayed history: paint P1 then P2 incrementally.
    handle.set_props(CardProps {
        title: "Hi!".to_string(),
    });
    host.run_until_idle();
    oracle.commit_all(&host.diffs_from(0)).expect("commit");
    let incr2 = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    assert_eq!(incr2.ops.len(), 1);
    let full2 = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let (s_incr, _) = oracle.surfaces();
    oracle.backend_mut().paint(s_incr, &incr2).expect("replay");
    let diff2 = oracle
        .assert_paints(&FramePlan::default(), &full2)
        .expect("oracle paints");
    assert_eq!(
        diff2, 0,
        "incremental history == full repaint after text change"
    );
}

// ---------------------------------------------------------------------------
// 4. SemanticsDiff dump on toggle + list shapes (payloads flow; emitters M10).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SemProps {
    on: bool,
    rows: usize,
}

impl Props for SemProps {}

fn render_sem(_ctx: &Ctx, props: &SemProps) -> VNode {
    let mut kids = vec![Div("track")
        .style(Style::new().size(44, 24))
        .semantics(Semantics::switch().checked(props.on).label("Wi-Fi"))
        .build()];
    for i in 0..props.rows {
        kids.push(
            Row("row")
                .key(i as u64)
                .style(Style::new().size(100, 20))
                .semantics(Semantics::list_item().selected(i == 1).label(if i == 0 {
                    "a"
                } else {
                    "b"
                }))
                .build(),
        );
    }
    Column::new().children(kids)
}

#[test]
fn semantics_diff_toggle_and_list() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 200.0);
    let handle = host.mount("sem", SemProps { on: true, rows: 2 }, render_sem);
    host.run_until_idle();

    let mut snap = SemanticsSnapshot::new();
    let d1 = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
    assert_eq!(d1.upserted.len(), 3, "toggle + 2 rows");
    assert!(d1.removed.is_empty());
    let dump = d1.dump();
    assert!(dump.contains("Wi-Fi"), "dump:\n{dump}");
    assert!(dump.contains("Switch"), "dump:\n{dump}");
    assert_eq!(snap.len(), 3);

    // Toggle flip → exactly one upsert with the new payload.
    handle.set_props(SemProps { on: false, rows: 2 });
    host.run_until_idle();
    let d2 = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
    assert_eq!(d2.upserted.len(), 1);
    assert_eq!(d2.upserted[0].semantics.checked, Some(false));
    assert!(d2.removed.is_empty());

    // Row removal → exactly one removal.
    handle.set_props(SemProps { on: false, rows: 1 });
    host.run_until_idle();
    let d3 = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snap));
    assert_eq!(d3.removed.len(), 1, "dump:\n{}", d3.dump());
    assert!(d3.upserted.is_empty(), "survivors unchanged: {}", d3.dump());
    assert_eq!(snap.len(), 2);
}

// ---------------------------------------------------------------------------
// 5. Rounded-box determinism: same tree twice → identical boxes and PNG.
// ---------------------------------------------------------------------------

#[test]
fn rounded_box_determinism_same_tree_twice() {
    type BoxesAndPng = (Vec<(f32, f32, f32, f32)>, Vec<u8>);
    fn run_once() -> BoxesAndPng {
        let mut rig = Rig::new("Hi");
        let boxes = rig.host.with_retained_mut(|rec, _| {
            let mut ids = rec.retained_ids();
            ids.sort();
            ids.iter()
                .filter_map(|id| rec.get(*id).and_then(|n| n.layout.clone()))
                .map(|b| (b.x, b.y, b.w, b.h))
                .collect::<Vec<_>>()
        });
        let plan = rig.build_incremental();
        rig.commit_all();
        rig.paint(&plan);
        (boxes, rig.png_bytes())
    }
    let (b1, p1) = run_once();
    let (b2, p2) = run_once();
    assert_eq!(b1, b2, "identical committed boxes");
    assert_eq!(p1, p2, "identical PNG bytes");
}

// ---------------------------------------------------------------------------
// 6. Contract surface: kind/caps/multi-surface/destroy/loud refusals.
// ---------------------------------------------------------------------------

#[test]
fn contract_surface_kind_caps_surfaces_and_refusals() {
    let be = CpuBackend::new();
    assert_eq!(be.kind(), PresenterKind::Cpu);
    assert_eq!(be.caps(), Caps::cpu_fallback());

    let mut be = CpuBackend::new();
    let desc = oppa::SurfaceDesc {
        width_px: 40,
        height_px: 40,
        background: SURFACE_BG,
    };
    let s1 = be.create_surface(desc).expect("s1");
    let s2 = be.create_surface(desc).expect("s2");

    // Hand-built plan: clip + layer + rect (exercises every stack op).
    let n = NodeId::new(7, 0);
    let plan = FramePlan {
        viewport_w: 40.0,
        viewport_h: 40.0,
        ops: vec![
            DrawOp::PushClip {
                x: 5.0,
                y: 5.0,
                w: 20.0,
                h: 20.0,
            },
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
            DrawOp::Pop,
        ],
        damage: vec![],
        stats: Default::default(),
        full_repaint: true,
    };
    be.paint(s1, &plan).expect("paint s1");
    be.paint(s2, &plan).expect("paint s2");
    assert_eq!(
        be.encode_png(s1).expect("png1"),
        be.encode_png(s2).expect("png2")
    );

    // Clip held: outside the clip rect the bg survives; inside, the
    // half-alpha gray blends over white (≈162).
    assert_eq!(be.pixel_rgba(s1, 30, 30).expect("px"), white());
    let (r, g, b, a) = be.pixel_rgba(s1, 10, 10).expect("px");
    assert_eq!(a, 255);
    assert!((r as i16 - 162).abs() <= 1, "blended r={r}");
    assert!((g as i16 - 162).abs() <= 1, "blended g={g}");
    assert!((b as i16 - 162).abs() <= 1, "blended b={b}");

    be.destroy_surface(s2).expect("destroy");
    assert!(matches!(
        be.paint(s2, &plan),
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

fn render_img(_ctx: &Ctx, _p: &()) -> VNode {
    // Fresh ImageCache::load("static/logo") yields ImageId(1) — asserted
    // at the test body; the render fn stays a plain fn (mount takes fn).
    Div("wrap").child(
        oppa::Img {
            src: oppa::ImageId(1),
            size: 36.0,
            radius: 18.0,
        }
        .into(),
    )
}

#[test]
fn rimg_refused_loudly_without_pixels() {
    let host = ComponentHost::new();
    host.set_viewport(100.0, 100.0);
    let cache = oppa::ImageCache::new();
    assert_eq!(cache.load("static/logo"), oppa::ImageId(1));
    host.mount("img", (), render_img);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    assert!(plan.ops.iter().any(|o| matches!(o, DrawOp::RImg { .. })));

    let mut be = CpuBackend::new();
    let s = be
        .create_surface(oppa::SurfaceDesc {
            width_px: 100,
            height_px: 100,
            background: SURFACE_BG,
        })
        .expect("surface");
    for d in host.diffs_from(0) {
        be.commit(&d).expect("commit");
    }
    let err = be.paint(s, &plan).expect_err("RImg must fail loudly");
    assert!(matches!(err, BackendError::UnsupportedOp(_)), "{err}");
    // No partial paint: surface still pristine bg.
    assert_eq!(be.pixel_rgba(s, 50, 50).expect("px"), white());
}

// ---------------------------------------------------------------------------
// 7. PAINT-phase wiring: build + commit through set_paint_pass.
// ---------------------------------------------------------------------------

#[test]
fn paint_phase_wiring_builds_and_commits() {
    let (fake, _) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    host.set_viewport(VW, VH);
    let backend: Rc<RefCell<CpuBackend>> = Rc::new(RefCell::new(CpuBackend::new()));
    let surface = backend
        .borrow_mut()
        .create_surface(oppa::SurfaceDesc {
            width_px: VW as u32,
            height_px: VH as u32,
            background: SURFACE_BG,
        })
        .expect("surface");
    let calls = Rc::new(Cell::new(0usize));
    let last_ops = Rc::new(Cell::new(usize::MAX));
    oppa_cpu::install_paint_hook(
        &host,
        backend.clone(),
        surface,
        1.0,
        calls.clone(),
        last_ops.clone(),
    );

    let _handle = host.mount(
        "card",
        CardProps {
            title: "Hi".to_string(),
        },
        render_card,
    );
    host.run_until_idle();
    assert!(calls.get() >= 1, "PAINT phase ran the hook");
    assert_eq!(last_ops.get(), 2, "mount plan through the phase");
    assert_eq!(
        backend.borrow().pixel_rgba(surface, 13, 8).expect("px"),
        ink()
    );

    // One more frame with no changes → empty plan through the same phase.
    let before = backend.borrow().encode_png(surface).expect("png");
    host.runtime().request_frame();
    host.run_until_idle();
    assert!(calls.get() >= 2);
    assert_eq!(last_ops.get(), 0, "static frame builds empty plan in-phase");
    assert_eq!(backend.borrow().encode_png(surface).expect("png"), before);
}

// ---------------------------------------------------------------------------
// 8. Windows-only: DirectWrite advances flow into the plan unmodified.
// ---------------------------------------------------------------------------

#[cfg(windows)]
#[test]
fn dwrite_advances_flow_unmodified_into_plan() {
    use oppa_text_dwrite::DWriteTextService;

    let svc = DWriteTextService::new().expect("dwrite service");
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
    assert_eq!(shaped.glyphs.len(), 2, "M0b: Hi is 2 glyphs");

    let host = ComponentHost::new();
    host.set_text_service(Box::new(svc));
    host.set_viewport(VW, VH);
    let _handle = host.mount(
        "card",
        CardProps {
            title: "Hi".to_string(),
        },
        render_card,
    );
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    let text_op = plan
        .ops
        .iter()
        .find_map(|o| match o {
            DrawOp::Text { glyphs, .. } => Some(glyphs),
            _ => None,
        })
        .expect("text op");
    assert_eq!(text_op.len(), shaped.glyphs.len());
    for (placed, shaped_g) in text_op.iter().zip(shaped.glyphs.iter()) {
        assert_eq!(
            placed.glyph_id, shaped_g.glyph_id,
            "glyph identity preserved"
        );
        assert_eq!(
            placed.advance, shaped_g.x_advance,
            "advance unmodified (never re-shaped)"
        );
    }
    println!(
        "dwrite Hi advances: {:?} total={}",
        shaped
            .glyphs
            .iter()
            .map(|g| g.x_advance)
            .collect::<Vec<_>>(),
        shaped.total_advance
    );

    // And it paints (block cells at shaped positions).
    let mut be = CpuBackend::new();
    let s = be
        .create_surface(oppa::SurfaceDesc {
            width_px: VW as u32,
            height_px: VH as u32,
            background: SURFACE_BG,
        })
        .expect("surface");
    for d in host.diffs_from(0) {
        be.commit(&d).expect("commit");
    }
    be.paint(s, &plan).expect("paint");
    assert_ne!(be.encode_png(s).expect("png").len(), 0);

    // Subpixel observation (M6 review starts here): the H cell ends at
    // 8+11.359375=19.359375, so pixel 19 is a partial-coverage AA fringe
    // strictly between ink and div bg — advances honored subpixel, no
    // re-rounding in the backend. Cells are solid (no outlines): the
    // documented gap Caps::text_as_paths=false names.
    let (r, g, b, a) = be.pixel_rgba(s, 19, 8).expect("fringe px");
    assert_eq!(a, 255);
    assert!(r > 17 && r < 68, "aa fringe r={r}");
    assert_eq!((g, b), (r, r), "gray-axis fringe");
}

// ---------------------------------------------------------------------------
// G8 image paint: decoded pixels (oppa-image shape) deposited with
// insert_image paint through DrawOp::RImg; unregistered ids still
// refuse loudly (the M4 refusal, now pending-specific).
// ---------------------------------------------------------------------------

/// Hand-built plan with one RImg (bypasses the builder — the builder
/// path is M4-proven; this tests the paint arm).
fn rimg_plan(x: f32, y: f32, w: f32, h: f32, image: oppa::ImageId) -> FramePlan {
    FramePlan {
        viewport_w: 8.0,
        viewport_h: 8.0,
        ops: vec![DrawOp::RImg {
            node: NodeId::new(1, 0),
            x,
            y,
            w,
            h,
            image,
        }],
        damage: vec![],
        stats: oppa::PlanStats::default(),
        full_repaint: true,
    }
}

fn img_backend() -> (CpuBackend, oppa::SurfaceId) {
    let mut backend = CpuBackend::new();
    let surface = backend
        .create_surface(oppa::SurfaceDesc {
            width_px: 8,
            height_px: 8,
            background: SURFACE_BG,
        })
        .expect("surface");
    (backend, surface)
}

#[test]
fn rimg_paints_inserted_pixels_exactly() {
    let (mut backend, surface) = img_backend();
    // 2x2 distinct opaque: red green / blue white.
    let rgba: Vec<u8> = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let id = oppa::ImageId(1);
    backend.insert_image(id, 2, 2, rgba);
    backend
        .paint(surface, &rimg_plan(2.0, 2.0, 2.0, 2.0, id))
        .expect("paint");
    assert_eq!(
        backend.pixel_rgba(surface, 0, 0),
        Some((255, 255, 255, 255))
    );
    assert_eq!(backend.pixel_rgba(surface, 2, 2), Some((255, 0, 0, 255)));
    assert_eq!(backend.pixel_rgba(surface, 3, 2), Some((0, 255, 0, 255)));
    assert_eq!(backend.pixel_rgba(surface, 2, 3), Some((0, 0, 255, 255)));
    assert_eq!(
        backend.pixel_rgba(surface, 3, 3),
        Some((255, 255, 255, 255))
    );
}

#[test]
fn rimg_scales_solid_blocks() {
    let (mut backend, surface) = img_backend();
    let solid_red: Vec<u8> = [
        255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
    ]
    .to_vec();
    let id = oppa::ImageId(2);
    backend.insert_image(id, 2, 2, solid_red);
    backend
        .paint(surface, &rimg_plan(0.0, 0.0, 4.0, 4.0, id))
        .expect("paint");
    for (x, y) in [(0, 0), (3, 0), (0, 3), (3, 3), (1, 2)] {
        assert_eq!(
            backend.pixel_rgba(surface, x, y),
            Some((255, 0, 0, 255)),
            "solid block scales filter-proof at ({x},{y})"
        );
    }
}

#[test]
fn rimg_unregistered_refuses_loudly() {
    let (mut backend, surface) = img_backend();
    let err = backend
        .paint(surface, &rimg_plan(0.0, 0.0, 2.0, 2.0, oppa::ImageId(99)))
        .expect_err("unregistered image refuses");
    assert!(
        err.to_string().contains("not registered"),
        "pending-specific refusal, never a placeholder: {err}"
    );
}

#[test]
#[should_panic(expected = "bytes !=")]
fn insert_image_rejects_length_mismatch() {
    let (mut backend, _) = img_backend();
    backend.insert_image(oppa::ImageId(3), 2, 2, vec![0u8; 15]);
}

#[test]
#[should_panic(expected = "zero size")]
fn insert_image_rejects_zero_size() {
    let (mut backend, _) = img_backend();
    backend.insert_image(oppa::ImageId(4), 0, 2, vec![]);
}

// ---------------------------------------------------------------------------
// G10 DPR oracle: the same text scene at dpr 1.0 vs 2.0 doubles
// geometry end to end (layout boxes in device px, CPU ink pixels) —
// the engine-side proof every shell reporter feeds.
// ---------------------------------------------------------------------------

/// One text leaf ("Hi") laid out + painted at `dpr` on a surface
/// sized for it. Returns (text box width px, ink pixel count,
/// first ink column x).
fn dpr_text_probe(dpr: f32) -> (f32, usize, u32) {
    let (fake, _) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    let (w_css, h_css) = (120.0, 60.0);
    host.set_viewport(w_css, h_css);
    host.set_layout_config(oppa::LayoutTextConfig {
        device_pixel_ratio: dpr,
        ..Default::default()
    });
    fn render_hi(_ctx: &Ctx, _props: &()) -> VNode {
        Text {
            text: Arc::from("Hi"),
            style: Text::title_small,
        }
        .into()
    }
    host.mount("hi", (), render_hi);
    host.run_until_idle();
    let id = oppa::find_retained_by_debug(&host, "text")[0];
    let text_w = host.committed_box(id).expect("text box").w;
    let (sw, sh) = ((w_css * dpr) as u32, (h_css * dpr) as u32);
    let mut backend = CpuBackend::new();
    let surface = backend
        .create_surface(oppa::SurfaceDesc {
            width_px: sw,
            height_px: sh,
            background: SURFACE_BG,
        })
        .expect("surface");
    let builder = FramePlanBuilder::new(dpr);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    backend.paint(surface, &plan).expect("paint");
    // Ink census over the surface (legacy bars — no font bytes, the
    // M4 serviceless shape; bars scale with the em size).
    let mut count = 0usize;
    let mut first_x = sw;
    for y in 0..sh {
        for x in 0..sw {
            let (r, g, b, _) = backend.pixel_rgba(surface, x, y).expect("pixel");
            if (r, g, b) != (255, 255, 255) {
                count += 1;
                first_x = first_x.min(x);
            }
        }
    }
    assert!(count > 0, "text inks pixels at dpr {dpr}");
    (text_w, count, first_x)
}

#[test]
fn dpr_two_doubles_geometry_and_quadruples_ink() {
    let (w1, n1, x1) = dpr_text_probe(1.0);
    let (w2, n2, x2) = dpr_text_probe(2.0);
    // FakeText adv = em * 0.625, em = 16 * dpr: "Hi" = 20px vs 40px.
    assert!(
        (w2 - 2.0 * w1).abs() < 1e-2,
        "layout width doubles: {w1} vs {w2}"
    );
    let area = n2 as f32 / n1 as f32;
    assert!(
        (3.0..=5.0).contains(&area),
        "ink area quadruples (~4x): {n1} vs {n2}"
    );
    assert_eq!(x2, 2 * x1, "first ink column doubles: {x1} vs {x2}");
}

// ---------------------------------------------------------------------------
// Round 1.4 (decision 255): overlay portals emit last (top z-layer)
// even when the portal precedes app content in the tree.
// ---------------------------------------------------------------------------

fn render_portal_first(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").children([
        Portal("p").child(
            Div("over")
                .style(Style::new().size(30, 10).bg(Color(0xCC_00_00)))
                .build(),
        ),
        Div("under")
            .style(Style::new().size(30, 10).bg(Color(0x00_00_CC)))
            .build(),
    ])
}

#[test]
fn portal_ops_emit_last_in_tree_order() {
    let host = ComponentHost::new();
    host.set_viewport(VW, VH);
    let _handle = host.mount("layers", (), render_portal_first);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    assert_eq!(plan.ops.len(), 2, "one fill per layer: {plan:?}");
    // Tree order is portal-first, but the portal fill paints last.
    assert!(
        matches!(
            plan.ops[0],
            DrawOp::Rect {
                color: Color(0x00_00_CC),
                ..
            }
        ),
        "app fill first: {:?}",
        plan.ops[0]
    );
    assert!(
        matches!(
            plan.ops[1],
            DrawOp::Rect {
                color: Color(0xCC_00_00),
                ..
            }
        ),
        "portal fill last (top z-layer): {:?}",
        plan.ops[1]
    );
    // A second full build re-emits both (nothing drained, nothing lost).
    let plan2 = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    assert_eq!(plan2.ops.len(), 2, "phase split loses nothing: {plan2:?}");
}

// ---------------------------------------------------------------------------
// Theme contract round (decision 323): the build theme owns the
// default text ink — explicit `Style::ink` wins in both modes.
// ---------------------------------------------------------------------------

fn ink_app(_ctx: &Ctx, _p: &()) -> VNode {
    Div("wrap")
        .style(Style::new().size(120, 60))
        .child(VNode::from(Text::new("hi")))
}

fn text_inks(plan: &FramePlan) -> Vec<Color> {
    plan.ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { ink, .. } => Some(*ink),
            _ => None,
        })
        .collect()
}

/// Unset builds paint exactly the pre-contract plan (Light's
/// `text_primary` IS the contract `INK`), and publishing Dark
/// re-inks every default to the dark `text_primary` — one shared
/// rule for CPU, Vello, and the plan-consuming tests.
#[test]
fn build_theme_owns_the_default_text_ink() {
    let (fake, _) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    host.set_viewport(VW, VH);
    host.mount("ink", (), ink_app);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    assert_eq!(builder.theme_mode(), ThemeMode::Light, "unset is Light");
    let light = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let inks = text_inks(&light);
    assert!(!inks.is_empty(), "the scene carries text");
    assert!(
        inks.iter().all(|i| *i == INK),
        "Light defaults are the contract ink: {inks:?}"
    );
    assert_eq!(
        INK,
        ThemeTokens::light().text_primary,
        "the no-churn identity the contract rests on"
    );
    builder.set_theme_mode(ThemeMode::Dark);
    assert_eq!(builder.theme_mode(), ThemeMode::Dark);
    let dark = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let inks = text_inks(&dark);
    assert!(
        inks.iter().all(|i| *i == ThemeTokens::dark().text_primary),
        "Dark defaults follow the theme: {inks:?}"
    );
    assert_ne!(
        ThemeTokens::dark().text_primary,
        INK,
        "the test spans two inks"
    );
}

fn inked_app(_ctx: &Ctx, _p: &()) -> VNode {
    Div("wrap")
        .style(Style::new().size(120, 60).ink(Color(0x12_34_56)))
        .child(VNode::from(Text::new("hi")))
}

/// An explicit `Style::ink` override wins in both modes (the theme
/// owns only the default — author ink is never re-derived).
#[test]
fn explicit_ink_wins_in_both_themes() {
    let (fake, _) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    host.set_viewport(VW, VH);
    host.mount("inked", (), inked_app);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        builder.set_theme_mode(mode);
        let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
        let inks = text_inks(&plan);
        assert_eq!(inks, vec![Color(0x12_34_56)], "{mode:?} keeps author ink");
    }
}
