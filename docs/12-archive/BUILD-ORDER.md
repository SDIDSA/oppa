# Build-Order & Milestone Plan — v1 (post-R5)

Companion to `DESIGN.md` (v1, closed). All architecture is settled except two
empirically-gated items: the text-editing authority spike (§9.2, not yet run)
and the Vello maturity tripwire (§6/§8.7, not yet triggered). This document is
build order only — it does not re-argue any locked decision.

Assumption stated once: **~4 engineers** (2 core, 1 platform/rendering,
1 tooling+web). Sizes are person-weeks (pw) with calendar weeks assuming that
shape.

---

## 1. Dependency graph

| Subsystem | Structurally requires first | Parallel? |
|---|---|---|
| **Node/signal storage** (generational slots, `NodeId` arena, handler registry, `StyleId` intern table, `keyed_state`) | Nothing — root | — |
| **Scheduler** | Storage — and nothing else. Co-designed with it: the §9.1 propagation contract *is* the reactive-core semantics; build as one unit | No |
| **Renderer contract** (traits + `TreeDiff`/`FramePlan`/`Caps`/`DrawOp`/`SemanticsDiff` types) | Only the storage data shapes (`NodeId`, `StyleId`, `ShapedRun`, `LayoutBox`) | **Yes** — pure type work, day 1 |
| **Reconciler/diff** | Scheduler EFFECTS phase, storage, VNode/Element shapes, keyed fragments, binding-edge trigger tracking | **Yes** — vs everything below it |
| **Layout engine** | Retained tree + LAYOUT pass-masks; TextService *interface* only (stub metrics to start) | **Yes** — unit-test against hand-built retained trees while the reconciler lands |
| **TextService** | Trait definition only; per-OS impls are mutually independent | **Yes**; the DirectWrite slice is on the *spike's* critical path, nothing else's |
| **Hot-reload harness** | RELOAD phase position, call-site-keyed signal slots, component manifest shape, opaque-props vtables, §9.6 lints | **Yes** — fully independent of every renderer; testable headless |
| **Vello backend** | Contract + shaped runs + glyph atlas | **Yes** — mutually independent of DOM and CPU backends |
| **DOM backend** | Contract + reconciler + layout boxes + §9.3 scroll wiring; **spike verdict (§9.2) only for its text/editing path** | **Yes** vs Vello; only the text path is spike-gated |
| **CPU fallback backend** (tiny-skia) | Contract + DrawOp set only | **Yes** — and cheap; see M4 |
| **A11y tree** | SemanticsDiff computation (reconciler + semantics payloads + committed layout bounds); per-platform *emitters* (UIA / AT-SPI / ARIA) are independent of backends and of each other | **Yes** per platform |
| **Editing session service** | Spike verdict, TextService slices, focus/IME plumbing | Gated by M1 (GPU-side regardless of verdict) |

True roots: **three** — storage+scheduler, contract types, TextService.
Everything else fans out from them. Only two cross-track gates exist:
spike verdict → DOM text path; drain-before-unload → scheduler (already
locked, no gate).

---

## 2. Sequenced milestones

### M0 — Reactive core + storage + scheduler skeleton · wk 1–2 · 2 eng · ~4 pw

- **Build:** the five primitives with locked semantics (topo propagation,
  one-run-per-pass, call-site tie-break, 3-pass re-entry budget with debug
  cycle printer, structural-equality gate, memos-never-write assert,
  `BatchGuard`, pull-recompute), generational slots, `NodeId` arena, handler
  registry, `PassMask`, on-demand phase loop with all seven phases
  (INPUT/RELOAD/LAYOUT/PAINT as no-op stubs), worker queue drained at INPUT.
- **Deps:** none.
- **Proves:** **#19** (propagation contract executable — budget actually
  bounds, cycle assert actually prints), **#11** (generational slots +
  generation checks mechanical, not prose), **#20**'s UI-thread/queue shape,
  **#18**'s on-demand loop.
- Everything downstream is CI-testable without pixels from here on.

