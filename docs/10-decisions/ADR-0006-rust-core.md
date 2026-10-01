# ADR-0006: Rust for core and components

Status: Accepted (R2; locked #10). Source: `12-archive/DESIGN.md` §3.

## Context

Decision #0: language/runtime, judged against perf, ergonomics,
binary size, hot reload, and the four-target story.

## Decision

Rust for the core (scheduler, reconciler, layout, contracts);
components also authored in Rust, hot-swapped as dylibs.

## Alternatives

C++ (never wins under any weighting — worst ergonomics, per-platform
build systems); Swift (ARC traffic, bundled runtime on Win/Linux,
Apple-anchored toolchain); Kotlin (JVM JIT+GC violates #1;
Native slower GC + multi-MB; FFI lock-in for non-Kotlin consumers).

## Consequences

#1/#3 bought outright; only credible four-target story (incl. wasm);
ownership forces the id-based indirection hot reload needs anyway;
costs accepted as design constraints (no tree references in user
code, signals-not-state capture, never Dart-grade reload, compile
times via crate splitting + mold/lld).
