# Android

Status: shell + restart path current (M10); swapchain present,
arm64, text slice, touch + IME closed (v1 remainder, Gaps 1–3).
Sources: `12-archive/DESIGN.md` §§5.2, 6.1; locked #16–#17;
`12-archive/BUILD-ORDER.md` (M10); [`STATE.md`](../../STATE.md).

- **Framework behavior:** pipeline ownership via wgpu/GLES; GPU
  rasterizer on GLES 3.1-class drivers with tiny-skia CPU fallback
  (Caps-negotiated). Decisions 388–389 removed Vello and the
  `custom` qualifier: prior on-device numbers below were
  Vello-measured and stand as history; re-proof needs a phone (see
  [`STATE.md`](../../STATE.md) Blocked). The proof app drives the
  unified loop engine `oppa-app::SceneCore` (decision 397) — no
  private re-implementation of the presentation pipeline remains
  in `oppa-android-app`.
- **Packaging:** blessed paths are cargo-apk (dev/test) + Gradle
  assembly (release), manual aapt2 flow as fallback — see
  [packaging](../packaging.md) (G4, decision 215). Per-app APKs go
  through the CLI: `cargo oppa build --android` in the app dir
  (shell sync + NDK `.so` set + aapt2/javac/d8/zipalign/apksigner
  into `android/<name>-release.apk`, host-verified) over the
  generic `oppa-android-runner` (any root via `android_main!`).
  Proven on `apps/login` (21 MB, align + signature + badging
  green); generic-runner boot proven on RMX3370 (login renders,
  cold window ~190 ms / content ~750 ms; soft keyboard end-to-end
  — focus policy + proxy + commits + submit all on-device).
