# Project state (current)

Status: current. Last verified: 2026-10-01 (through Decision 370).
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
- Phase 36 PR3 (decision 355, gate green 2026-10-01): Text v2 —
  `VNode::RichText` (shape-per-span then join, shared size; joined
  bytes stay the caret/selection space); per-run ink splitting paint
  ops (single-ink scenes byte-identical) on the shared builder (CPU +
  Vello) and DOM spans; web `@font-face` serves the measured DejaVu
  bytes (closes decision-81 drift); DejaVu-pinned corpus extended
  (join transparency, bold-span breaks/carets/inks, empty-span).
- Phase 36 PR4 (decisions 356–359, gate green 2026-10-01): Render &
  animation v2 — multi-stop `Keyframes` (stops + segment ease +
  once/loop/ping-pong; keyframes win; target closes final leg; stamp
  snaps; DOM stepped inline path); native shadow blur (`blur_radius`
  on `DrawOp::Shadow`; Vello gaussian, CPU box-blur, CSS box-shadow;
  tol-16 < 10% hardware-calibrated; supersedes stepped expansion);
  static images via one cache deposit (CPU/Vello `insert_cached`, DOM
  PNG data-URI; URL keys unchanged); retained `Canvas` (`Tag::Canvas`
  + spec; lowers to Rect/RRect/Path/Text; childless leaf).
- Phase 37a (decisions 360–361, gate green 2026-10-01): Ergonomics —
  `ctx.child_auto`/`child_keyed` on `(caller, ordinal)` (no `TypeId`);
  53 control call-sites migrated (load-bearing manual keys stay);
  `component_manifest!` monomorphized generics (`Name::<A>(P<A>)`,
  canonical symbols; bare/shorthand forms refuse loudly, proven
  end-to-end in a new reload binary).
- Phase 37b (decisions 362–364, gate green 2026-10-01): App services —
  `TaskId` cancellation (parked/queued never run, dependents unblock,
  stage `Cancelled`; running sets a cooperative token) + pluggable
  `Fetcher` (scripted/closure/wasm-binding) with `cancel_fetch`→`Idle`;
  zero-stdout host-level ring diagnostics; signal/collection
  write-through persistence over `KvStore` (seed-once, encode/decode;
  seed warns + initial, writes panic, corrupt snapshots seed empty).
- Phase 37c (decision 365, gate green 2026-10-01): `cargo oppa new`
  scaffolder (embedded templates, package rename, checkout-pinned
  path deps, loud refusals — a generated desktop project compiles
  against the live checkout); templates ride `child_auto`; G9 reload
  recipe on `oppa-reload` rustdoc (+ `app_loop`).
- Phase 38a (decisions 366–367, gate green 2026-10-01): Form
  validation props (G7) — `invalid` / `required` / `error_message` /
  `helper_text` on TextInput, TextArea, Checkbox, Toggle,
  RadioGroup, Select, Slider (announced marks via decision-352
  payloads, `error`-ink borders, footer captions; validators stay
  app-side; valid trees byte-identical); Slider `Home`/`End` jumps
  + `value_num`/`min_value`/`max_value` range announcement (G18)
  over the continuous pointer-capture drag.
- Phase 38b (decisions 368–370, gate green 2026-10-01): New
  controls part 1 — `Tree` (G12: hierarchy over `Collection`,
  `vlist_window` windowing + slot recycling, chevron `Path`s,
  Up/Down/Left/Right/Enter, `Tree`/`TreeItem` roles), `Splitter`
  (G13: `Vertical`/`Horizontal` 2-pane divider, `ColResize` /
  `RowResize`, fraction signal + minima clamping, drag + arrow
  nudge), `DatePicker` (G14: controlled `Date`, `Portal` month
  `Grid` popup + `TextInput` `YYYY-MM-DD` parse bridge, min/max
  bounds on every path, arrow/Home/End day steps).

## Now (in progress)

- Phase 38 (Control Catalog, Cookbook, Styling, Packaging & PWA):
  38a–38b shipped (validation + slider keys; Tree, Splitter,
  DatePicker); remaining: Toolbar/Menubar/FilePicker +
  RichText/Image/Canvas/NavHost wrappers (G21–G22); G10/G11
  cookbook + styling; theme/styling pass; desktop installers + PWA
  packaging.

## Next (accepted, not started)

Ordered productization P1s (G10–G11, G19–G23): cookbook additions,
styling page flip, packaging promotion, PWA story,
Toolbar/Menubar/FilePicker, RichText/Image/Canvas/NavHost,
persistence helpers.

Planned v2 specs (unimplemented; full text in git history):
DOM→framework value-loop text entry.

Accepted-carry: c3 CDP/Edge re-baseline whenever the harness is next
exercised, Firefox full-leg automation (optional tooling), weak-GPU
sustained-cost standing rule.

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
- Standing debt: one pre-existing `FpsApp` clippy lint on record; hot-reload true unload is v2 scope; inline-child effect precision stops at the window (open architecture question).
- Deliberate non-goals (not debt): fixed decorative literals, `SELECTION_FILL`, OS title-bar theming; right-held menu drag-select; color-emoji rendering.
