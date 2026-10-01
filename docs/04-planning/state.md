# Code State — M0 + M0b + M1 spike + verdict merge + M1 remainder (Reactive core + storage + scheduler + TextService + §9.2 verdict adopted into DESIGN + the window shell / real-IME / Vello-debug round + the TSF-aware re-run round + the text-store round + the IME verification round + the bidi/combining/ZWJ round + M2 reconciler/component model + M2b hot-reload harness + M3 layout engine + M4 CPU backend / FramePlan builder + image-diff oracle + M5 events / hit-testing / focus / Toggle end-to-end + M6 Vello backend / driver matrix + M7 DOM backend / parity corpus + M8 virtualization / transition evaluator / §9.4 stamp end-to-end + M9 reload product loop / fuzzer gate — renderer-freeze precondition green + M10 Android shell + AT-SPI emitters + GLES row — v1 HANDOFF in docs/HANDOFF-V1.md — v1 remainder (Gaps 1–6 closed without a phone) — phone round (Snapdragon 870: arm64 runs, Adreno oracle + frame cost + visible present, bet narrowed) — v1 close-out (present depth, Linux input, stale-APK rule) — Phases 8–18 readiness plan complete (readiness plan continues) — 19.0 planning reconciliation + first Linux typecheck (records + upstream note) — 19.1 Linux tests + first app runs + first live input proof — 19.2 Linux workspace gate + 15.2 flake-watch closure — 19.3 Windows live pass (agent-mechanical legs) — 19.4 Android assessment (equipment-bound) — 19.5 web sink Edge re-baseline + Firefox first boot — 19.6 Phase-19 close-out (fork decided) — 19.7 Vulkan fix-up attempt (port-proven, reverted, finding named) — 19.8 Vulkan teardown race root-caused + 7.4 matrix green — eyeball fix-up (decision 323: resize cursors + theme architecture) — 20.1 Android MainEvent pattern fix + planning state alignment (decision 324) — 20.2 horizontal wheel + minimize guard (decision 325) — 20.3 runtime window icon (decision 326) — 20.4 Android sustained damage-loop harness (decision 327) — 21.1 per-component timers (decision 328) — 21.2 scrollbar wiring + idle fade (decision 329) — 21.3 menu completeness (decision 330) — 22.1 clipboard & selection shortcuts (decision 331) — 22.2 TextArea vertical caret (decision 332) — 23.1 per-key subscriptions (decision 333) — 23.2 focus ring + modal trap (decision 334) — 24.1 Task Studio (decision 335)

Snapshot: 2026-10-01, Round 35 Testkit keys + text
(decision 351, final): `Harness::key` / `key_with` /
`type_text` (headless `DesktopLoop::type_text`: same filter,
quiet miss, returns count) / `press_labeled` (tab-order +
semantics, loud on miss); new `keyboard_text.rs` 5 green;
`focus_ring_trap.rs` forwards to the helpers. Gates: fmt clean,
testkit green, clippy save pre-existing lints, wasm clean,
workspace green exit 0. G6 CLOSED — all six goal P0s done.
Phase 35 CLOSED (351).

Snapshot: 2026-10-01, Round 34 Onboarding repair + CI
(decision 350, final): getting-started §0 hello() verbatim the
shipped template shape; §1 web manifest = hello-web manifest +
oppa-macros with a keep-in-sync rule + path-deps/no-SemVer line;
wasm-bindgen pin lockfile-driven (0.2.128 today) with the install
command (getting-started + hello-web README); first repo CI at
`.github/workflows/ci.yml` (protocol gates on push/PR).
Gates: fmt clean, both starter templates check green, clippy save
pre-existing lints, workspace green exit 0. G1–G5 CLOSED.
Phase 34 CLOSED (350).

Snapshot: 2026-10-01, Phase 33 Cluster Caret (decision 349,
final): caret_boundary steps shaped clusters (shaperless stays
scalar); backlog row fully closed. Gates: fmt clean, clippy save
pre-existing `FpsApp`; workspace 1 transient clipboard failure
(proven green alone, box contention), rest green incl. Shift pair.
Phase 33 CLOSED (349).

Snapshot: 2026-10-01, Phase 32 Regional Pairing (decision 348,
final): WordClass::Regional + WB15/WB16 parity runs + pair-aligned
double-click; regional test green. Phase 32 CLOSED (348).
Gates: fmt clean, clippy save pre-existing `FpsApp`, workspace
green exit 0 (incl. both former Shift flakers).

Snapshot: 2026-10-01, Phase 31 Emoji Word Rules (decision 347,
final): WordClass::Emoji + class-tracked runs + double-click arm;
backlog row closed (regional pairs named follow-up). Gates: fmt
clean, clippy save pre-existing `FpsApp`; workspace has the 2
known environmental Shift failures, everything else green.
Phase 31 CLOSED (347).

Snapshot: 2026-10-01, Phase 30 Flake Closure + Boot Proof
(decision 346, final): quiet-box full suite green (flake watch
CLOSED as environmental); hello-web Edge boot smoke PASS
(ready + click-flip, zero errors; artifacts deleted after).
Phase 30 CLOSED (346).
Gates: workspace green, fmt clean.

Snapshot: 2026-09-30, Phase 29 Round 29.2 hello-web template
(decision 345, final): templates/hello-web (bindgen glue over
WasmHost + trimmed bootstrap + README) proven in-checkout and
copied-out; sync_system_theme moved into the harness.
Gates: template/wasm/clippy/fmt green; workspace has 2
ENVIRONMENTAL failures (shell-win live-Shift sampling, no code
contact -- see rounds.md flake watch). Phase 29 CLOSED (344-345).

Snapshot: 2026-09-30, Phase 29 Wasm Host Harness (decision 344,
final): WasmHost extraction from WebApp (mount_root/boot/shell +
all bindings, demo state stays in WebApp); web-app.md harness-first;
proof test green. Phase 29 CLOSED (344).
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green (0 failures, web 14).

Snapshot: 2026-09-30, Phase 28 Starter Template (decision 343,
final): templates/hello-desktop detached crate (root-excluded)
proven in-checkout and copied-out; getting-started points at it.
Proposed 28.2 recorded (hello-web blocked on reusable wasm
harness). Phase 28 CLOSED (343).
Gates: template checks green (both ways), fmt clean.

Snapshot: 2026-09-30, Phase 27 Live Close Completion (decision 342,
final): host close flag + Ctx::request_close, desktop pump drains
through the veto consult, poll hook (take-call-restore) + Task
Studio live wiring (drift veto + native-dialog writer +
request_close); cookbook section 7 + window.md. Phase 27 CLOSED (342).
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green (0 failures).

Snapshot: 2026-09-30, Phase 26 Control API Pages (decision 341,
final): seven new pages under docs/09-api/controls/ (text-input,
select, tabs, modal, toast, datagrid, menu) in the button.md shape
with every prop claim checked against code; overview indexes all
eleven. Proposed (not built): ctx.request_close + runner-side
write bridge for live veto-modal completion. Phase 26 CLOSED (341).
Gates: fmt clean (no code touched).

Snapshot: 2026-09-30, Phase 25 Real-Dev Usability (decisions 336-340,
final): hello-desktop example + desktop quickstart (25.1), Toast
transient-feedback control + Role::Status emitters (25.2),
run_desktop_with loop hook + app cookbook (25.3), API doc refresh
across widget/layout/controls/application/window/web-app (25.4),
packaging recipes per platform (25.5). Phase 25 CLOSED (336-340).
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green (0 failures, incl. 5 toast, hello_counter,
desktop_with_hook).

Follow-up: 2026-09-30, Task Studio scroll pipeline fix
(post-24.1, no new decision): wheel events now move
`VirtualList`/`DataGrid` rows without an explicit `bind_scroll`
(owner self-wire, clamped to committed content bounds); Win32
wheel-down converts to positive `dy` at 120 px/notch; grid rows
render viewport-relative under the pinned header
(`header + tops - y`, header at `0.0`) with the row window over
the body height and extent covering rows + header. Live
follow-up: the attached `Scrollbar` portal spanned its whole
target and swallowed wheel/press routing for the rows beneath —
handlerless portals no longer claim hits through their own box
(children still hit first); plus a transparent 20px press/hover
gutter around the painted 12px bar so the chrome summons without
pixel-hunting (gutter presses page/drag, stated tradeoff).
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green.

Snapshot: 2026-09-30, Round 24.1 Task Studio (decision 335,
final): `studio.rs` reference app (grid + keyed inspector +
context actions + tooltips + theme + JSON/CSV export +
dirty-gated save modal); seed/key props (no cross-runtime
signals) + hooks outbox; runnable example (green build);
headless E2E green (create → search/sort → edit → dup/del →
CPU damage/pixels → Vello commit acceptance → veto → modal
save → mock-dialog file → DOM order). Phase 24 CLOSED (335).
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green.
MISSION COMPLETE: Decisions 324–335, Rounds 20.1–24.1, all
green with zero lingering failures.

Snapshot: 2026-09-30, Round 23.2 focus ring + modal trap
(decision 334): host `focus_visible` modality (Tab sets,
pointer clears); `ThemeTokens::focus_ring` + inset 2px rings on
all eight interactive controls (TabButton tints its band —
edge conflict refused by design); the 5.2 router trap proven
holding Cancel/OK both directions (backdrop entry stands per
decision 96); DOM inherits via inset-ring CSS. First testkit
integration suite (3 tests). Phase 23 CLOSED (333-334).
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green.
Next: Phase 24 — Round 24.1 Task Studio (final).

Snapshot: 2026-09-30, Round 23.1 per-key subscriptions (decision
333): lazy per-key slots (`Store::get_keyed`,
`Collection::get_row` as `Signal<Option<..>>`);
`Store::insert`/`Collection::update_row` write value + slot with
zero version fan-out (structural ops sync + broadcast);
Collection separated (versioned order + silent value/slot maps,
single truth, no overlay); row props carry the collection.
Proven: root precision (Store 2,1,1; bare 1-of-3), version
skipped while coarse bumps, 1,000-row window refreshes fresh
with version readers quiet. Boundary (probed): inline children
share the root dep set, so the owning window re-derives —
per-row child isolation is the named v2 question. M2 coarse
item closed.
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green.
Next: Round 23.2 focus ring + modal trap.

Snapshot: 2026-09-30, Round 22.2 TextArea vertical caret (decision
332, recalibrated — the control exists since 5.1): session wrap
config + preferred-x anchor + visual-line engine over
`layout_text` (hard-break completion, shaper-less fallback);
`TextArea` publishes `width - 16`; loop routes Up/Down on
multiline sessions only; per-line selection/caret proven on CPU
plan + DOM (Vello shares the plan; headless count zero by
design). 9 tests. Phase 22 CLOSED (331-332).
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green (two transient environmental flakes, re-runs green).
Next: Phase 23 — Round 23.1 per-key subscriptions.

Snapshot: 2026-09-30, Round 22.1 clipboard & selection shortcuts
(decision 331): session word edges (two-phase Ctrl-step),
extend ops, single-step vocabulary, masked copy/cut refusal,
native shift-anchor persistence (accumulate, collapse-first,
select-all re-seed); `TextInput` publishes masked;
`DesktopLoop::step_session_nav` consumes arrows/Home/End
(plain/shift/ctrl) on focused fields; `keys::HOME/END` +
Linux mapping added. 10 tests (6 session, 3 loop, 1 publish).
The brief's `read/write_clipboard` names map to the loop-owned
`Box<dyn Clipboard>`. UP/DOWN left for 22.2.
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green.
Next: Round 22.2 TextArea vertical caret (recalibrated — the
control already exists since 5.1).

Snapshot: 2026-09-30, Round 21.3 menu completeness (decision 330):
hover highlight unified with arrows (hover_move + mouse hit-test,
guarded), `clamp_popup_anchor` flips Menu/ContextMenu/Tooltip
inside the viewport (one-shot settled reads), declared-only
`EventKind::DragRelease` + `last_drag_release` in the router
(tap-gate verified — far releases never press) with list +
wrapper drag-select. 5 tests. Findings: settled-subscription +
child effects spin (one-shot pattern is the fix); right-held
single-gesture drag-select out of reach (tap-to-open). All 17.1
items closed; Phase 21 CLOSED (328-330).
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green.
Next: Phase 22 — Round 22.1 clipboard & selection shortcuts.

Snapshot: 2026-09-30, Round 21.2 scrollbar wiring + idle fade
(decision 329): `VirtualList`/`DataGrid` render the interactive
`Scrollbar` overlay sharing their instance offset (default on,
`scrollbar` builder to opt out; idle 1200ms) with viewport/extent
from settled box + `content_size`; `Scrollbar.idle_hide_ms` arms a
per-render 21.1 timeout (last-event-wins, hover/press keep chrome;
legacy `None` path byte-identical). 4 attachment tests (thumb-drag
to 100% both controls, clocked idle cycles) + `settle_clocked`
(M8 stepping — frozen-clock `run_until_idle` spins on live
transitions, characterized). Both 17.2 scrollbar items closed.
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green (one environmental os-32 lock, clean re-run).
Next: Round 21.3 menu UX completeness.

Snapshot: 2026-09-30, Round 21.1 per-component timers (decision 328):
`ctx.use_timeout` / `ctx.use_interval` backed by host-tracked
`HostTimer` entries (monotonic `TimerId`s, per-render ownership via
18.2 cleanups, frozen while suspended); `host.next_timer_due_ms` /
`host.tick_timers` (one-shots consumed, intervals snap with no
burst, dead-owner backstop sweep) wired into `DesktopLoop`
(Windows MsgWait horizon + settle tick, Linux `about_to_wait`
fire-first + `WaitUntil`) and `WebApp::tick`. 9 new tests (6 core:
once/cancel/cadence/unmount/freeze/rerender; loop fire+repaint;
2 waiter horizons). The 17.2 timer gap closed; Android pumps on
input ticks (long-press precedent, stated).
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green.
Next: Round 21.2 scrollbar attachment + idle fade.

Snapshot: 2026-09-30, Round 20.4 Android sustained damage-loop harness
(decision 327): `run_sustained_damage_loop` in `oppa-android-app`
(60 center-tap flips through settle/build/paint/readback on the CPU
arm; cold baseline excluded from steady min/p50/p95/max;
per-frame byte-drift guard; `damage.txt` + the record appended into
`oracle.txt`/`meta.txt` next to the cold timings). 4 tests executed
green headlessly against the workspace rlibs (device-crate is
host-uncompilable by ndk-sys — method recorded, harness deleted;
`check --tests` proves the device harness compiles). On-device
numbers await the 19.4 phone session. Phase 20 CLOSED (324-327).
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green, aarch64 lib+tests clean.
Next: Phase 21 — Round 21.1 per-component timers (`use_timeout`).

Snapshot: 2026-09-30, Round 20.3 runtime window icon (decision 326):
`WindowIcon::new` (loud zero-dim/length validation) +
`WindowControl::set_icon` / `DesktopLoop::set_icon(Option<..>)`;
Windows sends SMALL+BIG `WM_SETICON` via `CreateIconIndirect`
(BGRA swap, owned pairs destroyed on replace/drop), Linux stores
into `DesiredWindow` + `Icon::from_rgba` (attach replays, live
sets share `apply_icon`). 7 new/extended tests (validation,
scripted record, loop forward, GDI build + own-2/replace-2/reset-0,
desired store, winit convert). The 16.3 icon backlog item closed.
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm clean,
workspace green.
Next: Round 20.4 Android sustained damage-loop harness.

Snapshot: 2026-09-30, Round 20.2 Windows horizontal wheel &
minimize-to-1x1 guard (decision 325): `WM_MOUSEHWHEEL` handled in
`oppa-shell-win` (new `ShellEvent::HWheel`, `HWHEEL_LINE_PX = 120.0`,
`Cmd::Scroll { dx, dy: 0.0 }` with tilt-right-negative sign per the
brief formula — the Round 9.2/9.3 backlog item closed); the
loop-bottom live-size poll skips the resize when `IsIconic` or the
raw client area is zero (the 19.8 minimize-thrash note closed —
`hwnd_alive` break still first, settle still runs). 4 new tests
(shell dispatch both directions, app horizontal-feed drive, skip
decision table, live-window drawable proof). Gates: fmt clean,
clippy save pre-existing `FpsApp`, wasm clean, workspace green.
Next: Round 20.3 runtime window icon (`set_icon`).

Snapshot: 2026-09-30, Round 20.1 Android `MainEvent` pattern fix &
planning state alignment (decision 324): `MainEvent::Resume { .. }` /
`SaveState { .. }` struct-variant patterns at
`oppa-android-app/src/lib.rs:185-191` + `surface.rs:197`
(android-activity 0.6.1 shape drift from the Decision 323
verification — behavior unchanged, patterns only); the device-crate
aarch64 check is green again. Planning reconciliation: Decision 323 +
Round 19.8 snapshots restored above Round 19.3 (newest-first);
`backlog.md` closes the Vulkan successor (19.8) + the MainEvent drift
(20.1) and records the 323 non-goals as not-debt; `current-sprint.md`
marks Phase 19 CLOSED with Phase 20 opening as the hardening ledger.
Gates: fmt clean, clippy save pre-existing `FpsApp`, wasm check clean,
device-crate aarch64 clean, workspace tests green.
Next: Round 20.2 (`WM_MOUSEHWHEEL` + minimize-to-1x1 guard).

Snapshot: 2026-09-30, Eyeball fix-up round — resize cursors +
theme architecture (decision 323): `WM_SETCURSOR` claims the
framework cursor for `HTCLIENT` only (borders delegate to
`DefWindowProcW` — the resize arrows are back); theme owns
default ink + page background on every presenter (builder
`set_theme_mode`, themed caret fallback, desktop surface refit,
DOM body style + change-only patch stanza + bootstrap applier,
web/android runner publish); the sink's dead Dark toggle now
owns the host palette with tokenized furniture. Workspace green
(all suites, clippy save pre-existing `FpsApp`, fmt clean),
Edge sink 14-leg re-baseline pass on the fresh build. Device-
crate aarch64 check fails pre-existing (android-activity 0.6.1
`MainEvent` shape drift, untouched files — recorded, not worked
around). Fresh sink live on the desktop (Vulkan) for the user's
dark/border eyeball.
Next: the user's eyeball verdict; Phase 20 hardening ledger.

Snapshot: 2026-09-30, Round 19.8 Vulkan fix-up successor — teardown
race root-caused, hinstance kept, 7.4 matrix green (no new decision):
19.7's two findings are ONE race — `WM_CLOSE` destroys the HWND inside
`drive_cmd`, the loop-bottom poll misreads the dead window as a 1x1
resize, `get_capabilities` on the dead-HWND surface answers
`offered: []`, and the `GetDC` fallback dies 1400
(`ERROR_INVALID_WINDOW_HANDLE`, `is_window=false` — the new
diagnostics proving it in one live run). `hwnd_alive` guards exit
teardown cleanly (`window gone; exiting cleanly`, exit 0);
live-window failures stay FATAL; reconfigure retries once, loudly.
Live on the RTX 3060 Ti: `GPU path (vulkan / Immediate)`, two
`SetWindowPos` resizes reconfigure with zero failure lines,
`WM_CLOSE` exits 0 — empty-capabilities never occurs on a live
window. 7.4 matrix re-run green: app 50/50 + vello suites (Win),
29/29 + 52/52 (WSL), clippy/fmt/check clean both targets,
Wayland-GPU (`vulkan / Mailbox`, llvmpipe) + CPU/X11 softbuffer 20 s
stable each. Phase 19's mechanical round DONE; 19.6 fork stands
(Phase 20 hardening ledger next; human eyeball still invited).
Next: Phase 20 opens; the human-invited live session.

Snapshot: 2026-09-29, Round 19.3 Windows live pass — agent-mechanical legs
(no new decision; user eyeball invited for the human legs): live
`kitchen_sink` on the RTX 3060 Ti presents DX12/Immediate after a loud
Vulkan-surface fallback (`Vulkan requires raw-window-handle's
Win32::hinstance to be set` — named fix-up question; DX12 unaffected), the
full TSF chain engages S_OK on the live window, `WM_CLOSE` destroys cleanly
(16.3 live), and the REAL COM save + folder dialogs opened and dismissed
(`Ok(None)`) on the desktop — the 16.1 headless-by-construction legs are
now live-proven. Temp probe examples deleted after use (zz_ precedent);
zero source changes. Human-invited: IME manual pass, registry theme flips,
menu/tooltip eyeball, Windows Escape-at-root retest.
Next: Round 19.4 Android pass; Round 19.5 web sink pass (superseded — 19.4–19.8 + Decision 323 banked above; Phase 19 mechanical DONE).

Snapshot: 2026-09-29, Round 19.7 Vulkan fix-up attempt — ported, proven,
reverted with the deeper finding named (no new decision): 19.3's open
question answered (fps-demo carried the hinstance fix privately;
oppa-app never had it); the port was executed and PROVEN live — Vulkan
surface + adapter (RTX 3060 Ti) + renderer + cache + startup configure
all succeeded (`GPU path (vulkan / Immediate)`); it then exposed a
deeper finding — the resize-hook reconfigure saw EMPTY surface
capabilities (`offered: []`) on the same surface+adapter that had just
answered `present_modes`, and the CPU fallback failed `GetDC` (never
exercised on GPU boxes) fataling the run. Reverted (defaulting this box
to a fataling path is worse than the DX12 status quo); `oppa-vello`'s
configure error now names offered formats+alpha (§9 diagnostics, kept);
fps-demo re-proven serving Vulkan live today. Successor fix-up round
named: reconfigure lifecycle + mid-loop fallback hardening + re-port +
7.4 matrix re-run.
Next: the successor fix-up round; the human-invited live session.

Snapshot: 2026-09-29, Round 19.6 Phase-19 close-out (records only):
findings ledger — Vulkan-surface hinstance fix-up promoted (19.3);
flake watch CLOSED (19.2); Linux notes/test/gates all CLOSED
(19.0–19.2); live-OS agent-mechanical half CLOSED (19.3: save+folder
dialogs live `Ok(None)`, WM_CLOSE live, TSF S_OK); Firefox first boot
smoke banked (19.5); Android sustained loop pre-condition named (19.4,
equipment-bound). Fork decided from evidence: **one remaining
mechanical round (Vulkan hinstance fix-up + 7.4 matrix re-run), then
Phase 20 opens as the hardening ledger; the product-milestone fork
re-opens only after the human eyeball session completes the invited
legs.** Zero framework regressions found by the mechanical pass.
Next: the Vulkan fix-up round, then the human-invited live session.

Snapshot: 2026-09-29, Round 19.5 web sink — Edge re-baseline +
Firefox first boot (no new decision): wasm pkg rebuilt fresh (lock
pins wasm-bindgen 0.2.128; installed CLI matches — HANDOFF's 0.2.129
note is stale drift, lockfile authoritative). `node spike/web/sink.mjs`
on current headless Edge: **14/14 legs, pass=true, exit 0** —
OQ-SINK-1's 7.18 fix re-proven live (typing=Ada). Firefox (first
evidence in repo history): boots the sink clean via puppeteer-core
23.11.1 WebDriver BiDi (`browser: 'firefox'`) — ready, title, zero
console errors. Scope stated: boot smoke; full-leg automation is a
named BiDi rig extension. Safari unavailable on Windows, stated. Temp
probe deleted after use.
Next: Round 19.6 adoption close-out.

Snapshot: 2026-09-29, Round 19.2 Linux workspace gate + 15.2 flake-watch
closure (no new decision — mechanical gate + one fix-up): five workspace
manifests target-gated their `windows*` deps under
`[target.'cfg(windows)'.dependencies.windows]` (oppa-shell-win,
oppa-text-dwrite, oppa-uia, spike-textedit, oppa-fps-demo); oppa-uia
lib items/tests gated; spike-textedit bins split into thin
`#[cfg(windows)] mod imp { include!(...) }` wrappers + `_impl/` bodies
(include! expansions cannot carry inner doc comments — E0753 caught by
the Windows re-check, heads became plain comments). **First
full-workspace Linux check in repo history: green** (0 errors; Linux
clippy on the five crates clean; Linux fmt clean after installing the
missing rustfmt component). Windows unregressed: fmt/check/clippy 0 and
all 98 test suites OK. Fix-up: the 15.2 oppa-shell-win clipboard flake
reproduced in isolation (deterministic NULL handle + ERROR_SUCCESS from
GetClipboardData — sandbox clipboard virtualization answers writes,
never renders to raw readers); production `read_now` now names that
condition instead of printing a success string as an error, and the
test skips loudly per the 17.3 access-denied precedent — watch item
CLOSED.
Next: Round 19.3 Windows live pass.

Snapshot: 2026-09-29, Round 19.1 Linux test + first app runs (no new
decision — validation leg 1): on WSL Ubuntu, `cargo test -p oppa-app`
29/29 and `cargo test -p oppa-shell-linux` 52/52 green (matching the
16.3 table count; the X11 clipboard round-trip ran live) — the first
Linux test execution on this tree; `linux_demo` opened/presented/exited 0
(800×600, DejaVu 69 faces); `kitchen_sink` presented clean on both
llvmpipe-Vulkan and softbuffer paths (no error lines); and the first live
Linux input proof landed — synthetic Escape through real X11 exited the
sink cleanly while the no-Escape control survived to the 30 s timeout
(`host_inject_escape_or_input` → `escape_exits` verified live). No source
changes; the four standing typecheck notes stay closed per 19.0.
Next: Round 19.2 Linux workspace gate (manifest target-gating).

Snapshot: 2026-09-29, Round 19.0 Planning Reconciliation + First Linux
Typecheck (no new decision — records + mechanical gate): `backlog.md` and
`current-sprint.md` reconciled to the post-Phase-18 reality (all M0–M10 +
v1-remainder + close-out work closed; accepted-unimplemented items carried
forward — the live-OS proof passes, the 15.2 shell-win flake watch, 17.1
menu gaps, 17.2 VirtualList/DataGrid scrollbar wiring + idle fade, 16.3
window icon, emoji/ZWJ word rules, c3 re-baseline, per-key subscriptions,
generic-props manifests, web metric drift, `WM_MOUSEHWHEEL`, weak-GPU
sustained cost; the four target-gated linux.rs typecheck notes are now
CLOSED by this round, see below).
`PROJECT.md` "missing by design" corrected (all shipped); the M4–M6-era
scratch pad archived verbatim at `docs/12-archive/current-sprint-2026-09-26.md`.
Phase 19 (validation on a real application) opened in
`production-readiness-plan.md` with Round 19.0 as its mechanical gate.
First Linux build (WSL Ubuntu, Rust 1.98.1): `cargo check -p oppa-app
--all-targets` clean — 0 errors/0 warnings — closing the compile-correctness
substance of the 15.2/16.1/16.2/16.3 standing notes; Linux clippy shows one
test-only lint (toolchain-version gap, recorded); Linux test execution and
full-workspace Linux gates remain open (latter blocked upstream by
third-party `windows-future 0.3.2` not compiling on non-Windows targets —
recorded, not worked around). No source changes this round. No new decision
(mechanical gate + records round).
Next: Phase 19 live validation pass (brief in `production-readiness-plan.md`).

Snapshot: 2026-09-29, Round 18.3 Mobile App Lifecycle Hooks
(decision 322): `AppLifecycleState::{Active, Paused, Suspended}` exposed via
`ctx.lifecycle()` and `host.lifecycle()` / `host.set_lifecycle()`. When paused or
suspended (`host.is_lifecycle_suspended()`), host tickers (`tick_dwell`, `tick_flings`,
`fire_due_longpresses`) are suspended and Android event loops throttle frames.
`AndroidShell::note_lifecycle_and_sync` maps `LifecycleState` (`Resumed` -> `Active`,
`Paused` -> `Paused`, `Stopped`/`Destroyed` -> `Suspended`) into the host.
Core 148/148, shell-android 28/28, workspace 100% green; clippy gate holds save `FpsApp`; fmt clean; wasm check clean.
322. **Mobile lifecycle pause updates reactive state, throttles frames, and suspends tickers.**
Phase 18 CLOSED. Phases 8–18 COMPLETE.

Snapshot: 2026-09-29, Round 18.2 Component Lifecycle Cleanup Hooks
(decision 321): `ctx.on_cleanup` registers per-instance cleanups (`Box<dyn FnOnce()>`),
executed in reverse registration order (LIFO). Cleanups execute: (1) before an
instance re-runs (in `run_instance` for roots, and `child`/`try_child` for inline children);
(2) on unmount/eviction via `MountHandle::unmount` / `ComponentHost::unmount` / `evict_instance`
recursively through child instances; (3) on `HostInner::drop` across all surviving instances.
Core 147/147, workspace 100% green; clippy gate holds save `FpsApp`; fmt clean; wasm check clean.
321. **Component cleanups execute LIFO on re-run, unmount, eviction, and host drop.**
Next: Round 18.3 Mobile App Lifecycle Hooks. Phase 18 OPEN.

Snapshot: 2026-09-29, Round 18.1 UI Error Boundary Component
(decision 320): `ErrorBoundary` wraps child component render inside
`ctx.catch_unwind` / `ctx.try_child` (using `std::panic::catch_unwind(AssertUnwindSafe(...))`);
prevents panics from crashing the host or thread; resets `input_owner` hygiene
on unwind. Renders custom or default fallback card with `"error-boundary-retry"`
button; retry trigger signal re-evaluates the child. Reconciler `diff_node` updated
to sync `RetainedNode.debug` on in-place element diffs.
Controls 94/94, workspace 100% green; clippy gate holds save `FpsApp`; fmt clean; wasm check clean.
320. **Error boundaries catch child panics; host survives and recovers on retry.**
Next: Round 18.2 Component Lifecycle Cleanup Hooks. Phase 18 OPEN.

Snapshot: 2026-09-29, Round 17.3 Masked TextInput & Tooltips
(decision 319): `TextInput` and `UncontrolledTextInput` mask characters
with bullet glyphs (`•`) when `.masked: bool` is set, preserving underlying
`EditSession` cleartext and placeholder display. `Tooltip` component dwell-mounts
an anchored `Portal` card below the anchor node after 500ms continuous hover,
dismissing immediately on pointer leave or press; avoids infinite layout-generation
re-render loops via `committed_box_by_debug`.
Controls 92/92, workspace 100% green; clippy gate holds save `FpsApp`; fmt clean; wasm check clean.
319. **Masked inputs hide characters with bullets while preserving cleartext; tooltips dwell-mount without layout cycles.**
Next: Round 18.1 UI Error Boundary. Phase 17 CLOSED. Phase 18 OPEN.

Snapshot: 2026-09-29, Round 17.2 Interactive Draggable Scrollbar
(decision 318): `Scrollbar` overlay component over `ScrollArea` target
viewport (thumb height = max(24, viewport²/content), linear in clamped offset);
track click pages by viewport, thumb drag captures pointer and maps
linearly through travel/content ratio; arrows page when track holds
focus; auto-hide fades chrome out when disengaged and in on hover/press/scroll;
recolors with Light/Dark tokens. Core layout extended with `track_layout_generation`
and `settled_box_by_debug` for reliable post-layout overlay mount;
`layout_portal` extended to support child `x` and `absolute_y` offsets.
Controls 90/90, core 143/143; clippy gate holds save `FpsApp`; fmt clean; wasm check clean.
318. **Draggable scrollbars overlay scroll areas; thumbs follow content ratio.**
Next: Round 17.3 tooltips and password masking. Phase 17 OPEN.

Snapshot: 2026-09-29, Round 17.1 Menu primitives (decision
317): `Menu` / `MenuItem` / `ContextMenu` on anchored portals —
arrows/Enter keyboard, disabled + separators, cursor anchoring
on `on_context_menu`, one focus-derived blur edge for
Escape/outside/tab dismissals (rows deliberately ownerless so
focus never fragments mid-gesture). Controls 84/84; clippy gate
holds save `FpsApp`; fmt clean; wasm check clean. Details:
`rounds.md` 17.1 entry.
317. **Right-click menus mount, arrows move, Escape dismisses.**
Next: Round 17.2 draggable scrollbar. Phase 17 OPEN.

Snapshot: 2026-09-29, Round 16.3 Window chrome + close veto
(decision 316): `WindowControl` seam with runtime title/min/max/
fullscreen on `DesktopLoop` (quiet headless no-ops) plus a
loop-level close handler — `WM_CLOSE` queues instead of
destroying (Windows) and `CloseRequested` consults the veto on
both runners, so refusing handlers keep the pump running for
unsaved-changes modals. Core 142/142, app 50/50, shell-win
18/18, shell-linux 52/52; clippy gate holds save `FpsApp`; fmt
clean; wasm check clean. Details: `rounds.md` 16.3 entry.
316. **Windows retitle live; closes ask first.**
Next: Round 17.1 context menus. Phase 16 CLOSED.

Snapshot: 2026-09-29, Round 16.2 OS theme auto-detection
(decision 315): `SystemThemeSource` seam + `sync_system_theme`
(change-only, quiet otherwise) with registry/`WM_SETTINGCHANGE`
on Windows, portal query + `SettingChanged` watcher on Linux,
and `matchMedia` query + listener on web — every reading lands
in `host.set_theme` with controls recoloring in place. Core
141/141, app 48/48, shell-win 17/17, shell-linux 51/51, web
13/13; clippy gate holds save `FpsApp`; fmt clean; wasm check
clean. Details: `rounds.md` 16.2 entry.
315. **Apps follow the system dark mode, live.**
Next: Round 16.3 window management + close veto. Phase 16 OPEN.

Snapshot: 2026-09-29, Round 16.1 System file dialogs (decision
314): blocking save + folder seams (`FileDialogOptions` /
`FolderDialogOptions`, dismissal-`None`) with COM `IFileSaveDialog`
+ `FOS_PICKFOLDERS` backends on Windows and `SaveFile`-portal +
`--save`-zenity blocking impls on Linux; `DesktopLoop`
`save_file_dialog` / `pick_folder_dialog` with runner installs
and graceful headless `None`. Core 140/140, shell-win 13/13,
shell-linux 47/47, app 46/46; clippy gate holds save `FpsApp`;
fmt clean; wasm check clean. Details: `rounds.md` 16.1 entry.
314. **Apps ask where to save; cancel is data, never a crash.**
Next: Round 16.2 OS theme auto-detection. Phase 16 OPEN.

Snapshot: 2026-09-29, Round 15.2 DesktopLoop overlays + blink demand
(decision 313): `repaint()` sets selection + caret before
`build_full` (live frames finally highlight and bar) with
`last_plan` observability; `poll_blink`/`blink_tick_in_secs` drive
flip repaints on both runners (Windows horizon wakes ≤500ms,
Linux `about_to_wait` repaints + redraws). App 44/44; clippy gate
holds save `FpsApp`; fmt clean; wasm check clean. Shell-win live-OS
tests flaked environmentally twice (green in isolation + final
workspace green — see `rounds.md` 15.2 OQs). Details:
`rounds.md` 15.2 entry.
313. **Live frames highlight; the bar blinks without input.**
Next: Round 16.1 system file dialogs. Phase 15 CLOSED.

Snapshot: 2026-09-29, Round 15.1 Caret bar + blink clock
(decision 312): `CaretPaint` (absolute box-space x/y/h + ink) from
`focused_caret_paint` (focused + collapsed + blink-visible at the
active cluster's leading edge) through `FramePlanBuilder::set_caret`
(one 2px `Rect` on the field node) and a synced DOM `caret` div;
session blink is 500ms on/off with every caret/content op resetting
to solid-visible. Core 139/139, controls 75/75, m7 44/44; clippy gate
holds save `FpsApp`; fmt clean; wasm check clean. Details:
`rounds.md` 15.1 entry.
312. **Focused fields paint a blinking bar; taps reset it.**
Next: Round 15.2 DesktopLoop selection+caret wiring. Phase 15 OPEN.

Snapshot: 2026-09-29, Round 14.1 Generic Props in derive
(decision 311): `#[derive(Props)]` parses generics + where
clauses (lifetimes `'static`, types `Clone + 'static`, consts
free); all seven catalog generic props migrated; `ListProps<T>`
mounts and renders. Macros 21/21, controls 74/74; clippy gate
holds save `FpsApp`; fmt clean; wasm check clean. Details:
`rounds.md` 14.1 entry.
311. **Generics derive; lists render.**
Phases 8–14 CLOSED. Decisions 297–311 shipped.

Snapshot: 2026-09-29, Round 13.3 Paginated fetch + DataGrid
(decision 310): per-page FetchState + generation-guarded loads
streaming into collections; grid with column templates + pinned
header recycles through scroll/filter/pages. Fetch 12/12,
controls 73/73; clippy gate holds save `FpsApp`; fmt clean; wasm
check clean. Details: `rounds.md` 13.3 entry.
310. **Pages stream; headers pin; stale discards.**
Next: Round 14.1 generic Props. Phase 13 CLOSED.

Snapshot: 2026-09-29, Round 13.2 Collection data source +
virtualized list (decision 309): keyed-state `Collection` (stable
RowIds, filter/sort/page, Send writer via 13.1 rendezvous) feeding
`VirtualList` (prefix-sum windows, slot recycle, Update-only
rebinds). Store 10/10, controls 69/69; clippy gate holds save
`FpsApp`; fmt clean; wasm check clean. Details: `rounds.md`
13.2 entry.
309. **Windows materialize; handles stay stable.**
Next: Round 13.3 fetch/cache integration. Phase 13 OPEN.

Snapshot: 2026-09-29, Round 13.1 Concurrent worker preparation
(decision 308): every submit walks `Queued → Prepared → Ready →
Done` (BTreeMap prep table, id-ordered promotion, transition log
proves strict order); `spawn_fetch_with_retry` runs up to N tries
in one task run, exhaustion surfacing `Failed("<last> (retry
budget exhausted after N attempts)")`. Core fetch 9/9; clippy
gate holds save `FpsApp`; fmt clean; wasm check clean. Details:
`rounds.md` 13.1 entry.
308. **Stages walk in order; budgets exhaust distinctly.**
Next: Round 13.2 collection data source. Phase 13 OPEN.

Snapshot: 2026-09-29, Round 12.1 Incremental web DOM patching
(decision 307): `take_patch` keyed diff (swaps/attrs/sels/spacers/
removes/places) replaces the full-page swap — bindings return patch
JSON, both bootstraps apply by `data-pid`, converged typing is
quiet, background updates never address the focused field. m7
43/43, web lib 12/12, sink 5/5; clippy gate holds save `FpsApp`;
fmt clean; wasm check clean. Details: `rounds.md` 12.1 entry.
307. **Patches mutate; focus survives.**
Next: Round 13.1 concurrent worker preparation. Phase 12 CLOSED.

Snapshot: 2026-09-29, Round 11.2 Reactive theme system
(decision 306): `ThemeTokens` Light/Dark behind a host-level signal
+ `ctx.theme()` — every control paints from tokens (Light
pixel-identical, documented exceptions only); toggling recolors
Button/TextInput/Toggle/Modal in place with state surviving. Core
126/126, controls 64/64; clippy gate holds save `FpsApp`; fmt clean;
wasm check clean. Details: `rounds.md` 11.2 entry.
306. **One signal recolors the catalog, instances survive.**
Next: Round 12.1 incremental web DOM patching. Phase 11 CLOSED.

Snapshot: 2026-09-29, Round 11.1 Asymmetric padding, margins &
corner radii (decision 305): per-side pads/margins through the
layout engine (symmetric trees byte-identical) + `RRect.radii`
painted per-vertex on CPU/Vello/DOM through one shared clamp rule.
m3 50/50, m4 18/18, m6 24/24, dom lib 11/11, testkit 10/10; clippy
gate holds save `FpsApp`; fmt clean; wasm check clean. Details:
`rounds.md` 11.1 entry.
305. **Every side and corner styles independently.**
Next: Round 11.2 reactive theme system.

Snapshot: 2026-09-29, Round 10.2 Touch scroll momentum & inertia
(decision 304): windowed release velocity + exponential decay flings
ticked explicitly (never frame demand — long-press doctrine),
grabbing cancels, Windows settle pass ticks + repaints. `scroll_drag`
7/7 MockClock-deterministic; clippy gate holds save `FpsApp`; fmt
clean; wasm check clean. Details: `rounds.md` 10.2 entry.
304. **Flings decay across ticks until settled.**
Next: Round 11.1 asymmetric box model. Phase 10 CLOSED.

Snapshot: 2026-09-29, Round 10.1 Pointer drag scrolling on
`ScrollArea` (decision 303): per-pointer `ScrollDrag` states arm past
tap slop in feed-bound containers and stream move deltas through the
shared wheel-feed rule (native direction, no dead zone); scrolled
drags never tap, sub-slop taps and unbound containers behave exactly
as before. New `scroll_drag` 4/4; clippy gate holds save `FpsApp`;
fmt clean; wasm check clean. Details: `rounds.md` 10.1 entry.
303. **Touch drags scroll; child taps stay silent.**
Next: Round 10.2 touch scroll momentum.

Snapshot: 2026-09-29, Round 9.3 2D wheel scrolling & trackpad pan
(decision 302): `scroll_x` per-instance signal + `bind_scroll_x`
feed — `dx` accumulates clamped to content bounds when bound,
ignored when not; vertical path untouched. New `scroll_x` 4/4, app
40/40; clippy gate holds save `FpsApp`; fmt clean; wasm check clean.
Details: `rounds.md` 9.3 entry.
302. **Horizontal deltas scroll bound containers, clamp, else quiet.**
Next: Round 10.1 pointer drag scrolling. Phase 9 CLOSED.

Snapshot: 2026-09-29, Round 9.2 Mouse button taxonomy
(decision 301): `PointerButton` on Down/Up + `SecondaryPress`/
`ContextMenu` router events (secondary taps never fire primary
presses; auxiliary quiet; chords never steal captures) + Win32 R/M
classification and Linux left/right/middle mapping with runner
forwarding. New `mouse_buttons` 5/5, app 39/39, shell-win 9/9,
shell-linux 42/42; clippy gate holds save `FpsApp`; fmt clean; wasm
check clean. Details: `rounds.md` 9.2 entry.
301. **Right-clicks dispatch menu events, never presses.**
Next: Round 9.3 2D wheel scrolling.

Snapshot: 2026-09-29, Round 9.1 Event-driven Windows event loop
(decision 300): `MsgWaitForMultipleObjectsEx` + one-shot waitable
timer replace the 8ms spin — settled loops block until input,
live holds/worker/transitions tick at 8ms with a message-less settle
pass. App 38/38, shell-win 8/8; clippy gate holds save `FpsApp`; fmt
clean; wasm check clean on web crates. Details: `rounds.md` 9.1 entry.
300. **Idle blocks; active work ticks.**
Next: Round 9.2 mouse button taxonomy.

Snapshot: 2026-09-29, Round 8.3 Mouse cursor shape infrastructure
(decision 299): `Style.cursor` + `hover_cursor` inherit walk + shell
seam with Win32 `WM_SETCURSOR` stock mapping and winit 1:1 mapping +
DOM `cursor:` class rule; enabled Button/Toggle/Checkbox show the
hand, fields the I-beam, disabled the arrow. Core 125/125, controls
63/63, dom lib 10/10, shell-win 7/7, shell-linux 41/41, app 37/37;
clippy gate holds save `FpsApp`; fmt clean; wasm check clean.
Details: `rounds.md` 8.3 entry.
299. **Hover shows the hand on controls, the I-beam in fields.**
Next: Round 9.1 event-driven Windows event loop. Phase 8 CLOSED.

Snapshot: 2026-09-29, Round 8.2 Mouse drag-selection & word/line
selection (decision 298): router multi-click chains (1 caret, 2 word,
3+ hard line) + field `on_drag` streams into `drag_x` + themed
`SELECTION_FILL` rects on the shared plan (CPU/Vello) and `sel` divs
(DOM) through one `selection_rects` rule; Windows Shift+Click wired
end to end. Core lib 124/124, controls 61/61, dom 36/36, app 36/36;
clippy gate holds save `FpsApp`; fmt clean; wasm check clean.
Details: `rounds.md` 8.2 entry.
298. **Drags select, double-clicks word, triple-clicks line — every
presenter highlights.**
Next: Round 8.3 cursor-shape infrastructure.

Snapshot: 2026-09-29, Round 8.1 Tap-to-caret & shaper-fed hit testing
(decision 297): router publishes the tap Up point + modifiers
(`last_press_*`; keyboard clears to the `caret_to_end` fallback) with
`text_origin_under` + `ensure_session_shaper` helpers — TextInput and
TextArea map taps via `click_x` and Shift+Click via `shift_click_x`.
Core lib 123/123, controls 58/58 (incl. 4 new tap/shift/keyboard/area
pins); clippy gate holds save `FpsApp`; fmt clean; wasm check clean.
Details: `rounds.md` 8.1 entry.
297. **Taps land on cluster boundaries; Shift extends.**
Next: Round 8.2 drag-selection & word/line selection.

Snapshot: 2026-09-29, Round 7.21 Full-viewport modals + anchored
select popups (decision 296): portals default to viewport height
with parent-origin offset anchoring and a fill-only height-hint
channel — Modal dims 800x600 with a both-axis-centered card and
zero-gap closed state; Select keeps its 32px box with the list in
an under-box popup that shifts nothing. Core m3 48/48, controls
54/54 (incl. the two eyeball-fix pins); clippy gate holds save
`FpsApp`; fmt clean. Details:
`rounds.md` 7.21 entry.
296. **Overlays cover the window; popups anchor to parents.**
Next: user eyeball — modal dim offset bug found + fixed
(`reposition` skips viewport portals; anchored ones still
track); shaping crash found + fixed (control runs zero-width);
relaunch eyeball all-pass (dim origin, popup anchor, newline
field), clean exit 0.

Snapshot: 2026-09-29, Round 7.20 Live modal-loop resize
(decision 295): `WM_SIZE` runs the runner's resize hook
synchronously from `wndproc` (state behind `Rc<RefCell>`, never
borrowed across the pump) — border drags reflow + repaint +
re-present live with zero DWM stretching; the loop-bottom
live-size poll stays as fallback. Shell 6/6, app 35/35; clippy
gate holds save `FpsApp`; fmt clean. Details: `rounds.md` 7.20
entry.
295. **Border drags present live frames on Windows.**
Next: user eyeball — DONE (live reflow, slider drags, pressed
states all pass; nothing broken).

Snapshot: 2026-09-29, Round 7.19 Windows pointer drag pipeline
(decision 294): discrete Down/Move/Up/Cancel flow Win32→router
(SetCapture-held drags, CAPTURECHANGED tripwire, legacy Click tap
+ quiet Drag kept) — Slider-shaped drags update from pointer x
through `drive_cmd`, shell pump order pinned, spike folds Moves
into `drag_x`. App 34/34, shell-win 5/5; clippy gate holds save
`FpsApp`; fmt clean. Details: `rounds.md` 7.19 entry.
294. **Windows drags ride the discrete pointer pipeline.**
Next: user eyeball.

Snapshot: 2026-09-29, Round 7.18 Placeholder-typing fix (decision
293): OQ-SINK-1 closed — U8 text events fall back to the owning
instance session (explicit binds win unchanged), and DOM fields
render `value=""` with a native `placeholder` attribute (the
placeholder is never the value). Pinned control tests green
(51/51), new DOM placeholder test, Edge `sink.mjs` flips to a
full 14/14 pass with zero errors. Controls 51/51, dom 35/35, web
16/16; clippy gate holds save `FpsApp`; fmt clean. Details:
`rounds.md` 7.18 entry.
293. **Placeholder typing feeds through the session fallback,
and placeholders render natively.** Next: user eyeball.

Snapshot: 2026-09-29, Round 7.17 Sink-on-web (decision 292):
`KitchenSinkApp` mounts through `WebApp::new_with_root` (new
`WebApp::new_sink` wasm export + `host()` escape hatch) with 4
headless web tests (roles/fields/vectors, check/toggle, select,
all-tabs+modal+platform+pixels) and a real Edge pass
(`spike/web/sink.mjs` over rebuilt `web/pkg` + `web/sink.html`):
13/14 legs green, zero console errors, demo page still boots.
The miss found OQ-SINK-1 (typing into an empty+placeholder
`TextInput` drops quietly — framework-level, pinned by an ignored
control test; on DOM the placeholder renders AS the input value).
Web 16/16, controls 49/49 + 1 ignored; clippy gate holds save
`FpsApp`; fmt clean. Details: `rounds.md` 7.17 entry.
292. **The sink runs on web except placeholder typing, which is a
named silent-drop bug (OQ-SINK-1).** Next: the OQ-SINK-1 fix
round (placeholder-typing contract), then user eyeball.

Snapshot: 2026-09-29, Round 7.16 Vectors (decision 291):
first-class `DrawOp::Path` + `Tag::Path` + `Path` component on all
three renderers (CPU parses SVG data backend-side, Vello encodes
via kurbo, DOM renders inline `<svg>`), SVG asset decoding in
`oppa-image` (resvg, no font stack — `<text>` refuses loudly), and
vector Checkbox check + Select chevron in `oppa-controls`. Core
123/123, controls 49/49, testkit 9/9, cpu/vector 5/5, vello/vector
4/4, image 8/8 (+1 e2e), dom 34/34, web 12/12, app 32/32; clippy
gate holds save `FpsApp`; fmt clean. User eyeball still invited.
Details: `rounds.md` 7.16 entry.
291. **Vectors are first-class display-list citizens on every
renderer, and SVGs decode into the image pipeline.** Next: user
eyeball (objective's last item) + the 7.16 open questions below.

Snapshot: 2026-09-28, Round 7.15 Tactile (decision 290):
pressed deepens Primary (`Ctx::pressed`, one tint), vector-check
squares, steppers flank the trackbox (root untouched); `chrome`
inlined. App 32/32, controls 48/48, testkit 9+2; clippy gate
holds save `FpsApp`; fmt clean. User eyeball still invited.
Details: `rounds.md` 7.15 entry.
290. **Controls give tactile feedback and place steppers beside
the rail.** Next: user eyeball (objective's last item).

Snapshot: 2026-09-28, Round 7.14 Segoe eyeball (decision 289):
sink Form tab rendered through the real DirectWrite/Segoe stack —
aligned, no overflow, chevron fallback resolves; temp rig fully
reverted. Controls 48/48; fmt clean. User eyeball still invited.
Details: `rounds.md` 7.14 entry.
289. **The catalog holds under Segoe metrics.** Next: user
eyeball (objective's last item).

Snapshot: 2026-09-28, Round 7.13 States (decision 288):
checked-Checkbox, on-Toggle, Slider 0/100 verified at glyph level
— pins and travel correct, no adjustments, temp rig deleted. All
gates green. User eyeball still invited. Details: `rounds.md`
7.13 entry.
288. **State extremes render correctly at glyph level.** Next:
user eyeball (objective's last item).

Snapshot: 2026-09-28, Round 7.12 Eyeball (decision 287):
agent pass over five CPU-rendered sink shots (all tabs + open
modal) — one visual language, no defects; temp rig deleted.
App 32/32, controls 48/48, testkit 7+1; clippy gate holds save
`FpsApp`; fmt clean. User eyeball still invited. Details:
`rounds.md` 7.12 entry.
287. **The polished catalog reads as one language on pixels.**
Next: user eyeball (objective's last item).

Snapshot: 2026-09-28, Round 7.11 Radius rule (decision 286):
TextInput, TextArea, Select box + list, and the tab-bar ring share
corner 4 (paint-only, zero churn). Controls 48/48, testkit 7+1
(+1 gray-surface paint); clippy gate holds save `FpsApp`; fmt
clean. Visual eyeball open. Details: `rounds.md` 7.11 entry.
286. **Rectangular controls share corner 4.** Next: remaining
consistency + the visual eyeball.

Snapshot: 2026-09-28, Round 7.10 Finished chrome (decision
285): Slider rail/fill/knob, Button radius 6 + centered labels,
active-tab underline (stale no-edge-border note corrected); a
Div→Row chrome churn broke tab remount identity and was reverted
(recorded on `chrome`). App 32/32, controls 48/48 (+1), testkit
6+1 (+1 paint); clippy gate holds save `FpsApp`; fmt clean.
Visual eyeball open. Details: `rounds.md` 7.10 entry.
285. **Slider, Button, and Tabs render finished chrome.** Next:
remaining catalog consistency + the visual eyeball.

Snapshot: 2026-09-28, Round 7.9 Drawn controls (decision 284):
Checkbox (20×20 box + white "✓" on Primary) and Toggle (pill +
knob + visible label) redrawn on the Radio pattern, catalog
palette, fallback-covered glyphs (DWrite test + DejaVu proof).
App 32/32 (+1), controls 47/47 (+2), testkit green; clippy gate
holds save `FpsApp`; fmt clean. Visual eyeball open. Details:
`rounds.md` 7.9 entry.
284. **Checkbox and Toggle are drawn controls.** Next: rest of
the catalog polish (Slider track, Button radius, Tabs indicator).

Snapshot: 2026-09-28, Round 7.8 Android interactive (decision
283): post-proof interactive phase reuses the present loop unbounded
until Destroy (evidence banked first, failures non-fatal) — taps
switch tabs mid-loop and post-`DONE` with no ANR. Targets check +
clippy clean; fmt clean. Details: `rounds.md` 7.8 entry.
283. **The Android app stays a live UI until Destroy.** Next:
platform runs continue (real-GPU timing open).

Snapshot: 2026-09-28, Round 7.7 Android hardening (decision
282): chain gains `Noto Sans Symbols` (chevron panic closed), scene
scales in dp (`dpr=2.625` on-device), every loop drains input (ANR
closed), background white per request. Targets check + clippy
clean; text-android 16/16; fmt clean; on-device `done` with
`content=nonblank`, no ANR on post-done taps. Details:
`rounds.md` 7.7 entry.
282. **Android scales in dp and never ignores input.** Next:
platform runs continue (real-GPU timing open).

Snapshot: 2026-09-28, Round 7.6 Android Kitchen Sink (decision
281): `oppa-android-app` mounts the shared `KitchenSinkApp` with
`/system/fonts` text + all faces in both backends, refits the scene
to the live window (no inset bailout), and presents the swapchain;
blank pixmaps fail loudly. Both android targets check + clippy
clean; text-android 16/16 (+1); fmt clean both workspaces.
PROVEN on-device (emulator `-gpu host`): phase `done`, no error,
`content=nonblank`, 1080x2400 presents, screencap shows the sink.
Details: `rounds.md` 7.6 entry.
281. **Android presents the shared Kitchen Sink at the live window
size.** Next: on-device proof, then platform runs continue
(real-GPU timing open).

Snapshot: 2026-09-28, Round 7.5 Wayland WSLg Weston fix (decision
280): Root-caused Wayland crash in WSLg — `winit` CSD subsurfaces
trigger `libpixman` segfault in Weston RDP-RAIL (upstream #1386).
Configured `winit` features without `wayland-csd-adwaita`, deferred
`softbuffer` in `ShellWindow` to first CPU present (zero dual-surface
conflict), and ordered redraw after GPU init. `kitchen_sink` & `oppa-fps`
run stably under Wayland GPU + CPU fallback + X11. App 31/31, WSL app 21/21
+ shell 40/40 + fps 2/2 green; clippy clean save `FpsApp`; fmt clean.
Details: `rounds.md` 7.5 entry.
280. **Linux window shells omit `wayland-csd-adwaita` and defer softbuffer.**
Next: platform runs continue (Android APK next; real-GPU timing open).

Snapshot: 2026-09-28, Round 7.4 Unified GPU runner (decision
279): `DesktopLoop` owns CPU + Vello twins (GPU primary, CPU
fallback; `OPPA_RENDERER` override) — Windows (Vulkan→DX12) and
Linux (Vulkan) present hardware swapchains, `Outdated`
reconfigures, every refusal falls back loudly. App 31/31 (+4),
vello green; WSL app 21/21 + shell 40/40; sink Wayland GPU
(`vulkan/Mailbox`) 20s stable + CPU/X11 20s stable; clippy gate
holds save `FpsApp`; fmt clean. Details: `rounds.md` 7.4 entry.
279. **Desktop presents GPU-first with loud CPU fallback.**
Next: platform runs continue (Android APK next; real-GPU timing
open).

Snapshot: 2026-09-28, Round 7.2 Windows faces (decision
277): shared-clone DW record + inject-all-faces after
mount/DPR + per-frame top-up — title/tabs/chevron
rasterize, bars only on unreadable files (loud). dwrite
shape green (+1), app 27/27 (+1); clippy gate holds; fmt
clean. Details: `rounds.md` 7.2 entry.
277. **Windows injects every shaped face.** Next: platform
runs continue (Android APK next; Linux face round open).

Snapshot: 2026-09-28, Completion gate (no new decision):
workspace release suite green with zero failures (all 5
completion criteria hold — rounds 273–276 tables present,
Phase 7 closed at 276, clippy save `FpsApp`, fmt clean).
Phases 6 and 7 CLOSED. Framework stands at Decision 276.
Details: `rounds.md` Completion entry.

Snapshot: 2026-09-28, Round 7.1 Kitchen Sink (decision
276): `KitchenSinkApp` (4 tabs: form/layout/overlays/
platform — controls 238–275 exercised) in
`oppa-controls::kitchen_sink` + `examples/kitchen_sink.rs`
desktop main (600x700); headless testkit proof (mount/
4-tab switches/modal/pixel-diff/finite boxes/CPU paints).
Example + controls 45/45 + testkit 5/5 green; linux-gnu +
wasm checks green; clippy gate holds; fmt clean. Phase 7
CLOSED (7.1). Details: `rounds.md` 7.1 entry.
276. **One sink, every target.** Next is the workspace-wide
release gate (completion criteria).

Snapshot: 2026-09-28, Round 6.4 Pluggable runner
(decision 275): `WebApp::new_with_root` mounts any root
component (demo `new()` unchanged); Android gains
`mount_app` + `SceneState::setup_with` (proof loop
byte-identical). Web 12/12 + shell-android green; wasm +
aarch64 checks green (incl. the device crate in its own
workspace); clippy gate holds; fmt clean. Phase 6 CLOSED
(6.1–6.4). Details: `rounds.md` 6.4 entry.
275. **Every target mounts arbitrary roots.** Next is Phase
7, Round 7.1 (Kitchen Sink showcase).

Snapshot: 2026-09-28, Round 6.3 Web text measurement
(decision 274): `WebApp::new` installs the bundled DejaVu
Sans rustybuzz service + `DejaVu Sans` layout config —
scene text lays out with real advances (non-zero, finite)
instead of zero width; wasm target check green. Web 11/11
(10 + 1 measure); in-round hygiene moved test-only
`change` under `#[cfg(test)]` (controls 45/45); clippy
gate holds; fmt clean. Details: `rounds.md` 6.3 entry.
274. **Web text measures through bundled DejaVu Sans.**
Next is Round 6.4 (pluggable multi-target runner).

Snapshot: 2026-09-28, Round 6.2 Windows DPI awareness
(decision 273): `ensure_dpi_awareness` (Per-Monitor V2 first,
v1 fallback, loud refusal status) called at the top of
`run_windows` ahead of `CreateWindowExW` — the Round-2.4
live-DPI engine now receives true `WM_DPICHANGED` instead
of OS bitmap scaling. App 26/26 (25 + 1 DPI status);
clippy gate holds; fmt clean. Details: `rounds.md` 6.2 entry.
273. **Windows declares Per-Monitor V2 DPI awareness.** Next
is Round 6.3 (Web text measurement on wasm).

Snapshot: 2026-09-28, Round 6.1 Release verification
(decision 272, Phase 6 opened as Production close-out):
full workspace green under the optimizer after two
release-only fixes (profile-matched hot fixtures, serial GL
shader init). Debug suites unregressed; clippy gate holds;
fmt clean. Details: `rounds.md` 6.1 entry.
272. **Release is green.** Next is Round 6.2 (Windows
DPI-awareness declaration scoped).

Snapshot: 2026-09-28, Round 5.4 Uncontrolled values
(decision 271, Phase 5 closed): session `on_change` funnel +
`Change<T>` notification on the five value controls +
uncontrolled companions. Controls 45/45, core lib 118/118;
full workspace green, zero failures; clippy clean save
`FpsApp` (184); fmt clean. Details: `rounds.md` 5.4 entry.
271. **Value controls report and self-manage.** Next is
Phase 6 (6.1 first).

Snapshot: 2026-09-28, Round 5.3 Slider I/O (decision 270,
OQ-G2-1 closed): arrow keycodes (Linux/Android mapped, Win32
passthrough, web mapped) + directional dispatch + drag
notification/positions/host reads + slider track wiring.
Controls 41/41; core arrow/drag suites green; shell tables
green; full workspace green, zero failures; linux-gnu check
green; clippy clean save `FpsApp` (184); fmt clean.
Details: `rounds.md` 5.3 entry.
270. **Sliders drag and step.** Next is Round 5.4 (Phase-5
close-out).

Snapshot: 2026-09-28, Round 5.2 Focus trap (decision 269):
derived dialog trap + TAB cycling (forward/backward) +
close-restores-global + Modal docs. Controls 39/39; full
workspace green, zero failures; clippy clean save `FpsApp`
(184); fmt clean. Details: `rounds.md` 5.2 entry.
269. **Tab stays inside open dialogs.** Next is Round 5.3.

Snapshot: 2026-09-28, Round 5.1 TextArea (decision 268, Phase 5 opened):
`TextArea` role/leaf + router Enter-newline + `TextArea` control
(auto-height) + `<textarea>` DOM/aria/UIA/AT-SPI + bootstrap token.
m5 14/14, controls 37/37, dom green; full workspace green, zero
failures; clippy clean save `FpsApp` (184); fmt clean.
Details: `rounds.md` 5.1 entry.
268. **Multi-line editing is a role, not a layout.**
Next is Round 5.2 popups/focus-trap.

Snapshot: 2026-09-28, Round 4.4 Web DOM images (decision
267, Phase 4 closed): src threading (Element→retained→diff→
builder) + cache reverse map + `<img>` render (src/alt/
geometry, loud unregistered refusal) + data-URI demo +
Edge decode E2E (naturalWidth 16, zero errors). Core/dom/
web suites green; wasm release green; full workspace green,
zero failures; clippy clean save `FpsApp` (184); fmt clean.
Details: `rounds.md` 4.4 entry.
267. **Images render as `<img>`, resolved through the cache.**
Next is Phase 5 widgets (5.1 TextArea first).

Snapshot: 2026-09-28, Round 4.3 Web storage (decision 266):
`localStorage` KvStore backend + app-owned settings signal +
boot restore + persist-after-paint + Edge reload E2E
(persisted on/off across reloads, zero errors). 4 new tests
(web 9/9); wasm release green (incl. a `Storage`-feature
fix); full workspace green, zero failures; clippy clean save
`FpsApp` (184); fmt clean. Details: `rounds.md` 4.3 entry.
266. **Settings persist through localStorage, restored on
boot.** Next is Round 4.4 Web DOM images.

Snapshot: 2026-09-28, Round 4.2 Web history (decision 265,
OQ-G6-1 closed): nav push/replace/pop bindings + history
bridge + boot adoption + route panel + bootstrap wiring +
Edge E2E (toggle + fetch + nav, zero errors). 2 new tests
(web 5/5); wasm release green (incl. a `Location`-feature
fix); full workspace green, zero failures; clippy clean save
`FpsApp` (184); fmt clean. Details: `rounds.md` 4.2 entry.
265. **URLs are syntax over the stack, both directions.**
Next is Round 4.3 Web storage.

Snapshot: 2026-09-28, Round 4.1 Web fetch (decision 264,
host driver + wasm binding): `start/resolve/snapshot_fetch`
with generation discard + `fetch_start`/`fetch_resolve`
bindings + bootstrap bridge + quote demo + Edge E2E
(toggle + quote, zero errors). 3 new tests (lib 116/116,
web 3/3); full workspace green, zero failures; clippy clean
save `FpsApp` (184); fmt clean. Details: `rounds.md` 4.1 entry.
264. **Promises drive the same fetch shape.** Phase 4 opened;
next is Round 4.2 Web history (OQ-G6-1).

Snapshot: 2026-09-28, Round 3.4 Android storage (decision
263, shell selection/validation + JNI getters): `AppDirs`
resolve-with-fallback + `NativeFs` round-trip validation +
app-crate `getFilesDir`/`getCacheDir` + dirs-first main
wiring (record in meta). 3 new host tests
(shell-android 21/21); full workspace green, zero failures;
aarch64 check + clippy green; root clippy clean save `FpsApp`
(184); fmt clean. OQ-label note in `rounds.md` (brief's
OQ-G6-2 vs G6 intake records). Details: `rounds.md` 3.4 entry.
263. **Scoped dirs resolve with fallback, validate before use.**
Device run open; Phase 3 continues.

Snapshot: 2026-09-28, Round 3.3 Android back nav (decision
262, host chain + contract): `handle_back`/`BackOutcome`
(composition-cancel → focus-clear → unhandled) + router ESC
arm on the same chain + nav.rs BackPress section (author
popups → host → runner nav/exit) + BACK→ESC pin. Popup +
field chain proven in controls (33/33); m5 12/12; full
workspace green, zero failures; clippy clean save `FpsApp`
(184); fmt clean. Details: `rounds.md` 3.3 entry.
262. **Back dismisses one layer per press, composition first.**
OQ-G11-2 closed; next is Round 3.4 Android app storage
(OQ-G6-2).

Snapshot: 2026-09-28, Round 3.2 Android gestures (decision
261, core router + input facts): tap/swipe/drag lift
classification (new constants + pure classifier) + `Swipe`
kind/handler + capture-owner dispatch (quiet fallback) +
tap slop-gate (far releases stop pressing) + disarm flags.
8 new tests (core 19/19 incl. 6 gesture, lib 114/114,
shell-android 18/18 + contract 5/5); full workspace green,
zero failures; clippy clean save `FpsApp` (184); fmt clean.
Details: `rounds.md` 3.2 entry.
261. **Lifts are taps, swipes, or drag releases.** OQ-G11-1
closed; next is Round 3.3 Android back navigation
(OQ-G11-2).

Snapshot: 2026-09-28, Round 3.1 Android keyboard (decision
260, shell intake + policy + queue + JNI bridge): `CommitText`/
`DeleteSurrounding` events/cmds (no M0 events, loud refusals)
+ change-only Show/Hide policy + focused-session helpers +
thread-safe entry queue + app-crate show/hide natives/drain
wired into the imm phase + `OppaIme.java` proxy
(device-pending). 9 new host tests (shell-android 16/16 +
contract 5/5); full workspace green, zero failures;
aarch64 check green (incl. a `JNIString`-path fix);
clippy clean save `FpsApp` (184); fmt clean.
Details: `rounds.md` 3.1 entry.
260. **Android keyboard path is policy + queue + bridge.**
Composition + device run open; next is Round 3.2 Android
touch gestures (OQ-G11-1).

Snapshot: 2026-09-28, Round 2.4 dynamic DPI (decision 259,
loop + host + builder + both shells): `set_device_pixel_ratio`
(CSS-stable re-base + surface refit + settle) + `resize` CSS
division + value-gated config dirt + builder `set_dpr` +
Windows `WM_DPICHANGED` snapshot/drive/SetWindowPos/startup
seed + Linux `Density` event/cmd/runner re-base + cursor
rescale. 11 new tests (app 25/25, shell-linux 40/40,
incl. real sent-message + re-shape proofs); full workspace
green, zero failures; Linux-gnu check green; clippy clean save
`FpsApp` (184); fmt clean. Details: `rounds.md` 2.4 entry.
259. **DPI crossings re-base live on both desktops.** Phase 2
CLOSED (2.1–2.4); `WM_DISPLAYCHANGE`/manifest stay follow-ups;
next is Phase 3 Android (3.1 virtual keyboard first).

Snapshot: 2026-09-28, Round 2.3 Linux file dialog (decision
258, `oppa-shell-linux` dbus + dialog + seam): hand-rolled D-Bus
client (marshal/parse/framing/AUTH/Hello/calls/signals) +
portal worker thread (supersede/close/generations) + zenity
fallback via `CommandRunner` seam + `request_open_dir` +
`file://` decoding + shell `file_dialog` seam. 15 new tests
(shell-linux 35/35: framing, loopback flow, argv/output/state,
probe, URIs, options, response codes, seam); full workspace
green, zero failures; Linux-gnu check green (incl. a
`SocketAddrExt`-gating fix); clippy clean save `FpsApp` (184);
fmt clean. Details: `rounds.md` 2.3 entry.
258. **Linux picks files through portals, zenity as fallback.**
OQ-G12-1 closed (save dialogs later); next is Round 2.4
dynamic DPI/display changes (OQ-G10-2).

Snapshot: 2026-09-28, Round 2.2 Linux clipboard (decision 257,
`oppa-shell-linux` x11rb + core seam + `run_linux`): `LinuxClipboard`
(CLIPBOARD owner/requestor, INCR both ways, serve pump, bounded
waits, Wayland-via-XWayland) + `Clipboard::service` seam +
idle-wake gating + session-local fallback without X. 6 new tests
(shell-linux 20/20: pure fidelity + loud no-display +
display-gated round-trip); Win32 probe race-hardened (monitor race
measured 0/8 vs 8/8, paced + retry); full workspace green, zero
failures; clippy clean save `FpsApp` (184); fmt clean. Policy:
`\u{}` escapes in test literals after an emission fault. Details:
`rounds.md` 2.2 entry.
257. **Linux copies through X11 selections with a served pump.**
OQ-G3-1 closed (Wayland-native + PRIMARY follow-ups); next is
Round 2.3 Linux file dialog (OQ-G12-1).

Snapshot: 2026-09-28, Round 2.1 native IME delivery (decision
256, `oppa-app` + host + both shells): `feed_ime*`/`ime_anchor`
loop API + `focused_ime_anchor` (leaf-exact on-demand shaper; Rc
service) + Windows `WinImeMapper`/echo-swallow/`drive_ime`/anchor
+ `pump_events` fix + best-effort TSF + Linux `Ime*` events/cmds/
translate arms + runner enablement/cursor-area + dep-gating and
`PresentInfo` fixes (Linux target checks green). 14 new tests
(app 19/19, shell-linux 14/14); full workspace green, zero
failures; clippy clean save `FpsApp` (184); fmt clean. Details:
`rounds.md` 2.1 entry.
256. **IME composes into focused sessions on both desktops.**
Live-IME manual pass stays open (unsimulatable); next is Round
2.2 Linux clipboard backend (OQ-G3-1).

Snapshot: 2026-09-28, Round 1.4 portals/overlays (decision 255,
core + builder + Modal): `Tag::Portal`/`Portal()` (viewport-anchored,
flow-skipped everywhere, DOM Block arm) + `outermost_portals` shared
helper + portal-first hit test + builder two-phase paint-last walk +
`clear_retired_input` unmount hygiene (Down-time instance records;
focus/captures/arms/pressed-flags release) + `Modal` migrated onto
the layer. 4 new tests (m3 45/45, m5 12/12, m4 17/17, controls
31/31 Modal green); full workspace green, zero failures; clippy
clean save `FpsApp` (184); fmt clean. Details: `rounds.md` 1.4.
255. **Overlays anchor to the viewport and clear after
themselves.** Phase 1 CLOSED (1.1–1.4); anchors, full-bleed dim,
and focus traps stay follow-ups (5.2/5.3); next is Phase 2 (2.1
native IME first).

Snapshot: 2026-09-28, Round 1.3 visual styling primitives
(decision 254, style + shared builder + DOM): `Shadow.blur` (stepped
solids, `.shadow()` unchanged) + `BorderEdges` (quad + 4 singles,
merge/last-wins) + `LinearGradient` (vertical/horizontal strips);
builder-only expansion (no new `DrawOp`, backends untouched); loud
conflicts (border-vs-edges, bg-vs-gradient, sharp-vs-round,
negative blur/edges); DOM CSS mapped. 12 builder tests + 2 oracle
rows, both CPU↔Vello exact 0; lib 113/113, m6 23/23, dom green;
full workspace green, zero failures; clippy clean save `FpsApp`
(184); fmt clean. Details: `rounds.md` 1.3 entry.
254. **Effects expand to shared solids; conflicts refuse loudly.**
Gaussian/native interpolation stay follow-ups; next is Round 1.4
portals/overlays (Modal still composes a Row overlay).

Snapshot: 2026-09-28, Round 1.2 Row flex-wrap (decision 253,
core style/layout): `FlexWrap::NoWrap/Wrap` + `.flex_wrap()` builder
+ layout-bits arm + `layout_row_wrap` (greedy line-breaking, per-line
justify/align/fill shares, gap-doubled line gap, auto container
height; unconstrained Wrap falls through single-line; non-Row Wrap
panics naming the tag). 5 exact-coordinate tests (m3 44/44: 3-line
38-high wrap, Center per-line, fill remainder 36, NoWrap overflow
preserved, Column loud refusal); full workspace green, zero
failures; clippy clean save `FpsApp` (184); fmt clean. Details:
`rounds.md` 1.2 entry.
253. **Rows wrap greedily per line; other axes refuse loudly.**
Column wrap-to-columns stays an open follow-up; next is Round 1.3
visual styling (shadow blur, gradients, per-edge borders).

Snapshot: 2026-09-28, Round 1.1 multi-line wrapping & intrinsic
sizing (decision 252, core layout + m3 proof): `check_constrain_width`
(NaN/negative widths panic at viewports, `given_w`, `constrain_w`/
`explicit_w`, both pure wrap entry points; infinite stays intrinsic) +
5 exact-coordinate tests (m3 39/39: wrapped auto-height expansion
61.25 with line y 0/15.75/31.5/47.25, Row fill-share wrap 16+4 with
row h 29.75, 3 loud-width should-panics); full workspace green
(m6 21/21, m10_gles 4/4 on re-run after the known contention flake);
clippy clean save `FpsApp` (184); fmt clean. Details: `rounds.md`
1.1 entry.
252. **Widths constrain loudly; wrap proves intrinsic height.** No new
pass invented (block/fill shares already constrain; re-wrap shapes
nothing); Row non-fill stays intrinsic (flex no-wrap); zero stays
legal; next is Round 1.2 Flex-Wrap (genuinely absent).

Snapshot: 2026-09-28, Shipped ProgressBar & Badge controls
(decision 251, core role + 3 emitter arms + catalog + showcase):
`Role::ProgressBar` + `Semantics::progressbar(text)` (AT-SPI
`progress bar`, ARIA `progressbar` + `aria-valuetext`, UIA progress
type; no v1 patterns — OQ-G2-2); stateless `ProgressBar`
(160x12 pill track + proportional fill, clamped, NaN loud) +
`Badge` (24-high pill, Primary/Success/Dim, filler+label) +
Profile status row + quota meter. 4 new tests (controls 31/31);
full workspace green; clippy clean save `FpsApp` (184); fmt
clean. Details: `rounds.md` meter-chip entry.
251. **Meters display, chips announce.** Percent text rides the
constructor (radio/tab `selected` precedent); no `enabled` on a
handlerless display; Success green chosen (no catalog green);
`_32` hex trips the suffix lint (ungrouped); AT-SPI drops
`value_text` (OQ-G2-2, Slider standing).

Snapshot: 2026-09-28, Desktop mouse wheel scrolling
(decision 250, core + both shells + runner): `scroll_target_at`
(hit-test + walk to first `Scroll`-handler ancestor) +
`DesktopLoop::scroll_at` (inject + settle + repaint, quiet miss);
Win32 `WM_MOUSEWHEEL` (screen→client at message time) →
`Wheel` → `Cmd::Scroll`; Linux positioned `Wheel` (cursor dp,
LineDelta x30) → `LinuxCmd::Scroll` (runner-matched, loud in
`to_input_event`). 1 headless test (app 10/10; walk-up + feed +
miss); shells 9/9 + 3/3; full workspace green; clippy clean
save `FpsApp` (184); fmt clean. Spike rig `Cmd` match extended
(experiment, not architecture). Details: `rounds.md` wheel
entry.
250. **Wheel resolves, never fabricates.** Handler-only walk
(`kind_handler` refuses feed-only nodes — the brief's OR would
hand inject a panic); shells carry positions, targets resolve
runner-side (decision 100 lives there now); `ignored_wheel`
retired; `WM_MOUSEHWHEEL` unwired (stated).

Snapshot: 2026-09-28, Layout expressiveness — vertical
`fill_height` & margins (decision 249, core layout): `Style::
fill_height/margin_x/margin_y/margin` + engine arms (Column
main-axis share via max-grow, Row cross-axis fill merged into
Stretch, margins in Row/Column/Div/Stack/ScrollArea placement +
extents) + `style_layout_bits` nested (tuples cap `Eq` at 12).
3 exact-coordinate tests (m3 34/34); 31 pre-existing green
(absent margins are zeros — bit-identical flow); full workspace
green; clippy clean save `FpsApp` (184); fmt clean. Details:
`rounds.md` fill-margins entry.
249. **Fill and margins compose.** No `given_h` threading —
`set_box_h` max-grow (Stretch precedent, subtree stays
top-aligned); fill shares never shrink for margins, out-of-flow
bypasses them, block-lite forced one test into a Row wrapper
(stated); `Into<Px>` never existed (builders take `IntoPx`).

Snapshot: 2026-09-28, Responsive window resizing & viewport
invalidation (decision 248, core + `oppa-app`): `Reconciler::
mark_layout_dirty` (LAYOUT|PAINT mirror) + `set_viewport` compares,
dirties the root, and requests a frame (same-value writes and
pre-mount sizing stay no-ops); `DesktopLoop::resize` unchanged
(242 already refits + settles — the settle re-runs layout now).
1 headless test (app 9/9; mutation-checked: stripped
invalidation leaves stale 200x150 boxes); full workspace green;
clippy clean save `FpsApp` (184); fmt clean. Details:
`rounds.md` resize entry.
248. **Viewport writes invalidate.** No `fill_height` exists —
the test proves recompute through root fill + main-axis
centering; cross-axis centering at root reads content height
during the pass (root-fill fix-up stretches after children
place — proven shape, asserted out of the test, stated).

Snapshot: 2026-09-28, Shipped Dropdown / Select control
(decision 247, core role + 3 emitter arms + catalog + showcase):
`Role::ComboBox` + `Semantics::combobox()` (AT-SPI `combo box`,
ARIA `combobox`, UIA combo type; options reuse `list_item` +
`selected`); generic `Select` (combobox box + toggle + option list
over controlled `selected`/`open`, per-option `ctx.child`
instances) + showcase Theme picker in Preferences. 3 new tests
(controls 27/27); full workspace green incl. m10_gles 4/4, m6
21/21, dom 30/30; clippy clean save `FpsApp` (184); fmt clean.
Details: `rounds.md` select entry.
247. **Select composes a picker.** `#[derive(Props)]` rejects
generics (compiler-verified) — manual `impl Props` (Tabs
precedent); list is a `Div` (vertical like `Column`, keeps the
debug label `Column::new()` cannot carry); no explicit list
height (row height is shaper-fed — invented math refused);
no light-dismiss in v1 (stated).

Snapshot: 2026-09-28, Desktop keyboard editing shortcuts
(decision 246, `oppa-app` + core keys + Linux table): `keys::A/C/V/
X/Y/Z` + `DesktopLoop` clipboard ownership (in-memory default,
Win32 installed, Linux session-local until OQ-G3-1) + `step`
interception (ctrl, no alt/meta — the AltGr guard) running
select-all/undo/redo/copy/cut/paste on the focused session,
consumed; Windows samples live alt into injected modifiers; Linux
table routes the six letters + runner skips `Char` under shortcut
modifiers. 3 headless tests (app 8/8, shell-linux 9/9); full
workspace green; clippy clean save `FpsApp` (184); fmt clean.
Linux runner arm read-verified (this box is Windows). Details:
`rounds.md` shortcuts entry.
246. **Shortcuts ride one interception point.** Centralized in
`step`, not per-platform dispatch (both arms + headless tests
share the path); session method names are `*_selection_to` /
`paste_from` (the brief's `copy/cut/paste` never existed);
transient clipboard failures are loud on stderr, never fatal;
Linux `ModifiersChanged`-after-press race acknowledged.

Snapshot: 2026-09-28, Shipped Tabs & multi-view navigation
(decision 245, core roles + 3 emitter arms + catalog + showcase
refactor): `Role::Tab/TabList` + `Semantics::tab/tab_list`
(AT-SPI `page tab[list]`, ARIA `tab[list]` + `aria-selected`,
UIA tab-item/tab types + SelectionItem); generic `Tabs` (bar +
active panel, per-tab `ctx.child` instances) + 3-tab showcase
(Profile/Preferences/Actions). 2 new tests (controls 24/24);
full workspace green incl. m10_gles 4/4, m6 21/21, dom 30/30;
clippy clean save `FpsApp` (184); fmt clean. Details:
`rounds.md` tabs entry.
245. **Tabs compose views.** Content rides factory closures
(stored `VNode` cannot satisfy `Props: Clone` — compiler-verified;
lazy + fresh per render); active = bold + primary ink (the
spec's ink alternative — no edge-only border primitive exists);
empty/unmatched renders an empty panel, quietly documented.

Snapshot: 2026-09-28, Shipped Radio & RadioGroup control
(decision 244, core role + 3 emitter arms + catalog + showcase):
`Role::RadioButton` + `Semantics::radio(selected)` (AT-SPI
`radio button`, ARIA `radio` + `aria-checked`, UIA radio type +
SelectionItem); `Radio` (18px circle, centered 8px dot,
`radio` role) + generic `RadioGroup` (column over `Signal<T>`,
exclusivity by construction) + showcase Plan group. 4 new tests
(controls 22/22); full workspace green incl. m10_gles 4/4, m6
21/21, dom 30/30; clippy clean save `FpsApp` (184); fmt clean.
Details: `rounds.md` radio entry.
244. **Radio composes choice.** Dot pins at (5, 5) out-of-flow
(knob precedent — indicator stays a `Div`); group owns
exclusivity, radios only report; AT-SPI name follows the
space-separated canonical source (brief's underscore form
overridden, stated).

Snapshot: 2026-09-28, Live keyboard text entry in `oppa-app`
(decision 243, core seam + both runner arms + Linux intake):
`keys::BACKSPACE/DELETE` + `focused_field_session()` (focus →
press owner → instance → sole session; quiet miss, loud on
ambiguity); `DesktopLoop::type_text/backspace/delete_forward`
(caret-aware session ops, no binding needed); Win32
Char/Backspace/Delete routing; Linux Char intake + editing
keycodes. 2 headless runner tests + live `TextInput` typing test
(app 5/5, controls 18/18); full workspace green incl. m10_gles
4/4, m6 21/21, dom 30/30; clippy clean save `FpsApp` (184); fmt
clean. Details: `rounds.md` typing entry.
243. **Typing rides the focused session.** No app pairs, no
`bind_text` round-trip (the session owns the value signal);
control chars never become text; editing keys are consumed, never
also injected; Linux `Ime::*` stays dark (no IME enablement —
commits cannot arrive); `to_input_event(Char)` panics loudly;
Linux runner arm read-verified (this box is Windows).

Snapshot: 2026-09-28, Desktop app runner + controls showcase
(decision 242, new `crates/oppa-app` + controls example):
`WindowOptions` + `run_desktop` + platform-free `DesktopLoop`
(host/mount/inject/repaint) with Windows (Win32 + DWrite + GDI)
and Linux (winit + system text + softbuffer) glue; showcase mounts
all 6 controls in a 500x600 window. 3 headless runner tests;
full workspace green incl. m10_gles 4/4, m6 21/21, dom 30/30;
clippy clean save `FpsApp` (184); fmt clean. Details: `rounds.md`
desktop-runner entry.
242. **Runner composes proven paths.** `fn`-pointer component
(mount contract, stated); pointer + keys only (text needs the
field-binding seam — follow-up); Escape-at-root exits, window
close exits 0; viewport-height dimming and hidpi limits stated;
Linux arm read-verified (this box is Windows).

Snapshot: 2026-09-28, Modal dialog control (decision 241,
`oppa-controls` + `Role::Dialog` arms; closes OQ-G2-3's overlay
half): `ModalProps` (title + controlled `open` + 360 + dismiss +
OK/Cancel) + `Modal` composing Row overlay/backdrop + dialog card +
Cancel/Confirm `Button`s (G2 precedent — no new `Tag`, no contract
changes); `Semantics::dialog()` with AT-SPI/DOM/UIA arms. 4 new
tests (controls 17/17); full workspace green incl. m10_gles 4/4,
m6 21/21, dom 30/30; clippy clean save `FpsApp` (184); fmt clean.
Details: `rounds.md` modal entry.
241. **Modal composes an overlay.** Row (not Stack — Stack lays out
unconstrained, verified) centers via fill share; dim is full-width
× card height (viewport-height dim needs a height primitive —
stated); backdrop-subtree presses dismiss (ancestor-walk
precedent), buttons capture their own; Confirm+Cancel both close;
opt-out backdrop is handlerless; focus-trap needs router support
(follow-up). In-round fix: stale focus after unmount no longer
panics (decision-95 precedent).

Snapshot: 2026-09-28, Shipped TextInput control (decision 240,
`crates/oppa-controls` only): `TextInputProps` (label + controlled
value + placeholder + enabled + 200x32 + `BodySecondary` + debug) +
`TextInput` composing `Div` chrome (bg/border/pad) + `TextField`
payload + instance-keyed `EditSession` (G2 precedent — no new `Tag`,
no contract changes). 4 new tests (controls 13/13); full workspace
green incl. m10_gles 4/4, m6 21/21, dom 30/30; clippy clean save
`FpsApp` (184); fmt clean. Details: `rounds.md` textinput entry.
240. **TextInput composes, never branches.** Outer carries
`text_field` label+disabled (Button-chrome precedent); the value
leaf is the single bindable `TextField`; placeholder is a dimmed
plain span (never the value — stated DOM divergence); press parks
the caret at end (router focuses); disabled is structurally
handlerless + untabbable.

Snapshot: 2026-09-28, Typography expressiveness — arbitrary font
sizes + weights (decision 239, `crates/oppa` only):
`TextClass::Custom { size_px, weight }` + `Text::new` builder
(`.size/.weight/.bold/.build`, untouched default `BodySecondary`);
layout resolves custom sizes absolutely and threads the weight into
`TextStyle` at every shape site (measure + ellipsis caches keyed by
weight; weight inherits through `Text` wrappers like the size does);
both tokens byte-identical. 6 new tests (m3 31/31); full workspace
green incl. m10_gles 4/4, m6 21/21, dom 30/30; clippy clean save
`FpsApp` (184); fmt clean. Details: `rounds.md`
typography-expressiveness entry.
239. **Arbitrary text sizes + weights.** `Custom` sizes are absolute
CSS px (config-independent); weight-only builders default to 14px
(the default-config body size, stated not silent); zero sizes and
out-of-`1..=999` weights panic loudly; presenters unchanged
(`em_size` already carries size, shaping carries weight).

Snapshot: 2026-09-28, Layout expressiveness — vertical padding
+ flexbox alignment (decision 238, `crates/oppa` only; the round
brief labeled it 237, which OQ-G11-2 already holds, so 238 keeps
numbering truthful): `Style::pad_y` + `AlignItems` + `JustifyContent`
(additive — defaults Start/0, pre-existing trees byte-identical);
Row/Column justify + align + `pad_y` with loud NaN/negative refusals
and DPR-snapped commits. 8 new tests (m3 25/25); pre-existing 17
unmodified; full workspace green save the known m10_gles contention
flake (4/4 solo); clippy clean save `FpsApp` (184); fmt clean.
Details: `rounds.md` layout-expressiveness entry.
238. **Vertical padding + flex alignment.** `pad_y` mirrors `pad_x`
(top+bottom); Row justifies X + aligns Y, Column justifies Y +
aligns X (Center/End/SpaceBetween; Stretch grows unconstrained
cross sizes only, fixed sizes stay Start; SpaceBetween clamps
negative leftover to 0; out-of-flow `x`/`absolute_y` bypass both
axes); Stack/ScrollArea take `pad_y` symmetrically (flow from the
content origin); NaN/Inf/negative sizes-pads-gaps panic loudly.

Snapshot: 2026-09-27, OQ-G11-2 distinct long-press actions
(decision 237, `crates/oppa` only): `EventKind::LongPress` +
`on_long_press` builder; fire resolves the current hold handler
lazily with Press fallback (additive — pre-existing controls
byte-identical); press ownership still routes (hold-only inert
by rule). 4 new tests (g11 13/13); m5 10/10 unmodified; clippy
genuinely clean save `FpsApp` (184); fmt clean. Details:
`rounds.md` OQ-G11-2 entry.
237. **Distinct hold actions.** `on_long_press` declares them;
fire dispatches `LongPress` when present, `Press` otherwise
(lazy at deadline — re-renders resolve); no tab/focus/emitter
change (hold has no v1 ARIA mapping; menus/haptics stay
app-side).

Snapshot: 2026-09-27, OQ-G8-1 Vello image paint arm (decision
236, `oppa-vello` only): `insert_image`/`remove_image` (loud
validation, straight-alpha peniko deposit) + `encode_plan` RImg
arm (validate-first, `ImageBrushRef` without clone, layer alpha,
affine scale); oracle proof exact-0 CPU-vs-Vello at 1:1
(`m6_vello` 21/21); DOM caller updated (30/30); m10_gles flake
reconfirmed environmental (4/4 solo). OQ-G8-1 narrows to DOM.
Details: `rounds.md` OQ-G8-1 entry.
236. **Vello paints registered images.** Straight-alpha upload
(no CPU-side conversion); oracle-exact with CPU at identity;
pending-specific refusal preserved; DOM arm stays open.

Snapshot: 2026-09-27, V3 CLOSED (decisions 205–235, ADR-0014):
all 16 verified v2 gaps shippable in-tree — P0 editing (G1),
clipboard (G3), controls (G2), packaging doc (G4), storage seams
(G5), navigation (G6); P1 fetch pattern (G7), image decode (G8),
fallback contract (G9), DPR (G10), touch (G11), file picker
(G12); P2 catalog a11y (G13), reload loop (G14), test harness
(G15), budgets (G16). Suite green workspace-wide (lib proofs
111 oppa + all suites, 0 failed); clippy genuinely clean save
intentional `FpsApp` (184); fmt clean. What remains is the
accumulated per-round OQ list (owners named in `rounds.md`).
Details: `rounds.md` V3-G16 entry (+ G1–G15 entries above it).
235. **Budgets floor releases per tier** (scene-labeled measured
baselines + >10% regression rule); oracle gates block
exactness regressions (exact-0 rows, tol-16 ≤60, atlas/static/
scroll rows); adaptive stays Proposed over `PresentReport`
input; version = Cargo 0.1.0 (no stability promise yet); no
crash reporter ships (file oracle diffs).

Snapshot: 2026-09-27, V3 G15 app test harness (decision 234,
new `oppa-testkit` + page, no framework changes): `Harness`
(mount/pump/tap/center/advance/host escape hatch; loud lookups;
clock-gated time) with 4 self-proving tests + doctest; `07-
testing/app-harness.md`. Clippy-hygiene correction: 17 warnings
from earlier V3 rounds fixed (unused-mut, bool-asserts,
negated-floats, doc indent, struct-init, vec-repeat) — clippy now
genuinely clean save intentional `FpsApp` (184), filters
corrected. Warning-touched suites re-run green. Open: OQ-G15-1
(pixel asserts), OQ-G15-2 (app fuzz). P2 last: G16. Details:
`rounds.md` V3-G15 entry.
234. **Test seam = opt-in Harness over public API.** No
test-only backdoors (green proves app-reachable); loud
missing-label/unboxed/clockless-advance panics; deterministic
time only from an owned `MockClock`.

Snapshot: 2026-09-27, V3 G14 reload app loop (decision 233,
example + page, no harness changes): `oppa-reload/examples/
app_loop.rs` (typed boot → install → drive → `reload_to` →
report; state 41→42 survives, 0 evicted, PASS printed) +
`09-api/hot-reload.md` (loop, survival, eviction, Android
pointer). Example run green, harness suites green; clippy clean
save `FpsApp` (184); fmt clean. Open: OQ-G14-1 (watcher),
OQ-G14-2 (dylib deploy loop). Next: G15 app test seam. Details:
`rounds.md` V3-G14 entry.
233. **App reload loop shape.** Typed boot, manifest install,
normal frame driving, rescan + `reload_to`, report reading;
`StaticSource` demonstrates (source-agnostic loop), `DylibSource`
deploys; evictions are the loud restart class (§5.1), survivors
are residence (ADR-0013).

Snapshot: 2026-09-27, V3 G13 catalog a11y end-to-end (decision
232, tests + one dev-dep): app-authored controls through retained
semantics per leg — DOM HTML roles (real `oppa-controls` via
`ctx.child`), AT-SPI mirror roles/states, UIA COM types + Checkbox
Toggle round-trip (Button Invoke refusal documents OQ-G2-2). All
legs green (dom incl. new test, atspi 11, uia 2/2); clippy clean
save `FpsApp` (184); fmt clean. Open: OQ-G13-1 (SR human pass),
OQ-G13-2 (live-bus/event legs). Next: G14 reload-to-apps.
Details: `rounds.md` V3-G13 entry.
232. **Catalog roles proven end-to-end per leg.** Unit mappings
(G2) + composed proofs (this round); the human-plus-screen-
reader pass stays open (no harness substitutes for a person +
SR — stated, not silent).

Snapshot: 2026-09-27, V3 G12 file-picker seam (decisions
230–231, core `dialog.rs` + Win32 backend): `FilePickerOptions`/
`FileFilter`/`PickError`/`FileDialog` (request supersedes, poll
level-triggered, dismissal-is-`Ok(vec![])`) + `ScriptedDialog`;
`PlatformShell::file_dialog` seam (default `None`); Win32 modal
`GetOpenFileNameW` (dismiss-vs-failure via extended error,
failures keep identity); picked paths feed G5/G8 seams by app
composition. Tests: oppa lib 111/0, shell-win 3/3; clippy clean
save `FpsApp` (184); fmt clean. Open: OQ-G12-1..5 (Linux/Android/
Web pickers, DnD, multi-window/menus/save-dir/tray). P1 CLOSED
(G7–G12); remaining P2 (G13–G16). Details: `rounds.md` V3-G12
entry.
230. **Request/poll picker completion.** G3 shape reused for
dialogs (modal blocking is documented OS behavior, not a hang);
dismissal is data (`Ok(vec![])` — clipboard-empty precedent);
re-poll repeats the last result (Text-feed self-heal precedent).
231. **One-method shell seam.** Default `None` (pre-G12 shells
untouched, refuse via `Unsupported`); Win32 wired (HWND-bound,
extended-error honesty); Linux/Android/Web deferred with their
platform paths named.

Snapshot: 2026-09-27, V3 G11 multi-pointer + long-press
(decisions 227–229, core router + Linux/Android intake + app
glue): per-id captures (lowest-id legacy read, last-lift flag
clear, global/targeted cancel split); hold-still 0.5 s / 10 px
fires the same press handler via pump-check firing (never
self-demanding — the first TIME-drive shape hung M5 holds and was
replaced); intake carries ids, MultiTouch refusals retired;
9 new tests, M5 10/10 unmodified green; Android app target check
clean. Full touched suites green; clippy clean save `FpsApp`
(184); fmt clean. Open: OQ-G11-1 (pinch/momentum), OQ-G11-2
(distinct hold actions), OQ-G11-3 (Win32 touch), OQ-G11-4 (fps
driver input), OQ-G11-5 (multi-move batches). P1 last: G12.
Details: `rounds.md` V3-G11 entry.
227. **Multi-pointer router.** One capture per stable pointer id
(mouse 0, touch index); `capture_node()` reads lowest-id
(deterministic — M5 asserts hold); pressed clears on last lift
per owner (shared flags); `Cancel` splits global (`None`,
legacy tripwire) vs targeted (`Some(id)`); id-less Down/Up/Move
panic as malformed (constructors always set ids).
228. **Long-press hold-fire.** Same press handler, no new event
kind; 0.5 s / 10 px reasoned constants; slop-move disarms (tap
unaffected); consumed-flag kills double dispatch; retired-node
fire-cancel; pump-check firing without self-demand (hang-forced
refinement — a held finger neither spins the loop nor hangs
idle; bound: first pump/event at/after deadline).
229. **Intake carries ids.** `LinuxCmd`/`AndroidCmd` pointer
variants gain `id`; MultiTouch refusals + counters retired
(empty error types kept for `take_errors` shape); fabricated
second-finger app-glued Down deleted; Win32 touch + fps input
stay OQ (unwritten beats unverifiable).

Snapshot: 2026-09-27, V3 G10 DPR plumbing (decision 226, core
helpers + 4 shell reporters + oracle): `dpr_from_dpi` /
`dpr_from_scale_factor` (loud on garbage) with per-shell live
reporters (Win32 per-monitor DPI, Linux/Android density aliases,
web devicePixelRatio cfg-gated); 2x oracle green first run
(layout width exactly doubles, ink ~4x, first column doubles);
wasm target check clean; demos stay 1.0 (surface resize is
per-demo work). Host suites green; clippy clean save `FpsApp`
(184); fmt clean. Open: OQ-G10-1 (demo HiDPI), OQ-G10-2 (DPR
change events). Next: G11 touch. Details: `rounds.md` V3-G10
entry.
226. **One DPR rule, live reporters.** Single shared conversion
(`dpr_from_dpi/scale_factor` — zero/negative/NaN panic, never
clamped); shells report live density (Win32 `GetDpiForWindow`
with logged 1.0 fallback on 0-read; winit scale as density;
Android density; web devicePixelRatio with compile-time host
default — `web_sys` traps on host, so no runtime guessing);
engine proven at 2.0 end to end (boxes + pixels).

Snapshot: 2026-09-27, V3 G9 fallback contract, never tofu
(decisions 224–225, tests + spec doc): stale "no fallback code"
corrected (per-item fallback + slice chains ship — the round
contracts them in `03-spec/text/fallback.md`); rustybuzz
never-tofu tests (CJK/PUA refusals naming codepoints, U+1F600
positive with script-990 proof, chain-miss tolerance);
Linux-slice Ok-or-loud CJK/emoji tests — 8/8 green on real Ubuntu
fonts (WSL). Host suites green (rustybuzz 10+5, linux portable 2,
android 15); clippy clean save `FpsApp` (184); fmt clean. Open:
OQ-G9-1 (bundled CJK/emoji bytes), OQ-G9-2 (color emoji render),
OQ-G9-3 (Android positives). Next: G10 DPR plumbing. Details:
`rounds.md` V3-G9 entry.
224. **Fallback contract.** Requested family, then slice chain in
order (weight/style-exact first), else loud `Backend` naming
`U+XXXX` — never `.notdef` tofu, never silent skip. Per-slice
status measured (Latin everywhere; CJK shapes-or-loud; emoji
classified + chain-routed, color render deferred).
225. **Ok-or-loud test strategy.** Assertions hold on any font set
(bare or full): bundled-font refusal tests run everywhere;
system-font positives run `cfg(target_os = "linux")` with both
arms asserting the contract.

Snapshot: 2026-09-27, V3 G8 image decode, PNG spine (decisions
222–223, new `oppa-image` + `oppa-cpu` paint arm): magic-sniffed
decode to straight RGBA8 (PNG via vendored `png` 0.18, dims capped
pre-alloc, JPEG/GIF/WebP refuse by name, APNG refuses); CPU
`insert_image` (loud validation, tiny-skia-exact premultiply) +
RImg paint (scale + layer alpha + clip mask; unregistered refuses
pending-specifically); PNG-bytes-to-exact-pixels E2E; contract
note reworded + `PlanStats` root export. Tests: image 3 + e2e 1,
cpu m4 15/15; clippy clean save `FpsApp` (184); fmt clean. Open:
OQ-G8-1 (Vello/DOM arms), OQ-G8-2 (decode pump), OQ-G8-3 (more
formats), OQ-G8-4 (EXIF). Next: G9 font fallback. Details:
`rounds.md` V3-G8 entry.
222. **PNG-first decode seam.** Straight alpha out (backends own
their layout); dims-before-alloc OOM guard (`MAX_DIMENSION` 8192,
reasoned); every other format refuses by sniffed name (JPEG names
OQ-G8-3 with its dep); APNG refuses rather than first-framing
silently. Vendored-dep-only rule held (no network risk).
223. **CPU paints registered images.** Validate-at-insert
(font-rule precedent), tiny-skia-exact premultiply (no deposit/
paint drift), full paint citizenship (scale, layer alpha, clip
mask); Vello/DOM refusal stays (their arms are the OQ, not silent
placeholders); no framework decode pump (G7 spawn + UI-thread
insert is the documented pattern).

Snapshot: 2026-09-27, V3 G7 blessed fetch-to-render (decisions
220–221, `crates/oppa` only): `FetchState<T>` keyed signal +
`Ctx::fetch_state`/`spawn_fetch` (Loading sync, executor thread,
`keyed_state` rendezvous, §9.6 tags) + `fetch_key` namespacing;
wasm shares the shape via binding-driven writes (`spawn_fetch`
refuses loudly there); pattern page `09-api/async-fetch.md; 3
tests (Loading sync at mount, Ready, Failed-as-state). `cargo test
-p oppa` green (lib 105/0); clippy clean save `FpsApp` (184); fmt
clean. Open: OQ-G7-1 (wasm binding driver), OQ-G7-2 (cancel),
OQ-G7-3 (progress); reload examples stay G14. Next: G8 image
decode. Details: `rounds.md` V3-G7 entry.
220. **Blessed fetch-to-render.** `FetchState<T>` in a keyed signal
(`fetch_key("route:name")` namespacing) rendered with a plain
match; `spawn_fetch` = Loading-now + thread work +
generation-tagged submit through the `keyed_state` rendezvous (the
§9.6 tribal pattern productized — signals never cross threads by
doctest lock).
221. **Wasm shares shape, not driver.** No threads on wasm:
`spawn_fetch` panics explicitly (never the cryptic OS stub); the
binding resolves promises into the same keyed signal from the UI
thread + requests a frame (verdict-(b) split applied to async).

Snapshot: 2026-09-27, V3 G6 stack-first navigation (decisions
218–219, `crates/oppa` only): `NavStack` (push/pop/`go_back`/
replace/reset/`push_link`, root never pops — `AtRoot` tells shells
to exit) + `Route` (name + ordered params, parse/encode exact
round-trip, loud `NavError`) + named outcomes (`PopOutcome`,
`ReplaceOutcome`); host-independent (held in `Signal` by apps, no
scheduler coupling); 6 tests. `cargo test -p oppa` green (lib
102/0); clippy clean save `FpsApp` (184); fmt clean. Open: OQ-G6-1
(web history bridge), OQ-G6-2 (shell back/deep-link intake),
OQ-G6-3 (typed tables/guards). P0 CLOSED (G1/G3/G2/G4/G5/G6 all
shippable); P1 (G7–G12) and P2 (G13–G16) remain, next session picks
up at G7. Details: `rounds.md` V3-G6 entry.
218. **Stack-first navigation.** Identity is stack position, not
URI: deep-links parse into stack ops and encode back out; the web
history bridge is deferred (URIs are syntax, never the source of
truth). Consecutive-duplicate pushes allowed (dedupe is app
policy — login flows `replace`); multi-segment links keep the
full path as the name (segment splitting is app policy).
219. **Host-independent nav state.** No router/input/scheduler
touch — `NavStack` is plain state; reactivity comes from holding
it in a `Signal` (tested pattern), persistence from `KvStore`;
shell intake targets are `go_back()`/`push_link()` (unwired,
OQ-G6-2); no route tables/guards in v1 (app-owned, OQ-G6-3);
every edge names itself (`Pop/ReplaceOutcome`, `NavError`).

Snapshot: 2026-09-27, V3 G5 persistence/network decision
(decisions 216–217, ADR-0014, `crates/oppa` only, std-only):
`KvStore` (bytes, flat keys) + `FsSandbox` (lexical jail) +
`StoreError` + `InMemoryKv`/`InMemoryFs`/`NativeFs` references
(`src/store.rs`, 5 tests incl. real-OS tempdir round-trip); sync-
first ruling (G3 poll not repeated — sync option everywhere);
async backends (IndexedDB/OPFS) deferred with bridge sketched;
fetch stays its own decision. `cargo test -p oppa` green (lib
96/0); clippy clean save `FpsApp` (184); fmt clean. Open: OQ-G5-1
(Android filesDir), OQ-G5-2 (shell data-dir), OQ-G5-3 (async
bridge), OQ-G5-4 (symlink hardening), OQ-G5-5 (fetch). Details:
`rounds.md` V3-G5 entry; ADR-0014.
216. **Sync-first storage seams.** `KvStore` + `FsSandbox` are
plain sync `Result` traits — the clipboard''s request/poll is
explicitly not repeated, justified by asymmetry (storage has a
sync option on every platform: `localStorage` / `std::fs`;
clipboard reads did not). `localStorage` (~5 MB, backend-side
UTF-8) is the named current wasm answer; IndexedDB/OPFS are
OQ-G5-3 with the bridge shape sketched, not silent.
217. **Seam shapes.** Bytes core-side (backends encode);
flat non-empty keys (empty loudly `InvalidKey`); lexical jail
(relative + `..`-free, absolute refused — symlink escape is the
documented bound, OQ-G5-4); `NotFound` is a query outcome
(`Ok(None)` KV / `Err(NotFound)` FS); references in core
(memory ×2 + std `NativeFs`); per-platform roots named
(Android-private via future JNI, `%APPDATA%`, `~/.local/share`,
web storage) with shell `app_data_dir` deferred (OQ-G5-2).

Snapshot: 2026-09-27, V3 G4 packaging doc (decision 215,
docs-only, no code): `docs/06-platforms/packaging.md` — one blessed
path per target (Android cargo-apk dev/test + Gradle release +
aapt2 fallback; web pinned-bindgen + static serve; Windows/Linux
release binaries; Apple explicitly no-path; installer formats open;
machine-local debt pointer) + one pointer bullet per platform
overview. `cargo fmt --check` clean; suite untouched since G2.
Details: `rounds.md` V3-G4 entry.
215. **Blessed packaging paths.** Dual Android (cargo-apk dev/test,
manual — env signing against the machine-local test keystore;
Gradle release, proven — dual-ABI jniLibs, NDK r29), pinned-
bindgen web (0.2.128, `pkg/` untracked-regenerate), release
binaries desktop, Apple absent (no shells — a new-platform round,
not a packaging edit). Installer formats (.msi/MSIX/.deb) stay
open with no invented recipes.

Snapshot: 2026-09-27, V3 G2 control catalog, first four
(decisions 212–214, `crates/oppa-controls` new + additive core/
emitter arms): Button/Checkbox/Toggle/Slider as `fn(&Ctx, &P) ->
VNode` composing `Div`/`Text`/handlers/`Semantics` (no new `Tag`);
controlled signals, `ctx.child` use, press-only (Slider =
steppers), disabled structurally handler-less, explicit default
sizes; `Role::Button/Checkbox/Slider` + `value_text` with ARIA/
AT-SPI/UIA arms (Checkbox Toggle pattern; Button Invoke + Slider
RangeValue deferred). 9 control tests + emitter/builder tests;
per-control pages + overview current. Touched suites all green
(oppa lib 91/0, controls 9/9, dom 29, atspi, uia); clippy clean
save `FpsApp` (184); fmt clean. Open: OQ-G2-1 (slider drag/arrows),
OQ-G2-2 (Invoke/RangeValue + AT-SPI value), OQ-G2-3
(Dialog/Menu/overlay), OQ-G2-4 (uncontrolled). Details: `rounds.md`
V3-G2 entry.
212. **Catalog mechanism (compose, don't branch).** New
`oppa-controls` crate (`oppa`-only dep) — controls are component
functions over the proven vocabulary; zero reconciler/layout/
backend changes by construction. `Action = Rc<dyn Fn()>` keeps
props `Clone + 'static` (hot boundary, §5.1).
213. **Control conventions.** Controlled state (locked #24);
`ctx.child` use (own hover/press/focus flags per instance — M8/F6);
press-only interactions (pointer + M5 Enter/Space; Slider steps via
two Button steppers); disabled drops the handler structurally (no
tab stop — decision 96, never a silent no-op handler); explicit
default sizes (96×32 buttons, 20×20 boxes, 160×32 slider) so
headless trees get hit boxes without a text service.
214. **Semantics extension.** `Role::Button/Checkbox/Slider`,
`Semantics::{button,checkbox,slider}`, `value_text: Option<Arc<str>>`
(zero-dep — formatting is the control's job); emitter arms total on
all three legs (ARIA roles + `aria-valuetext`; AT-SPI names +
Checkbox `checkable`; UIA types + Checkbox Toggle); Button Invoke
+ Slider RangeValue + AT-SPI numeric value deferred (OQ-G2-2).

Snapshot: 2026-09-27, V3 G3 clipboard (decisions 209–211,
`crates/oppa` + `crates/oppa-shell-win`): `Clipboard` trait
(`src/clipboard.rs`) — `Result`-returning writes, request/poll
async-capable reads, `read_text_now` surfaces `Err(Pending)` instead
of blocking, plain text only; `PlatformShell::clipboard()` seam
(default `None` — pre-G3 shells compile untouched, refuse loudly);
`EditSession` copy/cut (`Result<Option<String>>`) + `paste_from`
(`PasteOutcome::{Pasted, Empty, WhileComposing}`, discrete undo
unit, zero-mutation on pending); `Win32Clipboard` (`CF_UNICODETEXT`,
RAII close guard, empty-write clears) owned by `Win32Shell`, proven
by a real-OS save/round-trip/restore test. 12 new tests (3 core + 8
session + 1 Win32). Touched-crate suites all green (oppa lib 90/0,
shell-win 1/1 real OS, shell-linux 9, shell-android 7+5, spike
session 10/10); clippy clean save intentional `FpsApp` (184); fmt
clean. Open: OQ-G3-1 (Linux backend), OQ-G3-2 (Android JNI),
OQ-G3-3 (web async settler + permissions), rich formats out.
Details: `rounds.md` V3-G3 entry.
209. **Clipboard trait shape (async-capable, sync-friendly).**
`write_text`/`clear` return `Result<(), ClipboardError>` (mid-round
correction: a `()` write would swallow routine transient lock
failures silently); reads split `request_read` + `poll_read`
(`None` = unsettled async, `Some` = settled/refused — the web
promise shape without an executor); `read_text_now` =
request + one poll, `Err(Pending)` instead of blocking (the UI
thread can never hang on a read). Plain text only (rich formats a
later round). `ClipboardError::{Unsupported(&'static str shell),
Pending, Backend(String)}` — all loud, all `Display`.
210. **Shell seam + Win32 scope.** One additive default method,
`PlatformShell::clipboard() -> Option<&mut dyn Clipboard>`
(`None` default — Linux/Android/spike/test shells untouched, refuse
loudly through `Unsupported`). Win32 wired this round
(`Win32Clipboard`: `OpenClipboard(None)` UI-task association,
`EmptyClipboard` before set, `GMEM_MOVEABLE` alloc with free-only-
on-failure ownership, `CloseGuard` RAII on every path, empty write
clears, `from_utf16` loud refusal); Linux/Android/Web deferred
(OQ-G3-1..3).
211. **Session copy/cut/paste semantics.** Collapsed/composing/empty
→ `Ok(None)`, clipboard untouched; write failures propagate
(`Err`, retry next frame); empty/absent paste → `Ok(Empty)` with no
undo entry; composing paste → `Ok(WhileComposing)` (platform owns
the field, decision 207); pending → `Err(Pending)` with zero
mutation; successful paste breaks the open coalescing run first
(discrete undo unit). Every no-op names itself (`PasteOutcome`) —
callers never guess whether the paste landed.

Snapshot: 2026-09-27, V3 G1 product editing sessions (decisions
205–208, `crates/oppa` only): `EditSession` (`src/editing.rs`) —
cloneable per-instance handle over author-owned
`Signal<SharedString>` (locked #24) with core-side
caret/selection/composition + bounded multi-level undo/redo (depth
32, coalesced runs, atomic composition commits); `Ctx::edit_session`
(call-site-keyed, survives hot swap); `bind_edit_session` (U8 text
feed), focus-change auto-commit + `notify_edit_focus_lost` (locked
#27 — corrects the spike's pre-merge cancel; spike evidence
`focus_loss_cancels_per_session_policy` now stale, spike untouched);
shaper-optional with loud installed-shaper failures + char-boundary
flooring. 20 new tests (FakeService spike parity) + 3 host-binding
tests. `cargo test -p oppa` green (lib 79/0, all suites 0 failed);
clippy clean save intentional `FpsApp` (184); fmt clean. Open:
OQ-G1-1 (raw signal writes bypass undo), OQ-G1-2 (router focus for
handler-less fields — decision 96 stands), OQ-G1-3 (CJK dictionary
segmentation). Details: `rounds.md` V3-G1 entry.
205. **Edit-session residence/API (per-instance handle).**
`Ctx::edit_session(content: Signal<SharedString>) -> EditSession`,
keyed by call-site source-hash + ordinal (same re-seed rule as
`ctx.signal`); stored in host-side `InstanceRecord` (survives hot
swap — only props drain, §5.1); first run's signal wins (same init
rule). `&self` methods throughout (shared-handle idiom, like
`Signal`/`ScrollOffset`) so host iteration behind shared refs works.
206. **Bounded multi-level undo/redo.** Depth 32 each
(`EDIT_UNDO_DEPTH` — reasoned bound, not derived law). Contiguous
collapsed `insert` runs coalesce; consecutive collapsed `backspace`
/ `delete_forward` runs coalesce same-direction; `delete_selection`
is always discrete; composition commits push nothing (the
pre-composition snapshot is the atomic unit) but still clear redo;
any other op (caret/selection moves, undo/redo, focus, platform
feed) breaks the run; new edits clear redo. Empty-stack
undo/redo and op no-ops (empty insert, backspace at 0,
delete-forward at end, collapsed delete) are quiet no-ops.
207. **Shaping-optional session geometry.** No shaper installed →
pointer-mapped ops (`click/shift-click/drag/dbl-click`,
`cluster_leading_x`, `caret_rect`) are graceful no-ops (`None` /
`0.0` / unchanged) — headless-without-service is setup state, not a
wiring bug. Installed shaper failing on non-empty text → loud panic
(backend contract). Every byte offset is floored to a char boundary
(`clamp_byte`) so platform offsets can never panic a slice.
Content edits (`insert`/`delete_*`) are no-ops while composing (the
platform owns the field until commit/cancel — spike parity).
208. **Focus/feed wiring.** `notify_focus_lost` commits the active
composition with its current text (locked #27); `set_focus_node`
calls it on real focus changes only (additive no-op when nothing
composes — M5 behavior untouched); shells with external focus call
`notify_edit_focus_lost` directly. `apply_platform_text` is the
programmatic full-value feed (one undo entry, composition dropped,
caret to end; identical-value no-op). Routed `InputEvent::Text`
reaches session content via `bind_edit_session` (over `bind_text`,
decision 188); caret/selection clamp lazily against the composite
(level-triggered feeds self-heal).

Snapshot: 2026-09-27, v2 handoff (`docs/HANDOFF-V2.md`):
16 verified remaining gaps G1–G16 for real-app developers
(P0: spike-only editing, no catalog/clipboard/packaging/
persistence-network; P1: navigation, app-async, image
decode, font fallback, DPR plumbing, touch, desktop
integration; P2: a11y/reload/test-seam/perf-contract),
each with file:line evidence; three v1 corrections
recorded; order suggested, no decisions minted. Details:
`rounds.md` v2-handoff entry.

Snapshot: 2026-09-27, cross-platform FPS counter (user-asked
example, decisions 203–204): new `oppa-fps` crate runs one
ComponentHost scene on Windows, Linux, Android, and Web —
shared app core (live-size recenter per 202) + native winit
driver (GPU-then-loud-CPU) + standalone Web rAF driver, same
bundled DejaVu bytes shaping/rasterizing everywhere.
Framework additions, all additive: `oppa-fonts`, rustybuzz
`from_bytes_with_chain` + `face_bytes`, `web_time` timings
in oppa-vello. Verified table (release, screenshots):
Windows Vulkan/Immediate 1008; Linux llvmpipe
Vulkan/Immediate 132; Android emulator CPU fallback 8
(Vulkan absent/limits); physical RMX3370 Adreno 650
Vulkan/Mailbox 215 release (123 pre blitter-cache, 62
debug — per-frame `TextureBlitter::new` was 50-65% of the
frame on Adreno; `PresentReport` now carries stage walls
and `GpuCtx` caches one blitter per format; 60Hz system
cap noted); Web Firefox CPU canvas 145, open-browser
Chromium WebGPU path verified (BrowserWebGpu adapter,
rAF vsync-paced; stage means over 5s window for 1ms
timer). Full
serial green save the known m10_gles load flake; clippy
(+wasm target) and fmt clean. Details: `rounds.md`
cross-platform entry; decisions 203–204.
201. (No entry was ever recorded under this number; the
briefing cited it as precedent and code never referenced
it. Skipped to keep numbering truthful — next is 202.)
202. **Swapchain must track the live client size (uncapped-blank
root cause).** The Vulkan Mailbox/Immediate blank + 1:1
Ok/Outdated storm was resize-triggered, not driver-caused:
any resize (user, harness move, minimize) desynced the
800x600-constant swapchain from the client, and every
reconfigure rebuilt the same wrong size. The loop now reads
live client size, reconfigures + rebuilds the scene surface
(create-before-publish) + refits viewport + recenters text
on change; the Outdated arm reconfigures live; sustained
storms log loudly. `install_vello_paint_hook_shared`
(Rc-cell target; old fn delegates). Corrects the interim
driver-block theory (disproven — never recorded).
203. **Cross-platform FPS example shape.** `oppa-fps`: `app.rs`
shared core (scene, EMA, recenter, CPU present helpers),
`driver.rs` native winit loop (Windows/Linux/Android),
`web.rs` rAF loop, `clock.rs` (`AppClock` over the
framework `Clock` seam — `SystemClock` panics on wasm by
construction), `android.rs` (`android_main` + builder ext),
bundled DejaVu via `oppa-fonts`. Demos stay separate:
`oppa-fps-demo` is the Windows lab. winit is example-owned
(the per-platform shells remain the blessed app paths; a
blocking pump cannot serve the Web leg).
204. **Web platform findings.** `Instant` panics on wasm
(example clock seam, no framework change); WebGPU
bring-up is async (`block_on` would hang — CPU-direct,
follow-up recorded); winit 0.30.13 web panics at window
creation (observer-setup `RefCell` reentrancy — hand-rolled
driver); winit appends its canvas only with
`with_append(true)`; bare-`fn` bindings against DOM
properties throw silent fatal traps (use functions;
`window.onerror`→title + `#oppa-status` div keep the
headless page observable).

Snapshot: 2026-09-27, fps-demo startup arc (user-asked demo +
two additive framework APIs, decision 199): `oppa-fps-demo`
(white window, black centered live FPS, Vello present) surfaced
a ~10s blank-unresponsive start; measured split (adapter 252ms,
device 143ms, `Renderer::new` 9.7s, first present 33ms);
~22 eager vello pipelines x slow Dx12 compile (probe: heavy
shader 1040ms Dx12 vs 246ms Vulkan; Renderer::new 15.5s vs
1.9s); `ensure_gpu_for_surface_with_cache` added (old path
identical) with verified-negative Dx12 persistence; demo
prefers Vulkan (disk cache works) with Dx12 fallback;
`ShellConfig.visible` + `show()` for appear-with-content.
Cold Vulkan 2.7s, warm 1.1s (was 7–16s Dx12 every launch).
Details: `rounds.md` fps-demo entry; decision 199.
200. **CPU glyph rasterization (demo arc).**
`CpuBackend::set_font_bytes`/`set_font_for` (mirroring
Vello's names/semantics, M7 decision 110): runs with a
usable face rasterize real ab_glyph coverage (positions
mirror the Vello encoder -- origin at `x + g.x,
y + baseline`, unhinted, `font_size = em_size`, empty
outlines ink nothing); runs without one keep legacy
advance-cell bars, so every pre-existing oracle expectation
holds byte-identically. Invalid bytes rejected at
registration (loud), never at paint; clips honored by
rect-stack testing. Suite: DejaVu include (no new
binaries), charmap-derived ids, coverage/position/bars/
determinism asserts. No atlas cache in v1 (stated bound --
per-frame re-raster is microseconds for demo-size text).

Snapshot: 2026-09-26, v2 item 2 paragraph shaping (this round —
framework change, decisions 193–198): `oppa-linebreak` crate
(unicode-linebreak 0.1.5, zero tailorings) behind the core
`BreakSource` trait; `layout_text_with_breaks` (opportunity-only
wrap, over-wide push-whole, soft-trim, forward affinity) beside
the untouched greedy default; per-`\n`-paragraph shape+stitch in
the engine; RTL cluster-range repair in the shared core (counts
held, ranges did not); golden corpus (stub + DejaVu anchor +
dwrite, both classes) and the browser app leg (multi-line label
on screen, toggle + add, zero errors). 379/0 serial (saved log,
+35), clippy/fmt clean; WSL slices green. Details: `rounds.md`
item-2 entry; §5y; decisions 193–198.

Snapshot: 2026-09-26, F2 characterization (this round — no
framework change): no repro (200/200 host + 6/6 browser +
all dumps correct); timeouts decomposed to waiter flaws +
predicates; verdict is harness noise, decision 191
withdraws the item-7 referral, M9 gate stands, `f2_probe`
retained as tripwire with dump-before-claim rule. Item 2
unparked next. Details: `rounds.md` F2 entry.

Snapshot: 2026-09-26, scoped sizing + finish (this round):
decision 189 scoped the fix (space-measured empty fields +
position-only zero-box inputs — no paragraph machinery, stop
condition never triggered); temp todo app finished end to end
(native type → 8 rows → toggles → clear-done removal, zero
console errors); friction round 2 recorded (zero-space
overlap + wrapper pattern documented; press-dispatch flake
flagged to item 7 as decision 190, not fixed here); WSLg
item confirmed already-closed (decision 185 stands, no new
work). 343/0 serial (+2), clippy/fmt clean. Details:
`rounds.md` sizing/finish entry.

Snapshot: 2026-09-26, U8 text entry (this round — framework
change, decision 188): `InputEvent::Text` feed-only channel +
`bind_text`/`text_fields()` + `node_for_pid` + oppa-web
`text()` binding with swap-preserving bootstrap; proven
core 5/5, backend pid round-trip, and browser-native
(type → observe → add row → focus/value survive unrelated
swap, zero console errors). 341/0 serial (+6), clippy/fmt
clean. Details: `rounds.md` U8 entry; spec
`v2-textentry.md` (Q1–Q3 answered, Q4 deferred, Q5 closed).

Snapshot: 2026-09-26, usability round (this round — docs only,
one demo comment): outsider todo app built out-of-repo and
driven in headless Edge (1/2 → 1/3 → 2/3 → 2/4, zero console
errors — no monorepo-only build assumption found); stuck
points fixed (getting-started rewritten, web-app.md new,
widget.md corrected, web overview + README entry pointers);
WSLg sustained-present death root-caused ENVIRONMENTAL
(Weston libpixman SIGSEGV ×2 in dmesg + restart stamp in
weston.log + dual-client simultaneity — matches
microsoft/wslg#1386; no shell/demo code change). 335/0
serial, clippy/fmt clean. Details: `rounds.md` usability
entry; decisions 182–185.

Snapshot: 2026-09-26, v2 open (this round, spec only — no code):
environment re-verified (adb: emulator-5554 only, no phone;
Windows cargo 1.97.1 with `cargo test -p oppa --lib transition`
6/6 sanity green; WSL rust 1.98.1 via login shell), item 1
(TIME/keyframes) proposed as the opener, one-page spec written
at `docs/04-planning/v2-keyframes.md` (coverage + proof question,
mechanical acceptance, boundary, open questions Q1–Q6). Full
suite not re-run (tree unchanged — 335/0 stands). Details:
`rounds.md` v2-open entry; decision 181.

Snapshot: 2026-09-26, v1 close-out (this round): every v1
residual fixable on Windows + emulator is closed — emulator
re-run on the final APK (oracle, GL decomposition second
sample, 6 presents, both taps with screencaps, text 15/15),
Linux input mapping (7 contract tests, demo wired), stale-APK
verification rule. 335/0, clippy/fmt clean. Details:
`rounds.md` close-out entry; decisions 177–180.

Snapshot: 2026-09-26, phone round (this round): the user
connected a Realme GT Neo 3T (Snapdragon 870 / Adreno 650) and
the missing measurement walked in — arm64 executes, Adreno
Vulkan oracle exact-0 with cross-ISA SHAs intact, full-scene
render+readback 86–88 ms at 1080x2400, visible present + both
taps on-screen, text 8/9 with font-drift isolated, Adreno GLES
wall mapped. Bet verdict in `08-performance/mobile.md`
(narrowed, hatch stays costed) + decomposed (16.7 steady
render-only — decision 176). Details: `rounds.md` phone
entry; decisions 169–176; `HANDOFF-V1.md` §5.

Snapshot: 2026-09-26, v1 remainder (this round): every gap
closable without a physical phone is closed — Android arm64 +
swapchain present (blit deleted), Android text slice (JNI +
rustybuzz over device fonts, byte-exact), Android touch + IME
on device (adb taps flip on-screen, M1 shapes survive, IMM
policy), Linux shell + text (WSLg window, DejaVu measures, CPU
paints, no sudo), UIA event raising (HWND host + client pump,
toggle flip observed), web app story (wasm + bootstrap + M7
harness green). One framework bug fixed (ownerless hover
panic — decision 165). The phone-owned frame cost has since
been measured on Snapdragon 870 (phone round above) and the
bet narrowed — weak-tier silicon + the incremental loop are
what remain open.

Snapshot: 2026-09-26, v1 handoff: `docs/HANDOFF-V1.md` written —
the one read-once artifact (what this is, per-platform proof
with artifacts, decisions, v2 deferrals, the single genuinely
open item, declined-not-open items, doc debt, where to look).
v1 is closed except weak-mobile-GPU frame cost (no phone).

Snapshot: 2026-09-26, M10 Android-gap closure (same day): the
emulator is up (Medium_Phone_API_36.1, left running) and the
device-owned rows are measured, not assumed — API 36, x86_64
ABI, SwiftShader GLES 3.0 max (`ANDROID_EMU_gles_max_version_3_0`),
SELinux Enforcing, 1080×2400@420. Two corrections: the bet's
"GLES 3.1-class" phrasing is a 3.0 floor (decisions 142–143);
adb's daemon must be started detached (`start-server` via
`Start-Process` — synchronous start is what hung). Still
NDK-blocked: no NDK installed, MSVC target only (JDK 25
present); the unlocked recipe is `x86_64-linux-android`, API 36.

Snapshot: 2026-09-26, NDK installed (same day, after the gap
closure): r29 side-by-side via sdkmanager (licenses accepted),
Rust `x86_64-linux-android` + `aarch64-linux-android` targets
added, `oppa-shell-android` and `oppa-atspi` cross-`check` green
for Android x86_64 (real `target/x86_64-linux-android` artifacts
— decision 144). Toolchain gap closed; app-glue gap (Gradle +
activity + APK) remains.

Snapshot: 2026-09-26, on-device proof (same day, last): APK
(`crates/oppa-android-app`, aapt2/zipalign/apksigner, no Gradle)
installed and run on the visible emulator — CPU arm
byte-identical to host CPU pixels (cross-ISA determinism),
Vello-GL arm byte-identical to on-device CPU pixels (exact-0
oracle through the Android GLES stack, `-gpu host` 3.1), timings
recorded. SwiftShader walls precisely drawn (GLES3.0-no-compute,
Vulkan-16KB-UBO — decisions 145–146). v1 evidence chain now
reaches real Android pixels.

Snapshot: 2026-09-26, v1 closure (same day): everything closable
without a phone is closed — AT-SPI live-bus 25/25 on WSL
(real registryd 2.60), Gradle `assembleDebug` green and run,
Windows UIA provider with action round-trips, both
checkpoint-named staleness items fixed. Deliberately left open,
each with its reason: weak-GPU frame cost (no phone —
user-omitted), Android a11y service (Java+JNI package,
self-serving without TalkBack), text slices (JNI bridge +
platform-shaper comparison is a text milestone, not a gap).

Snapshot: 2026-09-26, after the M10 round: Android shell
(`oppa-shell-android`: intake classification into the shared
`InputEvent` pipeline, lifecycle machine, restart-only reload —
no `oppa-reload` dependency by construction), AT-SPI emitter
layer (`oppa-atspi`: total role/state table + incremental tree
mirror + wire vocabulary, Linux-only scope), and the GLES row
(GL-constrained Vello device holds the M6 oracle standard at
1080×2400: exact 0 / tol-16 0; CPU fallback measured at the same
standard). Restart == cold start proven pixel- and dump-identical.
v1 milestone chain complete; device-owned residuals named
(weak-GPU frame cost, live-bus validation, NDK/JNI + surface +
text slice + Android a11y service).

Snapshot: 2026-09-26, after the M9 round: the reload product loop
and fuzzer gate — swaps interleaved with mid-scroll (slot-keyed
list, INPUT-fed + TIME-fed), mid-transition (live interpolators at
swap), mid-IME-composition (core-side session buffer preserved),
mid-input-burst (6405 events, INPUT→RELOAD ordering per hook
frame), and in-flight async tasks (cancel/discard race hit 76/76
swaps). The mechanical property holds everywhere asserted (no
retired slot touched, no retired task applied, live evaluator
nodes all resolve); the generational checks are proven to fire
(`m9_generational_proof`). Gate DECLARED in
`docs/03-spec/reload/freeze.md`: renderers may freeze; TSF
machinery stays a platform-track residual (no generational surface
there — grep-verified). Findings: three test-side rig bugs found
and fixed in the round (char-boundary writer, settle-vs-live
timing, stamp measurement placement); zero engine violations on
three seeds. M2b verdict: working swap path, not scaffolding;
true unload stays deferred (decision 61 stands).

Snapshot: 2026-09-26, after the M8 round: the §4.2 payoff trace
asserted against real backends — recycled ContactList/ContactRow
(slot keys, window-tracking positions, keyed_state selection),
the TIME transition evaluator honoring the binding-edge stamp
(GPU interpolation + DOM CSS mapping with one-commit
suppression), window-lag compensation under a scripted offset
sweep. Locked #13 proven (0 structure ops/tick, 20 repainted
cells, identity selection); #22/§9.4 proven (flash count 0,
interpolators 0 on stamped commits); one overscan constant (4)
stands for both backends. Findings: F6 (inline-child handler
ownership) + tail-freeze + splice order + straddle margin found
and fixed in the round; slot-position CSS churn recorded as
follow-up. M5 frame counts reframed (commit-frame + tail, #7
untouched).

Snapshot: 2026-09-26, after the M7 round: third presenter on the
proven contract — the DOM backend (`oppa-dom`: TreeDiff→DOM
mutations, StyleId→CSS rules, §9.3 native scroll, ARIA incl. the
verdict-(b) text/edit path), sharing the M4 builder and no raster
code. Locked #2 proven; #23 INPUT-fed with ≤1-frame trail on the
injected clock. M6-carried text polish closed: exact em size +
per-run font identity ride every `DrawOp::Text` (decision 110, second
contract lock touch; the Vello single-face bound ends with it).
Finding F3 closed in full. Parity corpus measured, not folklored:
10/10 gated rows green in the flat subset (engine vs Edge,
untracked text width 81.03125 == 81.03125 exact) + 1 record-only
text-height row. Measured: scroll-tick structure ops 0 end-to-end,
offset trail ≤1 frame, three-backend rounded boxes identical.
Findings: F5 (render-boundary quote escaping) found and fixed in
the round; F3 closed.

Snapshot: 2026-09-26, after the M6 round: first GPU presenter — the
Vello backend on the M4-proved contract, driven by the same
dirty-subtree FramePlans the CPU backend consumes (same plans, second
rasterizer). Locked #17 proven; #21 observable and bounded; #18
serviced on a backend where the compositor is us. M5-carried opens
resolved: `Color` alpha stays opaque + separate opacity (decision
103, no representation change, cross-backend pixel proof); F1 closed
by engine-side PAINT stamping (decision 104). One contract lock
touch: `DrawOp::Text` gains `baseline` (decision 105; CPU ignores it,
GPU places at `y + baseline`). Tripwire verdict: PASS on evidence
(RTX 3060 Ti + fallback rows, glyph review vs the M4 baseline).
Measured: atlas delta 0.0, static-frame GPU work 0, strict-geometry
diff 0/0, curves tol-16 diff 12 (bound 60).

Snapshot: 2026-09-25, after the M5 round: first real widget — the
§4.1 Toggle end-to-end on the CPU backend driven by real `InputEvent`
payloads through framework primitives (hit-test walk + capture/focus
router in INPUT's `BatchGuard`), incl. its `Semantics::switch`
payload. Locked #7 proven; #3 exercised by a stateful interactive
widget. M4's forced Style questions resolved (border + ink as stated
fields, no backend change); alpha + F1 re-recorded open with owners.
Measured: cancel case clean, input→visual 1 frame.

Snapshot: 2026-09-25, after the M4 round: first runnable — one static
component through core→reconciler→layout→FramePlan→CPU backend→PNG plus
a SemanticsDiff dump (`oppa::render` contract types + `oppa-cpu`
tiny-skia backend + FramePlan builder from dirty subtrees + headless
image-diff oracle, 10 acceptance tests). Locked #5 proven
implementable (backend shares no core code beyond the contract types +
public retained reads). One genuine engine fix inside the round
(reconciler finding F2: fresh text leaves carried no dirty flags);
one documented limitation (finding F1: position-only LAYOUT moves do
not rebuild plans).

Snapshot: 2026-09-25, after the M3 round: framework-owned layout engine
(`oppa::layout`: flex subset + block-lite + absolute, inline wrap/BiDi/
optional ellipsis, `TextService` measure protocol with per-node cache,
one-frame-delayed settled metrics, shared DPR rounding at commit
positions), wired through the LAYOUT phase + `ComponentHost`
(core-side ledger, residence-safe), bidi visual ordering oracle-proven
(65.00px source divergence → ≤2px visual residual, locked #29 closed),
wrap measured at 1 shape + 0 re-shapes. No lock needed changing
(decisions 68–82 are interpretations; #6 fulfilled, #29 closed).

Snapshot: 2026-09-25, after the M2b round: hot-reload harness
(`oppa-reload`: manifest scan, real dylib swap, typed drain/adopt,
retire-not-unload, generation-tagged task executor), `#[component]`
call-site lint + `component_manifest!` + `#[hot_crate]` (oppa-macros),
per-runtime run stacks (core tracking fix), fuzzer v1 + real-dylib
swap test green. Locks #14/#25 proven; no lock needed changing
(two genuine soundness findings fixed inside the round: stale-code
re-runs and cross-image TLS tracking loss — decisions 60–66).

Snapshot: 2026-09-25, after the M2 round: reconciler + `#[component]`/`Ctx`/
`VNode` system headless-complete against the two locked §4 examples (toggle
+ virtualized ContactList/ContactRow, with six stated mechanical deltas
D1–D6, nothing silently reshaped); slot-keyed recycling, the one-commit
rebind stamp, LRU `keyed_state`, and source-hash signal reseeding all
proven by tests. DESIGN §2.3(b) partially closed as locked #29 (unchanged
by this round — no lock needed changing); DESIGN.md otherwise untouched.

Snapshot: 2026-09-25, after the bidi/combining/ZWJ round: corpus rig
v2 (bidi Latin+Arabic+digits, decomposed e-acute, ZWJ technologist)
measured on both arms; combining-mark cluster parity + ZWJ
single-cluster closed on both-arm agreement; bidi visual ordering
re-deferred to M3 (measured, not assumed); DESIGN §2.3(b) **partially
closed as locked #29** — the M3 visual-ordering deferral is the only
open freeze item.
Companion to `DESIGN.md` (v1, closed — the §9.2 verdict is merged as locked
#27 / §2.3's second text path) and `BUILD-ORDER.md`. This document records
what is implemented, what the tests actually assert, the interpretation
decisions made where the design was ambiguous, and the exact boundary
against later milestones.

---

## 1. Verification status

| Command | Result |
|---|---|
| `cargo test` (debug, whole workspace, `-j1` serial) | **335 passed / 0 failed** (was 328: +7 shell-linux input contract (9 = 7 input + 2 pack); uia lib 1 + uia_emit 1 + uia_events 1; rustybuzz unit 2 + shape_android 15; shape_linux host 2; oppa-web 1; rest per the v1-remainder entry). Parallel full run flaked once in `m10_gles` (wgpu-hal EGL context-lock deadlock + lock-poison cascade — known cross-binary GPU contention; 4/4 green in isolation and in the serial run). |
| `cargo test --release -p oppa --test m2_reconciler` | 11 passed (unchanged; M2 regression guard green — seam compatible both ways) |
| `cargo clippy --all-targets` | clean (0 warnings; ~13 fixed this round, all in M8 test files: bool asserts, unused imports/fields, snake-case components) |
| `cargo fmt --all -- --check` | clean |
| `cargo run -p spike-textedit --bin spike_ime_shell -- --ime-pass` | the automated real-IME pass ran and recorded `spike/results/ime_manual.json` (75 raw Win32 log rows incl. 17 step markers, per-step observables, environment + TSF/store activation facts); verdict **PASS 6/6, zero divergences — second consecutive hands-off PASS** (repetition run; DESIGN §2.3(a) closed as locked #28 on this evidence; §5e) |

Dependencies: the core crate `oppa` is **zero-dependency** (std only);
`oppa-macros` is a zero-dependency proc-macro crate (`#[component]` +
`#[derive(Props)]` + `component_manifest!` + `#[hot_crate]`,
dev-dependency of `oppa` for the M2 acceptance tests);
`oppa-reload` adds `libloading` 0.9 (the harness; dev-deps oppa-macros
+ hot-fixture rlib for types);
`oppa-text-dwrite` adds `windows` 0.62 (Windows only, by definition);
`oppa-shell-win` adds `windows` 0.62 features (the shell);
`oppa-cpu` adds `tiny-skia` 0.12 (the M4 backend; dev-dep
`oppa-text-dwrite` on Windows only, for the advance-fidelity test);
`oppa-vello` adds `vello` 0.10 / `wgpu` 29 / `pollster` 0.4 (the M6
backend; depends on `oppa-cpu` for the shared FramePlan builder +
`tiny-skia` for the oracle's CPU arm; dev-dep `oppa-text-dwrite` on
Windows only, for the atlas/oracle pixel tests);
`oppa-dom` adds nothing (std + `oppa`; structural `oppa-cpu` dep for
the shared FramePlan builder in the paint hook; dev-deps
`oppa-macros` + `oppa-vello` + `vello` 0.10 for the box-compare/encode
tests; Windows-only dev-dep `oppa-text-dwrite` for the real-font rows);
`spike-textedit` adds `windows` 0.62 features + `vello` 0.10 / `wgpu` 29 /
`raw-window-handle` / `pollster` (the debug renderer + the pass driver).
The spike's Web-DOM arm harness (`spike/web/`) is dev-only Node tooling
(`puppeteer-core` driving system Edge); it is not part of the Rust
workspace. M7 adds two scripts there (`parity.mjs`, `dom_text.mjs`;
same substrate). Edition 2021, rustc 1.97.

The M1 merge round changed only the three documents; the M1 remainder
round added the shell crate, the host binary, the two additive session
ops + the composition-start accessor, and the DWrite debug hook — the
core's test surface is unchanged.

---

## 2. Workspace layout

```
Cargo.toml            virtual workspace (members below)
DESIGN.md             design reference (v1, closed)
BUILD-ORDER.md        milestone plan
STATE.md              this file
ROUNDS.md             round delta history
spike/
  corpus.json         the shared rig (generated by the Windows arm runner)
  results/            windows.json / web.json / verdict.json (the spike's raw evidence)
                      + parity_page.html / parity_expected.json / parity.json
                      + dom_field.html / dom_text.json (M7's raw evidence)
  web/
    index.html        the instrumented real <input> field (hook = recorder only)
    harness.mjs       Web-DOM arm driver (puppeteer-core + headless Edge)
    compare.mjs       verdict computation + per-criterion records
    probe.mjs         font/origin calibration probe (diagnostics)
    parity.mjs        M7 §8.5 corpus driver (oppa-dom page vs Edge rects)
    dom_text.mjs      M7 verdict-(b) suite driver (shared ops vs real <input>)
    package.json      dev-only Node tooling (puppeteer-core)
crates/
  oppa/               core crate (M0): reactive core, storage, scheduler, TextService contract    src/
      lib.rs          module declarations + crate-root re-exports
      arena.rs        GenArena<T>, GenerationalId, SlotError, NodeId, NodeArena<T>
      pass_mask.rs    PassMask (STRUCTURE|STYLE|LAYOUT|PAINT|TEXT|SEMANTICS)
      hash.rs         SymbolHash (FNV-1a 64), fnv1a64
      handlers.rs     HandlerId, HandlerFn, HandlerRegistry
      interner.rs     Interner<T> → StyleId
      worker.rs       HotGeneration, WorkerResult, WorkerQueue
                      (+ M2b: TaskScope/TaskResultItem, TaskPump executor,
                      WorkerResult::from_box; HotGeneration repr(transparent))
      shell.rs        PlatformShell trait (M0 subset + set_ime), Event, EventKind
      clock.rs        Clock trait, SystemClock, MockClock
      lint.rs         check_no_ambient_state (const) + unit tests (M2b §9.6)
      style.rs        Style/StyleBuilder/IntoPx, Color, Transition/Ease, MsExt, Shadow, Px (M2)
      semantics.rs    Semantics (switch/list_item builders), Role (M2)
      vnode.rs        Tag (closed set + Custom), Element/ElementBuilder, VNode,
                      Div/Row/Stack/ScrollArea/Column/Text/Img constructors (M2)
      component.rs    Props, OpaqueProps (+try_get), Store<Id,V>, ImageCache,
                      ScrollOffset, Ctx (+spawn), ComponentHost, MountHandle,
                      RenderFn, InstanceSnapshot (M2; M2b: reload_snapshot,
                      take/set_props_raw, mark_all_component_effects_dirty,
                      evict_instance, assert_no_outgoing_props,
                      set_render_table, mount_erased)
                      (+ M4: with_retained_mut, diffs_from — the paint-pass
                      surface)
      reconciler.rs   RetainedNode, TreeDiff/DiffOp, slot-keyed diff (M2)
                      (+ M4: retained_ids, take_paint_masks, diffs_from;
                      fresh text leaves get STRUCTURE|LAYOUT|PAINT — F2)
      reload.rs       ComponentDesc, ManifestView, DrainedProps,
                      DrainPropsFn/AdoptPropsFn (M2b manifest ABI)
      render.rs       PresenterKind, Caps, SurfaceDesc/Id, DrawOp,
                      FramePlan (+PlanStats), DamageRect, SemanticsDiff/
                      Snapshot + compute + dump, BackendError, PaintStats,
                      RendererBackend trait, INK (M4 contract types)
      text.rs         TextService trait + ShapedRun/TextRun/Cluster/TextStyle +
                      glyph↔byte mapping, caret math, DPR rounding (M0b)
      ime.rs          ImeCompositionEvent/Handler/Feed, ImeOps, dispatch (M0b)
      reactive/
        mod.rs        Runtime + Signal<T>/Memo<T>/Effect/BatchGuard + untrack;
                      frame loop, phase fns, propagation engine orchestration
                      (+ M2: mark_memo_binding, take_binding_fired,
                      keyed_state store API, mark_effect_dirty, Effect::id)
                      (+ M2b: spawn_task, drop_pending_tasks, per-runtime
                      run_stack in shared state, INPUT task-outbox drain,
                      stats tasks_done/tasks_dropped)
                      (+ M4: set_paint_pass + real paint_phase — same hook
                      shape as set_layout_pass)
        state.rs      RuntimeState (arenas, edge map, pending, pass ctx, stats)
                      + pass keys/heap, budget violation + cycle finder
                      (+ M2: MemoNode.is_binding, binding_edge_fired,
                      KeyedStore with capacity-64 LRU)
                      (+ M2b: run_stack, task_pump)
    tests/
      propagation.rs           18 tests — #19 propagation contract
      scheduler_on_demand.rs   10 tests — #18 on-demand loop
      storage_generations.rs    6 tests — #11 generation checks
      handler_registry.rs       8 tests — #11/#5.3 registry keying
      m2_reconciler.rs         10 tests — M2 acceptance (toggle + list ports,
                               cycle budget ×2, recycling, rebind stamp,
                               keyed_state ×3, reseeding, opaque props)
  oppa-macros/        M2 authoring macros (zero-dep proc-macro crate)
    src/lib.rs        #[component] (checked pass-through + M2b call-site lint:
                      nested-fn + conditional/loop creation calls; combinator
                      bodies spared by design) + #[derive(Props)] (Props
                      marker impl) + component_manifest! (manifest export +
                      render/drain/adopt glue, optional `export`) +
                      #[hot_crate] (compile-time ambient-state lint) +
                      string-level unit tests (13)
  oppa-reload/        M2b hot-reload harness (libloading 0.9)
    src/lib.rs        crate docs (swap protocol) + re-exports
    src/source.rs     ComponentSource trait, StaticSource, DylibSource
    src/registry.rs   HotRegistry (install/request_swap/arm/reload_to),
                      ReloadReport, Evicted/EvictReason
    tests/reload_cycle.rs   8 tests — static-manifest swaps (keep/reseed/
                                survival, eviction ×2, registry, tasks,
                                keyed survival, hook path)
    tests/fuzz_reload.rs    fuzzer v1 (seeded xorshift, keep/reseed model,
                                exactly-once task accounting)
    tests/real_dylib.rs     real cdylib swap (adopt/reseed/tracking/
                                discovery/registry across images)
    fixture/hot-fixture/    excluded cdylib fixture (v1 + --features v2;
                                types shared with tests as rlib)
  oppa-text-dwrite/   Windows DirectWrite backend (M0b, first real TextService)
    src/lib.rs        DWriteTextService: enumerate/shape/measure via
                      factory + analyzer + system collection + fallback
    tests/shape.rs    13 tests — hand-checked shaping against Segoe UI
  oppa-shell-win/     M1 remainder: the minimal Windows window shell
    src/lib.rs        crate-root re-exports (+ ImeState, + TsfBridge/TsfStatus)
    src/tsf.rs        the TSF-aware path: ITfThreadMgr activation +
                      ITfDocumentMgr association + IS_TEXT input scope
                      (TSF round) + `ShellStore` (text-store round: full
                      `ITextStoreACP` + `ITfContextOwner` +
                      `ITfContextOwnerCompositionSink` via owner QI) and
                      the edit/composition sinks translating TIP
                      transactions into the IMM-shaped `ImeMessage`s
    src/win.rs        Win32Shell: the real window proc wired to the M0
                      PlatformShell trait (pump_events + set_ime), the
                      real OS IME message handling (WM_IME_* snapshotted
                      at message time via ImmGetCompositionStringW),
                      candidate-window anchoring via
                      ImmSetCompositionWindow/ImmSetCandidateWindow
                      (the M0b contract surface wired for real), TSF
                      enablement (`enable_tsf` + per-step focus re-assert)
  spike-textedit/     §9.2 spike: Windows-GPU arm + shared rig (M1)
    src/lib.rs        module decls (rig, session, oracle, json)
    src/rig.rs        the single-source-of-truth corpus: strings, cluster
                      tables, IME scenario scripts, editing-op suites
    src/session.rs    EditingSession: the framework-authority editing model
                      (Signal-backed content, caret/selection/undo, the
                      ImeCompositionHandler sink, set_ime anchor emission)
                      + the M1-remainder additive ops (extend_caret,
                      select_all) and the composition_start_byte accessor
    src/oracle.rs     criterion-1 references: IDWriteTextLayout
                      HitTestTextPosition + a real Win32 EDIT control
    src/json.rs       minimal JSON writer (rig is zero-new-deps beyond windows)
    src/bin/spike_win_arm.rs   runner → spike/corpus.json + spike/results/windows.json
    src/bin/spike_ime_shell.rs M1 remainder host: one editable field in the
                      real window, the real-IME mapper (real Win32 IME
                      messages → normalized events → dispatch_ime_event →
                      the session), the Vello debug renderer (text via
                      ShapedRun glyphs + caret + selection highlight +
                      composition underline), the automated pass driver
                      (--ime-pass) → spike/results/ime_manual.json
    tests/session.rs  10 tests — session regression (word rule, undo,
                      composition anchoring, delete-range, canonical stream)
  oppa-cpu/         M4 CPU backend (tiny-skia 0.12) + FramePlan builder +
                    image-diff oracle (first runnable presenter)
    src/lib.rs        crate docs (the #5 proof statement) + re-exports
    src/builder.rs    FramePlanBuilder (incremental from FRAME_MASK drain +
                      ancestor closure; full ignoring masks; damage union;
                      ScrollArea clips; RRect/Circle/Shadow/Text/RImg ops)
    src/backend.rs    CpuBackend (RendererBackend impl: per-surface pixmaps,
                      NodeId-keyed registry, retained-op replay, PNG encode,
                      pixel accessor, loud RImg/unknown-surface refusals)
    src/oracle.rs     OracleSession (incremental vs full byte-compare) +
                      image_diff_count
    src/hook.rs       install_paint_hook (PAINT-phase build + ordered commit)
    tests/m4_cpu.rs   10 tests — static PNG spot checks, plan minimality
                      (empty + text-subtree), oracle (static + history),
                      SemanticsDiff (toggle + list), determinism, contract
                      surface (clip/layer/surfaces/refusals), RImg refusal,
                      paint-phase wiring, DWrite advance fidelity (Windows)
                      (+ M6: text-subtree damage 1→2 for the F1 stamp)
  oppa-vello/       M6 Vello backend (vello 0.10): first GPU presenter
    src/lib.rs        crate docs (the #17 proof statement) + re-exports
    src/atlas.rs      GlyphAtlas (M7: default face + per-id faces;
                      explicit→default→loud selection; placement log
                      names the face per glyph)
    src/encoder.rs    FramePlan → vello::Scene (full DrawOp coverage;
                      loud RImg/no-font refusals; scene-side layer opacity)
    src/backend.rs    VelloBackend (RendererBackend impl: per-surface
                      scenes, NodeId registry, retained-op replay,
                      vsync present ledger, skew bound, GPU readback)
    src/oracle.rs     GpuOracle (CPU-vs-Vello pixel compare: exact +
                      tolerance-banded + ink-column diffs)
    src/hook.rs       install_vello_paint_hook (PAINT-phase wiring)
    tests/m6_vello.rs 18 tests — DrawOp coverage, RImg/no-font refusals,
                      Caps, contract surface, unchanged-surface skip,
                      skew bound, TIME cadence, F1 stamp, box determinism,
                      opacity encode, Vello paint hook (headless); adapter
                      matrix + atlas fidelity + geometry/text oracles +
                      glyph review + alpha pixel proof (Windows + GPU)
                      (+ M7: the hand-built Text literal carries em_size +
                      one FontRun — decision 110; multi-face atlas:
                      `set_font_for` + explicit→default→loud selection)
  oppa-dom/         M7 DOM backend (std + oppa + oppa-cpu): third presenter
    src/lib.rs        crate docs (the #2/#23 proof statement) + re-exports
    src/css.rs        StyleSheet (StyleId→stable `.s{bits}` rules; static
                      decls incl. inset-ring box-shadow; structural fields
                      emit nothing) + unit tests
    src/aria.rs       Semantics→ARIA table (switch/listitem/implicit
                      textbox; checked/selected/label/disabled) + unit tests
    src/dom.rs        DomBackend (RendererBackend impl: NodeId registry,
                      retained-read sync, overflow container + spacer +
                      slots, foreign elements, scrollTop ledger, loud
                      Image/unknown-surface refusals)
    src/page.rs       deterministic full-page render (sheet inlined +
                      slot anchor-off + spacer rules + data-pid hooks)
    src/hook.rs       install_dom_paint_hook (shared-builder PAINT wiring)
    tests/m7_dom.rs   27 tests — mutation minimality (mount/update/
                      reorder/remove/scroll-tick zero), CSS identity +
                      churn + no-inline-spam, scroll shape + overscan +
                      ≤1-frame currency, ARIA switch parity + removal,
                      verdict-(b) input + hole, em/fonts on three arms,
                      three-backend box compare, Caps third row, surface
                      refusals, paint hook; per-run Vello encode +
                      Edge parity corpus + Edge editing suite (Windows)
```


---

## 3. M0 — what is implemented (unchanged since the M0 snapshot, paths moved)

### 3.1 Generational storage (locked #11, §3.1, §9.6)

- `GenArena<T>`: slot arena with free-list reuse; **generation bumps on
  reuse**; `retire` returns the value; `get`/`get_mut` panic with the full
  refusing reason on retired/stale/OOB; `try_get`/`try_get_mut` return
  `SlotError`. No silent stale access, ever. `NodeId`/`NodeArena<T>` are the
  retained-node arena (the real `RetainedNode` payload lands with M2).
- Reactive nodes live in three separate arenas (signals/memos/effects) with
  typed handles.

### 3.2 The five reactive primitives (§7.9, semantics §9.1)

- **Signal<T>**: type-erased `Arc<T>` slots; `get` (T: Clone), `get_arc`,
  `set`, `update`. Writes bump `version` and invalidate per the contract.
- **Memo<T>** (`memo`, `memo_with_eq(f, cmp)`, `memo_named`): lazy
  computation, structural-`PartialEq` gate by default, lazily marked, settled
  in EFFECTS, pull-recompute on reads outside EFFECTS. Re-entrancy guard:
  closure taken during its run, restored after; nested re-entry panics.
- **Effect** (`effect`, `effect_named`): immediate initial run, re-runs in
  EFFECTS when dirtied.
- **BatchGuard** (`rt.batch()`): values apply immediately; the invalidation
  fan-out defers to the outermost batch end; nested batches merge; out-of-LIFO
  drops panic.
- **untrack(f)**: no dependencies recorded inside.

### 3.3 Propagation contract (locked #19)

- Heap-ordered passes keyed `(dependency depth, creation seq)`; one run per
  node per pass (`last_run_pass` stamped at claim; dirty consumed at claim so
  mid-run writes re-mark cleanly).
- Writes during a run: downstream not yet run → folds into the pass heap;
  downstream already ran → re-entry pass via `pending`.
- Reverse edges register immediately at read time so mid-run writes reach the
  running reader; `commit_deps` prunes stale edges and repairs depths.
- Budget 3 passes/frame; past it: debug → panic with the cycle chain
  (`cycle: cycler -> counter -> cycler` / `a_eff -> a_state -> b_eff ->
  b_state`); release → defer once to the next frame's EFFECTS (same budget),
  then park the unsettled set with a rate-limited log.
- **Memos never write signals** (or create effects): unconditional panic in
  all profiles — now DESIGN.md locked **#26**.

### 3.4 Frame loop (locked #18)

`TIME → INPUT → RELOAD → EFFECTS → LAYOUT → PAINT/COMMIT → A11Y`, on-demand
(`has_demand`: frame request, input, animation, reload, pending dirt, worker
messages). LAYOUT/PAINT are counting stubs. TIME advances the injected clock
and services the animation registry (the seam transitions/scroll physics plug
into). INPUT: shell pump + registry dispatch under one `BatchGuard` + the
worker-queue drain (generation-tagged; retired-generation results discarded).
RELOAD: phase position + hook semantics.

### 3.5 Registry / storage services (§5.3, §2.2)

- `HandlerId` = stable symbol hash (FNV-1a 64); atomic whole-table flip;
  dispatch take/put-back for re-entrancy; unresolved ids panic loudly.
- `StyleId` intern table with structural dedup.
- `PassMask` bit flags with or/contains/iter/display.
- `WorkerQueue`: generation-tagged results, drained at INPUT, retired-gen
  discard counted in stats.

---

## 4. M0b — TextService contract + DirectWrite backend

### 4.1 The contract (`crates/oppa/src/text.rs`, zero-dependency)

The trait the spike (M1) and the layout engine (M3) consume:

```rust
pub trait TextService {
    fn enumerate_fonts(&self) -> Vec<FontInfo>;
    fn shape(&self, text: &str, style: &TextStyle) -> Result<ShapedRun, TextError>;
    fn measure_line(&self, run: &ShapedRun) -> MeasuredRun { run.single_line_metrics() }
}
```

Data shapes:

- `TextStyle { family, font_size_px, device_pixel_ratio, weight: FontWeight(u16),
  style: FontStyle, stretch: FontStretch(u16), letter_spacing_px, locale }`.
- `ShapedRun { glyphs: Vec<ShapedGlyph>, runs: Vec<TextRun>, clusters:
  Vec<Cluster>, total_advance, text_len_bytes }` — the unit renderers
  rasterize and the spike tests against (DESIGN §2.3: display lists carry
  pre-shaped, pre-positioned runs).
- `ShapedGlyph { glyph_id: u32, x_advance, x_offset, y_offset }` (device px).
- `TextRun { byte_range (UTF-8), glyph_range, rtl, script (ISO 15924 numeric),
  font_id, font_metrics }` — the per-run font/script/bidi boundaries for the
  single-line field.
- `Cluster { byte_range, glyph_range }` — the grapheme-cluster mapping.
- `FontMetrics { ascent, descent, line_gap }` (device px, baseline at 0);
  `MeasuredRun { width, ascent, descent, line_gap }`;
  `CaretRect { x, y, width, height }` (the candidate-window anchor).
- `TextError { FontNotFound, EmptyText, Backend }`.

Pure mapping/anchor math lives as `ShapedRun` methods (unit-testable with no
backend, and the exact math both backends and the spike share):

- `glyph_index_for_byte_offset(byte) -> Option<usize>` — mid-cluster bytes
  snap to the cluster's first glyph (carets never split a cluster).
- `byte_offset_for_glyph_index(glyph) -> Option<usize>` — cluster start byte.
- `caret_x(byte) -> f32` — leading edge of the containing cluster; the caret
  at/after the text end is the run's total advance.
- `byte_offset_for_x(x) -> usize` — cluster midpoint rule: leading half →
  the cluster's start byte, trailing half → its end byte; past the end →
  text length.
- `caret_rect(byte) -> CaretRect` — caret-height box at the caret x
  (ascent/descent from the run's tallest font).
- `single_line_metrics()` / `pen_x_at(i)`.

DPR rounding (§8.8): the shared `round_to_device_px(value, dpr)` helper —
rounding happens at commit positions only; shaping advances stay subpixel.

### 4.2 IME composition surface (`crates/oppa/src/ime.rs`)

Stub-but-real, backend-agnostic — the spike fills it in per-backend rather
than inventing it:

- `ImeCompositionEvent { CompositionStarted{start_byte},
  CompositionUpdated{composition, caret_byte}, CompositionCommitted{committed},
  CompositionCancelled, DeleteRange{range} }` — the normalized
  begin/update/commit shape of §9.2.
- `ImeCompositionHandler` trait — the core-side sink (the editing session
  service will implement it).
- `dispatch_ime_event(&mut dyn ImeCompositionHandler, &event)` — the single
  dispatch point both the scripted feed and platform wiring route through
  (criterion 3's no-lost/no-duplicated seam).
- `ImeCompositionFeed` — scripted sequences for tests and backend feeds.
- `ImeOps { SetCaretRect{x,y,w,h}, ShowCandidateWindow, HideCandidateWindow }`
  — the candidate-window control, wired into
  `PlatformShell::set_ime(&mut self, ops)` (default no-op until a backend
  wires a real IME).

### 4.3 The DirectWrite backend (`crates/oppa-text-dwrite`)

The first real TextService. Backend choice: **Windows/DirectWrite** —
predetermined twice over: BUILD-ORDER puts the DirectWrite slice on the
§9.2 spike's critical path (M1's Windows arm shapes through it), and this
development environment is Windows. The other platforms' backends are
follow-up work.

Pipeline per string: UTF-8 → UTF-16 with a **code-unit → UTF-8 byte-offset
table** (surrogate pairs map both units to the pair's start); script analysis
via `IDWriteTextAnalyzer::AnalyzeScript` and bidi via `AnalyzeBidi`
(implemented as the two COM callback objects the API requires); per script
run, font selection through the system font fallback (`IDWriteFontFallback::
MapCharacters`, looping until the run is fully mapped — mixed-coverage text
becomes multiple font pieces); then `GetGlyphs` + `GetGlyphPlacements` per
piece. The em size handed to DirectWrite is `font_size_px *
device_pixel_ratio`, so every advance/offset/caret comes back in device px.

- `enumerate_fonts`: family walk over the system collection; `FontId =
  family_index × 4096 + font index` (stable across calls for a stable font
  set); first localized name per family.
- `shape`: loud `FontNotFound` check up front (DirectWrite's fallback would
  otherwise silently substitute a default font for a missing *family*; the
  fallback's job is missing *glyphs* within a mapped run, which is honored);
  letter tracking adds to every advance except the run's final glyph (the
  trailing caret position equals the run width).
- `!Send` (COM pointers); UI-thread use, same regime as the reactive core.

### 4.4 M0b tests

Core unit tests (no backend; run everywhere): scripted IME sequence delivered
exactly once in order (criterion 3's shape), cancel-mid-composition,
delete-range mapping; cluster-map round-trips on synthetic runs including a
surrogate pair; shared device rounding determinism across DPRs.

DirectWrite integration tests (`tests/shape.rs`, real system fonts,
hand-checked):

- "Hi" → exactly 2 glyphs, all advances positive, deterministic re-shape.
- Run width == sum of advances; `measure_line` width == total advance;
  ascent/descent > 0.
- 'W' advances > 3× 'i' (hand-checked Segoe UI relation).
- "héllo" → 5 glyphs; cluster byte starts `[0, 1, 3, 4, 5]` (é = bytes 1–3);
  byte→glyph mapping with mid-cluster snap (byte 2 → é's glyph); glyph→byte
  resolves to the cluster start.
- "日本語" → 3 glyphs through the system fallback (Segoe UI has no CJK
  coverage); byte starts `[0, 3, 6]`; every byte in each 3-byte char maps to
  its glyph.
- "👍" → exactly 1 cluster covering all 4 UTF-8 bytes / 2 UTF-16 units; no
  byte inside the pair splits it.
- Caret positions monotone; round-trip at every cluster boundary byte;
  mid-cluster clicks snap to cluster edges; trailing caret == run width.
- DPR 2 vs 1 doubles every advance and the total width; shared rounding
  helper snaps to the device grid.
- Letter tracking widens every inter-glyph advance except the last; the
  trailing caret = plain width + (n−1)×spacing.
- `enumerate_fonts` non-empty and contains "Segoe UI".
- Unknown family → loud `FontNotFound`; empty text → `EmptyText`; italic
  axis honored.

---

## 5. M1 spike — the §9.2 text-editing spike (M1; verdict merged by the M1 merge round)

One editable single-line field built twice against the same rig: the
**Windows-GPU arm** (framework-authority editing session over
`DWriteTextService`; the mechanism variant (a) uses) and the **Web-DOM arm**
(a real `<input type="text">` in headless Edge driven through native
clicks/keys and the browser's own IME pipeline, with framework-side hooks
that only record what the DOM reports — no custom JS IME handling, per the
round's scope). Artifacts: `spike/corpus.json` (shared rig, generated by the
Windows arm), `spike/results/windows.json`, `spike/results/web.json`,
`spike/results/verdict.json` (all raw; nothing averaged).

### 5.1 What was built

- **EditingSession** (`crates/spike-textedit/src/session.rs`): content in an
  author-owned `Signal<String>` (controlled pattern, locked #24); caret,
  selection, composition buffer, and single-level undo as core-side session
  state; `ImeCompositionHandler` sink fed by `ImeCompositionFeed` through the
  one `dispatch_ime_event` seam; candidate anchoring emitted through
  `PlatformShell::set_ime` (recorded by a `RecordingShell`); cluster-stepped
  caret motion and hit-testing via `ShapedRun`'s pure math.
- **Caret oracles** (`src/oracle.rs`): `IDWriteTextLayout::HitTestTextPosition`
  (DirectWrite's canonical caret API) and a real Win32 EDIT control
  (`EM_POSFROMCHAR`/`EM_GETRECT`, DPI-unaware thread, identical font/size) —
  the two independent platform references criterion 1 names.
- **Shared rig** (`src/rig.rs` → `spike/corpus.json`): corpus strings with
  two-way cluster tables (UTF-8 bytes ↔ UTF-16 units, device-px positions),
  7 IME scenario scripts (zh candidate commit, ja romaji→kana→candidate,
  cancel-mid, in-composition caret navigation, rapid zh↔ja switch,
  delete-range re-anchor, focus loss mid-composition), and 3 editing-op
  suites (latin_edit, multibyte_edit, undo_granularity).
- **Web-DOM arm** (`spike/web/`): instrumented page + Node harness. IME
  drives Chromium's native text-input state machine via CDP
  (`Input.imeSetComposition` for start/update/cancel; commits through
  `Input.insertText` during composition, which Chromium delivers as
  compositionupdate + compositionend-with-data — the real-IME commit shape).
- **Session regression tests** (`tests/session.rs`, 10 tests, no backend).
- **Engine fix pulled in by the corpus**: `oppa-text-dwrite`'s
  `IDWriteTextAnalysisSource::GetLocaleName` returned a `u32::MAX`
  length sentinel that made every `MapCharacters` call *after the first*
  fail with E_INVALIDARG — multi-font-piece strings ("héllo 👍",
  "你好world", mixed Latin/CJK) could not shape at all. M0b's tests never
  shaped multi-piece text; the spike's corpus did. Fixed (exact remaining
  length) and all 13 M0b tests still pass.

### 5.2 Raw results (verdict.json carries every number)

| Criterion | Windows-GPU arm | Web-DOM arm |
|---|---|---|
| 1 — IME geometry (Windows-scoped) | **PASS**: max Δ 0.000 device px vs IDWriteTextLayout on every corpus string at DPR 1 and 2; max Δ 1.41 px vs the native EDIT control at DPR 1 (tolerance N=2, all strings pass); intra-cluster oracle spread 0 (neither engine splits clusters); composition tracking 39/39 steps ≤ 0.000 px | N/A by scope (criterion 1 restated Windows-only; native IME anchoring is the browser's) |
| 2 — hit-test parity | sweep answers produced (238 probes, 4 strings) | 235/238 identical; the 3 mismatches are the exact cluster-midpoint tie (x=8/24/40 on 日本語: framework puts the switch AT the midpoint, Chromium from mid+1). Selection ops 9/12: all 3 mismatches are double-click word rules (browser trailing-space inclusion ×2; CJK dictionary segmentation 日本語 ×1). Boundary geometry: browser switch points vs framework midpoints within ±0.5 px on every string |
| 3 — composition fidelity | **PASS**: all 7 scenarios, canonical stream == script, state follows the session's semantics | Core scenarios (zh commit, ja kana→candidate, cancel-mid, caret-nav, rapid switch): value matches the framework composite at **every** step, no lost/duplicated characters, commits in the native IME shape. Two classified divergences: (i) phase-stream shape — the DOM expresses commit/cancel as compositionupdate→compositionend pairs (normalizable, same class as §9.3 scroll normalization); (ii) focus-loss: Chromium **commits** on blur, the spike session cancels (contract item); plus delete-range-mid-composition is **not drivable through CDP** (rig gap, needs one manual real-IME pass) and the scripted composition caret lands ±1 (CDP `compositionCaret` quirk — the caret *is* reported via selection, i.e. observable) |
| 4 — one model | **PASS** (suites recorded with resolved op_x coordinates) | latin_edit **9/9 exact** — values, carets, and selections at every step, *including the browser restoring the pre-undo selection [3,8) exactly like the framework session*; multibyte_edit 4/5 (dbl-click word rule); undo_granularity 3/4 (deliberate exposure: single-level undo vs browser burst coalescing) |

### 5.3 The verdict — argued in REPORT.md §5, adopted into DESIGN by the M1 merge round

**Verdict: (b) on Web** — presenter-owned editing authority (real
`<input>`), with the framework guaranteeing *behavior* through the
shared editing-operation suite. Argued from the criteria in REPORT.md
§5 (the DOM's native mechanism meets the behavior contract 2–4 with all
divergences spec-able contract items; (a)-on-Web was not built this
round, per scope, and would additionally have to prove the unmeasured
hidden-input candidate-anchoring fidelity). Not re-argued by the merge;
adopted as DESIGN locked **#27**, with #24 amended (verdict pointer) and
#5 amended (the "possibly" hedge dropped — editing sessions confirmed).

**Adopted with the verdict (exact DESIGN.md touch list):**

- locked **#5** amended — "possibly editing sessions" dropped; one-line
  reason + origin pointer to spike/REPORT.md §5 (pattern of #26's
  amendment to #19).
- locked **#24** amended — the deferred "(a) vs (b) decided by the
  spike" now names the verdict (b on Web) and points at #27.
- locked **#27** added — verdict (b) on Web as a normative lock, with
  the adopted contract items and the gated freeze.
- **§2.3** carries the renderer contract's second text path as a
  first-class clause: the DOM backend owns editing authority
  (caret/selection/IME/undo) for editable fields; behavior (via the
  shared suite) is guaranteed, not mechanism; non-editable text is
  unchanged. Two permanent rules written in: framework-measured tracked
  text is never delegated to CSS `letter-spacing` (one-unit
  trailing-edge divergence — REPORT.md §5, "Hello world" 92.03125 vs
  91.03125); the DOM text/editing contract's freeze is **gated** on the
  two blocking conditions (REPORT.md finding #5 manual pass; RTL/bidi +
  combining-marks/ZWJ corpus coverage or explicit re-deferral with named
  owner/milestone).
- **§9.2** records the resolution; the editing-session spec now includes
  commit-on-focus-loss (REPORT.md finding #6) and the criterion-4
  editing-op suite as the permanent cross-backend contract test, whose
  word-boundary + tie-break rules are spec — **binding the Windows-GPU
  session too** (it must replicate the browser-compatible conventions,
  not just the DOM arm's job).
- **§8.10** updated: the spike gate has passed; residuals tracked (below).

**What is now gated vs. locked:** the verdict, the #5 amendment, the
§2.3 clause, commit-on-focus-loss, and the shared-suite word/tie-break
rules are **locked** (#5/#24/#27, §2.3, §9.2). The DOM text/editing
contract's **freeze is gated** — it may not be marked frozen until both
§2.3 blocking conditions clear.

**Tracked open items carried forward (NOT closed by this merge):**

1. **Variant A on Web's fidelity remains unmeasured** (hidden-input
   candidate anchoring, framework-rendered hit-test parity, ARIA-mediated
   a11y — REPORT.md §5 residual 1). Moot under (b); needed only if (a) is
   ever reconsidered. Tracked in §8.10 and §7.
2. **The manual real-IME delete-range pass is still outstanding**
   (REPORT.md finding #5 / residual 2) — now a **blocking condition** on
   the DOM text/editing contract freeze (DESIGN §2.3). M1 remainder;
   tracked in §7. **The M1 remainder round attempted it and did not
   complete it** (the automation rig could not engage the real IME's
   composition engine on a plain Win32 window — the raw evidence is
   recorded in `spike/results/ime_manual.json` and ROUNDS.md's M1
   remainder entry); the gate stays open, with the TSF-aware window
   (document manager + context + input scope) documented as the
   engagement route.

## 5b. M1 remainder — what was built

### 5b.1 The window shell (`crates/oppa-shell-win`)

A real Win32 window hosting one editable field's vehicle: class
registration + `CreateWindowExW` + the proc, wired to the M0
`PlatformShell` trait (`pump_events` + `set_ime`).

- `pump_events` drains the internal queue in arrival order and emits
  M0-normalized `Event { kind, handler }` values (the registry contract,
  exercised); the payload-bearing shapes (`Cmd`: click/drag/key/char/
  focus) ride a 1:1 queue the registered field handler drains in event
  order (decision 27).
- Real IME handling in the proc: `WM_IME_STARTCOMPOSITION`,
  `WM_IME_COMPOSITION` (GCS_COMPSTR/GCS_COMPATTR/GCS_CURSORPOS/
  GCS_DELTASTART snapshotted at message time via `ImmGetCompositionStringW`),
  `WM_IME_ENDCOMPOSITION`, `WM_IME_NOTIFY` (logged + `DefWindowProcW` for
  the OS candidate machinery), `WM_IME_SETCONTEXT` (the composition
  window suppressed in the show-mask; the OS candidate UI stays).
- `set_ime(ImeOps::SetCaretRect)` performs the real anchoring:
  `ImmSetCompositionWindow` (CFS_POINT) + `ImmSetCandidateWindow`
  (CFS_CANDIDATEPOS) from the session's candidate anchor (run-relative
  device px → client px at DPR 1); show/hide ops logged only.

### 5b.2 The host binary (`spike_ime_shell`)

The field's frame loop: OS message drain → the anchor-op drain (the
session's `set_ime` emissions → the real anchoring calls) → one session
frame (`rt.request_frame()` + `run_once()`; the INPUT phase pumps the
shell and dispatches the field handler) → the Vello debug render.

- The real-IME mapper (the platform wiring, session-aware): START →
  `CompositionStarted{anchor}` then `DeleteRange{selection}` when a
  selection is active (decision 28); `WM_IME_COMPOSITION` with
  `GCS_RESULTSTR` → `CompositionCommitted`; END without a commit →
  `CompositionCancelled`; an update without a begin → anchored start
  then `CompositionUpdated` (cursor mapped composition-relative UTF-16 →
  UTF-8 bytes, reported in composite coordinates per spike decision 20).
- The Vello debug renderer: the field's composite text shaped per frame
  through `DWriteTextService`, drawn as positioned glyph runs (per
  shaped piece; the same file DirectWrite shaped from, resolved via the
  new `font_file_source` hook — decision 30 in the round entry), plus
  the caret rect, the selection highlight, the composition underline.
- The automated driver (`--ime-pass`): arms the installed IME, drives
  the scenario (Home; Ctrl+A select-all; the composition letters
  n/i/h/a/o; Space confirm; Ctrl+Z; a second composition over the
  restored selection; Escape cancel), records the raw message stream +
  per-step observables + the environment facts into
  `spike/results/ime_manual.json`, computes the verdict.

### 5b.2 The manual pass result (the freeze gate)

**FAIL — the pass did not complete.** The composition never engaged:
`WM_IME_STARTCOMPOSITION`/`WM_IME_COMPOSITION`/`WM_IME_ENDCOMPOSITION`
never arrived (zero across the pass); the IME's UI machinery was alive
(SETCONTEXT + repeated NOTIFies) and the injected letters typed as plain
text. The app-side IMM controls (`ImmSetOpenStatus`/`ImmSetConversionStatus`)
report success without the mode sticking; an injected Shift tap types a
stray character rather than the EN/CH toggle. Classification: a
rig/automation gap — the session's IME seam was exercised with the real
message stream that DID arrive (SETCONTEXT/NOTIFY routed through
`dispatch_ime_event` correctly); what is missing is the composition
engine's engagement, which requires the TSF-aware window path (document
manager + context + input scope) — the framework's own platform-shell
IME work, not spike-side scripting. Per the round's rules the gate is
**re-flagged, not marked satisfied**; DESIGN §2.3 untouched. Full record:
`spike/IME-PASS.md` (the pass's report: the environment, the raw message
stream, the classification, the toolchain facts).

## 5c. TSF-aware re-run — what was built (this round)

The engagement route IME-PASS.md §6 named as the fix, built where it
belongs: `crates/oppa-shell-win/src/tsf.rs` (`TsfBridge`), PlatformShell's
own IME infrastructure (M3+ work), not spike-binary code. The host
(`spike_ime_shell`) only calls `shell.enable_tsf()` after the existing
`arm_ime` and re-asserts TSF focus per pass step; the scenario, keys,
settle timing, and verdict checks are byte-unchanged from the M1 remainder
round (STEPS / `send_keys` / 320 ms + 12 ms settle / `check_*` untouched).

- **Activation** (`TsfBridge::activate(hwnd)`): `CoCreateInstance(
  CLSID_TF_ThreadMgr)` → `ITfThreadMgr::Activate()` (client_id 32 this
  run) → `CreateDocumentMgr()` → `CreateContext(client, 0, punk=None)`
  (edit_cookie 0; **no `ITextStoreACP` implemented — the stated remaining
  gap**) → `Push(context)` → `AssociateFocus(hwnd, docmgr)` →
  `SetFocus(docmgr)` — every step S_OK this run, each logged with its
  HRESULT into the pass's environment record.
- **Input scope**: the window declares `IS_TEXT` (57); `GetProperty(
  GUID_PROP_INPUTSCOPE)` on the context **failed E_FAIL (0x80004005)** this
  run — recorded, not retried. A property *value* set needs an edit
  session over a TextStore-backed range, which does not exist without the
  store above.
- **Focus**: `SetFocus(docmgr)` re-asserted per step (S_OK at all 17
  steps); focus-gain/loss notes follow `Cmd::FocusChanged`.
- **Re-run result: FAIL — composition still never engaged.**
  `WM_IME_STARTCOMPOSITION`/`COMPOSITION`/`ENDCOMPOSITION` zero times
  across 105 log rows (17 markers + 88 OS messages); every injected letter
  arrived as `WM_KEYDOWN`+`WM_CHAR` plain text; the composition column is
  empty at all 17 steps (content trace `Hello world` → `nihao` → `nihao `
  → `nihao` → `nihaonihao`, identical in shape to the M1 remainder run).
  The IME stayed attached (one `WM_IME_SETCONTEXT`, 8× `WM_IME_NOTIFY`,
  no code-6 storm this run; per-step conv reads 0x1). Full record: ROUNDS.md's
  TSF entry (per-step table, ordered message stream, classification).
  The freeze gate (DESIGN §2.3 blocking condition (a)) stays open;
  DESIGN.md untouched. `spike/results/ime_manual.json` now holds THIS
  round's evidence (it overwrote the M1 remainder file; that round's
  numbers survive in `spike/IME-PASS.md` and the M1 remainder ROUNDS entry).
  Confounder recorded separately: `spike/IME-CONFOUNDER.md` — the
  zh-Hans-CN language features (Basic typing et al.) were still
  installing during both FAIL runs, so IME-readiness is an unisolated
  variable alongside the missing store; neither run may be read as "TSF
  path exhausted" until that note's discriminating test clears.

## 5d. Text-store round — what was built (this round)

`ShellStore` (`crates/oppa-shell-win/src/tsf.rs`): the TIP-facing copy
of the one field's text (UTF-16/ACP) + selection, implementing the full
`ITextStoreACP` (28 methods) + `ITfContextOwner`, passed as
`CreateContext`'s punk (edit_cookie 1 this run). Lock discipline:
synchronous grant scoped to the `OnLockGranted` call (the standard ACP
sample pattern; the borrow is dropped across the TIP callback —
holding it would panic on re-entrant `SetText`), strict `TS_E_NOLOCK`
on TIP mutations without a lock, lenient reads. View/geometry/embedded
entry points return `E_NOTIMPL` (recorded; candidate positioning may
suffer — the session draws inline). Attributes: zero supported
(`FindNextAttrTransition` halts immediately).

Composition delivery: `ITfTextEditSink` advised on the context (cookie
1; edit-flush path) + `ITfContextOwnerCompositionSink` implemented on
`ShellStore` itself — the context discovers it by QI'ing the owner
punk; advising it on the source fails `TS_E_NOOBJECT` (0x80040202,
two runs of evidence). TIP transactions become the SAME `ImeMessage`s
the IMM path produces (`StartComposition` / `Composition{comp/result}`
/ `EndComposition`) through the same `ShellEvent::Ime` queue, so the
mapper and session semantics are untouched. The composition span is
tracked from the TIP's own `SetText`/`Insert` calls; a non-empty final
span commits (result, then END), an empty one cancels (END only).
Host: `enable_tsf(text, sel)` seeds the store; each tick drains the
store trace into the step notes and mirrors the settled session back
while no composition owns the store (guarded + logged inside the
shell). `--wait-secs N` pumps messages pre-pass so a human can focus
the window (harness, not pass content; foreground-at-end recorded).

Also fixed this round (M0 core): `Runtime::input_phase` held the state
borrow across `shell.pump_events()` — the first signal-writing pump
callback (the mapper → session, via the new composition messages)
panicked in `Signal::get`. Take-the-shell-out + restore, same shape
the TIME phase uses for animation closures. All 88 tests still pass.

**Re-run result: FAIL with composition ENGAGED — the first real Pinyin
composition through our window.** Owner-QI `OnStartComposition` fired,
per-letter `SetText` spans tracked (`(11,12)` …), session buffered
`n/i/h/a/o` in turn (`content "Hello worldniha" comp "o" caret 16` at
r1:o), Space committed (`"Hello worldnihao "`, comp cleared), undo
removed exactly the commit (`"Hello worldnihao"`, caret 16). Zero
lock/async/NOLOCK violations in the trace; sync skipped while active,
synced after. Message stream: 192 rows; letters arrive as `KEYUP` only
(TIP consumes `KEYDOWN`+`CHAR`); 37× `NOTIFY` (code-6 storm back);
synthesized `Start → updates → result → End` per letter in `ime_events`
(`comp: Some("n")` … `result: Some("n")`). Full record: ROUNDS.md's
text-store entry. Why still FAIL, per check: c1 — the user's focus
click (`WM_LBUTTONDOWN` (210,86)) landed mid-pass and collapsed
select-all to `(11,11)` (environment race, not code); r1/c3/c4 shapes
follow from the collapsed selection (append-at-caret); r2 same; c6 —
Esc ended WITH the reading (`final span text "o"`, faithfully
translated as commit): real-TIP Esc-finalizes vs the scenario's
IMM-era cancel expectation (rig-vs-real open item). DESIGN.md
untouched; the gate question is now scenario-semantics, not
engagement. `spike/results/ime_manual.json` holds THIS run (earlier
runs' numbers survive in their prose records); two mid-round runs were
discarded as invalid (game-keystroke bleed; unfocused window — see
`spike/IME-CONFOUNDER.md` §7).

## 5e. IME verification round — repetition + close (this round)

No code changes (verification + paper trail only). Repetition run,
`--ime-pass --wait-secs 2`, hands-off verified (zero mouse traffic,
fg True at all 17 steps): **PASS 6/6, zero divergences** (75 rows =
17 markers + 58 OS). Full per-step table in the ROUNDS entry;
identical shapes to the first PASS (held `"ni'hao"` caret 6, commit
你好 caret 6, atomic restore caret 11 sel `(0,11)`, Esc-cancel to
`""`). Activation S_OK a third straight time post-reorder
(`pre-activate: foreground=true` → verified active `0x804/
FA550B04`). Divergences vs the first PASS, all immaterial: 75 vs 70
rows (OS chatter: one extra KEYUP, 8 vs 6 NOTIFY); IMM conv reads
`0x1` (vs `0x401`) with identical engagement — the IMM conv read does
not drive composition, the TSF path does; TIP reading style held both
times (no per-letter variance this round). Unification across all
eight runs: profile activation S_OK ⟺ the thread had a focused window
at call time (all three E_INVALIDARG runs read fg False at init/home;
the windowless probe fails deterministically; all five S_OK runs were
focused) — recorded as decision 40. On this evidence (two consecutive
hands-off PASS runs) DESIGN §2.3 blocking condition (a) is **closed
as locked #28**; (b) remains the only open freeze condition.
`spike/results/ime_manual.json` holds THIS run.

## 5f. Bidi/combining/ZWJ round — corpus rig v2 on both arms (this round)

Corpus (`rig.rs`, `RIG_VERSION` 1→2; `hit_strings` 4→7, inherited by
`anchor_strings`; op-suite bases unchanged — editing ops on the new
classes is deeper scope than this measurement round, stated):
`"abc "` + Arabic U+0645 U+0631 U+062D U+0628 U+0627 + `" 123"`
(visual reordering), `"cafe"` + U+0301 decomposed e-acute (pairs with
precomposed `"héllo"`), `"a"` + U+1F469 U+200D U+1F4BB + `"b"` (ZWJ).
All non-ASCII corpus forms are ASCII escapes in source (mojibake-proof
by construction — see decision 44).

Windows-GPU arm (`spike_win_arm`, all Segoe UI, no fallback
surprises): bidi 13 clusters source-ordered, x monotonic; decomposed
é ONE cluster u=(3,5) b=(3,6); ZWJ ONE cluster u=(1,6) b=(1,12)
spanning 11 bytes. c1: bidi FAILs both oracles (65px DPR1 / 130px DPR2
vs layout, 45.3 vs EDIT — run-order flip magnitude, not noise); every
other string passes (combining 0/0.9, ZWJ 0/0.52; pre-existing strings
unchanged). Web-DOM arm (headless Edge, same corpus.json): c2 sweep —
bidi 60 mismatches, combining 0, ZWJ 0, pre-existing 日本語 3
unchanged from M1; boundary 0 everywhere; sel-ops mismatches are all
six dbl-click word rules (3 pre-existing + bidi/café/ZWJ dbl-click),
zero drag/shift-click divergence on any string. Verdict: c1 FAIL
(bidi rows only), c2 FAIL (60 bidi + 3 pre-existing sweep; 6
word-rule sel), c3 FAIL (all scenarios phases False + caret drift,
zero value loss — pre-existing CDP/Edge drift by construction proof:
c3's entire input closure is untouched code and the font stack is
provably unchanged since all new strings mapped Segoe UI; separate
re-baseline owed, out of scope), c4 PASS_WITH_DOCUMENTED_DIVERGENCES
(suites untouched).

Unit pins (`shape.rs`, 13→16 tests): Arabic bytes 4..14 all in rtl
runs + Latin non-rtl (rtl flag's first assertion); decomposed single
cluster (3,6); ZWJ single cluster (1,12). Suite total 91/91.

Classification (authority vs gap vs limitation): bidi visual-order =
expected/documented limitation, M3 work reasserting itself (exactly
M0b ROUNDS 240-243) — NOT a new problem; combining/ZWJ geometry +
hit-test = closed on both-arm agreement; ZWJ dbl-click (session
`(1,1)` collapsed vs browser `(0,4)` — `word_class` has no
emoji/ZWJ awareness: emoji/ZWJ classify Separator) = implementation
gap in session word rules, same family as the documented CJK
dictionary divergence; c3 = rig-environment drift, not a (b) finding.
Decision: PARTIAL — close combining cluster parity + ZWJ
single-cluster (covered: geometry, hit-test, click/drag/shift-click
selection); re-defer (i) bidi visual ordering → M3 layout engine,
(ii) word-segmentation incl. ZWJ-emoji → shared-suite spec items, M2
editing session, (iii) scalar caret-stepping through combining
clusters → future suite base (untested remainder, not a failure).
DESIGN §2.3(b) partially closed as locked #29. Full record: ROUNDS.md's
(b) entry.

## 5g. M2 — reconciler + component model (this round)

The piece that turns the reactive primitives into the `#[component]`/
`VNode` system the §4 examples assume. Headless (no renderer consumed the
diff stream yet); LAYOUT/PAINT stay counting stubs.

### 5g.1 VNode representation (§2.2 shapes + stated extras)

- `Tag`: Div/Stack/Row/Column/Text/Image/ScrollArea + `Custom(u64)` escape
  hatch (lock #17 mitigation built in, not bolted on).
- `Element { tag, debug, key, style, text_hint, semantics, handlers,
  children }`: `debug` (the builder label) and resolved-`Style`-by-value
  are the two extras vs §2.2 — interning happens once at the reconcile
  boundary so bodies stay pure values. Pending handler closures ride
  `RefCell` slots in the ephemeral attachment; the reconciler drains them
  into the registry at commit and retains ids only (lock #11 holds).
- `VNode::{Element(Box), Text, Fragment, Hole}` — boxed (churning ephemeral
  tree, `large_enum_variant`); fragments transparent, holes absent.
- Builders mirror §4 verbatim (`Div("track")`, `Row("slot")`,
  `ScrollArea("list")`, `Column::new().gap(2)`, `.key/.style/.semantics/
  .on_press/.child/.children/.content_size`) with two mechanical deltas:
  childless chains end in `.build()` (D4), numbers accept both literal
  spellings via `IntoPx` (an integer literal never infers to `f32`).

### 5g.2 Component execution model (§4 + §9.1)

- A mounted component *is* an effect: its body runs in the effect's dep
  scope, so reads track and writes follow the propagation contract
  unchanged (topo order, one-run-per-pass, 3-pass budget — proven at
  component level, §5g.5).
- `Ctx::signal/memo/binding` key per-instance state by call-site
  source-hash + per-run ordinal (`#[track_caller]` + a macro expanding at
  the invocation point — a helper would collapse every site to one key).
  Body edits inserting a signal shift later sites → re-seed, never shuffle
  (§5.1, proven §5g.5). A type change at an unchanged site panics loudly
  (restart class).
- `ctx.hovered()/pressed()/focused()` are per-instance bool signals (the
  seam M5 hit-testing writes through; tests drive them directly).
  `ctx.scroll_offset()` is the framework `ScrollOffset` handle (identical
  semantics all backends; tests `.set` it, the headless feed).
- `ctx.child(name, key, props, render)` expands inline with a child
  instance scope recording `(symbol, key, parent)`; scheduling stays the
  parent effect's in M2 (decision 48). Props cross by reference out of
  `OpaqueProps` (type-erased + hot-side clone glue + generation tag).
- `Store<Id, V>` (ordered ids + map, one version signal — coarse, decision
  52), `ImageCache` (content-addressed stub), `ComponentHost`/`MountHandle`
  (`set_props` = opaque swap + explicit effect scheduling). Keyed-state
  capacity defaults to 64, overridable per runtime (`set_keyed_capacity`;
  per-list sizing answers decision 50's namespacing deferral in M8 —
  decision 126).

### 5g.3 The reconciler (§2.2 contract, locks #4/#13/#22)

- Old VNode + new VNode → `TreeDiff { ops, suppress_transitions }`;
  `Add/Remove/Move/Update` with `structure_ops()` counting. Keyed children
  diff in place, unkeyed by order, incompatible pairs replace, keyed
  reorders move. Roots are single Elements.
- PassMask mapping: structure→STRUCTURE|LAYOUT|PAINT (+LAYOUT on the
  parent); style→STYLE|PAINT + LAYOUT for the layout-affecting subset;
  text→TEXT|PAINT; semantics→SEMANTICS; handler kind-set change→PAINT.
- Handler identity = (NodeId, kind), one per kind: re-runs rebind closures
  under retained ids (zero steady-state churn); kind-set changes update.
- `suppress_transitions` arrives as whole-commit data from the scheduler's
  binding-edge flag (the accepted per-commit limit, lock #22); honored by
  the M8 evaluator (TIME interpolator on GPU, CSS mapping + one-commit
  suppression on DOM — §5n, decisions 120–125).

### 5g.4 The locked examples

Both run (toggle end-to-end; list scroll/selection/rebind/eviction). Six
mechanical deltas D1–D6, classified in the M2 ROUNDS entry as necessity (N:
D1 call-syntax reads, D3 two-parameter `Store`, D4 `.build()` + float
literals), §2.2-compliance + deferred sugar (D2: `ctx.child` instantiation
— §4.2's five-parameter row contradicts the locked `Component<P>` type, so
§2.2 wins), scope reduction (D5-emit: payload-less `emit` narrows the
proven `on_change` guarantee to handler-delivery; value delivery is M5 —
closed pre-M9: the registry carries routing while values travel via
shared signals passed as props, the M5/M8 pattern, not the sketched
payload mechanism),
by-design app-side (D5 themes) and stated deferral (D6 stub image cache).
The bodies' logic is the locked text.

### 5g.5 M2 tests (11 integration + 8 lib unit)

`m2_reconciler.rs`: toggle acceptance; component write-back settle +
divergence-assert (debug) / defer-then-park (release, same gating as M0 —
lock #19, see decision 59); zero-structure-op scroll + silent sub-row
scroll + slot-instance stability; rebind stamp set vs real-change stamp
clear; keyed_state default-64 LRU + configured-capacity-8 + component
rebind-back + far-scroll eviction; body-edit reseeding `(99, 10, 20)` not
`(11, 21, 20)`; opaque props clone+generation. Lib unit: style 2,
semantics 1, vnode 2, reconciler 3 (mount/update-only, one-commit stamp,
keyed recycle).

## 5h. M2b - hot-reload harness + fuzzer v1 (this round)

Per BUILD-ORDER M2b: manifest export/scan, dylib swap, opaque
generation-tagged props with hot-side vtable clone/drop,
drain-before-unload, atomic registry flip; §8.1 re-seed assert +
call-site lint; §9.6 crate-level state lint; §8.4 fuzzer v1 extended to
the §9.6 task/message path. Proves #14 (incl. new-component-addition
via manifest scan), #25 (residence), §5.3 drain ordering, §9.6
cancellation/discard race closure. No lock needed changing.

### 5h.1 What was built

- **`oppa::reload` manifest ABI** (`reload.rs`): `ComponentDesc`
  (symbol + props type name + render/drain/adopt), `ManifestView`
  (the stable export shape), `DrainedProps` (thin pointer; null =
  drain refusal), typed drain/adopt fn types with the hot-glue panic
  rule (failures are null returns — panics must never cross the dylib
  boundary, which aborts on Windows).
- **`component_manifest!`** (oppa-macros): per-component
  render/drain/adopt glue + `OnceLock`-built table (type_name is not
  const-stable) + optional `export,` prefix for the `#[no_mangle]`
  dylib symbol (static test manifests omit it — two exports would
  collide at link time).
- **`HotRegistry`** (oppa-reload): `install` (record the loaded
  manifest — a first swap without it reports `MissingDrain`),
  `request_swap` + `arm` (RELOAD-phase hook), `reload_to`
  (drain → cancel → advance → retire → rescan → adopt → evict →
  assert → re-run), `ReloadReport` (drained/adopted/evicted/tasks/
  effects/worker deltas/retired_images), `find_entry` (mounting
  discovered components).
- **Render resolution by symbol** (`ComponentHost::run_instance` +
  `set_render_table` + `mount_erased`): post-swap re-runs resolve the
  current code per run — never the mount-time pointer. Found by the
  reseed test failing (probe 48, not 141): without it, re-runs execute
  stale code, and real swaps would execute unloaded pages.
- **Task executor** (§9.6): one background thread per runtime,
  `Send`-only bodies, `TaskScope::submit` into the generation-tagged
  INPUT drain; `drop_pending_tasks` at RELOAD; running tasks finish but
  their submits discard by tag. `Ctx::spawn` carries the handler
  capture discipline (non-`Send` captures do not compile).
- **Per-runtime run stacks** (core tracking fix): stacks live in shared
  `RuntimeState`, not TLS — dylib-executed reads/writes otherwise see
  an empty stack and silently lose tracking. `untrack` keeps its legacy
  TLS stack, consulted by readers alongside (single-image behavior
  bit-identical; cross-image untrack of host-executed reads is a
  documented limitation).
- **Lints**: `#[component]` flags creation calls in nested `fn` items
  and conditional/loop bodies (run-varying counts ⇒ shuffle);
  loop-combinator bodies (`.map(|slot| ...)` — the locked §4.2 pattern)
  spared by design, with the analysis recorded in the macro docs.
  `#[hot_crate]` enforces the state lint at compile time via
  `oppa::lint::check_no_ambient_state` (`include_str!` for file
  modules, embedded literal for inline ones, `#[cfg(not(test))]`).

### 5h.2 Two genuine soundness findings (fixed, not filed away)

1. **Stale-code re-runs** (above): the mount-baked render pointer.
   Fixed by per-run symbol resolution; proven by the reseed test.
2. **Hot-vtabled values outliving unload** (found by the real-dylib
   test segfaulting in `probe.set`): signal/memo slot values, keyed
   handles, memo closures, and handler entries can carry vtables from
   a retired image. M2b resolution — **retire, don't unload**
   (decision 60): retired images stay mapped (bounded leak, counted),
   so every vtable stays valid. True unload needs shared-core linking
   (M9 product-loop work, tracked).
3. **Cross-image `TypeId` inequality** (found by the same test:
   `stored hot_fixture::EmptyProps but read as hot_fixture::EmptyProps`):
   identical types in different images have different `TypeId`s.
   Adopt/mount paths use type-NAME checks + same-toolchain layout
   equality instead; `OpaqueProps::try_get` + null-returning drain glue
   replace every cross-boundary panic.
4. **Cross-image TLS tracking loss** (above): fixed by shared-state
   run stacks; proven by the post-swap `counter.set(42) → probe 142`
   assert in the real-dylib test.

### 5h.3 M2b tests (8 cycle + 1 fuzzer + 1 real-dylib + 19 unit)

- `reload_cycle.rs`: identical-rescan keep, body-edit reseed (141) +
  signal survival, UnknownSymbol eviction without panic,
  TypeMismatch eviction, handler re-resolution, task apply/discard
  partition, keyed_state survival, RELOAD-hook path.
- `fuzz_reload.rs`: seeded xorshift, 150 ops over two manifests with a
  keep/reseed/revive model (revisits revive per-manifest values —
  keyed-storage consequence, §5.1), exactly-once task accounting
  (mailbox increments == applied; retired never run).
- `real_dylib.rs`: builds the fixture cdylib twice, real retire+rescan,
  adopt/reseed/survival/tracking/discovery/registry across images.
- Unit: oppa lib +6 (ambient-state scanner), macros 13
  (manifest parse/expand, body lint incl. the locked-pattern
  clean case, hot_crate expansion).

## 5i. M3 — layout engine (this round)

Per BUILD-ORDER M3 + locked #6: flexbox subset + block-lite + absolute
positioning, inline text runs (wrap, BiDi, optional-v1 ellipsis),
measure↔layout protocol, one-frame-delayed feedback, shared DPR
rounding. Proves #6 (engine-owned layout, stable boxes); closes locked
#29's bidi visual-ordering freeze item by oracle measurement. No
renderer code (no DrawOp/FramePlan — M4).

### 5i.1 What was built (`crates/oppa/src/layout.rs` + wiring)

- **Types:** `LayoutBox` (snapped x/y, subpixel w/h, content_w/h,
  `LaidLine`s with visual `LaidRun` glyph runs + `LaidCluster` maps +
  forward-affinity `caret_x`/`caret_position`), `LayoutTextConfig`
  (family/sizes/DPR/ellipsis flag), `MeasuredText` cache,
  `LayoutStats` (incl. `nodes_shaped` — the wrap round-trip
  instrument), `LayoutEngine` + `LayoutLedger` (engine + the
  settled-generation signal).
- **Flows:** Row (fixed-intrinsic + equal fill split, 2 flow passes on
  redistribute), Column (intrinsic unless `fill_width`), Div
  (block-lite full-width stack), Stack (overlay max-extent), ScrollArea
  (explicit viewport + `content_size` floor + `absolute_y` slots),
  per-axis `.x`/`.absolute_y` overrides, Custom as block-lite.
- **Text:** greedy cluster-boundary wrap + `\n` breaks over cached
  advances (re-wrap shapes nothing), UBA-lite levels 0/1/2 with the EN
  digit island, optional single-line ellipsis with an amortized "…"
  shape, `shape`+`measure_line` protocol with loud backend failures.
- **Wiring:** boxes + measure cache inline on `RetainedNode` (the §2.2
  sketch; reconciler-side residence); `set_layout_pass` + real
  `layout_phase` (framework code only); host-owned ledger/service/
  viewport with `committed_box`/`settled_box`/`layout_stats`;
  `Ctx::settled_layout`; commit dirt implies frame demand; hint
  changes dirty LAYOUT (mapping fix).

### 5i.2 M3 tests (9 lib unit + 16 integration + 1 oracle)

- Lib unit (synthetic runs, no backend): LTR order, RTL mirror, visual
  carets, source-vs-visual divergence miniature, digit island, wrap
  boundaries, `\n` splits, ellipsis marker, empty input.
- `m3_layout.rs` (hand-built trees via builders, counting fake
  shaper): row/column/block/fill geometry, knob-style `.x`,
  `absolute_y` pinning + zero auto-height, ScrollArea spacer + slot
  offsets, single-line measurement, hint sizes, empty-text zero,
  corpus-shape BiDi order + carets, wrap-without-reshape, ellipsis,
  dirty discipline (style-only skip; cache absorbs flag dirt; TEXT
  change re-measures exactly one leaf), one-frame-delay contract with
  per-frame asserts, DPR commit-only rounding.
- `bidi_layout.rs` (Windows, real DirectWrite + `IDWriteTextLayout`
  oracle): source divergence **65.00px at byte 4** (the recorded
  number, reproduced); visual carets ≤2px at all 14 boundaries.

### 5i.3 Measured tripwires (both pre-answered as measure-and-document)

- Wrap round-trip count: **1 shape per text change, 0 on re-wrap**,
  ≤2 flow passes, zero re-entrancy — within the BUILD-ORDER bound, no
  scope finding.
- DirectWrite-vs-wasm drift: DW side characterized (device-px
  subpixel advances, deterministic; single-tail RTL resolution);
  wasm stays an open item for the web work (feeds §8.8).

## 5j. M4 — CPU backend + FramePlan builder + image-diff oracle (this round)

Per BUILD-ORDER M4 + §3: the first runnable — one static component
(styled Div: padding/radius/background + one shaped text line) through
core→reconciler→layout→FramePlan→CPU backend→PNG, plus a
SemanticsDiff dump. Proves locked #5 (the contract is implementable by
a backend sharing no core code — `oppa-cpu` imports only the contract
types plus public retained reads; it never touches the layout engine
internals, the reconciler internals, or the text engine),
text-as-data end to end, and damage discipline on the one backend
where it must.

### 5j.1 What was built

- **Contract types (`crates/oppa/src/render.rs`, core-side):**
  `PresenterKind` (Cpu/GpuDrawList/Dom), `Caps` (+`cpu_fallback`:
  no blur/MSAA, `text_as_paths=false`), `SurfaceDesc/Id`,
  `DrawOp` (Rect/RRect/Circle/Shadow/Text/RImg/PushClip/PushLayer/
  Pop), `PlacedGlyph` (pre-shaped, pre-positioned cells),
  `DamageRect`, `FramePlan` (+`PlanStats`: visited/emitted/skipped/
  unboxed, `full_repaint` arm flag), `SemanticsEntry/Diff/Snapshot`
  + `compute_semantics_diff` + deterministic `dump`,
  `BackendError` (loud), `PaintStats` (incl. `skipped_empty`), the
  `RendererBackend` trait (every method: kind/caps/create/destroy/
  commit/paint), fixed `INK`.
- **FramePlan builder (`oppa-cpu/src/builder.rs`, presenter-side):**
  drains `STRUCTURE|STYLE|PAINT|TEXT` via `take_paint_masks`, emits
  only dirty nodes under the ancestor closure (clean subtrees skipped
  whole and counted), damage = dirty-box union, ScrollArea PushClip
  wrapping, opacity baked per op, RImg emitted for `Tag::Image`
  (backend refuses it). `build_full` ignores masks and never drains
  (the oracle reference arm).
- **CPU backend (`oppa-cpu/src/backend.rs`, tiny-skia 0.12):**
  per-surface pixmaps, NodeId-keyed commit registry, retained-op
  replay per paint (incremental splice / full replace), empty plans
  skip the surface untouched, PNG encode + save, `pixel_rgba` spot
  accessor, rounded boxes as bands + corner discs, offset-solid
  shadows, geometric clip stack rebuilt into masks per op, layer
  alpha stack.
- **Oracle (`oppa-cpu/src/oracle.rs`):** `OracleSession` paints the
  incremental and full-repaint plans to two surfaces and byte-
  compares (`image_diff_count`); the permanent CI substrate (M8
  consumes it frame-by-frame).
- **Phase wiring:** `Runtime::set_paint_pass` + real `paint_phase`
  (same hook shape as `set_layout_pass`); `install_paint_hook`
  builds + ordered-commits + paints per PAINT phase.
  `Reconciler::retained_ids` / `take_paint_masks` / `diffs_from` and
  `ComponentHost::with_retained_mut` are the builder's public
  surface — the §3 tripwire answered structurally (no core internals
  crossed).
- **One genuine engine fix (finding F2, not a drive-by):** fresh
  `VNode::Text` leaves carried no dirty flags (the Element arm sets
  STRUCTURE|LAYOUT|PAINT; the Text arm set nothing — unobservable
  while no consumer read paint masks). The module's own Add mapping
  covers all fresh nodes, so text leaves get the same bits. M2/M3
  suites stay green under it (m2_reconciler run in release too).
- **One documented limitation (finding F1):** LAYOUT is consumed by
  M3's engine run, so a position-only move with no paint flag does
  not rebuild plans. No M4 scene hits it (every visual change here
  carries STRUCTURE/STYLE/PAINT/TEXT); engine-side PAINT stamping
  on moved boxes is M5+ work, not a silent rebuild here.

### 5j.2 M4 tests (`tests/m4_cpu.rs`, 10 tests)

- Static PNG: mount plan is exactly 2 ops (RRect + Text) with 3
  damage rects; 7 hand-computed spots (surface white, div gray in
  padding/top-band/past-text, ink at both glyph centers, white at
  the rounded corner (0,0)); PNG saved to the temp dir, non-trivial
  size; StyleId sharing re-asserted through `node_styles`.
- Minimality: no-change rebuild is an empty plan (visited 1,
  skipped 3) and the paint skips (`skipped_empty`, surface
  byte-identical); props-driven "Hi"→"Hi!" rebuilds exactly 1 Text
  op with 1 damage rect (full static plan is 2); third build empty.
- Oracle: static incremental == full (0 differing pixels);
  post-change history replay (P1 then P2) == full repaint (0).
- SemanticsDiff: toggle + 2 rows → 3 upserts, dump contains
  "Wi-Fi"/Switch; flip → 1 upsert `checked=false`; row removal →
  1 removal, survivors silent. Nothing asserted about emitters.
- Determinism: same tree twice → identical boxes + identical PNG.
- Contract surface: kind/caps, two surfaces paint equal, clip holds
  (outside white), half-alpha layer blends to ≈162 (±1), destroy →
  `UnknownSurface`, zero-size → `BadSurface`.
- RImg refused loudly (`UnsupportedOp`, surface pristine).
- Paint-phase wiring: mount plan (2 ops) through the phase with ink
  pixels on the surface; next frame's plan is empty in-phase.
- DWrite (Windows): "Hi" shapes to 2 glyphs, advances
  **[11.359375, 3.875] total 15.234375**; the plan's Text op carries
  identical ids + advances (never re-shaped); paints.

### 5j.3 Measured tripwires (both pre-answered as measure-and-document)

- **Damage-discipline payoff:** mount 2 ops / 3 visited / 0 skipped;
  static rebuild 0 ops / 1 visited / 3 skipped (100% skipped, zero
  raster); text change 1 op vs full 2 (visited 3, skipped 0 — the
  open chain walks clean ancestors). Verdict: DOCUMENTED FINDING,
  not a re-architecture — at this scale the walk is 3 nodes so the
  CPU saving is trivially positive, and the empty-plan skip removes
  all raster on static frames; the payoff that matters is
  architectural (the M8 frame-by-frame substrate exists now).
- **CPU glyph quality vs native:** advances honored subpixel-exact
  (11.359375 carried into the plan; pixel 19 is a partial-coverage
  AA fringe strictly between ink and bg — asserted). Cells are
  solid fills (no outlines): `Caps::text_as_paths=false` names the
  gap honestly. The M6 review starts here — no blind tuning done.

## 5k. M5 — events, hit-testing, focus, first real widget (this round)

Per BUILD-ORDER M5: normalized `InputEvent` plumbing, core-side
hit-test walk, `pressed()`/`hovered()`/`focused()` wired through
the hit-test, keyboard events + deterministic Tab order, the §4.1
Toggle end-to-end on the CPU backend. Proves locked #7 (one
normalized enum + framework hit-testing everywhere); §4.1's central
claim — one propagation mechanism replacing six — is
load-bearing-tested (stuck-pressed-on-cancel solved in framework
primitives, semantics written in the same expression as visuals);
#3 exercised by a stateful interactive widget. No lock needed
changing (decisions 93–102 are interpretations).

### 5k.1 What was built

- **`oppa::input` (new module):** `InputEvent`
  (Pointer/Key/Focus/Scroll/Ime with real payloads — positions,
  codes, modifiers, targets), `PointerAction`, `Modifiers`,
  `KeyState`, v1 key codes, and the pure tree walks (`hit_test`
  over committed boxes — renderer-independent; `press_owner_node`;
  `handler_of`/`press_handler_of`; `is_within`; `tab_order`).
  The M0 `Event { kind, handler }` stays as the registry-dispatch
  seam underneath (handlers stay ids — ADR-0007, no identity
  redesign).
- **Runtime:** `queued_inputs` + `push_input` (injection path,
  requests a frame), `set_input_hook` (host router, same shape as
  the layout/paint passes), `handler_owners`/`input_owner`/
  `register_handler_owned` (routing table only — identity
  untouched). INPUT drains inputs inside the same `BatchGuard` as
  events; `has_demand` covers the queue.
- **Host router (`component.rs`):** framework-owned `InputState`
  (hover/capture/focus), the capture machine (Move→hover;
  Down→capture + pressed + focus-follows-click; Up→dispatch iff the
  up-hit is in the capture subtree; Cancel→clear, no dispatch),
  keyboard (Tab/Shift+Tab wrap, Enter/Space pulse + dispatch,
  Escape blur, quiet unhandled keys), Scroll/Ime kind-dispatch or
  loud miss, validated explicit Focus. Redundant flag writes
  skipped; `run_instance` tags handler registrations with the
  running instance (restored on drop). Public surface:
  `inject_input`, `hit_test`, `tab_order`, node-level reads,
  `debug_instance_flags`.
- **Style fields:** `Border{width,color}` + `.border()` (inset ring,
  paint-only) and `.ink()` with interning coverage; `render.rs`
  module docs retire the two M4 open questions (the lock touch).
  CPU builder: ring as outer + inset fills in existing shape ops
  (no new `DrawOp`, `backend.rs` untouched); ink resolves
  self→ancestors→`INK` (inheritance stated — `Text` leaves carry
  no style slot).
- **Toggle:** the M2 port verbatim plus one additive focus-ring
  border line (same match, same track/knob shapes, same semantics
  expression).

### 5k.2 M5 tests (15 new)

- `m5_input.rs` (10, core): overlap both states + loud misses;
  hover/press/focus transitions with per-instance mirrors; cancel
  tripwire + release-outside silence; knob→track routing; tab
  determinism (two fresh builds) + walks + repeats; Space
  activation; Escape + quiet keys; one-frame settle; loud
  Scroll/Ime/stale-focus refusals.
- `m5_toggle.rs` (4, CPU): press flips pixels AND `checked` (1
  upsert) in one 1-frame commit + oracle 0-px history check;
  measured one-frame test (1 frame, 1 PAINT, non-empty plan);
  focus-ring pixels + exactly-2 ring ops + Escape clearing; ink
  style→op→pixels with the `INK` control.
- Style unit: border/ink structural identity + intern dedup.
- Guards: M2 direct-dispatch toggle tests untouched (11 green —
  seam compatible both ways); M4's 10 unmodified (green —
  no backend change).

### 5k.3 Measured tripwires

- **Cancel case:** press/cancel/leave/release-outside → capture
  cleared, 0 dispatches, `checked` false, settles promptly. Clean.
- **Input→visual:** Down+Up → settled state + pixels + dispatch in
  **1 frame** (exactly one PAINT phase).
- Probe-point lesson: interior probes must clear the inset corner
  arcs ((5,20) reads ring color by correct geometry); hover-tint
  priority (pressed > hovered > on) is asserted behavior.

## 5l. M6 — Vello backend + driver matrix (this round)

Per BUILD-ORDER M6 (parallel with M7 per the dependency graph): the
first GPU presenter on the contract M4 proved, driven by the same
dirty-subtree FramePlans the CPU backend consumes. Proves locked
#17 (Vello desktop GPU), #21 (per-surface commit, ≤1-frame skew
observable and bounded), #18 (TIME interpolation serviced where the
compositor is us). Tripwire evaluated at this milestone's gate on
evidence; Skia hatch stays costed, unbuilt.

### 5l.1 What was built

- **`oppa-vello` (new crate):** `GlyphAtlas` (single-face v1 bound +
  placement log — pre-shaped cells in, never re-shaped);
  `encode_plan` (every `DrawOp` into a real `vello::Scene` with the
  CPU replay's discipline: alpha handling, LIFO clip/layer stacks
  with layers-first merged pops, loud `RImg`/no-font refusals
  before staging); `VelloBackend` (`RendererBackend` impl:
  per-surface scenes + `NodeId` registry + retained-op replay +
  vsync present ledger + skew bound + headless GPU readback);
  `GpuOracle` (CPU-vs-Vello pixel compare: exact + tol-banded +
  ink-column diffs); `install_vello_paint_hook` (PAINT-phase wiring,
  same shape as the CPU hook). Consumes the shared `oppa-cpu`
  `FramePlanBuilder` (same plans, second rasterizer — shares no
  raster code with it).
- **F1 engine fix (`oppa::layout`):** `commit_box` + the root-resize
  path stamp `PAINT` on boxes that changed with no `FRAME_DIRT`
  (`FRAME_DIRT` mirrors M4's `FRAME_MASK`), counted in
  `LayoutStats::paint_stamped` (decision 104 — F1 closed).
- **Contract lock touch (`oppa::render` + builder):**
  `DrawOp::Text` gains `baseline` (decision 105); the CPU backend
  ignores it (cells unchanged); the Vello encoder places the
  glyph-run origin at `y + baseline`.
- **DWrite engine fix (`oppa-text-dwrite`):**
  `face_file_reference` never resolved a file (finding F4 — the
  spike's debug renderer silently skipped every text run on the
  `None` path); fixed (two-step `GetFiles` + local-loader path
  resolution).
- **Test seam (`oppa::component`):** additive `ComponentHost::
  with_clock` for deterministic vsync-cadence proofs (TIME + mock
  clock, no behavior change).

### 5l.2 M6 tests (`tests/m6_vello.rs`, 18 tests)

- Headless (run everywhere): DrawOp coverage (7 staged shapes incl.
  the M5 border-ring pair) + `RImg`/no-font loud refusals; Caps
  declaration (+ the CPU row beside it); multi-surface/destroy/
  zero-size/unknown loud misses; unchanged-surface skip (static
  stages 0 work); skew ledger (0 together, 2 staggered as a
  violation, 0 on catch-up); TIME interpolation at vsync cadence
  (5 ticks, monotonic x, 5 presents at 1/60 spacing, static tick
  stages 0); F1 keyed-removal stamp (stamped 1, rebuilt 1 op +
  2 damage, history == full 0 px); box determinism; opacity encode
  (no contract change); Vello paint-hook parity.
- GPU + DirectWrite (Windows + hardware, loud requirement): adapter
  matrix rows; mount plan still M4-shaped (RRect + Text); atlas
  fidelity (DWrite "Hi" advances flow unmodified, subpixel pen
  exact); geometry oracle split (strict axis-aligned 0/0; curves
  tol-16 within 60); text position equivalence (ink columns 0,
  shape diff nonzero by design); glyph review (outline-AA fringe
  + alpha pixel proof, tol-2 diff 0).
- Guards green: all M4 (with the F1 damage 1→2 update) + M5 suites
  unmodified otherwise.

### 5l.3 Measured numbers (not claimed)

- **Atlas fidelity:** DWrite "Hi" advances [11.359375, 3.875]
  total 15.234375 (the M4 baseline, byte-identical) flow
  unmodified into placed glyphs; max delta 0.0; successive x
  deltas equal advances subpixel-exact.
- **Static-frame GPU work:** 0 staged units (`skipped_empty`,
  meter drains to 0 — mirroring the CPU empty-plan skip).
- **Cross-backend diffs:** strict geometry exact 0 / tol-3 0 of
  7200; curves exact 462 / tol-16 12 (bound 60); text ink columns
  0 with shape diff 1047 (outlines vs cells, expected nonzero);
  alpha tol-2 diff 0 with both blends at ≈162 gray.
- **Glyph review:** 86 outline-AA fringe pixels (beats the M4
  cell-fringe floor); fringe is gray-axis between ink and card bg.
- **F1:** `paint_stamped` 1 on the keyed removal; rebuilt 1 op +
  2 damage; oracle history == full repaint (0 px).
- **TIME:** 5/5 ticks rebuild, x tracks `now × 600` exactly,
  monotonic; 5 presents at 1/60 spacing; settled tick empty.
- **Driver matrix:** primary NVIDIA GeForce RTX 3060 Ti
  (Vulkan) + weakest-available Microsoft Basic Render Driver
  (Dx12) — pixel oracles ran on hardware. GLES 3.1-class
  weakest-hardware row stays open (no GLES adapter on this box;
  M10 Android-device row owns it).

## 5m. M7 — DOM backend + parity corpus (this round)

Per BUILD-ORDER M7 (parallel with M6 per the dependency graph; M6
now done): the third presenter on the M4-proved contract — TreeDiff
in, DOM + CSS out, while CPU and Vello keep proving pixels. Proves
locked #2 (the Web presenter is a real backend on the same
contract) and #23 (offset INPUT-fed, trailing ≤1 frame). The M6
forced remainder (exact em size + per-run font identity, decision
105's stated scope) lands here as a second contract lock touch, not
a quiet widening — with three-backend proof. The verdict gates ONLY
the text/edit path (locked #27); non-text work owes it nothing.

### 5m.1 What was built

- **`oppa-dom` (new crate):** `StyleSheet` (StyleId→stable
  `.s{bits}` rules; static decls incl. the inset-ring box-shadow;
  structural fields emit nothing); `aria_attrs` (the total
  Semantics→ARIA table); `DomBackend` (`RendererBackend` impl:
  NodeId-keyed element registry, retained-read sync, overflow
  container + spacer + slots with `overflow-anchor: none` via the
  sheet, foreign elements, browser-owned scrollTop ledger, loud
  Image/unknown-surface refusals); `render_page` (deterministic
  full page + data-pid hooks); `install_dom_paint_hook`
  (shared-builder PAINT wiring — same plans, same damage
  discipline; shares no raster/DOM code).
- **Contract lock touch (`oppa::render` + layout + builder,
  decision 110):** `DrawOp::Text` gains `em_size` (exact,
  `font_size_px × dpr` at measure time) + `fonts: Vec<FontRun>`
  (per-run `family` + shaper-local `font_id`, merged runs);
  `LaidRun` carries `font_id` (from the source `TextRun`) +
  resolved `family` (engine font table from the service
  enumeration, requested-family fallback); `LaidLine` carries
  `em_size` (post-pass — `layout_text` stays pure and signature-
  stable). CPU ignores the new fields (cells unchanged);
  `Caps::dom` declares the third row (1024/false/true/true).
- **Vello multi-face (decision 110):** atlas holds default +
  per-id faces (`set_font_for`; explicit→default→loud);
  the encoder draws one run per `FontRun` at `font_size =
  em_size` (ends the `= line_height` approximation and the
  single-face bound — finding F3 closed in full).
- **Core additive seams (decisions 112–113):**
  `ElementBuilder::on_scroll/on_ime` (target declaration —
  M5's loud-miss rule stands); `ComponentHost::bind_scroll`
  (the §9.3 INPUT-feed mapping: routed scroll deltas accumulate
  into the framework-owned offset signal inside INPUT's
  `BatchGuard`; v1 vertical-only, `dx` ignored, stated);
  `vnode::TextField` + `Semantics::text_field` (editable leaf —
  no new `Tag`, the #24 behavior flag); `vnode::Custom`
  (authoring for the `Tag::Custom` escape hatch — the
  external-element hole).
- **Corpus substrate (`spike/web/`):** `parity.mjs` (engine page
  vs Edge rects, box ±0.5 / text-width ±1.0, heights record-only)
  + `dom_text.mjs` (shared op suites vs the real `<input>`,
  click-scan boundary mapping, trusted typing path).

### 5m.2 M7 tests (`tests/m7_dom.rs`, 27 tests + 7 lib unit)

- Headless (run everywhere): mount/update/reorder/remove
  minimality; scroll-tick zero structure ops (TreeDiff count +
  DOM mutation count); CSS identity/churn/no-inline-spam;
  scroll shape (overflow + spacer + anchor-off) + overscan math;
  ≤1-frame currency on the injected clock (applied once,
  never double); ARIA switch parity + payload removal;
  verdict-(b) input (shape/value/label/absorption/update);
  external hole (marker + box + children); em/fonts plan
  assertions (TwoFaceFake: Alpha/Beta segmentation) + DOM
  spans + atlas selection; three-backend box compare;
  Caps third row; loud surfaces/Image; DOM paint hook.
- Windows + Edge: per-run Vello encode with real faces
  (Segoe + CJK fallback, placement + face selection exact);
  parity corpus flat subset green; editing suite green.
- Guards green: all M4 + M5 + M6 suites unmodified except the
  one M6 hand-built Text literal (gains the two lock-touch
  fields — the lock touch, not a drive-by).

### 5m.3 Measured numbers (not claimed)

- **Scroll-tick structure ops:** 0 (TreeDiff) + 0 (DOM
  mutations) on a 5-slot keyed tick with all row values
  re-derived (the M8 payoff trace starts here).
- **Offset currency:** injected Scroll sits unapplied before
  the frame, lands after exactly 1 `run_once` (≤1 frame),
  never double-applies; ledger == signal (48.0 == 48.0).
- **Parity corpus:** 10/10 gated rows green (match_rate 1.0;
  Edge 153 headless, dpr 1): 6/6 boxes exact, 2/2 text
  positions exact, untracked text width 81.03125 ==
  81.03125 exact (reproduces M1's finding through the real
  backend) + 1 record-only row (text height 21.28 vs 21,
  dh 0.28 — browser line box vs engine metrics, listed).
- **Editing suite:** latin_edit 10/10 (incl. the browser
  restoring pre-undo selection [3,8)); multibyte dblclick
  [0,3) (CJK dictionary rule confirmed); undo_granularity
  recorded (CDP insertTexts are separate units — "aHello
  world" after one Ctrl+Z; rig-vs-real note, same family as
  M1's CDP quirks).
- **Three-backend boxes:** CPU DrawOp rects == DOM
  serialized geometry exactly; Vello + CPU commit the same
  tree (live counts equal); glyph cells shared.
- **Static-frame DOM work:** settled sync touches 0;
  idle schedules no PAINT (mirrors CPU skip + Vello 0-work).

## 5n. M8 — Virtualization + transition evaluator + §9.4 stamp end-to-end (this round)

Per BUILD-ORDER M8 (deps M5 + M6 + M7 + M2's stamp, all done): the
full §4.2 payoff trace asserted against real backends — recycled
ContactList/ContactRow with slot keys, the TIME transition evaluator
honoring the binding-edge stamp, window-lag compensation under a
scripted offset sweep. Proves locked #13 (zero structure ops per
scroll tick, ~30-cell repaint, per-instance selection), #22/§9.4
(phantom-flash eliminated), §9.3's window-lag compensation.

### 5n.1 What was built

- **Transition evaluator (`oppa::transition`, new module, host-owned):**
  `TransitionEvaluator` (targets per node from retained styles;
  unstamped animatable deltas with a live transition create one TIME
  interpolator per property; stamped deltas snap and count
  `suppressed`; Adds/first-sight/no-transition/zero-duration snap;
  `created`/`active_count`/`live_nodes`/`is_settled` instruments +
  `prune_dead`); `ease_at` closed-form cubics (decision 120); sRGB
  channel lerp (gamma-correct is v2); transparent endpoints snap
  (decision 103); `resolve_bg`/`resolve_opacity` (what backends
  paint); `settle(now)` (the TIME drive). Fed every commit from
  `reconcile_root` with the frame clock; the creating commit
  registers the settle animation once (flag-held) and the animation
  re-dirties live nodes PAINT-dirty per frame (`Reconciler::
  mark_paint_dirty`, new) — decision 125.
- **Builder overlay (`oppa-cpu`):** `build_incremental_evaluated` /
  `build_full_evaluated` (same walk, same masks; `bg`/`opacity`
  through the evaluator at `now`; identity off-interpolation). All
  three paint hooks build evaluated plans at the frame clock
  (decision 122, uniform rule).
- **Order-preserving splice (`oppa_cpu::splice_retained`, shared CPU +
  Vello):** dirty op-runs replace in place (append-at-end covered
  foregrounds on tail frames — track over knob, cell bg over text).
- **DOM mapping (`oppa-dom`):** `transition:` decls per carried
  animatable (`transition_decls`, decision 121); `commit` arms
  `suppress_armed`, exactly one `sync` consumes it (touched elements
  get inline `transition:none`; next sync is the clearing frame);
  `scroll_window_overscan` parameterizes the M7 helper
  (`scroll_window` keeps the one constant).
- **Core additive seams (decisions 124–126):** `HandlerAttachment.
  owner` stamped at render time (`stamp_handler_owner` in
  `Ctx::child` innermost-wins + `run_instance`;
  `register_handler_owned_as` prefers the stamp — finding F6);
  `Ctx::child` nests the `input_owner` guard with restore;
  `Runtime::now_secs` + `input_owner()` accessors;
  `ComponentHost::run_once` + `with_evaluator(_mut)` accessors.
- **M5 frame-count reframe (decision 122):** four assertions
  (`m5_input` cancel + one-frame; `m5_toggle` press-flips +
  one-frame-to-pixels) now read commit-frame (`run_once`: dispatch +
  semantics + capture/focus, same frame, sharper) + interpolation
  tail (settled after). #7 untouched; evaluator internals are not
  contract; the hooks' value source is stated here.

### 5n.2 M8 tests (26 new: 6 lib unit + 10 core + 2 CPU + 6 DOM + 2 Vello)

- Lib unit (`transition::tests`): easing monotonic/exact; mount
  records; unstamped interpolation + exact settle; stamped snap;
  no-transition snap; keyed recycle with live transition.
- `oppa/tests/m8_virtualization.rs` (10): mount/scale slot-key
  stability (N=1000, K=20); full-list zero-structure sweep (977
  stamped moves; leading-overscan + sub-row ticks commit at most
  empty diffs); keyed_state selection out-and-back; per-slot flag
  attribution (hover/press/focus slot-scoped — decision 123);
  TIME-vs-INPUT feed equivalence; TIME evaluator off the injected
  clock (~8 frames for 120 ms, then idle); stamped sweep (zero
  created, zero flashes, selection jumps under the stamp);
  no-transition counter-discriminator.
- `oppa-cpu/tests/m8_sweep.rs` (2): N=300 FakeText oracle-exact
  sweep + repaint bound (decision 119's numbers); +2/+4 overscan
  experiment.
- `oppa-dom/tests/m8_transitions.rs` (6): CSS decls + no-inline
  rule; one-commit stamp sequence (mount-stamped, unstamped clear,
  stamped flag, clearing frame, quiet); overscan math; spec +
  as-built lag cover; INPUT-fed DOM sweep (structure 0 + mutations
  0 + touched bound + churn accounting).
- `oppa-vello/tests/m8_sweep.rs` (2): headless plan-level sweep
  (geometry rows); Windows+GPU pixel sweep exact per tick.
- Guards green: M4 + M6 (18, incl. GPU rows) + M7 (27) behaviorally
  unmodified; M5 reframed per decision 122 (substance untouched).

### 5n.3 Measured numbers (not claimed)

- **Structure ops/tick:** 0 on every window move (977-move core
  sweep N=1000; 276-move CPU/DOM sweeps N=300; 126-move Vello
  sweeps N=150). Non-moving ticks commit at most empty diffs.
- **Repainted cells/tick:** 20 max (K=20; the §4.2 unit is cells —
  20 ≤ ~30). Builder ops 62 max (~3/slot: bg + 2 text + clip
  pair); damage 121 max (6 nodes/slot); over=2: 50 ops / 16 cells.
  Vello headless (geometry rows): 22 ops, staged work 21/tick max.
- **Phantom-flash count:** 0 on every sweep (evaluated == target
  per cell per tick) with live 120 ms transitions on every row.
- **Interpolators on binding-edge commits:** 0 created (instrument
  delta); suppression counted (non-vacuous); unstamped changes
  create exactly 1/property, settle exact at 120 ms, loop idles.
- **Oracle deltas:** CPU exact 0 every tick; Vello exact 0 +
  tol-16 0 every tick (rects have no AA ramp); DOM mutations Δ 0,
  touched ≤ 6K+8; Vello registry tracks retained 1:1.
- **Lag cover:** spec ±4 rows/frame (over=4) / ±2 (over=2);
  as-built fixed-K window covers the same on rendered rows,
  sub-row steps included (the +1 straddle margin — decision 127).
- **Stylesheet churn:** exactly +1 rule per window move (entering
  edge slot's new `absolute_y` — decision 111 identity is
  payload-based; rules declaration-identical; dedup follow-up
  recorded, not built).

## 5o. M9 — reload product loop + fuzzer gate (this round)

Per BUILD-ORDER M9 (deps M2b + M8, both done): the swap integrated
with the real loop — reload mid-scroll, mid-transition,
mid-IME-composition, mid-input-burst, with live `ctx.spawn`-path
tasks — and the fuzzer extended to the full §8.4+§9.6 matrix.
Proves **#15 under adversarial timing, #25 under load**; the gate
it guards (renderer freeze per §8.4's placement) is DECLARED in
`docs/03-spec/reload/freeze.md`. No lock needed changing
(decisions 129–134 are interpretations + scope).

### 5o.1 M2b verdict first (scaffolding vs working path)

M2b left a working swap path, confirmed against `crates/` before
assuming either way: manifest export/scan
(`component_manifest!` + `HotRegistry::install` rescan),
**real** dylib swap (`DylibSource` over `libloading`,
adopt/reseed/tracking/discovery/registry across images —
`real_dylib.rs` green), typed drain/adopt with the hot-glue panic
rule, drain-before-unload ordering, atomic registry flip via
post-swap re-runs, per-run symbol resolution (stale-code fix),
shared-state run stacks (TLS fix), retire-not-unload (decision
61), generation-tagged executor with cancel-at-RELOAD +
discard-by-tag. What M9 adds is the product-loop timing: swaps
fired mid-gesture/mid-interpolation/mid-composition/mid-burst/
mid-task through both the direct (`reload_to`: swap-then-drain)
and RELOAD-hook (`request_swap` + frames: INPUT→RELOAD→EFFECTS)
paths. True unload (shared-core linking) stays deferred —
decision 61 stands, the retire model re-confirmed with its bounded
leak counted per report.

### 5o.2 The gate fuzzer (`oppa-reload/tests/m9_reload_gate.rs`, 6 tests)

Five scenarios + the mechanical proof, each with its own seed
(`OPPA_FUZZ_SEED` override, seed printed first), xorshift
interleavings of workload ops / clock / burst sizes / task sleeps
/ swap points / swap paths. Default-seed counters (seeds 1, 42
repeat green, same shape — ROUNDS entry):

- **mid-scroll** (250 iters, N=200/K=20 slot-keyed list, zebra +
  120 ms rows): 52 swaps (14 hook + 38 direct), 38 mid-sweep, 56
  INPUT-fed + 63 TIME-fed scrolls, 66 non-vacuous stamped ticks.
  Every tick: 0 structure ops; stamped ticks: 0 created;
  slot ids stable across every swap; no retired props.
- **mid-transition** (250 iters, 20 ms flip rows — decision 130):
  75 flips, 95 swaps, **37 swaps with a live interpolator**,
  75 exact settles. Live nodes resolve every swap and flip.
- **mid-IME** (200 iters, 104 sequences / 363 normalized events):
  78 swaps, **51 mid-composition**; the buffer and canonical
  stream are byte-identical across every one; Ime kind-routing
  survives (handler re-registration via re-runs).
- **mid-input-burst** (200 iters, 127 bursts / 6405 events):
  60 hook + 105 direct swaps; 60/60 hook frames show
  INPUT→RELOAD ordering (pre-drained phase log); exactly-once
  per burst under both orders (pairs == hits delta; quiet keys
  never dispatch).
- **in-flight tasks** (150 iters, 178 spawned): 76 swaps, race
  hit 76/76 (pending-drop and/or retired-discard every swap);
  applied-path 26/26 same-gen applies; end state
  done+dropped == spawned, applied == 26 (only no-swap tasks).
- **generational proof**: Retired/StaleGeneration/
  AlreadyRetired/OutOfBounds all loud on `GenArena`, retired
  signal reads panic, `NodeId` arena same discipline — the
  checks the fuzzer relies on fire.

### 5o.3 Measured numbers (default seed; the gate's evidence)

- Structure ops on scroll ticks: 0 (119 measured ticks incl.
  sub-row straddles, whole-row steps, jumps, fling-backs).
- Interpolators created on stamped commits: 0 (scroll ticks +
  swap re-runs); suppressed counted non-vacuous 66×.
- Phantom-flash class: settled == target every flip (75/75);
  live-at-swap interpolators resolve 37/37.
- Composition buffers preserved 51/51; canonical stream
  undisturbed 51/51; Ime routes 1:1.
- Burst exactly-once: 127/127 bursts (6405 events, 5–40 pairs
  each + quiet keys); ordering frames 60/60.
- Task partition: applied 26 (all no-swap), discarded 73,
  dropped 79; mailbox touched by retired gens 0×.
- Engine violations found: **0** on three seeds. Three
  test-side rig bugs found and fixed in-round (5o.4); they are
  disclosed, not counted.

### 5o.4 Findings fixed inside the round (rig bugs, not engine)

1. **Stamp measured at the wrong point** (vacuous suppression):
   first draft snapshotted `suppressed` around swaps (which change
   nothing → stamp nothing). Moved to scroll ticks; swaps assert
   created-delta 0 only.
2. **Settle-vs-live timing**: `run_until_idle` wall-settles the
   20 ms interp, so liveness after it is always 0. Creation
   proven via monotonic `created`; liveness observed after one
   `run_once`.
3. **Byte-vs-char writer**: the IME writer panicked on
   `insert_str`/`drain` once content went multibyte. Clamped to
   char floors (test-side simplicity — decision 132).

## 5p. M10 — Android + a11y emitters + GLES row (this round,
last v1 milestone)

Per BUILD-ORDER M10: the Android shell (surface/input/lifecycle),
restart-only reload, the GLES weakest-hardware row, and the
AT-SPI emitters. No lock needed changing (decisions 135–141 are
scope + interpretation).

### 5p.1 Android shell (`crates/oppa-shell-android`, std + oppa)

`AndroidShell` implements `PlatformShell` with Win32's trait
shape (`pump_events` + `set_ime` log + 1:1 `AndroidCmd` queue):
MotionEvent down/move/up/cancel → `InputEvent` pointer
constructors, keycodes → `Key` (BACK dismisses to ESCAPE, unknown
codes pass through for the router's quiet rule — decision 96),
dp × density at the boundary, focus intents shell-side (no
fabricated targets). Multi-touch is a loud `ShellError`, never a
merge. `AndroidLifecycle` (Created→Started→Resumed→Paused→
Stopped→Destroyed + relaunch edges; illegal jumps `Err`): pause
closes the render gate with the graph retained, resume wakes
once, destroy arms restart. No NDK/JNI linkage here — the
Activity glue is a thin forwarder (platform-track follow-up).

### 5p.2 Restart-only reload (lock #16, re-confirmed)

No `oppa-reload` dependency in the shell crate (no dylib path by
construction); current AOSP sepolicy still carries the W^X
`neverallow`s and Play policy still bans untrusted code, so the
R3 grounding constraint stands unchanged. Relaunch builds a
fresh host — proven pixel- and dump-identical to cold start
under the same Android-classified script (3 presses: PNG equal,
dump equal, checked both).

### 5p.3 GLES row (`oppa-vello`: `probe_gles_adapter` +
`ensure_gpu_gles` + `tests/m10_gles.rs`)

GL-only wgpu instance at LowPower. Surprise against the plan:
this box HAS a GL driver (RTX via WGL), so the row ran for real
instead of recording `Err`: CPU-vs-Vello(GL device) exact 0 /
tol-16 0 on sharp rects at 1080×2400 (the M6 standard), and the
CPU fallback measured at the same standard (incremental == full,
0 px; `Caps::cpu_fallback`; `RImg` loud + pristine). Stand-in
statement: desktop GL proves backend-path conformance, not
weak-mobile-GPU performance — frame cost stays device-owned
(`08-performance/mobile.md`: bet half measured).

### 5p.4 AT-SPI emitters (`crates/oppa-atspi`, std + oppa,
Linux-only scope)

Total role/state table (canonical at-spi2 names — switch →
"toggle button", list-item → "list item", text-field → "entry",
generic → "filler"; `checkable`/`checked`, `selectable`/
`selected`, `editable`, `enabled`/`sensitive`) + incremental
`AtspiTree` (value changes emit state events only — the M8
scroll-tick load case never resyncs; removals announce once and
forget) + wire vocabulary (`object:state-changed:*`,
`children-changed`, `property-change`). Toggle + list-item proven
emitting and queryable through the real diff pipeline, including
slot-rebind value announcements with no re-add. Android's
accessibility service and Windows UIA explicitly out (different
APIs — named residuals). Live-bus validation needs Linux
(session bus + registryd) and is open.

### 5p.5 Findings in the round (infra, not engine)

1. **Corrupt rmeta** (`can't find crate for oppa_cpu` with the
   file present): fixed by deleting the single rmeta and
   rechecking — transient, recorded so the next occurrence is
   recognized.
2. **GL test flake under parallel threads** (green alone, one
   full-file failure): serialized the two GPU-touching tests
   behind a file-static lock (M6 `GPU_LOCK` family) — 3/3 green
   since. One later full-workspace failure with no captured
   message: `ensure_gpu_gles` now retries once with both errors
   logged (persistent failure still fails loudly).
3. **Two test-side bugs** (my assertions, not the engine):
   un-drained mount events in the AT-SPI removal test; two-press
   script ending with checked == false in the restart test.

### 5p.6 Android-gap closure (follow-up, same day)

Emulator booted headless (`-no-window -no-audio -no-boot-anim
-gpu swiftshader_indirect`; process `qemu-system-x86_64-headless`,
`emulator-5554`, left running) and interrogated via adb —
`sys.boot_completed=1` on first poll. Measured: API 36, x86_64,
`ro.opengles.version` 0x30000, SurfaceFlinger GLES "Google
SwiftShader 4.0.0.1 / OpenGL ES 3.0" with
`ANDROID_EMU_gles_max_version_3_0`, GLES RenderEngine
(`vulkan_renderengine: false`), SELinux Enforcing, display
1080×2400@420 (the m10 scene resolution exactly; density 2.625
vs the test's 2.0). No on-device executables: NDK absent, so no
cross-compiled binaries, no APK, no on-device GLES pixels —
the version interrogation is the measurement. W^X liveness on
the image follows from Enforcing + API 36 (targetSdk>28
regime); nothing exploit-like attempted.

## 6. Interpretation decisions (where the docs were ambiguous)

### M0 decisions

1. **`ctx.binding` / `keyed_state` were NOT built.** They were named in the
   milestone request alongside the "five primitives", but the five locked
   primitives (§7.9) are Signal/Memo/Effect/BatchGuard/untrack, and
   BUILD-ORDER assigns `ctx.binding` trigger-set tracking and
   `keyed_state`+LRU to **M2**. BUILD-ORDER was followed; the binding-edge
   machinery slots into `mark_dirty`/pass triggers at M2.
   **SUPERSEDED by M2 (this round):** both are built — `Ctx::binding`
   (memo variant flagged `is_binding`, value change raises the per-commit
   stamp consumed as `TreeDiff.suppress_transitions`) and core-side
   `keyed_state` with capacity-64 LRU (§5g.2–§5g.3, decisions 48–52).
2. **Pull-recompute reading**: "its downstream propagation still happens in
   EFFECTS" is interpreted as "no effect/component *execution* mid-INPUT",
   not "no dirty-marking mid-INPUT". A pulled dirty memo recomputes
   immediately and marks direct dependents dirty (effects stay dirty until
   EFFECTS). The strictly-deferred reading would yield stale values for
   chained memo reads unless every read version-checks; version checks are
   present anyway (for mid-batch correctness).
3. **Memos-never-write panics in release too — RESOLVED, locked.** The
   lock said debug-assert; the implementation panics in all profiles and
   was flagged for review. Review verdict: keep the release-mode panic —
   a silently-dropped write would corrupt application state invisibly and
   be far harder to diagnose in the field than a crash carrying the exact
   message already implemented ("memos never write signals (§9.1 hard
   invariant)"). Recorded as DESIGN.md locked **#26** (amendment to
   #19/§9.1); the open question is closed, no code change required.
4. **Release budget behavior**: "defer the remaining dirt to the next frame's
   EFFECTS (same budget)" is implemented as one retry frame, then park. A
   strict every-frame retry would itself be the livelock the same paragraph
   forbids. Rate-limiting = the two-step log (defer frame + park frame).
5. **PlatformShell** carries `pump_events` (exercised) and `set_ime` (default
   no-op; the spike wires real IME through it). The rest of §2.2's trait
   (`request_frame`, `set_dpi_aware`, `set_cursor`, `semantics`, `text()`)
   joins when its consumers exist (platform shells; `text()` waits for M3
   when layout consumes the TextService) — the method set stays additive so
   the M0 loop does not move.
6. **TIME** services an animation-callback registry (`FnMut(f64) -> bool`).
   The transition evaluator (M8) and scroll physics (M8/§9.3) plug into this
   registry; nothing transition-shaped is built.
7. **RELOAD** at M0 is the phase position + hook semantics (registry flip +
   batched writes in-phase). Drain-before-unload, opaque props, manifest
   scan, generation-tagged task cancellation: M2b/M9.
8. **Event shape**: `Event { kind, handler }` is deliberately minimal
   (pre-routed); the full normalized `InputEvent` enum and hit-test routing
   arrive at M5. The registry contract (handler-as-id) is already final.

### M0b decisions

9. **Backend pick was effectively predetermined**: BUILD-ORDER routes the
   DirectWrite slice onto the spike's critical path, and the development
   environment is Windows. Stated rather than assumed; other platforms'
   backends (rustybuzz/swash for wasm/Linux, HarfBuzz-class for Linux,
   platform APIs for Android) are separate follow-up work.
10. **Device-px outputs**: the em size handed to the shaper is
    `font_size_px × device_pixel_ratio`, so all returned advances/offsets/
    carets are device px and the spike's ±2-device-px caret checks apply
    directly. §8.8's rounding is the shared `round_to_device_px` helper,
    applied at commit positions only (never inside shaping).
11. **Run ordering**: runs are emitted in source order for an LTR base
    direction; true bidi reordering is layout-engine work (M3, v1 bound).
    RTL runs are still shaped correctly (bidi captured via
    `SetBidiLevel`; odd resolved level → `rtl` flag on the run) so the
    spike's corpus can exercise them.
12. **Plain shaping**: no typographic features are passed (empty feature
    list), so no ligatures form and clusters map cleanly for the spike's
    covered scripts. Ligature features are a later authoring concern.
13. **Loud family check**: an unknown family fails with `FontNotFound`
    before shaping; DirectWrite's `MapCharacters` would otherwise silently
    substitute a default font. The system fallback still covers
    missing-glyph runs within a valid family (asserted by the CJK test).
14. **Letter tracking semantics**: spacing is added to every advance except
    the run's final glyph, so the trailing caret position equals the run
    width (no phantom trailing gap).
15. **Caret/cluster rules**: caret x = containing cluster's leading edge
    (mid-cluster bytes snap; never across a cluster boundary); end-of-text
    caret = total advance; hit-test uses cluster midpoints (leading half →
    start byte, trailing half → end byte). Cluster-to-glyph ranges assume
    monotonicity within a piece (true for LTR single-direction pieces; the
    spike corpus is the check).
16. **`enumerate_fonts`**: the first localized name (index 0) per family;
    `FontId = family_index × 4096 + font_index`, deterministic for a stable
    font set.

### Spike decisions (this round)

17. **Criterion-1 tolerance N = 2 device px** (DESIGN left it as a
    placeholder): matches M0b's handoff ("caret ±2 device px, never across
    cluster boundaries"), equals 1 CSS px at DPR 2, sits far inside
    DESIGN's "within one caret height" bound, and is the tightest bound
    that does not require two different rendering pipelines
    (GetGlyphs/GetGlyphPlacements vs IDWriteTextLayout, and GDI-hinted
    integer advances for the EDIT control) to agree subpixel-exactly.
18. **Criterion 1 is Windows-arm-scoped, concretized** against this
    corpus: framework `caret_rect` (as delivered through `set_ime`) vs
    `IDWriteTextLayout::HitTestTextPosition` (both DPRs) and vs a real
    Win32 EDIT control (DPR 1, DPI-unaware thread context), every cluster
    boundary + trailing caret, plus tracking through every composition
    edit and in-composition caret navigation step. The Web-DOM arm's
    native IME anchoring is the browser's own; a real `<input>` exposes no
    queryable caret rect to the framework (recorded: the selection range
    rect reads as empty) — noted as a (b)-side fact, not a pass/fail.
19. **Scripted IME on both arms; "candidate selection" emulated as
    commit-with-different-text** (nihao → 你好; konnitiha → kana conversion
    step → 今日は). A real OS IME's candidate UI is not scriptable in
    either rig; the spike's question is the event stream and in-progress
    state, not the candidate UI. Windows arm: `ImeCompositionFeed` through
    `dispatch_ime_event`. Web arm: CDP IME scripting — which routes
    through Chromium's real text-input state machine (an OS IME's path),
    not a JS-side emulation; commit via `Input.insertText` during active
    composition (measured: compositionupdate + compositionend-with-data —
    the native commit shape; `Input.imeCommitText` does not exist in this
    Edge build).
20. **`CompositionUpdated.caret_byte` is composite coordinates** (content
    prefix + composition text), consistent with M0b's existing test data
    (`start_byte 4`, caret 10 for a 6-byte composition); the session
    stores the composition-relative caret internally.
21. **Focus-loss policy — RESOLVED, adopted as spec.** The spike session
    cancels mid-composition on
    focus loss (committing nothing, per M0b's cancel test); Chromium
    *commits* on blur (compositionend-with-data). Every native platform
    commits on focus loss, so the editing session **adopts
    commit-on-focus-loss** — the spike session's cancel-on-blur was the
    outlier, not Chromium. Was recorded as a behavior-contract alignment
    item, not an authority-model failure; adopted into the
    editing-session spec by the M1 merge round (DESIGN §9.2 + locked
    #27).
22. **The spike's word rule was spike-local — its replacement rules are
    now adopted as spec**: contiguous alphanumerics =
    one word; each CJK ideograph/kana/hangul char = its own word; other =
    separator. The rig deliberately exposed the browser's divergent
    conventions (trailing-space inclusion on double-click; CJK dictionary
    segmentation of 日本語). Under verdict (b) the shared editing-op
    suite is the permanent cross-backend contract test and its rules are
    spec: mid-cluster hit-test ties resolve to the leading edge;
    double-click word selection adopts browser-compatible conventions
    including CJK dictionary segmentation — **binding the Windows-GPU
    session too** (adopted by the M1 merge round; DESIGN §9.2 + locked
    #27).
23. **Variant A on Web was not built** (round scope: two arms, Windows-GPU
    vs Web-DOM). DESIGN's spike text names three variants; the (a)-on-Web
    risks it alone measures (hidden-input candidate anchoring, framework-
    rendered hit-test parity, ARIA-mediated a11y) are recorded as the
    residual open item in the verdict. The (a)/(b) decision rule is
    applied with that mapping stated explicitly, not silently. Still
    open after the M1 merge — tracked (§5.3), not resolved by it.
24. **RTL/bidi and combining-marks/ZWJ corpus entries are a stated gap**,
    not silently covered: M0b's handoff puts bidi ordering at M3 (runs
    carry `rtl` as metadata; ordering is source order), and this round's
    corpus scope is the one handed down (ASCII + héllo/日本語 + surrogate
    pair + zh/ja composition). A bidi corpus entry would silently re-open
    M3 scope; flagged instead (verdict.json `out_of_scope_flags`).
25. **Geometry-addressed suite ops resolve x against the *current*
    composite** (the text evolves during the suite): the Windows session
    resolves "cluster k" to its own cluster leading edge (+1 px, leading
    half) of the current composite, and the Web arm clicks the same
    recorded x against the same current value — so criterion 4 compares
    editing behavior, not static-geometry artifacts. Index-addressed
    normalization is code-point indices on both arms (UTF-8 bytes on
    Windows, UTF-16 units on Web, mapped through the shared cluster
    tables).
26. **The spike's runtime integration is deliberately minimal**: content
    is a real `Signal`, frames run via `rt.run_once()` after each scripted
    step, and IME anchoring flows through the shell seam — but the full
    INPUT-phase routing/hit-test pipeline is M5, so the rig drives the
    session's methods directly (the model claims rest on the session's
    shape, not on full scheduler integration; stated, not assumed).

### M1 remainder decisions (this round)

27. **The shell's payload queue (`Cmd`) exists because M0's `Event`
    shape cannot carry payloads** (kind + pre-routed handler only; the
    full `InputEvent` enum is M5). The shell emits both: the
    M0-normalized events (the trait/registry contract — the runtime's
    INPUT phase pumps and dispatches them) and the 1:1 payload queue
    (drained by the registered field handler in event order). Stated
    rather than silently diverging.
28. **The mapper's composition-over-selection ordering** (spec to be
    verified, not asserted): the app-side adapter clears the active
    selection at composition start (the behavior real editors
    implement); the mapper feeds `CompositionStarted{anchor}` BEFORE
    `DeleteRange{selection}` so the session's atomic pre-composition
    undo snapshot captures the PRE-deletion content. Unverified this
    round (the composition never engaged — §5b.3); recorded as
    spec-to-be-verified.
29. **The automated pass IS the manual pass** (real keys through the
    real OS IME via `SendInput` — the gate's definition). The rig
    drives it programmatically because there is no human in the loop in
    this environment; the pass DID drive real input, and what it could
    not do was engage the composition engine (the finding).
30. **`DWriteTextService::font_file_source(font_id) ->
    Option<(path, face_index)>`** — a debug-renderer hook (inherent
    method, NOT in the frozen `TextService` trait), recorded at first
    sight during `shape` and consumed by the Vello debug renderer to
    load the same physical file (path + face index for TTCs) that
    DirectWrite shaped from. The real Vello backend (M3+) owns its own
    glyph-atlas path and will not consume this.

### TSF re-run decisions (this round)

31. **`punk=None` is the documented stopping point, not a shortcut.**
    `CreateContext` accepts a NULL text-store object (the call returned
    S_OK with edit_cookie 0), so ThreadMgr + DocMgr + association + focus
    + declared scope is a complete, honest increment — and the pass
    result (still no engagement) now isolates the remaining variable to
    the missing `ITextStoreACP` (+ the E_FAIL input-scope property)
    rather than the whole TSF stack. Building the store is the next
    hypothesis, stated as such (decision 34), not smuggled into this
    round's claims.
32. **"Same rig, unchanged" means steps/keys/timing/verdict.**
    The host diff is additive only: `enable_tsf` after `arm_ime`,
    per-step `tsf_reassert_focus` lines into the step notes, and
    `tsf_note_focus` on `FocusChanged` (the session still ignores focus
    cmds). STEPS, `send_keys`, the 320 ms/12 ms settle, and all six
    `check_*` functions are untouched — so the FAIL is comparable with
    the M1 remainder FAIL, not a new rig's verdict.
33. **Borrow-across-dispatch is a shell invariant now.** TSF association
    made `SetForegroundWindow` synchronously re-enter the proc
    (`ImmActivateLayout` → `SendMessage` → proc) and exposed that
    `focus_window`/`set_ime` held the shared borrow across OS calls
    (first `--ime-pass` run panicked in the proc). Rule recorded:
    shell methods copy the HWND out before any OS/TSF call that can
    dispatch. Fixed in `focus_window` and the `set_ime` IMM sequence.
34. **The next fix is a guess and is labeled one.** A real
    `ITextStoreACP` (text + selection + `AdviseSink`/`RequestLock`
    discipline over the field's content) is the obvious next engagement
    hypothesis — it is UNTESTED and is not claimed as the fix. Per the
    round's rules no further fix was attempted after the FAIL.

### Text-store round decisions (this round)

35. **Composition callbacks arrive via owner QI, not `AdviseSink`.**
    Two runs of evidence: advising `ITfContextOwnerCompositionSink` on
    the context source fails `TS_E_NOOBJECT` (0x80040202 — identified
    from the generated bindings, not guessed). The context discovers
    the interface by QI'ing `CreateContext`'s punk, so it is implemented
    on `ShellStore` itself. `ITfTextEditSink` IS advised (cookie 1) and
    stays as the flush path.
36. **M0 `input_phase` must not hold the state borrow across the pump.**
    The first signal-writing pump callback in the project's history
    (mapper → session, via the new composition messages) panicked in
    `Signal::get`. Fix is take-shell-out + restore, mirroring the TIME
    phase's animation-closure shape. Pump callbacks may write signals —
    now a stated runtime invariant, not an accident.
37. **`--wait-secs` is harness, not pass content.** It pumps messages
    pre-pass so a human can focus the window and records
    foreground-at-end; steps, keys, settle, verdict untouched. A late
    focus click can still race the first keys (this round's c1) — clean
    runs must be hands-off end to end (`IME-CONFOUNDER.md` §7).
38. **Non-empty final span commits; empty cancels — and the TIP's action
    is translated, not second-guessed.** Esc ended WITH the reading
    (`"o"`), so it committed. If that mismatches IMM-era cancel
    expectations, the scenario (not the translation) is the open item.
    Same for letter-by-letter commit spans: real-TIP behavior,
    faithfully mirrored.
39. **Store posture: strict mutations, lenient reads, `E_NOTIMPL`
    geometry.** `SetText`/`Insert`/`SetSelection` require a held lock
    (`TS_E_NOLOCK` otherwise, logged); reads never fail for lock state;
    views/points/extents/embedded are `E_NOTIMPL`; attributes zero. Any
    of these that a TIP actually needs will show up in the store trace
    as the next evidence — none did this round.

### Verification-round decisions (this round)

40. **Profile activation S_OK ⟺ focused window at call time.** Across
    all eight runs plus the windowless probe: every E_INVALIDARG
    coincides with no foreground window on the calling thread
    (fg False at init/home in all three failing runs; the probe fails
    deterministically with no window at all); all five S_OK runs were
    focused. Mechanism unproven (Win32 documents only the args, not
    the focus precondition), but the correlation is total and the
    reordered arm (activate after focus+HKL+IMM) is 3/3 S_OK. No code
    depends on the theory — only run discipline (focus settles first).
41. **Separator normalization in the r-checks.** Real readings carry
    Pinyin syllable quotes; the session mirrors the TIP byte-exact per
    its contract, so the check strips `'` and asserts
    caret-at-reading-end rather than hardcoding one formatting.
42. **c4's `caret == 0` was a check bug.** The pre-composition state IS
    the select-all state (caret 11 by `select_all`'s own contract);
    the snapshot restores it faithfully. Corrected to the c1 state,
    not relaxed.
43. **Gate (a) closed as locked #28.** Two consecutive hands-off PASS
    runs (6/6, zero divergences) verify the delete-range-mid-
    composition path against the real IME. **(b) (RTL/bidi +
    combining-marks/ZWJ corpus) is now the only open freeze condition**
    on the DOM text/editing contract.

### Bidi/combining/ZWJ decisions (this round)

44. **No shell text round-trips on UTF-8 sources.** A PowerShell
    `Get-Content`/`Set-Content` round-trip double-encoded `rig.rs`'s
    non-ASCII lines (ANSI-decode → UTF-8-write); caught by a codepoint
    audit, reversed programmatically (Windows-1252-reverse + UTF-8
    decode, zero unmappable chars), verified line by line, plus 12
    pre-existing mojibake sequences repaired in `spike_ime_shell.rs`
    (comments only) and an all-sources sweep clean. Rule: edit tool or
    .NET explicit-UTF8 for writes, codepoint audits for verification —
    never trust terminal rendering of non-ASCII.
45. **Corpus non-ASCII is ASCII escapes in source.** Decomposed and ZWJ
    forms cannot be visually distinguished from their lookalikes, so
    they are written as `\u{...}` escapes with the codepoints stated
    in comments. Attempting literal glyphs produced ambiguous bytes
    twice before the escape rule was adopted.
46. **Partial closure of (b), per evidence.** Combining cluster parity
    + ZWJ single-cluster (geometry, hit-test, click/drag/shift-click
    selection) closed — both arms agree with zero mismatches.
    Bidi visual ordering re-deferred to M3 (measured divergence, the
    predicted shape). Word-segmentation incl. ZWJ-emoji + scalar
    combining-caret stepping tracked as shared-suite spec items (M2
    editing session). c3's FAIL is drift by construction proof and
    does not touch the (b) decision; its re-baseline is separately
    owed.
47. **Pre-existing mojibake in `spike_ime_shell.rs`, repaired this
    round (own entry, not part of 44/45).** Found by the same codepoint
    audit that caught the `rig.rs` incident: 12 double-encoded
    sequences in header/comment lines from an earlier round's tooling
    (4 right-double-quote, 7 right-single-quote, 1 section-sign —
    UTF-8 bytes E2 80 9D / E2 80 99 / C2 A7 read as Latin-1 pairs),
    comments only, zero functional impact. Fixed here rather than
    deferred because the audit was already open, the repair is
    comment-only with zero behavior change, and leaving known
    corruption beside active work is negligent — verified by rescan
    (remaining non-ASCII is the legitimate set) plus a green build.
    In-scope-adjacent (encoding repair; reconciler, layout, renderer,
    and all semantics untouched) — explicitly not scope creep, and
    distinct from 44's forward rule and 45's corpus-escape rule.

### M2 decisions (this round)

48. **Per-instance *state* scoping is M2; per-instance *scheduling* is M5.**
    `Ctx::child` expands children inline with child instance scopes, so
    invalidation is the parent effect's while state keying is the child's.
    Diff minimality (selection touches only changed rows) is proven; child
    effects surviving parent runs is scheduler-integration scope, stated in
    `component.rs` module docs rather than smuggled in.
49. **`suppress_transitions` is whole-commit data.** The scheduler flag is
    per-commit by construction — exactly lock #22's accepted v1 limit
    ("suppression is per-commit, not per-cause"), not a weaker reading.
50. **"Sufficiently far" defaults to past 64 distinct touches and is
    overridable per runtime.** `KeyedStore::DEFAULT_CAPACITY` 64 is ~5× the
    13-slot §4 window (ordinary overscroll never evicts mid-gesture, deep
    scrolls do) — a reasoned default, proven configurable
    (`set_keyed_capacity`, with a capacity-8 test driving eviction at 9).
    It is *not* tied to `n_slots`: the store is global per runtime, so one
    list's geometry cannot govern it without per-list namespacing —
    answered in M8 by explicit per-host sizing instead (decision 126).
51. **Eviction retires the slot.** Post-evict access through an old handle
    fails loudly per #11 (never aliases new state); re-access by key
    re-seeds to init.
52. **`Store` tracking is coarse in M2** (one version signal; memo equality
    gates dedup downstream). Per-key granular subscriptions stay future
    past M8 (stated, not built — the M8 sweep proves the coarse version
    carries full-list virtualization).
53. **Handler identity = (NodeId, kind), one per kind per node.** Re-runs
    rebind closures under retained ids (zero steady-state churn); only a
    changed kind-set is an update. Multi-handler-per-kind is M5 scope.
54. **`#[component]` is pass-through in M2; manifest scan is M2b.** `Props`
    requires `Clone + 'static` (opaque clone-out per run) — enforced at the
    derive site.
55. **`Style::new()` returns the builder; `Column::new()` returns the
    element builder** (explicit `new_ret_no_self` allows — the §4 surface
    reads verbatim; a second naming scheme would diverge the examples).
56. **Roots are single Elements** (fragments compose child lists only);
    holes hold no retained slots; incompatible pairs replace (Remove+Add,
    never silent coercion).
57. **Style changes dirty LAYOUT only for the layout-affecting subset**
    (`w/h/x/absolute_y/fill_width/pad_x/gap/content_size`); handler-only
    changes ride PAINT as the commit carrier.
58. **`ctx.child` takes the component symbol** for hot-reload identity:
    child instances record `(symbol, key, parent)`, asserted by the
    slot-stability test.

59. **Component-level divergence is lock #19, not a #26 departure — no new
    lock owed.** The release defer-then-park is #19's own prescribed text
    ("debug-assert with cycle path; rate-limited release log + deferred
    dirt … never a silent livelock"); component re-runs are effects and
    settle through the untouched M0 `settle()` (single `budget_violation`
    call site; zero profile branches in any M2 file). Against #26
    (memo-writes panic in *both* profiles): same never-silent principle,
    different problem — #26's alternative was *silence* (a dropped write
    corrupting state invisibly), while #19's release path is loud-log +
    deferred dirt + parked-with-reason. Mechanism differs, principle holds.

### M2b decisions (this round)

60. **`keyed_state` survives swaps — never drained.** The M2
    `OpaqueProps` doc comment anticipated calling `drain_keyed` at
    reload; the residence rule (§9.6, lock #25) wins instead: keyed
    entries hold core-side `Signal` handles (core vtables only — no hot
    code inside), so survival is sound and draining would destroy live
    virtualization/itembound state for nothing. `drain_keyed` remains
    the capacity/eviction helper, not a reload step.
61. **Retire, don't unload (M2b).** The real-dylib test segfaulted in
    `probe.set` because signal slot values (and memo values, keyed
    handles, memo closures, handler entries) can carry vtables from the
    retired image — unloading turns the next drop/re-run into
    use-after-unload. M2b retires images (bounded leak ≈100s of KB per
    swap, counted in every `ReloadReport`); true unload needs
    shared-core linking (all core vtables in one never-unloaded image)
    plus generation-tagged transient drains, and is tracked M9
    product-loop work — explicitly NOT attempted here.
62. **Runs resolve code by symbol, every run.** Mount-baked render
    pointers go stale at the first swap (caught by the reseed test:
    probe 48 instead of 141 — and real swaps would execute unloaded
    pages). `run_instance` consults the harness table first, mount
    fallback second; the table re-points before any post-swap run.
63. **Cross-image `TypeId`s are unequal — names + layouts rule.**
    Identical types in different images have different `TypeId`s
    (measured: `stored hot_fixture::EmptyProps but read as
    hot_fixture::EmptyProps`). Adopt/mount paths use type-NAME checks
    plus same-toolchain layout equality; `OpaqueProps::try_get` and
    null-returning drain glue replace every cross-boundary panic
    (panics unwinding through dylib frames abort on Windows).
64. **Run stacks live in shared state, not TLS.** Per-image TLS splits
    tracking (dylib-executed reads see an empty stack). Plain field in
    `RuntimeState` — no key, no `CURRENT`, no inner `RefCell` (accesses
    never cross user code). `untrack` keeps its legacy TLS stack,
    consulted alongside (single-image behavior bit-identical; hot
    `untrack` suppresses same-image reads; host-callback reads inside
    hot `untrack` blocks still track — documented limitation).
65. **Lint spares the locked §4.2 combinator pattern.** The first lint
    draft flagged `ctx.binding` inside `.map(|slot| ...)` — the locked
    example itself. Analysis: one base key per closure line with
    ordinal disambiguation is sound for prefix-stable iteration
    (internal edits re-seed by line change); only nested-`fn` items and
    conditional/loop bodies (run-varying counts) are flagged.
    Dynamic-bound combinators stay accepted-and-documented, not linted.
66. **Revisit revival + key accumulation (fuzzer findings).** Returning
    to previously-seen code revives its old site keys (keyed-storage
    consequence of §5.1 — the fuzzer models per-manifest values).
    Corollary: instance maps accumulate dead site keys across many
    swaps (bounded, tiny entries) — accepted for v1 iteration
    sessions; pruning is M9 product-loop scope.
67. **Task capture discipline is compiler-enforced `Send`.**
     `ctx.spawn`/`spawn_task` bodies cannot capture signals (`!Send`
     does not compile) — the §9.6 discipline needs no lint. The
     authoring pattern is `keyed_state` mailboxes (proven by the
     exactly-once fuzz accounting). Residual, stated: a task RUNNING
     across a swap executes retired-image code to completion, but its
     submits discard by tag — it cannot touch post-swap state.

### M3 decisions (this round)

68. **Text-config defaults live in `LayoutTextConfig`, not in `Style`.**
     No locked spec names the default text family/sizes: Segoe UI,
     title_small 16 / body_secondary 14 / default 14 (CSS px), DPR 1,
     ellipsis off. The corpus rig uses Segoe UI 16; the title default
     matches it. Bare `VNode::Text` inherits the ambient size.
69. **Run gating is LAYOUT-flag + measure-cache-miss (revised
     mid-round).** First draft gated on LAYOUT|TEXT|STRUCTURE — but
     TEXT/STRUCTURE persist for their future M4 consumers, so the run
     would never skip. Final: LAYOUT (consumed after the pass) plus a
     light cache-validity walk (fresh nodes, changed bytes, changed
     resolved sizes — TEXT-only changes surface here). Flag dirt alone
     never re-shapes; style-only commits skip the run (`empty` stats).
70. **Absolute positioning is per-axis.** `.x` overrides horizontal
     placement, `.absolute_y` vertical; either axis flows normally when
     absent (the §4.1 knob: x=23, y in flow). `absolute_y` children
     leave auto height and the vertical cursor (virtualized slots must
     not grow content — `content_size` floors it instead); x-only
     children still count vertically; Row x-children leave the width
     sum. Nothing here was specified; all three rules are stated.
71. **Container semantics split Div from Column.** Div (block-lite):
     every child full content width. Column (flex): intrinsic unless
     `fill_width`. Row: fixed children intrinsic, fills split the
     remainder equally (needs the width first — the only two-pass
     flow). Stack: overlay at the content origin, max-extent sizing.
     ScrollArea: explicit viewport (fallback: children extent),
     Column-like flow for normal children, pinned `absolute_y` slots,
     `content_size` floors the scroll extent.
72. **`Custom` lays out as block-lite; childless `Image` without
     explicit size is zero.** The escape hatch has no layout contract;
     stacking is the least surprising default, stated.
73. **Ellipsis is a default-off config flag (optional-v1 as specified).**
     When on with a finite width: single-line truncate of the logical
     tail + "…" marker (one amortized shape per style). Wrap is the v1
     default; truncation is opt-in — no style field exists to say
     otherwise.
74. **Default viewport 800×600 CSS px; the root fills it unless
     explicitly sized.** Scaled by the config DPR inside the engine.
     Backends/a11y read the root layer as the viewport.
75. **Commit dirt implies frame demand.** `reconcile_root` requests a
     frame on non-empty diffs — component effects run synchronously at
     mount (outside any frame), so without this LAYOUT would never see
     the first commit. The on-demand loop still idles on empty commits.
76. **BiDi is UBA-lite levels + forward-affinity carets (both
     oracle-driven).** Levels: LTR runs 0, RTL runs 1, ASCII-digit
     (EN) runs inside RTL runs 2 (UBA I2 — DirectWrite resolves the
     corpus tail as one RTL run; mirroring it whole renders "321",
     the oracle proved the island). Neutrals ride their run's level
     (the N1 grouping the backend already encodes). Carets: LTR left
     edge / RTL right edge (forward affinity — the boundary duality is
     real: one logical position, two visual spots); line ends follow
     the first/last *logical* cluster (the oracle puts an RTL-final
     trailing caret mid-line: 55.195, not 94.320). Limits, stated:
     Arabic-Indic digits stay level 1, neutrals beyond N1-grouping
     unhandled, LTR paragraph base only, intra-cluster glyph storage
     assumed logical (true 1:1 for the covered scripts).
77. **Boxes live inline on `RetainedNode` (the DESIGN §2.2 sketch).**
     Reconciler-side residence (lock #25 by construction — swaps reuse
     slot-keyed nodes with boxes intact; replacements re-measure).
     Removals retiring boxes raise `boxes_dropped` so the ledger
     publishes. Companion mapping fix: `text_hint` changes dirty
     LAYOUT (hint resizes measurement; PAINT-only was wrong).
78. **DPR: positions snap, extents stay subpixel.** Style px are CSS
     px, scaled by the config DPR at the engine entry; only committed
     x/y pass `round_to_device_px` (coordinate-system spec).
79. **Wrap rules.** Greedy at cluster boundaries over cached advances;
     an over-wide single cluster stands alone (never split); `\n`
     hard-breaks (the newline cluster is dropped, zero width); line
     height is ascent+descent with `line_gap` between lines (the box
     excludes the trailing gap).
80. **Measure failures.** Empty text / no installed service → zero
     box, never an error (headless M2 frames stay green serviceless).
     `FontNotFound`/backend failures panic loudly (config/backend
     bugs). A failing "…" shape degrades to no marker (decoration
     must not crash layout).
81. **Wasm metric drift stays open.** The DirectWrite side is
     characterized (device-px subpixel advances, deterministic
     re-shape); the wasm slice is unmeasured here and feeds the §8.8
     rounding rules from the web work, not this round.
82. **No new locks.** Lock #6 is fulfilled by the build, #29's bidi
     item closed by oracle measurement, #25 holds by construction
     (ledger + boxes core-side; post-swap re-runs re-measure through
     the same dirty path — the M2b suite passing under the installed
     LAYOUT hook is the evidence).

### M4 decisions

83. **Contract types live core-side (`oppa::render`); the builder and
     the rasterizer live presenter-side (`oppa-cpu`).** The scene
     schema is owned per ADR-0001; the §3 tripwire ("presenter
     secretly needs core internals") is answered structurally — the
     backend crosses only `retained_ids` / `take_paint_masks` /
     `diffs_from` / `get` / `Interner::get` / committed boxes. No new
     locks (locked #5 proven implementable, not amended).
84. **`Color` is opaque sRGB `0xRRGGBB`; translucency is the separate
     `opacity` field.** v1 has no alpha channel; `bg ==
     TRANSPARENT` emits no fill op. (Black ink `0x000000` vs
     TRANSPARENT share a value — the check applies to `bg` only;
     recorded, not resolved.)
85. **Fixed `INK` for text; no border rendering.** `Style` has no
     text-ink field (so `DrawOp::Text` carries fixed near-black
     `INK`) and no border-width field (BUILD-ORDER §3 names a border
     the typed style cannot express) — both are OPEN QUESTIONS for
     M5+, not silently invented fields. Shadow blur degrades to an
     offset solid (`Caps::blur_backdrop=false`, M8 evaluator scope).
86. **Opacity bakes into ops; `PushLayer` is honored but never
     emitted by this builder.** Keeps one alpha mechanism per plan;
     the stack exists for GPU-future plans (backend-tested
     directly). `PushClip` wraps ScrollArea viewports; clip/layer
     pops are LIFO-merged with layers-first (exact for
     builder-emitted plans — stated approximation).
87. **Backend replays its retained op list per paint; damage is the
     rebuilt-set record.** No damage blits in M4 (correctness =
     the builder rebuilt exactly the dirty set, proven by the
     oracle's 0-pixel diffs). Empty plans skip the surface — that,
     not partial raster, is the static-≈0-CPU mechanism.
88. **F2: fresh text leaves get STRUCTURE|LAYOUT|PAINT.** The
     reconciler's own Add mapping covers all fresh nodes; the Text
     arm's omission was unobservable until the first paint consumer.
     Fix, not drive-by (module docs already promised it).
89. **F1: LAYOUT-consumed positions do not rebuild plans.**
     Position-only moves with no paint flag are a documented
     limitation; engine-side PAINT stamping on moved boxes is M5+
     work. Nothing in M4 scope hits it.
90. **RImg fails loudly until async decode exists.** No placeholder
     pixels, no silent skip — and validation precedes all raster so
     a refusal leaves the surface pristine. Image pixels stay M8+.
91. **Hot-reload interplay: presenter state by NodeId, plans rebuilt
     through masks.** Renderer state (surfaces, retained ops,
     registry) is presenter-side keyed by `NodeId`; boxes stay
     core-side (lock #25). Post-swap re-runs re-measure/re-layout
     through the dirty-mask path and the backend replays the rebuilt
     dirty subtrees — no full rebuild, no new locks expected.
     (M9's reload-during-paint timing stays adversarial scope.)
92. **No new locks.** #5 proven, #6/#29 untouched, #25 holds by the
     split above; #3 exercised only as payload flow (emitters M10).

### M5 decisions (this round)

93. **`InputEvent` is the full payload-carrying enum; M0 `Event`
    stays underneath.** Decision 27's deferred enum is built in
    `crate::input` (not a workaround, not a parallel path): the
    router hit-tests to `NodeId`, resolves owners, writes flags,
    and dispatches through the existing registry. Both seams stay
    green (M2 direct-dispatch tests untouched).
94. **Hit-test rule: deepest wins, later-sibling ties,
    inclusive-exclusive containment, unboxed skipped, misses are
    `None`.** Adjacent siblings never both claim a shared edge;
    returning the root for an in-viewport point is correct
    containment, not a fallback — the rule bans only returning
    nodes that do not contain the point.
95. **Capture model: dispatch iff the up-hit is in the capture
    subtree; Cancel always clears with no dispatch;
    release-outside is a silent no-op; focus follows click.**
    Clicking a miss or a handler-less subtree changes nothing
    (normal UI, not a wiring bug — documented, not loud).
96. **Focusable = Press-handler nodes in DFS pre-order; Tab/
    Shift+Tab wrap; Enter/Space pulse + dispatch (repeats
    ignored); Escape blurs; other keys go to the focused Key
    handler or are quiet no-ops.** The quiet-keys call is
    deliberate: keys are ambient (most UIs ignore most keys),
    while handler misses are wiring bugs — different loudness,
    stated. Explicit `Focus` targets must be live press owners
    (loud refusal otherwise).
97. **Handler→instance attribution is render-time per-instance
    (M8 finding F6 reworks the M5 rule).** Attachments stamp the
    creating instance at render time (`Ctx::child` innermost-wins +
    `run_instance`); the reconciler prefers the stamp
    (`register_handler_owned_as`; unstamped hand-built VNodes keep the
    running-owner fallback). Inline-child per-slot flag attribution
    (virtualized rows) is defined slot-scoped and proven (decision
    123) — the M5 "root-run granularity" statement stands only for
    the fallback path now.
98. **Border + ink are stated Style-struct fields (lock #8 grows
    fields, interning intact); alpha stays open (M6); F1 stays
    open (M6).** Border is paint-only and inset (excluded from
    `style_layout_bits` by statement); the ring reuses shape ops
    (inner radius shrinks by ring width, floor 0); ink inherits
    self→ancestors→`INK`. The Toggle's knob moves carry PAINT, so
    F1 is unhit — re-recorded, not resolved.
99. **Win32→`InputEvent` mapping is platform-track follow-up
    (M6).** `ShellEvent`/`Cmd` already carry the payload shapes;
    this round's real-input vehicle is the inject path (real
    payloads through framework primitives, not signal writes).
    Touching the shell would risk the spike host for no M5 proof.
100. **`Scroll`/`Ime` variants are carried and kind-routed, with no
    new machinery.** They dispatch to the target's kind handler
    or fail loudly; scroll physics is plain signal writes either
    side of the seam (M8 decision 128 — no physics engine built)
    and editing sessions are M2 scope.
101. **Measured: cancel clean, input→target 1 frame + tail (M8
    reframe, decision 122).** Cancel: 0 dispatches, state held.
    Input settles dispatch + semantics + capture/focus in the commit
    frame (`run_once` — sharper than the old `frames == 1`); the
    Toggle's 120 ms track-bg interpolation tail follows. The old
    "input→visual 1 frame" reading predates the evaluator (transitions
    were data-only in M5); the #7 substance is unchanged.
102. **No new locks.** #7 proven, #3 exercised interactive, #25
    holds (input state core-side); #5/#6/#29 untouched.

### M6 decisions (this round)

103. **`Color` stays opaque `0xRRGGBB` + separate `opacity` (M5's
    open alpha, resolved with no representation change).** The GPU
    blend path folds per-op opacity into brush alpha and
    `PushLayer{opacity}` into a viewport-clipped scene opacity layer
    (isolated group, `SrcOver`) — the layer opacity lives
    scene-side only, never double-multiplied into brush alpha (the
    round's encoder fix: 0.8 read as 0.64 before it). Proven by the
    cross-backend alpha pixel proof (half-alpha `CARD_BG` over white
    ≈162 gray ±2 on both rasterizers, tol-2 diff 0).
104. **F1 closed: engine-side PAINT stamping on moved boxes.**
    `LayoutCtx::commit_box` (plus the root-resize path) stamps
    `PAINT` when a box changes while carrying none of
    `STRUCTURE|STYLE|PAINT|TEXT` (`FRAME_DIRT`), counted in
    `LayoutStats::paint_stamped`. The builder rebuilds stamped
    subtrees instead of replaying stale pixels. Proven by the keyed
    removal test (survivor shifts up: stamped 1, rebuilt 1 op +
    2 damage, stamped history == full repaint 0 px). M4's
    text-subtree damage expectation moves 1→2 with it (the wrapper's
    content extent now rebuilds — the old 1 encoded the F1
    limitation, not the contract).
105. **`DrawOp::Text` gains `baseline` (lock touch on the contract
    types).** Finding F3: the op carried no baseline (and still
    carries no em size / per-run font identity), so the GPU backend
    had no principled vertical placement (baseline-at-line-top put
    Segoe UI almost entirely off-surface: fringe 0). The builder now
    carries `line.baseline` through; the CPU backend ignores the
    field (cells unchanged — M4/M5 suites green via `..` arms); the
    Vello encoder places the glyph-run origin at `y + baseline`
    with `font_size = line_height` (stated scale approximation;
    exact em size + multi-font atlases are M7 text-polish scope).
    Single-face v1 bound stands (one injected face per surface;
    no-font Text fails loudly, never tofu).
106. **Vello `Caps`: 64 layers, no blur/backdrop, MSAA always,
    `text_as_paths` true.** Blur degradation is contractual (the
    `Shadow` op carries no blur radius — both backends paint the
    same offset solid; upstream `draw_blurred_rounded_rect` needs a
    contract blur field first, M8 scope). AA is always on (no
    toggle); text is real outlines via `draw_glyphs` (closes the M4
    `text_as_paths=false` gap the review starts from).
107. **Cross-backend oracle tolerance policy (stated, not assumed).**
    Axis-aligned fills (rect/shadow/clip/layer) agree pixel-exact
    (strict plan 0/0 over 7200). Curves (RRect/Circle/ring) agree
    within tol-16 ≤60 (measured 12: tiny-skia bands+discs with
    per-piece AA vs Vello's single analytic path — corner/edge ramp
    shape, not interiors). Text agrees in position (ink columns
    coldiff 0) while differing in shape by design (outlines vs
    cells, exact diff 1047 nonzero — else the review is vacuous).
    Glyph review floor beaten (86 outline-AA fringe pixels vs the
    M4 cell-fringe floor).
108. **Tripwire verdict: PASS on evidence (no Skia hatch).** Matrix:
    primary NVIDIA GeForce RTX 3060 Ti (Vulkan) + weakest-available
    Microsoft Basic Render Driver (Dx12) — both probed and recorded,
    pixel oracles run on hardware (loud requirement, never a silent
    software fallback; parallel GPU tests serialize on a global lock
    — concurrent Vulkan/DX12 device use hangs the driver).
    `SkiaBackend` stays costed (2–4 wk), unbuilt: no hard wall was
    evidenced. GLES 3.1-class weakest-hardware row stays open
    (this box has no GLES adapter — named gap, M10 Android-device
    row owns it, not a silent pass).
109. **Findings fixed inside the round (not drive-bys).** F4:
    `DWriteTextService::face_file_reference` never resolved a file
    (single-call `GetFiles` with 0 capacity reads E_INVALIDARG; the
    "key IS the path" reading yields garbage) — the spike's Vello
    debug renderer silently skipped every text run on the `None`
    path. Fixed (two-step `GetFiles` + `IDWriteLocalFontFileLoader`
    path resolution; Segoe UI → `C:\WINDOWS\FONTS\SEGOEUI.TTF`) with
    the M6 atlas as the first real consumer. `ComponentHost::
    with_clock` added (additive, tests-only seam for deterministic
    vsync-cadence proofs).

### M7 decisions (this round)

110. **`DrawOp::Text` gains `em_size` + `fonts` (second contract
    lock touch — decision 105's stated remainder, not a quiet
    widening).** Finding F3 closed in full. `em_size` is exact
    (`font_size_px × dpr` at measure time — ends the `font_size =
    line_height` approximation); `FontRun{glyph_range, family,
    font_id}` carries per-run identity (ends the single-face
    bound). `LaidRun` keeps the shaper id + a resolved family
    (engine table from the service enumeration; requested-family
    fallback for unmapped ids — e.g. headless fakes — stated, not
    silent);     `LaidLine.em_size` fills post-pass so `layout_text`
    stays pure and signature-stable (all call sites + cookbook
    untouched). The builder merges consecutive same-font runs
    (fallback boundaries, not cluster boundaries, ride the op).
    CPU ignores both fields (cells unchanged — M4/M5 green);
    Vello draws one run per `FontRun` at `font_size = em_size`
    with explicit→default→loud face selection; DOM emits
    per-run `<span>`s. Proven on all three arms (plan test +
    real-face encode + DOM spans).
111. **StyleId→CSS rule identity + the inline split +
    `Caps::dom`.** Classes are `s{bits}` (interner ids stable
    across commits, so names never churn); a new style adds
    exactly one rule, the rest untouched. Static style never
    appears inline (asserted — geometry, measured fonts, values,
    and scroll offsets ride inline because they are per-node
    data, not style). Inset border stays off the `border`
    property (inset `box-shadow` ring — a `border` would move
    layout and split-brain the engine); shadow degrades to the
    same blur-less offset solid as both rasterizers. `Caps::dom`
    = 1024 layers (stacking contexts are cheap — "no practical
    limit" as a bounded testable number), no blur/backdrop,
    MSAA on, text as paths.
112. **§9.3 INPUT feed (`bind_scroll`) + overscan + feed
    semantics.** The browser scroll event maps into the
    framework-owned offset signal at the INPUT boundary (same
    gate as every platform event — phase ordering by
    construction); the headless feed is `bind_scroll` (the DOM
    shell performs it from browser events; GPU TIME physics
    writes the same signals directly). Deltas accumulate
    (wheel-shaped); `dx` ignored (v1 vertical-only, stated);
    unbound targets keep M5 dispatch-only behavior; the target
    still needs its `Scroll` handler (M5's loud-miss rule
    stands — `on_scroll`/`on_ime` close the authoring gap).
    `OVERSCAN_SLOTS = 4` + the pure `scroll_window` helper
    (M8 consumes it; window lag, not tearing, is the stated
    consequence of the ≤1-frame trail).
113. **One foreign-element mechanism, two callers.** `Tag::Custom`
    (external hole, `data-external` marker, box preserved,
    framework children pass through — v1 hole is a marked box,
    not a true void element, stated) and verdict-(b) fields
    (`TextField` semantics → real `<input>`, presenter-owned
    editing per #27) share the presenter-recognized-special-case
    path. No new `Tag` (locked #24): `vnode::TextField` is the
    `Text`-struct shape (hint + value child) with the behavior
    flag; layout/measure/reconciler treat it as text and only
    the presenter branches. Field children absorb (inputs are
    void; the value carries their text) — absorbed ids stay
    keyed but unmaterialized. `vnode::Custom` exposes the
    documented #17 escape hatch (previously unauthorable).
114. **ARIA mapping (total table) + `SemanticsDiff` parity.**
    Switch→`role=switch`, ListItem→`role=listitem`,
    TextField→no role (implicit textbox) + native `disabled`
    when set; checked/selected/label map except state on
    textfields; `disabled=false` and absent optionals emit
    nothing. Parity = same source, two readers (diff upsert ==
    element attrs + bounds — asserted, incl. payload removal
    without node removal).
115. **Ancestor-relative offsets + static text wrappers
    (parity-found, measured — not assumed).** The corpus caught
    two backend bugs, both fixed in the round: nested absolute
    elements double-counted ancestors (col children +64 — the
    row-at-origin cases passed by coincidence), and transparent
    text wrappers collapsed to zero-width containing blocks
    (wrapping their own payload child). Offsets now subtract
    the positioned-ancestor box (slots resolve against their
    ScrollArea — the spacer sits at the container origin);
    hint-only wrappers render static (children resolve against
    the real container). Boxes went 7/10 → 10/10 on this fix.
116. **Parity verdict: green in the flat subset, stated as
    exactly that.** 10/10 gated rows (boxes ±0.5, text widths
    ±1.0, Edge 153, dpr 1): no wrap/scroll/absolute cases ran —
    general parity is NOT claimed. The one record-only row
    (text height 21.28 vs 21) is listed, never hidden, never
    gating. Three-backend box compare: CPU DrawOp rects ==
    DOM serialized geometry exactly; Vello commits the same
    tree (live counts equal); the M6 two-backend assert joins
    its third row.
117. **Render-boundary escaping (finding F5, found and fixed in
    the round).** A `font-family:"Segoe UI"` quote inside
    `style="..."` ends the attribute early, silently dropping
    font size + `white-space` (measured: text wrapped to
    37×36). All style-attribute content now escapes at the
    single render boundary; single-token families skip quotes
    (less noise); inputs carry the value descendant's measured
    family + size (caret/IME anchoring agrees with the engine).
118. **Editing-suite verdict (verdict-(b) path, shared ops).**
    latin_edit 10/10 through the real `<input>` (incl. the
    browser restoring pre-undo selection [3,8) — M1's finding
    reproduced through backend-generated HTML); multibyte
    dblclick [0,3) (the adopted CJK dictionary rule
    confirmed); undo_granularity recorded (CDP `insertText`
    calls are separate undo units — one Ctrl+Z yields "aHello
    world"; rig-vs-real note in M1's CDP-quirk family, not a
    framework bug). The op sequence mirrors `rig::op_suites`
    (pointer-noted; the rig stays the single source of truth).

### M8 decisions (this round)

119. **Overscan: one constant stands (the M7 split, resolved).**
    `OVERSCAN_SLOTS = 4` on GPU and Web — no fork. Measured:
    +2 (K=16) and +4 (K=20) both sweep clean (0 structure,
    oracle 0, flashes 0, created 0); +4 costs +12 ops/tick
    (+25%) for double the lag cover (spec ±4 vs ±2
    rows/frame — ±4 rows at 60 Hz ≈ 13k px/s, ample). Uniformity
    beats a quiet fork; the cost is counted. DESIGN §4.2's +2
    sketch is superseded-as-a-number by this measurement (the
    archive stays verbatim).
120. **v1 animatable set + curve interpretation.** `bg` +
    `opacity` only (the CSS-expressible set, §9.1 — the DOM
    mapping holds exactly because the evaluator never
    interpolates anything CSS cannot express); everything else
    jumps by lock. Easings are closed-form cubics approximating
    the CSS names (Linear/In `t³`/Out `1-(1-t)³`/InOut split
    cubic) — deterministic, monotonic, exact at both ends; the
    curve SHAPE is platform tuning, not contract (monotonicity
    + exact settle are). sRGB channel lerp (gamma-correct is
    v2, stated); transparent endpoints snap (decision 103);
    zero-duration snaps, never interpolates.
121. **DOM CSS mapping (per carried animatable).** `transition:`
    lists `background-color` iff `bg` is present, `opacity` iff
    `opacity` is present; a bare `transition` (neither) declares
    nothing (the evaluator has no target on either backend).
    Stamped frames flag exactly the touched set inline
    (`transition:none`); untouched elements have no new value
    to transition toward (finer than the v1 per-commit limit,
    consistent with it).
122. **Production hooks paint evaluated plans (uniform rule) +
    M5 frame counts reframed.** All three hooks build evaluated
    plans at the frame clock (off-interpolation identity — same
    walk, same masks). The Toggle's 120 ms transition makes the
    loop run commit-frame + tail, so four M5 assertions now read
    commit-frame via `run_once` (dispatch + semantics +
    capture/focus, same frame — sharper than the old
    `frames == 1`) + settled tail after. #7 untouched; this is a
    measurement touch with the rationale stated, not a quiet
    contract change.
123. **Per-slot flag attribution (defined, M6-carried open
    closed).** Hover/press/focus are slot-scoped (stable position
    identity): rows render the CURRENT item through the slot's
    flags; flags never follow an item to another slot; press
    handlers read the slot's current binding at dispatch
    (handlers capture signals, not data — §4.2). Matches the DOM
    (the same element keeps :hover/focus across a rebind).
124. **Finding F6 (fixed in-round): inline-child handler
    ownership.** Drains happen in the root effect (running owner
    always the root), so every inline child's handler attributed
    to the root and per-row flags never fired (M5 proved flags on
    roots only). `HandlerAttachment.owner` stamped at render time
    (`Ctx::child` innermost-wins + `run_instance`); the
    reconciler prefers the stamp (`register_handler_owned_as`;
    unstamped hand-built VNodes keep the M5 fallback).
    `Ctx::child` nests the `input_owner` guard with restore.
125. **TIME drive owned by the host.** The creating commit
    registers the settle animation once (flag-held — TIME runs
    before EFFECTS, so upfront registration drops before the
    first interpolation exists); the animation re-dirties live
    nodes PAINT-dirty per frame (`mark_paint_dirty`) and drops
    at settle. Static UI idles; interpolation progress re-enters
    the damage discipline (without this, tails freeze on the
    first interpolated frame — found by M5 pixels, fixed
    in-round).
126. **Keyed capacity per-list sizing (M2 decision 50's
    deferral, answered).** One global LRU (default 64 stands for
    unconfigured hosts — the bound is proven separately) +
    explicit per-host sizing (`set_keyed_capacity`) for lists
    whose identity set exceeds it. No per-list namespaces in v1
    (revisit iff measurement shows cross-list pressure).
127. **Window-tracking slots + straddle margin.** Slot identity
    (keys) stays fixed while positions follow the overscanned
    window (`window_first`); moves are LAYOUT-only Updates,
    structure stays 0. K = ceil(vp/row) + 1 straddle + 2·over —
    the +1 caught by the as-built lag loop (fixed-K without it
    undercovers by one row at straddling offsets near max
    velocity). Non-moving ticks (leading-overscan absorption,
    sub-row offsets) commit at most empty diffs — stated, with
    the memo-gate mechanism named.
128. **TIME-physics = plain signal writes (no new machinery).**
    GPU scroll physics writes the same `ScrollOffset` signals
    TIME-phase; Web maps browser events INPUT-phase (the M7
    seam); tests drive one arm by direct writes, the other by
    injected `Scroll` events, and assert identical windows.
    Fling curves are platform tuning, not M8 proof (stated).

### M9 decisions (this round)

129. **M2b was a working path (verified, not assumed).** Manifest
    scan, real dylib swap, typed drain/adopt, retire-not-unload,
    generation-tagged executor, per-run symbol resolution,
    shared run stacks — all in `crates/` with tests green before
    this round. M9's scope is the product-loop timing on top;
    true unload stays deferred (decision 61 stands).
130. **Fuzz transition duration 20 ms, stated.** Wall-settle bound
    for the SystemClock hosts; §9.4 stamp semantics are
    duration-free (scroll rows keep the showcase 120 ms —
    stamped commits never arm the animation).
131. **SystemClock for all fuzz hosts, stated.** A frozen
    MockClock + live interpolation spins `run_until_idle`
    unboundedly (the M8 pump note); wall-settling terminates.
132. **IME writer clamps to char floors, stated.** Byte-oriented
    test simplicity; cluster-stepping stays spike-session
    behavior, never fuzz-modeled.
133. **Swap-path mix with pre-drained phase logs, stated.**
    Direct (`reload_to`) and hook (`request_swap` + frames) both
    counted per scenario; ordering checks drain the log first so
    they read the swap frame, never stale history.
134. **Gate scope follows the #29 partial-closure precedent.**
    Renderer freeze is declared on core-mechanism evidence; the
    TSF machinery residual (no generational surface —
    grep-verified over `oppa-shell-win`: no
    `Signal`/`GenArena`/`NodeId`/`Runtime`) stays platform-track,
    not a renderer gate. Per-iteration fuzz runs `StaticSource`;
    the dylib path is proven per run by `real_dylib.rs` (same
    `reload_to`).

### M10 decisions (this round)

135. **AT-SPI is Linux-only here.** BUILD-ORDER M10 + linux
    overview name AT-SPI over D-Bus; Android's accessibility
    service is a different API (`AccessibilityNodeInfo` via
    Java) and is explicitly out — stated, not assumed. Windows
    UIA likewise still open.
136. **No NDK/JNI in the shell crate.** Classification,
    lifecycle, and restart live headless-testably in std + oppa;
    the Activity glue will be a thin forwarder (needs NDK +
    device — platform-track follow-up, same class as the
    wgpu-Android surface and the text slice).
137. **BACK dismisses to ESCAPE at the classifier.** Both mean
    dismiss; the mapping is classification like Win32's VK map,
    not new semantics. Unknown keycodes pass through for the
    router's quiet rule (decision 96) — no shell-side fork.
138. **Multi-touch refusal is logged, not panicked.** A second
    finger must not crash the app; the `ShellError` drain is the
    loud channel (same principle as rate-limited release logs —
    loud without crashing the loop).
139. **GL-stand-in scope, stated both directions.** Desktop GL
    proves backend-path conformance at the M6 pixel standard;
    it proves nothing about weak-mobile-GPU frame cost (that
    half stays device-owned in `08-performance/mobile.md`).
140. **GL test lock is test-only.** The file-static mutex
    serializes this binary's GPU-touching tests (M6 `GPU_LOCK`
    family); production code is untouched. The one-retry in
    `ensure_gpu_gles`'s caller logs both errors — persistent
    failure still fails.
141. **No 03-spec entry for M10.** The milestone demands no new
    lock or freeze declaration; platform overviews + testing +
    perf docs carry the status. M9's freeze declaration is
    untouched (still out of scope to revisit).

### Gap-closure decisions (same day as M10)

142. **The GLES floor is 3.0, not "3.1-class".** Emulator-measured
    (`ANDROID_EMU_gles_max_version_3_0`, SwiftShader 4.0.0.1) —
    a doc-phrasing correction in living docs only; the
    12-archive wording stands as written history (rule 3), and
    no design changes (wgpu's GL requirement is 3.0, met).
143. **adb's daemon starts detached.** Synchronous `adb devices`
    with a dead daemon blocks the pipe (the hang); `start-server`
    via `Start-Process` first, then query. Emulator likewise
    launches detached. Recorded so the recipe is reusable.

### NDK-install decisions (same day as M10)

144. **NDK r29 side-by-side (`29.0.14206865`) + both Rust
    Android targets; `oppa-shell-android` + `oppa-atspi`
    cross-`check` green for `x86_64-linux-android`.** Linker is
    the NDK `x86_64-linux-android35-clang.cmd` wrapper (minSdk
    35 ≤ emulator API 36). No repo `.cargo/config.toml` added —
    the recipe lives here until APK assembly needs it permanent.
    Remaining step is app glue (Gradle + activity), not
    toolchain.

### On-device decisions (same day as M10)

145. **The on-device oracle is byte-equality, same standard.**
    Device GL pixels vs device CPU pixels (same scene, same
    run): exact 0 at 1080×2400 — no tolerance needed, none
    taken. Device CPU vs host CPU: exact 0 (cross-ISA
    determinism of the reconciler→builder→tiny-skia chain).
146. **Emulator GPU modes, stated:** SwiftShader (default/
    `swiftshader_indirect`) cannot serve Vello — GLES 3.0 has
    no compute, SwiftShader Vulkan caps UBOs at 16KB vs
    Vello's 64KB. Both walls are capability facts with exact
    wgpu limit strings, not perf judgments. `-gpu host` serves
    GLES 3.1 through the Android stack (integration proof,
    still not weak-hardware perf).

### v1-remainder decisions (this round)

152. **Surface contexts bind their own instance, always.**
    A headless adapter from another `wgpu::Instance` can report
    a surface "supported" while its device cannot see it
    (observed as a `Surface does not exist` panic in wgpu-core
    storage). `ensure_gpu_for_surface` (re)creates from the
    given instance every call; scenes survive (per-surface),
    only the device-side context is re-made.
153. **The display half rides the instance descriptor on
    Android.** GLES requires the platform display at instance
    creation when presenting; `NativeWindow` carries only the
    window half, so an owned unit `AndroidDisplay` fills the
    descriptor (rwh 0.6 has no owned handle; ndk's `rwh_06`
    impl needs the feature enabled explicitly).
154. **Vello presents via intermediate texture + blit.**
    vello 0.10 has no render-to-surface (only
    `render_to_texture`); the documented pattern is an
    `Rgba8Unorm` intermediate + `TextureBlitter` into the
    current surface texture. Surface format prefers non-sRGB
    8-bit (same choice as `vello::util`).
155. **arm64 is built, not run, here.** Both CD ABIs ship in
    the APK; the emulator is x86_64, so arm64 execution needs
    ARM hardware/a phone — stated next to the build proof.
156. **Face choice is weight/style-exact first.** The device's
    fuller font dir picked Naskh Bold where the host had only
    Regular (same glyph ids, different advances — the device
    comparison caught it). Requested family, then chain, each
    tiered exact-match before coverage-only; within-family
    weight fallback stays silent in v1 (single-face families).
157. **JNI array signatures carry the `[` prefix.**
    `Set.toArray` is `"()[Ljava/lang/Object;"` — the no-prefix
    form throws NoSuchMethodError on-device.
158. **Fullscreen theme for tappable top-edge content.** The
    status bar eats y<63 taps (first tap run missed the loop);
    `Theme.NoTitleBar.Fullscreen` fixes it with no scene change
    (SHAs preserved).
159. **The full EditingSession stays Windows-bound.**
    `spike-textedit` depends on Windows-only crates; the device
    runs the M1 event *shapes* through the core dispatch seam
    plus real IMM policy calls — the session itself is not
    ported (would be a framework refactor, out of scope).
160. **No sudo needed on Linux, by construction.** User-local
    rustup + runtime-`dlopen` windowing (winit/softbuffer) +
    pure-Rust shaper over the system font dir. Recursive font
    walk (distros nest families; flat dirs unaffected).
161. **First present waits for configure.** Committing before
    the Wayland configure ack is a protocol violation WSLg
    Weston punishes by dropping the client (observed twice);
    the redraw (post-ack) is the first legal commit point.
    Sustained high-count presents on WSLg Weston die ~8th
    present (named follow-up); the X11 path needs
    `libxkbcommon-x11` (absent, apt sudo-blocked).
162. **HWND association rules (learned from UIA validation):**
    LPARAM objids arrive zero-extended (mask low 32 bits);
    `UiaRootObjectId` (-25) is queried first and bound (answer
    both); fragment validation requires `Navigate(Parent)`
    success — the HWND parent link (decision 149 confirmed in
    code, test-local `HwndParent`); host tokens propagate
    across derived fragments (the subscribe-breaker).
163. **Event-capability interfaces are required, not optional.**
    UIA refuses subscriptions without
    `IRawElementProviderAdviseEvents` (`E_NOTIMPL` to the
    client) and needs a readable state property
    (`ToggleState` added) — both additive to the provider.
164. **Non-Element event scopes are rejected in this calling
    setup** (E_INVALIDARG on desktop too; the .NET client
    accepts Subtree, so environmental/calling-convention, not
    UIA). Worked around via `FindFirst` + Element (both proven
    primitives); raw child-walk topology stays out of scope.
165. **Ownerless hover writes skip** (decision 95's "changes
    nothing" + the Down path's own early return). Hovering or
    clicking a handler-less node panicked in
    `set_flag_for_node` — no real pointer stream survives that;
    the one genuine framework fix this round (M5 suite still
    green).
166. **wasm time comes from rAF, never a wall clock**
    (`Instant` is unavailable on `wasm32-unknown-unknown`);
    `MockClock` + `tick(now_ms)` drives TIME tails.
    Full-page HTML swap on change is the v1 binding;
    fine-grained patching is v2.
167. **Device pulls are binary-safe or they are lies.**
    `adb shell cat` + shell redirect mangles bytes (CRLF/UTF-16
    damage observed twice: 20 MB pulls, PNG screenshots).
    `adb exec-out` captured by python (`subprocess` bytes, plus
    `\r\n` normalization for text) is the only pull shape;
    on-device `cmp`/`sha256sum` needs no pull at all.
168. **Serial full-suite runs are contention-free.**
    Cross-binary GPU tests (m6 + m10 families) deadlock the EGL
    context lock under parallel cargo (known family, decisions
    108/140); `-j1` verifies clean. Not a code regression
    (isolation green).

### phone-round decisions (this round)

169. **View/Window bars calls MUST run on the UI thread.**
    Off-thread calls throw `CalledFromWrongThreadException`,
    which ART escalates to SIGABRT on the next JNI call —
    fatal, uncatchable from Rust, no `error.txt` (tombstone).
    NativeActivity Rust gets there through one app Java source
    (`OppaUi.hideBars` + `runOnUiThread`); Rust makes exactly
    one JNI call (the static).
170. **Attached-native-thread lookup sees only the system
    loader.** `find_class` misses app classes that ARE in the
    APK, and `NativeActivity.getClass().getClassLoader()` is
    boot (framework class). App classes load through a
    `DexClassLoader` over `ApplicationInfo.sourceDir` (public
    field — no loader needed for any step).
171. **Adreno GLES refuses Vello; Adreno Vulkan serves.**
    Surface device request fails
    (`max_storage_buffers_per_shader_stage` 8 > 4) — third GPU
    wall, Adreno-specific, same chip that serves exact-0 over
    Vulkan. GLES-first-then-Vulkan ordering is the standing
    attempt order (GL serves the emulator, Vulkan the phone).
172. **Hidden bars do not re-lay-out the native window.**
    The window stuck at 1080x2290 under hidden bars; explicit
    display-size layout params (from `getRealMetrics`) force
    1080x2400. The window may sit offset (+55 display) after
    bars hide — taps target display = window + offset (the
    MotionEvent values already are window coords; the offset
    is read off the first tap, never assumed).
173. **ColorOS shell restrictions (all observed, none assumed):**
    `pm clear` needs `CLEAR_APP_USER_DATA` (denied — clean via
    `run-as rm` by explicit name; globs fail under run-as
    cwd); `policy_control` needs `WRITE_SECURE_SETTINGS`
    (denied — no system immersive override); background reaper
    kills idle runs (whitelist + prompt pulls); phone clock
    skews minutes (order files by sequence, not mtime).
174. **Font drift is not shaper drift.** Phone NotoColorEmoji
    (8.96 MB) vs emulator (10.2 MB): same advances, glyph id
    543 vs 568. Host reshape with the pulled phone bytes
    reproduces 543 exactly — the shaper is bit-deterministic
    across x86_64 host / x86_64 emulator / arm64 phone given
    identical bytes.
175. **Bet verdict: narrowed, hatch stays costed.** Adreno 650
    Vulkan full-scene render+readback 86–88 ms at 1080x2400
    (two runs); tiny-skia 293–550 ms; device 5 s cold
    one-time. No hard wall on mid-tier silicon — but the SD870
    is not weak-tier, and full-scene is not the incremental
    loop, so weak-tier (Mali-G52/Adreno-610 class) plus the
    sustained damage-cost loop stay open.
176. **Decompose frame costs before judging them (phone
    follow-up).** The 86 ms bundle splits (N=20, first
    separate): ~126 ms one-time compile, 16.7 steady
    render-only, 12.5 readback+map, 35.5 present CPU
    (2 samples). Vello 0.10 re-runs the full pipeline per
    frame — ~10k tiles at 1080x2400 regardless of content —
    so the steady render is a resolution-proportional floor
    sitting at the 16.6 ms budget for FULL scenes. Production
    skips readback/compile/stall and pipelines; per-frame
    damage cost stays the unmeasured half.

### close-out decisions (this round)

177. **Verify APK contents before install, never timestamps.**
    Gradle stages whatever `.so` is on disk silently; the
    emulator ran a full cycle on a stale x86_64 target
    (forensics: expected strings absent). Both ABIs rebuild +
    string-check after every shared-code change.
178. **Linux input mirrors Android classification.**
    Left/touch-0 pointer commands, four-key table, sampled
    modifiers, loud multi-touch drain, counted ignores —
    contract-tested like `android_contract` (7 tests).
179. **Lift coordinates travel with the lift.** A TouchUp at
    (0,0) misroutes to the origin (spurious-flip class);
    winit `TouchEnded` carries its location — use it.
180. **Multi-touch standing proof.** Shell drain
    (`second_finger_never_dispatches`) + device-proven
    index-0 forwarding through identical code; a physical
    second finger or root is the only closer (sendevent needs
    root, `motionevent` is single-pointer).

### v2 decisions (this session)

181. **v2 opens with item 1 (TIME/keyframes); items 2–3 queue in
    order.** The ordered backlog already sequences 1–3; item 1's
    substrate is fully banked (M8 evaluator + stamp pipeline +
    oracle standard + wasm `tick()` binding — all verified in-tree
    before the spec); its acceptance is provable host+browser with
    no phone (the session has emulator-only adb); and it unblocks
    the item-7 schedule (hardening after 1–3 minimum). One-page
    spec at `docs/04-planning/v2-keyframes.md`; build answers
    Q1–Q6, none pre-answered here.
182. **Keyframes spec paused for the usability round (procedural).**
    Decision 181 stands and `v2-keyframes.md` stays the recorded
    proposal; nothing was built against it this round (docs-only
    by brief — no framework features, no fuzzer/freeze/lock
    touches).
183. **App template lives in docs, not a new crate.** The proven
    blocks are embedded in getting-started + web-app.md; a
    template crate would add workspace build burden and bit-rot
    surface. The living example stays `crates/oppa-web`, linked
    (every linked file existence-checked).
184. **PascalCase `non_snake_case` warnings named, macro untouched.**
    The §4 naming convention fights the rustc default lint, but
    the warnings are harmless and silencing them inside
    `#[component]` would be a framework change — out of scope
    for a docs round.
185. **WSLg sustained-present death is environmental (proven).**
    Evidence chain: 3 reproductions with varying death counts
    (#16 ECONNRESET, #10–11 EPIPE, dual-client #11/#13 in one
    wall window) ruling out fixed-count pool exhaustion; two
    Weston libpixman SIGSEGVs (distinct PIDs) in this box's own
    dmesg; fresh Weston init in `/mnt/wslg/weston.log` at
    22:01:00; upstream microsoft/wslg#1386 exact signature
    match. No shell/demo behavior changed (one demo comment
    repointed). Decision-161's observation stands; its cause is
    now recorded. Real compositors not implicated; X11-under-WSLg
    shares the fate only via the same Weston (Xwayland child).
186. **v2 closure gate: usable + stable on desktop, mobile, and
    web.** Capability tracks are not the gate: items 1
    (keyframes), 3 (async decode/pruning), and 6 (wasmi) ship
    when ready but do not block closure. Gate-critical is the
    per-platform usability/stability delta: web text entry (U8),
    Linux IME policy, DOM fine-grained patching (item 4),
    production UIA hosting + AT focus/editing (item 5),
    paragraph shaping across all shaper slices (item 2),
    fuzzer/freeze hardening (item 7), and sustained-loop
    evidence on device.     Bounds that stay bounds without new
    facts: weak-tier silicon, Firefox/Safari, multi-touch
    device proof, Android a11y service (declined).
187. **Closure phase runs all gate-critical streams, in order:**
    U8 web text entry → item 2 paragraph shaping → item 4 DOM
    patching → Linux IME policy → item 5 UIA hosting → item 7
    hardening last, with device sustained-loop evidence
    alongside (emulator-available, no phone needed). Each
    stream: one-page spec, build, prove, record. Items 1/3/6
    stay parked; v1 history stays frozen.
188. **U8 channel: `InputEvent::Text { target, value: String }`
    (lock-#7 touch, decision-110 precedent).** Feed-only
    routing — no handler dispatch, because `TextField` leaves
    materialize with empty handler tables by construction
    (leaf `From` impls), so a Scroll-symmetric dispatch would
    panic on every field; leaf-builder machinery is out of
    U8 scope. Mirror of `bind_scroll` otherwise:
    `bind_text` + `bound_text` + prune beside the evaluator.
    Missing feed is a quiet no-op (deliberate, stated):
    the value stream is idempotent and level-triggered
    (every keystroke re-sends the full value — a drop
    self-heals, unlike edge-triggered scroll deltas), and
    the authoring docs state the bind requirement. Q4
    composition deferred (latin acceptance); Q5 closed by
    decision 96 (unhandled keys are accepted no-ops).
189. **Scoped field sizing: single-line intrinsic only, no
    paragraph machinery.** Two halves, both on existing
    primitives: (A) DOM omits width/height for zero-box
    fields (browser-intrinsic sizing — the `HtmlKind::Text`
    precedent: never echo an unmeasured number back;
    verdict-(b) owns field presentation); (B) layout measures
    an empty field payload as one space (parent-carried
    `TextField` semantics gates it — static empty text stays
    zero per the `empty_text_is_zero` guard; the space advance
    + font ascent/descent are measured, never invented).
    Non-empty fields already size intrinsically on measured
    platforms. If paragraph shaping proves necessary, stop —
    it is deferred v2 feature work, not this stream.
190. **Press-dispatch flake under swap history: observed, not
    fixed, referred to item 7.** The finish harness saw
    identical row clicks toggle-bearing intermittently
    (first-try green some runs, retry-needed others; one run
    proved MIDCLICK 2/8 while its own waiter timed out).
    Workaround (harness-only): hover-settle + bounded retry.
    No framework code touched for this — it is exactly the
    class the fuzzer/freeze hardening stream exists to cover;
    the M9 gate's adversarial-timing scenarios should include
    click-after-many-swaps before item 7 closes.
191. **F2 closed as harness noise; the item-7 referral (190) is
    WITHDRAWN — no bug demonstrated, nothing to cover.**
    Characterization: no minimal repro found (200/200 host
    rounds + 6/6 browser pages + every post-click dump
    correct + zero console/page errors across all runs); the
    three timeouts are explained by waiter flaws (one proven
    transient-miss, two minefield predicates) and
    rAF-polled waits on a loaded headless page, with the two
    thin observations lacking the post-click dumps that could
    distinguish no-dispatch from predicate-miss. Dispatch is
    exonerated; the M9 gate stands unchallenged (no coverage
    gap claimed without a bug). Standing rule from this
    episode: any future timeout must dump post-click state
    before dispatch failure may be claimed; the retained
    `f2_probe` regression test reopens the question on any
    failure.
192. **Item 0: break tables live in a new `oppa-linebreak`
    crate, never in core.** One-line reason: core keeps its
    M0 zero-dependency invariant (`[dependencies]` empty to
    this day) — third-party tables sit behind a trait in an
    adjacent crate, matching the `oppa-text-*` pattern, and
    `layout_text` consumes opportunities the way it already
    consumes `ShapedRun`s.
193. **Q1: adopt `unicode-linebreak` 0.1.5 (Apache-2.0, Unicode
    15.0.0), zero tailorings.** Lighter than `icu_segmenter`;
    `Allowed` offsets filtered to the `BreakSource` contract
    (sorted, unique, interior, soft only — `Mandatory` stays
    cluster-driven). The trait lives in core beside
    `TextService` (std-only, so `[dependencies]` stays empty);
    the tables + `UnicodeBreakSource` live in `oppa-linebreak`
    (this is what 192's "trait in an adjacent crate" means in
    the `oppa-text-*` pattern: trait in core, tables outside).
    Two self-caught expectation bugs on the way: no break
    between consecutive spaces or `//` (one break after the
    run — the observed UAX output, not my guess, is pinned).
194. **Q2 rules: over-wide spans push whole; trailing ASCII
    blanks trim at soft breaks only; never break inside a
    cluster; ellipsis keeps greedy truncation.** Spans (clusters
    between consecutive opportunities) are atomic: overflow
    against a non-empty line starts the next line whole, and an
    over-wide span on an empty line overflows whole (a single
    over-wide cluster stands alone — the greedy rule
    preserved). Trim strips whole trailing space/tab clusters
    at soft commits (hyphens, CJK, NBSP stay); paragraph-final
    blanks are kept (hard breaks don't trim). Stray offsets
    (not on cluster ends) cut nothing — inert, never splits.
    Ellipsis is truncation, not wrapping: it delegates to the
    greedy single-line path unchanged (stated boundary).
195. **Q3: wrap-point forward affinity — the break byte belongs
    to the next line's leading caret.** `LaidLine::caret_x`
    reads the leading edge for any byte at/before the line's
    first cluster; `LayoutBox::caret_position` routes bytes in
    no line (trimmed spaces, the dropped `\n`) to the first
    line starting at/after the byte, else the last line's
    trailing caret (unchanged). Uniform for soft and hard
    break bytes (pinned in the mixed-`\n` corpus row).
196. **Q5: shape-whole-paragraph-then-break stays (M3
    no-re-shape rule); `\n`-paragraphs shape separately then
    stitch; RTL cluster ranges repaired.** Joining across a
    hard break is meaningless, joining across soft breaks is
    preserved (each `\n`-paragraph shapes whole; wrap re-flows
    over cache). Real backends map no font for `\n` (loud
    refusal — pre-existing slice bound), so the engine splits
    at `\n`, shapes pieces, stitches (byte/glyph ranges
    rebased, `text_len_bytes` over the gaps); `split_paragraphs`
    is gap-tolerant (cluster-covered and gap `\n` both cut).
    The Arabic proof caught a real shared-core bug: RTL runs
    emitted visual-ordered ZERO-LENGTH cluster ranges (v1
    suites pinned cluster counts only) — now logical
    partitioning ranges in logical order (LTR behavior
    byte-identical; dwrite already logical, slices aligned).
    Noted change: `"\n"`-only text now commits a zero box
    (was: empty lines from a shaped newline cluster).
197. **Q4 corpus: stub rules + vendored anchor + tol-banded
    system fonts.** `oppa/tests/paragraph.rs` (uniform fake,
    both classes): six golden paragraphs — latin prose,
    over-wide word, URL, Arabic RTL, CJK no-space, mixed
    `\n` — break bytes byte-exact (text-derived), advances +
    per-byte carets byte-exact, 1 shape per paragraph (2 for
    the `\n` row), greedy default pinned. ONE vendored font:
    DejaVu Sans (Bitstream Vera — verified permissive at
    build; sha256 + size recorded beside the file): the
    rustybuzz slice replays the coverable rows byte-exact
    (WSL confirms cross-platform determinism); CJK coverage
    absence is a pinned loud bound (never tofu). dwrite slice
    replays all six opportunity-exact with advances tol-banded
    (`|Δ| ≤ max(1.0px, 5%)`). Machine-local font debt: Segoe
    UI + this box's CJK fallback face (dwrite), DejaVu system
    set (WSL proof env only).
198. **Q6: dwrite slice opportunity-exact, zero backend-specific
    code.** Only a test file + dev-dep were added; `lib.rs`
    untouched. Engine-level proof (real service + real source
    through `LayoutLedger`, committed boxes): break bytes
    equal the corpus on all six rows (P5 via the engine's
    split-stitch — direct whole-text shaping still refuses
    the newline run loudly, unchanged); widths inside the
    decision-197 band; affinity line indices exact.
199. **Disk-backed pipeline cache + deferred window show (demo
    arc).** `VelloBackend::ensure_gpu_for_surface_with_cache`
    (additive — the old path is byte-identical): requests the
    `PIPELINE_CACHE` device feature where advertised, seeds
    creation from caller bytes, returns `get_data` output for
    the caller to store; unsupported backends yield `None`
    (degraded, never failed); stale data rejected by wgpu
    validation (`fallback: true`). Verified negative: Dx12 has
    no `get_data` in wgpu-hal 29 (Vulkan-only), so Dx12 is
    uncacheable — the demo therefore prefers Vulkan (measured
    `Renderer::new` 1.9s vs 10–16s on Dx12 here) with default
    backends as fallback; backend choice stays app policy, the
    framework stays agnostic. `ShellConfig.visible` (default
    true) + `Win32Shell::show()` (additive; HWND copied out
    first — `ShowWindow` reenters the wndproc, holding the
    borrow panics): hidden window + pumped init + warmup
    present, then show with content. Numbers (release, RTX
    3060 Ti): Dx12 cold 7–16s every launch; Vulkan cold 2.7s
    (624281-byte cache); Vulkan warm 1.1s to content.

## 5q. v1 closure (this round — everything closable without a phone)

User brief: close v1, omit the phone (weak-GPU frame cost),
close the rest. Three closures, three justified opens, two
staleness fixes. No lock changed (decisions 147–151).

### 5q.1 AT-SPI live-bus 25/25 (WSL Ubuntu 26.04)

No sudo in WSL, but none needed: `at-spi2-registryd` 2.60 and
`python3-dbus` ship with the image. Tooling lessons: adb-style
daemon lesson repeated — `dbus-daemon --session` overrides
`--address` (use the canonical `at-spi-bus-launcher`, detached);
the registry has no Properties interface (methods-only).
Authoritative tables pulled from the ABI header + the running
impl: states CHECKED=4/SELECTABLE=22/CHECKABLE=41 (semantics
match usage exactly — the binary-strings absence of
"selectable" is a suffix-merging artifact, mechanism proven by
5 clear cases); roles TOGGLE_BUTTON=61/LIST_ITEM=31/FILLER=20/
ENTRY=77 (append-only ABI practice). Our exact tree data served
over D-Bus: registry echo (incl. its CamelCase
canonicalization — two vocabularies, both valid input),
role names+numbers, state numbers, children, extents,
flip-signal delivery. Scripts: `spike/atspi_bus_*.{sh,py}`.

### 5q.2 Gradle `assembleDebug` green and run

Gradle 8.14.3 (no-daemon — agent rule) + JDK 21 (Gradle 8
cannot run on the box JDK 25; Temurin 21 downloaded) + AGP
8.7.3, Kotlin DSL (`.kts` — the first failure was Groovy
parsing Kotlin), single manifest of truth via sourceSets, cargo
`.so` staged to jniLibs. Installed on the emulator: full
evidence set reproduced (DONE + pixels + meta).

### 5q.3 Windows UIA provider (`crates/oppa-uia`)

COM provider over `UiaTree` (CheckBox/ListItem/Edit/Group +
Toggle/SelectionItem/Value patterns): toggle + list-item
queryable through real interfaces with AT Toggle()/Select()
driving framework presses end-to-end (re-read flips).
Unsupported patterns fail `E_NOINTERFACE` (no null-IUnknown
exists); `SetFocus`/AT-editing/`HostRawElementProvider` fail
`E_NOTIMPL` (documented v1 bounds); event raising needs HWND
hosting (shell-window follow-up).

### 5q.4 Staleness closed

bidi.md "M2 editing session" → M1 spike session (substance
already right — `word_class`/caret stepping verified in
`spike-textedit`); state.rs decision-50 tail now matches the
decision's own M8 resolution (explicit per-host sizing,
decision 126). Doc-only, no behavior.

### 5q.5 Deliberately still open (each with its reason)

- Weak-GPU frame cost: no phone — user-omitted.
- Android a11y service: Java service + JNI node bridge with no
  TalkBack on the image means the only reader is our own
  logger — self-serving, proving less than the banked
  Linux/Windows proofs. Concrete recipe, not a framework gap.
- Text slices: Android slice needs the JNI bridge + device
  fonts; Linux slice needs sudo-blocked apt; a fourth
  (rustybuzz) implementation would prove shaper-agnosticism
  but closes no named residual — all three are a text
  milestone, not v1-closure.

### v1-closure decisions (this round)

147. **AT-SPI numbers come from the ABI header, names from the
    running impl.** `GetState→au` ids (checked=4,
    selectable=22, checkable=41) are ABI-stable per
    `atspi-constants.h`; role names + interface shapes from
    live introspection; role numbers from append-only
    practice (toggle-button=61, list-item=31, filler=20,
    entry=77). The bus proves names marshal and deliver; a
    real AT client (Orca) would be needed to independently
    confirm numeric ids — stated, not hidden.
148. **Gradle runs no-daemon + JDK 21, recorded.** Agent rule
    (daemons hang) + Gradle 8/JDK 25 incompatibility are
    environment facts future runs will hit again; the recipe
    (single-use fork, `JAVA_HOME` at Temurin 21, `.kts`
    naming) is the durable part.
149. **UIA absence is explicit errors, not nulls.** windows-rs
    cannot express null fragments/patterns, so missing
    targets fail `E_NOINTERFACE` and unservable actions fail
    `E_NOTIMPL` — both asserted in tests. Event raising stays
    with HWND hosting (shell window), not the provider.
150. **Staleness fixes are pointer-only.** Both named items
    were wrong milestone pointers over right substance —
    fixed without touching behavior or locks.
151. **v1 closes with three justified opens.** Phone perf
    (user-omitted), Android a11y service (self-serving
    without TalkBack), text slices (text-milestone scope).
    None is a framework gap discovered late; all have
    concrete next steps.

User brief: close v1, omit the phone (weak-GPU frame cost),
close the rest. Three closures, three justified opens, two
staleness fixes. No lock changed (decisions 147–151).

### 5q.1 AT-SPI live-bus 25/25 (WSL Ubuntu 26.04)

No sudo in WSL, but none needed: `at-spi2-registryd` 2.60 and
`python3-dbus` ship with the image. Tooling lessons: adb-style
daemon lesson repeated — `dbus-daemon --session` overrides
`--address` (use the canonical `at-spi-bus-launcher`, detached);
the registry has no Properties interface (methods-only).
Authoritative tables pulled from the ABI header + the running
impl: states CHECKED=4/SELECTABLE=22/CHECKABLE=41 (semantics
match usage exactly — the binary-strings absence of
"selectable" is a suffix-merging artifact, mechanism proven by
5 clear cases); roles TOGGLE_BUTTON=61/LIST_ITEM=31/FILLER=20/
ENTRY=77 (append-only ABI practice). Our exact tree data served
over D-Bus: registry echo (incl. its CamelCase
canonicalization — two vocabularies, both valid input),
role names+numbers, state numbers, children, extents,
flip-signal delivery. Scripts: `spike/atspi_bus_*.{sh,py}`.

### 5q.2 Gradle `assembleDebug` green and run

Gradle 8.14.3 (no-daemon — agent rule) + JDK 21 (Gradle 8
cannot run on the box JDK 25; Temurin 21 downloaded) + AGP
8.7.3, Kotlin DSL (`.kts` — the first failure was Groovy
parsing Kotlin), single manifest of truth via sourceSets, cargo
`.so` staged to jniLibs. Installed on the emulator: full
evidence set reproduced (DONE + pixels + meta).

### 5q.3 Windows UIA provider (`crates/oppa-uia`)

COM provider over `UiaTree` (CheckBox/ListItem/Edit/Group +
Toggle/SelectionItem/Value): toggle + list-item queryable
through real interfaces with AT Toggle()/Select() driving
framework presses end-to-end (re-read flips). Unsupported
patterns fail `E_NOINTERFACE` (no null-IUnknown exists);
`SetFocus`/AT-editing/`HostRawElementProvider` fail
`E_NOTIMPL` (documented v1 bounds); event raising needs HWND
hosting (shell-window follow-up).

### 5q.4 Staleness closed

bidi.md "M2 editing session" → M1 spike session (substance
already right — `word_class`/caret stepping verified in
`spike-textedit`); state.rs decision-50 tail now matches the
decision's own M8 resolution (explicit per-host sizing,
decision 126). Doc-only, no behavior.

### 5q.5 Deliberately still open (each with its reason)

- Weak-GPU frame cost: no phone — user-omitted.
- Android a11y service: Java service + JNI node bridge with no
  TalkBack on the image means the only reader is our own
  logger — self-serving, proving less than the banked
  Linux/Windows proofs. Concrete recipe, not a framework gap.
- Text slices: Android slice needs the JNI bridge + device
  fonts; Linux slice needs sudo-blocked apt; a fourth
  (rustybuzz) implementation would prove shaper-agnosticism
  but closes no named residual — all three are a text
  milestone, not v1-closure.

---

## 5r. v1 remainder (this round — Gaps 1–6 closed without a phone)

Six closures (rounds.md v1-remainder entry is the delta;
decisions 152–168):

- **Gap 1:** swapchain present via `wgpu::Surface`
  (`oppa-vello` surface path + app `surface.rs`), blit deleted;
  x86_64 + aarch64 built and packaged; emulator oracle exact-0,
  SHAs banked, screencap ON.
- **Gap 2:** `oppa-text-rustybuzz` core + `oppa-text-android`
  over `/system/fonts` + JNI `SystemFonts` bridge; 9/9 corpus
  lines byte-exact on-device (Bold/Regular bug found + fixed).
- **Gap 3:** live touch wiring (`adb input` flips on-screen
  both ways, density 2.625) + M1 shapes pre/post taps (3/3×2)
  + IMM policy (`show/hide true`) through the shell IME log.
- **Gap 4:** `oppa-text-linux` + `oppa-shell-linux` + WSLg
  `linux_demo` (window 800×600, DejaVu measures, CPU paints,
  exit 0; no sudo). Follow-ups: sustained WSLg presents, X11
  path, Linux input/IME.
- **Gap 5:** `tests/uia_events.rs` (HWND host + `CUIAutomation`
  pump; two flips observed with values + sender); provider
  gains `set_host_hwnd`, `AdviseEvents`, readable
  `ToggleState`, host-token propagation.
- **Gap 6:** `crates/oppa-web` (wasm `WebApp` + bootstrap +
  index) run against the M7 substrate (  `webapp.mjs` green,
  Edge 154); one framework fix (ownerless hover skip,
  decision 165).
- Verification: 328/0 serial, clippy/fmt clean, WSL green,
  device oracle + taps + feeds + shapes green, webapp green,
  uia_events 3/3.

---

## 5s. Phone round (this round — Snapdragon 870 walks in)

The user connected a Realme GT Neo 3T mid-session; the missing
measurement ran the same APK (arm64). arm64 executes (API-35
`.so` loads on API 31); Adreno Vulkan oracle exact-0 with
cross-ISA SHAs intact (`5a2f…` / `7cd4…` on all three ISAs);
render+readback 86–88 ms; visible present + both taps on-screen
(0x55/0x44 at display (22,67) = window +55 offset); text 8/9
with font-drift isolated host-side; Adreno GLES wall mapped;
feeds 3/3x2, IMM show/hide true, 228 faces / 407 JNI fonts.
App hardening: `imm_mode.rs` + `OppaUi.java`, window-independent
records first (`oracle.txt`), headless tap fallback, UI-thread
hop + DexClassLoader rules (decisions 169–170), ColorOS notes
(173). Evidence: `device-out-phone/`, tombstones quoted in
rounds. Bet verdict: narrowed per decision 175, decomposed per
176 (16.7 steady render-only, 12.5 readback, 35.5 present CPU;
`frameloop.txt` pulled).

---

## 5t. v1 close-out (this round — everything v1 fixable on Windows + emulator)

Emulator re-run on the final APK (strictly sequenced single
run): oracle exact-0 + SHAs, GL decomposition second sample
(38.5 / 11.9 / 26.7), 6 presents (52.1 avg), two taps with
matching OFF/ON screencaps, text 15/15, feeds + IMM green.
Stale-APK rule (verify contents, never timestamps — decision
177). Linux input mapping closed (7 contract tests host +
WSL, demo wired; `translate` reviewed-only). Present depth now
n=6+6 emulator (+2 phone). Decisions 177–180. Verification:
335/0 serial, clippy/fmt clean.

---

## 5u. v2 open (this round — spec only, no code)

First-action round: environment re-verified (adb single
emulator-5554, no phone; Windows cargo 1.97.1;
`cargo test -p oppa --lib transition` 6/6 sanity green; WSL rust
1.98.1 via login shell, `CARGO_TARGET_DIR=/tmp/oppa-target`
per-invocation; `spike/web/webapp.mjs` harness present),
item 1 proposed as the opener (decision 181), one-page spec at
`v2-keyframes.md` (coverage + proof question, mechanical
acceptance at the M8 oracle standard, boundary, open questions
Q1–Q6). Full suite not re-run — tree unchanged, 335/0 stands;
clippy/fmt untouched (no `.rs` touched).

---

## 5v. Usability round (this round — docs only, one demo comment)

Outsider web app (§1 of the rounds entry: two components,
shared state, keyed list, proven in headless Edge with zero
console errors); stuck-point fixes (§3: getting-started
rewritten, `09-api/web-app.md` new, `widget.md` corrected,
entry pointers in web overview + README; template stays
docs-embedded per decision 183); WSLg death root-caused
environmental (§4: Weston libpixman SIGSEGV, decision 185 —
no shell/demo behavior change, one demo comment repointed).
Remaining app-story gap (U8) is CLOSED by §5w below
(supersedes this line — recorded, not rewritten away). Verification: 335/0 serial (saved log),
shell-linux 9/0 post-touch, clippy/fmt clean.

---

## 5w. U8 text entry (this round — first gate-critical stream)

Spec `v2-textentry.md` → decision 188 (lock-#7 touch) → build
→ three-level proof → record. Channel, bind/query/prune,
pid map, oppa-web binding + preserving bootstrap (demo
scene untouched). Authoring closed in `09-api/web-app.md`.
Bounds: zero-size field rendering (layout track owns it);
composition deferred; item 4 subsumes the swap shim.
Verification: 341/0 serial (saved log: +5 text_entry, +1
node_for_pid; fmt-only touch after, targeted re-runs green),
clippy clean, fmt clean.

---

## 5x. Scoped sizing + usability finish (this round)

Decision 189 → build (layout space-measure + DOM geom
omission) → tests (+2) → browser proof (177×21 intrinsic,
native focus/type) → finish deltas (8 rows, toggles,
clear-done removal) → friction-2 + flake referral (190).
Paragraph shaping untouched (stop condition never met).
WSLg confirmed already-closed, no new work. Verification:
343/0 serial (saved log), m3 17/17 post-fmt, clippy/fmt
clean.

---

## 5y. v2 item 2 paragraph shaping (this round — second gate-critical stream)

Spec `v2-paragraph.md` → decisions 193–198 → build → four-level
proof → record. Item 0 first (new `oppa-linebreak` crate + core
`BreakSource` trait, 9 unit tests), then `layout_text_with_breaks`
+ engine wiring + affinity beside the untouched greedy path, then
the corpus (stub 7 + DejaVu anchor 5 + dwrite 6, both classes),
then the browser app leg. Two genuine finds inside the round:
(1) the shared core emitted degenerate RTL cluster ranges
(counts held, ranges did not — decision 196 repair, slices
aligned); (2) real backends refuse `\n` runs loudly, so the
engine shapes `\n`-paragraphs whole and stitches (decision 196
— the adopted shape-whole-then-break direction, not a detour).

Proof (four levels, not demos): stub corpus pins rules
byte-exact (breaks, advances, per-byte carets, affinity,
trailing, 1-shape-per-paragraph, greedy default); DejaVu anchor
pins real-shaper advances byte-exact (WSL green too —
cross-platform determinism); dwrite slice pins the same break
bytes opportunity-exact with tol-banded widths and zero
backend changes; browser leg (`drive-para.mjs`,
`verdict-para.json` pass=true) proves multi-line content on
screen (seeded label two visual lines, row toggle 1/3→2/3,
long label added whole, zero console errors, screenshots
eyeballed).

Observed bounds (not decisions): serviceless web renders
`white-space:normal` (browser owns text flow — documented
design, `pre` applies to measured runs only); the `\n` in the
seeded web label collapses, its two visual lines are browser
soft-wrap; `"\n"`-only text commits zero; direct whole-text
shaping still refuses `\n` loudly on both real slices.
Verification: 379/0 serial (saved log: +9 linebreak, +7
paragraph, +5 dejavu, +6 dwrite paragraph, +8 layout unit),
clippy clean, fmt clean; WSL linux slice 6/6 + linebreak 9/9 +
dejavu 5/5; android slice 15/15 in the serial run.

Machine-local assets: `drive-para.mjs` + `verdict-para.json`
(pass=true) + `shot-para-*.png` + `probe-dom.mjs` in the
outsider-todo temp dir (rebuilt wasm 446745 bytes with the
item-2 code); `v2-para-test.log` (UTF-16, counted from file).

---

### Eyeball fix-up decisions (this round)

323. **Theme owns default ink and page background on every
    presenter.** `DrawOp::Text` without `Style::ink` resolves to
    the build theme's `text_primary` (published per frame;
    Light's IS the contract `INK`, so unset builds are
    pixel-identical); the caret fallback matches; desktop
    surfaces clear to the theme `background` (refit on toggle);
    the DOM body carries both via full-page style + a
    change-only patch stanza the bootstrap applies; explicit
    author ink still wins everywhere. The sink's dead Dark
    toggle now owns the host palette, and its page furniture
    resolves from tokens. Non-goals recorded in the round:
    fixed decorative literals (gradient card, swatch cells),
    `SELECTION_FILL`, and the OS window chrome stay as-is.

---

## 7. Deliberately NOT built (boundary)

Per BUILD-ORDER §5 and the milestone split:

- **M0b remaining**: the other platforms' TextService backends
  (rustybuzz/swash wasm slice, HarfBuzz-class Linux stack, Android platform
  APIs) — mutually independent follow-up work; the measure↔shape protocol
  exists now for them to implement. V1-remainder update (§5r):
  Android + Linux slices CLOSED via one shared rustybuzz core
  (`oppa-text-rustybuzz` + thin wrappers); wasm text stays
  serviceless by web decision (locked #2 scope).
- **M1 remaining after the bidi/combining/ZWJ round**: the *real* Vello backend
  machinery (the debug renderer is throwaway once M3+ lands); the other
  platforms' shells; the M3 layout engine's bidi visual ordering
  (measured this round, re-deferred with evidence); shared-suite word
  rules incl. ZWJ-emoji + scalar combining-caret stepping (spec items,
  M2 editing session); a c3 re-baseline (pre-existing CDP/Edge drift,
  unrelated to any gate). The DOM text/editing contract freeze:
  (a) **closed as locked #28** (two hands-off 6/6 real-IME runs);
  (b) **partially closed as locked #29** (combining cluster parity +
  ZWJ single-cluster closed; visual-ordering deferral is the only open
  freeze item). Variant A on Web's fidelity remains unmeasured
  (REPORT.md residual 1; moot under (b)).
- **M2**: DONE this round — reconciler, component model, `Ctx`/
  per-instance dep tracking, `ctx.binding`, keyed lists, `keyed_state`,
  TreeDiff emission, binding-edge stamps (§5g; ROUNDS.md M2 entry).
  Per-instance *scheduling* (child effects), per-key store granularity,
  the TIME transition evaluator, and RSX/struct-literal sugar stay
  downstream (M5/M8/deferred — decisions 48/52/53).
- **M2b**: DONE this round — manifest export/scan (`component_manifest!`
  + `HotRegistry::install` rescan), real dylib swap (retire model),
  opaque generation-tagged props with hot-side clone/drop + typed
  drain/adopt, drain-before-unload ordering, registry re-resolution via
  post-swap re-runs, §8.1 re-seed assert (`assert_no_outgoing_props` +
  keep/reseed/revive tests) + call-site lint, §9.6 crate-level state
  lint (`#[hot_crate]`), §8.4 fuzzer v1 + task/message path (executor,
  cancel-at-RELOAD, discard-by-tag, exactly-once accounting), per-run
  symbol resolution, shared-state run stacks (§5h; ROUNDS.md M2b entry;
  decisions 60–67).
- **M3**: DONE this round — framework-owned layout engine
  (`oppa::layout` + LAYOUT-phase wiring + host ledger), flex subset +
  block-lite + absolute, inline wrap/BiDi/optional ellipsis, the
  `TextService` measure protocol with per-node cache, one-frame-delayed
  settled metrics, shared DPR rounding at commit positions (§5i;
  ROUNDS.md M3 entry; decisions 68–82). Bidi visual ordering closed by
  oracle measurement (locked #29's freeze item); wrap measured at 1
  shape + 0 re-shapes (no scope finding); wasm drift open (§8.8).
- **M4**: DONE this round — renderer contract types (`oppa::render`:
  PresenterKind/Caps/DrawOp/FramePlan/SemanticsDiff/RendererBackend),
  tiny-skia CPU backend + dirty-subtree FramePlan builder + headless
  image-diff oracle + PAINT-phase wiring (§5j; ROUNDS.md M4 entry;
  decisions 83–92). Locked #5 proven implementable; findings F1
  (LAYOUT-only moves don't rebuild) and F2 (fresh-text-leaf flags)
  recorded; damage payoff + glyph quality measured, not assumed.
- **M5**: DONE this round — `InputEvent` plumbing (`oppa::input`),
  core-side hit-test walk, capture/focus router in INPUT's
  `BatchGuard`, keyboard + deterministic Tab order, §4.1 Toggle
  end-to-end on the CPU backend incl. `Semantics::switch` (§5k;
  ROUNDS.md M5 entry; decisions 93–102). Locked #7 proven; #3
  exercised interactive. Border + ink resolved as stated Style
  fields (no backend change); alpha (M6 Vello-blend owner) + F1
  (M6 PAINT-stamping owner) re-recorded open; inline-child flag
  attribution (M8) + Win32→`InputEvent` mapping (platform track)
  tracked as follow-ups.
- **M6**: DONE this round — Vello backend (`oppa-vello`: Scene encoder,
 onto the same dirty-subtree FramePlans) + driver matrix on RTX 3060 Ti
  + fallback row + glyph review vs the M4 baseline (§5l; ROUNDS.md M6
  entry; decisions 103–109). Locked #17 proven; #21 observable and
  bounded (skew ledger); #18 serviced with the compositor owned
  (TIME interpolation at vsync cadence off the injected clock).
  Tripwire PASS on evidence; Skia hatch stays costed, unbuilt.
- **M7**: DONE this round — DOM backend (`oppa-dom`: TreeDiff→DOM,
  StyleId→CSS, native scroll, external hole, ARIA incl. the
  verdict-(b) text/edit path) + exact em size + per-run font
  identity (decision 110) + measured parity corpus (§5m; ROUNDS.md
  M7 entry; decisions 110–118). Locked #2 proven; #23 INPUT-fed
  with ≤1-frame trail. Finding F3 closed in full; finding F5
  found and fixed in the round. The DOM text/editing freeze stays
  gated on §2.3's blocking conditions (only the M3
  visual-ordering deferral remains open — #29's residue).
- **M8**: DONE this round — virtualization + transition evaluator
  (§5n; ROUNDS.md M8 entry; decisions 119–128). Locked #13 proven
  (zero structure ops/tick, 20 repainted cells ≤ ~30, per-instance
  selection via keyed_state); #22/§9.4 proven (phantom-flash 0,
  interpolators 0 on stamped commits, TIME curve + DOM one-commit
  suppression); §9.3 lag cover measured (one constant 4 stands).
  Findings: F6 + tail-freeze + splice order + straddle margin found
  and fixed in-round; slot-position CSS churn recorded as follow-up
  (touches #111 — M9/cleanup, not smuggled in).
- **M9**: DONE this round — reload product loop + fuzzer gate
  (§5o; ROUNDS.md M9 entry; decisions 129–134). Locked #15 proven
  under adversarial timing (5 scenarios, ~300 swaps, 0 engine
  violations on three seeds); #25 proven under load (cancel/
  discard race 76/76, exactly-once partition). Gate DECLARED in
  `docs/03-spec/reload/freeze.md`: renderers may freeze. M2b
  verdict: working swap path (not scaffolding); true unload stays
  deferred (decision 61 stands).
- **M10**: DONE this round — Android shell + AT-SPI emitters +
  GLES row (§5p + §5p.6 gap closure; ROUNDS.md M10 entries;
  decisions 135–143). Last v1 milestone: input roundtrips the
  shared pipeline, restart == cold start (pixel- + dump-identical),
  AT-SPI toggle/list-item emit + queryable, GL path conforms at
  the M6 oracle standard (exact 0) with CPU fallback measured,
  emulator row measured (API 36 / GLES 3.0 / Enforcing).
  Remaining residuals: weak-GPU frame cost, live-bus validation,
  app glue (Gradle + activity + APK — toolchain installed per
  decision 144: NDK r29, both Rust Android targets, cross-check
  green; on-device CPU + Vello-GL pixels proven per decisions
  145–146, raw evidence in `crates/oppa-android-app/device-out/`).
  V1-remainder update (§5r, decisions 152–168): app glue CLOSED
  (Gradle APK with both ABIs, swapchain present, touch + IME on
  device); text slices CLOSED (Android JNI + rustybuzz slice,
  Linux sudo-free slice); live-bus was already closed in §5q.
  Still genuinely open: weak-GPU frame cost (no phone).
- **Beyond v1**: animations/TIME interpolation (beyond the
  `.transition` primitive), image async decode.
- Also: no signal-drop refcount semantics (retirement is explicit; the
  M2b fuzzer drives it), no `!Send`-checked worker thread in tests (the
  queue is single-threaded at M0 by design; real cross-thread submit is
  the M2b executor's job — proven), no multi-line/paragraph shaping or
  line breaking (v1's bound: single-line flat text-in-flex; DESIGN
  §2.3). True-unload (shared-core linking) and site-key pruning are v2
  deferrals (`docs/HANDOFF-V1.md` §4; decisions 61/66 — M9 shipped the
  product loop with the retire model confirmed; neither was built there
  or since).
- **Ready-readiness-plan ongoing (Phases 19+)**: the Phase-19 brief
  (validation on a real application, desktop + Android + web) and its
  carried debt live in `production-readiness-plan.md` +
  `backlog.md`; Linux test execution and the full-workspace Linux
  gate are recorded-opens from Round 19.0 (upstream `windows-future`
  Linux compile ungatedness).

---

## 8. API quick reference

```rust
// --- M0: reactive core
let rt = Runtime::new();                      // or Runtime::with_clock(Rc<dyn Clock>)
let s  = rt.signal(0i32);                     // Signal<T>, !Send
let s  = rt.signal_named("counter", 0i32);    // + stable label for the cycle printer
let m  = rt.memo(|| s.get() * 2);             // structural PartialEq gate
let m  = rt.memo_with_eq(|a: &u32, b: &u32| a % 2 == b % 2, || s.get());
let e  = rt.effect(|| { s.get(); });          // runs now, re-runs in EFFECTS
untrack(|| s.get());                          // no dependency created
let _g = rt.batch();                          // RAII; fan-out at outermost drop; nested merges

s.get();            s.get_arc();              // tracked reads
s.set(5);           s.update(|v| v + 1);      // writes
m.read();           m.read_arc();             // pull-recompute + tracked reads

rt.register_handler(HandlerId::from_symbol("btn.press"), || { s.set(1); });
rt.swap_handlers(map);                        // atomic registry flip
rt.dispatch(Event { kind: EventKind::Press, handler: id });
rt.push_event(event);                         // internal queue, drained at INPUT

rt.worker_submit(|rt| { s.set(7); });         // gen-tagged; drained at INPUT
rt.generation(); rt.advance_hot_generation();

rt.request_frame(); rt.request_reload();
rt.add_animation(|now| { s.set(now); false }); // TIME-phase task; false retires
rt.set_reload_hook(|rt| { rt.swap_handlers(new); });
rt.set_a11y_pass(|| { /* SemanticsDiff emission later */ });
rt.set_shell(Box::new(mock_shell));           // mock_shell may honor set_ime(ImeOps)

rt.run_once() -> bool;      // one frame if demand
rt.run_until_idle() -> usize;
rt.has_demand() -> bool;
rt.stats() -> Stats;        // frames, passes_total, passes_last_frame, phase_runs[7],
                            // worker_applied, worker_discarded
rt.take_phase_log() -> Vec<Phase>;

rt.retire_signal(&s); rt.retire_memo(&m); rt.retire_effect(&e);  // loud after

// Storage
let mut arena: GenArena<u64> = GenArena::new();
let id = arena.alloc(7); arena.retire(id)?; arena.get(id) /* panics if retired */;

// StyleId
let mut styles: Interner<String> = Interner::new();
let sid: StyleId = styles.intern("bg:red;radius:12".to_string());

// --- M0b: TextService contract (core, zero-dep)
let mut style = TextStyle::new("Segoe UI", 16.0);
style.device_pixel_ratio = 2.0;
style.letter_spacing_px = 1.0;

let run: ShapedRun = text_service.shape("héllo", &style)?;
let glyph = run.glyph_index_for_byte_offset(2);        // mid-cluster snap
let byte = run.byte_offset_for_glyph_index(glyph.unwrap());
let x = run.caret_x(byte);                             // cluster leading edge
let hit = run.byte_offset_for_x(x);                    // midpoint rule
let anchor: CaretRect = run.caret_rect(byte);          // candidate-window anchor
let measured: MeasuredRun = text_service.measure_line(&run);
let box_px = round_to_device_px(13.37, 2.0);           // §8.8 shared rule

// --- M0b: IME composition surface
let mut feed = ImeCompositionFeed::new();
feed.push(ImeCompositionEvent::CompositionStarted { start_byte: 4 });
feed.push(ImeCompositionEvent::CompositionUpdated {
    composition: "ni hao".to_string(),
    caret_byte: 10,
});
feed.push(ImeCompositionEvent::CompositionCommitted {
    committed: "ni hao".to_string(),
});
feed.drain(&mut editing_session);              // impl ImeCompositionHandler

// --- M0b: the Windows backend
let dwrite = DWriteTextService::new()?;        // !Send; UI thread
let fonts = dwrite.enumerate_fonts();
let run = dwrite.shape("日本語", &style)?;

// --- M1 spike: the editing session (spike-textedit crate)
let ime_ops = Rc::new(RefCell::new(Vec::new()));
let rt = Runtime::new();
rt.set_shell(Box::new(RecordingShell { ime_ops: ime_ops.clone() }));
let mut session = EditingSession::new(
    rt.clone(),
    Rc::new(DWriteTextService::new()?),
    style,                       // TextStyle (device px out)
    "Hello world".to_string(),   // initial content
    ime_ops,                     // shared set_ime record
);
session.click_x(12.5);                         // hit-test → caret (cluster rule)
session.shift_click_x(30.0);                   // extend selection
session.insert("!");                           // controlled write → Signal
session.undo();                                // single-level restore
session.composite_text();                      // committed + in-progress text
session.composite_caret_byte();                // caret in composite coordinates
session.caret_rect();                          // candidate anchor (device px)
session.commit_frame();                        // rt.run_once() + emit set_ime op
session.observable();                          // {content, caret, sel, composition}
run_ime_steps(&mut session, &scenario.steps);  // rig scenario driver
session.take_canonical();                      // criterion-3 per-event stream

// --- M1 spike: the rig (single source of truth for both arms)
// cargo run -p spike-textedit --bin spike_win_arm   → spike/corpus.json + results/windows.json
// node spike/web/harness.mjs                        → results/web.json
// node spike/web/compare.mjs                        → results/verdict.json

// --- M1 remainder: the window shell (oppa-shell-win)
let cfg = ShellConfig { title: "oppa — one field".into(), ..Default::default() };
let shell = Win32Shell::new(cfg)?;             // class + window + proc
shell.process_os_messages();                   // OS → internal queue; true on WM_QUIT
let events = shell.pump_events();              // M0-normalized (the trait contract)
let cmds = shell.take_cmds();                  // 1:1 payloads (Cmd), event order
shell.set_ime(ImeOps::SetCaretRect { x, y, width, height });  // real anchoring
let ime = shell.ime_status();                  // ImeState { open, conv, sent, hkl }
let _msg_log = shell.take_message_log();       // every message, arrival order
let _ime_log = shell.take_ime_log();           // the shell's IME-event lines
// FIELD_EVENT: the handler id the M0 registry dispatches (the field's op
// mapping lives in the host's registered closure).

// --- TSF re-run: the TSF-aware path (oppa-shell-win/src/tsf.rs)
let tsf_lines = shell.enable_tsf("Hello world", (0, 0))?; // ThreadMgr +
                                               // DocMgr + associate +
                                               // focus + IS_TEXT, seeded
let status = shell.tsf_status();               // Option<TsfStatus>
                                               // { client_id, edit_cookie,
                                               //   associated, focused,
                                               //   input_scope, has_context }
let _line = shell.tsf_reassert_focus();        // per-step SetFocus re-assert
let _line = shell.tsf_note_focus(true);        // on Cmd::FocusChanged

// --- Text-store round: the TIP-facing store (same file)
let _line = shell.tsf_sync_external(&content, (sel0, sel1)); // mirror the
                                               // settled session in
                                               // (skipped while a TIP
                                               // composition owns it)
let _trace: Vec<String> = shell.tsf_take_store_log(); // TIP-transaction
                                               // trace for step notes
// cargo run -p spike-textedit --bin spike_ime_shell -- --ime-pass --wait-secs 2

// --- M1 remainder: the session additions (additive, tested)
session.extend_caret(1);                       // shift-extended arrow move
session.select_all();                          // Ctrl+A: sel (0, len), caret len
let start = session.composition_start_byte();  // the active composition's anchor

// --- M1 remainder: the DWrite debug hook (NOT in the TextService trait)
let (path, face_index) = dwrite.font_file_source(run.runs[0].font_id)?;

// --- M1 remainder: the automated real-IME pass
// cargo run -p spike-textedit --bin spike_ime_shell -- --ime-pass
//   → spike/results/ime_manual.json (125 raw Win32 messages + per-step
//     observables + the environment record); verdict FAIL — the
//     composition never engaged (the freeze gate stays open)

// --- M2: component model + reconciler (oppa crate)
use oppa_macros::{component, Props};  // #[component] pass-through; #[derive(Props)]

#[derive(Clone, Props)]
struct ToggleProps { label: SharedString, initial: bool, enabled: bool,
                     on_change: HandlerId, theme: ToggleTheme }

#[component]
fn Toggle(ctx: &Ctx, props: &ToggleProps) -> VNode {
    let is_on = ctx.signal(props.initial);   // per-instance, call-site keyed
    let item = ctx.binding(|| store.get(0)); // binding edge (§9.4 stamp)
    let m = ctx.memo(|| is_on.get());        // per-instance re-derivation
    let anim = ctx.keyed_state::<u32>(7, || 0); // item-keyed, LRU (cap 64)
    let hovered = ctx.hovered();             // framework flags (M5 feeds)
    let offset = ctx.scroll_offset();        // .get()/.set()/.row(h)
    let child = ctx.child("ContactRow", slot, &row_props, ContactRow);
    ctx.emit(props.on_change);               // payload-less until M5
    Div("track")
        .style(Style::new().size(44, 24).radius(12).bg(track)
            .opacity(props.enabled.then_some(1.0))
            .transition(Transition::new(120.ms(), Ease::Out)))
        .semantics(Semantics::switch().checked(is_on.get()).label(&props.label))
        .on_press(move || is_on.set(!is_on.get()))
        .child(Div("knob").style(Style::new().size(18, 18).circle().x(knob_x)).build())
}

let host = ComponentHost::new();             // owns Runtime + Reconciler
let handle = host.mount("Toggle", props, Toggle);  // = one EFFECTS effect
handle.set_props(props2);                    // opaque swap + effect scheduling
host.run_until_idle();
let diff: TreeDiff = host.last_diff().unwrap();
diff.structure_ops(); diff.update_ops(); diff.suppress_transitions;

let store = Store::new(&rt, ids, values);    // Store<Id, V>: get/lookup/len
store.get(3); store.lookup(&id); store.set(ids2, values2);
let img = ImageCache::new().load("av3");     // content-addressed stub
let p = OpaqueProps::new(props, rt.generation());  // clone()/get::<T>()/try_get()
let s = rt.keyed_state::<u32>(7, || 0);      // core-side LRU state
rt.take_binding_fired(); rt.drain_keyed(); rt.mark_effect_dirty(effect.id());
rt.keyed_capacity(); rt.set_keyed_capacity(8); // default 64, overridable

// --- M3: layout engine (oppa::layout; framework-owned, locked #6)
host.set_text_service(Box::new(dwrite));   // the measure source
host.set_viewport(800.0, 600.0);           // CSS px (default 800x600)
host.set_layout_config(LayoutTextConfig { ellipsis: true, ..Default::default() });
host.run_until_idle();                     // EFFECTS reconcile, LAYOUT engine+publish
let b: LayoutBox = host.committed_box(id).unwrap();  // untracked (paint/a11y/tests)
let s: LayoutStats = host.layout_stats();  // nodes_shaped = shape() calls this run
let settled: Option<LayoutBox> = host.settled_box(id);  // tracked: re-runs next EFFECTS
// inside a component body (an effect): ctx.settled_layout(id) — same contract
let ordered = oppa::layout::order_visual(&shaped, text, &logical_indices);
let lines = oppa::layout::layout_text(&shaped, text, avail_w, ellipsis_adv, asc, desc, gap);
b.caret_position(byte);                    // (line_idx, visual x), forward affinity

// --- M5: input events + hit-testing + focus (oppa crate + oppa-cpu)
host.inject_input(InputEvent::pointer_down(22.0, 12.0)); // real payloads, never signal writes
host.inject_input(InputEvent::key(oppa::input::keys::TAB, KeyState::Pressed));
host.run_until_idle();                     // 1 frame: INPUT route → EFFECTS → LAYOUT → PAINT
host.hit_test(10.0, 12.0);                 // Option<NodeId>: deepest wins, misses are None
host.tab_order();                          // Vec<NodeId>: press owners, DFS pre-order
host.hovered_node(); host.capture_node(); host.focused_node();
host.debug_instance_flags(instance);       // (hovered, pressed, focused), untracked
// Style: Style::new().border(2, color).ink(color) — inset ring + inherited ink

// --- M7: DOM backend + scroll feed + text fields (oppa crate + oppa-dom)
Div("track").on_scroll(|| {}).on_ime(|| {}); // scroll/IME target declaration
host.bind_scroll(list_node, offset);       // §9.3 INPUT feed: routed dy accumulates
host.bound_scroll(list_node);              // Option<f32>: the fed signal, untracked
let field: VNode = TextField { text: Arc::from("Ada"), style: Text::title_small, label: Arc::from("Name") }.into();
let hole = Custom("player", 7).style(Style::new().size(160, 90)).build(); // external hole
let mut dom = DomBackend::new(1.0);        // RendererBackend: commit(TreeDiff)
let mut sheet = StyleSheet::new(1.0);      // class_for(StyleId, &Style) → stable "s{bits}"
dom.sync(&rec, &styles, &mut sheet);       // retained-read re-derive → SyncStats{touched, elements}
dom.note_browser_scroll(list_node, 48.0);  // browser-owned scrollTop ledger (observed, never written)
oppa_dom::scroll_window(offset, row_h, viewport_h, n); // (first, end) ± OVERSCAN_SLOTS(4)
oppa_dom::aria_attrs(&semantics);          // total ARIA table
render_page("t", &dom, &sheet);            // deterministic full page + data-pid hooks
// install_dom_paint_hook(&host, backend, sheet, surface, dpr, calls, touched); // PAINT wiring
// op.em_size + op.fonts on DrawOp::Text (exact em + per-run FontRun — decision 110)

// --- M2b: hot reload (oppa crate + oppa-macros + oppa-reload)
use oppa_macros::{component_manifest, hot_crate};

#[component]                              // + call-site lint (nested-fn, conditional/loop)
fn Toggle(ctx: &Ctx, props: &ToggleProps) -> VNode { /* ... */ }

oppa_macros::component_manifest![export, Toggle(ToggleProps)];  // or no `export` (static)
oppa_component_manifest() -> ManifestView                       // stable dylib export

#[hot_crate]                             // compile-time ambient-state lint
mod components;

ctx.spawn(|scope| {                      // Send-only bodies; results via scope
    scope.submit(move |rt| {             // generation-tagged INPUT drain
        rt.keyed_state::<u32>(MAIL_KEY, || 0).set(done);
    });
});
rt.drop_pending_tasks(outgoing);         // cancel-at-RELOAD (returns count)
rt.stats().tasks_done; rt.stats().tasks_dropped;

let mut reg = HotRegistry::new(host.clone());
reg.install(Box::new(source));           // record the loaded manifest
HotRegistry::arm(&reg);                  // RELOAD-phase hook
HotRegistry::request_swap(&reg, Box::new(v2));
host.run_until_idle();
let report: &ReloadReport = reg.last_report().unwrap();
report.ok();                             // no evictions
host.mount_erased("NewOnly", props, render);  // manifest-discovered mount
```
