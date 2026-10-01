# App cookbook — wiring shipped seams into real apps

Status: current (Round 25.3, decision 338). Every recipe composes
shipped API — no new framework surface. Each names the in-repo
precedent it abbreviates; when prose and code disagree, the code
wins. Entry point: [getting started](../05-implementation/getting-started.md);
runnable reference: Task Studio (`crates/oppa-controls/src/studio.rs`,
`crates/oppa-controls/examples/task_studio.rs`,
`crates/oppa-app/tests/task_studio_e2e.rs`).

## 1. Navigation — a `NavStack` in a signal

Identity is the stack position, not the URI
(`crates/oppa/src/nav.rs`): hold the stack in a root-owned signal,
`match` on `current()`, push/pop from event handlers.

```rust
use oppa::{NavStack, Route};

// Root setup (once): seed from a root route.
let nav = ctx.signal(NavStack::new(Route::new("tasks").unwrap()));

// Going deeper (e.g. row press): params are ordered pairs.
let go = nav.clone();
let route = Route::new("detail").unwrap().param("id", "7").unwrap();
go.update(|mut stack| { stack.push(route); stack });

// Reading: the screen is a pure function of the top.
match nav.get().current() {
    Some(top) if top.name == "detail" => { /* detail screen */ }
    _ => { /* task list */ },
}

// Back: pop, and stay at root instead of underflowing.
nav.update(|mut stack| { stack.pop(); stack });

// Deep links are syntax over the same stack:
nav.update(|mut stack| { stack.push_link("oppa://tasks/detail?id=7").unwrap(); stack });
let back_out: String = nav.get().current().unwrap().to_path();
```

System back (Android `BACK`, desktop `ESC`) dismisses exactly one
layer — author popups, then IME composition, then focus, then the
runner pops the stack or exits (`nav.rs` "BackPress" docs). No
route tables in v1: segment validation and guards are app policy.

## 2. Persistence — `KvStore` settings, `NativeFs` files

Small prefs go through `KvStore` (`crates/oppa/src/store.rs` —
`get` returns `Ok(None)` when missing, never an error; empty keys
refuse loudly):

```rust
use oppa::store::{InMemoryKv, KvStore, NativeFs};

// Headless/tests: InMemoryKv. Shipping desktop: NativeFs rooted at
// your app-data dir (lexically jailed — `..` and absolute paths
// refuse loudly). Web: BrowserKv over localStorage
// (crates/oppa-web/src/storage.rs, `oppa:`-prefixed UTF-8 strings).
let mut kv = InMemoryKv::new();
kv.set("theme", b"dark".to_vec()).unwrap();
let dark = kv.get("theme").unwrap() == Some(b"dark".to_vec());

// Larger blobs (documents, exports): NativeFs::new(root)?.write(path, bytes).
let mut fs = NativeFs::new(app_data_dir).unwrap();
fs.write("notes/today.json", &bytes).unwrap();
```

Load once at startup into signals; save on change (dirty-gated —
see §7). Task Studio exports JSON/CSV through the same seam
(`export_tasks_json` / `export_tasks_csv` in `studio.rs`).

## 3. Async — `fetch_state` + `spawn_fetch`, paged `Collection`s

One state shape, rendered with a plain `match` (full contract:
[async-fetch](async-fetch.md)):

```rust
use oppa::FetchState;

let key = ctx.fetch_key("settings:avatar");
let avatar = ctx.fetch_state::<String>(key);
ctx.spawn_fetch(key, || download_avatar()); // native driver; Loading set synchronously
match avatar.get() {
    FetchState::Idle => { /* not asked */ }
    FetchState::Loading => { /* spinner */ }
    FetchState::Ready(url) => { /* image */ }
    FetchState::Failed(err) => { /* retry */ }
}
```

Lists stream pages into a `Collection` (`spawn_fetch_page` +
`Collection::new(&ctx.host().runtime(), key)` — the Task Studio
shape), rendered through `VirtualList` /
`DataGrid` with per-row granular updates (Round 23.1). On wasm
`spawn_fetch` refuses loudly — the platform binding resolves the
promise and writes the same keyed signal from the UI thread.

## 4. Form validation — signals + error text, no new props

Validation is app state, not control state: keep one error signal
per field (or per form), derive it in the submit handler, paint it
as plain text under the field.

```rust
let title = ctx.signal(SharedString::from(""));
let title_error = ctx.signal(SharedString::from(""));
// ... TextInput bound to `title` ...
if title.get().is_empty() {
    title_error.set(SharedString::from("A title is required"));
}
if !title_error.get().is_empty() {
    VNode::from(Text { text: title_error.get(), style: Text::body_secondary })
}
```

