# Accessibility model

Status: current as design (locked #3); emitters **Planned** (M4/M7/M10).
Sources: `12-archive/DESIGN.md` §§1, 2.2–2.3, 9.2; `12-archive/BUILD-ORDER.md` (M4/M10).

The accessibility/semantic tree is **computed from the UI model and
emitted as part of the renderer contract from day one** (locked #3).
Semantics live inline on retained nodes (`.semantics(...)` written in
the same expression as visuals, so role/state cannot drift) and diff
like style: `SemanticsDiff` flows through `PlatformShell::semantics`
with layout-committed bounds, in the A11Y scheduler phase.

Text-edit payload is a v1 contract requirement, not a retrofit: role
`TextField`, live value, selection range, composition state (composing
string + caret), plus selection/IME change events for announcements.

Per-platform emitters are independent of backends and of each other:
Windows UIA, Linux AT-SPI over DBus (**highest-risk a11y target** —
background probe starts at M4, deliberately not a tail item), Web ARIA
(M7, incl. the verdict-(b) text-edit payload).

See also: [a11y architecture](../02-architecture/accessibility/overview.md),
[testing: accessibility](../07-testing/accessibility.md), [code:
`crates/oppa/src/semantics.rs`](../../crates/oppa/src/semantics.rs).
