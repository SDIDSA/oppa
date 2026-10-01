# Production Readiness Plan — Productizing Oppa (Phases 20–24)

Single source of truth across harness iterations and context wipes.
Baseline: Round 19.8 + Eyeball Fix-Up Round / Decision 323 (2026-09-30).
Prior readiness phases (Phases 8–19, Decisions 297–323) are complete and banked in
[`rounds.md`](rounds.md) and [`state.md`](state.md).

Per-round protocol ([`AGENTS.md`](../../AGENTS.md) §8):
1. Implement the round scope + headless/unit tests (loud failures over silent fallbacks, no invented architecture).
2. Run the verification gates (`$env:CARGO_INCREMENTAL="0"` on Windows):
   - `cargo fmt --all -- --check`
   - `cargo clippy --all-targets`
   - `cargo check --target wasm32-unknown-unknown -p oppa -p oppa-controls -p oppa-dom -p oppa-web`
   - `cargo test --workspace -j1`
3. Append the round delta to [`rounds.md`](rounds.md), update the snapshot at the top of [`state.md`](state.md), and check off `[x]` + log the execution line in this file.

Status legend: `[ ]` incomplete, `[x]` completed.

---

## Phase 20: Platform & Runner Hardening (Closing Phase 9–19 Mechanical Debt)

- [x] **Round 20.1: Android `MainEvent` Pattern Fix & Planning State Alignment (Decision 324)**
  Files: `crates/oppa-android-app/src/lib.rs`, `crates/oppa-android-app/src/surface.rs`, `docs/04-planning/state.md`, `docs/04-planning/backlog.md`, `docs/04-planning/current-sprint.md`.
  Fix `MainEvent::Resume { .. }` and `MainEvent::SaveState { .. }` struct-variant patterns against `android-activity 0.6.1` at `crates/oppa-android-app/src/lib.rs:185-191` and `crates/oppa-android-app/src/surface.rs:197` (the drift noted in Decision 323 verification). Reconcile the chronological snapshot ordering at the top of `state.md` (placing Decision 323 and Round 19.8 above Round 19.3) and update `backlog.md` / `current-sprint.md` to reflect Round 19.8 + Decision 323 completion.
  Verify: `cargo check --manifest-path crates/oppa-android-app/Cargo.toml --target aarch64-linux-android` compiles with 0 errors; workspace gates pass.

- [x] **Round 20.2: Windows Horizontal Wheel (`WM_MOUSEHWHEEL`) & Minimize-to-1×1 Guard (Decision 325)**
  Files: `crates/oppa-shell-win/src/win.rs`, `crates/oppa-shell-win/src/events.rs`, `crates/oppa-app/src/windows.rs`.
  Close the Round 9.2/9.3 `WM_MOUSEHWHEEL` backlog item and the Round 19.8 minimize thrash note:
  1. Handle `WM_MOUSEHWHEEL` (`0x020E`) in `oppa-shell-win`, translating the wheel delta into `WinCmd::Scroll { dx, dy: 0.0 }` → `InputEvent::Scroll { dx, dy: 0.0 }` (note Win32 `WM_MOUSEHWHEEL` positive delta is tilt-right, which scrolls content left: `dx = -(wheel_delta / 120.0) * LINE_PX`, matching vertical `ScrollArea` sign convention).
  2. In `crates/oppa-app/src/windows.rs`, guard the loop-bottom live-size poll (`IsIconic` or zero-area `GetClientRect` before clamping to `1x1`) so minimizing the window does not trigger a pointless `1x1` surface resize and GPU reconfigure.
  Verify: unit tests in `oppa-shell-win` prove `WM_MOUSEHWHEEL` dispatches horizontal scroll deltas that move a horizontal `ScrollArea`; `oppa-app` tests prove zero/minimized client rects do not reconfigure the surface to `1x1`.

- [x] **Round 20.3: Runtime Window Icon API (`set_icon`) (Decision 326)**
  Files: `crates/oppa-app/src/lib.rs`, `crates/oppa-shell-win/src/win.rs`, `crates/oppa-shell-linux/src/winit_loop.rs`, `crates/oppa-app/src/windows.rs`, `crates/oppa-app/src/linux.rs`.
  Close the Round 16.3 open question (`backlog.md`): add `WindowIcon { rgba: Vec<u8>, width: u32, height: u32 }` (with loud validation that `width > 0`, `height > 0`, and `rgba.len() == (width * height * 4) as usize`) and `WindowControl::set_icon` / `DesktopLoop::set_icon(Option<WindowIcon>)`. Wire to `WM_SETICON` (`ICON_SMALL` + `ICON_BIG` via `CreateIconIndirect` from BGRA pixels, destroying any previously owned `HICON` handles on replace/drop) on Windows and `winit::window::Icon::from_rgba` → `Window::set_window_icon` on Linux.
  Verify: headless `DesktopLoop` and shell tests verify valid RGBA icon installation, `None` icon reset, and loud refusal on invalid buffer lengths across Windows and Linux paths.

- [x] **Round 20.4: Android Sustained Damage-Loop Harness Mode (Decision 327)**
  Files: `crates/oppa-android-app/src/frameloop.rs`, `crates/oppa-android-app/src/lib.rs`.
  Build the Round 19.4 pre-condition round (`backlog.md`): add an instrumented sustained damage-loop run mode (`run_sustained_damage_loop`) in `oppa-android-app` that drives N consecutive single-control state flips through `host.run_until_idle()` → `builder.build_full` → `backend.paint` → `render_pixels`, recording steady-state incremental damage frame timings (`min`/`p50`/`p95`/`max` ms) into `oracle.txt` / `meta.txt` alongside the full-scene cold timings.
  Verify: unit tests cover damage-loop timing aggregation and formatting; `cargo check --manifest-path crates/oppa-android-app/Cargo.toml --target aarch64-linux-android` compiles cleanly.

