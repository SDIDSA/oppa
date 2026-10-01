//! Round 1.3 acceptance (decision 254) as superseded by Phase 36 PR4
//! (decision 356): visual styling primitives expand in the shared
//! FramePlan builder — blurred shadows emit one native-blur
//! [`DrawOp::Shadow`] (Vello gaussian, CPU box-blur, CSS box-shadow),
//! per-edge border bands, and linear-gradient strips — with loud
//! conflicts, all without touching any backend (no new `DrawOp`).
//!
//! Div-only scenes (no text service needed): the root fills the 120x60
//! viewport; the styled child sits at the origin.

use oppa::{Color, ComponentHost, Ctx, Div, DrawOp, Style, VNode};
use oppa_cpu::FramePlanBuilder;

fn plan_of(render: fn(&Ctx, &()) -> VNode) -> Vec<DrawOp> {
    let host = ComponentHost::new();
    host.set_viewport(120.0, 60.0);
    let _handle = host.mount("fx", (), render);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    host.with_retained_mut(|rec, styles| builder.build_full(rec, styles))
        .ops
}

fn render_blur(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").child(
        Div("fx")
            .style(
                Style::new()
                    .size(40, 20)
                    .shadow(2, 3, Color(0x00_00_00))
                    .shadow_blur(2),
            )
            .build(),
    )
}

#[test]
fn blurred_shadow_emits_one_native_blur_op() {
    // Phase 36 PR4 (decision 356, supersedes the stepped expansion):
    // blur 2 (dpr 1) emits one `Shadow` op carrying the radius —
    // backends blur natively (Vello gaussian, CPU box-blur, CSS).
    let ops = plan_of(render_blur);
    assert_eq!(ops.len(), 1, "one op, no expansion rects: {ops:?}");
    assert!(
        matches!(
            ops[0],
            DrawOp::Shadow {
                x: 0.0,
                y: 0.0,
                w: 40.0,
                h: 20.0,
                dx: 2.0,
                dy: 3.0,
                blur_radius: 2.0,
                ..
            }
        ),
        "native blur op carries geometry + radius: {:?}",
        ops[0]
    );
    assert!(
        matches!(
            ops[0],
            DrawOp::Shadow {
                color: Color(0x00_00_00),
                ..
            }
        ),
        "op carries the shadow color: {:?}",
        ops[0]
    );
}

fn render_blur_zero(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").child(
        Div("fx")
            .style(Style::new().size(40, 20).shadow(2, 3, Color(0x00_00_00)))
            .build(),
    )
}

#[test]
fn zero_blur_keeps_the_single_shadow_op() {
    let ops = plan_of(render_blur_zero);
    assert_eq!(ops.len(), 1, "one op: {ops:?}");
    assert!(
        matches!(
            ops[0],
            DrawOp::Shadow {
                x: 0.0,
                y: 0.0,
                w: 40.0,
                h: 20.0,
                dx: 2.0,
                dy: 3.0,
                ..
            }
        ),
        "shipped offset solid unchanged: {:?}",
        ops[0]
    );
}

fn render_edges(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").child(
        Div("fx")
            .style(
                Style::new()
                    .size(40, 20)
                    .border_edges(4, 0, 2, 6, Color(0xAA_BB_CC)),
            )
            .build(),
    )
}

#[test]
fn per_edge_bands_span_without_overlap() {
    let ops = plan_of(render_edges);
    // Top full-width, bottom full-width, left between them; right is 0.
    assert_eq!(ops.len(), 3, "top + bottom + left: {ops:?}");
    assert!(
        matches!(
            ops[0],
            DrawOp::Rect {
                x: 0.0,
                y: 0.0,
                w: 40.0,
                h: 4.0,
                ..
            }
        ),
        "top band: {:?}",
        ops[0]
    );
    assert!(
        matches!(
            ops[1],
            DrawOp::Rect {
                x: 0.0,
                y: 18.0,
                w: 40.0,
                h: 2.0,
                ..
            }
        ),
        "bottom band: {:?}",
        ops[1]
    );
    assert!(
        matches!(
            ops[2],
            DrawOp::Rect {
                x: 0.0,
                y: 4.0,
                w: 6.0,
                h: 14.0,
                ..
            }
        ),
        "left band sits between: {:?}",
        ops[2]
    );
}

fn render_gradient(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").child(
        Div("fx")
            .style(
                Style::new()
                    .size(10, 6)
                    .bg_gradient(Color(0x00_00_00), Color(0xFF_FF_FF)),
            )
            .build(),
    )
}

