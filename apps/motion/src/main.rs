//! Desktop entry (Windows + Linux, same source): mounts the shared
//! `app::motion_root` with the FPS counter armed (the frame-cost
//! readout for the motion rooms). Run with `cargo run` (or
//! `cargo oppa run --desktop`).

mod app;

use oppa_app::{run_desktop_with, WindowOptions};

fn main() {
    if let Err(e) = run_desktop_with(
        WindowOptions::new("Motion Lab", 1100, 750).with_fps_counter(true),
        (),
        app::motion_root,
        |_| {},
    ) {
        eprintln!("motion: FATAL: {e}");
        std::process::exit(1);
    }
}
