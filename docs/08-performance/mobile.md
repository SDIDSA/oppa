# Mobile performance

Status: half measured at M10 (fallback done, weak-GPU adequacy
device-owned). Sources: `12-archive/DESIGN.md` §§6.1, 8.7,
9.5; `12-archive/BUILD-ORDER.md` (M6/M10);
`04-planning/rounds.md` (M10 entry).

The re-testable bet: Skia's robustness on hostile mobile GPUs was
re-scoped into Vello-on-GLES-3.1 + full-scene tiny-skia fallback,
unvalidated at mobile resolutions.

Watch items (§8.7): Caps-gate blur/backdrop until filters mature;
validate GLES 3.1-class driver coverage early (M6 matrix on weakest
hardware); keep the `SkiaBackend` hatch costed (2–4 wk); re-evaluate
only on a hard wall.

M10 measurement (`crates/oppa-vello/tests/m10_gles.rs`):

- CPU fallback at 1080×2400: incremental == full repaint, 0 px;
  press repaint 1 op; `Caps::cpu_fallback` declared; `RImg`
  refuses loudly with a pristine surface.
- GL backend path conforms: CPU-vs-Vello(GL device) exact 0 /
  tol-16 0 on sharp rects (M6 standard). Stand-in: desktop GL on
  an RTX — path conformance, not weak-hardware performance.
- Emulator-measured floor (gap closure, Medium_Phone_API_36.1):
  SwiftShader GLES **3.0** max — the bet's "GLES 3.1-class"
  phrasing above is corrected to a 3.0 floor (wgpu's GL
  requirement, met on the measured image). Archive wording
  stands as history; this page carries the correction.
- On-device rows (`crates/oppa-android-app/device-out/`,
  `-gpu host`, GLES 3.1 NVIDIA): Vello-GL pixels byte-equal to
  CPU pixels (exact 0, 1080×2400); CPU pixels byte-equal to host
  CPU pixels (cross-ISA determinism). Timings on the emulator
  (integration data, not weak-hardware proof): tiny-skia full
  scene 207/104 ms, GL device 4.65 s one-time, GL paint <0.05
  ms, GL readback 214 ms. SwiftShader capability walls
  (GLES3.0-no-compute, Vulkan-16KB-UBO) close the
  software-emulation path for Vello precisely.
- Still device-owned: Vello frame cost on weak mobile GPUs at
  real resolutions. That measurement, not this page, decides the
  bet's remaining half.
- **Real-hardware row (phone round, Realme GT Neo 3T / Snapdragon
  870 / Adreno 650 / API 31, 1080x2400@408):**
  `crates/oppa-android-app/device-out-phone/`. arm64 runs the
  full workload: Vello-**Vulkan** pixels byte-equal to arm64 CPU
  pixels (exact-0 oracle); arm64 CPU SHAs byte-equal to the banked
  x86_64/host SHAs (cross-ISA determinism now spans three ISAs);
  swapchain present through Adreno Vulkan
  (`surface=1080x2400 format=Rgba8Unorm`, white scene + both
  toggle flips visible in screencaps, `adb input` driven);
  text 8/9 lines exact (emoji glyph id 543 vs 568 is phone font
  bytes, reproduced host-side — shaper bit-deterministic);
  feeds 3/3x2, IMM `show/hide true`, 228 faces / 407 JNI fonts.
  Timings (two runs): tiny-skia full scene 365/293 ms (ON;
  551/349 ms OFF incl. first paint), Vello device 5.4/4.2 s
  one-time (cold shader compile), **Vello full-scene
  render+readback 87.9/85.5 ms at 1080x2400** (stable ±3%).
  Adreno **GLES refuses Vello** (`max_storage_buffers_per_shader_stage`
  8 > 4 — third GPU wall, Adreno-specific; Vulkan serves).
- **Bet verdict (narrowed, not closed):** no hard wall on real
  mid-tier silicon (Adreno 650 Vulkan serves exact-0 at 86 ms
  full-scene); the `SkiaBackend` hatch stays costed (2–4 wk) —
  retiring it needs weak-tier silicon (Mali-G52/Adreno-610
  class, unmeasured) plus a sustained incremental frame loop
  (this round measured full-scene render+readback; per-frame
  damage cost is the M8 discipline, unmeasured on device).
  Re-evaluate only on a hard wall (standing rule).
- **Decomposition (follow-up run, same phone, N=20):**
  `device-out-phone/frameloop.txt`. The 86 ms bundle splits
  into first-render pipeline compile (~126 ms one-time:
  full_first 155.8 vs steady 29.2), steady full-loop 29.2 ms
  (min 25.4 / max 43.1 — user-phone background variance),
  steady render-only 16.7 ms (min 14.5 / max 24.0), implied
  readback+map 12.5 ms, present CPU 35.5 ms avg (2 samples —
  thin, includes a full render + blit + swap). Reading: the
  steady GPU render (16.7) sits at the 16.6 ms frame budget for
  FULL scenes (tile-grid floor at 2.59 Mpx — resolution
  proportional, scene independent); production frames skip the
  12.5 ms readback and the 126 ms compile, and run pipelined
  (no `wait_indefinitely` stall). Per-frame damage cost and
  weak-tier behavior remain the two unmeasured halves —
  the loop harness (`frameloop.rs`, `PresentReport::cpu_ms`)
  exists for exactly that next run.
- **Emulator GL decomposition (final APK re-run, N=20, two
  runs):** `device-out/frameloop.txt`: full_first 234/192,
  full_mean 49.6/38.5 (40–66 / 33–51), bare_first 12–14,
  bare_mean 10.8–11.9 (9–15), readback implied 38.7/26.7;
  present CPU 66.8/52.1 ms avg over 6 presents each (2 initial
  + 4 tap re-presents). The translator's readback path costs
  2–3x Adreno's (27–39 vs 12.5) while its render-only is
  faster (11–12 vs 16.7 — host GPU); present CPU is higher on
  the translator (52–67 vs 35.5, thin samples both). Oracle
  re-proven exact-0 with SHAs intact on the final APK; text
  15/15 against the fresh device shapes; both taps flip
  on-screen with matching screencaps.
