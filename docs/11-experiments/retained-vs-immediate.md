# Experiment: retained vs. immediate mode

Status: **Accepted** (settled R1; locked #4).
Source: `12-archive/DESIGN.md` §§2.1, 2.4.

| | Immediate (egui-style) | Fully retained (Flutter 3-tree) | Chosen: retained model, 2 trees |
|---|---|---|---|
| Render perf | Redraws every frame; caching bolted on per-widget | Best: dirty subtrees only | Same win, minus one tree |
| Ergonomics | Collapses for complex apps (implicit state) | Fine but verbose | React-level via fine-grained reactivity |
| Binary size | Smallest | Largest | Between; one-time reconciler cost |
| Hot reload | Trivial | Hard (cross-reload identity) | Medium (key/identity discipline) |
| a11y/text/IME | Reinvented ad hoc (egui's debt) | Natural | Natural |

Two trees, not three, because dependency tracking knows which
instances to re-run — the widget tree collapses into functions.
Resulting decision: [ADR-0003](../10-decisions/ADR-0003-two-tree-retained-model.md).
