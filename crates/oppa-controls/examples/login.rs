//! Portable login page — desktop entry (Windows + Linux, same source).
//!
//! The app code is `oppa_controls::{login_root, LoginApp, LoginState}`
//! (`crates/oppa-controls/src/login.rs`): no `cfg`, no shell imports,
//! no file/clipboard/GPU code. Web mounts the same `login_root` via
//! `WasmHost::mount_root("Login", (), login_root)`; Android mounts it
//! in `SceneCore` the same way.
//!
//! Run: `cargo run -p oppa-controls --example login`
//! (`OPPA_RENDERER=cpu` forces software when the GPU refuses).

use oppa_app::{run_desktop, WindowOptions};
use oppa_controls::login_root;

fn main() {
    if let Err(e) = run_desktop(WindowOptions::new("Sign in", 400, 420), (), login_root) {
        eprintln!("login: FATAL: {e}");
        std::process::exit(1);
    }
}
