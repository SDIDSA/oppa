# Project state (current)

Status: current. Last verified: 2026-10-04 (through Decision 412).
This is the only planning file. Rounds overwrite it in place — never append
snapshots, never create per-round files. History is in git (`git log -- docs/STATE.md`).

## Done (shipped — do not re-plan)

- Engine M0–M10: reactive core + storage + scheduler, reconciler/component model, hot-reload harness + fuzzer gate, layout engine, CPU/Vello/DOM backends, events + hit-test + focus, virtualization + transition evaluator, Android shell + AT-SPI emitters.
- v1 remainder + close-out: Linux/Android/Web text slices, phone round (Snapdragon 870 oracle + frame cost + visible present), present depth, Linux input, stale-APK rule.
- Productization Phases 8–35: controls catalog (15 controlled components), Task Studio reference app + E2E, `hello-desktop` + `hello-web` templates, Toast, cookbook, API-doc refresh, packaging recipes, `WasmHost` harness, testkit key/text helpers, first repo CI. Productization goals G1–G6 closed.
- Validation: full-workspace Linux gate, Windows live pass (COM dialogs, TSF chain, `WM_CLOSE`), Edge 14/14 + Firefox boot smoke, Vulkan teardown-race fix, Xn damage-loop harness mode.
- Phase 36 PR1 (decision 352, gate green 2026-10-01): Semantics + A11Y action
  parity — validation marks, numeric range bounds, `Role::{Tree, TreeItem,
  MenuItem}`; DOM `aria-invalid/required/errormessage/valuenow/min/max`,
  AT-SPI Action + Value in the wire tree, UIA Invoke + RangeValue with
  COM-thread → host-loop marshaling; MenuItem control migrated off ListItem.
- Phase 36 PR2a (decision 353, gate green 2026-10-01): Layout Grid — 2D
  `Tag::Grid` (`GridTrack::{Px, Fr, Auto}`, templates, spans, auto-flow),
  `flex_grow`/`flex_shrink` (weighted pool, `fill` ≡ weight 1, explicit
  wins, opt-in shrink), `min`/`max` clamping (resolved clamp, explicit
  contradictions refuse); all pre-existing layout suites byte-identical.
- Phase 36 PR2b (decision 354, gate green 2026-10-01): 2D `ScrollArea` —
  `ScrollOffset { x, y }` unification (`ScrollXY`/`ScrollOffset2D`
  sharing the 1D twin signals), horizontal thumb math, `content_w`
  overflow with x self-wire, `Shift+Wheel` routing at the shell layer
  (win + linux), transposed `Scrollbar` axis (incl. G18 range payload);
  all pre-existing scroll suites green.
- Phase 36 PR3 (decision 355, gate green 2026-10-01): Text v2 —
  `VNode::RichText` (shape-per-span then join, shared size; joined
  bytes stay the caret/selection space); per-run ink splitting paint
  ops (single-ink scenes byte-identical) on the shared builder (CPU +
  Vello) and DOM spans; web `@font-face` serves the measured DejaVu
  bytes (closes decision-81 drift); DejaVu-pinned corpus extended
  (join transparency, bold-span breaks/carets/inks, empty-span).
- Phase 36 PR4 (decisions 356–359, gate green 2026-10-01): Render &
  animation v2 — multi-stop `Keyframes` (stops + segment ease +
  once/loop/ping-pong; keyframes win; target closes final leg; stamp
  snaps; DOM stepped inline path); native shadow blur (`blur_radius`
  on `DrawOp::Shadow`; Vello gaussian, CPU box-blur, CSS box-shadow;
  tol-16 < 10% hardware-calibrated; supersedes stepped expansion);
  static images via one cache deposit (CPU/Vello `insert_cached`, DOM
  PNG data-URI; URL keys unchanged); retained `Canvas` (`Tag::Canvas`
  + spec; lowers to Rect/RRect/Path/Text; childless leaf).
- Phase 37a (decisions 360–361, gate green 2026-10-01): Ergonomics —
  `ctx.child_auto`/`child_keyed` on `(caller, ordinal)` (no `TypeId`);
  53 control call-sites migrated (load-bearing manual keys stay);
  `component_manifest!` monomorphized generics (`Name::<A>(P<A>)`,
  canonical symbols; bare/shorthand forms refuse loudly, proven
  end-to-end in a new reload binary).
- Phase 37b (decisions 362–364, gate green 2026-10-01): App services —
  `TaskId` cancellation (parked/queued never run, dependents unblock,
  stage `Cancelled`; running sets a cooperative token) + pluggable
  `Fetcher` (scripted/closure/wasm-binding) with `cancel_fetch`→`Idle`;
  zero-stdout host-level ring diagnostics; signal/collection
  write-through persistence over `KvStore` (seed-once, encode/decode;
  seed warns + initial, writes panic, corrupt snapshots seed empty).
- Phase 37c (decision 365, gate green 2026-10-01): `cargo oppa new`
  scaffolder (embedded templates, package rename, checkout-pinned
  path deps, loud refusals — a generated desktop project compiles
  against the live checkout); templates ride `child_auto`; G9 reload
  recipe on `oppa-reload` rustdoc (+ `app_loop`).
