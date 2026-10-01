//! Renderer contract types (DESIGN §2.2–§2.3, locked #5; BUILD-ORDER M4).
//!
//! These types are the whole scene→presenter boundary: the reconciler's
//! [`TreeDiff`](crate::reconciler::TreeDiff), the per-frame [`FramePlan`]
//! (viewport + ordered [`DrawOp`]s + damage), the [`Caps`] negotiation, and
//! the [`SemanticsDiff`] for the a11y emitters. Backends (M4 CPU first)
//! implement [`RendererBackend`] using only these types plus public retained
//! reads — if a presenter secretly needs core internals, that is an M4
//! finding, not a silent import (BUILD-ORDER §3).
//!
//! Interpretation decisions (M4; carried to `04-planning/state.md`):
//!
//! - `Color` is opaque sRGB `0xRRGGBB`. There is no alpha channel in v1;
//!   translucency arrives via the separate `opacity` fields (and the
//!   `PushLayer` op). A `bg` of [`Color::TRANSPARENT`] emits no fill op.
//!   (M5 re-record: alpha stays open, M6 Vello-blend owner — decision 98.)
//! - `Style::ink` (M5) overrides the text color; [`DrawOp::Text`] falls
//!   back to the build theme's `text_primary` when absent (theme
//!   contract round; Light's is fixed [`INK`], near-black — the M4
//!   open question's stated Style-struct field, decision 98).
//! - `Style::border` (M5) is an inset ring: the builder emits an outer
//!   fill in the border color plus an inset background fill, reusing the
//!   existing `Rect`/`RRect`/`Circle` ops (no new `DrawOp`, no backend
//!   change). Resolves the M4 open question as a stated Style-struct
//!   field (decision 98); BUILD-ORDER §3's M4 Div border stays
//!   unrendered history.
//! - Shadow blur is a v1 quantized stepped soft shadow (Round 1.3,
//!   decision 254 — supersedes the old M8-evaluator deferral): the
//!   builder expands `blur > 0` into offset solid rects with a linear
//!   alpha falloff, so the CPU backend agrees with every other backend
//!   by construction; [`Caps::blur_backdrop`] stays false (no native
//!   blur anywhere — a true Gaussian stays a follow-up).
//! - `RImg` paints registered pixels, refuses the rest (G8, decisions
//!   222-223; Vello arm closed the OQ-G8-1 half): the CPU backend paints
//!   images deposited with `insert_image` (decoded via `oppa-image`),
//!   and so does the Vello backend (straight-alpha peniko upload —
//!   the two agree pixel-exact on the oracle); unregistered ids refuse
//!   loudly rather than paint placeholders. DOM refusal stays until
//!   its arm lands.
//! - `Path` paints resolution-independent vectors (decision 291):
//!   SVG path data in the node's local space (translated by the
//!   committed box origin), solid fill and/or stroke, baked opacity.
//!   Backends parse the same data through their native engines
//!   (tiny-skia via a backend-owned parser — `tiny-skia-path` 0.12
//!   ships no `from_svg`, verified against the registry source —
//!   Vello via `kurbo::BezPath::from_svg`, DOM via inline `<svg>`);
//!   stroke caps/joins are fixed round everywhere. Invalid data
//!   refuses loudly (backend error, never a silent skip).

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::arena::NodeId;
use crate::reconciler::{Reconciler, TreeDiff};
use crate::semantics::Semantics;
use crate::style::Color;
use crate::text::FontId;
use crate::vnode::{ImageId, StrokeDesc};

/// Fixed text ink (see module docs: `Style` has no text-color field in v1).
pub const INK: Color = Color(0x11_11_11);

/// Text-selection highlight fill (Round 8.2, decision 298): the themed
/// selection rectangle painted behind selected text runs on every
/// backend (emitted as plain `Rect` ops by the shared builder, so no
/// backend carries selection logic — the Round-1.3 effects pattern).
/// Fixed until the Phase-11 theme system owns it (recorded, not silent).
pub const SELECTION_FILL: Color = Color(0xB3_D7_FF);

/// Caret bar width in device px (Round 15.1, decision 312): the shared
/// builder paints every focused caret as a vertical bar exactly this
/// wide, so CPU, Vello, and DOM agree by construction (the DOM arm
/// converts to CSS px through its own DPR, same origin-relative rule
/// as the selection divs).
pub const CARET_WIDTH_PX: f32 = 2.0;

/// One field's painted caret (Round 15.1, decision 312): the focus
/// owner's subtree (`field`) plus the caret bar's absolute box-space
/// origin (`x`, `y`, device px) and height (`h`, device px — the
/// shaper's caret height resolved against the laid line). Width is
/// always [`CARET_WIDTH_PX`] (carried by the builder, never the
/// payload). Constructed by the host from the focused session — never
/// synthesized by backends. `None` paints exactly the pre-15.1 plan.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CaretPaint {
    pub field: NodeId,
    pub x: f32,
    pub y: f32,
    pub h: f32,
    pub color: Color,
}

