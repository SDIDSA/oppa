# Architecture overview

Status: current (M0–M2 implemented headless; backends planned).
Sources: `12-archive/DESIGN.md` §§2, 9; `04-planning/state.md` §2.

```text
Application (components in Rust, hot-swappable dylibs)
    ↓  signals/state → VNode (ephemeral)
UI Runtime (reactive core + scheduler + reconciler + layout)
    ↓  TreeDiff + FramePlan + SemanticsDiff
Widget Tree (retained nodes: style, layout, flags, dirty masks)
    ↓  Layout / Input / Rendering / Accessibility
Platform Abstraction (RendererBackend + PlatformShell + TextService)
    ↓
Windows / Linux / Android / Web
```

Rule: **everything inside `RendererBackend` + `PlatformShell` is
throwaway per-platform code; everything above them is written once.**
A target "exists" when it implements both traits. That boundary is what
makes the UI model renderer-agnostic.

Subsystem overviews (each states what it owns, does not own, depends
on, and where its code/tests/specs live):

- [runtime](runtime.md) · [layers](layers.md)
- [rendering](rendering/overview.md) · [layout](layout/overview.md) ·
  [input](input/overview.md) · [text](text/overview.md) ·
  [accessibility](accessibility/overview.md) ·
  [windowing](windowing/overview.md) · [resources](resources/overview.md)
- [platform abstraction](platform-abstraction/overview.md)

Implementation status (2026-09-25): reactive core, storage, scheduler,
TextService contract + DirectWrite backend, Win32 shell + TSF store,
reconciler + component model — done. Layout engine, backends, emitters
— planned (see [roadmap](../00-vision/roadmap.md)).
