//! Consolidated test suite for oppa-gpu.

use std::sync::Arc;

use oppa::{
    Color, DiffOp, DrawOp, FramePlan, NodeId, PresenterKind, RendererBackend, SurfaceDesc, TreeDiff,
};
use oppa_cpu::CpuBackend;
use oppa_gpu::{diff_count_tol, gpu_caps, GpuBackend, RgbaImage};

#[test]
fn test_caps_and_kind() {
    let backend = GpuBackend::new();
    assert_eq!(backend.kind(), PresenterKind::GpuDrawList);
    let caps = backend.caps();
    assert_eq!(caps, gpu_caps());
    assert_eq!(caps.max_layers, 64);
    assert!(!caps.blur_backdrop);
    assert!(!caps.msaa);
    assert!(!caps.text_as_paths);
}

#[test]
fn test_surface_lifecycle() {
    let mut backend = GpuBackend::new();
    let desc = SurfaceDesc {
        width_px: 200,
        height_px: 100,
        background: Color(0xFF_FF_FF),
    };
    let s = backend.create_surface(desc).expect("surface created");
    assert_eq!(backend.retained_op_count(s), Some(0));

    // Zero size surface must be refused loudly
    let bad_desc = SurfaceDesc {
        width_px: 0,
        height_px: 100,
        background: Color(0xFF_FF_FF),
    };
    assert!(backend.create_surface(bad_desc).is_err());

    backend.destroy_surface(s).expect("surface destroyed");
    assert_eq!(backend.retained_op_count(s), None);
}

#[test]
fn test_empty_plan_skip() {
    let mut backend = GpuBackend::new();
    let desc = SurfaceDesc {
        width_px: 100,
        height_px: 100,
        background: Color(0xFF_FF_FF),
    };
    let s = backend.create_surface(desc).unwrap();
    let empty_plan = FramePlan {
        viewport_w: 100.0,
        viewport_h: 100.0,
        ops: vec![],
        full_repaint: false,
        ..Default::default()
    };

    let stats = backend.paint(s, &empty_plan).expect("empty paint succeeds");
    assert!(stats.skipped_empty);
    assert_eq!(stats.ops_executed, 0);
}

#[test]
fn test_refuse_unregistered_image() {
    let mut backend = GpuBackend::new();
    let desc = SurfaceDesc {
        width_px: 100,
        height_px: 100,
        background: Color(0xFF_FF_FF),
    };
    let s = backend.create_surface(desc).unwrap();
    let plan = FramePlan {
        viewport_w: 100.0,
        viewport_h: 100.0,
        ops: vec![DrawOp::RImg {
            node: NodeId::new(1, 0),
            x: 10.0,
            y: 10.0,
            w: 50.0,
            h: 50.0,
            image: oppa::ImageId(999),
        }],
        full_repaint: true,
        ..Default::default()
    };

    let res = backend.paint(s, &plan);
    assert!(res.is_err(), "unregistered image must be refused loudly");
}

#[test]
fn test_refuse_invalid_svg_path() {
    let mut backend = GpuBackend::new();
    let desc = SurfaceDesc {
        width_px: 100,
        height_px: 100,
        background: Color(0xFF_FF_FF),
    };
    let s = backend.create_surface(desc).unwrap();
    let plan = FramePlan {
        viewport_w: 100.0,
        viewport_h: 100.0,
        ops: vec![DrawOp::Path {
            node: NodeId::new(1, 0),
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
            data: Arc::from("NOT_A_VALID_SVG_PATH"),
            fill: Some(Color(0xFF_00_00)),
            stroke: None,
            opacity: 1.0,
        }],
        full_repaint: true,
        ..Default::default()
    };

    let res = backend.paint(s, &plan);
    assert!(res.is_err(), "invalid SVG path must be refused loudly");
}

