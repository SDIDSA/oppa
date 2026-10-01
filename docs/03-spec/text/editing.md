# Text editing behavior

Status: accepted (locked #24 + #27); GPU session current (spike
crate); product session current (G1: `EditSession` in
`crates/oppa/src/editing.rs` over `Ctx::edit_session` — decisions
205–208, bounded multi-level undo, focus-loss commits); DOM path
done (M7: real `<input>` authority + editing suite green). Sources:
`12-archive/DESIGN.md` §§2.3, 9.2; `04-planning/state.md` §5.3.

- **No new `Tag`.** Editable = behavior flag + framework
  editing-session service over the five primitives.
- **Content = author-owned signal** (controlled pattern); caret,
  selection, composition, single-level undo = core-side session
  keyed to the focused node, surviving hot swap. Undo: insert/delete
  runs coalesced, composition commits atomic, bounded depth.
  Global undo is v2.
- **Authority (locked #27):** GPU backends own editing; on Web the
  DOM backend owns editing authority (caret/selection/IME/undo) for
  recognized editable fields. The framework guarantees behavior
  through the **shared editing-operation suite** (the permanent
  cross-backend contract test), not mechanism.
- Adopted conventions (bind the Windows-GPU session too):
  commit-on-focus-loss; leading-edge mid-cluster ties;
  browser-compatible double-click word selection incl. CJK dictionary
  segmentation.
- **Clipboard (G3, decisions 209–211):** copy/cut/paste ride the
  `Clipboard` trait (`crates/oppa/src/clipboard.rs` — async-capable
  request/poll reads, `Result` writes, plain text only) through the
  `PlatformShell::clipboard` seam (Win32 wired; Linux/Android/Web
  refuse loudly); session ops in `crates/oppa/src/editing.rs`
  (`PasteOutcome` names every no-op).
- **Freeze: DECLARED (pre-M9 checkpoint).** Both §2.3 blocking
  conditions are evidenced closed: (a) real-IME delete-range
  verification — **CLOSED as locked #28** (two hands-off PASS runs,
  raw in `spike/results/ime_manual.json`); (b) bidi/combining/ZWJ
  corpus — **CLOSED** (combining parity + ZWJ single-cluster on
  both-arm agreement; bidi visual ordering closed in M3, oracle
  ≤2px at all 14 boundaries — locked #29). The DOM text/editing
  contract is frozen on this evidence.

Contract tests: [editing-contract](../../07-testing/editing-contract.md).
Evidence: [experiments](../../11-experiments/text-editing-spike.md).
