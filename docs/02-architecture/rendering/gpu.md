# GPU backend (Vello)

Status: current (M6 — `oppa-vello` proves the contract; see
`../../04-planning/state.md` §5l). Spike folded in only the glyph-API
confirm item; M6 built the real backend + oracle + matrix.
Sources: `12-archive/DESIGN.md` §6; `12-archive/BUILD-ORDER.md` (M1/M6).

Decision (locked #17): Vello on desktop GPU in v1; Android ships Vello
via wgpu on GLES 3.1-class drivers with the tiny-skia CPU fallback.

- Coverage of `DrawOp`: rects/rounded rects/paths/images/positioned
  glyphs — yes (glyph API consumes pre-shaped positioned runs);
  clips, opacity/blend layers — yes; blur/backdrop — partial/immature
  → `Caps`-negotiated graceful degradation.
- Honest maturity note: compute-shader rasterizer (GL 4.3+/GLES
  3.1-class or D3D12/Vulkan/Metal; no WebGL fallback);
  production-adjacent, not Skia-grade; driver coverage on weak/mobile
  GPUs is the known weak spot; damage stays ours at op-submission
  level (v2 refinement).
- M6 deliberate stress: DONE — RTX 3060 Ti (Vulkan) + Microsoft
  Basic Render Driver (Dx12) rows + glyph review vs. the M4 baseline
  (atlas delta 0.0; strict geometry 0/0; curves tol-16 12 of bound
  60). Tripwire verdict: PASS on evidence; `SkiaBackend` stays
  costed (2–4 wk), unbuilt. GLES 3.1-class weakest-hardware row
  stays open (no GLES adapter on the M6 box — M10 owns it).
- M7 text polish (decision 110): the atlas holds default + per-id
  faces (`set_font_for`; explicit→default→loud selection) and the
  encoder draws one run per `FontRun` at the exact `em_size`
  (ends the M6 `font_size = line_height` approximation and the
  single-face bound — finding F3 closed). Proven with real faces
  (Segoe + CJK fallback: placement + face selection exact).

See also: [ADR-0009](../../10-decisions/ADR-0009-rasterizer-vello.md),
[performance: mobile](../../08-performance/mobile.md).
