//! Web entry: exports the shared `app::login_root` through the
//! standard [`export_app`](oppa_web::export_app) shell (wasm only —
//! other targets use this crate's API directly). Run with
//! `cargo oppa serve` (builds `--lib`, so the desktop `main` never
//! compiles to wasm).
//!
//! Android entry: mounts the same root through the shared runner
//! (one line — `cargo oppa build --android` links it into the
//! activity `.so`).

mod app;

// The portable surface (host tests, Android builds, and any future
// runner drive this — the wasm export below is only the browser
// binding).
pub use app::{email_error, login_root, password_error, LoginState};

#[cfg(target_arch = "wasm32")]
use oppa_web::export_app;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
export_app!(LoginWebApp, "Login", app::login_root);

#[cfg(target_os = "android")]
oppa_android_runner::android_main!(app::login_root, "com.oppa.login");
