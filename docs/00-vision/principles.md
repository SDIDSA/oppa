# Principles

Status: current. Distilled from `12-archive/DESIGN.md` §§1–2, 9.

- **Own the pipeline where we can, rent it where the platform wins.**
  GPU backends own pixels; the Web backend owns the scene mapping while
  the browser owns pixels, scroll physics, and editable-text mechanics.
- **Accessibility is architecture, not a feature.** Semantic payloads
  live in retained nodes and diff like style (`01-design/accessibility-model.md`).
- **One propagation mechanism.** Signal invalidation → re-run → diff →
  pass dirty flags replaces the six mechanisms of traditional toolkits
  (property system, templating, interaction, a11y, virtualization,
  events).
- **Framework owns layout and the event model; platforms own surfaces.**
  Renderers never compute layout; there is one normalized `InputEvent`
  enum everywhere.
- **Typed, interned styles — no cascade.** No specificity, no cascade
  merging; themes are token tables resolved to styles.
- **Swap code, never types** (hot reload). The retained tree is never
  referenced by user code — only by id.
- **Loud failures over silent corruption.** Unknown font families fail;
  stale generational access panics; memo writes panic in all profiles;
  unsettled propagation is a reported bug, never a silent livelock.
- **Behavior over mechanism across backends.** Where mechanisms must
  differ (Web editing, Web scroll), a shared contract test suite —
  not prose — defines sameness.
