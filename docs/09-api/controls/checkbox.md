# Checkbox

Status: current (G2, decisions 212–213). Source: `crates/oppa-controls/src/lib.rs` (`Checkbox`, `CheckboxProps`).

Controlled checkbox: 20×20 box + label, `checkbox` role with live
`checked`. Press flips the author-owned signal (the controlled
pattern, locked #24). The box is drawn (radius 4; Primary fill +
white "✓" when checked — Round 7.9), not a text mark.

```rust
let show = ctx.signal(false);
let props = CheckboxProps { label: "T&C".into(), checked: show, enabled: true };
let vnode = ctx.child("oppa::Checkbox", 1, &props, oppa_controls::Checkbox);
```

- Disabled: no flip, `disabled(true)` semantics, handler-less (same
  structural refusal as [Button](button.md)).
- Screen readers: `role="checkbox"` + `aria-checked` (DOM),
  `check box` + `checkable` (AT-SPI), CheckBox type + Toggle pattern
  (UIA).

Related: [controls overview](overview.md).
