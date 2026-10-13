//! SAF open bridge (decision 427): trampoline-fired picker
//! results into a [`FileDialog`](oppa::FileDialog) the sink's
//! Browse button drives.
//!
//! Split where provable: option mapping, generation gating, and
//! dismissal/level-trigger rules live host-tested in
//! `oppa-shell-android::picker`; Java (`OppaPickerActivity`)
//! fires the intent and reads bytes; this module is the middle —
//! fire-entry verification, the process-wide rendezvous,
//! cache-copy, the static-linked result entry, and the `FileDialog`
//! impl.
//!
//! Cache-copy rule (stated): picked bytes land app-private under
//! `picked/` (sanitized file name, never a directory escape), so
//! the `PathBuf` the control shows is always a real readable file
//! — never a fabricated path. Dismissal settles `Ok(vec![])`;
//! read failure settles loud `Backend`; fire failure latches loud
//! `Backend` too (a void `request_open` has no `Err` channel).
//! Polls re-return the last result (level-triggered); a second
//! request supersedes (stale callback generations discard).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use android_activity::AndroidApp;
use oppa::{FileDialog, FilePickerOptions, PickError};
use oppa_shell_android::picker::saf_spec_for;

use oppa_android_runner::ime_bridge::with_attached;

/// The process-wide picker rendezvous (the Java trampoline
/// resolves on the UI thread, the loop thread polls — installed
/// once in `android_main`, lives forever; callbacks before
/// install log loudly and drop, never panic).
static PICKER: OnceLock<Mutex<PickerState>> = OnceLock::new();

/// Install flag (the `REGISTERED` precedent in `ime_bridge` —
/// RegisterNatives twice fails loudly, so the second ensure is a
/// no-op record instead).
static INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct PickerState {
    app: AndroidApp,
    cache_dir: PathBuf,
    generation: u64,
    pending: Option<u64>,
    last: Option<Result<Vec<PathBuf>, PickError>>,
}

impl PickerState {
    /// Fires the trampoline (UI-thread hop inside Java — off-thread
    /// activity calls are process-fatal, the `OppaUi` rule).
    /// Fire failure latches loud `Backend` (the void-request rule
    /// above — the next poll surfaces it, never a stuck pending).
    fn fire(&mut self, options: &FilePickerOptions) {
        self.generation += 1;
        let gen = self.generation;
        self.pending = Some(gen);
        let spec = saf_spec_for(options);
        let fired = with_attached(&self.app, |env, activity| {
            // App-loader class object (see `load_picker_class` —
            // the fire initializes `<clinit>` + loadLibrary under
            // this loader, where both resolve).
            let cls_obj = load_picker_class(env, &activity, "com.oppa.app.OppaPickerActivity")?;
            let cls: &jni::objects::JClass = (&cls_obj).into();
            let elem_cls = env
                .find_class("java/lang/String")
                .map_err(|e| format!("find String: {e}"))?;
            let arr = env
                .new_object_array(
                    spec.patterns.len() as i32,
                    &elem_cls,
                    jni::objects::JObject::null(),
                )
                .map_err(|e| format!("mime array: {e}"))?;
            for (i, pattern) in spec.patterns.iter().enumerate() {
                let s = env
                    .new_string(pattern)
                    .map_err(|e| format!("pattern string: {e}"))?;
                env.set_object_array_element(&arr, i as i32, s)
                    .map_err(|e| format!("mime element: {e}"))?;
            }
            env.call_static_method(
                cls,
                "pick",
                "(Landroid/app/Activity;[Ljava/lang/String;ZI)V",
                &[
                    jni::objects::JValue::from(&activity),
                    jni::objects::JValue::from(&arr),
                    jni::objects::JValue::Bool(u8::from(spec.allow_multiple)),
                    jni::objects::JValue::Int(gen as i32),
                ],
            )
            .map_err(|e| format!("OppaPickerActivity.pick: {e}"))?;
            Ok(())
        });
        if let Err(e) = fired {
            self.pending = None;
            self.last = Some(Err(PickError::Backend(format!("picker fire: {e}"))));
        }
    }
}

