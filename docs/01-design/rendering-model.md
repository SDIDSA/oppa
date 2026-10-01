# Rendering model

Status: current as design (locked #4–#5, #17); backends **Planned**
(M4/M6/M7) except the throwaway Vello debug renderer (Experimental).
Sources: `12-archive/DESIGN.md` §§2.1, 2.3, 6, 9.5.

What renderers receive: a structural `TreeDiff` (node ids +
per-node payload deltas — never components or ephemeral diffs) and, per
frame, a `FramePlan` (viewport, ordered `DrawOp`s, damage, layer plans)
built only from dirty subtrees — static UIs cost ~0 repaint CPU.
`ExternalTexture` covers embedded native content (video; on Web,
`<video>`/`<iframe>` holes).

Backend roster (locked #17; attribution corrected R5, decision
unchanged): **Vello is the v1 GPU rasterizer for desktop** (pure-Rust
toolchain, adequate-on-desktop-GPU routing); **tiny-skia CPU backend**
as the Caps-negotiated fallback for hostile targets; **Skia is the
documented escape hatch** (~2–4 weeks per backend behind the same
contract), never a core change. Damage discipline limits ops submitted,
not pixels touched (v2 refinement). Blur/backdrop filters are
partial/immature in Vello → negotiated through `Caps` with graceful
degradation.

Deciding frame, stated plainly: **pure-Rust toolchain ergonomics and
smaller binaries were chosen over rendering robustness on hostile GPUs**
(#2 + #3 decided; perf #1's force was spent by the R1 platform
carve-outs). The §8.7 watch items are where the bet is re-tested.

See also: [rendering architecture](../02-architecture/rendering/overview.md),
[ADR-0009](../10-decisions/ADR-0009-rasterizer-vello.md),
[spec: display list](../03-spec/rendering/display-list.md).
