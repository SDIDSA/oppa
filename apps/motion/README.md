# motion — motion lab: one app, every platform, one command each

The motion showcase (`src/app.rs`) is written **once** — signals,
`Row`/`Column`/`Div`/`Text`, `Button`, no platform imports. Three
thin shells mount it unmodified:

| Platform | Command (from this dir) | Shell |
|---|---|---|
| Windows / Linux | `cargo oppa check`, `cargo oppa build --desktop`, `cargo oppa run --desktop` (or plain `cargo run`) | `src/main.rs` → `run_desktop_with` (FPS counter armed = frame-cost readout) |
| Web | `cargo oppa serve` (or `run --web`) | `src/lib.rs` → `export_app!` + generated `web/` |
| Android | `cargo oppa build --android [--variant debug\|release]` → `android/motion-release.apk`; install needs a device (`adb install -r`, then launch) | `src/lib.rs` → `android_main!` + generated `android/` shell |

Desktop, web, and Android share the exact `motion_root`; the UI
file has no `cfg` and names no shell. The `android/` shell and
`web/` shell generate at build time (missing-only).

## Scope honesty

Round 1 is the skeleton: tab shell, duration/easing params, one
static stub per room. Each stub names the framework primitive it
waits for (tracks T1 → T2 → T3 in `src/app.rs`). Nothing here fakes
motion — primitives land in `crates/` first, rooms light up after.
