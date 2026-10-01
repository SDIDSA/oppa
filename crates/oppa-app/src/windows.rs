//! Windows event/present glue (decision 242, Round 7.4 decision 279):
//! `Win32Shell` window + DirectWrite shaping + GPU-first paint (Vello
//! hardware swapchains primary, GDI blit fallback). The input mapping
//! (`Cmd` → framework `InputEvent`) is written out explicitly here:
//! the only proven Win32→framework mapper lives in the spike rig
//! (session-direct, not pipeline input), so this small layer is new
//! code over proven endpoints (`inject_input`, `keys`). Swapchain
//! bring-up mirrors `oppa-fps::driver` (Vulkan first, then DX12,
//! pipeline-cache round-trip, Immediate/Mailbox/Fifo pick).
//!
//! IME delivery (Round 2.1, decision 256): the shell snapshots
//! `WM_IME_STARTCOMPOSITION` / `WM_IME_COMPOSITION` (result +
//! composition strings) / `WM_IME_ENDCOMPOSITION` at message time;
//! this module maps them onto the focused session through
//! [`DesktopLoop::feed_ime`] and pumps the candidate anchor after
//! every step. TSF engagement is best-effort (TSF-only IMEs never
//! send `WM_IME_*` without a document manager — the M1 verdict —
//! and the bridge re-emits through the same message pipeline, so
//! the mapper below is untouched by it).

use std::cell::RefCell;
use std::collections::HashSet;
use std::num::NonZeroIsize;
use std::rc::Rc;

use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};

use oppa::shell::PlatformShell;
use oppa::text::FontId;
use oppa::{
    Ctx, EditSession, ImeCompositionEvent, ImeOps, InputEvent, KeyState, Modifiers, Props, VNode,
};
use oppa_shell_win::{
    Cmd, ImeMessage, ShellConfig, WaitOutcome, Win32Clipboard, Win32FolderDialog, Win32SaveDialog,
    Win32Shell, Win32SystemTheme, Win32WindowControl,
};
use oppa_text_dwrite::DWriteTextService;

use crate::{gpu_disabled_by_env, DesktopLoop, WindowOptions};

/// Live hardware present target (Round 7.4): the `wgpu` swapchain
/// surface for the runner's HWND plus the negotiated mode and backend
/// name for logs. The scene lives in [`DesktopLoop`]'s Vello twin;
/// this is only the swapchain half (mirrors
/// `oppa-fps::driver::GpuState` minus the scene). GDI `blit_rgba`
/// stays as the loud CPU fallback.
struct WindowsGpu {
    surface: wgpu::Surface<'static>,
    mode: wgpu::PresentMode,
    name: String,
}

/// Ordered GPU backend attempts per target (mirrors the fps driver:
/// Windows probes Vulkan first — measured Vello bring-up ~2s vs
/// ~6-16s Dx12, and only Vulkan persists pipeline-cache data in
/// wgpu-hal 29 — then DX12).
fn backend_attempts() -> Vec<(&'static str, wgpu::Backends)> {
    vec![
        ("vulkan", wgpu::Backends::VULKAN),
        ("dx12", wgpu::Backends::DX12),
    ]
}

fn cache_dir() -> Option<std::path::PathBuf> {
    std::env::var("LOCALAPPDATA")
        .ok()
        .map(|b| std::path::PathBuf::from(b).join("oppa-app"))
}

/// Builds the GPU swapchain against `hwnd`, or returns the loud
/// reason to fall back to GDI (mirrors
/// `oppa-fps::driver::FpsDriver::build_gpu`).
fn build_gpu_windows(
    hwnd: windows::Win32::Foundation::HWND,
    loop_: &mut DesktopLoop,
    size: (u32, u32),
) -> Result<WindowsGpu, String> {
    let (w, h) = (size.0.max(1), size.1.max(1));
    let hwnd_val = hwnd.0 as isize;
    // Round 19.8: the fps-demo hinstance shape re-ported (19.7 proved it
    // serves Vulkan end to end, then reverted it for the empty-capabilities
    // reconfigure below). Vulkan requires the real module handle on the
    // raw window handle; DX12 tolerates None.
    let mut win_handle = Win32WindowHandle::new(
        NonZeroIsize::new(hwnd_val).ok_or_else(|| "null HWND for gpu surface".to_string())?,
    );
    win_handle.hinstance = unsafe {
        windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
            .ok()
            .and_then(|h| NonZeroIsize::new(h.0 as isize))
    };
    if win_handle.hinstance.is_none() {
        eprintln!("oppa-app: GetModuleHandleW failed; Vulkan surface may refuse (DX12 unaffected)");
    }
    let raw_window = RawWindowHandle::Win32(win_handle);
    let raw_display = RawDisplayHandle::Windows(WindowsDisplayHandle::new());
    for (name, backends) in backend_attempts() {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = backends;
        let instance: &'static wgpu::Instance = Box::leak(Box::new(wgpu::Instance::new(desc)));
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(raw_display),
            raw_window_handle: raw_window,
        };
        let surface = match unsafe { instance.create_surface_unsafe(target) } {
            Ok(s) => s,
            Err(e) => {
                eprintln!("oppa-app: {name} surface failed: {e}");
                continue;
            }
        };
        let cache_path = cache_dir().map(|d| d.join(format!("vello-pipeline-cache-{name}.bin")));
        let cache_data = cache_path.as_ref().and_then(|p| std::fs::read(p).ok());
        eprintln!(
            "oppa-app: {name} pipeline cache {}",
            cache_data
                .as_ref()
                .map(|d| format!("{} bytes loaded (warm)", d.len()))
                .unwrap_or_else(|| "absent (cold)".to_string())
        );
        match loop_.ensure_gpu_for_surface_with_cache(instance, &surface, cache_data) {
            Ok((adapter, saved)) => {
                eprintln!("oppa-app: {name} {adapter}");
                if let (Some(path), Some(data)) = (cache_path, saved) {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    match std::fs::write(&path, &data) {
                        Ok(()) => {
                            eprintln!("oppa-app: pipeline cache {} bytes saved", data.len())
                        }
                        Err(e) => eprintln!("oppa-app: pipeline cache save failed: {e}"),
                    }
                }
                let mode = loop_.pick_gpu_mode(&surface);
                loop_
                    .configure_gpu_surface(&surface, w, h, mode)
                    .map_err(|e| format!("configure surface: {e}"))?;
                loop_.enable_gpu(mode);
                return Ok(WindowsGpu {
                    surface,
                    mode,
                    name: name.to_string(),
                });
            }
            Err(e) => {
                eprintln!("oppa-app: {name} gpu failed: {e}");
            }
        }
    }
    Err("no working GPU backend".to_string())
}

/// Reconfigures the GPU swapchain to the loop's live viewport (Round
/// 7.4 — resize/DPR arms call this; Round 19.8: a reconfigure failure
/// retries once before disabling GPU loudly and keeping GDI, never a
/// silent blank. The retry is insurance against a transient surface
/// refusal; 19.7's empty-capabilities read (`offered: []`) root-caused
/// to a destroyed window at teardown (see `hwnd_alive`) — not a live
/// transient — so a repeated refusal still disables; the 7.4
/// loud-fallback design is unchanged, only given one second chance
/// with both attempts on stderr).
fn reconfigure_gpu(loop_: &mut DesktopLoop, gpu: &WindowsGpu) {
    if !loop_.is_gpu() {
        return;
    }
    let (w, h) = loop_.viewport();
    if let Err(e) = loop_.configure_gpu_surface(&gpu.surface, w, h, gpu.mode) {
        eprintln!(
            "oppa-app: gpu reconfigure failed ({w}x{h} {mode:?}: {e}); retrying once",
            mode = gpu.mode
        );
        if let Err(e2) = loop_.configure_gpu_surface(&gpu.surface, w, h, gpu.mode) {
            eprintln!("oppa-app: gpu reconfigure retry failed: {e2}; CPU fallback");
            loop_.disable_gpu();
        } else {
            eprintln!("oppa-app: gpu reconfigure retry succeeded ({w}x{h})");
        }
    }
}

/// Presents one frame on the active backend (Round 7.4): GPU
/// swapchain primary with loud GDI fallback. `Outdated`
/// reconfigures and retries once; any other GPU failure logs the
/// exact error and falls back to `blit_rgba` for this frame (the next
/// frame retries GPU). Both failing is a loud `Err` (never a silent
/// blank).
/// Shared frame state behind the live-resize hook (Round 7.20,
/// decision 295): `run_windows` owns this under `Rc<RefCell<..>>`
/// so `WM_SIZE` — dispatched synchronously into `wndproc` from
/// inside `process_os_messages`, including while the OS modal
/// sizing loop traps the thread — can resize + repaint +
/// re-present on the same thread. The main loop never holds the
/// borrow across `process_os_messages` (that is where the hook
/// borrows it), so the two never collide.
pub(crate) struct AppState {
    loop_: DesktopLoop,
    gpu: Option<WindowsGpu>,
}

impl AppState {
    /// Reconfigures the GPU swapchain to the loop's live viewport
    /// (the [`reconfigure_gpu`] free function — the handle moves
    /// out and back so the `RefCell` borrow never splits mutably
    /// and immutably at once).
    fn sync_swapchain(&mut self) {
        let gpu = self.gpu.take();
        if let Some(ref g) = gpu {
            reconfigure_gpu(&mut self.loop_, g);
        }
        self.gpu = gpu;
    }

    /// Presents one frame on the active backend (the
    /// [`present_frame`] free function — same take/restore shape as
    /// [`AppState::sync_swapchain`]).
    fn present(&mut self, hwnd: windows::Win32::Foundation::HWND) -> Result<(), String> {
        let gpu = self.gpu.take();
        let r = present_frame(&mut self.loop_, hwnd, gpu.as_ref());
        self.gpu = gpu;
        r
    }
}

/// Builds the `WM_SIZE` hook installed via
/// [`Win32Shell::set_resize_callback`]: a same-size or zero-size
/// step is a quiet no-op, anything else resizes + repaints,
/// reconfigures the swapchain, and presents — the live frame the
/// modal sizing loop would otherwise never produce (DWM would
/// bitmap-stretch the stale buffer until release). Every step is
/// best-effort (`let _`) like the rest of this runner's HWND
/// effects: a failed step degrades to the loop-bottom live-size
/// poll, never a panic inside `wndproc`.
pub(crate) fn make_resize_callback(
    state: Rc<RefCell<AppState>>,
    hwnd: windows::Win32::Foundation::HWND,
) -> Rc<dyn Fn(u32, u32)> {
    Rc::new(move |w, h| {
        let mut s = state.borrow_mut();
        if (w, h) != s.loop_.viewport() && w > 0 && h > 0 {
            let _ = s.loop_.resize(w, h);
            let _ = s.loop_.repaint();
            s.sync_swapchain();
            let _ = s.present(hwnd);
        }
    })
}

