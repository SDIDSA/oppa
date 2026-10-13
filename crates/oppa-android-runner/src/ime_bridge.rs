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
use oppa_shell_android::selection_menu::{MenuAction, MenuActionQueue};
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
            ImeTextItem::ComposeText(text) => {
                shell.push_event(AndroidEvent::ComposeText { text });
            }
            ImeTextItem::FinishComposing => {
                shell.push_event(AndroidEvent::FinishComposing);
            }
        }
    }
    if n > 0 {
        eprintln!("oppa-ime: drained {n} queue items");
    }
    n
}

/// Prints the pending Java exception to logcat and clears it (a
/// bare JNI error string never names the Java side — without this
/// the failure is opaque). Best-effort: diagnostics must never
/// fail the call they describe.
pub fn log_java_exception(env: &mut jni::JNIEnv<'_>) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
}

/// Attaches the current thread and runs one JNI body (null
/// VM/activity fail loudly). The attach guard lives inside this
/// call, so every reference stays local: nothing escapes (the old
/// transmute-to-'static shape would detach under the caller —
/// unsound, never do that).
pub fn with_attached<T>(
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
/// tombstone-proven sequence — native-thread `find_class` misses
/// app classes; the loader over our own APK path does not).
/// The loader is process-wide and shared: every `loadClass` must
/// resolve through the SAME loader object, because each loader
/// mints its OWN `Class` — natives registered on one instance do
/// not exist on another, and the first commit then dies with
/// `UnsatisfiedLinkError` on an identically-named but unbound
/// class (observed on-device). Returns the loaded class object.
pub fn load_app_class<'b>(
    env: &mut jni::JNIEnv<'b>,
    activity: &jni::objects::JObject<'_>,
    name: &str,
) -> Result<jni::objects::JObject<'b>, String> {
    let loader = dex_loader(env, activity)?;
    let name_jstr = env
        .new_string(name)
        .map_err(|e| format!("new_string: {e}"))?;
    let cls_obj = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[jni::objects::JValue::from(&name_jstr)],
        )
        .map_err(|e| format!("loadClass {name}: {e}"))?
        .l()
        .map_err(|e| format!("{name} class: {e}"))?;
    Ok(cls_obj)
}

/// The shared `DexClassLoader` (one per process — see
/// [`load_app_class`]): built once from the APK path + code cache
/// + native lib dir, then reused for every class load.
static DEX_LOADER: std::sync::OnceLock<std::sync::Mutex<Option<jni::objects::GlobalRef>>> =
    std::sync::OnceLock::new();

/// Returns the shared loader (building + caching it on first use).
/// A global ref crosses attached threads freely; each call derives
/// a fresh local for the actual `loadClass` invocation, so no local
/// table ever fills on the permanently-attached loop thread.
fn dex_loader<'b>(
    env: &mut jni::JNIEnv<'b>,
    activity: &jni::objects::JObject<'_>,
) -> Result<jni::objects::JObject<'b>, String> {
    let mut guard = DEX_LOADER
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .map_err(|e| format!("loader lock: {e}"))?;
    if guard.is_none() {
        let loader = build_dex_loader(env, activity)?;
        let global = env
            .new_global_ref(&loader)
            .map_err(|e| format!("loader global ref: {e}"))?;
        *guard = Some(global);
    }
    let global = guard
        .as_ref()
        .ok_or_else(|| "loader vanished after store".to_string())?;
    env.new_local_ref(global.as_obj())
        .map_err(|e| format!("loader local ref: {e}"))
}

