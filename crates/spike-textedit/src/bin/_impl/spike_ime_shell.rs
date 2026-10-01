// The M1-remainder host: one editable single-line field in a REAL Win32
// window, with REAL OS IME wiring ” the vehicle for the manual real-IME
// pass that gates the DOM text/editing contract's freeze (DESIGN §2.3,
// REPORT.md finding #5).
//
// Wiring (the M0b contract surfaces, now fed by a real IME):
// - real window proc ’ `Win32Shell` (implements `PlatformShell`);
//   `pump_events` drains real mouse/keyboard events; `set_ime` performs
//   the real `ImmSetCompositionWindow` / `ImmSetCandidateWindow`
//   candidate-window anchoring.
// - real OS IME messages (`WM_IME_*`, snapshotted via
//   `ImmGetCompositionString` at message time) ’ normalized
//   `ImeCompositionEvent`s ’ `dispatch_ime_event` ’ the `EditingSession`
//   (`ImeCompositionHandler`) ” the same seam the spike proved with
//   scripted input, now sourced from a real IME instead of
//   `ImeCompositionFeed`.
// - Vello debug rendering: the field's text (via `ShapedRun`'s glyph
//   output), the caret rect, the selection highlight, the composition
//   underline. Debug-grade; throwaway once the real Vello backend (M3+)
//   lands.
//
// Modes:
// - (default) an interactive window for a human.
// - `--ime-pass` automated: activates the installed zh-Hans-CN Microsoft
//   Pinyin IME, drives the delete-range-mid-composition scenario with
//   real key input (`SendInput` ” real keys through the real OS IME, not
//   CDP scripting and not the spike's `ImeCompositionFeed`), records the
//   raw Win32 message stream + the `ImmGetCompositionString` reads, and
//   writes `spike/results/ime_manual.json`.
//
// (Round 19.2: this body is included into the bin's `#[cfg(windows)] mod
// imp` — inner attributes/doc comments are illegal in an include!
// expansion, so the historical `//!` block became plain comments and the
// former `#![cfg(windows)]` crate attr moved to the wrapper's module gate;
// the crate-level doc lives on the bin wrapper.)

use oppa::ime::{dispatch_ime_event, ImeCompositionEvent, ImeOps};
use oppa::reactive::Runtime;
use oppa::shell::{Event, PlatformShell};
use oppa::text::{TextService, TextStyle};
use oppa_shell_win::{
    Cmd, ImeMessage, ImeState, MessageRecord, ShellConfig, Win32Shell, FIELD_EVENT,
};
use oppa_text_dwrite::DWriteTextService;
use spike_textedit::json::J;
use spike_textedit::session::{EditingSession, SessionState};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};
use wgpu::SurfaceTargetUnsafe;

// ---------------------------------------------------------------------------
// Field geometry (client px, DPR 1)
// ---------------------------------------------------------------------------

const ORIGIN_X: f32 = 24.0;
const BASELINE_Y: f32 = 96.0;
const FIELD_TOP: f32 = 64.0;
const FIELD_BOTTOM: f32 = 132.0;
const FIELD_RIGHT: f32 = 616.0;

const VK_HOME: u32 = 0x24;
const VK_END: u32 = 0x23;
const VK_LEFT: u32 = 0x25;
const VK_RIGHT: u32 = 0x27;
const VK_Z: u32 = 0x5A;
const VK_SHIFT: u32 = 0x10;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_CONTROL: u32 = 0x11;
const VK_N: u32 = 0x4E;
const VK_I: u32 = 0x49;
const VK_H: u32 = 0x48;
const VK_A: u32 = 0x41;
const VK_O: u32 = 0x4F;

pub fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--list-profiles") {
        return list_profiles();
    }
    let automated = args.iter().any(|a| a == "--ime-pass");
    // Pre-pass focus window: `--wait-secs N` pumps messages for N seconds
    // before the first step so a human can focus the window. The pass
    // itself (steps, keys, timing, verdict) is unchanged.
    let wait_secs = args
        .iter()
        .position(|a| a == "--wait-secs")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    app(automated, wait_secs)
}

// ---------------------------------------------------------------------------
// The IME mapper: real Win32 IME messages ’ the normalized surface
// ---------------------------------------------------------------------------

struct ImeMapper {
    session: Rc<RefCell<EditingSession>>,
    /// A composition is in flight (started, not yet committed/cancelled).
    comp_active: bool,
    /// UTF-16 length of a just-committed string whose WM_CHARs (if the IME
    /// also delivers them) are swallowed so committed text is not inserted
    /// twice.
    swallow_pending: usize,
}

impl ImeMapper {
    /// Real OS IME message ’ the normalized surface ’ the session.
    ///
    /// Mapping decisions (recorded in the round log):
    /// - Composition over an active selection: `CompositionStarted` is
    ///   fed BEFORE `DeleteRange{selection}` so the session's atomic
    ///   pre-composition undo snapshot captures the PRE-deletion content
    ///   (Ctrl+Z must restore the selection's original text).
    /// - `GCS_RESULTSTR` at `WM_IME_COMPOSITION` is the commit; `END`
    ///   without a commit is the cancel.
    /// - `GCS_CURSORPOS` is composition-relative UTF-16; mapped to UTF-8
    ///   bytes and reported in composite coordinates (spike decision 20).
    fn handle(&mut self, msg: &ImeMessage) {
        let mut session = self.session.borrow_mut();
        match msg {
            ImeMessage::StartComposition => {
                let sel = session.selection();
                let anchor = sel.0;
                dispatch_ime_event(
                    &mut *session,
                    &ImeCompositionEvent::CompositionStarted { start_byte: anchor },
                );
                if sel.0 != sel.1 {
                    dispatch_ime_event(
                        &mut *session,
                        &ImeCompositionEvent::DeleteRange { range: sel },
                    );
                }
                self.comp_active = true;
            }
            ImeMessage::Composition {
                comp,
                cursor_pos,
                result,
                ..
            } => {
                if let Some(text) = result {
                    dispatch_ime_event(
                        &mut *session,
                        &ImeCompositionEvent::CompositionCommitted {
                            committed: text.clone(),
                        },
                    );
                    self.swallow_pending = text.encode_utf16().count();
                    self.comp_active = false;
                } else if let Some(comp) = comp {
                    if !self.comp_active {
                        // An update with no begin (platforms race): anchor.
                        let anchor = session.selection().0;
                        dispatch_ime_event(
                            &mut *session,
                            &ImeCompositionEvent::CompositionStarted { start_byte: anchor },
                        );
                        self.comp_active = true;
                    }
                    let caret = utf16_offset_to_byte(comp, (*cursor_pos).max(0) as usize);
                    let start = session.composition_start_byte().unwrap_or(0);
                    dispatch_ime_event(
                        &mut *session,
                        &ImeCompositionEvent::CompositionUpdated {
                            composition: comp.clone(),
                            caret_byte: start + caret,
                        },
                    );
                }
            }
            ImeMessage::EndComposition if self.comp_active => {
                // END without a result commit: the IME cancelled the
                // composition (ESC, or a superseding action).
                dispatch_ime_event(&mut *session, &ImeCompositionEvent::CompositionCancelled);
                self.comp_active = false;
            }
            _ => {}
        }
    }
}

