# Documentation index

Status: current. This tree is the navigable map of the project.

Read in this order:

1. `00-vision/` — what and why (stable).
2. `01-design/` — the conceptual model (implementation-independent).
3. `02-architecture/` — subsystem structure and boundaries.
4. `03-spec/` — behavioral contracts (observable behavior, not trivia).
5. `04-planning/` — what is next and what is unfinished.
6. `05-implementation/` — how contributors work in this codebase.
7. `06-platforms/` — per-platform integration, limitations, status.
8. `07-testing/` — how correctness is established.
9. `08-performance/` — requirements and measurements (no invented numbers).
10. `09-api/` — the public application-developer interface.
11. `10-decisions/` — Architecture Decision Records (traceability).
12. `11-experiments/` — exploratory work (never authoritative).
13. `12-archive/` — frozen pre-consolidation records, preserved verbatim.

The pre-consolidation records (`12-archive/DESIGN.md`, `12-archive/BUILD-ORDER.md`,
`12-archive/IME-SESSION.md`) are preserved verbatim under `12-archive/`; the
living logs (`state.md`, `rounds.md`) live in `04-planning/`;
experiment evidence stays under `spike/`. The documents below
extract, deduplicate, and organize their content. Where a summary
here and an archived record disagree, the archived record is
authoritative-raw — file an issue instead of silently picking a
side.

Status labels used across this tree: **Current** (implemented and
authoritative), **Planned** (accepted, not yet implemented), **Proposed**
(not accepted), **Experimental** (under investigation),
**Deprecated / Superseded** (no longer current).

Start at [`00-vision/vision.md`](00-vision/vision.md). For a one-page
working map, see [`../PROJECT.md`](../PROJECT.md).