fn present_frame(
    loop_: &mut DesktopLoop,
    hwnd: windows::Win32::Foundation::HWND,
    gpu: Option<&WindowsGpu>,
) -> Result<(), String> {
    if loop_.is_gpu() {
        if let Some(gpu) = gpu {
            match loop_.present_gpu(&gpu.surface) {
                Ok(_) => return Ok(()),
                Err(e) if e.contains("Outdated") => {
                    eprintln!("oppa-app: gpu present outdated ({e}); reconfiguring");
                    let (w, h) = loop_.viewport();
                    match loop_.configure_gpu_surface(&gpu.surface, w, h, gpu.mode) {
                        Ok(_) => match loop_.present_gpu(&gpu.surface) {
                            Ok(_) => return Ok(()),
                            Err(e2) => {
                                eprintln!("oppa-app: gpu retry failed: {e2}; CPU fallback");
                            }
                        },
                        Err(ce) => {
                            eprintln!("oppa-app: gpu reconfigure failed: {ce}; CPU fallback");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("oppa-app: gpu present failed: {e}; CPU fallback");
                }
            }
        } else {
            eprintln!("oppa-app: gpu active without surface; CPU fallback");
        }
    }
    blit_rgba(loop_, hwnd)
}

/// Live async key state (the shell samples shift/ctrl at proc time;
/// alt is not carried by `Cmd`, so the runner samples it here at
/// dispatch — the pump runs every ~8ms while shortcut keys are held.
/// Same bit test as the shell's own `key_down`).
fn key_down(vk: u16) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
    unsafe { GetKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

/// One pumped command's outcome: exit requests unwind the loop
/// (clean exit 0); damage re-presents.
pub(crate) struct DriveOut {
    pub(crate) exit: bool,
    pub(crate) damaged: bool,
}

/// Maps one Win32 shell command into framework input. Discrete
/// pointer commands step straight into the reactive router
/// (Round 7.19, decision 294 — parity with Linux/Android):
/// Down focuses + presses + captures, Moves drive `on_drag` through
/// the capture owner (slider track math reads the capture position),
/// Up classifies the tap / drag-release, Cancel trips the global
/// release. Keys pass their virtual-key code through (the
/// framework key space matches Win32 VKs for the routed set —
/// verified in `input::keys`); Backspace/Delete route to the focused
/// session (consumed, never also injected); Ctrl+letter editing
/// shortcuts are consumed inside [`DesktopLoop::step`] (same
/// never-injected rule); wheel ticks hit-test their scroll target
/// through [`DesktopLoop::scroll_at`]; printable `WM_CHAR`s type into
/// it unless the IME mapper is holding them as commit echo (see
/// [`WinImeMapper`]). Legacy `Click` still synthesizes a tap (kept
/// for backward compatibility — the shell no longer emits it now
/// that Down/Up step discretely); legacy `Drag` is a quiet no-op
/// (the 1D from/to collapse cannot address the router — Moves carry
/// the drag now). Click variants the pipeline cannot express
/// (double/shift field clicks) collapse to a plain tap — stated, not
/// silent.
pub(crate) fn drive_cmd(
    loop_: &mut DesktopLoop,
    shell: &mut Win32Shell,
    ime: &mut WinImeMapper,
    cmd: Cmd,
) -> Result<DriveOut, String> {
    let quiet = DriveOut {
        exit: false,
        damaged: false,
    };
    match cmd {
        Cmd::PointerDown {
            x,
            y,
            shift,
            button,
        } => {
            // Round 8.2: shift rides into the router's tap modifiers
            // (Shift+Click extends field selections end to end).
            // Round 9.2: the OS button rides into the tap taxonomy
            // (secondary taps dispatch menu events, never presses).
            let modifiers = if shift {
                Modifiers::shift()
            } else {
                Modifiers::NONE
            };
            let damage = loop_.step(InputEvent::Pointer {
                id: Some(0),
                action: oppa::input::PointerAction::Down { button },
                x,
                y,
                modifiers,
            })?;
            // A press may move field focus: re-anchor + re-sync the
            // candidate machinery to the newly focused session.
            post_input_ime(loop_, shell);
            // Round 8.3: Down sets hover too — publish its cursor.
            sync_cursor(loop_, shell);
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::PointerMove { x, y } => {
            let damage = loop_.step(InputEvent::pointer_move(x, y))?;
            // Round 8.3: hover moved — publish the hovered style.
            sync_cursor(loop_, shell);
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::PointerUp {
            x,
            y,
            shift,
            button,
        } => {
            // Round 8.2: release shift rides with the press (the tap
            // modifiers the field Shift+Click arm reads). Round 9.2:
            // the release button pairs with the Down by pointer id.
            let modifiers = if shift {
                Modifiers::shift()
            } else {
                Modifiers::NONE
            };
            let damage = loop_.step(InputEvent::Pointer {
                id: Some(0),
                action: oppa::input::PointerAction::Up { button },
                x,
                y,
                modifiers,
            })?;
            // A release may move field focus (tap): re-anchor +
            // re-sync like a press.
            post_input_ime(loop_, shell);
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::PointerCancel => {
            let damage = loop_.step(InputEvent::pointer_cancel())?;
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::Click { x, y, .. } => {
            let (x, y) = (x as f32, y as f32);
            let damage = loop_.step(InputEvent::pointer_down(x, y))?;
            let damage = damage + loop_.step(InputEvent::pointer_up(x, y))?;
            // A click may move field focus: re-anchor + re-sync the
            // candidate machinery to the newly focused session.
            post_input_ime(loop_, shell);
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::Key { vk, shift, ctrl } => {
            // Fresh user intent closes the commit-echo window (a stale
            // swallow count must never eat real typing — the echo of a
            // commit always arrives without an interleaving key).
            ime.clear_swallow();
            if loop_.escape_exits(vk, true) {
                return Ok(DriveOut {
                    exit: true,
                    damaged: false,
                });
            }
            // Editing keys are consumed by the focused session (never
            // also injected — a future field `Key` handler must not
            // see them twice).
            if vk == oppa::input::keys::BACKSPACE {
                let damage = loop_.backspace()?;
                post_input_ime(loop_, shell);
                return Ok(DriveOut {
                    exit: false,
                    damaged: damage > 0,
                });
            }
            if vk == oppa::input::keys::DELETE {
                let damage = loop_.delete_forward()?;
                post_input_ime(loop_, shell);
                return Ok(DriveOut {
                    exit: false,
                    damaged: damage > 0,
                });
            }
            // Alt rides live state, not `Cmd` (decision 246): the
            // `step` interception refuses ctrl+alt (AltGr types real
            // chars on many layouts), and the injected modifiers
            // should stop dropping alt in the same stroke.
            use windows::Win32::UI::Input::KeyboardAndMouse::VK_MENU;
            let damage = loop_.step(InputEvent::Key {
                code: vk,
                modifiers: Modifiers {
                    shift,
                    ctrl,
                    alt: key_down(VK_MENU.0),
                    ..Modifiers::NONE
                },
                state: KeyState::Pressed,
                repeat: false,
            })?;
            // Arrows (and any key) may move the composition caret.
            post_input_ime(loop_, shell);
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::Char { ch } => {
            // Control chars (< 0x20) are shortcut shadows (Backspace,
            // Enter, Escape arrive via `KeyDown`) — skipped per the
            // spike's WM_CHAR rule; `type_text` guards the rest.
            if (ch as u32) < 0x20 {
                return Ok(quiet);
            }
            // Commit echo (the IME delivering the committed text a
            // second time as WM_CHARs) is swallowed so committed text
            // is never inserted twice — the spike's swallow rule.
            if ime.swallow_char(ch) {
                return Ok(quiet);
            }
            let damage = loop_.type_text(&ch.to_string())?;
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::Scroll { x, y, dx, dy } => {
            let damage = loop_.scroll_at(x, y, dx, dy)?;
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::Drag { .. } => Ok(quiet),
        Cmd::FocusChanged(focused) => {
            if !focused {
                // External focus management (locked #27): a real focus
                // change commits any active IME composition.
                loop_.host().notify_edit_focus_lost();
            }
            // The TSF document manager follows window focus.
            shell.tsf_note_focus(focused);
            Ok(quiet)
        }
        Cmd::DpiChanged { w, h, dpi, .. } => {
            // Monitor DPI change (Round 2.4, OQ-G10-2): the HWND
            // repositioning to the suggested rect happens in
            // `run_windows` (`SetWindowPos` — outside the pipeline);
            // here the pipeline re-bases DPR first (CSS stable, so
            // the resize below usually no-ops) and repaints crisply
            // at the new scale. x/y ride the Cmd for the runner.
            loop_.set_device_pixel_ratio(oppa::dpr_from_dpi(dpi))?;
            // Negative suggested sizes are OS garbage — clamp before
            // the u32 cast (wrapping would surface as billions).
            loop_.resize(w.max(0) as u32, h.max(0) as u32)?;
            let damage = loop_.repaint()?;
            Ok(DriveOut {
                exit: false,
                damaged: damage > 0,
            })
        }
        Cmd::SystemThemeChanged => {
            // OS light/dark flip (Round 16.2, decision 315):
            // re-query the installed source into the reactive theme
            // signal (themed bodies recolor in place); the sync
            // repaints itself exactly when the mode moved.
            let changed = loop_.sync_system_theme()?;
            Ok(DriveOut {
                exit: false,
                damaged: changed,
            })
        }
        Cmd::CloseRequested => {
            // Close veto (Round 16.3, decision 316): the app-level
            // handler decides — approved closes destroy through the
            // normal DestroyWindow→WM_DESTROY→quit flow (the pump
            // exits on the quit message); vetoed closes keep the
            // window running so a modal can mount on the next frame.
            if loop_.close_requested() {
                shell.destroy_window();
            }
            Ok(quiet)
        }
    }
}

/// The app-side IME mapper: snapshotted Win32 IME messages onto the
/// focused session (Round 2.1, decision 256 — the spike's mapper
/// re-homed from a session handle to the focused-field lookup, so
/// composition follows focus instead of one rig field).
///
/// Mapping decisions (spike parity, recorded in the round log):
/// - Composition over an active selection: `CompositionStarted` is
///   fed BEFORE `DeleteRange{selection}` so the session's atomic
///   pre-composition undo snapshot captures the PRE-deletion content
///   (Ctrl+Z must restore the selection's original text).
/// - `GCS_RESULTSTR` at `WM_IME_COMPOSITION` is the commit; `END`
///   without a commit is the cancel.
/// - `GCS_CURSORPOS` is composition-relative UTF-16; mapped to UTF-8
///   bytes in composite coordinates (spike decision 20).
/// - Composition state rides the session (`is_composing`), not a
///   mapper flag — there is exactly one source of truth, so a cold
///   update anchors like the session's own race path and a cold
///   commit inserts like the session spells it.
/// - `swallow_pending` (UTF-16 units of the last commit) is the only
///   mapper state: commit echo `WM_CHAR`s consume it instead of
///   inserting. A fresh key or a new composition closes the echo
///   window (stale counts must never eat real typing); a short echo
///   resets and inserts (spike parity).
pub(crate) struct WinImeMapper {
    pub(crate) swallow_pending: usize,
}

impl WinImeMapper {
    pub(crate) fn new() -> Self {
        Self { swallow_pending: 0 }
    }

    /// One snapshotted message into normalized session events (pure
    /// over the session's observable state — headless-testable).
    pub(crate) fn map_message(
        &mut self,
        msg: &ImeMessage,
        session: &EditSession,
    ) -> Vec<ImeCompositionEvent> {
        match msg {
            ImeMessage::StartComposition => {
                self.swallow_pending = 0;
                let sel = session.selection();
                let mut out = vec![ImeCompositionEvent::CompositionStarted { start_byte: sel.0 }];
                if sel.0 != sel.1 {
                    out.push(ImeCompositionEvent::DeleteRange { range: sel });
                }
                out
            }
            ImeMessage::Composition {
                comp,
                cursor_pos,
                result,
                ..
            } => {
                if let Some(text) = result {
                    self.swallow_pending = text.encode_utf16().count();
                    return vec![ImeCompositionEvent::CompositionCommitted {
                        committed: text.clone(),
                    }];
                }
                if let Some(comp) = comp {
                    let mut out = Vec::new();
                    let start = match session.composition_start_byte() {
                        Some(s) => s,
                        None => {
                            // An update with no begin (platforms race):
                            // anchor like the session's own race path.
                            let sel = session.selection();
                            out.push(ImeCompositionEvent::CompositionStarted { start_byte: sel.0 });
                            if sel.0 != sel.1 {
                                out.push(ImeCompositionEvent::DeleteRange { range: sel });
                            }
                            sel.0
                        }
                    };
                    let caret = start + utf16_offset_to_byte(comp, (*cursor_pos).max(0) as usize);
                    out.push(ImeCompositionEvent::CompositionUpdated {
                        composition: comp.clone(),
                        caret_byte: caret,
                    });
                    out
                } else {
                    Vec::new()
                }
            }
            ImeMessage::EndComposition => {
                if session.is_composing() {
                    // END without a result commit: the IME cancelled
                    // (ESC, or a superseding action).
                    vec![ImeCompositionEvent::CompositionCancelled]
                } else {
                    Vec::new()
                }
            }
            ImeMessage::Notify { .. } | ImeMessage::SetContext { .. } => Vec::new(),
        }
    }

    /// Consumes one `WM_CHAR` as commit echo when the swallow count
    /// covers it (spike parity). Returns true when swallowed.
    pub(crate) fn swallow_char(&mut self, ch: char) -> bool {
        let units = ch.len_utf16();
        if self.swallow_pending >= units {
            self.swallow_pending -= units;
            true
        } else {
            self.swallow_pending = 0;
            false
        }
    }

    /// Closes the commit-echo window (fresh user intent — see the
    /// struct docs).
    pub(crate) fn clear_swallow(&mut self) {
        self.swallow_pending = 0;
    }
}

/// UTF-16 offset into a UTF-8 byte offset (spike parity — surrogate
/// pairs consume two units, lone units floor into the char).
pub(crate) fn utf16_offset_to_byte(text: &str, mut utf16_index: usize) -> usize {
    for (byte, ch) in text.char_indices() {
        let units = ch.len_utf16();
        if utf16_index < units {
            return byte;
        }
        utf16_index -= units;
    }
    text.len()
}

/// Drives one snapshotted IME message into the focused session and
/// re-anchors + re-syncs the candidate machinery. Quiet `Ok(false)`
/// with no focused field (the `feed_ime` focus-race precedent).
/// Returns whether anything damaged (re-present).
pub(crate) fn drive_ime(
    loop_: &mut DesktopLoop,
    shell: &mut Win32Shell,
    ime: &mut WinImeMapper,
    msg: &ImeMessage,
) -> Result<bool, String> {
    let Some(session) = loop_.host().focused_field_session() else {
        return Ok(false);
    };
    let events = ime.map_message(msg, &session);
    if events.is_empty() {
        return Ok(false);
    }
    let damage = loop_.feed_ime(&events)?;
    post_input_ime(loop_, shell);
    Ok(damage > 0)
}

/// Candidate anchor + TSF store refresh after input that may have
/// moved the caret or the focus (Round 2.1): the OS candidate window
/// follows every IME step, click, and key through the host-resolved
/// anchor (dpr-1 client px — the loop paints at dpr 1.0 into a
/// DPI-unaware window, so device px are client px); the TSF text
/// store mirrors the focused session (no-ops when TSF is off).
fn post_input_ime(loop_: &DesktopLoop, shell: &mut Win32Shell) {
    if let Some([x, y, width, height]) = loop_.ime_anchor() {
        shell.set_ime(ImeOps::SetCaretRect {
            x,
            y,
            width,
            height,
        });
    }
    if shell.tsf_enabled() {
        if let Some(session) = loop_.host().focused_field_session() {
            let st = session.observable();
            shell.tsf_sync_external(&st.content, st.sel);
        }
    }
}

/// Active-work wait bound in ms (Round 9.1, decision 300): the pre-9.1
/// poll granularity, now a deadline bound instead of a spin — holds
/// fire, worker outboxes drain, and transition tails retire within
/// this of becoming ready. Settled loops block indefinitely (near-0%
/// CPU), so this never sets the idle cadence, only the active one.
const ACTIVE_WAIT_MS: u32 = 8;

/// Wait horizon for the event-driven loop (Round 9.1 — pure,
/// headless-testable): `None` blocks until the next OS message
/// (settled: no frame demand, no live interpolations, no worker
/// traffic, no armed holds); `Some(ACTIVE_WAIT_MS)` ticks while
/// background work is live. Long-press arms never create frame
/// demand by design (G11 — a held finger neither spins the loop nor
/// hangs it), so they are polled explicitly here; flings likewise
/// (Round 10.2 — momentum ticks explicitly, never self-demand).
/// Round 15.2 (decision 313): a focused caret flips without input,
/// so the horizon also wakes at the next flip (ceiled to whole ms —
/// a due flip wakes in 1ms, repaints, and re-arms ~500ms out, never
/// a spin); the earliest live deadline wins.
fn wait_timeout_ms(host: &oppa::ComponentHost) -> Option<u32> {
    let background = host.runtime().has_demand()
        || host.longpress_armed_count() > 0
        || host.with_evaluator(|ev| ev.active_count()) > 0
        || host.fling_count() > 0;
    let blink_ms = host
        .caret_blink_in_secs()
        .map(|s| (s * 1000.0).ceil().max(1.0) as u32);
    // Round 21.1 (decision 328): component timers join the horizon
    // (ceiled to whole ms like the blink arm — a due timer wakes in
    // 1ms, fires, and re-arms at its cadence, never a spin).
    let now_ms = host.now_ms();
    let timer_ms = host
        .next_timer_due_ms(now_ms)
        .map(|due| ((due - now_ms).max(0.0).ceil().max(1.0)) as u32);
    // Earliest live deadline wins (any arm absent drops out).
    let mut horizon = None;
    for ms in [background.then_some(ACTIVE_WAIT_MS), blink_ms, timer_ms]
        .into_iter()
        .flatten()
    {
        horizon = Some(horizon.map_or(ms, |h: u32| h.min(ms)));
    }
    horizon
}

/// Publishes the hovered style's cursor to the shell (Round 8.3,
/// decision 299): hover resolution lives core-side
/// (`hover_cursor`); the shell owns the OS cursor. Unstyled hover
/// falls back to the arrow — leaving a field restores it.
fn sync_cursor(loop_: &DesktopLoop, shell: &mut Win32Shell) {
    shell.set_cursor(
        loop_
            .host()
            .hover_cursor()
            .unwrap_or(oppa::CursorIcon::Default),
    );
}

/// Grows the frame so the client area is exactly the scene size
/// (`CreateWindowExW` takes outer dims — without this the client is
/// smaller than the pixmap and every present clips; decision 202 in
/// the fps-demo, demo-side policy copied here).
fn fit_client_to_scene(hwnd: windows::Win32::Foundation::HWND, w: u32, h: u32) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRect, GetWindowLongW, SetWindowPos, GWL_STYLE, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOZORDER, WINDOW_STYLE,
    };
    unsafe {
        let style = WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32);
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: w as i32,
            bottom: h as i32,
        };
        let _ = AdjustWindowRect(&mut rc, style, false);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            rc.right - rc.left,
            rc.bottom - rc.top,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Whether the window still exists (Round 19.8 root-cause guard:
/// 19.7's empty-capabilities reconfigure + GetDC FATAL were one
/// teardown race — `WM_CLOSE` destroys the HWND synchronously inside
/// `drive_cmd`, and the loop-bottom poll then read the dead window
/// as 1x1 and reconfigured + presented into it. A dead window exits
/// the loop cleanly; a live-window failure stays loud FATAL).
fn hwnd_alive(hwnd: windows::Win32::Foundation::HWND) -> bool {
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)).as_bool() }
}

fn live_client_size(hwnd: windows::Win32::Foundation::HWND) -> (u32, u32) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
    unsafe {
        let mut cr = RECT::default();
        let _ = GetClientRect(hwnd, &mut cr);
        (
            (cr.right - cr.left).max(1) as u32,
            (cr.bottom - cr.top).max(1) as u32,
        )
    }
}

/// Raw (unclamped) client dimensions in px — negative/zero when the
/// window has no drawable area (minimized or destroyed windows read
/// zeros from `GetClientRect`).
fn raw_client_size(hwnd: windows::Win32::Foundation::HWND) -> (i32, i32) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
    unsafe {
        let mut cr = RECT::default();
        let _ = GetClientRect(hwnd, &mut cr);
        (cr.right - cr.left, cr.bottom - cr.top)
    }
}

/// Whether the window is minimized (iconic) per the OS.
fn window_minimized(hwnd: windows::Win32::Foundation::HWND) -> bool {
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsIconic(hwnd).as_bool() }
}

/// Headless-testable resize-skip decision (Round 20.2, decision 325):
/// a minimized window, or one whose client area is zero/negative,
/// has no drawable area — resizing the surface to the clamped 1x1
/// is a pointless reconfigure (the 19.8 minimize-thrash note), so
/// the loop-bottom poll skips it and keeps the pre-minimize
/// viewport for a clean restore. Destroyed windows never reach this
/// (the `hwnd_alive` break above runs first — a dead window exits,
/// it does not skip).
fn should_skip_resize_for_client_area(raw_w: i32, raw_h: i32, iconic: bool) -> bool {
    iconic || raw_w <= 0 || raw_h <= 0
}

/// OS-bridging skip check for the loop-bottom poll: `IsIconic` or a
/// zero-area `GetClientRect`, read before any 1x1 clamping.
fn client_area_missing(hwnd: windows::Win32::Foundation::HWND) -> bool {
    let (w, h) = raw_client_size(hwnd);
    should_skip_resize_for_client_area(w, h, window_minimized(hwnd))
}

/// Applies the OS-suggested bounds on a DPI crossing (Round 2.4):
/// move + resize the window in one `SetWindowPos` (keeping z-order
/// and activation — a DPI move must never restack or steal focus).
/// Best-effort like every other HWND effect here (the pipeline
/// re-bases independently, so a failed move degrades to the live
/// size poll, never a silent blank).
fn move_window_to_suggested(
    hwnd: windows::Win32::Foundation::HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            x,
            y,
            w.max(1),
            h.max(1),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Blits the loop's latest paint to the window client DC (RGBA →
/// BGRA + `SetDIBitsToDevice`). Loud on any failure (a present
/// failure is never a silent blank frame). Same pattern as the
/// fps-demo's `present_cpu` (the proven GDI path).
fn blit_rgba(loop_: &DesktopLoop, hwnd: windows::Win32::Foundation::HWND) -> Result<(), String> {
    use windows::Win32::Graphics::Gdi::{
        GetDC, ReleaseDC, SetDIBitsToDevice, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        RGBQUAD,
    };
    let rgba = loop_.rgba8()?;
    let (w, h) = loop_.viewport();
    if rgba.len() != w as usize * h as usize * 4 {
        return Err(format!("oppa-app: pixels {} != {w}x{h}x4", rgba.len()));
    }
    let mut bgra = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD {
            rgbBlue: 0,
            rgbGreen: 0,
            rgbRed: 0,
            rgbReserved: 0,
        }],
    };
    unsafe {
        let dc = GetDC(Some(hwnd));
        if dc.is_invalid() {
            // Round 19.8: the mid-loop fallback died here in 19.7 with a
            // bare "GetDC failed" (a never-exercised-on-GPU-boxes path).
            // Name the OS reason and the window's liveness — the fix
            // round's evidence depends on it.
            let code = windows::Win32::Foundation::GetLastError();
            let alive = windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)).as_bool();
            return Err(format!(
                "oppa-app: GetDC failed (last_error={code:?}, is_window={alive})"
            ));
        }
        let rows = SetDIBitsToDevice(
            dc,
            0,
            0,
            w,
            h,
            0,
            0,
            0,
            h,
            bgra.as_ptr() as *const core::ffi::c_void,
            &bmi,
            DIB_RGB_COLORS,
        );
        ReleaseDC(Some(hwnd), dc);
        if rows != h as i32 {
            return Err(format!(
                "oppa-app: SetDIBitsToDevice painted {rows} of {h} rows"
            ));
        }
    }
    Ok(())
}

/// Process DPI-awareness declaration (Round 6.2, decision 273):
/// without an explicit declaration Windows runs Win32 processes in
/// System-DPI unaware mode — bitmap-stretching the window on
/// High-DPI screens and never dispatching `WM_DPICHANGED` — which
/// leaves the Round-2.4 live-DPI engine dormant. Declares
/// Per-Monitor V2 first; where V2 is unavailable (older Windows 10
/// builds) falls back to per-monitor v1. Both OS calls refuse when
/// awareness is already declared (manifest or a previous call), so
/// the refusal arm is an expected status, never a panic — logged on
/// stderr per the loud-failure rule.
///
/// Must run before `CreateWindowExW` (called in [`run_windows`]
/// ahead of `Win32Shell::new`): awareness declared after window
/// creation does not apply to that window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DpiAwareness {
    /// Per-Monitor V2 declared — true `WM_DPICHANGED` delivery.
    PerMonitorV2,
    /// V2 unavailable; per-monitor v1 declared instead.
    PerMonitorV1,
    /// Both declarations refused (manifest-declared or already set).
    Unavailable,
}

