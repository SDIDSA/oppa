//! FramePlan builder from dirty subtrees (M4 damage discipline).
//!
//! The builder drains `STRUCTURE|STYLE|PAINT|TEXT` through
//! [`Reconciler::take_paint_masks`](oppa::Reconciler::take_paint_masks) and
//! emits ops only for dirty nodes plus their ancestor chain (ancestors emit
//! nothing themselves — they only open the walk). A subtree with no dirty
//! node anywhere below it is skipped whole and counted in
//! [`PlanStats::subtrees_skipped`](oppa::render::PlanStats). LAYOUT is
//! consumed by M3's engine run (not a builder input — a position-only move
//! with no paint flag is a documented finding, F1 in the M4 round entry,
//! not a silent rebuild); SEMANTICS flows to the A11Y phase.
//!
//! Damage is the union of dirty committed boxes (recorded on the plan for
//! the oracle and future raster use; the M4 backend replays its retained
//! op list per paint, so damage correctness today means "the dirty set is
//! exactly the rebuilt set").

use std::collections::{HashMap, HashSet};

use oppa::{
    CaretPaint, DamageRect, DrawOp, FramePlan, Interner, LayoutBox, NodeId, PassMask, PlacedGlyph,
    Reconciler, SelectionPaint, Style, Tag, ThemeMode, ThemeTokens, CARET_WIDTH_PX,
};

/// Mask bits the builder consumes (LAYOUT stays with M3, SEMANTICS with A11Y).
pub const FRAME_MASK: PassMask = PassMask::from_bits(
    PassMask::STRUCTURE.bits()
        | PassMask::STYLE.bits()
        | PassMask::PAINT.bits()
        | PassMask::TEXT.bits(),
);

/// Builds [`FramePlan`]s from the retained tree. Stateless across builds
/// (the dirty state lives core-side in the pass masks); `dpr` scales
/// CSS-px style payloads into the device-px space layout boxes use.
/// The text-selection overlay ([`SelectionPaint`]) is build-scoped state
/// (Round 8.2, decision 298 — set per frame from the host's focused
/// session; `None` paints exactly the pre-8.2 plan, so every existing
/// caller and oracle row is untouched until it opts in). The caret bar
/// ([`CaretPaint`]) rides the same rule (Round 15.1, decision 312 —
/// `None` paints exactly the pre-15.1 plan). The build theme
/// ([`ThemeMode`]) rides the same rule again (theme contract round —
/// unset paints exactly the pre-contract plan: Light's `text_primary`
/// IS the contract [`oppa::render::INK`], so every existing caller
/// and oracle row is untouched until a runner publishes Dark).
pub struct FramePlanBuilder {
    dpr: f32,
    selection: std::cell::Cell<Option<SelectionPaint>>,
    caret: std::cell::Cell<Option<CaretPaint>>,
    theme: std::cell::Cell<ThemeMode>,
}

impl FramePlanBuilder {
    pub fn new(dpr: f32) -> Self {
        Self {
            dpr: dpr.max(f32::EPSILON),
            selection: std::cell::Cell::new(None),
            caret: std::cell::Cell::new(None),
            theme: std::cell::Cell::new(ThemeMode::Light),
        }
    }

    pub fn dpr(&self) -> f32 {
        self.dpr
    }

    /// Sets the build-scoped text selection (Round 8.2 — the host's
    /// focused session range; `None` disables highlight emission).
    /// Takes `&self` so paint hooks set it per frame without
    /// re-creating the builder.
    pub fn set_selection(&self, sel: Option<SelectionPaint>) {
        self.selection.set(sel);
    }

    /// The current build-scoped selection (diagnostics/tests).
    pub fn selection(&self) -> Option<SelectionPaint> {
        self.selection.get()
    }

    /// Sets the build-scoped caret bar (Round 15.1 — the host's
    /// focused caret; `None` disables caret emission). Takes `&self`
    /// so paint hooks set it per frame without re-creating the
    /// builder.
    pub fn set_caret(&self, caret: Option<CaretPaint>) {
        self.caret.set(caret);
    }

    /// The current build-scoped caret (diagnostics/tests).
    pub fn caret(&self) -> Option<CaretPaint> {
        self.caret.get()
    }

    /// Sets the build theme (theme contract round — the runner's host
    /// theme mode, published per frame like the overlays above; unset
    /// means Light, which paints exactly the pre-contract plan).
    /// Takes `&self` so paint hooks set it per frame without
    /// re-creating the builder.
    pub fn set_theme_mode(&self, mode: ThemeMode) {
        self.theme.set(mode);
    }

    /// The current build theme (diagnostics/tests).
    pub fn theme_mode(&self) -> ThemeMode {
        self.theme.get()
    }

