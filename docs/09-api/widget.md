# Widget API

Status: current (M2 authoring shapes + Round 14.1 generic Props;
corrected against the compiler by the out-of-repo usability app —
sketches that didn't compile were fixed here, not in the app).

```rust
#[derive(Clone, Props)] // generics welcome: lifetimes, Clone types, consts (14.1)
struct ToggleProps { label: SharedString, initial: bool }

#[component] // pass-through + call-site lint; names stay PascalCase
fn Toggle(ctx: &Ctx, props: &ToggleProps) -> VNode {
    let is_on = ctx.signal(props.initial);
    Div("track")
        .style(Style::new().size(44, 24).bg(Color(0x55_55_55)))
        .semantics(Semantics::switch().checked(is_on.get()).label(&props.label))
        .on_press(move || is_on.set(!is_on.get()))
        .child(Div("knob").style(Style::new().size(18, 18)).build())
}
```

Shapes that compile (each of these cost an outsider a build
cycle before it was written here):

- `Div/Row/Stack/ScrollArea` are constructor **functions**;
  `Column` is `Column::new()`; `Text` is a **struct literal**
  (`Text { text: SharedString, style: Text::title_small }` —
  `SharedString` is `Arc<str>`), converted with `.into()`
  (`Text::new("Hi").size(22).bold()` builds the same literal).
- `#[derive(Props)]` handles generics (lifetimes `'static`,
  `Clone + 'static` types, consts — decision 311); generic
  components render as `RadioGroup::<Plan>` / `Tabs::<Page>`
  (the showcase precedent). `component_manifest!` still needs a
  concrete monomorph for the pointer table — generic args there
  refuse loudly until a design lands.
- `.child()` / `.children()` each take `VNode`s and **terminate
  the chain** (they return `VNode`): one of them comes last,
  with no `.build()` after. Childless chains end in `.build()`.
- Child *components* instantiate inline and return `VNode`:
  `ctx.child("name", key_u64, &props, RenderFn)` (key is `u64`,
  name is hot-reload identity).
- `Signal::update` takes `FnOnce(T) -> T`: **return** the new
  value (`todos.update(|mut v| { v[i] = x; v })`); `on_press`
  takes `Fn() + 'static` (clone signals in, never borrow).
- `host.mount("Name", props, render)` with plain `#[component]`
  fns is the whole app story. `component_manifest!` is
  hot-reload machinery (M2b), not app setup.

Full current signatures: `04-planning/state.md` §8. Usage model:
[widget-model](../01-design/widget-model.md). App tutorial:
[getting started](../05-implementation/getting-started.md).
