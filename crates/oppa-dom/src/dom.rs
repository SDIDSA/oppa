//! [`TreeDiff`](oppa::TreeDiff) → DOM mutations (M7, decisions 111–114).
//!
//! [`DomBackend`] is the third [`RendererBackend`](oppa::RendererBackend):
//! `commit` absorbs the reconciler's edit script into a `NodeId`-keyed
//! element registry (Add/Remove/Move link structure exactly — index
//! violations panic loudly, never silently relink); [`sync`](DomBackend::sync)
//! re-derives classes, geometry, text, and ARIA from public retained
//! reads (the DOM reader, never a re-lay-outer — browser layout runs
//! for nothing: every element carries absolute geometry).
//!
//! Minimality is preserved end-to-end: structure only moves on
//! structure ops (a scroll tick with zero structure ops mutates zero
//! DOM structure — counted from [`TreeDiff`]s, asserted in the M7
//! tests); content re-derives per dirty node. [`SyncStats::touched`]
//! is the static-frame meter (0 on settled frames, mirroring the CPU
//! empty-plan skip and the Vello zero-staged-work rule).
//!
//! Scroll (§9.3): `ScrollArea` commits to an overflow container plus a
//! spacer plus absolutely-positioned slots with `overflow-anchor: none`.
//! The browser owns `scrollTop` (synthesized scrolling is rejected). The
//! INPUT-mapping entry records the browser-reported position (see
//! [`note_browser_scroll`](DomBackend::note_browser_scroll)) and the
//! shell maps it into the framework-owned offset signal (see
//! [`ComponentHost::bind_scroll`](oppa::ComponentHost)). The backend
//! itself never writes scroll positions.
//!
//! Foreign elements (one mechanism, two callers, decision 113):
//! `Tag::Custom` (external-element hole, `data-external` marker) and
//! verdict-(b) editable fields (`TextField` semantics → real `<input>`,
//! presenter-owned editing per locked #27). Field children are absorbed
//! (the input is void; the value carries their text) — absorbed ids
//! stay keyed in the registry but materialize no element.

use std::collections::{HashMap, HashSet};

use oppa::{
    BackendError, Caps, ImageCache, Interner, NodeId, PaintStats, PresenterKind, Reconciler,
    RendererBackend, Role, Style, SurfaceDesc, SurfaceId, Tag, ThemeMode, ThemeTokens, TreeDiff,
};

use crate::aria::aria_attrs;
use crate::css::StyleSheet;

/// §9.3 v1 overscan constant: virtualized windows pad ±4 slots around
/// the visible range (window lag, not tearing — the offset trails the
/// browser by ≤ 1 frame, so overscan covers the stale frame).
pub const OVERSCAN_SLOTS: u32 = 4;

/// Visible row window with overscan (pure helper M8's virtualization
/// consumes): `(first, one_past_last)` row indices covering
/// `[offset, offset + viewport)` padded by [`OVERSCAN_SLOTS`],
/// clamped to `[0, row_count]`. Uniform rows (v1 scope).
pub fn scroll_window(
    offset_px: f32,
    row_h: f32,
    viewport_h: f32,
    row_count: usize,
) -> (usize, usize) {
    scroll_window_overscan(
        offset_px,
        row_h,
        viewport_h,
        row_count,
        OVERSCAN_SLOTS as usize,
    )
}

/// [`scroll_window`] with an explicit overscan (M8 overscan decision:
/// the sweep measures the repaint bound at the DESIGN-sketch `+2` and
/// the shipped `+4` through this helper; [`scroll_window`] stays the
/// one-constant production path).
pub fn scroll_window_overscan(
    offset_px: f32,
    row_h: f32,
    viewport_h: f32,
    row_count: usize,
    over: usize,
) -> (usize, usize) {
    if row_h <= 0.0 || row_count == 0 {
        return (0, 0);
    }
    let first = (offset_px / row_h).floor().max(0.0) as usize;
    let last = ((offset_px + viewport_h) / row_h).ceil().max(0.0) as usize;
    (first.saturating_sub(over), (last + over).min(row_count))
}

mod element;
mod render;

pub(crate) use element::same_own;
pub use element::{
    json_escape, AttrPatch, DomElement, DomRun, HtmlKind, PagePatch, PlacePatch, SyncStats,
    ThemePatch,
};

pub struct DomBackend {
    dpr: f32,
    elements: HashMap<NodeId, DomElement>,
    /// Shared image-key registry (round 4.4): the app's `ImageCache`
    /// handle (Rc-shared, zero-cost clone) — `<img>` sources resolve
    /// here at derive time.
    images: ImageCache,
    /// Field-absorbed descendants (keyed, unmaterialized — see module docs).
    absorbed: HashSet<NodeId>,
    /// Structural mutations absorbed since construction (Add+Remove+Move
    /// — the DOM-level minimality meter beside the diff-level count).
    mutations: u64,
    roots: Vec<NodeId>,
    surfaces: HashMap<SurfaceId, SurfaceDesc>,
    next_surface: u64,
    /// Browser-owned scroll positions per scroll container (observed via
    /// [`note_browser_scroll`](Self::note_browser_scroll), never written).
    scroll_tops: HashMap<NodeId, f32>,
    /// Binding-edge stamp armed by [`commit`](RendererBackend::commit)
    /// (M8, §9.4): a stamped diff in this frame disables CSS transitions
    /// on the elements that re-derive in the following [`sync`](Self::sync)
    /// — exactly one sync consumes it (the v1 per-commit limit: the whole
    /// stamped frame's touched set, not per-node provenance).
    suppress_armed: bool,
    /// Text selection overlay (Round 8.2, decision 298): the host's
    /// focused session range, painted as highlight divs on the field
    /// container (same shared `selection_rects` rule as the rasterizers;
    /// `None` derives byte-identical markup to pre-8.2).
    selection: Option<oppa::SelectionPaint>,
    /// Caret bar overlay (Round 15.1, decision 312): the host's
    /// focused caret, painted as a `caret` div on the field
    /// container (same absolute device-px space as the plan's caret
    /// `Rect`, converted to CSS px here; `None` derives
    /// byte-identical markup to pre-15.1).
    caret: Option<oppa::CaretPaint>,
    /// Page chrome theme (theme contract round): the runner's host
    /// mode, published per frame like the overlays above. Drives
    /// the full-page `<body>` style and the change-only patch
    /// stanza (CSS inheritance carries both to every non-inked
    /// node — no per-node patch, explicit `color:` still wins).
    /// Light default: pre-contract markup byte-identical.
    theme: ThemeMode,
    /// Pending page-chrome flip (set by [`set_theme_mode`](Self::set_theme_mode)
    /// on change, consumed by [`sync`](Self::sync) as touched work —
    /// a bare toggle on an unthemed tree still emits its stanza).
    theme_dirty: bool,
    /// Last mode the patch channel carried. Starts as Light (the
    /// browser's implicit baseline — a settled Light tree takes
    /// empty, exactly like pre-contract; the first take on a Dark
    /// tree still emits its stanza).
    last_patched_theme: Option<ThemeMode>,
    /// Touched count awaiting the next `paint` (the work meter).
    pending_touched: usize,
    paints: u64,
    /// Last emitted page (Round 12.1): the element map + roots the
    /// browser currently holds — full pages prime it
    /// ([`mark_rendered`](Self::mark_rendered)), patches diff
    /// against it ([`take_patch`](Self::take_patch)). A full clone
    /// per emission (O(n) at test scale — the trees are hundreds of
    /// nodes, never thousands).
    snap: HashMap<NodeId, DomElement>,
    snap_roots: Vec<NodeId>,
    /// Registered web fonts (Phase 36 PR3, decision 355): family →
    /// raw sfnt bytes, served to the page as `@font-face` data URIs
    /// so the browser shapes with the same bytes the framework
    /// measured with (decision-81 parity). Re-registering a family
    /// replaces its bytes (last wins, stated).
    fonts: Vec<(String, Vec<u8>)>,
    /// Keyframe tracker for the stepped path (Phase 36 PR4, decision
    /// 357): fed per diff by the paint hook (same diffs, same order
    /// as the host evaluator — deterministic twins), sampled at
    /// `kf_now` during derive. Plain tweens never consult it (they
    /// keep the CSS mapping).
    kf_eval: oppa::TransitionEvaluator,
    /// Frame time of the last tracked commit (the hook's clock —
    /// track and sample share it, never skew).
    kf_now: f64,
}

