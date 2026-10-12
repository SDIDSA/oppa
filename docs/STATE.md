# Project state (current)

Status: current. Last verified: 2026-10-12 (through Decision 482).
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
- Portable login demo (gate green 2026-10-05): shared `oppa-controls::login`
  (`LoginApp`/`login_root` + pure `email_error`/`password_error` — signals,
  `TextInput` validation marks, `Button` only; no `cfg`, no platform imports)
  mounted unchanged by desktop `examples/login.rs` (`run_desktop`: Windows +
  Linux), web `WebApp::new_login` (`web/login.html`, same bindings as the
  sink bootstrap), and the Android `SceneCore`; headless proofs
  `crates/oppa-testkit/tests/login_page.rs` 3/3 + `crates/oppa-web/tests/login_web.rs`
  3/3 (rules, empty/valid submit, kind-change retag on the wire); Edge proof
  `spike/web/login.mjs` PASS (boot, empty-submit errors, typed valid login,
  welcome, zero console errors). Found and fixed a genuine web gap on the
  way: validation footers reuse the field's node id with a new element kind
  (input→div), which the patcher expressed as in-place attrs + child places
  — unrepresentable in the DOM (void-input parenting hid the bound leaf and
  deadened the visible shell). `take_patch` now swaps kind-changed subtrees
  whole (focal pass heals focus); the four bootstrap appliers replace (never
  duplicate) blobs colliding with live pids.
- Standard web run path (gate green 2026-10-05): web is one
  command now — `cargo oppa serve` (pinned wasm build +
  CLI-pinned wasm-bindgen + no-store serve + browser open, loud
  refusals for skew/busy ports); apps name their root once via
  `oppa_web::export_app!` (8-method binding shell generated) and
  every page boots through the one shared `web/oppa-bootstrap.js`
  (`bootOppa`: keyed applier, events, rAF, boot theme chrome via
  `WasmHost::theme_chrome`, opt-in PWA) — the four drifting
  bootstrap copies are deleted, the template ships the shared file
  inline-booted from `index.html`, and the template's bindgen pin
  mirrors the workspace lockfile by test. Proven end to end:
  fresh scaffold → serve → Edge boot + counter clicks, zero
  console errors; demo/sink/login rigs all PASS on the shared
  module; full workspace 78 suites green.
- App CLI verbs (gate green 2026-10-05): `cargo oppa
  check|build|run` with exactly one of `--desktop|--web|--android`
  (`serve` stays as `run --web`); `check` validates layout + host
  and wasm checks, `build` emits artifacts, `run` launches
  (desktop `cargo run`, web serve+open, Android loud refusal:
  no per-app pipeline, no device). Shells collapsed to macros:
  `oppa_app::desktop_main!` twinning `export_app!` (template +
  `apps/login` entries are one line each); `serve` builds `--lib`
  and resolves workspace target/lockfile via `cargo metadata`.
  Android rides the same verbs: `check` covers both ABIs (no NDK
  needed — check never links), `build --android` NDK-links per-ABI
  `.so` files into `android/libs/` (SDK/NDK/adb discovered, never
  assumed; `llvm-readobj`-proven `EM_AARCH64`/`EM_X86_64`), and
  `run --android` builds then names the staged binaries in its
  loud no-device refusal (APK/install/launch stay manual until a
  device proves a generic deploy).
- Android soft keyboard end-to-end (gate green 2026-10-05,
  RMX3370 on-device): tap focuses the field, the shell
  focus policy fires, the proxy `EditText` attaches lazily, the
  keyboard shows on the proxy, commits insert through the
  focused session, validation + submit reach `Welcome, ada@x.co!`.
  Fixed en route: window focus never fed (`has_focus` gate shut),
  proxy never attached, `loadLibrary` in a pathless `DexClassLoader`
  (`UnsatisfiedLinkError` — removed, `RegisterNatives` binds
  directly), per-call loader instances unbinding natives (one
  shared loader), commits panicking in `to_input_event` (session
  insert + quiet miss instead), `log_java_exception` diagnostics.
  Manual-dismiss re-show: taps inside the focused field force-show
  unconditionally (a layout-listener edge was tried first — deaf
  under the fullscreen adjustNothing window, reverted with its
  manifest flag).
- Android hardware keys (gate green 2026-10-05, emulator
  on-device): DEL→Backspace + forward-del→Delete map in the shell
  classifier (decision 243 — runners route, router stays quiet);
  the runner routes Backspace/Delete/arrows to the focused session
  (Up/Down multiline-guarded, mirroring desktop step parity).
  Proven: `ab` + DPAD-left + forward-del → `a`.
- Android start-time optimization (gate green 2026-10-05,
  RMX3370/API 31 on-device): cold launch 1452 ms → ~190 ms
  window / ~750 ms content. Staged font injection (Roboto+Symbols
  at boot for first paint, 222 remaining faces after first
  present — CJK-first paints keep the eager proof path),
  parallel font thread over the window wait, boot placeholder
  theme (`OppaBoot` window background via generated res/ +
  `--auto-add-overlay` link), per-stage `oppa-boot` logcat
  timings. Deferred faces land ~200 ms post-pixels on the same
  thread (taps queue in the OS, never lost).
- Per-app release APK (gate green 2026-10-05): `cargo oppa build
  --android` assembles a signed release APK — generic
  `oppa-android-runner` crate (verbatim-moved touch/fonts/IME/
  immersive modules + simplified event-paced present loop over
  `SceneCore`, one-line `android_main!` entry), generated shell
  (`AndroidManifest.xml` + package-rewritten java, missing-only),
  aapt2/javac/d8/zipalign/apksigner pipeline with a dev keystore.
  Proven host-side on `apps/login`: release 15 MB / debug 317 MB
  (desktop twins: 8.1 MB release / 19 MB debug exe — same variant
  contract everywhere: release strips native code via llvm-strip
  and links without `--debug-mode`; `--variant debug`, aliased by
  `--dev`, keeps full symbols with the debuggable flag — xmltree
  proven both ways), align + signature + badging green (package,
  both ABIs, launchable NativeActivity), `android_main` exported.
  On-device boot proven on RMX3370 (login renders + interacts —
  runner out of Blocked); `run --android` names the APK with the
  manual `adb install` lines.
  Web shells generate at launch, cached: `oppa-bootstrap.js`
  always syncs to the embedded canonical copy (byte-compared),
  `index.html`/`manifest.json`/`sw.js` generate from
  `export_app!` + package name only when missing (present files
  stay hand-owned; stale stems refuse) — proven by deleting
  `apps/login/web/` and relaunching (regenerates + Edge boots
  with chrome, zero errors).
- Vsync-first presents (gate green 2026-10-05): `pick_gpu_mode`
  negotiates Fifo → Mailbox → Immediate (event-paced UI no longer
  presents uncapped — no tearing, no wasted GPU); benchmarks take
  the new `pick_uncapped_mode` (previous order, proof benchmark
  leg repointed, behavior preserved). Pure preference fn tested
  both directions.
- Steady FPS badge (gate green 2026-10-05): the overlay meter
  publishes whole-second windowed FPS/ms while presents flow (badge
  steps ≤1 Hz on uncapped jitter instead of flickering hundreds)
  and falls back to the fast-EMA hold for sparse taps (decision
  395 preserved); idle skips drop the partial window (no near-zero
  publishes across gaps); ms always agrees with FPS.
- Pretty login (gate green 2026-10-05): `apps/login` redesigned
  with framework vocabulary only (gradient page, surface card with
  radius/border/shadow, monogram, themed controls — zero app
  branching, tokens carry both palettes); logic, validators, and
  debug names untouched. Proven on desktop dark + emulator light
  from the same file.
- Sera-style sign-in example (2026-10-11): `apps/sera-login` replicates a
  dark dental sign-in page (wordmark top bar, saved-login shortcuts |
  email/password split card, footer) with framework vocabulary only
  (`Row`/`Column`/`Div`/`Text`, `TextInput` validation marks,
  `Checkbox`, `Button`; page/card chrome as literals, forced-dark root
  so themed controls match); accepted approximation: field/system glyphs
  render as DejaVu-covered ASCII (no icon slots / color emoji in v1 —
  stated in the module docs). Portable like `apps/login` (desktop
  `run_desktop`, web `export_app!`, Android `android_main!`); headless
  `tests/sera_page.rs` 4/4 (row fills email, delete drops one, empty /
  valid submit); `cargo check` / `clippy --all-targets` zero warnings /
  `rustfmt --check` green on the new package. Fix 2026-10-11: the
  shortcut rows first shipped as `Div` roots (block-lite stacks the row
  vertically with overflowing glyphs) — roots and all centering boxes
  are `Row`s now, matching the horizontal reference. Input-precision
  pass 2026-10-11: near-black 48px chrome owns bg/border/radius/pad
  (themed `#1E1E1E`/`#555` was warm and rough against the card),
  hand-drawn vector envelope/lock `Path` icons, borderless inner
  fields, label/field groups at 6px rhythm, manual footers under the
  chrome (a footer inside the horizontal box would flex-squeeze) with
  the chrome edge going error-red while `invalid`/`required` still
  announce. Measured-palette pass 2026-10-11: every literal sampled
  off the reference pixels (page `#191D23`, card `#25272C`, rows
  `#171A1E` inset look, `#3C3F44` hairlines, avatars `#6BA3FF` /
  `#FF7F7F`, labels `#D7D7D7`, roles `#919191`, button + link
  `#6BA3FF` with `#171A1E` button ink); panes widened to 452/33px so
  rows and fields share one 386px content width; the sign-in button is
  a hand-rolled press `Row` (dark ink the themed control cannot take).
  Aliasing verdict: the GPU backend runs `sample_count: 1` everywhere,
  so tessellated `Path` strokes are the only un-antialiased primitive
  (text rides the AA glyph atlas, rects/circles are analytic SDF) —
  the envelope is now the DejaVu `✉` glyph (coverage proven in the
  shipped TTF) and the padlock an SDF-rect composite, leaving one 2px
  dark-on-blue exit-door path where aliasing stays invisible. Remix
  pass 2026-10-11: all eight icons are Remix Icon v4.9.1 `line`
  glyphs (© Remix Design, Apache-2.0 — mail-line, lock-line,
  delete-bin-line, login-box-line, notification-3-line,
  information-line, cloudy-line, more-line, vendored as SVGs under
  `apps/sera-login/icons/`); user-verified picks (lock-line over
  lock-2-line, login-box-line over logout-box-r-line).
  Baked-icon pass 2026-10-11 (answers the aliasing): the GPU backend
  tessellates at `sample_count: 1` with no MSAA, so `Path` fills keep
  hard edges — the eight Remix SVGs now ship under
  `apps/sera-login/icons/` (fills baked per usage) and rasterize once
  through the framework's own `oppa-image::decode_svg` (resvg, full
  AA) at 4× into the shared `ImageCache`, rendered as `ImageView`
  leaves; every backend serves the same smooth pixels. New
  `deposit_icons` proof test asserts all eight deposits land at 4×
  size with partial-alpha fringe — headless 5/5 green. Rhythm pass
  2026-10-11 (answers the residual unclean feel): run-encoded pixel
  profiles of the reference crop decoded its exact vertical rhythm —
  the sign-in button carries its icon at 17px left padding with the
  label centered on the full width (symmetric 35px insets now), rows
  are 74px (15px pads, 42px avatars), panes 460/30px sharing one
  400px content width, and the airy gaps are explicit (title→subtitle
  18, subtitle→rows 27, email 28, password 21, checkbox 30, button
  21, link 22). Remaining delta is typography: the reference face
  renders ~13% narrower than DejaVu Sans Bold at the same nominal
  size, and no font-family API exists app-side — framework font
  support stays the follow-up.
