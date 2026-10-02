// ---------------------------------------------------------------------------
// Window chrome, dialogs, clipboard, theme, poll hook
// ---------------------------------------------------------------------------

use super::*;

impl DesktopLoop {
    /// Current output scale (device px per CSS px).
    pub fn dpr(&self) -> f32 {
        self.dpr
    }

    /// Re-bases the loop on a new device pixel ratio keeping the CSS
    /// viewport stable (Round 2.4, OQ-G10-2 — the Desktops call this
    /// on DPI/scale changes): layout config + builder follow, the
    /// host viewport rescales to CSS, the surface refits to device
    /// px, and layout re-runs (measure keys carry the DPR, so text
    /// re-shapes; the `set_viewport` dirt from decision 248 reflows
    /// boxes — nothing stale survives at the new scale). Same-value
    /// calls are a no-op. Non-finite or non-positive scales panic
    /// loudly (a scale is never zero/NaN — shells map raw OS values
    /// through `dpr_from_dpi`/`dpr_from_scale_factor` first).
    pub fn set_device_pixel_ratio(&mut self, dpr: f32) -> Result<(), String> {
        if !dpr.is_finite() || dpr <= 0.0 {
            panic!(
                "desktop loop: device pixel ratio {dpr} refused — scales are finite and positive"
            );
        }
        if dpr == self.dpr {
            return Ok(());
        }
        // CSS size is viewport_px / old_dpr (exact at 1.0, the only
        // previously reachable state — no drift by construction).
        let css_w = self.viewport.0 as f32 / self.dpr;
        let css_h = self.viewport.1 as f32 / self.dpr;
        self.dpr = dpr;
        let mut config = self.host.layout_config();
        config.device_pixel_ratio = dpr;
        self.host.set_layout_config(config);
        self.builder.set_dpr(dpr);
        self.host.set_viewport(css_w, css_h);
        let w = (css_w * dpr).round().max(1.0) as u32;
        let h = (css_h * dpr).round().max(1.0) as u32;
        self.viewport = (w, h);
        self.refit_surface()?;
        self.host.run_until_idle();
        Ok(())
    }

    /// Mounts the root component and settles the first frame.
    pub fn mount<P: Props>(
        &self,
        name: &str,
        props: P,
        render: fn(&Ctx, &P) -> VNode,
    ) -> MountHandle<P> {
        let handle = self.host.mount(name, props, render);
        self.host.run_until_idle();
        handle
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
    /// field docs for the headless default).
    pub fn set_save_dialog(&mut self, dialog: Box<dyn SaveFileDialog>) {
        self.save_dialog = Some(dialog);
    }

    /// Installs the folder-picker backend (Round 16.1, decision 314
    /// — same glue rule as the save slot).
    pub fn set_folder_dialog(&mut self, dialog: Box<dyn FolderDialog>) {
        self.folder_dialog = Some(dialog);
    }

    /// Prompts the user for a save destination (Round 16.1, decision
    /// 314): blocks on the native modal dialog and returns the
    /// picked path, or `None` when dismissed — or when no backend is
    /// installed (headless/web stay graceful, never a panic).
    /// Backend failures log loudly to stderr and settle `None` (the
    /// clipboard-transient precedent — a wedged picker must not kill
    /// the app; callers needing the reason drive the backend
    /// directly).
    pub fn save_file_dialog(&mut self, options: FileDialogOptions) -> Option<PathBuf> {
        let dialog = self.save_dialog.as_mut()?;
        match dialog.save(options) {
            Ok(path) => path,
            Err(e) => {
                eprintln!("oppa-app: save dialog failed: {e}");
                None
            }
        }
    }

    /// Prompts the user for a destination directory (Round 16.1,
    /// decision 314 — same settle rules as
    /// [`DesktopLoop::save_file_dialog`]: picked path, dismissal
    /// `None`, graceful headless `None`, loud-stderr backend
    /// failures).
    pub fn pick_folder_dialog(&mut self, options: FolderDialogOptions) -> Option<PathBuf> {
        let dialog = self.folder_dialog.as_mut()?;
        match dialog.pick(options) {
            Ok(path) => path,
            Err(e) => {
                eprintln!("oppa-app: folder dialog failed: {e}");
                None
            }
        }
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
        if self.host.theme().mode() == mode {
            return Ok(false);
        }
        self.host.set_theme(mode);
        self.host.run_until_idle();
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
}