impl DomBackend {
    pub fn new(dpr: f32) -> Self {
        Self {
            dpr,
            elements: HashMap::new(),
            images: ImageCache::new(),
            absorbed: HashSet::new(),
            mutations: 0,
            roots: Vec::new(),
            surfaces: HashMap::new(),
            next_surface: 1,
            scroll_tops: HashMap::new(),
            suppress_armed: false,
            pending_touched: 0,
            paints: 0,
            selection: None,
            caret: None,
            theme: ThemeMode::Light,
            theme_dirty: false,
            last_patched_theme: Some(ThemeMode::Light),
            snap: HashMap::new(),
            snap_roots: Vec::new(),
            fonts: Vec::new(),
            kf_eval: oppa::TransitionEvaluator::new(),
            kf_now: 0.0,
        }
    }

    pub fn dpr(&self) -> f32 {
        self.dpr
    }

    /// Shares the app's image-key registry (round 4.4 — the same
    /// handle the scene's `cache.load` calls go through, so every
    /// retained id resolves).
    pub fn set_images(&mut self, images: ImageCache) {
        self.images = images;
    }

    /// Registers one web font (Phase 36 PR3, decision 355): the
    /// browser serves `family` from these sfnt bytes (data-URI
    /// `@font-face`, see [`DomBackend::font_face_css`]) — the same
    /// bytes the framework measured with, closing the decision-81
    /// display-vs-measure drift. Re-registering a family replaces
    /// its bytes (last wins). Empty bytes refuse loudly (a packaging
    /// bug, never a silent fallback).
    pub fn register_font(&mut self, family: &str, ttf_bytes: &[u8]) {
        if ttf_bytes.is_empty() {
            panic!(
                "dom: register_font({family}) with empty bytes — refused, never a silent fallback"
            );
        }
        if let Some(slot) = self.fonts.iter_mut().find(|(f, _)| f == family) {
            slot.1 = ttf_bytes.to_vec();
        } else {
            self.fonts.push((family.to_string(), ttf_bytes.to_vec()));
        }
    }

    /// `@font-face` CSS for every registered font (Phase 36 PR3):
    /// one block per family, bytes inline as base64 data URIs
    /// (emitted once per page boot, never per patch — the page shell
    /// injects it before any text renders). Empty with no fonts
    /// (unregistered trees render exactly as before).
    pub fn font_face_css(&self) -> String {
        let mut out = String::new();
        for (family, bytes) in &self.fonts {
            out.push_str(&format!(
                "@font-face{{font-family:\"{}\";src:url(data:font/ttf;base64,{}) format(\"truetype\");}}\n",
                family.replace('"', ""),
                base64_encode(bytes),
            ));
        }
        out
    }

    pub fn element(&self, id: NodeId) -> Option<&DomElement> {
        self.elements.get(&id)
    }

    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// Committed roots in order (page render entry).
    pub fn root_ids(&self) -> Vec<NodeId> {
        self.roots.clone()
    }

    pub fn absorbed_count(&self) -> usize {
        self.absorbed.len()
    }

    pub fn is_absorbed(&self, id: NodeId) -> bool {
        self.absorbed.contains(&id)
    }

    /// Structural mutations absorbed since construction.
    pub fn mutations(&self) -> u64 {
        self.mutations
    }

    /// Browser-reported `scrollTop` for a scroll container (None = never
    /// observed — never a silent zero).
    pub fn scroll_top(&self, target: NodeId) -> Option<f32> {
        self.scroll_tops.get(&target).copied()
    }

    /// Records the browser's scroll position (§9.3 INPUT-mapping entry:
    /// the shell calls this when mapping the browser scroll event, then
    /// injects the corresponding `InputEvent::Scroll`). Returns the
    /// previous position (None on first sight — the delta mapper's input).
    pub fn note_browser_scroll(&mut self, target: NodeId, top_px: f32) -> Option<f32> {
        self.scroll_tops.insert(target, top_px)
    }

    /// Retained node behind a rendered `data-pid` (U8 INPUT-mapping
    /// entry: the page reports the edited input's pid; the shell
    /// resolves it here, then injects `InputEvent::Text`). None =
    /// unknown pid (stale post-swap markup — never a silent node).
    pub fn node_for_pid(&self, pid: &str) -> Option<NodeId> {
        self.elements.keys().find(|id| pid_of(**id) == pid).copied()
    }

    /// Re-derives every live element from retained reads. Dirty nodes
    /// (the commit's Update set) plus new shells re-derive; clean nodes
    /// compare equal and cost nothing observable. Returns the touched
    /// count (0 on settled frames). Image sources resolve through the
    /// backend's shared cache (round 4.4 — unregistered ids refuse
    /// loudly, the old blanket refusal naming the exact id instead).
    pub fn sync(
        &mut self,
        rec: &Reconciler,
        styles: &Interner<Style>,
        sheet: &mut StyleSheet,
    ) -> Result<SyncStats, BackendError> {
        let mut touched = 0usize;
        self.absorbed.clear();
        // Fields absorb their subtrees first (children must not
        // materialize while a field ancestor exists).
        if let Some(root) = rec.root() {
            self.mark_absorbed(rec, root, false);
        }
        let ids = self.dfs_ids(rec);
        let mut live: HashSet<NodeId> = HashSet::new();
        // §9.4 one-commit suppression (M8): exactly one sync consumes the
        // armed stamp — stamped or not, the flag clears here, so the
        // `transition:none` inline below can only ever describe the sync
        // that directly followed the stamped commit.
        let suppress = std::mem::replace(&mut self.suppress_armed, false);
        for id in ids {
            let Some(node) = rec.get(id) else {
                continue;
            };
            // Round 4.4: image sources must resolve (unregistered or
            // id-less images refuse loudly here — the old blanket
            // refusal naming the exact node instead of failing the
            // whole sync silently later). Phase 36 PR4: pre-decoded
            // cache pixels resolve too (static path — a key OR pixels
            // satisfies the gate; derive prefers pixels).
            if node.tag == Tag::Image
                && node
                    .image
                    .and_then(|image_id| {
                        self.images
                            .pixels_of(image_id)
                            .map(|_| ())
                            .or_else(|| self.images.key_of(image_id).map(|_| ()))
                    })
                    .is_none()
            {
                return Err(BackendError::UnsupportedOp(format!(
                    "Image on {id:?} (image {:?}): no cache key or pixels — register the source \
                     with ImageCache::load (URLs) or insert_pixels (statics) first (hand-built Image elements carry \
                     nothing to resolve)",
                    node.image
                )));
            }
            if self.absorbed.contains(&id) {
                self.elements.remove(&id);
                continue;
            }
            live.insert(id);
            let el = self.derive(rec, styles, sheet, id);
            match self.elements.get(&id) {
                Some(prev) if prev == &el => {}
                _ => {
                    // Stamped frames write target values with transitions
                    // disabled (DESIGN §9.4's DOM half); only otherwise
                    // touched elements are flagged (an untouched element
                    // has no new value to transition toward).
                    let mut el = el;
                    if suppress {
                        el.no_transition = true;
                    }
                    self.elements.insert(id, el);
                    touched += 1;
                }
            }
        }
        // Drop shells for retired nodes (commit drives removals; this is
        // the belt-and-braces for direct-sync callers).
        self.elements.retain(|id, _| live.contains(id));
        self.roots = rec.root().into_iter().collect();
        // Theme contract round: a flipped page chrome is real sync
        // work (the body-style stanza) even when no element
        // re-derived — without this a bare toggle on an unthemed
        // tree emits nothing and the body goes stale.
        if self.theme_dirty {
            self.theme_dirty = false;
            touched += 1;
        }
        self.pending_touched += touched;
        Ok(SyncStats {
            touched,
            elements: self.elements.len(),
        })
    }

