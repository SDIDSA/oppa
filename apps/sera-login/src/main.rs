//! Desktop entry (Windows + Linux, same source): always `Extended`
//! with no native buttons and no in-app bar at all — the page is
//! flexible space, the centered card, and the footer, while the OS
//! keeps managing the window (borders resize, snapping/tiling
//! works). Close/minimize/maximize stay on the OS: Alt+F4, taskbar,
//! snap layouts. Run with `cargo run` (or `cargo oppa run
//! --desktop`).

use oppa_app::{run_desktop_with, WindowOptions};
use oppa_controls::{CaptionAction, CaptionOutbox};
use sera_login::{sera_extended_root, SeraExtendedProps};

fn main() {
    // Always Extended, never native buttons: the window style and the
    // caption zones are both app-source decisions (no flags, no
    // per-OS branches — one shape on Windows and Linux).
    let hooks = CaptionOutbox::default();
    let props = SeraExtendedProps {
        hooks: hooks.clone(),
        native: false,
    };
    if let Err(e) = run_desktop_with(
        WindowOptions::new("Sera — Sign in", 1400, 770)
            .extended()
            .with_native_caption_buttons(false)
            .with_app_fonts(oppa_fonts::READEX_PRO_FAMILY, oppa_fonts::READEX_PRO_FACES),
        props,
        sera_extended_root,
        |loop_| {
            // Caption bridge (the shared `CaptionOutbox` shape):
            // kept for the shape — with no buttons declared the
            // outbox never arms, so the hook drains nothing and the
            // bar stays a pure drag region.
            let poll_hooks = hooks.clone();
            loop_.set_poll_hook(Some(Box::new(move |loop_| {
                for action in poll_hooks.take_actions() {
                    match action {
                        CaptionAction::Minimize => loop_.minimize(),
                        CaptionAction::Maximize => loop_.toggle_maximize(),
                        CaptionAction::Close => loop_.host().request_close(),
                    }
                }
            })));
        },
    ) {
        eprintln!("sera-login: FATAL: {e}");
        std::process::exit(1);
    }
}
