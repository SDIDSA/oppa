//! tiny-skia CPU backend: per-surface commit path + PNG output.
//!
//! Presenter-side state is keyed by [`NodeId`](oppa::NodeId): `commit`
//! absorbs the [`TreeDiff`](oppa::TreeDiff) (add/remove bookkeeping),
//! `paint` splices the plan's ops into a retained per-surface op list and
//! replays the whole list. Replay (not damage blits) is the M4 raster
//! discipline: damage correctness today means the builder rebuilt exactly
//! the dirty set (proven by the oracle), while an empty plan skips the
//! surface untouched (static ≈ 0 CPU on repaint).
//!
//! Text rasterization: each [`PlacedGlyph`](oppa::PlacedGlyph) fills its
//! advance cell in ink — positions and advances come from the layout's
//! pre-shaped runs, never re-shaped here. `Caps::text_as_paths` is false
//! by documentation: the M6 glyph-quality review starts from these
//! block cells (AA fringes at subpixel cell edges only).
//!
//! Opt-in glyph path (decision 200): runs whose `font_id` has a face
//! registered via [`CpuBackend::set_font_bytes`]/[`set_font_for`]
//! rasterize real ab_glyph coverage (positions mirror the Vello
//! encoder exactly -- origin at `x + g.x, y + baseline`, unhinted,
//! `font_size = em_size`); runs without a usable face keep the
//! legacy bars, so every pre-existing oracle expectation holds
//! byte-identically.
//!
//! Loud refusals: `RImg` (no decoded pixels in v1), unknown surfaces,
//! zero-size surfaces, and font bytes rejected at `set_font_*` time
//! (parsed once there — paint never fails on fonts).
//!
//! Vector paths ([`DrawOp::Path`](oppa::DrawOp), decision 291): parsed
//! through the backend-owned SVG parser ([`path`](crate::path) —
//! `tiny-skia-path` 0.12 ships no `from_svg`), translated by the
//! committed box origin, filled winding + stroked round/round.
//! Invalid data fails the paint loudly before touching pixels (no
//! partial paints); zero-width strokes skip (a tiny-skia 0-width
//! stroke would hairline — never silently).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oppa::{
    BackendError, Caps, DrawOp, FontId, FramePlan, ImageId, NodeId, PaintStats, PresenterKind,
    RendererBackend, SurfaceDesc, SurfaceId, TreeDiff,
};
use tiny_skia::Pixmap;

mod raster;
mod replay;

pub(super) struct Surface {
    pub(super) desc: SurfaceDesc,
    pub(super) pixmap: Pixmap,
    /// Retained display list: every live node's latest ops, in splice
    /// order. Incremental paints replace the dirty nodes' ops and replay
    /// all; full repaints replace the list wholesale.
    pub(super) retained: Vec<DrawOp>,
}

/// Order-preserving incremental splice (M8): dirty nodes' op-runs are
/// replaced IN PLACE at their first retained position. Paint order
/// follows plan order at replay, so appending dirty runs at the end
/// would let a re-spliced background cover its foreground — the M5
/// toggle's track over its knob on every interpolation tail frame, or a
/// recycled cell's bg over its own text. Runs for nodes never retained
/// before append in plan order. Stack ops (`PushClip`/`PushLayer`/`Pop`
/// — nodeless) pass through in place, with the plan's stack ops appended
/// (dirty-ScrollArea clip re-emit ordering stays the documented M5+ open
/// item: no M8/M5 scene re-emits clips incrementally).
pub fn splice_retained(retained: &mut Vec<DrawOp>, plan_ops: &[DrawOp]) {
    let dirty: HashSet<NodeId> = plan_ops.iter().filter_map(|o| o.node()).collect();
    // Per-node runs in plan order (the builder emits each node's ops
    // contiguously, parent-first).
    let mut runs: HashMap<NodeId, Vec<DrawOp>> = HashMap::new();
    let mut run_order: Vec<NodeId> = Vec::new();
    let mut stack_ops: Vec<DrawOp> = Vec::new();
    for op in plan_ops {
        match op.node() {
            Some(n) => {
                if !runs.contains_key(&n) {
                    run_order.push(n);
                }
                runs.entry(n).or_default().push(op.clone());
            }
            None => stack_ops.push(op.clone()),
        }
    }
    let old = std::mem::take(retained);
    let mut done: HashSet<NodeId> = HashSet::new();
    for op in old {
        match op.node() {
            Some(n) if dirty.contains(&n) => {
                if done.insert(n) {
                    if let Some(run) = runs.remove(&n) {
                        retained.extend(run);
                    }
                }
            }
            _ => retained.push(op),
        }
    }
    for n in run_order {
        if let Some(run) = runs.remove(&n) {
            retained.extend(run);
        }
    }
    retained.extend(stack_ops);
}

