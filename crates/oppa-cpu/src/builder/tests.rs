use super::*;
use oppa::{ComponentHost, Ctx, Div, DrawOp, Props, RichText, TextService, TextSpan, VNode};

/// Uniform-advance fake (the m3/m4 rig shape): one cluster per
/// byte, advance = em × 0.625. Weight/family ignored (ink is
/// layout-independent — spans shape identically, paint differs).
struct FakeText;

impl TextService for FakeText {
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
        let n = text.len();
        Ok(oppa::ShapedRun {
            glyphs: (0..n)
                .map(|_| oppa::ShapedGlyph {
                    glyph_id: 0,
                    x_advance: adv,
                    x_offset: 0.0,
                    y_offset: 0.0,
                })
                .collect(),
            runs: vec![oppa::TextRun {
                byte_range: (0, n),
                glyph_range: (0, n),
                rtl: false,
                script: 0,
                font_id: oppa::FontId(0),
                font_metrics: oppa::FontMetrics {
                    ascent: em * 0.8,
                    descent: em * 0.2,
                    line_gap: 0.0,
                },
            }],
            clusters: (0..n)
                .map(|i| oppa::Cluster {
                    byte_range: (i, i + 1),
                    glyph_range: (i, i + 1),
                })
                .collect(),
            total_advance: adv * n as f32,
            text_len_bytes: n,
        })
    }
}

#[derive(Clone)]
struct RichProps;
impl Props for RichProps {}

fn rich_scene(ctx: &Ctx, _: &RichProps) -> VNode {
    let _ = ctx.signal(0u32);
    Div("wrap").child(
        RichText::new(vec![
            TextSpan::new("ab").ink(oppa::Color(0xFF_00_00)),
            TextSpan::new("cd"),
        ])
        .into(),
    )
}