### M0b — TextService slices · wk 1–2 · 1 eng · ~2 pw (parallel with M0)

- **Build:** trait + minimal DirectWrite slice (enumerate, shape, measure,
  cluster map, DPR rounding) + minimal rustybuzz-class slice compiled to wasm
  (doubles as the Linux track's first cut and feeds the spike's Web-A arm).
- **Deps:** trait only.
- **Proves:** §8.2 first cut; the measure↔shape protocol is implementable as
  specified.
- **Unknown named:** DirectWrite cluster↔index semantics under combining
  marks/ZWJ — exercised by the spike corpus, not assumed.

### M1 — §9.2 TEXT-EDITING SPIKE · wk 2–3 · 2 eng · ~2 pw

Placed at the earliest slot its dependencies allow; the only prerequisite is
M0b.

- **Build:** exactly the doc's spike — one editable single-line field;
  Windows-GPU arm (shaped runs drawn **through Vello**, which folds in the §6
  confirm item: does Vello's glyph API actually accept externally-shaped,
  subpixel-positioned runs), Web-DOM arms A and B; the four pass/fail
  criteria as a scripted rig, not manual QA.
- **Deps:** M0b + a minimal window shell (`set_ime`, cursor rect). The Web
  arms need none of the core.
- **Proves:** **#24** — turns "decided by the spike" into *decided*; also
  converts §6's "consumes exactly" claim from prose to evidence. It is also
  the first real consumer of the measure↔layout protocol.
- **Output:** if (b) wins, the DOM-backend milestone gains the
  second-text-path contract clause (an amendment, not architecture). Runs
  fully parallel with M2 — no core-track blocking.

### M2 — Reconciler + component model (headless) · wk 3–5 · 2 eng · ~4 pw

- **Build:** `Ctx` per-instance dep tracking, component-fn model,
  VNode/Fragment/Hole + keyed lists, diff vs retained arena → `TreeDiff` +
  payload deltas + pass-mask setting; binding-edge (`ctx.binding`)
  trigger-set tracking in the scheduler with `suppress_transitions` stamping
  in the diff payload (as data, before any evaluator exists); `keyed_state`
  with LRU.
- **Deps:** M0. Parallel with M1.
- **Proves:** **#4** (two trees: value-only changes yield zero structure ops
  — the §4.2 trace's diff half, asserted), **#12** (keyed slots +
  `keyed_state`), **#11** (handlers-as-ids round-trip through the registry),
  §9.4's stamp exists as data from day one.

### M2b — Hot-reload harness + fuzzer v1 · wk 3–5 · 1 eng · ~2.5 pw (parallel with M2)

- **Build:** manifest export/scan, dylib swap, opaque generation-tagged props
  with hot-side vtable clone/drop, drain-before-unload, atomic registry flip;
  §8.1 re-seeding debug-assert + call-site lint; §9.6 crate-level state lint;
  §8.4 fuzzer v1, **already extended to the §9.6 task/message path**.
- **Deps:** M0 slots/registry; M2's manifest shape.
- **Proves:** **#14** (incl. new-component-addition via manifest scan),
  **#25** (residence rule: Store/signals/`keyed_state` survive; hot crate
  holds nothing), §5.3 drain ordering, §9.6 cancellation/discard race
  closure.
- Identity churn gets adversarial coverage *before any renderer exists* —
  fuzzed against the headless diff harness.

### M3 — Layout engine · wk 5–7 · 2 eng · ~5 pw

- **Build:** flexbox subset + block-lite + absolute positioning (both locked
  showcase examples use `.x`/`.absolute_y` — enumerated into scope per §7.6's
  honest reading), inline text runs (line break, BiDi; ellipsis optional-v1),
  measure↔layout protocol, one-frame-delayed feedback wiring, DPR rounding
  rules shared by all future backends.
- **Deps:** M0 (LAYOUT phase), M2 (retained tree), M0b.
- **Proves:** **#6** (engine-owned layout, stable boxes); **the wrap
  round-trip count is measured here, not assumed** — if wrap needs >2 passes
  or re-entrancy, that's a documented scope finding against §7.6's string,
  flagged at this milestone, not re-architected.