pub struct CpuBackend {
    next_surface: u64,
    surfaces: HashMap<SurfaceId, Surface>,
    /// Presenter-side node registry (commit bookkeeping, keyed by NodeId).
    live_nodes: HashSet<NodeId>,
    paints_total: u64,
    /// Glyph faces for the opt-in raster path (decision 200): explicit
    /// per shaper-reported id, else the default, else legacy bars.
    faces: FaceTable,
    /// Decoded image registry (G8, decision 223): premultiplied
    /// pixmaps keyed by `ImageId`, deposited with `insert_image`
    /// (usually decoded off-thread via `oppa-image`, deposited on the
    /// UI thread). Missing ids refuse loudly at paint — pending
    /// images never paint placeholders.
    images: HashMap<ImageId, Pixmap>,
}

/// Validated font bytes: parsed once at `set_font_*` time (loud
/// there), so paint-time construction cannot fail. Shared
/// (`Arc`) — the runner hands one allocation per face that both
/// backends reference instead of copying (decision 437: ~3x font
/// RSS on font-rich devices otherwise).
#[derive(Clone)]
pub(super) struct FaceBytes {
    bytes: Arc<[u8]>,
    index: u32,
}

impl FaceBytes {
    fn checked(bytes: Arc<[u8]>, index: u32, which: &str) -> Self {
        ab_glyph::FontRef::try_from_slice_and_index(&bytes, index)
            .unwrap_or_else(|e| panic!("oppa-cpu: {which} font bytes rejected: {e}"));
        Self { bytes, index }
    }
}

#[derive(Default)]
pub(super) struct FaceTable {
    default: Option<FaceBytes>,
    explicit: HashMap<FontId, FaceBytes>,
}

impl FaceTable {
    pub(super) fn face_for(&self, id: FontId) -> Option<&FaceBytes> {
        self.explicit.get(&id).or(self.default.as_ref())
    }
}

impl CpuBackend {
    pub fn new() -> Self {
        Self {
            next_surface: 1,
            surfaces: HashMap::new(),
            live_nodes: HashSet::new(),
            paints_total: 0,
            faces: FaceTable::default(),
            images: HashMap::new(),
        }
    }

    /// Registers the default glyph face (mirrors Vello's
    /// `set_font_bytes`): `DrawOp::Text` runs whose `font_id` has no
    /// explicit face rasterize through this one. Replaces any previous
    /// default; bytes are validated here (loud), never at paint.
    pub fn set_font_bytes(&mut self, bytes: Arc<[u8]>, index: u32) {
        self.faces.default = Some(FaceBytes::checked(bytes, index, "default"));
    }

    /// Registers the face for one shaper-reported font id (mirrors
    /// Vello's `set_font_for`, M7 decision 110): runs carrying this id
    /// rasterize real glyphs; runs without any usable face keep the
    /// legacy advance-cell bars.
    pub fn set_font_for(&mut self, id: FontId, bytes: Arc<[u8]>, index: u32) {
        self.faces
            .explicit
            .insert(id, FaceBytes::checked(bytes, index, "explicit"));
    }

