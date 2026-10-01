# Radio / RadioGroup

Status: current (decision 244). Source: `crates/oppa-controls/src/lib.rs`
(`Radio`, `RadioProps`, `RadioGroup`, `RadioGroupProps`, `RadioOption`).

Single-choice group over the core vocabulary: 18x18 circle
indicator (8x8 centered dot when selected) + label per option.
Controlled `selected`; clicking an option sets it, so exactly one
option reads selected. Generic over `T: Clone + PartialEq + 'static`.

```rust
let plan = ctx.signal(Plan::Free);
ctx.child("app::Plan", 2,
    &RadioGroupProps {
        options: vec![
            RadioOption { value: Plan::Free, label: SharedString::from("Free") },
            RadioOption { value: Plan::Pro, label: SharedString::from("Pro") },
        ],
        selected: plan.clone(),
        enabled: true,
    },
    RadioGroup::<Plan>)
```

- Standalone `Radio` takes `{ label, selected: bool, enabled,
  on_select }` (the group wires `on_select` to its signal).
- Semantics: `radio` role + live `selected` (ARIA `radio` +
  `aria-checked`; UIA SelectionItem like `ListItem`).
- Disabled is structurally handler-less (decision 213).

Related: [controls overview](overview.md).
