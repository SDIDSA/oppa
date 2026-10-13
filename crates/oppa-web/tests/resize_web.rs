//! Web resize (decision 470): the scene follows its container.
//!
//! Headless (no browser): validation refuses loudly, zero-area
//! parks, same-size stays quiet, and a real change reflows through
//! the refit contract as a full reload with geometry following the
//! new viewport. The browser leg (observer → binding → patch →
//! applier across viewport sizes) rides `spike/web/resize.mjs`.

use oppa::{Ctx, Div, Style, VNode};
use oppa_web::WasmHost;

/// Viewport-wide probe strip: its committed width always equals the
/// live viewport width (portal content spans the viewport, and the
/// fill strip spans its parent).
fn probe_root(_ctx: &Ctx, _props: &()) -> VNode {
    oppa::Portal("vp").child(
        oppa::Column::new()
            .style(Style::new().fill_width().fill_height())
            .children([Div("vp-mark")
                .style(Style::new().fill_width().h(10))
                .build()]),
    )
}

fn mount_probe() -> WasmHost {
    WasmHost::mount_root("Probe", (), probe_root)
}

fn mark_width(app: &WasmHost) -> f32 {
    let id = oppa::find_retained_by_debug(app.host(), "vp-mark")
        .into_iter()
        .next()
        .expect("probe strip retained");
    app.host()
        .committed_box(id)
        .expect("probe strip laid out")
        .w
}

#[test]
#[should_panic(expected = "refused")]
fn resize_nan_width_panics() {
    mount_probe().resize(f32::NAN, 600.0, 1.0);
}

#[test]
#[should_panic(expected = "refused")]
fn resize_negative_height_panics() {
    mount_probe().resize(800.0, -1.0, 1.0);
}

#[test]
#[should_panic(expected = "refused")]
fn resize_zero_dpr_panics() {
    mount_probe().resize(800.0, 600.0, 0.0);
}

#[test]
#[should_panic(expected = "refused")]
fn resize_nan_dpr_panics() {
    mount_probe().resize(800.0, 600.0, f32::NAN);
}

#[test]
fn resize_parks_on_zero_area() {
    let mut app = mount_probe();
    assert_eq!(app.resize(0.0, 600.0, 1.0), None, "zero width parks");
    assert_eq!(app.resize(800.0, 0.0, 1.0), None, "zero height parks");
    assert_eq!(
        app.host().viewport_size(),
        (800.0, 600.0),
        "parked resize keeps the last viewport"
    );
    assert_eq!(
        mark_width(&app),
        800.0,
        "parked resize lays out nothing new"
    );
}

#[test]
fn resize_quiet_on_same_size() {
    let mut app = mount_probe();
    assert_eq!(
        app.resize(800.0, 600.0, 1.0),
        None,
        "boot viewport + boot DPR is a no-op"
    );
}

#[test]
fn resize_reflows_and_reloads() {
    let mut app = mount_probe();
    assert_eq!(mark_width(&app), 800.0, "probe starts at boot width");
    let patch = app
        .resize(1280.0, 800.0, 1.0)
        .expect("real change repaints");
    assert!(
        patch.contains("\"full\":true"),
        "resize replays from zero through the refit contract"
    );
    assert_eq!(app.host().viewport_size(), (1280.0, 800.0));
    assert_eq!(
        mark_width(&app),
        1280.0,
        "geometry follows the new viewport"
    );
    // And back: every distinct size relays, none sticks.
    let back = app.resize(800.0, 600.0, 1.0).expect("shrink repaints");
    assert!(back.contains("\"full\":true"));
    assert_eq!(app.host().viewport_size(), (800.0, 600.0));
    assert_eq!(mark_width(&app), 800.0);
}

#[test]
fn resize_dpr_only_reflows() {
    let mut app = mount_probe();
    let patch = app
        .resize(800.0, 600.0, 2.0)
        .expect("DPR flip with identical CSS sizes still reflows");
    assert!(patch.contains("\"full\":true"));
    assert_eq!(app.host().layout_config().device_pixel_ratio, 2.0);
    assert_eq!(
        app.resize(800.0, 600.0, 2.0),
        None,
        "matching DPR reading stays quiet"
    );
    // Family survives the read-modify-write (never reconstructed
    // from defaults).
    assert!(app.host().layout_config().family.contains("DejaVu"));
}
