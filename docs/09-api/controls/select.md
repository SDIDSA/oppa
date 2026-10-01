# Select

Status: current (decision 247). Source: `crates/oppa-controls/src/lib.rs`
(`Select`, `SelectProps`, `SelectItem`).

Dropdown picker over the core vocabulary: a 32-high box (current
selection + chevron) with the option list in an under-box anchored
portal that shifts nothing (decision 296). Both halves are
author-owned signals.

```rust
let theme = ctx.signal(Theme::System);
let open = ctx.signal(false);
ctx.child("app::Theme", 2,
    &SelectProps::new(
        vec![
            SelectItem { value: Theme::Light, label: SharedString::from("Light") },
            SelectItem { value: Theme::Dark, label: SharedString::from("Dark") },
        ],
        theme.clone(),
        open.clone(),
    ),
    Select::<Theme>)
```

- Controlled `selected: Signal<T>` + `open: Signal<bool>`; `width`
  and `enabled` are pub fields (width defaults to 160 — decision
  213 explicit sizes).
- Generic over `T: Clone + PartialEq + 'static` (decision 311);
  render as `Select::<Theme>`.
- Semantics: `combobox` role with the selection as its label;
  options ride `listitem` + `selected` (decision 247).
- Tab contents are factory closures sharing the instance namespace
  — child `(name, key)` pairs must stay unique across all options
  (the Tabs precedent, decision 245).

Related: [controls overview](overview.md).
