# ADR-0013: Core-side state residence; release-mode memo panic

Status: Accepted (R5 addendum locked #25; M0 review locked #26).
Sources: `12-archive/DESIGN.md` §§9.1, 9.6; `04-planning/state.md` §§5–6 (decisions 3, 59).

## Context

Opaque props covered props only — `Store<T>`, `image_cache`, and
in-flight tasks had no residence rules; and the memo-write rule
said debug-assert while the implementation panicked everywhere.

## Decision

(1) Anything surviving a swap lives core-side in reactive storage
(`Store<T>`, `image_cache`, signals, `keyed_state`); `ctx.spawn`
futures run on the framework executor under the handler capture
rule, generation-tagged, cancelled at RELOAD with retired-queue
results discarded. Crate-level lint rejects hot-crate ambient
state. (2) A memo writing a signal or creating an effect panics in
release too — a silently-dropped write corrupts state invisibly;
the crash message is the diagnostic.

## Alternatives

Silent drop / debug-only assert — rejected (field-invisible
corruption is worse than a crash). Tasks surviving reloads
(re-binding async bodies) — v2.

## Consequences

Component-level divergence stays under #19 (loud-log + defer +
park — no new lock, decision 59). Reload restarts in-flight work;
the pattern is incremental signal writes at await points. Fuzzer
covers the task/message path from M2b.
