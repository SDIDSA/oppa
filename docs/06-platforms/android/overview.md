# Android

Status: shell + restart path current (M10); swapchain present,
arm64, text slice, touch + IME closed (v1 remainder, Gaps 1–3).
Sources: `12-archive/DESIGN.md` §§5.2, 6.1; locked #16–#17;
`12-archive/BUILD-ORDER.md` (M10); `04-planning/rounds.md`.

- **Framework behavior:** pipeline ownership via wgpu/GLES; Vello
  on GLES 3.1-class drivers with tiny-skia CPU fallback
  (Caps-negotiated).
- **Packaging:** blessed paths are cargo-apk (dev/test) + Gradle
  assembly (release), manual aapt2 flow as fallback — see
  [packaging](../packaging.md) (G4, decision 215).
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
  stays Android-device-owned ([mobile perf](../../08-performance/mobile.md)).
  Emulator-measured (Medium_Phone_API_36.1, `-gpu
  swiftshader_indirect`, gap closure): API 36, x86_64 ABI,
  SwiftShader GLES **3.0** max (`ANDROID_EMU_gles_max_version_3_0`,
  GLES RenderEngine, `vulkan_renderengine: false`), SELinux
  Enforcing, 1080×2400@420 — the bet's "GLES 3.1-class" phrasing
  is corrected to a GLES 3.0 floor (wgpu's GL requirement, met;
  the 12-archive wording stands as written history).
- **On-device proof (`crates/oppa-android-app`, APK built with
  aapt2/zipalign/apksigner, no Gradle):** NativeActivity cdylib
  renders the m10 mobile scene offscreen and writes raw pixels +
  timings; SwiftShader rows refuse with precise capability
  reasons (GLES3.0-no-compute;
  Vulkan-16KB-uniform-binding vs Vello's 64KB — Vello
  unservable there, not a perf question). Under `-gpu host`
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
  a node tree for custom-rendered content needs a Java-side
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
- **Swapchain present CLOSED (Gap 1):** `VelloBackend` gained the
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
- **Phone round CLOSED (Snapdragon 870 / Adreno 650 / API 31):**
  `device-out-phone/`. arm64 executes (API-35 `.so` on API 31);
  Vulkan oracle exact-0, SHAs intact across three ISAs;
  `surface=1080x2400 format=Rgba8Unorm` via Adreno Vulkan with
  both flips visible on-screen; text 8/9 (emoji drift isolated
  to phone font bytes); Adreno GLES refuses Vello
  (`max_storage_buffers_per_shader_stage` 8 > 4); feeds 3/3x2,
  IMM show/hide true, 228 faces / 407 JNI fonts. Window needed
  a Java UI-thread hop (`OppaUi.hideBars` + explicit
  display-size params) — direct View calls are SIGABRT-fatal
  off-thread, attached-thread lookup sees only the system
  loader (DexClassLoader over `sourceDir` is the path).
  Frame cost in `08-performance/mobile.md`.
