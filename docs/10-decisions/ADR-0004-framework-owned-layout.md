# ADR-0004: Framework-owned layout engine

Status: Accepted (R1; locked #6). Source: `12-archive/DESIGN.md` §2.3.

## Context

Layout placement determines whether backends stay thin and whether
results are stable across reloads and a11y bounds.

## Decision

One layout engine, written once, producing `LayoutBox` per node.
Renderers never compute layout. Layout of text runs belongs to the
model layer — display lists carry pre-shaped, pre-positioned runs.

## Alternatives

Renderer-side layout — rejected (N implementations of the hardest
code; Web split-brain; unstable boxes).

## Consequences

v1 scope flexbox subset + block-lite (+absolute); Web bound to flat
flexbox/text-in-flex; text measurement via `TextService`; engine
lands at M3.