#[test]
fn test_retained_splice_and_commit() {
    let mut backend = GpuBackend::new();
    let desc = SurfaceDesc {
        width_px: 100,
        height_px: 100,
        background: Color(0xFF_FF_FF),
    };
    let s = backend.create_surface(desc).unwrap();

    let n1 = NodeId::new(1, 0);
    let n2 = NodeId::new(2, 0);

    let plan1 = FramePlan {
        viewport_w: 100.0,
        viewport_h: 100.0,
        ops: vec![
            DrawOp::Rect {
                node: n1,
                x: 10.0,
                y: 10.0,
                w: 20.0,
                h: 20.0,
                color: Color(0xFF_00_00),
                opacity: 1.0,
            },
            DrawOp::Rect {
                node: n2,
                x: 40.0,
                y: 40.0,
                w: 20.0,
                h: 20.0,
                color: Color(0x00_FF_00),
                opacity: 1.0,
            },
        ],
        full_repaint: true,
        ..Default::default()
    };

    backend.paint(s, &plan1).unwrap();
    assert_eq!(backend.retained_op_count(s), Some(2));

    // Commit node remove removes ops from surface
    let diff = TreeDiff {
        ops: vec![DiffOp::Remove { id: n1 }],
        suppress_transitions: false,
    };
    backend.commit(&diff).unwrap();
    assert_eq!(backend.retained_op_count(s), Some(1));
}

#[test]
fn test_oracle_compare_geometry() {
    let mut cpu = CpuBackend::new();
    let mut gpu = GpuBackend::new();

    let desc = SurfaceDesc {
        width_px: 120,
        height_px: 80,
        background: Color(0xFF_FF_FF),
    };

    let s_cpu = cpu.create_surface(desc).unwrap();
    let s_gpu = gpu.create_surface(desc).unwrap();

    let n = NodeId::new(1, 0);
    let plan = FramePlan {
        viewport_w: 120.0,
        viewport_h: 80.0,
        ops: vec![
            DrawOp::Rect {
                node: n,
                x: 10.0,
                y: 10.0,
                w: 30.0,
                h: 25.0,
                color: Color(0x33_66_99),
                opacity: 1.0,
            },
            DrawOp::RRect {
                node: n,
                x: 50.0,
                y: 10.0,
                w: 30.0,
                h: 25.0,
                radius: 4.0,
                radii: None,
                color: Color(0x99_33_66),
                opacity: 1.0,
            },
            DrawOp::Circle {
                node: n,
                cx: 100.0,
                cy: 25.0,
                r: 12.0,
                color: Color(0x33_99_33),
                opacity: 1.0,
            },
            DrawOp::Shadow {
                node: n,
                x: 10.0,
                y: 45.0,
                w: 40.0,
                h: 20.0,
                dx: 2.0,
                dy: 2.0,
                blur_radius: 0.0, // solid offset shadow
                color: Color(0x55_55_55),
            },
            DrawOp::Path {
                node: n,
                x: 60.0,
                y: 45.0,
                width: 30.0,
                height: 25.0,
                data: Arc::from("M 0 0 L 30 0 L 15 25 Z"),
                fill: Some(Color(0xCC_66_00)),
                stroke: None,
                opacity: 1.0,
            },
        ],
        full_repaint: true,
        ..Default::default()
    };

    cpu.paint(s_cpu, &plan).unwrap();
    gpu.paint(s_gpu, &plan).unwrap();

    let cpu_pixmap = cpu.pixmap(s_cpu).unwrap();
    let cpu_img = RgbaImage::from_cpu_pixmap(cpu_pixmap).expect("cpu pixmap opaque");

    match gpu.render_pixels(s_gpu) {
        Ok(gpu_img) => {
            assert_eq!(cpu_img.width, gpu_img.width);
            assert_eq!(cpu_img.height, gpu_img.height);

            // Verify spot checks inside the solid interior of Rect (15, 15)
            let (r_cpu, g_cpu, b_cpu, a_cpu) = cpu_img.pixel(15, 15).unwrap();
            let (r_gpu, g_gpu, b_gpu, a_gpu) = gpu_img.pixel(15, 15).unwrap();
            assert_eq!((r_cpu, g_cpu, b_cpu, a_cpu), (r_gpu, g_gpu, b_gpu, a_gpu));

            // Verify tolerance bound on differing boundary pixels
            let diff_tol = diff_count_tol(&cpu_img, &gpu_img, 15);
            let total_pixels = (desc.width_px * desc.height_px) as usize;
            let diff_ratio = diff_tol as f64 / total_pixels as f64;
            // Fewer than 5% of pixels differ past ±15 tolerance (only anti-aliased subpixel fringes)
            assert!(
                diff_ratio < 0.05,
                "diff_tol={diff_tol} ({:.2}%) exceeds 5% bound",
                diff_ratio * 100.0
            );
        }
        Err(e) => {
            eprintln!("Skipping hardware pixel readback: {e}");
        }
    }
}

