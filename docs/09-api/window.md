# Window API

Status: current. Sources: `crates/oppa-app/src/lib.rs`
(`WindowOptions`, `DesktopLoop`, `run_desktop`), `crates/oppa/src/window.rs`
(`WindowControl`, `WindowIcon`); decisions 242, 316, 326, 338.

One call runs a desktop app (Windows/Linux); one hook owns window
decisions:

```rust
use oppa_app::{run_desktop, run_desktop_with, WindowOptions};

// No-hook path: open, mount, pump until close.
run_desktop(WindowOptions::new("My app", 800, 600), (), MyRoot).unwrap();

// Owned path: configure runs after mount, before the pump.
run_desktop_with(options, props, MyRoot, |loop_| {
    loop_.set_title("My app — edited");
    loop_.set_close_handler(std::rc::Rc::new(|| true)); // false vetoes
}).unwrap();
```

Runtime chrome on the loop (`set_title`, `set_icon(Option<WindowIcon>)`,
`set_min_size` / `set_max_size`, `set_fullscreen`, `set_close_handler`)
forwards through the `WindowControl` seam (Win32 `WM_SETICON` /
`WM_CLOSE`-queue on Windows, winit icon/control on Linux; quiet
headless no-ops — decision 316). Components request close via
`ctx.request_close()` (drains through the veto consult, desktop-only
— decision 342); runner-side per-iteration work installs through
`set_poll_hook` (Task Studio's exit writer is the precedent). Native file dialogs
(`save_file_dialog` / `pick_folder_dialog`, dismissal-`None`),
clipboard, and OS theme source ride the same loop (decisions 246,
314, 315). Renderer: GPU-first with loud CPU fallback
(`OPPA_RENDERER=cpu|gpu` override — decision 279).

Multi-window stays v2 (one window = one loop today). Per-platform
shells: [platforms](../06-platforms/windows/overview.md).
