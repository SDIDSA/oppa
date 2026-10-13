use super::*;

// ---------------------------------------------------------------------------
// 8. Contract surface: Caps third row + loud misses + paint hook
// ---------------------------------------------------------------------------

#[test]
fn caps_negotiation_joins_its_third_row() {
    let dom = DomBackend::new(1.0);
    assert_eq!(dom.kind(), PresenterKind::Dom);
    let caps = dom.caps();
    assert_eq!(caps, Caps::dom());
    assert_eq!(caps.max_layers, 1024, "stacking contexts are cheap");
    assert!(!caps.blur_backdrop, "same offset-solid degradation");
    assert!(caps.msaa, "browser coverage is always on");
    assert!(caps.text_as_paths, "the browser shapes real outlines");
    // Beside the rasterizer rows: neither rasterizer shapes text as
    // paths (decision 388 — custom is the only GPU row left).
    let cpu = oppa_cpu::CpuBackend::new();
    let gpu_backend = oppa_gpu::GpuBackend::new();
    assert!(!cpu.caps().text_as_paths);
    assert!(!gpu_backend.caps().text_as_paths);
}

#[test]
fn contract_surface_fails_loudly() {
    use oppa::RendererBackend;
    let mut dom = DomBackend::new(1.0);
    let bad = dom.create_surface(oppa::SurfaceDesc {
        width_px: 0,
        height_px: 60,
        background: SURFACE_BG,
    });
    assert!(matches!(bad, Err(BackendError::BadSurface(_))));
    let surf = dom
        .create_surface(oppa::SurfaceDesc {
            width_px: 200,
            height_px: 120,
            background: SURFACE_BG,
        })
        .expect("surface");
    let unknown = oppa::SurfaceId(999);
    assert!(matches!(
        dom.paint(unknown, &oppa::FramePlan::default()),
        Err(BackendError::UnknownSurface(_))
    ));
    assert!(matches!(
        dom.destroy_surface(unknown),
        Err(BackendError::UnknownSurface(_))
    ));
    dom.destroy_surface(surf).expect("destroy");
    let after_destroy = dom.paint(surf, &oppa::FramePlan::default());
    assert!(
        matches!(after_destroy, Err(BackendError::UnknownSurface(_))),
        "destroyed surfaces stay loud: {after_destroy:?}"
    );

    // Image nodes without a registered source refuse loudly (round
    // 4.4 — the refusal names the node instead of rendering a
    // placeholder; registered ids render `<img>`, proven below).
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Img", (), render_img);
    host.run_until_idle();
    let mut sheet = StyleSheet::new(1.0);
    let err = host.with_retained_mut(|rec, styles| dom.sync(rec, styles, &mut sheet));
    assert!(
        matches!(err, Err(BackendError::UnsupportedOp(_))),
        "{err:?}"
    );
}

fn render_img(_ctx: &Ctx, _props: &()) -> VNode {
    oppa::Img {
        src: oppa::ImageId(7),
        size: 36.0,
        radius: 18.0,
    }
    .into()
}

/// Round 4.4 (decision 267): registered images render real
/// `<img>` elements (src resolved, alt from the semantics label,
/// explicit geometry), and src changes re-render.
#[test]
fn img_renders_src_alt_and_geometry() {
    use oppa::{ImageCache, Signal};
    #[derive(Clone, Props)]
    struct ImgProps {
        src: Signal<oppa::ImageId>,
    }
    fn render_pic(_ctx: &Ctx, p: &ImgProps) -> VNode {
        VNode::from(oppa::Img {
            src: p.src.get(),
            size: 36.0,
            radius: 4.0,
        })
    }
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let cache = ImageCache::new();
    let a = cache.load("img/a.png");
    let b = cache.load("img/b.png?x=1&y=2");
    let src = host.runtime().signal(a);
    host.mount("Pic", ImgProps { src: src.clone() }, render_pic);
    host.run_until_idle();
    let mut dom = DomBackend::new(1.0);
    dom.set_images(cache);
    let mut sheet = StyleSheet::new(1.0);
    host.with_retained_mut(|rec, styles| dom.sync(rec, styles, &mut sheet))
        .expect("registered image syncs");
    let id = oppa::find_retained_by_debug(&host, "img")[0];
    let html = dom.render_node(id);
    assert!(html.starts_with("<img "), "{html}");
    assert!(html.contains("src=\"img/a.png\""), "{html}");
    assert!(
        html.contains("alt=\"\""),
        "no label means empty alt, {html}"
    );
    assert!(html.contains("width:36px;"), "explicit geometry, {html}");
    assert!(html.contains("height:36px;"), "{html}");
    let page = oppa_dom::render_page("t", &dom, &sheet);
    assert!(
        page.contains("border-radius:4px;"),
        "radius rides the class rule, {page}"
    );
    // Src change (with a query string to prove escaping paths
    // round-trip) re-renders the element.
    src.set(b);
    host.run_until_idle();
    let touched = host
        .with_retained_mut(|rec, styles| dom.sync(rec, styles, &mut sheet))
        .expect("src change syncs")
        .touched;
    assert!(touched > 0, "src change touches");
    let html = dom.render_node(id);
    assert!(html.contains("src=\"img/b.png?x=1&amp;y=2\""), "{html}");
}

