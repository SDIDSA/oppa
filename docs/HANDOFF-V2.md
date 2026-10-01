# HANDOFF — oppa v2 (read-once, standalone)

## 1. What this is

**v2** is everything proven since `HANDOFF-V1.md`: the fps-demo
startup arc (decisions 193–200), the uncapped-blank root-cause
round (202), the cross-platform FPS counter (203–204), the
phone release + instrumentation rounds, and the WebGPU web leg
with its leak fix, resize, and reentrancy rounds. v1's engine
proofs (M0–M10, fuzzer gate, oracles) stand untouched — v2 adds
a runnable app on four targets, not new engine claims.

Final tally at write time: full serial suite green save the
known m10_gles load-contention flake (fails under host GPU
load, 4/4 alone repeatedly — environmental, documented
pattern); `cargo clippy --all-targets` + wasm-target clippy
clean save the intentional `FpsApp` case (decision 184);
`cargo fmt --check` clean.

## 2. What's proven since v1, per platform (artifacts, not adjectives)

**One app, four targets** (`crates/oppa-fps`, release,
screenshots in session temp): Windows Vulkan/Immediate 1008;
Linux WSL/llvmpipe Vulkan/Immediate 145 (x11rb capture);
physical RMX3370/Snapdragon 870/Adreno 650 Vulkan/Mailbox 215
release (62 debug, 123 pre-blitter-cache); Web Firefox CPU
canvas 145 + open-browser Chromium WebGPU path
(`backend=BrowserWebGpu`). Shared scene, shared DejaVu bytes,
live-size recenter everywhere (202).

**Framework additions, all additive:** `crates/oppa-fonts`
(bundled DejaVu + license); `RustybuzzService::
from_bytes_with_chain` + `face_bytes` with parity/rejection/
round-trip tests; `PresentReport` stage walls + 5s mean lines
on every driver; `GpuCtx` blitter cache (3.6–5.2 ms/frame on
Adreno → 0.00) + frame-target cache + per-submit maintenance
poll (8 GB VRAM sawtooth → flat); `?bench=N` uncapped
throughput probe (`render_submit_only`, no acquire/present/
poll); async `ensure_gpu_for_surface_async` for the web lane.

**Uncapped-blank, closed:** Vulkan Mailbox/Immediate blank +
1:1 Ok/Outdated storm was resize-triggered (stale 800×600
reconfigure), not driver-caused. Loops track live client size
with create-before-publish surface swaps; sustained storms log
loudly. Native resize re-verified post-fix (1127×735 →
1435×721, no storm, recentered).

## 3. Corrections to v1 (living docs correct, archive stays frozen)

- v1 §5 "evaluated and declined" a fourth text slice
  (rustybuzz) — then v1-remainder landed exactly that:
  `oppa-text-rustybuzz` is now the shared shaper core and the
  Linux/Android slices ride it. The decline is spent; what
  remains is productizing editing, not shaping (§4 G1).
- v1 §5's open phone item is narrowed to closed on Adreno 650
  (oracle exact-0, release loop 215, stage table in rounds);
  weak-tier silicon (Mali-G52/Adreno-610 class) stays
  unmeasured, and sustained-thermal behavior is unmeasured
  everywhere.
- v1 §6's wasm-bindgen-cli 0.2.129 is now pinned 0.2.128 to
  match the lockfile (machine-local install).

## 4. Verified remaining gaps (each checked against the tree)

P0 — no real app ships without these:

- **G1. Editable text is spike-only.** `EditingSession` exists
  only under `crates/spike-textedit/` (zero references
  outside it); `ctx.edit_session`, named by
  `09-api/controls/overview.md`, does not exist in code;
  the `TextField` vnode (`oppa/src/vnode.rs:360`) has no
  product session; IME wiring is proven on Windows TSF only
  (Linux IME policy is the stated follow-up, Android IMM was
  validated on-device). No text selection anywhere. Contract
  without implementation: `07-testing/editing-contract.md`,
  `03-spec/text/editing.md`.
- **G2. No control catalog.** The vocabulary is `Div`/`Text`/
  `TextField`/`ScrollArea`/`Image` (`oppa/src/vnode.rs`);
  `09-api/controls/overview.md` documents Toggle/list-cell/
  editable-field as *patterns* and states per-control pages
  ship with the controls. No Button/Checkbox/Slider/Dialog/
  Menu in code.
- **G3. No clipboard.** Zero hits tree-wide (core + all
  shells). Blocks G1's usefulness directly.
- **G4. No packaging path.** The cargo-apk flow was proven
  once manually (SDK 34–37 table, `--lib` for the cdylib,
  `CARGO_APK_RELEASE_*` env signing, self-signed
  machine-local keystore); none of it is a documented blessed
  path, and per-platform overviews (`06-platforms/*/`) carry
  no icons/splash/versioning/signing story. Apple shells do
  not exist (no `oppa-shell-mac`, no `oppa-shell-ios`).
