# Current sprint

Status: reconciled snapshot 2026-09-29 (post Phase 18 close, Decision 322).
Authoritative code state: [`state.md`](state.md); per-round evidence:
[`rounds.md`](rounds.md); phase plan: [`production-readiness-plan.md`](production-readiness-plan.md).

## Where the tree actually stands

Phase 18 is CLOSED — Phases 8–18 are all complete (decisions 297–322:
tap-to-caret, drag selection, cursor shapes, the event-driven Windows loop,
the mouse-button taxonomy, 2D scroll, touch drag + fling, per-side box model +
per-corner radii, Light/Dark theme tokens, keyed DOM patching, worker prep
stages, `Collection`/`VirtualList`, paged fetch + `DataGrid`, generic Props
derive, caret bar + blink, DesktopLoop selection/caret wiring, save/folder
dialogs, OS theme detection, close veto, menus, scrollbars, tooltips,
masking, error boundaries, cleanup hooks, mobile lifecycle). Scratch-pad
where-it-started notes from the M4–M6 era were merged into the compact record
below — the round-by-round story lives in `rounds.md`; the full snapshots in
`state.md`; the architecture decisions in `10-decisions/README.md`.

## Active phase: 19 — Validation on a real application (CLOSED 2026-09-30)

**Status 2026-09-30 (post 19.8 + Decision 323, reconciled by Round 20.1):**
Rounds 19.0–19.8 plus the eyeball fix-up round (Decision 323: resize cursors
+ theme architecture) are done — Linux typecheck + suites + first app runs +
first full-workspace Linux gate all green; the 15.2 flake watch closed;
live Windows legs proven (DX12 sink run, TSF chain, WM_CLOSE destroy, real
COM save/folder dialogs with `Ok(None)` dismissal); web re-baselined at
14/14 on current Edge + first Firefox boot evidence; Android assessed
equipment-bound with a named pre-condition round (Phase 20 builds the
damage-loop harness mode); Vulkan teardown race root-caused with `hwnd_alive`
guards and the 7.4 matrix re-run green (19.8); resize cursors + theme-owned
ink/background shipped with a live Vulkan sink eyeball (323).
**Zero framework regressions found.** Per the 19.6 fork, **Phase 20 opens as
the hardening ledger** (see [`production-readiness-plan.md`](production-readiness-plan.md)
Phases 20–24); the human eyeball session (IME manual pass, registry theme
flips, menu/tooltip visuals — invited) stays open alongside.

### Superseded status note (2026-09-29 post close-out, kept per rule 3) Rounds 19.0–19.3 and 19.5–19.6 are
done — Linux typecheck + suites + first app runs + first full-workspace
Linux gate all green; the 15.2 flake watch closed (sandbox clipboard
virtualization, root-caused); live Windows legs proven (DX12 sink run, TSF
chain, WM_CLOSE destroy, real COM save/folder dialogs with `Ok(None)`
dismissal); web re-baselined at 14/14 on current Edge + first Firefox boot
evidence; Android assessed equipment-bound with a named pre-condition
round. **Zero framework regressions found.** Next per the 19.6 fork: the
Vulkan hinstance fix-up round (the pass's one concrete finding), the human
eyeball session (IME manual pass, registry theme flips, menu/tooltip
visuals — invited), then Phase 20 opens as the hardening ledger. Full
evidence: `rounds.md` 19.0–19.6; round list + outcomes:
[`production-readiness-plan.md`](production-readiness-plan.md).

### Phase 19 first moves (mechanical, before any new framework work)

Full round list persisted in
[`production-readiness-plan.md`](production-readiness-plan.md) §Phase 19
execution (19.1 Linux test+run → 19.2 Linux workspace gate → 19.3 Windows
live pass → 19.4 Android pass → 19.5 web sink pass → 19.6 adoption
close-out). Status of each lives there; this file keeps only the headline.

1. ~~**Linux build green**~~ — **DONE 2026-09-29 (Round 19.0):** WSL Ubuntu
   `cargo check -p oppa-app --all-targets` compiled `linux.rs` clean (0
   errors / 0 warnings), closing the 15.2/16.1/16.2/16.3 standing notes'
   compile-correctness substance. Remaining open pieces are recorded: Linux
   test execution, and the full-workspace Linux gate (blocked upstream by
   third-party `windows-future 0.3.2`; see the 19.0 entry in `rounds.md`).
2. **Kitchen-sink validation pass** — point real-application content at
   the tree (the `KitchenSinkApp` already half-provides this): exercise
   scroll-translation, IME composition on hardware, Light/Dark flips, the
   Tooltip/portal/menu paths headlessly on Windows first, then
   on-device (Android cdylib + web build) to surface what headless suites
   cannot.
3. **Close out stale debt** — the four files that still reference Phases
   8–18-era open questions (15.2 live-OS flake watch; menu hover/edge/
   drag-select; `WM_MOUSEHWHEEL`; VirtualList/DataGrid scrollbar wiring)
   get closure decisions either as fixes or as explicitly accepted-scope
   items in the next plan.

### Debt inventory (all documented, nothing hidden)

| Item | Status | Where recorded |
|---|---|---|
| Linux typecheck of the 15.2/16.1/16.2/16.3 target-gated edits | **closed 2026-09-29** (Round 19.0; Linux test-run + full-workspace gate remain recorded-opens) | `rounds.md` Round 19.0 |
| Live-OS proof passes (dialogs/registry/portal/`matchMedia`/close-veto/IME) | open — headless by construction | `rounds.md` OQ deltas 16.1–16.3 + the 2.1 IME standing note |
| `oppa-shell-win` live-OS test flakiness | watch — environmental so far | `rounds.md` 15.2 OQ |
| Runtime window icon, menu hover/ clamp / drag-select, VirtualList/DataGrid scrollbar wiring, scrollbar idle fade | feature follow-ups | `rounds.md` 16.3/17.1/17.2 OQs |

No other unstarted-but-accepted items exist; everything else in the old
backlog is done (see [`backlog.md`](backlog.md) and the stale-snapshot note
at the top of that file).

## Archived sprint ramble (M4–M6 era, superseded)

The pre-reconciliation "Just finished / Next" scratch pad from the Sept 26
snapshot is preserved verbatim in
[`docs/12-archive/current-sprint-2026-09-26.md`](../12-archive/current-sprint-2026-09-26.md)
per rule 3 (do not delete authoritative-raw records); it describes only the
M4–M6 era and must not be read as current status.