---

## Phase 21: Component Timers, Scrollbar Wiring & Menu UX Completeness

- [x] **Round 21.1: Per-Component Reactive Timer & Interval Hooks (`ctx.use_timeout` / `ctx.use_interval`) (Decision 328)**
  Files: `crates/oppa/src/component.rs`, `crates/oppa/src/clock.rs`, `crates/oppa-app/src/lib.rs`, `crates/oppa-app/src/windows.rs`, `crates/oppa-app/src/linux.rs`, `crates/oppa-web/src/lib.rs`.
  Close the Round 17.2 per-component timer gap (`backlog.md`):
  1. Add `ctx.use_timeout(delay_ms, callback)` and `ctx.use_interval(period_ms, callback)` on `Ctx`, backed by host-tracked timer entries tied to the calling `ComponentId`.
  2. Automatically cancel component timers on re-render or unmount via the Round 18.2 cleanup lifecycle, and pause timer progression while `host.is_lifecycle_suspended()` (Round 18.3).
  3. Expose `host.next_timer_due_ms(now_ms)` and `host.tick_timers(now_ms) -> usize` and wire them into `DesktopLoop` (Windows `MsgWaitForMultipleObjectsEx` timeout / Linux `about_to_wait` `ControlFlow::WaitUntil`) and `WebApp::tick`.
  Verify: unit tests prove `use_timeout` fires once at `now + delay_ms`, `use_interval` fires repeatedly at `period_ms` cadence, unmounting a component cancels its active timers immediately, and `Paused`/`Suspended` lifecycle freezes timer firing.

- [x] **Round 21.2: `VirtualList` & `DataGrid` Scrollbar Attachment + Wall-Clock Idle Fade (Decision 329)**
  Files: `crates/oppa-controls/src/lib.rs`, `crates/oppa/src/component.rs`.
  Close both Round 17.2 open questions (`backlog.md`):
  1. Expose scroll offset, viewport height, and total content extent in `VirtualList` and `DataGrid` so both controls render the Round 17.2 interactive `Scrollbar` overlay (supporting thumb drag, track click page-scroll, and wheel synchronization).
  2. Upgrade `Scrollbar` auto-hide to support wall-clock idle fade (`idle_hide_ms`, e.g. `1200` ms after the last scroll or pointer event) using the Round 21.1 timer hook so the thumb disappears after inactivity without requiring another pointer move.
  Verify: testkit tests prove dragging the scrollbar thumb on `VirtualList` and `DataGrid` scrolls the virtualized row window to 100%, and advancing the host clock past `idle_hide_ms` hides the scrollbar automatically.

- [x] **Round 21.3: Menu UX Completeness — Hover Highlight, Viewport-Edge Clamping & Drag-Select (Decision 330)**
  Files: `crates/oppa-controls/src/menu.rs`, `crates/oppa-controls/src/lib.rs`, `crates/oppa/src/component.rs`.
  Close all three Round 17.1 open questions (`backlog.md`):
  1. **Hover highlight**: pointer move/enter over an enabled `MenuItem` updates the menu's highlighted row index (unified with Up/Down arrow navigation) and paints a themed hover/active row background.
  2. **Viewport-edge clamping**: clamp/flip anchored portal coordinates (`Menu`, `ContextMenu`, `Tooltip`) against the host viewport dimensions (`viewport_width`, `viewport_height`) so popups opened near the right or bottom window edges remain fully inside the viewport.
  3. **Drag-select into menu**: pressing to open a menu, dragging the pointer onto an enabled `MenuItem`, and releasing invokes that item and closes the menu.
  Verify: testkit tests verify row highlight follows pointer hover, menus/tooltips spawned near `(viewport_w - 5, viewport_h - 5)` clamp within viewport bounds, and press-drag-release activates the target menu item.

---

## Phase 22: Text Editing Ergonomics & Multi-Line `TextArea`

- [x] **Round 22.1: Clipboard & Selection Keyboard Shortcuts in `TextInput` / `DesktopLoop` (Decision 331)**
  Files: `crates/oppa/src/editing.rs`, `crates/oppa-controls/src/lib.rs`, `crates/oppa-app/src/lib.rs`.
  Complete desktop/keyboard text editing ergonomics across `EditSession`, `TextInput` / `UncontrolledTextInput`, and `DesktopLoop`:
  1. Selection & navigation shortcuts: `Ctrl+A` (select all), `Shift+Left` / `Shift+Right` / `Shift+Home` / `Shift+End` (extend selection), `Ctrl+Left` / `Ctrl+Right` (word-step caret move), and `Ctrl+Shift+Left` / `Ctrl+Shift+Right` (word-step selection extension).
  2. Clipboard shortcuts wired through `PlatformShell` (`read_clipboard` / `write_clipboard`): `Ctrl+C` (copy selected range; no-op when `masked: true`), `Ctrl+X` (cut selected range to clipboard and delete; no-op when `masked: true`), and `Ctrl+V` (paste clipboard text replacing active selection and advancing caret).
  Verify: headless tests prove `Ctrl+A`, `Ctrl+C`, `Ctrl+X`, `Ctrl+V`, and word-step selection round-trip through a mock/headless `PlatformShell` clipboard, and verify `masked: true` suppresses `Ctrl+C`/`Ctrl+X` cleartext exfiltration.