- **G5. No persistence or network.** No fs/http/kv in core;
  no such deps in any workspace `Cargo.toml`. Every app
  hand-rolls settings/cache/sync with no seam (and no answer
  for wasm's async-only storage).

P1 — painful without:

- **G6. No navigation.** No router/back-stack/deep-links
  anywhere in core.
- **G7. No app-level async pattern.** `WorkerQueue` serves
  the reactive core (`oppa/src/reactive/state.rs:229`);
  `oppa-reload` is referenced by no example; fetch→render on
  wasm's single thread has no blessed shape.
- **G8. Images decode nothing.** `RImg` is a loud refusal
  (`oppa-cpu/src/backend.rs:402`, "no decoded pixels in v1 —
  async decode unscoped"); v1 §4 owns this deferral with the
  worker mailbox reserved.
- **G9. Fonts stop at Latin.** No emoji/CJK/fallback code in
  any text crate; system lookup exists per-slice but no
  cross-platform fallback chain.
- **G10. DPR unplumbed.** The engine supports it
  (`LayoutConfig::device_pixel_ratio`, tested 1.0–2.0 in
  `oppa/src/layout.rs`); every shell and driver passes 1.0.
- **G11. Touch half-wired.** Linux (`oppa-shell-linux/src/
  input.rs`) and Android (`oppa-shell-android/src/events.rs`)
  classify single-touch-as-mouse with loud multitouch
  refusal; Win32 shell is mouse-only; the `oppa-fps` winit
  driver ignores `Touch` entirely; gestures (long-press,
  pinch, momentum) exist nowhere.
- **G12. No desktop integration.** No multi-window, menus,
  dialogs, file picker, drag-and-drop, or tray in any shell.

P2 — maturity:

- **G13. a11y half-productized.** Emitters proven (UIA real
  COM, AT-SPI live bus); app-facing `Semantics` builders
  cover switch/list-item/text-field (`oppa/src/
  semantics.rs`); screen-reader end-to-end for app-authored
  controls is unverified.
- **G14. Reload not wired to apps.** M2b/M9 harness + fuzzer
  gate exist; neither example references `oppa-reload`.
- **G15. No app test seam.** `07-testing/` covers the
  framework; developers get no headless pump + assert story.
- **G16. No performance contract or release story.** Numbers
  exist per target but no budgets, no thermal/adaptive plan,
  no versioning/crash-report/logging guidance.

Suggested order (proposed, not decided): G1 → G3 → G2 →
G4-packaging-doc → G5-decision → G6.

## 5. Explicitly not gaps (decided, do not relitigate)

- Android accessibility *service*: evaluated and declined in
  v1 §5 (no on-device reader; self-serving). Re-open only
  with a named reader.
- Restart-only Android reload (locked #16) and cancel-at-
  reload cost (v1 §3): accepted constraints, not missing
  features.
- `SkiaBackend` hatch: costed 2–4 wk, re-evaluate only on a
  hard wall (standing rule; Adreno 650 was not one).

## 6. Documentation debt (new class since v1)

- Session harnesses live outside the repo
  (`exp-fps.ps1`, `exp-resize.ps1`, `marionette*.py`,
  throwaway `xshot/`, Playwright headless shell used
  read-only from its cache); `web/pkg/` (17 MB generated
  bundle) is deliberately untracked — regenerate per the
  rounds note.
- Release keystore is machine-local (`~/.android/
  oppa-local.keystore`, test-only) wired by env vars;
  wasm-bindgen CLI pinned machine-locally (0.2.128).
- Screenshots for the v2 table live in session temp, not in
  the repo (precedent: v1 §6 device-out rule).

## 7. Where to look

- Deltas: `04-planning/rounds.md` (newest last:
  uncapped-blank, cross-platform, phone/instrumentation,
  webgpu/bench/leak/resize/reentrancy) and
  `04-planning/state.md` (decisions 202–204).
- This handoff's parents: `HANDOFF-V1.md` (engine proofs),
  `12-archive/` (frozen).
- Gaps' homes: `03-spec/text/editing.md` (G1 contract),
  `07-testing/editing-contract.md` (G1 tests),
  `09-api/controls/overview.md` (G2 patterns),
  `06-platforms/*/` (G4/G11 per-target notes),
  `10-decisions/` ADRs 0002/0008/0012 (a11y, reload, editing).
- Code: `crates/oppa` (core), `crates/oppa-fps` (the
  four-target proof), `crates/oppa-vello` (GPU backend),
  `crates/spike-textedit` (G1 evidence, experiment only).
