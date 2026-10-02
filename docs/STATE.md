# Project state (current)

Status: current. Last verified: 2026-10-02 (through Decision 385).
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
  override (`OPPA_RENDERER=custom_gpu`).
- Phase 41b (decision 385, gate green 2026-10-01): Host-level ImageCache & runner
  image sync — `ComponentHost` and `Ctx` own shared `ImageCache`; `DomBackend`
  syncs host images per commit; `DesktopLoop` (and testkit harnesses) upload
  static pixels into CPU, Vello, and CustomGpu backends via `insert_cached`;
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
   `shell_platform.rs` 461; message vocabulary vs trait impl, paths preserved via re-exports).
  Cross-module callees widened to `pub(super)` (component / dbus subtrees only, no public API change);
  `crate::dbus` paths preserved via hub re-exports (`connect_unix`/`connect_abstract` stay `#[cfg(unix)]`;
  test-only `MSG_METHOD_CALL`/`parse_address` import from their submodules). All public APIs preserved 100% via re-exports;
  all workspace quality gates green (fmt, clippy, wasm, tests).

## Now (in progress)

- Phase 41 complete (decisions 384–385 green). Awaiting user selection of next deferred item (Items #2–#9).

## Next (accepted, not started)

Ordered productization P1s: all closed — G7–G23 done through
Phase 39b (catalog, cookbook/styling, data-dir, installers, PWA,
value loop, reference apps).

Accepted-carry: weak-GPU sustained-cost standing rule.

Accepted-carry: file-size follow-ups (`oppa-controls/tests.rs`, `oppa-dom/dom.rs`,
`reactive/mod.rs`, `vnode.rs`, `oppa-cpu/builder.rs`, `oppa-vello/backend.rs`,
`oppa-macros/lib.rs`, `oppa-app` tests) stay over the hard 1000-line limit —
same mechanical split pattern applies.

## Blocked (with blocker)

| Item | Blocked by |
|---|---|
| Android on-device sustained/thermal numbers | No phone attached; emulator walls mapped |
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
- Standing debt: hot-reload true unload is v2 scope; inline-child effect precision stops at the window (open architecture question); headless TaskStudio quiescence resolved (decision 383 — gated unshaped text re-measure); full studio tests remain bounded-frame by doctrine.
- Deliberate non-goals (not debt): fixed decorative literals, `SELECTION_FILL`, OS title-bar theming; right-held menu drag-select; color-emoji rendering.
