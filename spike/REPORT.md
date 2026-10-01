# Round M1 — the §9.2 text-editing spike (2026-09-25)

**The question this round answered empirically:** which text-editing
authority model survives contact with real platforms?

- **Model (a)** — the framework owns caret/selection/glyph-index uniformly;
  presenters only rasterize.
- **Model (b)** — presenter-owned editing authority (browser owns caret,
  selection, IME, undo); the framework guarantees *behavior*, not mechanism.

The spike was **built and run; the (a)/(b) call below is argued from its
measurements**, not asserted.

**Scope as handed down:** one editable single-line field, built twice
against the same corpus and pass/fail rig — the **Windows-GPU arm** (the
already-built `DWriteTextService` + a minimal editing session: caret,
selection range, insert/delete, single-level undo; framework-driven,
DirectWrite doing shaping/measurement only) and the **Web-DOM arm** (a real
`<input type="text">` in a real browser, driven through its native
machinery, with framework-side hooks that only record what the DOM reports
— selection API, IME composition events). Explicitly out of scope:
multi-line/paragraph editing, custom JS IME handling on the DOM arm, full
undo/redo stacks, renderer/layout work beyond placing and hit-testing this
one field, other platforms' text shaping.

**Artifacts:**

```
spike/corpus.json               the shared rig (single source of truth, both arms)
spike/results/windows.json      Windows-GPU arm raw results
spike/results/web.json          Web-DOM arm raw results
spike/results/verdict.json      per-criterion verdicts + recommendation
crates/spike-textedit/          the Windows arm + rig (Rust)
spike/web/                      the Web-DOM arm (instrumented page + Node harness)
```

---

## 1. What was built

### 1.1 The shared rig (`rig.rs` → `spike/corpus.json`)

One corpus, defined once, consumed by both arms — never hand-mirrored:

- **Hit strings** with two-way cluster tables (UTF-8 byte spans ↔ UTF-16
  code-unit spans ↔ device-px positions at DPR 1): `"Hello world"`
  (ASCII baseline), `"héllo"` and `"日本語"` (M0b's multi-byte set, so
  cluster/glyph-index correctness carries over), `"héllo 👍"` (surrogate
  pair exercised by the same click probes).
- **7 IME scenario scripts**: zh candidate commit (`nihao` → `你好`);
  ja romaji→kana→candidate (`konnitiha` → `こんにちは` → `今日は`);
  cancel mid-composition; in-composition caret navigation; rapid zh↔ja
  switching; delete-range re-anchor; focus loss mid-composition. Both
  languages are real multi-candidate sequences, not one-to-one mappings.
- **3 editing-op suites**: `latin_edit`, `multibyte_edit`,
  `undo_granularity` (see §3, criterion 4).
- The tolerance constant, the Web font stack (the family DirectWrite's
  fallback actually mapped per string, so both engines render the same
  faces), and tracking-width data for the letter-tracking gap check.

### 1.2 Windows-GPU arm (`crates/spike-textedit/src/session.rs`)

`EditingSession` — the framework-authority editing model:

- content in an **author-owned `Signal<String>`** (locked #24's controlled
  pattern), edits write through the signal;
- caret, selection, composition buffer, and **single-level undo** as
  core-side session state (IME commits are atomic: the pre-composition
  snapshot is the undo unit);
- the `ImeCompositionHandler` sink, fed through `ImeCompositionFeed` →
  `dispatch_ime_event` (the M0b no-lost/no-dup seam);
- candidate anchoring emitted through **`PlatformShell::set_ime`** — the
  contract surface — into a recording shell;
- caret motion and hit-testing via `ShapedRun`'s pure math
  (`byte_offset_for_x` midpoint rule, cluster snapping);
- frames run via `rt.run_once()` after each scripted step.

10 regression tests (backend-free, fake shaper).

### 1.3 Criterion 1's two platform references (`oracle.rs`)

