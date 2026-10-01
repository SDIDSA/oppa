//! Vector paths end to end (decision 291): `Path` VNode → retained
//! payload → [`DrawOp::Path`] → CPU pixels. Fill proves exact
//! interior/exterior pixels (deep-interior coverage is exact under
//! any correct winding rasterizer); stroke proves presence along
//! the centerline; invalid data fails loudly; builder-side payload
//! gaps refuse loudly.

use oppa::{
    Color, ComponentHost, Ctx, DrawOp, FramePlan, NodeId, Path, PlanStats, Props, RendererBackend,
    VNode,
};
use oppa_cpu::{CpuBackend, FramePlanBuilder};

const WHITE: (u8, u8, u8, u8) = (255, 255, 255, 255);
const RED: (u8, u8, u8, u8) = (255, 0, 0, 255);

#[derive(Clone)]
struct Tri;
impl Props for Tri {}

/// Root-level triangle: 60×60 leaf, fill only.
fn render_tri(_ctx: &Ctx, _p: &Tri) -> VNode {
    Path::new("tri")
        .data("M 5 5 L 55 5 L 30 55 Z")
        .fill(Color(0xFF_00_00))
        .size(60, 60)
        .build()
}

fn mount_tri() -> ComponentHost {
    let host = ComponentHost::new();
    host.set_viewport(60.0, 60.0);
    host.mount("Tri", Tri, render_tri);
    host.run_until_idle();
    host
}

fn paint_full(host: &ComponentHost, cpu: &mut CpuBackend, surf: oppa::SurfaceId) {
    for diff in host.diffs_from(0) {
        cpu.commit(&diff).expect("cpu commits");
    }
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    cpu.paint(surf, &plan).expect("cpu paints");
}

fn white_surface(cpu: &mut CpuBackend) -> oppa::SurfaceId {
    cpu.create_surface(oppa::SurfaceDesc {
        width_px: 60,
        height_px: 60,
        background: Color(0xFF_FF_FF),
    })
    .expect("surface builds")
}

/// The builder emits one `DrawOp::Path` with the committed origin,
/// size, payload, and baked opacity.
#[test]
fn builder_emits_path_with_box_and_payload() {
    let host = mount_tri();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    assert_eq!(plan.ops.len(), 1, "one path op, got {:?}", plan.ops);
    match &plan.ops[0] {
        DrawOp::Path {
            x,
            y,
            width,
            height,
            data,
            fill,
            stroke,
            opacity,
            ..
        } => {
            assert_eq!((*x, *y), (0.0, 0.0), "root origin");
            assert_eq!((*width, *height), (60.0, 60.0), "explicit size");
            assert_eq!(data.as_ref(), "M 5 5 L 55 5 L 30 55 Z");
            assert_eq!(*fill, Some(Color(0xFF_00_00)));
            assert_eq!(*stroke, None);
            assert_eq!(*opacity, 1.0);
        }
        other => panic!("expected Path, got {other:?}"),
    }
}

/// Fill paints exact red strictly inside the triangle
/// (centroid (30, 21.7) — at y=22 the span is x 13.5..46.5, so
/// (30, 22) is 8+ px from any edge, past every AA fringe) and
/// leaves the corners surface-white.
#[test]
fn triangle_fill_paints_interior_and_spares_exterior() {
    let host = mount_tri();
    let mut cpu = CpuBackend::new();
    let surf = white_surface(&mut cpu);
    paint_full(&host, &mut cpu, surf);
    assert_eq!(
        cpu.pixel_rgba(surf, 30, 22),
        Some(RED),
        "deep-interior pixel is exact fill"
    );
    for (x, y) in [(2, 58), (57, 57), (2, 2), (57, 2)] {
        assert_eq!(
            cpu.pixel_rgba(surf, x, y),
            Some(WHITE),
            "exterior pixel ({x}, {y}) stays surface-white"
        );
    }
    // The fill covers the 1250-px triangle (edge AA fringes blend
    // toward white along the diagonals — correct coverage, not
    // strays — so solid-red pixels land near the area, never near
    // zero and never flooding the surface).
    let mut solid = 0;
    for p in cpu.pixmap(surf).expect("pixmap").pixels() {
        if p.red() > 200 && p.green() < 100 && p.blue() < 100 {
            solid += 1;
        }
    }
    assert!(
        (900..=1600).contains(&solid),
        "solid fill near the 1250-px area, got {solid}"
    );
}