    /// Primes the emitted-page snapshot after a full-page render
    /// (Round 12.1): the browser holds exactly this tree, so the
    /// next [`take_patch`](Self::take_patch) diffs against it.
    pub fn mark_rendered(&mut self) {
        self.snap = self.elements.clone();
        self.snap_roots.clone_from(&self.roots);
    }

    /// Diffs the live tree against the last emitted page and returns
    /// the minimal patch (Round 12.1, decision 307) — then re-primes
    /// the snapshot, so consecutive takes describe consecutive
    /// transitions. Rules (each proven by the focus-preservation
    /// suite):
    /// - removals/additions emit topmost only (descendants ride the
    ///   ancestor op);
    /// - reordered/new children emit one `place` per affected live
    ///   parent (final order — the applier moves live nodes, never
    ///   detaches them, so focus survives moves);
    /// - own-changed leaves swap whole (`Field`/`Area` sync the
    ///   value property instead — never an element swap while the
    ///   browser may hold caret);
    /// - own-changed parents of fields sync open-tag state in place
    ///   (never an outerHTML swap over a live field);
    /// - field-free changed subtrees swap whole (nothing focusable
    ///   inside, by retained-kind walk);
    /// - `.sel` highlights and `.spacer` heights sync through their
    ///   marker classes (unkeyed decorations, never element swaps).
    pub fn take_patch(&mut self) -> PagePatch {
        let mut patch = PagePatch::default();
        if self.snap_roots != self.roots && !(self.snap_roots.is_empty() && self.snap.is_empty()) {
            // Root identity changed mid-session (remount-class):
            // indescribable incrementally — full-swap fallback.
            patch.full = true;
            // The full page carries the current page chrome (and the
            // reload payload nests inside the live root, never
            // touching the real `<body>`), so the stanza rides here
            // too — never a duplicate on the next incremental take.
            patch.theme = self.theme_stanza();
            self.mark_rendered();
            return patch;
        }
        if self.snap.is_empty() && !self.elements.is_empty() {
            // Never emitted (no full page yet): same fallback.
            patch.full = true;
            patch.theme = self.theme_stanza();
            self.mark_rendered();
            return patch;
        }
        // Parent maps (child → parent) for both trees.
        let mut snap_parent: HashMap<NodeId, NodeId> = HashMap::new();
        for (p, el) in &self.snap {
            for c in &el.children {
                snap_parent.insert(*c, *p);
            }
        }
        // Current child lists are read inline below (places); only
        // the snap side needs a parent map (removal topmost).
        let has_snap_ancestor =
            |mut id: NodeId,
             set: &std::collections::HashSet<NodeId>,
             parents: &HashMap<NodeId, NodeId>| {
                while let Some(p) = parents.get(&id) {
                    if set.contains(p) {
                        return true;
                    }
                    id = *p;
                }
                false
            };
        // Removals: in snap, gone now — topmost only.
        let removed: std::collections::HashSet<NodeId> = self
            .snap
            .keys()
            .copied()
            .filter(|id| !self.elements.contains_key(id))
            .collect();
        let mut removes: Vec<String> = removed
            .iter()
            .copied()
            .filter(|id| !has_snap_ancestor(*id, &removed, &snap_parent))
            .map(pid_of)
            .collect();
        removes.sort();
        patch.removes = removes;
        // Additions: live now, absent in snap. Place parents (below)
        // are live-prev, so their added kids are topmost by
        // construction — blobs carry whole subtrees.
        let added: std::collections::HashSet<NodeId> = self
            .elements
            .keys()
            .copied()
            .filter(|id| !self.snap.contains_key(id))
            .collect();
        // Places: live parents whose child list changed (final order
        // + blobs for the added kids). Added parents ride ancestor
        // blobs — no place op of their own.
        let mut place_parents: Vec<NodeId> = Vec::new();
        for (p, el) in &self.elements {
            if added.contains(p) {
                continue;
            }
            let before = self
                .snap
                .get(p)
                .map(|e| e.children.as_slice())
                .unwrap_or(&[]);
            if before != el.children.as_slice() {
                place_parents.push(*p);
            }
        }
        place_parents.sort_by_cached_key(|id| pid_of(*id));
        for p in place_parents {
            let el = &self.elements[&p];
            // Blobs for added kids (topmost by construction — place
            // parents are live-prev, and blobs carry whole subtrees).
            let mut blobs = Vec::new();
            for c in &el.children {
                if added.contains(c) {
                    blobs.push((pid_of(*c), self.render_node(*c)));
                }
            }
            patch.places.push(PlacePatch {
                parent: pid_of(p),
                kids: el.children.iter().map(|c| pid_of(*c)).collect(),
                blobs,
            });
        }
        // Own-changes on live-both elements.
        let mut ids: Vec<NodeId> = self.elements.keys().copied().collect();
        ids.sort_by_cached_key(|id| pid_of(*id));
        for id in ids {
            let (Some(el), Some(prev)) = (self.elements.get(&id), self.snap.get(&id)) else {
                continue;
            };
            if el == prev {
                continue;
            }
            let pid = pid_of(id);
            // Unkeyed decorations ride marker classes, never swaps.
            // (Round 15.1: the caret bar joins the selection payload —
            // one `sels` entry carries both, so blink toggles never
            // need a new patch kind.)
            if el.sel_rects != prev.sel_rects || el.caret_rect != prev.caret_rect {
                patch.sels.push((pid.clone(), self.render_decorations(el)));
            }
            if el.spacer_h != prev.spacer_h {
                patch
                    .spacers
                    .push((pid.clone(), el.spacer_h.unwrap_or(0.0)));
            }
            if same_own(el, prev) {
                continue;
            }
            if el.children.is_empty() {
                match el.kind {
                    HtmlKind::Field | HtmlKind::Area => patch.attrs.push(self.chrome_of(el, prev)),
                    _ => patch.swaps.push((pid, self.render_node(id))),
                }
            } else if !self.subtree_has_field(id) {
                patch.swaps.push((pid, self.render_node(id)));
            } else {
                patch.attrs.push(self.chrome_of(el, prev));
            }
        }
        patch.swaps.sort();
        patch.attrs.sort_by(|a, b| a.pid.cmp(&b.pid));
        patch.sels.sort();
        patch.spacers.sort_by(|a, b| a.0.cmp(&b.0));
        // Ancestor suppression: a swap recreates its whole subtree
        // from current render — descendant swaps/attrs/sels/places
        // and removals underneath describe the same transition twice.
        // (The applier converges either way; this keeps patches
        // minimal — the round's stated goal.)
        if !patch.swaps.is_empty() {
            let mut covered: std::collections::HashSet<NodeId> = std::collections::HashSet::new();
            for (pid, _) in &patch.swaps {
                if let Some(id) = self.node_for_pid(pid) {
                    let mut stack = vec![id];
                    while let Some(n) = stack.pop() {
                        if n != id {
                            covered.insert(n);
                        }
                        if let Some(el) = self.elements.get(&n) {
                            stack.extend(el.children.iter().copied());
                        }
                    }
                }
            }
            let is_covered = |pid: &str| {
                self.node_for_pid(pid)
                    .is_some_and(|id| covered.contains(&id))
            };
            let swap_roots: std::collections::HashSet<String> =
                patch.swaps.iter().map(|(s, _)| s.clone()).collect();
            patch.swaps.retain(|(pid, _)| !is_covered(pid));
            patch.attrs.retain(|a| !is_covered(&a.pid));
            patch.sels.retain(|(pid, _)| !is_covered(pid));
            patch.spacers.retain(|(pid, _)| !is_covered(pid));
            patch.removes.retain(|pid| !is_covered(pid));
            patch
                .places
                .retain(|p| !is_covered(&p.parent) && !swap_roots.contains(p.parent.as_str()));
        }
        // Theme contract round: the page-chrome stanza rides
        // change-only — a bare toggle touches no elements but must
        // still move the body style (sync() already counted the
        // flip as touched work, so the caller takes this patch).
        patch.theme = self.theme_stanza();
        self.mark_rendered();
        patch
    }