fn utf16_offset_to_byte(text: &str, mut utf16_index: usize) -> usize {
    for (byte, ch) in text.char_indices() {
        let units = ch.len_utf16();
        if utf16_index < units {
            return byte;
        }
        utf16_index -= units;
    }
    text.len()
}

// ---------------------------------------------------------------------------
// The automated pass
// ---------------------------------------------------------------------------

/// One recorded pass observation: the marker, the session's observable
/// state right after the step settled, and the caret rects the shell
/// anchored through `set_ime` since the previous marker.
struct PassStep {
    marker: String,
    observable: SessionState,
    anchored: Vec<[f32; 4]>,
    ime: ImeState,
    foreground: bool,
    notes: Vec<String>,
}

struct Driver {
    next: usize,
    finished: bool,
}

/// One pass step: the marker recorded, and the VK keys sent through the
/// real OS IME (`SendInput` ” real keyboard events, not CDP scripting).
struct DriverStep {
    marker: &'static str,
    keys: &'static [u32],
    shift: bool,
    ctrl: bool,
}

const STEPS: &[DriverStep] = &[
    DriverStep {
        marker: "init",
        keys: &[],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "home",
        keys: &[VK_HOME],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "select-all (ctrl+a)",
        keys: &[VK_A],
        shift: false,
        ctrl: true,
    },
    DriverStep {
        marker: "r1: n",
        keys: &[VK_N],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "r1: i",
        keys: &[VK_I],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "r1: h",
        keys: &[VK_H],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "r1: a",
        keys: &[VK_A],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "r1: o",
        keys: &[VK_O],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "commit (space)",
        keys: &[VK_SPACE],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "undo (ctrl+z)",
        keys: &[VK_Z],
        shift: false,
        ctrl: true,
    },
    DriverStep {
        marker: "r2: n",
        keys: &[VK_N],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "r2: i",
        keys: &[VK_I],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "r2: h",
        keys: &[VK_H],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "r2: a",
        keys: &[VK_A],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "r2: o",
        keys: &[VK_O],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "cancel (esc)",
        keys: &[VK_ESCAPE],
        shift: false,
        ctrl: false,
    },
    DriverStep {
        marker: "pass end",
        keys: &[],
        shift: false,
        ctrl: false,
    },
];

