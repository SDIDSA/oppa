//! Android system clipboard (decision 443): plain text through
//! `ClipboardManager` (`ClipData.newPlainText` / `getPrimaryClip`
//! with `coerceToText`), closing the decision-442 open follow-up
//! that left Ctrl+C/X/V router-quiet on device.
//!
//! Threading: every method runs on the loop thread through
//! [`with_attached`](super::ime_bridge::with_attached) (permanent
//! attach, same as the IME show/hide path). `ClipboardManager`
//! calls are Binder IPC — callable off the UI thread, never
//! blocking it. Reads settle synchronously (no async leg —
//! `request_read` arms, the first `poll_read` settles, the Win32
//! sync-backend precedent).
//!
//! Construction validates the manager is reachable; a missing
//! service fails loudly and the runner falls back to the
//! session-local clipboard (same best-effort shape as the Linux
//! no-display fallback — in-app copy/paste instead of a dead loop).

use android_activity::AndroidApp;
use oppa::{Clipboard, ClipboardError};

use crate::ime_bridge::with_attached;

/// Android system clipboard over the app's `ClipboardManager`.
///
/// Borrows the `AndroidApp` (the runner owns it for the whole loop —
/// no raw pointers, no cross-thread sharing; `Clipboard` is `!Send`
/// by trait design and this backend never leaves the loop thread).
pub struct AndroidClipboard<'a> {
    app: &'a AndroidApp,
    outstanding: bool,
}

impl<'a> AndroidClipboard<'a> {
    /// Binds the app's `ClipboardManager` (loud `Err` when the
    /// service is missing — the runner falls back, never silently).
    pub fn new(app: &'a AndroidApp) -> Result<Self, String> {
        with_attached(app, |env, activity| {
            let manager = clipboard_manager(env, &activity)?;
            if manager.is_null() {
                return Err("ClipboardManager is null".to_string());
            }
            Ok(())
        })?;
        Ok(Self {
            app,
            outstanding: false,
        })
    }

    fn write_now(&mut self, text: &str) -> Result<(), ClipboardError> {
        with_attached(self.app, |env, activity| {
            let manager = clipboard_manager(env, &activity)?;
            if text.is_empty() {
                // `clearPrimaryClip` needs API 28+ (minSdk 29 —
                // always present, never a version branch).
                call_void(env, &manager, "clearPrimaryClip", "()V")?;
                return Ok(());
            }
            let label = env
                .new_string("oppa")
                .map_err(|e| format!("clip label: {e}"))?;
            let value = env
                .new_string(text)
                .map_err(|e| format!("clip value: {e}"))?;
            let clip_class = env
                .find_class("android/content/ClipData")
                .map_err(|e| format!("find ClipData: {e}"))?;
            let clip = env
                .call_static_method(
                    &clip_class,
                    "newPlainText",
                    "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;",
                    &[
                        jni::objects::JValue::from(&label),
                        jni::objects::JValue::from(&value),
                    ],
                )
                .map_err(|e| format!("newPlainText: {e}"))?
                .l()
                .map_err(|e| format!("clip object: {e}"))?;
            call_void_obj(
                env,
                &manager,
                "setPrimaryClip",
                "(Landroid/content/ClipData;)V",
                &clip,
            )?;
            Ok(())
        })
        .map_err(backend)
    }

