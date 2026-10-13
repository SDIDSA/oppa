use super::*;

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
