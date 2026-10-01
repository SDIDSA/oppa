# Packaging — blessed paths (G4)

Status: current (decision 215 + Round 25.5 recipes, decision 340). Sources: `04-planning/rounds.md`
(cargo-apk table, Gradle closure, web `pkg/` rule),
`docs/HANDOFF-V2.md` §6 (machine-local debt),
`crates/oppa-android-app/` (manifest + `apk/` + `gradle/`),
`packaging/` (checked-in recipes).

One blessed path per target below. Labels: **proven** (ran on this
box), **manual** (ran once by hand, follow the steps exactly),
**fallback** (low-level, works, not the daily path),
**open** (no blessed story yet — not silently covered).

## Android

- **Dev/test (manual): `cargo-apk` 0.10.** Requires
  `[package.metadata.android.sdk]` min/target (34–37 installed here;
  the default 30 is not) and `--lib` for the cdylib (`--bin`
  panics against it); winit Android needs
  `android-activity/native-activity` + `android_main` in the lib
  (`EventLoopBuilderExtAndroid`). Signing via `CARGO_APK_RELEASE_*`
  env against the self-signed machine-local keystore
  (`~/.android/oppa-local.keystore`, test-only — never ships).
  Install via `adb push` + `pm install` (the streamed installer
  flakes); run the emulator with `-gpu swiftshader_indirect` (host-GL
  breaks screencap AND app present — rcEnc DMA assertion).
- **Release (proven): Gradle assembly.** Gradle 8.14.3 + JDK 21, AGP
  8.7.3, `--no-daemon`; single manifest of truth; cargo `.so`
  staged to `jniLibs` for both ABIs (`x86_64` + `aarch64`);
  `assembleDebug` green, installed, full evidence reproduced on the
  emulator. NDK r29 side-by-side; Rust targets
  `x86_64-linux-android` / `aarch64-linux-android`
  (linker `x86_64-linux-android35-clang.cmd`, minSdk 35 ≤ emulator
  API 36). See `crates/oppa-android-app/gradle/`.
- **Fallback: manual aapt2/zipalign/apksigner** (no Gradle) — the
  M10 on-device proof ran exactly this. See
  `crates/oppa-android-app/apk/` + `AndroidManifest.xml`.
