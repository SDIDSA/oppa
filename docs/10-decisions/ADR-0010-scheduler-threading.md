# ADR-0010: Phase scheduler, single UI thread, per-surface present

Status: Accepted (R5; locked #18–#21). Source: `12-archive/DESIGN.md` §9.1.

## Context

Locked #15 named phases, not a scheduler; every subsystem's
correctness argument needed reactive semantics.

## Decision

Frame loop `TIME → INPUT → RELOAD → EFFECTS → LAYOUT →
PAINT/COMMIT → A11Y`, on-demand. Topological propagation, one run
per node per pass, 3-pass budget (debug cycle-assert, release
defer-then-park). Structural `PartialEq` memo gate; lazy memos with
pull-recompute; no user code mid-LAYOUT/PAINT; one-frame-delayed
layout feedback. Single UI thread (`!Send` signals);
framework-owned workers with INPUT-drained generation-tagged queue.
Global state boundary; per-surface atomic commit/present; skew ≤ 1
frame.

## Alternatives

Subtree-parallel layout, shared-memory textures — deferred as
measured-v2-only options behind unchanged phases.

## Consequences

Costs named: no parallel layout/reconcile; mailbox textures;
cancel-at-reload task restarts. Revisit tripwires are measured
evidence only. Memos-never-write amended to panic in all profiles
(locked #26 — see ADR-0013).