- [x] **Round 22.2: Multi-Line `TextArea` Control & Vertical Caret Navigation (Decision 332)**
  Files: `crates/oppa/src/editing.rs`, `crates/oppa-controls/src/lib.rs`, `crates/oppa-cpu/src/builder.rs`, `crates/oppa-dom/src/dom.rs`.
  Build a multi-line `TextArea` (and `UncontrolledTextArea`) control in `oppa-controls`:
  1. Extend `EditSession` (when configured as multi-line) to accept `\n` on `Enter`, compute visual line slices via `layout_text` (`oppa-linebreak` UAX #14 break opportunities), and support `ArrowUp` / `ArrowDown` (+ `Shift+ArrowUp` / `Shift+ArrowDown`) caret movement preserving the preferred horizontal x-anchor across lines.
  2. Render multi-line selection highlight rects and line-aware caret positioning in `FramePlanBuilder` (`oppa-cpu` / `oppa-vello`) and emit `<textarea>` in `oppa-dom`.
  Verify: unit and testkit tests verify `Enter` inserts newlines, `ArrowUp`/`ArrowDown` navigates across hard and wrapped lines preserving column affinity, and CPU/Vello/DOM emit multi-line selection and caret geometry.

---

## Phase 23: Reactive Scaling & Accessibility / Focus Polish

- [x] **Round 23.1: Per-Key Granular Subscriptions in `Store` & `Collection` (Decision 333)**
  Files: `crates/oppa/src/store.rs`, `crates/oppa-controls/src/lib.rs`.
  Close the M2 / Round 13.2 coarse-invalidation item (`backlog.md`):
  1. Add per-key / per-`RowId` dependency tracking in `Store` (`get_keyed`) and `Collection` (`get_row`) using lazy per-key signal slots so mutating an existing key/row via `Store::insert` or `Collection::update_row` (when sort/filter membership is unchanged) notifies only the subscribers reading that specific key/row.
  2. Keep structural mutations (`insert` of a new key, `remove`, `set_filter`, `set_sort`) notifying the collection-level query signal.
  Verify: in a 1,000-row `Collection` mounted inside `VirtualList`, updating a single visible row's value via `update_row` re-evaluates only that row's component slot (1 row render, 0 sibling row re-renders).

- [x] **Round 23.2: Keyboard Focus Ring & Modal Focus-Trap Completeness (Decision 334)**
  Files: `crates/oppa/src/style.rs`, `crates/oppa/src/component.rs`, `crates/oppa-controls/src/lib.rs`, `crates/oppa-cpu/src/builder.rs`, `crates/oppa-dom/src/dom.rs`.
  Productize keyboard navigation visibility and modal accessibility:
  1. Track keyboard-driven focus modality (`focus_visible` when focus moves via `Tab` / `Shift+Tab` / keyboard) on `ComponentHost` and render a themed focus ring (`ThemeTokens::focus_ring`) on focused interactive controls (`Button`, `Checkbox`, `Toggle`, `Select`, `Slider`, `Tabs`, `TextInput`, `TextArea`).
  2. Enforce modal focus trapping so `Tab` and `Shift+Tab` cycle strictly among focusable descendants inside an open `Modal` / `Dialog` portal without escaping to background controls.
  Verify: testkit tests verify `Tab` sets `focus_visible` and emits focus-ring borders on controls, pointer click clears `focus_visible`, and `Tab`/`Shift+Tab` inside an open `Modal` wraps strictly within the modal's focusable children.

---

## Phase 24: End-to-End Dogfooding — Production Reference Application

- [x] **Round 24.1: "Oppa Task Studio" — Real Multi-Workflow Cross-Platform Reference App (Decision 335)**
  Files: `crates/oppa-controls/src/studio.rs`, `crates/oppa-controls/src/lib.rs`, `crates/oppa-controls/examples/task_studio.rs`, `crates/oppa-app/tests/task_studio_e2e.rs`.
  Build a cohesive, task-focused reference application (beyond the widget-catalog `KitchenSinkApp`) that exercises real application architecture end-to-end:
  1. **Workspace & Data Grid**: a `Collection`-backed `DataGrid` / `VirtualList` of tasks with search filtering, column sorting, per-row status toggles (using Round 23.1 granular updates), and an attached draggable `Scrollbar` (Round 21.2).
  2. **Detail Inspector**: a split-pane editor with `TextInput` (title + keyboard clipboard shortcuts from Round 22.1), multi-line `TextArea` (description notes from Round 22.2), `Select` priority picker, `ContextMenu` row actions (Round 21.3), and `Tooltip` action hints.
  3. **Lifecycle & System Integration**: unsaved-changes dirty state wired into `DesktopLoop::set_close_handler` (opening a "Save / Discard / Cancel" confirmation `Modal` with Round 23.2 focus trap on `WM_CLOSE`), native `save_file_dialog` JSON/CSV export, live Light/Dark theme toggle, and an `ErrorBoundary` wrapping the inspector pane.
  Verify: headless E2E test suite in `oppa-app` / `oppa-testkit` drives the full workflow (create task → search/sort → edit title & multi-line notes → right-click context menu duplicate/delete → attempt window close with unsaved changes → modal veto → save via mock dialog → clean exit) across CPU, Vello, and DOM backends with 0 errors.

---

## Follow-up fix: Task Studio scroll pipeline (post-mission)

- [x] **Task Studio wheel/scroll coordinate fix (no new decision)**
  Files: `crates/oppa-shell-win/src/win.rs`, `crates/oppa/src/component.rs`, `crates/oppa/src/input.rs`, `crates/oppa/src/layout.rs`, `crates/oppa-controls/src/lib.rs`, `crates/oppa-app/src/lib.rs`, `crates/oppa-app/src/windows.rs`, `crates/oppa/tests/m5_input.rs`, `docs/04-planning/state.md`, `docs/04-planning/rounds.md`.
  Fix the live Task Studio scroll breaks: Win32 wheel-down now converts to positive `dy` at `WHEEL_LINE_PX` per notch; unbound `Scroll` targets self-wire to their handler owner's instance offset (clamped to committed content bounds; explicit `bind_scroll` still wins); `VirtualList`/`DataGrid` rows render viewport-relative with the grid header pinned at `0.0`, the grid row window over the body height, and grid extent covering rows + header. Live follow-up: the attached `Scrollbar` overlay portal spanned its whole target and swallowed wheel/press routing for the rows beneath — handlerless portals no longer claim hits through their own box (children still hit first; handler-carrying portals unchanged); the track press node spans a transparent 20px gutter with the painted 12px bar nested inside so hover summons without pixel-hunting (plan rects byte-identical; gutter presses page/drag, stated).
  Verify: Win32 wheel sign/scale test, grid wheel-to-rows test, real-`WM_MOUSEWHEEL`-through-shell-`drive_cmd` e2e into an unbound grid, core handlerless-portal hit test, gutter-hover test; updated viewport-relative window/extent expectations; full gates pass.

---

## Phase 25: Real-Dev Usability (Productization Last Mile)

Goal: a working developer outside this repo can start a desktop app in
minutes, learn app architecture from one cookbook, trust the API docs,
show transient feedback, and package per platform. Baseline: Phases
20–24 CLOSED (Decisions 324–335) + scroll-pipeline follow-up, all green.

Gap ledger (each verified against the tree 2026-09-30 — not relitigated
engine work):

- **D1. Desktop onboarding missing.** `getting-started.md` is web-wasm
  only; `run_desktop` (one call, decision 242) has no tutorial and no
  minimal hello example (`kitchen_sink`/`showcase`/`task_studio` are
  large). A new dev's first desktop `cargo run` is undocumented.
- **D2. API docs drifted from shipped code.** `09-api/widget.md` says
  "no generics" (false since 14.1/decision 311); `layout.md` says
  "grid is v2" (false — `DataGrid`, decision 310);
  `controls/overview.md` lists 4 controls, ~20 shipped;
  `application.md` is "proposed" with a `Runtime::new` sketch instead
  of `run_desktop`/`DesktopLoop`; `window.md` sketches `Win32Shell`
  instead of `WindowOptions`/`WindowControl`; `web-app.md` says
  "full-HTML swap" (false since 12.1/decision 307), "hardcoded demo"
  (false since 6.4/decision 275), "wasm text unmeasured" (false since
  6.3/decision 274).
- **W1. No transient-feedback widget.** `Modal` + `Tooltip` exist;
  nothing covers save-confirmations / error toasts. Every real app
  needs it.
- **C1. No app cookbook.** `NavStack` (G6), `KvStore`/`NativeFs` (G5),
  `fetch` (G7), validation, theme toggle, close-veto, timers,
  `ErrorBoundary` all ship, but no single doc wires them into app
  architecture. Task Studio proves it; no prose teaches it.
- **P1. Packaging open stories.** Windows installer + icon/version
  resource, Linux `.deb`, web offline/PWA, Android release signing
  machine-local (`packaging.md` marks each **open**). Recipes +
  checked-in artifacts owed.

- [x] **Round 25.1: Hello-desktop example + desktop quickstart (Decision 336)**
  Files: `crates/oppa-controls/examples/hello.rs`,
  `docs/05-implementation/getting-started.md`.
  A minimal counter app (signal + `Button` + `Text`, ~60 lines,
  `cargo run -p oppa-controls --example hello`) plus a "Desktop in
  5 minutes" section at the top of getting-started (scaffold outside
  the repo with path deps, `run_desktop`, Escape-exits, `OPPA_RENDERER`
  override, pointer to `oppa-testkit` for headless tests).
  Verify: `cargo check -p oppa-controls --examples`; fmt/clippy clean;
  headless testkit mount of the hello root shape.

