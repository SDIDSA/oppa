# Project state (current)

Status: current. Last verified: 2026-10-14 (through Decision 522).
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
- Focus signal binding on fields (decision 480, 2026-10-12):
  `TextInputProps` and `TextAreaProps` accept an author-owned
  `focused: Option<Signal<bool>>` (builder `.focused(sig)` /
  `.focus_signal(sig)`); field instances synchronize their
  `ctx.focused()` signal into the author's signal untracked,
  enabling compound container chrome (such as `apps/sera-login`'s
  `credential_field` chrome border) to react to child focus without
  cascade or remounting; unset signals keep byte-identical trees.
  Gates: fmt clean, workspace clippy zero warnings, wasm check
  clean, `oppa-controls` 152 tests green (incl. new focus-and-blur
  signal sync test), `sera-login` 8 tests green.
- Optical text input caret tuning (decision 481, 2026-10-12):
  Text input carets were previously anchored at raw unconstrained line
  ascent/height bounds (`rect.y = -metrics.ascent`, `rect.height = ascent + descent` ≈ 1.33em),
  causing the blinking bar to extend ~5px above capital letters and
  float into the field's top padding. `focused_caret_paint` now computes
  caret height and baseline vertical offset optically:
  `caret_h = (em * 1.10).min(line_h)` and
  `y_offset = (line_baseline - em * 0.85).max(0.0).min(line_h - caret_h)`
  (where `em = line.em_size`), framing capital letters snugly with ~1.5px
  headroom across empty and populated text fields; headless/unlaid fallback
  uses 0.82h centered. Gates: fmt clean, workspace clippy zero warnings,
  `oppa` 16+10+5+7+15+18+10+7+4+6+5 tests green, `oppa-controls` 152 tests green,
  `sera-login` 8 tests green.
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
  (`WS_CAPTION|WS_SIZEBOX` masked). User re-proof 2026-10-12:
  Transparent shows the desktop around the floating card with a live
  counter (screenshot); Extended tiling/snap still open. Gates: fmt clean, workspace clippy zero warnings,
  `oppa-shell-win` 30/30 (incl. live frame-bits, blur HRESULT,
  erase claim, clamp math).