/// Installs the bridge (call once, before mount): creates the
/// cache dir, latches the rendezvous, and dry-resolves the fire
/// entry (a missing `OppaPickerActivity` fails loudly here, never
/// at first tap).
pub fn ensure_picker(app: &AndroidApp, files_dir: &Path) -> Result<String, String> {
    if INSTALLED.load(std::sync::atomic::Ordering::Acquire) {
        return Ok("picker=already-installed".to_string());
    }
    let cache_dir = files_dir.join("picked");
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("picked dir: {e}"))?;
    PICKER
        .set(Mutex::new(PickerState {
            app: app.clone(),
            cache_dir,
            generation: 0,
            pending: None,
            last: None,
        }))
        .map_err(|_| "picker install race".to_string())?;
    ensure_picker_callbacks(app)?;
    Ok("picker=installed".to_string())
}

/// Resolves the pending request from the trampoline callback
/// (UI thread): dismissed → empty, bytes → cache-copy + path,
/// stale generations discard (never half-swapped state).
/// Callbacks before install log loudly and drop.
fn resolve_pick(content: Option<Vec<u8>>, name: Option<String>, generation: u64, dismissed: bool) {
    let Some(lock) = PICKER.get() else {
        eprintln!("oppa-picker: callback before install — dropped");
        return;
    };
    let mut state = match lock.lock() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("oppa-picker: rendezvous lock failed: {e}");
            return;
        }
    };
    if state.pending != Some(generation) {
        eprintln!("oppa-picker: stale generation {generation} — dropped");
        return;
    }
    state.pending = None;
    if dismissed {
        state.last = Some(Ok(Vec::new()));
        return;
    }
    let Some(bytes) = content else {
        state.last = Some(Err(PickError::Backend("picker read failed".to_string())));
        return;
    };
    let file_name = name
        .as_deref()
        .and_then(|n| Path::new(n).file_name())
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("picked");
    let dest = state.cache_dir.join(file_name);
    if let Err(e) = std::fs::write(&dest, &bytes) {
        state.last = Some(Err(PickError::Backend(format!("picker cache: {e}"))));
        return;
    }
    state.last = Some(Ok(vec![dest]));
}

/// Loads a class through the *app* `PathClassLoader` (never the
/// `DexClassLoader`): `activity.getClassLoader()` returns the
/// loader that owns the app's own classes, so everything downstream
/// — the `pick()` fire (which initializes `<clinit>` +
/// `System.loadLibrary`) and the reverse static-native dispatch —
/// runs under one loader that sees both the dex and the lib dir.
/// Loading through `ime_bridge::load_app_class`'s DexClassLoader
/// instead runs `<clinit>` where the lib is invisible and kills the
/// process. Load-only here (no initialization, no side effects).
fn load_picker_class<'a>(
    env: &mut jni::JNIEnv<'a>,
    activity: &jni::objects::JObject<'_>,
    name: &str,
) -> Result<jni::objects::JObject<'a>, String> {
    let loader = env
        .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .map_err(|e| format!("getClassLoader: {e}"))?
        .l()
        .map_err(|e| format!("loader obj: {e}"))?;
    if loader.is_null() {
        return Err("app ClassLoader is null".to_string());
    }
    let name_jstr = env
        .new_string(name)
        .map_err(|e| format!("class name string: {e}"))?;
    let cls = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[jni::objects::JValue::from(&name_jstr)],
        )
        .map_err(|e| format!("loadClass {name}: {e}"))?
        .l()
        .map_err(|e| format!("{name} class: {e}"))?;
    if cls.is_null() {
        return Err(format!("loadClass {name} returned null"));
    }
    Ok(cls)
}

