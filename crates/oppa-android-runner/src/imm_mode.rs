//! Immersive fullscreen over JNI (phone hardening): hides the
//! system navigation bar so the window is the full display
//! (1080x2400), not the inset region (1080x2290 on a 3-button-nav
//! device). Without this the strict size check fails loudly and
//! nothing presents — correct but useless on phones with system
//! bars; a real app owns its window.
//!
//! Runtime screen-mode toggle: apps flip the bars from component
//! code (`Ctx::set_system_bars_visible`) and the runner drains the
//! host request once per pump iteration (see
//! `drain_system_bars_request` below) — the scene stays full-bleed
//! either way, since both Java halves keep the explicit
//! full-display size plus `FLAG_LAYOUT_NO_LIMITS` and the
//! short-edges cutout, so showing the bars overlays them instead of
//! resizing the native window and no refit/repaint is owed.
//!
//! Threading (learned from a tombstone — hard requirement):
//! `View`/`Window` bars calls throw
//! `CalledFromWrongThreadException` off the UI thread, which ART
//! escalates to SIGABRT on the next JNI call (fatal, uncatchable
//! from Rust, no `error.txt`). NativeActivity Rust has no UI-thread
//! hook, so the whole sequence lives in `OppaUi.hideBars` /
//! `OppaUi.showBars` (app Java source, `runOnUiThread`); Rust makes
//! exactly ONE JNI call — the static — which is thread-safe. Success
//! is verified by the window size afterward, never assumed from
//! these calls.

use android_activity::AndroidApp;

/// One bars call over the UI-thread hop (`OppaUi.hideBars` /
/// `OppaUi.showBars`). `java_package` is the app's dotted package
/// (`OppaUi` resolves inside it — every app ships its own copy under
/// its own package). Returns the record. Any failure is a loud `Err`.
fn call_bars_method(app: &AndroidApp, java_package: &str, method: &str) -> Result<String, String> {
    // Shared attach preamble + class loader (one copy in
    // `ime_bridge` — the tombstone notes below apply to both).
    // Class-loader note (tombstone-proven, twice): `find_class`
    // from an attached native thread resolves against the SYSTEM
    // loader, and `NativeActivity.getClass().getClassLoader()` is
    // the BOOT loader (framework class) — both miss app classes
    // that ARE in the APK. App classes load through a
    // `DexClassLoader` over our own APK path.
    // Dispatch marker (device profiling: logcat timestamps split
    // tap→dispatch (queue + pump + router + drain, Rust side) from
    // dispatch→post-log (JNI + UI-thread hop, Java side) — one line
    // per explicit toggle, never per frame.
    eprintln!("oppa-bars: dispatch {method}");
    super::ime_bridge::with_attached(app, |env, activity| {
        let cls_obj =
            super::ime_bridge::load_app_class(env, &activity, &format!("{java_package}.OppaUi"))?;
        let cls: &jni::objects::JClass = (&cls_obj).into();
        // The single JNI call chain: the whole bars sequence runs on
        // the UI thread inside `OppaUi` (any direct View/Window call
        // from this thread is process-fatal — see the module docs).
        env.call_static_method(
            cls,
            method,
            "(Landroid/app/Activity;)V",
            &[jni::objects::JValue::from(&activity)],
        )
        .map_err(|e| format!("OppaUi.{method}: {e}"))?;
        Ok(format!("bars={method}-ui-thread-hop"))
    })
}

/// Requests immersive-sticky fullscreen through the UI-thread hop.
/// Returns the record. Any failure is a loud `Err`.
pub fn hide_system_bars(app: &AndroidApp, java_package: &str) -> Result<String, String> {
    call_bars_method(app, java_package, "hideBars")
}

/// Requests the status + navigation bars shown through the
/// UI-thread hop (the hide mirror — the window keeps its explicit
/// full-display size, so the scene never resizes). Returns the
/// record. Any failure is a loud `Err`.
pub fn show_system_bars(app: &AndroidApp, java_package: &str) -> Result<String, String> {
    call_bars_method(app, java_package, "showBars")
}

/// Applies one mode through the bridge (runner-side — heals and the
/// initial assert call this with the runner-tracked mode; a refused
/// bridge logs loudly and continues, never failing the run).
pub fn apply_system_bars(app: &AndroidApp, java_package: &str, visible: bool) {
    let r = if visible {
        show_system_bars(app, java_package)
    } else {
        hide_system_bars(app, java_package)
    };
    if let Err(e) = r {
        eprintln!("oppa-bars: apply visible={visible} refused [{e}]");
    }
}

/// Drains the app-requested bars mode (runner-side — pumps call this
/// once per iteration): a pending
/// [`ComponentHost::take_system_bars_request`](oppa::ComponentHost::take_system_bars_request)
/// applies through the bridge and becomes the tracked `current`
/// (last write wins — rapid flips apply once); `None` leaves
/// `current` untouched (no request, no bridge call).
pub fn drain_system_bars_request(
    host: &oppa::ComponentHost,
    app: &AndroidApp,
    java_package: &str,
    current: &mut bool,
) {
    if let Some(visible) = host.take_system_bars_request() {
        apply_system_bars(app, java_package, visible);
        *current = visible;
    }
}
