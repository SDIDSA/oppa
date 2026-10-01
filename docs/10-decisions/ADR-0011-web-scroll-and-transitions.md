# ADR-0011: Native web scroll; binding-edge transition stamp

Status: Accepted (R5; locked #22–#23). Source: `12-archive/DESIGN.md` §§9.3–9.4.

## Context

Two stress-test findings: synthesizing Web scroll from wasm
re-buys the platform behavior the DOM lock inherits; slot-keyed
recycling phantom-animates every rebound cell under transitions.

## Decision

(1) Web scroll is native browser scrolling (`ScrollArea` =
overflow container + spacer); the `offset` signal is INPUT-fed and
may trail ≤ 1 frame; `ctx.scroll_offset()` keeps identical
semantics on all backends. (2) Re-run deltas triggered through a
binding edge (`ctx.binding`) carry `suppress_transitions` for
exactly one commit — values jump, no interpolator.

## Alternatives

Synthesized scrolling — rejected (perf #1 deciding by routing).
Per-value transition provenance — deferred unless measured.

## Consequences

"Physics normalized away" scoped to GPU; Web feel is CSS-tunable
with documented caveats; overscan +4 and `overflow-anchor: none`
compensate lag. Coincident real changes under a rebind stamp are
suppressed too (accepted v1 limit). Proven as data in M2; honored
by the M8 evaluator; stressed per-frame on both backends.
