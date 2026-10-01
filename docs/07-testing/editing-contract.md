# Editing contract tests

Status: current (shared suite = permanent cross-backend contract).
Sources: `12-archive/DESIGN.md` §9.2; `04-planning/state.md` §§5–5g; `spike/corpus.json`.

The spike's criterion-4 rig is the permanent test both editing
mechanisms run: op suites (`latin_edit` 9/9 exact incl. pre-undo
selection restore; `multibyte_edit`; `undo_granularity`) + cluster
tables, driven by `spike_win_arm` + `harness.mjs` + `compare.mjs`
over `spike/corpus.json`. Its rules are spec: leading-edge
mid-cluster ties; browser-compatible double-click word selection
incl. CJK dictionary segmentation — binding the Windows-GPU session
too.

Keep runnable as backends land (M2's editing session and M3's DOM
text path both consume it). Corpus rig v2 adds bidi, decomposed
e-acute, ZWJ strings; editing ops on the new classes are M2-session
scope (stated, not silently covered).

Related: [editing spec](../03-spec/text/editing.md),
[experiments](../11-experiments/text-editing-spike.md).
