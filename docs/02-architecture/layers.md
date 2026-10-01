# Layers

Status: current. Source: `12-archive/DESIGN.md` §§2.1–2.3.

| Layer | Owns | Does not own |
|---|---|---|
| Component model + state | components, signals, reconciliation trigger | pixels, layout math |
| Reconciler | VNode→retained diff, `TreeDiff`, pass masks, `suppress_transitions` stamp | rendering, measurement |
| Layout engine (planned M3) | measure/position, `LayoutBox` per node | text shaping internals (consumes `TextService`) |
| Event routing (planned M5) | classification, hit-test walk, focus order | OS event production |
| Rendering backends (planned M4/M6/M7) | `FramePlan` consumption, present | app state, layout |
| A11y pipeline | `SemanticsDiff` computation → per-platform emitters | backend internals |
| `PlatformShell` (per-platform) | surface, events, IME, DPI, lifecycle | UI model |
| `TextService` (per-platform) | shaping, measurement, fallback | layout decisions |

Cross-layer invariants: no user code mid-LAYOUT/PAINT; renderers never
compute layout; presenters hold no app/UI-model state; all worker
results enter at INPUT.

Code map: [`05-implementation/project-structure.md`](../05-implementation/project-structure.md).
