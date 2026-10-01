# Architecture (current)

Status: current. Last verified: 2026-10-01.
History lives in git, not here: this file describes `HEAD`. Rounds overwrite it in place.

## Pipeline

```text
signals/state → components → VNode (ephemeral) → RetainedNode tree
  → layout → display lists + damage → renderer commits
```

One frame runs `TIME → INPUT → RELOAD → EFFECTS → LAYOUT → PAINT/COMMIT → A11Y`
on a single UI thread. Workers hand results back through the `INPUT` queue.

## Layers (code truth)

| Layer | Code | Owns |
|---|---|---|
| Reactive core + scheduler + storage | `crates/oppa/src/reactive/`, `arena.rs`, `worker.rs`, `clock.rs`, `handlers.rs`, `store.rs` | signals/memos/effects, topo-by-depth propagation (≤1 run/node/pass, 3-pass budget), timer registry, generational-arena storage |
| Components + reconciler | `component.rs`, `vnode.rs`, `reconciler.rs`, `style.rs`, `semantics.rs`, `interner.rs`, `pass_mask.rs`, `hash.rs` | VNode→retained diff → `TreeDiff`, typed interned styles, `SemanticsDiff`, handler-id registry |
| Layout (M3, current) | `crates/oppa/src/layout.rs` | measure/position/`LayoutBox` per node; v1 = flexbox subset + block-lite + absolute; DPR rounding at commit |
| Input + events (M5, current) | `crates/oppa/src/input.rs`, host router in `component.rs` | one normalized `InputEvent` enum, hit-test walk, pressed/hovered/focused state, Tab order |
| Text | `crates/oppa/src/text.rs`, `ime.rs` + `crates/oppa-text-dwrite/` (Windows), `crates/oppa-text-rustybuzz/`, `crates/oppa-text-linux/`, `crates/oppa-text-android/` | `TextService` shaping/measure/fallback, cluster caret + hit-test, editing sessions, IME normalization |
| Rendering contract + backends | `crates/oppa/src/render.rs` + `crates/oppa-cpu/` (M4, current), `crates/oppa-vello/` (M6, current), `crates/oppa-dom/` (M7, current) | `RendererBackend` / `FramePlan` / `Caps` / `DrawOp`; FramePlan builder + headless oracle in CPU backend |
| Shells + app loop | `crates/oppa/src/shell.rs` + `crates/oppa-shell-win/` (current), `crates/oppa-shell-linux/` (current), `crates/oppa-shell-android/` + `crates/oppa-android-app/` (current), `crates/oppa-app/` (`DesktopLoop`, current), `crates/oppa-web/` (`WasmHost`, current) | surface/lifecycle, event pump, IME/DPI/cursor, present |
| Macros | `crates/oppa-macros/` | `#[derive(Props)]`, `#[component]`, `component_manifest!` |
| Controls | `crates/oppa-controls/` | 15 controlled components (button … menu/tooltip/datagrid, see `docs/STATE.md` Done) |
| Hot reload | `crates/oppa-reload/` | manifest-scan dylib harness; retire-don't-unload (true unload is v2 scope) |
| Test harness | `crates/oppa-testkit/` | headless `Harness` + `DesktopLoop::type_text`, key/press helpers |
| Support | `crates/oppa-image/`, `crates/oppa-fonts/`, `crates/oppa-linebreak/`, `crates/oppa-fps/` | image cache, font lookup, line breaking, fps demo |

## Seams (what is throwaway per-platform)

Everything behind `RendererBackend` + `PlatformShell` + `TextService`
(`crates/oppa/src/shell.rs`, `text.rs`) is per-platform and replaceable.
Everything above is written once. Renderers never compute layout. Backends hold
backend-mechanism state (DOM nodes, glyph atlases, layer caches) but no
application/UI-model state.

## Backends and shells

- Desktop GPU: Vello (`oppa-vello`), current. CPU fallback: tiny-skia (`oppa-cpu`), current. Skia hatch: costed, unbuilt (v2 decision).
- Web: DOM backend (`oppa-dom`), current — browser owns pixels, scroll physics, editable-text mechanics; framework owns the scene mapping.
- Shells: Windows, Linux, Android all current. Android hot reload is restart-only (~2–10 s) by decision. No macOS/iOS targets by design.
- Per-platform status and limits: `docs/06-platforms/`.