- Phase 38a (decisions 366–367, gate green 2026-10-01): Form
  validation props (G7) — `invalid` / `required` / `error_message` /
  `helper_text` on TextInput, TextArea, Checkbox, Toggle,
  RadioGroup, Select, Slider (announced marks via decision-352
  payloads, `error`-ink borders, footer captions; validators stay
  app-side; valid trees byte-identical); Slider `Home`/`End` jumps
  + `value_num`/`min_value`/`max_value` range announcement (G18)
  over the continuous pointer-capture drag.
- Phase 38b (decisions 368–370, gate green 2026-10-01): New
  controls part 1 — `Tree` (G12: hierarchy over `Collection`,
  `vlist_window` windowing + slot recycling, chevron `Path`s,
  Up/Down/Left/Right/Enter, `Tree`/`TreeItem` roles), `Splitter`
  (G13: `Vertical`/`Horizontal` 2-pane divider, `ColResize` /
  `RowResize`, fraction signal + minima clamping, drag + arrow
  nudge), `DatePicker` (G14: controlled `Date`, `Portal` month
  `Grid` popup + `TextInput` `YYYY-MM-DD` parse bridge, min/max
  bounds on every path, arrow/Home/End day steps).
- Phase 38c (decisions 371–372, gate green 2026-10-01): New
  controls part 2 — `Toolbar` + `Menubar` + `FilePicker` (G21:
  one shared bar recipe — container-owned focus, highlight
  cursor, hit-test activation, Left/Right rove, Enter invokes;
  `Menubar` opens a standalone `Menu` per title with a focus-edge
  dismissal rule; `FilePicker` wraps open/save/folder
  request/poll behind one trigger) + G22 wrappers (`RichText`
  display, `Image` leaf + alt, `Canvas` spec replay, `NavHost`
  stack switch — the Phase 36 leaves as components).
- Phase 38d (decisions 373–376, gate green 2026-10-01): Debt,
  packaging & PWA — `app_data_dir(app_name)` per-platform roots
  (G23: `%APPDATA%` / macOS support dir / `$XDG_DATA_HOME` or
  `~/.local/share`, wasm refuses; root-escape names refuse;
  symlink lexical bound re-documented); G10/G11 cookbook (3
  recipes) + styling reference landed in crate rustdocs (zero new
  `.md`); packaging promotion (G19: Windows release-exe +
  tarball mechanics verified with outputs; rc/cargo-deb absent
  stays manual/open); PWA story (G20: template `manifest.json` +
  `sw.js` + registration flow through `cargo oppa new`, release
  `.wasm` weighed at ~2.87 MB served).
- Phase 39a (decisions 377–379, gate green 2026-10-01): DOM →
  framework value loop (U8) — payload-carrying `InputEvent::Text`
  (Q1) + signal-passed binding (Q3) with platform commits
  reporting through `on_change` (no undo push, caret to end);
  focused-field swap preservation via keyed patches (Q2, zero DOM
  ops on unrelated ticks, value intact); latin-acceptance floor
  with `isComposing` preedit guard + bubbled-key ownership (Q4,
  Q5); loop proven counted per keystroke (framework) + mutation
  counts (DOM) with the M7 browser editing suite unchanged.
- Phase 39b (decision 380, gate green 2026-10-01): Reference apps
  + final audit —   `KitchenSinkApp` gains a Views tab (Tree,
  Toolbar, Menubar, RichText, Canvas, Image, NavHost) with
  validation + DatePicker on Form, Grid + Splitter on Layout, a
  keyed replay-gated keyframes pulse on Overlays (stable `.key()`
  identity — unkeyed order-pairing across tab switches would spawn
  foreign tracks that frozen-clock harnesses cannot settle), and a
  FilePicker on Platform;
  `Task Studio` gains a File/View `Menubar` (under the body —
  the grid keeps its top-anchored row geometry), a grid|inspector
  `Splitter`, empty-title validation, a RichText status, and a
  Canvas done-meter (all E2E debugs preserved, E2E green);
  living-docs + platform audit closes Now/Next in place.
- Phase 40a (decision 381, gate green 2026-10-01): Lint-zero —
  eliminated pre-existing clippy warnings on record (fps_app naming in
  oppa-fps and oppa-fps-demo, DesktopPollHook type alias in oppa-app,
  needless borrows and unused import in oppa-web); clippy gate runs at
  zero warnings across all targets; CONTRIBUTING.md gate note updated.
- Phase 40b (decision 382, gate green 2026-10-01): Verification sweep —
  c3 CDP/Edge re-baseline exercised live against headless Edge
  (`spike/web/harness.mjs`, `compare.mjs`, `dom_text.mjs`, `parity.mjs`,
  `sink.mjs`, `webapp.mjs` all green; `bootstrap.js` state boot-sync);
  Firefox full-leg automation blocked by CDP-only puppeteer harness
  (BiDi/geckodriver tooling absent); PWA load-once-offline hand pass
  blocked by interactive browser session; Safari passes blocked by
  missing Apple hardware.

