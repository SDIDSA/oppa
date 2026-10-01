//! Android entry: `android_main` (required by `android-activity`)
//! funnels the `AndroidApp` into the shared driver, which builds
//! the winit loop around it. The scene core is identical to every
//! other platform; suspend/resume is handled by the driver's
//! teardown/setup (the `NativeActivity` window comes and goes).

use android_activity::AndroidApp;

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    crate::driver::run_android(app, None);
}