/// A stroked check paints white along its centerline (2.5px round
/// stroke covers centerline pixels fully) and leaves far corners
/// Primary. Same geometry the Checkbox ships (decision 291).
#[test]
fn check_stroke_paints_centerline_and_spares_corners() {
    const PRIMARY: (u8, u8, u8, u8) = (0x22, 0x66, 0xCC, 255);
    let plan = FramePlan {
        viewport_w: 20.0,
        viewport_h: 20.0,
        ops: vec![
            DrawOp::Rect {
                node: NodeId::new(1, 0),
                x: 0.0,
                y: 0.0,
                w: 20.0,
                h: 20.0,
                color: Color(0x22_66_CC),
                opacity: 1.0,
            },
            DrawOp::Path {
                node: NodeId::new(1, 1),
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 20.0,
                data: "M 4.5 10.5 L 8.5 14.5 L 15.5 6".into(),
                fill: None,
                stroke: Some(oppa::StrokeDesc {
                    color: Color(0xFF_FF_FF),
                    width: 2.5,
                }),
                opacity: 1.0,
            },
        ],
        damage: vec![],
        stats: PlanStats::default(),
        full_repaint: true,
    };
    let mut cpu = CpuBackend::new();
    let surf = cpu
        .create_surface(oppa::SurfaceDesc {
            width_px: 20,
            height_px: 20,
            background: Color(0xFF_FF_FF),
        })
        .expect("surface builds");
    cpu.paint(surf, &plan).expect("paints");
    // Fully-covered probes (every pixel-square corner within the
    // 1.25px half-width, so analytic AA covers them exactly):
    // on-segment (6,12), inside the round join disc (8,14),
    // on-segment (11,11).
    for (x, y) in [(6, 12), (8, 14), (11, 11)] {
        assert_eq!(
            cpu.pixel_rgba(surf, x, y),
            Some(WHITE),
            "stroke centerline ({x}, {y}) paints white"
        );
    }
    for (x, y) in [(2, 2), (17, 17), (10, 4)] {
        assert_eq!(
            cpu.pixel_rgba(surf, x, y),
            Some(PRIMARY),
            "far corner ({x}, {y}) stays Primary fill"
        );
    }
}

/// Invalid path data fails the paint loudly (no partial paint — the
/// surface keeps proving it by staying paintable after).
#[test]
fn invalid_path_data_fails_loudly() {
    let plan = FramePlan {
        viewport_w: 10.0,
        viewport_h: 10.0,
        ops: vec![DrawOp::Path {
            node: NodeId::new(1, 0),
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            data: "M 0 0 X 1 1".into(),
            fill: Some(Color(0xFF_00_00)),
            stroke: None,
            opacity: 1.0,
        }],
        damage: vec![],
        stats: PlanStats::default(),
        full_repaint: true,
    };
    let mut cpu = CpuBackend::new();
    let surf = cpu
        .create_surface(oppa::SurfaceDesc {
            width_px: 10,
            height_px: 10,
            background: Color(0xFF_FF_FF),
        })
        .expect("surface builds");
    let err = cpu.paint(surf, &plan).expect_err("invalid data refuses");
    assert!(
        err.to_string().contains("invalid SVG path data"),
        "names the cause, got {err}"
    );
}

/// Style paint fields never ride a Path node (the builder refuses
/// before any backend could diverge on them).
#[test]
#[should_panic(expected = "bg on a Path node")]
fn style_bg_on_path_panics_loudly() {
    use oppa::{Div, Style};
    #[derive(Clone)]
    struct P;
    impl Props for P {}
    fn render(_ctx: &Ctx, _p: &P) -> VNode {
        // Hand-built Path element carrying a style bg (the `Path`
        // component never produces this — the builder still refuses).
        let mut vnode = Path::new("bad")
            .data("M 0 0 L 1 1 Z")
            .fill(Color(1))
            .size(4, 4)
            .build();
        if let VNode::Element(ref mut e) = vnode {
            e.style = Style::new().size(4, 4).bg(Color(2)).build();
        }
        Div("wrap").child(vnode)
    }
    let host = ComponentHost::new();
    host.mount("P", P, render);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let _ = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
}