pub fn ensure_dpi_awareness() -> DpiAwareness {
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwareness, SetProcessDpiAwarenessContext,
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, PROCESS_PER_MONITOR_DPI_AWARE,
    };
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_ok() {
            return DpiAwareness::PerMonitorV2;
        }
        // V2 unavailable on older Windows 10 builds — per-monitor
        // v1 still opts out of bitmap scaling.
        if SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE).is_ok() {
            return DpiAwareness::PerMonitorV1;
        }
    }
    eprintln!(
        "oppa-app: DPI awareness declaration refused (already declared via manifest or an \
         earlier call?) — the OS may bitmap-scale this window instead of sending WM_DPICHANGED"
    );
    DpiAwareness::Unavailable
}

/// Injects raster faces for every font id the layout has shaped
/// so far (Round 7.2, decision 277): reads each recorded id's
/// backing file and registers it on the loop, so bold/italic/
/// fallback runs rasterize real glyphs instead of advance-cell
/// bars. `injected` memoizes across calls (repeat ids skip —
/// file IO happens once per id, never per frame); the return
/// lists newly injected ids this call. Every miss warns loudly
/// and continues (decision-200 best-effort — a missing face must
/// never kill the window).
pub(crate) fn inject_recorded_faces(
    loop_: &mut DesktopLoop,
    dwrite: &DWriteTextService,
    injected: &mut HashSet<FontId>,
) -> Vec<FontId> {
    let mut fresh = Vec::new();
    for id in dwrite.recorded_font_ids() {
        if !injected.insert(id) {
            continue;
        }
        match dwrite.font_file_source(id) {
            Some((path, index)) => match std::fs::read(&path) {
                Ok(bytes) => {
                    loop_.set_font_for(id, bytes, index);
                    fresh.push(id);
                }
                Err(e) => eprintln!("oppa-app: cpu face {id:?} unreadable: {e} (bars)"),
            },
            None => eprintln!("oppa-app: no cpu face source for {id:?} (bars)"),
        }
    }
    fresh
}