#[test]
fn test_oracle_image_rendering() {
    let mut cpu = CpuBackend::new();
    let mut gpu = GpuBackend::new();

    let desc = SurfaceDesc {
        width_px: 60,
        height_px: 60,
        background: Color(0xFF_FF_FF),
    };

    let s_cpu = cpu.create_surface(desc).unwrap();
    let s_gpu = gpu.create_surface(desc).unwrap();

    let img_id = oppa::ImageId(42);
    let mut img_data = Vec::with_capacity(20 * 20 * 4);
    for y in 0..20 {
        for _x in 0..20 {
            if y < 10 {
                img_data.extend_from_slice(&[255, 0, 0, 255]); // red top half
            } else {
                img_data.extend_from_slice(&[0, 0, 255, 255]); // blue bottom half
            }
        }
    }

    cpu.insert_image(img_id, 20, 20, img_data.clone());
    gpu.insert_image(img_id, 20, 20, img_data);

    let n = NodeId::new(1, 0);
    let plan = FramePlan {
        viewport_w: 60.0,
        viewport_h: 60.0,
        ops: vec![DrawOp::RImg {
            node: n,
            x: 10.0,
            y: 10.0,
            w: 20.0,
            h: 20.0,
            image: img_id,
        }],
        full_repaint: true,
        ..Default::default()
    };

    cpu.paint(s_cpu, &plan).unwrap();
    gpu.paint(s_gpu, &plan).unwrap();

    let cpu_pixmap = cpu.pixmap(s_cpu).unwrap();
    let cpu_img = RgbaImage::from_cpu_pixmap(cpu_pixmap).unwrap();

    if let Ok(gpu_img) = gpu.render_pixels(s_gpu) {
        // Red top half interior (15, 15)
        let (r, _g, b, a) = gpu_img.pixel(15, 15).unwrap();
        assert_eq!((r, a), (255, 255));
        assert!(b < 5);

        // Blue bottom half interior (15, 25)
        let (r, _g, b, a) = gpu_img.pixel(15, 25).unwrap();
        assert_eq!((b, a), (255, 255));
        assert!(r < 5);

        let diff_tol = diff_count_tol(&cpu_img, &gpu_img, 5);
        let total = (desc.width_px * desc.height_px) as usize;
        assert!((diff_tol as f64 / total as f64) < 0.05);
    }
}

#[test]
fn test_oracle_blurred_shadow() {
    let mut cpu = CpuBackend::new();
    let mut gpu = GpuBackend::new();

    let desc = SurfaceDesc {
        width_px: 100,
        height_px: 100,
        background: Color(0xFF_FF_FF),
    };

    let s_cpu = cpu.create_surface(desc).unwrap();
    let s_gpu = gpu.create_surface(desc).unwrap();

    let n = NodeId::new(1, 0);
    let plan = FramePlan {
        viewport_w: 100.0,
        viewport_h: 100.0,
        ops: vec![DrawOp::Shadow {
            node: n,
            x: 20.0,
            y: 20.0,
            w: 50.0,
            h: 50.0,
            dx: 5.0,
            dy: 5.0,
            blur_radius: 6.0,
            color: Color(0x22_22_22),
        }],
        full_repaint: true,
        ..Default::default()
    };

    cpu.paint(s_cpu, &plan).unwrap();
    gpu.paint(s_gpu, &plan).unwrap();

    let cpu_pixmap = cpu.pixmap(s_cpu).unwrap();
    let cpu_img = RgbaImage::from_cpu_pixmap(cpu_pixmap).unwrap();

    if let Ok(gpu_img) = gpu.render_pixels(s_gpu) {
        // Deep interior of shadow (45, 45) must be darkened on both
        let (r_cpu, _, _, _) = cpu_img.pixel(45, 45).unwrap();
        let (r_gpu, _, _, _) = gpu_img.pixel(45, 45).unwrap();
        assert!(r_cpu < 100);
        assert!(r_gpu < 100);

        // Exterior at corner (5, 5) must be pure background white on both
        assert_eq!(cpu_img.pixel(5, 5).unwrap(), (255, 255, 255, 255));
        assert_eq!(gpu_img.pixel(5, 5).unwrap(), (255, 255, 255, 255));
    }
}