- Page composition fixed (2026-10-11, real-shaping proof): the card
  packed under the top bar instead of centering — by framework
  construction, flex shares and justification only engage under an
  explicit height and the engine hands the root a `None` height
  hint, so a flow middle never grows. The whole page now rides one
  viewport-tall column in a `Portal` (portal children lay out
  against the live viewport height, decision 296) with
  `SpaceBetween`: bar top, caption bottom, card on the true center
  at any window size, with equal 58px chrome above/below. New
  headless test mounts `sera_root` at 1280x800 under rustybuzz +
  bundled DejaVu and asserts center ±2px, equal chrome ±2px, footer
  within 26px of the bottom — 6/6 green.
- Web repaint trap fixed (decision 469, found live 2026-10-11 while
  booting `apps/sera-login` for web): decision 436's paint stage
  profiler captured `Instant::now()` unconditionally in the shared
  `SceneSession::repaint` (and the `RgbPresenter` twin-split twin)
  while only the printing was env-gated — `Instant::now()` traps on
  wasm32-unknown-unknown, so EVERY web repaint panicked
  (`pageerror: unreachable`, boot stuck at `booting…`). No browser
  leg ran after 2026-10-06, so it sat undetected. Timestamps now
  capture lazily behind the flag (`Option<Instant>`, log shape
  byte-identical when armed). Gates: `cargo fmt --check` +
  `clippy -p oppa-app --all-targets` zero warnings +
  `cargo test -p oppa-app` 83/83 (incl. the 436 opt-out test) +
  Edge boot of `apps/sera-login` clean (screenshot proof below). No
  contract change (diagnostic-only path).
- Web resize (decision 470, gate green 2026-10-11): the web
  viewport is no longer locked at 800x600 — the scene follows its
  container through a bootstrap `ResizeObserver` (content-box CSS
  px + live `devicePixelRatio`) into a new 9th binding
  `WasmHost::resize` (+ `export_app!` + `WebApp` forwards):
  non-finite/negative sizes or non-positive DPR refuse loudly,
  zero-area parks with the last scene kept, same viewport + same
  DPR stays quiet, and each distinct reading applies synchronously
  (event-binding precedent, no debounce timer) through viewport +
  DPR read-modify-write, settle, and the shared refit contract as
  a full reload patch (correctness first — keyed resize patches
  stay a follow-up). Shells own sizing: template + generated
  `index.html` roots go fluid (`100vw`/`100vh`, fixed px stays a
  one-line opt-out); pre-470 shells without the export stay
  fixed-size with one warning. Gates: `cargo fmt --check` +
  workspace `clippy --all-targets` zero warnings +
  `cargo test -p oppa-web` 35/35 (incl. new `resize_web.rs` 8/8:
  refusals, park, quiet, refit-shape, DPR-only, shrink) +
  `cargo test -p cargo-oppa` 25/25 (template byte-match incl.
  synced bootstrap) + Edge `spike/web/resize.mjs` PASS (fluid
  `apps/sera-login` shell: 1280x800 boot centers the email field
  at x=889, widen to 1600x900 tracks dx=+160/dy=+50 exactly,
  shrink to 1000x700 tracks dx=-300, zero console errors;
  screenshots `target/sera-resize-*.png`). `spike/web/resize.mjs`
  runs both arms (`OPPA_BROWSER=firefox` via the driver.mjs BiDi
  recipe): Firefox 157 PASS with byte-identical tracking numbers,
  so the resize chain is engine-proven, not Chromium-only. Status-bar
  follow-up (same round): the `#oppa-status` dev bar sat above the
  100vh root, overshooting the visible area by its height — shell
  roots are now a 100vh flex column (status keeps natural height,
  scene flex-fills the remainder, `overflow: hidden`), so the
  observed viewport IS the visible area; template, generator, and
  proof shell all carry it.
- Web field background (decision 471, 2026-10-11): native
  `<input>`/`<textarea>` painted the UA field background (white, or
  dark-scheme gray under the theme `color-scheme`), blanketing the
  wrapping `Div` fill on every web field and clashing with composite
  chrome — the base CSS now resets `background:transparent`
  alongside outline/border (the container owns the box on all
  backends). Gates: `oppa-dom` lib 24/24, clippy clean, Edge proof
  re-green with pixel-verified uniform `#171A1E` interiors.
- Web caret + typing latency (decision 472, 2026-10-11): the DOM
  backend painted a framework caret bar beside the native one
  (misplaced — wasm metrics vs browser layout), contradicting the
  locked verdict-(b) caret ownership already stated in SPEC. The
  whole DOM caret pipeline is removed (`DomBackend` state, derive,
  div, patch comparison, presenter leg as a documented no-op;
  selection/handles untouched): one native caret, and blink toggles
  no longer touch the `sels` channel. Latency measured on the served
  app (11 keys, EventTiming + longtask probe): dev wasm blocks
  ~370ms/key (~60 longtasks to 215ms — unoptimized shaping + full
  pipeline on the sync input path), release blocks nothing (zero
  longtasks either way). Shipped fix = caret removal (all profiles)
  + serve release (the default) for instant feel; the pipeline stays
  synchronous by design (caret atomicity). Gates: `oppa-dom`
  24 + 50 + 6 green (caret test rewritten to the verdict), clippy
  clean, Edge re-proof green.
- Desktop input lag (decision 473, 2026-10-11): the desktop leg of
  the same complaint — every GPU-served frame also ran the full
  CPU software raster (`cpu_paint` defaults true and only the
  Android runner opted out), so each keystroke paid engine (~9ms
  headless-measured with real shaping) plus a redundant 1280x800
  software raster in dev. Desktop Windows/Linux bring-up now opts
  out like Android (nobody reads the pixmap while the swapchain
  serves); transient present failures refresh the pixmap once via
  new `SceneCore::refresh_cpu_pixmap` without leaving GPU mode
  (retry semantics preserved — `disable_gpu` still owns the
  permanent fallback and re-arms there). Gates: fmt clean,
  workspace clippy zero warnings, `oppa-app` 84 + e2e green (incl.
  new opt-out/refresh test), sera-login 6/6 over the changed
  framework.
- Selection handles are mobile-only (decision 474, 2026-10-11):
  renderer-drawn bars/knobs published for every live selection on
  every backend, so mouse drag-selects grew touch handles on
  desktop. New host flag `selection_handles_visible` (default true
  — zero behavior change until a runner opts out); publication
  `None` kills both painting and grab arming in one cut (drags near
  edges fall through to plain selection drags). Desktop
  Windows/Linux runners set false at startup; Android and Web keep
  current behavior (touch-capable web gating stays a follow-up).
  Gates: fmt clean, workspace clippy zero warnings, `oppa` 245 +
  `oppa-app` 85 green (incl. new flag + hide/restore tests). Other
  Firefox legs stay rig-limited (tolerance, not a claim).
- Selected text inverts for contrast (decision 475, 2026-10-12):
  dark-mode near-white on the fixed light-blue highlight had no
  contrast, on all pixel backends. New `SELECTION_INK` (fixed
  near-black): the shared builder splits `Text` ops at selection
  boundaries (cluster granularity — clusters never split, so ink
  edges land exactly where highlight rects do; same gate, so they
  always agree), covering CPU + GPU by construction; unselected
  scenes keep byte-identical ops. DOM needs no twin (the browser
  inverts native selections itself). Also renumbered the session's
  2026-10-11 decisions 463–468 to 469–474 — 463 was already taken
  (API-29 ranchu) — and recorded all seven plus this one in
  `docs/DECISIONS.md`. Gates: fmt clean, workspace clippy zero
  warnings, wasm check clean, `oppa-app` paint suite green (incl.
  new dark-mode full/partial/collapse inversion test), full
  workspace suite green.