/// One field's painted selection (Round 8.2, decision 298): the press
/// owner's subtree (`field`) plus the session's ordered byte range.
/// Builders highlight every laid line under `field` overlapping `range`;
/// collapsed ranges paint nothing. Constructed by the host from the
/// focused session — never synthesized by backends.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SelectionPaint {
    pub field: NodeId,
    pub range: (usize, usize),
}

/// Which presenter family a backend belongs to (renderer-contract doc).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PresenterKind {
    /// Software rasterizer (M4: tiny-skia; ADR-0009 fallback).
    Cpu,
    /// GPU display-list consumer (M6: Vello).
    GpuDrawList,
    /// Browser-owned pixels (M7: DOM).
    Dom,
}

/// Capability negotiation (renderer-contract doc). The CPU backend reports
/// [`Caps::cpu_fallback`]: no blur/backdrop, no MSAA, glyph cells (not
/// paths) for text.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Caps {
    pub max_layers: u32,
    pub blur_backdrop: bool,
    pub msaa: bool,
    /// True when the backend fills real glyph outlines; false when it
    /// renders glyph cells from shaped advances (M4 CPU: false — the M6
    /// glyph-quality review starts from this documented gap).
    pub text_as_paths: bool,
}

impl Caps {
    pub fn cpu_fallback() -> Self {
        Self {
            max_layers: 8,
            blur_backdrop: false,
            msaa: false,
            text_as_paths: false,
        }
    }

    /// The DOM backend's declaration (M7, decision 111): effectively
    /// unbounded layers (stacking contexts are cheap — 1024 names "no
    /// practical limit" while staying a bounded, testable number); no
    /// native blur/backdrop (the `Shadow` op carries no blur radius —
    /// the DOM arm reads the style's blur/shadow/gradient/edge fields
    /// directly into CSS, same paint-only rule as both rasterizers);
    /// MSAA always on (browser area coverage has no toggle); real
    /// outlines (the browser shapes its own glyphs from the same
    /// family/size the plan names).
    pub fn dom() -> Self {
        Self {
            max_layers: 1024,
            blur_backdrop: false,
            msaa: true,
            text_as_paths: true,
        }
    }
}

/// Surface description (device px — layout boxes are already device px).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SurfaceDesc {
    pub width_px: u32,
    pub height_px: u32,
    pub background: Color,
}

/// Presenter-side surface handle.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SurfaceId(pub u64);

/// One positioned glyph cell: pre-shaped, pre-positioned data the
/// rasterizer consumes without re-shaping or re-laying-out (DESIGN §2.3).
/// `x` is relative to the text box origin (device px, subpixel — advances
/// stay subpixel per the coordinate-system spec).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PlacedGlyph {
    pub glyph_id: u32,
    pub x: f32,
    pub advance: f32,
}

/// One font run inside a [`DrawOp::Text`] op (M7 lock touch, decision
/// 110): the per-run font identity decision 105 deferred. `glyph_range`
/// indexes the op's flattened `glyphs` vec; `family` is the
/// backend-resolvable family name (CSS `font-family` on DOM, atlas-face
/// key on Vello); `font_id` is the shaper-local id (DWrite file
/// resolution + diagnostics). Ranges are non-empty, ordered, and
/// non-overlapping; consecutive same-font runs arrive merged.
#[derive(Clone, PartialEq, Debug)]
pub struct FontRun {
    pub glyph_range: (usize, usize),
    pub family: String,
    pub font_id: FontId,
}

