# Tabs

Status: current (decision 245). Source: `crates/oppa-controls/src/lib.rs`
(`Tabs`, `TabsProps`, `TabItem`).

Tab bar + exactly-one-visible panel. Controlled `active: Signal<T>`;
clicking a tab sets it.

```rust
let page = ctx.signal(Page::Profile);
ctx.child("app::Tabs", 9,
    &TabsProps {
        tabs: vec![
            TabItem { value: Page::Profile, label: SharedString::from("Profile"), content: profile },
            TabItem { value: Page::Settings, label: SharedString::from("Settings"), content: settings },
        ],
        active: page,
        enabled: true,
    },
    Tabs::<Page>)
```

- Tab content is an `Rc<dyn Fn(&Ctx) -> VNode>` factory closing over
  root-owned signals, so every tab reads live state (the showcase
  precedent). Factories share the Tabs instance namespace — child
  `(name, key)` pairs must stay unique across all tabs.
- Generic over `T: Clone + PartialEq + 'static` (decision 311).
- Semantics: `tablist` container, `tab` + `selected` per tab.
- Active-tab underline + shared corner-4 chrome (decisions 285–286).

Related: [controls overview](overview.md).
