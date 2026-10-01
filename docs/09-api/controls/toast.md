# Toast

Status: current (decision 337). Source: `crates/oppa-controls/src/lib.rs`
(`Toast`, `ToastProps`, `ToastVariant`).

Transient non-modal feedback: a bottom-center card (variant dot +
message + Dismiss) over a handler-less full-viewport anchor. The
inverse of Modal — a toast never blocks input and never takes
focus. Controlled `open`.

```rust
let saved = ctx.signal(false);
ctx.child("app::SavedToast", 40,
    &ToastProps::new("Saved", saved.clone()).variant(ToastVariant::Success),
    Toast)
// ... after a successful save: saved.set(true)
```

- `ToastProps::new(message, open)` — 4 s wall-clock auto-dismiss by
  default (the `use_timeout` hook; re-renders re-arm, cleanup
  cancels); `.auto_dismiss_ms(..)` overrides, `.sticky()` keeps it
  until dismissed.
- Variants tint the leading dot only (`Info` = theme primary,
  `Success`/`Error` = fixed decorative literals per the 323
  precedent); the card rides theme surface + ring like Modal.
- Semantics: `status` role + message label (ARIA `role="status"`,
  AT-SPI `notification`, UIA StatusBar) — announced, never focused.
- Closed renders an explicit 0x0 portal (the Modal precedent —
  zero gap in parent flex layouts).

Related: [controls overview](overview.md).