/// Diagnostic harness (no window, no scenario): list the OS-registered
/// TSF language profiles for zh-CN and the currently active one, so the
/// hardcoded TIP CLSID / profile GUID in `arm_ime` can be checked
/// against reality instead of assumed.
fn list_profiles() -> Result<(), String> {
    use windows::core::GUID;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::TextServices::{
        CLSID_TF_InputProcessorProfiles, ITfInputProcessorProfiles, TF_LANGUAGEPROFILE,
    };
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() && hr != windows::core::HRESULT(1) {
            return Err(format!("CoInitializeEx: {hr:?}"));
        }
        let profiles: ITfInputProcessorProfiles =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("CoCreateInstance: {e}"))?;
        let enumerator = profiles
            .EnumLanguageProfiles(0x0804)
            .map_err(|e| format!("EnumLanguageProfiles(0x0804): {e}"))?;
        println!("registered profiles for langid 0x0804:");
        loop {
            let mut buf = [TF_LANGUAGEPROFILE::default(); 4];
            let mut fetched = 0u32;
            enumerator
                .Next(&mut buf, &mut fetched)
                .map_err(|e| format!("Next: {e}"))?;
            if fetched == 0 {
                break;
            }
            for p in buf.iter().take(fetched as usize) {
                println!(
                    "  clsid={:?} profile={:?} active={} catid={:?}",
                    p.clsid,
                    p.guidProfile,
                    p.fActive.as_bool(),
                    p.catid
                );
            }
        }
        // Active profile per the TIP CLSID arm_ime passes.
        let tip = GUID::from_u128(0x81D4E9C9_1D3B_41BC_9E6C_4B40BF79E35E);
        let mut langid = 0u16;
        let mut active = GUID::zeroed();
        match profiles.GetActiveLanguageProfile(&tip, &mut langid, &mut active) {
            Ok(()) => println!("active for TIP {tip:?}: langid={langid:#x} profile={active:?}"),
            Err(e) => println!("GetActiveLanguageProfile(TIP): FAILED {e}"),
        }
        println!("arm_ime assumes profile FA550B04-5AD7-411F-A5AC-CA038EC515D7");
        // Attempt the exact activation arm_ime performs, then re-read
        // state: does the HRESULT reproduce in isolation, and does the
        // active profile change even on failure?
        let profile = GUID::from_u128(0xFA550B04_5AD7_411F_A5AC_CA038EC515D7);
        match profiles.ActivateLanguageProfile(&tip, 0x0804, &profile) {
            Ok(()) => println!("probe ActivateLanguageProfile: S_OK"),
            Err(e) => println!("probe ActivateLanguageProfile: FAILED {e}"),
        }
        let mut langid2 = 0u16;
        let mut active2 = GUID::zeroed();
        match profiles.GetActiveLanguageProfile(&tip, &mut langid2, &mut active2) {
            Ok(()) => {
                println!("active after attempt: langid={langid2:#x} profile={active2:?}")
            }
            Err(e) => println!("GetActiveLanguageProfile(after): FAILED {e}"),
        }
    }
    Ok(())
}
/// COM init, TSF profile activation (the real IME), the HKL fallback, and
/// the IMM open/conversion state (native pinyin, phrase prediction). Every
/// step is logged — the pass's environment record. Returns an error if no
/// real IME could be activated (never a silent no-op: the pass must run
/// against a real IME).
fn arm_ime(
    shell: &Win32Shell,
    env_log: &std::rc::Rc<std::cell::RefCell<Vec<String>>>,
) -> Result<(), String> {
    use windows::core::{GUID, HRESULT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Input::Ime::{
        ImmGetContext, ImmGetConversionStatus, ImmGetOpenStatus, ImmReleaseContext,
        ImmSetConversionStatus, ImmSetOpenStatus, IME_CMODE_NATIVE, IME_SMODE_PHRASEPREDICT,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        ActivateKeyboardLayout, GetKeyboardLayoutList, ACTIVATE_KEYBOARD_LAYOUT_FLAGS, HKL,
    };
    use windows::Win32::UI::TextServices::{
        CLSID_TF_InputProcessorProfiles, ITfInputProcessorProfiles,
    };

    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        env_log
            .borrow_mut()
            .push(format!("CoInitializeEx(APARTMENTTHREADED): {hr:?}"));
        if hr.is_err() && hr != HRESULT(1) {
            // S_OK(0) / S_FALSE(1) are both fine; anything else is fatal.
            return Err(format!("CoInitializeEx failed: {hr:?}"));
        }

        // TSF profile activation happens at the END of arming (after the
        // window is focused and the HKL + IMM context are set): the call
        // needs every precondition in place. GUIDs verified against
        // EnumLanguageProfiles output (--list-profiles).
        let tip_clsid = GUID::from_u128(0x81D4E9C9_1D3B_41BC_9E6C_4B40BF79E35E);
        let profile = GUID::from_u128(0xFA550B04_5AD7_411F_A5AC_CA038EC515D7);
        let profiles: ITfInputProcessorProfiles =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("CoCreateInstance(InputProcessorProfiles): {e}"))?;

        // Fallback / verification: switch the thread to an HKL whose
        // language is zh (0x0804) — the IME HKL the system exposes once the
        // language is in the user list.
        let n = GetKeyboardLayoutList(None);
        if n <= 0 {
            return Err("GetKeyboardLayoutList returned nothing".to_string());
        }
        let mut list = vec![HKL::default(); n as usize];
        let got = GetKeyboardLayoutList(Some(&mut list));
        let mut zh_hkl = None;
        for hkl in list.iter().take(got as usize) {
            let lang = hkl.0 as usize & 0xFFFF;
            env_log
                .borrow_mut()
                .push(format!("HKL {:#018x} (lang {:#06x})", hkl.0 as usize, lang));
            if lang == 0x0804 {
                zh_hkl = Some(*hkl);
            }
        }
        if let Some(hkl) = zh_hkl {
            match ActivateKeyboardLayout(hkl, ACTIVATE_KEYBOARD_LAYOUT_FLAGS(0)) {
                Ok(_) => env_log
                    .borrow_mut()
                    .push("ActivateKeyboardLayout(zh HKL) OK".to_string()),
                Err(e) => env_log
                    .borrow_mut()
                    .push(format!("ActivateKeyboardLayout FAILED: {e}")),
            }
        } else {
            env_log
                .borrow_mut()
                .push("no zh (0x0804) HKL found in the layout list".to_string());
        }

        // The IME context: open + native pinyin + phrase prediction.
        let hwnd = shell.hwnd();
        let himc = ImmGetContext(hwnd);
        if himc.is_invalid() {
            return Err("ImmGetContext: no IME context for the window".to_string());
        }
        let opened = ImmGetOpenStatus(himc).as_bool();
        env_log
            .borrow_mut()
            .push(format!("ImmGetOpenStatus before: {opened}"));
        if !ImmSetOpenStatus(himc, true).as_bool() {
            return Err("ImmSetOpenStatus(true) failed".to_string());
        }
        let mut conv = windows::Win32::UI::Input::Ime::IME_CONVERSION_MODE::default();
        let mut sent = windows::Win32::UI::Input::Ime::IME_SENTENCE_MODE::default();
        if !ImmGetConversionStatus(himc, Some(&mut conv), Some(&mut sent)).as_bool() {
            return Err("ImmGetConversionStatus failed".to_string());
        }
        env_log.borrow_mut().push(format!(
            "conversion before: {:#x}, sentence before: {:#x}",
            conv.0, sent.0
        ));
        if !ImmSetConversionStatus(himc, IME_CMODE_NATIVE, IME_SMODE_PHRASEPREDICT).as_bool() {
            return Err("ImmSetConversionStatus failed".to_string());
        }
        if !ImmGetConversionStatus(himc, Some(&mut conv), Some(&mut sent)).as_bool() {
            return Err("ImmGetConversionStatus (verify) failed".to_string());
        }
        env_log.borrow_mut().push(format!(
            "conversion after: {:#x}, sentence after: {:#x}",
            conv.0, sent.0
        ));
        let _ = ImmReleaseContext(hwnd, himc);

        // TSF profile activation, last: window focused + HKL armed + IMM
        // context open. Foreground + resulting active profile are logged
        // as state evidence (the call flaps S_OK/E_INVALIDARG across
        // runs with identical args — see the session record).
        env_log.borrow_mut().push(format!(
            "pre-activate: foreground={}",
            shell.is_foreground()
        ));
        match profiles.ActivateLanguageProfile(&tip_clsid, 0x0804, &profile) {
            Ok(()) => env_log.borrow_mut().push(
                "TSF ActivateLanguageProfile: Microsoft Pinyin (zh-Hans-CN) activated".to_string(),
            ),
            Err(e) => env_log.borrow_mut().push(format!(
                "TSF ActivateLanguageProfile (specific profile) FAILED: {e}"
            )),
        }
        let mut got_lang = 0u16;
        let mut got_profile = GUID::zeroed();
        match profiles.GetActiveLanguageProfile(&tip_clsid, &mut got_lang, &mut got_profile) {
            Ok(()) => env_log.borrow_mut().push(format!(
                "post-activate: active langid={got_lang:#x} profile={got_profile:?}"
            )),
            Err(e) => env_log
                .borrow_mut()
                .push(format!("post-activate: GetActive FAILED: {e}")),
        }
    }
    Ok(())
}

/// Re-arms the IME's mode when the context reads alphanumeric (MS Pinyin
/// re-initializes its per-document mode at focus attach; its EN/CH toggle
/// is the Shift key — a Shift tap flips it back to native pinyin).
fn rearm_ime_if_reset(
    shell: &Win32Shell,
    _env_log: &std::rc::Rc<std::cell::RefCell<Vec<String>>>,
) -> Vec<String> {
    use windows::Win32::UI::Input::Ime::{
        ImmGetContext, ImmReleaseContext, ImmSetConversionStatus, ImmSetOpenStatus,
        IME_CMODE_NATIVE, IME_SMODE_PHRASEPREDICT,
    };
    let mut notes = Vec::new();
    let status = shell.ime_status();
    if status.conversion_mode == IME_CMODE_NATIVE.0 && status.context_open {
        return notes;
    }
    // The IMM path (open + native conversion) — reported as tried.
    unsafe {
        let himc = ImmGetContext(shell.hwnd());
        if himc.is_invalid() {
            notes.push("rearm: no context".to_string());
            return notes;
        }
        let opened = ImmSetOpenStatus(himc, true).as_bool();
        let conv =
            ImmSetConversionStatus(himc, IME_CMODE_NATIVE, IME_SMODE_PHRASEPREDICT).as_bool();
        let _ = ImmReleaseContext(shell.hwnd(), himc);
        notes.push(format!(
            "rearm: imm open={opened} conv={conv} (was conv={:#x}, hkl {:#x})",
            status.conversion_mode, status.active_hkl
        ));
    }
    // If the mode still reads alphanumeric, that is recorded as the
    // finding: the app cannot flip MS Pinyin's per-document EN/CH mode
    // from the IMM side, and an injected Shift tap types a stray
    // character rather than toggling the IME.
    if shell.ime_status().conversion_mode != IME_CMODE_NATIVE.0 {
        let after = shell.ime_status();
        notes.push(format!(
            "rearm: shift tap NOT applied (mode still conv={:#x})",
            after.conversion_mode
        ));
    }
    notes
}
impl Driver {
    fn new() -> Self {
        Self {
            next: 0,
            finished: false,
        }
    }

