use super::*;

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
            xf: PaintXf::identity(),
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
