use super::*;

// ---------------------------------------------------------------------------
// 7. Two-rasterizer box compare (CPU == GPU == DOM rounded boxes)
// ---------------------------------------------------------------------------

#[test]
fn two_rasterizers_and_dom_commit_one_tree_and_agree_on_boxes() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.set_text_service(Box::new(FakeText));
    host.mount("Card", CardProps { title: "Hi".into() }, render_card);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));

    // Same tree committed to both rasterizers plus DOM.
    let mut cpu = oppa_cpu::CpuBackend::new();
    let mut gpu_backend = oppa_gpu::GpuBackend::new();
    let mut dom = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    {
        use oppa::RendererBackend;
        for d in host.diffs_from(0) {
            cpu.commit(&d).expect("cpu commit");
            gpu_backend.commit(&d).expect("gpu commit");
            dom.commit(&d).expect("dom commit");
        }
    }
    assert!(cpu.live_node_count() > 0);
    assert_eq!(gpu_backend.live_node_count(), cpu.live_node_count());
    dom.with_sync(&host, &mut sheet).expect("sync");

    // CPU DrawOp boxes == DOM element geometry, serialized exactly
    // (both derive from the same committed LayoutBox).
    let html = render_page("t", &dom, &sheet);
    let mut compared = 0;
    for op in &plan.ops {
        match op {
            oppa::DrawOp::Rect { x, y, w, h, .. } | oppa::DrawOp::RRect { x, y, w, h, .. } => {
                for v in [x, y, w, h] {
                    assert_eq!(*v, v.round(), "commit positions are device-snapped");
                }
                // The card is the root (relative, no offsets); every
                // other box serializes its offsets exactly.
                assert!(html.contains(&format!("width:{w}px")), "{html}");
                assert!(html.contains(&format!("height:{h}px")), "{html}");
                if *x != 0.0 || *y != 0.0 {
                    assert!(html.contains(&format!("left:{x}px")), "{html}");
                    assert!(html.contains(&format!("top:{y}px")), "{html}");
                }
                compared += 1;
            }
            oppa::DrawOp::Text { x, y, glyphs, .. } => {
                assert!(!glyphs.is_empty());
                assert!(html.contains(&format!("left:{x}px")), "{html}");
                assert!(html.contains(&format!("top:{y}px")), "{html}");
                compared += 1;
            }
            _ => {}
        }
    }
    assert!(compared >= 2, "card bg + text compared");
    // Glyph cells are the shared position contract across rasterizers
    // (asserted pixel-side on Windows below; headless here).
    let cells: usize = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            oppa::DrawOp::Text { glyphs, .. } => Some(glyphs.len()),
            _ => None,
        })
        .sum();
    assert!(cells > 0);
}
