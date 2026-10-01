# Archive — frozen raw records

Status: frozen (read-only history). These files are the
pre-consolidation records, preserved **verbatim** — no meaningful
information was deleted in the migration. They are no longer the
navigable documentation; for that, start at
[`../README.md`](../README.md).

| File | What it is | Extracted into |
|---|---|---|
| `DESIGN.md` | v1 closed design reference (1383 lines; §§1–9, locks #1–#29) | `00-vision/` … `03-spec/`, `10-decisions/` |
| `BUILD-ORDER.md` | Milestone plan M0–M10, dependency graph, risk checkpoints | `04-planning/`, `00-vision/roadmap.md` |
| `IME-SESSION.md` | Real-IME final PASS report (TSF window + store) | `11-experiments/ime-verification.md` |

Living logs (updated every round) live in
[`../04-planning/`](../04-planning/backlog.md): `state.md`
(cumulative snapshot) and `rounds.md` (append-only delta history).

`spike/` (rig, corpus, results, `REPORT.md`, `IME-PASS.md`,
`IME-CONFOUNDER.md`, `web/` harness) stays colocated with the
experiment machinery it documents; it is summarized in
[`../11-experiments/`](../11-experiments/README.md).

Rule: on any conflict between this archive and the tree, the
archive is authoritative-raw — file the contradiction, do not
silently resolve it.