    /// Change-only page-chrome stanza (theme contract round): returns
    /// `Some` and advances the marker when the mode moved since the
    /// last patch — `None` on repeats (never per-frame noise).
    fn theme_stanza(&mut self) -> Option<ThemePatch> {
        if self.last_patched_theme != Some(self.theme) {
            self.last_patched_theme = Some(self.theme);
            let t = ThemeTokens::of(self.theme);
            Some(ThemePatch {
                bg: dom_hex(t.background),
                ink: dom_hex(t.text_primary),
            })
        } else {
            None
        }
    }

    /// True when the retained subtree under `id` holds a live field
    /// (element-kind walk — roles resolve at derive, so kinds are
    /// the proof, never heuristics).
    fn subtree_has_field(&self, id: NodeId) -> bool {
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let Some(el) = self.elements.get(&n) else {
                continue;
            };
            if matches!(el.kind, HtmlKind::Field | HtmlKind::Area) {
                return true;
            }
            stack.extend(el.children.iter().copied());
        }
        false
    }

    /// Own open-tag state mirroring [`render_node`](Self::render_node)
    /// (Round 12.1 — change both together: the class/style/attrs
    /// strings below must read exactly what the open tags emit).
    fn chrome_of(&self, el: &DomElement, prev: &DomElement) -> AttrPatch {
        let pid = pid_of(el.node);
        let cls = el.classes.join(" ");
        let no_trans = transition_inline(el);
        let (style, mut attrs, value) = match &el.kind {
            HtmlKind::Field => (
                format!(
                    "{}{}{}{}",
                    el.inline_geom, el.inline_font, no_trans, el.inline_kf
                ),
                el.attrs.clone(),
                Some(el.text.clone()),
            ),
            HtmlKind::Area => (
                format!(
                    "{}{}{}{}",
                    el.inline_geom, el.inline_font, no_trans, el.inline_kf
                ),
                el.attrs.clone(),
                Some(el.text.clone()),
            ),
            HtmlKind::Scroll => (
                format!(
                    "{}{}{}overflow:auto;",
                    el.inline_geom, no_trans, el.inline_kf
                ),
                el.attrs.clone(),
                None,
            ),
            HtmlKind::Text => (
                format!("{}{}{}", el.inline_geom, el.inline_font, el.inline_kf),
                el.attrs.clone(),
                None,
            ),
            _ => (
                format!("{}{}{}", el.inline_geom, no_trans, el.inline_kf),
                el.attrs.clone(),
                None,
            ),
        };
        if !el.placeholder.is_empty() {
            attrs.push(("placeholder".to_string(), el.placeholder.clone()));
        }
        // Stale names the applier removes (placeholder included —
        // absence is state, never leftover markup).
        let mut before: Vec<&str> = prev.attrs.iter().map(|(k, _)| k.as_str()).collect();
        if !prev.placeholder.is_empty() {
            before.push("placeholder");
        }
        let mut drops = Vec::new();
        for k in before {
            if !attrs.iter().any(|(n, _)| n == k) && !drops.iter().any(|d| d == k) {
                drops.push(k.to_string());
            }
        }
        drops.sort();
        // Own-text HTML for non-field kinds (fields ride `value`;
        // leaves ride swaps — this covers own-text edits on parents
        // of live fields, degenerate but never silent).
        let text_changed = el.text != prev.text || el.runs != prev.runs;
        let text = match &el.kind {
            HtmlKind::Field | HtmlKind::Area => None,
            _ if text_changed => Some(self.render_runs(el)),
            _ => None,
        };
        AttrPatch {
            pid,
            cls,
            style,
            attrs,
            drops,
            value,
            text,
        }
    }

    // -- internals -------------------------------------------------------

    fn dfs_ids(&self, rec: &Reconciler) -> Vec<NodeId> {
        let mut out = Vec::new();
        if let Some(root) = rec.root() {
            let mut stack = vec![root];
            while let Some(id) = stack.pop() {
                out.push(id);
                if let Some(n) = rec.get(id) {
                    for child in n.children.iter().rev() {
                        stack.push(*child);
                    }
                }
            }
        }
        out
    }

    /// Marks field-absorbed subtrees (children of `TextField` /
    /// `TextArea` nodes).
    /// `under_field` propagates down; field nodes themselves materialize.
    fn mark_absorbed(&mut self, rec: &Reconciler, id: NodeId, under_field: bool) {
        let Some(node) = rec.get(id) else {
            return;
        };
        let is_field = node
            .semantics
            .as_ref()
            .is_some_and(|s| s.role == Role::TextField || s.role == Role::TextArea);
        if under_field {
            self.absorbed.insert(id);
        }
        for child in node.children.clone() {
            self.mark_absorbed(rec, child, under_field || is_field);
        }
    }

    fn derive(
        &mut self,
        rec: &Reconciler,
        styles: &Interner<Style>,
        sheet: &mut StyleSheet,
        id: NodeId,
    ) -> DomElement {
        let node = rec.get(id).expect("derive on a live node");
        let style = styles.get(node.style).cloned().unwrap_or_default();
        let class = sheet.class_for(node.style, &style);
        let is_field = node
            .semantics
            .as_ref()
            .is_some_and(|s| s.role == Role::TextField);
        let is_area = node
            .semantics
            .as_ref()
            .is_some_and(|s| s.role == Role::TextArea);
        let kind = if is_field {
            HtmlKind::Field
        } else if is_area {
            HtmlKind::Area
        } else {
            match node.tag {
                Tag::Text => HtmlKind::Text,
                Tag::ScrollArea => HtmlKind::Scroll,
                Tag::Custom(cid) => HtmlKind::External(cid),
                // Round 4.4: real `<img>` (src resolved — `sync`
                // refuses unregistered ids loudly before derive, so
                // the lookup below cannot miss; alt honors the
                // semantics label, else empty). Phase 36 PR4
                // (decision 359): pre-decoded cache pixels win over
                // the key string (data-URI PNG — the static path);
                // bare keys keep rendering as URLs (round-4.4 rule,
                // unchanged).
                Tag::Image => {
                    let src = node
                        .image
                        .and_then(|image_id| image_src(&self.images, image_id))
                        .expect("sync refused unregistered images");
                    let alt = node
                        .semantics
                        .as_ref()
                        .and_then(|s| s.label.clone())
                        .map(|label| label.to_string())
                        .unwrap_or_default();
                    HtmlKind::Image { src, alt }
                }
                // Inline vector (decision 291): the retained payload
                // rides `d` verbatim. Style paint fields never apply
                // (the plan builder refuses them too — both entries
                // stay loud so a DOM-only consumer cannot silently
                // diverge from the rasterizers).
                Tag::Path => {
                    if style.bg.is_some() {
                        panic!("dom sync: bg on Path node {id:?} — path paint rides data/fill/stroke, never style bg");
                    }
                    if style.border.is_some() || style.border_edges.is_some() {
                        panic!("dom sync: border on Path node {id:?} — path paint rides data/fill/stroke, never style rings");
                    }
                    if style.bg_gradient.is_some() {
                        panic!("dom sync: bg_gradient on Path node {id:?} — path paint rides data/fill/stroke, never style gradients");
                    }
                    if style.shadow.is_some() {
                        panic!("dom sync: shadow on Path node {id:?} — path paint rides data/fill/stroke, never style shadows");
                    }
                    if style.has_any_radius() {
                        panic!("dom sync: radius/circle on Path node {id:?} — paths carry their own geometry, never style shape flags");
                    }
                    if style.ink.is_some() {
                        panic!("dom sync: ink on Path node {id:?} — path paint rides data/fill/stroke, never style ink");
                    }
                    let spec = node.path.clone().unwrap_or_else(|| {
                        panic!("dom sync: Path node {id:?} without a path payload — refused, never a silent hole")
                    });
                    if spec.data.trim().is_empty() {
                        panic!("dom sync: Path node {id:?} has blank path data — refused, never a silent hole");
                    }
                    if spec.fill.is_none() && spec.stroke.is_none() {
                        panic!("dom sync: Path node {id:?} has neither fill nor stroke — refused, never a silent hole");
                    }
                    let dpr = self.dpr.max(f32::EPSILON);
                    let (vw, vh) = node
                        .layout
                        .as_ref()
                        .map(|b| (b.w / dpr, b.h / dpr))
                        .unwrap_or((0.0, 0.0));
                    HtmlKind::Vector {
                        data: spec.data.to_string(),
                        fill: spec.fill.map(dom_hex).unwrap_or_else(|| "none".to_string()),
                        stroke: spec
                            .stroke
                            .map(|s| dom_hex(s.color))
                            .unwrap_or_else(|| "none".to_string()),
                        stroke_width: spec.stroke.map(|s| s.width),
                        view: (vw, vh),
                    }
                }
                // Retained canvas (Phase 36 PR4, decision 358): the spec
                // lowers to inline SVG children (rects/paths/texts in
                // authoring px, 1:1 like `Vector`). Laid canvas text
                // does NOT double-render as spans (the node carries no
                // text payload — `text_runs` slices empty). Missing
                // specs refuse loudly (never a silent hole).
                Tag::Canvas => {
                    let spec = node.canvas.clone().unwrap_or_else(|| {
                        panic!("dom sync: Canvas node {id:?} without a canvas payload — refused, never a silent hole")
                    });
                    let dpr = self.dpr.max(f32::EPSILON);
                    let (vw, vh) = node
                        .layout
                        .as_ref()
                        .map(|b| (b.w / dpr, b.h / dpr))
                        .unwrap_or((0.0, 0.0));
                    HtmlKind::Canvas {
                        inner: canvas_inner_svg(&spec),
                        view: (vw, vh),
                    }
                }
                Tag::Div | Tag::Row | Tag::Column | Tag::Stack | Tag::Portal | Tag::Grid => {
                    HtmlKind::Block
                }
            }
        };
        let is_root = rec.root() == Some(id);
        // Ancestor-relative offsets (decision 115): nested absolute
        // elements resolve against their positioned ancestor's box, so
        // committed (stage-space) boxes convert by subtracting it.
        // Transparent text wrappers are static pass-throughs (never
        // positioned — an empty auto-width wrapper would collapse to a
        // zero-width containing block and wrap its own child); the walk
        // skips them when finding the positioned ancestor.
        let origin = positioned_origin(rec, id);
        // Static wrappers (transparent layout pass-throughs) carry no
        // geometry — children resolve against the real container.
        let positioned = !is_root && !is_static_wrapper(rec, id);
        let inline_geom = geom_inline(
            node.layout.as_ref(),
            is_root,
            &kind,
            self.dpr,
            origin,
            positioned,
        );
        let mut attrs = node.semantics.as_ref().map(aria_attrs).unwrap_or_default();
        if node
            .semantics
            .as_ref()
            .is_some_and(|s| (s.role == Role::TextField || s.role == Role::TextArea) && s.disabled)
        {
            attrs.push(("disabled".to_string(), String::new()));
        }
        if kind == HtmlKind::Scroll {
            attrs.push(("data-scroll".to_string(), String::new()));
            if let Some(top) = self.scroll_tops.get(&id) {
                attrs.push(("data-scrolltop".to_string(), css_num(*top)));
            }
        }
        // Slot anchoring (§9.3): browser anchoring fights our own spacer
        // repositioning, so it is off inside recycled lists.
        if matches!(
            kind,
            HtmlKind::Block
                | HtmlKind::Text
                | HtmlKind::Image { .. }
                | HtmlKind::Vector { .. }
                | HtmlKind::Area
        ) && parent_is_scroll(rec, id)
        {
            attrs.push(("data-slot".to_string(), String::new()));
        }
        let (text, runs, inline_font, placeholder) = match &kind {
            HtmlKind::Field | HtmlKind::Area => {
                let (value, placeholder) = field_texts(rec, id);
                (
                    value,
                    Vec::new(),
                    field_font(rec, id, self.dpr),
                    placeholder,
                )
            }
            HtmlKind::Text => {
                let (text, runs, font) = text_runs(rec, id, self.dpr);
                (text, runs, font, String::new())
            }
            HtmlKind::Canvas { .. } => {
                // Canvas `<text>` inherits the element font (the
                // `text_runs` pattern — family + size from the first
                // laid line; the node carries no text payload).
                (
                    String::new(),
                    Vec::new(),
                    canvas_font(rec, id, self.dpr),
                    String::new(),
                )
            }
            _ => (String::new(), Vec::new(), String::new(), String::new()),
        };
        let children = match &kind {
            // Inputs, areas, images, vectors, and canvases are
            // void-shaped (value/text content/src/`d`/spec carry the
            // payload); external holes keep their framework children
            // (v1 hole is a marked box, not a true void element —
            // stated).
            HtmlKind::Field
            | HtmlKind::Area
            | HtmlKind::Image { .. }
            | HtmlKind::Vector { .. }
            | HtmlKind::Canvas { .. } => Vec::new(),
            _ => node
                .children
                .iter()
                .copied()
                .filter(|c| !self.absorbed.contains(c))
                .collect(),
        };
        let spacer_h = match &kind {
            HtmlKind::Scroll => node
                .layout
                .as_ref()
                .map(|b| b.content_h / self.dpr.max(f32::EPSILON)),
            _ => None,
        };
        let (caret_rect, caret_ink) = match self.caret_decoration(id, origin) {
            Some((r, hex)) => (Some(r), hex),
            None => (None, String::new()),
        };
        // Stepped keyframe values (Phase 36 PR4): the live track's
        // evaluated paint, re-declared inline while it runs (empty
        // otherwise — unkeyframed markup byte-identical).
        let inline_kf = match self.kf_eval.resolve_keyframe(id, self.kf_now) {
            Some((bg, op)) => {
                let mut s = String::new();
                if let Some(c) = bg {
                    s.push_str(&format!("background-color:{};", dom_hex(c)));
                }
                s.push_str(&format!("opacity:{};", css_num(op)));
                s
            }
            None => String::new(),
        };
        DomElement {
            node: id,
            kind,
            classes: vec![class],
            inline_geom,
            inline_font,
            text,
            runs,
            attrs,
            children,
            spacer_h,
            placeholder,
            sel_rects: self.selection_rects(rec, id, origin),
            caret_rect,
            caret_ink,
            // `derive` never sets the stamp itself: `sync` flags exactly
            // the touched set of the sync that consumed it (see above),
            // so the flag clears on the next re-derive by comparison.
            no_transition: false,
            inline_kf,
        }
    }

    /// Sets the text-selection overlay for the next [`sync`](Self::sync)
    /// (Round 8.2 — the host's focused session range; `None` derives
    /// byte-identical markup to pre-8.2).
    pub fn set_selection(&mut self, sel: Option<oppa::SelectionPaint>) {
        self.selection = sel;
    }

    /// Sets the caret bar overlay for the next [`sync`](Self::sync)
    /// (Round 15.1 — the host's focused caret; `None` derives
    /// byte-identical markup to pre-15.1).
    pub fn set_caret(&mut self, caret: Option<oppa::CaretPaint>) {
        self.caret = caret;
    }

    /// Sets the page-chrome theme for the next [`sync`](Self::sync)
    /// (theme contract round — the runner's host mode, published per
    /// frame like the overlays above). Change-only: repeats are free,
    /// a flip arms one touched-work unit so its stanza emits.
    pub fn set_theme_mode(&mut self, mode: ThemeMode) {
        if mode != self.theme {
            self.theme = mode;
            self.theme_dirty = true;
        }
    }

    /// The current page-chrome theme (diagnostics/tests).
    pub fn theme_mode(&self) -> ThemeMode {
        self.theme
    }

    /// Feeds committed diffs to the keyframe tracker (Phase 36 PR4,
    /// decision 357): the paint hook calls this every frame with the
    /// frame's diffs (possibly none) — settling runs unconditionally
    /// so diff-less animation frames still retire tracks (otherwise
    /// stepped values would stick past the settle), then each diff
    /// tracks in order with the frame clock. Same diffs, same order,
    /// same clock as the host evaluator — deterministic twins.
    pub fn track_animation(
        &mut self,
        diffs: &[oppa::TreeDiff],
        rec: &oppa::Reconciler,
        styles: &oppa::interner::Interner<oppa::Style>,
        now: f64,
    ) {
        self.kf_eval.settle(now);
        for diff in diffs {
            self.kf_eval.track_commit(diff, rec, styles, now);
        }
        self.kf_now = now;
    }
}

