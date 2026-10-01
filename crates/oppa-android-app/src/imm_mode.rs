//! Immersive fullscreen over JNI (phone hardening): hides the
//! system navigation bar so the window is the full display
//! (1080x2400), not the inset region (1080x2290 on a 3-button-nav
//! device). Without this the strict size check fails loudly and
//! nothing presents — correct but useless on phones with system
//! bars; a real app owns its window.
//!
//! Threading (learned from a tombstone — hard requirement):
//! `View`/`Window` bars calls throw
//! `CalledFromWrongThreadException` off the UI thread, which ART
//! escalates to SIGABRT on the next JNI call (fatal, uncatchable
//! from Rust, no `error.txt`). NativeActivity Rust has no UI-thread
//! hook, so the whole sequence lives in `OppaUi.hideBars`
//! (app Java source, `runOnUiThread`); Rust makes exactly ONE JNI
//! call — the static — which is thread-safe. Success is verified
//! by the window size afterward, never assumed from these calls.

use android_activity::AndroidApp;

/// Requests immersive-sticky fullscreen through the UI-thread hop.
/// Returns the record. Any failure is a loud `Err`.
pub fn hide_system_bars(app: &AndroidApp) -> Result<String, String> {
    let vm_ptr = app.vm_as_ptr() as *mut jni::sys::JavaVM;
    if vm_ptr.is_null() {
        return Err("JavaVM pointer is null".to_string());
    }
    // SAFETY: live NativeActivity VM, outlives the call.
    let vm =
        unsafe { jni::JavaVM::from_raw(vm_ptr) }.map_err(|e| format!("JavaVM::from_raw: {e}"))?;
    let mut env = vm
        .attach_current_thread_permanently()
        .map_err(|e| format!("attach thread: {e}"))?;
    let activity =
        unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    if activity.is_null() {
        return Err("Activity pointer is null".to_string());
    }
    // The single JNI call chain: the whole bars sequence runs on
    // the UI thread inside `OppaUi.hideBars` (any direct
    // View/Window call from this thread is process-fatal — see the
    // module docs).
    //
    // Class-loader note (tombstone-proven, twice): `find_class`
    // from an attached native thread resolves against the SYSTEM
    // loader, and `NativeActivity.getClass().getClassLoader()` is
    // the BOOT loader (framework class) — both miss app classes
    // that ARE in the APK. App classes load through a
    // `DexClassLoader` over our own APK path
    // (`ApplicationInfo.sourceDir`, a public field read off the
    // activity — no loader needed for any step here).
    let app_info = env
        .call_method(
            &activity,
            "getApplicationInfo",
            "()Landroid/content/pm/ApplicationInfo;",
            &[],
        )
        .map_err(|e| format!("getApplicationInfo: {e}"))?
        .l()
        .map_err(|e| format!("app info: {e}"))?;
    let apk_obj = env
        .get_field(&app_info, "sourceDir", "Ljava/lang/String;")
        .map_err(|e| format!("sourceDir: {e}"))?
        .l()
        .map_err(|e| format!("apk path: {e}"))?;
    let apk: String = env
        .get_string(&jni::objects::JString::from(apk_obj))
        .map_err(|e| format!("apk string: {e}"))?
        .into();
    let cache_dir = env
        .call_method(&activity, "getCodeCacheDir", "()Ljava/io/File;", &[])
        .map_err(|e| format!("getCodeCacheDir: {e}"))?
        .l()
        .map_err(|e| format!("code cache: {e}"))?;
    let cache_path_obj = env
        .call_method(&cache_dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .map_err(|e| format!("cache path: {e}"))?
        .l()
        .map_err(|e| format!("cache string obj: {e}"))?;
    let cache_path: String = env
        .get_string(&jni::objects::JString::from(cache_path_obj))
        .map_err(|e| format!("cache string: {e}"))?
        .into();
    let dex_cls = env
        .find_class("dalvik/system/DexClassLoader")
        .map_err(|e| format!("find DexClassLoader: {e}"))?;
    let cl_cls = env
        .find_class("java/lang/ClassLoader")
        .map_err(|e| format!("find ClassLoader: {e}"))?;
    let sys_loader = env
        .call_static_method(
            &cl_cls,
            "getSystemClassLoader",
            "()Ljava/lang/ClassLoader;",
            &[],
        )
        .map_err(|e| format!("system loader: {e}"))?
        .l()
        .map_err(|e| format!("sys loader obj: {e}"))?;
    let apk_jstr = env
        .new_string(&apk)
        .map_err(|e| format!("apk jstring: {e}"))?;
    let cache_jstr = env
        .new_string(&cache_path)
        .map_err(|e| format!("cache jstring: {e}"))?;
    let dex_loader = env
        .new_object(
            &dex_cls,
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/ClassLoader;)V",
            &[
                jni::objects::JValue::from(&apk_jstr),
                jni::objects::JValue::from(&cache_jstr),
                jni::objects::JValue::from(&jni::objects::JObject::null()),
                jni::objects::JValue::from(&sys_loader),
            ],
        )
        .map_err(|e| format!("new DexClassLoader: {e}"))?;
    let name = env
        .new_string("com.oppa.app.OppaUi")
        .map_err(|e| format!("new_string: {e}"))?;
    let cls_obj = env
        .call_method(
            &dex_loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[jni::objects::JValue::from(&name)],
        )
        .map_err(|e| format!("loadClass OppaUi: {e}"))?
        .l()
        .map_err(|e| format!("OppaUi class: {e}"))?;
    let cls: &jni::objects::JClass = (&cls_obj).into();
    env.call_static_method(
        cls,
        "hideBars",
        "(Landroid/app/Activity;)V",
        &[jni::objects::JValue::from(&activity)],
    )
    .map_err(|e| format!("OppaUi.hideBars: {e}"))?;
    Ok("immersive=ui-thread-hop".to_string())
}
