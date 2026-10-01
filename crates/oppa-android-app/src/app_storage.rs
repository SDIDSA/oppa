//! Scoped-storage JNI getters + device wiring (Round 3.4).
//!
//! The host-provable half lives in `oppa-shell-android`
//! (`resolve_app_dirs` fallback selection, `validate_dirs`
//! round-trip, both unit-tested): this module holds only what
//! needs a VM — the `getFilesDir`/`getCacheDir` strings — plus
//! the one device entry that feeds them into the shell logic and
//! records the outcome.
//!
//! Fallback order (the shell decides — see `resolve_app_dirs`):
//! explicit JNI path, else `internal_data_path`, else loud `Err`
//! (no `/tmp` on device).

use android_activity::AndroidApp;
use oppa_shell_android::{dirs_record, resolve_app_dirs, validate_dirs, AppDirs};

use crate::ime_bridge::with_attached;

/// Reads an absolute dir path off the activity
/// (`getFilesDir`/`getCacheDir` + `getAbsolutePath` — the
/// `imm_mode` call pattern). Nulls fail loudly (a null dir is a
/// platform fact, and the shell fallback needs to know).
fn activity_dir(app: &AndroidApp, method: &str) -> Result<String, String> {
    with_attached(app, |env, activity| {
        let dir = env
            .call_method(&activity, method, "()Ljava/io/File;", &[])
            .map_err(|e| format!("{method}: {e}"))?
            .l()
            .map_err(|e| format!("{method} obj: {e}"))?;
        if dir.is_null() {
            return Err(format!("{method} returned null"));
        }
        let path_obj = env
            .call_method(&dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
            .map_err(|e| format!("getAbsolutePath: {e}"))?
            .l()
            .map_err(|e| format!("path obj: {e}"))?;
        let path: String = env
            .get_string(&jni::objects::JString::from(path_obj))
            .map_err(|e| format!("path string: {e}"))?
            .into();
        Ok(path)
    })
}

/// Resolves + validates both scoped dirs on device (the round's
/// device entry): JNI getters feed the shell resolver (each getter
/// failure degrades to `None` — the shell fallback decides), then
/// the round-trip validation proves both dirs before any proof
/// output lands. Returns the dirs plus the verbatim record.
pub fn resolve_and_validate(app: &AndroidApp) -> Result<(AppDirs, String), String> {
    let jni_files = activity_dir(app, "getFilesDir")
        .ok()
        .map(std::path::PathBuf::from);
    let jni_cache = activity_dir(app, "getCacheDir")
        .ok()
        .map(std::path::PathBuf::from);
    let internal = app.internal_data_path();
    let dirs = resolve_app_dirs(internal, jni_files, jni_cache)?;
    let validation = validate_dirs(&dirs)?;
    Ok((dirs.clone(), format!("{} {validation}", dirs_record(&dirs))))
}
