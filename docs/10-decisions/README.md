# Architecture Decision Records

Status: current. Existing architectural decisions, preserved with
origin pointers. If the reason for a decision is unknown, the ADR
says so — no historical reasoning is invented.

| ADR | Decision | Status |
|---|---|---|
| [0001](ADR-0001-pipeline-ownership-dom-web.md) | Pipeline ownership split; DOM web backend | Accepted (R1) |
| [0002](ADR-0002-accessibility-in-contract.md) | A11y/semantic tree in renderer contract day one | Accepted (R1) |
| [0003](ADR-0003-two-tree-retained-model.md) | Retained model, two trees, fine-grained reactivity | Accepted (R1) |
| [0004](ADR-0004-framework-owned-layout.md) | One layout engine; renderers never lay out | Accepted (R1) |
| [0005](ADR-0005-typed-styles-no-cascade.md) | Typed interned styles; token-table themes | Accepted (R1) |
| [0006](ADR-0006-rust-core.md) | Rust for core and components | Accepted (R2) |
| [0007](ADR-0007-handlers-as-ids-generational.md) | Handlers-as-ids; generational storage | Accepted (R2) |
| [0008](ADR-0008-hot-reload.md) | Manifest-scan dylib harness; Android restart-only | Accepted (R3) |
| [0009](ADR-0009-rasterizer-vello.md) | Vello desktop GPU; CPU fallback; Skia hatch | Accepted (R4; attribution corrected R5) |
| [0010](ADR-0010-scheduler-threading.md) | Phase scheduler; single UI thread; per-surface present | Accepted (R5) |
| [0011](ADR-0011-web-scroll-and-transitions.md) | Native web scroll; binding-edge transition stamp | Accepted (R5) |
| [0012](ADR-0012-text-editing-authority.md) | Verdict (b) on Web; second text path; freeze gates | Accepted (M1 #27–#29) |
| [0013](ADR-0013-state-residence.md) | Core-side residence; generation-tagged tasks; memo-write panic | Accepted (R5 addendum + M0 review) |
| [0014](ADR-0014-app-storage.md) | Sync KV + sandboxed-file seams; async backends deferred | Accepted (V3 G5 #216–#217) |

Full locked list (#1–#29) with round origins: `12-archive/DESIGN.md` §7.
Deferred v2 items: [non-goals](../00-vision/non-goals.md).
