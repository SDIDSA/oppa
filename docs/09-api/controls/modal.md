# Modal

Status: current (decisions 241, 255, 296). Source: `crates/oppa-controls/src/lib.rs`
(`Modal`, `ModalProps`).

Modal dialog: dimmed full-viewport backdrop + centered card
(title + end-aligned Cancel/Confirm). Controlled `open`; closing
unmounts the portal (focus/captures inside clear, global tab order
restores).

```rust
let open = ctx.signal(false);
ctx.child("app::Confirm", 8,
    &ModalProps::new("Delete file?", open.clone())
        .on_confirm(move || confirmed.set(true)),
    Modal)
```

- `ModalProps::new(title, open)` — 360-wide card default (`.width(..)`
  overrides); `.on_confirm(..)` / `.on_cancel(..)`; backdrop dismiss
  on by default, `.no_backdrop_dismiss()` leaves the scrim
  visual-only (never a silent no-op handler — decision 213).
- Open captures every press (portal hit priority — decision 255);
  `Tab`/`Shift+Tab` cycle strictly inside the card (focus trap,
  decision 334); `focus_visible` modality paints inset rings.
- Semantics: `dialog` role + title label. DOM inherits via
  inset-ring CSS; CPU/Vello share the plan.
- Dirty-gated close (unsaved-changes veto) wires
  `DesktopLoop::set_close_handler` to raise the modal —
  [cookbook](../cookbook.md) §7, Task Studio precedent.

Related: [controls overview](overview.md).
