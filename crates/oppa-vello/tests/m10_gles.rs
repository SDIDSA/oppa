//! M10 weakest-hardware row (GLES / mobile fallback) plus the
//! emulator-measured Android graphics row (gap closure).
//!
//! The M6/M9 re-testable bet ("Vello-on-weak-GPU adequacy plus
//! full-scene CPU fallback at real mobile resolutions") split by
//! what runs where. The GL-oracle arm below holds the M6 pixel
//! standard through a GL-constrained Vello device; the CPU-fallback
//! arm needs no GPU at all. The emulator interrogation (API 36,
//! x86_64, SwiftShader GLES 3.0 max, SELinux Enforcing,
//! 1080×2400@420) lives in the ROUNDS gap-closure entry and the
//! platform docs — note the floor correction there: the bet's
//! "GLES 3.1-class" phrasing is wrong, the measured floor is GLES
//! 3.0 (wgpu's GL requirement, met).
//!
//! Stand-in statement: desktop GL on an RTX proves the GL backend
//! path conforms; it does not prove weak-mobile-GPU performance —
//! that half stays Android-device-owned. Same standard here, not a
//! lower bar: no GLES pixel claim is made without GLES pixels.

#![allow(non_snake_case)]

use oppa::{
    BackendError, Caps, Color, ComponentHost, Ctx, ImageId, Img, RendererBackend, Semantics, Style,
    SurfaceDesc, VNode,
};
use oppa_cpu::{CpuBackend, FramePlanBuilder, OracleSession};
use oppa_vello::oracle::{diff_count_exact, diff_count_tol, RgbaImage};
use oppa_vello::VelloBackend;

const MW: f32 = 1080.0;
const MH: f32 = 2400.0;

/// Serializes GL-driver use across this binary's parallel test
/// threads (M6 finding, same family: concurrent device/adapter
/// traffic on one driver flakes — observed as an intermittent
/// oracle failure with all-GPU tests green in isolation).
static GL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn MobileScene(ctx: &Ctx, _props: &()) -> VNode {
    let is_on = ctx.signal(false);
    let bg = if is_on.get() {
        Color(0x44_44_44)
    } else {
        Color(0x55_55_55)
    };
    let s = is_on.clone();
    oppa::Div("screen")
        .style(Style::new().size(MW, MH).bg(Color(0xFF_FF_FF)))
        .child(
            oppa::Div("toggle")
                .style(Style::new().size(44, 24).bg(bg))
                .semantics(Semantics::switch().checked(is_on.get()).label("Wi-Fi"))
                .on_press(move || s.set(!s.get()))
                .build(),
        )
}

fn mobile_desc() -> SurfaceDesc {
    SurfaceDesc {
        width_px: MW as u32,
        height_px: MH as u32,
        background: Color(0xFF_FF_FF),
    }
}

#[test]
fn gles_adapter_row_is_recorded_not_assumed() {
    let _guard = GL_TEST_LOCK.lock().expect("gl test lock");
    // The probe never panics; whichever outcome lands is eprintln'd
    // into the test log as the row's evidence (Ok names the
    // adapter the oracle arm below must then serve through).
    match VelloBackend::probe_gles_adapter() {
        Ok(info) => {
            eprintln!("m10[gles-row] GL adapter present: {info}");
        }
        Err(e) => {
            eprintln!("m10[gles-row] no GLES adapter on this box: {e}");
            eprintln!(
                "m10[gles-row] verdict: GLES-GPU pixel row OPEN (Android-device row owns it)"
            );
        }
    }
}

#[test]
fn cpu_fallback_holds_at_mobile_resolution() {
    let host = ComponentHost::new();
    host.set_viewport(MW, MH);
    host.mount("MobileScene", (), MobileScene);
    host.run_until_idle();

    // The Caps-negotiated fallback declaration (the contract the
    // hostile-target path degrades through — blur, MSAA, and glyph
    // outlines are someone else's job on this row).
    let caps = Caps::cpu_fallback();
    assert!(!caps.blur_backdrop);
    assert!(!caps.msaa);
    assert!(!caps.text_as_paths);
    assert_eq!(caps.max_layers, 8);

    // Incremental == full repaint at 1080×2400, CPU-exact (0 px).
    let builder = FramePlanBuilder::new(1.0);
    let mut oracle = OracleSession::new(mobile_desc()).expect("mobile oracle surfaces");
    let (incr_plan, full_plan) = host.with_retained_mut(|rec, styles| {
        (
            builder.build_incremental(rec, styles),
            builder.build_full(rec, styles),
        )
    });
    assert!(!incr_plan.ops.is_empty(), "mount must emit ops");
    assert!(!incr_plan.damage.is_empty(), "mount must emit damage");
    oracle
        .commit_all(&host.diffs_from(0))
        .expect("oracle commits mount history");
    assert_eq!(
        oracle
            .assert_paints(&incr_plan, &full_plan)
            .expect("oracle paints mount"),
        0,
        "incremental != full repaint at mobile resolution"
    );
    let seen = host.diff_count();
    let incr_surface = oracle.surfaces().0;
    let png_before = oracle
        .backend_mut()
        .encode_png(incr_surface)
        .expect("mount png encodes");

    // Press through the shared pipeline: damage discipline rebuilds
    // exactly the dirty subtree, still 0 px off full repaint, and
    // the surface actually changes (repaint happened, not skipped).
    host.inject_input(oppa::InputEvent::pointer_down(10.0, 12.0));
    host.inject_input(oppa::InputEvent::pointer_up(10.0, 12.0));
    host.run_until_idle();
    let (incr2, full2) = host.with_retained_mut(|rec, styles| {
        (
            builder.build_incremental(rec, styles),
            builder.build_full(rec, styles),
        )
    });
    oracle
        .commit_all(&host.diffs_from(seen))
        .expect("oracle commits press history");
    assert_eq!(
        oracle
            .assert_paints(&incr2, &full2)
            .expect("oracle paints press"),
        0,
        "incremental != full repaint after press at mobile resolution"
    );
    let png_after = oracle
        .backend_mut()
        .encode_png(incr_surface)
        .expect("press png encodes");
    assert_ne!(png_before, png_after, "press repainted nothing");
    eprintln!(
        "m10[mobile-fallback] 1080x2400 mount_ops={} press_ops={} oracle=0/0 png_bytes={}",
        incr_plan.ops.len(),
        incr2.ops.len(),
        png_after.len()
    );
}

