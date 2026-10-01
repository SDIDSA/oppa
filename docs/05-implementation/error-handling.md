# Error handling

Status: current. Sources: `04-planning/state.md` §§3–4; locked #26.

Project rule: **loud failures, never silent corruption.**

- Generational access: `get`/`get_mut` panic with the full refusing
  reason on retired/stale/OOB; `try_get`/`try_get_mut` return
  `SlotError`. Post-eviction `keyed_state` access fails loudly, never
  aliases new state (decision 51).
- Unknown font family → loud `FontNotFound` (DirectWrite would
  silently substitute); empty text → `EmptyText`.
- Memo writes a signal or creates an effect → panic in **all**
  profiles (locked #26: a dropped write corrupts state invisibly in
  the field; the crash message is the diagnostic).
- Propagation past budget → debug panic with cycle path;
  release rate-limited log + defer + park (locked #19).
- Unresolved `HandlerId` dispatch panics loudly; out-of-LIFO batch
  drops panic; nested memo re-entry panics.
- TSF store posture: strict mutations (`TS_E_NOLOCK` without a held
  lock, logged), lenient reads, `E_NOTIMPL` geometry (decision 39).
