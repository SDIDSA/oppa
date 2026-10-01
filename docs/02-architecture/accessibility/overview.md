# Accessibility subsystem — overview

Status: design current (locked #3); computation planned (M4),
emitters planned (M7/M10). Sources: `12-archive/DESIGN.md` §§1–2;
`12-archive/BUILD-ORDER.md` (M4/M10).

- **Owns:** `SemanticsDiff` computation (reconciler + semantics
  payloads + committed layout bounds) and per-platform emitters.
- **Does not own:** visual rendering, hit-testing.
- **Emitters** (independent of backends and each other): Windows UIA,
  AT-SPI over DBus (highest-risk target; background probe from M4),
  Web ARIA (M7, incl. verdict-(b) text-edit payload).
- **M4:** `SemanticsDiff` computed + dumped alongside the first PNG.
- **Specs:** semantics payload shapes follow `crates/oppa/src/semantics.rs`
  (`Semantics::switch/list_item` builders in M2).
- **Tests:** [accessibility](../../07-testing/accessibility.md).