/// Phase 36 PR3: a two-ink line emits one `DrawOp::Text` per ink
/// run (red span + node-ink span); a single-ink line keeps exactly
/// one op (existing scenes stay byte-identical).
#[test]
fn mixed_ink_line_splits_text_ops_per_ink() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.set_text_service(Box::new(FakeText));
    host.mount("Rich", RichProps, rich_scene);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let texts: Vec<&DrawOp> = plan
        .ops
        .iter()
        .filter(|op| matches!(op, DrawOp::Text { .. }))
        .collect();
    assert_eq!(texts.len(), 2, "one op per ink run, got {}", texts.len());
    let inks: Vec<oppa::Color> = texts
        .iter()
        .map(|op| match op {
            DrawOp::Text { ink, .. } => *ink,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(inks[0], oppa::Color(0xFF_00_00), "span ink first");
    assert_ne!(inks[1], inks[0], "node ink resolves separately");
    // Glyph coverage splits 2 + 2 across the ops.
    let counts: Vec<usize> = texts
        .iter()
        .map(|op| match op {
            DrawOp::Text { glyphs, .. } => glyphs.len(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(counts, vec![2, 2]);
}

#[derive(Clone)]
struct CanvasProps;
impl Props for CanvasProps {}

fn canvas_scene(ctx: &Ctx, _: &CanvasProps) -> VNode {
    let _ = ctx.signal(0u32);
    Div("wrap").child(
        oppa::Canvas::new("plot")
            .style(oppa::Style::new().size(100, 50))
            .rect(0.0, 0.0, 100.0, 50.0, oppa::Color(0xFF_00_00))
            .path(
                "M 0 0 L 10 10",
                None,
                Some(oppa::StrokeDesc {
                    color: oppa::Color(0x00_00_00),
                    width: 2.0,
                }),
            )
            .build(),
    )
}

/// Phase 36 PR4 (decision 358): canvas spec ops lower to shape
/// ops at the committed box origin (no new `DrawOp`s); style
/// paint on a canvas refuses loudly.
#[test]
fn canvas_spec_lowers_to_translated_shape_ops() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.mount("Plot", CanvasProps, canvas_scene);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let rects: Vec<&DrawOp> = plan
        .ops
        .iter()
        .filter(|op| matches!(op, DrawOp::Rect { .. }))
        .collect();
    assert_eq!(rects.len(), 1, "one canvas rect, got {}", rects.len());
    assert!(
        matches!(
            rects[0],
            DrawOp::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 50.0,
                ..
            }
        ),
        "box-origin translation: {:?}",
        rects[0]
    );
    assert_eq!(
        plan.ops
            .iter()
            .filter(|op| matches!(op, DrawOp::Path { .. }))
            .count(),
        1,
        "canvas path lowers"
    );
    assert!(
        plan.ops
            .iter()
            .filter(|op| matches!(op, DrawOp::Text { .. }))
            .count()
            == 0,
        "no text ops without text (serviceless)"
    );
}

/// Phase 36 PR4 (decision 359): `insert_cached` pulls one cache
/// deposit; undeposited ids refuse loudly at deposit time.
#[test]
fn insert_cached_pulls_one_deposit_and_refuses_when_absent() {
    use crate::backend::CpuBackend;
    let cache = oppa::ImageCache::new();
    let rgba = vec![255u8; 2 * 2 * 4];
    let id = cache.insert_pixels("dot", 2, 2, rgba);
    let mut be = CpuBackend::new();
    be.insert_cached(&cache, id);
    be.remove_image(id);
}

#[test]
#[should_panic(expected = "no pre-decoded pixels")]
fn insert_cached_refuses_undeposited_ids() {
    use crate::backend::CpuBackend;
    let cache = oppa::ImageCache::new();
    let id = cache.load("img/ghost.png");
    CpuBackend::new().insert_cached(&cache, id);
}

#[derive(Clone)]
struct BlurProps;
impl Props for BlurProps {}

fn blur_scene(ctx: &Ctx, _: &BlurProps) -> VNode {
    let _ = ctx.signal(0u32);
    Div("wrap").child(
        Div("box")
            .style(
                oppa::Style::new()
                    .size(20, 20)
                    .x(10.0)
                    .shadow(0, 0, oppa::Color(0x00_00_00))
                    .shadow_blur(3),
            )
            .build(),
    )
}

/// Phase 36 PR4 (decision 356): the blurred shadow paints soft —
/// solid core, partial fringe, transparent far field (never the
/// old stepped solids, never a hard edge).
#[test]
fn blurred_shadow_paints_soft_core_and_fringe() {
    use crate::backend::CpuBackend;
    use oppa::RendererBackend;
    let host = ComponentHost::new();
    host.set_viewport(60.0, 40.0);
    host.mount("Blur", BlurProps, blur_scene);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    assert_eq!(
        plan.ops
            .iter()
            .filter(|op| matches!(op, DrawOp::Shadow { .. }))
            .count(),
        1,
        "one native blur op"
    );
    let mut be = CpuBackend::new();
    let s = be
        .create_surface(oppa::SurfaceDesc {
            width_px: 60,
            height_px: 40,
            background: oppa::Color(0xFF_FF_FF),
        })
        .expect("surface");
    be.paint(s, &plan).expect("paint");
    let px = |x: u32, y: u32| be.pixel_rgba(s, x, y).expect("pixel");
    let (r, g, b, _) = px(20, 10);
    assert!(
        r < 64 && g < 64 && b < 64,
        "core near-black, got ({r}, {g}, {b})"
    );
    assert_eq!(px(0, 0), (255, 255, 255, 255), "far field stays white");
    let (er, _, _, _) = px(33, 10);
    assert!(
        er > 0 && er < 255,
        "fringe partial (soft edge, not a step), got {er}"
    );
    assert_eq!(px(50, 10).0, 255, "past the spread stays white");
}

fn bad_canvas_scene(ctx: &Ctx, _: &CanvasProps) -> VNode {
    let _ = ctx.signal(0u32);
    // Hand-built canvas with a child (the builder refuses
    // children — canvas leaves are childless by construction).
    VNode::Element(Box::new(oppa::Element {
        tag: oppa::Tag::Canvas,
        debug: "bad-canvas".to_string(),
        key: None,
        style: oppa::Style::new().size(100, 50).build(),
        text_hint: None,
        image: None,
        path: None,
        canvas: Some(oppa::CanvasSpec { ops: Vec::new() }),
        semantics: None,
        handlers: Vec::new(),
        children: vec![VNode::Text("x".into())],
    }))
}

/// Phase 36 PR4: canvas children refuse loudly at layout (the
/// leaf paints its spec — children would double-paint).
#[test]
#[should_panic(expected = "childless by construction")]
fn canvas_children_refuse_loudly() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.mount("BadCanvas", CanvasProps, bad_canvas_scene);
    host.run_until_idle();
}