    /// One pass step, synchronously: log the marker, send the keys
    /// (real keyboard events through the real OS IME), then pump until
    /// the IME's async message burst settles, run one session frame, and
    /// record the post-state. Per-EVENT records — the IME commits faster
    /// than any step pacing, so the record happens inside the step.
    fn tick(
        &mut self,
        shell: &Rc<RefCell<Win32Shell>>,
        session: &Rc<RefCell<EditingSession>>,
        rt: &Runtime,
        _env_log: &std::rc::Rc<std::cell::RefCell<Vec<String>>>,
        pass_log: &mut Vec<PassStep>,
    ) {
        if self.finished {
            return;
        }

        // Keep the IME's mode armed (MS Pinyin resets it at focus attach).
        let notes: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        rearm_ime_if_reset(&shell.borrow(), &notes);
        // TSF focus re-assert (additive; steps, keys and settle timing
        // below are unchanged from IME-PASS.md §3).
        if let Some(line) = shell.borrow_mut().tsf_reassert_focus() {
            notes.borrow_mut().push(line);
        }
        let rearm_notes = notes.borrow().clone();

        // The step.
        let step = &STEPS[self.next];
        let marker = step.marker;
        if !shell.borrow().is_foreground() {
            let _ = shell.borrow_mut().focus_window();
        }
        shell.borrow_mut().log_marker(marker);
        if !step.keys.is_empty() {
            send_keys(step.keys, step.shift, step.ctrl);
        }
        self.next += 1;
        if self.next >= STEPS.len() {
            self.finished = true;
        }

        // Settle: drain the OS message queue and run session frames
        // until the burst is quiet.
        let deadline = std::time::Instant::now() + Duration::from_millis(320);
        loop {
            let _ = shell.borrow_mut().process_os_messages();
            rt.request_frame();
            let _ = rt.run_once();
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(12));
        }

        // Record the post-state (with the live IME engagement).
        let ime = shell.borrow().ime_status();
        let foreground = shell.borrow().is_foreground();
        let anchored = shell.borrow_mut().take_anchored_rects();
        let observable = session.borrow().observable();
        // TSF store: surface the TIP-transaction trace, then mirror the
        // settled session back into the store (the shell skips this while
        // a TIP composition owns the store). Steps, keys, settle timing
        // and verdict checks are unchanged.
        let mut step_notes = rearm_notes;
        step_notes.extend(shell.borrow_mut().tsf_take_store_log());
        if let Some(line) = shell
            .borrow_mut()
            .tsf_sync_external(&observable.content, observable.sel)
        {
            step_notes.push(line);
        }
        pass_log.push(PassStep {
            marker: marker.to_string(),
            observable,
            anchored,
            ime,
            foreground,
            notes: step_notes,
        });
    }
}

/// Real key input through the real OS IME: `SendInput` with the scan codes
/// resolved from the VK codes (real keyboard events — not CDP scripting
/// and not the spike's `ImeCompositionFeed`).
fn send_keys(vks: &[u32], shift: bool, ctrl: bool) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC, VIRTUAL_KEY,
    };
    unsafe {
        // The IME's keystroke hook sees the scancode stream; the letters
        // go through it with KEYEVENTF_SCANCODE. The extended keys
        // (Home/End/arrows) are the exception: their unextended scancodes
        // are the numpad keys (Home's 0x47 is Numpad-7), so they go
        // through the VK path.
        let extended = |vk: u32| matches!(vk, 0x23..=0x28);
        let kbd = |vk: u32, up: bool| {
            let scan = MapVirtualKeyW(vk, MAPVK_VK_TO_VSC) as u16;
            let (flags, _use_vk) = if extended(vk) {
                (
                    if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    false,
                )
            } else {
                (
                    if up {
                        KEYEVENTF_KEYUP | KEYEVENTF_SCANCODE
                    } else {
                        KEYEVENTF_SCANCODE
                    },
                    true,
                )
            };
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk as u16),
                        wScan: scan,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }
        };
        let mut inputs = Vec::new();
        if shift {
            inputs.push(kbd(VK_SHIFT, false));
        }
        if ctrl {
            inputs.push(kbd(VK_CONTROL, false));
        }
        for &vk in vks {
            inputs.push(kbd(vk, false));
            inputs.push(kbd(vk, true));
        }
        if ctrl {
            inputs.push(kbd(VK_CONTROL, true));
        }
        if shift {
            inputs.push(kbd(VK_SHIFT, true));
        }
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

// ---------------------------------------------------------------------------
// Vello rendering (debug-grade; throwaway once the real backend lands)
// ---------------------------------------------------------------------------

struct FontCache {
    files: std::collections::HashMap<String, std::sync::Arc<Vec<u8>>>,
    fonts: std::collections::HashMap<(String, u32), vello::peniko::FontData>,
}

struct VelloHost {
    device: wgpu::Device,
    queue: wgpu::Queue,
    ctx: vello::util::RenderContext,
    surface: vello::util::RenderSurface<'static>,
    renderer: vello::Renderer,
    scene: vello::Scene,
    fonts: FontCache,
    service: Rc<DWriteTextService>,
    style: TextStyle,
    width: u32,
    height: u32,
    env_log: Vec<String>,
}

