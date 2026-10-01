# Experiment: real-IME verification passes

Status: **Accepted** — freeze condition (a) closed (locked #28).
Raw: `spike/IME-PASS.md`, `spike/IME-CONFOUNDER.md`,
`12-archive/IME-SESSION.md`, `spike/results/ime_manual.json`,
`crates/oppa-shell-win/`, `crates/spike-textedit/src/bin/
spike_ime_shell.rs`.

Sequence (all recorded, none smoothed over):

1. **M1 remainder — FAIL, composition never engaged.** Plain Win32
   window; `WM_IME_STARTCOMPOSITION/COMPOSITION/ENDCOMPOSITION`
   zero times; keys fell through as plain text. Classified
   rig/automation gap (TSF-only IME vs. IMM compat layer), not a
   session bug. Raw: `spike/IME-PASS.md`.
2. **TSF-aware re-run — FAIL, still no engagement.**
   ThreadMgr+DocMgr+focus+`IS_TEXT` with `punk=None` (S_OK) —
   isolated the variable to the missing `ITextStoreACP`.
3. **Confounder:** zh-Hans-CN language features were still
   installing during both FAILs (IME-readiness unisolated alongside
   the missing store). Raw: `spike/IME-CONFOUNDER.md`.
4. **Text-store round — FAIL with composition ENGAGED.** Full
   28-method `ITextStoreACP` + owner-QI composition sink; first real
   Pinyin composition (`nihao` → `你好` + atomic undo). Remaining
   FAIL items were scenario-semantics (focus-click race;
   real-TIP Esc-finalizes vs. IMM-era cancel expectation).
5. **Verification round — PASS 6/6, zero divergences, twice
   consecutive hands-off** (real Pinyin held readings, commit,
   atomic undo, Esc-cancel through PlatformShell's own TSF window +
   store). Two check-side bugs fixed explicitly (Pinyin quote
   normalization; undo caret = select-all state). Raw:
   `12-archive/IME-SESSION.md`.

Unification: activation S_OK ⟺ focused window at call time
(decision 40). TIP reading style varies run to run (held vs.
per-letter) — honestly undetermined, re-investigate if seen.
