# Resources subsystem — overview

Status: current as contract/stub; full pipeline planned.
Sources: `12-archive/DESIGN.md` §§2.2, 8.9, 9.6; code:
`crates/oppa/src/component.rs` (`ImageCache`), `crates/oppa/src/worker.rs`.

- **Owns:** content-addressed `image_cache` (core-side service
  alongside `TextService`), decode workers, generation-independent
  in-flight decodes (images are content, not hot code — only the
  await is generation-scoped; re-runs re-request).
- **Current:** `ImageCache::load` stub behind the content-addressed
  shape (M2 delta D6 — stated deferral, not a silent cut).
- **Planned (post-scoping):** async decode + `ExternalTexture`
  latest-wins mailbox pipeline. Deliberately unscoped in v1
  (`12-archive/BUILD-ORDER.md` §5.7): ship static pre-decoded images first so
  worker-queue and generation discipline are not designed against a
  moving target.
- **Threading:** producers hand frames through the mailbox into the
  INPUT-drained queue — never shared memory into the scene graph.
  Revisit tripwire: a producer that cannot sustain frame rate through
  the mailbox reopens the threading lock on measured evidence.
