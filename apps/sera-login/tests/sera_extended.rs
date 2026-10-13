//! Headless proof of the `Extended` desktop frame
//! (`sera_extended_root`): the bare native frame with the custom top
//! strip as its only chrome (transparent, click-through, no native
//! buttons) over the centered card. The portable `sera_root` mounts
//! the same strip under system chrome (web/Android mount that).

use oppa::WindowStyle;
use oppa_controls::CaptionOutbox;
use oppa_testkit::Harness;
use sera_login::{sera_extended_root, sera_root, SeraExtendedProps};

fn mount_extended() -> (Harness, CaptionOutbox) {
    mount_extended_with(true)
}

fn mount_extended_with(native: bool) -> (Harness, CaptionOutbox) {
    let app = Harness::new();
    app.set_viewport(1280.0, 800.0);
    let hooks = CaptionOutbox::default();
    app.mount(
        "Sera",
        SeraExtendedProps {
            hooks: hooks.clone(),
            native,
        },
        sera_extended_root,
    );
    (app, hooks)
}

#[test]
fn extended_mounts_transparent_strip_chrome() {
    let (app, _) = mount_extended();
    // One transparent strip: the HeaderBar drag marker holding the
    // brand + icon cluster, and no caption buttons anywhere.
    for debug in [
        "oppa::HeaderBar",
        "sera-bar",
        "sera-bar-brand",
        "sera-bar-mark",
        "sera-bar-sys",
        "sys-bell",
        "sys-info",
        "sys-cloud",
        "sys-more",
        "sera-sys-end",
    ] {
        app.node(debug);
    }
    for debug in ["header-min", "header-max", "header-close", "sera-top"] {
        assert!(
            oppa::find_retained_by_debug(app.host(), debug).is_empty(),
            "{debug} leaves no trace under the custom strip"
        );
    }
    // The same split card lives below the strip.
    app.node("sera-card");
    app.node("sera-submit");
    app.node("sera-foot");
}

#[test]
fn extended_strip_publishes_drag_region_without_zones() {
    let (app, _) = mount_extended();
    assert!(
        !oppa::drag_regions_for_style(app.host(), WindowStyle::Extended).is_empty(),
        "the click-through strip publishes an OS drag region"
    );
    assert!(
        oppa::drag_regions_for_style(app.host(), WindowStyle::Decorated).is_empty(),
        "style gating holds: the same tree publishes nothing as Decorated"
    );
    assert!(
        oppa::native_caption_zones(app.host()).is_empty(),
        "no declared buttons means no zones"
    );
}

#[test]
fn extended_caption_outbox_publishes_but_idles() {
    let (app, hooks) = mount_extended();
    // The root still publishes its caption outbox (the bridge
    // shape), but with no buttons declared nothing can ever arm it.
    let Some((minimize, maximize, close)) = hooks.get() else {
        panic!("the root publishes its caption outbox on first render");
    };
    assert!(!minimize.get() && !maximize.get() && !close.get());
    app.run_idle();
    assert!(!minimize.get() && !maximize.get() && !close.get());
}

#[test]
fn extended_card_rows_select_and_delete() {
    let (app, _) = mount_extended();
    app.node("saved-3");
    app.tap("delete-3");
    assert!(
        oppa::find_retained_by_debug(app.host(), "saved-3").is_empty(),
        "delete drops the row under Extended chrome too"
    );
    // Empty submit takes the error path without panic.
    app.tap("sera-submit");
    app.node("sera-card");
}

#[test]
fn extended_without_native_buttons_draws_none() {
    let (app, _) = mount_extended_with(false);
    // The strip is identical with or without native buttons (the
    // flag is inert — the strip alone is the chrome either way).
    for debug in ["header-min", "header-max", "header-close"] {
        assert!(
            oppa::find_retained_by_debug(app.host(), debug).is_empty(),
            "{debug} leaves no trace with native buttons off"
        );
    }
    assert!(
        oppa::native_caption_zones(app.host()).is_empty(),
        "no buttons means no zones"
    );
    app.node("sera-bar");
    app.node("sera-card");
}

#[test]
fn portable_root_mounts_shared_strip_without_chrome() {
    let app = Harness::new();
    app.mount("Sera", (), sera_root);
    assert!(
        oppa::find_retained_by_debug(app.host(), "oppa::HeaderBar").is_empty(),
        "the portable tree carries no drag chrome"
    );
    assert!(
        oppa::drag_regions_for_style(app.host(), WindowStyle::Extended).is_empty(),
        "no header means no drag region, even gated Extended"
    );
    // The shared strip renders as a plain row (no header marker).
    app.node("sera-bar");
    app.node("sera-card");
}