- [x] **Round 25.2: Toast transient-feedback control (Decision 337)**
  Files: `crates/oppa-controls/src/lib.rs` (+ tests).
  Controlled `Toast` over the `Modal` viewport-portal precedent:
  `open: Signal<bool>`, `ToastVariant::{Info,Success,Error}`,
  `auto_dismiss_ms: Option<u64>` via the 21.1 `use_timeout` hook,
  manual dismiss button, `status` role semantics. DOM + CPU/Vello
  through the shared plan (no new `Tag` per decision 212).
  Verify: mounts when open / unmounts on dismiss; mock-clock
  auto-dismiss; variant styling structural pins; workspace gates green.

- [x] **Round 25.3: App cookbook (Decision 338)**
  Files: `docs/09-api/cookbook.md` (new), links from
  `getting-started.md` + `09-api/application.md`.
  One page wiring shipped seams into app architecture with
  compile-shaped snippets: navigation (`NavStack` in a signal +
  back-press chain), persistence (`KvStore`/`NativeFs` native +
  `BrowserKv` web), async (`fetch_state`/`spawn_fetch` + paged
  `Collection`), form validation (signals + error `Text`, no new
  props), theme toggle (`host.set_theme`), dialogs + close-veto,
  timers (`use_timeout`), failure isolation (`ErrorBoundary`).
  Every snippet names the Task Studio / showcase precedent it
  abbreviates. Verify: doc-only — fmt N/A; snippets eyeballed
  against signatures; links resolve.

