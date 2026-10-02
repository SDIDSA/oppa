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

pub(crate) mod drive;
pub(crate) mod gpu;
pub(crate) mod ime;
pub(crate) mod window;

pub(crate) use drive::drive_cmd;
use gpu::{build_gpu_windows, reconfigure_gpu, WindowsGpu};
pub(crate) use ime::{drive_ime, WinImeMapper};
use window::{
    blit_rgba, client_area_missing, ensure_dpi_awareness, fit_client_to_scene, hwnd_alive,
    live_client_size, move_window_to_suggested, wait_timeout_ms,
};

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
mod dpi_tests;
#[cfg(test)]
mod faces_tests;
#[cfg(test)]
mod pointer_tests;