/// Display-list op (display-list spec: the "immediate" half; damage lives
/// on the [`FramePlan`], not per-op).
#[derive(Clone, PartialEq, Debug)]
pub enum DrawOp {
    Rect {
        node: NodeId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
        opacity: f32,
    },
    RRect {
        node: NodeId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        /// Per-corner radii in device px, CSS order `[tl, tr, br,
        /// bl]` (Round 11.1, decision 305): `Some` exactly when the
        /// style set per-corner radii (each already resolved against
        /// the uniform shorthand and clamped to the box by the shared
        /// builder — backends take them as-is); `None` is the uniform
        /// `radius` fast path above.
        radii: Option<[f32; 4]>,
        color: Color,
        opacity: f32,
    },
    Circle {
        node: NodeId,
        cx: f32,
        cy: f32,
        r: f32,
        color: Color,
        opacity: f32,
    },
    /// Offset solid shadow (no blur — `Caps::blur_backdrop` degradation).
    Shadow {
        node: NodeId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        dx: f32,
        dy: f32,
        color: Color,
    },
    /// Pre-shaped text: one op per laid line — box origin + glyph cells
    /// (never re-shaped, never re-laid-out by the backend). `y` is the
    /// line top (device px, subpixel); each cell is `(x + g.x, y,
    /// g.advance × line_height)` in ink. `baseline` is the baseline
    /// offset from the line top (== ascent, device px, subpixel) — the
    /// GPU backend places the glyph-run origin at `y + baseline`
    /// (M6 lock touch, decision 105); the CPU backend ignores it
    /// (cells start at the line top, unchanged). `em_size` is the exact
    /// em size in device px (== `font_size_px × dpr` at measure time —
    /// ends decision 105's `font_size = line_height` scale
    /// approximation; the Vello encoder sizes runs with it). `fonts`
    /// carries the per-run font identity (ends the single-face atlas
    /// bound — the Vello atlas holds one face per distinct `font_id`).
    /// (M7 lock touch, decision 110.)
    Text {
        node: NodeId,
        /// Box origin x (device px, snapped commit position).
        x: f32,
        /// Line top y (device px, subpixel).
        y: f32,
        line_height: f32,
        /// Baseline offset from the line top (device px, subpixel).
        baseline: f32,
        /// Exact em size (device px, subpixel).
        em_size: f32,
        glyphs: Vec<PlacedGlyph>,
        /// Per-run font identity, glyph-range aligned with `glyphs`.
        fonts: Vec<FontRun>,
        ink: Color,
        opacity: f32,
    },
    /// Static pre-decoded image (pixels arrive with async decode, M8+).
    /// Backends refuse this loudly until then — never a placeholder.
    RImg {
        node: NodeId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        image: ImageId,
    },
    /// Vector shape (decision 291): `data` is SVG path data in the
    /// node's local space — the backend translates it by `(x, y)`
    /// (the committed box origin, device px) and paints it at
    /// authoring scale (1:1 with device px at DPR 1). `width` /
    /// `height` are the committed box size (hit-testing and damage,
    /// not clipping — paint may cover the stroke fringe outside the
    /// box). `fill` and/or `stroke` paint (at least one is `Some` —
    /// the builder guarantees it); `opacity` bakes like every op.
    Path {
        node: NodeId,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        data: Arc<str>,
        fill: Option<Color>,
        stroke: Option<StrokeDesc>,
        opacity: f32,
    },
    PushClip {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    PushLayer {
        opacity: f32,
    },
    Pop,
}

impl DrawOp {
    /// The retained node this op paints (None for stack ops).
    pub fn node(&self) -> Option<NodeId> {
        match self {
            DrawOp::Rect { node, .. }
            | DrawOp::RRect { node, .. }
            | DrawOp::Circle { node, .. }
            | DrawOp::Shadow { node, .. }
            | DrawOp::Text { node, .. }
            | DrawOp::RImg { node, .. }
            | DrawOp::Path { node, .. } => Some(*node),
            DrawOp::PushClip { .. } | DrawOp::PushLayer { .. } | DrawOp::Pop => None,
        }
    }
}

/// Damaged region (device px). Damage lives on the plan, not per-op.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DamageRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Per-build accounting: the damage-discipline instrument (M4 tripwire —
/// rebuilt-vs-skipped is measured, not assumed).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PlanStats {
    /// Retained nodes visited by the build walk.
    pub nodes_visited: usize,
    /// Draw ops emitted.
    pub ops_emitted: usize,
    /// Nodes skipped because neither they nor any descendant was dirty.
    pub subtrees_skipped: usize,
    /// Dirty nodes with no committed box (layout has not run yet).
    pub nodes_unboxed: usize,
}

/// One frame's display list: viewport + ordered ops + damage + stats.
#[derive(Clone, Debug, Default)]
pub struct FramePlan {
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub ops: Vec<DrawOp>,
    pub damage: Vec<DamageRect>,
    pub stats: PlanStats,
    /// True when built ignoring dirty masks (the oracle's full-repaint
    /// arm). Incremental builds never set this.
    pub full_repaint: bool,
}

impl FramePlan {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// One a11y-tree entry: semantics payload + committed bounds (the M10
/// emitters' input; M4 computes + dumps it).
#[derive(Clone, PartialEq, Debug)]
pub struct SemanticsEntry {
    pub node: NodeId,
    pub semantics: Semantics,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Per-commit a11y delta: upserts (new or changed payloads/bounds) plus
/// removals. Nothing is asserted about emitters in M4 (M10 scope) — only
/// that payloads flow.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct SemanticsDiff {
    pub upserted: Vec<SemanticsEntry>,
    pub removed: Vec<NodeId>,
}

impl SemanticsDiff {
    pub fn is_empty(&self) -> bool {
        self.upserted.is_empty() && self.removed.is_empty()
    }