impl VelloHost {
    fn draw(&mut self, client: (i32, i32), session: &EditingSession) {
        self.sync_size(client);

        use vello::kurbo::{Affine, Rect, Stroke};
        use vello::peniko::{Brush, Fill};

        let bg = vello::peniko::Color::from_rgba8(250, 250, 248, 255);
        let border = vello::peniko::Color::from_rgba8(180, 180, 180, 255);
        let sel = vello::peniko::Color::from_rgba8(59, 130, 246, 90);
        let text = vello::peniko::Color::from_rgba8(20, 20, 20, 255);
        let caret = vello::peniko::Color::from_rgba8(220, 60, 60, 255);
        let comp = vello::peniko::Color::from_rgba8(30, 130, 200, 255);

        self.scene.reset();
        self.scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(bg),
            None,
            &Rect::new(0.0, 0.0, self.width as f64, self.height as f64),
        );
        self.scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            &Brush::Solid(border),
            None,
            &Rect::new(
                (ORIGIN_X - 10.0) as f64,
                (FIELD_TOP - 10.0) as f64,
                FIELD_RIGHT as f64,
                (FIELD_BOTTOM + 10.0) as f64,
            ),
        );

        let composite = session.composite_text();
        let Ok(shaped) = self.service.shape(&composite, &self.style) else {
            return;
        };
        let metrics = shaped.single_line_metrics();

        // Selection highlight (behind the text).
        let (a, b) = session.selection();
        if a != b {
            let x0 = ORIGIN_X + shaped.caret_x(a);
            let x1 = ORIGIN_X + shaped.caret_x(b);
            self.scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &Brush::Solid(sel),
                None,
                &Rect::new(
                    x0 as f64,
                    (BASELINE_Y - metrics.ascent) as f64,
                    x1 as f64,
                    (BASELINE_Y + metrics.descent) as f64,
                ),
            );
        }

        // The text: one glyph run per shaped piece.
        for run in &shaped.runs {
            let Some(font_data) = self.font_of_run(run) else {
                continue;
            };
            let mut pen = shaped.pen_x_at(run.glyph_range.0);
            let glyphs = shaped.glyphs[run.glyph_range.0..run.glyph_range.1]
                .iter()
                .map(|g| {
                    let x = pen + g.x_offset;
                    let y = g.y_offset;
                    pen += g.x_advance;
                    vello::Glyph {
                        id: g.glyph_id,
                        x,
                        y,
                    }
                })
                .collect::<Vec<_>>();
            self.scene
                .draw_glyphs(&font_data)
                .transform(Affine::translate((ORIGIN_X as f64, BASELINE_Y as f64)))
                .font_size(self.style.em_size())
                .hint(false)
                .brush(Brush::Solid(text))
                .draw(Fill::NonZero, glyphs.into_iter());
        }

        // Composition underline.
        if let Some(start) = session.composition_start_byte() {
            let comp_len = session.composition_string().len();
            if comp_len > 0 {
                let x0 = ORIGIN_X + shaped.caret_x(start);
                let x1 = ORIGIN_X + shaped.caret_x(start + comp_len);
                let y = BASELINE_Y + metrics.descent + 2.0;
                self.scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    &Brush::Solid(comp),
                    None,
                    &Rect::new(x0 as f64, y as f64, x1 as f64, (y + 1.5) as f64),
                );
            }
        }

        // The caret.
        let caret_x = if composite.is_empty() {
            ORIGIN_X
        } else {
            ORIGIN_X + shaped.caret_x(session.composite_caret_byte())
        };
        self.scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(caret),
            None,
            &Rect::new(
                (caret_x - 1.0) as f64,
                (BASELINE_Y - metrics.ascent) as f64,
                (caret_x + 1.0) as f64,
                (BASELINE_Y + metrics.descent) as f64,
            ),
        );

        // Present.
        let frame = match self.surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                self.env_log
                    .push("get_current_texture: suboptimal (reconfigured)".to_string());
                f
            }
            other => {
                self.env_log
                    .push(format!("get_current_texture: {other:?} — reconfiguring"));
                self.ctx.configure_surface(&self.surface);
                return;
            }
        };
        let frame_view = frame.texture.create_view(&Default::default());
        if let Err(e) = self.renderer.render_to_texture(
            &self.device,
            &self.queue,
            &self.scene,
            &self.surface.target_view,
            &vello::RenderParams {
                base_color: bg,
                width: self.surface.config.width,
                height: self.surface.config.height,
                antialiasing_method: vello::AaConfig::Area,
            },
        ) {
            self.env_log.push(format!("render_to_texture: {e}"));
            return;
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.surface.blitter.copy(
            &self.device,
            &mut encoder,
            &self.surface.target_view,
            &frame_view,
        );
        self.queue.submit([encoder.finish()]);
        frame.present();
    }

    fn font_of_run(&mut self, run: &oppa::text::TextRun) -> Option<vello::peniko::FontData> {
        let (path, index) = self.service.font_file_source(run.font_id)?;
        let key = (path.clone(), index);
        if !self.fonts.fonts.contains_key(&key) {
            let bytes = match self.fonts.files.get(&path) {
                Some(b) => b.clone(),
                None => {
                    let read = std::fs::read(&path).ok()?;
                    let arc = std::sync::Arc::new(read);
                    self.fonts.files.insert(path.clone(), arc.clone());
                    arc
                }
            };
            let blob = vello::peniko::Blob::new(bytes);
            self.fonts
                .fonts
                .insert(key.clone(), vello::peniko::FontData::new(blob, index));
        }
        self.fonts.fonts.get(&key).cloned()
    }

    /// The swapchain size tracks the client rect (WM_SIZE); reconfigure
    /// when it changed (resize = not crashing, nothing more).
    fn sync_size(&mut self, client: (i32, i32)) {
        let w = client.0.max(1) as u32;
        let h = client.1 as u32;
        if w != self.width || h != self.height {
            self.ctx.resize_surface(&mut self.surface, w, h);
            self.width = w;
            self.height = h;
        }
    }
}

// ---------------------------------------------------------------------------
// The app
// ---------------------------------------------------------------------------

