// ---------------------------------------------------------------------------
// Engine + ledger + top-level run + pass context
// ---------------------------------------------------------------------------

use super::*;

// ---------------------------------------------------------------------------
// Engine + ledger (one-frame-delay wiring)
// ---------------------------------------------------------------------------

/// The engine: config plus amortized caches (ellipsis advances per style).
/// Owned by [`LayoutLedger`]; never in hot crates (lock #25 residence).
pub struct LayoutEngine {
    pub(crate) config: LayoutTextConfig,
    pub(crate) ellipsis_widths: HashMap<(u64, u32, u32, u16), f32>,
    /// Shaper-local [`FontId`] → backend-resolvable family name, built
    /// from the installed [`TextService`]'s enumeration (M7, decision
    /// 110). The font set is stable per decision 16, so the table
    /// refreshes only while empty; ids the shaper reports but the table
    /// never named resolve to the requested family (stated fallback —
    /// e.g. headless fakes with an empty enumeration).
    pub(crate) font_names: HashMap<FontId, String>,
    /// The [`BreakSource`] the opportunity wrap path consumes (v2 item
    /// 2, decision 193). None -> legacy greedy cluster-boundary wrap
    /// (all existing M3 behavior preserved byte-for-byte).
    pub(crate) break_source: Option<Rc<dyn BreakSource>>,
}

impl LayoutEngine {
    pub fn new(config: LayoutTextConfig) -> Self {
        Self {
            config,
            ellipsis_widths: HashMap::new(),
            font_names: HashMap::new(),
            break_source: None,
        }
    }

    pub fn config(&self) -> &LayoutTextConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: LayoutTextConfig) {
        self.config = config;
    }

    /// Installs the break source for opportunity-driven wrapping
    /// ([`layout_text_with_breaks`]). None restores legacy greedy wrap.
    pub fn set_break_source(&mut self, source: Option<Rc<dyn BreakSource>>) {
        self.break_source = source;
    }

    /// The installed break source, if any (cloned `Rc` — sources are
    /// stateless and shared across hosts/rigs).
    pub fn break_source(&self) -> Option<Rc<dyn BreakSource>> {
        self.break_source.clone()
    }

    /// Refreshes the font-identity table from the installed service when
    /// it is still empty (no-op without a service). Called once per
    /// layout run, before traversal.
    pub fn refresh_font_names(&mut self, service: Option<&dyn TextService>) {
        if !self.font_names.is_empty() {
            return;
        }
        let Some(service) = service else {
            return;
        };
        for info in service.enumerate_fonts() {
            self.font_names
                .entry(info.id)
                .or_insert_with(|| info.family.clone());
        }
    }

    /// Backend-resolvable family for a shaper-reported run id, falling
    /// back to the requested family when unmapped (see the field docs).
    pub fn family_of(&self, id: FontId, fallback: &str) -> String {
        self.font_names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| fallback.to_string())
    }
}

/// Framework-side layout state: the engine plus the settled-generation
/// signal that makes feedback one frame delayed. Core-side (owned by the
/// component host), never in hot crates.
pub struct LayoutLedger {
    pub(crate) engine: LayoutEngine,
    pub(crate) generation: Signal<u64>,
    pub(crate) last_stats: LayoutStats,
}

impl LayoutLedger {
    pub fn new(rt: &Runtime) -> Self {
        Self {
            engine: LayoutEngine::new(LayoutTextConfig::default()),
            generation: rt.signal(0u64),
            last_stats: LayoutStats::default(),
        }
    }

    pub fn set_config(&mut self, config: LayoutTextConfig) {
        self.engine.set_config(config);
    }

    /// Installs the break source on the owned engine (see
    /// [`LayoutEngine::set_break_source`]).
    pub fn set_break_source(&mut self, source: Option<Rc<dyn BreakSource>>) {
        self.engine.set_break_source(source);
    }

    pub fn config(&self) -> &LayoutTextConfig {
        self.engine.config()
    }