    /// Deterministic line dump (the BUILD-ORDER §3 smoke artifact): sorted
    /// by node identity so repeated dumps diff cleanly.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        let mut up = self.upserted.clone();
        up.sort_by_key(|e| (e.node.gen().index(), e.node.gen().generation()));
        for e in &up {
            out.push_str(&format!(
                "upsert node={:?} role={:?} checked={:?} selected={:?} disabled={} label={:?} bounds=({:.1},{:.1},{:.1},{:.1})\n",
                e.node,
                e.semantics.role,
                e.semantics.checked,
                e.semantics.selected,
                e.semantics.disabled,
                e.semantics.label.as_deref(),
                e.x,
                e.y,
                e.w,
                e.h,
            ));
        }
        let mut rem = self.removed.clone();
        rem.sort_by_key(|id| (id.gen().index(), id.gen().generation()));
        for id in &rem {
            out.push_str(&format!("remove node={id:?}\n"));
        }
        out
    }
}

/// Core-side snapshot the diff is computed against (lives wherever the
/// caller keeps it — core-side per lock #25; never in hot crates).
#[derive(Clone, Debug, Default)]
pub struct SemanticsSnapshot {
    entries: HashMap<NodeId, SemanticsEntry>,
}

impl SemanticsSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Computes the [`SemanticsDiff`] against `snapshot` and advances the
/// snapshot. Pure over public retained reads (committed boxes + payloads).
pub fn compute_semantics_diff(rec: &Reconciler, snapshot: &mut SemanticsSnapshot) -> SemanticsDiff {
    let mut diff = SemanticsDiff::default();
    let mut seen: Vec<NodeId> = Vec::new();
    for id in rec.retained_ids() {
        let Some(n) = rec.get(id) else { continue };
        let Some(payload) = n.semantics.clone() else {
            continue;
        };
        let (x, y, w, h) = match &n.layout {
            Some(b) => (b.x, b.y, b.w, b.h),
            None => continue,
        };
        seen.push(id);
        let entry = SemanticsEntry {
            node: id,
            semantics: payload,
            x,
            y,
            w,
            h,
        };
        match snapshot.entries.get(&id) {
            Some(prev) if prev == &entry => {}
            _ => {
                snapshot.entries.insert(id, entry.clone());
                diff.upserted.push(entry);
            }
        }
    }
    let live: std::collections::HashSet<NodeId> = seen.into_iter().collect();
    let gone: Vec<NodeId> = snapshot
        .entries
        .keys()
        .copied()
        .filter(|id| !live.contains(id))
        .collect();
    for id in gone {
        snapshot.entries.remove(&id);
        diff.removed.push(id);
    }
    diff
}

/// Backend failure (loud by rule: unsupported content errors, never silent
/// placeholders).
#[derive(Clone, PartialEq, Debug)]
pub enum BackendError {
    UnknownSurface(SurfaceId),
    UnsupportedOp(String),
    BadSurface(String),
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendError::UnknownSurface(id) => write!(f, "unknown surface {id:?}"),
            BackendError::UnsupportedOp(why) => write!(f, "unsupported op: {why}"),
            BackendError::BadSurface(why) => write!(f, "bad surface: {why}"),
        }
    }
}

impl std::error::Error for BackendError {}

/// Per-paint accounting on the backend side.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PaintStats {
    pub ops_executed: usize,
    pub paints: u64,
    /// True when the plan was empty and the surface was untouched
    /// (static UI ≈ 0 CPU on repaint).
    pub skipped_empty: bool,
}

/// The renderer contract (renderer-contract doc): every backend implements
/// every method. `commit` absorbs the [`TreeDiff`] (presenter-side state is
/// keyed by [`NodeId`]); `paint` rasterizes a [`FramePlan`].
pub trait RendererBackend {
    fn kind(&self) -> PresenterKind;
    fn caps(&self) -> Caps;
    fn create_surface(&mut self, desc: SurfaceDesc) -> Result<SurfaceId, BackendError>;
    fn destroy_surface(&mut self, id: SurfaceId) -> Result<(), BackendError>;
    fn commit(&mut self, diff: &TreeDiff) -> Result<(), BackendError>;
    fn paint(&mut self, surface: SurfaceId, plan: &FramePlan) -> Result<PaintStats, BackendError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_caps_degrade_loudly() {
        let caps = Caps::cpu_fallback();
        assert!(!caps.blur_backdrop, "no blur on CPU — offset solids");
        assert!(!caps.text_as_paths, "glyph cells until M6");
        assert!(caps.max_layers >= 1);
    }

    #[test]
    fn semantics_dump_is_deterministic() {
        let d = SemanticsDiff::default();
        assert!(d.is_empty());
        assert_eq!(d.dump(), "");
    }
}
