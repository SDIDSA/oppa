# Concurrency

Status: current (M0 queue + M2 generation tags + M2b executor).
Sources: `12-archive/DESIGN.md` §§9.1, 9.6; locked #20, #25;
`04-planning/state.md` §5h.

- Single UI thread owns the entire reactive pipeline; reactive
  types are `!Send` — load-bearing (lock-free handler capture, no
  slot synchronization).
- Workers are framework-owned (image decode, glyph atlas, wgpu
  submission, async executor). Handoff rule: results enter only via
  the queue drained at INPUT; a worker never touches the reactive
  graph. Pump callbacks may write signals (stated invariant since
  decision 36).
- `WorkerQueue`: generation-tagged results; retired-generation
  results discarded at drain (counted in stats).
- Task executor (M2b): one thread per runtime; `spawn_task` takes
  `Send`-only bodies, `TaskScope::submit` feeds the generation-tagged
  INPUT drain; `drop_pending_tasks` at RELOAD; running tasks finish
  but their submits discard by tag. Cross-thread submit proven by
  the task tests + fuzzer exactly-once accounting.
- `ExternalTexture` via latest-wins mailbox, not shared memory.
- No parallel layout/reconcile (named cost).

Revisit tripwire: serial core work exceeding ~⅓ of the frame budget
at 60 Hz on mid-tier hardware, or a mailbox-starved texture
producer — measured evidence only. `!Send` ergonomic friction is
never a trigger.
