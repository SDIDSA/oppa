# Roadmap

Status: current (snapshot 2026-09-25). Source: `12-archive/BUILD-ORDER.md`,
`04-planning/state.md` §1, `04-planning/rounds.md`.

Assumption (from `12-archive/BUILD-ORDER.md`): ~4 engineers; sizes are person-weeks.

| Milestone | Scope | Status |
|---|---|---|
| M0 reactive core + storage + scheduler | five primitives, generational slots, propagation contract, on-demand loop | **Current** — done, 110 tests green |
| M0b TextService slices | trait + DirectWrite backend + IME surface | **Current** — done |
| M1 text-editing spike | verdict (b) on Web; real-IME verification; bidi/combining/ZWJ corpus | **Current** — done (locked #27–#29) |
| M2 reconciler + component model | `Ctx`, VNode, slot-keyed diff, binding stamp, `keyed_state` | **Current** — done (headless) |
| M2b hot-reload harness + fuzzer | manifest scan, dylib swap, opaque props, re-seed asserts | **Planned** — next |
| M3 layout engine | flexbox subset + block-lite, measure↔layout, bidi visual ordering | **Planned** — next |
| M4 first runnable | CPU backend, FramePlan builder, image-diff oracle, SemanticsDiff dump | **Planned** |
| M5 events + focus + Toggle | hit-test walk, `pressed/hovered/focused`, Tab order | **Planned** |
| M6 Vello backend + driver matrix | DrawOp coverage, atlas, Caps, per-surface present | **Planned** |
| M7 DOM backend | TreeDiff→DOM, native scroll, ARIA, verdict-(b) text path | **Planned** |
| M8 virtualization + transitions | `ScrollArea` recycling, evaluator honoring stamp | **Planned** |
| M9 hot-reload product loop + fuzzer gate | reload mid-scroll/transition/composition | **Planned** |
| M10 Android + a11y close-out | Android shell, restart reload, UIA/AT-SPI/ARIA emitters | **Planned** |

First runnable (M4): one styled `Div` + one shaped text line through
core → reconciler → layout → `FramePlan` → CPU backend → PNG, plus a
SemanticsDiff dump. The Toggle is deliberately *not* first (it would drag
events/hit-testing/focus in front of the core-model proof).

Details: [`04-planning/`](../04-planning/backlog.md). Total estimate:
≈19 calendar weeks ≈ 4.5 months (see `12-archive/BUILD-ORDER.md`).
