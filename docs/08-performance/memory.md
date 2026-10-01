# Memory and binary size

Status: direction current (locked #3, #11); measurements planned.
Sources: `12-archive/DESIGN.md` §§1, 3, 6.

- Priority #3: no language runtime (Rust core, wasm first-class).
- Vello side: pure-Rust end to end (no C++ in toolchain/CI);
  Skia would cost ~2–6 MB desktop + CanvasKit-class wasm weight
  for a platform whose v1 backend is DOM.
- Retained-tree discipline: two trees, not three (one tree of
  overhead saved); shared reconciler is a one-time cost; style
  interning + `Arc<str>` amortize payloads across thousands of
  nodes.
- Generational arenas with free-list reuse; explicit retirement
  (no GC pauses — JIT warmup + GC pauses were the Kotlin/JVM
  rejection); `keyed_state` LRU-bounded (default 64).
- No binary-size, heap, or atlas numbers measured — none reported.
