//! JNI bridge (v1 remainder, Gap 2): enumerates the *platform's*
//! font list via `android.graphics.fonts.SystemFonts` (API 29+)
//! through the `JavaVM` the `NativeActivity` runs on.
//!
//! This is the JNI half of the Android text slice: the shaper half
//! (`oppa-text-android`, rustybuzz over the same files) shapes; this
//! module proves the file set it shapes is the set the platform
//! itself advertises. The record (`fonts_jni.txt`) carries the
//! sorted platform paths + the TTC indices; the host compares them
//! against the shaped faces.

use android_activity::AndroidApp;

/// Queries the platform font list through JNI. Returns one
/// `path|ttc_index` line per font, sorted, joined by `\n`.
/// Any failure is a loud `Err` naming the JNI step (a missing
/// SystemFonts API, an unattached thread, or an unexpected shape
/// all fail here — never an empty list standing in for fonts).
pub fn query_system_fonts(app: &AndroidApp) -> Result<String, String> {
    let vm_ptr = app.vm_as_ptr() as *mut jni::sys::JavaVM;
    if vm_ptr.is_null() {
        return Err("JavaVM pointer is null".to_string());
    }
    // SAFETY: the pointer comes from the live NativeActivity; the
    // VM outlives this call.
    let vm =
        unsafe { jni::JavaVM::from_raw(vm_ptr) }.map_err(|e| format!("JavaVM::from_raw: {e}"))?;
    let mut env = vm
        .attach_current_thread_permanently()
        .map_err(|e| format!("attach thread: {e}"))?;
    let cls = env
        .find_class("android/graphics/fonts/SystemFonts")
        .map_err(|e| format!("find SystemFonts: {e}"))?;
    let set = env
        .call_static_method(cls, "getAvailableFonts", "()Ljava/util/Set;", &[])
        .map_err(|e| format!("getAvailableFonts: {e}"))?
        .l()
        .map_err(|e| format!("Set object: {e}"))?;
    // Note: array return types carry the `[` prefix in JNI
    // signatures (`()[Ljava/lang/Object;` — the no-prefix form
    // throws NoSuchMethodError, found on-device).
    let arr_obj = env
        .call_method(&set, "toArray", "()[Ljava/lang/Object;", &[])
        .map_err(|e| format!("Set.toArray: {e}"))?
        .l()
        .map_err(|e| format!("array object: {e}"))?;
    let arr = jni::objects::JObjectArray::from(arr_obj);
    let len = env
        .get_array_length(&arr)
        .map_err(|e| format!("array length: {e}"))?;
    let mut lines: Vec<String> = Vec::new();
    for i in 0..len {
        let font = env
            .get_object_array_element(&arr, i)
            .map_err(|e| format!("font {i}: {e}"))?;
        let file = env
            .call_method(&font, "getFile", "()Ljava/io/File;", &[])
            .map_err(|e| format!("font {i} getFile: {e}"))?
            .l()
            .map_err(|e| format!("font {i} File: {e}"))?;
        let path_obj = env
            .call_method(&file, "getAbsolutePath", "()Ljava/lang/String;", &[])
            .map_err(|e| format!("font {i} getAbsolutePath: {e}"))?
            .l()
            .map_err(|e| format!("font {i} path: {e}"))?;
        let path_str: String = env
            .get_string(&jni::objects::JString::from(path_obj))
            .map_err(|e| format!("font {i} path string: {e}"))?
            .into();
        let ttc = env
            .call_method(&font, "getTtcIndex", "()I", &[])
            .map_err(|e| format!("font {i} getTtcIndex: {e}"))?
            .i()
            .map_err(|e| format!("font {i} ttc int: {e}"))?;
        lines.push(format!("{path_str}|{ttc}"));
    }
    if lines.is_empty() {
        return Err("SystemFonts returned zero fonts".to_string());
    }
    lines.sort();
    Ok(lines.join("\n"))
}
