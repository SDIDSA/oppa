# Async fetch to render (G7)

Status: current (decisions 220–221). Sources:
`crates/oppa/src/fetch.rs`, `Ctx::fetch_state` / `Ctx::spawn_fetch`.

One state shape, two drivers. Components hold remote data in a
keyed `FetchState<T>` signal (`Idle / Loading / Ready(T) /
Failed(String)`) and render it with a plain `match`:

```rust
let key = ctx.fetch_key("settings:avatar");
let avatar = ctx.fetch_state::<String>(key);
ctx.spawn_fetch(key, || download_avatar()); // native driver
```

- Native: `spawn_fetch` sets `Loading` synchronously, runs the
  `Send` closure on the executor thread, and submits the result
  through the `keyed_state` rendezvous (signals never cross threads;
  generation tags discard post-swap landings — §9.6).
- Web: no threads — `spawn_fetch` refuses loudly. The platform
  binding resolves the promise, writes the same keyed signal from
  the UI thread, and requests a frame.
- Keys are `fetch_key("route:name")` strings hashed to u64 (one
  global namespace per runtime — readable names, no silent
  collisions).

Not here: fetch backends (OQ-G5-5), cancellation tokens (OQ-G7-2),
progress (OQ-G7-3), reload-harness examples (G14).
