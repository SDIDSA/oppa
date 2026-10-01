//! Unified kitchen sink application (Phase 7, decision 276):
//! the reference cross-platform app exercising every framework
//! capability — form controls, flex layout, visual effects,
//! overlays, file picking, persistent storage, and async fetch.
//!
//! Run: `cargo run -p oppa-controls --example kitchen_sink`
//! (Windows/Linux desktops; needs a display server). The same
//! root mounts headlessly in `oppa-testkit`, on Web through
//! `WebApp::new_with_root`, and on Android through `mount_app`.

use oppa_app::{run_desktop, WindowOptions};
use oppa_controls::KitchenSinkApp;

fn main() {
    if let Err(e) = run_desktop(
        WindowOptions::new("Oppa Kitchen Sink", 600, 700),
        (),
        KitchenSinkApp,
    ) {
        eprintln!("kitchen_sink: FATAL: {e}");
        std::process::exit(1);
    }
}