- Toggleable native caption buttons (decision 485, from the linked
  JavaFX `EXTENDED` spec's superimposed-buttons requirement):
  `WindowOptions::with_native_caption_buttons(bool)` (default true,
  `Extended`-only, Windows-only — Linux has no OS zones, so bridged
  buttons always own the press there). The Windows runner publishes
  `native_caption_zones` (the `HeaderBar` caption-button boxes, so
  zones align with visuals by construction) and the shell answers
  `HTMINBUTTON`/`HTMAXBUTTON`/`HTCLOSE` ahead of everything — real
  actions with the close veto intact (hover glow paints in-scene —
  decision 486 below — since DWM draws none on a stripped frame).
  Apps wire `on_*` unconditionally as the Linux/off fallback
  (shadowed harmlessly where the OS answers first). Opt-out demos:
  sera `extended` example + Stylebook `--native-buttons` (footer
  names the live mode; 5/5 headless). Gates: fmt clean, workspace
  clippy zero warnings, `oppa` 249/249, `oppa-app` 91/91, shell-win
  live HT-code proof green. User-verified live: zones fire
  (hundreds of hit-test hits, close acting) but nothing glows —
  DWM paints no hover on a stripped frame.
- In-scene native caption hover glow (decision 486): shell
  publishes the hovered zone per `WM_NCHITTEST` (cleared on
  `WM_NCMOUSELEAVE` via non-client tracking) and the Windows runner
  drains it per pump into a change-gated host signal
  (`caption_hover`, the toolbar-model precedent); `HeaderBar`
  paints the glow (signal red + light ink on close, themed wash
  elsewhere — paint-only, geometry identical). Gates: fmt clean,
  clippy zero warnings, `oppa` 250/250, `oppa-app` 92/92,
  `oppa-controls` hover paint green, shell hover set/clear green.
  Awaiting user glow + snap-flyout proof.
- True Win11 caption glyphs (decision 487, user-asked): Segoe-metric
  strokes on a 16px box replace the ASCII approximations, baked per
  palette at 4x (`CaptionIcons::deposit`, same-id replace) with the
  close-hover white shared; the 46px button cell centers the 16px
  leaf on both axes (center deltas < 1px, geometrically proven).
  Gates: fmt clean, clippy zero warnings, `oppa-controls` 160/160
  (incl. AA-fringe, centering, theme-swap tests), `stylebook` 5/5.
- Self-performed native caption clicks (decision 488, user-verified
  live: hover glows, clicks dead — `DefWindowProc` tracking needs
  real caption metrics): the shell performs caption clicks itself
  (minimize/maximize via the bridge's `ShowWindow` calls, close via
  the `CloseRequested` veto path, double-clicks swallowed). Gates:
  fmt clean, clippy zero warnings, shell-win live click/double/
  queue proofs green. Awaiting user click + snap-flyout proof.
- Native borders/shadows + flush caption buttons (decision 489,
  user-asked): every frameless window extends the 1px DWM glass
  frame (the `WM_NCCALCSIZE` strip takes border+shadow with the
  visuals — border line and drop shadow survive; corners/colors stay
  OS-owned), and `HeaderBar` drops its trailing pad whenever buttons
  render so the close box ends exactly at the bar edge (zones follow
  by construction). Gates: fmt clean, clippy zero warnings,
  shell-win glass-frame proof green, header dock proof green.
  Awaiting user border/shadow/dock proof.
- Native resize metrics + no OS snap flyout (decision 490,
  user-asked twice): delivered border points now resize by genuine
  OS frame metrics (the proc consults `DefWindowProcW` first and
  accepts only sizing codes — zones/drag still primary, button codes
  stay ours-alone, `HTCAPTION` maps to client so nested controls keep
  working). But the snap flyout is unreachable: `DWMWA_CAPTION_BUTTON_BOUNDS`
  is get-only (the `DwmSetWindowAttribute` publish failed loud with
  `E_INVALIDARG` and was reverted, not shipped), and a
  `WindowFromPoint` probe locks that outside points never route to
  stripped windows — true outside-grab needs an owned transparent
  margin ring (major surgery, explicitly deferred). A scene-drawn
  snap flyout remains the deterministic follow-up if wanted. Gates:
  fmt clean, clippy zero warnings, workspace suite green.
- Real custom-frame geometry via `WM_GETTITLEBARINFOEX` (decision
  491): the runner publishes the title-bar box on the same sync and
  the shell answers the OS-documented custom-titlebar query with
  real bar/min/max/close rects in screen coordinates (honest
  states, invisible help) instead of phantom standard metrics —
  the channel DWM features and accessibility tools read. Gates: fmt
  clean, clippy zero warnings, core bar-box union, shell live-proc,
  runner end-to-end proofs green. Awaiting user snap-flyout proof.
- Outside resize grabs + Win11 snap-layouts flyout (decision 492,
  gate green 2026-10-12): resolves both remaining Windows desktop
  Extended issues (outside resize grabs over dropshadow + windowed snap
  layouts flyout) following the JavaFX `StageStyle.EXTENDED` architecture
  (OpenJDK PR #1605 / JDK-8313424): Extended `WM_NCCALCSIZE` preserves
  `DefWindowProcW`'s non-client border insets (left, right, bottom ==
  `SM_CXSIZEFRAME + SM_CXPADDEDBORDER`) while restoring `new_size.top = original_top`
  (plus `SM_CXPADDEDBORDER + SM_CYSIZEFRAME` when maximized, matching the
  monitor work area exactly). `WM_NCHITTEST` checks caption button zones
  first (beating the top sizing band and `DefWindowProcW` in the button
  top sliver `0.0 <= pt.y < top_border_height`), then passes non-client border
  hits directly from `DefWindowProcW` (`HTLEFT`, `HTRIGHT`, `HTBOTTOM`,
  corners over dropshadow) while client points query drag regions
  (`HTCAPTION`, top edge `HTTOP`/corners), and client body (`HTCLIENT`).
  Standard desktop frame negotiation aligns with Win11 DWM frame
  expectations to enable native Snap Layouts flyout on maximize button hover
  in windowed mode (awaiting live verification). `fit_client_to_scene`
  compensates HWND bounds with `+ 2 * frame_x` width and `+ frame_y` height
  so scene size matches client rect exactly; `title_bar_info` maps client coords
  to screen via `ClientToScreen`; 1px glass frame extension scoped to non-Extended
  frameless styles (real frame already provides border/shadow). Gates: fmt clean,
  clippy zero warnings, wasm check clean, full workspace tests green (incl.
  positive OS `WindowFromPoint` margin routing delivery proof, outside resize,
  button top sliver, maximized work area, and snap tests, updated title_bar_info,
  `oppa-shell-win` 39/39, `oppa-app` 93/93, `stylebook` 5/5). Awaiting user
  live snap-flyout proof.

- `HeaderBar` custom chrome slots + Sera goes `Extended` (decision 493): `HeaderBarProps` gains opt-in `title_view` / `tools_view` factories (the Splitter-pane `Rc` precedent — `VNode` is move-only), rendering inside the bar node so drag, zones, glow, and docking work unchanged; unset slots render exactly the pre-493 tree. The sera-login desktop binary mounts `sera_extended_root` under `WindowOptions::extended()` (merged 48px chrome, native zones on, poll-hook bridge as fallback) while web/Android keep `sera_root` (`Decorated`); builders shared, never forked. Gates: fmt + clippy + wasm clean, header_bar 12/12, sera-login 13/13 green.
- Window-chrome philosophy redo (decision 494): `Decorated` = native frame + header + behavior, `Undecorated` = truly bare (`WS_POPUP`, everything `HTCLIENT` — no resize/snap/shadow/glass), `Extended` = native frame with no OS header and optional native buttons. `HeaderBar` is no longer a prebuilt bar but the drag-region trait (transparent marker filling available width, packing to content) with separate `CaptionButtons` markers placed inside the composed chrome (493 slots removed — one content factory); native-off declares no buttons at all. Found en route: `window_drag_rects` published degenerate zero-area boxes (now filtered) and composed chrome hit a fill-width-into-intrinsic-Row zero trap (the marker fills by construction). Demos: Stylebook chromes `Extended` only, sera-login main keeps its merged chrome (+`--bridged` drag-only mode), the dummy `examples/extended.rs` showcase deleted. Gates: fmt + clippy + wasm clean, `oppa-controls` 12/12 rewritten, `oppa` window 7/7, `oppa-shell-win` 40/40, `oppa-app` 93/93, `sera-login` 14/14, `stylebook` 5/5 green.
- Maximize cell swaps to the restore glyph (decision 495, user-found live: the square never changed on maximize): new change-gated host `maximized` signal (`Ctx::maximized`, default windowed) drained per pump from the live OS state (`IsZoomed` on Windows, `is_maximized` on Linux); `CaptionButtons` wears the overlapping-squares restore glyph while set (user-supplied filled set for all four glyphs, baked themed like maximize, same centering/AA proofs). Gates: fmt + clippy + wasm clean, core signal + swap tests green; user glyph set at 10px still 13/13.
- Right-click raises the window menu on Extended bars (decision 496, user-found live): `WM_NCRBUTTONUP` over `HTCAPTION` arrived unanswered, so the shell shows the system menu explicitly over drag chrome and caption zones (native parity, buttons included), forwarding the command as `WM_SYSCOMMAND` so Close honors the veto; no borrows span the modal loop. Gates: fmt + clippy + wasm clean, pure trigger + live `#32768` proof green (watcher `WM_CANCELMODE` dismissal — `EndMenu` and synthesized Escape both wedged on thread/focus).
- Live-resize white sliver (decision 497, user-found: a white sliver lagged one resize step behind the texture): each step's `WM_PAINT` `BeginPaint` filled newly exposed strips with the `COLOR_WINDOW` brush after the step's present had landed (flip-model swapchains share the surface with GDI, so the fill overwrote fresh pixels). No style GDI-erases anymore and the class carries a NULL brush — the framework owns every pixel. Gates: fmt + clippy clean, erase-skip proven on all four styles.
- Modal sizing defers presents to release (decision 498, superseded below by the throttle): `WM_SIZE` steps skipped the resize hook inside the modal loop (DWM stretches instead) and `WM_EXITSIZEMOVE` fired it once with the final live size; non-modal resizes (maximize button) keep the instant path.
- Resize-step throttle at ~30Hz (decision 498 rewrite, user-pushed twice: deferral rejected — mid-frames must keep presenting — and the compositor never stretches, only crops-or-fills): applied steps throttle to one per 33ms (measured CPU-side cost is 0–1ms, so the lag lives downstream in reconfigure churn + vsync-blocked acquire), mid-frames show the last applied frame, and `WM_EXITSIZEMOVE` still fires once so release lands crisp. Gates: fmt + clippy clean, shell + runner resize tests green.
- App-bundled fonts + Checkbox sizing props (decision 499, user-asked: Readex Pro default for sera-login): `WindowOptions::with_app_fonts(family, faces)` builds a rustybuzz service from the app's bytes on both desktop runners (replacing the system service) with every face injected into both raster backends before first paint (`None` keeps system fonts — other apps untouched); `WasmHost::boot/mount_root_with_fonts` + a 6-arg `export_app!` arm do the same for web (statics for measure, the variable file as the single display `@font-face` — DOM emits no `font-weight`, so web weights stay browser-synthesized like the shipped DejaVu); `CheckboxProps` gains `box_px`/`box_radius`/`label_style` (20.0/4.0/body_secondary defaults keep every existing oracle byte-identical). Android runner unwired (no device to prove it — system fonts there, stated). Readex Pro statics (400/600/700, instanced via fonttools) + OFL live in `oppa-fonts`. Gates: fmt + clippy clean, wasm build clean, oppa-app 94/94, oppa-web 18/18, sera-login 15/15 (incl. BOLD-shapes-Bold-face proof + 454px/155px/160px geometry under Readex metrics).
- `cargo oppa add-font` (decision 500, user-asked: any developer font — Poppins proven end to end): vendors a TTF/OTF (static or variable) into an app — pure-Rust sfnt facts (magic, Unicode family, OS/2 weight, `wght`-axis detection; woff/woff2/TTC refuse with instructions), variable weights instanced via local fonttools (loud when absent), per-weight outputs under `fonts/`, sibling-license copy with a family-relevance rule (a stray other-family OFL never launders in; otherwise a confirm-before-shipping warning), whole-file `src/fonts.rs` regeneration from scanning `fonts/`, and a wiring report (mod line + desktop/web snippets + remeasure note). No new product deps. Gates: 10/10 new tests (parse facts off repo fonts, static vend + module + shaping proof, family/weight/layout refusals, license preference, fonttools-gated variable e2e), clippy clean; live CLI run vendored Poppins into a fresh scaffold and the wired app `cargo check`s green.
- Barless centered login redesign (decision 501, user-spec: two-panel card + copyright, no title bar): the in-app bar, emblem, and icon cluster are gone on every frame (Extended keeps the bare native frame with an empty drag region — window move now lives on the OS); the page is equal flex fillers + card + footer (20px bottom) with the haze layer first and hover tips last. Card is a centered row (radius 20, 1.3 outline): 450px panels at 25px padding with 12px rhythm, full-height 1.3 divider, left list in a 236px scroll viewport (exactly three rows — more scroll instead of growing), title 18 bold + subtitle 14 with a flex spacer. Rows (15px pads, 1.3 border, Medium 16/12 text) and trash (20px) are hover-owning components: row tints + primary outline on hover, trash reddens with an immediate hand-rolled error-red tip above it (the `Tooltip` control renders below with a dwell and no tint — all three miss). Fields: 14px Medium labels inset 5px, 18px icons, 12px gaps, 14/10/12 pads, radius 12, 1.3 outline; button centers label + 20px icon as a group with a balancing 20px spacer; footer is 13px secondary. `Checkbox` gains `label_ink` (row pad/gap become the 4/10 defaults). Gates: sera-login 16/16 (incl. pointer-driven tip test + filler/list/divider geometry), controls 165, wasm clean; measured 901px card, equal 157.8px fillers, 236px list, full-height divider.
- Login inputs + transparent strip chrome (decision 502, user-spec refinement): field chrome is 12px left / 10px icon gap (text at 40px, 12px right, 14px top/bottom), 150px minimum, radius 12, 1.3 outline, focus-blue/error-red edges; errors render below in a right-aligned slot (6px left, 4px bottom); placeholders ride primary-white at 70% via a new `TextInputProps.placeholder_ink` (all other sites pass `None`); subtitle floats 27px over the email label (12 + 3px strip + 12, no divider); corner hazes tightened to blur 30 with 30px+ bleed. The transparent top strip returns on both frames (thin, full-width, click-through `HeaderBar` content on desktop): 7/8/6/8 left group with a 20px logo mark + 16px SemiBold tertiary `Sera`, top-right group (2px pads, 10px gaps) of bell/info/green-network/overflow + a 0-wide end spacer; native buttons stay at height 0. Gates: sera-login 16/16, controls 165, testkit, dom catalog, app 94, web 18, wasm clean; measured bar 1400×33, right group docked at x=1398, error row right-aligned under the box.
- Dentacore tooth mark in the bar (user-supplied `dentacore/.../logo.svg`): single-path tooth with nested group/path transforms — vendored as `icons/logo.svg` with the transforms flattened into 0..48 coordinates (the vector loader scales raw numbers, never transforms), rendered as a native 20px tertiary `Path` leaf replacing the drawn disc emblem.
- Input box height theory proven (decision 503, user-asked: boxes read ~8px tall): committed chrome was 58px — pads (28) + content (30) where the content band came from the fixed 30px slot/field while the 16px entry line is only 20px, so text floated with 5px slack per side. Fixed by removing the borderless `TextInput` pads (the container owns every inset now — the field takes exactly its text line) with the icon-row content height made pad-aware; sera field/slot heights go 30 → 20. Committed chrome reads exactly 48px (line + pads; the 1.3px border paints outside layout, so the rendered box is line + 30.6 — the theory, with the app's 16px line). Locked by a regression test; controls 165 green (borderless date-picker field gains room, the safe direction).
- Reserved error slots + dimmer placeholders (decision 504, user follow-ups): the error row always renders (idle: one space in the error ink — errors never move the form); placeholder ink drops from white-at-70% to white-at-35% (0x59); the follow-up below proved translucency itself was broken framework-wide.
- Color alpha honored by all painters (decision 505, user-found live: placeholders rendered plain white at any alpha): `ts_color`/`to_rgba_f32`/DOM hex all read alpha from style opacity alone, dropping the color byte — every translucent literal painted opaque. New `Color::alpha_u8` rule (top byte of 32-bit values, opaque for 24-bit literals, zero-value reads opaque with `TRANSPARENT` working only through builder skips) applied in all three converters; only the app's own translucent colors change behavior, every existing scene byte-identical by construction (all prior literals are 24-bit or full-alpha). Gates: new unit test, cpu/gpu/dom/app/web/controls suites green.
- Full-window backdrop (decision 506, user-spec: 3-layer backdrop): flat solid base #171A1E (#0d0d0d + 8% toward #8FB0E6) + two viewport-sized true radial fills — primary-wash #1D242E low-left at (30, h-30), accent-wash #272624 high-right at (w-30, 30) (accents #6BA3FF/#FDC76F washed 93% toward base, verified byte-exact offline; rims target the base itself, so opaque wash→base stops composite exactly like a transparent feather). Handlerless out-of-flow boxes paint first and ignore clicks. Geometry asserted headless (full-bleed boxes).
- True radial gradients in the framework (decision 507, user-caught: the discs were flat washes, not gradients): new `RadialGradient` style (node-local center + radius + two stops) lowered by the shared builder into 1px concentric solid-circle steps (largest first — the linear-strip doctrine radially; no new `DrawOp`), with the full refusal matrix (bg/linear/round/leaf mirrors), native `radial-gradient` CSS on DOM, and ring-count/color/conflict tests. Sera adopts it for both backdrop clouds.
- CPU glyph ink alpha (decision 509, user-found live on the CPU presenter: placeholders ignored their alpha there too): `rasterize_glyphs` read RGB off the ink and multiplied coverage by style opacity only — the same dropped-byte bug as decision 505, one layer deeper. Ink alpha now folds into the coverage product (premultiplied identity preserved); proven by a pixel test (50%-alpha black over white lands mid-gray). GPU text already multiplied via the instance color (verified in-shader); DOM was already correct.
- Framework `Link` control (decision 508, user-found live: the signup underline spanned the full panel width): new `LinkProps`/`Link` — content-sized label in theme primary with button semantics, press-to-activate (structurally handlerless when disabled/inert), hover-only/always/never underline via a paint-only inset band (layout never moves), pointer cursor when live. Sera adopts it (hand-rolled row link deleted; ink now theme primary per the "13 primary" spec). Gates: 4/4 new control tests (press, hover-only, modes, disabled), sera 16/16 incl. a hug-the-text width assert.
- Sera-login verbosity audit + shared ergonomics (decision 510, user-asked: the 1299-line page hand-rolled framework-shaped code — ~400 lines of SVG parse/scale, typography wrappers, field chrome, press-row button, page-level tooltip, caption outbox, style chains). Core gains `VNode::with_ink` + `TextBuilder::ink` (the wrapper as a chain link), `Path::from_svg` + `svg_path_data`/`extract_svg_path`/`scale_svg_path` (single-path SVG viewBox-scaled, loud on missing data), and `StyleBuilder` shorthands (`square`/`center`/`pad_all`/`pad_xy`/`pad_trbl` — pure aliases, oracles untouched); controls gain `Button` brand overrides (`fill`/`pressed_fill`/`ink`/`radius`/`text_style`, native `Path` vector-icon slot with dual-slot refusal, `FullWidth` true-center label + `full_width`/`content_pad` box), `Tooltip` `Above`/`instant`/`fill`/`ink` (below/dwell/surface unchanged), shared `FieldChrome` composite (label + chrome + reserved error slot, focus/error/idle edges), `VectorIcon` (parse/scale/leaf over `Path::from_svg`), `CaptionOutbox` + `take_actions` (the Stylebook/sera publish-once bridge — Stylebook keeps its own copy, migration is a follow-up), and chained `TextInputProps` builders (`debug`/`required`/`invalid`/`error_message`/`helper_text`/`width`/`height`); every addition defaults to the exact shipped tree. Sera rewrites onto them (`app.rs` 1299 → 893 lines: shared `sera_page`, `FieldChrome` fields, branded `Button` submit, `Tooltip` trash tips, `CaptionOutbox` bridge; same geometry/behavior). Pre-existing tree dirt fixed on the way: two `placeholder_ink`-less `TextInputProps` literals (studio/date-picker) that never compiled with the decision-502 field, and workspace-wide `cargo fmt` drift. Gates: fmt clean, workspace clippy + sera clippy zero warnings, wasm check clean, `oppa` lib 262 (incl. 7 new: shorthands, ink + refusal, from_svg + 2 refusals — `nativefs_tempdir` fails on sandbox os-error-5, untouched path), `oppa-controls` 181/181 (incl. 9 new: brand button + refusal, vector icon ×3, above tooltip, field chrome ×2 + builder chain, outbox), sera-login 16/16; full workspace suite green except 7 environmental targets (cargo-oppa 4, nativefs, e2e tempdir, cpu png write, android app-dirs — all sandbox os-error-5 — plus dom browser 2 on puppeteer EPERM and reload real_dylib on the sandboxed nested fixture build; hot-fixture itself checks clean, none touch the changed paths).

## Now (in progress)

- Layout animation, decisions 521-522 (2026-10-14, gates below): closes STATE Blocked row 1 (the layout-animation row — the deferred round, designed first, then built). Opt-in `Style.layout_anim(Transition)` (one spec per node; siblings share a timeline by declaring equal specs): position-only moves FLIP through the transform machinery (521 — `commit_box`/`reposition`/`set_box_h` all record old/new boxes; a per-pass merge runs tracks from the first old to the last new; host relativizes against recorded ancestors (transforms compose down every backend, so absolute deltas would double-apply), suppresses scroll-driven moves via per-container offset snapshots, culls off-viewport targets, skips stamped commits; flips live in a separate evaluator map composed outermost inside `resolve_transform` + the keyframe triple, keyframed so DOM steps them; mid-flight retargets add the residual onto the live offset) and explicit-px size changes run the fenced box interpolator (522 — one `BoxInterp` track type, settle-exact with device-grid rounding; children keep end-state boxes under an axis-aligned clip for accordion reveals; side table behind `effective_box` (committed boxes never see interpolated values — they would read as perpetual change); engine target always wins (restart-from-current, retarget-never-cancel); refusals judged on final spans at drain time (changed axis without explicit-px, text content; mixed move+resize on refused nodes jumps). DOM samples backend-owned track specs on its own clock (host settle never freezes a lagging backend — the exit-twin precedent). Found and fixed en route: (a) container placement bypasses `commit_box` (`reposition` translates committed boxes in place — capture unified across all three box writers with a per-run mount set); (b) multi-pass transients (measure-at-origin commits + container reposition in one pass — per-node span merging + final-box agreement filter); (c) float-strict classification (1-ulp recomputation wobble is not a move — epsilon kind tests, exact boxes); (d) unused-memo bindings never fire (pull semantics — the stamp test reads its memo); (e) root `.x()` pins at the origin (moving wraps nest one down in tests); (f) click-pump vsync flow settles short fences before first backend sync (web delivery proof uses a long fence; production rAF ticks flow ~16ms). Proven: evaluator flip/fence/retarget/compose units, 9 host tests (fence/flip/parent-relative-zero/scroll-suppress/retarget/dirt-bounded/refusal/stamp/undeclared-snap), builder fence oracle, 3 DOM tests (geometry/flip-step/clip), 3 motion room proofs incl. the shared-timeline invariant (card.bottom <= sibling.top every frame), web patch-JSON delivery test; 7th motion room live. Undeclared trees byte-identical.
Gates (CONTRIBUTING order, 2026-10-14): `cargo fmt --all -- --check` clean; `cargo clippy --all-targets` zero warnings; wasm check clean; full `cargo test --workspace --no-fail-fast` green (`target/round-521-full.log`, 106/106 suites, zero failure markers); motion app 25/25 + fmt/clippy green.
- Exit-deferral, decision 520 (2026-10-13, gates below): closes STATE Blocked row 1 and supersedes the appear-first exit clause — Modal/Toast/Menu dismissals fade out instead of snapping. Opt-in `Style.exit` (`Transition::exit()` 180ms Out mirror; tween-only by type); the outermost declaring removed root pins its subtree by value (structure + styles + boxes + payloads + owners; unlaid skips quietly) with one Once fade per node from frozen values (opacity → 0, rest pinned, delay honored, stamps snap). Loops refuse loudly, Once tracks freeze mid-flight. Ghosts paint through the shared builder after portals (portal-exact; transformed ancestors stated-approximate) with pinned-box damage; hit/focus/a11y excluded by construction; siblings close ranks; re-show cancels; observers fire at exit end. The 519 sweep exempts deferred owners (no eviction leak — proven: timers fire mid-exit, cleanups + orphan sweep at exit end) with post-exit collection. DOM keeps ghosts click-through with stepped inline values (twin fed on the presenter leg + every paint so diffless frames step), deleting at settle; pixel legs ride per-paint diff replay with zero backend changes. Found and fixed en route: (a) two frozen-clock hangs (clocked toast auto-dismiss + clocked menu drag-release now settle with flowing time past the exit); (b) the builder's empty-tree early return skipped the exit layer (ghosts paint with no live root); (c) DOM settle-drops counted no touched work (pure deletions still patch); (d) WasmHost pumps flow vsync time (tests tick relative, never absolute post-pump); (e) whole-subtree eviction hole (recursively evicted children escaped the timer/handler sweep — stale rows, live timers; closure sweeps on every path, single-record sweep evictions); (f) orphan-sweep over-collection (global GC re-seeded live slot/conditional state, breaking the studio menu; narrowed to just-released exit owners). In-flight gesture owners pin across both sweeps. Side effect, kept: the presenter twin feed also heals web keyframes/springs stepping (same unfed gap, headless-proven). Proven: evaluator synthesis/stamp-snap/loop-panic unit tests, host fade/release/hit-exclusion/re-show/519-no-leak tests, builder ghost oracle (+undeclared snap control), DOM keep/step/drop/cancel tests, web patch-JSON delivery test, motion overlay room fade asserts; all 6 rooms + 22/22 proofs green, undeclared trees byte-identical.
Gates (CONTRIBUTING order, 2026-10-13): `cargo fmt --all -- --check` clean; `cargo clippy --all-targets` zero warnings; wasm check clean; full `cargo test --workspace --no-fail-fast` green (`target/round-520-full.log`); motion app 22/22 + fmt/clippy green.
- Subtree-unmount instance retirement, decision 519 (2026-10-13, gates below): closes STATE Blocked row 2 — `Remove` ops carried no instance mapping, so removed subtrees' instances/timers/handler rows lingered (only root unmount evicted). Every `Element` stamps its creating instance (innermost `Ctx::child` wins, the handler-stamp twin; text leaves inherit the parent owner), retained per node; `Remove` carries the pre-retire owner and slot-recycles that re-home a survivor record the displaced owner. Host sweep per commit (candidates minus live owners): child-first `evict_instance` (LIFO cleanups cancel per-render timers first, parent links scrub so re-show re-seeds), leftover-timer sweep, handler-owner prune; effect-holding roots never swept (explicit unmount only — the first cut retired parked non-winning roots and broke two multi-mount controls tests, fixed in the same round). Tracks already retired above, observers keep fire-on-unmount. Quiet commits skip the census; owner metadata never enters plans (live trees byte-identical). Proven: hide-cancels-timer + re-show-reseeds + displaced-recycle headless tests (both new-behavior tests fail with the sweep disabled — negative control), mid-flight-track lock test; all 6 motion rooms + 22/22 proofs green unchanged.
Gates (CONTRIBUTING order, 2026-10-13): `cargo fmt --all -- --check` clean; `cargo clippy --all-targets` zero warnings; full `cargo test --workspace --no-fail-fast` green (`target/round-519-full.log`, 104/104 suites, zero failure markers); motion app 22/22 + fmt/clippy green. One full-run-only miss, classified environmental: `oppa-shell-win` live `extended_ncrbuttonup_raises_system_menu_live` (real-HWND foreground/menu watcher, no component code on its path) missed under parallel load, green isolated and 44/44 for the crate alone.
- CPU translucent rounded-rect single-blend, decision 518 (2026-10-13, gates below): found live running the Motion Lab on the CPU presenter — the springs fade box rendered as a dim pane-grid instead of a uniform translucent fill (user screenshot; reproduced headless mid-flight + settled). Root cause (pre-existing, not T1b): `fill_rounded`/`fill_rounded_corners` bands+discs overlap each other, and translucent fills double-blend every overlap — invisible at opacity 1, which is why no oracle caught it (the springs fade is the first translucent rounded rect in flight; GPU SDF + DOM border-radius single-evaluate, so CPU-only). Fix: `fill_rrect` keeps opaque fills on the legacy path byte-identically and routes translucent fills through an opaque scratch + one alpha blit (the rotate detour paints opaque in-layer with the blit carrying the alpha — same single-blend law); all-zero radii keep the plain-rect fast path in both modes. Proven: uniform + per-corner translucent fills read identical center/edge/corner values inside the analytic single-blend band (m4), opaque control still exact, repro frames uniform after the fix (before/after PNGs compared visually, then deleted). Scoreboard: framework added tests 3 (m4 corners).
Gates (CONTRIBUTING order, 2026-10-13): `cargo fmt --all -- --check` clean; `cargo clippy --all-targets` zero warnings; full `cargo test --workspace --no-fail-fast` green (`target/round-518-full.log`).
- Motion lab T1b — rotate application, decision 517 (2026-10-13, gates below): the T1a rotate refusal is lifted everywhere (values already stored + interpolated — the evaluator raw-lerps, untouched). Core: `PaintXf` full 2D affine beside `AffineST` (zero-rotation degrades bit-exact through `to_full_affine`; residual `full ∘ st⁻¹` is exactly identity for ST-only chains, so every unrotated fast path stays byte-identical; the gate keeps only finite/positive). Builder: dual accumulation (ST bakes geometry exactly as before, full affine feeds damage + the per-op residual stamp in the same splice-safe per-node range); damage is the rotated AABB when live else the exact `map_rect`; clips stay axis-aligned AABBs (conservative, stated). Hit: full-affine walk with exact inverse (`accumulated_transform` now returns `PaintXf` — no app callers; origins/paging stay layout-space per 511, regions follow the visual). CPU: direct tiny-skia transforms (Rect, solid shadows, paths, images) + temp-layer detour for the axis-aligned manual helpers (rounded bands+discs, discs, glyph coverage, blurred-shadow kernels — axis-aligned paint, then blit through the residual with the target-space clip mask). GPU: instance `xf_abcd`/`xf_t` + shader multiply on all three instanced pipelines (SDF stays local-frame) + CPU-rotated path tessellation. DOM: `rotate()` between translate and scale (canonical order) + stepped inline. Transforms room: rotation loop (±45° loop on its own rect under the shared Play switch) + Twist 90° one-shot, boxes pinned, quiescence everywhere. Proven: 90° solid-rect CPU pixel-exact + layout-ghost bg (m4), rounded-rect detour interior-exact under a nested translate+rotate residual (m4), real glyph-coverage detour moves the darkest pixel to its rotated image with the old footprint vacated (m4 text), CPU/GPU interior-exact + tol-16 < 5% on hardware (gpu oracle), quarter-turn hit visual/ghost split (m5), rotated damage AABB (builder), CSS spelling (dom). Perf (dev profile, 800×600 single rounded rect, headless `build_full` + CPU paint over 50 frames): identity 0.003ms build + 2.8ms/frame; rotated-30° 0.003ms build + 13.0ms/frame (temp-layer ~4.7× unoptimized — desktop production GPU-serves so the CPU pixmap is skipped per 473; fallback/oracle path only; release unmeasured). Android: no device in this environment — the event-paced pump device leg is not run (recorded honestly; T1b adds no Android-specific code — rotation rides the shared plan through the unchanged `SceneSession` repaint). SPEC Transitions rewritten (residual bake + per-backend application + rotated damage + tol-banded pixels), DECISIONS 517, GLOSSARY Transform entry; ARCHITECTURE rendering row gains the residual clause. Scoreboard: app +~90 lines (spin/twist leaves + buttons + 2 headless proofs, 22/22); framework added tests 12 (style 4: gate/degrade/quarter-turn/residual; builder 1: residual+damage; m5 1 hit + accumulation updates; m4 2 pixel + chips; m4 text 1 coverage-detour; gpu 1 oracle; css 1 spelling).
Gates (CONTRIBUTING order, 2026-10-13): `cargo fmt --all -- --check` clean (workspace + app); `cargo clippy --all-targets` zero warnings (workspace + app); wasm check clean (five crates + motion lib); `cargo test` motion 22/22 (+ 2 unit); Android `--target aarch64-linux-android` check clean (same `motion_root` mounts via `android_main!`); full `cargo test --workspace --no-fail-fast` green 104/104, zero failures (`target/round-517-full3.log`; the mid-round `full.log`/`full2.log` runs are superseded — `full.log` caught 7 stale-fingerprint doc-test compiles from source edits made mid-run, `full2.log` predates the detour probes; both superseded, none committed).
- Motion lab round 5 — remaining rooms + runner liveness, decisions 515–516 (2026-10-13, gates below): springs room live (decision 513 end to end — translate/scale overshoot sampled by sweep, opacity clamp pinned, settle exact, boxes held); ambient room live (ping-pong orb loop + three-step completion chain driven by `observe_settled` from app code — mid-chain bg sequencing proves link-by-link firing, unmount retires the loop to zero demand); ease cycle gains Spring (5th). Found and fixed en route: (a) the Play-loops desktop freeze — bare `run_until_idle` spins forever under never-retiring loop tracks, pegging the UI thread and starving events — runners settle with `ComponentHost::pump_frames` instead (`PUMP_TICK_FRAMES` 4 paces timer ticks, `PUMP_INPUT_FRAMES` 64 drains input cascades; Once-only worlds quiesce early, identically); desktop both platforms (input + settle + blink + theme + mount paths), Android input/blink/boot, web `tick`/gesture pumps; (b) the motion-lab report that every animation jumps start→end — desktop `repaint()` built retained targets, never TIME-evaluated values — now builds `build_full_evaluated` at the frame clock (quiet trees byte-identical by the identity overlay; proven by a mid-flight gray-pixel test with a pre-fix negative proof). Found pre-existing (recorded, out of scope): pure `0x000000` is value-identical to `TRANSPARENT`, so black fills skip paint and black tweens snap (M6/103 + 505 design — ink-black still works; never use black as a tween/fill endpoint). Hardened finding: observing from a press closure can fire before the link's commit exists — link 1 observes from render (proven by timestamp trace: observe 250ms → fire 400ms → observe). Platform legs: `oppa build --web` ok (real wasm release + bindgen, `web/pkg` holds the motion bundle), `serve` 200 on `/`, Android `--target aarch64-linux-android` check clean (same `motion_root` mounts via `android_main!`), desktop release rebuilt for user verification. SPEC Reactivity gains both rules, DECISIONS 515–516, GLOSSARY pump entry. Scoreboard: app ~1650 lines; framework added tests 8 (pump_frames 2, desktop loop-input regression 1, mid-flight repaint 1, rooms prove the rest headless). Gates (CONTRIBUTING order, 2026-10-13, privileged shell): `cargo fmt --all -- --check` clean (workspace + app); `cargo clippy --all-targets` zero warnings (workspace + app); wasm check clean (workspace five crates + motion lib + motion Android target); full `cargo test --workspace` completely green (zero failures); motion 24/24; desktop release rebuilt with decisions 515–516 for user verification.
- Motion lab round 4 — control micro-tweens + overlay entrances, decision 514 (2026-10-13, gates below): `Transition::micro` (120ms Out) on Button press/hover (new hover wash toward white — pressed stays deeper), Checkbox check/press/hover, MenuItem highlight flips, and Tooltip card fade; `Transition::enter` (180ms Out) + shared rise for Modal (scrim + card), Toast card, and Menu plate through a shared `enter_shown` appear visibility (guarded reset + next-tick flip; exits snapped until decision 520 superseded the appear-first contract with opt-in deferral). Hit follows the hidden visual (unflipped overlays miss taps, correctly — headless rigs tick like runners). Hardened findings: (a) MockClock+frozen idle pumps spin on any live track — hover washes start tweens on plain taps, so the static tour, drag tests, and every live-track test use press + bounded frames or settle-with-flowing-time exclusively; (b) unmounted subtrees' instances/timers linger (only root unmount evicts — recorded in Blocked with the ownership-wiring question); tests assert behavior, never stale timer counts. Micro + overlay rooms live in `apps/motion` (7 proofs: button hold, checkbox toggle, menu hover+tap, tooltip fade, modal/toast/menu entrances with dismiss paths, boxes pinned, quiescence everywhere). SPEC Transitions extended (micro/overlay/exit), DECISIONS 514, Blocked gains exit-deferral + instance-retirement rows. Scoreboard: app 1452 lines (+535: micro + overlay rooms with 7 headless proofs); framework added tests 12 (motion module 3, style constructors 1, testkit pressed-tint update, controls menu/toast fallout updates, web pump + sink proofs standing).
Gates (CONTRIBUTING order, 2026-10-13): `cargo fmt --all -- --check` clean (workspace + app); `cargo clippy --all-targets` zero warnings (workspace + app); wasm check clean (workspace five crates + motion lib); `cargo test` motion 22/22; controls 184/184; oppa lib 282 (+1 known sandbox failure that passes privileged); testkit 24/24; web 35/35; sera-login 16/16; privileged full `cargo test --workspace` completely green (proving the sandboxed reds environmental — cargo-oppa tempdirs, nativefs tempdir, png writes, app_dirs, puppeteer EPERM, nested fixture build).
- Motion lab round 3 — delay/stagger + springs + completion, decisions 512–513 (2026-10-13, gates below): `Transition.delayed` / `Keyframes.delayed` shift whole tracks (entry through the delay, full legs after, settle at delay + duration; CSS `transition-delay` on DOM); `Ease::Spring` (canonical underdamped `1 − e^(−12t)·cos(4πt)`, ≈5% overshoot, sub-visual rest residual, exact ends — paint clamps into gamut, transforms ride it, spring tweens step inline with no CSS declaration); `ComponentHost.observe_settled` one-shot TIME callbacks (settle/cancel/stamp/unmount; loops pend; chains re-observe per link — proven driving a two-link sequence back to zero demand). Found and fixed en route: TIME-phase take/restore silently dropped re-entrantly registered animations (chained drain lost mid-tick, tracks stranded demandless) — arrivals append now. Hardened finding: unkeyed rooms diff into each other by position across tabs (instance keys never reach VNode matching), recycling card nodes into loop leaves with cross-room delta tracks that spin idle pumps forever under a frozen clock — rooms mount tab-keyed now (replace, mount-jump, zero tracks; locked by test), and live-track tests use press + bounded frames exclusively (tap/run_until_idle with live tracks hangs — doctrine, thrice proven this round). Entrance room live (fade + rise stagger on boot/tab-switch/Replay, boxes pinned). SPEC Transitions extended (delay/completion/spring), DECISIONS 512–513, GLOSSARY delay/completion/spring entries. Scoreboard: app 917 lines (+265: entrance room + 2 headless tests); framework added tests 11 (evaluator 6: spring curve/track/clamp + delay tween/keyframes + stepping split; css 2: delay spelling + spring silence; component observe 3 incl. the chained sequence).
Gates (CONTRIBUTING order, 2026-10-13): `cargo fmt --all -- --check` clean (workspace + app); `cargo clippy --all-targets` zero warnings (workspace + app); wasm check clean (workspace five crates + motion lib); `cargo test` motion 15/15; full `cargo test --workspace --no-fail-fast` green except the same 7 environmental targets as rounds 1–2 (identical pass/fail splits: cargo-oppa 32+4, oppa lib 281+1, e2e, m4 21+1, m7 49+2, real_dylib, android 52+2 — sandbox os-error-5, puppeteer EPERM, nested-build artifact). Per-suite: controls 181/181, dom lib 29/29.
- Motion lab round 2 — transform animatables T1a, decision 511 (2026-10-13, gates below): paint-only `Transform` (translate px, center-based scale, center-based rotate) extends the decision-120 lock to the CSS-expressible set. Evaluator: `AnimProp::Transform`, per-track xf legs (tweens + keyframes with carry-forward stops), `resolve_transform` + keyframe triple, stamp/restart preserved; mid-flight deltas restart from the previous target (unknown (b) answered empirically). Shared builder bakes each node's accumulated affine into its own ops (new `builder/xform.rs`: per-variant geometry, command-aware path-data rescale, identity no-op; walker threads accumulation with portal reset + transformed damage AABBs) — CPU/GPU agree with zero raster changes. DOM: `transform:` declarations + `transition: transform` + stepped inline (tweens keep CSS). Hit follows the visual through the shared router (evaluator-live sampling; `accumulated_transform`/`local_point` on the host; field caret/drag, handle grabs, bar cells, menu rows mapped). Rotate stores but refuses loudly at every application point (T1b: shaders + raster plumbing); runner-driven chrome anchors (toolbar/menu/scrollbar/scroll-drag) stay layout-space, stated. Testkit gains `advance_frames` (bounded M8 stepping — `advance`/`tap` spin forever under live tracks; thumb rule learned when a mid-flight test hung the suite). Transforms room live in `apps/motion` (loop tracks on rect/text/vector + scale one-shot, boxes pinned still). SPEC Transitions rewritten, DECISIONS 511, GLOSSARY Transform entry; ARCHITECTURE rendering row gains the bake clause. Scoreboard: app 652 lines (+254: live room + 2 headless tests); framework added tests 32 (evaluator 5 incl. the mid-flight-restart probe answering unknown (b), style 7, builder/bake 11, m5 hit 4, m4 pixel 2, gpu oracle 1, css 2).
Gates (CONTRIBUTING order, 2026-10-13): `cargo fmt --all -- --check` clean (workspace + app); `cargo clippy --all-targets` zero warnings (workspace + app); wasm check clean (workspace five crates + motion lib); `cargo test` motion 13/13; full `cargo test --workspace --no-fail-fast` green except the same 7 environmental targets as round 1 (verified same tests: `nativefs_tempdir` + cargo-oppa ×4 + studio e2e:353 + m4 png + android app_dirs ×2 = sandbox os-error-5; m7 browser ×2 = puppeteer EPERM; `real_dylib` = nested-build rig artifact). Per-suite green records: oppa lib 272 (only the env failure), controls 181/181, dom lib 27/27, cpu lib 25/25, m5 20/20, m4 21 passed + the env png row, gpu 12/12.
- Motion lab round 1 — audit + skeleton (2026-10-13, no framework change, no decision number): code-truth audit of the transition system (`transition.rs` evaluator, `style.rs` Transition/Keyframes/Ease, DOM css/hook mapping, scrollbar + kitchen-sink usages) confirms the brief — animatables are `bg`+`opacity` only, `Transition` has no delay field, easings are cubics only, no completion signal, no transforms, no enter/exit. Missing-primitive order: transforms (T1) → delay/springs/completion (T2) → control tweens + overlay entrances (T3). New standalone `apps/motion/` (own workspace like `apps/sera-login`: `src/app.rs` tab shell + duration/easing params + one honest static stub per room naming its missing primitive, `main.rs` via `run_desktop_with` with the FPS counter armed, `lib.rs` via `export_app!` + `android_main!`, `tests/motion_page.rs` 7/7 + unit 2/2, README run table). SPEC/ARCHITECTURE unchanged this round (no contract touched). Gates (app-local): `cargo fmt --check` clean, `cargo clippy --all-targets` zero warnings, `cargo test` 9/9 green; workspace suite + wasm check recorded below.
Scoreboard: app 398 lines (`src/app.rs` + `lib.rs` + `main.rs` + `tests/motion_page.rs`); framework added tests 0 (no `crates/` change — honest zero).
Gates (workspace, this round): root `cargo fmt --all -- --check` clean; app `cargo check --target wasm32-unknown-unknown --lib` clean (export shell compiles to wasm); full `cargo test --workspace --no-fail-fast`: everything green except 7 targets, all classified environmental (zero `crates/` edits this round — none attributable to the change): `oppa --lib` `nativefs_tempdir`, `cargo-oppa --lib` ×4 (scaffold/sync layouts), `oppa-app task_studio_e2e` (`create_dir_all(temp_dir)`), `oppa-cpu m4_cpu` png spot-check, `oppa-shell-android --lib` app_dirs ×2 — all sandbox `os error 5` tempdir/sandbox-root writes (the documented os-error-5 class); `oppa-dom m7_dom` browser ×2 — puppeteer `EPERM mkdtemp` (the documented EPERM class); `oppa-reload real_dylib` — nested fixture build fails in-test while the identical manual build is green (environmental/rig artifact, mechanism unstated).
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

Motion lab T1b landed by decision 517 (see Now) — no accepted rotate work remains.

Motion lab remaining (accepted, queued): device legs — looping tracks keep every runner presenting (ambient room proves loop demand + unmount drain headless; Android event-paced pump needs a device leg — no device in this environment), settle returns every runner to zero scheduled work, per-frame flight cost recorded headless by decision 517 (dev-profile numbers above; on-device + release profiles need hardware); then the desktop run leg (pending retest after the decision-515 pump fix), all new framework APIs headless-tested.

Accepted-carry: file-size follow-ups all closed (`oppa-cpu/builder.rs`
by decision 402, `vnode.rs` by decision 404, `oppa-app` tests by decision 405,
`oppa-controls/tests.rs` by decisions 406–408, `m4_cpu` tests by decision 410,
`oppa-cpu/backend.rs` by decision 411, `oppa-shell-win/tsf.rs` by decision 412,
`oppa-shell-linux/input.rs` by decision 413, `m9_reload_gate` tests by decision 414,
`m3_layout` tests by decision 415, `m7_dom` tests by decision 416).

## Blocked (with blocker)

| Item | Blocked by |
|---|---|
| Layout animation (height/width interpolation) — RESOLVED by decisions 521-522 (opt-in FLIP + fenced interpolation; this row stays as the record, superseded in place) | Was: per-frame-layout architectural question — answered by capture-side recording (no engine re-runs) + evaluator-side interpolation + `effective_box` side table (pass-mask/damage discipline and quiescence untouched) |
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