    /// Re-bases the builder on a new DPR (Round 2.4, OQ-G10-2):
    /// same clamp rule as [`FramePlanBuilder::new`] (garbage never
    /// reaches here — callers validate loudly first). Stateless
    /// across builds, so swapping the scale dirties nothing by
    /// itself; the layout invalidation that must accompany it lives
    /// wherever the scale change is observed (the runners).
    pub fn set_dpr(&mut self, dpr: f32) {
        self.dpr = dpr.max(f32::EPSILON);
    }

    /// Incremental build: drains [`FRAME_MASK`] and rebuilds exactly the
    /// dirty subtrees. A second build with no intervening commit yields an
    /// empty plan (static UI ≈ 0 CPU on repaint).
    pub fn build_incremental(&self, rec: &mut Reconciler, styles: &Interner<Style>) -> FramePlan {
        let drained = rec.take_paint_masks(FRAME_MASK);
        let dirty: HashSet<NodeId> = drained.into_iter().map(|(id, _)| id).collect();
        self.build_with(rec, styles, Some(&dirty), false, None)
    }

    /// Incremental build painting TIME-evaluated values (M8, §9.4): same
    /// damage discipline as [`build_incremental`](Self::build_incremental),
    /// but `bg`/`opacity` resolve through the transition evaluator at
    /// `now` (live interpolations paint mid-flight; stamped recycles
    /// paint their jumped targets). The oracle's full arm must use the
    /// matching [`build_full_evaluated`](Self::build_full_evaluated) at
    /// the same `now`.
    pub fn build_incremental_evaluated(
        &self,
        rec: &mut Reconciler,
        styles: &Interner<Style>,
        eval: &oppa::TransitionEvaluator,
        now: f64,
    ) -> FramePlan {
        let drained = rec.take_paint_masks(FRAME_MASK);
        let dirty: HashSet<NodeId> = drained.into_iter().map(|(id, _)| id).collect();
        self.build_with(rec, styles, Some(&dirty), false, Some((eval, now)))
    }

    /// Full build: ignores dirty masks and emits every node. Never drains
    /// (the oracle's reference arm must not consume the discipline state).
    pub fn build_full(&self, rec: &Reconciler, styles: &Interner<Style>) -> FramePlan {
        self.build_with(rec, styles, None, true, None)
    }

    /// Full build painting TIME-evaluated values (M8: the oracle
    /// reference arm matching `build_incremental_evaluated`).
    pub fn build_full_evaluated(
        &self,
        rec: &Reconciler,
        styles: &Interner<Style>,
        eval: &oppa::TransitionEvaluator,
        now: f64,
    ) -> FramePlan {
        self.build_with(rec, styles, None, true, Some((eval, now)))
    }

    fn build_with(
        &self,
        rec: &Reconciler,
        styles: &Interner<Style>,
        dirty: Option<&HashSet<NodeId>>,
        full: bool,
        eval: Option<(&oppa::TransitionEvaluator, f64)>,
    ) -> FramePlan {
        let mut plan = FramePlan {
            full_repaint: full,
            ..FramePlan::default()
        };
        let Some(root) = rec.root() else {
            return plan;
        };
        // Ancestor closure: a node opens the walk iff it is dirty or has a
        // dirty descendant (incremental only).
        let mut open: HashSet<NodeId> = HashSet::new();
        if let Some(dirty) = dirty {
            for id in dirty {
                let mut cur = Some(*id);
                while let Some(c) = cur {
                    if !open.insert(c) {
                        break;
                    }
                    cur = rec.get(c).and_then(|n| n.parent);
                }
            }
        }
        let mut walker = Walker {
            builder: self,
            rec,
            styles,
            dirty,
            full,
            open,
            eval,
            caret: self.caret(),
            theme: self.theme_mode(),
            portal_phase: false,
            plan: &mut plan,
        };
        walker.walk(root);
        // Overlay layers paint last (Round 1.4, decision 255 — the top
        // z-layer): phase 1 skips portal subtrees without accounting
        // them; phase 2 walks outermost portals in tree order (later
        // portals emit later, so they paint on top). Clean portals in
        // a dirty frame were already counted inside their closed
        // ancestors' phase-1 skip, so only open portals walk here —
        // skipped subtrees are never double-counted.
        walker.portal_phase = true;
        for portal in rec.outermost_portals() {
            if walker.full || walker.open.contains(&portal) {
                walker.walk(portal);
            }
        }
        plan.stats.ops_emitted = plan.ops.len();
        plan
    }
}

struct Walker<'a> {
    builder: &'a FramePlanBuilder,
    rec: &'a Reconciler,
    styles: &'a Interner<Style>,
    dirty: Option<&'a HashSet<NodeId>>,
    full: bool,
    open: HashSet<NodeId>,
    /// TIME evaluator overlay + sample time (None = retained values).
    eval: Option<(&'a oppa::TransitionEvaluator, f64)>,
    /// Build-scoped caret bar (Round 15.1): snapshotted per build so
    /// the walk emits one bar even if the hook re-sets mid-build.
    caret: Option<CaretPaint>,
    /// Build theme (theme contract round): snapshotted per build so
    /// the default-ink fallback is one mode for the whole plan even
    /// if the hook re-sets mid-build.
    theme: ThemeMode,
    /// Overlay phase (Round 1.4): false walks the app tree skipping
    /// portal subtrees; true walks portal layers (top z-layer).
    portal_phase: bool,
    plan: &'a mut FramePlan,
}