    /// Deposits decoded pixels for `id` (G8, decision 223): straight-
    /// alpha `RGBA8` (the `oppa-image` output shape) is validated
    /// (`rgba.len() == w*h*4`, non-zero, loudly) and premultiplied
    /// with tiny-skia's exact formula, then stored. Replaces any
    /// previous deposit. Bytes are validated here (loud), never at
    /// paint — the `set_font_bytes` rule applied to images.
    pub fn insert_image(&mut self, id: ImageId, width: u32, height: u32, rgba_straight: Vec<u8>) {
        if width == 0 || height == 0 {
            panic!(
                "oppa-cpu: insert_image {id:?}: zero size {width}x{height} — refused, never silent"
            );
        }
        let expect = width as usize * height as usize * 4;
        if rgba_straight.len() != expect {
            panic!(
                "oppa-cpu: insert_image {id:?}: {} bytes != {width}x{height}x4 ({expect}) — refused, never silent",
                rgba_straight.len()
            );
        }
        let mut premul = Vec::with_capacity(expect);
        for px in rgba_straight.chunks_exact(4) {
            let a = px[3];
            premul.push(raster::premultiply_u8(px[0], a));
            premul.push(raster::premultiply_u8(px[1], a));
            premul.push(raster::premultiply_u8(px[2], a));
            premul.push(a);
        }
        let size = tiny_skia::IntSize::from_wh(width, height).expect("non-zero checked above");
        let pixmap = Pixmap::from_vec(premul, size).expect("length checked above");
        self.images.insert(id, pixmap);
    }

    /// Drops a deposited image (returns false when absent — removal is
    /// idempotent teardown, not a refusal).
    pub fn remove_image(&mut self, id: ImageId) -> bool {
        self.images.remove(&id).is_some()
    }

    /// True when the backend holds pre-decoded pixels for `id`.
    pub fn has_image(&self, id: ImageId) -> bool {
        self.images.contains_key(&id)
    }

    /// Deposits one cache entry's pre-decoded pixels (Phase 36 PR4,
    /// decision 359 — the static image path): pulls `(width, height,
    /// RGBA8)` from the [`ImageCache`](oppa::ImageCache) and inserts
    /// (same validation, same premultiplication). Loud when the cache
    /// holds no pixels for the id (undeposited statics never paint
    /// placeholders — the RImg refusal moves here, to deposit time).
    pub fn insert_cached(&mut self, cache: &oppa::ImageCache, id: ImageId) {
        let Some((width, height, rgba)) = cache.pixels_of(id) else {
            panic!(
                "oppa-cpu: insert_cached {id:?}: no pre-decoded pixels in the cache — deposit with insert_pixels first (static pre-decoded only)"
            );
        };
        self.insert_image(id, width, height, rgba);
    }

    pub fn paints_total(&self) -> u64 {
        self.paints_total
    }

    pub fn live_node_count(&self) -> usize {
        self.live_nodes.len()
    }

    pub fn retained_op_count(&self, surface: SurfaceId) -> Option<usize> {
        self.surfaces.get(&surface).map(|s| s.retained.len())
    }

    /// Raw surface access for the oracle (byte-level image compare).
    pub fn pixmap(&self, surface: SurfaceId) -> Option<&Pixmap> {
        self.surfaces.get(&surface).map(|s| &s.pixmap)
    }

    /// Spot-check accessor: premultiplied RGBA at a device pixel (equal
    /// to straight RGBA wherever alpha is 255 — every M4 spot check).
    pub fn pixel_rgba(&self, surface: SurfaceId, x: u32, y: u32) -> Option<(u8, u8, u8, u8)> {
        let px = self.surfaces.get(&surface)?.pixmap.pixel(x, y)?;
        Some((px.red(), px.green(), px.blue(), px.alpha()))
    }