#[cfg(windows)]
#[test]
fn test_oracle_text_rendering() {
    use oppa::text::TextService;
    use oppa_text_dwrite::DWriteTextService;

    let svc = DWriteTextService::new().expect("dwrite service");
    let style = oppa::TextStyle::new("Segoe UI", 20.0);
    let shaped = svc.shape("Oppa GPU", &style).expect("shape text");
    let fid = shaped.runs[0].font_id;
    let (font_path, font_index) = svc.font_file_source(fid).expect("font file");
    let font_bytes = std::fs::read(&font_path).expect("read font bytes");

    let mut cpu = CpuBackend::new();
    let mut gpu = GpuBackend::new();

    cpu.set_font_bytes(font_bytes.clone(), font_index);
    gpu.set_font_bytes(font_bytes, font_index);

    let desc = SurfaceDesc {
        width_px: 160,
        height_px: 60,
        background: Color(0xFF_FF_FF),
    };

    let s_cpu = cpu.create_surface(desc).unwrap();
    let s_gpu = gpu.create_surface(desc).unwrap();

    let mut cur_x = 0.0;
    let mut placed_glyphs = Vec::new();
    for g in &shaped.glyphs {
        placed_glyphs.push(oppa::PlacedGlyph {
            glyph_id: g.glyph_id,
            x: cur_x + g.x_offset,
            advance: g.x_advance,
        });
        cur_x += g.x_advance;
    }

    let font_runs: Vec<oppa::FontRun> = shaped
        .runs
        .iter()
        .map(|r| oppa::FontRun {
            glyph_range: (r.glyph_range.0, r.glyph_range.1),
            family: "Segoe UI".to_string(),
            font_id: r.font_id,
        })
        .collect();

    let n = NodeId::new(1, 0);
    let plan = FramePlan {
        viewport_w: 160.0,
        viewport_h: 60.0,
        ops: vec![DrawOp::Text {
            node: n,
            x: 10.0,
            y: 10.0,
            line_height: 24.0,
            baseline: 18.0,
            em_size: 20.0,
            glyphs: placed_glyphs,
            fonts: font_runs,
            ink: Color(0x00_00_00),
            opacity: 1.0,
        }],
        full_repaint: true,
        ..Default::default()
    };

    cpu.paint(s_cpu, &plan).unwrap();
    gpu.paint(s_gpu, &plan).unwrap();

    let cpu_pixmap = cpu.pixmap(s_cpu).unwrap();
    let cpu_img = RgbaImage::from_cpu_pixmap(cpu_pixmap).unwrap();

    if let Ok(gpu_img) = gpu.render_pixels(s_gpu) {
        // Assert that both CPU and GPU inked text onto the background
        let cpu_ink_count = cpu_img
            .pixels
            .chunks_exact(4)
            .filter(|p| p[0] < 250)
            .count();
        let gpu_ink_count = gpu_img
            .pixels
            .chunks_exact(4)
            .filter(|p| p[0] < 250)
            .count();
        assert!(cpu_ink_count > 50, "CPU should have inked text");
        assert!(gpu_ink_count > 50, "GPU should have inked text");

        // Ink count should be very close (within 20% due to subpixel rendering differences)
        let ratio = cpu_ink_count as f64 / gpu_ink_count as f64;
        assert!(
            ratio > 0.8 && ratio < 1.25,
            "ink counts should closely match: cpu={cpu_ink_count}, gpu={gpu_ink_count}"
        );
    }
}