fn app(automated: bool, wait_secs: u64) -> Result<(), String> {
    unsafe {
        let _ = windows::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_UNAWARE,
        );
    }

    let rt = Runtime::new();
    let service =
        Rc::new(DWriteTextService::new().map_err(|e| format!("DWriteTextService::new: {e}"))?);
    let mut style = TextStyle::new("Segoe UI", 16.0);
    style.device_pixel_ratio = 1.0;
    let ime_ops: Rc<RefCell<Vec<ImeOps>>> = Rc::new(RefCell::new(Vec::new()));
    let session = Rc::new(RefCell::new(EditingSession::new(
        rt.clone(),
        service.clone(),
        style.clone(),
        "Hello world".to_string(),
        ime_ops.clone(),
    )));

    let cfg = ShellConfig {
        title: "oppa — one editable field (IME pass vehicle)".to_string(),
        width: 640,
        height: 200,
        record_messages: automated,
        suppress_os_composition_window: true,
        visible: true,
    };
    let shell = Rc::new(RefCell::new(Win32Shell::new(cfg)?));

    // The swallow counter, shared between the IME mapper (sets it on
    // commit) and the field handler (consumes it on WM_CHAR).
    let swallow = Rc::new(Cell::new(0usize));

    // The M0 registry handler: the field's op mapping (real events, in
    // event order; one command per pumped event).
    {
        let session = session.clone();
        let shell = shell.clone();
        let swallow = swallow.clone();
        // Last discrete pointer x (field-relative) for the
        // PointerMove→drag_x fold below (Round 7.19: the shell feeds
        // discrete Down/Move/Up now — a Move carries one point, the
        // session's drag takes a from/to pair, so the previous point
        // rides here; Up/Cancel clear it).
        let last = Rc::new(Cell::new(None::<f32>));
        rt.register_handler(FIELD_EVENT, move || {
            let cmds = shell.borrow_mut().take_cmds();
            for cmd in cmds {
                let mut s = session.borrow_mut();
                match cmd {
                    Cmd::Click {
                        dbl_click,
                        shift,
                        x,
                        y: _,
                    } => {
                        let fx = x as f32 - ORIGIN_X;
                        if dbl_click {
                            s.dbl_click_x(fx);
                        } else if shift {
                            s.shift_click_x(fx);
                        } else {
                            s.click_x(fx);
                        }
                    }
                    Cmd::Drag { from_x, to_x } => {
                        s.drag_x(from_x - ORIGIN_X, to_x - ORIGIN_X);
                    }
                    // Discrete pointer feed (Round 7.19, decision
                    // 294): Down parks the caret like a plain click,
                    // Moves fold through the last point into drag_x,
                    // Up/Cancel close the gesture. Double/shift clicks
                    // no longer arrive discretely (the shell maps every
                    // press to PointerDown — stated, not silent; the
                    // legacy Click arm above keeps serving them if
                    // constructed).
                    Cmd::PointerDown { x, .. } => {
                        let fx = x - ORIGIN_X;
                        s.click_x(fx);
                        last.set(Some(fx));
                    }
                    Cmd::PointerMove { x, y: _ } => {
                        let fx = x - ORIGIN_X;
                        if let Some(prev) = last.get() {
                            s.drag_x(prev, fx);
                        }
                        last.set(Some(fx));
                    }
                    Cmd::PointerUp { .. } => {
                        last.set(None);
                    }
                    Cmd::PointerCancel => {
                        last.set(None);
                    }
                    Cmd::Key { vk, shift, ctrl } => match (vk, shift, ctrl) {
                        (VK_LEFT, true, _) => s.extend_caret(-1),
                        (VK_LEFT, _, _) => s.caret_move(-1),
                        (VK_RIGHT, true, _) => s.extend_caret(1),
                        (VK_RIGHT, _, _) => s.caret_move(1),
                        (VK_HOME, true, _) => s.extend_caret(-99),
                        (VK_HOME, _, _) => s.caret_to_start(),
                        (VK_END, true, _) => s.extend_caret(99),
                        (VK_END, _, _) => s.caret_to_end(),
                        (VK_A, _, true) => s.select_all(),
                        (VK_Z, _, true) => s.undo(),
                        _ => {}
                    },
                    Cmd::Char { ch } => {
                        // Control chars (Ctrl+Z's 0x1A, ESC's 0x1B, Enter's
                        // 0x0D...) are not text — they are the keyboard
                        // shortcut's WM_CHAR shadow; skip them.
                        if (ch as u32) < 0x20 {
                            continue;
                        }
                        let pending = swallow.get();
                        let units = ch.len_utf16();
                        if pending >= units {
                            swallow.set(pending - units);
                        } else {
                            swallow.set(0);
                            s.insert(&ch.to_string());
                        }
                    }
                    Cmd::FocusChanged(f) => {
                        // TSF focus follows the window (additive; the
                        // session ignores focus cmds as before).
                        shell.borrow_mut().tsf_note_focus(f);
                    }
                    // Spike rig only (decision 250 broke the
                    // exhaustive match — not architecture): one
                    // editable field, nothing scrollable; wheel
                    // ticks have no target here. DPI crossings
                    // likewise have no target (Round 2.4 adds a
                    // variant the single-field rig never drives).
                    // OS theme flips likewise have no target
                    // (Round 16.2 adds a variant the rig never
                    // drives — the product runner owns it). Close
                    // requests likewise have no target (Round 16.3
                    // adds a variant the rig never drives — the
                    // product runner owns the veto).
                    Cmd::Scroll { .. } => {}
                    Cmd::DpiChanged { .. } => {}
                    Cmd::SystemThemeChanged => {}
                    Cmd::CloseRequested => {}
                }
            }
        });
    }

    // The IME mapper sink: real OS IME message → normalized → session.
    {
        let mapper = Rc::new(RefCell::new(ImeMapper {
            session: session.clone(),
            comp_active: false,
            swallow_pending: 0,
        }));
        let sink_mapper = mapper.clone();
        shell.borrow_mut().set_ime_callback(Rc::new(move |msg| {
            sink_mapper.borrow_mut().handle(msg);
        }));
    }

    // The runtime's shell: forward to the real Win32 shell.
    struct Forward(Rc<RefCell<Win32Shell>>);
    impl PlatformShell for Forward {
        fn pump_events(&mut self) -> Vec<Event> {
            self.0.borrow_mut().pump_events()
        }
        fn set_ime(&mut self, ops: ImeOps) {
            self.0.borrow_mut().set_ime(ops);
        }
    }
    rt.set_shell(Box::new(Forward(shell.clone())));

    // Arm the installed zh-Hans-CN Microsoft Pinyin IME (the pass runs
    // against the REAL OS IME - never a silent no-op).
    let env_log = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    arm_ime(&shell.borrow(), &env_log)?;

    // TSF-aware association (shell crate): thread manager activation +
    // document manager backed by the real text store, associated with the
    // window + IS_TEXT input scope, seeded with the settled field state.
    // Failures are recorded, never silent: the pass still runs so the
    // message stream shows exactly what engaged.
    let init = session.borrow().observable();
    match shell.borrow_mut().enable_tsf(&init.content, init.sel) {
        Ok(lines) => {
            for l in lines {
                env_log.borrow_mut().push(l);
            }
        }
        Err((e, lines)) => {
            for l in lines {
                env_log.borrow_mut().push(l);
            }
            env_log
                .borrow_mut()
                .push(format!("tsf: enable_tsf FAILED: {e}"));
        }
    }
    {
        let tsf = shell.borrow().tsf_status();
        env_log
            .borrow_mut()
            .push(format!("tsf: status after enable: {tsf:?}"));
    }

    // Vello/wgpu setup.
    let mut vello = init_vello(
        shell.borrow().hwnd(),
        service.clone(),
        style.clone(),
        &env_log,
    )?;

    // MS Pinyin's per-document EN/CH mode is initialized at focus attach
    // (the vello/wgpu setup may re-trigger it); the app cannot set it from
    // the IMM side (the set calls report success and the mode still reads
    // alphanumeric) — recorded as this round's finding. The Shift tap is
    // NOT used: the injected Shift produced a stray character into the
    // field's content, not an IME mode flip. The engagement remains open.
    {
        let pre = shell.borrow().ime_status();
        env_log.borrow_mut().push(format!(
            "pre-engage: open={} conv={:#x} sent={:#x} hkl={:#x}",
            pre.context_open, pre.conversion_mode, pre.sentence_mode, pre.active_hkl
        ));
    }

    // The automated pass.
    let mut pass_log: Vec<PassStep> = Vec::new();
    let mut driver = automated.then(Driver::new);

    // Focus window (additive; not part of the pass): let a human bring
    // the window forward before the first step injects keys. Messages
    // keep pumping so the window stays alive; the foreground state at
    // the end is recorded for the report.
    if automated && wait_secs > 0 {
        println!("FOCUS THE TEST WINDOW NOW — pass starts in {wait_secs}s");
        for _ in 0..wait_secs * 10 {
            let _ = shell.borrow_mut().process_os_messages();
            std::thread::sleep(Duration::from_millis(100));
        }
        env_log.borrow_mut().push(format!(
            "focus wait: {wait_secs}s elapsed, foreground={}",
            shell.borrow().is_foreground()
        ));
    }

    loop {
        let quit = shell.borrow_mut().process_os_messages();

        // Drain the session's anchor ops → the real anchoring calls
        // (run-relative device px → client px).
        {
            let ops: Vec<ImeOps> = ime_ops.borrow_mut().drain(..).collect();
            for op in ops {
                let client = match op {
                    ImeOps::SetCaretRect {
                        x,
                        y,
                        width,
                        height,
                    } => ImeOps::SetCaretRect {
                        x: x + ORIGIN_X,
                        y: y + BASELINE_Y,
                        width,
                        height,
                    },
                    other => other,
                };
                shell.borrow_mut().set_ime(client);
            }
        }

        // Drive the pass (each step: send → settle → record).
        if let Some(d) = driver.as_mut() {
            d.tick(&shell, &session, &rt, &env_log, &mut pass_log);
            if d.finished {
                let verdict = pass_verdict(&pass_log);
                let msg_log = shell.borrow_mut().take_message_log();
                let ime_log = shell.borrow_mut().take_ime_log();
                let json = pass_json(&verdict, &pass_log, &vello.env_log, &msg_log, &ime_log);
                let _ = std::fs::write("spike/results/ime_manual.json", json);
                if verdict.status == "PASS" {
                    return Ok(());
                }
                return Err(verdict.status);
            }
        }

        // One session frame in interactive mode (the pass settles its own).
        if driver.is_none() {
            rt.request_frame();
            let _ = rt.run_once();
            if let Some(rect) = session.borrow().caret_rect() {
                ime_ops.borrow_mut().push(ImeOps::SetCaretRect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                });
            }
        }

        // Render (debug-grade Vello).
        vello.draw(shell.borrow().client_size(), &session.borrow());

        if quit {
            return Ok(());
        }
    }
}
// ---------------------------------------------------------------------------
// Vello/wgpu setup
// ---------------------------------------------------------------------------

