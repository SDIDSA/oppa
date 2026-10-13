use super::*;

// ---------------------------------------------------------------------------
// 7. PAINT-phase wiring: build + commit through set_paint_pass.
// ---------------------------------------------------------------------------

#[test]
fn paint_phase_wiring_builds_and_commits() {
    let (fake, _) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    host.set_viewport(VW, VH);
    let backend: Rc<RefCell<CpuBackend>> = Rc::new(RefCell::new(CpuBackend::new()));
    let surface = backend
        .borrow_mut()
        .create_surface(oppa::SurfaceDesc {
            width_px: VW as u32,
            height_px: VH as u32,
            background: SURFACE_BG,
        })
        .expect("surface");
    let calls = Rc::new(Cell::new(0usize));
    let last_ops = Rc::new(Cell::new(usize::MAX));
    oppa_cpu::install_paint_hook(
        &host,
        backend.clone(),
        surface,
        1.0,
        calls.clone(),
        last_ops.clone(),
    );

    let _handle = host.mount(
        "card",
        CardProps {
            title: "Hi".to_string(),
        },
        render_card,
    );
    host.run_until_idle();
    assert!(calls.get() >= 1, "PAINT phase ran the hook");
    assert_eq!(last_ops.get(), 2, "mount plan through the phase");
    assert_eq!(
        backend.borrow().pixel_rgba(surface, 13, 8).expect("px"),
        ink()
    );

    // One more frame with no changes → empty plan through the same phase.
    let before = backend.borrow().encode_png(surface).expect("png");
    host.runtime().request_frame();
    host.run_until_idle();
    assert!(calls.get() >= 2);
    assert_eq!(last_ops.get(), 0, "static frame builds empty plan in-phase");
    assert_eq!(backend.borrow().encode_png(surface).expect("png"), before);
}