/// Builds the `DexClassLoader` over our APK path (tombstone-proven
/// sequence — native-thread `find_class` misses app classes).
fn build_dex_loader<'b>(
    env: &mut jni::JNIEnv<'b>,
    activity: &jni::objects::JObject<'_>,
) -> Result<jni::objects::JObject<'b>, String> {
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
    // Native library path (the `OppaIme` static initializer runs
    // `System.loadLibrary` — with a null library path the lookup
    // misses the already-loaded activity `.so` and class init dies
    // with `UnsatisfiedLinkError` before any method body runs
    // (`OppaUi` has no statics, which is why immersive worked
    // while attach never did).
    let lib_dir_obj = env
        .get_field(&app_info, "nativeLibraryDir", "Ljava/lang/String;")
        .map_err(|e| format!("nativeLibraryDir: {e}"))?
        .l()
        .map_err(|e| format!("lib dir: {e}"))?;
    let lib_dir: String = env
        .get_string(&jni::objects::JString::from(lib_dir_obj))
        .map_err(|e| format!("lib dir string: {e}"))?
        .into();
    let lib_jstr = env
        .new_string(&lib_dir)
        .map_err(|e| format!("lib jstring: {e}"))?;
    let dex_loader = env
        .new_object(
            &dex_cls,
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/ClassLoader;)V",
            &[
                jni::objects::JValue::from(&apk_jstr),
                jni::objects::JValue::from(&cache_jstr),
                jni::objects::JValue::from(&lib_jstr),
                jni::objects::JValue::from(&sys_loader),
            ],
        )
        .map_err(|e| format!("new DexClassLoader: {e}"))?;
    Ok(dex_loader)
}

