//! Web layout animation (decisions 521-522): fence interpolation
//! flows through real patch JSON on the production presenter leg
//! (per-frame spec publish feeds the backend, sync samples them on
//! the backend clock, settle returns the committed target).
//!
//! Headless (no browser): interpolated `width:` arriving mid-flight
//! and the bare target at settle proves the mechanism the browser
//! would paint (per-frame values live in the hook unit tests;
//! delivery lives here).
//!
//! The test fence runs 2000ms: the click pump flows up to 64 frames
//! of mock time (≈1s), which would settle a production-length fence
//! before the first backend sync ever samples it (production rAF
//! ticks flow ~16ms, so production fences paint every interior
//! frame — same machinery, duration-agnostic proof).

use oppa::{Ctx, Div, Style, VNode};
use oppa_web::WasmHost;

/// Expanding card: press toggles explicit width under a layout_anim
/// declaration (press to grow).
fn layout_root(ctx: &Ctx, _: &()) -> VNode {
    let wide = ctx.signal(false);
    let w = wide.clone();
    Div("card")
        .style(
            Style::new()
                .size(if wide.get() { 200.0 } else { 100.0 }, 50.0)
                .bg(oppa::Color(0x22_66_CC))
                .layout_anim(oppa::Transition::new(2000, oppa::Ease::Linear)),
        )
        .on_press(move || w.set(true))
        .build()
}

fn card_center(app: &WasmHost) -> (f32, f32) {
    let id = oppa::find_retained_by_debug(app.host(), "card")
        .into_iter()
        .next()
        .expect("card retained");
    let b = app.host().committed_box(id).expect("card laid out");
    (b.x + b.w / 2.0, b.y + b.h / 2.0)
}

#[test]
fn web_fence_interpolates_through_patches_then_settles() {
    let mut app = WasmHost::mount_root("Layout", (), layout_root);
    // Settle boot (pumps flow vsync time forward — never tick
    // absolute times after pumped interactions, or the clock runs
    // backwards under the live fence; read the clock instead).
    let _ = app.tick(1000.0);
    let (cx, cy) = card_center(&app);
    // Grow: the press frame patches (geometry already committed),
    // and mid-flight frames carry strictly interior widths.
    let _ = app.click(cx, cy);
    let t0 = app.host().now_ms();
    let patch = app.tick(t0 + 100.0).expect("fence frames keep patching");
    assert!(
        patch.contains("width:1"),
        "mid-flight width rides the wire, got {patch}"
    );
    assert!(
        !patch.contains("width:100px") && !patch.contains("width:200px"),
        "mid-flight width is strictly interior, got {patch}"
    );
    // Past the fence: the target derives bare and the tree
    // goes quiet.
    let patch = app.tick(t0 + 3000.0).expect("settle frame patches");
    assert!(
        patch.contains("width:200px"),
        "settled fence derives the target, got {patch}"
    );
}