impl<'a> Walker<'a> {
    fn is_dirty(&self, id: NodeId) -> bool {
        self.full || self.dirty.is_some_and(|d| d.contains(&id))
    }

    fn is_open(&self, id: NodeId) -> bool {
        self.full || self.open.contains(&id)
    }

    fn walk(&mut self, id: NodeId) {
        let Some(node) = self.rec.get(id) else { return };
        // Overlay gate (Round 1.4): phase 1 skips portal subtrees whole
        // with no accounting (phase 2 accounts them exactly once, and
        // the double-count guard in `build_with` keeps clean portals
        // inside their ancestors' skip). Nested portals emit inline
        // with their parent layer once the phase is true.
        if node.tag == Tag::Portal && !self.portal_phase {
            return;
        }
        self.plan.stats.nodes_visited += 1;
        if !self.is_open(id) {
            self.plan.stats.subtrees_skipped += 1 + count_descendants(self.rec, id);
            return;
        }
        let children = node.children.clone();
        let style_id = node.style;
        let tag = node.tag;
        // Round 4.4: the retained image id threads into the op (the
        // backend still refuses without pixels — now naming the real
        // id; `None` keeps the legacy dummy for hand-built nodes).
        let image = node.image;
        let box_opt = node.layout.clone();

        let style: Style = self.styles.get(style_id).cloned().unwrap_or_default();
        let dpr = self.builder.dpr;

        if self.is_dirty(id) {
            match box_opt {
                Some(b) => {
                    self.plan.damage.push(DamageRect {
                        x: b.x,
                        y: b.y,
                        w: b.w,
                        h: b.h,
                    });
                    // ScrollArea viewports clip their content (M4: emitted
                    // whenever the viewport node rebuilds).
                    let clips = tag == Tag::ScrollArea;
                    if clips {
                        self.plan.ops.push(DrawOp::PushClip {
                            x: b.x,
                            y: b.y,
                            w: b.w,
                            h: b.h,
                        });
                    }
                    emit_node_ops(
                        self.plan,
                        self.rec,
                        self.styles,
                        &style,
                        &b,
                        id,
                        tag,
                        image,
                        dpr,
                        self.eval,
                        self.builder.selection(),
                        self.caret,
                        self.theme,
                    );
                    for child in &children {
                        self.walk(*child);
                    }
                    if clips {
                        self.plan.ops.push(DrawOp::Pop);
                    }
                }
                None => {
                    self.plan.stats.nodes_unboxed += 1;
                    for child in &children {
                        self.walk(*child);
                    }
                }
            }
        } else {
            // Open but clean: emits nothing itself, still walks children
            // (a dirty descendant may hide below).
            for child in &children {
                self.walk(*child);
            }
        }
    }
}

fn count_descendants(rec: &Reconciler, id: NodeId) -> usize {
    let mut n = 0;
    let mut stack: Vec<NodeId> = rec.get(id).map(|x| x.children.clone()).unwrap_or_default();
    while let Some(c) = stack.pop() {
        n += 1;
        if let Some(node) = rec.get(c) {
            stack.extend(node.children.iter().copied());
        }
    }
    n
}

/// Opacity of a style payload (v1: baked into every emitted op; the
/// `PushLayer` stack exists for GPU-future plans and is honored by the
/// backend, but this builder never emits it).
fn opacity_of(style: &Style) -> f32 {
    style.opacity.map(|p| p.get()).unwrap_or(1.0)
}

/// Per-corner radii in device px, CSS order `[tl, tr, br, bl]`
/// (Round 11.1, decision 305): `Some` exactly when the style sets
/// per-corner radii (each resolved against the uniform shorthand,
/// then clamped to half the given box — the one shared rule every
/// backend paints, so corners agree by construction and can never
/// overflow the box). `None` is the uniform path (possibly no
/// rounding at all) — pre-11.1 plans byte-identical.
fn resolve_radii(style: &Style, w: f32, h: f32, dpr: f32) -> Option<[f32; 4]> {
    let corners = style.corner_radii()?;
    let cap = (w.min(h) * 0.5).max(0.0);
    Some(corners.map(|c| (c.get() * dpr).min(cap).max(0.0)))
}

