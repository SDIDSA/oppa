# Documentation index (current)

Status: current. Six living files describe `HEAD`. History is in git, not here.

| File | Tracks |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | How the code works now (layers, code paths, seams, backends/shells) |
| [SPEC.md](SPEC.md) | Behavioral contracts (observable guarantees, oracle gates) |
| [STATE.md](STATE.md) | Done / Now / Next / Blocked / Known issues (the only planning file) |
| [DECISIONS.md](DECISIONS.md) | Active decisions, 1 line each (rationale in linked ADRs) |
| [GLOSSARY.md](GLOSSARY.md) | Canonical terms (wins on naming disputes) |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Gates, entry points, working rules |

Kept alongside (current-state, not history):

- [`06-platforms/`](06-platforms/) — per-platform status, limits, recipes.
- [`10-decisions/`](10-decisions/) — ADRs 0001–0014 (decision rationale).
- [`12-archive/`](12-archive/) — frozen pre-consolidation records, read-only.

Rules: rounds overwrite the six files in place and delete superseded text in
the same edit. No new doc files without deleting/merging an old one. Start at
[`../PROJECT.md`](../PROJECT.md).
