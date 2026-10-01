# Rendering subsystem — overview

Status: current as contract design; backends planned.
Sources: `12-archive/DESIGN.md` §§2.3, 6, 9.5.

- **Owns:** consuming `TreeDiff` + `FramePlan` + `Caps`, presenting
  surfaces, glyph-atlas/layer/DOM-node mechanism state.
- **Does not own:** layout, app/UI-model state, event semantics,
  text shaping (consumes pre-shaped runs).
- **Contract:** [`renderer-contract`](renderer-contract.md) —
  `TreeDiff`, `FramePlan` (`viewport`, `draw_ops`, `damage`,
  `target_layers`), `Caps`, `DrawOp`, `ExternalTexture`.
- **Backends:** [GPU (Vello)](gpu.md) · [software fallback](software.md)
  · DOM backend (see [web platform](../../06-platforms/web/overview.md)).
- **Dependencies:** retained tree + layout boxes + `TextService`
  measurement; `PlatformShell` for surfaces.
- **Code (planned):** M4 CPU backend, M6 Vello backend, M7 DOM backend.
  The `spike_ime_shell` Vello debug renderer is throwaway (see
  [experiments](../../11-experiments/renderer-debug.md)).
- **Specs:** [display list](../../03-spec/rendering/display-list.md),
  [coordinate system](../../03-spec/rendering/coordinate-system.md).
- **Tests:** [visual regression](../../07-testing/visual-regression.md).
