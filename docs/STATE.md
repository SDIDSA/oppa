# Project state (current)

Status: current. Last verified: 2026-10-01 (through Decision 351).
This is the only planning file. Rounds overwrite it in place — never append
snapshots, never create per-round files. History is in git (`git log -- docs/STATE.md`).

## Done (shipped — do not re-plan)

- Engine M0–M10: reactive core + storage + scheduler, reconciler/component model, hot-reload harness + fuzzer gate, layout engine, CPU/Vello/DOM backends, events + hit-test + focus, virtualization + transition evaluator, Android shell + AT-SPI emitters.
- v1 remainder + close-out: Linux/Android/Web text slices, phone round (Snapdragon 870 oracle + frame cost + visible present), present depth, Linux input, stale-APK rule.
- Productization Phases 8–35: controls catalog (15 controlled components), Task Studio reference app + E2E, `hello-desktop` + `hello-web` templates, Toast, cookbook, API-doc refresh, packaging recipes, `WasmHost` harness, testkit key/text helpers, first repo CI. Productization goals G1–G6 closed.
- Validation: full-workspace Linux gate, Windows live pass (COM dialogs, TSF chain, `WM_CLOSE`), Edge 14/14 + Firefox boot smoke, Vulkan teardown-race fix, Xn damage-loop harness mode.
- Phase 36 PR1 (decision 352, gate green 2026-10-01): Semantics + A11Y action
  parity — validation marks, numeric range bounds, `Role::{Tree, TreeItem,
  MenuItem}`; DOM `aria-invalid/required/errormessage/valuenow/min/max`,
  AT-SPI Action + Value in the wire tree, UIA Invoke + RangeValue with
  COM-thread → host-loop marshaling; MenuItem control migrated off ListItem.
- Phase 36 PR2a (decision 353, gate green 2026-10-01): Layout Grid — 2D
  `Tag::Grid` (`GridTrack::{Px, Fr, Auto}`, templates, spans, auto-flow),
  `flex_grow`/`flex_shrink` (weighted pool, `fill` ≡ weight 1, explicit
  wins, opt-in shrink), `min`/`max` clamping (resolved clamp, explicit
  contradictions refuse); all pre-existing layout suites byte-identical.
- Phase 36 PR2b (decision 354, gate green 2026-10-01): 2D `ScrollArea` —
  `ScrollOffset { x, y }` unification (`ScrollXY`/`ScrollOffset2D`
  sharing the 1D twin signals), horizontal thumb math, `content_w`
  overflow with x self-wire, `Shift+Wheel` routing at the shell layer
  (win + linux), transposed `Scrollbar` axis (incl. G18 range payload);
  all pre-existing scroll suites green.

## Now (in progress)

- Phase 36 PR3 (Core Engine v2): Text v2 — multi-span
  `VNode::RichText(Vec<TextSpan>)` shaped across line breaks per
  `v2-paragraph.md`, cluster-accurate caret/hit-test, web text-metric
  parity from bundled reference font bytes.

## Next (accepted, not started)

Ordered productization P1s (G7–G23): form-validation props, `cargo-oppa new` (G8), reload recipe (G9), cookbook additions, styling page flip, Tree, Splitter, Date picker, Grid + h-scroll, fetch backends/cancel, log facade, accessibility-action gap (UIA Invoke/RangeValue, AT-SPI Action/Value), packaging promotion, PWA story, Toolbar/Menubar/FilePicker, RichText/Image/Canvas/NavHost, persistence helpers.

Planned v2 specs (unimplemented; full text in git history): TIME-interpolation keyframes, shared-core paragraph shaping, DOM→framework value-loop text entry.

Accepted-carry: `component_manifest!` generics (refused loudly until designed), web text-metric drift audit, c3 CDP/Edge re-baseline whenever the harness is next exercised, Firefox full-leg automation (optional tooling), weak-GPU sustained-cost standing rule.

## Blocked (with blocker)

| Item | Blocked by |
|---|---|
| Android on-device sustained/thermal numbers | No phone attached; emulator walls mapped |
| Live human passes (IME composition manual, theme flips, menu/tooltip eyeball, close-veto app) | Needs a person at the keyboard |
| macOS/iOS shells | No Apple hardware; excluded by design |
| Weak-tier silicon validation (Mali-G52/Adreno-610 class) | Needs hardware wall |
| Safari pass, Firefox full automation | Needs machine/browser session |
| Multi-window `DesktopLoop` (secondary `SurfaceId` windows) | Deferred by plan; per-surface granularity is out — re-decision required to revive |
| Variable-height virtualized rows (prefix-sum index) | Deferred by plan — re-decision required to revive |
| `cargo oppa reload-check` as a G9 substitute | Unlisted extra scope, not approved — G9 is the doc recipe |

## Known issues (not code bugs unless noted)

- Environmental flakes: `oppa-shell-win` live-Shift sampling; sandboxed clipboard returns NULL handle with `ERROR_SUCCESS` (loud skip); transient clipboard box contention in full-suite runs.
- Stated edges: ZWJ-inside-regional-indicator double-click selects the joiner alone (pathological); right-held single-gesture drag-select stays out (tap-to-open).
- Standing debt: one pre-existing `FpsApp` clippy lint on record; web text-metric drift open; hot-reload true unload is v2 scope; inline-child effect precision stops at the window (open architecture question).
- Deliberate non-goals (not debt): fixed decorative literals, `SELECTION_FILL`, OS title-bar theming; right-held menu drag-select; color-emoji rendering.
