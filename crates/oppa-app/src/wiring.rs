// ---------------------------------------------------------------------------
// Window chrome, dialogs, clipboard, theme, poll hook
// ---------------------------------------------------------------------------

use super::*;

impl DesktopLoop {
    /// Current output scale (device px per CSS px).
    pub fn dpr(&self) -> f32 {
        self.scene.dpr()
    }

    /// Re-bases the loop on a new device pixel ratio keeping the CSS
    /// viewport stable (Round 2.4, OQ-G10-2 — the runners call this
    /// on DPI/scale changes). The full rebase rule (layout config +
    /// builder + host viewport + surface refit + layout re-run, the
    /// loud zero/NaN refusal, the same-value no-op) lives on
    /// [`SceneCore::set_device_pixel_ratio`] — one path for desktop
    /// and Android density rebases alike.
    pub fn set_device_pixel_ratio(&mut self, dpr: f32) -> Result<(), String> {
        self.scene.set_device_pixel_ratio(dpr)
    }

    /// Mounts the root component and settles the first frame.
    pub fn mount<P: Props>(
        &self,
        name: &str,
        props: P,
        render: fn(&Ctx, &P) -> VNode,
    ) -> MountHandle<P> {
        let handle = self.scene.host().mount(name, props, render);
        self.scene.host().run_until_idle();
        handle
    }

    /// Installs the open-dialog backend (decision 425 — the G12
    /// shell seam's loop home: the platform glue calls this with
    /// the OS backend; see the field docs for the headless
    /// default).
    pub fn set_open_dialog(&mut self, dialog: Rc<RefCell<dyn FileDialog>>) {
        self.open_dialog = Some(dialog);
    }

    /// Borrows the open-dialog handle for app controls (decision
    /// 425 — the `FilePicker` handle rule: runners forward this
    /// `Rc` into control props so the trigger drives the same live
    /// backend the loop owns). `None` means unsupported (never a
    /// silent button).
    pub fn open_dialog_handle(&self) -> Option<Rc<RefCell<dyn FileDialog>>> {
        self.open_dialog.clone()
    }

    /// Whether an open-dialog backend is installed (decision 425 —
    /// probe before prompting so apps can disable open UI on
    /// platforms with no backend).
    pub fn has_open_dialog(&self) -> bool {
        self.open_dialog.is_some()
    }

    /// Installs the shortcut clipboard (decision 246 — the platform
    /// glue calls this with the OS backend; see the field docs for
    /// the default).
    pub fn set_clipboard(&mut self, clipboard: Box<dyn Clipboard>) {
        self.clipboard = clipboard;
    }

    /// Borrows the shortcut clipboard (test read-back, embedder
    /// peeks — same backend the shortcuts use, never a copy).
    pub fn clipboard(&mut self) -> &mut dyn Clipboard {
        &mut *self.clipboard
    }

    /// Installs the save-dialog backend (Round 16.1, decision 314 —
    /// the platform glue calls this with the OS backend; see the
    /// field docs for the headless default). Stored shared (decision
    /// 425) so [`DesktopLoop::save_dialog_handle`] forwards the live
    /// backend to app controls.
    pub fn set_save_dialog(&mut self, dialog: Rc<RefCell<dyn SaveFileDialog>>) {
        self.save_dialog = Some(dialog);
    }

    /// Installs the folder-picker backend (Round 16.1, decision 314
    /// — same glue rule as the save slot).
    pub fn set_folder_dialog(&mut self, dialog: Rc<RefCell<dyn FolderDialog>>) {
        self.folder_dialog = Some(dialog);
    }

    /// Borrows the save-dialog handle for app controls (decision 425
    /// — same forwarding rule as
    /// [`DesktopLoop::open_dialog_handle`]).
    pub fn save_dialog_handle(&self) -> Option<Rc<RefCell<dyn SaveFileDialog>>> {
        self.save_dialog.clone()
    }