- [x] **Round 25.4: API doc refresh (Decision 339)**
  Files: `docs/09-api/{widget,layout,application,window,web-app}.md`,
  `docs/09-api/controls/overview.md`.
  Correct exactly the D2 drift items (generics, DataGrid, full
  catalog list, `run_desktop`/`DesktopLoop`, `WindowOptions` +
  `WindowControl`, incremental DOM patching, `new_with_root`,
  web text measurement) with zero behavior claims beyond code.
  Verify: doc-only; every corrected claim cites its decision/code.

- [x] **Round 25.5: Packaging recipes (Decision 340)**
  Files: `docs/06-platforms/packaging.md` + checked-in artifacts
  (`packaging/windows/hello.rc` + version-resource note,
  `packaging/linux/cargo-deb` config for the hello shape,
  `packaging/web/sw.js` offline skeleton, Android release-signing
  checklist promoting the machine-local env vars to a documented
  rotation story).
  Verify: Windows `.rc` compiles shape eyeballed (no MSVC gate
  claimed); `cargo-deb` config parses if toolchain present else
  marked manual; sw.js served headlessly if Node present else
  marked manual. No invented green.

---

## Phase 26: Control API Pages + Proposed Close Channel

Docs round first (decision 341): only 4 of ~20 shipped controls
have pages. Proposed second (recorded, not built — rule 2): the
component-to-runner close channel the live veto flow needs.

- [x] **Round 26.1: Per-control API pages (Decision 341)**
  Files: `docs/09-api/controls/{text-input,select,tabs,modal,toast,datagrid,menu}.md`,
  `docs/09-api/controls/overview.md`.
  Seven pages in the `button.md` shape with every prop claim
  verified against code (two corrections caught: `on_change` is a
  pub field; `SelectProps` builds only through `new`). Overview
  indexes all eleven pages.
  Verify: doc-only — fmt clean; links resolve to existing files.

- [x] **Proposed 26.2: Component-initiated close + runner write bridge**
  (decided 2026-09-30 by user vote: desktop-only close flag +
  runner polling hook; full parity deferred). Shipped as Phase 27
  (decision 342) below.
  (open design questions, do not implement silently): `Ctx::request_close`
  host flag drained per pump iteration by the desktop runners
  (re-entering the veto protocol, so clean states exit and dirty
  states re-raise the modal); a runner-side bridge completing
  Save-to-disk-then-exit for the Task Studio veto modal. Parity
  questions open: Android finish-activity, web `window.close`
  gesture limits, polling vs component-side `NativeFs` write.
  Until decided, the live example stays veto-via-`run_desktop_with`
  and the E2E suite stays the full-flow proof.

---

## Phase 27: Live Close Completion (decision 342, user-voted scope)

Close the Proposed-26.2 loop: desktop-only component close flag +
runner polling hook + Task Studio live wiring. Android/Web parity
stays a named gap (flag stays set, never drained there).

- [x] **Round 27.1: Close flag, poll hook, live Task Studio (Decision 342)**
  Files: `crates/oppa/src/component.rs` (`HostInner::close_requested_flag`
  + `ComponentHost::request_close/take_close_request` + `Ctx::request_close`),
  `crates/oppa-app/src/lib.rs` (`DesktopLoop::poll_hook` +
  `set_poll_hook`/`run_poll_hook` take-call-restore),
  `crates/oppa-app/src/windows.rs` (pump-top drain through
  `drive_cmd(CloseRequested)`), `crates/oppa-app/src/linux.rs`
  (`about_to_wait` drain through `close_requested()`),
  `crates/oppa-controls/examples/task_studio.rs` (drift veto + poll
  writer through the native save dialog + `request_close`),
  `docs/09-api/cookbook.md` (§7), `docs/09-api/window.md`.
  Verify: host flag set/drain-once/re-arm + ctx forward (oppa);
  hook run/restore/remove + flag-through-veto (oppa-app);
  `cargo check -p oppa-controls --examples`; full gates green.

---

## Phase 28: Starter Template (decision 343)

Copy-out onboarding: a standalone crate a new dev copies, repoints,
and runs — proven both ways, not just claimed.

- [x] **Round 28.1: hello-desktop template (Decision 343)**
  Files: `templates/hello-desktop/{Cargo.toml,src/main.rs,README.md}`,
  root `Cargo.toml` (exclude), `docs/05-implementation/getting-started.md` (§0 pointer).
  Detached `[workspace]` + relative path deps; root exclude keeps
  workspace gates hermetic. Proven: `cargo check --manifest-path`
  in-checkout AND copied to a temp dir with absolute repointed
  paths (both exit 0; temp dir deleted after). The `hello_counter`
  testkit suite already proves the root shape headlessly.
  Verify: template checks green; fmt clean.

- [ ] **Proposed 28.2: hello-web template (blocked on design)**
  `WebApp` is the wasm-bound demo itself — a custom web app
  re-implements the host+dom+sheet+cursor rig (~100 lines,
  drift-prone) until `oppa-web` exposes a reusable harness
  (`new_with_root` exists but the binding layer around it does
  not). Options: extract a `WasmHost` rig crate, or bless a
  copy-paste `web/` scaffold with a drift test. Not built
  silently; getting-started §1-3 stays the web path.