- Phase 40c (decision 383, gate green 2026-10-01): Studio quiescence & demand
  instrumentation — diagnostic `Runtime::demand_sources()` and `pending_labels()`
  (mirrored on `ComponentHost` along with `has_demand()`); isolated headless
  demand source to `Effect 0` (TaskStudio root) subscribed via `settled_box_by_debug`
  (Scrollbar/Menubar) to `layout.generation`, perpetually dirtied by unshaped text
  re-measure loops when `TextService` is absent; gated `needs_measure` on
  `service.is_some()`; studio achieves quiescence in ≤ 2 frames across bare and
  shaper-backed hosts; bounded-frame studio tests maintained by doctrine.
- Phase 41 (decision 384, gate green 2026-10-01): Custom GPU rasterizer
  (`oppa-gpu`, Deferred Item #1) — pure-Rust backend on standard `wgpu`
  render pipelines (zero compute shaders, zero C/C++ FFI); instanced
  analytical SDF AA quads (sharp rects, 4-corner rounded rects, circles),
  closed-form separable 2D Gaussian erf shadows, dynamic glyph atlas
  (`R8Unorm` via `ab_glyph`), Lyon vector path tessellation, scissor
  clipping, layer opacity, direct swapchain render passes (`present_surface`),
  pixel-oracle tested (<5% shadow tolerance, exact geometry/images) against
  `oppa-cpu`; integrated into `DesktopLoop` in `oppa-app` with CLI/env
  override (`OPPA_RENDERER=gpu*`).
- Phase 41b (decision 385, gate green 2026-10-01): Host-level ImageCache & runner
  image sync — `ComponentHost` and `Ctx` own shared `ImageCache`; `DomBackend`
  syncs host images per commit; `DesktopLoop` (and testkit harnesses) upload
  static pixels into CPU and GPU backends via `insert_cached`;
  `KitchenSinkApp` Views tab deposits into `ctx.images()`; web Views tab crash
  eliminated with full tab roundtrip proven in Edge headless.
- Windows TSF input scope diagnostic cleanup (gate green 2026-10-02): `tsf.rs`
  uninstantiated range property on fresh context logs expected fallback to `IS_TEXT`
  rather than false-alarm `FAILED` error.
- DatePicker unified single-box chrome (gate green 2026-10-02): `TextInputProps::borderless`
  allows text field to render seamlessly inside composite controls; `DatePicker`
  uses borderless inner text field, eliminating double-box visual nesting.
- Source code decomposition (gate green 2026-10-02): Modularized and split high-LOC files
  across workspace adhering to cohesion guidelines (<600 lines target). `crates/oppa-controls/src/lib.rs`
  (11,974 lines $\rightarrow$ 158 lines, 22 dedicated component modules + `tests.rs`), `crates/oppa/src/layout.rs`
  (4,857 lines $\rightarrow$ 1,981 lines, extracted `text.rs`, `grid.rs`, `flex.rs`, `containers.rs`, `tests.rs`),
  `crates/oppa/src/component.rs` (6,013 lines $\rightarrow$ 2,794 lines, extracted `host_input.rs`, `ctx.rs`, `tests.rs`),
  `crates/oppa-app/src/lib.rs` (3,386 $\rightarrow$ 1,436 lines), `crates/oppa-app/src/windows.rs`
  (2,447 $\rightarrow$ 1,385 lines), `crates/oppa-shell-win/src/win.rs` (2,111 $\rightarrow$ 1,528 lines),
  `crates/oppa/src/editing.rs` (3,019 $\rightarrow$ 1,990 lines), `crates/oppa/src/store.rs` (1,508 $\rightarrow$ 939 lines),
  `crates/oppa/src/fetch.rs` (1,019 $\rightarrow$ 164 lines), `crates/oppa/src/layout/flex.rs`
  (1,148 $\rightarrow$ `flex.rs` 405 + `flex_wrap.rs` 325 + `flex_vertical.rs` 424),
  `crates/oppa/src/component/host_input.rs` (1,419 $\rightarrow$ hub 64 + `host_scroll.rs` 314 +
  `host_timers.rs` 102 + `host_pointer.rs` 295 + `host_pointer_state.rs` 314 + `host_key.rs` 163 +
  `host_focus.rs` 192), `crates/oppa-shell-linux/src/dbus.rs` (1,397 $\rightarrow$ hub 34 + `sig.rs` 109 +
   `value.rs` 154 + `parse.rs` 137 + `msg.rs` 224 + `transport.rs` 367 + `tests.rs` 398),
   `crates/oppa-dom/src/dom.rs` (2,589 $\rightarrow$ 2,386 lines, extracted `dom/tests.rs`),
   `crates/oppa-macros/src/lib.rs` (1,689 $\rightarrow$ 1,427 lines, extracted `tests.rs`),
   `crates/oppa/src/vnode.rs` (1,445 $\rightarrow$ 1,278 lines, extracted `vnode/tests.rs`),
   `crates/oppa/src/style.rs` (1,195 $\rightarrow$ 1,026 lines, extracted `style/tests.rs`),
   `crates/oppa-cpu/src/builder.rs` (1,569 $\rightarrow$ 1,272 lines, extracted `builder/tests.rs`),
   `crates/oppa-shell-linux/src/file_dialog.rs` (1,815 $\rightarrow$ 1,244 lines, extracted `file_dialog/tests.rs`),
   `crates/oppa-macros/src/lib.rs` (1,427 $\rightarrow$ 151 lines, extracted `props.rs` 206 +
   `scan.rs` 196 + `manifest.rs` 398 + `hot_crate.rs` 85 + `component_lint.rs` 399; shared
   byte-scanners in `scan.rs`, proc-macro entries stay at the root),
   `crates/oppa-shell-linux/src/file_dialog.rs` (1,244 $\rightarrow$ 399-line hub +
   `portal.rs` 643 + `zenity.rs` 232; worker spawn as `PortalWorker::spawn`,
   shared `single_result` stays hub, all public paths preserved via re-exports),
   `crates/oppa/src/layout.rs` (1,980 $\rightarrow$ 355-line hub + `primitives.rs` 251 +
   `scrollbar.rs` 116 + `measure.rs` 146 + `engine.rs` 350 + `node.rs` 461 + `leaf.rs` 343;
   `LayoutCtx` methods follow the existing per-file-impl precedent, paths preserved via re-exports),
   `crates/oppa/src/component.rs` (2,797 $\rightarrow$ 2,377-line hub + `store.rs` 136 +
   `images.rs` 97 + `host_reload.rs` 201; `component::Store`/`ImageCache` paths preserved
   via re-exports, `SiteKey` + instance backends widened to `pub(crate)`),
   `crates/oppa/src/editing.rs` (1,991 $\rightarrow$ 83-line hub + `session.rs` 344 +
   `caret.rs` 844 + `edit_ops.rs` 334 + `platform.rs` 185 + `geometry.rs` 235;
   `editing::` paths preserved via re-exports, shared state/helpers `pub(crate)`),
   `crates/oppa-dom/src/dom.rs` (2,386 $\rightarrow$ 1,700-line hub + `element.rs` 386 +
   `render.rs` 313; element model + serialization split out, paths preserved via re-exports),
   `crates/oppa-shell-win/src/win.rs` (1,528 $\rightarrow$ 588-line hub + `events.rs` 488 +
   `shell_platform.rs` 461; message vocabulary vs trait impl, paths preserved via re-exports),
   `crates/oppa-app/src/lib.rs` (1,436 $\rightarrow$ 315-line hub + `gpu.rs`/`wiring.rs`/
   `step.rs`/`frame.rs`/`resize.rs`/`text_input.rs`, all $<300; method groups move verbatim),
   `crates/oppa-app/src/windows.rs` (1,385 $\rightarrow$ 510-line hub + `gpu.rs`/`drive.rs`/
   `ime.rs`/`window.rs`; `pub(crate)` submodules, test-only helpers use deep paths),
   `crates/oppa/src/reactive/mod.rs` (1,588 $\rightarrow$ 194-line hub + `runtime.rs`/`batch.rs`/
   `tasks.rs`/`input.rs`/`passes.rs`/`scheduler.rs` 453 + `signal.rs`/`memo.rs`/`effect.rs`;
   run engine stays one cohesive module, paths preserved via re-exports),
   `crates/oppa/src/component.rs` (2,208 $\rightarrow$ 640-line hub + `host_mount.rs` 317 +
   `host_binding.rs` 386 + `host_query.rs` 275 + `host_frame.rs` 145 + `host_services.rs` 469;
   `MountHandle` re-exported, 3 callees widened to `pub(super)`),
   `crates/oppa-dom/src/dom.rs` (1,638 $\rightarrow$ 239-line hub + `scroll.rs` 55 + `sync.rs` 393 +
   `patch.rs` 322 + `encode.rs` 446 + `commit.rs` 209; helpers `pub(crate)`, public paths via hub re-exports),
   `crates/oppa-cpu/src/builder.rs` (decision 402, 1,272 $\rightarrow$ 333-line hub + `node.rs` 385 +
   `shape.rs` 251 + `leaf.rs` 184 + `resolve.rs` 109; node dispatch vs fills/rings/bands/gradients/shadows
   vs path/canvas payload leaves vs style/ink/edge queries, `builder::node_styles` preserved via hub
   re-export, cross-module callees `pub(super)`),
   `crates/oppa/src/vnode.rs` (decision 404, 1,179 $\rightarrow$ 225-line hub + `element.rs` 347 +
   `text.rs` 258 + `media.rs` 369; core tree types vs chainable element builders/factories vs text leaves
   vs payload leaves, every `vnode::` path preserved via hub re-exports, `auto_handler_id` widened to
   `pub(super)`, test-only `Color`/`FontWeight` use deep paths),
   `crates/oppa-app/src/tests.rs` (decision 405, 2,283 $\rightarrow$ 177-line hub + `basic`/`editing`/`paint`/
   `services`/`scroll`/`ime`/`win_ime`/`dpr_gpu`/`repaint`/`win_dpi` test modules, all $<400; shared
   FakeText + field/flip rigs + key helpers stay hub, groups `use super::*` with test-only deep-path
   imports where the hub glob does not reach),
   `crates/oppa-controls/src/tests.rs` part 1 (decision 406: hub + `buttons`/`fields`/`overlays`, ~1,160
   lines out; shared FakeText + measure/press/lookup helpers + `settle_clocked` stay hub, `ToastCase`
   rides with its toast tests, groups carry test-only deep-path imports),
   part 2 (decision 407: + `pickers`/`interaction`/`bindings`; shared `tab` key helper + `change`/`blowing`
   reporters and the check/toggle/slider render rigs move to hub with later regions, all call sites
   unchanged),
   part 3 (decision 408: + `pointer`/`chrome`/`lists`/`scrollbars`/`attached`/`feedback`/`validation`/
   `widgets`/`bars`/`showcase` — `tests.rs` fully decomposed to hub + 16 modules, all $<600; shared
   tap/hover/drag gestures, list/grid rigs, scroll rigs and `ChromePair` move to hub with later regions),
   `crates/oppa-cpu/tests/m4_cpu.rs` (decision 410, 1,124 → 225-line hub + 13 area modules under
   `tests/m4_cpu/` via `#[path]` — plain `mod` beside a test-target root resolves as siblings, not
   into the subdirectory; shared `FakeText` + card `Rig` + color probes stay hub, groups
   `use super::*`, `dwrite` via file-level `#![cfg(windows)]`, `rimg_refused` rides with its
   image tests; same 20 tests, same outcomes — dirty 11/9, clean 20/20),
   `crates/oppa-cpu/src/backend.rs` (decision 411, 1,124 → 384-line hub + `raster.rs` 404 +
   `replay.rs` 368; callees widened to `pub(super)`, call sites qualified, every `backend::`
   public path preserved; fn-name set 39/39 identical, per-suite baseline-identical),
   `crates/oppa-shell-win/src/tsf.rs` (decision 412, 1,118 → 412-line hub + `store.rs` 144 +
   `text_store.rs` 558 + `sinks.rs` 41; state/structs `pub(super)`, trait-impl methods bare,
   `tsf::{TsfBridge, TsfStatus}` paths preserved; fn-name set 50/50 identical, slices
   verified line-identical, per-suite baseline-identical).
  Cross-module callees widened to `pub(super)` (component / dbus subtrees only, no public API change);
  `crate::dbus` paths preserved via hub re-exports (`connect_unix`/`connect_abstract` stay `#[cfg(unix)]`;
  test-only `MSG_METHOD_CALL`/`parse_address` import from their submodules). All public APIs preserved 100% via re-exports;
  all workspace quality gates green (fmt, clippy, wasm, tests).
- Cross-backend debug performance overlay (gate green 2026-10-02): `DebugOverlayConfig` and
  `OverlayMeter` implemented at `ComponentHost` root level in `crates/oppa/src/debug_overlay.rs`;
  tracks rolling EMA FPS, frame delta ms, dirty node count, and presenter badge (`[CustomGpu/API]`, `[CPU]`, `[DOM]`);
  flags individual boolean toggles, disabled by default (zero-cost);
  configured via `WindowOptions::with_debug_overlay` / `with_fps_counter` or `OPPA_DEBUG_FPS=1` env var;
  works across every presenter backend uniformly; all quality gates verified green.
- Uncapped continuous repaint (decision 386, gate green 2026-10-02):
  `WindowOptions::with_continuous_repaint(bool)` (default `false`) +
  `DesktopLoop` getter/setter, `OPPA_CONTINUOUS_REPAINT=1` / `OPPA_BENCHMARK=1`
  env override; armed pump horizon `Some(0)` (Windows wait + Linux Poll) with
  repaint + `request_frame` re-arm after every present on all desktop backends
  (Custom GPU, CPU), overlay on or off; Android/Web stay display-paced.
  Overlay FPS clock ticks on presents (never reconciles — no 0 FPS after idle
  gaps); badge text refreshes through a throttled 4Hz root re-run.
- Vello removed, single GPU backend (decision 388, supersedes ADR-0009 buy-the-rasterizer): `oppa-vello` + `oppa-fps`/`oppa-fps-demo` deleted; single GPU twin selected by `OPPA_RENDERER=cpu|gpu|gpu-auto|gpu-vulkan|gpu-dx12|gpu-gl` (retired `vello*` spellings refuse loudly into auto); badge `Gpu/API` names the recorded winner; `oppa-dom` oracle tests + Android harness ported (`ensure_gpu_gles`/`ensure_gpu_vulkan`/`render_noread` added to the backend); SPEC oracle + ARCHITECTURE + platform pages updated in the same change. Decision 389 drops the `custom` qualifier (`CustomGpuBackend`→`GpuBackend`, flags `gpu-*`, retired `custom*` spellings refuse loudly into auto).

- Android proof-app benchmark mode + FPS badge (decision 393, gate green 2026-10-03): `DEBUG_BADGE` / `BENCHMARK_PHASE` flags both on — overlay armed before mount (env unsettable under NativeActivity), `Gpu/GL` / `Gpu/Vulkan` published at swapchain bring-up, bounded present phase uncapped (non-blocking poll, step + present + badge tick every frame, negotiated Immediate→Mailbox→Fifo with loud Fifo fallback — the emulator translator offers Fifo only); interactive phase stays event-paced Fifo. Release packaging: `cargo --release` both ABIs (~12 MB .so), Gradle stages `release/` + `assembleRelease` signed with the dev debug key (8.1 MB APK), manifest `debuggable` unhardcoded (Gradle debug auto-assigns; aapt2 path takes the link-time debug-mode flag). Proven live on Medium_Phone_API_36.1 (`29 FPS | 38.9ms | 1 dirty | [Gpu/GL]` badge on screen). Gates: fmt + Android-target clippy green (`cargo test` unbuildable on host by construction — `ndk-sys` refuses non-Android; the on-device oracle run is the test).
- Full-bleed window (decision 396, gate green 2026-10-03): `OppaUi.hideBars` sets `LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES` (default mode letterboxed below the camera strip) + `wait_for_window` polls to size-settle instead of freezing the first sighting. Proven live on the RMX3370: scene renders full-bleed 1080x2400 (only the debug badge's own dark pill remains at top-left).
- Overlay idle-gap skip (decision 395, gate green 2026-10-03): `note_frame` resyncs past 1s gaps instead of dividing by them (event-driven taps printed `0 FPS` after idle, against the stated doctrine). Proven live on the RMX3370: tap → 30s idle → tap shows a held live rate (`35 FPS | 4.0ms`) instead of gap-poisoned `0 FPS`. Unit: new hold-rate test; all 9 overlay tests + fmt + clippy green.
- Android tap-path theme sync (decision 394, gate green 2026-10-03): `apply_tap_batch` syncs the builder theme every build + refits the page-background surfaces on toggle (desktop `frame.rs` rule via a shared `rebase_surfaces` core; blank-guard compares the live bg); the step returns the live GPU scene id (a rebase re-homes surfaces mid-loop — presenting the call-time id aborted the loop and froze the app). A dp-passthrough coordinate experiment was tried and reverted (taps regressed); corrected diagnosis: the router hit-tests device px (`LayoutBox` commits device px), so the px→dp→px round trip is correct spacing — proven by on-device event-vs-track-box comparison with accurate flips. Proven live on the RMX3370 phone (Vulkan row): uniform dark scene after toggle (near-black page bg, themed controls + text), loop alive after (`66 FPS [Gpu/Vulkan]`), taps keep processing.
- Unified app-loop engine (decision 397, gates green 2026-10-03): the platform-free pixel pipeline now lives once in `oppa-app::SceneCore` (host + `FramePlanBuilder` + both paint arms + surfaces, diff replay, selection/caret overlay publish, theme/page-bg contract with surface refit, resize/DPR rebase, GPU bring-up state + present-mode negotiation, static-image deposit sync, plan/caret telemetry). `DesktopLoop` composes it behind the unchanged public API (moved bodies are verbatim, error wording preserved — tests grep it) and `oppa-android-app` drives it: `SceneState` collapsed to one `SceneCore` (its private re-implementations — the plan-build/paint ladder, `rebase_surfaces`, `sync_page_bg`, `straight_rgba`, `pick_uncapped_mode`, density rebase — deleted; presents address the engine's live GPU surface id, eliminating the stale-id threading; `enable_gpu_pinned` keeps zygote-inherited env out of the arm decision). The merge also fixes the Android pipeline's missing static-image `insert_cached` sync (decision 385 rode desktop only). Web stays presenter-owned (locked #1). Gates: host `cargo check --workspace --all-targets` + clippy + fmt --all --check green; `cargo test -p oppa-app --lib` 44/31 — identical to the refactoring baseline (the 31 reds pre-date this change, see Now); Android target `cargo +stable check/clippy --target aarch64-linux-android --all-targets` green (host compile impossible — `ndk-sys` refuses non-Android; on-device oracle run remains the test, still blocked on hardware).
- Presenter-adapter split (decision 398, gates green 2026-10-03): the
  decision-397 `SceneCore` split along the locked-#1 seam — the shared
  loop logic (diff replay into presenters, overlay publish, theme
  publish + page-bg refit trigger, plan build, image-sync trigger, step
  ladder, resize/DPR rules, plan/caret telemetry) lives in
  `oppa-app::SceneSession` behind the tiny `ScenePresenter` trait,
  with `RgbPresenter` (CPU arm + GPU twin, surfaces, refit/rebase,
  image deposits, present, readout) as the pixel adapter behind the
  unchanged `SceneCore` facade, AND the web loop driving the same
  session through a `DomPresenter` adapter in `oppa-web`
  (DOM sync/patch/render stay crate-local, not on the trait); wgpu/
  oppa-gpu/raw-window-handle and the pixel modules are
  `cfg(not(wasm32))`, so no surface/wgpu/readout path can compile into
  `oppa-web`. No behavior change — all suites identical to the
  recorded pre-refactor baseline (red counts unchanged).
- `taps.txt` clobber fix (decision 399, Android-target check/clippy
  green 2026-10-03): the bounded + interactive end-of-run whole-file
  `tap_log` writes overwrote the incremental per-batch `cmds/density`
  oracle lines — one `write_taps_file` helper now carries both
  (oracle lines first, run-log summary below, each layout unchanged,
  no new output path for host pulls). Host `cargo test` unbuildable
  on host by construction (`ndk-sys` refuses non-Android); no
  behavioral test possible on host — the on-device oracle run is the
  test (no device attached; shape verified by code review only).
- Benchmark seam replaces the engine escape hatches (decision 400,
  Android-target check/clippy green 2026-10-03): `frameloop.rs`
  measures through five narrow `SceneCore` methods
  (`ensure_benchmark_gpu_gles/vulkan`, `benchmark_gpu_pixels/noread`,
  `benchmark_cpu_frame`) — same scene, same pieces, live surface ids
  re-read every frame; the `parts_mut`/`gpu_backend_mut` accessors are
  removed with no callers left. No host behavior change; on-device
  numbers prove on device (no device attached).
- Per-adapter diff-cursor policy recorded (decision 401, prose-only
  2026-10-03): the pixel adapters replay `diffs_from(0)` every paint
  (fresh-surface correctness), the DOM adapter walks a committed
  cursor (touched-count patch semantics) — the invariant now lives in
  `docs/SPEC.md` Runner (decision 398 paragraph); no code change (the
  `ScenePresenter::commit`/`paint` adapters already carried it).
- `repaint` returns the adapter's frame metric (decision 409, gates
  green 2026-10-03): `SceneSession::repaint` returned the plan's
  damage-rect count for every adapter while the `ScenePresenter::paint`
  contract promises damage for pixels and touched-work for DOM — so
  damage-but-untouched DOM frames (background presses, converged value
  re-feeds) fell into the full-page fallback and reported work that
  never touched the page. One line at the contract point
  (`Ok(pres.paint(...))` instead of `Ok(damage)`); pixel legs provably
  identical (`RgbPresenter::paint` returns exactly the old value).
  Both clean-env web failures green; full workspace per-suite identical
  to the locked baseline otherwise.
- Headless `tap_log` persistence (decision 403, Android-target
  check/clippy green 2026-10-03): `drive_headless_taps` pushed
  per-batch `batch=N t=…ms cmds=… density=…` timing lines into the
  run-log that its end-of-run write never persisted (oracle lines
  only) — the headless write now routes through the decision-399
  `write_taps_file` helper (oracle records first, timing log appended
  below, same shape, no new output path), and the caller re-persists
  best-effort after the `interactive skipped (no window)` note (the
  windowed arm's `let _` rule — bonus-phase bookkeeping never fails
  the banked proof). Host `cargo test` unbuildable by construction
  (`ndk-sys` refuses non-Android); no behavioral test possible on
  host — the on-device oracle run is the test (no device attached;
  shape verified by code review only).

## Now (in progress)

- Debt round 2 OPEN (decisions 410–412 landed: `m4_cpu` tests → hub + 13 area modules,
  `oppa-cpu/backend.rs` → hub + `raster`/`replay`, `oppa-shell-win/tsf.rs` → hub +
  `store`/`text_store`/`sinks`; every shared rig in hub, every public path
  preserved, full workspace per-suite identical to the locked baseline after each commit;
  remaining >1000-line files, smallest first:
  `oppa-shell-linux/input.rs`, `m9_reload_gate` tests, `m3_layout` tests, `m7_dom` tests last;
  spike rig excluded).
- Debt + attribution round CLOSED (decisions 402–409: `builder.rs`,
  `vnode.rs`, `oppa-app` tests, `oppa-controls` tests fully decomposed
  to hub + responsibility modules, every public path preserved;
  headless `tap_log` persistence; 23-target attribution — 21 rig
  artifact, 2 genuine web gaps, all recorded below).
- Web-gap close-out complete (decision 409): both env-independent
  failures shared one root cause — `SceneSession::repaint` returned
  plan-damage count for every adapter while the trait contract promises
  the adapter's frame metric (damage for pixels, touched-work for DOM),
  so damage-but-untouched DOM frames fell into the full-page fallback
  (background clicks and converged value re-feeds reported work that
  never touched the page). One-line fix at the contract point, zero
  behavior change on the pixel legs (`RgbPresenter::paint` returns
  exactly the old value); both web suites green, full workspace suite
  per-suite identical to the locked baseline otherwise.
- Loop-doctrine close-outs complete (decisions 399–401 shut the
  decision-398 follow-ups: `taps.txt` keeps batches with the summary
  appended, `frameloop.rs` measures through the engine benchmark
  seam, diff-cursor policy recorded). Workspace suite red counts
  unchanged by the 397–401 rounds (31 in `oppa-app --lib`, 14 in
  `oppa` g11_touch — all failing identically before these rounds'
  edits, mid-flight core work in the tree).

## Next (accepted, not started)

Ordered productization P1s: all closed — G7–G23 done through
Phase 39b (catalog, cookbook/styling, data-dir, installers, PWA,
value loop, reference apps).

Accepted-carry: weak-GPU sustained-cost standing rule.

Accepted-carry: file-size follow-ups all closed (`oppa-cpu/builder.rs`
by decision 402, `vnode.rs` by decision 404, `oppa-app` tests by decision 405,
`oppa-controls/tests.rs` by decisions 406–408).

## Blocked (with blocker)

| Item | Blocked by |
|---|---|
| Android on-device sustained/thermal numbers + GPU re-proof (prior numbers Vello-measured) | No phone attached; emulator walls mapped |
| Live human passes (IME composition manual, theme flips, menu/tooltip eyeball, close-veto app) | Needs a person at the keyboard |
| macOS/iOS shells | No Apple hardware; excluded by design |
| Weak-tier silicon validation (Mali-G52/Adreno-610 class) | Needs hardware wall |
| Firefox full-leg automation | Rig limitation: spike/web puppeteer harness is CDP-only; Firefox lacks CDP endpoint without BiDi/geckodriver tooling |
| PWA load-once-offline hand pass | Needs an interactive browser session |
| Safari pass | No Safari binary or Apple hardware on this machine |
| Multi-window `DesktopLoop` (secondary `SurfaceId` windows) | Deferred by plan; per-surface granularity is out — re-decision required to revive |
| Variable-height virtualized rows (prefix-sum index) | Deferred by plan — re-decision required to revive |
| `cargo oppa reload-check` as a G9 substitute | Unlisted extra scope, not approved — G9 is the doc recipe |

## Known issues (not code bugs unless noted)

- Environmental flakes: `oppa-shell-win` live-Shift sampling; sandboxed clipboard returns NULL handle with `ERROR_SUCCESS` (loud skip); transient clipboard box contention in full-suite runs.
- Stated edges: ZWJ-inside-regional-indicator double-click selects the joiner alone (pathological); right-held single-gesture drag-select stays out (tap-to-open).
- Standing debt: hot-reload true unload is v2 scope; inline-child effect precision stops at the window (open architecture question); headless TaskStudio quiescence resolved (decision 383 — gated unshaped text re-measure); full studio tests remain bounded-frame by doctrine; Dark-OS web boot shows the static light body until the first patch carries the theme stanza (settled pages take nothing — body heals on first interaction).
- Deliberate non-goals (not debt): fixed decorative literals, `SELECTION_FILL`, OS title-bar theming; right-held menu drag-select; color-emoji rendering.
- Red-suite attribution (locked `target/baseline-401-final.log` — normalized multiset identical to
  `target/baseline-398-HEAD.log`; attribution only, zero behavior changed to make anything pass):
  recording-shell env artifact (rig artifact) explains 21 of the 23 red targets. The recording shell
  exports `OPPA_DEBUG_FPS=1` + `OPPA_BENCHMARK=1` + `OPPA_CONTINUOUS_REPAINT=1` +
  `OPPA_RENDERER=custom_gpu` (retired spelling, decision 389 → parses to auto GPU-first). `OPPA_DEBUG_FPS=1`
  arms the debug overlay in every default config, so every mounted scene carries the badge layer
  (`host_mount.rs:219-244`: pill `RRect` at 8,8 radius-4 `0x1E1E1E` @0.88 + `0x00FF66` ink + cold-meter
  `60 FPS | 16.6ms | N dirty` spans — values never presented, only mounted); the benchmark flags force
  uncapped repaint at runner startup. Proven by removal: `m4_cpu` 11p/9f → 20p/0f with the four vars
  unset (`target/probe-m4cpu-clean.log`); full workspace with a clean env: 21 red targets green, 0 newly
  red (`target/probe-cleanenv-full.log`, 99 ok + the 2 web targets below — after decision 409 the same
  clean-env run is 101/101 green, `target/round-409-cleanfull.log`). Dirty→clean: g11_touch 8/14,
  m3_layout 49/1, m5_input 9/7, m8_virtualization 8/2, oppa-app lib 44/31, task_studio_e2e 0/1, atspi_emit
  1/1, oppa-controls lib 104/33, oppa-cpu lib 12/1, m4_style_fx 6/6, m5_toggle 0/4, vector_path 4/1,
  m7_dom 32/18 (both browser rows incl.), m8_transitions 5/1, m9_reload_gate 5/1, real_dylib 0/1,
  android_contract 2/4, testkit lib 7/4, keyboard_text 4/1, uia_emit 0/2 — all green clean. Standing rule:
  suite runs must use a clean env; with the benchmark vars set, red counts measure the shell, not the
  tree. (Full-suite runs also rewrite the tracked browser-rig outputs under `spike/results/` — run
  artifacts, restored before committing, never committed.)
  - Genuine gaps, env-independent — both FIXED by decision 409 (shared
  root cause, not two bugs): `oppa-web --lib` `tests::toggle_flips_through_binding_calls`
  (`lib.rs:563` — background click returned a full-page reload instead of miss-is-None) and
  `kitchen_sink_web` `focused_field_survives_background_updates` (`kitchen_sink_web.rs:394` —
  converged re-feed yielded a patch instead of quiet-None). Both were damage-but-untouched DOM frames
  (`repaint` returned plan-damage as touched-work, tripping the full-page fallback); both green after
  the one-line contract fix, full suite otherwise baseline-identical.