fn init_vello(
    hwnd: windows::Win32::Foundation::HWND,
    service: Rc<DWriteTextService>,
    style: TextStyle,
    env_log: &std::rc::Rc<std::cell::RefCell<Vec<String>>>,
) -> Result<VelloHost, String> {
    let (width, height) = (640u32, 200u32);
    let mut ctx = vello::util::RenderContext::new();

    let mut window_handle =
        Win32WindowHandle::new(std::num::NonZeroIsize::new(hwnd.0 as isize).ok_or("null hwnd")?);
    window_handle.hinstance = None;
    let target = SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: Some(RawDisplayHandle::Windows(WindowsDisplayHandle::new())),
        raw_window_handle: RawWindowHandle::Win32(window_handle),
    };
    let surface = unsafe { ctx.instance.create_surface_unsafe(target) }
        .map_err(|e| format!("create_surface_unsafe: {e}"))?;
    let surface = pollster::block_on(ctx.create_render_surface(
        surface,
        width,
        height,
        wgpu::PresentMode::Fifo,
    ))
    .map_err(|e| format!("create_render_surface: {e}"))?;

    let dev_id = surface.dev_id;
    let device = ctx.devices[dev_id].device.clone();
    let queue = ctx.devices[dev_id].queue.clone();
    let renderer = vello::Renderer::new(&device, vello::RendererOptions::default())
        .map_err(|e| format!("vello Renderer::new: {e}"))?;

    env_log.borrow_mut().push(format!(
        "vello host: {}x{} on the window's swapchain",
        width, height
    ));

    Ok(VelloHost {
        device,
        queue,
        ctx,
        surface,
        renderer,
        scene: vello::Scene::new(),
        fonts: FontCache {
            files: std::collections::HashMap::new(),
            fonts: std::collections::HashMap::new(),
        },
        service,
        style,
        width,
        height,
        env_log: (*env_log.borrow()).clone(),
    })
}

// ---------------------------------------------------------------------------
// The pass verdict
// ---------------------------------------------------------------------------

struct Verdict {
    status: String,
    checks: Vec<(String, bool, String)>,
    divergences: Vec<String>,
}

fn pass_verdict(pass_log: &[PassStep]) -> Verdict {
    let mut checks: Vec<(String, bool, String)> = Vec::new();
    let mut divergences: Vec<String> = Vec::new();

    // c1: select-all landed (Ctrl+A over "Hello world").
    check_select_all(pass_log, &mut checks, &mut divergences);
    // c2: mid-composition (first pass, after the last letter): content
    // cleared (the selection deleted at composition start), the
    // composition buffered session-side, the caret inside it.
    check_mid_composition(pass_log, "r1: o", "r1", &mut checks, &mut divergences);
    // c3: commit replaces the composition.
    check_commit(pass_log, &mut checks, &mut divergences);
    // c4: the undo unit is atomic.
    check_undo(pass_log, &mut checks, &mut divergences);
    // c5: the second pass (composition over the restored selection).
    check_mid_composition(pass_log, "r2: o", "r2", &mut checks, &mut divergences);
    // c6: cancel mid-composition.
    check_cancel(pass_log, &mut checks, &mut divergences);

    let ok = divergences.is_empty();
    Verdict {
        status: if ok {
            "PASS".to_string()
        } else {
            "FAIL".to_string()
        },
        checks,
        divergences,
    }
}

fn find_step<'a>(pass_log: &'a [PassStep], marker: &str) -> Option<&'a SessionState> {
    pass_log
        .iter()
        .find(|s| s.marker == marker)
        .map(|s| &s.observable)
}

fn check_select_all(
    pass_log: &[PassStep],
    checks: &mut Vec<(String, bool, String)>,
    divergences: &mut Vec<String>,
) {
    let name = "c1 select-all (ctrl+a): sel == (0, 11)";
    match find_step(pass_log, "select-all (ctrl+a)") {
        Some(s) => {
            let ok = s.content == "Hello world" && s.sel == (0, 11);
            let detail = format!("content {:?} sel {:?}", s.content, s.sel);
            checks.push((name.to_string(), ok, detail.clone()));
            if !ok {
                divergences.push(format!("{name}: {detail}"));
            }
        }
        None => {
            checks.push((name.to_string(), false, "step never recorded".to_string()));
            divergences.push(format!("{name}: step never recorded"));
        }
    }
}

