# Backlog

Status: reconciled 2026-09-29 from [`production-readiness-plan.md`](production-readiness-plan.md)
Phases 8–18 (decisions 297–322), [`state.md`](state.md) (snapshot: Round 18.3 /
Decision 322), [`docs/HANDOFF-V1.md`](../HANDOFF-V1.md), and a tree check of
`crates/`. This file was last rewritten in the M4–M6 era (2026-09-26) and did
not know M7–M10, the v1 close-out, or Phases 8–18 happened — anyone reading the
old snapshot would have re-planned finished work. The authoritative code-state
record is `state.md`; this file only carries accepted-but-unimplemented items.

## Accepted and unimplemented

| Item | Where accepted | Notes / pointers |
|---|---|---|
| ~~Linux typecheck closure for the four target-gated standing notes (15.2, 16.1, 16.2, 16.3)~~ | `rounds.md` 19.0 | **Closed 2026-09-29:** WSL Ubuntu `cargo check -p oppa-app --all-targets` compiled `linux.rs` clean (0 errors / 0 warnings). |
| ~~Linux test execution~~ | `rounds.md` 19.1 | **Closed 2026-09-29:** `cargo test -p oppa-app` 29/29 and `-p oppa-shell-linux` 52/52 green on WSL; `linux_demo` presented + exited 0; `kitchen_sink` clean on llvmpipe-Vulkan and softbuffer; first live Linux input proof (Escape-at-root via xdotool). |
| ~~Full-workspace Linux gate~~ | `rounds.md` 19.2 | **Closed 2026-09-29:** five manifests target-gated; `cargo check --workspace --all-targets` green on Linux (first in repo history); Linux clippy + fmt clean on the touched crates. |
| Live-OS proof passes — agent-mechanical half | ~~`rounds.md` OQ deltas~~ → **closed by 19.3** | **Closed 2026-09-29:** real COM save + folder dialogs opened and dismissed (`Ok(None)`) on the live desktop; `WM_CLOSE` destroy live; TSF chain S_OK on the live window (probes deleted after use). |
| Live-OS proof passes — human-invited half | `rounds.md` 19.3 OQs | IME composition manual pass (the 2.1 standing pass), registry/portal theme flips both directions, menu edge/tooltip eyeball, Windows Escape-at-root retest with HWND-targeted input, close-veto with a handler-installing app. Needs a person at the keyboard; Phase 19 stays open on these. |
| ~~`oppa-shell-win` live-OS flakiness watch~~ | `rounds.md` 15.2 OQ → **closed by 19.2** | **Closed 2026-09-29:** reproduced in isolation (deterministic), root-caused — sandbox clipboard virtualization returns a NULL handle with `ERROR_SUCCESS`; production error now names the condition; test skips loudly per the 17.3 precedent. Environment, not a code bug. |
| ~~Vulkan surface fix-up (successor round, precisely scoped)~~ | `rounds.md` 19.7 → **closed by 19.8** | **Closed 2026-09-30:** teardown race root-caused (`WM_CLOSE` destroys HWND in `drive_cmd`; loop-bottom poll misreads dead window as 1×1 resize); `hwnd_alive` guards + one loud reconfigure retry; 7.4 matrix green. Minimize-to-1×1 thrash recorded as benign follow-up (Round 20.2). |
| ~~Device-crate `MainEvent` shape drift (android-activity 0.6.1)~~ | `rounds.md` eyeball 323 OQ → **closed by 20.1** | **Closed 2026-09-30:** `MainEvent::Resume { .. }` / `SaveState { .. }` struct-variant patterns at `oppa-android-app/src/lib.rs:185-191` + `surface.rs:197`; aarch64 check green. |
| Theme eyeball follow-ups (fixed decorative literals, `SELECTION_FILL`, OS title-bar theming) | `rounds.md` eyeball fix-up OQ (decision 323) | Deliberate non-goals per decision 323; not debt. |
| ~~Android sustained damage-loop harness mode~~ + device session | `rounds.md` 19.4 → harness **built by 20.4** | **Harness built 2026-09-30** (`run_sustained_damage_loop`, cold+steady record into oracle/meta). Still equipment-bound: no device attached; emulator stacks are mapped walls (decisions 142/171). Remaining: an on-device session (human + phone) to collect the numbers. |
| Firefox full-leg web automation (BiDi rig extension) | `rounds.md` 19.5 | Boot smoke PASSED (first Firefox evidence; ready/title/zero errors). The 14-leg input automation is Edge-shaped; porting to BiDi is optional tooling, not framework debt. |
| ~~Runtime window icon~~ | `rounds.md` 16.3 OQ, decision 316 → **closed by 20.3** | **Closed 2026-09-30:** `WindowIcon::new` (loud validation) + `WindowControl::set_icon` / `DesktopLoop::set_icon`; `WM_SETICON` SMALL+BIG with owned-handle lifecycle on Windows, `Icon::from_rgba` on Linux. |
| ~~Menu gaps: hover-highlight, viewport-edge clamping, drag-select into the menu~~ | `rounds.md` 17.1 OQs, decision 317 → **closed by 21.3** | **Closed 2026-09-30:** hover unifies with arrow highlight; `clamp_popup_anchor` flips Menu/ContextMenu/Tooltip inside the viewport; press-drag-release invokes via the new declared-only `DragRelease` router event. Right-held single-gesture drag-select stays out (tap-to-open). |
| ~~VirtualList/DataGrid scrollbar wiring~~ | `rounds.md` 17.2 + `ScrollbarProps` docs → **closed by 21.2** | **Closed 2026-09-30:** both controls share their instance offset with an attached `Scrollbar` (default on, `scrollbar()` builder); viewport/extent from settled box + `content_size`. |
| ~~Scrollbar wall-clock idle fade~~ | `rounds.md` 17.2 OQ → **closed by 21.2** | **Closed 2026-09-30:** `ScrollbarProps.idle_hide_ms` (attachments use 1200ms) via the 21.1 timer hook; hover/press keep chrome; `None` keeps the event-driven path. |
| ~~Emoji/ZWJ double-click word rules~~ + ~~scalar combining-caret~~ | `state.md` bidi-round verdict (locked #29, decisions 44/45); emoji half → **closed by 31.1**; caret half → **closed by 33.1** | **Closed 2026-10-01:** `WordClass::Emoji` (EP cover + ZWJ/VS16 glue, Word-like runs, class-tracked run slot so alnum/emoji never glue); double-click + word steps ride it. Regional-indicator pairs closed by 32.1. **Closed 2026-10-01:** `caret_boundary` steps shaped-cluster starts (shaperless stays scalar per decision 207). |
| c3 CDP/Edge re-baseline | `rounds.md` bidi round ("Separate c3 re-baseline owed") | Rig drift vs current Edge under CDP IME scenarios, unrelated to any gate; owed whenever the harness is next exercised. |
| ~~Per-key granular subscriptions~~ | M2 standing note in `state.md` §5 collections → **closed by 23.1** | **Closed 2026-09-30:** lazy per-key slots (`Store::get_keyed`, `Collection::get_row`); `Store::insert`/`Collection::update_row` notify precisely with no version fan-out; structural ops sync + broadcast. Inline children share their root's dep set (M2), so in-list precision stops at the window — documented open architecture question (effect-per-child). |
| Generic props in `component_manifest!` | `rounds.md` 14.1 OQ, decision 311 | Manifest mount needs a concrete monomorph for the pointer table; generic args refused loudly until a design lands. |
| Web text metric drift | decision 81 (`state.md` §6) | DWrite-side text-layout metric audit in progress; browser-side numbers stay open. |
| ~~`WM_MOUSEHWHEEL` (horizontal wheel input, Win32)~~ | `rounds.md` 9.2 + wheel-round OQs → **closed by 20.2** | **Closed 2026-09-30:** `WM_MOUSEHWHEEL` handled in `oppa-shell-win` (`ShellEvent::HWheel` → `Cmd::Scroll { dx, dy: 0 }` at `HWHEEL_LINE_PX` scale, tilt-right-negative); `drive_cmd` forwards `dx` into the bound horizontal feed. |
| ~~Minimize-to-1×1 thrash~~ | `rounds.md` 19.8 OQ → **closed by 20.2** | **Closed 2026-09-30:** loop-bottom poll skips the resize when `IsIconic` or raw client area is zero; pre-minimize viewport kept for clean restore. |
| Weak-GPU sustained cost (Mali-G52/Adreno-610 class; sustained damage loop) | `docs/08-performance/mobile.md`, decisions 171/175 | Snapdragon 870 measured (full-scene 86–88 ms, ~16.7 ms steady render); the hatch stays costed, re-evaluate on a hard wall (standing rule). |

## Done — compact ledger (do not re-plan; full deltas in `rounds.md`, code state in `state.md`)

| Item | Milestone / phase | Verdict |
|---|---|---|
| Hot-reload harness + fuzzer | M2b | Done (`state.md` §5h). Retire-not-unload holds (decision 61); true unload was **never** built and is v2 scope (`HANDOFF-V1.md` §4) — not an M9 debt. |
| Layout engine | M3 | Done (flex + block-lite + absolute + BiDi; locked #6). |
| CPU backend + FramePlan + image oracle | M4 | Done (= MVP; `state.md` §5j). |
| Events + hit-testing + focus + Toggle | M5 | Done (`state.md` §5k). |
| Vello backend + driver matrix | M6 | Done (`state.md` §5l). Skia hatch stays *costed, unbuilt* (decision 175). |
| DOM backend + parity corpus | M7 | Done (`state.md` §5m). |
| Virtualization + transition evaluator | M8 | Done (`state.md` §5n). |
| Reload product loop + fuzzer gate | M9 | Done (`state.md` §5o). Source-key pruning stays bounded by design; true unload never shipped (v2). |
| Android shell + AT-SPI + GLES row | M10 | Done (`state.md` §5p). AT-SPI is **Linux-scope by decision 135** — the old backlog row's "Android + emitters" pairing was wrong; Android a11y was evaluated-and-declined (`HANDOFF-V1.md` §5). |
| Linux text/shell, wasm text slice, Android text slice | v1 remainder | Done (rustybuzz + DejaVu/Ubuntu/Noto chain; bundled rustybuzz on web; JNI `/system/fonts` bridge). |
| Phases 8–18 (decisions 297–322) | Readiness plan | Done — see [`production-readiness-plan.md`](production-readiness-plan.md) execution log. |