- **Unknown named:** measure round-trip count; DirectWrite-vs-wasm metric
  drift (feeds §8.8).

### M4 — FIRST RUNNABLE · wk 7–8 · ~2.5 pw

See §3 for the justification.

- **Build:** tiny-skia CPU backend implementing the full contract, FramePlan
  builder from dirty subtrees, per-surface commit path, SemanticsDiff
  computed + dumped, and the **headless image-diff / full-repaint-assert
  oracle** (the "debug full-repaint + image-diff mode" idea) as a fourth
  pseudo-backend.
- **Deps:** M0–M3.
- **Proves:** **#5** — the contract is implementable by a backend sharing no
  code with the core (the assumption every "backend = weeks" estimate
  silently rests on); text-as-data end-to-end; damage discipline limits ops
  on the one backend where it must.

### M5 — Events, hit-testing, focus, first real widget · wk 8–10 · ~3 pw

- **Build:** normalized `InputEvent` plumbing, GPU hit-test walk,
  `pressed()`/`hovered()`/`focused()` primitives, keyboard events, minimal
  deterministic Tab order over the retained tree, **Toggle end-to-end**
  (incl. its `Semantics::switch` payload) on the CPU backend.
- **Deps:** M4.
- **Proves:** **#7**; and §4.1's central claim — one propagation mechanism
  replacing six — becomes load-bearing-tested (stuck-pressed-on-cancel
  solved in framework primitives, semantics written in the same expression
  as visuals); **#3** exercised by a *stateful interactive* widget, not a
  static dump. Input→visual within one frame (`BatchGuard`) becomes
  measurable.

### M6 — Vello backend + driver matrix · wk 10–12 · 1 eng + hardware · ~3 pw (parallel with M7)

- **Build:** full DrawOp coverage, glyph atlas from shaped runs, `Caps` incl.
  blur-degradation path, per-surface present at vsync cadence while
  animating, unchanged-surface skip.
- **Deps:** M4, M0b.
- **Proves:** **#17**, **#21** (per-surface commit, ≤1-frame skew observable
  and bounded), **#18** (TIME services transition interpolation on a backend
  where the compositor is us).
- **Deliberate stress, not assumption:** GLES 3.1-class driver matrix on the
  weakest available hardware; glyph-quality review against a native reference
  (AA/tessellation *is* the text pipeline here).
- **Named unknowns — not estimated around:** Vello glyph-API behavior
  (partially answered at M1), GLES driver coverage, upstream blur-filter
  maturity window. **Tripwire evaluated at this milestone's gate; SkiaBackend
  stays costed at 2–4 wk, built only on a hard wall.**

### M7 — DOM backend · wk 10–13 · 1–1.5 eng · ~4.5 pw (parallel with M6)

- **Build:** TreeDiff→DOM mutations, StyleId→CSS rules, §9.3 native scroll
  (overflow container + spacer; offset fed at INPUT; +4 overscan;
  `overflow-anchor: none` on slots), external-element hole, ARIA mapping
  incl. the text-edit payload per spike verdict.
- **Deps:** contract, reconciler, layout; **spike verdict gates only its
  text/edit path** — non-text work proceeds from wk 10 regardless.
- **Proves:** **#2**, **#23** (offset trails ≤1 frame, virtualization reads a
  foreign-fed signal without tearing), §8.5's caveat becomes *measured*
  (parity corpus: engine-measured vs browser-rendered in the flat subset),
  not documented folklore.
- **Named unknown:** browser candidate-window anchoring fidelity if (a) won —
  the same corpus re-run at production fidelity, cost bounded by the spike's
  verdict, not guessed.

### M8 — Virtualization + transitions + §9.4 stamp end-to-end · wk 13–15 · 2 eng · ~5 pw

