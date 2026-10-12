//! Unified kitchen sink application (Phase 7, decision 276):
//! the reference cross-platform app exercising every framework
//! capability — form controls, flex layout, visual effects,
//! overlays, file picking, persistent storage, and async fetch.
//!
//! Run: `cargo run -p oppa-controls --example kitchen_sink`
//! (Windows/Linux desktops; needs a display server). The same
//! root mounts headlessly in `oppa-testkit`, on Web through
//! `WebApp::new_with_root`, and on Android through `mount_app`.
//!
//! File picking (decision 425): the configure closure forwards the
//! live loop handles (native Win32/COM + portal/zenity dialogs)
//! into the props' dialog slots, so the Platform tab drives real
//! OS pickers here — every other runner keeps the scripted
//! default.

use oppa_app::{run_desktop_with, WindowOptions};
use oppa_controls::{KitchenSinkApp, KitchenSinkProps};

fn main() {
    let props = KitchenSinkProps::default();
    let fill = props.dialogs.clone();
    if let Err(e) = run_desktop_with(
        WindowOptions::new("Oppa Kitchen Sink", 600, 700),
        props,
        KitchenSinkApp,
        move |loop_| {
            let mut slots = fill.borrow_mut();
            if let Some(handle) = loop_.open_dialog_handle() {
                slots.open = handle;
            }
            if let Some(handle) = loop_.save_dialog_handle() {
                slots.save = handle;
            }
            if let Some(handle) = loop_.folder_dialog_handle() {
                slots.folder = handle;
            }
        },
    ) {
        eprintln!("kitchen_sink: FATAL: {e}");
        std::process::exit(1);
    }
}
