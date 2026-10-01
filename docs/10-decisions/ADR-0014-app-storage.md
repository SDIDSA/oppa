# ADR-0014: App storage seams — sync KV + sandboxed files

Status: Accepted (V3 G5; decisions 216–217).
Sources: `crates/oppa/src/store.rs`; `04-planning/rounds.md` V3-G5.

## Context

No fs/http/kv in core and no such deps workspace-wide (HANDOFF-V2
§4 G5): every app hand-rolls settings/cache/sync with no seam, and
wasm's async-only storage had no answer. Network fetch is
explicitly out of this decision (OQ-G5-5).

## Decision

Two sync-first traits in core (`oppa::store`, std-only):

- `KvStore` — bytes under flat non-empty keys
  (`get/set/remove/clear`; missing reads are `Ok(None)`).
- `FsSandbox` — `read/write/remove/exists/list` under a lexical
  jail (relative, `..`-free; violations are `InvalidKey`).
- `StoreError::{Unsupported, NotFound, InvalidKey, Backend}` —
  every refusal named.
- References: `InMemoryKv`, `InMemoryFs` (tests/headless),
  `NativeFs` (`std::fs` under an auto-created root, parents on
  write, `NotFound` mapped from the OS).

Sync-first is deliberate (decision 216): unlike clipboard reads
(async on web — hence request/poll, ADR-agnostic decision 209),
storage has a sync option on every platform (`localStorage` on
web, `std::fs` on native), so no poll machinery is repeated.

## Alternatives

Async-native traits (or request/poll for storage): rejected —
unsync-shaped APIs would force every native caller through a pump
for operations the OS answers immediately. Async-only backends
(IndexedDB, OPFS) arrive later through a bridge (OQ-G5-3).

`localStorage`-shaped string-only KV: rejected — bytes core-side,
backends encode (the wasm backend UTF-8-checks loudly when it
lands).

## Consequences

- Per-platform roots named, not built: Android app-private
  (needs JNI `filesDir` — OQ), Windows `%APPDATA%`/exe dir, Linux
  `~/.local/share`, web `localStorage` (sync, ~5 MB, string
  encoding at the backend) — shell `app_data_dir()` exposure is
  OQ-G5-2.
- Known bound: the jail is lexical — symlinks inside the sandbox
  can point out (OQ-G5-4). No locking/watching (stated, not
  smuggled).
- Fetch stays its own decision (OQ-G5-5: CORS/mixed-content make
  the web leg diverge more than KV/FS ever do).
