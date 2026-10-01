//! Soft-keyboard bridge (Round 3.1): the interactive IME path.
//!
//! Two halves, split where they are provable:
//!
//! - Host-provable (in `oppa-shell-android`, unit-tested): the
//!   [`ImeTextQueue`](oppa_shell_android::ImeTextItem) FIFO, the
//!   `CommitText`/`DeleteSurrounding` intake events + commands,
//!   the `Show`/`Hide` visibility policy, and the focused-session
//!   insert/delete helpers.
//! - Device-only (here, android-target compiled): `show_keyboard`
//!   / `hide_keyboard` over JNI (decor view + window token — the
//!   same proven calls `imm_policy` records), native-method
//!   registration for the Java `InputConnection` proxy, and the
//!   `extern` text entries that push the queue the pump drains.
//!
//! Relationship to `imm_policy` (stated, not duplicated): that
//! function is the validation probe (service non-null, enabled
//! count, show/hide booleans on the decor view). These functions
//! are the interactive path (focus-driven, proxy-fed). Same IMM,
//! different caller.
//!
//! Threading: the proxy calls the native entries on the IME
//! thread; the pump drains on the loop thread; the queue is the
//! only shared state (mutex'd, never blocking the callback beyond
//! a push). `show`/`hide` may run on any thread (IMM calls are
//! thread-safe; only `View`/`Window` *bars* calls needed the
//! `OppaUi` UI-thread hop — see `imm_mode`).
//!
//! Java half: `OppaUi.java` hosts the hidden `EditText` +
//! `InputConnection` proxy (`OppaIme` class) that calls these
//! native entries. The Java compiles under Gradle only —
//! device-pending, marked there.

use std::sync::OnceLock;

use android_activity::AndroidApp;
use oppa_shell_android::events::AndroidEvent;
use oppa_shell_android::ime_queue::{ImeTextItem, ImeTextQueue};
use oppa_shell_android::shell::AndroidShell;

/// The process-wide IME text queue (the Java proxy pushes, the
/// pump drains — created on first use, lives forever).
fn ime_queue() -> &'static ImeTextQueue {
    static QUEUE: OnceLock<ImeTextQueue> = OnceLock::new();
    QUEUE.get_or_init(ImeTextQueue::new)
}

/// Native-entry registration state (RegisterNatives twice fails
/// loudly — the second ensure is a no-op record instead).
static REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Drains the JNI queue (the pump path — one drain per frame; each
/// item becomes shell intake downstream).
pub fn drain_ime_queue() -> Vec<ImeTextItem> {
    ime_queue().drain()
}

/// Pushes drained items into shell intake (the pump half of the
/// bridge — the shell classifies them into runner-matched
/// commands from here). Returns the drained count (the record
/// path reports it verbatim — never hardcoded).
pub fn drain_ime_queue_into(shell: &mut AndroidShell) -> usize {
    let mut n = 0;
    for item in drain_ime_queue() {
        n += 1;
        match item {
            ImeTextItem::CommitText(text) => {
                shell.push_event(AndroidEvent::CommitText { text });
            }
            ImeTextItem::DeleteSurrounding {
                before_chars,
                after_chars,
            } => shell.push_event(AndroidEvent::DeleteSurrounding {
                before_chars,
                after_chars,
            }),
        }
    }
    n
}

/// Attaches the current thread and runs one JNI body (the proven
/// `imm_mode` preamble — null VM/activity fail loudly). The attach
/// guard lives inside this call, so every reference stays local:
/// nothing escapes (the old transmute-to-'static shape would detach
/// under the caller — unsound, never do that). Crate-visible for
/// the sibling JNI modules (one preamble, not three copies).
pub(crate) fn with_attached<T>(
    app: &AndroidApp,
    f: impl FnOnce(&mut jni::JNIEnv<'_>, jni::objects::JObject<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let vm_ptr = app.vm_as_ptr() as *mut jni::sys::JavaVM;
    if vm_ptr.is_null() {
        return Err("JavaVM pointer is null".to_string());
    }
    // SAFETY: live NativeActivity VM, outlives the call.
    let vm =
        unsafe { jni::JavaVM::from_raw(vm_ptr) }.map_err(|e| format!("JavaVM::from_raw: {e}"))?;
    let mut guard = vm
        .attach_current_thread_permanently()
        .map_err(|e| format!("attach thread: {e}"))?;
    let activity =
        unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    if activity.is_null() {
        return Err("Activity pointer is null".to_string());
    }
    f(&mut guard, activity)
}

/// Loads our APK class through a `DexClassLoader` (the
/// tombstone-proven `imm_mode` sequence — native-thread
/// `find_class` misses app classes; the loader over our own APK
/// path does not). Returns the loaded class object.
fn load_app_class<'a>(
    env: &mut jni::JNIEnv<'a>,
    activity: &jni::objects::JObject<'_>,
    name: &str,
) -> Result<jni::objects::JObject<'a>, String> {
    let app_info = env
        .call_method(
            activity,
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
        .call_method(activity, "getCodeCacheDir", "()Ljava/io/File;", &[])
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
    let name_jstr = env
        .new_string(name)
        .map_err(|e| format!("new_string: {e}"))?;
    let cls_obj = env
        .call_method(
            &dex_loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[jni::objects::JValue::from(&name_jstr)],
        )
        .map_err(|e| format!("loadClass {name}: {e}"))?
        .l()
        .map_err(|e| format!("{name} class: {e}"))?;
    Ok(cls_obj)
}

/// Registers the `OppaIme` native entries (idempotent — second
/// call records, never re-registers). Must run before the proxy
/// can call back (the Java half loads the class but never
/// `System.loadLibrary`-binds by name — NativeActivity has no
/// loader hook of its own).
pub fn ensure_ime_callbacks(app: &AndroidApp) -> Result<String, String> {
    if REGISTERED.load(std::sync::atomic::Ordering::Acquire) {
        return Ok("ime-callbacks=already-registered".to_string());
    }
    with_attached(app, |env, activity| {
        let cls_obj = load_app_class(env, &activity, "com.oppa.app.OppaIme")?;
        let cls: &jni::objects::JClass = (&cls_obj).into();
        env.register_native_methods(
            cls,
            &[
                jni::NativeMethod {
                    name: jni::strings::JNIString::from("onCommitText"),
                    sig: jni::strings::JNIString::from("(Ljava/lang/String;)V"),
                    fn_ptr: on_commit_text as *mut std::ffi::c_void,
                },
                jni::NativeMethod {
                    name: jni::strings::JNIString::from("onDeleteSurroundingText"),
                    sig: jni::strings::JNIString::from("(II)V"),
                    fn_ptr: on_delete_surrounding_text as *mut std::ffi::c_void,
                },
            ],
        )
        .map_err(|e| format!("register OppaIme natives: {e}"))?;
        REGISTERED.store(true, std::sync::atomic::Ordering::Release);
        Ok("ime-callbacks=registered".to_string())
    })
}

/// Fetches the IMM + decor view + window token (the proven
/// `imm_policy` triple — shared so the two callers cannot drift).
fn imm_view_token<'a>(
    env: &mut jni::JNIEnv<'a>,
    activity: &jni::objects::JObject<'_>,
) -> Result<
    (
        jni::objects::JObject<'a>,
        jni::objects::JObject<'a>,
        jni::objects::JObject<'a>,
    ),
    String,
