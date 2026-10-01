//! M6 visual artifacts: side-by-side CPU vs Vello PNGs.
//!
//! Run: `cargo run -p oppa-vello --example visuals`
//! Geometry pairs render everywhere; text cards need Windows (DirectWrite
//! shaping + Segoe UI bytes for the Vello outlines). Prints every path.

use std::sync::Arc;

use oppa::{
    Color, ComponentHost, Ctx, Div, DrawOp, FramePlan, NodeId, Props, RendererBackend, VNode,
};
use oppa_cpu::FramePlanBuilder;
use oppa_vello::VelloBackend;

const CARD_BG: Color = Color(0x44_44_44);
const SURFACE_BG: Color = Color(0xFF_FF_FF);
const VW: f32 = 120.0;
const VH: f32 = 60.0;

fn surface_desc() -> oppa::SurfaceDesc {
    oppa::SurfaceDesc {
        width_px: VW as u32,
        height_px: VH as u32,
        background: SURFACE_BG,
    }
}

fn save_cpu(be: &oppa_cpu::CpuBackend, s: oppa::SurfaceId, path: &std::path::Path) {
    be.save_png(s, path).expect("save cpu png");
    println!("CPU   : {}", path.display());
}

fn save_vello(be: &mut VelloBackend, s: oppa::SurfaceId, path: &std::path::Path) {
    let img = be.render_pixels(s).expect("gpu readback");
    let size = tiny_skia::IntSize::from_wh(img.width, img.height).expect("nonzero size");
    let px = tiny_skia::Pixmap::from_vec(img.pixels.clone(), size).expect("pixmap from readback");
    let bytes = px.encode_png().expect("png encode");
    std::fs::write(path, bytes).expect("write vello png");
    println!("Vello : {}", path.display());
}

/// The M6 coverage plan: clip + 0.8 layer (rect/rrect/circle/shadow) +
/// the M5 border-ring pair. No text — renders headless anywhere.
fn coverage_plan() -> FramePlan {
    let n = NodeId::new(7, 0);
    let ring = NodeId::new(8, 0);
    FramePlan {
        viewport_w: VW,
        viewport_h: VH,
        ops: vec![
            DrawOp::PushClip {
                x: 5.0,
                y: 5.0,
                w: 100.0,
                h: 50.0,
            },
            DrawOp::PushLayer { opacity: 0.8 },
            DrawOp::Rect {
                node: n,
                x: 10.0,
                y: 10.0,
                w: 30.0,
                h: 20.0,
                color: CARD_BG,
                opacity: 1.0,
            },
            DrawOp::RRect {
                node: n,
                x: 45.0,
                y: 10.0,
                w: 30.0,
                h: 20.0,
                radius: 5.0,
                radii: None,
                color: CARD_BG,
                opacity: 1.0,
            },
            DrawOp::Circle {
                node: n,
                cx: 95.0,
                cy: 20.0,
                r: 8.0,
                color: CARD_BG,
                opacity: 1.0,
            },
            DrawOp::Shadow {
                node: n,
                x: 10.0,
                y: 35.0,
                w: 30.0,
                h: 10.0,
                dx: 1.0,
                dy: 2.0,
                color: Color(0x00_00_00),
            },
            DrawOp::Pop,
            DrawOp::Pop,
            DrawOp::RRect {
                node: ring,
                x: 4.0,
                y: 4.0,
                w: 40.0,
                h: 24.0,
                radius: 6.0,
                radii: None,
                color: Color(0xAA_BB_CC),
                opacity: 1.0,
            },
            DrawOp::RRect {
                node: ring,
                x: 6.0,
                y: 6.0,
                w: 36.0,
                h: 20.0,
                radius: 4.0,
                radii: None,
                color: CARD_BG,
                opacity: 1.0,
            },
        ],
        damage: vec![],
        stats: Default::default(),
        full_repaint: true,
    }
}

fn main() {
    let out = std::env::temp_dir().join("oppa-m6-visuals");
    std::fs::create_dir_all(&out).expect("create out dir");
    println!("out dir: {}", out.display());

    // Geometry pair (portable — no fonts involved).
    let plan = coverage_plan();
    let mut cpu = oppa_cpu::CpuBackend::new();
    let cs = cpu.create_surface(surface_desc()).expect("cpu surface");
    cpu.paint(cs, &plan).expect("cpu paint");
    save_cpu(&cpu, cs, &out.join("geo_cpu.png"));

    let mut vello = VelloBackend::new();
    let vs = vello.create_surface(surface_desc()).expect("vello surface");
    vello.paint(vs, &plan).expect("vello paint");
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        save_vello(&mut vello, vs, &out.join("geo_vello.png"));
    })) {
        Ok(()) => {}
        Err(_) => println!("Vello : <no GPU adapter - geometry Vello row skipped>"),
    }

    // Text cards (Windows + GPU: DWrite-shaped "Hi", cells vs outlines).
    #[cfg(windows)]
    {
        #[derive(Clone)]
        struct CardProps {
            title: String,
        }
        impl Props for CardProps {}
        fn render_card(_ctx: &Ctx, props: &CardProps) -> VNode {
            let text: VNode = oppa::Text {
                text: Arc::from(props.title.as_str()),
                style: oppa::Text::title_small,
            }
            .into();
            Div("card")
                .style(
                    oppa::Style::new()
                        .size(100, 40)
                        .pad_x(8)
                        .radius(6)
                        .bg(CARD_BG),
                )
                .child(text)
        }
        use oppa::TextService;
        let host = ComponentHost::new();
        host.set_text_service(Box::new(
            oppa_text_dwrite::DWriteTextService::new().expect("dwrite"),
        ));
        host.set_viewport(VW, VH);
        let _handle = host.mount("card", CardProps { title: "Hi".into() }, render_card);
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));

        let mut cpu = oppa_cpu::CpuBackend::new();
        let cs = cpu.create_surface(surface_desc()).expect("cpu surface");
        for d in host.diffs_from(0) {
            cpu.commit(&d).expect("cpu commit");
        }
        cpu.paint(cs, &plan).expect("cpu paint");
        save_cpu(&cpu, cs, &out.join("card_cpu.png"));

        // Segoe UI bytes for the single-face atlas.
        let probe = oppa_text_dwrite::DWriteTextService::new().expect("dwrite");
        let shaped = probe
            .shape("Hi", &oppa::TextStyle::new("Segoe UI", 16.0))
            .expect("shape Hi");
        let fid = shaped.runs[0].font_id;
        let (path, index) = probe.font_file_source(fid).expect("font file");
        let bytes = std::fs::read(&path).expect("read font");
        let mut vello = VelloBackend::new();
        vello.set_font_bytes(bytes, index);
        let vs = vello.create_surface(surface_desc()).expect("vello surface");
        for d in host.diffs_from(0) {
            vello.commit(&d).expect("vello commit");
        }
        vello.paint(vs, &plan).expect("vello paint");
        save_vello(&mut vello, vs, &out.join("card_vello.png"));
    }
    #[cfg(not(windows))]
    {
        println!("text cards: Windows-only (DirectWrite) - skipped");
    }
}
