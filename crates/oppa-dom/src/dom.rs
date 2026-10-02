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

mod commit;
mod element;
mod encode;
mod patch;
mod render;
mod scroll;
mod sync;

pub(crate) use element::same_own;
pub use element::{
    json_escape, AttrPatch, DomElement, DomRun, HtmlKind, PagePatch, PlacePatch, SyncStats,
    ThemePatch,
};
#[cfg(test)]
pub(crate) use encode::merge_text_runs;
pub use encode::pid_of;
pub(crate) use encode::{
    base64_encode, canvas_font, canvas_inner_svg, css_family, css_num, dom_hex, esc, field_font,
    field_texts, geom_inline, image_src, is_static_wrapper, parent_is_scroll, placeholder_attr,
    positioned_origin, text_runs, transition_inline,
};
pub use scroll::{scroll_window, scroll_window_overscan, OVERSCAN_SLOTS};

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

    /// Retained node behind a rendered `data-pid` (U8 INPUT-mapping
    /// entry: the page reports the edited input's pid; the shell
    /// resolves it here, then injects `InputEvent::Text`). None =
    /// unknown pid (stale post-swap markup — never a silent node).
    pub fn node_for_pid(&self, pid: &str) -> Option<NodeId> {
        self.elements.keys().find(|id| pid_of(**id) == pid).copied()
    }
}

#[cfg(test)]
mod tests;
