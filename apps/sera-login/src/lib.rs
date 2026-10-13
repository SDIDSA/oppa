//! Web entry: exports the shared `app::sera_root` through the
//! font-overriding [`export_app`](oppa_web::export_app) shell (wasm
//! only — other targets use this crate's API directly): the browser
//! measures the Readex Pro statics and displays the variable face
//! under the same family. Run with `cargo oppa serve` (builds
//! `--lib`, so the desktop `main` never compiles to wasm).
//!
//! Android entry: mounts the same root through the shared runner
//! (one line — `cargo oppa build --android` links it into the
//! activity `.so`; the Android runner has no app-font override yet,
//! so Android keeps its system fonts — stated, never silent).

mod app;

// The portable surface (host tests, Android builds, and any future
// runner drive this — the wasm export below is only the browser
// binding).
pub use app::{
    default_icons, default_logins, deposit_icons, email_error, password_error, sera_extended_root,
    sera_root, Icons, SavedLogin, SeraApp, SeraExtendedProps, SeraState,
};
// Vector icons ride the shared framework helper now (decision 510) —
// re-exported here so harnesses keep one import root.
pub use oppa_controls::VectorIcon;

pub use app::APP_NAME;

/// Readex Pro shaping family for the Sera app (desktop
/// `WindowOptions::with_app_fonts` + the web shell below read the
/// same constants, so every wired runner agrees).
pub const SERA_FONT_FAMILY: &str = oppa_fonts::READEX_PRO_FAMILY;
/// Static measuring faces in weight order.
pub const SERA_FONT_FACES: &[(&str, &[u8])] = oppa_fonts::READEX_PRO_FACES;
/// Variable display face for the browser `@font-face`.
pub const SERA_FONT_DISPLAY: &[u8] = oppa_fonts::READEX_PRO_VARIABLE;

#[cfg(target_arch = "wasm32")]
use oppa_web::export_app;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
export_app!(
    SeraWebApp,
    "Sera",
    app::sera_root,
    SERA_FONT_FAMILY,
    SERA_FONT_FACES,
    SERA_FONT_DISPLAY
);

#[cfg(target_os = "android")]
oppa_android_runner::android_main!(app::sera_root, "com.oppa.seralogin");