fn parent_is_scroll(rec: &Reconciler, id: NodeId) -> bool {
    rec.get(id)
        .and_then(|n| n.parent)
        .and_then(|p| rec.get(p))
        .is_some_and(|p| p.tag == Tag::ScrollArea)
}

/// True for transparent text wrappers (the `Text`-struct shape: hint,
/// no own payload — layout pass-throughs, never visuals). They render
/// as static spans so absolutely-positioned payload children resolve
/// against the nearest real container instead of a degenerate
/// zero-width wrapper block.
fn is_static_wrapper(rec: &Reconciler, id: NodeId) -> bool {
    rec.get(id).is_some_and(|n| {
        n.tag == Tag::Text
            && n.text.as_ref().is_none_or(|t| t.is_empty())
            && !n
                .semantics
                .as_ref()
                .is_some_and(|s| s.role == Role::TextField || s.role == Role::TextArea)
    })
}

/// Stage-space origin of the node's positioned ancestor (the box to
/// subtract for ancestor-relative offsets). Skips static wrappers;
/// slots resolve against their ScrollArea (the spacer sits at the
/// container origin, so spacer-relative == container-relative).
/// `(0,0)` when no ancestor carries a box yet (pre-layout).
fn positioned_origin(rec: &Reconciler, id: NodeId) -> (f32, f32) {
    let mut cur = rec.get(id).and_then(|n| n.parent);
    while let Some(p) = cur {
        if !is_static_wrapper(rec, p) {
            return rec
                .get(p)
                .and_then(|n| n.layout.as_ref())
                .map(|b| (b.x, b.y))
                .unwrap_or((0.0, 0.0));
        }
        cur = rec.get(p).and_then(|n| n.parent);
    }
    (0.0, 0.0)
}

