# Toggle

Status: current (G2, decisions 212–213). Source: `crates/oppa-controls/src/lib.rs` (`Toggle`, `ToggleProps`).

The M5 §4.1 pattern as a shipped control: `switch` role with live
`checked`, press flips the author-owned signal. Visual is a pill
track + sliding knob with a visible label (Round 7.9), not
on/off text.

```rust
let wifi = ctx.signal(false);
let props = ToggleProps { label: "Wi-Fi".into(), on: wifi, enabled: true };
let vnode = ctx.child("oppa::Toggle", 1, &props, oppa_controls::Toggle);
```

- Disabled: no flip, `disabled(true)` semantics, handler-less.
- Screen readers: `role="switch"` + `aria-checked` (DOM),
  `toggle button` + `checkable` (AT-SPI), CheckBox type + Toggle
  pattern (UIA) — the M5-verified payload, unchanged.

Related: [controls overview](overview.md).