#[test]
fn vertical_gradient_strips_span_exactly() {
    let ops = plan_of(render_gradient);
    // 6px span -> 6 one-px strips, first near-black, last near-white.
    assert_eq!(ops.len(), 6, "one strip per device px: {ops:?}");
    let mut y = 0.0f32;
    for (k, op) in ops.iter().enumerate() {
        match op {
            DrawOp::Rect {
                x: 0.0,
                y: oy,
                w: 10.0,
                h: 1.0,
                ..
            } => {
                assert_eq!(*oy, y, "strip {k} stacks exactly");
                y += 1.0;
            }
            other => panic!("strip {k} is a full-width 1px rect: {other:?}"),
        }
    }
    let first = match ops[0] {
        DrawOp::Rect { color, .. } => color,
        ref other => panic!("{other:?}"),
    };
    let last = match ops[5] {
        DrawOp::Rect { color, .. } => color,
        ref other => panic!("{other:?}"),
    };
    // t = (k+0.5)/6: 255/12 = 21.25 -> 21; 255*11/12 = 233.75 -> 234.
    assert_eq!(first, Color(0x15_15_15), "first strip lerped: {first:?}");
    assert_eq!(last, Color(0xEA_EA_EA), "last strip lerped: {last:?}");
}

fn render_gradient_horizontal(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").child(
        Div("fx")
            .style(
                Style::new()
                    .size(6, 4)
                    .bg_gradient_horizontal(Color(0x00_00_00), Color(0xFF_00_00)),
            )
            .build(),
    )
}

#[test]
fn horizontal_gradient_runs_left_to_right() {
    let ops = plan_of(render_gradient_horizontal);
    assert_eq!(ops.len(), 6, "one strip per device px: {ops:?}");
    let first = match ops[0] {
        DrawOp::Rect { color, .. } => color,
        ref other => panic!("{other:?}"),
    };
    let last = match ops[5] {
        DrawOp::Rect { color, .. } => color,
        ref other => panic!("{other:?}"),
    };
    assert_eq!(first.0 & 0xFF_00_00, 0x15_00_00, "red ramps up: {first:?}");
    assert_eq!(last.0 & 0xFF_00_00, 0xEA_00_00, "red ramps up: {last:?}");
    assert!(
        matches!(
            ops[0],
            DrawOp::Rect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 4.0,
                ..
            }
        ),
        "first strip is the left column: {:?}",
        ops[0]
    );
}

fn render_ring_gradient(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").child(
        Div("fx")
            .style(
                Style::new()
                    .size(10, 10)
                    .border(2, Color(0xAA_BB_CC))
                    .bg_gradient(Color(0x00_00_00), Color(0xFF_FF_FF)),
            )
            .build(),
    )
}

#[test]
fn uniform_ring_wraps_inset_gradient_strips() {
    let ops = plan_of(render_ring_gradient);
    // Ring outer + 6 inset strips (6px span at x=2).
    assert_eq!(ops.len(), 7, "ring + 6 strips: {ops:?}");
    assert!(
        matches!(
            ops[0],
            DrawOp::Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
                color: Color(0xAA_BB_CC),
                ..
            }
        ),
        "ring outer first: {:?}",
        ops[0]
    );
    assert!(
        matches!(
            ops[1],
            DrawOp::Rect {
                x: 2.0,
                y: 2.0,
                w: 6.0,
                h: 1.0,
                ..
            }
        ),
        "strips inset by the ring: {:?}",
        ops[1]
    );
}

fn render_border_conflict(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").child(
        Div("fx")
            .style(
                Style::new()
                    .size(10, 10)
                    .border(1, Color(1))
                    .border_top(1, Color(1)),
            )
            .build(),
    )
}

#[test]
#[should_panic(expected = "border + border_edges")]
fn border_plus_edges_refuses_loudly() {
    let _ = plan_of(render_border_conflict);
}

fn render_bg_conflict(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").child(
        Div("fx")
            .style(
                Style::new()
                    .size(10, 10)
                    .bg(Color(1))
                    .bg_gradient(Color(1), Color(2)),
            )
            .build(),
    )
}

#[test]
#[should_panic(expected = "bg + bg_gradient")]
fn bg_plus_gradient_refuses_loudly() {
    let _ = plan_of(render_bg_conflict);
}

fn render_gradient_radius(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").child(
        Div("fx")
            .style(
                Style::new()
                    .size(10, 10)
                    .radius(3)
                    .bg_gradient(Color(1), Color(2)),
            )
            .build(),
    )
}

#[test]
#[should_panic(expected = "radius/circle")]
fn gradient_with_radius_refuses_loudly() {
    let _ = plan_of(render_gradient_radius);
}

fn render_edges_circle(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").child(
        Div("fx")
            .style(Style::new().size(10, 10).circle().border_top(1, Color(1)))
            .build(),
    )
}

#[test]
#[should_panic(expected = "radius/circle")]
fn edges_with_circle_refuses_loudly() {
    let _ = plan_of(render_edges_circle);
}

fn render_negative_blur(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").child(
        Div("fx")
            .style(
                Style::new()
                    .size(10, 10)
                    .shadow(1, 1, Color(1))
                    .shadow_blur(-2),
            )
            .build(),
    )
}

#[test]
#[should_panic(expected = "negative")]
fn negative_blur_refuses_loudly() {
    let _ = plan_of(render_negative_blur);
}

fn render_negative_edge(_ctx: &Ctx, _: &()) -> VNode {
    Div("root").child(
        Div("fx")
            .style(Style::new().size(10, 10).border_top(-1, Color(1)))
            .build(),
    )
}

#[test]
#[should_panic(expected = "negative")]
fn negative_edge_refuses_loudly() {
    let _ = plan_of(render_negative_edge);
}