#[allow(clippy::too_many_arguments)]
fn emit_node_ops(
    plan: &mut FramePlan,
    rec: &Reconciler,
    styles: &Interner<Style>,
    style: &Style,
    b: &LayoutBox,
    id: NodeId,
    tag: Tag,
    image: Option<oppa::ImageId>,
    dpr: f32,
    eval: Option<(&oppa::TransitionEvaluator, f64)>,
    selection: Option<SelectionPaint>,
    caret: Option<CaretPaint>,
    theme: ThemeMode,
) {
    // M8 evaluated values: live interpolations paint mid-flight, stamped
    // recycles paint their jumped targets, everything else paints the
    // retained values (the overlay is identity off the evaluator's
    // tracked set).
    let (bg, opacity) = match eval {
        Some((e, now)) => (
            e.resolve_bg(id, style.bg, now),
            e.resolve_opacity(id, opacity_of(style), now),
        ),
        None => (style.bg, opacity_of(style)),
    };
    let mut style_evaluated;
    let style_ref: &Style = match eval {
        Some(_) => {
            style_evaluated = style.clone();
            style_evaluated.bg = bg;
            style_evaluated.opacity = Some(oppa::Px::of(opacity));
            &style_evaluated
        }
        None => style,
    };
    let opacity = opacity_of(style_ref);
    if opacity <= 0.0 {
        return;
    }
    if tag == Tag::Image {
        // No decoded pixels in v1 (async decode unscoped) — the op records
        // the intent with the retained id; the backend refuses it loudly.
        // (`None` — a hand-built Image node without `Img` — keeps the
        // legacy dummy id, refused downstream just as loudly.)
        plan.ops.push(DrawOp::RImg {
            node: id,
            x: b.x,
            y: b.y,
            w: b.w,
            h: b.h,
            image: image.unwrap_or(oppa::ImageId(0)),
        });
        return;
    }
    // Vector leaf (decision 291): style paint fields never apply to a
    // path (its paint rides the payload) — any of them set is an
    // authoring bug and refuses loudly, so no backend can silently
    // diverge on it. The path op carries the committed box origin +
    // size with the retained payload.
    if tag == Tag::Path {
        emit_path_op(plan, rec, id, b, style_ref, opacity);
        return;
    }
    if let Some(shadow) = &style_ref.shadow {
        emit_shadow(plan, id, b, shadow, dpr);
    }
    // Round 1.3 (decision 254): conflicting paint specs refuse loudly —
    // the builder cannot know which fill the author meant, so ambiguity
    // is a spec bug, never a precedence guess. Validated before the
    // opacity early-return so invisible subtrees stay loud too.
    if let Some(shadow) = &style_ref.shadow {
        let blur = shadow.blur.get();
        if !blur.is_finite() {
            panic!("plan: shadow blur is non-finite ({blur}) — NaN/Inf never paints silently");
        }
        if blur < 0.0 {
            panic!("plan: shadow blur is negative ({blur}) — negative blur never paints silently");
        }
    }
    if style_ref.border.is_some() && style_ref.border_edges.is_some() {
        panic!("plan: border + border_edges both set — ambiguous ring spec; set exactly one");
    }
    if style_ref.bg_gradient.is_some() && style_ref.bg.is_some() {
        panic!("plan: bg + bg_gradient both set — gradient replaces bg; set exactly one");
    }
    if style_ref.bg_gradient.is_some() && style_ref.has_any_radius() {
        panic!("plan: bg_gradient with radius/circle — sharp strips vs round shape (follow-up)");
    }
    // Inset border ring (M5 `Style::border`): an outer fill in the
    // border color plus an inset background fill, reusing the existing
    // shape ops (no new DrawOp, no backend change). Paint-only and
    // inset, so layout never moves. Round 1.3: `border_edges` paints
    // four sharp bands over the fill instead (validated + emitted
    // below); radius/circle with edges refuses loudly (sharp bands vs
    // round shape), while the uniform ring keeps honoring them.
    let border_w = style_ref.border.and_then(|bd| {
        if bd.color != oppa::Color::TRANSPARENT
            && bd.width.get() > 0.0
            && b.w > 2.0 * bd.width.get()
            && b.h > 2.0 * bd.width.get()
        {
            Some((bd.width.get() * dpr, bd.color))
        } else {
            None
        }
    });
    let edge_bands = style_ref
        .border_edges
        .and_then(|e| resolve_edge_bands(e, dpr));
    if edge_bands.is_some() && (style_ref.has_any_radius()) {
        panic!("plan: border_edges with radius/circle — sharp bands vs round shape (follow-up)");
    }
    if let Some(g) = style_ref.bg_gradient {
        if b.w > 0.0 && b.h > 0.0 {
            if let Some((w, border_color)) = border_w {
                // Uniform-ring outer + inset gradient (mirrors the solid
                // ring path below: outer fill, then the fill inset).
                emit_shape(
                    plan,
                    style_ref,
                    b,
                    id,
                    tag,
                    border_color,
                    opacity,
                    dpr,
                    0.0,
                    0.0,
                    b.w,
                    b.h,
                );
                emit_gradient(
                    plan,
                    id,
                    b.x + w,
                    b.y + w,
                    b.w - 2.0 * w,
                    b.h - 2.0 * w,
                    g,
                    opacity,
                );
            } else {
                emit_gradient(plan, id, b.x, b.y, b.w, b.h, g, opacity);
            }
        }
    } else if let Some(bg) = style_ref.bg {
        if bg != oppa::Color::TRANSPARENT && b.w > 0.0 && b.h > 0.0 {
            if let Some((w, border_color)) = border_w {
                emit_shape(
                    plan,
                    style_ref,
                    b,
                    id,
                    tag,
                    border_color,
                    opacity,
                    dpr,
                    0.0,
                    0.0,
                    b.w,
                    b.h,
                );
                emit_shape(
                    plan,
                    style_ref,
                    b,
                    id,
                    tag,
                    bg,
                    opacity,
                    dpr,
                    w,
                    w,
                    b.w - 2.0 * w,
                    b.h - 2.0 * w,
                );
            } else if style_ref.circle {
                plan.ops.push(DrawOp::Circle {
                    node: id,
                    cx: b.x + b.w * 0.5,
                    cy: b.y + b.h * 0.5,
                    r: b.w.min(b.h) * 0.5,
                    color: bg,
                    opacity,
                });
            } else if let Some(radii) = resolve_radii(style_ref, b.w, b.h, dpr) {
                // Round 11.1: per-corner radii ride `RRect.radii`
                // (the uniform `radius` below stays the resolved
                // shorthand for info — backends branch on `radii`).
                plan.ops.push(DrawOp::RRect {
                    node: id,
                    x: b.x,
                    y: b.y,
                    w: b.w,
                    h: b.h,
                    radius: style_ref.radius.map(|r| r.get() * dpr).unwrap_or(0.0),
                    radii: Some(radii),
                    color: bg,
                    opacity,
                });
            } else if let Some(r) = style_ref.radius {
                let radius = r.get() * dpr;
                if radius > 0.0 {
                    plan.ops.push(DrawOp::RRect {
                        node: id,
                        x: b.x,
                        y: b.y,
                        w: b.w,
                        h: b.h,
                        radius,
                        radii: None,
                        color: bg,
                        opacity,
                    });
                } else {
                    plan.ops.push(DrawOp::Rect {
                        node: id,
                        x: b.x,
                        y: b.y,
                        w: b.w,
                        h: b.h,
                        color: bg,
                        opacity,
                    });
                }
            } else {
                plan.ops.push(DrawOp::Rect {
                    node: id,
                    x: b.x,
                    y: b.y,
                    w: b.w,
                    h: b.h,
                    color: bg,
                    opacity,
                });
            }
        }
    }
    // Per-edge inset bands overlay the fill (inset like the ring, never
    // layout — bands wider than the box clamp to it, never overflow it).
    if let Some(([top, right, bottom, left], color)) = edge_bands {
        if b.w > 0.0 && b.h > 0.0 {
            emit_edge_bands(plan, id, b, [top, right, bottom, left], color, opacity);
        }
    }
    if tag == Tag::Text {
        // Round 8.2 (decision 298): the themed selection rectangle —
        // one `Rect` per overlapping laid line, emitted BEFORE the
        // line's `Text` op (background order) through the shared
        // `selection_rects` rule, so CPU and Vello agree by
        // construction and no backend carries selection logic.
        if let Some(sel) = selection {
            if sel.range.0 != sel.range.1 && oppa::input::is_within(rec, id, sel.field) {
                for r in b.selection_rects(sel.range) {
                    let (x0, y0, x1, y1) = (r[0], r[1], r[2], r[3]);
                    if x1 > x0 && y1 > y0 {
                        plan.ops.push(DrawOp::Rect {
                            node: id,
                            x: x0,
                            y: y0,
                            w: x1 - x0,
                            h: y1 - y0,
                            color: oppa::SELECTION_FILL,
                            opacity,
                        });
                    }
                }
            }
        }
        for line in &b.lines {
            // Flatten runs into cells while recording the per-run font
            // identity (M7, decision 110): consecutive same-font runs
            // merge so fallback boundaries — not cluster boundaries —
            // are what the op carries.
            let mut glyphs: Vec<PlacedGlyph> = Vec::new();
            let mut fonts: Vec<oppa::FontRun> = Vec::new();
            for run in &line.runs {
                if run.glyphs.is_empty() {
                    continue;
                }
                let base = glyphs.len();
                glyphs.extend(run.glyphs.iter().map(|g| PlacedGlyph {
                    glyph_id: g.glyph_id,
                    x: g.x,
                    advance: g.x_advance,
                }));
                let span = (base, glyphs.len());
                match fonts.last_mut() {
                    Some(last) if last.family == run.family && last.font_id == run.font_id => {
                        last.glyph_range.1 = span.1
                    }
                    _ => fonts.push(oppa::FontRun {
                        glyph_range: span,
                        family: run.family.clone(),
                        font_id: run.font_id,
                    }),
                }
            }
            if glyphs.is_empty() {
                continue;
            }
            // Glyph cells carry the layout's pre-positioned advances into
            // the backend; the backend fills cells, never re-shapes.
            // `baseline` rides along for the GPU backend (M6 decision
            // 105); the CPU backend ignores it, `em_size`/`fonts` alike
            // (cells unchanged).
            plan.ops.push(DrawOp::Text {
                node: id,
                x: b.x,
                y: b.y + line.y,
                line_height: line.height,
                baseline: line.baseline,
                em_size: line.em_size,
                glyphs,
                fonts,
                ink: resolve_ink(rec, styles, id, style, theme),
                opacity,
            });
        }
    }
    // Round 15.1 (decision 312): the focused caret bar — one
    // `CARET_WIDTH_PX`-wide vertical `Rect` in the field's text ink,
    // emitted exactly once on the field container itself (after its
    // text, so the bar paints above the glyphs). Degenerate heights
    // paint nothing (the host already refuses them; this is the
    // belt-and-braces, mirroring the selection's empty-overlap rule).
    if let Some(c) = caret {
        if c.field == id && c.h > 0.0 {
            plan.ops.push(DrawOp::Rect {
                node: id,
                x: c.x,
                y: c.y,
                w: CARET_WIDTH_PX,
                h: c.h,
                color: c.color,
                opacity,
            });
        }
    }
}

