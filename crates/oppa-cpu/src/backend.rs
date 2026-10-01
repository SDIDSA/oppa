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
    BackendError, Caps, DrawOp, FontId, FontRun, FramePlan, ImageId, NodeId, PaintStats,
    PlacedGlyph, PresenterKind, RendererBackend, SurfaceDesc, SurfaceId, TreeDiff,
};
use tiny_skia::{
    Color as TSColor, FillRule, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Transform,
};

use crate::path::build_path;

struct Surface {
    desc: SurfaceDesc,
    pixmap: Pixmap,
    /// Retained display list: every live node's latest ops, in splice
    /// order. Incremental paints replace the dirty nodes' ops and replay
    /// all; full repaints replace the list wholesale.
    retained: Vec<DrawOp>,
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
/// there), so paint-time construction cannot fail.
#[derive(Clone)]
struct FaceBytes {
    bytes: Arc<Vec<u8>>,
    index: u32,
}

impl FaceBytes {
    fn checked(bytes: Vec<u8>, index: u32, which: &str) -> Self {
        ab_glyph::FontRef::try_from_slice_and_index(&bytes, index)
            .unwrap_or_else(|e| panic!("oppa-cpu: {which} font bytes rejected: {e}"));
        Self {
            bytes: Arc::new(bytes),
            index,
        }
    }
}

#[derive(Default)]
struct FaceTable {
    default: Option<FaceBytes>,
    explicit: HashMap<FontId, FaceBytes>,
}

impl FaceTable {
    fn face_for(&self, id: FontId) -> Option<&FaceBytes> {
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
    pub fn set_font_bytes(&mut self, bytes: Vec<u8>, index: u32) {
        self.faces.default = Some(FaceBytes::checked(bytes, index, "default"));
    }

    /// Registers the face for one shaper-reported font id (mirrors
    /// Vello's `set_font_for`, M7 decision 110): runs carrying this id
    /// rasterize real glyphs; runs without any usable face keep the
    /// legacy advance-cell bars.
    pub fn set_font_for(&mut self, id: FontId, bytes: Vec<u8>, index: u32) {
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
            premul.push(premultiply_u8(px[0], a));
            premul.push(premultiply_u8(px[1], a));
            premul.push(premultiply_u8(px[2], a));
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

fn ts_color(c: oppa::Color, opacity: f32) -> TSColor {
    let a = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    TSColor::from_rgba8(
        ((c.0 >> 16) & 0xFF) as u8,
        ((c.0 >> 8) & 0xFF) as u8,
        (c.0 & 0xFF) as u8,
        a,
    )
}

fn surface_bg(desc: SurfaceDesc) -> TSColor {
    ts_color(desc.background, 1.0)
}

fn rect_of(x: f32, y: f32, w: f32, h: f32) -> Option<Rect> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Rect::from_xywh(x, y, w, h)
}

fn paint_of(c: oppa::Color, opacity: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(ts_color(c, opacity));
    p.anti_alias = true;
    p
}

/// Box-blurred solid shadow into `target` (Phase 36 PR4, decision
/// 356): fills the rect solid on a transparent layer, runs two
/// separable box passes (radius `r` each — the gaussian
/// approximation the Vello/CSS arms pair with tol-banded, never
/// pixel-exact), and composites the layer back with `draw_pixmap`.
/// Returns false when nothing paints (degenerate rect or zero
/// alpha — quiet, like every other degenerate op).
#[allow(clippy::too_many_arguments)]
fn paint_box_blurred_shadow(
    target: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    color: oppa::Color,
    alpha: f32,
    mask: Option<&tiny_skia::Mask>,
) -> bool {
    if w <= 0.0 || h <= 0.0 || alpha <= 0.0 {
        return false;
    }
    let k = radius.ceil().max(1.0) as usize;
    // Two-pass coverage needs the kernel diameter plus one on every
    // side (a 3r spread stays inside; larger radii clip the far tail
    // — stated bound, the oracle bands it).
    let m = (k * 2 + 1) as i32;
    let lw = (w.ceil() as i32 + 2 * m).max(1);
    let lh = (h.ceil() as i32 + 2 * m).max(1);
    if lw > 4096 || lh > 4096 {
        panic!(
            "cpu: blurred shadow layer {lw}x{lh} exceeds the 4096px bound — refused, never silent"
        );
    }
    let Some(mut layer) = Pixmap::new(lw as u32, lh as u32) else {
        return false;
    };
    let fill = {
        let mut p = Paint::default();
        p.set_color(ts_color(color, alpha));
        p.anti_alias = false;
        p
    };
    // Solid rect at the layer origin + margin (integer-aligned: the
    // layer is a scratch space, snapping here never moves paint).
    let rx = m;
    let ry = m;
    let rw = w.ceil() as i32;
    let rh = h.ceil() as i32;
    if let Some(rc) = Rect::from_xywh(rx as f32, ry as f32, rw as f32, rh as f32) {
        layer.fill_rect(rc, &fill, Transform::identity(), None);
    }
    box_blur_in_place(&mut layer, k);
    box_blur_in_place(&mut layer, k);
    target.draw_pixmap(
        x.floor() as i32 - m,
        y.floor() as i32 - m,
        layer.as_ref(),
        &PixmapPaint {
            opacity: 1.0,
            ..PixmapPaint::default()
        },
        Transform::identity(),
        mask,
    );
    true
}

/// One separable box-blur pass over premultiplied pixels (sliding
/// window, edge-extended): radius `k` in device px.
fn box_blur_in_place(pixmap: &mut Pixmap, k: usize) {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    if w == 0 || h == 0 || k == 0 {
        return;
    }
    let n = 2 * k + 1;
    // Horizontal pass.
    let mut tmp = vec![0u8; w * h * 4];
    for y in 0..h {
        let mut acc = [0u32; 4];
        for dx in 0..n {
            let sx = dx.min(w - 1).saturating_sub(k);
            let p = &pixmap.pixels()[y * w + sx.min(w - 1)];
            acc[0] += p.red() as u32;
            acc[1] += p.green() as u32;
            acc[2] += p.blue() as u32;
            acc[3] += p.alpha() as u32;
        }
        for x in 0..w {
            let o = (y * w + x) * 4;
            tmp[o] = (acc[0] / n as u32) as u8;
            tmp[o + 1] = (acc[1] / n as u32) as u8;
            tmp[o + 2] = (acc[2] / n as u32) as u8;
            tmp[o + 3] = (acc[3] / n as u32) as u8;
            let out_x = x.saturating_sub(k);
            let in_x = (x + k + 1).min(w - 1);
            let out_p = &pixmap.pixels()[y * w + out_x];
            let in_p = &pixmap.pixels()[y * w + in_x];
            acc[0] = acc[0] - out_p.red() as u32 + in_p.red() as u32;
            acc[1] = acc[1] - out_p.green() as u32 + in_p.green() as u32;
            acc[2] = acc[2] - out_p.blue() as u32 + in_p.blue() as u32;
            acc[3] = acc[3] - out_p.alpha() as u32 + in_p.alpha() as u32;
        }
    }
    // Vertical pass (reads horizontal output, writes back).
    let mut back = vec![0u8; w * h * 4];
    for x in 0..w {
        let mut acc = [0u32; 4];
        let at = |yy: usize| &tmp[(yy * w + x) * 4..(yy * w + x) * 4 + 4];
        for dy in 0..n {
            let sy = dy.min(h - 1).saturating_sub(k);
            let p = at(sy);
            acc[0] += p[0] as u32;
            acc[1] += p[1] as u32;
            acc[2] += p[2] as u32;
            acc[3] += p[3] as u32;
        }
        for y in 0..h {
            let o = (y * w + x) * 4;
            back[o] = (acc[0] / n as u32) as u8;
            back[o + 1] = (acc[1] / n as u32) as u8;
            back[o + 2] = (acc[2] / n as u32) as u8;
            back[o + 3] = (acc[3] / n as u32) as u8;
            let out_y = y.saturating_sub(k);
            let in_y = (y + k + 1).min(h - 1);
            let out_p = at(out_y);
            let in_p = at(in_y);
            acc[0] = acc[0] - out_p[0] as u32 + in_p[0] as u32;
            acc[1] = acc[1] - out_p[1] as u32 + in_p[1] as u32;
            acc[2] = acc[2] - out_p[2] as u32 + in_p[2] as u32;
            acc[3] = acc[3] - out_p[3] as u32 + in_p[3] as u32;
        }
    }
    for (dst, src) in pixmap.pixels_mut().iter_mut().zip(back.chunks_exact(4)) {
        *dst = tiny_skia::PremultipliedColorU8::from_rgba(src[0], src[1], src[2], src[3])
            .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
    }
}

/// Pixel inside every active clip rect (all `PushClip`s are rects, so
/// testing the stack is exact — no mask needed).
fn inside_clips(px: i32, py: i32, clips: &[(f32, f32, f32, f32)]) -> bool {
    let (fx, fy) = (px as f32, py as f32);
    clips
        .iter()
        .all(|(x, y, w, h)| fx >= *x && fy >= *y && fx < x + w && fy < y + h)
}

/// Rasterizes one font run's glyphs in ink (opt-in glyph path,
/// decision 200): ab_glyph alpha coverage per cell, SrcOver-blended
/// premultiplied over the pixmap. Positions mirror the Vello encoder
/// exactly -- glyph origin at `(x + g.x, y + baseline)` (y=0 is the
/// baseline), unhinted, `font_size = em_size`. Empty-outline glyphs
/// (spaces) ink nothing. Returns the outlined glyph count for
/// `executed`.
#[allow(clippy::too_many_arguments)]
fn rasterize_glyphs(
    pix: &mut Pixmap,
    face: &FaceBytes,
    cells: &[PlacedGlyph],
    x: f32,
    y: f32,
    baseline: f32,
    em_size: f32,
    ink: oppa::Color,
    alpha: f32,
    clips: &[(f32, f32, f32, f32)],
) -> usize {
    use ab_glyph::{Font, FontRef, PxScale};
    // Validated at `set_font_*` time over the same bytes: infallible here.
    let font = FontRef::try_from_slice_and_index(&face.bytes, face.index)
        .expect("face bytes validated at registration");
    // ab_glyph's PxScale is font-height pixels (ascent + descent), not
    // em: convert so the rendered EM is exactly `em_size` device px
    // (what Vello's `font_size` means -- same outlines, same size).
    // Degenerate units table (absent/zero) falls back to 1:1.
    let px = match font.units_per_em() {
        Some(upe) if upe > 0.0 => em_size * font.height_unscaled() / upe,
        _ => em_size,
    };
    let scale = PxScale { x: px, y: px };
    let (ir, ig, ib) = (
        ((ink.0 >> 16) & 0xFF) as f32,
        ((ink.0 >> 8) & 0xFF) as f32,
        (ink.0 & 0xFF) as f32,
    );
    let (w, h) = (pix.width() as i32, pix.height() as i32);
    let data = pix.data_mut();
    let mut done = 0usize;
    for g in cells {
        let positioned = ab_glyph::GlyphId(g.glyph_id as u16)
            .with_scale_and_position(scale, ab_glyph::point(x + g.x, y + baseline));
        let Some(outlined) = font.outline_glyph(positioned) else {
            continue;
        };
        done += 1;
        let bounds = outlined.px_bounds();
        let (ox, oy) = (bounds.min.x as i32, bounds.min.y as i32);
        outlined.draw(|dx, dy, cov| {
            let a_tot = cov * alpha;
            if a_tot <= 0.0 {
                return;
            }
            let (px, py) = (ox + dx as i32, oy + dy as i32);
            if px < 0 || py < 0 || px >= w || py >= h || !inside_clips(px, py, clips) {
                return;
            }
            let i = (py as u32 * w as u32 + px as u32) as usize * 4;
            let ia = 1.0 - a_tot;
            data[i] = (ir * a_tot + data[i] as f32 * ia).round().clamp(0.0, 255.0) as u8;
            data[i + 1] = (ig * a_tot + data[i + 1] as f32 * ia)
                .round()
                .clamp(0.0, 255.0) as u8;
            data[i + 2] = (ib * a_tot + data[i + 2] as f32 * ia)
                .round()
                .clamp(0.0, 255.0) as u8;
            data[i + 3] = (255.0 * a_tot + data[i + 3] as f32 * ia)
                .round()
                .clamp(0.0, 255.0) as u8;
        });
    }
    done
}

fn fill_disc(pixmap: &mut Pixmap, cx: f32, cy: f32, r: f32, paint: &Paint, mask: Option<&Mask>) {
    if r <= 0.0 {
        return;
    }
    let mut pb = PathBuilder::new();
    pb.push_circle(cx, cy, r);
    if let Some(path) = pb.finish() {
        pixmap.fill_path(&path, paint, FillRule::Winding, Transform::identity(), mask);
    }
}

fn fill_rounded(
    pixmap: &mut Pixmap,
    rect: (f32, f32, f32, f32),
    r: f32,
    paint: &Paint,
    mask: Option<&Mask>,
) {
    let (x, y, w, h) = rect;
    let r = r.min(w * 0.5).min(h * 0.5);
    if r <= 0.0 {
        if let Some(rc) = rect_of(x, y, w, h) {
            pixmap.fill_rect(rc, paint, Transform::identity(), mask);
        }
        return;
    }
    // Center bands + four corner discs (no arc API needed, deterministic).
    if let Some(rc) = rect_of(x + r, y, w - 2.0 * r, h) {
        pixmap.fill_rect(rc, paint, Transform::identity(), mask);
    }
    if let Some(rc) = rect_of(x, y + r, w, h - 2.0 * r) {
        pixmap.fill_rect(rc, paint, Transform::identity(), mask);
    }
    for (cx, cy) in [
        (x + r, y + r),
        (x + w - r, y + r),
        (x + r, y + h - r),
        (x + w - r, y + h - r),
    ] {
        fill_disc(pixmap, cx, cy, r, paint, mask);
    }
}

/// Per-corner rounded fill (Round 11.1, decision 305): the
/// [`fill_rounded`] bands-and-discs rule generalized — each corner
/// disc takes its own radius (CSS order `[tl, tr, br, bl]`, already
/// box-clamped by the shared builder) and edge strips span the
/// per-side maxima (unequal neighbors need their own strips — a
/// shared band would notch the straights). All-equal radii paint
/// exactly the uniform union (center + top + bottom rebuild the
/// vertical band; the side strips overdraw the horizontal band with
/// identical paint).
fn fill_rounded_corners(
    pixmap: &mut Pixmap,
    rect: (f32, f32, f32, f32),
    radii: [f32; 4],
    paint: &Paint,
    mask: Option<&Mask>,
) {
    let (x, y, w, h) = rect;
    let [tl, tr, br, bl] = radii;
    if tl <= 0.0 && tr <= 0.0 && br <= 0.0 && bl <= 0.0 {
        if let Some(rc) = rect_of(x, y, w, h) {
            pixmap.fill_rect(rc, paint, Transform::identity(), mask);
        }
        return;
    }
    let fill = |pixmap: &mut Pixmap, r: (f32, f32, f32, f32)| {
        if let Some(rc) = rect_of(r.0, r.1, r.2, r.3) {
            pixmap.fill_rect(rc, paint, Transform::identity(), mask);
        }
    };
    // Center, top/bottom edge strips, left/right edge strips.
    fill(
        pixmap,
        (
            x + tl.max(bl),
            y + tl.max(tr),
            w - tl.max(bl) - tr.max(br),
            h - tl.max(tr) - bl.max(br),
        ),
    );
    fill(pixmap, (x + tl, y, w - tl - tr, tl.max(tr)));
    fill(
        pixmap,
        (x + bl, y + h - bl.max(br), w - bl - br, bl.max(br)),
    );
    fill(pixmap, (x, y + tl, tl.max(bl), h - tl - bl));
    fill(
        pixmap,
        (x + w - tr.max(br), y + tr, tr.max(br), h - tr - br),
    );
    for (cx, cy, r) in [
        (x + tl, y + tl, tl),
        (x + w - tr, y + tr, tr),
        (x + w - br, y + h - br, br),
        (x + bl, y + h - bl, bl),
    ] {
        fill_disc(pixmap, cx, cy, r, paint, mask);
    }
}

fn clip_mask(w: u32, h: u32, x: f32, y: f32, cw: f32, ch: f32) -> Option<Mask> {
    let mut mask = Mask::new(w, h)?;
    let mut pb = PathBuilder::new();
    pb.move_to(x.max(0.0), y.max(0.0));
    pb.line_to((x + cw).min(w as f32), y.max(0.0));
    pb.line_to((x + cw).min(w as f32), (y + ch).min(h as f32));
    pb.line_to(x.max(0.0), (y + ch).min(h as f32));
    pb.close();
    let path = pb.finish()?;
    mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
    Some(mask)
}

/// Geometric rect intersection (clip-stack math stays in backend logic;
// lint: debate resolved at M4 — per-op mask rebuild below, M6+ optimization).
fn intersect_rect(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    let x = a.0.max(b.0);
    let y = a.1.max(b.1);
    let r = (a.0 + a.2).min(b.0 + b.2);
    let b2 = (a.1 + a.3).min(b.1 + b.3);
    (x, y, (r - x).max(0.0), (b2 - y).max(0.0))
}

/// tiny-skia's exact premultiply (color.rs `premultiply_u8`) — the
/// seam stores what the rasterizer would store, bit-for-bit, so
/// deposit-time expectations and paint-time pixels cannot drift.
fn premultiply_u8(c: u8, a: u8) -> u8 {
    let prod = u32::from(c) * u32::from(a) + 128;
    ((prod + (prod >> 8)) >> 8) as u8
}

fn replay(
    surface: &mut Surface,
    ops: &[DrawOp],
    faces: &FaceTable,
    images: &HashMap<ImageId, Pixmap>,
) -> Result<usize, BackendError> {
    // Validate loudly before touching pixels: no partial paints.
    // Registered images paint; unregistered ids refuse (pending images
    // never paint placeholders — G8, decision 223). Path data parses
    // here too (decision 291 — invalid vectors fail the whole paint,
    // never mis-paint half a frame).
    for op in ops {
        if let DrawOp::RImg { node, image, .. } = op {
            if !images.contains_key(image) {
                return Err(BackendError::UnsupportedOp(format!(
                    "RImg on {node:?} (image {image:?}): pixels not registered — \
                     decode via oppa-image and insert_image first (no decoded pixels \
                     paint placeholders, never silently)"
                )));
            }
        }
        if let DrawOp::Path { node, data, .. } = op {
            if let Err(why) = build_path(data) {
                return Err(BackendError::UnsupportedOp(format!(
                    "Path on {node:?}: invalid SVG path data ({why}) — refused, never mis-painted"
                )));
            }
        }
    }
    surface.pixmap.fill(surface_bg(surface.desc));
    let (sw, sh) = (surface.desc.width_px, surface.desc.height_px);
    let mut executed = 0usize;
    let mut alpha_stack: Vec<f32> = vec![1.0];
    // Clip stack as geometric rects (PushClip/Pop are LIFO-disciplined by
    // the builder: ScrollArea viewports wrap their subtrees). The effective
    // clip is rebuilt into a mask per painted op — M4-scale cheap, M6+
    // optimization to cache it.
    let mut clip_stack: Vec<(f32, f32, f32, f32)> = Vec::new();
    // Merged pop order (documented approximation, exact for
    // builder-emitted plans which never mix clips and layers in one
    // subtree): layers pop before clips.
    let mut layer_depth = 0usize;
    for op in ops {
        let alpha = alpha_stack.iter().product::<f32>();
        let eff = clip_stack
            .iter()
            .copied()
            .reduce(intersect_rect)
            .unwrap_or((0.0, 0.0, sw as f32, sh as f32));
        let full_surface = eff.0 <= 0.0 && eff.1 <= 0.0 && eff.2 >= sw as f32 && eff.3 >= sh as f32;
        // Borrow dance: the mask (if any) is built fresh per op.
        let mask = if full_surface {
            None
        } else {
            clip_mask(sw, sh, eff.0, eff.1, eff.2, eff.3)
        };
        let mask_ref = mask.as_ref();
        match op {
            DrawOp::Rect {
                x,
                y,
                w,
                h,
                color,
                opacity,
                ..
            } => {
                let a = alpha * opacity;
                if a > 0.0 {
                    if let Some(rc) = rect_of(*x, *y, *w, *h) {
                        surface.pixmap.fill_rect(
                            rc,
                            &paint_of(*color, a),
                            Transform::identity(),
                            mask_ref,
                        );
                        executed += 1;
                    }
                }
            }
            DrawOp::RRect {
                x,
                y,
                w,
                h,
                radius,
                radii,
                color,
                opacity,
                ..
            } => {
                let a = alpha * opacity;
                if a > 0.0 {
                    // Round 11.1: per-corner radii generalize the
                    // bands-and-discs rule (each corner disc takes its
                    // own radius; bands span the edge maxima — the
                    // uniform shape is the all-equal special case,
                    // pixel-identical to before).
                    match radii {
                        Some(rs) => fill_rounded_corners(
                            &mut surface.pixmap,
                            (*x, *y, *w, *h),
                            *rs,
                            &paint_of(*color, a),
                            mask_ref,
                        ),
                        None => fill_rounded(
                            &mut surface.pixmap,
                            (*x, *y, *w, *h),
                            *radius,
                            &paint_of(*color, a),
                            mask_ref,
                        ),
                    }
                    executed += 1;
                }
            }
            DrawOp::Circle {
                cx,
                cy,
                r,
                color,
                opacity,
                ..
            } => {
                let a = alpha * opacity;
                if a > 0.0 && *r > 0.0 {
                    fill_disc(
                        &mut surface.pixmap,
                        *cx,
                        *cy,
                        *r,
                        &paint_of(*color, a),
                        mask_ref,
                    );
                    executed += 1;
                }
            }
            DrawOp::Shadow {
                x,
                y,
                w,
                h,
                dx,
                dy,
                blur_radius,
                color,
                ..
            } => {
                if *blur_radius <= 0.0 {
                    // Offset solid (the pre-PR4 shape, pixel-exact).
                    if let Some(rc) = rect_of(x + dx, y + dy, *w, *h) {
                        surface.pixmap.fill_rect(
                            rc,
                            &paint_of(*color, alpha),
                            Transform::identity(),
                            mask_ref,
                        );
                        executed += 1;
                    }
                } else {
                    // Box-blurred shadow (Phase 36 PR4, decision 356):
                    // solid rect into a layer pixmap, two separable box
                    // passes (horizontal + vertical), composited back.
                    // Radius in device px; non-finite refuses loudly.
                    if !blur_radius.is_finite() {
                        panic!(
                            "cpu: shadow blur_radius non-finite ({blur_radius}) — never paints silently"
                        );
                    }
                    if paint_box_blurred_shadow(
                        &mut surface.pixmap,
                        x + dx,
                        y + dy,
                        *w,
                        *h,
                        *blur_radius,
                        *color,
                        alpha,
                        mask_ref,
                    ) {
                        executed += 1;
                    }
                }
            }
            DrawOp::Text {
                x,
                y,
                line_height,
                baseline,
                em_size,
                glyphs,
                fonts,
                ink,
                opacity,
                ..
            } => {
                let a = alpha * opacity;
                if a > 0.0 && !glyphs.is_empty() {
                    // Run segmentation mirrors the Vello encoder: empty
                    // `fonts` (hand-built plans) is one implicit run
                    // over all cells.
                    let implicit = [FontRun {
                        glyph_range: (0, glyphs.len()),
                        family: String::new(),
                        font_id: FontId(0),
                    }];
                    let runs: &[FontRun] = if fonts.is_empty() { &implicit } else { fonts };
                    for run in runs {
                        let (lo, hi) = (
                            run.glyph_range.0.min(glyphs.len()),
                            run.glyph_range.1.min(glyphs.len()),
                        );
                        if lo >= hi {
                            continue;
                        }
                        let cells = &glyphs[lo..hi];
                        match faces.face_for(run.font_id) {
                            Some(face) => {
                                executed += rasterize_glyphs(
                                    &mut surface.pixmap,
                                    face,
                                    cells,
                                    *x,
                                    *y,
                                    *baseline,
                                    *em_size,
                                    *ink,
                                    a,
                                    &clip_stack,
                                );
                            }
                            // No usable face: legacy advance-cell bars
                            // (all pre-existing oracle behavior preserved).
                            None => {
                                let paint = paint_of(*ink, a);
                                for g in cells {
                                    if g.advance <= 0.0 {
                                        continue;
                                    }
                                    if let Some(rc) = rect_of(x + g.x, *y, g.advance, *line_height)
                                    {
                                        surface.pixmap.fill_rect(
                                            rc,
                                            &paint,
                                            Transform::identity(),
                                            mask_ref,
                                        );
                                        executed += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            DrawOp::Path {
                x,
                y,
                data,
                fill,
                stroke,
                opacity,
                ..
            } => {
                let a = alpha * opacity;
                if a > 0.0 {
                    // Validated above — unparseable data never reaches paint.
                    let path = build_path(data)
                        .expect("validated above — invalid paths never reach paint");
                    let t = Transform::from_translate(*x, *y);
                    if let Some(fill_color) = fill {
                        surface.pixmap.fill_path(
                            &path,
                            &paint_of(*fill_color, a),
                            FillRule::Winding,
                            t,
                            mask_ref,
                        );
                        executed += 1;
                    }
                    if let Some(s) = stroke {
                        // Zero-width strokes skip: tiny-skia strokes a
                        // width-0 path as a hairline, which would be a
                        // silent surprise (negative widths refuse at
                        // plan build, so only zero reaches here).
                        if s.width > 0.0 {
                            let ts_stroke = tiny_skia::Stroke {
                                width: s.width,
                                miter_limit: 4.0,
                                line_cap: tiny_skia::LineCap::Round,
                                line_join: tiny_skia::LineJoin::Round,
                                dash: None,
                            };
                            surface.pixmap.stroke_path(
                                &path,
                                &paint_of(s.color, a),
                                &ts_stroke,
                                t,
                                mask_ref,
                            );
                            executed += 1;
                        }
                    }
                }
            }
            DrawOp::RImg {
                x, y, w, h, image, ..
            } => {
                let a = alpha_stack.iter().product::<f32>();
                if a > 0.0 && *w > 0.0 && *h > 0.0 {
                    let pixmap = images
                        .get(image)
                        .expect("validated above — unregistered images never reach paint");
                    let (iw, ih) = (pixmap.width() as f32, pixmap.height() as f32);
                    if iw > 0.0 && ih > 0.0 {
                        // Source→dest scale (from_row: [sx 0 0 sy 0 0] —
                        // the (x, y) offset rides draw_pixmap's own args).
                        let t = Transform::from_row(*w / iw, 0.0, 0.0, *h / ih, 0.0, 0.0);
                        surface.pixmap.draw_pixmap(
                            x.round() as i32,
                            y.round() as i32,
                            pixmap.as_ref(),
                            &PixmapPaint {
                                opacity: a.clamp(0.0, 1.0),
                                ..PixmapPaint::default()
                            },
                            t,
                            mask_ref,
                        );
                        executed += 1;
                    }
                }
            }
            DrawOp::PushClip { x, y, w, h } => {
                clip_stack.push((*x, *y, *w, *h));
            }
            DrawOp::PushLayer { opacity } => {
                alpha_stack.push(opacity.clamp(0.0, 1.0));
                layer_depth += 1;
            }
            DrawOp::Pop => {
                if layer_depth > 0 {
                    alpha_stack.pop();
                    layer_depth -= 1;
                } else if !clip_stack.is_empty() {
                    clip_stack.pop();
                }
            }
        }
    }
    Ok(executed)
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
        pixmap.fill(surface_bg(desc));
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
                DiffOp::Remove { id } => {
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
        let executed = replay(s, &ops, &self.faces, &self.images)?;
        Ok(PaintStats {
            ops_executed: executed,
            paints: self.paints_total,
            skipped_empty: false,
        })
    }
}
