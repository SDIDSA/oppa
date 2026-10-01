# Pointer input

Status: accepted as design (locked #7); routing current (M5 — see
`04-planning/state.md` §5k). Sources: `12-archive/DESIGN.md` §§2.2,
4.1; `04-planning/state.md` §5b.1.

- `Pointer(id, action /* Down/Move/Up/Cancel */, position, modifiers)`
  is the single normalized shape on all backends.
- GPU: framework hit-tests against the retained tree; Web: the
  browser hit-tests, the framework maps the event.
- Hit-test rule (stated, decision 94): deepest node wins,
  same-depth ties go to the later sibling, containment is
  inclusive-exclusive, unboxed nodes are skipped, misses return
  `None` (no silent root fallback).
- Capture rule (stated, decision 95): Down captures the press-owner
  node (self-or-nearest Press-handler ancestor); Up dispatches iff
  the up-hit lies in the capture subtree; Cancel clears with no
  dispatch; release-outside is a silent no-op; focus follows click.
- `pressed()`/`hovered()` are framework-owned per-instance signals —
  stuck-pressed-on-cancel and hover drag-out are solved once in the
  primitives, not per control (proven load-bearing-tested at M5:
  press/cancel/leave/release-outside leaves nothing stuck).
- Current: real Win32 mouse messages (`DOWN/DBLCLK/UP/MOVE`) flow
  through `pump_events` + the `Cmd` payload queue; the Win32→
  `InputEvent` mapping is platform-track follow-up (M6, decision 99).

Touch and gesture recognizers: no dedicated spec exists — the
project's v1 input surface is pointer/key/focus/IME. Do not invent
gesture semantics here; they are future work when a consumer exists.