/// Vector emission (decision 291): one [`DrawOp::Path`] per path
/// node — the committed box origin/size plus the retained
/// [`PathSpec`](oppa::PathSpec). Style paint fields (`bg`, rings,
/// gradients, shadows, shape flags, ink) never apply to paths: path
/// paint rides `data`/`fill`/`stroke` only, so any of them set is an
/// authoring bug and refuses loudly here (no backend can diverge on
/// it). Payload gaps (no spec, blank data, neither fill nor stroke,
/// bad stroke width) refuse loudly too — [`Path::build`](oppa::Path)
/// already enforces these for component-built nodes; this arm covers
/// hand-built `Tag::Path` elements with the same loudness.
fn emit_path_op(
    plan: &mut FramePlan,
    rec: &Reconciler,
    id: NodeId,
    b: &LayoutBox,
    style: &Style,
    opacity: f32,
) {
    if style.bg.is_some() {
        panic!(
            "plan: bg on a Path node {id:?} — path paint rides data/fill/stroke, never style bg"
        );
    }
    if style.border.is_some() || style.border_edges.is_some() {
        panic!("plan: border on a Path node {id:?} — path paint rides data/fill/stroke, never style rings");
    }
    if style.bg_gradient.is_some() {
        panic!("plan: bg_gradient on a Path node {id:?} — path paint rides data/fill/stroke, never style gradients");
    }
    if style.shadow.is_some() {
        panic!("plan: shadow on a Path node {id:?} — path paint rides data/fill/stroke, never style shadows");
    }
    if style.circle || style.has_any_radius() {
        panic!("plan: radius/circle on a Path node {id:?} — paths carry their own geometry, never style shape flags");
    }
    if style.ink.is_some() {
        panic!(
            "plan: ink on a Path node {id:?} — path paint rides data/fill/stroke, never style ink"
        );
    }
    let Some(spec) = rec.get(id).and_then(|n| n.path.clone()) else {
        panic!("plan: Tag::Path node {id:?} without a path payload — Path nodes carry PathSpec (hand-built Path elements must set it)");
    };
    if spec.data.trim().is_empty() {
        panic!("plan: Tag::Path node {id:?} has blank path data — refused, never a silent no-op");
    }
    if spec.fill.is_none() && spec.stroke.is_none() {
        panic!("plan: Tag::Path node {id:?} has neither fill nor stroke — refused, never a silent no-op");
    }
    if let Some(s) = spec.stroke {
        if !s.width.is_finite() {
            panic!(
                "plan: Tag::Path node {id:?} stroke width is non-finite ({}) — NaN/Inf never paints silently",
                s.width
            );
        }
        if s.width < 0.0 {
            panic!(
                "plan: Tag::Path node {id:?} stroke width is negative ({}) — negative widths never paint silently",
                s.width
            );
        }
    }
    plan.ops.push(DrawOp::Path {
        node: id,
        x: b.x,
        y: b.y,
        width: b.w,
        height: b.h,
        data: spec.data,
        fill: spec.fill,
        stroke: spec.stroke,
        opacity,
    });
}

