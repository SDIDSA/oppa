use super::*;

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
