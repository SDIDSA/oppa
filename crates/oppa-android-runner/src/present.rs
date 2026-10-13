//! Interactive present loop for any root: lifecycle + swapchain
//! generations + event-paced presents.
//!
//! Ported shapes from the proof harness
//! (`crates/oppa-android-app/src/surface.rs`, decisions 396/428)
//! minus everything proof-only (benchmark uncapping, scripted taps,
//! oracle asserts, pick registry, record files): the loop pumps
//! touch input, drains the IME queue, maps shell IME policy to the
//! keyboard (plus tap re-show inside the focused field — system
//! dismissals leave focus set but take the keyboard, which the
//! change-only policy cannot see), repaints on change, and presents
//! through the engine's live GPU surface id. Window generations
//! re-acquire per pass (Stop/Resume recreates the window at the
//! same size, new pointer); a missing window parks instead of
//! spinning or failing; in-generation present failures stay loud
//! `Err`s.

use android_activity::{ndk::native_window::NativeWindow, AndroidApp};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

use oppa::shell::PlatformShell;
use oppa_app::SceneCore;
use oppa_shell_android::events::AndroidCmd;
use oppa_shell_android::input_controller::{AndroidInput, ToolbarOp};
use oppa_shell_android::selection_menu::{byte_to_utf16_units, MenuAction, MenuSnapshot, SelCause};

use crate::ime_bridge;
use crate::imm_mode;
use crate::soft_present;
use crate::touch::TouchDriver;

/// Window + display pair: `NativeWindow` carries only the window
/// half, wgpu surface creation needs both (the display half on
/// Android is a unit value).
struct AndroidWindow<'a> {
    window: &'a NativeWindow,
}

impl<'a> HasWindowHandle for AndroidWindow<'a> {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.window.window_handle()
    }
}

impl<'a> HasDisplayHandle for AndroidWindow<'a> {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::android())
    }
}

/// Owned Android display half for the wgpu instance descriptor
/// (GLES requires the platform display at instance creation when
/// presenting; the value is a unit — owned so it can live in the
/// descriptor).
#[derive(Debug, Clone, Copy)]
struct AndroidDisplay;

impl HasDisplayHandle for AndroidDisplay {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::android())
    }
}

/// Probe-surface factory over the instance table (decision 432):
/// one `wgpu` instance per backend row, created once and reused
/// across window generations (the adapter/device re-ensure still
/// runs per generation — the old device was bound to the old
/// surface). Instances are box-leaked on first creation (the
/// desktop leak precedent — the table never shrank anyway, so the
/// process-lifetime set is identical; `'static` refs keep the
/// returned surface independent of table reallocs, which a plain
/// vec-element borrow cannot prove). The window wrapper stays
/// caller-owned: the factory only copies its `&` out (shared refs
/// are `Copy`), so the surface never borrows factory-built locals.
/// GLES requires the platform display handle at instance creation
/// when presenting (wgpu contract); Vulkan ignores it. The Android
/// display half is a unit value, owned so it can live in the
/// descriptor.
struct AndroidSurfaceFactory<'w> {
    instances: &'w mut Vec<(wgpu::Backends, &'static wgpu::Instance)>,
    wrap: &'w AndroidWindow<'w>,
}

impl<'x> oppa_app::SurfaceFactory<'x> for AndroidSurfaceFactory<'x> {
    fn make_surface(
        &mut self,
        backends: wgpu::Backends,
    ) -> Result<oppa_app::BuiltSurface<'x>, String> {
        if self.instances.iter().all(|(b, _)| *b != backends) {
            let mut desc =
                wgpu::InstanceDescriptor::new_with_display_handle(Box::new(AndroidDisplay));
            desc.backends = backends;
            let leaked: &'static wgpu::Instance = Box::leak(Box::new(wgpu::Instance::new(desc)));
            self.instances.push((backends, leaked));
        }
        let instance: &'x wgpu::Instance = self
            .instances
            .iter()
            .find(|(b, _)| *b == backends)
            .map(|(_, instance)| *instance)
            .expect("instance row recorded above");
        let surface = instance
            .create_surface(self.wrap)
            .map_err(|e| format!("{e:?}"))?;
        Ok(oppa_app::BuiltSurface { instance, surface })
    }
}

/// One lifecycle poll: folds Pause/Stop into the persistent `paused`
/// flag and returns the edge flags the loop acts on once. `heal`
/// forces a full swapchain rebuild (return paths —
/// Resume/Start/Terminate); `redraw` only asks for a repaint +
/// present on the live surface (same-size WindowResized /
/// ContentRectChanged / RedrawNeeded / InitWindow edges — the
/// emulator repeats WindowResized ~1 Hz with no size change, so
/// rebuilding on it thrashed a ~700 ms re-ensure per pass).
struct PollOutcome {
    destroyed: bool,
    heal: bool,
    redraw: bool,
}

fn poll_once(app: &AndroidApp, timeout: std::time::Duration, paused: &mut bool) -> PollOutcome {
    use android_activity::{MainEvent, PollEvent};
    let mut out = PollOutcome {
        destroyed: false,
        heal: false,
        redraw: false,
    };
    app.poll_events(Some(timeout), |event| match event {
        PollEvent::Main(MainEvent::Destroy) => {
            out.destroyed = true;
        }
        PollEvent::Main(MainEvent::Pause) | PollEvent::Main(MainEvent::Stop) => {
            *paused = true;
        }
        PollEvent::Main(MainEvent::Resume { .. }) | PollEvent::Main(MainEvent::Start) => {
            *paused = false;
            out.heal = true;
        }
        PollEvent::Main(MainEvent::TerminateWindow { .. }) => {
            out.heal = true;
        }
        PollEvent::Main(MainEvent::InitWindow { .. })
        | PollEvent::Main(MainEvent::WindowResized { .. })
        | PollEvent::Main(MainEvent::RedrawNeeded { .. })
        | PollEvent::Main(MainEvent::ContentRectChanged { .. }) => {
            out.redraw = true;
        }
        _ => {}
    });
    out
}