/// Shadow emission (Round 1.3, decision 254): blur 0 keeps the shipped
/// offset-solid [`DrawOp::Shadow`]; blur > 0 expands into `ceil(blur)`
/// 1-device-px stepped [`DrawOp::Rect`]s — step `i` grows the offset box
/// by `i` px on every side with opacity `1 - i/n` (linear falloff).
/// Shared builder code, solid rects only: every backend paints the same
/// pixels by construction (the strict-geometry oracle proves the
/// compositing half). Blur validity is checked by the caller.
fn emit_shadow(plan: &mut FramePlan, id: NodeId, b: &LayoutBox, shadow: &oppa::Shadow, dpr: f32) {
    let blur_dev = shadow.blur.get() * dpr;
    let dx = shadow.x.get() * dpr;
    let dy = shadow.y.get() * dpr;
    if blur_dev <= 0.0 {
        plan.ops.push(DrawOp::Shadow {
            node: id,
            x: b.x,
            y: b.y,
            w: b.w,
            h: b.h,
            dx,
            dy,
            color: shadow.color,
        });
        return;
    }
    let n = blur_dev.ceil().max(1.0) as usize;
    for i in 0..n {
        let grow = i as f32;
        plan.ops.push(DrawOp::Rect {
            node: id,
            x: b.x + dx - grow,
            y: b.y + dy - grow,
            w: b.w + 2.0 * grow,
            h: b.h + 2.0 * grow,
            color: shadow.color,
            opacity: 1.0 - i as f32 / n as f32,
        });
    }
}

