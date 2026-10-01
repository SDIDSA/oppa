# HANDOFF — oppa v1 (read-once, standalone)

## 1. What this is

**oppa** is a Rust reactive UI framework: fine-grained signals /
memos / effects drive a retained widget tree; each frame the
reconciler diffs the tree, a `FramePlan` builder turns the diff
into backend-neutral draw ops, and a backend (CPU/tiny-skia,
Vello GPU, DOM) paints them. One UI thread, on-demand frames
(idle costs nothing). Platforms: Windows (Win32 shell +
DWrite), Linux (planned shell, AT-SPI), Android (shell +
restart-only reload, GLES row), Web/DOM (ARIA).

**v1** is the milestone chain M0–M10 plus the closure round
documented here plus the v1-remainder round (Gaps 1–6, same
session date): every milestone proved its contract with
mechanical tests, not demos. Final tally: **335 passed / 0
failed**, clippy clean, fmt clean (was 305 at handoff: +23
remainder (uia_events, shape_android, linux/text/shell/web
rows) +7 close-out (shell-linux input contract)). The one measurement that
could still change a design decision — Vello frame cost on weak
mobile GPUs — needs a physical phone nobody on the team has
(§5). Everything else closable is closed.

## 2. What's proven, per platform (artifacts, not adjectives)

**Core (all platforms).** Generational slots: use-after-retire
panics loudly (`GenArena` proof test). Reload fuzzer gate (M9):
~300 swaps across mid-scroll / mid-transition / mid-IME /
mid-input-burst / in-flight-tasks with zero engine violations —
`crates/oppa-reload/tests/m9_reload_gate.rs`, gate declared in
`docs/03-spec/reload/freeze.md` (the precondition for freezing
renderers).

**Windows.** Real Win32 window + real OS IME wiring
(`crates/oppa-shell-win`); UIA provider over the retained tree
(`crates/oppa-uia`) — toggle/list-item queryable through real
COM interfaces with AT Toggle()/Select() driving framework
presses end-to-end (`tests/uia_emit.rs`); event raising proven
through an HWND-hosted provider + real `CUIAutomation` client
pump — a toggle flip raises an observable property-changed
event with old/new values (`tests/uia_events.rs`, v1-remainder;
production HWND hosting stays with the shell window per
decision 149).

**Linux.** Shell + text slice closed in v1-remainder (no sudo):
`oppa-text-rustybuzz` (shared shaper core) +
`oppa-text-linux` (DejaVu/Ubuntu/Noto chain over
`/usr/share/fonts`) + `oppa-shell-linux` (winit window +
softbuffer CPU present) — window 800×600 opens under WSLg,
DejaVu measures (30 faces, 0 skipped), CPU paints (~30 ms),
exit 0. Close-out: input mapping into the shared pipeline
(left/touch-0, four-key table, sampled modifiers, loud
multi-touch drain — 7 contract tests; IME policy stays the
follow-up). AT-SPI emitter layer (`crates/oppa-atspi`: total
role/state table, incremental tree mirror) validated against
the **live bus**: our exact tree data served to the real
`at-spi2-registryd` 2.60 and read back 25/25 — registry echo,
role names+numbers, state numbers, children, extents,
flip-signal delivery (`spike/atspi_bus_*.{sh,py}`).