- `IDWriteTextLayout::HitTestTextPosition` — DirectWrite's canonical caret
  API (per-UTF-16-unit caret x), run at DPR 1 and DPR 2.
- A **real Win32 EDIT control** (`EM_POSFROMCHAR` / `EM_GETRECT`,
  DPI-unaware thread so client px == DPR-1 device px, `WM_SETFONT` Segoe UI
  16px) — the "native edit control with identical font/size/DPR" cross-check
  DESIGN §9.2 names.

### 1.4 Web-DOM arm (`spike/web/`)

A real single-line `<input type="text">` in headless Edge (Chromium),
pinned to the same font stack (kerning/ligatures off to match the
framework's plain shaping; `--font-render-hinting=none` so advances stay
subpixel-faithful). The page's hook script **only records** what the DOM
natively reports (composition events, beforeinput, selectionchange,
focus/blur, selection API) — no JS-side IME handling, no selection
overriding, per the round's scope.

Driving: real mouse events (clicks, drags, shift-click, double-click),
real keys (typing, arrows, End, Ctrl+Z), and IME through **Chromium's
native IME pipeline** via CDP `Input.imeSetComposition` (start / update /
cancel) — which routes through the same text-input state machine an OS IME
drives. Commit path discovered empirically: `Input.insertText` *during* an
active composition delivers `compositionupdate` + `compositionend`-with-
data — the real-IME commit shape. (`Input.imeCommitText` does not exist in
this Edge build; the probe log is recorded in the results.)

### 1.5 Engine fix the corpus pulled out of the M0b backend

`oppa-text-dwrite`'s analysis-source `GetLocaleName` returned a
`u32::MAX` "rest of text" length sentinel. That works for the *first*
`MapCharacters` call and makes **every subsequent call on the same source
fail with E_INVALIDARG** — so any string requiring more than one font
piece (`"héllo 👍"`, `"你好world"`, `"abc日本語x"`, any mixed-coverage
text) could not shape at all. M0b's tests only ever shaped single-piece
strings; the spike's corpus caught this immediately. Fixed (exact remaining
length, per the API contract, with a comment); all 13 M0b tests still
green. This is the round's second "engine bug found by a test the prior
round's corpus couldn't reach" — same pattern as M0b's `caret_x`
trailing-caret fix.

---

## 2. The pass/fail rig, restated concretely

1. **IME geometry (N = 2 device px; Windows arm).** Framework caret rects —
   as delivered through `PlatformShell::set_ime`, i.e. the contract surface
   — must agree with (a) `IDWriteTextLayout::HitTestTextPosition` at every
   cluster boundary and the trailing caret, on every anchor string, at DPR 1
   *and* 2; and (b) the real EDIT control at DPR 1; plus tracking through
   every composition edit and in-composition caret-navigation step (39
   steps over the zh/ja scenario composites).
   **N=2 justification:** M0b's handoff fixes the Windows caret tolerance
   at ±2 device px; 2 device px = 1 CSS px at DPR 2; far inside DESIGN's
   "within one caret height" bound; and it is the *tightest* bound that
   does not demand two different rendering pipelines (GetGlyphs/
   GetGlyphPlacements vs IDWriteTextLayout, and GDI-hinted integer advances
   for the EDIT control) agree subpixel-exactly.
2. **Hit-test parity (both arms).** Same integer x set per string
   (0..=ceil(width)+2 → 238 sweep probes over the 4 hit strings), the same
   cluster-boundary probes (leading±1, midpoint, trailing−1), and the same
   mouse-selection ops (drag, shift-click extend, double-click word select)
   with the same x construction. Indices normalized to code points
   (Windows: UTF-8 bytes → code points; Web: UTF-16 selection → code points
   through the shared cluster tables). Zero mismatches required to pass.