/// Verifies the `OppaPickerActivity` fire entry exists (idempotent —
/// same rule as the IME entries: the second call records instead
/// of re-resolving).
///
/// NOTE (linkage rule, learned the loud way twice): only the
/// *forward* direction (Rust → Java `pick()`) resolves through the
/// DexClassLoader-loaded class object. The *reverse* direction
/// (system → native `onPickerResult`) dispatches through the
/// PathClassLoader's class object — a different object — so
/// `RegisterNatives` here silently misses and the first pick dies
/// with `UnsatisfiedLinkError`. The callback below is therefore
/// static-linked (`Java_…` export) with `System.loadLibrary` in the
/// Java `<clinit>` (loader association must happen under the app
/// loader — never force class initialization from native: resolving
/// the fire entry this way runs `<clinit>` under the Dex loader and
/// fails the whole setup loudly). This check loads the class only
/// (no initialization, no side effects).
pub fn ensure_picker_callbacks(app: &AndroidApp) -> Result<String, String> {
    if INSTALLED.load(std::sync::atomic::Ordering::Acquire) {
        return Ok("picker-callbacks=already-registered".to_string());
    }
    with_attached(app, |env, activity| {
        let cls_obj = load_picker_class(env, &activity, "com.oppa.app.OppaPickerActivity")?;
        let cls: &jni::objects::JClass = (&cls_obj).into();
        // Existence check only — never resolve method IDs here
        // (resolution would initialize the class under this
        // loader and trip its `<clinit>` loadLibrary loudly; a
        // typo'd fire signature surfaces at first tap through the
        // latched `Backend` instead).
        if cls.is_null() {
            return Err("OppaPickerActivity class missing".to_string());
        }
        Ok("picker-callbacks=registered".to_string())
    })?;
    INSTALLED.store(true, std::sync::atomic::Ordering::Release);
    Ok("picker-callbacks=registered".to_string())
}

/// `OppaPickerActivity.onPickerResult` — the trampoline resolves
/// here (UI thread): bytes + name + generation + dismissed flag.
/// Static-linked (see the linkage rule above — `RegisterNatives`
/// would miss across class loaders). Short JNI name (the method is
/// not overloaded, so the runtime tries this exact spelling first;
/// the long `__<sig>` suffix form miscounts underscores easily and
/// buys nothing here). A bad string fails loudly to stderr (the
/// device log channel — a void entry has no `error.txt` path) and
/// settles loud `Backend`, never a silent drop.
#[unsafe(no_mangle)]
unsafe extern "system" fn Java_com_oppa_app_OppaPickerActivity_onPickerResult(
    mut env: jni::JNIEnv<'_>,
    _class: jni::objects::JClass<'_>,
    content: jni::objects::JByteArray<'_>,
    name: jni::objects::JString<'_>,
    generation: jni::sys::jint,
    dismissed: jni::sys::jboolean,
) {
    if dismissed != 0 {
        resolve_pick(None, None, generation as u64, true);
        return;
    }
    if content.is_null() {
        resolve_pick(None, None, generation as u64, false);
        return;
    }
    let len = match env.get_array_length(&content) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("oppa-picker: array length failed: {e:?}");
            resolve_pick(None, None, generation as u64, false);
            return;
        }
    };
    let mut raw = vec![0i8; len as usize];
    if let Err(e) = env.get_byte_array_region(&content, 0, &mut raw) {
        eprintln!("oppa-picker: array read failed: {e:?}");
        resolve_pick(None, None, generation as u64, false);
        return;
    }
    let bytes: Vec<u8> = raw.iter().map(|b| *b as u8).collect();
    let name: Option<String> = if name.is_null() {
        None
    } else {
        match env.get_string(&name) {
            Ok(s) => Some(s.into()),
            Err(e) => {
                eprintln!("oppa-picker: name string failed: {e:?}");
                None
            }
        }
    };
    resolve_pick(Some(bytes), name, generation as u64, false);
}

/// Zero-state handle for control props (the handle rule): all
/// state lives in the process-wide rendezvous, so the sink holds
/// a plain `Rc` and the UI thread resolves behind the lock.
/// `!Send` like every UI-thread type (the handle itself never
/// crosses threads — only the rendezvous lock does).
#[derive(Clone, Copy, Debug, Default)]
pub struct PickerHandle;

impl FileDialog for PickerHandle {
    fn request_open(&mut self, options: FilePickerOptions) {
        let Some(lock) = PICKER.get() else {
            eprintln!("oppa-picker: request before install — dropped");
            return;
        };
        match lock.lock() {
            Ok(mut s) => s.fire(&options),
            Err(e) => eprintln!("oppa-picker: request lock failed: {e}"),
        }
    }

    fn poll_open(&mut self) -> Option<Result<Vec<PathBuf>, PickError>> {
        let lock = PICKER.get()?;
        match lock.lock() {
            Ok(s) => {
                if s.pending.is_some() {
                    None
                } else {
                    s.last.clone()
                }
            }
            Err(_) => Some(Err(PickError::Backend("picker lock failed".to_string()))),
        }
    }
}
