# login — one app, every platform, one command each

The login form (`src/app.rs`) is written **once** — signals,
`TextInput` validation marks, `Button`, no platform imports.
Three thin shells mount it unmodified:

| Platform | Command (from this dir) | Shell |
|---|---|---|
| Windows / Linux | `cargo oppa check`, `cargo oppa build --desktop`, `cargo oppa run --desktop` (or plain `cargo run`) | `src/main.rs` → `desktop_main!` |
| Web | `cargo oppa serve` (or `run --web`) | `src/lib.rs` → `export_app!` + generated `web/` |
| Android | `cargo oppa build --android [--variant debug\|release]` → `android/login-release.apk` (15 MB stripped, verified) or `login-debug.apk` (317 MB unstripped, debuggable); install needs a device (`adb install -r`, then launch) | `src/lib.rs` → `android_main!` + generated `android/` shell |

Desktop, web, and Android share the exact `login_root`; the UI
file has no `cfg` and names no shell. The `android/` shell
(`AndroidManifest.xml` + java) and `web/` shell generate at build
time (missing-only); `web/pkg/`, `android/libs/`, and the APK
regenerate every build.

## Android (device leg)

`cargo oppa build --android` assembles `android/login-release.apk`
through the generic `oppa-android-runner` (lifecycle + engine +
present + touch + text + IME + fullscreen for any root —
`android_main!(app::login_root, "com.oppa.login")` is the whole
entry). With a device attached: `adb install -r
android/login-release.apk`, then launch the `com.oppa.login`
NativeActivity. Proven on RMX3370/API 31: renders the form,
cold window ~190 ms / content ~750 ms (staged fonts + parallel
load + boot placeholder); soft keyboard types into the fields
and submit reaches welcome. Host-verified too: align + signature +
badging green, `android_main` exported.
