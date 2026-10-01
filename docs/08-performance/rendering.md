# Rendering performance

Status: design direction current; measurements planned.
Sources: `12-archive/DESIGN.md` §§2.1, 6, 9.1, 9.5.

- O(dirty), not O(tree): `FramePlan`s built from dirty subtrees;
  static UI ≈ 0 CPU; unchanged surfaces skip commit.
- Fine-grained invalidation beats VDOM sweeps without sacrificing
  ergonomics (adopted Solid/Svelte/Dioxus lesson).
- Damage limits ops submitted on CPU (v1); per-frame full-scene
  render on Vello is acceptable for v1 targets, noted as a v2
  refinement.
- Compositor costs off-thread: wgpu/Vello queues on GPU; the
  browser on Web (compositor-driven scroll is why Web scroll is
  native — perf #1 deciding by routing).
- Risk: Vello frame-latency characteristics of a compute pipeline
  differ from Skia's; AA/tessellation still receiving correctness
  work — re-tested at M6/M10.