pub fn run_windows<P: Props>(
    options: WindowOptions,
    props: P,
    component: fn(&Ctx, &P) -> VNode,
    configure: impl FnOnce(&mut DesktopLoop),
) -> Result<(), String> {
    // Round 6.2: declare before any window exists (see
    // `ensure_dpi_awareness` — after `CreateWindowExW` is too late).
    ensure_dpi_awareness();
    let mut shell = Win32Shell::new(ShellConfig {
        title: options.title.clone(),
        width: options.width as i32,
        height: options.height as i32,
        record_messages: false,
        suppress_os_composition_window: true,
        visible: true,
    })
    .map_err(|e| format!("oppa-app: create window: {e}"))?;
    let hwnd = shell.hwnd();
    fit_client_to_scene(hwnd, options.width, options.height);

    // COM apartment for the TSF bridge (spike parity — the bridge
    // CoCreates on this thread). Failure keeps the IMM path, never
    // kills the window.
    unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() {
            eprintln!("oppa-app: CoInitializeEx failed: {hr:?} (TSF off, IMM only)");
        }
    }
    // TSF engagement, best-effort and loud (TSF-only IMEs never send
    // WM_IME_* without a document manager — the M1 verdict — and the
    // bridge re-emits through the same ImeMessage pipeline, so the
    // mapper below is untouched by it).
    match shell.enable_tsf("", (0, 0)) {
        Ok(log) => {
            for line in log {
                eprintln!("oppa-app: tsf: {line}");
            }
        }
        Err((e, log)) => {
            eprintln!("oppa-app: TSF unavailable: {e} (IMM only)");
            for line in log {
                eprintln!("oppa-app: tsf: {line}");
            }
        }
    }
    // Snapshotted IME messages land here through the shell callback
    // (fired by `pump_events` below); the loop drains them before
    // each command batch (commit echo-swallow ordering — a commit
    // sets the count its trailing WM_CHARs consume).
    let ime_msgs: Rc<RefCell<Vec<ImeMessage>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let sink = ime_msgs.clone();
        shell.set_ime_callback(Rc::new(move |msg| sink.borrow_mut().push(msg.clone())));
    }

    // DirectWrite, shared two ways (Round 7.2): the host owns one
    // clone for layout shaping while this runner keeps the other —
    // the recording map is shared, so raster-face injection below
    // covers EVERY id the scene shaped (regular, bold, fallback),
    // not just a startup probe. Face injection stays best-effort
    // (decision 200): without a face the backend draws
    // advance-cell bars — warn loudly and continue, never kill
    // the window.
    let dwrite = DWriteTextService::new().map_err(|e| format!("oppa-app: DirectWrite: {e:?}"))?;
    let mut loop_ = DesktopLoop::new(
        options.width,
        options.height,
        Box::new(dwrite.clone()),
        "Segoe UI",
    )?;
    // The editing shortcuts read and write the real OS clipboard
    // (decision 246 — same backend the shell lends through its seam;
    // the handle is stateless, so a second owner changes nothing).
    loop_.set_clipboard(Box::new(Win32Clipboard::new()));
    // Native save + folder pickers (Round 16.1, decision 314 — COM
    // `IFileSaveDialog` / `IFileDialog+FOS_PICKFOLDERS`, modal to
    // this window; the STA apartment above covers them too).
    loop_.set_save_dialog(Box::new(Win32SaveDialog::new(hwnd)));
    loop_.set_folder_dialog(Box::new(Win32FolderDialog::new(hwnd)));
    // OS theme at startup (Round 16.2, decision 315): install the
    // registry reader and sync before mount so the first paint
    // already matches the system (matching the app default stays a
    // quiet no-op, never a repaint spin).
    loop_.set_theme_source(Box::new(Win32SystemTheme));
    let _ = loop_.sync_system_theme()?;
    // Runtime window chrome (Round 16.3, decision 316): title,
    // min/max, and fullscreen forward into the live shell (the
    // close veto consults the loop-installed app handler on
    // `WM_CLOSE` — installed by the app, defaulting to close).
    loop_.set_window_control(Box::new(Win32WindowControl::of_shell(&shell)));
    loop_.mount("App", props, component);
    // App ownership (Round 25.3, decision 338): after mount (host
    // signals exist) and after the platform backend installs above
    // (app configuration wins), before the pump below.
    configure(&mut loop_);

    // Initial DPR from the live monitor (Round 2.4 — the loop
    // constructs at 1.0; a non-96 monitor would otherwise paint
    // unscaled until the first crossing).
    loop_
        .set_device_pixel_ratio(shell.device_pixel_ratio())
        .map_err(|e| format!("oppa-app: initial dpr: {e}"))?;

    // Raster faces for everything the mount + DPR pass shaped
    // (Round 7.2 — regular, bold, fallback alike, or bars; Round 7.4
    // lands them in BOTH backends so the Vello twin never paints
    // tofu).
    // Re-topped every frame below: ids first shaped by later
    // input (a new fallback family) resolve on the next pump —
    // one frame of bars at most, file IO only for novel ids.
    let mut injected_faces: HashSet<FontId> = HashSet::new();
    inject_recorded_faces(&mut loop_, &dwrite, &mut injected_faces);

    // GPU-first bring-up (Round 7.4, decision 279): hardware
    // swapchains primary (Vulkan first, then DX12), GDI fallback.
    // Any failure logs loudly and keeps GDI (never a silent blank,
    // never fatal for a missing GPU); `OPPA_RENDERER=cpu` skips the
    // attempt entirely for deterministic CPU runs.
    let mut gpu: Option<WindowsGpu> = None;
    if gpu_disabled_by_env() {
        eprintln!("oppa-app: OPPA_RENDERER=cpu — CPU path (GDI)");
    } else {
        let size = loop_.viewport();
        match build_gpu_windows(hwnd, &mut loop_, size) {
            Ok(g) => {
                eprintln!("oppa-app: GPU path ({} / {:?})", g.name, g.mode);
                // Sync the Vello twin before the first present
                // (creation-time scene is stale once GPU enables
                // both-backend paints).
                if let Err(e) = loop_.repaint() {
                    return Err(format!("oppa-app: gpu warmup repaint: {e}"));
                }
                gpu = Some(g);
            }
            Err(e) => {
                eprintln!("oppa-app: {e}; CPU fallback");
            }
        }
    }

    // Warmup paint before the first pump so the window appears with
    // content, never blank (GPU presents the Vello twin when up,
    // GDI blits the CPU pixmap otherwise).
    loop_.repaint()?;
    present_frame(&mut loop_, hwnd, gpu.as_ref())?;

    // Live resize during the modal sizing loop (Round 7.20,
    // decision 295): border drags trap the thread inside
    // `DefWindowProcW`, so the loop below never runs mid-drag —
    // the state moves behind `Rc<RefCell<..>>` and `WM_SIZE`
    // resizes + repaints + re-presents synchronously from `wndproc`
    // instead. Registered after warmup (before this point no hook
    // exists to fire — `fit_client_to_scene` above already
    // settled).
    let state = Rc::new(RefCell::new(AppState { loop_, gpu }));
    shell.set_resize_callback(make_resize_callback(state.clone(), hwnd));

    // The IME mapper lives across frames: the commit echo-swallow
    // count spans pump batches (echo WM_CHARs may trail the commit
    // into a later batch, with no interleaving key).
    let mut ime = WinImeMapper::new();
    loop {
        // Round 9.1 (decision 300): event-driven wait replacing the
        // 8ms spin — settled loops block here until input (near-0%
        // CPU); live transitions/worker traffic/armed holds tick at
        // the poll bound. Already-queued messages wake immediately,
        // so no latency is added either way. `state` is NOT borrowed
        // across the wait (nothing to borrow — the timeout derives
        // from an owned `Option<u32>` below).
        let timeout = {
            let s = state.borrow();
            wait_timeout_ms(s.loop_.host())
        };
        let timed_out = matches!(shell.wait_for_input(timeout), WaitOutcome::Timeout);
        // `state` is NOT borrowed here: `WM_SIZE` dispatched
        // synchronously inside this pump borrows it (the hook
        // above) — holding it across this call would panic.
        if shell.process_os_messages() {
            break;
        }
        // Component-requested close + runner poll (Round 26.2,
        // decision 342 -- see `run_poll_hook` docs): poll first (it
        // may write files and ask for close), then a drained request
        // re-enters the veto consult exactly like WM_CLOSE. No state
        // borrow is held across either call (WM_SIZE borrows
        // synchronously -- same rule as the wait above).
        state.borrow_mut().loop_.run_poll_hook();
        let mut damaged = false;
        if state.borrow().loop_.host().take_close_request() {
            let out = drive_cmd(
                &mut state.borrow_mut().loop_,
                &mut shell,
                &mut ime,
                Cmd::CloseRequested,
            )?;
            if out.exit {
                return Ok(());
            }
            damaged |= out.damaged;
        }
        // THE PUMP (Round 2.1 — it was missing: without it no input,
        // IME included, ever reaches `take_cmds`, and the shell queue
        // grows unbounded): moves queue→cmds and fires the IME
        // callback above.
        let _ = shell.pump_events();
        // Raster-face top-up (Round 7.2): novel ids shaped since the
        // last frame resolve before this frame's paints (memoized —
        // steady state is a set diff, never file IO).
        inject_recorded_faces(&mut state.borrow_mut().loop_, &dwrite, &mut injected_faces);
        // IME first each batch (commit echo-swallow ordering — a
        // commit sets the count its trailing WM_CHARs consume).
        for msg in ime_msgs.borrow_mut().drain(..).collect::<Vec<_>>() {
            damaged |= drive_ime(&mut state.borrow_mut().loop_, &mut shell, &mut ime, &msg)?;
        }
        // DPR crossings resize the scene inside `drive_cmd`
        // (Round 2.4): capture the viewport so the swapchain can
        // follow below (Round 7.4 — LIVE size, never stale).
        let viewport_before = state.borrow().loop_.viewport();
        let mut had_cmds = false;
        for cmd in shell.take_cmds() {
            had_cmds = true;
            // Suggested-bounds positioning lives here (HWND effect —
            // outside the headless-testable pipeline, which re-bases
            // DPR + resizes + repaints through `drive_cmd` below).
            // `state` is free here on purpose: `SetWindowPos` can
            // dispatch `WM_SIZE` synchronously, and the hook needs
            // the borrow to run the live path.
            if let Cmd::DpiChanged { x, y, w, h, .. } = cmd {
                move_window_to_suggested(hwnd, x, y, w, h);
            }
            let out = drive_cmd(&mut state.borrow_mut().loop_, &mut shell, &mut ime, cmd)?;
            if out.exit {
                return Ok(());
            }
            damaged |= out.damaged;
        }
        // Swapchain follows DPR/size crossings (Round 7.4).
        if state.borrow().loop_.viewport() != viewport_before {
            state.borrow_mut().sync_swapchain();
        }
        if damaged {
            // Teardown race (Round 19.8): a `CloseRequested` in this
            // batch may have destroyed the window above — presenting
            // into it is a doomed GetDC, not a frame. Exit cleanly.
            if !hwnd_alive(hwnd) {
                eprintln!("oppa-app: window gone mid-batch; exiting cleanly");
                break;
            }
            state.borrow_mut().present(hwnd)?;
        }
        // Resize follows the live client (decision-202 class): a
        // resized window with a stale surface clips forever. With
        // the hook above this is usually a no-op already (the
        // modal-loop path resized live); it stays as the fallback
        // for sizes that arrived with no `WM_SIZE`.
        let live = live_client_size(hwnd);
        if live != state.borrow().loop_.viewport() {
            // Same teardown race: `GetClientRect` on a destroyed
            // window reads zeros (clamped to 1x1 above) — that is
            // not a resize, it is the window being gone. 19.7 read
            // it as a reconfigure and died on the empty
            // capabilities + GetDC of a dead HWND.
            if !hwnd_alive(hwnd) {
                eprintln!("oppa-app: window gone; exiting cleanly");
                break;
            }
            // Round 20.2 (decision 325): a minimized window also
            // reads a zero client area — that is not a resize
            // either. Skip the 1x1 reconfigure (the 19.8
            // minimize-thrash note) and keep the pre-minimize
            // viewport so un-minimize restores with no work. Only
            // the resize is skipped — the settle block below still
            // runs.
            if !client_area_missing(hwnd) {
                let mut s = state.borrow_mut();
                s.loop_.resize(live.0, live.1)?;
                s.loop_.repaint()?;
                s.sync_swapchain();
                s.present(hwnd)?;
            }
        }
        // Round 9.1: timer-driven settle — background work (hold
        // deadlines, worker outbox drains, transition retirement)
        // progresses even with zero OS messages. Message-driven
        // batches already settled through `drive_cmd` above, so this
        // runs only on message-less timer ticks (never a second
        // settle for one batch, never a spin: `run_until_idle`
        // returns 0 once settled and the wait re-blocks). Round
        // 10.2: momentum ticks here too (paced, one fling per tick —
        // flings never demand frames themselves). Round 15.2: the
        // caret blink ticks here too (`poll_blink` repaints the flip
        // itself — the arm below only presents it — so a blink frame
        // never double-paints).
        if !had_cmds && timed_out {
            let frames = state.borrow().loop_.host().run_until_idle();
            let flung = state.borrow().loop_.host().tick_flings();
            // Round 21.1 (decision 328): due component timers fire
            // here — `tick_timers` settles + repaints itself when
            // anything fired, so the arm below only presents it.
            let timers = state.borrow_mut().loop_.tick_timers()?;
            if frames > 0 || flung {
                state.borrow_mut().loop_.repaint()?;
                state.borrow_mut().present(hwnd)?;
            } else if timers > 0 || state.borrow_mut().loop_.poll_blink()? {
                // `tick_timers` repainted its own damage above (this
                // arm only presents it); the blink arm repaints
                // itself inside `poll_blink` for the same reason.
                state.borrow_mut().present(hwnd)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod dpi_tests {
    use super::*;

    /// Round 6.2: the DPI-awareness helper reports a known status
    /// instead of panicking — first call declares (V2 preferred, v1
    /// fallback) or reports refusal; a repeat call (awareness already
    /// set) must still answer with a status, never panic.
    #[test]
    fn dpi_awareness_init_reports_expected_status() {
        fn is_known(s: DpiAwareness) -> bool {
            matches!(
                s,
                DpiAwareness::PerMonitorV2 | DpiAwareness::PerMonitorV1 | DpiAwareness::Unavailable
            )
        }
        assert!(is_known(ensure_dpi_awareness()));
        assert!(is_known(ensure_dpi_awareness()));
    }
}

#[cfg(test)]
mod faces_tests {
    use super::*;
    use oppa::text::{FontWeight, TextService, TextStyle};

    /// Round 7.2: injection covers every recorded id (regular +
    /// bold), memoizes repeats, and leaves the loop paintable.
    /// Real DirectWrite (this box shapes Segoe UI headlessly —
    /// the win_ime tests already rely on real Win32 windows).
    #[test]
    fn recorded_faces_inject_covering_bold() {
        let dwrite = DWriteTextService::new().expect("DirectWrite present");
        let regular = dwrite
            .shape("Ag", &TextStyle::new("Segoe UI", 16.0))
            .expect("shapes");
        let mut bold_style = TextStyle::new("Segoe UI", 16.0);
        bold_style.weight = FontWeight::BOLD;
        let bold = dwrite.shape("Ag", &bold_style).expect("shapes bold");
        assert_ne!(
            regular.runs[0].font_id, bold.runs[0].font_id,
            "probe spans two ids"
        );
        let mut loop_ = DesktopLoop::new(
            200,
            150,
            Box::new(DWriteTextService::new().expect("DirectWrite present")),
            "Segoe UI",
        )
        .expect("headless loop builds");
        let mut done = HashSet::new();
        let fresh = inject_recorded_faces(&mut loop_, &dwrite, &mut done);
        assert_eq!(fresh.len(), 2, "regular + bold inject, got {fresh:?}");
        assert!(done.contains(&regular.runs[0].font_id));
        assert!(done.contains(&bold.runs[0].font_id));
        assert!(
            inject_recorded_faces(&mut loop_, &dwrite, &mut done).is_empty(),
            "repeats memoize (no file IO twice)"
        );
        loop_.repaint().expect("loop still paints");
    }

    /// Round 7.9: the Select chevron ("▾" U+25BE) shapes through
    /// DirectWrite (fallback-covered); "✓" (U+2713) shapes too (kept
    /// as backend coverage although the Checkbox check went vector
    /// in Round 7.15 — no control depends on it anymore).
    /// Real DirectWrite, headless like the test above.
    #[test]
    fn check_and_chevron_glyphs_shape() {
        let dwrite = DWriteTextService::new().expect("DirectWrite present");
        for glyph in ["✓", "▾"] {
            let run = dwrite
                .shape(glyph, &TextStyle::new("Segoe UI", 14.0))
                .unwrap_or_else(|e| panic!("shapes {glyph:?}: {e:?}"));
            assert!(
                !run.glyphs.is_empty(),
                "{glyph:?} shapes no glyphs (tofu downstream)"
            );
        }
    }
}

#[cfg(test)]
mod pointer_tests {
    use super::*;
    use oppa::text::{TextService, TextStyle};
    use oppa::{
        Cluster, Color, Div, FontId, FontMetrics, ShapedGlyph, ShapedRun, SharedString, Signal,
        Style, Text, TextError, TextRun,
    };

    /// Uniform-advance fake shaper (mirrors the `DesktopLoop`
    /// harness: body 14px → 8.75px/char).
    struct FakeText;

    impl TextService for FakeText {
        fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
            Vec::new()
        }

        fn shape(&self, text: &str, style: &TextStyle) -> Result<ShapedRun, TextError> {
            if text.is_empty() {
                return Err(TextError::EmptyText);
            }
            let em = style.font_size_px * style.device_pixel_ratio;
            let adv = em * 0.625;
            let metrics = FontMetrics {
                ascent: em * 0.75,
                descent: em * 0.25,
                line_gap: em * 0.125,
            };
            let mut glyphs = Vec::new();
            let mut clusters = Vec::new();
            for (k, (i, ch)) in text.char_indices().enumerate() {
                let len = ch.len_utf8();
                glyphs.push(ShapedGlyph {
                    glyph_id: k as u32,
                    x_advance: adv,
                    x_offset: 0.0,
                    y_offset: 0.0,
                });
                clusters.push(Cluster {
                    byte_range: (i, i + len),
                    glyph_range: (glyphs.len() - 1, glyphs.len()),
                });
            }
            Ok(ShapedRun {
                total_advance: adv * glyphs.len() as f32,
                text_len_bytes: text.len(),
                runs: vec![TextRun {
                    byte_range: (0, text.len()),
                    glyph_range: (0, glyphs.len()),
                    rtl: false,
                    script: 0,
                    font_id: FontId(0),
                    font_metrics: metrics,
                }],
                glyphs,
                clusters,
            })
        }
    }

    fn hidden_shell() -> Win32Shell {
        Win32Shell::new(ShellConfig {
            title: "pointer-test".to_string(),
            width: 200,
            height: 150,
            record_messages: false,
            suppress_os_composition_window: true,
            visible: false,
        })
        .expect("hidden test window builds")
    }

    #[derive(Clone)]
    struct FlipProps {
        on: Signal<bool>,
    }
    impl Props for FlipProps {}

    fn flip_app(_ctx: &Ctx, props: &FlipProps) -> VNode {
        let on = props.on.clone();
        Div("screen")
            .style(Style::new().size(200, 150).bg(Color(0xFF_FF_FF)))
            .child(
                Div("flip")
                    .style(Style::new().size(96, 32).bg(Color(0x88_88_88)))
                    .semantics(oppa::Semantics::button().label("Flip"))
                    .on_press(move || on.set(!on.get()))
                    .child(VNode::from(Text {
                        text: SharedString::from("Flip"),
                        style: Text::body_secondary,
                    })),
            )
    }

    /// Round 7.19 (decision 294): `drive_cmd` steps the discrete
    /// pointer feed into `DesktopLoop` — Down captures, Moves hold
    /// the capture owner, Up classifies the tap, Cancel releases
    /// without firing, and the legacy `Click` tap still works.
    #[test]
    fn drive_cmd_steps_discrete_pointer_down_move_up() {
        let loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        let on = loop_.host().runtime().signal(false);
        loop_.mount("Flip", FlipProps { on: on.clone() }, flip_app);
        let mut loop_ = loop_;
        let mut shell = hidden_shell();
        let mut ime = WinImeMapper::new();

        let id = oppa::find_retained_by_debug(loop_.host(), "flip")
            .into_iter()
            .next()
            .expect("flip node");
        let b = loop_.host().committed_box(id).expect("hit box");
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);

        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::PointerDown {
                x: cx,
                y: cy,
                shift: false,
                button: oppa::PointerButton::Primary,
            },
        )
        .expect("down steps");
        assert_eq!(
            loop_.host().capture_node_for(0),
            Some(id),
            "down captures the button"
        );

        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::PointerMove { x: cx + 5.0, y: cy },
        )
        .expect("move steps");
        assert_eq!(
            loop_.host().capture_node_for(0),
            Some(id),
            "moves hold the capture owner"
        );

        let out = drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::PointerUp {
                x: cx,
                y: cy,
                shift: false,
                button: oppa::PointerButton::Primary,
            },
        )
        .expect("up steps");
        assert!(out.damaged, "the tap repaints");
        assert!(on.get(), "down + up on the button fires the press");
        assert_eq!(
            loop_.host().capture_node_for(0),
            None,
            "up releases the capture"
        );

        // Cancel mid-press releases without firing.
        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::PointerDown {
                x: cx,
                y: cy,
                shift: false,
                button: oppa::PointerButton::Primary,
            },
        )
        .expect("down steps");
        assert_eq!(
            loop_.host().capture_node_for(0),
            Some(id),
            "second down captures again"
        );
        drive_cmd(&mut loop_, &mut shell, &mut ime, Cmd::PointerCancel).expect("cancel steps");
        assert_eq!(
            loop_.host().capture_node_for(0),
            None,
            "cancel releases the capture"
        );
        assert!(
            on.get(),
            "cancel never fires the press (still flipped exactly once)"
        );

        // The legacy Click still synthesizes its tap.
        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::Click {
                dbl_click: false,
                shift: false,
                x: cx as i32,
                y: cy as i32,
            },
        )
        .expect("legacy click steps");
        assert!(!on.get(), "legacy click tap flips back");
    }

    /// Round 8.2 (decision 298): `drive_cmd` forwards release shift
    /// into the router's tap modifiers (the Shift+Click field arm).
    #[test]
    fn drive_cmd_forwards_shift_to_tap_modifiers() {
        let loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        let on = loop_.host().runtime().signal(false);
        loop_.mount("Flip", FlipProps { on: on.clone() }, flip_app);
        let mut loop_ = loop_;
        let mut shell = hidden_shell();
        let mut ime = WinImeMapper::new();

        let id = oppa::find_retained_by_debug(loop_.host(), "flip")
            .into_iter()
            .next()
            .expect("flip node");
        let b = loop_.host().committed_box(id).expect("hit box");
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);

        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::PointerDown {
                x: cx,
                y: cy,
                shift: false,
                button: oppa::PointerButton::Primary,
            },
        )
        .expect("down steps");
        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::PointerUp {
                x: cx,
                y: cy,
                shift: true,
                button: oppa::PointerButton::Primary,
            },
        )
        .expect("up steps");
        assert!(on.get(), "shift tap still activates");
        assert!(
            loop_.host().last_press_modifiers().shift,
            "release shift reaches the router"
        );
    }

    /// Round 9.2 (decision 301): a secondary tap through `drive_cmd`
    /// fires `on_secondary_press` (and `on_context_menu` when
    /// declared) without ever firing the primary `on_press`.
    #[test]
    fn drive_cmd_secondary_tap_fires_secondary_never_primary() {
        use oppa::{Ctx, Props, VNode};
        #[derive(Clone)]
        struct MenuBtnProps {
            primary: Signal<bool>,
            secondary: Signal<bool>,
            menu: Signal<bool>,
        }
        impl Props for MenuBtnProps {}
        fn menu_btn(_ctx: &Ctx, p: &MenuBtnProps) -> VNode {
            let (primary, secondary, menu) =
                (p.primary.clone(), p.secondary.clone(), p.menu.clone());
            Div("menubtn")
                .style(Style::new().size(96, 32))
                .semantics(oppa::Semantics::button().label("Menu"))
                .on_press(move || primary.set(true))
                .on_secondary_press(move || secondary.set(true))
                .on_context_menu(move || menu.set(true))
                .build()
        }
        let loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        let (primary, secondary, menu) = (
            loop_.host().runtime().signal(false),
            loop_.host().runtime().signal(false),
            loop_.host().runtime().signal(false),
        );
        loop_.mount(
            "Menu",
            MenuBtnProps {
                primary: primary.clone(),
                secondary: secondary.clone(),
                menu: menu.clone(),
            },
            menu_btn,
        );
        let mut loop_ = loop_;
        let mut shell = hidden_shell();
        let mut ime = WinImeMapper::new();

        let id = oppa::find_retained_by_debug(loop_.host(), "menubtn")
            .into_iter()
            .next()
            .expect("button node");
        let b = loop_.host().committed_box(id).expect("hit box");
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        for cmd in [
            Cmd::PointerDown {
                x: cx,
                y: cy,
                shift: false,
                button: oppa::PointerButton::Secondary,
            },
            Cmd::PointerUp {
                x: cx,
                y: cy,
                shift: false,
                button: oppa::PointerButton::Secondary,
            },
        ] {
            drive_cmd(&mut loop_, &mut shell, &mut ime, cmd).expect("secondary steps");
        }
        assert!(!primary.get(), "secondary never fires the primary press");
        assert!(secondary.get(), "secondary tap fires on_secondary_press");
        assert!(menu.get(), "secondary tap fires on_context_menu");
        // Auxiliary taps stay quiet (taxonomy without v1 behavior).
        secondary.set(false);
        menu.set(false);
        for cmd in [
            Cmd::PointerDown {
                x: cx,
                y: cy,
                shift: false,
                button: oppa::PointerButton::Auxiliary,
            },
            Cmd::PointerUp {
                x: cx,
                y: cy,
                shift: false,
                button: oppa::PointerButton::Auxiliary,
            },
        ] {
            drive_cmd(&mut loop_, &mut shell, &mut ime, cmd).expect("auxiliary steps");
        }
        assert!(
            !primary.get() && !secondary.get() && !menu.get(),
            "auxiliary is quiet"
        );
    }

    /// Round 8.3 (decision 299): `drive_cmd` publishes the hovered
    /// style's cursor to the shell on Down/Move (hand over the styled
    /// button, arrow over empty space).
    #[test]
    fn drive_cmd_move_publishes_hover_cursor() {
        use oppa::{Ctx, Props, VNode};
        #[derive(Clone)]
        struct CursorBtn;
        impl Props for CursorBtn {}
        fn cursor_btn(_ctx: &Ctx, _: &CursorBtn) -> VNode {
            Div("cursor-btn")
                .style(
                    Style::new()
                        .size(96, 32)
                        .bg(Color(0x22_66_CC))
                        .cursor(oppa::CursorIcon::Pointer),
                )
                .on_press(|| {})
                .build()
        }
        let loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        loop_.mount("Cur", CursorBtn, cursor_btn);
        let mut loop_ = loop_;
        let mut shell = hidden_shell();
        let mut ime = WinImeMapper::new();

        let id = oppa::find_retained_by_debug(loop_.host(), "cursor-btn")
            .into_iter()
            .next()
            .expect("button node");
        let b = loop_.host().committed_box(id).expect("hit box");
        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::PointerMove {
                x: b.x + b.w / 2.0,
                y: b.y + b.h / 2.0,
            },
        )
        .expect("move steps");
        assert_eq!(
            shell.shared.borrow().current_cursor,
            Some(oppa::CursorIcon::Pointer),
            "hover over the styled button shows the hand"
        );
        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::PointerMove { x: 199.0, y: 149.0 },
        )
        .expect("move steps");
        assert_eq!(
            shell.shared.borrow().current_cursor,
            Some(oppa::CursorIcon::Default),
            "empty space restores the arrow"
        );
    }

    /// Round 9.1 (decision 300): the wait horizon blocks on settled
    /// loops and ticks at the poll bound while background work is
    /// live (requested frames, armed holds) — the event-driven
    /// policy, headless-proven here (the OS wait itself is shell
    /// scope, proven in `oppa-shell-win`).
    #[test]
    fn wait_timeout_blocks_when_idle_ticks_when_busy() {
        let loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        let on = loop_.host().runtime().signal(false);
        loop_.mount("Flip", FlipProps { on: on.clone() }, flip_app);
        let loop_ = loop_;
        assert_eq!(
            wait_timeout_ms(loop_.host()),
            None,
            "settled loop blocks until input"
        );
        // Requested frames are demand (worker drain shape).
        loop_.host().runtime().request_frame();
        assert_eq!(wait_timeout_ms(loop_.host()), Some(8));
        loop_.host().run_until_idle();
        assert_eq!(wait_timeout_ms(loop_.host()), None);
        // An armed hold ticks (holds never demand frames by design —
        // the policy polls them explicitly).
        let id = oppa::find_retained_by_debug(loop_.host(), "flip")
            .into_iter()
            .next()
            .expect("flip node");
        let b = loop_.host().committed_box(id).expect("hit box");
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        loop_
            .host()
            .inject_input(oppa::InputEvent::pointer_down(cx, cy));
        loop_.host().run_until_idle();
        assert_eq!(loop_.host().capture_node_for(0), Some(id), "hold is down");
        assert_eq!(wait_timeout_ms(loop_.host()), Some(8));
        loop_
            .host()
            .inject_input(oppa::InputEvent::pointer_cancel());
        loop_.host().run_until_idle();
        assert_eq!(wait_timeout_ms(loop_.host()), None);
    }

    /// Minimal TextInput-shaped tree (decision 240's shape, local so
    /// this module never depends on the controls catalog): focusable
    /// outer with its own session over the value signal.
    #[derive(Clone)]
    struct BlinkFieldProps {
        value: Signal<SharedString>,
    }
    impl Props for BlinkFieldProps {}

    fn blink_field_app(ctx: &Ctx, props: &BlinkFieldProps) -> VNode {
        let session = ctx.edit_session(props.value.clone());
        Div("blink-field")
            .style(Style::new().size(200, 32).bg(Color(0xFF_FF_FF)))
            .semantics(oppa::Semantics::text_field().label("Name"))
            .on_press(move || session.caret_to_end())
            .child(VNode::from(oppa::TextField {
                text: props.value.get(),
                style: Text::body_secondary,
                label: SharedString::from("Name"),
            }))
    }

    /// Round 15.2 (decision 313): a focused caret wakes the
    /// event-driven wait at the next blink flip (within the
    /// half-period) instead of blocking past it; unfocused loops
    /// still block until input.
    #[test]
    fn wait_timeout_wakes_for_caret_blink() {
        let loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        let value = loop_.host().runtime().signal(SharedString::from(""));
        loop_.mount(
            "BlinkField",
            BlinkFieldProps {
                value: value.clone(),
            },
            blink_field_app,
        );
        let mut loop_ = loop_;
        assert_eq!(wait_timeout_ms(loop_.host()), None, "unfocused loop blocks");
        // Tap the field: collapsed caret, blink reset to visible.
        let id = oppa::find_retained_by_debug(loop_.host(), "blink-field")
            .into_iter()
            .next()
            .expect("field");
        let b = loop_.host().committed_box(id).expect("hit box");
        loop_
            .step(oppa::InputEvent::pointer_down(b.x + 1.0, b.y + 1.0))
            .expect("down steps");
        loop_
            .step(oppa::InputEvent::pointer_up(b.x + 1.0, b.y + 1.0))
            .expect("up steps");
        assert_eq!(loop_.host().focused_node(), Some(id));
        match wait_timeout_ms(loop_.host()) {
            Some(ms) => assert!(
                (1..=500).contains(&ms),
                "blink wake within the half-period, got {ms}"
            ),
            None => panic!("focused caret must wake the wait"),
        }
    }

    /// Round 21.1 (decision 328): an armed component timer wakes the
    /// event-driven wait at its due instant; a timerless loop still
    /// blocks until input.
    #[test]
    fn wait_timeout_wakes_for_component_timer() {
        #[derive(Clone)]
        struct TimerArm;
        impl Props for TimerArm {}
        fn timer_arm_app(ctx: &Ctx, _props: &TimerArm) -> VNode {
            ctx.use_timeout(5000.0, || {});
            VNode::Hole
        }
        let loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        assert_eq!(wait_timeout_ms(loop_.host()), None, "no timers blocks");
        loop_.mount("TimerArm", TimerArm, timer_arm_app);
        loop_.host().run_until_idle();
        match wait_timeout_ms(loop_.host()) {
            Some(ms) => assert!(
                (1..=5000).contains(&ms),
                "timer wake within the delay, got {ms}"
            ),
            None => panic!("armed timer must wake the wait"),
        }
    }

    /// Round 16.2 (decision 315): a system-theme command re-queries
    /// the installed source into the reactive signal (damaged); with
    /// no source it stays a quiet no-op.
    #[test]
    fn drive_cmd_system_theme_syncs_source_into_signal() {
        use oppa::{ScriptedThemeSource, ThemeMode};
        let mut loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        let on = loop_.host().runtime().signal(false);
        loop_.mount("Flip", FlipProps { on: on.clone() }, flip_app);
        let mut shell = hidden_shell();
        let mut ime = WinImeMapper::new();
        // No source: quiet no-op.
        let out =
            drive_cmd(&mut loop_, &mut shell, &mut ime, Cmd::SystemThemeChanged).expect("drives");
        assert!(!out.damaged, "no source never repaints");
        assert_eq!(loop_.host().theme().mode(), ThemeMode::Light);
        // Scripted Dark: applies + damages.
        let mut source = ScriptedThemeSource::new();
        source.push_reading(ThemeMode::Dark);
        loop_.set_theme_source(Box::new(source));
        let out =
            drive_cmd(&mut loop_, &mut shell, &mut ime, Cmd::SystemThemeChanged).expect("drives");
        assert!(out.damaged, "mode flip repaints");
        assert_eq!(loop_.host().theme().mode(), ThemeMode::Dark);
    }

    /// Round 16.3 (decision 316): a close command destroys the
    /// window by default, but a refusing handler suppresses
    /// destruction — the pump keeps running instead of exiting.
    #[test]
    fn drive_cmd_close_veto_suppresses_destruction() {
        use ::windows::Win32::UI::WindowsAndMessaging::IsWindow;
        // Default (no handler): approved → destroyed.
        let mut loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        let mut shell = hidden_shell();
        let mut ime = WinImeMapper::new();
        let hwnd = shell.hwnd();
        let out = drive_cmd(&mut loop_, &mut shell, &mut ime, Cmd::CloseRequested).expect("drives");
        assert!(!out.exit && !out.damaged, "close is quiet either way");
        assert!(
            unsafe { !IsWindow(Some(hwnd)).as_bool() },
            "approved close destroys"
        );
        // Refusing handler: suppressed → alive.
        let mut loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        loop_.set_close_handler(std::rc::Rc::new(|| false));
        let mut shell = hidden_shell();
        let mut ime = WinImeMapper::new();
        let hwnd = shell.hwnd();
        let out = drive_cmd(&mut loop_, &mut shell, &mut ime, Cmd::CloseRequested).expect("drives");
        assert!(!out.exit && !out.damaged, "veto is quiet too");
        assert!(
            unsafe { IsWindow(Some(hwnd)).as_bool() },
            "vetoed close keeps the pump running"
        );
    }

    #[derive(Clone)]
    struct TrackProps {
        value: Signal<f32>,
    }
    impl Props for TrackProps {}

    /// Slider-shaped drag surface (the `Slider` track contract in
    /// miniature, without the controls dependency): press captures,
    /// moves map the capture x over the track box to 0..=100 snapped
    /// to 10s — the exact router path `Slider`'s `on_drag` reads
    /// through `capture_position`.
    fn track_app(ctx: &Ctx, props: &TrackProps) -> VNode {
        let host = ctx.host();
        let value = props.value.clone();
        Div("screen")
            .style(Style::new().size(200, 150).bg(Color(0xFF_FF_FF)))
            .child(
                Div("track")
                    .style(Style::new().size(100, 32).bg(Color(0xEE_EE_EE)))
                    .on_press(|| {})
                    .on_drag(move || {
                        let Some((x, _)) = host.capture_position() else {
                            return;
                        };
                        let track = oppa::find_retained_by_debug(&host, "track")
                            .into_iter()
                            .next()
                            .expect("track retained");
                        let b = host.committed_box(track).expect("track laid out");
                        let frac = ((x - b.x) / b.w).clamp(0.0, 1.0);
                        value.set((frac * 10.0).round() * 10.0);
                    })
                    .build(),
            )
    }

    /// A Slider-shaped control fed Down + Moves through `drive_cmd`
    /// updates its value from the pointer x (the Windows drag gap,
    /// closed: Down focuses + captures, Moves drive the drag math,
    /// Up keeps the value and drops the capture).
    #[test]
    fn drive_cmd_pointer_drag_drives_a_slider_shaped_track() {
        let loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        let value = loop_.host().runtime().signal(0.0f32);
        loop_.mount(
            "Track",
            TrackProps {
                value: value.clone(),
            },
            track_app,
        );
        let mut loop_ = loop_;
        let mut shell = hidden_shell();
        let mut ime = WinImeMapper::new();

        let track = oppa::find_retained_by_debug(loop_.host(), "track")
            .into_iter()
            .next()
            .expect("track node");
        let b = loop_.host().committed_box(track).expect("track box");
        let mid_y = b.y + b.h / 2.0;

        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::PointerDown {
                x: b.x + 10.0,
                y: mid_y,
                shift: false,
                button: oppa::PointerButton::Primary,
            },
        )
        .expect("down steps");
        assert_eq!(
            loop_.host().capture_node_for(0),
            Some(track),
            "press captures the track"
        );
        assert_eq!(value.get(), 0.0, "press alone sets nothing");

        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::PointerMove {
                x: b.x + 80.0,
                y: mid_y,
            },
        )
        .expect("move steps");
        assert!(
            (value.get() - 80.0).abs() < 0.001,
            "drag maps x to value, got {}",
            value.get()
        );

        // Past the end pins (clamp, never wrap).
        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::PointerMove {
                x: b.x + 200.0,
                y: mid_y,
            },
        )
        .expect("move steps");
        assert!(
            (value.get() - 100.0).abs() < 0.001,
            "drag past end pins max, got {}",
            value.get()
        );

        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::PointerUp {
                x: b.x + 200.0,
                y: mid_y,
                shift: false,
                button: oppa::PointerButton::Primary,
            },
        )
        .expect("up steps");
        assert!(
            (value.get() - 100.0).abs() < 0.001,
            "release keeps the value, got {}",
            value.get()
        );
        assert_eq!(
            loop_.host().capture_node_for(0),
            None,
            "release drops the capture"
        );
    }

    /// Round 7.20 (decision 295): firing the registered resize hook
    /// (the `WM_SIZE` path the modal sizing loop takes) resizes the
    /// viewport and repaints, so border drags present live frames
    /// instead of DWM-stretching a stale buffer.
    #[test]
    fn resize_callback_resizes_viewport_and_repaints() {
        let loop_ =
            DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
        let on = loop_.host().runtime().signal(false);
        loop_.mount("Flip", FlipProps { on: on.clone() }, flip_app);
        let state = Rc::new(RefCell::new(AppState { loop_, gpu: None }));
        let mut shell = hidden_shell();
        let hwnd = shell.hwnd();
        let cb = make_resize_callback(state.clone(), hwnd);
        shell.set_resize_callback(cb.clone());

        assert_eq!(state.borrow().loop_.viewport(), (200, 150));
        cb(300, 200);
        assert_eq!(
            state.borrow().loop_.viewport(),
            (300, 200),
            "the hook resizes the scene"
        );
        assert_eq!(
            state.borrow().loop_.rgba8().expect("pixels").len(),
            300 * 200 * 4,
            "the paint surface refits to the live size"
        );
        // A settled re-fire is a quiet no-op (no repaint storm).
        cb(300, 200);
        assert_eq!(state.borrow().loop_.viewport(), (300, 200));
        // Zero sizes never reach the scene (no surface exists).
        cb(0, 200);
        assert_eq!(state.borrow().loop_.viewport(), (300, 200));
    }

    /// Round 20.2 (decision 325): `drive_cmd` forwards a horizontal
    /// `Scroll` cmd (`dx != 0`, `dy == 0` — the `WM_MOUSEHWHEEL`
    /// shape) into the bound horizontal feed, end to end through
    /// hit-test resolution. Positive `dx` grows the feed, negative
    /// `dx` shrinks it (clamped, never wrapping).
    #[test]
    fn hwheel_scroll_cmd_drives_bound_horizontal_feed() {
        use oppa::Style;
        #[derive(Clone)]
        struct HScrollCase;
        impl Props for HScrollCase {}
        fn h_scroll_app(ctx: &Ctx, _props: &HScrollCase) -> VNode {
            let x = ctx.scroll_x();
            oppa::Div("root").child(
                oppa::ScrollArea("hlist")
                    .style(Style::new().size(200, 40).x(40))
                    .on_scroll(|| {})
                    .child(
                        oppa::Div("wide")
                            .style(Style::new().size(300, 20).x(x.get()))
                            .build(),
                    ),
            )
        }
        let loop_ =
            DesktopLoop::new(400, 300, Box::new(FakeText), "Test").expect("headless loop builds");
        let handle = loop_.mount("HScroll", HScrollCase, h_scroll_app);
        let root = handle.root_instance();
        let offset_x = loop_
            .host()
            .instance_scroll_x(root)
            .expect("scroll_x handle");
        let list = oppa::find_retained_by_debug(loop_.host(), "hlist")
            .into_iter()
            .next()
            .expect("list node");
        loop_.host().bind_scroll_x(list, offset_x.clone());
        let mut loop_ = loop_;
        let mut shell = hidden_shell();
        let mut ime = WinImeMapper::new();
        // (50, 10) hits the strip — walk-up resolves the list.
        let out = drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::Scroll {
                x: 50.0,
                y: 10.0,
                dx: 30.0,
                dy: 0.0,
            },
        )
        .expect("horizontal scroll steps");
        assert!(out.damaged, "moved strip repaints");
        assert_eq!(offset_x.get(), 30.0, "positive dx grows the feed");
        // Tilt the other way: the feed shrinks back, clamped at 0.
        drive_cmd(
            &mut loop_,
            &mut shell,
            &mut ime,
            Cmd::Scroll {
                x: 50.0,
                y: 10.0,
                dx: -120.0,
                dy: 0.0,
            },
        )
        .expect("reverse scroll steps");
        assert_eq!(offset_x.get(), 0.0, "negative dx shrinks, clamped at 0");
    }

    /// Round 20.2 (decision 325): the resize-skip decision over raw
    /// client values — zero/negative areas and iconic state skip,
    /// live sizes proceed. Destroyed windows never reach this (the
    /// `hwnd_alive` break runs first); the skip is strictly the
    /// minimize case.
    #[test]
    fn skip_decision_covers_minimized_and_zero_areas() {
        assert!(!should_skip_resize_for_client_area(200, 150, false));
        assert!(!should_skip_resize_for_client_area(1, 1, false));
        assert!(should_skip_resize_for_client_area(0, 0, false));
        assert!(should_skip_resize_for_client_area(0, 150, false));
        assert!(should_skip_resize_for_client_area(200, 0, false));
        assert!(should_skip_resize_for_client_area(-3, 100, false));
        assert!(should_skip_resize_for_client_area(200, 150, true));
        assert!(should_skip_resize_for_client_area(0, 0, true));
    }

    /// Round 20.2 (decision 325): the OS bridge reads a live window
    /// as drawable — no false skip in normal operation (a hidden
    /// test window is not iconic and reports its real client area).
    #[test]
    fn live_window_reports_drawable_client_area() {
        let shell = hidden_shell();
        let hwnd = shell.hwnd();
        assert!(!window_minimized(hwnd), "test window is not iconic");
        let (w, h) = raw_client_size(hwnd);
        assert!(w > 0 && h > 0, "live client area is drawable, got {w}x{h}");
        assert!(!client_area_missing(hwnd), "live window never skips");
    }

    /// Scroll-pipeline regression (Task Studio follow-up): a real
    /// `WM_MOUSEWHEEL` wheel-down through the hidden shell moves an
    /// unbound `DataGrid`'s owner offset end to end — shell queue →
    /// `pump_events` → `take_cmds` → `drive_cmd` → hit-test → owner
    /// self-wire. No `bind_scroll` anywhere (the production controls
    /// never call it). Covers the attached-overlay swallow: the
    /// `Scrollbar` portal spans the whole target, and hit-testing
    /// must fall through its handlerless box to the rows beneath.
    #[test]
    fn wheel_down_scrolls_unbound_datagrid_end_to_end() {
        use ::windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
        use ::windows::Win32::Graphics::Gdi::ClientToScreen;
        use ::windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_MOUSEWHEEL};
        use oppa_controls::{DataGrid, DataGridProps, GridCellProps, GridColumn};

        #[derive(Clone, Debug)]
        struct Item;
        fn cell(_ctx: &Ctx, p: &GridCellProps<Item>) -> VNode {
            Div("wcell")
                .style(Style::new().w(p.width).h(p.height))
                .build()
        }

        let mut loop_ =
            DesktopLoop::new(400, 300, Box::new(FakeText), "Test").expect("headless loop builds");
        let rt = loop_.host().runtime();
        let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:wheel-grid"));
        coll.ingest((0..20).map(|_| Item).collect::<Vec<_>>());
        let props = DataGridProps::new(
            coll,
            vec![GridColumn {
                header: SharedString::from("A"),
                width: 320.0,
                cell,
            }],
            |_: &Item| 36.0,
        )
        .size(320.0, 200.0)
        .overscan(2)
        .debug("wgrid");
        let handle = loop_.mount("WGrid", props, DataGrid);
        let root = handle.root_instance();
        let offset = loop_
            .host()
            .instance_scroll(root)
            .expect("grid scroll handle");
        let list = oppa::find_retained_by_debug(loop_.host(), "wgrid")
            .into_iter()
            .next()
            .expect("grid node");
        assert_eq!(
            loop_.host().bound_scroll(list),
            None,
            "the grid never binds an explicit feed"
        );
        // The wheel point resolves through the overlay to the area.
        assert_eq!(
            loop_.host().scroll_target_at(60.0, 100.0),
            Some(list),
            "hit-testing falls through the handlerless overlay portal"
        );

        // Wheel-down one notch over the grid body (client 60,100 →
        // screen coords for the message pack).
        let mut shell = hidden_shell();
        let mut pt = POINT { x: 60, y: 100 };
        unsafe {
            let _ = ClientToScreen(shell.hwnd(), &mut pt);
        }
        unsafe {
            let wparam = WPARAM(((-120i16 as u16 as u32) << 16) as usize);
            let lparam = LPARAM(((pt.y << 16) | pt.x) as isize);
            SendMessageW(shell.hwnd(), WM_MOUSEWHEEL, Some(wparam), Some(lparam));
        }
        let _ = shell.pump_events();
        let cmds = shell.take_cmds();
        assert_eq!(cmds.len(), 1, "one wheel tick dispatches one cmd");
        let mut ime = WinImeMapper::new();
        let mut moved = false;
        for cmd in cmds {
            let out = drive_cmd(&mut loop_, &mut shell, &mut ime, cmd).expect("wheel drives");
            moved |= out.damaged;
        }
        assert!(moved, "wheel damaged the frame");
        assert_eq!(offset.get(), 120.0, "wheel-down moved the owner offset");
        // Rows rode viewport-relative under the pinned header: row 1
        // sits at 28 + 36 - 120 = -56 with the header at the top.
        let header = oppa::find_retained_by_debug(loop_.host(), "dgrid-header")[0];
        let hb = loop_.host().committed_box(header).expect("header laid out");
        assert!(hb.y.abs() < 0.01, "header stays put, got {}", hb.y);
        let row1 = oppa::find_retained_by_debug(loop_.host(), "dgrid-cell")
            .into_iter()
            .find(|id| {
                loop_
                    .host()
                    .committed_box(*id)
                    .is_some_and(|b| (b.y + 56.0).abs() < 0.01)
            })
            .expect("row 1 rode to y=-56");
        let _ = row1;
    }
}
