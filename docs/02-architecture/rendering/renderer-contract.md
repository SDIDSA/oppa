# Renderer contract

Status: current as interface design (locked #5, plus the M6
`DrawOp::Text.baseline` lock touch — decision 105, plus the M7
`em_size` + per-run `fonts` lock touch — decision 110); CPU
backend implemented (M4 — `oppa-cpu` proves the contract; see
`../../04-planning/state.md` §5j); GPU backend implemented (M6 —
`oppa-vello` proves it second; see `../../04-planning/state.md`
§5l); DOM backend implemented (M7 — `oppa-dom` proves it third;
see `../../04-planning/state.md` §5m).
Sources: `12-archive/DESIGN.md` §§2.1, 2.3;
`12-archive/BUILD-ORDER.md` (M4 proves implementability).

```rust
enum PresenterKind { GpuDrawList, Dom }

trait RendererBackend {
    fn kind(&self) -> PresenterKind;
    fn create_surface(&mut self, desc: SurfaceDesc) -> SurfaceId;
    fn destroy_surface(&mut self, id: SurfaceId);
    fn commit(&mut self, surface: SurfaceId, diff: &TreeDiff);
    fn paint(&mut self, surface: SurfaceId, plan: &FramePlan);
    fn caps(&self) -> Caps;
}
```

- `TreeDiff` = added/removed/moved ids + per-node payload deltas
  (style id, text, handler ids). Backends key their own representation
  by `NodeId`.
- `FramePlan` = viewport + build/destroy-layer + ordered `DrawOp`s +
  damage + layer plans, built from dirty subtrees only.
- `Caps` = max layers, blur/backdrop support, MSAA, text-as-paths.
- `DrawOp` = `Rect | RImg | RRect | Text(ShapedRun) | Path |
  PushClip | PushLayer{opacity, blend} | Pop`.
- **Second text path (locked #27):** editable fields on Web are a
  presenter-recognized special case — the DOM backend owns editing
  authority; the contract guarantees behavior via the shared
  editing-operation suite, not mechanism. Non-editable text is
  unchanged (framework-measured/shaped). Permanent rule:
  framework-measured tracked text is never delegated to CSS
  `letter-spacing` (one-unit trailing-edge divergence, measured
  92.03125 vs 91.03125 px). Freeze is gated (see
  [editing spec](../../03-spec/text/editing.md)).

M4 exists to prove this contract implementable by a backend sharing no
core code — if a presenter secretly needs core internals, we learn at
week 8, not 16.
