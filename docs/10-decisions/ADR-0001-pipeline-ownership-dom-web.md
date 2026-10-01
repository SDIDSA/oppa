# ADR-0001: Pipeline ownership split; DOM web backend

Status: Accepted (R1; locked #1–#2). Source: `12-archive/DESIGN.md` §1.

## Context

One UI model must reach Windows, Linux, Android, and Web without N
implementations of everything above the pixels.

## Decision

We own the pipeline on Windows/Linux/Android; on Web we own the
scene schema and the browser owns the pixels. DOM is a first-class
renderer backend.

## Alternatives

Owning pixels on Web via a shipped wasm GPU rasterizer — rejected:
~1.5–2.5 MB, slow first paint, preloaded fonts, Flutter-web's
accessibility problems.

## Consequences

Web parity carve-outs accepted for v1 (flat flexbox/text-in-flex,
browser scroll physics, presenter-owned editing — locked #23, #27);
all are measured, not folklore, at M7.