    /// Borrows the folder-picker handle for app controls (decision
    /// 425 — same forwarding rule as
    /// [`DesktopLoop::open_dialog_handle`]).
    pub fn folder_dialog_handle(&self) -> Option<Rc<RefCell<dyn FolderDialog>>> {
        self.folder_dialog.clone()
    }

    /// Whether a save-dialog backend is installed (decision 422 —
    /// probe before prompting so apps can disable save UI on
    /// platforms with no backend instead of conflating "no backend"
    /// with "user dismissed").
    pub fn has_save_dialog(&self) -> bool {
        self.save_dialog.is_some()
    }

    /// Whether a folder-picker backend is installed (decision 422 —
    /// same probe rule as [`DesktopLoop::has_save_dialog`]).
    pub fn has_folder_dialog(&self) -> bool {
        self.folder_dialog.is_some()
    }

    /// Prompts the user for a save destination (Round 16.1, decision
    /// 314; decision 422 makes the missing-backend case loud):
    /// blocks on the native modal dialog and returns `Ok(Some)`
    /// with the picked path, `Ok(None)` when dismissed, and
    /// `Err` when no backend is installed
    /// (`PickError::Unsupported` — headless/Web/Android, never a
    /// silent `None`) or the backend fails (`PickError::Backend`,
    /// propagated, never swallowed to `None`). Callers needing the
    /// pre-422 graceful shape match `Ok(path)` explicitly.
    pub fn save_file_dialog(
        &mut self,
        options: FileDialogOptions,
    ) -> Result<Option<PathBuf>, PickError> {
        let Some(dialog) = self.save_dialog.as_ref() else {
            return Err(PickError::Unsupported("save dialog"));
        };
        dialog.borrow_mut().save(options)
    }

    /// Prompts the user for a destination directory (Round 16.1,
    /// decision 314; decision 422 — same loud-missing rule as
    /// [`DesktopLoop::save_file_dialog`]: picked path is
    /// `Ok(Some)`, dismissal is `Ok(None)`, missing backend and
    /// backend failures are `Err`).
    pub fn pick_folder_dialog(
        &mut self,
        options: FolderDialogOptions,
    ) -> Result<Option<PathBuf>, PickError> {
        let Some(dialog) = self.folder_dialog.as_ref() else {
            return Err(PickError::Unsupported("folder dialog"));
        };
        dialog.borrow_mut().pick(options)
    }

    /// Installs the OS theme reader (Round 16.2, decision 315 —
    /// the platform glue calls this with the OS source; see the
    /// field docs for the headless default).
    pub fn set_theme_source(&mut self, source: Box<dyn SystemThemeSource>) {
        self.theme_source = Some(source);
    }

    /// Forwards the OS theme into the reactive theme signal (Round
    /// 16.2, decision 315 — runners call this at startup and on
    /// every system theme-change message): queries the installed
    /// source and, when it reads a mode the app does not already
    /// have, sets it (every themed body re-renders, no instance
    /// re-created), settles, and repaints. Returns `true` when the
    /// mode changed (the runner presents). Quiet `false` with no
    /// source, on unreadable sources, or when the reading matches
    /// (a redundant OS message never spins a repaint). Paint
    /// failures are loud `Err`s like `repaint`.
    pub fn sync_system_theme(&mut self) -> Result<bool, String> {
        let mode = match self.theme_source.as_mut() {
            Some(source) => source.system_theme(),
            None => None,
        };
        let Some(mode) = mode else {
            return Ok(false);
        };
        if self.scene.host().theme().mode() == mode {
            return Ok(false);
        }
        self.scene.host().set_theme(mode);
        self.scene.host().run_until_idle();
        self.repaint()?;
        Ok(true)
    }

    /// Installs the runtime window-chrome control (Round 16.3,
    /// decision 316 — the platform glue calls this with the OS
    /// control; see the field docs for the headless default).
    pub fn set_window_control(&mut self, control: Box<dyn WindowControl>) {
        self.window_control = Some(control);
    }

