# Performance budgets and release gates (G16)

Status: current (decision 235). Sources: `HANDOFF-V2.md` §2
(measured table), `04-planning/rounds.md` (phone, fps-demo,
cross-platform entries), `08-performance/mobile.md`,
`08-performance/goals.md`.

Every number below was measured on the named scene and box —
budgets are floors/ceilings from those scenes, not universal
laws. A scene heavier than the cited one re-baselines (record
scene + box + the table row, never silently).

## Measured baselines (the table budgets derive from)

| Scene / box | Result |
|---|---|
| FPS scene, Windows Vulkan/Immediate, release | 1008 fps |
| FPS scene, Linux llvmpipe Vulkan/Immediate | 145 fps |
| FPS scene, Adreno 650 Vulkan/Mailbox, release | 215 fps (62 debug, 123 pre-blitter-cache) |
| Adreno 650 render-only steady (decision 176) | 16.7 ms |
| Adreno 650 full-scene render+readback @1080×2400 | 86–88 ms |
| Adreno blitter cache effect | 3.6–5.2 ms/frame → 0.00 |
| Web Firefox CPU canvas, FPS scene | 145 fps |
| fps-demo cold / warm start (Vulkan) | 2.7 s / 1.1 s (was 7–16 s Dx12 every launch) |
| linux_demo CPU paint @800×600 | ~30 ms |
| Emulator tiny-skia full scene (integration data, not proof) | 207 / 104 ms |

Unmeasured (open, not budgeted): weak-tier silicon
(Mali-G52/Adreno-610 class), sustained-thermal behavior
everywhere, Firefox/Safari web legs beyond the canvas number.

## Budgets (floors — a miss blocks release on that tier)

- Desktop GPU tier (RTX-3060Ti class): app scenes hold vsync
  60 fps; FPS-scene headroom must not regress >10% vs 1008.
- Adreno-650 class: render-only p95 ≤ 16.7 ms sustained on an
  equivalent scene; startup within the demo envelope below.
- Software tier: llvmpipe FPS-scene ≥ 100 fps (headroom under
  the measured 145); 800×600 CPU scenes ≤ 50 ms paint (margin
  over the measured ~30).
- Startup (desktop Vulkan): cold ≤ 3.5 s, warm ≤ 1.5 s on the
  fps-demo scene (margin over 2.7 / 1.1).
- Web canvas leg: FPS-scene ≥ 100 fps where measured (margin
  under 145; browser-compAT matrix stays open per G4).

## Oracle release gates (exactness — any violation blocks)

- CPU incremental == full repaint: exact 0 (M4 oracle).
- CPU-vs-Vello sharp geometry: exact 0; curves tol-16 ≤ 60
  (measured 12 — trips well before the bound).
- Glyph atlas delta 0.0; static-frame GPU work 0 (M6).
- Scroll ticks: 0 structure ops end to end; offset trail
  ≤ 1 frame (M7/M8).
- On-device GL where servable: exact-0 vs on-device CPU
  (phone + emulator rows).

## Thermal / adaptive plan (Proposed, unmeasured)

The monitor input exists (`PresentReport` stage walls + 5 s
means on every driver — v2 instrumentation). The policy does
not: sustained p95 past budget over N windows should step
down (overscan 4→2, blur/backdrop off, Mailbox→Fifo, GPU→CPU
fallback), then step back up after M clean windows. Thresholds
N/M and the per-step savings are unmeasured (need weak-silicon
+ thermal runs) — this section is a plan, not a control loop.

## Versioning, crash reports, logging (guidance)

- Version: the workspace Cargo version (`0.1.0`) — single
  source (G4). No stability promise yet: the renderer contract
  types are the compat surface; 0.x may reshape them with a
  decision + migration note.
- Logging: the loud-failures rule first (panic/message with the
  refusing reason); existing log points are `PresentReport`,
  `stats_summary` (shell intake), IME/msg/anchor logs, and
  `take_errors` drains. No log facade ships — use eprintln-class
  platform logging until one is decided.
- Crash reporting: nothing ships (open). Native crashes today
  mean a loud panic or a harness-dumpable oracle diff — file
  the report with the diff, not just the trace.
