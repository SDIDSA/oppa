# Display list

Status: accepted as contract design (locked #4–#5, plus M6
`DrawOp::Text.baseline` — decision 105, plus M7 `em_size` +
per-run `fonts` — decision 110); builder current for the CPU
(M4), Vello (M6), and DOM (M7) backends (`oppa-cpu`
`FramePlanBuilder` from dirty subtrees, shared by all three).
Sources: `12-archive/DESIGN.md` §§2.2–2.3.

```rust
enum DrawOp {
    Rect(FormatRect), RImg(ImageId, Box2), RRect(RRect),
    Text(ShapedRun), Path(PathId, Paint),
    PushClip(Clip), PushLayer { opacity: f32, blend: BlendMode }, Pop,
}
```

- The "immediate" half of the model: what a GPU backend consumes.
- Damage metadata lives on the `FramePlan`, not per-op.
- `Text(ShapedRun)` carries pre-shaped, pre-positioned glyph runs —
  the rasterizer fills a glyph atlas, nothing more. Since M6 the op
  also carries `baseline` (GPU places at `y + baseline`); since M7
  it carries the exact `em_size` (GPU sizes runs with it — the
  `font_size = line_height` approximation is ended) and per-run
  `fonts` (one `FontRun{family, font_id}` per fallback run —
  the single-face bound is ended; the DOM arm emits per-run
  `<span>`s from the same segmentation).
- Clipping/transforms/compositing beyond this enum are **not
  specified** — no clipping.md/transforms.md/compositing.md exists
  because the project has not defined them. Blur/backdrop behavior
  is `Caps`-negotiated degradation (see
  [GPU backend](../../02-architecture/rendering/gpu.md)).
- M6 field resolution (one lock touch): `DrawOp::Text.baseline`
  (CPU ignores it; GPU places at `y + baseline`). `Color` stays
  alpha-less by decision 103 (opacity folds into brush alpha +
  scene layers — proven pixel-equal).
- M7 field resolution (second lock touch, decision 110):
  `DrawOp::Text.em_size` + `fonts` (CPU ignores both — cells
  unchanged; GPU draws one run per `FontRun` at the exact em
  with explicit→default→loud face selection; DOM spans carry
  the measured family/size). Finding F3 closed in full.
- M5 field resolutions (no new ops, no backend change): `Style::border`
  builds as an outer fill + an inset background fill in the existing
  shape ops (inset ring, paint-only); `Style::ink` resolves per text
  node (own override, else nearest ancestor's, else contract `INK`)
   into `DrawOp::Text.ink`. (`Color` alpha-less stood through M6 by
   decision 103 — see above.)