/// Measured font style for a canvas element (Phase 36 PR4): family +
/// size from the first laid line's first run (the `text_runs`
/// pattern) — canvas `<text>` inherits it. Empty when nothing laid
/// (no text ops, or unmeasured — never an invented family).
fn canvas_font(rec: &Reconciler, id: NodeId, dpr: f32) -> String {
    let Some(n) = rec.get(id) else {
        return String::new();
    };
    let lines = n.layout.as_ref().map(|b| b.lines.as_slice()).unwrap_or(&[]);
    let Some(line) = lines.first() else {
        return String::new();
    };
    let Some(run) = line.runs.first() else {
        return String::new();
    };
    if line.em_size <= 0.0 {
        return String::new();
    }
    format!(
        "font-family:{};font-size:{}px;",
        css_family(&run.family),
        css_num(line.em_size / dpr.max(f32::EPSILON))
    )
}

/// Measured font style for a verdict-(b) input (family + size from
/// the value-carrying descendant's committed lines): the input renders
/// the framework-measured text, so caret geometry and IME anchoring
/// agree with the engine. Empty before layout.
fn field_font(rec: &Reconciler, id: NodeId, dpr: f32) -> String {
    let mut stack = vec![id];
    while let Some(cur) = stack.pop() {
        let kids = match rec.get(cur) {
            Some(n) => {
                if let Some(line) = n.layout.as_ref().and_then(|b| b.lines.first()) {
                    if line.em_size > 0.0 {
                        let family = line.runs.first().map(|r| r.family.as_str()).unwrap_or("");
                        return format!(
                            "font-family:{};font-size:{}px;",
                            css_family(family),
                            css_num(line.em_size / dpr.max(f32::EPSILON))
                        );
                    }
                }
                n.children.clone()
            }
            None => continue,
        };
        for child in kids.iter().rev() {
            stack.push(*child);
        }
    }
    String::new()
}

/// Value vs placeholder split for a verdict-(b) field (decision
/// 293): the value is the first text payload whose direct parent is
/// a `Tag::Text` node carrying `TextField`/`TextArea` semantics (the
/// bound leaf — the exact shape the `TextField`/`TextArea`
/// conversions produce, so layout treats it as text); every other
/// subtree text is the placeholder (the presentational span the
/// control renders while the value is empty). Empty value +
/// placeholder renders `value=""` with a `placeholder` attribute —
/// the placeholder is never the value (the control contract), so
/// the browser only ever sends typed text. Hand-built fields carry
/// no placeholder span and render exactly like before.
fn field_texts(rec: &Reconciler, id: NodeId) -> (String, String) {
    let mut value = String::new();
    let mut placeholder = String::new();
    // (node, parent_is_value_leaf): the parent decides the bucket,
    // never the node itself (bare text nodes never carry
    // semantics — the leaf Element does).
    let mut stack = vec![(id, false)];
    while let Some((cur, bucket)) = stack.pop() {
        let Some(n) = rec.get(cur) else {
            continue;
        };
        if let Some(t) = &n.text {
            if bucket {
                if value.is_empty() {
                    value = t.to_string();
                }
            } else if placeholder.is_empty() {
                placeholder = t.to_string();
            }
        }
        let leaf = n.tag == Tag::Text
            && n.semantics
                .as_ref()
                .is_some_and(|s| s.role == Role::TextField || s.role == Role::TextArea);
        for child in n.children.iter().rev() {
            stack.push((*child, leaf));
        }
        if !value.is_empty() && !placeholder.is_empty() {
            break;
        }
    }
    (value, placeholder)
}