- Control icon slots (decision 478, 2026-10-12): `TextInput` and
  `Button` take a decorative leading icon (`ImageCache` id +
  render size; SVGs deposit once through `deposit_svg_icon` at 4x
  with AA fringe, so icons stay smooth on the un-antialiased GPU
  path); unset icons keep byte-identical trees, and the icon is
  handlerless (never a tab stop). The icon row carries the
  explicit content height (parent-driven box growth never reflows
  a Row's children — measured 2px off before this). Gates: fmt
  clean, workspace clippy zero warnings, wasm check clean,
  `oppa-controls` 151 tests green (incl. slot geometry +
  behavior + deposit/AA-fringe/refusal tests), full workspace
  suite green.
- GPU multisample anti-aliasing (decision 479, 2026-10-12): the GPU
  backend rasterized with `sample_count: 1`, so tessellated
  path/curve edges staircased while every other backend covers
  area. All four pipelines now compile multisampled with a
  per-frame resolve (4x where the adapter's format supports it,
  probed step-down through 2x to direct 1x — weak adapters stay
  functional; single-slot resolve texture, steady state allocates
  nothing); `Caps.msaa` reports honestly (fresh backends still
  `false`). Measured on RTX 3060 Ti (dev profile, 1280x800 with
  120 vector ops): 2.45ms MSAA vs 2.84ms direct — no measurable
  regression (fixed per-frame costs dominate). Gates: fmt clean,
  workspace clippy zero warnings, wasm check clean, `oppa-gpu`
  11/11 on hardware (all oracle bounds hold, incl. new
  caps-agrees-with-probe test), full workspace suite green.
- Layout loudness + viewport-tall roots (decision 476,
  2026-10-12): `Div` children carrying `flex_grow`/`flex_shrink`
  used to size intrinsically with no diagnostic — now they refuse
  loudly (the `flex_wrap` precedent; use a `Column`). And a root
  carrying `fill_height` resolves viewport-tall (the engine hands
  the root the live viewport height, mirroring the width channel),
  so `SpaceBetween`/`Center` engage with real leftover — true
  vertical centering without the `Portal` workaround from the Sera
  build. Roots without `fill_height` pack exactly as before
  (hint gated on the fill flag). Gates: fmt clean, workspace
  clippy zero warnings, wasm check clean, `m3_layout` 54/54 (incl.
  2 loud-refusal + root center/pack-top tests), full workspace
  suite green.
- App theme tokens (decision 477, 2026-10-12): apps can publish a
  custom palette per mode (`host.set_theme_tokens_override`, `None`
  clears) — `ctx.theme().tokens()`, resolved text ink, page
  backgrounds, refit surfaces, and DOM chrome/stanzas all follow
  it with tracked re-renders, on every backend; unset slots stay
  byte-identical. Custom font faces stay blocked (sizes are safely
  app-settable via `set_layout_config` today, but shipped faces
  are runner-owned per backend — a face pipeline is a design
  follow-up, not smuggled in here). Gates: fmt clean, workspace
  clippy zero warnings, wasm check clean, new core/builder/app/DOM
  tests green, full workspace suite green.
- Portable login app (gate green 2026-10-05): `apps/login` proves
  write-once outside the core — one UI file (`src/app.rs`,
  primitives only) with thin shells (`main.rs` desktop,
  `lib.rs` web, `web/` shell, Android `mount_app` recipe in
  README). One command each: `cargo run` (desktop, Windows-proven
  GPU path) and `cargo oppa serve apps/login` (web, Edge-proven in
  the browser); Android rides `cargo oppa build --android` (APK,
  device-proven on RMX3370). `serve` also learned `--lib` builds + workspace-aware
  target/lockfile resolution (`cargo metadata`), so members and
  standalone apps alike serve.
- Phase 40a (decision 381, gate green 2026-10-01): Lint-zero —
  eliminated pre-existing clippy warnings on record (fps_app naming in
  oppa-fps and oppa-fps-demo, DesktopPollHook type alias in oppa-app,
  needless borrows and unused import in oppa-web); clippy gate runs at
  zero warnings across all targets; CONTRIBUTING.md gate note updated.
- Phase 40b (decision 382, gate green 2026-10-01): Verification sweep —
  c3 CDP/Edge re-baseline exercised live against headless Edge
  (`spike/web/harness.mjs`, `compare.mjs`, `dom_text.mjs`, `parity.mjs`,
  `sink.mjs`, `webapp.mjs` all green; state boot-sync);
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
   verified line-identical, per-suite baseline-identical),
   `crates/oppa-shell-linux/src/input.rs` (decision 413, 1,305 → 222-line hub + `events.rs` 265 +
   `shell.rs` 279 + `tests.rs` hub + 5 area suites, all $<190; zero visibility widening, one
   mechanical `crate::file_dialog` path fix, `input::{...}` paths preserved via hub re-exports;
   fn-name set 42/42 identical, per-suite baseline-identical),
   `crates/oppa-reload/tests/m9_reload_gate.rs` (decision 414, 1,656 → 199-line hub + 6 scenario
   modules under `tests/m9_reload_gate/` via `#[path]`, all $<430; cross-scenario `assert_*` helpers
   move to hub, groups `use super::*`; same 6 tests, same outcomes — dirty 5/1, clean 6/6;
   fn-name set 30/30 identical, per-suite baseline-identical on re-probe after one full run caught
   3 live-Shift pointer flakes, the documented environmental class),
   `crates/oppa/tests/m3_layout.rs` (decision 415, 1,879 → 224-line hub + 15 area modules under
   `tests/m3_layout/` via `#[path]`, all $<230; shared `FakeText`/`Rig`/`find_ids`/`approx` stay hub,
   groups `use super::*`, `delay` via file-level `#![allow(non_snake_case)]` for the `Panel`
   component fn; same 50 tests, same outcomes — dirty 49/1, clean 50/50; fn-name set 61/61
   identical, per-suite baseline-identical),
   `crates/oppa-dom/tests/m7_dom.rs` (decision 416, 2,763 → 370-line hub + 14 area modules under
   `tests/m7_dom/` via `#[path]`, all $<375; shared cards/slots/`Rig` stay hub, groups `use super::*`,
   cross-region `FieldProps`/`render_field` + `SwitchProps`/`render_switch` widened to `pub(super)`,
   the nested `#[cfg(windows)] mod browser` lifts verbatim (paths only gain segments); same 50 tests,
   same outcomes — dirty 32/18, clean 50/50; fn-name set 84/84 identical, per-suite baseline-identical
   on the third probe after two probes caught transient live-Shift flakes, the documented class).
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

