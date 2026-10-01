# App test harness (G15)

Status: current (decision 234). Source:
`crates/oppa-testkit/src/lib.rs` (`Harness`).

Headless pump + assert for app tests — the rig every framework
test reinvents, productized. Add to dev-dependencies and drive:

```rust
let app = Harness::new();
app.mount("MyScreen", props, render);
app.tap("ok-button");
assert!(toggled.get());
```

- `Harness::new()` (800×600, system clock) /
  `Harness::with_clock()` (owned `MockClock` + `advance(secs)`
  for deterministic holds and transitions).
- `mount` (pumps to idle), `run_idle` / `run_once`, `tap` /
  `press_down`, `node` / `center` (loud on missing/unboxed),
  `host()` escape hatch (exotic events, router reads, reloads).
- Composes public API only — no test-only backdoors, so a green
  harness test proves what the app can actually do.

Related: [editing contract](editing-contract.md) (the shared
cross-backend suite — the other half of the app-test story).
