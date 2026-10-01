# Accessibility testing

Status: emitters mapped (M10); live-bus open.
Sources: `12-archive/DESIGN.md` §§2.3, 9.2; `12-archive/BUILD-ORDER.md` (M4/M10).

- M4: `SemanticsDiff` computed + dumped with the first runnable —
  asserted from the start, not retrofitted.
- Toggle end-to-end (M5) carries its `Semantics::switch` payload
  (checked/is-on/label/disabled) through commit.
- Text-edit a11y: role, live value, selection range, composition
  state + change events — v1 contract requirements exercised by the
  editing-contract suites.
- Linux AT-SPI (M10, `crates/oppa-atspi`): total role/state table
  + incremental tree mirror + wire event vocabulary; toggle and
  list-item proven emitting and queryable through the real
  pipeline. Live-bus CLOSED in v1-closure (WSL Ubuntu 26.04,
  real `at-spi2-registryd` 2.60): 25/25 checks serving our exact
  tree data — registry echo, role names+numbers, state numbers,
  children, extents, flip-signal delivery. (Background probe ran
  from M4 per the original plan — deliberately never a tail item.)
- Windows UIA (v1-closure, `crates/oppa-uia`): provider over the
  retained tree (CheckBox/ListItem/Edit/Group + Toggle/
  SelectionItem/Value patterns); toggle + list-item queryable
  through real COM interfaces with AT actions driving back
  into the framework. Event raising needs HWND hosting (shell
  window follow-up); `SetFocus`/AT-editing out of v1 scope.
- Web ARIA from M7, incl. the verdict-(b) text-edit payload.