---

## Phase 29: Reusable Wasm Host Harness (decision 344)

Unblock the web template + every future web app: extract the
mechanical rig from `WebApp` into a composable harness (plain
Rust — exported constructors cannot be generic over props).

- [x] **Round 29.1: WasmHost extraction (Decision 344)**
  Files: `crates/oppa-web/src/host.rs` (new: `WasmHost` + moved
  `commit_and_sync`/`sync_and_render`/`boot_host`/`boot_shell`/
  `system_prefers_dark`), `crates/oppa-web/src/lib.rs` (`WebApp`
  composes the shell, keeps nav/settings/images/demo bindings),
  `docs/09-api/web-app.md` (harness-first Host section).
  API: `mount_root` (one call), `boot`+`shell` (pre-mount wiring),
  `click/hover/key/text/fetch_start/fetch_resolve/tick/html/host`.
  Behavior identical by construction (verbatim moves) + 1 new
  proof test (`wasm_host_mounts_custom_root_without_demo`).
  Verify: 13 web + 5 sink tests green; full gates green.

- [x] **Round 29.2: hello-web template (Decision 345)**
  Files: `templates/hello-web/{Cargo.toml,src/lib.rs,web/index.html,web/bootstrap.js,README.md}`,
  root `Cargo.toml` (exclude), `docs/05-implementation/getting-started.md` (web pointer),
  `crates/oppa-web/src/host.rs` (`sync_system_theme` moved into the harness so every
  web app gets live OS-theme follow, `WebApp` delegates).
  Same counter shape as the desktop hello behind ~25 lines of
  bindgen glue; trimmed bootstrap (patch applier snapshotted from
  the reviewed applier with drift porting notes). Proven:
  wasm-target check in-checkout AND copied-out (both exit 0),
  `node --check` clean; wasm-bindgen + serve stay manual
  (machine-local CLI pin). Closes Proposed 28.2.
  Verify: template checks green; fmt clean; full gates green.

---

## Phase 30: Flake Closure + Template Boot Proof (decision 346)

Two open verifications, one round: the Round-29.2 flake watch
(shell-win live-Shift sampling) and first-browser proof of the
hello-web template.

- [x] **Round 30.1: Quiet-box gate + hello-web Edge smoke (Decision 346)**
  Flake: all 10 `win::pointer_tests` green on re-run, then the
  FULL workspace suite green (exit 0) -- watch CLOSED as
  environmental (live `GetKeyState` sampling + box key-state
  flapping; zero code contact; no framework change, per precedent).
  Smoke: hello-web template built to wasm (release) + wasm-bindgen
  0.2.128 pkg, served, driven in headless Edge via trusted mouse
  input -- `__oppaReady`, "Clicked 0 times" at boot, "Clicked 1
  times" after click, zero console/page errors. Probe + `web/pkg`
  + template `target/` + lockfiles deleted after (source-only
  templates; builds regenerate). First live-browser proof of a
  custom `WasmHost` root.
  Verify: workspace exit 0; smoke PASS errors=0; fmt clean.

---

## Phase 31: Emoji Word Rules (decision 347)

Close the backlog emoji/ZWJ row: pictograph units for word runs
(double-click + word steps), UAX #29 break semantics between
alnum and emoji, ZWJ join sequences glued.

- [x] **Round 31.1: Emoji word class + run rules (Decision 347)**
  Files: `crates/oppa/src/editing.rs` (`WordClass::Emoji`,
  `is_emoji_joiner` hand ranges, class-tracked run slot in
  `word_edges`, Emoji arm in double-click expansion),
  `docs/04-planning/backlog.md` (row closed).
  Proven: `word_rule_emoji_runs_and_zwj_glue` (three units in
  "a<popper>b", shared edges for steps, ZWJ family selects whole);
  pre-existing word tests unbroken (equivalence argued + green).
  Regional-indicator pairs stay separate (named follow-up).
  Verify: full gates green.

---

## Phase 32: Regional-Indicator Pairing (decision 348)

Complete the emoji story (Round 31 follow-up): UAX #29 WB15/WB16
flag pairing -- pairs from the last non-RI, EP/RI boundaries
break, ZWJ transparent to parity (WB11-class).

- [x] **Round 32.1: RI pair units (Decision 348)**
  Files: `crates/oppa/src/editing.rs` (`WordClass::Regional`,
  `is_regional`, parity run slot in `word_edges`, pair-aligned
  double-click arm).
  Proven: `word_rule_regional_pairs` (pairs, lone tails, letter
  and pictograph bounds, steps); Round-31 tests unbroken.
  ZWJ-inside-RI double-click selects the joiner alone
  (pathological input, stated; edges still step one unit).
  Verify: full gates green (modulo the known environmental Shift
  pair — re-check at record time).

---

## Phase 33: Cluster Caret Stepping (decision 349)

Close the backlog scalar-combining half: arrows step shaped
clusters (one press crosses base+combining), shaperless stays
scalar per decision 207.

- [x] **Round 33.1: Cluster-aware caret_boundary (Decision 349)**
  Files: `crates/oppa/src/editing.rs` (`caret_boundary` reads
  `shape_cached` cluster starts + text end, sorted/deduped, with
  the char_indices fallback), `docs/04-planning/backlog.md`.
  Proven: `caret_steps_clusters_not_scalars` over a merging fake
  (production-shaped clusters without a font stack) + shaperless
  scalar assertion; ASCII suite unbroken (clusters == chars
  there, identical bounds).
  Verify: full gates green.