/// Liveness beat (logcat only — proves the loop iterates, never
/// that frames present).
fn maybe_beat(
    t_open: &std::time::Instant,
    last_beat: &mut std::time::Instant,
    presents: u32,
    heals: u32,
    paused: bool,
    window: bool,
) {
    use std::time::Duration;
    if last_beat.elapsed() > Duration::from_secs(10) {
        eprintln!(
            "oppa-loop: alive +{:.0}s presents={presents} heals={heals} paused={paused} window={window}",
            t_open.elapsed().as_secs_f64()
        );
        *last_beat = std::time::Instant::now();
    }
}

/// The live presenter for one window generation (decision 433):
/// the ladder's GPU win (swapchain surface + record strings) or
/// the terminal CPU row (no surface, no GPU state — frames post
/// the engine `rgba8` pixmap through the native-window lock).
enum LivePresenter<'a> {
    Gpu {
        surface: wgpu::Surface<'a>,
        adapter: String,
    },
    Cpu,
}

/// Waits for the live `NativeWindow` (up to 30s), requests immersive
/// mode up front, and returns the window with settled dimensions
/// (three agreeing samples or 5s — a growing window is followed to
/// rest, never frozen pre-immersive).
pub fn wait_for_window(
    app: &AndroidApp,
    java_package: &str,
) -> Result<(NativeWindow, u32, u32), String> {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(30);
    let window = loop {
        if let Some(w) = app.native_window() {
            break w;
        }
        if Instant::now() > deadline {
            return Err("no native window within 30s".to_string());
        }
        app.poll_events(Some(Duration::from_millis(200)), |_| {});
    };
    // Immersive request BEFORE measuring: a decor view that is not
    // attached yet swallows the flags (success, no effect), so the
    // loop below re-asserts — the size afterward is the only truth.
    let imm = imm_mode::hide_system_bars(app, java_package)
        .map(|s| format!("{s} "))
        .unwrap_or_else(|e| format!("immersive refused [{e}] "));
    let settle_end = Instant::now() + Duration::from_secs(5);
    let mut last = (0u32, 0u32);
    let mut steady = 0u32;
    let (ww, wh) = loop {
        let (cw, ch) = (window.width().max(1) as u32, window.height().max(1) as u32);
        if (cw, ch) == last {
            steady += 1;
        } else {
            last = (cw, ch);
            steady = 0;
        }
        if steady >= 3 || Instant::now() > settle_end {
            break (cw, ch);
        }
        app.poll_events(Some(Duration::from_millis(200)), |_| {});
    };
    eprintln!("oppa-phase: window {ww}x{wh} ({imm}settled)");
    Ok((window, ww, wh))
}