- **Build:** `ScrollArea` (spacer, slot keys, `offset` — TIME-physics on GPU,
  INPUT-fed on Web), recycled `ContactRow` list, transition evaluator (TIME
  interpolation on GPU; CSS mapping on DOM) honoring the binding-edge stamp.
- **Deps:** M5 (offset/primitives), M6 + M7 (both backends), M2's stamp
  plumbing.
- **Proves:** **#13** (the full §4.2 payoff trace asserted against real
  backends: zero structure ops per scroll tick, ~30-cell repaint,
  per-instance selection), **#22/§9.4** (phantom-flash eliminated), §9.3's
  window-lag compensation.
- **Deliberate stress:** recycled slots with 120 ms bg transitions under a
  scripted offset sweep, image-diffed frame-by-frame via the M4 oracle, on
  both backends. Transition-on-recycle stops being a resolved-on-paper risk
  here.

### M9 — Hot-reload product loop + fuzzer gate · wk 15–16 · 1 eng · ~2 pw

- **Build:** swap integrated with the real loop — reload mid-scroll,
  mid-transition, mid-IME-composition (core-side session survives per §9.2),
  with live `ctx.spawn` tasks; fuzzer extended to the full §8.4+§9.6 matrix;
  body-edit latency measured against 0.1–1 s; re-seed warnings exercised with
  real source edits.
- **Deps:** M2b, M8.
- **Proves:** **#15** under adversarial timing, **#25** under load, §5.1's
  edit-class matrix against a real app. **Gate: fuzzer green here is the
  precondition for calling renderers frozen** (per §8.4's placement).

### M10 — Android + a11y emitters close-out · wk 16–19 · 2 eng + background · ~6 pw

- **Build:** Android shell (wgpu/GLES with Caps→CPU fallback validated on
  device), restart-only reload, platform TextService slice; emitters: Windows
  UIA, **AT-SPI over DBus** (background probe started at M4 — highest-risk
  a11y target per §8.3, deliberately not last), Web ARIA from M7.
- **Deps:** M6 (Android is Vello/wgpu), M7 (ARIA), M4 (SemanticsDiff), M0b.
- **Proves:** **#16**/§5.2 (restart path = cold-start path, no scheduler
  work), the §9.5 re-testable bet finally *measured* (Vello-on-weak-GPU +
  tiny-skia fallback at real mobile resolutions), §8.3's incremental-sync
  semantics validated, **#3** end-to-end on three platforms.

**Total: ≈ 19 calendar weeks ≈ 4.5 months** at the stated shape, with slack
for CI/tooling. Schedule-tightest shared-code slot: M3.

---

## 3. First runnable milestone

**M4: one static component — a styled `Div` (padding/border/radius/background)
containing one shaped line of text — through core→reconciler→layout→
`FramePlan`→CPU backend→PNG, plus a SemanticsDiff dump.**

Why this and not the toggle:

- It exercises **every contract type** (`TreeDiff`, `FramePlan`, `Caps`,
  `SemanticsDiff`, every trait method) with zero optional machinery — no
  events, no hit-testing, no transitions, no IME.
- It proves the renderer contract is implementable by a backend with no
  shared code and no unknown glyph API — the one assumption all downstream
  backend estimates rest on. If the contract is incomplete (a presenter
  secretly needing core internals), you learn at week 8, not week 16.
- It proves **text-as-data end to end** — the stress test's single most
  load-bearing assumption — through the whole pipe, not in isolation.
- It is degenerate but not throwaway: it becomes the permanent CI smoke test
  and the substrate of the image-diff oracle.

The toggle — the "obviously useful" first feature — would drag events,
hit-testing, transitions, and focus in front of the core-model proof,
inverting the risk order: usefulness demonstrated before the model is
validated.

---

## 4. Risk-ordered checkpoints