> {
    let service_name = env
        .new_string("input_method")
        .map_err(|e| format!("new_string: {e}"))?;
    let imm = env
        .call_method(
            activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[jni::objects::JValue::from(&service_name)],
        )
        .map_err(|e| format!("getSystemService: {e}"))?
        .l()
        .map_err(|e| format!("IMM object: {e}"))?;
    if imm.is_null() {
        return Err("InputMethodManager is null".to_string());
    }
    let window = env
        .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])
        .map_err(|e| format!("getWindow: {e}"))?
        .l()
        .map_err(|e| format!("Window: {e}"))?;
    let view = env
        .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])
        .map_err(|e| format!("getDecorView: {e}"))?
        .l()
        .map_err(|e| format!("decor view: {e}"))?;
    let token = env
        .call_method(&view, "getWindowToken", "()Landroid/os/IBinder;", &[])
        .map_err(|e| format!("getWindowToken: {e}"))?
        .l()
        .map_err(|e| format!("token: {e}"))?;
    Ok((imm, view, token))
}

/// Shows the soft keyboard on the decor view (the focus path —
/// the runner calls this on `ImeRequest::Show`). Returns the
/// verbatim platform result (`false` with no focused view is a
/// platform fact, not a failure — the `imm_policy` rule).
pub fn show_keyboard(app: &AndroidApp) -> Result<String, String> {
    with_attached(app, |env, activity| {
        let (imm, view, _token) = imm_view_token(env, &activity)?;
        let show = env
            .call_method(
                &imm,
                "showSoftInput",
                "(Landroid/view/View;I)Z",
                &[
                    jni::objects::JValue::from(&view),
                    jni::objects::JValue::from(0),
                ],
            )
            .map_err(|e| format!("showSoftInput: {e}"))?
            .z()
            .map_err(|e| format!("show bool: {e}"))?;
        Ok(format!("ime-show={show}"))
    })
}

/// Hides the soft keyboard from the window token (the blur path —
/// the runner calls this on `ImeRequest::Hide`). Verbatim result.
pub fn hide_keyboard(app: &AndroidApp) -> Result<String, String> {
    with_attached(app, |env, activity| {
        let (imm, _view, token) = imm_view_token(env, &activity)?;
        let hide = env
            .call_method(
                &imm,
                "hideSoftInputFromWindow",
                "(Landroid/os/IBinder;I)Z",
                &[
                    jni::objects::JValue::from(&token),
                    jni::objects::JValue::from(0),
                ],
            )
            .map_err(|e| format!("hideSoftInputFromWindow: {e}"))?
            .z()
            .map_err(|e| format!("hide bool: {e}"))?;
        Ok(format!("ime-hide={hide}"))
    })
}

/// `OppaIme.onCommitText` — proxy `commitText` lands here (IME
/// thread) and queues (never touches the session — the pump owns
/// that). A bad string fails loudly to stderr (the device log
/// channel, same as the phase markers — a void entry has no
/// `error.txt` path) and queues nothing.
unsafe extern "system" fn on_commit_text<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    text: jni::objects::JString<'local>,
) {
    match env.get_string(&text) {
        Ok(s) => {
            let text: String = s.into();
            ime_queue().push_commit(text);
        }
        Err(e) => {
            eprintln!("oppa-ime: onCommitText string failed: {e:?}");
        }
    }
}

/// `OppaIme.onDeleteSurroundingText` — proxy `deleteSurroundingText`
/// lands here (IME thread) and queues.
unsafe extern "system" fn on_delete_surrounding_text<'local>(
    _env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    before: jni::sys::jint,
    after: jni::sys::jint,
) {
    // Negative counts are proxy bugs — clamp loudly (stderr,
    // like every other device-loud path here) to zero rather than
    // wrapping a u32 cast into billions.
    if before < 0 || after < 0 {
        eprintln!("oppa-ime: onDeleteSurroundingText negative ({before},{after}) — clamped");
    }
    ime_queue().push_delete(before.max(0) as u32, after.max(0) as u32);
}
