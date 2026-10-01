# v2 item 2 spec — paragraph shaping on the shared core

Status: planned (gate-critical per decision 186; nothing here is
implemented). Scope: one question — **what completes the text
milestone on the shared rustybuzz core, and how is it proven?**
The build answers the open questions below; the spec does not
pre-answer them.

## What exists today (all verified in-tree)

- `ShapedRun` contract ([text.rs](../../crates/oppa/src/text.rs)):
  clusters with byte ranges + advances, `measure_line`,
  single-line `caret_x` / `caret_rect`; locked rules in
  [shaping.md](../03-spec/text/shaping.md) (leading-edge
  carets, midpoint hit-test, no tofu, plain shaping).
- Shared core
  ([lib.rs](../../crates/oppa-text-rustybuzz/src/lib.rs),
  v1 bounds single-line): font-dir loader, script itemizer,
  rustybuzz shaper, caller-supplied fallback chain; serves
  the linux + android slices (dwrite shapes natively and
  separately).
- [`layout_text`](../../crates/oppa/src/layout.rs): `\n`
  hard breaks + **greedy width wrap at any cluster boundary**
  (splits words anywhere — no break opportunities), visual
  ordering, optional single-line ellipsis. Cross-line
  `caret_position` (line index + x, UBA duality) already
  exists and is proven on greedy lines.
- No line-break machinery in-tree (`unicode-segmentation`
  belongs to a wayland dep, not the text stack); no
  paragraph reference corpus.

## What item 2 adds (proposed, not decided)

1. **Break opportunities**: UAX #14-style classes computed
   pure in oppa core beside `layout_text`, consuming
   `ShapedRun`s — font-independent, so all three slices
   (dwrite included) are served at once. Opportunity-driven
   wrapping replaces greedy anywhere-splitting.
2. **Paragraph layout**: `\n` handling stays; per-paragraph
   opportunity wrapping; defined trailing-whitespace
   treatment (see Q2).
3. **Clusters/carets across lines**: `caret_position`
   proven across wrapped lines — per-byte expected carets
   pinned in the corpus, affinity at wrap points defined
   (see Q3).
4. **Byte-exact reference corpus per platform font set**:
   golden paragraphs with expected breaks, advances, and
   carets, recorded against Segoe UI / DejaVu / Noto-Roboto
   sets (variance policy in Q4; machine-local fonts stay
   listed doc debt).

## Acceptance (mechanical)

- Golden paragraphs wrap at break opportunities only
  (no mid-word splits except the defined over-wide
  fallback); committed lines + advances match the corpus
  byte-exact where pinned, tol-banded where named.
- Per-byte caret expectations hold across wrapped lines,
  including wrap-point affinity and trailing carets.
- M7 corpus (shaping, BiDi, editing) still green on all
  three slices' proof environments; corpus runner is a
  suite test, not a spike script.
- **App-level proof (not corpus-only):** real multi-line
  content in the out-of-repo todo app through the browser
  harness — wrapping labels proven on screen, not just in
  the oracle. The field-sizing episode proved corpus-green
  plus app-broken is a real failure mode; this milestone
  is not done without the app leg.

## Boundary (this item does not)

- No hyphenation dictionaries (opportunities per UAX #14,
  hyphenation out); no justification (ragged-right only);
  no vertical text; shaping features unchanged (plain
  shaping stays — decision 151's fourth-shaper refusal
  stands).
- Full multi-line *editing* (caret motion across lines in
  live fields) rides the corpus proofs but is not a
  separate acceptance — U8's loop stays the editing proof.

## Open questions (documented, never silently resolved)

- **Q1.** Break engine source: new pure-Rust UAX #14 tables
  in oppa core vs a `unicode-linebreak`-class crate dep?
  (Hand-written class tables would be invented data —
  adopt the crate or generate from UCD with a script.)
- **Q2.** Over-wide span fallback (break inside a cluster?
  push the whole span?) + trailing-whitespace treatment
  (trim, hang, or collapse?) — define against the corpus,
  never assume browser behavior.
- **Q3.** Wrap-point caret affinity: which line owns the
  break byte — trailing caret of line N or leading of N+1?
  Pinned in corpus, not assumed.
- **Q4.** Corpus scope + variance: which paragraphs/sizes
  per platform, and what is byte-exact vs tol-banded?
  (System fonts update under builds — Segoe UI ships with
  Windows — so the policy must name what's pinned.)
- **Q5.** Shaping interaction: keep shape-whole-paragraph
  -then-break (current, M3's no-re-shape rule) or
  shape-per-line? (Boundary shaping in Arabic/Indic scripts
  makes this a correctness question, not a perf one.)
- **Q6.** dwrite service path: the break layer consumes its
  native `ShapedRun`s identically — confirmed in the build,
  not assumed here.