/// Decision 291: `Tag::Path` leaves render inline `<svg>` (path
/// data verbatim in `d`, fill/stroke mapped, round caps/joins
/// stated, explicit geometry) — no font runs, no image cache.
#[test]
fn path_renders_inline_svg_with_paint_and_geometry() {
    fn render_vec(_ctx: &Ctx, _p: &()) -> VNode {
        oppa::Path::new("mark")
            .data("M 2 2 L 6 6 L 10 2")
            .stroke(Color(0x11_11_11), 2.0)
            .size(12, 8)
            .build()
    }
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Vec", (), render_vec);
    host.run_until_idle();
    let mut dom = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    host.with_retained_mut(|rec, styles| dom.sync(rec, styles, &mut sheet))
        .expect("path syncs");
    let id = oppa::find_retained_by_debug(&host, "mark")[0];
    let html = dom.render_node(id);
    assert!(html.starts_with("<svg "), "{html}");
    assert!(html.contains("d=\"M 2 2 L 6 6 L 10 2\""), "{html}");
    assert!(
        html.contains("fill=\"none\""),
        "unstroked half reads none, {html}"
    );
    assert!(html.contains("stroke=\"#111111\""), "{html}");
    assert!(html.contains("stroke-width=\"2\""), "{html}");
    assert!(html.contains("stroke-linecap=\"round\""), "{html}");
    assert!(html.contains("stroke-linejoin=\"round\""), "{html}");
    assert!(html.contains("viewBox=\"0 0 12 8\""), "{html}");
    assert!(html.contains("width:12px;"), "explicit geometry, {html}");
    assert!(html.contains("height:8px;"), "{html}");
}

#[test]
fn dom_paint_hook_joins_the_phase_discipline() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let backend = Rc::new(RefCell::new(DomBackend::new(1.0)));
    let sheet = Rc::new(RefCell::new(StyleSheet::new(1.0)));
    let surface = backend
        .borrow_mut()
        .create_surface(oppa::SurfaceDesc {
            width_px: 200,
            height_px: 120,
            background: SURFACE_BG,
        })
        .expect("surface");
    let paint_calls = Rc::new(Cell::new(0usize));
    let last_touched = Rc::new(Cell::new(usize::MAX));
    install_dom_paint_hook(
        &host,
        backend.clone(),
        sheet.clone(),
        surface,
        1.0,
        paint_calls.clone(),
        last_touched.clone(),
    );
    host.mount("Card", CardProps { title: "Hi".into() }, render_card);
    host.run_until_idle();
    assert!(paint_calls.get() > 0, "PAINT phases ran");
    assert_ne!(last_touched.get(), usize::MAX, "sync ran in phase");
    // Settle: a further idle run schedules no PAINT at all (static-frame
    // work 0 — the hook only observes, never invents demand).
    let calls = paint_calls.get();
    let touched = last_touched.get();
    host.run_until_idle();
    assert_eq!(paint_calls.get(), calls, "no spurious PAINT on idle");
    assert_eq!(last_touched.get(), touched, "no spurious sync on idle");
    assert!(backend.borrow().element_count() > 0);
}