    pub fn last_stats(&self) -> LayoutStats {
        self.last_stats
    }

    /// Untracked generation read (frame bookkeeping, tests).
    pub fn generation_value(&self) -> u64 {
        untrack(|| self.generation.get())
    }

    /// Tracked generation read (subscribes caller to layout publishes).
    pub fn track_generation(&self) -> u64 {
        self.generation.get()
    }

    /// Settled box read for effects: tracks the generation, so a publish in
    /// LAYOUT re-runs the reader in the next frame's EFFECTS. Returns the
    /// previous frame's box during the current frame (None before the first
    /// layout commits).
    pub fn settled(&self, rec: &Reconciler, id: NodeId) -> Option<LayoutBox> {
        let _ = self.generation.get();
        rec.get(id).and_then(|n| n.layout.clone())
    }

    /// Untracked committed-box read (paint/a11y/test path, never effects).
    pub fn committed(rec: &Reconciler, id: NodeId) -> Option<LayoutBox> {
        rec.get(id).and_then(|n| n.layout.clone())
    }

    /// Runs the engine over the retained tree and publishes: bumps the
    /// settled generation iff any box changed (or boxes were dropped by a
    /// removal). Must be called from the LAYOUT phase only — never runs
    /// user or component code.
    pub fn run(
        &mut self,
        rec: &mut Reconciler,
        styles: &Interner<Style>,
        service: Option<&dyn TextService>,
        viewport_w_css: f32,
        viewport_h_css: f32,
    ) -> LayoutStats {
        let stats = run_layout(
            rec,
            styles,
            service,
            &mut self.engine,
            viewport_w_css,
            viewport_h_css,
        );
        if stats.boxes_changed > 0 || stats.boxes_dropped {
            self.generation.update(|v| v + 1);
        }
        let mut out = stats;
        out.generation_bumped = stats.boxes_changed > 0 || stats.boxes_dropped;
        self.last_stats = out;
        out
    }
}

// ---------------------------------------------------------------------------
// Top-level run
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn run_layout(
    rec: &mut Reconciler,
    styles: &Interner<Style>,
    service: Option<&dyn TextService>,
    engine: &mut LayoutEngine,
    viewport_w_css: f32,
    viewport_h_css: f32,
) -> LayoutStats {
    let mut stats = LayoutStats::default();
    let Some(root) = rec.root() else {
        return stats;
    };
    // Round 1.1 (decision 252): viewports are widths — NaN/negative
    // never becomes a silent NaN tree.
    check_constrain_width("viewport_w_css", viewport_w_css);
    check_constrain_width("viewport_h_css", viewport_h_css);
    let dpr = engine.config.device_pixel_ratio.max(f32::EPSILON);
    // Run gating (decision 69): LAYOUT is the engine's own input, consumed
    // after the pass. TEXT/STRUCTURE flags are NOT inputs — they persist
    // for their future M4 consumers, so gating on them would run forever.
    // Text changes surface as measure-cache misses instead (keyed by bytes
    // + resolved style, so flag dirt alone never re-shapes and a TEXT-only
    // change still triggers exactly one re-measure).
    let dropped = rec.take_boxes_dropped();
    stats.boxes_dropped = dropped;
    let layout_dirty = rec.alive_ids().iter().any(|id| {
        rec.get(*id)
            .is_some_and(|n| n.pass_dirty.contains(PassMask::LAYOUT))
    });
    let needs_text = service.is_some() && needs_measure(rec, &engine.config, root);
    if !dropped && !layout_dirty && !needs_text {
        stats.empty = true;
        return stats;
    }
    let viewport_w = viewport_w_css * dpr;
    let viewport_h = viewport_h_css * dpr;
    // Font-identity table first (M7, decision 110): traversal resolves
    // per-run families through the engine cache from here on.
    engine.refresh_font_names(service);
    let inherited = engine.config.default_px;
    let root_style_w;
    let root_style_h;
    {
        let mut ctx = LayoutCtx {
            rec,
            styles,
            service,
            engine,
            stats: &mut stats,
            dpr,
            viewport_w,
            viewport_h,
        };
        let root_style = ctx.style_of(root);
        root_style_w = root_style.w.map(|p| p.get() * dpr);
        root_style_h = root_style.h.map(|p| p.get() * dpr);
        let gw = root_style_w.or(Some(viewport_w));
        ctx.layout_node(
            root,
            0.0,
            0.0,
            gw,
            None,
            0.0,
            0.0,
            inherited,
            crate::text::FontWeight::NORMAL,
        );
    }
    // Root fills the viewport unless explicitly sized (decision: the root
    // layer is the viewport for backends/a11y bounds).
    let want_w = root_style_w.unwrap_or(viewport_w);
    let want_h = root_style_h.unwrap_or(viewport_h);
    if let Some(n) = rec.node_mut(root) {
        if let Some(b) = n.layout.as_mut() {
            if b.w != want_w || b.h != want_h {
                b.w = want_w;
                b.h = want_h;
                // Same F1 rule as `commit_box`: a root resize with no
                // paint-relevant dirt still needs a rebuilt plan.
                if !n.pass_dirty.intersects(FRAME_DIRT) {
                    n.pass_dirty.set(PassMask::PAINT);
                    stats.paint_stamped += 1;
                }
                stats.boxes_changed += 1;
            }
        }
    }
    // Consume this pass's LAYOUT flags (TEXT/STRUCTURE stay for M4).
    for id in rec.alive_ids() {
        if let Some(n) = rec.node_mut(id) {
            n.pass_dirty.clear(PassMask::LAYOUT);
        }
    }
    if stats.layout_passes == 0 {
        stats.layout_passes = 1;
    }
    stats
}