/// Native `placeholder` attribute fragment (decision 293): empty
/// when there is no hint (no attribute emitted — a bare
/// `placeholder=""` would still be honest, but silence beats noise
/// for value-carrying fields), escaped at the render boundary like
/// every other attribute.
fn placeholder_attr(hint: &str) -> String {
    if hint.is_empty() {
        String::new()
    } else {
        format!(" placeholder=\"{}\"", esc(hint))
    }
}

/// Canvas spec → inner SVG markup (Phase 36 PR4, decision 358):
/// rects/paths lower directly (authoring px, 1:1 like `Vector`);
/// text ops lower to `<text>` with their size/fill at the op origin
/// (family inherits the element's inline font — see `derive`,
/// which seeds it from the first laid line, the `text_runs`
/// pattern). Pure over the spec (headless-testable).
fn canvas_inner_svg(spec: &oppa::CanvasSpec) -> String {
    use oppa::CanvasOp;
    let mut out = String::new();
    for op in &spec.ops {
        match op {
            CanvasOp::Rect { x, y, w, h, color } => {
                out.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
                    css_num(x.get()),
                    css_num(y.get()),
                    css_num(w.get()),
                    css_num(h.get()),
                    dom_hex(*color),
                ));
            }
            CanvasOp::RRect {
                x,
                y,
                w,
                h,
                radius,
                color,
            } => {
                out.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\"/>",
                    css_num(x.get()),
                    css_num(y.get()),
                    css_num(w.get()),
                    css_num(h.get()),
                    css_num(radius.get()),
                    dom_hex(*color),
                ));
            }
            CanvasOp::Path { data, fill, stroke } => {
                out.push_str(&format!(
                    "<path d=\"{}\" fill=\"{}\" stroke=\"{}\"{} stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
                    esc(data),
                    fill.map(dom_hex).unwrap_or_else(|| "none".to_string()),
                    stroke.map(|s| dom_hex(s.color)).unwrap_or_else(|| "none".to_string()),
                    stroke.map(|s| format!(" stroke-width=\"{}\"", css_num(s.width))).unwrap_or_default(),
                ));
            }
            CanvasOp::Text {
                text,
                size_px,
                ink,
                x,
                y,
                ..
            } => {
                out.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
                    css_num(x.get()),
                    css_num(y.get()),
                    css_num(*size_px as f32),
                    dom_hex(*ink),
                    esc(text),
                ));
            }
        }
    }
    out
}
/// Span content + per-run segmentation + measured font style for a text
/// node: the OWN payload runs through the committed lines' byte ranges
/// (the layout's segmentation, never re-measured here). Wrapper Text
/// nodes (hint, no payload — the `Text`-struct shape) render their
/// children only, never the descendant string again (duplicating it
/// would double content and dirty parents on every value change).
fn text_runs(rec: &Reconciler, id: NodeId, dpr: f32) -> (String, Vec<DomRun>, String) {
    let Some(n) = rec.get(id) else {
        return (String::new(), Vec::new(), String::new());
    };
    let payload: String = n.text.clone().map(|t| t.to_string()).unwrap_or_default();
    let lines = n.layout.as_ref().map(|b| b.lines.as_slice()).unwrap_or(&[]);
    let em = lines.first().map(|l| l.em_size).unwrap_or(0.0);
    let runs = merge_text_runs(lines, &payload);
    let text = if runs.is_empty() {
        payload
    } else {
        runs.iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join("")
    };
    let inline_font = if em > 0.0 {
        let family = runs.first().map(|r| r.family.clone()).unwrap_or_default();
        format!(
            "font-family:{};font-size:{}px;white-space:pre;",
            css_family(&family),
            css_num(em / dpr.max(f32::EPSILON))
        )
    } else {
        String::new()
    };
    (text, runs, inline_font)
}

/// Merges committed line runs into render runs (Phase 36 PR3):
/// slices the payload by each non-empty run's byte range, joins
/// consecutive same-`(family, ink)` runs back into one `<span>`
/// (fallback boundaries split, and so do span-ink boundaries).
/// Pure over lines + payload (headless-testable, never re-measured).
fn merge_text_runs(lines: &[oppa::LaidLine], payload: &str) -> Vec<DomRun> {
    let mut runs: Vec<DomRun> = Vec::new();
    for line in lines {
        for run in &line.runs {
            if run.glyphs.is_empty() {
                continue;
            }
            let slice = payload
                .get(run.byte_range.0.min(payload.len())..run.byte_range.1.min(payload.len()))
                .unwrap_or("")
                .to_string();
            if slice.is_empty() {
                continue;
            }
            let ink = run.ink.map(dom_hex);
            match runs.last_mut() {
                Some(last) if last.family == run.family && last.ink == ink => {
                    last.text.push_str(&slice)
                }
                _ => runs.push(DomRun {
                    family: run.family.clone(),
                    ink,
                    text: slice,
                }),
            }
        }
    }
    runs
}

/// Absolute geometry inline style. Root = relative container (no
/// offsets); every other positioned node = absolute at its committed
/// box minus the positioned-ancestor origin (browser layout runs for
/// nothing — the §8.5 flat-subset rule). Static wrappers
/// (`positioned == false`) carry nothing — pass-throughs, never
/// visuals. Viewport height is the committed box for scroll containers
/// too; the spacer (see `spacer_h`) carries the content extent. Slot
/// anchoring is NOT positional CSS (non-slots must keep anchoring):
/// the page stylesheet carries `[data-slot]{overflow-anchor:none}`
/// instead (see page.rs) — slots are marked via attrs in `derive`.
fn geom_inline(
    layout: Option<&oppa::LayoutBox>,
    is_root: bool,
    kind: &HtmlKind,
    dpr: f32,
    origin: (f32, f32),
    positioned: bool,
) -> String {
    let Some(b) = layout else {
        return String::new();
    };
    let dpr = dpr.max(f32::EPSILON);
    if is_root {
        return format!(
            "position:relative;width:{}px;height:{}px;",
            css_num(b.w / dpr),
            css_num(b.h / dpr)
        );
    }
    if !positioned {
        return String::new();
    }
    if *kind == HtmlKind::Text {
        // Text spans are width/height-auto: the browser shrink-wraps
        // the content, so the parity corpus measures the BROWSER's
        // text width against the engine's (a forced width would echo
        // our own number back — vacuous). Blocks keep explicit boxes.
        return format!(
            "position:absolute;left:{}px;top:{}px;",
            css_num((b.x - origin.0) / dpr),
            css_num((b.y - origin.1) / dpr)
        );
    }
    if *kind == HtmlKind::Field && (b.w <= 0.0 || b.h <= 0.0) {
        // U8 scoped sizing (decision 189): a zero box means
        // unmeasured (no TextService) or empty — never echo it back
        // as 0px (an invisible, unfocusable input). The browser owns
        // field presentation under verdict-(b) and sizes the input
        // intrinsically. Measured boxes keep explicit geometry below.
        return format!(
            "position:absolute;left:{}px;top:{}px;",
            css_num((b.x - origin.0) / dpr),
            css_num((b.y - origin.1) / dpr)
        );
    }
    format!(
        "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;",
        css_num((b.x - origin.0) / dpr),
        css_num((b.y - origin.1) / dpr),
        css_num(b.w / dpr),
        css_num(b.h / dpr)
    )
}