/// Feeds one input batch through the engine (the proof tap-batch
/// core minus taps scripting, oracle asserts, and record files).
/// Returns true when any work happened — the caller owns the single
/// repaint (profiling showed this helper's old internal repaint ran
/// back-to-back with the caller's, doubling tap-path paint cost).
fn apply_input_batch(
    scene: &mut SceneCore,
    cmds: &[oppa_shell_android::events::AndroidCmd],
    ime_drained: usize,
    ime_policy: usize,
    clipboard: &mut dyn oppa::Clipboard,
    android_input: &AndroidInput,
) -> Result<bool, String> {
    let mut work = ime_drained > 0 || ime_policy > 0;
    for cmd in cmds.iter().cloned() {
        match cmd {
            // Commits replace the open composition or insert cold
            // (decision 438 — the desktop `feed_ime_commit` shape, not
            // a raw insert: `insert` is a documented no-op while
            // composing, so committing mid-composition through it
            // silently drops the word). Empty commits are quiet no-ops;
            // quiet miss with no focused field (the router precedent
            // for unhandled keys — a commit to nowhere is never the
            // old `to_input_event` panic).
            AndroidCmd::CommitText { text } => {
                if !text.is_empty() {
                    if let Some(mut session) = scene.host().focused_field_session() {
                        oppa::dispatch_ime_event(
                            &mut session,
                            &oppa::ImeCompositionEvent::CompositionCommitted {
                                committed: text.clone(),
                            },
                        );
                        work = true;
                        eprintln!("oppa-ime: commit len={} committed", text.len());
                    } else {
                        eprintln!("oppa-ime: commit quiet-miss (no focused session)");
                    }
                }
            }
            AndroidCmd::DeleteSurrounding {
                before_chars,
                after_chars,
            } => {
                if let Some(session) = scene.host().focused_field_session() {
                    for _ in 0..before_chars {
                        session.backspace();
                    }
                    for _ in 0..after_chars {
                        session.delete_forward();
                    }
                    work = true;
                    eprintln!("oppa-ime: delete {before_chars},{after_chars} applied");
                } else {
                    eprintln!("oppa-ime: delete quiet-miss (no focused session)");
                }
            }
            // Live composition (decision 438): the `setComposingText`
            // feed — same event shape as the desktop `feed_ime_preedit`
            // (open-or-replace anchored at the selection, caret at the
            // end of the text — the Android `newCursorPosition` units
            // don't map 1:1 onto byte carets, so mid-composition
            // placement stays a stated simplification). Empty text is
            // a no-op that leaves an open composition open (the
            // `feed_ime_preedit` rule); quiet miss with no focused
            // field (the commit precedent).
            AndroidCmd::ComposeText { text } => {
                if !text.is_empty() {
                    if let Some(mut session) = scene.host().focused_field_session() {
                        let sel = session.selection();
                        let mut events = Vec::new();
                        if !session.is_composing() {
                            events.push(oppa::ImeCompositionEvent::CompositionStarted {
                                start_byte: sel.0,
                            });
                            if sel.0 != sel.1 {
                                events.push(oppa::ImeCompositionEvent::DeleteRange { range: sel });
                            }
                        }
                        let start = session.composition_start_byte().unwrap_or(sel.0);
                        events.push(oppa::ImeCompositionEvent::CompositionUpdated {
                            composition: text.clone(),
                            caret_byte: start + text.len(),
                        });
                        for ev in &events {
                            oppa::dispatch_ime_event(&mut session, ev);
                        }
                        work = true;
                        eprintln!(
                            "oppa-ime: compose len={} applied (composing={})",
                            text.len(),
                            session.is_composing()
                        );
                    } else {
                        eprintln!("oppa-ime: compose quiet-miss (no focused session)");
                    }
                }
            }
            // Finish-composing (decision 461): `finishComposingText`
            // commits the open buffer with its own text. Without this
            // a bare commit arriving right after (the Gboard space
            // flow: finish + commit " ") replaces the word with the
            // suffix alone — "ok" became " " on device. Quiet no-op
            // with no open composition or no focused field.
            AndroidCmd::FinishComposing => {
                if let Some(mut session) = scene.host().focused_field_session() {
                    if session.is_composing() {
                        let buffer = session.composition_text();
                        oppa::dispatch_ime_event(
                            &mut session,
                            &oppa::ImeCompositionEvent::CompositionCommitted {
                                committed: buffer.clone(),
                            },
                        );
                        work = true;
                        eprintln!("oppa-ime: finish len={} committed", buffer.len());
                    } else {
                        eprintln!("oppa-ime: finish quiet (no open composition)");
                    }
                } else {
                    eprintln!("oppa-ime: finish quiet-miss (no focused session)");
                }
            }
            // Hardware keys (decision 243 + desktop step parity —
            // runners route them to the focused session while the
            // router stays quiet): Backspace/Delete edit, arrows move
            // (Up/Down multiline-only, mirroring step_session_vline —
            // single-line fields keep router behavior). Decision 442
            // adds the modified chords (desktop `run_edit_shortcut` /
            // `step_session_nav` parity): Ctrl+A select-all, Ctrl+Z/Y
            // undo/redo, Shift/Ctrl+arrows and Home/End extend/move by
            // char/word/to-ends. Decision 443 closes the 442 clipboard
            // gap: Ctrl+C/X/V ride the loop clipboard (system
            // `ClipboardManager`, session-local fallback when the
            // service is missing) under the desktop loud-stderr-
            // never-fatal backend policy. Decision 452 routes the whole
            // matrix through the shared `action_for_key` /
            // `apply_edit_action` funnel (byte-identical with desktop
            // by construction). No Alt/Meta (AltGr guard, the desktop
            // step precedent). Press only (release is not a second
            // edit); quiet miss with no focused field; unmapped keys
            // flow to the router below.
            AndroidCmd::Key {
                code,
                pressed,
                modifiers,
            } if pressed => {
                // Hardware-key trace (decision 459 — one line per
                // press with focus/mapping fate, so a dead physical
                // keyboard diagnoses from logcat: quiet-miss means no
                // focused session, router means unmapped, failed means
                // the backend refused).
                let masked = scene
                    .host()
                    .focused_field_session()
                    .is_some_and(|s| s.is_masked());
                if let Some(mut session) = scene.host().focused_field_session() {
                    if modifiers.alt || modifiers.meta {
                        eprintln!("oppa-key: code={code} alt/meta → router (masked={masked})");
                        scene.host().inject_input(cmd.to_input_event());
                        work = true;
                    } else if let Some(action) = oppa::action_for_key(
                        code,
                        modifiers.shift,
                        modifiers.ctrl,
                        modifiers.alt,
                        modifiers.meta,
                    ) {
                        use oppa::input::actions::EditAction;
                        let ok = oppa::apply_edit_action(&session, action, &mut *clipboard);
                        eprintln!(
                            "oppa-key: code={code} action={action:?} ok={ok} (masked={masked})"
                        );
                        if !ok {
                            match action {
                                // Single-line Up/Down never consume:
                                // directional handlers keep their
                                // owners (the step_session_vline rule).
                                EditAction::LineUp
                                | EditAction::LineDown
                                | EditAction::ExtendLineUp
                                | EditAction::ExtendLineDown => {
                                    scene.host().inject_input(cmd.to_input_event());
                                }
                                // Clipboard backend failures log loudly
                                // and consume (never fatal, never
                                // router-forwarded as phantom keys).
                                EditAction::Copy | EditAction::Cut | EditAction::Paste => {
                                    eprintln!("oppa-menu: clipboard chord failed: {action:?}");
                                }
                                _ => {}
                            }
                        }
                        work = true;
                    } else {
                        // No action mapping: printable hardware/adb keys
                        // no IME translates commit through the soft funnel
                        // (decision 460 — the password-field silence was
                        // Gboard translating plain fields into
                        // `commitText` while refusing the password
                        // variation, with the proxy `onKeyDown` unicode
                        // path never firing for these keys; decision 461
                        // entry-probe). Ctrl-held or
                        // unmappable codes keep router-forwarding; a
                        // native-queue sighting means no commit exists yet
                        // (Gboard consumes the keys it translates), so this
                        // never double-delivers.
                        let text = (!modifiers.ctrl)
                            .then(|| oppa::text_for_hardware_key(code, modifiers.shift))
                            .flatten();
                        if let Some(ch) = text {
                            android_input.apply_hardware_text(&mut session, ch);
                            eprintln!("oppa-key: code={code} text-commit '{ch}' (masked={masked})");
                            work = true;
                        } else {
                            eprintln!("oppa-key: code={code} unmapped → router (masked={masked})");
                            scene.host().inject_input(cmd.to_input_event());
                            work = true;
                        }
                    }
                } else {
                    eprintln!("oppa-key: code={code} quiet-miss (no focused session)");
                    scene.host().inject_input(cmd.to_input_event());
                }
            }
            _ => {
                work = true;
                scene.host().inject_input(cmd.to_input_event());
            }
        }
    }
    if !work {
        return Ok(false);
    }
    scene.host().run_until_idle();
    Ok(true)
}

