# Reconciliation

Status: accepted (M2 implements). Sources: `12-archive/DESIGN.md` §§2.1–2.2;
`04-planning/state.md` §§5g.3–5g.5.

Old VNode + new VNode → `TreeDiff`; pass-mask mapping:

| Change | Masks |
|---|---|
| Structure add/remove/move | `STRUCTURE \| LAYOUT \| PAINT` (+ `LAYOUT` on parent) |
| Style id | `STYLE \| PAINT` + `LAYOUT` for the layout-affecting subset (`w/h/x/absolute_y/fill_width/pad_x/gap/content_size`) |
| Text content | `TEXT \| PAINT` |
| Semantics payload | `SEMANTICS` |
| Handler kind-set | `PAINT` (commit carrier) |

`suppress_transitions` arrives as whole-commit data from the
scheduler's binding-edge flag (accepted v1 limit, locked #22); the M8
evaluator honors it. Handler-only changes never touch layout.