/// Registers the `OppaIme` native entries (idempotent — second
/// call records, never re-registers). Must run before the proxy
/// can call back (the Java half loads the class but never
/// `System.loadLibrary`-binds by name — NativeActivity has no
/// loader hook of its own). `java_package` locates the app's copy.
pub fn ensure_ime_callbacks(app: &AndroidApp, java_package: &str) -> Result<String, String> {
    if REGISTERED.load(std::sync::atomic::Ordering::Acquire) {
        return Ok("ime-callbacks=already-registered".to_string());
    }
    with_attached(app, |env, activity| {
        let cls_obj = load_app_class(env, &activity, &format!("{java_package}.OppaIme"))?;
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
                jni::NativeMethod {
                    name: jni::strings::JNIString::from("onComposeText"),
                    sig: jni::strings::JNIString::from("(Ljava/lang/String;)V"),
                    fn_ptr: on_compose_text as *mut std::ffi::c_void,
                },
                jni::NativeMethod {
                    name: jni::strings::JNIString::from("onFinishComposing"),
                    sig: jni::strings::JNIString::from("()V"),
                    fn_ptr: on_finish_composing as *mut std::ffi::c_void,
                },
                jni::NativeMethod {
                    name: jni::strings::JNIString::from("onProxySelectionChanged"),
                    sig: jni::strings::JNIString::from("(II)V"),
                    fn_ptr: on_proxy_selection_changed as *mut std::ffi::c_void,
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
            eprintln!("oppa-ime: onCommitText len={}", text.len());
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
    eprintln!("oppa-ime: onDeleteSurroundingText {before},{after}");
    ime_queue().push_delete(before.max(0) as u32, after.max(0) as u32);
}

/// `OppaIme.onComposeText` — proxy `setComposingText` lands here
/// (IME thread) and queues (same hand-off as commits — the pump owns
/// the session). A bad string fails loudly and queues nothing.
unsafe extern "system" fn on_compose_text<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    text: jni::objects::JString<'local>,
) {
    match env.get_string(&text) {
        Ok(s) => {
            let text: String = s.into();
            eprintln!("oppa-ime: onComposeText len={}", text.len());
            ime_queue().push_compose(text);
        }
        Err(e) => {
            eprintln!("oppa-ime: onComposeText string failed: {e:?}");
        }
    }
}

/// `OppaIme.onFinishComposing` — proxy `finishComposingText`
/// lands here (IME thread) and queues (same hand-off as commits —
/// the pump owns the session).
unsafe extern "system" fn on_finish_composing<'local>(
    _env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
) {
    eprintln!("oppa-ime: onFinishComposing");
    ime_queue().push_finish();
}

/// The attached proxy view (the hidden 1x1 `EditText`): created
/// by [`ensure_proxy`], kept as a global ref while its window
/// generation lives. A pause/resume heal turns the window over and
/// the cached view detaches for good (`showSoftInput` ignores it as
/// "not served" forever while taps keep working), so the runner
/// drops the cache on every post-heal generation turn
/// ([`drop_proxy_cache`]) and the next focus re-attaches to the
/// live decor. Without a proxy `showSoftInput` reports false on the
/// bare decor view and no keyboard ever appears, no matter how
/// often the focus path asks.
static PROXY: std::sync::OnceLock<std::sync::Mutex<Option<jni::objects::GlobalRef>>> =
    std::sync::OnceLock::new();

/// Attaches the proxy view (idempotent — second call records the
/// cached handle, never re-attaches). Must precede the first
/// proxy-targeted show (the Java half never attaches itself).
/// A null view from Java fails loudly (attach logs to logcat and
/// returns null on failure — a missing proxy is structural, never
/// a quiet no-op).
pub fn ensure_proxy(app: &AndroidApp, java_package: &str) -> Result<String, String> {
    {
        let guard = PROXY.get_or_init(|| std::sync::Mutex::new(None));
        if guard
            .lock()
            .map_err(|e| format!("proxy lock: {e}"))?
            .is_some()
        {
            return Ok("ime-proxy=already-attached".to_string());
        }
    }
    with_attached(app, |env, activity| {
        let cls_obj = load_app_class(env, &activity, &format!("{java_package}.OppaIme"))?;
        let cls: &jni::objects::JClass = (&cls_obj).into();
        let view = env
            .call_static_method(
                cls,
                "attach",
                "(Landroid/app/Activity;)Landroid/view/View;",
                &[jni::objects::JValue::from(&activity)],
            )
            .map_err(|e| format!("OppaIme.attach: {e}"))?
            .l()
            .map_err(|e| format!("proxy view: {e}"))?;
        if view.is_null() {
            return Err("OppaIme.attach returned null".to_string());
        }
        let global = env
            .new_global_ref(&view)
            .map_err(|e| format!("proxy global ref: {e}"))?;
        *PROXY
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .map_err(|e| format!("proxy lock: {e}"))? = Some(global);
        Ok("ime-proxy=attached".to_string())
    })
}

/// Drops the cached proxy view (decision 441 — the resume half of
/// the proxy contract): safe to call every generation turn; empty
/// cache is a loud no-op, a held view is unreferenced properly
/// (never leaked one-per-heal). Never fails the caller — a stuck
/// cache degrades to the pre-441 behavior (typing dead after
/// resume) instead of killing the run.
pub fn drop_proxy_cache(app: &AndroidApp) -> Result<String, String> {
    with_attached(app, |_env, _activity| {
        let guard = PROXY.get_or_init(|| std::sync::Mutex::new(None));
        let mut slot = guard.lock().map_err(|e| format!("proxy lock: {e}"))?;
        match slot.take() {
            // Dropping the `GlobalRef` deletes it (jni `Drop`
            // impl) — never leaked one-per-heal.
            Some(_) => Ok("ime-proxy=dropped".to_string()),
            None => Ok("ime-proxy=nothing-cached".to_string()),
        }
    })
}

/// Shows the keyboard on the proxy view (the focus path — the
/// runner calls this on `ImeRequest::Show`): focuses the hidden
/// `EditText` and shows the input method on it, so commits land in
/// `onCommitText` instead of nowhere. Attaches lazily on first
/// show (no keyboard need, no proxy). `password` mirrors the focused
/// field's masked state (decision 444 — applied before the show, so
/// the connection the show creates already carries the password
/// variation). Returns the verbatim platform result.
pub fn show_proxy_keyboard(
    app: &AndroidApp,
    java_package: &str,
    password: bool,
) -> Result<String, String> {
    apply_password_mode(app, java_package, password)?;
    ensure_proxy(app, java_package)?;
    with_attached(app, |env, activity| {
        let proxy = PROXY
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .map_err(|e| format!("proxy lock: {e}"))?;
        let view = proxy
            .as_ref()
            .ok_or_else(|| "proxy vanished after attach".to_string())?;
        let cls_obj = load_app_class(env, &activity, &format!("{java_package}.OppaIme"))?;
        let cls: &jni::objects::JClass = (&cls_obj).into();
        match env.call_static_method(
            cls,
            "showIme",
            "(Landroid/app/Activity;Landroid/view/View;)V",
            &[
                jni::objects::JValue::from(&activity),
                jni::objects::JValue::from(view.as_obj()),
            ],
        ) {
            Ok(_) => Ok("ime-show=proxy".to_string()),
            Err(e) => {
                log_java_exception(env);
                Err(format!("OppaIme.showIme: {e}"))
            }
        }
    })
}

/// Proxy selection reports (decision 449, narrowed in 456): the
/// device half of the selection mirror. The proxy hosts no UI of its
/// own anymore — only its buffer moves (software-keyboard arrows,
/// Editor word selects) report back through the `onProxySelectionChanged`
/// native below (the queue is the only shared state — same thread
/// split as the IME text queue).
///
/// The process-wide menu action queue (Java pushes on the UI
/// thread, the pump drains on the loop thread).
fn menu_queue() -> &'static MenuActionQueue {
    static QUEUE: OnceLock<MenuActionQueue> = OnceLock::new();
    QUEUE.get_or_init(MenuActionQueue::new)
}