- **M10 built (`crates/oppa-shell-android`, std + `oppa` only):**
  `AndroidShell` (`PlatformShell`: intake queue, `pump_events`,
  `set_ime` log, dp→px classification into the shared
  `InputEvent` pipeline — same router Win32 feeds, proven by
  `tests/android_contract.rs` against M5's Toggle assertions);
  `AndroidLifecycle` (pause closes the render gate with the graph
  retained, resume wakes once, destroy arms restart); surface
  size/density tracking. Multi-touch is a loud `ShellError`,
  never a merged finger.
- **Reload: restart-only, re-confirmed M10** (locked #16 — no
  `oppa-reload` dependency exists in the shell crate by
  construction; current AOSP sepolicy still carries the W^X
  `neverallow`s, Play policy still bans untrusted code).
  Relaunch builds a fresh host — proven pixel- and
  dump-identical to cold start under the same script.
- **GLES row:** GL backend path conforms at the M6 oracle
  standard on desktop GL (exact 0); weak-hardware frame cost
  stays Android-device-owned (see the weak-tier rows in
  [`STATE.md`](../../STATE.md)).
  Emulator-measured (Medium_Phone_API_36.1, `-gpu
  swiftshader_indirect`, gap closure): API 36, x86_64 ABI,
  SwiftShader GLES **3.0** max (`ANDROID_EMU_gles_max_version_3_0`,
  GLES RenderEngine, `vulkan_renderengine: false`), SELinux
  Enforcing, 1080×2400@420 — the bet's "GLES 3.1-class" phrasing
  is corrected to a GLES 3.0 floor (wgpu's GL requirement, met;
  the 12-archive wording stands as written history). API 30 and below top out at GLES 2.0 with no guest Vulkan adapters (proven on Medium_Phone_API_30, default and host-GPU alike — the ladder always lands the terminal CPU row there), so GPU rows are only provable on API 31+ hardware or API 36+ images. Decision 463 closes API 29 the same way with wider evidence: the image advertises Vulkan 1.1 (`vulkan.ranchu.so` present) yet wgpu enumerates zero adapters — Vulkan and GL, including the force-fallback request — identically under `auto`, `host`, `swangle`, and `swiftshader` host modes (the API-29 image ships no ANGLE, so `swangle` is a no-op there); terminal CPU row stands.
- **On-device proof (`crates/oppa-android-app`, APK built with
  aapt2/zipalign/apksigner, no Gradle):** NativeActivity cdylib
  renders the m10 mobile scene offscreen and writes raw pixels +
  timings; SwiftShader rows refuse with precise capability
  reasons (GLES3.0-no-compute — Vello-measured;
  Vulkan-16KB-uniform-binding vs Vello's 64KB — Vello
  unservable there, not a perf question; the GPU backend has no
  compute shaders so both rows need re-proof). Under `-gpu host`
  (GLES 3.1, NVIDIA): Vello serves through the Android stack —
  on-device GL pixels **byte-equal** to on-device CPU pixels
  (exact 0 at 1080×2400), CPU arm byte-equal to host CPU pixels
  (cross-ISA determinism). Raw evidence:
  `crates/oppa-android-app/device-out/`. **Gradle assembly
  CLOSED in v1-closure** (Gradle 8.14.3 + JDK 21, AGP 8.7.3,
  `--no-daemon`, single manifest of truth, cargo .so staged to
  jniLibs): `assembleDebug` green, installed, full evidence set
  reproduced on the emulator.
- **Android accessibility service: OPEN (justified).** Populating
  a node tree for GPU-rendered content needs a Java-side
  `AccessibilityNodeProvider` + JNI bridge plus a reader
  service; without TalkBack on the image the only possible
  reader is our own logger — self-serving, proving less than
  the Linux/Windows emitter proofs already banked. Recipe is
  concrete (emulator + NDK present); the work is a Java+JNI
  package, not a gap in the framework.
- **Limitations (accepted, locked):** hot reload stays
  restart-only in v1 (~2–10 s). The `wasmi`-hosted component
  runtime is a deliberate v2 decision (costs: ~1–2 MB runtime +
  interpreter overhead on render).
- **App glue: was open, CLOSED this round (was: "still open
  needs app glue"):** JNI/`NativeActivity` intake wiring,
  wgpu-Android surface creation, and the platform `TextService`
  slice are all built and proven on-device (three bullets
  below); the Android accessibility service stays declined
  (self-serving without TalkBack — see the service bullet).
  Toolchain: NDK r29 (`29.0.14206865`, side-by-side) + Rust
  targets `x86_64-linux-android` / `aarch64-linux-android`
  (linker: `x86_64-linux-android35-clang.cmd`, minSdk 35 ≤
  emulator API 36); Gradle APK with both ABIs.
- **Swapchain present CLOSED (Gap 1):** the GPU backend gained the
  surface path (`ensure_gpu_for_surface` — always binds the
  surface's own instance, never reuses cross-instance adapters —
  plus `configure_surface` + `present_surface`: scene →
  intermediate texture → blit → `present`); the app presents
  through `wgpu::Surface` from the `NativeWindow` (software
  lock/post blit deleted). On the emulator (`-gpu host`, GLES
  3.1): `presented=surface=1080x2400 format=Rgba8Unorm`, offscreen
  oracle still exact-0, cross-ISA SHAs unchanged, screencap shows
  the ON toggle. Both ABIs built (`x86_64` + `aarch64`, both in
  the APK); arm64 runs nowhere here (x86_64 emulator — stated).
- **Text slice CLOSED (Gap 2):** `oppa-text-rustybuzz` (shared
  shaper core) + `oppa-text-android` (emulator font chain) over
  `/system/fonts` (216 faces, 0 skipped); JNI bridge queries
  `android.graphics.fonts.SystemFonts` (206 platform fonts, all
  shaped families present with right TTC indices). Reference
  corpus (9 strings) shaped on-device **byte-exact** vs the host
  reference. The comparison caught and fixed a real bug
  (Bold/Regular face selection — weight/style-exact ordering).
- **Touch + IME CLOSED (Gap 3):** live `input_events_iter` →
  `AndroidShell` → shared router → repaint → re-present;
  `adb input tap` flips the toggle on-screen both directions
  (screencaps 0x44/0x55 + `taps.txt` batches at density 2.625).
  M1 composition shapes through `dispatch_ime_event` pre/post taps
  (3/3 exact); IMM policy over JNI (`show=true hide=true`,
  enabled=2) routed through the shell IME log. Fullscreen theme
  was required (the status bar eats y<63 taps — finding). Bounds:
  multi-touch drain is shell-tested only; the full
  `EditingSession` stays Windows-bound (M1 shapes run on-device).
- **Clipboard + selection toolbar (decisions 443, 452–456):** `AndroidClipboard`
  over `ClipboardManager` (plain-text writes, empty clears,
  `coerceToText` reads, sync poll; missing service falls back to
  session-local memory loudly) feeds the Ctrl+C/X/V hardware chords
  (decision-442 matrix, now routed through the shared
  `action_for_key`/`apply_edit_action` funnel — desktop-identical by
  construction) and the renderer-drawn toolbar. Since decision 455
  no system UI starts: the runner publishes the controller's
  `ToolbarModel` to a host signal, the scene floats a handlerless
  overlay `Portal` by the field-box anchor, and a mirror-only proxy
  sync keeps the IME coherent; decision 456 deleted the `ActionMode`
  interceptor, synthetic trigger, and show/hide paths from all three
  `OppaIme.java` copies (Rust dropped the `onMenu*` natives).
  Toolbar history (446–447 system pill, 448 hold-to-select, 449 proxy
  reports, 451 touch-only raise) now reads as the retired
  native-hosting era — behavior preserved, pixels moved into the
  scene. Decision 448 hold-to-select and 449 proxy reports
  (software-keyboard moves) still feed the session; taps apply
  through the same loop clipboard, dismissal pins until the
  selection moves, heals reset. The toolbar anchors to the field
  box, not the tight selection — stated follow-up, not a silent
  gap. Device proof of the scene bar is closed (type →
  triple-tap selects with renderer-drawn knobs + scene pill → Cut
  empties the field with the text in the system clipboard (Gboard
  chip) → Paste restores it with the OS toast; placement fix 457
  verified the same pass). Decision 459 closes the masked half:
  sessions shape bullet display text with display↔composite
  mapping for every pointer op; collapsed holds arm the caret bar
  (Select-all when text, Paste always); ranged masked fields offer
  Select-all/Delete/Paste with copy/cut refused (`Delete` drops
  the selection without touching any clipboard); handles are 3 px
  bars, 11 px knobs, 20 px slop; every hardware-key press logs one
  `oppa-key` line with `OppaIme` fall-through logs in all three
  Java copies (physical keyboards need real hardware — see
  [`STATE.md`](../../STATE.md) Blocked for the logcat filter and
  interpretation guide). Proven on Medium_Phone_API_36.1
  (`target/triple459.png`); the login password placeholder is now
  `Enter password` (literal bullets were indistinguishable from
  masked content). Decision 460 fixes hardware/adb typing: the
  proxy view never holds view-focus so its `onKeyDown` unicode
  path never fires — email only worked because Gboard translates
  plain-field hardware keys into `commitText` while refusing the
  password variation. Unmapped printable keys now commit through
  the soft funnel (letters shift-aware, unshifted digits, space;
  never double-delivers — a native-queue sighting means no commit
  exists yet), and shell digits 7–16 map onto framework 0x30–0x39
  (closing a `KEYCODE_1`-as-Backspace collision). Proven on
  Medium_Phone_API_36.1 (`target/pw_digit460.png`,
  `target/mail_fix460.png`). Decision 461 renders live preedit
  inline (composition-epoch subscription around the display
  composite) and bridges `finishComposingText` (new native →
  queue → shell → pump step committing the open buffer with its
  own text, so space keeps the word): proven on
  Medium_Phone_API_29 (`target/r461_live.png`,
  `target/r461_space.png`, `target/r461_x.png`).
  Decision 444 scopes Gboard
  incognito to password fields (the blanket
  `NO_PERSONALIZED_LEARNING` from decision 440 forced incognito —
  and no clipboard history — on every field; the proxy now drops
  the flag and declares the password variation only when the
  focused session is masked).
- **Benchmark loop + FPS badge (decision 393):** `oppa-android-app`
  arms `DebugOverlayConfig::all()` before mount (env unsettable
  under NativeActivity) and publishes `Gpu/GL` / `Gpu/Vulkan` at
  swapchain bring-up; the bounded present phase runs uncapped
  (non-blocking poll, settle → repaint → present every frame,
  negotiated Immediate→Mailbox→Fifo, badge clock per present) so
  the overlay reads live present FPS — the unbounded interactive
  phase stays event-paced Fifo.
- **Phone round CLOSED (Snapdragon 870 / Adreno 650 / API 31):**
  `device-out-phone/`. arm64 executes (API-35 `.so` on API 31);
  Vulkan oracle exact-0, SHAs intact across three ISAs;
  `surface=1080x2400 format=Rgba8Unorm` via Adreno Vulkan with
  both flips visible on-screen; text 8/9 (emoji drift isolated
  to phone font bytes); Adreno GLES refuses Vello
  (`max_storage_buffers_per_shader_stage` 8 > 4 — Vello-specific, GPU re-proof pending); feeds 3/3x2,
  IMM show/hide true, 228 faces / 407 JNI fonts. Window needed
  a Java UI-thread hop (`OppaUi.hideBars` + explicit
   display-size params) — direct View calls are SIGABRT-fatal
   off-thread, attached-thread lookup sees only the system
   loader (DexClassLoader over `sourceDir` is the path).
   Frame cost follows the weak-tier rows in
   [`STATE.md`](../../STATE.md).
- **File pickers (decisions 422–424, 426–428):** `AndroidShell::file_dialog()` is still `None` and save/folder have no backends — PathBuf save/folder refuse as `PickError::Unsupported`; open rides the `picker_bridge` rendezvous (transparent `OppaPickerActivity` trampoline → bytes → `picked/` cache-copy → real path) swapped into the sink's open slot, so Browse fires the real DocumentsUI flow (registration proven on device; the pick gesture is manually testable); proof loops stay ANR-safe through heartbeat drains; the present loop heals the swapchain across the trip (decision 428: the Stop destroys the window, Resume recreates it — per-generation re-acquire + heal-present, so pick/dismiss returns render instead of black); see the contract in [`SPEC.md`](../../SPEC.md).
- **Runtime screen-mode toggle (decision 431):** apps flip status + navigation from component code (`ctx.set_system_bars_visible(bool)`, readable via `ctx.system_bars_visible()` — default hidden, the boot mode) over a host signal plus a last-wins runner command both Android loops drain once per pump iteration (`imm_mode::drain_system_bars_request` → hide/show over the `OppaUi` UI-thread hop; both halves use named constants with pre-30-only guards (boot theme plain `NoTitleBar`; show end-state the full LAYOUT trio; hide arms the window fullscreen flag pre-30). `showBars` keeps the explicit full-display size (`getCurrentWindowMetrics()` on 30+, real-metrics on 29) + short-edges cutout + decor-fits off, so the scene stays full-bleed — bars overlay, never resize, no refit owed; heals re-assert the tracked mode. Device verdict 2026-10-06: API-30 emulator full cycle clean both bars both ways; API-29 emulator likewise clean over two full cycles (Show draws both promptly, Hide restores the boot frame pixel-identically) — both halves use `MATCH_PARENT` size (explicit pixels freeze the drawn state on pre-30 policy) with the boot still settling full-bleed. Desktop/Web keep the signal update with no platform effect. Host + Android-target check/clippy green; proven on-device on Medium_Phone_API_36.1 via the `apps/login` bars button (Show → bars + label flip, Hide → pixel-identical boot frame, clean logcat).