/// Validates per-edge border widths (NaN/negative refuse loudly) and
/// resolves them to device px. Returns None when nothing would paint
/// (transparent color or all-zero widths).
fn resolve_edge_bands(e: oppa::BorderEdges, dpr: f32) -> Option<([f32; 4], oppa::Color)> {
    for (name, w) in [
        ("top", e.top),
        ("right", e.right),
        ("bottom", e.bottom),
        ("left", e.left),
    ] {
        let v = w.get();
        if !v.is_finite() {
            panic!("plan: border_edges.{name} is non-finite ({v}) — NaN/Inf never paints silently");
        }
        if v < 0.0 {
            panic!(
                "plan: border_edges.{name} is negative ({v}) — negative bands never paint silently"
            );
        }
    }
    let widths = [
        e.top.get() * dpr,
        e.right.get() * dpr,
        e.bottom.get() * dpr,
        e.left.get() * dpr,
    ];
    if e.color == oppa::Color::TRANSPARENT || widths.iter().all(|w| *w <= 0.0) {
        None
    } else {
        Some((widths, e.color))
    }
}

/// Per-edge inset bands: top/bottom span the full width, left/right sit
/// between them (no double-painted corners — sequential src-over would
/// still agree cross-backend, but non-overlap keeps single-backend
/// pixels obvious too). Bands clamp to the box, never overflow it.
fn emit_edge_bands(
    plan: &mut FramePlan,
    id: NodeId,
    b: &LayoutBox,
    edges: [f32; 4],
    color: oppa::Color,
    opacity: f32,
) {
    let [top, right, bottom, left] = edges;
    let top = top.min(b.h);
    let bottom = bottom.min(b.h);
    let left = left.min(b.w);
    let right = right.min(b.w);
    if top > 0.0 {
        plan.ops.push(DrawOp::Rect {
            node: id,
            x: b.x,
            y: b.y,
            w: b.w,
            h: top,
            color,
            opacity,
        });
    }
    if bottom > 0.0 {
        plan.ops.push(DrawOp::Rect {
            node: id,
            x: b.x,
            y: b.y + b.h - bottom,
            w: b.w,
            h: bottom,
            color,
            opacity,
        });
    }
    let mid_h = (b.h - top - bottom).max(0.0);
    if mid_h > 0.0 {
        if left > 0.0 {
            plan.ops.push(DrawOp::Rect {
                node: id,
                x: b.x,
                y: b.y + top,
                w: left,
                h: mid_h,
                color,
                opacity,
            });
        }
        if right > 0.0 {
            plan.ops.push(DrawOp::Rect {
                node: id,
                x: b.x + b.w - right,
                y: b.y + top,
                w: right,
                h: mid_h,
                color,
                opacity,
            });
        }
    }
}

/// Two-stop linear gradient as 1-device-px solid strips (Round 1.3,
/// decision 254): `n = round(span)` strips along the axis, strip colors
/// lerped in sRGB and rounded to the nearest step. Shared builder code
/// over the same solid-rect path the strict oracle proves exact, so all
/// backends agree by construction; native interpolation stays a
/// follow-up. The final edge is exact (no drift sliver).
#[allow(clippy::too_many_arguments)] // one call site; a params struct buys nothing
fn emit_gradient(
    plan: &mut FramePlan,
    id: NodeId,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    g: oppa::LinearGradient,
    opacity: f32,
) {
    let span = if g.horizontal { w } else { h };
    if span <= 0.0 || w <= 0.0 || h <= 0.0 {
        return;
    }
    let n = span.round().max(1.0) as usize;
    let base = if g.horizontal { x } else { y };
    let mut prev = base;
    for k in 0..n {
        let edge = if k + 1 == n {
            base + span
        } else {
            base + span * (k + 1) as f32 / n as f32
        };
        let color = lerp_color(g.from, g.to, (k as f32 + 0.5) / n as f32);
        let (sx, sy, sw, sh) = if g.horizontal {
            (prev, y, edge - prev, h)
        } else {
            (x, prev, w, edge - prev)
        };
        if edge > prev {
            plan.ops.push(DrawOp::Rect {
                node: id,
                x: sx,
                y: sy,
                w: sw,
                h: sh,
                color,
                opacity,
            });
        }
        prev = edge;
    }
}