/// Whether the focused field backs masked (password) content
/// (decision 444): the proxy keyboard's input variation follows
/// this — password fields keep the IME incognito, plain fields do
/// not. `false` with no focused session (the show path only fires
/// on editable focus, so this is the unfocused-tap edge).
fn focused_password(scene: &SceneCore) -> bool {
    scene
        .host()
        .focused_field_session()
        .is_some_and(|session| session.is_masked())
}

/// The toolbar mirror frame for this pump, if any (decision 443):
/// the focused session's committed text + selection (in Java string
/// units) + the focused field's committed box as the floating-mode
/// anchor. `None` covers everything unshowable — no focused session,
/// focus outside a field, or no laid box yet (quiet miss, advisory
/// geometry like the caret paint — never a wiring bug). Collapsed
/// selections show only with an armed collapsed-hold request
/// (decision 459 — the hold-to-paste path needs no selection).
/// `cause` names what drove the selection in (decision 451 — the
/// toolbar raises for touch-driven selections only).
fn menu_snapshot_for(
    scene: &SceneCore,
    cause: SelCause,
    caret_toolbar: bool,
) -> Option<MenuSnapshot> {
    let session = scene.host().focused_field_session()?;
    let (a, b) = session.selection();
    let ranged = a != b;
    if !ranged && !caret_toolbar {
        return None;
    }
    let focus = scene.host().focused_node()?;
    let laid = scene.host().committed_box(focus)?;
    let text = session.content_text();
    let (start, end) = (a.min(b), a.max(b));
    Some(MenuSnapshot {
        start16: byte_to_utf16_units(&text, start),
        end16: byte_to_utf16_units(&text, end),
        text,
        rect: (
            laid.x as i32,
            laid.y as i32,
            (laid.x + laid.w) as i32,
            (laid.y + laid.h) as i32,
        ),
        cause,
    })
}

/// The toolbar item set for one snapshot (decision 459): ranged
/// selections offer the full set minus Copy/Cut while masked
/// (cleartext never leaves a password field — deletion gets its own
/// item); collapsed carets offer Select-all (with text) plus Paste.
fn toolbar_model_for(scene: &SceneCore, snap: &MenuSnapshot) -> oppa::ToolbarModel {
    let ranged = snap.start16 != snap.end16;
    let masked = scene
        .host()
        .focused_field_session()
        .is_some_and(|s| s.is_masked());
    AndroidInput::toolbar_model_for(snap.rect, ranged, !snap.text.is_empty(), masked)
}

/// Maps shell IME policy to the keyboard (the closed loop the proof
/// only probed): `Show`/`Hide` drive the proxy keyboard;
/// `SetCaretRect` is anchor-only (the hidden proxy needs no anchor)
/// and skips quietly — per-frame caret rects are not failures.
/// Keyboard errors log loudly and continue (a missing IME must not
/// kill the app — hardware keys and adb input still feed fields).
fn apply_ime_policy(
    app: &AndroidApp,
    shell: &mut oppa_shell_android::shell::AndroidShell,
) -> usize {
    use oppa::ime::ImeOps;
    let mut actions = 0;
    for ops in shell.take_ime_log() {
        actions += 1;
        match ops {
            ImeOps::ShowCandidateWindow => {
                if let Err(e) = ime_bridge::show_keyboard(app) {
                    eprintln!("oppa-ime: show failed: {e}");
                }
            }
            ImeOps::HideCandidateWindow => {
                if let Err(e) = ime_bridge::hide_keyboard(app) {
                    eprintln!("oppa-ime: hide failed: {e}");
                }
            }
            ImeOps::SetCaretRect { .. } => {}
        }
    }
    actions
}

