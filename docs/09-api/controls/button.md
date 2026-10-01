# Button

Status: current (G2, decisions 212–213). Source: `crates/oppa-controls/src/lib.rs` (`Button`, `ButtonProps`).

Push-button over the core vocabulary (no new `Tag`): label text,
`button` role, optional press handler. Activation is pointer press
or Enter/Space on the focused control (the M5 router). Chrome is a
radius-6 `Div` with a centered label (Round 7.10).

```rust
let props = ButtonProps::new("OK", || save());
let vnode = ctx.child("oppa::Button", 1, &props, oppa_controls::Button);
```

- Stateless: activation rides `on_press` (`Action = Rc<dyn Fn()>`,
  props stay `Clone + 'static` for the hot boundary).
- Disabled (`.disabled()` or `enabled: false`): structurally
  handler-less — carries `disabled(true)` semantics, dims, and leaves
  the Tab order (v1 focusable means press-owner, decision 96).
- Default size 96×32 (`.size(w, h)` overrides); `debug` label
  defaults to `"button"`.

Related: [controls overview](overview.md).
