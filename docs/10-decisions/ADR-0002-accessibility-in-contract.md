# ADR-0002: Accessibility tree in the renderer contract from day one

Status: Accepted (R1; locked #3). Source: `12-archive/DESIGN.md` §1.

## Context

Retrofitting accessibility is the single most expensive mistake in
UI frameworks (Flutter cited as the warning).

## Decision

The accessibility/semantic tree is computed from the UI model and
emitted as part of the renderer contract from day one; semantics
live in retained nodes and diff like style.

## Alternatives

Deferring a11y to a later milestone — rejected as an architecture
constraint, not a feature call.

## Consequences

`SemanticsDiff` flows through `PlatformShell` from the first
runnable (M4 dump); text-edit a11y payload is a v1 contract
requirement; per-platform emitters (UIA, AT-SPI, ARIA) are
independent follow-ups.