---


## Phase 34: Onboarding Repair + CI (decision 350)

Goal-round-1 P0 bundle from the productization ledger
(`productization-gaps.md` G1–G5): one true first-build story plus
the repo's first CI. Baseline: Phase 33 CLOSED (349).

- [x] **Round 34.1: getting-started sync + wasm-bindgen pin + CI (Decision 350)**
  Files: `docs/05-implementation/getting-started.md`,
  `templates/hello-web/README.md`, `.github/workflows/ci.yml`,
  `docs/04-planning/productization-gaps.md` (new ledger),
  `docs/04-planning/rounds.md`, `docs/04-planning/state.md`.
  1. §0 desktop `hello()` verbatim the shipped template shape
     (`templates/hello-desktop/src/main.rs` ==
     `crates/oppa-controls/examples/hello.rs`).
  2. §1 web manifest = `templates/hello-web/Cargo.toml` +
     `oppa-macros` (§2 needs `#[derive(Props)]`) with a
     keep-in-sync rule + a path-deps/no-SemVer position line.
  3. wasm-bindgen pin lockfile-driven (workspace 0.2.128 today;
     copied-out templates generate their own lock) + exact
     `cargo install` command, in getting-started and the
     hello-web README.
  4. `.github/workflows/ci.yml`: fmt + clippy (no deny-warnings,
     pre-existing lints on record) + wasm-target check +
     `cargo test --workspace -j1` on push/PR.
  Verify: fmt clean; both starter templates check green
  (desktop host, web wasm32); clippy save pre-existing lints;
  full workspace suite green exit 0.

## Phase 35: Testkit Keyboard + Text Helpers (decision 351)

Goal-round-1 G6 from the productization ledger
(`productization-gaps.md`): promote every keyboard/text
hand-roll into the harness. Baseline: Phase 34 CLOSED (350).

- [x] **Round 35.1: `key` / `key_with` / `type_text` / `press_labeled` (Decision 351)**
  Files: `crates/oppa-testkit/src/lib.rs`,
  `crates/oppa-testkit/tests/keyboard_text.rs` (new),
  `crates/oppa-testkit/tests/focus_ring_trap.rs`,
  `docs/04-planning/productization-gaps.md`,
  `docs/04-planning/rounds.md`, `docs/04-planning/state.md`.
  1. `Harness::key` (Pressed + pump) and `key_with` (explicit
     modifiers) — the `tab()`/`shift_tab()` shapes, now one
     definition (the trap file forwards to them).
  2. `Harness::type_text` — headless `DesktopLoop::type_text`
     (same printable filter, same quiet miss, minus repaint;
     returns inserted count).
  3. `Harness::press_labeled` — tab-order + semantics-label
     press, loud on miss (promotes `press_labeled_button`).
  All compose public API only (crate rule: no test backdoors).
  Verify: 5 new tests green + trap suite green + fmt clean +
  clippy save pre-existing lints + wasm check clean + full
  workspace suite green exit 0.
## Execution Log