    fn read_now(&mut self) -> Result<Option<String>, ClipboardError> {
        with_attached(self.app, |env, activity| {
            let manager = clipboard_manager(env, &activity)?;
            let has = env
                .call_method(&manager, "hasPrimaryClip", "()Z", &[])
                .map_err(|e| format!("hasPrimaryClip: {e}"))?
                .z()
                .map_err(|e| format!("hasPrimaryClip bool: {e}"))?;
            if !has {
                return Ok(None);
            }
            let clip = env
                .call_method(
                    &manager,
                    "getPrimaryClip",
                    "()Landroid/content/ClipData;",
                    &[],
                )
                .map_err(|e| format!("getPrimaryClip: {e}"))?
                .l()
                .map_err(|e| format!("clip object: {e}"))?;
            if clip.is_null() {
                return Ok(None);
            }
            let count = env
                .call_method(&clip, "getItemCount", "()I", &[])
                .map_err(|e| format!("getItemCount: {e}"))?
                .i()
                .map_err(|e| format!("item count: {e}"))?;
            if count <= 0 {
                return Ok(None);
            }
            let item = env
                .call_method(
                    &clip,
                    "getItemAt",
                    "(I)Landroid/content/ClipData$Item;",
                    &[jni::objects::JValue::from(0)],
                )
                .map_err(|e| format!("getItemAt: {e}"))?
                .l()
                .map_err(|e| format!("clip item: {e}"))?;
            // `coerceToText` (not `getText`): styled/URI/intent
            // payloads paste as their text form instead of refusing
            // (plain-text clipboard contract — rich formats stay out,
            // but foreign clips still yield something readable).
            let seq = env
                .call_method(
                    &item,
                    "coerceToText",
                    "(Landroid/content/Context;)Ljava/lang/CharSequence;",
                    &[jni::objects::JValue::from(&activity)],
                )
                .map_err(|e| format!("coerceToText: {e}"))?
                .l()
                .map_err(|e| format!("clip text: {e}"))?;
            if seq.is_null() {
                return Ok(None);
            }
            let jstr = env
                .call_method(&seq, "toString", "()Ljava/lang/String;", &[])
                .map_err(|e| format!("toString: {e}"))?
                .l()
                .map_err(|e| format!("clip string: {e}"))?;
            let text: String = env
                .get_string(&jni::objects::JString::from(jstr))
                .map_err(|e| format!("clip decode: {e}"))?
                .into();
            Ok(if text.is_empty() { None } else { Some(text) })
        })
        .map_err(backend)
    }
}

impl Clipboard for AndroidClipboard<'_> {
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.write_now(text)
    }

    fn clear(&mut self) -> Result<(), ClipboardError> {
        self.write_now("")
    }

    fn request_read(&mut self) {
        self.outstanding = true;
    }

    fn poll_read(&mut self) -> Option<Result<Option<String>, ClipboardError>> {
        // Sync backend: every poll settles (the outstanding flag only
        // exists so the poll contract matches the async shells').
        self.outstanding = false;
        Some(self.read_now())
    }
}

/// Fetches the app's `ClipboardManager` (`"clipboard"` is
/// `Context.CLIPBOARD_SERVICE` — the `"input_method"` literal
/// precedent in `ime_bridge`).
fn clipboard_manager<'b>(
    env: &mut jni::JNIEnv<'b>,
    activity: &jni::objects::JObject<'_>,
) -> Result<jni::objects::JObject<'b>, String> {
    let name = env
        .new_string("clipboard")
        .map_err(|e| format!("service name: {e}"))?;
    let manager = env
        .call_method(
            activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[jni::objects::JValue::from(&name)],
        )
        .map_err(|e| format!("getSystemService: {e}"))?
        .l()
        .map_err(|e| format!("clipboard service: {e}"))?;
    if manager.is_null() {
        return Err("ClipboardManager is null".to_string());
    }
    Ok(manager)
}

/// Calls a `()V` method, surfacing a pending Java exception as the
/// error (a bare JNI error string never names the Java side — the
/// `log_java_exception` rule, but returning instead of only logging
/// so the clipboard reports loudly instead of pretending success).
fn call_void(
    env: &mut jni::JNIEnv<'_>,
    obj: &jni::objects::JObject<'_>,
    name: &str,
    sig: &str,
) -> Result<(), String> {
    let res = env.call_method(obj, name, sig, &[]);
    check(env, res.map(|_| ()), name)
}

/// Calls a `(LObject;)V` method with one object argument.
fn call_void_obj(
    env: &mut jni::JNIEnv<'_>,
    obj: &jni::objects::JObject<'_>,
    name: &str,
    sig: &str,
    arg: &jni::objects::JObject<'_>,
) -> Result<(), String> {
    let res = env.call_method(obj, name, sig, &[jni::objects::JValue::from(arg)]);
    check(env, res.map(|_| ()), name)
}

fn check<T>(
    env: &mut jni::JNIEnv<'_>,
    res: Result<T, jni::errors::Error>,
    what: &str,
) -> Result<T, String> {
    match res {
        Ok(v) => {
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_describe();
                let _ = env.exception_clear();
                Err(format!("{what}: Java exception (see logcat)"))
            } else {
                Ok(v)
            }
        }
        Err(e) => {
            crate::ime_bridge::log_java_exception(env);
            Err(format!("{what}: {e}"))
        }
    }
}

fn backend(msg: String) -> ClipboardError {
    ClipboardError::Backend(msg)
}
