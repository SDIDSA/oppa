//! Desktop entry (Windows + Linux, same source): one line over the
//! shared `app::login_root`. Run with `cargo run`.

mod app;

use oppa_app::desktop_main;

desktop_main!("Sign in", 440, 640, app::login_root);