    /// Retitles the window at runtime (Round 16.3, decision 316 —
    /// forwards into the installed control; quiet no-op with none,
    /// never a panic).
    pub fn set_title(&self, title: &str) {
        if let Some(control) = self.window_control.as_ref() {
            control.set_title(title);
        }
    }

    /// Constrains the resizable floor at runtime (`None` clears it
    /// — same forwarding/grace rules as
    /// [`DesktopLoop::set_title`]).
    pub fn set_min_size(&self, size: Option<(u32, u32)>) {
        if let Some(control) = self.window_control.as_ref() {
            control.set_min_size(size);
        }
    }

    /// Constrains the resizable ceiling at runtime (`None` clears
    /// it — same forwarding/grace rules as
    /// [`DesktopLoop::set_title`]).
    pub fn set_max_size(&self, size: Option<(u32, u32)>) {
        if let Some(control) = self.window_control.as_ref() {
            control.set_max_size(size);
        }
    }

    /// Toggles borderless fullscreen at runtime (state-preserving —
    /// same forwarding/grace rules as [`DesktopLoop::set_title`]).
    pub fn set_fullscreen(&self, fullscreen: bool) {
        if let Some(control) = self.window_control.as_ref() {
            control.set_fullscreen(fullscreen);
        }
    }

    /// Installs the runtime window icon (Round 20.3, decision 326 —
    /// `None` restores the OS default; same forwarding/grace rules
    /// as [`DesktopLoop::set_title`]).
    pub fn set_icon(&self, icon: Option<WindowIcon>) {
        if let Some(control) = self.window_control.as_ref() {
            control.set_icon(icon);
        }
    }

    /// Installs the app-level close veto (Round 16.3, decision 316
    /// — the brief's `host.set_close_handler`, homed on the loop:
    /// close is window lifecycle, and the runners consult it here).
    /// Returning `false` suppresses exit on `WM_CLOSE` /
    /// `CloseRequested` so unsaved-changes flows can mount instead
    /// of dying; `true` (and no handler at all) closes.
    pub fn set_close_handler(&mut self, handler: Rc<dyn Fn() -> bool>) {
        self.close_handler = Some(handler);
    }

    /// Whether a close request proceeds (Round 16.3, decision 316 —
    /// runners call this on `WM_CLOSE` / `CloseRequested`): the
    /// installed handler decides, `false` vetoing; no handler
    /// closes (pre-16.3 behavior, unchanged).
    pub fn close_requested(&self) -> bool {
        match self.close_handler.as_ref() {
            Some(handler) => handler(),
            None => true,
        }
    }

    /// Installs the runner-side poll hook (Round 26.2, decision 342 --
    /// the voted write bridge): runners invoke it once per pump
    /// iteration; `None` removes it. The hook observes runner-visible
    /// signals and drives runner-owned seams (dialogs, close) that
    /// component code cannot reach.
    pub fn set_poll_hook(&mut self, hook: Option<DesktopPollHook>) {
        self.poll_hook = hook;
    }

    /// Runs one poll-hook invocation (runner-side): take-call-restore,
    /// so a hook that blocks in a dialog backend cannot trip a
    /// runner borrow held across the call. No hook is a quiet no-op.
    pub fn run_poll_hook(&mut self) {
        if let Some(mut hook) = self.poll_hook.take() {
            hook(self);
            self.poll_hook = Some(hook);
        }
    }

    /// Whether the loop repaints continuously (benchmark mode):
    /// uncapped polling with a repaint + present every iteration
    /// instead of sleeping until input. Default `false`.
    pub fn continuous_repaint(&self) -> bool {
        self.continuous_repaint
    }

    /// Arms or disarms continuous repaint at runtime (the
    /// `WindowOptions::with_continuous_repaint` value lands here at
    /// runner startup; the `run_desktop_with` configure closure may
    /// override it before the pump starts).
    pub fn set_continuous_repaint(&mut self, enabled: bool) {
        self.continuous_repaint = enabled;
    }
}