/// Interactive generations until Destroy: bring-up per generation
/// (GL row first, then Vulkan; Fifo pinned — interactive stays
/// event-paced), event-paced frames (200 ms poll; input, IME, or
/// caret-blink work repaints + presents), heals on window swaps,
/// parks when windowless. `debug.oppa.continuous=1` shortens the poll
/// quantum to 8 ms instead (decision 434, tap-latency diagnosis —
/// presents stay damage-gated, default off). `deferred_fonts` inject into both
/// backends right after the first present (start-time staging —
/// the boot set already covers first paint). Returns a one-line
/// run summary.
pub fn drive_interactive(
    app: &AndroidApp,
    scene: &mut SceneCore,
    touch: &mut TouchDriver,
    density: f32,
    java_package: &str,
    mut deferred_fonts: Vec<(oppa::text::FontId, std::sync::Arc<[u8]>, u32)>,
) -> Result<String, String> {
    use std::time::{Duration, Instant};
    let t_open = Instant::now();
    let mut instances: Vec<(wgpu::Backends, &'static wgpu::Instance)> = Vec::new();
    let mut won: Option<wgpu::Backends> = None;
    let mut paused = false;
    let mut focus_reported = false;
    let mut heals = 0u32;
    let mut presents = 0u32;
    let mut last_beat = Instant::now();
    // Runner-tracked bars mode (boot is hidden/immersive —
    // `wait_for_window` hides up front; app requests drain into this
    // per iteration, heals re-assert it).
    let mut bars_visible = false;
    // Consecutive skipped no-op redraws (decision 435 bound — see
    // the skip site in the frame loop).
    let mut skipped_redraws: u32 = 0;
    // Loop clipboard (decision 443): the system `ClipboardManager`
    // backend feeds both the Ctrl+C/X/V chords and the toolbar
    // copy/cut/paste (one backend for both — the desktop single-
    // clipboard rule). A missing service falls back to session-local
    // memory with a one-time note (the Linux no-display precedent —
    // in-app copy/paste instead of a dead loop).
    let mut clip_fallback = oppa::InMemoryClipboard::new();
    let mut clip_jni = crate::clipboard_jni::AndroidClipboard::new(app);
    if let Err(e) = &clip_jni {
        eprintln!("oppa-menu: system clipboard unavailable: {e} (in-app only)");
    }
    let clipboard: &mut dyn oppa::Clipboard = match &mut clip_jni {
        Ok(bound) => bound,
        Err(_) => &mut clip_fallback,
    };
    // Unified Android input state (decision 452 — selection-driven,
    // dismissal-aware toolbar mirror + cause tracking + drag-settle;
    // reset on heals, where the decor dies).
    let mut android_input = AndroidInput::new();

    'generation: loop {
        maybe_beat(
            &t_open,
            &mut last_beat,
            presents,
            heals,
            paused,
            app.native_window().is_some(),
        );
        // Continuous heartbeat (decision 434): `debug.oppa.continuous`
        // shortens the input quantum from 200 ms to 8 ms for fps-feel
        // diagnosis (tap latency, not frame rate — presents stay
        // damage-gated, so idle burns nothing). NativeActivity
        // inherits a fixed environment, so the switch rides a system
        // property (`adb shell setprop debug.oppa.continuous 1`), read
        // per generation so it flips without reinstall. Absent =
        // 200 ms quantum (zero behavior change); the mode logs loudly,
        // never silently.
        let continuous = crate::sysprop::system_prop_bool("debug.oppa.continuous");
        if continuous {
            eprintln!("oppa-phase: continuous heartbeat on (debug.oppa.continuous=1)");
        }
        // Stage profiler (diagnosis only): `debug.oppa.profile=1`
        // logs one ms-bucketed line per frame-loop iteration (poll /
        // input+router / repaint / present) to split tap→present
        // latency on-device. Off = zero extra logging (the `Instant`s
        // below still tick — nanosecond noise, no log spam).
        let profile = crate::sysprop::system_prop_bool("debug.oppa.profile");
        if profile {
            eprintln!("oppa-phase: stage profiler on (debug.oppa.profile=1)");
            // Mirror into process env: the shared engine stages
            // (`SceneSession`, `RgbPresenter`) gate their paint
            // breakdown on `OPPA_PROFILE_PAINT`, which desktop sets
            // directly but NativeActivity cannot — same process, so
            // setting it here reaches them.
            std::env::set_var("OPPA_PROFILE_PAINT", "1");
        }
        // Acquire the live window, polling lifecycle while waiting
        // so Resume/Pause keep flowing. A missing window parks here
        // instead of failing the run.
        let window: NativeWindow = loop {
            let out = poll_once(app, Duration::from_millis(100), &mut paused);
            if out.destroyed {
                break 'generation;
            }
            if let Some(w) = app.native_window() {
                if !paused {
                    break w;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let (ww, hh) = (window.width().max(1) as u32, window.height().max(1) as u32);
        // Proxy follows the window generation (decision 441): a
        // pause/resume heal turns the window over and the cached
        // proxy view detaches for good (typing dies on resume while
        // taps keep working). Drop it on every post-boot generation
        // turn so the next focus re-attaches to the live decor;
        // never fails the run (a stuck cache only degrades typing).
        if heals > 0 {
            match ime_bridge::drop_proxy_cache(app) {
                Ok(note) => eprintln!("oppa-ime: heal {note}"),
                Err(e) => eprintln!("oppa-ime: heal proxy drop failed: {e} (typing may stay dead)"),
            }
        }
        // Ordered presenter ladder (decision 432): GL first, Vulkan
        // once it has won (winner-stickiness — the recorded winner
        // re-probes first after a heal), closed with the terminal
        // CPU row (decision 433: GPU-less windows post `rgba8`
        // frames through the native-window lock instead of failing
        // the run).
        let default_order = if won == Some(wgpu::Backends::VULKAN) {
            &oppa_app::ANDROID_GPU_ORDER_VULKAN_FIRST
        } else {
            &oppa_app::ANDROID_GPU_ORDER_GL_FIRST
        };
        let rows = oppa_app::presenter_rows(default_order, true);
        let wrap = AndroidWindow { window: &window };
        let mut factory = AndroidSurfaceFactory {
            instances: &mut instances,
            wrap: &wrap,
        };
        let mut attempt = move |scene: &mut oppa_app::SceneCore,
                                row: oppa_app::GpuRow,
                                instance: &wgpu::Instance,
                                surface: &wgpu::Surface<'_>|
              -> Result<oppa_app::GpuApplied, oppa_app::RowFail> {
            let tag = format!("{:?}", row.backends);
            // The bring-up pins the renderer kind (zygote-inherited
            // env must not decide the arm): `GpuAuto` records what
            // was attempted, never a silent substitution.
            let adapter = match scene.ensure_gpu_for_surface_pinned(
                oppa_app::RendererKind::GpuAuto,
                instance,
                surface,
            ) {
                Ok(adapter) => {
                    eprintln!("oppa-phase: surface-gpu {tag} {adapter}");
                    adapter
                }
                Err(e) => {
                    return Err(oppa_app::RowFail::Next(format!("{tag} refused [{e}]")));
                }
            };
            let format = match scene.configure_gpu_surface(surface, ww, hh, wgpu::PresentMode::Fifo)
            {
                Ok(f) => f,
                Err(e) => {
                    return Err(oppa_app::RowFail::Next(format!(
                        "{tag} configure [{adapter}]: {e}"
                    )));
                }
            };
            Ok(oppa_app::GpuApplied {
                adapter,
                mode: wgpu::PresentMode::Fifo,
                format,
            })
        };
        let (tag, presenter) =
            match oppa_app::walk_presenter_rows(&rows, &mut factory, &mut attempt, scene) {
                Ok(oppa_app::LadderOutcome::Gpu(up)) => {
                    let tag = format!("{:?}", up.backends);
                    won = Some(up.backends);
                    eprintln!(
                        "oppa-phase: presenting {ww}x{hh} [{}] {:?}",
                        up.adapter, up.format
                    );
                    (
                        tag,
                        LivePresenter::Gpu {
                            surface: up.surface,
                            adapter: up.adapter,
                        },
                    )
                }
                // Terminal CPU row (decision 433): no surface, no
                // GPU state — frames post below through the
                // native-window lock. `won` is untouched (the next
                // generation re-probes GPU first, so a transient
                // refusal heals back to hardware on its own).
                Ok(oppa_app::LadderOutcome::Cpu) => {
                    eprintln!("oppa-phase: software present (CPU pixmap)");
                    ("cpu".to_string(), LivePresenter::Cpu)
                }
                Err(e) => {
                    if app.native_window().is_some() {
                        return Err(format!(
                            "no surface-compatible GPU: {}",
                            e.attempts.join("; ")
                        ));
                    }
                    continue 'generation;
                }
            };
        // CPU-pixmap paint follows the resolved presenter (decision
        // 436): a GPU generation reads nothing from the pixmap, so
        // skip its full-scene software raster; a CPU generation keeps
        // painting it for the native-window lock. Safe because this
        // loop never falls back GPU→CPU mid-generation (present
        // failures are loud `Err`s, and `disable_gpu` repaints fresh
        // wherever a fallback does exist).
        scene.set_cpu_paint(matches!(presenter, LivePresenter::Cpu));
        // NOTE: the old `built`/`attempts` locals are gone — the
        // ladder owns the walk; attempts survive only inside the
        // failure message above.
        // Adopt the live size (a recreated window usually matches,
        // but the size is the only truth).
        if scene.viewport() != (ww, hh) {
            scene
                .resize(ww, hh)
                .map_err(|e| format!("{tag} refit {ww}x{hh}: {e}"))?;
            scene
                .repaint()
                .map_err(|e| format!("{tag} repaint after refit: {e}"))?;
            touch
                .shell_mut()
                .note_surface_changed(ww as f32 / density, hh as f32 / density);
        }
        // Heal present (repaint + present the fresh frame, so the
        // compositor stops showing black) with a mode-aware bars
        // re-assert (sticky flags clear on visibility changes — a
        // mount-time app request lands here before the first frame,
        // and heals re-apply the tracked mode instead of forcing
        // hidden, so a shown-bars app survives window turnover).
        imm_mode::drain_system_bars_request(scene.host(), app, java_package, &mut bars_visible);
        imm_mode::apply_system_bars(app, java_package, bars_visible);
        scene
            .repaint()
            .map_err(|e| format!("{tag} heal-repaint: {e}"))?;
        match &presenter {
            LivePresenter::Gpu { surface, adapter } => {
                scene
                    .present_gpu(surface)
                    .map_err(|e| format!("{tag} heal-present [{adapter}]: {e}"))?;
            }
            // Terminal CPU row: the freshly repainted pixmap posts
            // through the native-window lock (same heal contract —
            // the compositor stops showing black either way).
            LivePresenter::Cpu => {
                soft_present::ensure_software_window(&window, ww, hh)
                    .map_err(|e| format!("{tag} heal-setup: {e}"))?;
                soft_present::present_software(scene, &window)
                    .map_err(|e| format!("{tag} heal-present [CPU]: {e}"))?;
            }
        }
        scene.note_frame_presented();
        presents += 1;
        heals += 1;
        eprintln!(
            "oppa-boot: first-present +{}ms",
            t_open.elapsed().as_millis()
        );
        // Deferred font injection (start-time staging — the boot
        // set already covers first paint, so this bulk work lands
        // after pixels instead of before them; taps queue in the
        // OS meanwhile, never lost).
        if heals == 1 {
            let t_fonts = Instant::now();
            let mut n = 0;
            for (id, bytes, index) in std::mem::take(&mut deferred_fonts) {
                scene.set_font_for(id, bytes, index);
                n += 1;
            }
            eprintln!(
                "oppa-boot: deferred-fonts +{}ms ({n} faces)",
                t_fonts.elapsed().as_millis()
            );
        }

        'frame: loop {
            // Continuous heartbeat shortens the input quantum with the
            // present rate (8 ms vs the event-paced 200 ms).
            let quantum_ms = if continuous { 8 } else { 200 };
            let t_iter = std::time::Instant::now();
            let out = poll_once(app, Duration::from_millis(quantum_ms), &mut paused);
            let ms_poll = t_iter.elapsed().as_millis();
            if out.destroyed {
                break 'generation;
            }
            if out.heal {
                // Window turnover drops the cached proxy view (the
                // mirror below re-attaches on next focus), so the
                // controller resets: report memory, dismissal pins,
                // and suppression go with the old generation (fresh
                // boot re-decides).
                android_input.reset();
                break 'frame;
            }
            // Runtime screen-mode toggle (app `ctx.set_system_bars_visible`
            // requests drain here — one bridge call per request, never
            // polled; the scene stays full-bleed, only the host bars move).
            imm_mode::drain_system_bars_request(scene.host(), app, java_package, &mut bars_visible);
            // Window focus into the shell (the IME policy gates on
            // it — without this the keyboard never shows no matter
            // the field focus). Flushed immediately so backgrounding
            // hides the keyboard even though the paused path below
            // skips the input pump.
            let want_focus = !paused && app.native_window().is_some();
            if want_focus != focus_reported {
                focus_reported = want_focus;
                touch.shell_mut().push_event(
                    oppa_shell_android::events::AndroidEvent::FocusChanged(want_focus),
                );
                let _ = touch.shell_mut().pump_events();
            }
            if paused {
                continue 'frame;
            }
            let mut changed = out.redraw;
            // Change attribution for the stage profiler (which input
            // below dirtied this iteration — empty when clean; built
            // only while profiling so the hot path stays untouched).
            let mut why = String::new();
            if profile && out.redraw {
                why.push_str("redraw ");
            }
            // Touch batch → engine.
            let cmds = touch
                .pump_activity(app)
                .map_err(|e| format!("pump input: {e}"))?;
            if profile && !cmds.is_empty() {
                why.push_str(&format!("input({}) ", cmds.len()));
            }
            // IME commits → shell intake; policy → keyboard.
            let ime_drained = ime_bridge::drain_ime_queue_into(touch.shell_mut());
            let ime_policy = apply_ime_policy(app, touch.shell_mut());
            // Focus-driven keyboard (the shell contract: the runner
            // reports framework focus, the policy answers change-only,
            // the bridge executes). An editable focused field shows
            // the proxy keyboard; anything else hides it. Errors log
            // loudly and continue (a missing IME must not kill the
            // app — hardware keys and adb input still feed fields).
            let editable = scene.host().focused_node().is_some_and(|id| {
                scene
                    .host()
                    .retained_semantics(id)
                    .is_some_and(|s| s.role == oppa::Role::TextField)
            });
            match touch.shell_mut().note_field_focus(editable) {
                // (No scene repaint rides these — the keyboard is a
                // system window, not scene content.)
                Some(oppa_shell_android::shell::ImeRequest::Show) => {
                    if let Err(e) =
                        ime_bridge::show_proxy_keyboard(app, java_package, focused_password(scene))
                    {
                        eprintln!("oppa-ime: proxy show failed: {e}");
                    }
                }
                Some(oppa_shell_android::shell::ImeRequest::Hide) => {
                    if let Err(e) = ime_bridge::hide_keyboard(app) {
                        eprintln!("oppa-ime: hide failed: {e}");
                    }
                }
                None => {}
            }
            let batch_work = apply_input_batch(
                scene,
                &cmds,
                ime_drained,
                ime_policy,
                clipboard,
                &android_input,
            )
            .map_err(|e| format!("step: {e}"))?;
            if batch_work {
                changed = true;
                if profile {
                    why.push_str("batch ");
                }
            }
            // Toolbar taps → focused session (decision 443, owned by
            // `AndroidInput` since decision 452): drained once per
            // frame, applied in FIFO order through the same loop
            // clipboard the chords use; the mirror re-reads post-action
            // truth below, so copy/select-all (which keep the
            // selection) re-sync instead of dying. A dismissal near a
            // selection report pins nothing (decision 449 —
            // drag-start AND drag-settle both kill the mode
            // transiently, the latter landing a pump after the report;
            // the new snapshot re-shows on release like native).
            // Selection cause (decision 451 — the toolbar raises for
            // touch-driven selections only, verified native): pointer
            // or menu activity marks touch; keys alone mark keyboard;
            // input-quiet pumps inherit the last decided cause so IME
            // traffic never flips it.
            let menu_actions = ime_bridge::drain_menu_actions();
            let cause = android_input.cause_for(&cmds, &menu_actions, &[]);
            // Collapsed-hold request (decision 459 — consumed once per
            // pump; every snapshot build below shares it).
            let caret_toolbar = scene.host().take_caret_toolbar();
            if !menu_actions.is_empty() {
                let pre = menu_snapshot_for(scene, cause, false);
                let batch_has_selection = menu_actions
                    .iter()
                    .any(|a| matches!(a, MenuAction::SelectionChanged { .. }));
                if batch_has_selection {
                    android_input.note_selection_report(std::time::Instant::now());
                }
                let pin = android_input.pin_for(pre, batch_has_selection);
                for action in &menu_actions {
                    if *action == MenuAction::Dismissed {
                        android_input.note_dismissed(pin.clone());
                        continue;
                    }
                    let Some(session) = scene.host().focused_field_session() else {
                        eprintln!("oppa-menu: action quiet-miss (no focused session)");
                        continue;
                    };
                    if android_input.apply_menu_action(&session, *action, clipboard, pin.clone()) {
                        scene.host().run_until_idle();
                        changed = true;
                        if profile {
                            why.push_str("menu ");
                        }
                    } else {
                        eprintln!("oppa-menu: toolbar action failed: {action:?}");
                    }
                }
                android_input.end_batch(batch_has_selection);
            }
            // Scene-toolbar taps (decision 455): overlay items resolve
            // geometrically in the router (never focus, never tab
            // stops) and queue here; the pump applies them in arrival
            // order through the same loop clipboard the chords use.
            // The toolbar decision below re-reads post-action truth,
            // so Select-all (which keeps the selection) re-syncs
            // instead of dying.
            for action in scene.host().drain_toolbar_actions() {
                let Some(session) = scene.host().focused_field_session() else {
                    eprintln!("oppa-menu: toolbar tap quiet-miss (no focused session)");
                    continue;
                };
                if android_input.apply_toolbar_action(&session, action, clipboard) {
                    scene.host().run_until_idle();
                    changed = true;
                    if profile {
                        why.push_str("toolbar ");
                    }
                } else {
                    eprintln!("oppa-menu: toolbar tap failed: {action:?}");
                }
            }
            // Toolbar sync (decision 455; gating owned by
            // `AndroidInput`): a touch-driven selection — or a
            // collapsed caret with an armed hold request (decision
            // 459) — publishes the scene overlay model (the renderer
            // draws the bar — no system `ActionMode` starts anymore)
            // and mirrors the text + selection into the proxy buffer
            // (IME coherence: the keyboard tracks editor state through
            // context queries even with no mode standing); anything
            // else clears the overlay. Bridge failures log loudly
            // and continue (a missing proxy must not kill the app —
            // chords and typing still work).
            let snapshot = menu_snapshot_for(scene, cause, caret_toolbar);
            let want = snapshot.map(|snap| {
                let model = toolbar_model_for(scene, &snap);
                (snap, model)
            });
            match android_input.decide_toolbar(want) {
                ToolbarOp::Show(model) | ToolbarOp::Update(model) => {
                    eprintln!(
                        "oppa-menu: overlay items={} anchor={:?}",
                        model.items.len(),
                        model.anchor
                    );
                    scene.host().set_toolbar_model(Some(model));
                    // The mirror still needs the text frame the
                    // decision just consumed — re-read post-action
                    // truth (identical; actions settled above).
                    if let Some(snap) = menu_snapshot_for(scene, cause, caret_toolbar) {
                        if let Err(e) = ime_bridge::update_selection_menu(
                            app,
                            java_package,
                            &snap.text,
                            snap.start16,
                            snap.end16,
                            snap.rect,
                        ) {
                            eprintln!("oppa-menu: mirror failed: {e}");
                        }
                    }
                }
                ToolbarOp::Hide => {
                    scene.host().set_toolbar_model(None);
                    eprintln!("oppa-menu: overlay down");
                }
                ToolbarOp::None => {
                    if android_input.show_suppressed() {
                        eprintln!("oppa-menu: show suppressed (keyboard selection)");
                    }
                }
            }
            // Tap re-show (the reported stuck-keyboard bug): a system
            // dismissal (Back, swipe-down) leaves focus set but takes
            // the keyboard, and the change-only policy cannot see it
            // — so any tap landing inside the focused editable field
            // shows the proxy keyboard unconditionally (a redundant
            // show on an up keyboard is a platform no-op, never a
            // flicker). Focus transitions still ride the policy
            // above; this covers only the already-focused tap.
            if let Some(focused) = scene.host().focused_node() {
                let in_field = scene
                    .host()
                    .retained_semantics(focused)
                    .is_some_and(|s| s.role == oppa::Role::TextField)
                    && scene.host().committed_box(focused).is_some_and(|b| {
                        cmds.iter().any(|cmd| match cmd {
                            AndroidCmd::PointerDown { x, y, .. }
                            | AndroidCmd::PointerUp { x, y, .. } => {
                                *x >= b.x && *x <= b.x + b.w && *y >= b.y && *y <= b.y + b.h
                            }
                            _ => false,
                        })
                    });
                if in_field {
                    if let Err(e) =
                        ime_bridge::show_proxy_keyboard(app, java_package, focused_password(scene))
                    {
                        eprintln!("oppa-ime: tap re-show failed: {e}");
                    }
                }
            }
            // Caret blink demand (decision 462 — folded into the
            // outer repaint, never rastered twice): `poll_blink`
            // settles + repaints internally, so every blink flip
            // rastered the full scene once here (~270 ms on the CPU
            // row) and again at the outer repaint below (~260 ms).
            // Settle + read the flip here; the outer `if changed`
            // owns the single raster (a quiet poll still schedules
            // nothing — never a spin).
            scene.host().run_until_idle();
            let blinked = scene.caret_overlay_dirty();
            if blinked {
                changed = true;
                if profile {
                    why.push_str("blink ");
                }
            }
            let ms_input = t_iter.elapsed().as_millis();
            // No-op redraw edges (decision 435): a bare `out.redraw`
            // with no input, batch work, or blink behind it means
            // nothing in our model changed — the scene ignores system
            // bars and insets by architecture, so the presented frame
            // is already current (the emulator's ~1 Hz same-size
            // WindowResized plus the bars toggle's own ContentRect edge
            // both land here). Repainting them burns a full ~100 ms
            // pass per edge for identical pixels, so skip — but never
            // silently forever: every 30th consecutive skip still
            // repaints (bounds any backbuffer-loss staleness; a stale
            // surface then fails loudly through the normal present
            // path, exactly as today). Any real input beside the edge
            // repaints as before.
            if out.redraw && cmds.is_empty() && !batch_work && !blinked {
                if skipped_redraws < 30 {
                    skipped_redraws += 1;
                    changed = false;
                    if profile {
                        why.push_str("skip ");
                    }
                } else {
                    skipped_redraws = 0;
                }
            } else {
                skipped_redraws = 0;
            }
            // Damage-gated in both modes: continuous shortens the input
            // quantum but never repaints clean frames (a full repaint
            // every 8 ms pegs a phone core for identical pixels —
            // measured 93% idle burn; animations still tick through
            // `poll_blink` damage).
            let mut ms_repaint = 0u128;
            let mut ms_present = 0u128;
            if changed {
                let t_paint = std::time::Instant::now();
                scene.repaint().map_err(|e| format!("repaint: {e}"))?;
                ms_repaint = t_paint.elapsed().as_millis();
                match &presenter {
                    LivePresenter::Gpu { surface, adapter } => {
                        scene
                            .present_gpu(surface)
                            .map_err(|e| format!("present [{adapter}]: {e}"))?;
                    }
                    // Terminal CPU row: every changed frame posts the
                    // fresh pixmap (same repaint-then-present shape —
                    // the badge clock below ticks per present either
                    // way).
                    LivePresenter::Cpu => {
                        soft_present::present_software(scene, &window)
                            .map_err(|e| format!("present [CPU]: {e}"))?;
                    }
                }
                scene.note_frame_presented();
                presents += 1;
                ms_present = t_paint.elapsed().as_millis() - ms_repaint;
            }
            if profile {
                eprintln!(
                    "oppa-profile: poll={ms_poll}ms input={ms_input}ms repaint={ms_repaint}ms present={ms_present}ms changed={changed} why={why}"
                );
            }
        }
    }
    Ok(format!(
        "interactive presents={presents} heals={heals} uptime={:.0}s",
        t_open.elapsed().as_secs_f64()
    ))
}
