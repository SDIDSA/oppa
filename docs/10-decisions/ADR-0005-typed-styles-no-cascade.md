# ADR-0005: Typed interned styles, no CSS cascade

Status: Accepted (R1; locked #8). Source: `12-archive/DESIGN.md` §2.2.

## Context

A cascade brings specificity, merge semantics, and a second
restyling representation to keep in sync with component structure.

## Decision

Typed style structs (`layout/paint/behavior`) + interned `StyleId` +
token-table themes; small explicit inheritance set (text style,
direction).

## Alternatives

CSS cascade — rejected. Template replacement (XAML-style) —
rejected in favor of token-table swap (restyle) + slots (structure).

## Consequences

Theme flip = `StyleId` change per node (cheap); template drift
impossible (the function body is the template); interning shares
payloads across thousands of nodes.