#[test]
fn gles_oracle_holds_the_same_standard() {
    let _guard = GL_TEST_LOCK.lock().expect("gl test lock");
    // Same plan on both rasterizers, diffed in pixels at the M6
    // standard: axis-aligned fills agree pixel-EXACT (the scene is
    // sharp rects only — no curves, no text), with the tol-16 ≤60
    // curves bound as the backstop. Vello renders through the
    // GL-constrained device (`ensure_gpu_gles`); the CPU arm is the
    // tiny-skia fallback row. Stand-in statement: desktop GL on an
    // RTX proves the GL *backend path* conforms; it does not prove
    // weak-mobile-GPU *performance* — that half stays device-owned.
    let host = ComponentHost::new();
    host.set_viewport(MW, MH);
    host.mount("MobileScene", (), MobileScene);
    host.run_until_idle();
    // One press so the oracle covers a dirty-subtree repaint, not
    // just the mount (history replay on both arms).
    host.inject_input(oppa::InputEvent::pointer_down(10.0, 12.0));
    host.inject_input(oppa::InputEvent::pointer_up(10.0, 12.0));
    host.run_until_idle();

    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    assert!(!plan.ops.is_empty());

    let mut vello = VelloBackend::new();
    // One retry on transient driver errors (observed once as an
    // isolated full-workspace flake with 11+ green isolated runs —
    // same family as M6's driver-contention finding). Both attempts
    // are logged, so a persistent failure still fails loudly with
    // both errors instead of hiding behind the retry.
    let info = match vello.ensure_gpu_gles() {
        Ok(info) => info,
        Err(first) => {
            eprintln!("m10[gles-device] first attempt failed: {first} — retrying once");
            std::thread::sleep(std::time::Duration::from_millis(250));
            vello.ensure_gpu_gles().unwrap_or_else(|second| {
                panic!("GL device persistently unusable: {first} then {second}")
            })
        }
    };
    eprintln!("m10[gles-device] {info}");
    let vsurf = vello.create_surface(mobile_desc()).expect("gl surface");
    for d in host.diffs_from(0) {
        vello.commit(&d).expect("gl commit");
    }
    vello.paint(vsurf, &plan).expect("gl paint");
    let gl_img = vello.render_pixels(vsurf).expect("gl readback");

    let mut cpu = CpuBackend::new();
    let csurf = cpu.create_surface(mobile_desc()).expect("cpu surface");
    for d in host.diffs_from(0) {
        cpu.commit(&d).expect("cpu commit");
    }
    cpu.paint(csurf, &plan).expect("cpu paint");
    let cpu_img = RgbaImage::from_cpu_pixmap(cpu.pixmap(csurf).expect("cpu pixmap"))
        .expect("fallback surface stays opaque");

    let exact = diff_count_exact(&cpu_img, &gl_img);
    let tol16 = diff_count_tol(&cpu_img, &gl_img, 16);
    eprintln!("m10[gles-oracle] 1080x2400 exact={exact} tol16={tol16} (bounds 0 / 60)");
    assert_eq!(exact, 0, "axis-aligned fills must agree pixel-exact");
    assert!(tol16 <= 60, "tol-16 backstop (M6 curves bound)");
}

#[test]
fn rimg_refusal_stays_loud_on_the_fallback_row() {
    // Async decode lives in the §9.1 worker mailbox (out of v1 scope
    // per BUILD-ORDER §5.7): until it exists, image pixels fail
    // loudly and leave the surface pristine — never silent tofu,
    // including at mobile resolution.
    fn ImgScene(_ctx: &Ctx, _props: &()) -> VNode {
        oppa::Div("screen")
            .style(Style::new().size(MW, MH).bg(Color(0xFF_FF_FF)))
            .child(
                Img {
                    src: ImageId(7),
                    size: 36.0,
                    radius: 18.0,
                }
                .into(),
            )
    }
    let host = ComponentHost::new();
    host.set_viewport(MW, MH);
    host.mount("ImgScene", (), ImgScene);
    // No paint hook installed on purpose: the hook panics loudly on
    // backend refusal (never a blank frame), so the refusal is
    // asserted directly through a manual paint below. Frames settle
    // layout only (Div-only scene, no text service needed).
    host.run_until_idle();

    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    assert!(
        plan.ops
            .iter()
            .any(|op| matches!(op, oppa::DrawOp::RImg { .. })),
        "img scene must carry an RImg op"
    );
    let mut be = CpuBackend::new();
    let s = be.create_surface(mobile_desc()).expect("pristine surface");
    let err = be.paint(s, &plan).expect_err("RImg must refuse loudly");
    assert!(
        matches!(err, BackendError::UnsupportedOp(_)),
        "wrong refusal shape: {err:?}"
    );
    assert_eq!(
        be.pixel_rgba(s, 0, 0),
        Some((255, 255, 255, 255)),
        "refusal must leave the surface pristine"
    );
}
