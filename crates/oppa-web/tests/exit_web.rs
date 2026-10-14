//! Web exit-deferral (decision 520): dismissals fade through real patch
//! JSON on the production presenter leg (commit feeds the twin, sync
//! steps kept ghosts, settle drops them).
//!
//! Headless (no browser): the patch strings are what the bootstrap
//! applier consumes, so stepped `opacity:` + `pointer-events:none`
//! arriving mid-exit and vanishing at settle proves the mechanism the
//! browser would paint (per-frame values live in the hook unit tests;
//! delivery lives here).

use oppa::{Ctx, Div, Portal, Style, VNode};
use oppa_web::WasmHost;

/// Dismissable card: open renders the exit-declaring plate (press to
/// dismiss), closed renders nothing (the overlay dismiss shape).
fn exit_root(ctx: &Ctx, _: &()) -> VNode {
    let open = ctx.signal(true);
    if !open.get() {
        return Portal("x-closed").style(Style::new().size(0, 0)).build();
    }
    let o = open.clone();
    Div("card")
        .style(
            Style::new()
                .size(100, 50)
                .bg(oppa::Color(0x22_66_CC))
                .exit(oppa::Transition::exit()),
        )
        .on_press(move || o.set(false))
        .build()
}

fn card_center(app: &WasmHost) -> (f32, f32) {
    let id = oppa::find_retained_by_debug(app.host(), "card")
        .into_iter()
        .next()
        .expect("card retained while open");
    let b = app.host().committed_box(id).expect("card laid out");
    (b.x + b.w / 2.0, b.y + b.h / 2.0)
}

#[test]
fn web_dismissal_fades_through_patches_then_removes() {
    let mut app = WasmHost::mount_root("Exit", (), exit_root);
    // Settle boot (pumps flow vsync time forward — never tick
    // absolute times after pumped interactions, or the clock runs
    // backwards under the live exit; read the clock instead).
    let _ = app.tick(1000.0);
    let (cx, cy) = card_center(&app);
    // Dismiss: the removal keeps the ghost (click-through stepped
    // fade from the entry value) — a real patch, never quiet. The
    // pump flows host time, so the backend pins at repaint time and
    // the twin starts there too (same frame, same clock).
    let patch = app.click(cx, cy).expect("dismissal frame patches the keep");
    assert!(
        patch.contains("pointer-events:none"),
        "ghost is click-through on the wire, got {patch}"
    );
    assert!(
        patch.contains("opacity:"),
        "first stepped values ride the keep frame, got {patch}"
    );
    // Mid-exit, relative to the live clock: the fade steps through
    // patch frames.
    let t0 = app.host().now_ms();
    let patch = app.tick(t0 + 90.0).expect("fading frames keep patching");
    assert!(
        patch.contains("pointer-events:none"),
        "ghost stays click-through mid-exit, got {patch}"
    );
    assert!(
        patch.contains("opacity:"),
        "mid-exit steps ride the wire, got {patch}"
    );
    // Past the 180ms exit: the ghost drops (deletion patches too).
    let patch = app.tick(t0 + 1000.0).expect("drop frame patches");
    assert!(
        !patch.contains("pointer-events:none"),
        "settled ghost leaves the wire, got {patch}"
    );
    // Settled frames go quiet again.
    assert_eq!(app.tick(t0 + 2000.0), None, "quiet after the exit");
}