    /// Per-surface commit path → PNG bytes (tiny-skia's encoder).
    pub fn encode_png(&self, surface: SurfaceId) -> Result<Vec<u8>, BackendError> {
        let s = self
            .surfaces
            .get(&surface)
            .ok_or(BackendError::UnknownSurface(surface))?;
        s.pixmap
            .encode_png()
            .map_err(|e| BackendError::UnsupportedOp(format!("png encode failed: {e}")))
    }

    /// Per-surface commit path → PNG on disk (the M4 §3 artifact).
    pub fn save_png(&self, surface: SurfaceId, path: &std::path::Path) -> Result<(), BackendError> {
        let bytes = self.encode_png(surface)?;
        std::fs::write(path, bytes)
            .map_err(|e| BackendError::UnsupportedOp(format!("png write failed: {e}")))?;
        Ok(())
    }
}

impl Default for CpuBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RendererBackend for CpuBackend {
    fn kind(&self) -> PresenterKind {
        PresenterKind::Cpu
    }

    fn caps(&self) -> Caps {
        Caps::cpu_fallback()
    }

    fn create_surface(&mut self, desc: SurfaceDesc) -> Result<SurfaceId, BackendError> {
        if desc.width_px == 0 || desc.height_px == 0 {
            return Err(BackendError::BadSurface(format!(
                "zero-size surface {}x{}",
                desc.width_px, desc.height_px
            )));
        }
        let mut pixmap = Pixmap::new(desc.width_px, desc.height_px)
            .ok_or_else(|| BackendError::BadSurface("pixmap alloc failed".to_string()))?;
        pixmap.fill(raster::surface_bg(desc));
        let id = SurfaceId(self.next_surface);
        self.next_surface += 1;
        self.surfaces.insert(
            id,
            Surface {
                desc,
                pixmap,
                retained: Vec::new(),
            },
        );
        Ok(id)
    }

    fn destroy_surface(&mut self, id: SurfaceId) -> Result<(), BackendError> {
        self.surfaces
            .remove(&id)
            .map(|_| ())
            .ok_or(BackendError::UnknownSurface(id))
    }

    fn commit(&mut self, diff: &TreeDiff) -> Result<(), BackendError> {
        use oppa::DiffOp;
        for op in &diff.ops {
            match op {
                DiffOp::Add { id, .. } => {
                    self.live_nodes.insert(*id);
                }
                DiffOp::Remove { id, .. } => {
                    self.live_nodes.remove(id);
                    for surface in self.surfaces.values_mut() {
                        surface.retained.retain(|o| o.node() != Some(*id));
                    }
                }
                DiffOp::Move { .. } | DiffOp::Update { .. } => {
                    // Paint order follows plan order at replay; sibling
                    // reorder without repaint is an M5+ open item (no
                    // overlapping siblings in M4 scenes, so replay order is
                    // exact there — see the oracle test).
                }
            }
        }
        Ok(())
    }

    fn paint(&mut self, surface: SurfaceId, plan: &FramePlan) -> Result<PaintStats, BackendError> {
        let s = self
            .surfaces
            .get_mut(&surface)
            .ok_or(BackendError::UnknownSurface(surface))?;
        self.paints_total += 1;
        if plan.is_empty() {
            return Ok(PaintStats {
                ops_executed: 0,
                paints: self.paints_total,
                skipped_empty: true,
            });
        }
        if plan.full_repaint {
            s.retained = plan.ops.clone();
        } else {
            // Incremental splice, order-preserving (see `splice_retained`).
            splice_retained(&mut s.retained, &plan.ops);
        }
        let ops = s.retained.clone();
        let executed = replay::replay(s, &ops, &self.faces, &self.images)?;
        Ok(PaintStats {
            ops_executed: executed,
            paints: self.paints_total,
            skipped_empty: false,
        })
    }
}