/// Drains the menu queue (the pump path — one drain per frame; each
/// item applies to the focused session through the loop clipboard).
pub fn drain_menu_actions() -> Vec<MenuAction> {
    let items = menu_queue().drain();
    if !items.is_empty() {
        eprintln!("oppa-menu: drained {} actions", items.len());
    }
    items
}

/// Applies the focused field's password mode to the proxy
/// (decision 444): stores the masked flag on the Bridge and pokes
/// `restartInput` so the IME re-reads the connection (a shared
/// proxy keeps focus across field switches, so a bare flag would
/// only take on the next fresh connection — the restart makes the
/// switch immediate; on a view with no live connection it is a
/// harmless no-op). Failures propagate to the caller, which logs
/// loudly and continues (worst case the IME keeps the previous
/// field's variation until the next show, which re-applies — the
/// flag, not the restart, is the source of truth).
pub fn apply_password_mode(
    app: &AndroidApp,
    java_package: &str,
    password: bool,
) -> Result<String, String> {
    with_menu_bridge(app, java_package, |env, cls, activity, view| {
        match env.call_static_method(
            cls,
            "applyPasswordMode",
            "(Landroid/app/Activity;Landroid/view/View;Z)V",
            &[
                jni::objects::JValue::from(activity),
                jni::objects::JValue::from(view),
                jni::objects::JValue::from(password),
            ],
        ) {
            Ok(_) => Ok(format!("ime-password-mode={password}")),
            Err(e) => {
                log_java_exception(env);
                Err(format!("OppaIme.applyPasswordMode: {e}"))
            }
        }
    })
}

/// Runs one `OppaIme` menu static with the proxy view (attaches
/// lazily like the keyboard path — no selection, no proxy).
fn with_menu_bridge<T>(
    app: &AndroidApp,
    java_package: &str,
    f: impl FnOnce(
        &mut jni::JNIEnv<'_>,
        &jni::objects::JClass<'_>,
        &jni::objects::JObject<'_>,
        &jni::objects::JObject<'_>,
    ) -> Result<T, String>,
) -> Result<T, String> {
    ensure_proxy(app, java_package)?;
    with_attached(app, |env, activity| {
        let cls_obj = load_app_class(env, &activity, &format!("{java_package}.OppaIme"))?;
        let cls: &jni::objects::JClass = (&cls_obj).into();
        let proxy = PROXY
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .map_err(|e| format!("proxy lock: {e}"))?;
        let view = proxy
            .as_ref()
            .ok_or_else(|| "proxy vanished after attach".to_string())?;
        f(env, cls, &activity, view.as_obj())
    })
}