// ---------------------------------------------------------------------------
// Traversal context
// ---------------------------------------------------------------------------

pub(crate) struct LayoutCtx<'a> {
    pub(crate) rec: &'a mut Reconciler,
    pub(crate) styles: &'a Interner<Style>,
    pub(crate) service: Option<&'a dyn TextService>,
    pub(crate) engine: &'a mut LayoutEngine,
    pub(crate) stats: &'a mut LayoutStats,
    pub(crate) dpr: f32,
    /// Viewport width in device px (Round 1.4: portal layers anchor to
    /// it instead of their parent's constraint).
    pub(crate) viewport_w: f32,
    /// Viewport height in device px (Round 7.21, decision 296:
    /// unconstrained portals default to full-viewport height, so
    /// modal dimming covers the window instead of hugging content).
    pub(crate) viewport_h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Size {
    pub(crate) w: f32,
    pub(crate) h: f32,
}

/// Per-side padding in device px (Round 11.1 — resolved once in
/// [`resolve_pad`], read everywhere as one shape, so symmetric
/// trees flow byte-identically through the new formulas).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pad {
    pub(crate) left: f32,
    pub(crate) right: f32,
    pub(crate) top: f32,
    pub(crate) bottom: f32,
}

impl Pad {
    /// Horizontal inset pair (content width shrinks by this).
    pub(crate) fn x(&self) -> f32 {
        self.left + self.right
    }

    /// Vertical inset pair (content height shrinks by this).
    pub(crate) fn y(&self) -> f32 {
        self.top + self.bottom
    }
}

/// Per-side margins in device px (Round 11.1 — resolved once in
/// [`resolve_margin`], same single-shape rule as [`Pad`]).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Margin {
    pub(crate) left: f32,
    pub(crate) right: f32,
    pub(crate) top: f32,
    pub(crate) bottom: f32,
}

impl Margin {
    /// Horizontal pair (a container's auto width grows by this).
    pub(crate) fn x(&self) -> f32 {
        self.left + self.right
    }

    /// Vertical pair (a container's auto height grows by this).
    pub(crate) fn y(&self) -> f32 {
        self.top + self.bottom
    }
}