Masked inputs (`TextInputProps::masked`) suppress copy/cut
exfiltration — validate those from the signal, never by reading
pixels. Multi-line notes ride `TextArea` with the same pattern
(Task Studio's inspector is the precedent).

## 5. Theme — one signal recolors the catalog

`host.set_theme(ThemeMode::Dark)` flips every control in place
(state survives); custom paint reads `ctx.theme().tokens()`
(a tracked read — toggling re-renders automatically):

```rust
use oppa::{ThemeMode, ThemeTokens};
// Toggle (e.g. settings switch): host.set_theme(if dark { ThemeMode::Dark } else { ThemeMode::Light });
// Custom chrome: let t: ThemeTokens = ctx.theme().tokens(); Style::new().bg(t.surface)
```

Follow the OS instead: install a `SystemThemeSource` and call
`sync_system_theme()` (registry/`WM_SETTINGCHANGE` on Windows,
portal + watcher on Linux, `matchMedia` on web — Round 16.2).
Kitchen sink's platform tab wires both directions.

## 6. Transient feedback — `Toast`

Save-confirmations and errors ride the `Toast` control
(Round 25.2): controlled `open`, `Info`/`Success`/`Error` dot,
4 s auto-dismiss by default, sticky when `None`, `status`
semantics for screen readers. The anchor never claims presses —
a toast never blocks input (the inverse of `Modal`).

```rust
use oppa_controls::{Toast, ToastProps, ToastVariant};
let saved = ctx.signal(false);
let flag = saved.clone();
ctx.child("app::SavedToast", 40,
    &ToastProps::new("Saved", flag).variant(ToastVariant::Success),
    Toast)
// ... after a successful save: saved.set(true)
```

## 7. Window integration — `run_desktop_with`, dialogs, close veto

`run_desktop` is the no-hook path. When the app owns window
decisions, pass a configure closure — it runs after mount (host
signals exist) and after the platform backend installs (native
dialogs, OS clipboard, OS theme), so app configuration wins:

```rust
use oppa_app::{run_desktop_with, WindowOptions};
use std::rc::Rc;

run_desktop_with(options, props, MyApp, |loop_| {
    // Dirty-gated close: refuse WM_CLOSE / CloseRequested while
    // unsaved changes exist (Task Studio's "Save / Discard /
    // Cancel" modal is the full precedent).
    let dirty = loop_.host().runtime().signal(false);
    loop_.set_close_handler(Rc::new(move || !dirty.get()));
    // Runner-side poll: observe exit_requested, write the export
    // through the native save dialog, then ask for close (the
    // Task Studio example wires exactly this).
    loop_.set_poll_hook(Some(Box::new(move |loop_| {
        // ... write files, then loop_.host().request_close() ...
    })));
}).unwrap();
```

Components ask for close directly (`ctx.request_close()` --
File/Exit menu items, post-save exit): the flag drains once per
pump iteration through the veto consult, so dirty states re-raise
instead of exiting. Desktop-only drain (Windows + Linux); on
Android/Web the flag stays set (stated parity gap -- decision 342).

Native file dialogs ride the loop (`save_file_dialog` /
`pick_folder_dialog` -- COM pickers on Windows, portal/zenity on
Linux, dismissal-`None` on cancel and headless). Runtime chrome:
`set_title`, `set_icon`, `set_min_size` / `set_max_size`,
`set_fullscreen` through `WindowControl`.

## 8. Timers — `use_timeout` / `use_interval`

Per-component timers with cleanup ownership (unmount/re-render
cancels; suspended lifecycle freezes — Round 21.1):

```rust
// Inside a component: hide the hint 3 s after it shows.
let hint = ctx.signal(true);
let hide = hint.clone();
ctx.use_timeout(3000.0, move || hide.set(false));
```

`Scrollbar` idle fade (1200 ms) and `Toast` auto-dismiss are the
shipped precedents — copy their shape before inventing a new one.

## 9. Failure isolation — `ErrorBoundary`

Wrap panes that can fail (inspectors over untrusted data, plugin
content) so a child panic renders a fallback card instead of
crashing the host; retry re-evaluates the child:

```rust
use oppa_controls::{ErrorBoundary, ErrorBoundaryProps};
ctx.child("app::InspectorGuard", 50,
    &ErrorBoundaryProps::new(|ctx| inspector(ctx, &props)),
    ErrorBoundary)
```

Cleanups (`ctx.on_cleanup`) still run on unwind — timers and
subscriptions never leak through a caught panic (Round 18.2).
