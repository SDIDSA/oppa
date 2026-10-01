# ADR-0003: Retained model, two trees, fine-grained reactivity

Status: Accepted (R1; locked #4, #9). Sources: `12-archive/DESIGN.md` §§2.1,
2.4; [experiment](../11-experiments/retained-vs-immediate.md).

## Context

Immediate mode optimizes iteration speed at the direct expense of
rendering perf and of a11y/IME/virtualized lists (egui's debt);
fully retained triples (Flutter) cost verbosity and binary size.

## Decision

Retained declarative scene graph + fine-grained reactive model
(Signal/Memo/Effect/BatchGuard/untrack); immediate at the submit
level only. Two trees, not three: components are plain functions,
ephemeral VNodes diff locally against retained nodes — coarse Dart-
style reconciliation is unnecessary with dependency-tracked
invalidation.

## Alternatives

egui-style immediate mode — refused. Custom DSL components (Slint)
— refused (second toolchain, split FFI). Native-widget wrappers —
rejected (trade away pipeline ownership).

## Consequences

Best dirty-subtree repaint without a third tree; React-level
ergonomics without reconciliation sweeps; hot reload is medium
difficulty (explicit key/identity discipline).