/// Stable per-node id for the wire (`data-pid` — the keyed-patch
/// address, Round 12.1).
pub fn pid_of(id: NodeId) -> String {
    format!("{}-{}", id.gen().index(), id.gen().generation())
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

fn css_num(v: f32) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Image `src` resolution (round 4.4 + Phase 36 PR4, decision 359):
/// pre-decoded cache pixels win as a PNG data URI (static path);
/// otherwise the cache key renders as the URL (round-4.4 rule);
/// `None` when the cache knows neither (refuse loudly downstream).
/// Pure over the cache (headless-testable — no retained reads).
fn image_src(images: &oppa::ImageCache, id: oppa::ImageId) -> Option<String> {
    images
        .pixels_of(id)
        .and_then(|(w, h, rgba)| oppa_image::encode_png_rgba(w, h, &rgba).ok())
        .map(|png| format!("data:image/png;base64,{}", base64_encode(&png)))
        .or_else(|| images.key_of(id))
}

/// Minimal base64 encoder (Phase 36 PR3): standard alphabet, `=`
/// padding — just enough for `@font-face` data URIs, kept inline so
/// the backend adds no dependency for one call site.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut n: u32 = 0;
        for (i, b) in chunk.iter().enumerate() {
            n |= (*b as u32) << (16 - 8 * i);
        }
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Hex fill/stroke for vector paths (the stylesheet's `fmt_hex`
/// rule, duplicated — that helper is private to the css module and
/// the rule is one line, stated here rather than re-exported).
pub(crate) fn dom_hex(c: oppa::Color) -> String {
    format!("#{:06x}", c.0 & 0x00FF_FFFF)
}

fn css_family(family: &str) -> String {
    if family.is_empty() {
        "sans-serif".to_string()
    } else if family
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        // Single-token names need no quotes (avoids quote-escaping
        // noise in the common case; quoted names still escape at the
        // render boundary — finding F5).
        format!("{family},sans-serif")
    } else {
        format!("\"{}\",sans-serif", family.replace('"', ""))
    }
}

/// Inline `transition:none` for stamped elements (M8, §9.4): the CSS
/// half of the binding-edge stamp — the stamped commit's touched
/// elements write target values with transitions disabled for exactly
/// that commit. Empty for unstamped elements (the declaration itself
/// rides the shared class rule).
fn transition_inline(el: &DomElement) -> &'static str {
    if el.no_transition {
        "transition:none;"
    } else {
        ""
    }
}

impl RendererBackend for DomBackend {
    fn kind(&self) -> PresenterKind {
        PresenterKind::Dom
    }

    fn caps(&self) -> Caps {
        Caps::dom()
    }

    fn create_surface(&mut self, desc: SurfaceDesc) -> Result<SurfaceId, BackendError> {
        if desc.width_px == 0 || desc.height_px == 0 {
            return Err(BackendError::BadSurface(format!(
                "zero-size DOM surface {}x{}",
                desc.width_px, desc.height_px
            )));
        }
        let id = SurfaceId(self.next_surface);
        self.next_surface += 1;
        self.surfaces.insert(id, desc);
        Ok(id)
    }

    fn destroy_surface(&mut self, id: SurfaceId) -> Result<(), BackendError> {
        self.surfaces
            .remove(&id)
            .map(|_| ())
            .ok_or(BackendError::UnknownSurface(id))
    }

    /// Absorbs the edit script into the element registry (structure only
    /// — content re-derives in [`sync`](Self::sync)). Index violations
    /// panic loudly (a reconciler bug, never a silent relink). A stamped
    /// diff arms the one-commit transition suppression the next `sync`
    /// consumes (M8, §9.4).
    fn commit(&mut self, diff: &TreeDiff) -> Result<(), BackendError> {
        use oppa::DiffOp;
        self.suppress_armed |= diff.suppress_transitions;
        for op in &diff.ops {
            match op {
                DiffOp::Add {
                    id, parent, index, ..
                } => {
                    self.elements.insert(
                        *id,
                        DomElement {
                            node: *id,
                            kind: HtmlKind::Block,
                            classes: Vec::new(),
                            inline_geom: String::new(),
                            inline_font: String::new(),
                            text: String::new(),
                            runs: Vec::new(),
                            attrs: Vec::new(),
                            children: Vec::new(),
                            spacer_h: None,
                            placeholder: String::new(),
                            sel_rects: Vec::new(),
                            caret_rect: None,
                            caret_ink: String::new(),
                            no_transition: false,
                            inline_kf: String::new(),
                        },
                    );
                    match parent {
                        Some(p) => {
                            let el = self.elements.get_mut(p).unwrap_or_else(|| {
                                panic!("dom commit: Add under unknown parent {p:?}")
                            });
                            assert!(
                                *index <= el.children.len(),
                                "dom commit: Add index {index} beyond {} children on {p:?}",
                                el.children.len()
                            );
                            el.children.insert(*index, *id);
                            self.mutations += 1;
                        }
                        None => {
                            assert!(
                                *index <= self.roots.len(),
                                "dom commit: root Add index out of range"
                            );
                            self.roots.insert(*index, *id);
                            self.mutations += 1;
                        }
                    }
                }
                DiffOp::Remove { id } => {
                    let mut stack = vec![*id];
                    while let Some(cur) = stack.pop() {
                        if let Some(el) = self.elements.remove(&cur) {
                            stack.extend(el.children);
                        }
                        self.absorbed.remove(&cur);
                        self.scroll_tops.remove(&cur);
                    }
                    for el in self.elements.values_mut() {
                        if let Some(pos) = el.children.iter().position(|c| c == id) {
                            el.children.remove(pos);
                        }
                    }
                    self.roots.retain(|r| r != id);
                    self.mutations += 1;
                }
                DiffOp::Move { id, new_index } => {
                    let parent = self
                        .elements
                        .values()
                        .find(|el| el.children.contains(id))
                        .map(|el| el.node);
                    if let Some(p) = parent {
                        let el = self
                            .elements
                            .get_mut(&p)
                            .expect("dom commit: Move parent vanished");
                        let pos = el
                            .children
                            .iter()
                            .position(|c| c == id)
                            .expect("dom commit: Move target not under its parent");
                        el.children.remove(pos);
                        assert!(
                            *new_index <= el.children.len(),
                            "dom commit: Move index out of range on {p:?}"
                        );
                        el.children.insert(*new_index, *id);
                        self.mutations += 1;
                    } else if let Some(pos) = self.roots.iter().position(|r| r == id) {
                        self.roots.remove(pos);
                        self.roots.insert((*new_index).min(self.roots.len()), *id);
                        self.mutations += 1;
                    } else {
                        panic!("dom commit: Move target {id:?} has no parent");
                    }
                }
                DiffOp::Update { .. } => {
                    // Content re-derives in sync (dirty-compare there);
                    // no structural mutation here by construction.
                }
            }
        }
        Ok(())
    }

    fn paint(
        &mut self,
        surface: SurfaceId,
        plan: &oppa::FramePlan,
    ) -> Result<PaintStats, BackendError> {
        if !self.surfaces.contains_key(&surface) {
            return Err(BackendError::UnknownSurface(surface));
        }
        self.paints += 1;
        let ops = std::mem::replace(&mut self.pending_touched, 0);
        Ok(PaintStats {
            ops_executed: ops,
            paints: self.paints,
            skipped_empty: plan.is_empty() && ops == 0,
        })
    }
}

impl Default for DomBackend {
    fn default() -> Self {
        Self::new(1.0)
    }
}

#[cfg(test)]
mod tests;
