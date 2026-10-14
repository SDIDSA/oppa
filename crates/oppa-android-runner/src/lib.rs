//! Generic interactive Android runner: NativeActivity lifecycle +
//! engine + surface present + touch + text + IME + fullscreen for
//! any root component.
//!
//! The proof harness (`crates/oppa-android-app`) owns the benchmark
//! phases, scripted taps, oracle records, and picker validation —
//! everything proof-only stays there. What every app needs to boot
//! and interact lives here: the touch intake, the platform font
//! query, the immersive/IME JNI bridges, and the event-paced
//! present loop over the shared [`SceneCore`] engine. The proof
//! crate uses these same modules (no second copies).
//!
//! On-device boot proven on RMX3370/API 31 (`apps/login` renders
//! and interacts: cold window ~190 ms, content ~750 ms). Host and
//! android-target compile gates hold alongside.

pub mod clipboard_jni;
pub mod fonts_jni;
pub mod ime_bridge;
pub mod imm_mode;
pub mod present;
pub mod soft_present;
pub mod sysprop;
pub mod touch;

/// Re-exported for [`android_main!`] users (app crates name only
/// this crate, never `android-activity` directly).
pub use android_activity::AndroidApp;

/// Standard Android entry: generates the `android_main` symbol
/// mounting `root` (a `()`-props component) through
/// [`run_android`]. Custom props need a hand-written entry (the
/// documented escape hatch).
///
/// ```rust,ignore
/// use oppa_android_runner::android_main;
/// android_main!(app::login_root, "com.oppa.login");
/// ```
///
/// Native-only by construction (expands to nothing off Android —
/// host and wasm checks never see it).
#[macro_export]
macro_rules! android_main {
    ($root:path, $package:literal) => {
        #[unsafe(no_mangle)]
        #[cfg(target_os = "android")]
        fn android_main(app: ::oppa_android_runner::AndroidApp) {
            if let Err(e) = ::oppa_android_runner::run_android((), $root, app, $package) {
                eprintln!("oppa-android: FATAL: {e}");
            }
        }
    };
}

/// Boots the engine for any root and runs the interactive loop
/// until Destroy: `/system/fonts` text service with every face
/// injected into both backends before first paint, GPU arm pinned
/// (zygote env never decides it), density rebase from the live
/// config, IME callbacks registered, immersive requested, then
/// [`present::drive_interactive`]. Every failure is a loud `Err`
/// (never a black window standing in for one).
pub fn run_android<P: oppa::Props>(
    props: P,
    render: fn(&oppa::Ctx, &P) -> oppa::VNode,
    app: AndroidApp,
    java_package: &str,
) -> Result<(), String> {
    use oppa::text::TextService;
    use std::path::Path;
    use std::time::Instant;
    let t_boot = Instant::now();
    // Font loading runs on a loader thread while the main thread
    // waits for the window below (fonts are window-independent —
    // ~400 ms of scan+parse hides inside the window wait instead of
    // serializing before it). The join lands before mount, so
    // operation order is unchanged, only overlapped. A panicking
    // loader fails loudly at the join, never as missing fonts.
    let font_thread = std::thread::spawn(|| {
        let (texts, skipped) =
            oppa_text_android::AndroidTextService::from_dir(Path::new("/system/fonts"))
                .map_err(|e| format!("android fonts: {e}"))?;
        Ok::<_, String>((texts, skipped))
    });
    // Density + first window while fonts load (no scene needed —
    // the present loop re-acquires per generation anyway, so this
    // wait is purely overlap, never a second source of truth).
    let mut touch =
        touch::TouchDriver::new(&app, 1080.0, 2400.0).map_err(|e| format!("touch driver: {e}"))?;
    let density = touch.density();
    let (_window, _ww, _wh) = match present::wait_for_window(&app, java_package) {
        Ok(w) => w,
        Err(e) => {
            let _ = font_thread.join();
            return Err(e);
        }
    };
    let (texts, skipped) = font_thread
        .join()
        .map_err(|_| "font loader panicked".to_string())??;
    if !skipped.is_empty() {
        eprintln!("oppa-android: skipped font files: {skipped:?}");
    }
    let families = texts.families().to_vec();
    eprintln!("oppa-android: font families: {families:?}");
    // Staged injection (start-time optimization): the boot set
    // (Roboto + Noto Sans Symbols — the layout family plus the
    // symbol fallback the Round 7.6 chevron panic mandated) injects
    // now, so first paint shapes; every other face injects after
    // the first present on the same thread (a first paint shaping
    // outside the boot set refuses loudly instead of tofu — the
    // never-silent contract — so CJK-first apps keep the eager
    // proof path in `oppa-android-app`, never this runner).
    // Empty boot set falls back to eager (a missing Roboto is a
    // broken image, not a slow one).
    let mut boot_pairs: Vec<(oppa::text::FontId, std::sync::Arc<[u8]>, u32)> = Vec::new();
    let mut rest_pairs: Vec<(oppa::text::FontId, std::sync::Arc<[u8]>, u32)> = Vec::new();
    for info in texts.enumerate_fonts() {
        let boot = info.family == "Roboto" || info.family == "Noto Sans Symbols";
        if let Some((bytes, index)) = texts.face_bytes(info.id) {
            // Shared (decisions 437+439): the service hands out one
            // allocation per file — no copy at any layer.
            if boot {
                boot_pairs.push((info.id, bytes, index));
            } else {
                rest_pairs.push((info.id, bytes, index));
            }
        }
    }
    if boot_pairs.is_empty() {
        eprintln!("oppa-android: no Roboto boot faces — eager fallback");
        std::mem::swap(&mut boot_pairs, &mut rest_pairs);
    }
    eprintln!(
        "oppa-boot: fonts +{}ms (boot={} rest={})",
        t_boot.elapsed().as_millis(),
        boot_pairs.len(),
        rest_pairs.len()
    );
    let mut scene = oppa_app::SceneCore::new(1080, 2400, Box::new(texts), "Roboto")
        .map_err(|e| format!("scene: {e}"))?;
    for (id, bytes, index) in boot_pairs {
        scene.set_font_for(id, bytes, index);
    }
    scene.enable_gpu_pinned(oppa_app::RendererKind::GpuAuto, wgpu::PresentMode::Fifo);
    let _ = scene.host().mount("MobileApp", props, render);
    // Decision 515: bounded (mounts jump — quiesces immediately).
    scene.host().pump_frames(oppa_app::PUMP_INPUT_FRAMES);
    eprintln!("oppa-boot: mount-idle +{}ms", t_boot.elapsed().as_millis());
    // Density rebase (driver already made pre-window for overlap —
    // same rule, same place in the order).
    scene
        .set_device_pixel_ratio(density)
        .map_err(|e| format!("density: {e}"))?;
    // IME callbacks before any field can focus (registration is
    // idempotent — second call records, never re-registers).
    eprintln!(
        "oppa-android: {}",
        ime_bridge::ensure_ime_callbacks(&app, java_package)?
    );
    eprintln!("oppa-boot: pre-present +{}ms", t_boot.elapsed().as_millis());
    let record = present::drive_interactive(
        &app,
        &mut scene,
        &mut touch,
        density,
        java_package,
        rest_pairs,
    )
    .map_err(|e| format!("present loop: {e}"))?;
    eprintln!("oppa-android: {record}");
    Ok(())
}
