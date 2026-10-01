//! IME validation on device (v1 remainder, Gap 3): the M1
//! composition shapes through the core dispatch seam plus the
//! InputMethodManager policy through JNI, routed through the
//! shell's IME log.
//!
//! What runs here (and what does not — stated, not hidden):
//!
//! - The M1 scenario *shapes* (zh candidate commit, cancel-mid,
//!   delete-range re-anchor) drive `dispatch_ime_event` into a
//!   recording handler, once before the tap phase and once after.
//!   Both canonical streams must equal the scripts exactly
//!   (no lost/duplicated — the M0b criterion-3 shape). The full
//!   `EditingSession` does not run on-device: `spike-textedit`
//!   depends on Windows-only crates, so the session stays a
//!   Windows-track proof and the device runs its event shapes.
//! - `CompositionStarted` routes `ShowCandidateWindow` and
//!   commit/cancel routes `HideCandidateWindow` into the
//!   `AndroidShell` IME log (the framework-to-IMM policy direction).
//!   `SetCaretRect` anchoring is NOT synthesized: there is no
//!   laid-out caret on-device in this loop, and a fabricated anchor
//!   would prove nothing (recorded restriction).
//! - The IMM half calls the real manager over JNI: service
//!   non-null, enabled-method count, `showSoftInput` on the decor
//!   view, then `hideSoftInputFromWindow` with its token. Each
//!   result is recorded verbatim (a `false` from show is a platform
//!   fact — no focused view — not a failure).

use android_activity::AndroidApp;
use oppa::ime::ImeOps;
use oppa::ime::{dispatch_ime_event, ImeCompositionEvent, ImeCompositionHandler};
use oppa_shell_android::shell::AndroidShell;

/// One canonical stream entry per dispatched event.
struct RecordingHandler {
    canonical: Vec<String>,
}

impl ImeCompositionHandler for RecordingHandler {
    fn composition_started(&mut self, start_byte: usize) {
        self.canonical.push(format!("started@{start_byte}"));
    }
    fn composition_updated(&mut self, composition: &str, caret_byte: usize) {
        self.canonical
            .push(format!("updated:{composition}@{caret_byte}"));
    }
    fn composition_committed(&mut self, committed: &str) {
        self.canonical.push(format!("committed:{committed}"));
    }
    fn composition_cancelled(&mut self) {
        self.canonical.push("cancelled".to_string());
    }
    fn delete_range(&mut self, range: (usize, usize)) {
        self.canonical
            .push(format!("delete:({},{})", range.0, range.1));
    }
}

/// The M1 scenario shapes (zh commit / cancel-mid / delete-range),
/// each with its expected canonical stream.
fn scenarios() -> Vec<(Vec<ImeCompositionEvent>, Vec<String>)> {
    vec![
        (
            vec![
                ImeCompositionEvent::CompositionStarted { start_byte: 0 },
                ImeCompositionEvent::CompositionUpdated {
                    composition: "nihao".to_string(),
                    caret_byte: 5,
                },
                ImeCompositionEvent::CompositionUpdated {
                    // Candidate commit-with-different-text (the M1
                    // emulation of candidate selection).
                    composition: "\u{4F60}\u{597D}".to_string(),
                    caret_byte: 6,
                },
                ImeCompositionEvent::CompositionCommitted {
                    committed: "\u{4F60}\u{597D}".to_string(),
                },
            ],
            vec![
                "started@0".to_string(),
                "updated:nihao@5".to_string(),
                "updated:\u{4F60}\u{597D}@6".to_string(),
                "committed:\u{4F60}\u{597D}".to_string(),
            ],
        ),
        (
            vec![
                ImeCompositionEvent::CompositionStarted { start_byte: 3 },
                ImeCompositionEvent::CompositionUpdated {
                    composition: "konnitiha".to_string(),
                    caret_byte: 12,
                },
                ImeCompositionEvent::CompositionCancelled,
            ],
            vec![
                "started@3".to_string(),
                "updated:konnitiha@12".to_string(),
                "cancelled".to_string(),
            ],
        ),
        (
            vec![
                ImeCompositionEvent::CompositionStarted { start_byte: 0 },
                ImeCompositionEvent::DeleteRange { range: (0, 5) },
                ImeCompositionEvent::CompositionUpdated {
                    composition: "ni".to_string(),
                    caret_byte: 2,
                },
                ImeCompositionEvent::CompositionCommitted {
                    committed: "ni".to_string(),
                },
            ],
            vec![
                "started@0".to_string(),
                "delete:(0,5)".to_string(),
                "updated:ni@2".to_string(),
                "committed:ni".to_string(),
            ],
        ),
    ]
}

/// Runs every scenario through the dispatch seam, routing policy
/// ops into `shell`, and asserts canonical == script. Returns
/// `scenarios=3/3 events=<n>`.
pub fn run_composition_feed(shell: &mut AndroidShell) -> Result<String, String> {
    use oppa::shell::PlatformShell;
    let mut total = 0;
    for (events, expected) in scenarios() {
        let mut handler = RecordingHandler {
            canonical: Vec::new(),
        };
        for event in &events {
            match event {
                ImeCompositionEvent::CompositionStarted { .. } => {
                    shell.set_ime(ImeOps::ShowCandidateWindow);
                }
                ImeCompositionEvent::CompositionCommitted { .. }
                | ImeCompositionEvent::CompositionCancelled => {
                    shell.set_ime(ImeOps::HideCandidateWindow);
                }
                _ => {}
            }
            dispatch_ime_event(&mut handler, event);
        }
        total += events.len();
        if handler.canonical != expected {
            return Err(format!(
                "canonical drift: got {:?}, want {expected:?}",
                handler.canonical
            ));
        }
    }
    Ok(format!("scenarios=3/3 events={total}"))
}

/// Real IMM policy calls over JNI. Returns the verbatim record.
/// `show=false` (no focused view in a NativeActivity) is recorded,
/// not failed.
pub fn imm_policy(app: &AndroidApp) -> Result<String, String> {
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
    let service_name = env
        .new_string("input_method")
        .map_err(|e| format!("new_string: {e}"))?;
    let imm = env
        .call_method(
            &activity,
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
    let list = env
        .call_method(&imm, "getEnabledInputMethodList", "()Ljava/util/List;", &[])
        .map_err(|e| format!("getEnabledInputMethodList: {e}"))?
        .l()
        .map_err(|e| format!("method list: {e}"))?;
    let enabled = env
        .call_method(&list, "size", "()I", &[])
        .map_err(|e| format!("List.size: {e}"))?
        .i()
        .map_err(|e| format!("size int: {e}"))?;
    // Decor view for show/hide (the NativeActivity window's view).
    let window = env
        .call_method(&activity, "getWindow", "()Landroid/view/Window;", &[])
        .map_err(|e| format!("getWindow: {e}"))?
        .l()
        .map_err(|e| format!("Window: {e}"))?;
    let view = env
        .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])
        .map_err(|e| format!("getDecorView: {e}"))?
        .l()
        .map_err(|e| format!("decor view: {e}"))?;
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
    let token = env
        .call_method(&view, "getWindowToken", "()Landroid/os/IBinder;", &[])
        .map_err(|e| format!("getWindowToken: {e}"))?
        .l()
        .map_err(|e| format!("token: {e}"))?;
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
    Ok(format!(
        "imm=present enabled={enabled} show={show} hide={hide}"
    ))
}
