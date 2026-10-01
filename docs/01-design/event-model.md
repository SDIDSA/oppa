# Event model

Status: current as design (locked #7); routing **implemented** (M5 —
core-side hit-test + capture/focus router over committed boxes; see
`04-planning/state.md` §5k). Sources: `12-archive/DESIGN.md` §§2.2–2.3,
9.3; `04-planning/state.md` §5b.1.

One normalized `InputEvent` enum everywhere (locked #7 —
`crates/oppa/src/input.rs`):

```rust
Pointer(id, action /* Down/Move/Up/Cancel */, position, modifiers)
Scroll { target, delta }   // routed to the target's Scroll handler; physics M8
Key(code, modifiers, state, repeat)
Ime { target }             // routed to the target's Ime handler; sessions M2-scope
Focus(change)
```

Dispatch is hit-testing against the retained tree on GPU backends, and
browser-event → enum mapping on Web (the framework still owns logical
routing). Framework-provided reactive flags (`ctx.hovered/pressed/
focused`, `ctx.scroll_offset`) are derived once from this stream — this
is what fixes stuck-pressed-on-cancel per control without per-control
behavior classes (proven load-bearing-tested: press/cancel/leave/
release-outside leaves no stuck state, no dispatch).

Implementation: `Event { kind, handler }` stays as the registry-
dispatch seam underneath (handlers stay ids, ADR-0007); the M5 router
hit-tests to a `NodeId`, resolves the press owner (self-or-nearest
Press-handler ancestor), writes the owner's instance flags, and
dispatches through the existing registry. The 1:1 `Cmd` payload queue
in `oppa-shell-win` (interpretation decision 27) is unchanged; the
Win32→`InputEvent` mapping is platform-track follow-up (M6).
Deterministic Tab order = press-handler nodes in depth-first
pre-order. Measured: input→visual within one frame.

Web scroll corollary (locked #23): native browser scrolling; the
`offset` signal is fed from scroll events at the INPUT boundary and may
trail the browser by ≤ 1 frame (overscan +4, `overflow-anchor: none`
compensate); hit-testing needs no offset on Web.

See also: [input architecture](../02-architecture/input/overview.md),
[spec: pointer](../03-spec/input/pointer.md),
[spec: focus](../03-spec/input/focus.md).