| Risk (§8/§9) | First testable at | Deliberate stress test + tool (milestone) |
|---|---|---|
| **Vello glyph/driver maturity** (§6, §8.7) | M1 (spike's Windows arm draws through Vello — glyph API) | M6: driver matrix on weakest GLES 3.1-class hardware + glyph-quality review vs native reference; CPU-fallback ops-submission check. Tripwire *evaluated* at M6's gate; SkiaBackend costed, unbuilt |
| **Hot-reload identity churn** (§5.3, §8.4, §9.6) | M2b (harness + re-seed assert + state lint) | M9: fuzzer at full §8.4+§9.6 matrix — reload-during-{scroll, composition, transition, input-burst}; asserts no post-retirement slot touch, no retired-gen result applied, no cancelled future resumed. **Gate for renderer freeze** |
| **Transition-on-recycle** (§9.4) | M8 (recycled slots + transitions + both backends exist) | M8 itself: phantom-flash test — binding-edge commits must create zero interpolators, image-diffed per frame via M4 oracle, GPU and DOM variants; re-exercised mid-reload in M9 |
| **State residence** (§9.6, #25) | M2b (Store/image_cache/tasks core-side from their first lines) | M9 fuzzer extension (task/message path), plus the crate-level lint running in CI from M2b — enforcement, not prose |

Secondary §8 risks, same treatment:

- **Web scroll window-lag** — first testable M7, stressed M8 (overscan/+4 and
  `overflow-anchor: none` asserted under scripted scroll, browser-real, not
  emulated).
- **Linux AT-SPI** — background probe from M4, validated M10; do not schedule
  it as a tail item.
- **DPR rounding determinism** (§8.8) — cross-backend box-compare assert
  added at M6/M7 (same tree committed to CPU, Vello, DOM; rounded boxes must
  be identical).
- **Web text parity** (§8.5) — corpus measured at M7, quantifying
  caveat-vs-blocker.

---

## 5. What NOT to build yet

1. **Tween/implicit-animation DSL** — most tempting: scroll feel (§8.6) looks
   like it needs it. It doesn't: §9.1's TIME phase services scroll physics
   and transitions directly; the DSL is a v2 authoring surface. Building it
   early re-opens a locked deferral and buys nothing currently blocked.
2. **Variable-height rows (prefix-sum)** — looks like a small `ScrollArea`
   extension; it reworks slot-identity semantics (uniform-height slot math is
   load-bearing in §4.2), i.e., rework of #13, not an add-on.
3. **Grid layout** — no locked showcase consumes it; it would compete for M3,
   the tightest shared-code slot in the schedule.
4. **wasmi component runtime (Android reload parity)** — looks like "just
   embed an interpreter"; the harness's identity machinery is dylib-shaped
   (`#[no_mangle]` manifests, cross-dylib vtables, generation tags) and its
   wasm portability is unexamined — building it now means designing a second
   harness before the first is fuzz-proven. v1 Android is restart-only by
   lock.
5. **Native-hybrid presenters (HWND embedding)** — tempting as "free IME/a11y
   on desktop"; it would mask exactly the contract gaps v1 exists to exercise
   (IME anchoring, a11y-in-contract, Caps degradation) and would moot the
   §9.2 verdict on desktop. It is the escape hatch if the Vello tripwire
   fires — a bounded backend project then, not a parallel track now.
6. **Text-stack consolidation (one bundled shaping stack)** — tempting while
   writing two slices for the spike; the correct consolidation depends on the
   spike verdict (browser-as-authority or not) and §8.8. Per-OS first is the
   locked frame; consolidate after M7.
7. **Image/video stack + `ExternalTexture` pipeline** (§8.9) — `Img` appears
   in the showcase, so this tempts early; it's named in the contract but
   unscoped. Ship static, pre-decoded images behind the content-addressed
   cache stub; build async decode/mailbox only after scoping, so the
   worker-queue and generation discipline aren't designed against a moving
   target.
8. **Multi-window/per-surface reload granularity, global undo** — pure v2;
   RELOAD's global apply is load-bearing in the drain argument, and per-field
   undo is the v1 contract.

---

One structural note: the only milestone whose output rewrites another
milestone's contract is M1, and it lands in week 3 — before any editing or
DOM-text architecture exists. Everything else is build-order mechanics, not
decisions.
