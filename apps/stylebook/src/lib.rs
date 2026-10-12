//! Stylebook — one elegant root mounted under every
//! [`WindowStyle`](oppa::WindowStyle).
//!
//! A `HeaderBar` (frameless modes only — `Decorated` keeps the native
//! title bar with no duplicate in-app chrome) over a centered card:
//! the style name, its one-line contract, a counter proving the
//! scene stays interactive under each chrome, and the exact command
//! that launched it. The root paints no page background by design —
//! the surface clears to the theme color on opaque styles, and the
//! desktop shows through around the floating card on `Transparent`.
//!
//! - Desktop: `cargo run -p stylebook -- --style extended`
//!   (`decorated` | `undecorated` | `transparent` | `extended`,
//!   default `extended`).
//! - Web: `cargo oppa serve` (rides `Decorated` — window chrome is
//!   browser-owned there).
//! - Android: mount through the shared runner (see the `login` app
//!   README for the recipe; needs a device).

// Control components are `CamelCase` functions by framework convention
// (`#[component] fn Name` — M2 authoring surface).
#![allow(non_snake_case)]

mod app;

pub use app::{style_blurb, style_title, ChromeHooks, StylebookProps, StylebookRoot};

#[cfg(target_arch = "wasm32")]
use oppa_web::export_app;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
export_app!(StylebookWebApp, "Stylebook", app::StylebookRoot);

#[cfg(target_os = "android")]
oppa_android_runner::android_main!(app::StylebookRoot, "com.oppa.stylebook");