3. **Composition event fidelity (both arms).** The 7 scenario scripts must
   produce the same events, same ordering, same content at each composition
   step, with no lost or duplicated characters — and the *in-progress,
   not-yet-committed* state must be visible to the framework identically
   (Windows: the session buffer; Web: field value + compositionupdate data
   + selection).
4. **One model (both arms).** The same logical editing-op suite
   (insert / caret-move / click / drag / shift-click / dbl-click / undo)
   driven natively on each arm — same values, same click x-coordinates
   against the same current text — landing in the same observable state
   (value, caret, selection) at every step. Single-level undo per the v1
   scope.

Corpus gaps, stated not silently covered: **RTL/bidi** is absent per M0b's
handoff (bidi ordering is M3 work; runs carry `rtl` as metadata only), and
DESIGN's combining-marks/ZWJ corpus entries were not in this round's corpus
(a flag in `verdict.json`, not coverage).

---

## 3. Raw results (every number lives in the results JSONs; nothing averaged)

| Criterion | Windows-GPU arm | Web-DOM arm |
|---|---|---|
| **1 — IME geometry** | **PASS.** Max Δ vs `IDWriteTextLayout`: **0.000 device px** on all 8 anchor strings at DPR 1 and DPR 2 (worst index cell, trailing carets included). Max Δ vs the native EDIT control at DPR 1: **1.41 px** worst (こんにちは, CJK-via-fallback; all strings ≤ 2.0). Oracle intra-cluster spread 0 everywhere — neither engine positions a caret inside a cluster. Composition tracking: **39/39 steps ≤ 0.000 px** | N/A by scope — criterion 1 is Windows-arm-scoped in this round's restatement; native IME anchoring is the browser's own under (b). Recorded fact: a real `<input>` exposes **no queryable caret rect** to the framework (selection range rect reads 0×0) — a (b)-side fact, not a pass/fail |
| **2 — hit-test parity** | 238-probe sweep answers produced | **235/238 sweep probes identical**; the 3 mismatches are all the **exact cluster-midpoint tie** on 日本語 (x=8/24/40): the framework switches AT the midpoint (`x < mid → leading`), Chromium switches from mid+1 — a ≤1 px tie-break rule, not geometry. Selection ops **9/12**: all 3 mismatches are **double-click word rules** — the browser includes the trailing space after a word (×2) and dictionary-segments 日本語 as one word (×1). Boundary-geometry bisection: the browser's own click-boundary positions vs the framework's midpoints within **±0.5 px on every string** |
| **3 — composition fidelity** | **PASS.** All 7 scenarios: canonical stream == script exactly (no lost, no duplicated), state follows the session's documented semantics | Core scenarios (zh commit; ja kana→candidate; cancel-mid; in-composition caret nav; rapid zh↔ja switch): **value matches the framework composite at every step**, no lost/duplicated characters, commits arrive in the native IME shape (`compositionend`-with-data). Two classified divergences — see §4 |
| **4 — one model** | **PASS** (suites recorded with the resolved op_x coordinates the Web arm clicks) | `latin_edit` **9/9 exact** — value, caret, and selection at every step, *including the browser restoring the pre-undo selection [3,8) and caret 8 exactly like the framework session's single-level undo*; `multibyte_edit` 4/5 (dbl-click word rule — same finding as criterion 2); `undo_granularity` 3/4 (deliberate exposure: single-level undo restores pre-'b', the browser coalesces the typed burst and restores pre-'ab') |

---

## 4. Failure classification — the actual deliverable

Every mismatch above, with the fundamental-vs-fixable call:

| # | Finding | Classification | Why |
|---|---|---|---|
| 1 | Exact-midpoint tie-break (criterion 2, 3 probes) | **Spec-able rule detail** (not fundamental, not rig) | A one-line contract rule ("ties resolve to the leading edge" vs Chromium's from-mid+1); ≤1 px effect, full-width chars only |
| 2 | Double-click word rules (criteria 2+4, 3 probes) | **Spec-able rule detail** | Browser conventions: trailing-space inclusion; CJK dictionary segmentation (日本語 as one word). The shared suite must spec them; the framework session may adopt browser-compatible rules |
| 3 | DOM commit/cancel = `compositionupdate`→`compositionend` pairs (criterion 3) | **Implementation/contract gap — normalizable** | The DOM carries *more* events, not fewer; the content invariant holds; the same normalization class as §9.3's scroll events |
| 4 | Composition caret ±1 during composition (criterion 3) | **Rig artifact** | The caret *is* reported via the selection API (visibility proven); the exact positions come from CDP's `compositionCaret` param semantics, not a real IME |
| 5 | Delete-range-mid-composition (criterion 3) | **Rig gap — flagged, not silently dropped** | Not drivable through the CDP IME surface in this Edge build; needs one manual pass with a real OS IME before the DOM text contract freezes. Unverifiable here ⇒ **not a verdict input** |
| 6 | Focus-loss mid-composition (criterion 3): Chromium **commits** on blur; the spike session **cancels** | **Contract alignment item** | Every native platform commits on focus loss; the editing session should adopt commit-on-focus-loss. The Web arm is not the outlier — the session policy is |
| 7 | Undo burst granularity (criterion 4, 1 step) | **Spec-able contract item** | v1's single-level undo vs the browser's burst coalescing — exactly what the shared suite exists to spec |
| 8 | `<input>` caret rect not queryable | **(b)-side fact, not a failure** | Moot under (b) (the browser anchors IME natively); would have been an (a)-side cost |

**No criterion-2/3/4 failure is a fundamental authority-model mismatch.**
At no point did the DOM's native selection/IME machinery prove unqueryable
or unoverridable to the point that the framework's editing semantics could
not be observed or matched through it. The opposite happened: the DOM
natively *matched* the framework session exactly on the latin_edit suite,
including undo-restore-selection, unprompted.

---

## 5. The recommendation: **(b) on Web** — argued from the criteria

**Model (b): presenter-owned editing authority on Web (real `<input>`),
with the framework guaranteeing *behavior* through the shared
editing-operation suite.** The argument, from the numbers:

1. The DOM's native mechanism **meets the behavior contract natively**:
   criterion 2 at 235/238 + 9/12 with every mismatch a spec-able rule
   detail and geometry agreement to ±0.5 px; criterion 3 with in-progress
   composition state fully observable (field value + compositionupdate
   data + selection), native-shaped commits, and no lost/duplicated
   characters; criterion 4 with latin_edit exact 9/9.
2. **Model (a) on Web was not built this round** (scope: two arms), and its
   Web-specific costs — candidate-window anchoring fidelity over a hidden
   input, hit-test parity on framework-rendered text, ARIA-mediated a11y —
   are precisely the unmeasured part. (b) does not require risking them;
   (a) does. DESIGN's decision rule ("if A meets the criteria on Web, (a)
   wins; if A fails criteria that B passes, (b) wins") presumes both Web
   variants ran; with A-on-Web unmeasured and B-on-Web meeting the
   contract, the evidence-supported choice is (b) — stated with that
   mapping explicit, not silently treating the Windows arm as A-on-Web.
3. The (b) costs are enumerable and bounded: two editing implementations
   kept behaviorally in sync — mitigated by making this round's criterion-4
   rig the permanent cross-backend contract test — plus the enumerated
   normalization/spec items in §4.

**What this does to locked #5 (presenters are not fully stateless):**
round 5 already conceded the direction ("possibly editing sessions, pending
the §9.2 spike"); the spike **confirms** it and makes it concrete. The Web
presenter owns editing sessions (browser caret/selection/IME/undo) on top
of browser-hosted scroll state (§9.3) and browser-laid-out text. Two
normative follow-ups: amend locked #5 (drop "possibly") and write the
renderer contract's second text path as a first-class clause (editable
fields = presenter-recognized special case on Web). GPU backends own
editing under either verdict (locked #24) — this round's Windows arm
(session model + `set_ime` anchoring + the two-reference caret
verification) is exactly the mechanism they will use.

**Residual open items recorded:**
1. If (a) is ever (re)considered for Web, its hidden-input candidate-
   anchoring fidelity remains unmeasured by this round — moot under (b).
2. The IME delete-range (composition replacing a selection) path needs one
   manual real-IME pass before the DOM text contract freezes.

**Cross-backend contract gap check (M0b's letter-tracking convention)** —
compared, not silently diverged: the Web-DOM arm does not use `ShapedRun`
at all, so the "advance on every glyph except the run's final one (trailing
caret == run width)" convention is not needed there. Its equivalent
measurement (CSS `letter-spacing`) measurably diverges: "Hello world" at
16px with 1px tracking → DOM **92.03125** px vs framework **91.03125** px
(+1.0 = exactly one tracking unit; CSS letter-spacing also applies after
the final character), while untracked widths agree exactly
(81.03125 == 81.03125). Consequence: under (b) with native fields the gap
stays dormant (the framework never measures tracked text for DOM
rendering); under (a) — or any framework-measured tracked text on Web —
the framework must not delegate tracked measurement to CSS letter-spacing
or the trailing caret/width diverges by one unit.

---

## 6. Interpretation decisions (where the spec was ambiguous)

Full records live in STATE.md §6 (decisions 17–26); the short list:

1. **N = 2 device px** for criterion 1, with justification (§2).
2. **Criterion 1 is Windows-arm-only** in this round's restatement; the Web
   arm's native IME anchoring is the browser's own.
3. **Scripted IME on both arms**; "candidate selection" emulated as
   commit-with-different-text (nihao → 你好; konnitiha → kana → 今日は). A
   real OS IME's candidate UI is not scriptable in either rig; the spike's
   question is the event stream and in-progress state.
4. `CompositionUpdated.caret_byte` is **composite coordinates** (content
   prefix + composition text), consistent with M0b's existing test data.
5. **Focus-loss policy flagged**: session cancels (per M0b's cancel test);
   Chromium commits; adopt commit-on-focus-loss.
6. The spike's **word rule is spike-local** and was deliberately exposed
   against the browser's divergent conventions.
7. **Variant A on Web was not built** (round scope: two arms); the (a)/(b)
   decision rule is applied with that mapping stated explicitly.
8. **RTL + combining-marks/ZWJ recorded as corpus gaps** (flagged in
   verdict.json), not silently expanded.
9. Suite geometry ops resolve x against the **current composite** (text
   evolves during the suite); both arms click the same recorded x against
   the same current value.
10. Runtime integration is deliberately minimal (no M5 hit-test routing
    yet) and stated, not assumed.

---

## 7. Failures hit and fixed during the round

- Multi-piece shaping `E_INVALIDARG` (the `GetLocaleName` sentinel) —
  backend bug found by the corpus, fixed in `oppa-text-dwrite`; M0b's 13
  tests still green.
- EDIT-control oracle initially mis-measured: DPR-2 runs compared against
  the DPR-1-only control (systematic 2× deltas), and the GDI prefix width
  measured without the font selected into the DC. Fixed; per-cluster
  deltas then landed ≤1.41 px.
- Session word rule used the wrong boundary convention (`b0 <= b < b1`
  containment: a caret exactly on a cluster's start byte belongs to that
  cluster) — fixed and unit-tested.
- Criterion-4 geometry ops initially resolved x against the *original*
  corpus table instead of the current composite (clicks landed one cluster
  off after an insert) — fixed by resolving via the session and recording
  `op_x` for the Web arm.
- Three test-side expectation bugs in the new session tests (coordinate
  arithmetic); the engine was correct — same pattern as M0b's round.

---

## 8. Verification at round end

| Command | Result |
|---|---|
| `cargo test` (debug) | **88 passed / 0 failed** (core 61 + dwrite 13 + spike session 10 + 4 doctests) |
| `cargo clippy --all-targets` | clean |
| `cargo fmt --all -- --check` | clean |
| `cargo run -p spike-textedit --bin spike_win_arm` | `corpus.json` + `windows.json` regenerated |
| `node spike/web/harness.mjs` | `web.json` regenerated (headless Edge) |
| `node spike/web/compare.mjs` | `verdict.json`: c1 PASS / c2 FAIL(3+3 spec-able) / c3 FAIL(2 classified divergences + 1 rig gap) / c4 PASS_WITH_DOCUMENTED_DIVERGENCES |

---

## 9. Toolchain facts worth keeping (recorded to save the next round the archaeology)

- **CDP IME scripting (Edge ~140 headless):** `Input.imeSetComposition`
  works with `{selectionStart, selectionEnd, compositionText,
  compositionCaret}`; empty `compositionText` ends (cancels) the
  composition; the first `compositionstart` arrives together with the first
  non-empty update; commits via `Input.insertText` during composition
  arrive as compositionupdate→compositionend-with-data;
  `Input.imeCommitText` / `Ime.imeCommitText` are absent. Selection during
  composition is reported as the composition range (start → caret) — the
  DOM's "selection" is not a text selection while composing.
- **Real `<input>` limits (headless):** the caret rect of a focused input
  is not queryable via `getSelection()` range rects (reads 0×0).
- **Win32 EDIT control as a caret oracle (windows-rs 0.62):** `EM_GETRECT`
  / `EM_POSFROMCHAR` live in `Win32::UI::Controls`; `EM_POSFROMCHAR` packs
  x in the LOWORD (client coords; default format rect inset ~4 px left —
  subtract `EM_GETRECT.left`); returns −1 for the index at text length (use
  GDI `GetTextExtentPoint32W` with the same font for the trailing caret —
  and `SelectObject` the `WM_SETFONT` font first); run under
  `SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_UNAWARE)` so client
  px == 96-dpi px == the framework's DPR-1 device px.
- **DirectWrite (windows-rs 0.62):** `CreateTextLayout` takes text as
  `&[u16]` (no NUL; the length is derived); `HitTestTextPosition` takes
  plain `bool`. `MapCharacters` requires the source's `GetLocaleName` to
  return the *exact* remaining length — not a MAX sentinel (see §1.5).

---

## 10. What this round deliberately did not do

- Variant A on Web (hidden input; framework authority on Web) — out of
  scope; recorded as the residual open item.
- Vello drawing; the minimal window shell a real OS IME arm needs; the
  real-IME manual pass — M1 remainder.
- Multi-line/paragraph editing; full undo/redo stacks; bidi/RTL and
  combining-marks/ZWJ corpus entries (flagged, not covered).
- The DOM backend itself (M3+, gated by this verdict only for its text/
  editing path); the editing-session *service* proper (`ctx.edit_session`)
  — the spike built the model; the service integration lands with M2's
  component model.

## 11. Handoff notes

- The (b) verdict is a recommendation recorded in the spike's deliverables
  (`verdict.json`, this report, `STATE.md` §5.3). Making it normative means:
  amend locked #5 (drop "possibly"), write the renderer contract's second
  text path clause, and adopt the two flagged contract items
  (commit-on-focus-loss; shared-suite word/undo rules) into the
  editing-session spec.
- The criterion-4 rig (`corpus.json` op suites + cluster tables, driven by
  `spike_win_arm` + `harness.mjs` + `compare.mjs`) is the permanent
  cross-backend editing-contract test — keep it runnable as backends land
  (M3's DOM text path and M2's editing session both consume it).
- The GPU-side editing session model is proven and reusable as-is; next
  consumers: the real `ctx.edit_session` service (M2) and Vello caret/
  selection drawing (M1 remainder).
