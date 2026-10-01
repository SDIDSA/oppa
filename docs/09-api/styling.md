# Styling API

Status: proposed. Sources: `12-archive/DESIGN.md` §§2.2, 4.1;
[styling-model](../01-design/styling-model.md).

```rust
Style::new()
    .size(44, 24).radius(12).bg(track)
    .opacity(props.enabled.then_some(1.0))
    .transition(Transition::new(120.ms(), Ease::Out))
```

Themes are token tables (`props.theme`) resolved to styles — swap
tables, not stylesheets. A small explicit set inherits (text style,
direction). Restyle = `StyleId` change per node (cheap). Structural
variants compose via slots (`props.leading: Option<Component>`),
not template replacement.