**Android.** Shell classifying into the shared `InputEvent`
pipeline (same router Win32 feeds — `tests/android_contract.rs`
repeats M5's Toggle assertions through Android-classified
intake); restart-only reload proven pixel- and dump-identical
to cold start; NDK r29 + both Rust Android targets installed
with cross-`check` green; Gradle `assembleDebug` green and run
on the emulator. **On-device proof**
(`crates/oppa-android-app/device-out/`): CPU pixels
SHA256-equal to host CPU pixels (cross-ISA determinism);
Vello-GL pixels byte-equal to on-device CPU pixels (exact-0
oracle through the Android GLES stack, `-gpu host`); SwiftShader
refusals recorded with exact capability reasons (GLES3.0 has no
compute; SwiftShader Vulkan caps UBOs at 16 KB vs Vello's
64 KB). V1-remainder: the scene presents through the real
swapchain (`presented=surface=1080x2400`, blit deleted; both
ABIs built and packaged); text slice over `/system/fonts`
(rustybuzz, 216 faces) validated byte-exact against the host
  reference with a JNI `SystemFonts` bridge (206 platform fonts);
  `adb input` taps flip the toggle on-screen both ways and M1
  composition shapes + IMM policy validate through the shell logs.
  **Phone round** (`device-out-phone/`): arm64 executes the full
  workload (API-35 `.so` loads on API 31); Adreno Vulkan oracle
  exact-0 with SHAs intact across three ISAs; swapchain present
  + both flips visible on-screen; text 8/9 with phone font drift
  isolated host-side; Adreno GLES wall mapped; feeds + IMM green.

**Web/DOM.** ARIA mapping incl. the text-edit payload (M7);
parity corpus green. V1-remainder: minimal shippable app
(`crates/oppa-web`: wasm `WebApp` + JS bootstrap + rAF tick
binding, no new renderer features) run against the M7 harness
(raw server + headless Edge 154): switch false→true→false
through real pointer events, zero console errors
(`spike/web/webapp.mjs` → `spike/results/webapp.json`); Firefox/Safari untested (stated).

## 3. Architecture decisions (one line each)

