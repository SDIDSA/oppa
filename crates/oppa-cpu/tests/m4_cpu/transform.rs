use super::*;

// ---------------------------------------------------------------------------
// Decision 511: transformed paint end to end (baked geometry replays
// through the retained rasterizer — the GPU twin in
// `oppa-gpu/tests/gpu_backend.rs` paints the same plan).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ChipProps;
impl Props for ChipProps {}

fn render_chips(_ctx: &Ctx, _: &ChipProps) -> VNode {
    Div("stage").style(Style::new().size(120, 60)).children([
        Div("chip-move")
            .style(
                Style::new()
                    .size(20, 10)
                    .bg(Color(0xFF_00_00))
                    .transform(Transform::translate(60, 30)),
            )
            .build(),
        Div("chip-grow")
            .style(
                Style::new()
                    .size(10, 8)
                    .bg(Color(0x00_00_FF))
                    .transform(Transform::scale(2, 2)),
            )
            .build(),
        Div("chip-turn")
            .style(
                Style::new()
                    .size(20, 10)
                    .bg(Color(0x00_CC_00))
                    .transform(Transform::rotate(90)),
            )
            .build(),
        Div("chip-turn-round-wrap")
            .style(
                Style::new()
                    .size(20, 10)
                    .transform(Transform::translate(90, 0)),
            )
            .child(
                Div("chip-turn-round")
                    .style(
                        Style::new()
                            .size(20, 10)
                            .bg(Color(0xCC_CC_00))
                            .radius(3)
                            .transform(Transform::rotate(90)),
                    )
                    .build(),
            ),
    ])
}

fn paint_chips() -> CpuBackendRig {
    let host = ComponentHost::new();
    host.set_viewport(120.0, 60.0);
    host.mount("chips", ChipProps, render_chips);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let mut backend = CpuBackend::new();
    let surface = backend
        .create_surface(oppa::SurfaceDesc {
            width_px: 120,
            height_px: 60,
            background: SURFACE_BG,
        })
        .expect("surface");
    backend.paint(surface, &plan).expect("paint");
    CpuBackendRig { backend, surface }
}

struct CpuBackendRig {
    backend: CpuBackend,
    surface: oppa::SurfaceId,
}

#[test]
fn translated_rect_paints_at_visual_location() {
    let rig = paint_chips();
    let px = |x, y| rig.backend.pixel_rgba(rig.surface, x, y).expect("pixel");
    // Translated chip (20×10 at origin + (60,30)): red at the visual
    // spot, background where the layout box was.
    assert_eq!(px(70, 35), (255, 0, 0, 255), "visual location paints");
    assert_eq!(px(5, 5), white(), "layout ghost stays background");
}

#[test]
fn scaled_rect_paints_doubled_about_center() {
    let rig = paint_chips();
    let px = |x, y| rig.backend.pixel_rgba(rig.surface, x, y).expect("pixel");
    // Scaled chip (10×8 at origin ×2 about (5,4) → −5..15 × −4..12,
    // clipped top/left): blue inside the doubled visual.
    assert_eq!(px(10, 6), (0, 0, 255, 255), "doubled visual paints");
}

/// Decision 517: a quarter-turned chip stands upright — the 20×10 box
/// at y 18..28 turns about (10, 23) into the 5..15 × 13..33 visual
/// (temp-layer detour through the axis-aligned helpers, exact on the
/// solid interior).
#[test]
fn rotated_rect_paints_upright_aabb() {
    let rig = paint_chips();
    let px = |x, y| rig.backend.pixel_rgba(rig.surface, x, y).expect("pixel");
    assert_eq!(px(10, 28), (0, 204, 0, 255), "upright visual paints");
    assert_eq!(px(2, 25), white(), "layout ghost stays background");
}

/// Decision 517: the rounded detour paints through the temp layer —
/// the 20×10 r3 box turns 90° about its own center inside a +90x
/// translated wrapper (nested accumulation through the residual),
/// landing in the 95..105 × 23..43 visual (bands+discs axis-aligned
/// in-temp, then the residual blit — interior exact like the direct
/// path).
#[test]
fn rotated_rrect_paints_upright_through_detour() {
    let rig = paint_chips();
    let px = |x, y| rig.backend.pixel_rgba(rig.surface, x, y).expect("pixel");
    assert_eq!(px(100, 38), (204, 204, 0, 255), "detour interior paints");
    assert_eq!(px(92, 35), white(), "layout ghost stays background");
}