fn check_mid_composition(
    pass_log: &[PassStep],
    marker: &str,
    tag: &str,
    checks: &mut Vec<(String, bool, String)>,
    divergences: &mut Vec<String>,
) {
    let name = format!("{tag} mid-composition: content cleared, composition buffered session-side");
    match find_step(pass_log, marker) {
        Some(s) => {
            // Real Pinyin readings carry syllable-separator quotes
            // ("ni'hao", caret 6) where the authored shape assumed the
            // bare reading ("nihao", caret 5). The session mirrors the
            // TIP byte-exact (its contract), so the check normalizes
            // separators and asserts caret-at-reading-end instead of
            // hardcoding one TIP's formatting.
            let reading: String = s.composition.chars().filter(|&c| c != '\'').collect();
            let ok = s.content.is_empty()
                && reading == "nihao"
                && s.caret == s.composition.len()
                && s.sel == (s.caret, s.caret);
            let detail = format!(
                "content {:?} composition {:?} caret {}",
                s.content, s.composition, s.caret
            );
            checks.push((name.clone(), ok, detail.clone()));
            if !ok {
                divergences.push(format!("{name}: {detail}"));
            }
        }
        None => {
            checks.push((name.clone(), false, "step never recorded".to_string()));
            divergences.push(format!("{name}: step never recorded"));
        }
    }
}

fn check_commit(
    pass_log: &[PassStep],
    checks: &mut Vec<(String, bool, String)>,
    divergences: &mut Vec<String>,
) {
    let name = "c3 commit: selection replaced by the committed text";
    match find_step(pass_log, "commit (space)") {
        Some(s) => {
            let ok = s.content == "你好"
                && s.composition.is_empty()
                && s.caret == "你好".len()
                && s.sel == (s.caret, s.caret);
            let detail = format!(
                "content {:?} caret {} composition {:?}",
                s.content, s.caret, s.composition
            );
            checks.push((name.to_string(), ok, detail.clone()));
            if !ok {
                divergences.push(format!("{name}: {detail}"));
            }
        }
        None => {
            checks.push((name.to_string(), false, "step never recorded".to_string()));
            divergences.push(format!("{name}: step never recorded"));
        }
    }
}

fn check_undo(
    pass_log: &[PassStep],
    checks: &mut Vec<(String, bool, String)>,
    divergences: &mut Vec<String>,
) {
    let name = "c4 undo: atomic unit restores the pre-composition state";
    match find_step(pass_log, "undo (ctrl+z)") {
        Some(s) => {
            // The pre-composition state IS the select-all state (c1):
            // content "Hello world", caret 11 (select_all's own
            // contract: caret = end), sel (0, 11). An earlier revision
            // demanded caret == 0 here — a state that never existed in
            // the pass (pre-composition caret was 11) and contradicts
            // select_all; corrected, not relaxed.
            let ok = s.content == "Hello world" && s.caret == 11 && s.sel == (0, 11);
            let detail = format!("content {:?} caret {} sel {:?}", s.content, s.caret, s.sel);
            checks.push((name.to_string(), ok, detail.clone()));
            if !ok {
                divergences.push(format!("{name}: {detail}"));
            }
        }
        None => {
            checks.push((name.to_string(), false, "step never recorded".to_string()));
            divergences.push(format!("{name}: step never recorded"));
        }
    }
}

fn check_cancel(
    pass_log: &[PassStep],
    checks: &mut Vec<(String, bool, String)>,
    divergences: &mut Vec<String>,
) {
    let name = "c6 cancel: composition dropped, delete stays";
    match find_step(pass_log, "cancel (esc)") {
        Some(s) => {
            let ok = s.content.is_empty() && s.composition.is_empty();
            let detail = format!("content {:?} composition {:?}", s.content, s.composition);
            checks.push((name.to_string(), ok, detail.clone()));
            if !ok {
                divergences.push(format!("{name}: {detail}"));
            }
        }
        None => {
            checks.push((name.to_string(), false, "step never recorded".to_string()));
            divergences.push(format!("{name}: step never recorded"));
        }
    }
}

fn observable_json(state: &SessionState) -> J {
    J::o(vec![
        ("content".to_string(), J::s(state.content.clone())),
        ("caret".to_string(), J::n(state.caret as f64)),
        (
            "sel".to_string(),
            J::a(vec![J::n(state.sel.0 as f64), J::n(state.sel.1 as f64)]),
        ),
        ("composition".to_string(), J::s(state.composition.clone())),
    ])
}

fn pass_json(
    verdict: &Verdict,
    pass_log: &[PassStep],
    env_log: &[String],
    msg_log: &[MessageRecord],
    ime_log: &[String],
) -> String {
    let steps = pass_log
        .iter()
        .map(|s| {
            J::o(vec![
                ("marker".to_string(), J::s(s.marker.clone())),
                ("observable".to_string(), observable_json(&s.observable)),
                (
                    "anchored".to_string(),
                    J::a(
                        s.anchored
                            .iter()
                            .map(|r| J::a(r.iter().map(|v| J::n(*v as f64)).collect()))
                            .collect(),
                    ),
                ),
                (
                    "ime".to_string(),
                    J::o(vec![
                        ("open".to_string(), J::b(s.ime.context_open)),
                        ("conv".to_string(), J::n(s.ime.conversion_mode as f64)),
                        ("sent".to_string(), J::n(s.ime.sentence_mode as f64)),
                        ("hkl".to_string(), J::n(s.ime.active_hkl as f64)),
                    ]),
                ),
                ("foreground".to_string(), J::b(s.foreground)),
                (
                    "notes".to_string(),
                    J::a(s.notes.iter().map(J::s).collect()),
                ),
            ])
        })
        .collect();
    let messages = msg_log
        .iter()
        .map(|m| {
            J::o(vec![
                ("seq".to_string(), J::n(m.seq as f64)),
                ("msg".to_string(), J::n(m.msg as f64)),
                ("wparam".to_string(), J::n(m.wparam as f64)),
                ("lparam".to_string(), J::n(m.lparam as f64)),
            ])
        })
        .collect();
    let ime = ime_log.iter().map(J::s).collect();
    J::o(vec![
        ("status".to_string(), J::s(verdict.status.clone())),
        (
            "checks".to_string(),
            J::a(
                verdict
                    .checks
                    .iter()
                    .map(|(n, ok, d)| {
                        J::o(vec![
                            ("name".to_string(), J::s(n.clone())),
                            ("ok".to_string(), J::b(*ok)),
                            ("detail".to_string(), J::s(d.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "divergences".to_string(),
            J::a(verdict.divergences.iter().map(J::s).collect()),
        ),
        ("steps".to_string(), J::a(steps)),
        (
            "environment".to_string(),
            J::a(env_log.iter().map(J::s).collect()),
        ),
        ("win32_messages".to_string(), J::a(messages)),
        ("ime_events".to_string(), J::a(ime)),
    ])
    .render()
}
