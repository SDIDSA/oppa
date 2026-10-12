//! Desktop entry (Windows + Linux, same source): one line over the
//! shared `app::sera_root`. Run with `cargo run` (or
//! `cargo oppa run --desktop`).

mod app;

use oppa_app::desktop_main;

desktop_main!("Sera — Sign in", 1280, 800, app::sera_root);
