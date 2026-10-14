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

mod exit;
mod leaf;
mod node;
mod resolve;
mod shape;
mod xform;

pub use resolve::{node_styles, style_rule_count};
pub use xform::{bake_ops, scale_path_data};

use std::collections::HashSet;

use oppa::{
    AffineST, CaretPaint, DamageRect, DrawOp, FramePlan, Interner, NodeId, PaintXf, PassMask,
    Reconciler, Style, Tag, ThemeMode,
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
/// The text-selection overlay ([`SelectionPaint`](oppa::SelectionPaint)) is build-scoped state
/// (Round 8.2, decision 298 — set per frame from the host's focused
/// session; `None` paints exactly the pre-8.2 plan, so every existing
/// caller and oracle row is untouched until it opts in). The caret bar
/// ([`CaretPaint`]) rides the same rule (Round 15.1, decision 312 —
/// `None` paints exactly the pre-15.1 plan). The selection handles
/// ([`HandlePaint`](oppa::HandlePaint)) ride the same rule again
/// (decision 454 — `None` paints exactly the pre-454 plan). The build theme
/// ([`ThemeMode`]) rides the same rule again (theme contract round —
/// unset paints exactly the pre-contract plan: Light's `text_primary`
/// IS the contract [`oppa::render::INK`], so every existing caller
/// and oracle row is untouched until a runner publishes Dark).
pub struct FramePlanBuilder {
    dpr: f32,
    selection: std::cell::Cell<Option<oppa::SelectionPaint>>,
    caret: std::cell::Cell<Option<CaretPaint>>,
    handles: std::cell::Cell<Option<(oppa::HandlePaint, oppa::HandlePaint)>>,
    theme: std::cell::Cell<ThemeMode>,
    theme_tokens: std::cell::Cell<Option<oppa::ThemeTokens>>,
    /// Deferred-exit snapshots (decision 520 — published per frame
    /// like the overlays above; empty paints exactly the pre-520
    /// plan, so every existing caller and oracle row is untouched
    /// until a removal pins).
    exits: std::cell::RefCell<Vec<oppa::ExitPin>>,
}

impl FramePlanBuilder {
    pub fn new(dpr: f32) -> Self {
        Self {
            dpr: dpr.max(f32::EPSILON),
            selection: std::cell::Cell::new(None),
            caret: std::cell::Cell::new(None),
            handles: std::cell::Cell::new(None),
            theme: std::cell::Cell::new(ThemeMode::Light),
            theme_tokens: std::cell::Cell::new(None),
            exits: std::cell::RefCell::new(Vec::new()),
        }
    }

    pub fn dpr(&self) -> f32 {
        self.dpr
    }

    /// Sets the build-scoped text selection (Round 8.2 — the host's
    /// focused session range; `None` disables highlight emission).
    /// Takes `&self` so paint hooks set it per frame without
    /// re-creating the builder.
    pub fn set_selection(&self, sel: Option<oppa::SelectionPaint>) {
        self.selection.set(sel);
    }

    /// The current build-scoped selection (diagnostics/tests).
    pub fn selection(&self) -> Option<oppa::SelectionPaint> {
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

    /// Sets the build-scoped selection handles (decision 454 — the
    /// host's focused handle pair; `None` disables handle emission).
    /// Takes `&self` so paint hooks set it per frame without
    /// re-creating the builder.
    pub fn set_handles(&self, handles: Option<(oppa::HandlePaint, oppa::HandlePaint)>) {
        self.handles.set(handles);
    }

    /// The current build-scoped handles (diagnostics/tests).
    pub fn handles(&self) -> Option<(oppa::HandlePaint, oppa::HandlePaint)> {
        self.handles.get()
    }

    /// Sets the build theme (theme contract round — the runner's host
    /// theme mode, published per frame like the overlays above; unset
    /// means Light, which paints exactly the pre-contract plan).
    /// Takes `&self` so paint hooks set it per frame without
    /// re-creating the builder.
    pub fn set_theme_mode(&self, mode: ThemeMode) {
        self.theme.set(mode);
    }

    /// Sets the build palette override (decision 477 — the host's
    /// override-honoring tokens, published per frame beside the mode;
    /// `None` resolves through the mode exactly as before, so every
    /// existing caller and oracle row is untouched until a runner
    /// publishes custom tokens). Takes `&self` like every other
    /// per-frame publish.
    pub fn set_theme_tokens(&self, tokens: oppa::ThemeTokens) {
        self.theme_tokens.set(Some(tokens));
    }

    /// Clears the build palette override (diagnostics/tests).
    pub fn clear_theme_tokens(&self) {
        self.theme_tokens.set(None);
    }

    /// Sets the deferred-exit snapshots (decision 520 — the host's
    /// live pins, published per frame like the overlays above).
    /// Takes `&self` so paint hooks set it per frame without
    /// re-creating the builder.
    pub fn set_exits(&self, exits: Vec<oppa::ExitPin>) {
        *self.exits.borrow_mut() = exits;
    }

    /// The current deferred exits (diagnostics/tests).
    pub fn exits(&self) -> Vec<oppa::ExitPin> {
        self.exits.borrow().clone()
    }

    /// The current build palette override (diagnostics/tests).
    pub fn theme_tokens(&self) -> Option<oppa::ThemeTokens> {
        self.theme_tokens.get()
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
    /// but `bg`/`opacity`/`transform` resolve through the transition
    /// evaluator at `now` (live interpolations paint mid-flight;
    /// stamped recycles paint their jumped targets). The oracle's full
    /// arm must use the matching
    /// [`build_full_evaluated`](Self::build_full_evaluated) at the same
    /// `now`.
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
        // Decision 520: the exit layer runs even for empty trees — a
        // dismissed overlay leaves no live root behind its fading
        // ghost, and the ghost still paints (the early return below
        // covers the live walk only, never the exits).
        let root = rec.root();
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
            handles: self.handles(),
            // Decision 477: the palette snapshot rides beside the
            // mode (override when a runner published custom tokens,
            // else the built-in palette — unset paints exactly the
            // pre-477 plan).
            tokens: self
                .theme_tokens()
                .unwrap_or(oppa::ThemeTokens::of(self.theme_mode())),
            portal_phase: false,
            plan: &mut plan,
        };
        // No live root (unmounted tree): the live walk and the portal
        // phase go quiet — the exit layer below still paints ghosts.
        if let Some(root) = root {
            walker.walk(root, AffineST::identity(), PaintXf::identity());
        }
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
                // Decision 511: portal entries reset transform
                // accumulation (ancestors above a portal never move
                // its content — the DOM rule, where portal content
                // lives outside transformed ancestors' elements).
                walker.walk(portal, AffineST::identity(), PaintXf::identity());
            }
        }
        // Decision 520: deferred exits paint after portals (ghosts
        // topmost). Raw builds skip the layer (removal-instant
        // reference semantic); evaluated builds fade through it.
        // No-op without pins — quiet trees keep byte-identical plans.
        let tokens = walker.tokens;
        drop(walker);
        if let Some((e, now)) = eval {
            let exits = self.exits.borrow();
            if !exits.is_empty() {
                exit::emit_exit_layer(
                    &mut plan,
                    rec,
                    styles,
                    Some((e, now)),
                    &exits,
                    self.dpr,
                    tokens,
                );
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
    /// Build-scoped selection handles (decision 454): snapshotted per
    /// build like the caret above (one pair per plan, never doubled).
    handles: Option<(oppa::HandlePaint, oppa::HandlePaint)>,
    /// Build palette (decision 477): snapshotted per build
    /// (override when published, else the built-in mode palette),
    /// so custom tokens apply atomically to the whole plan even if
    /// the hook re-sets mid-build; unset paints exactly the
    /// pre-477 plan.
    tokens: oppa::ThemeTokens,
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

    fn walk(&mut self, id: NodeId, acc: AffineST, acc_full: PaintXf) {
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
                    // Decision 511: this node's paint transform
                    // (evaluator-live else retained) composes with the
                    // ancestry — damage covers the transformed AABB,
                    // ops bake the full accumulated affine. Decision
                    // 517: the ST bake stays byte-identical AND a full
                    // affine accumulates beside it; the per-op residual
                    // (`full ∘ st⁻¹`, identity for ST-only chains)
                    // rides each op to the backends at paint time.
                    let own = match self.eval {
                        Some((e, now)) => e.resolve_transform(id, style.transform, now),
                        None => style.transform.unwrap_or(oppa::Transform::identity()),
                    };
                    let (node_acc, node_full) = if own.is_identity() {
                        (acc, acc_full)
                    } else {
                        own.require_supported();
                        let cx = b.x + b.w * 0.5;
                        let cy = b.y + b.h * 0.5;
                        (
                            AffineST::compose(acc, own.to_local_affine(cx, cy)),
                            PaintXf::compose(acc_full, own.to_full_affine(cx, cy)),
                        )
                    };
                    let residual = PaintXf::residual(node_full, node_acc);
                    let (dx, dy, dw, dh) = if residual.is_identity() {
                        let (x0, y0) = node_acc.map_point(b.x, b.y);
                        (x0, y0, node_acc.sx * b.w, node_acc.sy * b.h)
                    } else {
                        node_full.map_rect_aabb(b.x, b.y, b.w, b.h)
                    };
                    self.plan.damage.push(DamageRect {
                        x: dx,
                        y: dy,
                        w: dw,
                        h: dh,
                    });
                    // ScrollArea viewports clip their content (M4: emitted
                    // whenever the viewport node rebuilds).
                    let clips = tag == Tag::ScrollArea;
                    if clips {
                        self.plan.ops.push(DrawOp::PushClip {
                            x: dx,
                            y: dy,
                            w: dw,
                            h: dh,
                        });
                    }
                    let start = self.plan.ops.len();
                    node::emit_node_ops(
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
                        self.handles,
                        self.tokens,
                        None,
                    );
                    xform::bake_ops(&mut self.plan.ops[start..], node_acc, residual);
                    for child in &children {
                        self.walk(*child, node_acc, node_full);
                    }
                    if clips {
                        self.plan.ops.push(DrawOp::Pop);
                    }
                }
                None => {
                    self.plan.stats.nodes_unboxed += 1;
                    for child in &children {
                        self.walk(*child, acc, acc_full);
                    }
                }
            }
        } else {
            // Open but clean: emits nothing itself, still walks children
            // (a dirty descendant may hide below).
            for child in &children {
                self.walk(*child, acc, acc_full);
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

#[cfg(test)]
mod tests;
