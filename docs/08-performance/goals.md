# Performance goals

Status: current (priorities locked #1; no invented numbers).
Sources: `12-archive/DESIGN.md` §§1, 6, 8–9.

- Priority order: rendering performance > ergonomics > binary size >
  hot reload. Single-threaded reactive work does not contradict #1:
  dominant frame costs (GPU rasterization, DOM mutation) are
  off-thread by construction and reactive work is O(dirty).
- Stated latency expectations (proposed targets, not measurements):
  desktop body edits ≈ 0.1–1 s; Android restart path ~2–10 s; Web
  wasm swap ~0.1 s. Frame-time, startup, and oracle numbers HAVE
  since been measured (v2: FPS table, demo startup arc, M4/M6
  oracles) — see [budgets](budgets.md), which floors releases on
  them. Binary-size numbers are still unmeasured — this tree
  invents none.
- Per-area status: [rendering](rendering.md) · [layout](layout.md) ·
  [memory](memory.md) · [startup](startup.md) · [mobile](mobile.md) ·
  [web](web.md) · [benchmarks](benchmarks.md) · [budgets](budgets.md).
