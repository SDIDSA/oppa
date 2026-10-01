# ErrorBoundary

Status: current (decision 320). Source: `crates/oppa-controls/src/lib.rs`
(`ErrorBoundary`, `ErrorBoundaryProps`, `ErrorFallback`, `ErrorListener`).

Failure isolation: wraps a child render so a child panic renders a
fallback card instead of crashing the host; retry re-evaluates the
child. Wrap panes over untrusted data (inspectors, plugin
content).

```rust
ctx.child("app::InspectorGuard", 50,
    &ErrorBoundaryProps::new(|ctx| inspector(ctx, &props)),
    ErrorBoundary)
```

- `ErrorBoundaryProps::new(child)` — child is `Rc<dyn Fn(&Ctx) ->
  VNode>`; `.fallback(|ctx, error_msg, reset| ...)` renders custom
  UI (`reset` retries); `.on_error(|msg| ...)` observes
  (`ErrorListener = Rc<dyn Fn(&str)>`).
- The default fallback card carries an `"error-boundary-retry"`
  button. The unwind resets `input_owner` hygiene; cleanups
  (`ctx.on_cleanup`) still run, so timers and subscriptions never
  leak through a caught panic (decision 321).

Related: [controls overview](overview.md), [cookbook](../cookbook.md) §9.