- Icons/splash/versioning: standard Android `res/` + manifest
  mechanism through the Gradle path (no oppa-specific icons ship —
  the pipeline is the platform's, not new framework code). Version
  is the workspace Cargo version (`0.1.0`).
- **Release signing checklist (Round 25.5 — docs, not automation):**
  generate a real keystore (`keytool -genkeypair`, RSA-2048+),
  keep it OUT of the repo (CI secret or hardware-backed store —
  the machine-local `~/.android/oppa-local.keystore` is test-only
  and never ships), sign release builds with `apksigner`
  (or the Gradle `signingConfigs` block reading env passwords),
  then `zipalign -c` to verify. Rotate by publishing the new
  certificate's SHA-256 alongside the release notes; Play uploads
  need the upload key registered first — all standard Android
  tooling, no oppa-specific step.

## Web

- **Blessed (proven):** `cargo build --release --target
  wasm32-unknown-unknown -p <app>`, then `wasm-bindgen --target web`
  (CLI pinned 0.2.128 to match the lockfile, machine-local install),
  serve the app dir (raw `python -m http.server` suffices). `web/pkg/`
  (generated bundle) is deliberately untracked — regenerate
  per above, never commit. Verified on this box 2026-10-01
  (`hello-web`): raw `hello_web.wasm` 3,445,522 bytes (~3.29 MiB);
  `wasm-bindgen 0.2.128` emits `pkg/hello_web.js` (28,550) +
  `pkg/hello_web_bg.wasm` (2,835,099, ~2.70 MiB) — ~2.87 MB total
  served shell (the old 17 MB figure is superseded by this
  weighing; budget follow-ups ride real app growth from here).
- **PWA shell (proven wiring, manual offline pass):**
  `templates/hello-web/web/` ships `manifest.json` (name, shell
  colors, `standalone` display — icons are app branding, none
  ships) + `sw.js` (cache-first for `index.html` + `manifest.json`
  + `bootstrap.js` + `pkg/`, versioned `CACHE` bump per release,
  bundle names following the package stem through `cargo oppa
  new`) with the one-line `serviceWorker.register` in the
  template `bootstrap.js` and the manifest link in `index.html`.
  The dev rig (`crates/oppa-web/web/`) links its own manifest for
  smoke but registers no worker (it stays network-fresh by
  design — stale caches would mask dev builds). Verify by hand:
  build, serve over http(s) (workers refuse `file://`), load once
  online, reload offline — the app boots. No background-update or
  push story (open); version is the Cargo version baked at build time.

## Windows

- **Blessed (proven):** `cargo build --release` — a single exe
  (shell + backends statically linked; no runtime deps beyond the OS).
  Verified on this box 2026-10-01: `cargo build --release -p
  cargo-oppa` → `target/release/cargo-oppa.exe`, 203,776 bytes,
  prints its usage (loud refusal on bad subcommands — the gate
  below ran against this exact binary).
- **Tarball (proven mechanics):** standard `tar` over the release
  exe — verified on this box 2026-10-01 (`bsdtar 3.8.8`):
  `tar -czf cargo-oppa-win64.tar.gz cargo-oppa.exe` → 95,959
  bytes, `tar -tzf` lists the exe cleanly. Same mechanics wrap an
  app exe (Linux `tar` is the same format story — GNU vs bsdtar
  interop is the platform's, not new framework code).
- **Version resource (manual recipe):**
  [`packaging/windows/hello.rc`](../../packaging/windows/hello.rc)
  (`VS_VERSION_INFO` tracking the Cargo version; `rc hello.rc` →
  link the `.res` via `cargo:rustc-link-arg` in a `build.rs`).
  Icons: add an `IDI_ICON1 ICON "app.ico"` line once you have your
  own `.ico` — none ships (app branding, not framework code).
  `rc.exe` is absent on this box, so the compile step stays a
  recipe, not a verified output.
- Installer story (**open**): no MSIX/MSI automation exists
  (`msiexec.exe` is only the install engine — authoring needs
  tooling this box does not have); use standard tooling when
  needed. Version is the Cargo version.

## Linux

- **Blessed (proven):** `cargo build --release` — the shell reaches
  X11/Wayland through runtime `dlopen` (winit + softbuffer), so no
  `-dev` packages at build time.
- **`.deb` wrapping (manual recipe):**
  [`packaging/linux/deb-metadata.toml.example`](../../packaging/linux/deb-metadata.toml.example)
  (copy the `[package.metadata.deb]` block into your app's
  `Cargo.toml`; binary + [`hello.desktop`](../../packaging/linux/hello.desktop)
  payload) + `cargo deb`. No `cargo-deb`/`dpkg` on this box, so the
  wrap step stays a recipe, not a verified output.
- Tarball wrapping: standard `tar` (mechanics proven — see the
  Windows tarball verification above; same format story).
- Dev-env note (this box): WSL stops distros between calls (keep
  background children alive with sleeps/PS jobs); Wayland+WSLg kills
  winit clients ~1s after map (environmental, Weston RDP rail) — the
  X11 path runs error-free.

## Apple

No path: `oppa-shell-mac` / `oppa-shell-ios` do not exist
(HANDOFF-V2 §4, verified). A macOS/iOS shell is a new-platform
round, not a packaging edit.

## Machine-local debt (not shippable, not committable)

Release keystore (`~/.android/oppa-local.keystore`, test-only),
wasm-bindgen CLI pin (0.2.128), `web/pkg/` output, session
harnesses outside the repo — see HANDOFF-V2 §6.
