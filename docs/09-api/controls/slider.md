# Slider

Status: current (G2, decisions 212–213). Source: `crates/oppa-controls/src/lib.rs` (`Slider`, `SliderProps`, `snap_value`, `value_text`).

Controlled slider as decrement/track/increment (press-only stepping):
two [Button](button.md) steppers (`step-dec`/`step-inc`) around a
`slider`-role track announcing `value_text` (`"50 percent"`). The
track shows a rail + Primary fill + knob (Round 7.10); since Round
5.3 the track also drags (pointer x maps to the snapped range) and
arrow keys step when focused (held keys repeat).

```rust
let vol = ctx.signal(50.0f32);
let props = SliderProps { label: "Volume".into(), value: vol, min: 0.0, max: 100.0, step: 10.0, enabled: true };
let vnode = ctx.child("oppa::Slider", 1, &props, oppa_controls::Slider);
```

- Steps snap to the grid from `min` then clamp (`snap_value` —
  panics loudly on non-positive step or `max < min`); ends are quiet
  no-ops (re-setting the same value).
- Paste-like discreteness does not apply (direct signal sets, no
  undo — undo for control state is OQ-G1-1's wider question).
- Screen readers: `role="slider"` + `aria-valuetext` (DOM);
  `slider` role only on AT-SPI/UIA (value patterns are OQ-G2-2).

Related: [controls overview](overview.md).
