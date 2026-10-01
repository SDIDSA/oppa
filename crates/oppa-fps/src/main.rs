//! Native entry (Windows, Linux, Android): run the shared driver.
//! Android arrives here through cargo-apk's NativeActivity glue
//! (plain `main` on the activity thread); no JNI in this crate.
//! (Web uses the cdylib `start` in `web.rs`, not this binary.)

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    oppa_fps::driver::run(None);
}

/// Placeholder bin for the wasm target (the module entry is the
/// cdylib `start`); never executed.
#[cfg(target_arch = "wasm32")]
fn main() {}
