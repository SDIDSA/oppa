# ADR-0009: Vello GPU rasterizer with CPU fallback

Status: Accepted (R4; locked #17; attribution corrected R5, decision
unchanged). Sources: `12-archive/DESIGN.md` §§6, 9.5.

## Context

Writing a vector GPU rasterizer is a multi-year effort (v2); v1
embeds an existing rasterizer behind our display list.

## Decision

Vello on desktop GPU in v1; tiny-skia CPU backend as the
Caps-negotiated fallback; custom GPU pipeline v2; Skia is the
documented escape hatch (~2–4 weeks per backend), never a core
change.

## Alternatives

Skia via `skia-safe` — rejected for v1: C++ FFI in a pure-Rust core,
heavy per-target C++ CI, ~2–6 MB desktop, CanvasKit-class wasm for a
DOM platform. "Skia v1 → Vello v2" phasing — rejected (two backend
migrations + contaminated CI from day one).

## Consequences

Decided by #2 + #3 (ergonomics, size), not #1 — stated plainly:
pure-Rust toolchain and smaller binaries over hostile-GPU
robustness. Blur/backdrop Caps-gated; driver matrix + glyph review
at M6; §8.7 watch items re-test the bet; mobile fallback
unvalidated until M10.
