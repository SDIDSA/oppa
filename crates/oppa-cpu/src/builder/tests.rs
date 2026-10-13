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

/// Decision 477: a published palette override recolors the
/// resolved default text ink (plain text with no span ink paints
/// the override's `text_primary`); unset resolves through the mode
/// exactly as before.
#[test]
fn custom_tokens_recolor_resolved_default_ink() {
    use oppa::{Text, ThemeMode, ThemeTokens};
    #[derive(Clone)]
    struct PlainProps;
    impl Props for PlainProps {}
    fn plain_scene(ctx: &Ctx, _: &PlainProps) -> VNode {
        let _ = ctx.signal(0u32);
        Div("wrap").child(VNode::from(Text::new("hi").size(14)))
    }
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.set_text_service(Box::new(FakeText));
    host.mount("Plain", PlainProps, plain_scene);
    host.run_until_idle();
    let ink_of = |host: &ComponentHost| {
        let builder = FramePlanBuilder::new(1.0);
        builder.set_theme_tokens(host.theme_tokens());
        let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
        plan.ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Text { ink, .. } => Some(*ink),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ink_of(&host),
        vec![ThemeTokens::of(ThemeMode::Light).text_primary],
        "unset override resolves through the mode"
    );
    let custom = ThemeTokens {
        text_primary: oppa::Color(0x12_34_56),
        ..ThemeTokens::of(ThemeMode::Light)
    };
    host.set_theme_tokens_override(ThemeMode::Light, Some(custom));
    host.run_until_idle();
    assert_eq!(
        ink_of(&host),
        vec![custom.text_primary],
        "published override recolors resolved ink"
    );
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

// ---------------------------------------------------------------------------
// Decision 511: transform bake (translate/scale fold into op geometry)
// ---------------------------------------------------------------------------

use oppa::{Ease, FramePlan, MockClock, Signal, Style, Text, Transform, Transition};
use std::rc::Rc;

#[derive(Clone)]
struct XfProps {
    xf: Option<Transform>,
}
impl Props for XfProps {}

fn xf_scene(_ctx: &Ctx, p: &XfProps) -> VNode {
    Div("root").style(Style::new().size(800, 600)).child(
        Div("box")
            .style(
                Style::new()
                    .size(100, 50)
                    .bg(oppa::Color(0x11_22_33))
                    .transform(p.xf),
            )
            .build(),
    )
}

fn box_plan(xf: Option<Transform>) -> (ComponentHost, FramePlan, oppa::NodeId) {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.mount("Xf", XfProps { xf }, xf_scene);
    host.run_until_idle();
    let id = oppa::find_retained_by_debug(&host, "box")[0];
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    (host, plan, id)
}

fn rect_of(plan: &FramePlan, id: oppa::NodeId) -> (f32, f32, f32, f32) {
    plan.ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Rect {
                node, x, y, w, h, ..
            } if *node == id => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .expect("box rect op")
}

/// Decision 511: identity is byte-identical to unset (the fast path
/// touches no numbers).
#[test]
fn identity_transform_plan_matches_untransformed() {
    let (_, plain, _) = box_plan(None);
    let (_, ident, _) = box_plan(Some(Transform::identity()));
    assert_eq!(plain.ops, ident.ops, "identity emits identical ops");
    assert_eq!(
        plain.damage, ident.damage,
        "identity emits identical damage"
    );
}

/// Decision 511: translate offsets geometry and damage exactly.
#[test]
fn translate_bakes_into_rect_geometry_and_damage() {
    let (_, plan, id) = box_plan(Some(Transform::translate(30, -10)));
    assert_eq!(rect_of(&plan, id), (30.0, -10.0, 100.0, 50.0));
    assert!(
        plan.damage
            .iter()
            .any(|d| (d.x, d.y, d.w, d.h) == (30.0, -10.0, 100.0, 50.0)),
        "damage covers the transformed AABB, got {:?}",
        plan.damage
    );
}

/// Decision 511: scale applies about the box center (100×50 at
/// origin × (2, 1) → x −50, w 200, y/h unchanged).
#[test]
fn scale_bakes_about_box_center() {
    let (_, plan, id) = box_plan(Some(Transform::scale(2, 1)));
    assert_eq!(rect_of(&plan, id), (-50.0, 0.0, 200.0, 50.0));
    let (_, plan, id) = box_plan(Some(Transform::scale(0.5, 0.5)));
    assert_eq!(rect_of(&plan, id), (25.0, 12.5, 50.0, 25.0));
}

#[derive(Clone)]
struct NestProps;
impl Props for NestProps {}

fn nest_scene(ctx: &Ctx, _: &NestProps) -> VNode {
    let _ = ctx.signal(0u32);
    Div("root").style(Style::new().size(800, 600)).child(
        Div("par")
            .style(
                Style::new()
                    .size(400, 400)
                    .bg(oppa::Color(0x11_11_11))
                    .transform(Transform::translate(10, 0)),
            )
            .child(
                Div("kid")
                    .style(
                        Style::new()
                            .size(50, 20)
                            .bg(oppa::Color(0x22_22_22))
                            .transform(Transform::translate(0, 20)),
                    )
                    .build(),
            ),
    )
}

/// Decision 511: nested transforms accumulate child-inside-parent
/// (kid at parent (10, 0) + own (0, 20) → (10, 20)).
#[test]
fn nested_transforms_accumulate_child_inside_parent() {
    let host = ComponentHost::new();
    host.set_viewport(800.0, 600.0);
    host.mount("Nest", NestProps, nest_scene);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let par = oppa::find_retained_by_debug(&host, "par")[0];
    let kid = oppa::find_retained_by_debug(&host, "kid")[0];
    assert_eq!(rect_of(&plan, par), (10.0, 0.0, 400.0, 400.0));
    assert_eq!(rect_of(&plan, kid), (10.0, 20.0, 50.0, 20.0));
}

/// Decision 511: the bake samples evaluator-live values (mid-flight
/// paints mid-flight geometry through the same channel as bg).
#[test]
fn evaluated_mid_flight_transform_bakes_live_geometry() {
    use oppa::MsExt;
    #[derive(Clone)]
    struct MvProps {
        dx: Signal<f32>,
    }
    impl Props for MvProps {}
    fn mv_scene(_ctx: &Ctx, p: &MvProps) -> VNode {
        let dx = p.dx.get();
        Div("root").style(Style::new().size(800, 600)).child(
            Div("box")
                .style(
                    Style::new()
                        .size(100, 50)
                        .bg(oppa::Color(0x11_22_33))
                        .transform(Transform::translate(dx, 0))
                        .transition(Transition::new(120.ms(), Ease::Linear)),
                )
                .build(),
        )
    }
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(800.0, 600.0);
    let dx = host.runtime().signal(0.0f32);
    host.mount("Mv", MvProps { dx: dx.clone() }, mv_scene);
    host.run_until_idle();
    dx.set(100.0);
    // The M8 pattern: `run_until_idle` spins forever under a frozen
    // clock while the tween lives, so clocked tests step `run_once`
    // vsync-by-vsync (the controls `settle_clocked` rule).
    for _ in 0..4 {
        clock.advance(1.0 / 60.0);
        host.run_once();
    }
    let now = clock.get();
    let builder = FramePlanBuilder::new(1.0);
    let id = oppa::find_retained_by_debug(&host, "box")[0];
    let plan = host.with_retained_mut(|rec, styles| {
        host.with_evaluator(|ev| builder.build_full_evaluated(rec, styles, ev, now))
    });
    let x = rect_of(&plan, id).0;
    // The bake paints exactly what the evaluator resolves (start-time
    // mechanics belong to the pump, not the bake — coherence is the
    // contract here).
    let want = host.with_evaluator(|ev| ev.resolve_transform(id, None, now).dx.get());
    assert!(
        (x - want).abs() < 1e-3,
        "bake paints the live sample ({x} vs {want} at t={now})"
    );
    assert!(
        (0.0 < want) && (want < 100.0),
        "mid-flight sample is strictly interior, got {want}"
    );
}

/// Decision 511: rotate and degenerate scales refuse at build (the
/// T1b line — never a silent snap).
#[test]
#[should_panic(expected = "T1b scope")]
fn rotate_refuses_loudly_at_build() {
    let (_, plan, _) = box_plan(Some(Transform::rotate(30)));
    let _ = plan;
}

#[test]
#[should_panic(expected = "scale must be positive")]
fn zero_scale_refuses_loudly_at_build() {
    let (_, plan, _) = box_plan(Some(Transform::scale(0, 1)));
    let _ = plan;
}

/// Decision 511: `scale_path_data` scales per-axis (H takes sx, V
/// takes sy), keeps arc flags/rotation, and refuses malformed data.
#[test]
fn scale_path_data_scales_per_axis_and_keeps_flags() {
    assert_eq!(
        scale_path_data("M 0 0 L 18 9 Z", 2.0, 2.0),
        "M 0 0 L 36 18 Z"
    );
    assert_eq!(
        scale_path_data("M 0 0 H 10 V 20", 2.0, 3.0),
        "M 0 0 H 20 V 60"
    );
    assert_eq!(scale_path_data("m 1 2 l 3 4", 2.0, 2.0), "m 2 4 l 6 8");
    assert_eq!(
        scale_path_data("M 0 0 C 1 2 3 4 5 6", 2.0, 1.0),
        "M 0 0 C 2 2 6 4 10 6"
    );
    assert_eq!(
        scale_path_data("M 0 0 A 5 10 0 0 1 20 20", 2.0, 3.0),
        "M 0 0 A 10 30 0 0 1 40 60"
    );
}

#[test]
#[should_panic(expected = "unknown path command")]
fn scale_path_data_unknown_command_refuses() {
    let _ = scale_path_data("M 0 0 X 1 2", 2.0, 2.0);
}

#[test]
#[should_panic(expected = "leading number")]
fn scale_path_data_leading_number_refuses() {
    let _ = scale_path_data("0 0 M 1 1", 2.0, 2.0);
}

/// Decision 511: text bakes translate exactly and scales advances
/// with `sx` / metrics with `sy` (the anamorphic rule).
#[test]
fn text_bake_translates_and_scales_glyph_geometry() {
    #[derive(Clone)]
    struct TextProps;
    impl Props for TextProps {}
    fn text_inner(ctx: &Ctx, xf: Option<Transform>) -> VNode {
        let _ = ctx.signal(0u32);
        Div("root").style(Style::new().size(800, 600)).child(
            Div("twrap")
                .style(Style::new().size(200, 30).transform(xf))
                .child(VNode::from(Text::new("hi").size(20).build())),
        )
    }
    fn text_scene_plain(ctx: &Ctx, _: &TextProps) -> VNode {
        text_inner(ctx, None)
    }
    fn text_scene_moved(ctx: &Ctx, _: &TextProps) -> VNode {
        text_inner(ctx, Some(Transform::translate(10, 0)))
    }
    fn text_scene_scaled(ctx: &Ctx, _: &TextProps) -> VNode {
        text_inner(ctx, Some(Transform::scale(2, 2)))
    }
    fn text_plan(scene: fn(&Ctx, &TextProps) -> VNode) -> FramePlan {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        host.set_text_service(Box::new(FakeText));
        host.mount("T", TextProps, scene);
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        host.with_retained_mut(|rec, styles| builder.build_full(rec, styles))
    }
    fn text_op(plan: &FramePlan) -> (f32, f32, f32, f32) {
        plan.ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Text {
                    x, glyphs, em_size, ..
                } => Some((*x, glyphs[0].advance, *em_size, glyphs[1].x)),
                _ => None,
            })
            .expect("text op")
    }
    let (px, pa, pe, pg1) = text_op(&text_plan(text_scene_plain));
    let (mx, ma, me, mg1) = text_op(&text_plan(text_scene_moved));
    assert!((mx - px - 10.0).abs() < 1e-3, "text shifts by exactly dx");
    assert_eq!(
        (ma, me, mg1),
        (pa, pe, pg1),
        "pure translate touches no glyph metrics"
    );
    let (_sx, sa, se, sg1) = text_op(&text_plan(text_scene_scaled));
    assert!((sa - pa * 2.0).abs() < 1e-6, "advances scale by sx");
    assert!((se - pe * 2.0).abs() < 1e-6, "em scales by sy");
    assert!((sg1 - pg1 * 2.0).abs() < 1e-6, "cell offsets scale by sx");
}