- Baseline: Round 19.8 + Eyeball Fix-Up Round / Decision 323 (2026-09-30). Next: Round 20.1 / Decision 324.
- 2026-09-30: Round 20.1 / Decision 324 done — Android `MainEvent::{Resume,SaveState} { .. }` patterns fixed (aarch64 green), state.md ordering reconciled, backlog + current-sprint updated for 19.8 + 323. Next: Round 20.2 / Decision 325.
- 2026-09-30: Round 20.2 / Decision 325 done — `WM_MOUSEHWHEEL` dispatches horizontal scroll (shell `HWheel` + app `dx` forwarding proven), minimize/zero-area skips the 1×1 reconfigure. Next: Round 20.3 / Decision 326.
- 2026-09-30: Round 20.3 / Decision 326 done — `WindowIcon` + `set_icon` on trait/loop/Win32 (owned HICON pairs) /Linux (desired+apply); 16.3 icon item closed. Next: Round 20.4 / Decision 327.
- 2026-09-30: Round 20.4 / Decision 327 done — sustained damage-loop harness (`DAMAGE_FRAMES=60`, cold+steady record into oracle/meta); 4 tests executed green headlessly + device check clean. Phase 20 CLOSED. Next: Round 21.1 / Decision 328.
- 2026-09-30: Round 21.1 / Decision 328 done — per-component timeout/interval hooks with cleanup ownership + lifecycle freeze, wired into desktop runners + web tick; 17.2 timer gap closed. Next: Round 21.2 / Decision 329.
- 2026-09-30: Round 21.2 / Decision 329 done — VirtualList/DataGrid attached Scrollbar (shared offset, 1200ms idle fade); 17.2 scrollbar items closed. Next: Round 21.3 / Decision 330.
- 2026-09-30: Round 21.3 / Decision 330 done — menu hover/clamp/drag-select (new declared-only DragRelease router event); 17.1 items closed. Phase 21 CLOSED. Next: Round 22.1 / Decision 331.
- 2026-09-30: Round 22.1 / Decision 331 done — session word-step/extend/masked + loop nav layer + HOME/END keys; clipboard round-trips + masked refusal proven. Next: Round 22.2 / Decision 332.
- 2026-09-30: Round 22.2 / Decision 332 done (recalibrated — control existed) — session visual-line nav + area publish + loop vline routing + CPU/DOM emission proofs. Phase 22 CLOSED. Next: Round 23.1 / Decision 333.
- 2026-09-30: Round 23.1 / Decision 333 done — per-key slots with zero version fan-out (root precision + fan-out tests proven; inline-attribution boundary documented as v2 question). Next: Round 23.2 / Decision 334.
- 2026-09-30: Round 23.2 / Decision 334 done — focus-visible modality + themed rings on 8 controls + proven modal trap; first testkit suite. Phase 23 CLOSED. Next: Round 24.1 / Decision 335.
- 2026-09-30: Round 24.1 / Decision 335 done — Task Studio reference app + runnable example + headless E2E green across CPU/Vello/DOM. Phase 24 CLOSED. MISSION COMPLETE (Decisions 324–335).
- 2026-09-30: Follow-up scroll fix done — Win32 wheel sign/scale, scroll owner self-wire, viewport-relative VirtualList/DataGrid rows + pinned header, handlerless-portal hit transparency, 20px scrollbar hover gutter. Full gates green.
- 2026-09-30: Round 25.1 / Decision 336 done — hello-desktop example + desktop quickstart in getting-started + headless hello_counter test. Next: Round 25.2 / Decision 337.
- 2026-09-30: Round 25.2 / Decision 337 done — Toast control (controlled open, variant dot, auto-dismiss/sticky, inert anchor) + Role::Status emitters (ARIA/AT-SPI/UIA); 5 tests green. Next: Round 25.3 / Decision 338.
- 2026-09-30: Round 25.3 / Decision 338 done — run_desktop_with loop hook (Windows + Linux runners, app config wins) + headless contract test + 9-recipe app cookbook. Next: Round 25.4 / Decision 339.
- 2026-09-30: Round 25.4 / Decision 339 done — API doc refresh (widget/layout/controls/application/window/web-app) + README status line. Next: Round 25.5 / Decision 340.
- 2026-09-30: Round 25.5 / Decision 340 done — packaging recipes (windows hello.rc, linux deb-metadata + desktop file, web sw.js node-checked) + packaging.md sections (offline, version resource, .deb, signing rotation). Phase 25 CLOSED. MISSION COMPLETE (Decisions 336-340).
- 2026-09-30: Round 26.1 / Decision 341 done — seven control API pages (text-input/select/tabs/modal/toast/datagrid/menu) prop-checked against code; overview indexes eleven. Proposed 26.2 recorded (request_close + write bridge open questions). Phase 26 CLOSED.
- 2026-09-30: Round 27.1 / Decision 342 done (user-voted scope) — host close flag + Ctx::request_close, Windows/Linux pump drains through veto, poll hook take-call-restore, Task Studio live veto + native-dialog writer; cookbook + window docs; 4 new tests. Phase 27 CLOSED.
- 2026-09-30: Round 28.1 / Decision 343 done — hello-desktop starter template (detached crate, root-excluded) proven in-checkout and copied-out; getting-started points at it. Proposed 28.2 recorded (hello-web blocked on reusable wasm harness). Phase 28 CLOSED.
- 2026-09-30: Round 29.1 / Decision 344 done — WasmHost harness extracted from WebApp (verbatim moves, behavior pinned by 13+5 tests green) + proof test + web-app.md; Proposed 28.2 now unblocked. Phase 29 CLOSED.
- 2026-09-30: Round 29.2 / Decision 345 done — hello-web template proven in-checkout and copied-out (wasm + node checks green); sync_system_theme into harness; closes Proposed 28.2. Workspace gate: 2 ENVIRONMENTAL failures (shell-win live-Shift sampling, rounds.md flake watch, no code contact). Phase 29 CLOSED (344-345).
- 2026-10-01: Round 30.1 / Decision 346 done — flake watch CLOSED (quiet-box full suite exit 0); hello-web Edge boot smoke PASS (ready + click-flip, zero errors; artifacts deleted after). Phase 30 CLOSED.
- 2026-10-01: Round 31.1 / Decision 347 done — emoji/ZWJ word rules (Emoji class + tracked runs + dbl-click arm, backlog row closed); workspace 2 known environmental Shift failures, rest green. Phase 31 CLOSED.
- 2026-10-01: Round 32.1 / Decision 348 done — RI pairing (Regional class + parity runs + pair dbl-click; stated ZWJ edge); workspace green exit 0 incl. former flakers. Phase 32 CLOSED.
- 2026-10-01: Round 33.1 / Decision 349 done — cluster caret stepping (shaped bounds, shaperless scalar; backlog row fully closed); 1 transient clipboard failure (green alone, box contention), rest green. Phase 33 CLOSED.
- 2026-10-01: Round 34.1 / Decision 350 done — onboarding repair (getting-started §0/§1 verbatim template shapes, lockfile-driven wasm-bindgen pin + install command, path-deps position) + first CI (`.github/workflows/ci.yml`, protocol gates). Ledger `productization-gaps.md` published (G1–G5 CLOSED). Gates: fmt clean, both templates check green, clippy save pre-existing lints, workspace exit 0. Phase 34 CLOSED. Next: Round 35 / Decision 351 (testkit key/type_text/press_labeled, G6).
- 2026-10-01: Round 35.1 / Decision 351 done — testkit key/key_with/type_text/press_labeled (public-API-only promotions) + 5-test keyboard_text.rs + trap-file forwarding. Ledger G6 CLOSED (all six goal P0s done). Gates: fmt clean, testkit green, clippy save pre-existing lints, wasm clean, workspace exit 0. Phase 35 CLOSED. Next dearest P1s: validation plumbing (G7), cargo-oppa new (G8), reload recipe (G9).