/// Java `(Activity, View, String, int x6)` signature shared by the
/// mirror entries (text + UTF-16 selection + device-px anchor rect),
/// returning whether the mode is live on return (the synchronous
/// gate in Java — never fire-and-forget, so a refused start retries
/// instead of wedging Rust in `shown`).
const MIRROR_SIG: &str = "(Landroid/app/Activity;Landroid/view/View;Ljava/lang/String;IIIIII)Z";

/// Packs one mirror call's arguments (text + UTF-16 selection +
/// rect) and invokes the named `OppaIme` static. `start16`/`end16`
/// are Java string units (the shell's
/// [`byte_to_utf16_units`](oppa_shell_android::selection_menu::byte_to_utf16_units)
/// conversion — never raw byte offsets, which split surrogate
/// halves on non-BMP text). Returns whether the mode is live.
#[allow(clippy::too_many_arguments)]
fn call_mirror(
    env: &mut jni::JNIEnv<'_>,
    cls: &jni::objects::JClass<'_>,
    method: &str,
    activity: &jni::objects::JObject<'_>,
    view: &jni::objects::JObject<'_>,
    text: &str,
    start16: usize,
    end16: usize,
    rect: (i32, i32, i32, i32),
) -> Result<bool, String> {
    let text_jstr = env
        .new_string(text)
        .map_err(|e| format!("menu text: {e}"))?;
    let args = [
        jni::objects::JValue::from(activity),
        jni::objects::JValue::from(view),
        jni::objects::JValue::from(&text_jstr),
        jni::objects::JValue::from(start16 as i32),
        jni::objects::JValue::from(end16 as i32),
        jni::objects::JValue::from(rect.0),
        jni::objects::JValue::from(rect.1),
        jni::objects::JValue::from(rect.2),
        jni::objects::JValue::from(rect.3),
    ];
    match env.call_static_method(cls, method, MIRROR_SIG, &args) {
        Ok(v) => v.z().map_err(|e| {
            log_java_exception(env);
            format!("OppaIme.{method} bool: {e}")
        }),
        Err(e) => {
            log_java_exception(env);
            Err(format!("OppaIme.{method}: {e}"))
        }
    }
}

/// Mirrors the session into the proxy buffer (decision 455 — IME
/// coherence without any system UI: the keyboard tracks editor state
/// through context queries even though no `ActionMode` ever starts;
/// the scene overlay in `host.toolbar_model` is the only toolbar).
/// Text, selection, and anchor cross in one UI-thread hop so the
/// buffer never serves stale content. The returned live-boolean is
/// the mirror's own acknowledgement (callers log it, never wedge).
#[allow(clippy::too_many_arguments)]
pub fn update_selection_menu(
    app: &AndroidApp,
    java_package: &str,
    text: &str,
    start16: usize,
    end16: usize,
    rect: (i32, i32, i32, i32),
) -> Result<bool, String> {
    with_menu_bridge(app, java_package, |env, cls, activity, view| {
        call_mirror(
            env,
            cls,
            "updateMenu",
            activity,
            view,
            text,
            start16,
            end16,
            rect,
        )
    })
}

/// `OppaIme.onProxySelectionChanged` — a proxy-side selection move
/// (decision 449, narrowed in 456 — software-keyboard arrows, Editor
/// word selects) queues the UTF-16 span for the pump (which converts
/// to bytes and absolute-sets the focused session, so copy/cut read
/// the visible span). Negative inputs are proxy bugs — clamped
/// loudly to zero rather than wrapping a usize cast into billions.
unsafe extern "system" fn on_proxy_selection_changed<'local>(
    _env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    start: jni::sys::jint,
    end: jni::sys::jint,
) {
    if start < 0 || end < 0 {
        eprintln!("oppa-menu: onSelectionChanged negative ({start},{end}) — clamped");
    }
    let (start16, end16) = (start.max(0) as usize, end.max(0) as usize);
    eprintln!("oppa-menu: onSelectionChanged {start16},{end16}");
    menu_queue().push_selection(start16, end16);
}