- Generational slots everywhere (#11): stale handles panic,
  never alias — the foundation every later proof stands on.
- Thin reload harness, manifest-scan dispatch, no proxy macros
  (#14): the swap path is boring code you can read.
- Drain-before-unload ordering + RELOAD phase after INPUT
  (#15, #19): reload never races in-flight input or tasks.
- Retire, don't unload (#61): images retire with a counted
  bounded leak; true unload needs shared-core linking (v2).
- Cancel-at-reload is the accepted v1 cost; task survival is v2
  (R3 grounding, §9.6).
- Restart-only reload on Android (#16): W^X/SELinux forbid
  executing reloaded code — re-confirmed against current AOSP
  sepolicy, not assumed from R3.
- Backend roster: Vello desktop GPU, tiny-skia CPU fallback,
  DOM, GLES row (#17); Caps-negotiated degradation, never
  silent tofu (refusals are loud + pristine).
- Renderer freeze requires the M9 fuzzer green (§8.4) — declared,
  not aspirational.
- Renderer numbers over renderer claims: CPU-exact oracle,
  tol-16 curves bound (60), byte-equality on-device.
- Test-environment honesty is recorded, not hidden: seeds
  printed, counters tabulated, vacuous-pass guards asserted
  (decision 44's UTF-8 audit, pre-drained phase logs,
  non-vacuous stamp counters).

## 4. Explicitly deferred to v2, by design (not backlog)

True unload via shared-core linking; site-key pruning;
`wasmi`-hosted component runtime; image async decode (worker
mailbox reserved); TIME interpolation beyond `.transition`;
multi-line/paragraph shaping and line-breaking (v2 item 2 CLOSED
2026-09-26: opportunity wrap + golden corpus + all slices + app
leg — justification/hyphenation/vertical text stay out);
production HWND
hosting for UIA events (mechanics proven in `tests/uia_events.rs`
— the window lives with the shell); AT-driven focus/editing. Each has a named owner (decision or
BUILD-ORDER slot); none was discovered late.

## 5. Genuinely open items

**One item was genuinely open and is now narrowed: Vello frame
cost on weak mobile GPUs at real resolutions.** A phone walked
in mid-session (Realme GT Neo 3T / Snapdragon 870 / Adreno
650): full-scene render+readback **86–88 ms at 1080x2400**
(two runs), oracle exact-0 over Adreno Vulkan, no hard wall —
then decomposed (N=20): ~126 ms one-time compile, **16.7 ms
steady render-only**, 12.5 ms readback+map, 35.5 ms present CPU
(2 samples) — verdict in `docs/08-performance/mobile.md`.
What remains open is narrower: weak-tier silicon
(Mali-G52/Adreno-610 class, unmeasured) plus the sustained
incremental frame loop (full-scene was measured; per-frame
damage cost was not). The `SkiaBackend` hatch stays costed
(2–4 wk); re-evaluate only on a hard wall (standing rule).

**Two items are NOT in this section, deliberately — read this
before adding them:**

- *Android accessibility service* was **evaluated and
  declined**, not left undone. Populating a node tree for
  custom-rendered content needs a Java-side
  `AccessibilityNodeProvider` + JNI bridge, and with no
  TalkBack on the image the only possible reader is our own
  logger service — self-serving, proving strictly less than
  the banked Linux (live-bus, real registry) and Windows
  (real COM) proofs. Doing it would add code and subtract
  honesty. If TalkBack/a real AT appears on a device,
  re-open with that reader named.
- *A fourth text-slice implementation* (e.g. rustybuzz) was
  **evaluated and declined as out-of-scope**, not overlooked.
  The named residuals are the Android platform slice (needs
  the JNI bridge + device fonts) and the Linux HarfBuzz slice
  (needs sudo-blocked apt) — a fourth shaper would prove
  shaper-agnosticism but closes neither named item. That work
  is a text milestone, not v1-closure.

## 6. Documentation debt (known, listed so it isn't discovered)

- Raw evidence lives outside the suite: `device-out/` pixels
  (30 MB, untracked) + shapes/taps/imm/meta records (v1-remainder,
  same dir) + `device-out-phone/` (phone round: oracle, meta with
  Adreno timings, shapes, taps, imm, fonts), WSL bus scripts +
  JSON in `spike/`, temp-producer tests deleted after use
  (zz_hostref, zz_atspidump, zz_probe* — precedent, not litter).
- Machine-local toolchain (NDK r29, Gradle 8.14.3, Temurin
  JDK 21, AVDs, debug keystore) is documented by path, not
  reproducible by script; no repo `.cargo/config.toml` for
  the Android linker (decision 144 — recipe in state.md).
  New this round, same class: WSL user-local rustup (1.98.1),
  `oppa-text-android/test-fonts/` pulls (byte copies from the
  emulator), `crates/oppa-web/web/pkg/` bindgen output,
  wasm32-unknown-unknown target + wasm-bindgen-cli 0.2.129.
- Role *numbers* for the bus app come from ABI-header order +
  append-only practice; Orca-level confirmation of ids was
  stated as not done (decision 147).
- Emulator AVDs and the visible-window working agreement live
  outside the repo (rounds notes only).
- `12-archive/` is frozen history by rule — including the
  "GLES 3.1-class" phrasing the emulator corrected to a 3.0
  floor; corrections live in living docs, never edits to the
  archive.

## 7. Where to look

- Start: `docs/04-planning/state.md` (title → snapshots →
  §1 verification table → §5 round notes → §6 decisions →
  §7 boundary) and `docs/04-planning/rounds.md` (per-round
  evidence, newest last).
- Contracts: `docs/03-spec/` (normative; freeze declarations
  live with what they freeze — e.g. `reload/freeze.md`).
- Architecture: `docs/02-architecture/`, decisions in
  `docs/10-decisions/`, platforms in `docs/06-platforms/`,
  accession record: `docs/12-archive/` (read-only).
- Code: `crates/oppa` (core), `crates/oppa-{cpu,vello,dom}`
  (backends), `crates/oppa-{shell-win,shell-android,reload,
  atspi,uia}` (platforms/reload/a11y), `crates/oppa-android-app`
  (excluded cdylib — the on-device proof), `crates/spike-textedit`
  (editing evidence).
- Session trail: `oppa-session-summary.md`,
  `oppa-v1-closure.md` (both on desktop, alongside this handoff's
  intent if not its content).
