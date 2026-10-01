# Goals

Status: current. Source: `12-archive/DESIGN.md` §1, locked #1.

Priorities, settled and in order:

1. **Rendering performance** — frame pacing, damage discipline, O(dirty)
   reactive work.
2. **Developer ergonomics** — fine-grained reactivity with plain-function
   components; one change-propagation mechanism instead of six.
3. **Binary size** — no language runtime; pure-Rust core.
4. **Hot reload / iteration speed** — sub-second body edits on desktop,
   wasm swap on Web, restart-only on Android in v1.

Resolution rule (locked): perf #1 vs. size #3 resolves as **buy the
rasterizer, own everything above it** — writing a vector GPU rasterizer
(tessellation, AA, glyph atlas, blend modes) is a multi-year effort and a
v2 decision; v1 embeds an existing rasterizer behind our own display
list (`12-archive/DESIGN.md` §6).
