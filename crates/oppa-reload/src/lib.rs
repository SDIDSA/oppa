//! Hot-reload harness (M2b §5.1/§5.3, locks #14/#25).
//!
//! The swap protocol, in order (all inside the RELOAD phase —
//! single-threaded, so no dispatch ever sees a half-swapped registry):
//!
//! 1. **Drain** live props through the *outgoing* manifest's typed drain
//!    glue (hot code, pre-swap — sound). Type names are copied to owned
//!    `String`s; no hot vtable or dylib rodata is touched past the swap.
//! 2. **Cancel** pending tasks of the outgoing generation
//!    ([`Runtime::drop_pending_tasks`](oppa::reactive::Runtime::drop_pending_tasks)).
//! 3. **Advance** the hot generation and **retire** the old image (M2b
//!    never unloads — see `HotRegistry`'s `retired` docs for why
//!    unloading would dangle hot-vtabled values).
//! 4. **Load + rescan**: take ownership of the new library, read its
//!    `oppa_component_manifest()` table, and re-point the host at the
//!    incoming code (runs resolve by symbol from here on).
//! 5. **Adopt** each drained payload through the *incoming* manifest's
//!    glue (type-name check; mismatch ⇒ loud eviction, payload leaks
//!    boundedly by design — its type is unknown, so it cannot be freed).
//! 6. **Evict** failures (effect retired — no post-swap run can touch
//!    missing props or stale code), **assert** no outgoing-generation
//!    props survive (debug), **re-run** every component (re-runs *are*
//!    the handler re-registration).
//!
//! [`StaticSource`] runs the same protocol in-process (no dylib) — the
//! headless harness the fuzzer and most tests drive. [`DylibSource`]
//! performs real `LoadLibrary` swaps (retired images stay mapped —
//! see above).
//!
//! ## Recipe: reload your hello-desktop (G9)
//!
//! 1. **Split the app.** Move component bodies into a hot crate
//!    (a `cdylib` + a normal lib target); the host crate keeps
//!    `main`, the `ComponentHost`, and the runner. Hot crates hold
//!    no ambient state (the `#[hot_crate]` lint refuses `static`s —
//!    state lives in signals/props, which is exactly what survives
//!    the swap).
//!
//! 2. **Export the manifest.** `component_manifest![export,
//!    Counter(CounterProps)]` emits `oppa_component_manifest` —
//!    the symbol [`DylibSource`] looks up. Monomorphized generics
//!    spell `Name::<A>(P<A>)` (one entry per instantiation).
//!
//! 3. **Drive the loop.** Boot the host, `HotRegistry::install`
//!    the manifest source, run frames; on rebuild, swap sources —
//!    the six steps above run inside the RELOAD phase. The
//!    `app_loop` example is this loop runnable
//!    (`cargo run -p oppa-reload --example app_loop`).
//!
//! 4. **Keep the rules.** Same symbols across versions (rename =
//!    evict + reseed, never migrate); same props layouts (a
//!    changed layout evicts with `TypeMismatch`, loudly); tasks
//!    re-submit under the new generation (in-flight bodies finish,
//!    their submits are discarded by tag).

pub mod registry;
pub mod source;

pub use registry::{EvictReason, Evicted, HotRegistry, ReloadReport};
pub use source::{ComponentSource, DylibSource, StaticSource};