/// sRGB channel lerp rounded to the nearest step (`Color` is opaque
/// `0xRRGGBB` — alpha never interpolates).
fn lerp_color(from: oppa::Color, to: oppa::Color, t: f32) -> oppa::Color {
    let channel = |shift: u32| {
        let a = ((from.0 >> shift) & 0xFF) as f32;
        let b = ((to.0 >> shift) & 0xFF) as f32;
        (a + (b - a) * t).round().clamp(0.0, 255.0) as u32
    };
    oppa::Color((channel(16) << 16) | (channel(8) << 8) | channel(0))
}

/// Effective text ink for a text node (M5 `Style::ink` + the theme
/// contract round): the node's own override, else the nearest
/// ancestor's, else the build theme's `text_primary` (Light's is the
/// contract [`oppa::render::INK`], so unset builds paint exactly the
/// pre-contract plan). Inheritance (not per-leaf authoring — the
/// `Text` leaf conversion carries no style slot) is the stated rule;
/// the walk is presenter-side reads only.
fn resolve_ink(
    rec: &Reconciler,
    styles: &Interner<Style>,
    id: NodeId,
    style: &Style,
    theme: ThemeMode,
) -> oppa::Color {
    if let Some(ink) = style.ink {
        return ink;
    }
    let mut cur = rec.get(id).and_then(|n| n.parent);
    while let Some(c) = cur {
        if let Some(ink) = rec
            .get(c)
            .and_then(|n| styles.get(n.style))
            .and_then(|s| s.ink)
        {
            return ink;
        }
        cur = rec.get(c).and_then(|n| n.parent);
    }
    ThemeTokens::of(theme).text_primary
}

/// One background fill at `(b.x + dx, b.y + dy, w, h)` in `color`,
/// honoring the style's circle/radius shape (the border ring's two
/// fills share this path; the inner fill shrinks the radius by the
/// ring width, floored at zero).
#[allow(clippy::too_many_arguments)]
fn emit_shape(
    plan: &mut FramePlan,
    style: &Style,
    b: &LayoutBox,
    id: NodeId,
    tag: Tag,
    color: oppa::Color,
    opacity: f32,
    dpr: f32,
    dx: f32,
    dy: f32,
    w: f32,
    h: f32,
) {
    let _ = tag;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let (x, y) = (b.x + dx, b.y + dy);
    if style.circle {
        plan.ops.push(DrawOp::Circle {
            node: id,
            cx: x + w * 0.5,
            cy: y + h * 0.5,
            r: w.min(h) * 0.5,
            color,
            opacity,
        });
    } else if let Some(radii) = resolve_radii(style, w, h, dpr) {
        // Round 11.1: the inset ring keeps per-corner shapes (each
        // corner shrinks by the inset, like the uniform rule below).
        let inset = dx.min(dy);
        let radii = radii.map(|r| (r - inset).max(0.0));
        if radii.iter().any(|r| *r > 0.0) {
            plan.ops.push(DrawOp::RRect {
                node: id,
                x,
                y,
                w,
                h,
                radius: 0.0,
                radii: Some(radii),
                color,
                opacity,
            });
        } else {
            plan.ops.push(DrawOp::Rect {
                node: id,
                x,
                y,
                w,
                h,
                color,
                opacity,
            });
        }
    } else if let Some(r) = style.radius {
        let radius = (r.get() * dpr - dx.min(dy)).max(0.0);
        if radius > 0.0 {
            plan.ops.push(DrawOp::RRect {
                node: id,
                x,
                y,
                w,
                h,
                radius,
                radii: None,
                color,
                opacity,
            });
        } else {
            plan.ops.push(DrawOp::Rect {
                node: id,
                x,
                y,
                w,
                h,
                color,
                opacity,
            });
        }
    } else {
        plan.ops.push(DrawOp::Rect {
            node: id,
            x,
            y,
            w,
            h,
            color,
            opacity,
        });
    }
}

/// Resolved-style sharing check (locked #8 stays): equal payloads intern
/// to one id, so the builder resolves through the table per node.
pub fn style_rule_count(styles: &Interner<Style>) -> usize {
    styles.len()
}

/// NodeId→style-id view used by backend-fidelity tests (which node paints
/// under which shared rule).
pub fn node_styles(rec: &Reconciler) -> HashMap<NodeId, oppa::StyleId> {
    rec.retained_ids()
        .into_iter()
        .filter_map(|id| rec.get(id).map(|n| (id, n.style)))
        .collect()
}