- Loud save/folder contract (decision 422): `DesktopLoop::save_file_dialog` / `pick_folder_dialog` return `Result<Option<PathBuf>, PickError>` (`Ok(Some)` = picked, `Ok(None)` = dismissed, `Err(Unsupported)` = no backend, `Err(Backend)` = failure) with `has_save_dialog` / `has_folder_dialog` probes; desktop runners unchanged (Win32 COM + Linux portal/zenity), Android/Web stay backend-less and now distinguishable.
- Bytes open-seam foundation (decision 423): `PickedFile{name, bytes}` + `FileOpener::request_bytes` / `poll_bytes` (+ `ScriptedOpener`) in `oppa`; host-testable `AndroidFileOpener` SAF-spec + generation-gated pending/resolve rendezvous in `oppa-shell-android`; `WasmHost::file_pick_start` / `file_pick_resolve` generation rendezvous in `oppa-web` (fetch pattern). Device SAF intent launch + `ContentResolver` reads and JS chooser glue (`<input>`/FS Access + `FileReader`) plus save-bytes stay follow-ups — no device/browser claims this round.
- Android picker device read path (decision 424, emulator-proven): `picker_validate` in `oppa-android-app` drives MediaStore insert/write/read/delete through `ContentResolver` with a byte-exact compare (204/204, `match=true`, `deleted=1` in `picker.txt`) and routes the device bytes through the opener rendezvous (`request_bytes` → `resolve_bytes` → `poll_bytes` assert) — the exact read path SAF open will use, with no chooser gesture. Built with the aapt2 fallback path (no Gradle distribution on this box — `javac` + `d8` supply the `classes.dex` Gradle normally builds; omitting it aborts headed runs in `hide_system_bars` with `ClassNotFoundException: OppaUi`, headless is unaffected) and proven live on Medium_Phone_API_36.1. The DocumentsUI gesture (intent fire + `onActivityResult` dispatch — `android-activity` 0.6 has no result-delivery support, so a transparent trampoline activity is the design) stays the manual pass.
- Proof-phase ANR heartbeat (decision 426): the 13:16 ANR (`user request after error` → SIGKILL) was synchronous proof stretches starving input dispatch (21 s pixel proof with taps landing mid-run), not the uncapped benchmark loop — which already drains per iteration. `run_frame_loop` / `run_sustained_damage_loop` take a per-frame heartbeat closure (outside the timed section, discard-never-inject) threaded from a `drain_input_queue` helper, plus drains at phase seams; `BENCHMARK_PHASE` is back on. Proven by tapping through the next startup proof: no ANR, run completes to `DONE`.
- SAF trampoline wired to Browse (decision 427): transparent `OppaPickerActivity` (fire intent + `onActivityResult` → native `onPickerResult` bytes/name/generation/dismissed + finish; `aapt2`-packaged via `javac` + `d8` like the IME classes) with a `picker_bridge` rendezvous (registration, cache-copy under `picked/`, `FileDialog` impl on a zero-state `PickerHandle`) swapped into the sink's open slot at setup — Browse fires the real DocumentsUI flow. Registration + fire-entry dry-resolve proven on device (`imm.txt`: `picker-callbacks=already-registered`); the pick gesture itself stays manual. Fixes folded in from live-fire failures: manifest `hasCode` false→true (a codeless manifest leaves the app classloader with an empty dex path — activity instantiation dies); result entry static-linked (reverse-direction dispatch uses the PathClassLoader's class object, which `RegisterNatives` on the Dex-loaded copy misses — first pick died `UnsatisfiedLinkError`) plus `System.loadLibrary` in the Java (same loader rule, prophylactically on `OppaIme` too).
- Sink drives real pickers (decision 425): `DesktopLoop` owns the open slot beside save/folder (all three held as shared `Rc` handles — runners install the native trio, apps borrow the live backend instead of a second instance) and `KitchenSinkProps` carries the three handles (scripted default seeded with one `demo-pick.png`, so headless/Web/Edge expectations hold; the desktop example forwards the live handles in its configure closure). Nothing drives the shell's open seam in production — the loop is the app path.
- Swapchain resume-heal after the SAF round-trip (decision 428, emulator-proven): the picker trip Stops NativeActivity (window destroyed, recreated on return — same size, new pointer); the present loop pinned one `&window`/`wgpu::Surface` for the whole run (Pause was a per-iteration local that never stuck, Stop/Terminate/Init/Resize/Redraw unhandled) and kept presenting into the dead surface with no error, while the interactive phase snapshotted `native_window()` once (DocumentsUI still front → permanent skip) — black until cold restart, beats above the pause check proving liveness only. `drive_present_loop` re-acquires per generation (lost/swapped/resized window or Resume/Start/Terminate → re-ensure + reconfigure + refit + repaint + heal-present, counted as `heals=`; same-window redraw edges repaint + present with no rebuild — rebuilding on the emulator's ~1 Hz spurious same-size `WindowResized` thrashed; missing window parks); `paused` is persistent (Pause/Stop set, Resume/Start clear); interactive waits up to 30 s for the window; headless `paused` hoist; temporary tapdbg/setup/heal probes reverted. Proven live on Medium_Phone_API_36.1: pick → return renders, dismiss → return renders, `reopen heal=2/3` + continued batches in `taps.txt`, no `error.txt`. Gates: `cargo fmt --all -- --check` + `cargo clippy --all-targets` (zero warnings) + wasm check + full workspace `cargo test --no-fail-fast` 101/101 green clean-env (`target/round-428-full-clean.log`, per-suite identical to the locked baseline — no shared-crate edits) + Android-target check/clippy green (`cargo test` unbuildable on host by construction; the on-device run is the test).
- Unified pick session (decision 429, emulator-proven): one app-facing press series over any open backend (`PickSession::press` in `oppa::dialog` — poll-first, request-if-idle, consume-once — plus the `PickBackend` blanket adapter over `FileDialog`/`FileOpener` and the shared `OpenOutcome` path/caption vocabulary). The old request-then-poll order discarded an async settlement before reading it, so the Browse/file captions never left `(none)`/`still open…` on Android; the session consumes an awaiting settlement instead of requesting over it, requests only when idle (a second tick while open re-polls — one dialog per press series), and consumes each settlement exactly once (backend level-echo never re-surfaces). Blocking backends settle in-press, async across presses — same call shape, same captions (`still open…` unified to the ellipsis everywhere). The session lives in a render-stable `ctx.signal` inside the `FilePicker` control (props are rebuilt every render — a props-carried session reset to idle on each re-render, including the badge's 4 Hz root re-run, and every press requested over the settlement) and beside the handles in `SinkDialogs` for the sink trigger; backend traits, runners, and scripted doubles unchanged. Proven live on Medium_Phone_API_36.1: Browse → pick → return renders with `still open…`, next Browse consumes with no new dialog — row shows the real `picked/…tapme.txt` path + `picked …` caption; batches continue, no `error.txt`. Gates: `cargo fmt --all -- --check` + `cargo clippy --all-targets` (zero warnings) + wasm check + full workspace `cargo test --no-fail-fast` clean-env (`target/round-429-full-clean.log`: 100 green in-run, the 1 red is the documented transient Win32 clipboard-contention flake, green on isolated re-probe — per-suite otherwise identical to the locked baseline) + Android-target check/clippy green.
- Filename-first picked-path display (decision 430): the `FilePicker` row showed one absolute-path line overflowing off-screen on phones (`/data/user/0/…tapme.txt`). It now renders the file name on line one plus the parent directory wrapping beneath in secondary ink under the row's remaining width (`fill_width` supplies the finite wrap constraint — unconstrained text measures one overflowing line; the global ellipsis config stays off); bare names and `(none)` stay one line, and the status caption names the file (`picked tapme.txt`) instead of the absolute path. `OpenOutcome::{file_name, dir_name}` in `oppa::dialog` (same split the control renders); backend traits, session law, and scripted doubles unchanged.
- Runtime screen-mode toggle, Android-only (decision 431, revised 2026-10-06): apps flip status + navigation bars from component code (`ctx.set_system_bars_visible(bool)` / `ctx.system_bars_visible()` — host signal defaulting hidden/immersive + last-wins runner command drained once per pump, the close-request precedent); both `OppaUi` halves use named `View`/`LayoutParams` constants with pre-30-only guards (plain `NoTitleBar` theme, full LAYOUT trio show end-state, `IMMERSIVE_STICKY` hide, window fullscreen flag armed/cleared pre-30 only, `MATCH_PARENT` size both halves) so the scene stays full-bleed — bars overlay, never resize, no refit owed; both Android loops drain per iteration and heals re-assert the tracked mode instead of forcing hidden; desktop/Web keep the signal update with no platform effect. Gates this round: `javac` vs API-36 `android.jar` (intended-deprecation notes only) + `cargo fmt --check` + `cargo test -p cargo-oppa` 25/25 + `clippy -p cargo-oppa` zero warnings (one Rust line: the theme constant; full workspace suite unaffected). Contract in `SPEC.md` Runner, mechanism on the Android platform page. By-the-book revision (2026-10-06, no contract change): both `OppaUi` copies use named `View`/`LayoutParams` constants; `hideBars` legacy bits include `IMMERSIVE_STICKY` (without it the framework force-clears the hide on first touch on API 29); boot theme is plain `NoTitleBar` (no forced `FLAG_FULLSCREEN` — the forced flag wedged the pre-30 policy across cycles); show end-state is the full LAYOUT trio (the framework warns against `LAYOUT_HIDE_NAVIGATION` alone — on 29 it leaves status undrawn); explicit display size reads `getCurrentWindowMetrics()` on 30+ (real-metrics on 29 only); pre-30-only guards arm/clear the window fullscreen flag and drop bar-background ownership in hide. Device verdict: API-30 emulator full Show/Hide cycle clean both bars both ways (~4 s, pixel-verified); API-29 emulator likewise clean over two full cycles — Show draws both bars promptly, Hide restores the boot frame pixel-identically (final sha equals boot sha), nav + status both ways, scene full-bleed throughout. The 29 fix was `MATCH_PARENT` window size in both halves: explicit pixels freeze the drawn state on pre-30 policy (boot-hidden stayed hidden, shown stayed shown) while `MATCH_PARENT` follows requests — boot still settles full-bleed 1080x2400 (`fillxfill` in dumpsys, no 2290 regression), so the explicit-size workaround is deleted. Process lesson: `cargo-oppa` embeds the Java template at compile time — reinstall the plugin after template edits and grep the synced copy for markers before installing, else device rounds silently test stale code (two rounds lost this way). Gates: `javac` clean vs API-36 `android.jar` (intended-deprecation notes only) + `cargo fmt --check` + `cargo test -p cargo-oppa` 25/25 + `clippy -p cargo-oppa` zero warnings (one Rust line: the theme constant; full workspace suite unaffected).
- Login app bars button (decision 431 follow-up, proven on-device 2026-10-06): `apps/login` gains a `login-bars` button under Sign in that flips the mode from a plain press handler (`ctx.host()` into the closure, the theme-toggle precedent — no `cfg`, no shell imports, file stays portable); the label reads the tracked signal (`Show/Hide system bars`). Throwaway tap proof green then deleted (mount `login_root` → tap → shown + `Some(true)` → tap → hidden + `Some(false)`); desktop check/clippy + wasm lib check green. Release APK (`cargo oppa build --android`, 15.7 MB) installed + launched on Medium_Phone_API_36.1: boot immersive full-bleed, tap Show → bars + label flip, tap Hide → pixel-identical boot frame (same screencap hash), clean logcat. API 29 leg superseded by decision 433: the ladder now closes with the terminal CPU row, so the GLES-2.0-only image presents via software (`software present (CPU pixmap)`, full-bleed scene, bars toggle proven there since).
- Shared presenter ladder (decision 432): one first-suitable-presenter-wins rule in `oppa-app::ladder` — per-platform GPU row tables (Windows Vulkan/DX12/GL per decision 390, Linux Vulkan/GL per 391, Android GL/Vulkan + Vulkan-winner stickiness) with the `OPPA_RENDERER` pin match living once, plus `walk_presenter_rows` (surface factory + row-tail closures, loud per-row refusals, recorded attempts, terminal CPU row). All four bring-up sites repointed (Windows GDI, Linux softbuffer, generic + proof Android loops) with identical orders, logs, records, instance reuse, pinned env, and Fifo/benchmark modes; Android attempt notes unified to the walker vocabulary (human logs only). CPU-as-terminal-row explicit: desktop ladders close with CPU (GDI/softbuffer legs); Android ladders close with it too since decision 433 (GPU-less windows post `rgba8` through the native-window lock — `LivePresenter::Cpu` / `ProofPresenter::Cpu`, re-probing GPU first each generation). Headless proofs: 4 ladder tests (tables, pins, refusal order, CPU resolve). The API-29 refusal story above predates decision 433 — since then the GLES-2.0-only image presents via the terminal CPU row instead of refusing.
- Continuous heartbeat via system property, Android-only (decision 434, proven on-device 2026-10-06): `debug.oppa.continuous=1` shortens the generic runner's input quantum from 200 ms to 8 ms for tap-latency diagnosis (presents stay damage-gated — the first version forced repaint+present every iteration and pegged a phone core at 93% idle, so the heartbeat is quantum-only by design). RMX3370 run: `continuous heartbeat on` logged, Adreno Vulkan present at 1080x2400, full-bleed render. Gates: `rustfmt --check` on the touched runner files + APK build (the Android-target compile gate) green, no warnings.
- Tap-path paint + profiler, Android runner (decision 435, emulator-measured 2026-10-06): `debug.oppa.profile` per-iteration buckets (`poll/input/repaint/present` + change attribution) plus a per-toggle dispatch marker split tap→bridge into queue/poll waits plus ~100 ms full repaints — the tap batch repainted back-to-back with the frame repaint (removed; caller owns the single repaint) and bare redraw edges repainted identical frames (skipped when no input/batch/blink stands behind them, repaint forced every 30th consecutive skip). Tap→bridge on the emulator 387 → ~300 ms with one repaint per dirty iteration; the remaining ~100 ms/repaint scene cost is SceneCore-level (shared with desktop) and stays open. Gates: APK build green, no warnings; device proof is emulator tap cycles (phone disconnected before re-measure).
- Skip CPU raster on GPU-served runs (decision 436, proven on-device 2026-10-06): paint profiling (`OPPA_PROFILE_PAINT` session stages + CPU/GPU-twin split) showed the full-scene software raster is ~100% of repaint cost (`cpu=104 ms gpu=0 ms`, 2418 ops replayed per tap) while the plan build is free. `RgbPresenter` gains a default-on `cpu_paint` switch the generic Android runner turns off per GPU generation; `disable_gpu` re-arms plus repaints once immediately so the fallback never serves stale pixels. Proven on Medium_Phone_API_31 (GLES 3.0 + host GL passthrough — the only emulator here that serves GPU): tap repaint `cpu=0 ms gpu=0 ms build=1 ms total=1-2 ms` with the toggle label flipped and both bars drawn through the swapchain. Gates: `cargo fmt`, `cargo test -p oppa-app` 83/83 (incl. the new opt-out test: pixmap + counter still, twin encodes, fallback repaints once), `cargo clippy -p oppa-app` zero warnings, APK build green. Emulator (CPU row) tap cycle behavior-identical.
- Share font bytes via Arc (decision 437, 2026-10-06): the 2 GB API-29 guest LMK-killed our foreground app at 363 MB RSS (system-wide watermark breach — but we were the biggest resident by far): /system/fonts is 72 MB and every face was heap-copied 3x (service Vec + runner Vec + per-backend Vec). `set_font_for`/`set_font_bytes` now take `Arc<[u8]>` across both backends, the presenter, scene, and all runners (both FaceTables already Arc'd internally — only the boundary copied). Gates: `cargo fmt`, full `cargo test` on oppa-cpu/oppa-gpu/oppa-app/oppa-dom green, clippy zero warnings, APK build green. Follow-up (not started): lazy per-family injection so resident tracks used fonts, not installed ones.
- Share font files across faces (decision 439, proven on-device 2026-10-07): the stutter/freezes were swap thrash with us as the cause — 672 MB PSS (659 MB swapped) on a 2.5 GB guest with swap 99.5% full and `kcompactd` at 43%. A temporary CPU-row probe (since removed) showed an identical ~670 MB heap on both rows, ruling out the GL translator; the hog was per-face full-file copies (`face_record` cloned the whole file per face, in the service and again per face in the runner). One `Arc<[u8]>` per file now, shared by every layer. Measured on Medium_Phone_API_36.1: native heap 668 → 122 MB, PSS 672 → 147 MB, guest back to 912 MB available; login renders pixel-identical, tap cycle clean. Gates: `cargo fmt`, full tests on text/runner/app/cpu/gpu/dom crates green, workspace clippy zero warnings, APK build green.
- Android IME live composition (decision 438, proven on-device 2026-10-06): key taps reached LatinIME (suggestions updated) but the proxy dropped `setComposingText` by documented design and raw `insert` no-ops while composing, so typing looked dead though every layer beneath worked. The proxy forwards composing text (native → queue/event/cmd → desktop-parity preedit feed) and commits route through `CompositionCommitted`; the hidden Bridge forwards printable hardware keys as single-char commits (navigation/action keys fall through to the native queue). Medium_Phone_API_31 proof: tap h → composing live, tap space → `commit len=2 committed`, `h` visible; full email `hi@test.com` typed key-by-key (letters via composing+commit, `@`/`.` via direct commits), every keystroke landing visibly. Medium_Phone_API_36.1 (2026-10-07): same build, GL row via host translator, bars cycle clean (show draws both, hide restores 0/0), tap h → direct `commitText` → `h` visible.
- 20 fps frame budget (profiled 2026-10-07; Medium_Phone_API_36.1 emulator + RMX3370 phone, 20 taps per mode): emulator event-paced tap response 100–250 ms, dominated by the 200 ms quantum wait plus emulator present (12 ms avg, 36–78 ms spikes, rare 139–247 ms swapchain stalls); engine work per frame is input 0–20 ms + repaint 0–6 ms. Emulator continuous (8 ms quantum): 2944 iterations / 40 dirty (damage gating holds), dirty-frame avg 42 ms, worst 87 ms — every over-50 ms frame is present-dominated. Phone (Adreno 650 Vulkan): event-paced dirty avg 46 ms with present 3–8 ms and repaint 0–2 ms (one 29 ms first-frame outlier); continuous dirty avg 8.9 ms, worst 25 ms — every tap answers in under half a 50 ms budget. Idle burn 3.5% emulator / ~7% phone in continuous. Verdict: 20 fps holds with margin on real hardware; the emulator swapchain stalls are rig overhead. Production quantum (always-8 ms vs adaptive) is an open product call, not taken here.
- IME editor-state sync + typing latency attribution (decision 440, RMX3370/SwiftKey 2026-10-07): the proxy now mirrors commits/deletes into the hidden buffer (`super` calls — IME context correctness) and declared `NO_PERSONALIZED_LEARNING` (reversed by decision 444 below — Gboard maps the flag to global incognito, killing clipboard history on every field). Measured tap→commit holds ~440–450 ms/key steady (~1500 ms first-after-idle), immune to inputType variation, the learning flag, and the sync fix; tap→own-window is 70 ms and commit→visible ~20 ms, so ~370 ms/key sits inside SwiftKey/system (closed-source). Per-field variations stay a layout nicety, not a speed fix; Gboard is the user-side discriminator.
- Proxy follows window generations (decision 441, proven on-device 2026-10-07): after pause/resume, typing died while taps kept working — the cached proxy view detaches for good at window turnover (`showSoftInput` ignored as "not served" forever; repeatable, not a focus race). The runner drops the proxy cache on every post-boot generation turn and the next focus re-attaches to the live decor. Medium_Phone_API_36.1 proof: resume → `heal ime-proxy=dropped` → fresh connection, commit lands visibly. Gates: `rustfmt --check` on touched files clean, `cargo clippy -p oppa-android-runner` zero warnings, APK build green. Side note: LatinIME commits open composing text on input-finish, so a keystroke straddling pause can legitimately land twice (1:1 delivery verified, not a duplication bug).
- Hardware keyboard shortcuts on Android (decision 442, 2026-10-07): Ctrl+A select-all, Ctrl+Z/Y undo/redo, Shift/Ctrl+arrows and Home/End extend/move by char/word/to-ends — desktop parity (`run_edit_shortcut` / `step_session_nav`) wired through modifiers sampled at the pump; the Bridge returns chords to the native queue instead of eating them. C/X/V rode decision 443's ClipboardManager bridge (closed — see below). Gates: `cargo test -p oppa-shell-android` 28+7 (incl. modifier mapping test), `javac` both copies, `cargo test -p cargo-oppa` 25/25, APK build green, clippy clean. Device proof pending a hardware keyboard (`adb input key*` dead on the emulator). Gates: `javac` both copies, `cargo test -p cargo-oppa` 25/25, APK build green, phone bursts + user-typed hello. Gates: `javac` both Java copies + `cargo test -p oppa-shell-android` 28+6 + `cargo test -p cargo-oppa` 25/25 + APK build green. Open follow-up: live composition has no render path (plan builder emits selection/caret overlays only), so composing text shows on commit; `adb input key*` is dead on that emulator (volume-key test), leaving the hardware-key branch implemented-but-unverified there.
- Tap-path paint + profiler, Android runner (decision 435, emulator-measured 2026-10-06): `debug.oppa.profile` per-iteration buckets (`poll/input/repaint/present` + change attribution) plus a per-toggle dispatch marker split tap→bridge into queue/poll waits plus ~100 ms full repaints — the tap batch repainted back-to-back with the frame repaint (removed; caller owns the single repaint) and bare redraw edges repainted identical frames (skipped when no input/batch/blink stands behind them, repaint forced every 30th consecutive skip). Tap→bridge on the emulator 387 → ~300 ms with one repaint per dirty iteration; the remaining ~100 ms/repaint scene cost is SceneCore-level (shared with desktop) and stays open. Gates: APK build green, no warnings; device proof is emulator tap cycles (phone disconnected before re-measure).
- Android clipboard + native selection toolbar (decision 443, host gates green 2026-10-07, device proof pending): closes the decision-442 clipboard gap — `AndroidClipboard` over `ClipboardManager` (plain-text writes, empty clears, `coerceToText` reads, sync poll; missing service falls back to session-local memory loudly) feeding both the newly-wired Ctrl+C/X/V chords (desktop loud-never-fatal backend policy) and the proxy-hosted floating `ActionMode` (Select all/Copy/Cut/Paste with own labels; per-pump Rust mirror of text + UTF-16 selection + field-box anchor; user-dismissal pins until the selection moves; heals reset). Menu taps cross as queued `MenuAction`s through the same loop clipboard (masked/composing session guards hold). Java lands template + proof copy (the 431 precedent — the proof harness registers the same 8 natives against its own copy, so an unmirrored template would fail its registration loudly). Host-provable half unit-tested (`selection_menu` 7/7 in `oppa-shell-android` 35+7). Follow-up decision 444 (below) fixes the Gboard-incognito side effect found during device testing. Gates: `cargo fmt --check` clean (both workspaces), main `cargo clippy --all-targets` zero warnings, Android-target `cargo clippy --target aarch64-linux-android --all-targets` zero warnings (incl. a one-line `sysprop` cast-lint fix from the newer clippy), Android-target `cargo check` green, wasm check green, full workspace `cargo test --no-fail-fast` clean-env green, `cargo test -p cargo-oppa` java byte-match green, `javac` all four Java files vs API-36 `android.jar` green (OppaUi deprecation notes only, pre-existing). Toolbar half device-proven by decision 446 (below); hardware chords remain blocked — see Blocked.
- Gboard incognito scoped to password fields (decision 444, 2026-10-07): device testing of 443 showed Gboard permanently incognito (hat icon) with no clipboard history — root cause our own decision-440 blanket `NO_PERSONALIZED_LEARNING`, which Gboard maps to global incognito. The proxy drops the flag and declares `TYPE_TEXT_VARIATION_PASSWORD` only when the focused session is masked (Rust publishes masked state on every keyboard show via `apply_password_mode` + `restartInput`; both Java copies mirrored). Latency-neutral per 440's own measurement. Gates: Android-target `cargo check` + `clippy --all-targets` green, `cargo fmt --check` clean, `javac` all four Java files vs API-36 green. Device-proven with the 446 build: Gboard normal in email with clipboard chip, predictions-off + masked chip + dimmed mic in password.
- Native toolbar restored with view-local rect (decision 446, proven on-device 2026-10-07): the 443 floating mode went live with zero popup windows and zero destroys across three silent rounds; a custom PopupWindow detour proved the mechanics (visible pill, working taps) but not the system UI. Reading the shipped `FloatingActionMode` source named the bug: it feeds our rect through `getChildVisibleRect` against the originating view's bounds (VIEW-LOCAL expected — the framework offsets into screen space itself), so screen coords clipped to empty against our small proxy and the show no-oped. The proxy now wears the field box synchronously (measure + layout in the same UI tick — params alone apply a traversal too late for the same-tick query) and the rect is translated by the proxy origin. Both Java copies mirrored; no Rust changes (same 3 gated statics). Proven on Medium_Phone_API_36.1 (`apps/login` release): system Cut/Copy/Paste pill on word selection, toolbar Copy reaches the system clipboard (Gboard chip appears, zero errors), toolbar Paste reads it back (OS "login pasted from your clipboard" toast), toolbar hides on collapse. Gates: `javac` all four Java files vs API-36 green, APK rebuild + install + tap cycle green. Item labels stayed our 4 English strings (closed by decision 447 below). Remaining open: hardware-keyboard Ctrl+C/X/V chords (same backend, untestable without a hardware keyboard — see Blocked).
- Proxy overlay anchor + PopupWindow detour (decision 445, 2026-10-07): the floating toolbar clips the content rect to the originating view's bounds, so the proxy wears the mirrored field box (transparent text/background, touch pass-through) — async params first (too late for the same-tick query), then synchronous measure + layout. A custom PopupWindow row in between proved the Rust loop end to end (visible pill, working taps) but was abandoned for the system UI.
- System-populated toolbar menu (decision 447, proven on-device 2026-10-07): our custom callback owned 4 English items; the OS populates Cut/Copy/Paste (+Share, overflow) with icons and localized strings only when the Editor starts the mode itself (verified in the shipped framework source: TextActionModeCallback + android.R.id.* == TextView.ID_*). The proxy registers an intercepting selection callback (public API): system builds its menu, our four ids forward to Rust and consume (no proxy-buffer edits, no double paste), Share/assist run natively. Trigger is a synthetic long-press into super.onTouchEvent (DOWN at the mirrored selection middle, UP past the timeout, touchscreen source set — obtain() leaves UNKNOWN and showUIForTouchScreen silently refuses the start). Single selection source stays Rust (real touches pass through; fire skips while one is in flight). Also removed the decision-445 setTextIsSelectable(false), which converts the buffer off Spannable and drops movement — silently killing proxy selection and every system gate. Proven on Medium_Phone_API_36.1: system pill with Share + selection handles, Copy/Paste end to end with the OS paste toast. Gates: `javac` all four Java files vs API-36 green, APK rebuild + install + tap cycle green, `cargo test -p cargo-oppa` 25/25 (template byte-match).
- Long-press hold word-selects on text fields (decision 448, proven on-device 2026-10-07): a real finger long-press died in the router (no on_long_press on fields → tap-to-caret fallback) and never reached the Editor, so only the synthetic fire could raise the toolbar. TextInput/TextArea declare on_long_press over a shared field_long_press (double-click word rule at the hold point); consumed release never taps, drag-after-hold still extends, pump sync raises the system toolbar. Proven on Medium_Phone_API_36.1 with discrete DOWN/UP injection (adb swipe ignores hold duration): 1s hold selects the word, system pill appears with Translate/Cut/Copy/Paste/Share + selection handles. Gates: `cargo test -p oppa-controls` 142/142 (incl. 3 new hold tests), `cargo fmt --check` + `cargo clippy -p oppa -p oppa-controls` clean, APK rebuild + install + hold cycle green.
- Proxy selection reports back to the session (decision 449, proven on-device 2026-10-07): the system toolbar's native handles drag the proxy buffer while Rust keeps the stale span (wrong-span Copy, handles snapping back on mirror). Bridge.onSelectionChanged reports (skipped while mirroring/empty; IME echoes guarded) into MenuAction::SelectionChanged; the pump absolute-sets via select_range (ordered/clamped/caret-on-end, no undo, composing skips) over utf16_to_byte_units. A drag-settle destroy arriving a pump after its report no longer pins (previous-batch OR — native re-shows on release; genuine swipes still pin). Proven on Medium_Phone_API_36.1: handle-drag shrinks the highlight to the dragged span, toolbar re-shows with Select all + Replace, Copy lands exactly the dragged span (Gboard chip "r"). Gates: oppa lib 231/231 (incl. 3 select_range tests), shell 39/39 (conversion + queue + pin), full workspace clean-env green, fmt + clippy (main + Android-target) clean, javac ×4 green, APK rebuild + install + drag cycle green.
- Drag-state maintenance for live handle drags (decision 450, proven on-device 2026-10-07): dragging a handle vanished it mid-drag and flickered the toolbar — every pump's mirror rebuilt the proxy buffer under the Editor's live drag, and re-fires landed synthetic long-presses inside it. Mirrors are change-gated (text/selection apply only on difference) and re-fires wait out an 800ms report-quiet window (first shows unaffected). Proven on Medium_Phone_API_36.1: stepped multi-second drag streams reports with zero mode churn, handles alive throughout, full system toolbar at release. Gates: shell 40/40 (incl. settle test), fmt + Android clippy clean, APK rebuild + install + sustained-drag cycle green.
- Toolbar raises for touch selections only (decision 451, host gates green 2026-10-07): verified in the shipped framework source that no key path starts the selection mode (Ctrl shortcuts run direct buffer ops, Shift+nav extends through movement, Ctrl+A even hides a live toolbar) — and probed on-device as far as possible (production build: no root for sendevent, emu-console injection lands nowhere). The pump carries a per-frame selection cause (pointer/menu/report = touch, keys alone = keyboard, quiet inherits) inside the snapshot; the initial raise fires for touch only, live toolbars still track key moves through Update, and a touch on a suppressed span converts an Update into a fresh fire. Gates: shell 43/43 (cause-flip, permit-gate, last-cause), fmt + Android clippy clean. Device proof (touch regression + hardware chords) needs a device pass — see Blocked.
- Configurable window styles (decision 482, gate green 2026-10-12):
  `WindowStyle::{Decorated, Undecorated, Transparent, Extended}`
  (default `Decorated` — every pre-482 app byte-identical) selected
  launch-only via `WindowOptions::{with_window_style, decorated,
  undecorated, transparent, extended}`. Shells map it at creation
  (Win32 `WS_POPUP` + `WM_NCCALCSIZE` strip + pure `frameless_hit_test`
  over 8px borders/caption regions in `WM_NCHITTEST`, proven live
  through the proc incl. client==window rect; winit
  `decorations(false)` + `transparent(true)` + `drag_window()`).
  `Transparent` pairs a premultiplied-alpha swapchain (bring-up +
  every reconfigure/outdated path re-applies the loop's alpha mode)
  with an alpha-0 GPU clear; CPU fallback stays opaque (XRGB forces
  it — stated limit). `Extended` publishes `window_drag_rects`
  (`HeaderBar` committed boxes minus press-owning children,
  area-exact) per pump iteration (Windows) / per press (Linux);
  caption buttons bridge through `WindowControl::{minimize,
  toggle_maximize}` + `ctx.request_close()`. New `HeaderBar`
  control (handlerless `Row` chrome, opt-in caption actions) plus
  `apps/sera-login/examples/extended.rs` showcase and the standalone
  `apps/stylebook` vehicle (one elegant root — header bar over a
  centered card with counter — mounted under `--style=` for all four
  chromes; caption bridge via poll-hook signal drain; portable
  desktop/web/Android shells; 4/4 headless). Follow-up from the
  vehicle: drag-region publish now flows through the single
  `drag_regions_for_style` policy point (geometry gated on the style
  in one place — runners no longer re-decide per platform). Gates: fmt clean, workspace clippy zero warnings, wasm
  check clean, full workspace suite green (incl. new style/drag/
  caption/alpha tests: `oppa` window 4/4, `oppa-controls`
  header_bar 4/4, `oppa-app` 89/89 incl. 2 style tests,
  `oppa-shell-win` 27/27 incl. live NCHITTEST, `oppa-shell-linux`
  55/55), `sera-login` 8/8, `stylebook` 4/4 over the changed framework.
- Live-resize reentrancy fix (decision 483, found live running
  Stylebook): the caption-bridge maximize panicked at startup —
  `run_poll_hook` borrows `state` across the hook call, and a hook
  HWND effect (`ShowWindow`) dispatches `WM_SIZE` synchronously, so
  the live resize hook's `borrow_mut` blew up inside `wndproc`. The
  hook now takes contested steps via `try_borrow_mut` and skips
  them (the loop-bottom live-size poll applies them on the same
  iteration — deferred, never lost). Gates: fmt clean, `oppa-app`
  90/90 (incl. the new skip-while-borrowed regression test),
  clippy zero warnings.
- Frameless keeps the OS frame; DWM blur-behind transparency
  (decision 484, user-verified live in Stylebook): bare `WS_POPUP`
  silently dropped Aero snap/tiling with the thick frame
  (Extended), and Transparent painted opaque white. The linked
  JavaFX `EXTENDED` design (frame kept for resize border, shadows,
  animations; visuals stripped in message handling) plus winit
  0.30's own backend name the proven recipe, now mirrored: every
  style creates `WS_OVERLAPPEDWINDOW` (snap, tiling, Win+arrows,
  taskbar menu, shadows, min/max animations survive); framelessness
  lives in `WM_NCCALCSIZE` (strip + maximized work-area clamp) and
  `WM_NCHITTEST` only. `Transparent` adds blur-behind with an empty
  region + `WM_ERASEBKGND` skip + style-aware `fit_client_to_scene`
  (`WS_CAPTION|WS_SIZEBOX` masked). Awaiting user re-proof on both
  styles. Gates: fmt clean, workspace clippy zero warnings,
  `oppa-shell-win` 30/30 (incl. live frame-bits, blur HRESULT,
  erase claim, clamp math).

## Now (in progress)

- Android input rewrite (decisions 452–462, complete): shared `oppa::input::actions` funnel + `AndroidInput` controller (runner routes keys/menus/toolbar through both); renderer-drawn selection handles end to end (`HandlePaint` from laid clusters, shared-builder bar + knob emission + DOM twins, slop-confirmed drags into `move_handle_edge`); renderer-drawn text toolbar as a scene overlay (shared vocabulary, host signal + appended overlay `Portal`, geometric taps, mirror-only proxy sync, CSS-px conversion + pre-layout measured hug per decision 457); proxy system-menu retirement (all three `OppaIme.java` copies drop interceptor/trigger/show-hide, Rust drops the five `onMenu*` natives); masked display mapping + caret toolbar + contextual items + `Delete` + enlarged handles + key tracing (decision 459); hardware/adb key fallback commits text (decision 460: unmapped printable keys commit through the soft `CompositionCommitted` funnel — letters shift-aware, unshifted digits, space — because the proxy `onKeyDown` path never fires and Gboard refuses the password variation; shell digits 7–16 map onto framework 0x30–0x39, closing a `KEYCODE_1`-as-Backspace collision); live composition render + finishComposing bridge (decision 461: `composition_epoch` re-renders the display composite inline while composing; new native → queue → shell → pump step commits the open buffer with its own text so space keeps the word). Thumb rule learned mid-round: conditional/always wraps break focus/layout/miss — the portal-append preserves all three. Device proof this round (Medium_Phone_API_36.1, `apps/login` release): boot + focus + typing end to end, triple-tap selects with both handle knobs + scene pill, Cut tap empties the field through the full loop with the text landing in the system clipboard (Gboard chip), toolbar dismisses on collapse — `target/avd-cut3.png`. Paste read-back proven the same pass (tap Paste restores the text with the OS "pasted from your clipboard" toast — `target/avd-paste.png`). Decision-459 pass (same device, rebuilt APK): Gboard-typed 8 chars land in the password field (content_len=8), double/triple-tap selects (0,8) with the scene pill showing Select-all/Delete/Paste (no Copy/Cut) and enlarged knobs — `target/triple459.png` — Delete clears to collapsed with the clipboard untouched, email unmasked unaffected; the login password placeholder is now `Enter password` (literal bullets were indistinguishable from masked content). Decision-460 pass (same device, rebuilt APK): `adb shell input keyevent` letters/digits/space commit into the password field (`text-commit` log lines, dark content dots — `target/pw_digit460.png`) while email still single-commits through Gboard with no fallback interference (`target/mail_fix460.png`). Decision-461 pass (Medium_Phone_API_29, rebuilt APK): Gboard taps render live preedit (`target/r461_live.png`), space keeps "ok " with typing continuing to "ok x" (`target/r461_space.png`, `target/r461_x.png`), password hardware fallback intact (`target/r461_pw.png`). Blink repaint fold (decision 462: the runner settles + reads the flip and lets the outer repaint own the single raster — ~1050 ms/keystroke cycle → ~510 ms at the 200 ms quantum on Medium_Phone_API_29, input work ≈0 throughout; remainder is rig-bound software raster + swapchain). Lesson: a stale-installed `cargo-oppa` silently injects stale Java (reinstalled from tree before the proving build). Next: hardware-keyboard chords — needs hardware, see Blocked. Gates: fmt + clippy zero warnings (workspace + Android-target) + wasm + javac×3 green; `oppa` 244/244, `oppa-controls` 147/147, `oppa-app` 83/83, `oppa-shell-android` 54/54, `cargo-oppa` 25/25, m5 16/16, m7_dom 50/50; full workspace green except the documented transient Win32 clipboard-contention flake.
- Flake quarantine CLOSED (decision 417, gates green 2026-10-04): the Win32 proc's live
  `GetKeyState` Shift/Ctrl sampling now consults a test-only `KEY_OVERRIDE` table first
  (empty = live, production path unchanged); both pointer-test `hidden_shell` helpers pin
  Shift+Ctrl released. Proven under fire: full workspace per-suite identical to the locked
  baseline with Shift physically held for the entire run (`target/probe-shiftheld-full.log`,
  101/101) as well as in the normal record.
- Firefox automation UNBLOCKED (tooling proven 2026-10-04, was Blocked): Firefox 157
  installed + puppeteer-core 23 drives it over native WebDriver BiDi — spawn headless with
  `--remote-debugging-port`, `connect({browserWSEndpoint: 'ws://127.0.0.1:PORT/session',
  protocol: 'webDriverBiDi'})`, full newPage/goto/evaluate proven live (`CONNECTED
  firefox/157.0`, `EVAL 2`). (`puppeteer.launch({product:'firefox'})` does not take here;
  manual spawn + connect is the recipe.) The full-leg BiDi port of the CDP harness
  (`harness.mjs` + parity/dom_text/sink suites) landed: `spike/web/driver.mjs`
  (Firefox spawn + BiDi connect) plus Firefox arms in `parity`/`sink`/`webapp`/`dom_text`/
  `harness` (decision 418; Edge blocks byte-identical, CDP kept on the Edge arm). Prior
  Firefox results (fixtures-independent legs, still current): `webapp` PASS; `sink` verdict
  == Edge; harness c2:14/c4:3 complete (c4 17/18 identical — CJK dbl-click is the known
  segmentation class; c3 skipped — no BiDi IME primitives).
- Browser-rig hygiene round 1 CLOSED (decision 419, gates green 2026-10-04): the committed
  `spike/results` fixtures (`dom_field.html`, `parity_page.html`, `parity_expected.json`)
  regenerated from a clean-env m7_dom run (all four dirty-env vars unset) — the debug-overlay
  badge is gone (no `00ff66`/`1e1e1e`/FPS text; pids renumbered from the engine, engine wins).
  Click-scan suites green on the clean fixtures on both browsers: `parity` 10/10 Edge +
  10/10 Firefox (record-only text-height row listed, never gating), `dom_text` 0 failed steps
  Edge + Firefox. Key finding: the dirty 13/14 `parity` failure (badge-text width dw≈10.77)
  was badge dirt, not a stale corpus — clean fixtures gate 10/10 with no expectation edit,
  so round 2 scope narrows to the sink role-count rebase (+ verify parity stays green).
  Known tolerance class observed, not a failure: `undo_granularity` records split on Edge
  (`aHello world`) vs coalesced on Firefox (`Hello world`) — both pass by suite design.
  Test-only lint fix in the same change: `#[allow(clippy::module_inception)]` on the lifted
  `browser::browser` module (decision-416 paths preserved; zero behavior change). Full
  workspace clean-env run 101/101 green, per-suite multiset-identical to
  `target/round-409-cleanfull.log` (dirty `baseline-401-final.log` stays 23 red by the
  standing rule). Logs: `target/round-419-full-clean.log`, `target/round-419-parity-edge.log`,
  `target/round-419-domtext-edge.log`, `target/round-419-parity-firefox.log`,
  `target/round-419-domtext-firefox.log`, `target/gate-419-{fmt,clippy,wasm}.log`.
  Remaining hygiene: sink expectation rebase done below (round 2), Firefox compare leg (round 3).
- Browser-rig hygiene round 2 CLOSED (decision 420, gates green 2026-10-04): stale rig
  expectations rebased to the current engine/app (engine wins, re-derived never tuned).
  `parity` needed NO rebase — 10/10 Edge + 10/10 Firefox on the round-419 clean fixtures
  (the dirty 13/14 row was badge dirt, confirmed closed). `sink` thresholds rebased in
  `spike/web/sink.mjs`: `combobox` 1→2 (Select + DatePicker, both `Semantics::combobox` —
  `select.rs:165`, `date_picker.rs:357` decision 370) and `tab` 4→5 (SinkTab gained Views —
  `kitchen_sink.rs` decision 380); both browsers had reported `{1,1,1,2,5}` live against
  the stale `{1,1,1,1,4}` gate. After the rebase the full sink story gates clean on both
  browsers (roles/check/pick-Mint/step-55/typing-Ada/chips/modal/fetch-mock/file/count/demo
  all true, `pass=true`; `webapp` PASS both arms unchanged). No product-code change, no
  rig-mechanism change (threshold constants + comment only; Edge/CDP blocks byte-identical,
  Firefox arms addition-only untouched). Full workspace clean-env run 101/101 green,
  per-suite multiset-identical to `target/round-409-cleanfull.log` (dirty
  `baseline-401-final.log` stays 23 red by the standing rule). Logs:
  `target/round-420-full-clean.log`, `target/round-420-{parity,domtext,sink,webapp}-{edge,
  firefox}.log` (plus pre-rebase mismatch captures `target/round-420-sink-{edge,
  firefox}.log`), `target/gate-420-{fmt,clippy,wasm}.log`.
  Remaining hygiene: Firefox compare leg done below (round 3).
- Browser-rig hygiene round 3 CLOSED (decision 421, gates green 2026-10-04): the Firefox
  compare leg — `compare.mjs` judges Windows-vs-Firefox from a Firefox-produced `web.json`
  (`OPPA_BROWSER=firefox` → `verdict-firefox.json`) alongside the unchanged Windows-vs-Edge
  verdict (proven byte-identical, 55,837 bytes same hash). Criterion computations shared;
  Firefox-only additions: c3 rig-limitation skip (BiDi has no IME primitives — recorded,
  never scored), browser labels, output name, and a `divergence_classes` summary reusing
  CJK segmentation / BiDi digit tails / undo-shape tolerance / c3 rig-limitation (plus an
  `unclassified` bucket that stays empty). First Windows-vs-Firefox verdict: c1 FAIL (shared
  Windows-arm evidence, identical to Edge) | c2 FAIL (sweep 340/411, selection 15/21 —
  strict gate, same philosophy as the Edge FAIL) | c3 SKIP_RIG_LIMITATION | c4
  PASS_WITH_DOCUMENTED_DIVERGENCES (latin 9/9, multibyte 4/5 CJK rule, undo 3/4 burst).
  Buckets: CJK 2, word-rule 5, BiDi tails 67, tie-break 4, undo-shape 1, c3-skip 1,
  unclassified 0. New of the same class (flagged, fixed in-arm): the Firefox c2/c4
  dbl-click + c2 shift-click shapes never delivered real input over BiDi (selections
  collapsed — down/up pairs and modifier-less clicks); diagnosed with throwaway probes
  (created, run, deleted) and repaired Firefox-only to the dom_text-proven shapes
  (`page.mouse.click` counted clicks, Shift held around the click — same trusted path,
  no expectation touched). One genuine browser-vs-browser divergence recorded: ZWJ-string
  dbl-click selects [0,4] on Edge vs [1,4] on Firefox (both differ from the session [1,1] —
  word-rule class). Full workspace clean-env 101/101 green, multiset-identical to
  `target/round-409-cleanfull.log`. Logs: `target/round-421-{harness-{edge,firefox{,2,3}},
  web-{edge,firefox{,3}},probe{,2},compare-{edge-before,edge-after,firefox{,2}},
  verdict-{edge-before,firefox{,2}},full-clean}.log/json` + `target/gate-421-{fmt,clippy,
  wasm}.log`. Hygiene close-out complete: fixtures clean (419), expectations rebased (420),
  Firefox leg recorded (421).
- Debt round 2 CLOSED (decisions 410–416: `m4_cpu` tests → hub + 13 area modules,
  `oppa-cpu/backend.rs` → hub + `raster`/`replay`, `oppa-shell-win/tsf.rs` → hub +
  `store`/`text_store`/`sinks`, `oppa-shell-linux/input.rs` → hub + `events`/`shell`/`tests`,
  `m9_reload_gate` tests → hub + 6 scenario modules, `m3_layout` tests → hub + 15 area modules,
  `m7_dom` tests → hub + 14 area modules;
  every shared rig in hub, every public path
  preserved, full workspace per-suite identical to the locked baseline after each commit;
  no >1000-line files remain outside the excluded spike rig;
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
`oppa-controls/tests.rs` by decisions 406–408, `m4_cpu` tests by decision 410,
`oppa-cpu/backend.rs` by decision 411, `oppa-shell-win/tsf.rs` by decision 412,
`oppa-shell-linux/input.rs` by decision 413, `m9_reload_gate` tests by decision 414,
`m3_layout` tests by decision 415, `m7_dom` tests by decision 416).

## Blocked (with blocker)

| Item | Blocked by |
|---|---|
| Android on-device sustained/thermal numbers + GPU re-proof (prior numbers Vello-measured) | RMX3370 attached (login boots + renders); sustained/thermal protocol + custom-GPU numbers not yet run |
| Android clipboard + selection-toolbar device proof (decisions 443, 452–461: hardware-keyboard copy/paste round-trip + scene-bar tap cycle) | System-toolbar half PROVEN on-device by decisions 446–450 (system pill, Copy→system clipboard, Paste→read-back, hide on collapse, hold-to-select, handle drags, drag-state) — then retired by 455–456. Scene-bar cycle PROVEN on-device 2026-10-07 (Medium_Phone_API_36.1, `apps/login` release: type → triple-tap selects with renderer-drawn knobs + scene pill → Cut empties the field with the text in the system clipboard (Gboard chip) → Paste restores it with the OS "pasted from your clipboard" toast; `target/avd-cut3.png`, `target/avd-paste.png`). Placement fix 457 verified the same pass (pill floats above the field). Masked cycle PROVEN the same day (decision 459: password type → double/triple-tap (0,8) → Select-all/Delete/Paste pill with no Copy/Cut → Delete clears with the clipboard untouched; `target/triple459.png`). Hardware/adb typing PROVEN the same day (decision 460: `adb shell input keyevent` — the same mechanism as a physical keyboard — commits letters/digits/space into the password field while email still single-commits through Gboard; `target/pw_digit460.png`, `target/mail_fix460.png`). Live-composition PROVEN on Medium_Phone_API_29 (decision 461: taps render preedit inline, space keeps the word with typing continuing, password fallback intact; `target/r461_live.png`, `target/r461_space.png`, `target/r461_x.png`, `target/r461_pw.png`). REMAINS on hardware: (a) hardware-keyboard Ctrl+C/X/V chords — needs a device with a hardware keyboard (or working `adb input key*`). Empty-field hold-to-paste stays host-test-only (`adb swipe` ignores hold duration) + code reasoning. |
| Live human passes (IME composition manual, theme flips, menu/tooltip eyeball, close-veto app) | Needs a person at the keyboard |
| macOS/iOS shells | No Apple hardware; excluded by design |
| Weak-tier silicon validation (Mali-G52/Adreno-610 class) | Needs hardware wall |
| PWA load-once-offline hand pass | Needs an interactive browser session |
| Safari pass | No Safari binary or Apple hardware on this machine |
| Multi-window `DesktopLoop` (secondary `SurfaceId` windows) | Deferred by plan; per-surface granularity is out — re-decision required to revive |
| Variable-height virtualized rows (prefix-sum index) | Deferred by plan — re-decision required to revive |
| Android native file-picker backend (open/save/folder via SAF/intents) | Open flow wired end to end (decisions 424/426/427: byte-read proof + ANR-safe loops + trampoline on Browse with cache-copy; registration proven on device) — the pick gesture itself is manually testable now (needs a human on the picker UI); save/folder stay follow-ups |
| Web native file-picker backend (open/save/folder via file input / File System Access) | Host half landed (decision 423: `WasmHost` pick rendezvous + `ScriptedOpener`); chooser glue blocked — hidden `<input>`/FS-Access + `FileReader` bytes + bindgen surface need browser gesture design (FS Access Chromium-only); save-bytes unfitted to the blocking contract |
| `cargo oppa reload-check` as a G9 substitute | Unlisted extra scope, not approved — G9 is the doc recipe |

## Known issues (not code bugs unless noted)

- Environmental flakes: `oppa-shell-win` live-Shift sampling (production still samples
  the live keyboard by design — real input; pointer-test suites pin Shift+Ctrl released via
  the decision-417 override, proven immune with Shift held for a full run); sandboxed clipboard returns NULL handle with `ERROR_SUCCESS` (loud skip); transient clipboard box contention in full-suite runs.
- Stated edges: ZWJ-inside-regional-indicator double-click selects the joiner alone (pathological); right-held single-gesture drag-select stays out (tap-to-open).
- Standing debt: hot-reload true unload is v2 scope; inline-child effect precision stops at the window (open architecture question); headless TaskStudio quiescence resolved (decision 383 — gated unshaped text re-measure); full studio tests remain bounded-frame by doctrine; dark-OS web boot heals on first interaction (the boot take consumed the stanza into a discarded patch while the full page's `<body style>` is dropped by the `innerHTML` mount — `boot_shell` now re-arms it, so the first patch re-emits the chrome; settled no-interaction pages still take nothing and keep the static light shell).
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
