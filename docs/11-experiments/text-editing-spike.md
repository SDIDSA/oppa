# Experiment: §9.2 text-editing spike (M1)

Status: **Accepted** — verdict (b) adopted (locked #27).
Raw: `spike/REPORT.md`, `spike/corpus.json`, `spike/results/
{windows,web,verdict}.json`, `spike/web/`,
`crates/spike-textedit/`.

One editable single-line field, built twice against one shared rig:
**Windows-GPU arm** (framework-authority `EditingSession` over
`DWriteTextService`) and **Web-DOM arm** (real `<input>` in headless
Edge via native clicks/keys + CDP IME scripting through Chromium's
real text-input state machine; hook records only).

Results: c1 geometry PASS (0.000 px vs `IDWriteTextLayout`; ≤1.41
px vs native EDIT); c2 235/238 (3 exact-midpoint ties) + word-rule
deltas; c3 core scenarios match, two classified divergences
(DOM commit shape normalizable; blur-commit adopted as spec) + one
rig gap (delete-range undrivable via CDP → real-IME passes); c4
`latin_edit` 9/9 exact incl. unprompted undo-selection match.

Every mismatch classified non-fundamental (tie-break rule,
browser word conventions, normalizable event shape, CDP artifact).
Variant A on Web (hidden-input authority) not built per scope —
unmeasured, moot under (b). Letter-tracking gap measured
(92.03125 vs 91.03125) → permanent no-delegation rule.

Resulting decisions: [ADR-0012](../10-decisions/ADR-0012-text-editing-authority.md);
specs [editing](../03-spec/text/editing.md),
[IME events](../03-spec/input/ime.md); contract tests
[editing-contract](../07-testing/editing-contract.md).
