//! G8 end to end: PNG bytes -> decode -> CPU insert -> RImg paint ->
//! exact pixels. The async half is a documented pattern (decode is a
//! blocking Send-friendly pure function: run it in ctx.spawn on
//! native, deposit with insert_image on the UI thread); this test
//! proves the sync spine the pattern rides.

use oppa::{DrawOp, FramePlan, ImageId, NodeId, PlanStats, RendererBackend};

fn rimg_plan(image: ImageId) -> FramePlan {
    FramePlan {
        viewport_w: 2.0,
        viewport_h: 2.0,
        ops: vec![DrawOp::RImg {
            node: NodeId::new(1, 0),
            x: 0.0,
            y: 0.0,
            w: 2.0,
            h: 2.0,
            image,
        }],
        damage: vec![],
        stats: PlanStats::default(),
        full_repaint: true,
    }
}

#[test]
fn png_bytes_reach_cpu_pixels_exactly() {
    // 2x2 distinct opaque, encoded to PNG bytes in-test (no fixtures).
    let rgba: Vec<u8> = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .expect("header")
            .write_image_data(&rgba)
            .expect("pixels");
    }
    let img = oppa_image::decode_image(&bytes).expect("decodes");
    assert_eq!((img.width, img.height), (2, 2));

    let mut backend = oppa_cpu::CpuBackend::new();
    let surface = backend
        .create_surface(oppa::SurfaceDesc {
            width_px: 2,
            height_px: 2,
            background: oppa::Color(0x00_00_00),
        })
        .expect("surface");
    let id = ImageId(42);
    backend.insert_image(id, img.width, img.height, img.rgba);
    backend.paint(surface, &rimg_plan(id)).expect("paints");
    assert_eq!(backend.pixel_rgba(surface, 0, 0), Some((255, 0, 0, 255)));
    assert_eq!(backend.pixel_rgba(surface, 1, 0), Some((0, 255, 0, 255)));
    assert_eq!(backend.pixel_rgba(surface, 0, 1), Some((0, 0, 255, 255)));
    assert_eq!(
        backend.pixel_rgba(surface, 1, 1),
        Some((255, 255, 255, 255))
    );
}
