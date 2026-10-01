//! M3 layout engine: framework-owned, written once (locked #6, ADR-0004,
//! BUILD-ORDER M3). Renderers never compute layout; this engine produces one
//! [`LayoutBox`] per retained node (x, y, w, h, content size, shaped text
//! runs) and renderers rasterize what it positioned.
//!
//! v1 scope (constraints spec, locked #6): flexbox subset (Row/Column with
//! `gap`/`pad_x`/`pad_y`/`fill_width`/`fill_height`/`margin_x`/`margin_y`,
//! `align_items` cross-axis + `justify_content` main-axis, Decisions
//! 237/249; `flex_wrap` Row line-breaking, decision 253) + block-lite (Div: full-width
//! vertical stack;
//! Stack: overlay; ScrollArea: explicit viewport + `content_size` spacer) +
//! Portal viewport-anchored overlay layer (decision 255) +
//! absolute positioning (per-axis `.x` / `.absolute_y` overrides) + inline
//! text runs (width-driven wrap, `\n` hard breaks, UBA-lite BiDi visual
//! ordering over the `rtl` run metadata, optional-v1 ellipsis behind a
//! config flag). Grid and variable-height rows are v2 and have no code
//! paths here.
//!
//! Measure↔layout protocol: the engine consumes [`TextService`] (`shape` +
//! `measure_line` via [`ShapedRun`] math) and caches one [`MeasuredText`]
//! per text node inline on the retained node, keyed by (text bytes, resolved
//! size, family, dpr). Wrap re-flows over cached advances, so wrapping never
//! re-shapes. The engine consumes only its own LAYOUT flag (a text-hint
//! change sets it — hint resizes measurement); TEXT/STRUCTURE flags persist
//! for their future M4 consumers, and text changes surface as cache misses,
//! so flag dirt alone never re-shapes.
//!
//! Feedback wiring (§3-spec/layout/constraints.md): settled boxes are read
//! through [`LayoutLedger::settled`], which tracks a generation signal. The
//! engine publishes (bumps the generation) in the LAYOUT phase — after
//! EFFECTS ran — so effects reading settled metrics observe previous-frame
//! values and re-run in the next frame's EFFECTS. Effects must not write
//! what layout reads (retained styles/text, the boxes, the generation):
//! the write handles are framework-owned and not exposed; a feedback loop
//! through app signals is user logic and is documented, not enforced.
//!
//! DPR rounding (coordinate-system spec, §8.8): [`round_to_device_px`] at
//! commit positions (box x/y) only; advances, widths, and line geometry stay
//! subpixel.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::arena::NodeId;
use crate::hash::fnv1a64;
use crate::interner::Interner;
use crate::pass_mask::PassMask;
use crate::reactive::{untrack, Runtime, Signal};
use crate::reconciler::Reconciler;
use crate::style::{AlignItems, FlexWrap, GridTrack, JustifyContent, Style};
use crate::text::{
    round_to_device_px, BreakSource, FontId, ShapedRun, TextError, TextService, TextStyle,
};
use crate::vnode::{Tag, TextClass};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Text inputs the [`Style`] struct does not carry (M3 interpretation
/// decision 68: no locked spec names the default text family/sizes, so they
/// live here as explicit config with documented defaults, not as silent
/// constants). The corpus rig uses Segoe UI 16px; the defaults match it for
/// the title class.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutTextConfig {
    /// Font family handed to the shaper for every text leaf.
    pub family: String,
    /// CSS px for `TextClass::TitleSmall` leaves.
    pub title_px: f32,
    /// CSS px for `TextClass::BodySecondary` leaves.
    pub body_px: f32,
    /// CSS px when no hint resolves (bare `VNode::Text`, image-adjacent).
    pub default_px: f32,
    /// Output scale: advances come back in device px; commit positions snap
    /// to this grid. Viewport inputs are CSS px and are scaled inside.
    pub device_pixel_ratio: f32,
    /// Optional-v1 ellipsis: when true, text under a finite width constraint
    /// truncates to a single line with a trailing "…" instead of wrapping.
    /// Default off (wrap is the v1 default; truncation is opt-in).
    pub ellipsis: bool,
}

impl Default for LayoutTextConfig {
    fn default() -> Self {
        Self {
            family: "Segoe UI".to_string(),
            title_px: 16.0,
            body_px: 14.0,
            default_px: 14.0,
            device_pixel_ratio: 1.0,
            ellipsis: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Output shapes (DESIGN §2.2: `layout: LayoutBox { x, y, w, h, content_size,
// text_runs }` lives on the retained node)
// ---------------------------------------------------------------------------

/// One positioned glyph: renderer-ready, visual left-to-right order.
#[derive(Clone, Debug, PartialEq)]
pub struct LaidGlyph {
    pub glyph_id: u32,
    /// Visual x relative to the text box origin (device px, subpixel).
    pub x: f32,
    pub x_advance: f32,
}

/// One visual run within a line: a single [`TextRun`]'s glyphs on this line.
#[derive(Clone, Debug, PartialEq)]
pub struct LaidRun {
    /// Logical UTF-8 byte span covered (min start, max end over the run's
    /// clusters on this line).
    pub byte_range: (usize, usize),
    pub rtl: bool,
    pub glyphs: Vec<LaidGlyph>,
    /// Shaper-local font identity of the source [`TextRun`]
    /// (M7 lock touch, decision 110 — `layout_text` sets this; the
    /// backend-resolvable `family` name is filled by the layout caller
    /// post-pass, see [`LayoutEngine::family_of`]).
    pub font_id: FontId,
    /// Backend-resolvable family name (CSS `font-family` / atlas-face
    /// key). Empty when laid via [`layout_text`] directly (the pure
    /// unit-test path, which owns no font table); always resolved on
    /// committed boxes.
    pub family: String,
}

/// One cluster positioned in visual order (hit-test/caret source).
#[derive(Clone, Debug, PartialEq)]
pub struct LaidCluster {
    pub byte_range: (usize, usize),
    /// Visual x relative to the text box origin (device px, subpixel).
    pub x: f32,
    pub width: f32,
    pub rtl: bool,
    /// True for the synthesized ellipsis marker (zero-length byte range at
    /// the text end).
    pub ellipsis: bool,
}

/// One laid line: visual runs plus the cluster map in visual order.
#[derive(Clone, Debug, PartialEq)]
pub struct LaidLine {
    /// Line top relative to the text box origin (device px, subpixel).
    pub y: f32,
    pub height: f32,
    /// Baseline offset from the line top (== ascent).
    pub baseline: f32,
    /// Exact em size (device px, subpixel — `font_size_px × dpr` at
    /// measure time). `layout_text` leaves this 0.0: the pure function
    /// owns no measure context, so the layout caller fills it post-pass
    /// (see `layout_text_leaf`); always resolved on committed boxes.
    /// (M7 lock touch, decision 110.)
    pub em_size: f32,
    /// Visual width used (sum of cluster widths).
    pub width: f32,
    pub runs: Vec<LaidRun>,
    pub clusters: Vec<LaidCluster>,
}

impl LaidLine {
    /// Caret x for a logical byte offset within this line, forward affinity
    /// (where the next character begins): an LTR cluster's left edge, an
    /// RTL cluster's right edge. At direction boundaries this differs from
    /// the prefix end — one logical position, two visual positions (the
    /// UBA trailing-edge duality; DirectWrite exposes it via
    /// `HitTestTextPosition`'s trailing-edge flag). The line ends follow
    /// the same rule: byte 0 sits before the first logical cluster, the
    /// trailing caret after the last logical one (not at the line width —
    /// an RTL-final line's trailing caret is mid-line, as the oracle
    /// proves). Mid-cluster bytes snap to their cluster.
    /// Wrap-point forward affinity (decision 195): any byte at or
    /// before the line's first cluster reads its leading edge, so a
    /// break byte (trailing blanks trimmed from the prior line, or a
    /// hard-break newline) belongs to this line's leading caret.
    pub fn caret_x(&self, byte_offset: usize) -> f32 {
        let real: Vec<&LaidCluster> = self.clusters.iter().filter(|c| !c.ellipsis).collect();
        let Some(first) = real.iter().min_by_key(|c| c.byte_range.0).copied() else {
            return 0.0;
        };
        if byte_offset <= first.byte_range.0 {
            return if first.rtl {
                first.x + first.width
            } else {
                first.x
            };
        }
        for c in &real {
            if byte_offset >= c.byte_range.0 && byte_offset < c.byte_range.1 {
                return if c.rtl { c.x + c.width } else { c.x };
            }
        }
        let last = real
            .iter()
            .max_by_key(|c| c.byte_range.1)
            .copied()
            .expect("non-empty");
        if last.rtl {
            last.x
        } else {
            last.x + last.width
        }
    }
}

/// The per-node box the engine commits (DESIGN §2.2 sketch, M3-real).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LayoutBox {
    /// Commit position (device px, snapped via [`round_to_device_px`]).
    pub x: f32,
    /// Commit position (device px, snapped via [`round_to_device_px`]).
    pub y: f32,
    /// Outer width/height (device px, subpixel preserved).
    pub w: f32,
    pub h: f32,
    /// Scrollable/overflowing extent (device px, subpixel preserved).
    /// `content_size` style floors `content_h`; ScrollArea viewports read it.
    pub content_w: f32,
    pub content_h: f32,
    /// Laid text lines (empty for non-text nodes).
    pub lines: Vec<LaidLine>,
}

impl LayoutBox {
    /// Caret position for a logical byte offset in a text box: the line
    /// holding the byte (first covering line wins) and its visual x.
    /// Wrap-point forward affinity (decision 195): a break byte lives
    /// in no line (trailing blanks are trimmed from line N, the
    /// newline cluster is dropped) — it belongs to the next line's
    /// leading caret, i.e. the first line starting at or after the
    /// byte. Past the end, the last line's trailing caret (unchanged).
    /// Returns `(0, 0.0)` for boxes without lines.
    pub fn caret_position(&self, byte_offset: usize) -> (usize, f32) {
        for (i, line) in self.lines.iter().enumerate() {
            if line.clusters.iter().any(|c| {
                !c.ellipsis && byte_offset >= c.byte_range.0 && byte_offset < c.byte_range.1
            }) {
                return (i, line.caret_x(byte_offset));
            }
        }
        for (i, line) in self.lines.iter().enumerate() {
            let start = line
                .clusters
                .iter()
                .filter(|c| !c.ellipsis)
                .map(|c| c.byte_range.0)
                .min();
            if start.is_some_and(|s| s >= byte_offset) {
                return (i, line.caret_x(byte_offset));
            }
        }
        match self.lines.last() {
            Some(line) => (self.lines.len() - 1, line.caret_x(byte_offset)),
            None => (0, 0.0),
        }
    }

    /// Selection highlight rects for `range` (Round 8.2, decision 298):
    /// one box-space `[x0, y0, x1, y1]` rect per laid line overlapping
    /// the ordered byte range (edges via [`LaidLine::caret_x`], so RTL
    /// and wrap affinity match the caret). Empty overlap and collapsed
    /// ranges yield no rects. Shared by the FramePlan builder (CPU/Vello
    /// `Rect` emission) and the DOM backend (highlight divs) — one rule,
    /// every presenter agrees by construction.
    pub fn selection_rects(&self, range: (usize, usize)) -> Vec<[f32; 4]> {
        let (lo, hi) = (range.0.min(range.1), range.0.max(range.1));
        if lo >= hi {
            return Vec::new();
        }
        let mut out = Vec::new();
        for line in &self.lines {
            let mut cov: Option<(usize, usize)> = None;
            for c in line.clusters.iter().filter(|c| !c.ellipsis) {
                cov = Some(match cov {
                    None => (c.byte_range.0, c.byte_range.1),
                    Some((a, b)) => (a.min(c.byte_range.0), b.max(c.byte_range.1)),
                });
            }
            let Some((c0, c1)) = cov else { continue };
            let (o0, o1) = (lo.max(c0), hi.min(c1));
            if o0 >= o1 {
                continue;
            }
            let x0 = self.x + line.caret_x(o0);
            let x1 = self.x + line.caret_x(o1);
            out.push([
                x0.min(x1),
                self.y + line.y,
                x0.max(x1),
                self.y + line.y + line.height,
            ]);
        }
        out
    }
}

/// Scrollbar track width, device px (Round 17.2, decision 318 —
/// overlay grab target: wide enough to hit reliably without
/// stealing content; a reasoned constant, not a derived law —
/// override by editing this const with a new decision).
pub const SCROLLBAR_TRACK_PX: f32 = 12.0;

/// Scrollbar hover/press gutter width, device px (follow-up —
/// the painted track stays [`SCROLLBAR_TRACK_PX`], but the hit
/// node spans this wider gutter leftwards over the content edge
/// so the chrome summons without pixel-hunting; presses inside
/// the gutter page/drag instead of reaching the covered content
/// strip — the native overlay-scrollbar tradeoff, stated).
pub const SCROLLBAR_HIT_PX: f32 = 20.0;

/// Minimum thumb extent, device px (Round 17.2 — a usable grab
/// target at any content scale; never smaller).
pub const SCROLLBAR_MIN_THUMB_PX: f32 = 24.0;

/// Thumb geometry, viewport-relative (Round 17.2, decision 318):
/// `y` is the thumb-top offset from the track top, `h` the thumb
/// extent. Shared by every presenter (and the hit-test rule), so
/// CPU, Vello, and DOM agree by construction.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ScrollbarThumb {
    pub y: f32,
    pub h: f32,
}

/// Horizontal thumb geometry (Phase 36 PR2b, decision 354 — G15):
/// the exact transpose of [`ScrollbarThumb`] — `x` is the
/// thumb-left offset from the track left, `w` the thumb extent.
/// Same formula, same contract, one axis over.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ScrollbarThumbX {
    pub x: f32,
    pub w: f32,
}

/// Maximum scroll offset for an extent (`content - viewport`,
/// floored at zero — no negative travel, never NaN from empty
/// boxes).
pub fn scrollbar_max_offset(viewport_h: f32, content_h: f32) -> f32 {
    if !viewport_h.is_finite() || !content_h.is_finite() {
        panic!(
            "scrollbar extent must be finite, got viewport {viewport_h} content {content_h} — \
             refuse, never paint"
        );
    }
    (content_h - viewport_h).max(0.0)
}

/// Thumb rect for a scrolled extent (Round 17.2, decision 318):
/// `h = max(24, viewport² / content)` shrinking with growth,
/// `y` linear in the clamped offset over the travel
/// (`viewport - h`). `None` without overflow or on degenerate
/// (zero/negative) boxes — quiet, never a zero-height thumb.
/// Transient overscroll clamps into range (normal operation —
/// the unclamped vertical feed routinely overshoots); only
/// non-finite inputs refuse loudly (the layout rule).
pub fn scrollbar_thumb(viewport_h: f32, content_h: f32, offset: f32) -> Option<ScrollbarThumb> {
    if !viewport_h.is_finite() || !content_h.is_finite() || !offset.is_finite() {
        panic!(
            "scrollbar geometry must be finite, got viewport {viewport_h} content {content_h} \
             offset {offset} — refuse, never paint"
        );
    }
    if viewport_h <= 0.0 || content_h <= viewport_h {
        return None;
    }
    let h = (viewport_h * viewport_h / content_h).max(SCROLLBAR_MIN_THUMB_PX);
    // A min-clamped thumb taller than the track leaves no travel —
    // pin it full-track instead of a negative run.
    let h = h.min(viewport_h);
    let travel = viewport_h - h;
    let max = scrollbar_max_offset(viewport_h, content_h);
    let y = if travel <= 0.0 || max <= 0.0 {
        0.0
    } else {
        offset.clamp(0.0, max) / max * travel
    };
    Some(ScrollbarThumb { y, h })
}

/// Horizontal thumb rect (Phase 36 PR2b, decision 354 — G15): the
/// [`scrollbar_thumb`] formula transposed (`w = max(24, viewport² /
/// content)`, `x` linear in the clamped offset). Same loudness (only
/// non-finite inputs refuse), same quiet `None` without overflow.
pub fn scrollbar_thumb_x(viewport_w: f32, content_w: f32, offset: f32) -> Option<ScrollbarThumbX> {
    if !viewport_w.is_finite() || !content_w.is_finite() || !offset.is_finite() {
        panic!(
            "scrollbar geometry must be finite, got viewport {viewport_w} content {content_w} \
             offset {offset} — refuse, never paint"
        );
    }
    if viewport_w <= 0.0 || content_w <= viewport_w {
        return None;
    }
    let w = (viewport_w * viewport_w / content_w).max(SCROLLBAR_MIN_THUMB_PX);
    // A min-clamped thumb wider than the track leaves no travel —
    // pin it full-track instead of a negative run.
    let w = w.min(viewport_w);
    let travel = viewport_w - w;
    let max = scrollbar_max_offset(viewport_w, content_w);
    let x = if travel <= 0.0 || max <= 0.0 {
        0.0
    } else {
        offset.clamp(0.0, max) / max * travel
    };
    Some(ScrollbarThumbX { x, w })
}

// ---------------------------------------------------------------------------
// Measure cache (inline on the retained node; lifecycle is the node's)
// ---------------------------------------------------------------------------

/// Cache key: re-shape iff the bytes or the resolved style changed. Flags
/// alone never re-shape. Weight rides the key (decision 239) so a bold
/// re-style never hits a regular-weight cache entry.
fn measure_key(
    text: &str,
    size_px: f32,
    weight: crate::text::FontWeight,
    cfg: &LayoutTextConfig,
) -> TextMeasureKey {
    TextMeasureKey {
        bytes_hash: fnv1a64(text.as_bytes()),
        size_bits: size_px.to_bits(),
        dpr_bits: cfg.device_pixel_ratio.to_bits(),
        family_hash: fnv1a64(cfg.family.as_bytes()),
        weight: weight.0,
    }
}

/// Light pre-pass for run gating: true iff some text leaf's cache key
/// misses (fresh node, changed bytes, or changed resolved size/weight —
/// the TEXT-only change surfaces here, not via flags). Weight inherits
/// through transparent `Text` wrappers exactly like the size does (the
/// measured bare-text leaf always carries hint `None` — see
/// [`LayoutCtx::layout_node`]).
fn needs_measure(rec: &Reconciler, cfg: &LayoutTextConfig, root: NodeId) -> bool {
    let mut stack = vec![(root, cfg.default_px, crate::text::FontWeight::NORMAL)];
    while let Some((id, inherited, inherited_weight)) = stack.pop() {
        let Some(n) = rec.get(id) else {
            continue;
        };
        if n.tag == Tag::Text {
            if let Some(text) = &n.text {
                if !text.is_empty() {
                    let size = resolve_text_px(n.text_hint, inherited, cfg);
                    let weight = resolve_text_weight(n.text_hint, inherited_weight);
                    let key = measure_key(text, size, weight, cfg);
                    if n.measured.as_ref().is_none_or(|m| m.key != key) {
                        return true;
                    }
                }
            }
        }
        let inner = resolve_text_px(n.text_hint, inherited, cfg);
        let inner_weight = resolve_text_weight(n.text_hint, inherited_weight);
        for child in n.children.iter().rev() {
            stack.push((*child, inner, inner_weight));
        }
    }
    false
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct TextMeasureKey {
    bytes_hash: u64,
    size_bits: u32,
    dpr_bits: u32,
    family_hash: u64,
    weight: u16,
}

/// One text node's cached measurement: the shaped run plus its metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct MeasuredText {
    key: TextMeasureKey,
    pub shaped: ShapedRun,
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
}

// ---------------------------------------------------------------------------
// Stats (the wrap round-trip instrument: `nodes_shaped` counts `shape()`
// calls the engine performed this run)
// ---------------------------------------------------------------------------

/// Mask bits the FramePlan builder consumes (M4 `FRAME_MASK`): a box that
/// changed while carrying none of these is a position-only LAYOUT move
/// (finding F1) — the engine stamps PAINT on it (decision 104) so the
/// builder rebuilds it instead of replaying stale pixels.
const FRAME_DIRT: PassMask = PassMask::from_bits(
    PassMask::STRUCTURE.bits()
        | PassMask::STYLE.bits()
        | PassMask::PAINT.bits()
        | PassMask::TEXT.bits(),
);

/// Per-run engine accounting. `nodes_shaped` is the measured wrap
/// round-trip count: shapes performed, not lines laid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LayoutStats {
    pub nodes_visited: usize,
    pub nodes_shaped: usize,
    pub lines_laid: usize,
    pub boxes_changed: usize,
    pub boxes_dropped: bool,
    pub generation_bumped: bool,
    /// Boxes whose change carried no paint-relevant dirt and were
    /// therefore stamped PAINT engine-side (F1 resolution, decision
    /// 104). Measured per run; the builder test asserts the count.
    pub paint_stamped: usize,
    /// Flow passes over the tree (2 when Row fill-width redistribution
    /// re-flows fill children into their shares; else 1; 0 when skipped).
    pub layout_passes: u32,
    /// True when the dirty set was empty and nothing ran.
    pub empty: bool,
}

// ---------------------------------------------------------------------------
// Engine + ledger (one-frame-delay wiring)
// ---------------------------------------------------------------------------

/// The engine: config plus amortized caches (ellipsis advances per style).
/// Owned by [`LayoutLedger`]; never in hot crates (lock #25 residence).
pub struct LayoutEngine {
    config: LayoutTextConfig,
    ellipsis_widths: HashMap<(u64, u32, u32, u16), f32>,
    /// Shaper-local [`FontId`] → backend-resolvable family name, built
    /// from the installed [`TextService`]'s enumeration (M7, decision
    /// 110). The font set is stable per decision 16, so the table
    /// refreshes only while empty; ids the shaper reports but the table
    /// never named resolve to the requested family (stated fallback —
    /// e.g. headless fakes with an empty enumeration).
    font_names: HashMap<FontId, String>,
    /// The [`BreakSource`] the opportunity wrap path consumes (v2 item
    /// 2, decision 193). None -> legacy greedy cluster-boundary wrap
    /// (all existing M3 behavior preserved byte-for-byte).
    break_source: Option<Rc<dyn BreakSource>>,
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
    engine: LayoutEngine,
    generation: Signal<u64>,
    last_stats: LayoutStats,
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
    if !dropped && !layout_dirty && !needs_measure(rec, &engine.config, root) {
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

struct LayoutCtx<'a> {
    rec: &'a mut Reconciler,
    styles: &'a Interner<Style>,
    service: Option<&'a dyn TextService>,
    engine: &'a mut LayoutEngine,
    stats: &'a mut LayoutStats,
    dpr: f32,
    /// Viewport width in device px (Round 1.4: portal layers anchor to
    /// it instead of their parent's constraint).
    viewport_w: f32,
    /// Viewport height in device px (Round 7.21, decision 296:
    /// unconstrained portals default to full-viewport height, so
    /// modal dimming covers the window instead of hugging content).
    viewport_h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Size {
    w: f32,
    h: f32,
}

/// Per-side padding in device px (Round 11.1 — resolved once in
/// [`resolve_pad`], read everywhere as one shape, so symmetric
/// trees flow byte-identically through the new formulas).
#[derive(Clone, Copy, Debug)]
struct Pad {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl Pad {
    /// Horizontal inset pair (content width shrinks by this).
    fn x(&self) -> f32 {
        self.left + self.right
    }

    /// Vertical inset pair (content height shrinks by this).
    fn y(&self) -> f32 {
        self.top + self.bottom
    }
}

/// Per-side margins in device px (Round 11.1 — resolved once in
/// [`resolve_margin`], same single-shape rule as [`Pad`]).
#[derive(Clone, Copy, Debug)]
struct Margin {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl Margin {
    /// Horizontal pair (a container's auto width grows by this).
    fn x(&self) -> f32 {
        self.left + self.right
    }

    /// Vertical pair (a container's auto height grows by this).
    fn y(&self) -> f32 {
        self.top + self.bottom
    }
}

/// Loud style validation (Decision 237: loud failures over silent
/// degradation). NaN/infinite in any layout-relevant field panics; negative
/// sizes/pads/gaps/content floors panic (offsets `x`/`absolute_y` may be
/// negative but must still be finite). Called per node at layout entry.
fn check_layout_px(field: &str, v: f32, allow_negative: bool) {
    if !v.is_finite() {
        panic!("layout: style.{field} is non-finite ({v}) — NaN/Inf never lays out silently");
    }
    if !allow_negative && v < 0.0 {
        panic!("layout: style.{field} is negative ({v}) — negative sizes/pads/gaps never lay out silently");
    }
}

fn resolve_pad(style: &Style, dpr: f32) -> Pad {
    // Round 11.1: each set side wins over its symmetric shorthand
    // (CSS-shorthand rule — `pad_x`/`pad_y` stay the concise path).
    let px = style.pad_x.map(|p| p.get()).unwrap_or(0.0);
    let py = style.pad_y.map(|p| p.get()).unwrap_or(0.0);
    let top = style.pad_top.map(|p| p.get()).unwrap_or(py);
    let bottom = style.pad_bottom.map(|p| p.get()).unwrap_or(py);
    let left = style.pad_left.map(|p| p.get()).unwrap_or(px);
    let right = style.pad_right.map(|p| p.get()).unwrap_or(px);
    check_layout_px("pad_x", px, false);
    check_layout_px("pad_y", py, false);
    check_layout_px("pad_top", top, false);
    check_layout_px("pad_bottom", bottom, false);
    check_layout_px("pad_left", left, false);
    check_layout_px("pad_right", right, false);
    Pad {
        left: left * dpr,
        right: right * dpr,
        top: top * dpr,
        bottom: bottom * dpr,
    }
}

fn resolve_gap(style: &Style, dpr: f32) -> f32 {
    let g = style.gap.map(|p| p.get()).unwrap_or(0.0);
    check_layout_px("gap", g, false);
    g * dpr
}

/// Symmetric child margins (Decision 249): validated like pads
/// (non-negative finite — negative margins panic loudly, never
/// collapse silently) and scaled by DPR at the same site. Round
/// 11.1: each set side wins over its symmetric shorthand (same
/// CSS-shorthand rule as [`resolve_pad`]).
fn resolve_margin(style: &Style, dpr: f32) -> Margin {
    let mx = style.margin_x.map(|p| p.get()).unwrap_or(0.0);
    let my = style.margin_y.map(|p| p.get()).unwrap_or(0.0);
    let top = style.margin_top.map(|p| p.get()).unwrap_or(my);
    let bottom = style.margin_bottom.map(|p| p.get()).unwrap_or(my);
    let left = style.margin_left.map(|p| p.get()).unwrap_or(mx);
    let right = style.margin_right.map(|p| p.get()).unwrap_or(mx);
    check_layout_px("margin_x", mx, false);
    check_layout_px("margin_y", my, false);
    check_layout_px("margin_top", top, false);
    check_layout_px("margin_bottom", bottom, false);
    check_layout_px("margin_left", left, false);
    check_layout_px("margin_right", right, false);
    Margin {
        left: left * dpr,
        right: right * dpr,
        top: top * dpr,
        bottom: bottom * dpr,
    }
}

fn resolve_explicit(style: &Style, dpr: f32) -> (Option<f32>, Option<f32>) {
    let w = style.w.map(|p| {
        let v = p.get();
        check_layout_px("w", v, false);
        v * dpr
    });
    let h = style.h.map(|p| {
        let v = p.get();
        check_layout_px("h", v, false);
        v * dpr
    });
    if let Some(v) = w {
        if !v.is_finite() {
            panic!("layout: scaled style.w is non-finite ({v})");
        }
    }
    if let Some(v) = h {
        if !v.is_finite() {
            panic!("layout: scaled style.h is non-finite ({v})");
        }
    }
    (w, h)
}

fn resolve_offsets(style: &Style, dpr: f32) -> (Option<f32>, Option<f32>) {
    let x = style.x.map(|p| {
        let v = p.get();
        check_layout_px("x", v, true);
        v * dpr
    });
    let ay = style.absolute_y.map(|p| {
        let v = p.get();
        check_layout_px("absolute_y", v, true);
        v * dpr
    });
    (x, ay)
}

fn resolve_floor(style: &Style, dpr: f32) -> f32 {
    let c = style.content_size.map(|p| p.get()).unwrap_or(0.0);
    check_layout_px("content_size", c, false);
    c * dpr
}

/// Min/max clamp on the committed box (Phase 36 PR2a, decision 353):
/// resolved sizes clamp into `[min, max]`; an *explicit* `w`/`h`
/// outside the clamp refuses loudly (an authoring contradiction —
/// never a silent snap). A `min > max` contradiction refuses loudly
/// too. Runs in [`LayoutEngine::commit_box`](LayoutEngine)
/// so every arm (Row/Column/Grid/Stack/Scroll/portal/text)
/// honors it from one site.
fn resolve_clamp(style: &Style, dpr: f32) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
    let cvt = |name: &str, v: Option<crate::style::Px>| {
        v.map(|p| {
            let v = p.get();
            check_layout_px(name, v, false);
            v * dpr
        })
    };
    let min_w = cvt("min_w", style.min_w);
    let min_h = cvt("min_h", style.min_h);
    let max_w = cvt("max_w", style.max_w);
    let max_h = cvt("max_h", style.max_h);
    for (lo, hi, axis) in [(min_w, max_w, "width"), (min_h, max_h, "height")] {
        if let (Some(lo), Some(hi)) = (lo, hi) {
            if lo > hi {
                panic!("layout: min_{axis} {lo} exceeds max_{axis} {hi} — contradictory clamp, never a silent pick");
            }
        }
    }
    (min_w, min_h, max_w, max_h)
}

fn check_leftover(axis: &str, v: f32) {
    if !v.is_finite() {
        panic!("layout: {axis} leftover is non-finite ({v}) — NaN/Inf never lays out silently");
    }
}

/// Loud width-constraint validation (Round 1.1, decision 252): every
/// width the engine flows text into must be a usable constraint.
/// `None`/infinite = intrinsic (single-line, no constraint) and stays
/// legal; NaN is never a width (it previously read as infinite and laid
/// out a silent single line); negative widths never lay out silently.
fn check_constrain_width(name: &str, v: f32) {
    if v.is_nan() {
        panic!("layout: {name} is NaN — NaN widths never lay out silently");
    }
    if v < 0.0 {
        panic!("layout: {name} is negative ({v}) — negative widths never lay out silently");
    }
}

impl<'a> LayoutCtx<'a> {
    fn style_of(&self, id: NodeId) -> Style {
        self.rec
            .get(id)
            .and_then(|n| self.styles.get(n.style).cloned())
            .unwrap_or_default()
    }

    /// True when `id` is an overlay portal (Round 1.4): flow containers
    /// skip these children (no extent/gap/justify contribution) and lay
    /// them viewport-anchored through the `Tag::Portal` arm instead.
    fn is_portal(&self, id: NodeId) -> bool {
        self.rec.get(id).is_some_and(|n| n.tag == Tag::Portal)
    }

    /// Lays portal children after a container's own flow (parents
    /// call this for the portal children they skipped), threading
    /// the caller's content origin so portals with `x`/`absolute_y`
    /// offsets anchor to their parent (Round 7.21, decision 296 —
    /// dropdown popups ride their `Select` box). Each portal
    /// re-enters through `layout_node` (the `Tag::Portal` arm
    /// ignores position/width arguments by construction, but
    /// forwards the anchor + height hint).
    fn layout_portals(
        &mut self,
        portals: &[NodeId],
        parent_x: f32,
        parent_y: f32,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) {
        for portal in portals {
            self.layout_node(
                *portal,
                0.0,
                0.0,
                None,
                None,
                parent_x,
                parent_y,
                inherited_px,
                inherited_weight,
            );
        }
    }

    /// Commits one box, snapping the position to the device grid (extents
    /// stay subpixel). Counts real changes for the publish decision.
    /// Phase 36 PR2a: min/max clamping applies here (see
    /// [`resolve_clamp`]) — one site, every arm.
    #[allow(clippy::too_many_arguments)] // one call site; a params struct buys nothing
    fn commit_box(
        &mut self,
        id: NodeId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        content_w: f32,
        content_h: f32,
        lines: Vec<LaidLine>,
    ) {
        // Commit positions snap to the device grid; extents stay subpixel
        // (coordinate-system spec: rounding at commit positions only).
        let style = self.style_of(id);
        let (explicit_w, explicit_h) = resolve_explicit(&style, self.dpr);
        let (min_w, min_h, max_w, max_h) = resolve_clamp(&style, self.dpr);
        // Explicit sizes outside the clamp are authoring contradictions
        // (loud, never a silent snap); resolved sizes clamp.
        if let Some(e) = explicit_w {
            if min_w.is_some_and(|m| e < m) || max_w.is_some_and(|m| e > m) {
                panic!(
                    "layout: explicit w {e} outside clamp [{:?}, {:?}] — contradictory size, never a silent snap (node {id:?})",
                    min_w.unwrap_or(f32::NEG_INFINITY),
                    max_w.unwrap_or(f32::INFINITY),
                );
            }
        }
        if let Some(e) = explicit_h {
            if min_h.is_some_and(|m| e < m) || max_h.is_some_and(|m| e > m) {
                panic!(
                    "layout: explicit h {e} outside clamp [{:?}, {:?}] — contradictory size, never a silent snap (node {id:?})",
                    min_h.unwrap_or(f32::NEG_INFINITY),
                    max_h.unwrap_or(f32::INFINITY),
                );
            }
        }
        let w = min_w.map(|m| w.max(m)).unwrap_or(w);
        let w = max_w.map(|m| w.min(m)).unwrap_or(w);
        let h = min_h.map(|m| h.max(m)).unwrap_or(h);
        let h = max_h.map(|m| h.min(m)).unwrap_or(h);
        let want = LayoutBox {
            x: round_to_device_px(x, self.dpr),
            y: round_to_device_px(y, self.dpr),
            w,
            h,
            content_w,
            content_h,
            lines,
        };
        if let Some(n) = self.rec.node_mut(id) {
            if n.layout.as_ref() != Some(&want) {
                // F1 (decision 104): a box that changed with no
                // paint-relevant dirt is a position-only LAYOUT move —
                // stamp PAINT engine-side so the FramePlan builder
                // rebuilds it. Nodes already carrying
                // STRUCTURE|STYLE|PAINT|TEXT rebuild anyway, so the
                // stamp fires only for the LAYOUT-only case (fresh
                // nodes, style/text changes, and handler-only updates
                // all carry FRAME_DIRT bits already).
                if !n.pass_dirty.intersects(FRAME_DIRT) {
                    n.pass_dirty.set(PassMask::PAINT);
                    self.stats.paint_stamped += 1;
                }
                n.layout = Some(want);
                self.stats.boxes_changed += 1;
            }
        }
    }

    /// Intrinsic + placed layout of one node. `given_w` is a finite width
    /// imposed by the parent (block full-width, flex fill share, explicit);
    /// `None` sizes intrinsically (text stays single-line). `given_h` is
    /// the portal content height (Round 7.21, decision 296): `Some`
    /// only on the portal→child edge, consumed only by `fill_height`
    /// children without explicit `h` (they lay out window-tall, so a
    /// modal backdrop centers like an explicitly-sized container);
    /// every other child ignores it — the hint never invents size.
    /// `parent_x`/`parent_y` is the calling container's content
    /// origin, forwarded to the `Tag::Portal` arm for offset
    /// anchoring (meaningless on every other arm). `inherited_px`
    /// / `inherited_weight` thread through transparent `Text` wrappers to
    /// the measured bare-text leaf (which always carries hint `None`).
    /// Returns the intrinsic content size (before explicit overrides).
    #[allow(clippy::too_many_arguments)] // threaded recursion shape; a params struct buys nothing
    fn layout_node(
        &mut self,
        id: NodeId,
        ox: f32,
        oy: f32,
        given_w: Option<f32>,
        given_h: Option<f32>,
        parent_x: f32,
        parent_y: f32,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) -> Size {
        self.stats.nodes_visited += 1;
        // Round 1.1 (decision 252): a parent-imposed width is a
        // constraint — NaN/negative never flows silently into children.
        if let Some(w) = given_w {
            check_constrain_width("given_w", w);
        }
        // The portal height hint obeys the same loud rule (NaN/Inf/
        // negative never lay out silently).
        if let Some(h) = given_h {
            check_layout_px("given_h", h, false);
        }
        let Some(node) = self.rec.get(id) else {
            return Size { w: 0.0, h: 0.0 };
        };
        let tag = node.tag;
        let children = node.children.clone();
        let text = node.text.clone();
        let hint = node.text_hint;
        let style = self.style_of(id);
        let pad = resolve_pad(&style, self.dpr);
        let gap = resolve_gap(&style, self.dpr);
        let (explicit_w, explicit_h) = resolve_explicit(&style, self.dpr);
        // Portal height hint (Round 7.21, decision 296): a
        // `fill_height` child of a portal lays out against the
        // portal's content height; explicit `h` always wins, and
        // non-fill children size intrinsically (the hint is gated
        // on the fill flag, never a silent size invention).
        let explicit_h = explicit_h.or(if style.fill_height { given_h } else { None });
        let content_floor = resolve_floor(&style, self.dpr);
        let align = style.align_items.unwrap_or(AlignItems::Start);
        let justify = style.justify_content.unwrap_or(JustifyContent::Start);

        // Text leaf: owns text payload (bare VNode::Text, or Tag::Text with
        // inline text). The Text-struct wrapper (Tag::Text + text_hint + a
        // text child) falls through to the transparent-container path.
        if tag == Tag::Text && text.is_some() {
            let text = text.unwrap_or_default();
            let size_px = resolve_text_px(hint, inherited_px, &self.engine.config);
            let weight = resolve_text_weight(hint, inherited_weight);
            return self.layout_text_leaf(
                id,
                &text,
                size_px,
                weight,
                ox,
                oy,
                explicit_w.or(given_w),
                explicit_w,
                explicit_h,
            );
        }
        // Non-container leaves (Image with explicit size; unknown leaves).
        if children.is_empty() {
            let w = explicit_w.or(given_w).unwrap_or(0.0);
            let h = explicit_h.unwrap_or(0.0);
            self.commit_box(id, ox, oy, w, h, w, h.max(content_floor), Vec::new());
            return Size { w, h };
        }
        // Transparent Text wrapper: pass width + resolved hint through
        // (size and weight alike — the bare-text leaf inherits both).
        // Portals skip the inline stack (viewport-anchored, like every
        // other container — a portal inside a text wrapper is
        // degenerate but consistent).
        if tag == Tag::Text {
            let inner_px = resolve_text_px(hint, inherited_px, &self.engine.config);
            let inner_weight = resolve_text_weight(hint, inherited_weight);
            let mut extent = Size { w: 0.0, h: 0.0 };
            let mut cy = oy;
            let mut max_w = 0.0f32;
            let mut portals: Vec<NodeId> = Vec::new();
            for child in &children {
                if self.is_portal(*child) {
                    portals.push(*child);
                    continue;
                }
                let s = self.layout_node(
                    *child,
                    ox,
                    cy,
                    given_w.or(explicit_w),
                    None,
                    0.0,
                    0.0,
                    inner_px,
                    inner_weight,
                );
                cy += s.h;
                max_w = max_w.max(s.w);
                extent.h += s.h;
            }
            self.layout_portals(&portals, ox, oy, inner_px, inner_weight);
            extent.w = max_w;
            let w = explicit_w.or(given_w).unwrap_or(extent.w);
            let h = explicit_h.unwrap_or(extent.h);
            self.commit_box(
                id,
                ox,
                oy,
                w,
                h,
                extent.w,
                extent.h.max(content_floor),
                Vec::new(),
            );
            return Size { w, h };
        }

        let content_x = ox + pad.left;
        let content_y = oy + pad.top;
        match tag {
            Tag::Row => self.layout_row(
                id,
                &children,
                ox,
                oy,
                content_x,
                content_y,
                pad,
                gap,
                explicit_w,
                explicit_h,
                given_w,
                content_floor,
                align,
                justify,
                inherited_px,
                inherited_weight,
            ),
            Tag::Column | Tag::Div => self.layout_vertical(
                id,
                &children,
                tag,
                ox,
                oy,
                content_x,
                content_y,
                pad,
                gap,
                explicit_w,
                explicit_h,
                given_w,
                content_floor,
                align,
                justify,
                inherited_px,
                inherited_weight,
            ),
            Tag::Stack => self.layout_stack(
                id,
                &children,
                ox,
                oy,
                content_x,
                content_y,
                pad,
                explicit_w,
                explicit_h,
                given_w,
                content_floor,
                inherited_px,
                inherited_weight,
            ),
            Tag::ScrollArea => self.layout_scroll(
                id,
                &children,
                ox,
                oy,
                content_x,
                content_y,
                pad,
                gap,
                explicit_w,
                explicit_h,
                given_w,
                content_floor,
                inherited_px,
                inherited_weight,
            ),
            Tag::Grid => self.layout_grid(
                id,
                &children,
                ox,
                oy,
                content_x,
                content_y,
                pad,
                gap,
                explicit_w,
                explicit_h,
                given_w,
                content_floor,
                inherited_px,
                inherited_weight,
            ),
            // Overlay layer (Round 1.4, decision 255): viewport-anchored,
            // never parent-flow — parents skip portal children and lay
            // them through this arm, which ignores ox/oy/given but
            // forwards the parent anchor (Round 7.21).
            Tag::Portal => {
                self.layout_portal(id, parent_x, parent_y, inherited_px, inherited_weight)
            }
            // Custom escape hatch (locked #17): unknown content stacks
            // block-lite (decision 72). Image with children: same.
            // Path with children (hand-built only — the `Path`
            // component is childless): same (decision 291).
            Tag::Custom(_) | Tag::Image | Tag::Path => self.layout_vertical(
                id,
                &children,
                Tag::Div,
                ox,
                oy,
                content_x,
                content_y,
                pad,
                gap,
                explicit_w,
                explicit_h,
                given_w,
                content_floor,
                align,
                justify,
                inherited_px,
                inherited_weight,
            ),
            Tag::Text => unreachable!("text nodes handled above"),
        }
    }

    /// Row: horizontal flex. Fixed children size intrinsically; `fill_width`
    /// children split the remaining content width equally (re-flowed into
    /// their shares — the second flow pass in stats).
    ///
    /// Decision 237: pads inset the content box (`content_x = ox +
    /// pad.left`, auto width adds the horizontal pair); Round 11.1:
    /// asymmetric sides flow through the same formulas (symmetric trees
    /// read identical values — see [`Pad`]). `justify_content`
    /// distributes leftover main-axis space (Start/Center/End/
    /// SpaceBetween, SpaceBetween clamps negative leftover to 0 so gaps
    /// never shrink); `align_items` places the cross axis (Start/Center/
    /// End offsets within the content height, Stretch grows
    /// unconstrained — `style.h` None — children to the content height
    /// via max-grow, fixed children stay Start).
    #[allow(clippy::too_many_arguments)]
    fn layout_row(
        &mut self,
        id: NodeId,
        children: &[NodeId],
        ox: f32,
        oy: f32,
        content_x: f32,
        content_y: f32,
        pad: Pad,
        gap: f32,
        explicit_w: Option<f32>,
        explicit_h: Option<f32>,
        given_w: Option<f32>,
        content_floor: f32,
        align: AlignItems,
        justify: JustifyContent,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) -> Size {
        let outer_w = explicit_w.or(given_w);
        let content_w = outer_w.map(|w| (w - pad.x()).max(0.0));
        // Round 1.2 (decision 253): Wrap with a constrained width breaks
        // into lines; unconstrained Wrap has nothing to break against and
        // falls through as single-line (stated on `FlexWrap`, never silent).
        let wrap = self.style_of(id).flex_wrap.unwrap_or(FlexWrap::NoWrap);
        if wrap == FlexWrap::Wrap && content_w.is_some() {
            return self.layout_row_wrap(
                id,
                children,
                ox,
                oy,
                content_x,
                content_y,
                pad,
                gap,
                explicit_w,
                explicit_h,
                given_w,
                content_floor,
                align,
                justify,
                inherited_px,
                inherited_weight,
            );
        }
        // Pass 1: fixed flow children intrinsically (measured at the origin,
        // placed at the cursor afterwards). Margins ride the advance and
        // the cross extent (Decision 249); absent margins are zeros, so
        // this math is bit-identical to the pre-margin flow.
        // Phase 36 PR2a (decision 353): `flex_grow` children measure
        // intrinsically (their base size) and join the pass-2 share
        // pool with `weight = grow`; `fill_width` rides the same pool
        // with weight 1 (no flex present = bit-identical to the old
        // equal split). Explicit `w` always wins (flex ignored —
        // stated, the CSS flex-basis rule).
        let mut fixed_w = 0.0f32;
        let mut max_h = 0.0f32;
        let mut max_margin_h = 0.0f32;
        let mut fills: Vec<NodeId> = Vec::new();
        let mut fills_h: Vec<NodeId> = Vec::new();
        let mut flex_base: HashMap<NodeId, f32> = HashMap::new();
        let mut sizes: HashMap<NodeId, Size> = HashMap::new();
        let mut flow_count = 0usize;
        let mut portals: Vec<NodeId> = Vec::new();
        for child in children {
            // Portals never join parent flow (Round 1.4 — laid
            // viewport-anchored after placement, contributing no
            // extent, gap, or justify share).
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            if cs.absolute_y.is_some() || cs.x.is_some() {
                continue; // out-of-flow on at least one axis; placed later
            }
            flow_count += 1;
            let m = resolve_margin(&cs, self.dpr);
            if cs.fill_height && cs.h.is_none() {
                fills_h.push(*child);
            }
            if cs.fill_width {
                fills.push(*child);
            } else if cs.w.is_none() && cs.flex_grow.is_some_and(|g| g.get() > 0.0) {
                // Flexible base: measured intrinsically, grown in pass 2.
                let s = self.layout_node(
                    *child,
                    0.0,
                    0.0,
                    None,
                    None,
                    0.0,
                    0.0,
                    inherited_px,
                    inherited_weight,
                );
                flex_base.insert(*child, s.w);
                fills.push(*child);
                // Base sizes do NOT join fixed_w (they grow from the
                // remainder pool — the CSS flex-basis rule); the base
                // floors the share below (grow never shrinks).
                max_h = max_h.max(s.h);
                max_margin_h = max_margin_h.max(s.h + m.y());
                sizes.insert(*child, s);
            } else {
                let s = self.layout_node(
                    *child,
                    0.0,
                    0.0,
                    None,
                    None,
                    0.0,
                    0.0,
                    inherited_px,
                    inherited_weight,
                );
                fixed_w += s.w + m.x();
                max_h = max_h.max(s.h);
                max_margin_h = max_margin_h.max(s.h + m.y());
                sizes.insert(*child, s);
            }
        }
        // Pass 2: distribute the remainder by weight among fill/flex children.
        // Fixed margins already sit inside `fixed_w`, so shares split
        // what is left; fill-child margins offset position (never
        // shrink the share — overflow stays overflow, stated).
        // Phase 36 PR2a (decision 353): one weighted pool — `fill_width`
        // rides weight 1 with base 0 (no flex present = the old equal
        // split exactly), `flex_grow` rides its factor over its
        // intrinsic base. Bases leave the pool first (the CSS
        // flex-basis rule); the base floors the share (grow never
        // shrinks — shrink is the opt-in `flex_shrink` arm below).
        if !fills.is_empty() {
            self.stats.layout_passes = self.stats.layout_passes.max(2);
            let gaps = gap * flow_count.saturating_sub(1) as f32;
            let flex_bases: f32 = fills.iter().filter_map(|c| flex_base.get(c).copied()).sum();
            let remainder = content_w
                .map(|cw| (cw - fixed_w - flex_bases - gaps).max(0.0))
                .unwrap_or(0.0);
            check_leftover("row fill", remainder);
            let total: f32 = fills
                .iter()
                .map(|c| {
                    self.style_of(*c)
                        .flex_grow
                        .map(|g| g.get())
                        .filter(|g| *g > 0.0)
                        .unwrap_or(1.0)
                })
                .sum();
            if !total.is_finite() || total <= 0.0 {
                panic!("layout: row fill weights non-finite or empty (total {total})");
            }
            for child in fills {
                let cs = self.style_of(child);
                let m = resolve_margin(&cs, self.dpr);
                let weight = cs
                    .flex_grow
                    .map(|g| g.get())
                    .filter(|g| *g > 0.0)
                    .unwrap_or(1.0);
                let base = flex_base.get(&child).copied().unwrap_or(0.0);
                let share = base + remainder * weight / total;
                if !share.is_finite() {
                    panic!("layout: row flex share is non-finite ({share})");
                }
                let s = self.layout_node(
                    child,
                    0.0,
                    0.0,
                    Some(share),
                    None,
                    0.0,
                    0.0,
                    inherited_px,
                    inherited_weight,
                );
                max_h = max_h.max(s.h);
                max_margin_h = max_margin_h.max(s.h + m.y());
                sizes.insert(child, s);
            }
        }
        let extent_w_pre = if flow_count == 0 {
            0.0
        } else {
            let mut sum_w = 0.0f32;
            for (child, s) in &sizes {
                let cs = self.style_of(*child);
                let m = resolve_margin(&cs, self.dpr);
                sum_w += s.w + m.x();
            }
            sum_w + gap * flow_count.saturating_sub(1) as f32
        };
        // Stretch + fill_height (cross axis): grow unconstrained
        // (`style.h` None) flow children to the content height
        // (max-grow, never shrink below the measured size so overflow
        // stays overflow). Stretch is container-driven (`align_items`);
        // `fill_height` is the same growth child-driven (Decision 249)
        // — one block because the math is identical; explicit `h`
        // always wins over both.
        let content_h_box = explicit_h.map(|h| (h - pad.y()).max(0.0));
        if (align == AlignItems::Stretch || !fills_h.is_empty()) && flow_count > 0 {
            let target = content_h_box.unwrap_or(max_h);
            check_leftover("row align", target);
            // Collect first (style reads are immutable; box writes are not).
            let stretchable: Vec<(NodeId, f32, f32)> = sizes
                .iter()
                .filter_map(|(child, s)| {
                    let cs = self.style_of(*child);
                    if cs.h.is_none() && (align == AlignItems::Stretch || cs.fill_height) {
                        Some((*child, s.w, s.h))
                    } else {
                        None
                    }
                })
                .collect();
            for (child, w, h) in stretchable {
                let grown = h.max(target);
                if (grown - h).abs() > f32::EPSILON {
                    self.set_box_h(child, grown);
                    sizes.insert(child, Size { w, h: grown });
                    max_h = max_h.max(grown);
                    let cs = self.style_of(child);
                    let m = resolve_margin(&cs, self.dpr);
                    max_margin_h = max_margin_h.max(grown + m.y());
                }
            }
        }
        // Phase 36 PR2a (decision 353): opt-in shrink. A constrained
        // row overflowing its content width gives the excess back
        // proportionally to `flex_shrink × laid width` (the CSS
        // scaled-shrink rule); children without `flex_shrink` (or
        // with explicit `w`) never shrink — overflow stays overflow,
        // the shipped single-line rule. Shrunk children re-lay into
        // the smaller width (re-wrap is correct for text); the extent
        // re-derives, so a fully absorbed excess justifies clean.
        let mut extent_w = extent_w_pre;
        if let Some(cw) = content_w {
            let overflow = extent_w - cw;
            if overflow > f32::EPSILON {
                let shrinkable: Vec<(NodeId, f32, f32)> = sizes
                    .iter()
                    .filter_map(|(child, s)| {
                        let cs = self.style_of(*child);
                        let f = cs.flex_shrink.map(|p| p.get()).unwrap_or(0.0);
                        if f > 0.0 && cs.w.is_none() && s.w > 0.0 {
                            Some((*child, s.w, f * s.w))
                        } else {
                            None
                        }
                    })
                    .collect();
                let total: f32 = shrinkable.iter().map(|(_, _, w)| w).sum();
                if total > 0.0 && total.is_finite() {
                    self.stats.layout_passes = self.stats.layout_passes.max(2);
                    for (child, base, weight) in shrinkable {
                        let new_w = (base - overflow * weight / total).max(0.0);
                        if !new_w.is_finite() {
                            panic!("layout: row shrink width non-finite ({new_w})");
                        }
                        if (new_w - base).abs() > f32::EPSILON {
                            let s = self.layout_node(
                                child,
                                0.0,
                                0.0,
                                Some(new_w),
                                None,
                                0.0,
                                0.0,
                                inherited_px,
                                inherited_weight,
                            );
                            sizes.insert(child, s);
                        }
                    }
                    let mut sum_w = 0.0f32;
                    max_h = 0.0;
                    max_margin_h = 0.0;
                    for (child, s) in &sizes {
                        let cs = self.style_of(*child);
                        let m = resolve_margin(&cs, self.dpr);
                        sum_w += s.w + m.x();
                        max_h = max_h.max(s.h);
                        max_margin_h = max_margin_h.max(s.h + m.y());
                    }
                    extent_w = sum_w + gap * flow_count.saturating_sub(1) as f32;
                }
            }
        }
        // Justify (main axis X): leftover distribution.
        let leftover = content_w.map(|cw| cw - extent_w).unwrap_or(0.0);
        check_leftover("row justify", leftover);
        let (start_offset, gap_extra) = match justify {
            JustifyContent::Start => (0.0, 0.0),
            JustifyContent::Center => (leftover / 2.0, 0.0),
            JustifyContent::End => (leftover, 0.0),
            JustifyContent::SpaceBetween => {
                if flow_count <= 1 {
                    (0.0, 0.0)
                } else {
                    (0.0, leftover.max(0.0) / (flow_count - 1) as f32)
                }
            }
        };
        if !start_offset.is_finite() || !gap_extra.is_finite() {
            panic!(
                "layout: row justify offsets non-finite (start {start_offset}, extra {gap_extra}, leftover {leftover})"
            );
        }
        let content_h_align = content_h_box.unwrap_or(max_h);
        // Place flow children back in child order, then out-of-flow.
        // Margins offset the child origin and ride the advance
        // (Decision 249); alignment still solves the border box in
        // the content space, then the margin shifts it.
        let mut cursor = content_x + start_offset;
        let mut first = true;
        for child in children {
            let Some(s) = sizes.get(child).copied() else {
                continue;
            };
            if !first {
                cursor += gap + gap_extra;
            }
            first = false;
            let cs = self.style_of(*child);
            let m = resolve_margin(&cs, self.dpr);
            let y = match align {
                AlignItems::Start | AlignItems::Stretch => content_y,
                AlignItems::Center => content_y + (content_h_align - s.h) / 2.0,
                AlignItems::End => content_y + (content_h_align - s.h),
            } + m.top;
            if !y.is_finite() {
                panic!("layout: row align y non-finite ({y})");
            }
            self.reposition(*child, cursor + m.left, y);
            cursor += s.w + m.x();
        }
        let mut extent_h = max_margin_h;
        for child in children {
            if sizes.contains_key(child) || self.is_portal(*child) {
                continue;
            }
            let cs = self.style_of(*child);
            let (ox_off, ay_off) = resolve_offsets(&cs, self.dpr);
            let cx = ox_off.map(|v| content_x + v).unwrap_or(content_x);
            // Out-of-flow bypasses cross-axis alignment (Decision 237):
            // `absolute_y` pins at `content_y + ay`; x-only sits at the
            // content top (backward-compatible: `oy` when `pad_y` is 0).
            let cy = ay_off.map(|ay| content_y + ay).unwrap_or(content_y);
            let s = self.layout_node(
                *child,
                cx,
                cy,
                None,
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            // x-only children still count vertically (knob-in-track shape);
            // absolute_y children never grow the auto extent (decision 70).
            if cs.absolute_y.is_none() {
                extent_h = extent_h.max(s.h);
            }
        }
        self.layout_portals(
            &portals,
            content_x,
            content_y,
            inherited_px,
            inherited_weight,
        );
        let w = explicit_w.or(given_w).unwrap_or(extent_w + pad.x());
        let h = explicit_h.unwrap_or(extent_h + pad.y());
        self.commit_box(
            id,
            ox,
            oy,
            w,
            h,
            extent_w,
            extent_h.max(content_floor),
            Vec::new(),
        );
        Size { w, h }
    }

    /// Row with `flex_wrap = Wrap` under a constrained width: greedy
    /// line-breaking over flow children (Round 1.2, decision 253).
    ///
    /// Rules:
    /// - A child joins the current line while it fits (`used + gap +
    ///   width + margins <= content width`); otherwise it starts the next
    ///   line whole. An over-wide lone child overflows its line (the
    ///   over-wide push-whole precedent — never shredded).
    /// - `fill_width` children never trigger a break; each takes an equal
    ///   share of its line's remainder (re-laid into the share — the
    ///   pass-2 rule; margins never shrink the share; a clamped-to-zero
    ///   remainder keeps overflow as overflow). `flex_grow` children
    ///   ride this same fill-equivalent path per line (Phase 36 PR2a:
    ///   weights do not cross line breaks — equal shares, stated).
    /// - The main-axis `gap` doubles as the cross-axis line gap;
    ///   `justify_content` applies per line; `align_items` (including
    ///   Stretch max-grow, and `fill_height` grown the same way) applies
    ///   within each line's border-box height.
    /// - Container height is auto (explicit `h` still wins, lines stack
    ///   top-down); out-of-flow children keep the single-line rules
    ///   (x-only counts vertically, `absolute_y` never grows the extent).
    #[allow(clippy::too_many_arguments)]
    fn layout_row_wrap(
        &mut self,
        id: NodeId,
        children: &[NodeId],
        ox: f32,
        oy: f32,
        content_x: f32,
        content_y: f32,
        pad: Pad,
        gap: f32,
        explicit_w: Option<f32>,
        explicit_h: Option<f32>,
        given_w: Option<f32>,
        content_floor: f32,
        align: AlignItems,
        justify: JustifyContent,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) -> Size {
        let outer_w = explicit_w.or(given_w);
        let content_w = outer_w
            .map(|w| (w - pad.x()).max(0.0))
            .expect("layout: row wrap without a constrained width");
        // Pass 1: fixed flow children intrinsically; fills ride with zero
        // pre-width and are re-laid into per-line shares afterwards.
        let mut sizes: HashMap<NodeId, Size> = HashMap::new();
        let mut flow: Vec<(NodeId, bool)> = Vec::new();
        let mut portals: Vec<NodeId> = Vec::new();
        for child in children {
            // Portals never join parent flow (Round 1.4).
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            if cs.absolute_y.is_some() || cs.x.is_some() {
                continue; // out-of-flow on at least one axis; placed later
            }
            // Phase 36 PR2a (decision 353): `flex_grow` rides the
            // fill-equivalent path per line (equal shares — weights do
            // not cross line breaks, stated); `flex_shrink` is ignored
            // (over-wide lone children overflow — the existing rule).
            let is_fill =
                cs.fill_width || (cs.w.is_none() && cs.flex_grow.is_some_and(|g| g.get() > 0.0));
            if !is_fill {
                let s = self.layout_node(
                    *child,
                    0.0,
                    0.0,
                    None,
                    None,
                    0.0,
                    0.0,
                    inherited_px,
                    inherited_weight,
                );
                sizes.insert(*child, s);
            }
            flow.push((*child, is_fill));
        }
        // Greedy break (child order preserved; fills never break a line).
        let mut lines: Vec<Vec<NodeId>> = Vec::new();
        let mut used = 0.0f32;
        for (child, is_fill) in &flow {
            let m = resolve_margin(&self.style_of(*child), self.dpr);
            let pre = if *is_fill {
                0.0
            } else {
                sizes
                    .get(child)
                    .copied()
                    .unwrap_or(Size { w: 0.0, h: 0.0 })
                    .w
                    + m.x()
            };
            let first_in_line = lines.last().is_none_or(|l: &Vec<NodeId>| l.is_empty());
            let add = if first_in_line { pre } else { gap + pre };
            if !first_in_line && used + add > content_w {
                lines.push(vec![*child]);
                used = pre;
            } else {
                match lines.last_mut() {
                    Some(line) => line.push(*child),
                    None => lines.push(vec![*child]),
                }
                used += add;
            }
        }
        // Pass 2: each line's fills split that line's remainder equally.
        // Fixed margins already sit inside `fixed`, so shares split what is
        // left; fill-child margins offset position, never shrink the share.
        let mut reflowed = false;
        for line in &lines {
            if !line.iter().any(|c| !sizes.contains_key(c)) {
                continue;
            }
            reflowed = true;
            let mut fixed = 0.0f32;
            for c in line {
                if let Some(s) = sizes.get(c).copied() {
                    let m = resolve_margin(&self.style_of(*c), self.dpr);
                    fixed += s.w + m.x();
                }
            }
            fixed += gap * line.len().saturating_sub(1) as f32;
            let remainder = (content_w - fixed).max(0.0);
            check_leftover("row wrap fill", remainder);
            let fill_count = line.iter().filter(|c| !sizes.contains_key(*c)).count();
            let share = remainder / fill_count as f32;
            if !share.is_finite() {
                panic!("layout: row wrap fill share is non-finite ({share})");
            }
            let fills: Vec<NodeId> = line
                .iter()
                .copied()
                .filter(|c| !sizes.contains_key(c))
                .collect();
            for child in fills {
                let s = self.layout_node(
                    child,
                    0.0,
                    0.0,
                    Some(share),
                    None,
                    0.0,
                    0.0,
                    inherited_px,
                    inherited_weight,
                );
                sizes.insert(child, s);
            }
        }
        if reflowed {
            self.stats.layout_passes = self.stats.layout_passes.max(2);
        }
        // Line geometry: border-box max drives alignment, margin-inclusive
        // max drives the container extent (Decision 249 on both axes).
        let mut widths: Vec<f32> = Vec::with_capacity(lines.len());
        let mut borders: Vec<f32> = Vec::with_capacity(lines.len());
        let mut heights: Vec<f32> = Vec::with_capacity(lines.len());
        for line in &lines {
            let mut w = 0.0f32;
            let mut border = 0.0f32;
            let mut h = 0.0f32;
            for c in line {
                let s = sizes.get(c).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                let m = resolve_margin(&self.style_of(*c), self.dpr);
                w += s.w + m.x();
                border = border.max(s.h);
                h = h.max(s.h + m.y());
            }
            if !line.is_empty() {
                w += gap * line.len().saturating_sub(1) as f32;
            }
            widths.push(w);
            borders.push(border);
            heights.push(h);
        }
        // Stretch + fill_height (cross axis, per line): grow unconstrained
        // (`style.h` None) members to the line's border-box height via
        // max-grow, then recompute the margin-inclusive line height (the
        // single-line F1-adjacent rule, applied per line).
        let wants_grow = align == AlignItems::Stretch
            || flow.iter().any(|(c, _)| {
                let cs = self.style_of(*c);
                cs.fill_height && cs.h.is_none()
            });
        if wants_grow {
            for (li, line) in lines.iter().enumerate() {
                let target = borders[li];
                check_leftover("row wrap align", target);
                let growable: Vec<(NodeId, f32)> = line
                    .iter()
                    .filter_map(|c| {
                        let cs = self.style_of(*c);
                        let s = sizes.get(c).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                        if cs.h.is_none() && (align == AlignItems::Stretch || cs.fill_height) {
                            Some((*c, s.w))
                        } else {
                            None
                        }
                    })
                    .collect();
                for (child, w) in growable {
                    let s = sizes
                        .get(&child)
                        .copied()
                        .unwrap_or(Size { w: 0.0, h: 0.0 });
                    let grown = s.h.max(target);
                    if (grown - s.h).abs() > f32::EPSILON {
                        self.set_box_h(child, grown);
                        sizes.insert(child, Size { w, h: grown });
                    }
                }
                let mut h = 0.0f32;
                for c in line {
                    let s = sizes.get(c).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                    let m = resolve_margin(&self.style_of(*c), self.dpr);
                    h = h.max(s.h + m.y());
                }
                heights[li] = h;
            }
        }
        // Place lines top-down; justify + align solve per line.
        let mut cursor_y = content_y;
        for (li, line) in lines.iter().enumerate() {
            let leftover = content_w - widths[li];
            check_leftover("row wrap justify", leftover);
            let (start_offset, gap_extra) = match justify {
                JustifyContent::Start => (0.0, 0.0),
                JustifyContent::Center => (leftover / 2.0, 0.0),
                JustifyContent::End => (leftover, 0.0),
                JustifyContent::SpaceBetween => {
                    if line.len() <= 1 {
                        (0.0, 0.0)
                    } else {
                        (0.0, leftover.max(0.0) / (line.len() - 1) as f32)
                    }
                }
            };
            if !start_offset.is_finite() || !gap_extra.is_finite() {
                panic!(
                    "layout: row wrap justify offsets non-finite (start {start_offset}, extra {gap_extra}, leftover {leftover})"
                );
            }
            let mut cursor_x = content_x + start_offset;
            let mut first = true;
            for child in line {
                let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                if !first {
                    cursor_x += gap + gap_extra;
                }
                first = false;
                let m = resolve_margin(&self.style_of(*child), self.dpr);
                let y = match align {
                    AlignItems::Start | AlignItems::Stretch => cursor_y,
                    AlignItems::Center => cursor_y + (borders[li] - s.h) / 2.0,
                    AlignItems::End => cursor_y + (borders[li] - s.h),
                } + m.top;
                if !y.is_finite() {
                    panic!("layout: row wrap align y non-finite ({y})");
                }
                self.reposition(*child, cursor_x + m.left, y);
                cursor_x += s.w + m.x();
            }
            cursor_y += heights[li] + gap;
        }
        let extent_w = widths.iter().copied().fold(0.0f32, f32::max);
        let mut extent_h = if lines.is_empty() {
            0.0
        } else {
            heights.iter().sum::<f32>() + gap * lines.len().saturating_sub(1) as f32
        };
        for child in children {
            if sizes.contains_key(child) || self.is_portal(*child) {
                continue;
            }
            let cs = self.style_of(*child);
            let (ox_off, ay_off) = resolve_offsets(&cs, self.dpr);
            let cx = ox_off.map(|v| content_x + v).unwrap_or(content_x);
            // Out-of-flow bypasses wrapping like alignment (Decision 237);
            // x-only children still count vertically, `absolute_y` never
            // grows the auto extent (decision 70).
            let cy = ay_off.map(|ay| content_y + ay).unwrap_or(content_y);
            let s = self.layout_node(
                *child,
                cx,
                cy,
                None,
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            if cs.absolute_y.is_none() {
                extent_h = extent_h.max(s.h);
            }
        }
        self.layout_portals(
            &portals,
            content_x,
            content_y,
            inherited_px,
            inherited_weight,
        );
        let w = explicit_w.or(given_w).unwrap_or(extent_w + pad.x());
        let h = explicit_h.unwrap_or(extent_h + pad.y());
        self.commit_box(
            id,
            ox,
            oy,
            w,
            h,
            extent_w,
            extent_h.max(content_floor),
            Vec::new(),
        );
        Size { w, h }
    }

    /// Column (flex, intrinsic unless `fill_width`) and Div (block-lite:
    /// every child full content width). Otherwise identical vertical flow.
    ///
    /// Decision 237: pads inset the content box (`content_x = ox +
    /// pad.left`, auto sizes add the pairs); Round 11.1: asymmetric
    /// sides flow through the same formulas (see [`Pad`]).
    /// `justify_content` distributes leftover vertical space
    /// (Start/Center/End/SpaceBetween, SpaceBetween clamps negative
    /// leftover to 0); `align_items` places the cross axis
    /// (Start/Center/End offsets within the content width, Stretch
    /// re-flows unconstrained — no `w`, no `fill_width`, non-block —
    /// children into the content width; block children already fill
    /// when constrained). `x` overrides win over cross-axis
    /// alignment; `absolute_y` children bypass both axes (pinned at
    /// `content_y + ay`).
    #[allow(clippy::too_many_arguments)]
    fn layout_vertical(
        &mut self,
        id: NodeId,
        children: &[NodeId],
        tag: Tag,
        ox: f32,
        oy: f32,
        content_x: f32,
        content_y: f32,
        pad: Pad,
        gap: f32,
        explicit_w: Option<f32>,
        explicit_h: Option<f32>,
        given_w: Option<f32>,
        content_floor: f32,
        align: AlignItems,
        justify: JustifyContent,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) -> Size {
        // Round 1.2 (decision 253): only Row wraps — a Wrap on vertical /
        // block flow refuses loudly (unimplemented axis, never a silent
        // single column). Column wrap-to-columns is the named follow-up.
        if self
            .style_of(id)
            .flex_wrap
            .is_some_and(|w| w == FlexWrap::Wrap)
        {
            panic!(
                "layout: flex_wrap=Wrap on {tag:?} is not supported in v1 — only Row wraps; remove Wrap or use a Row"
            );
        }
        let block = tag == Tag::Div;
        let outer_w = explicit_w.or(given_w);
        let content_w = outer_w.map(|w| (w - pad.x()).max(0.0));
        // Phase 1: measure flow children (origin, placed afterwards).
        // Stretchable Column children (unconstrained, non-fill, non-block)
        // measure into the content width when it is already known.
        let mut sizes: HashMap<NodeId, Size> = HashMap::new();
        let mut flow_ids: Vec<NodeId> = Vec::new();
        let mut portals: Vec<NodeId> = Vec::new();
        for child in children {
            // Portals never join parent flow (Round 1.4).
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            if cs.absolute_y.is_some() {
                continue; // out-of-flow vertically; placed after
            }
            flow_ids.push(*child);
            let stretch_here =
                !block && align == AlignItems::Stretch && cs.w.is_none() && !cs.fill_width;
            let cw = if block || cs.fill_width || (stretch_here && content_w.is_some()) {
                content_w
            } else {
                None
            };
            // content_w may be None (intrinsic parent): block/fill children
            // then size intrinsically (no constraint to fill).
            let s = self.layout_node(
                *child,
                0.0,
                0.0,
                cw,
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            sizes.insert(*child, s);
        }
        // fill_height (main axis, Decision 249): a constrained parent
        // (explicit `h`) divides its remaining content height equally
        // among fill children. Max-grow through `set_box_h` — the
        // Row-Stretch precedent: the outer box grows, the subtree
        // stays top-aligned; never shrink, so overflow stays
        // overflow. Unconstrained parents are a no-op (no share to
        // divide — and max-grow would no-op on it anyway).
        // Fill-child margins do not shrink the share (same rule as
        // Row fill_width); fixed margins already sit inside the
        // fixed sum below.
        // Phase 36 PR2a (decision 353): one weighted pool on the
        // height axis — `fill_height` rides weight 1 with base 0 (no
        // flex present = the old equal split exactly), `flex_grow`
        // rides its factor over its intrinsic base (explicit `h`
        // always wins — flex ignored, the Row rule).
        let content_h_fill = explicit_h.map(|h| (h - pad.y()).max(0.0));
        if let Some(content_h) = content_h_fill {
            let fill_h: HashSet<NodeId> = flow_ids
                .iter()
                .copied()
                .filter(|child| {
                    let cs = self.style_of(*child);
                    // Flex is a Column contract (Row owns the width
                    // axis; Div is block-lite and ignores both shares
                    // — stated on the `flex_grow` field docs).
                    let grown = if tag == Tag::Column {
                        cs.flex_grow
                            .map(|g| g.get())
                            .filter(|g| *g > 0.0)
                            .unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    (cs.fill_height || grown > 0.0) && cs.h.is_none()
                })
                .collect();
            if !fill_h.is_empty() {
                let mut fixed_h = 0.0f32;
                for child in &flow_ids {
                    if fill_h.contains(child) {
                        continue;
                    }
                    let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                    let cs = self.style_of(*child);
                    let m = resolve_margin(&cs, self.dpr);
                    fixed_h += s.h + m.y();
                }
                let gaps = gap * flow_ids.len().saturating_sub(1) as f32;
                // Flex bases leave the pool first (the Row rule):
                // pool members riding `flex_grow` (not `fill_height`)
                // keep their intrinsic base; `fill_height` bases are 0
                // (never measured into the pool — the old rule).
                let flex_bases: f32 = fill_h
                    .iter()
                    .map(|child| {
                        let cs = self.style_of(*child);
                        if cs.fill_height {
                            0.0
                        } else {
                            sizes.get(child).map(|s| s.h).unwrap_or(0.0)
                        }
                    })
                    .sum();
                let remainder = (content_h - fixed_h - flex_bases - gaps).max(0.0);
                check_leftover("column fill", remainder);
                let total: f32 = fill_h
                    .iter()
                    .map(|child| {
                        // Div ignores flex (see the pool filter above).
                        if tag != Tag::Column {
                            return 1.0;
                        }
                        let cs = self.style_of(*child);
                        cs.flex_grow
                            .map(|g| g.get())
                            .filter(|g| *g > 0.0)
                            .unwrap_or(1.0)
                    })
                    .sum();
                if !total.is_finite() || total <= 0.0 {
                    panic!("layout: column fill weights non-finite or empty (total {total})");
                }
                for child in fill_h {
                    let s = sizes
                        .get(&child)
                        .copied()
                        .unwrap_or(Size { w: 0.0, h: 0.0 });
                    let cs = self.style_of(child);
                    let flex_here = tag == Tag::Column;
                    let weight = if flex_here {
                        cs.flex_grow
                            .map(|g| g.get())
                            .filter(|g| *g > 0.0)
                            .unwrap_or(1.0)
                    } else {
                        1.0
                    };
                    let base = if cs.fill_height || !flex_here {
                        0.0
                    } else {
                        s.h
                    };
                    let share = base + remainder * weight / total;
                    if !share.is_finite() {
                        panic!("layout: column flex share is non-finite ({share})");
                    }
                    let grown = s.h.max(share);
                    if (grown - s.h).abs() > f32::EPSILON {
                        self.set_box_h(child, grown);
                        sizes.insert(child, Size { w: s.w, h: grown });
                    }
                }
            }
        }
        let mut extent_w = 0.0f32;
        for child in &flow_ids {
            let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
            let cs = self.style_of(*child);
            let m = resolve_margin(&cs, self.dpr);
            // x-override children keep their width contribution (decision
            // 70); full-width block children already span content_w.
            // Either way the extent carries the margins (Decision 249).
            extent_w = extent_w.max(if block { content_w.unwrap_or(s.w) } else { s.w } + m.x());
        }
        // Stretch second pass for auto-width parents: re-flow unconstrained
        // children into the measured max so they fill it (fill-equivalent;
        // re-wrap is correct for text, unlike a blind box expand).
        let effective_content_w = content_w.unwrap_or(extent_w);
        if !block && align == AlignItems::Stretch && content_w.is_none() && !flow_ids.is_empty() {
            let targets: Vec<NodeId> = flow_ids
                .iter()
                .copied()
                .filter(|child| {
                    let cs = self.style_of(*child);
                    cs.w.is_none() && !cs.fill_width
                })
                .collect();
            if !targets.is_empty() {
                self.stats.layout_passes = self.stats.layout_passes.max(2);
                for child in targets {
                    let old = sizes
                        .get(&child)
                        .copied()
                        .unwrap_or(Size { w: 0.0, h: 0.0 });
                    if old.w < effective_content_w {
                        let s = self.layout_node(
                            child,
                            0.0,
                            0.0,
                            Some(effective_content_w),
                            None,
                            0.0,
                            0.0,
                            inherited_px,
                            inherited_weight,
                        );
                        sizes.insert(child, s);
                    }
                }
                extent_w = effective_content_w;
                for child in &flow_ids {
                    if let Some(s) = sizes.get(child) {
                        extent_w = extent_w.max(s.w);
                    }
                }
            }
        }
        // Height extent carries the vertical margins (Decision 249);
        // absent margins are zeros, so this sums exactly the old way.
        let mut extent_h_pre: f32 = if flow_ids.is_empty() {
            0.0
        } else {
            let mut sum_h = 0.0f32;
            for c in &flow_ids {
                let h = sizes.get(c).map(|s| s.h).unwrap_or(0.0);
                let cs = self.style_of(*c);
                let m = resolve_margin(&cs, self.dpr);
                sum_h += h + m.y();
            }
            sum_h + gap * flow_ids.len().saturating_sub(1) as f32
        };
        // Phase 36 PR2a (decision 353): opt-in vertical shrink on the
        // Column axis. Heights do not drive measurement (unlike widths
        // — re-laying into a smaller height re-measures identically),
        // so shrink clamps the committed box in place (`set_box_h`
        // min-grow, the Stretch max-grow mirror; the subtree keeps its
        // relative positions, only the outer `h` changes — content past
        // the box overflows visibly, the CSS-overflow rule). Children
        // without `flex_shrink` (or with explicit `h`) never shrink.
        // Div ignores the whole arm (block-lite — the pool-filter rule).
        if tag == Tag::Column {
            if let Some(content_h) = content_h_fill {
                let overflow = extent_h_pre - content_h;
                if overflow > f32::EPSILON {
                    let shrinkable: Vec<(NodeId, f32, f32)> = flow_ids
                        .iter()
                        .filter_map(|child| {
                            let cs = self.style_of(*child);
                            let f = cs.flex_shrink.map(|p| p.get()).unwrap_or(0.0);
                            let h = sizes.get(child).map(|s| s.h).unwrap_or(0.0);
                            if f > 0.0 && cs.h.is_none() && h > 0.0 {
                                Some((*child, h, f * h))
                            } else {
                                None
                            }
                        })
                        .collect();
                    let total: f32 = shrinkable.iter().map(|(_, _, w)| w).sum();
                    if total > 0.0 && total.is_finite() {
                        for (child, base, weight) in shrinkable {
                            let new_h = (base - overflow * weight / total).max(0.0);
                            if !new_h.is_finite() {
                                panic!("layout: column shrink height non-finite ({new_h})");
                            }
                            if (new_h - base).abs() > f32::EPSILON {
                                self.set_box_h(child, new_h);
                                if let Some(s) = sizes.get(&child).copied() {
                                    sizes.insert(child, Size { w: s.w, h: new_h });
                                }
                            }
                        }
                        let mut sum_h = 0.0f32;
                        for c in &flow_ids {
                            let h = sizes.get(c).map(|s| s.h).unwrap_or(0.0);
                            let cs = self.style_of(*c);
                            let m = resolve_margin(&cs, self.dpr);
                            sum_h += h + m.y();
                        }
                        extent_h_pre = sum_h + gap * flow_ids.len().saturating_sub(1) as f32;
                    }
                }
            }
        }
        // Justify (main axis Y): leftover distribution.
        let content_h_box = explicit_h.map(|h| (h - pad.y()).max(0.0));
        let effective_content_h = content_h_box.unwrap_or(extent_h_pre);
        let leftover = effective_content_h - extent_h_pre;
        check_leftover("column justify", leftover);
        let (start_offset, gap_extra) = match justify {
            JustifyContent::Start => (0.0, 0.0),
            JustifyContent::Center => (leftover / 2.0, 0.0),
            JustifyContent::End => (leftover, 0.0),
            JustifyContent::SpaceBetween => {
                if flow_ids.len() <= 1 {
                    (0.0, 0.0)
                } else {
                    (0.0, leftover.max(0.0) / (flow_ids.len() - 1) as f32)
                }
            }
        };
        if !start_offset.is_finite() || !gap_extra.is_finite() {
            panic!(
                "layout: column justify offsets non-finite (start {start_offset}, extra {gap_extra}, leftover {leftover})"
            );
        }
        // Place flow children in order with cross-axis alignment.
        // Margins offset the child origin and ride the advance
        // (Decision 249, same rule as Row); `x` overrides compose
        // with the horizontal margin (flow children only —
        // `absolute_y` bypasses margins like alignment, stated).
        let mut cursor = content_y + start_offset;
        let mut first = true;
        for child in &flow_ids {
            let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
            if !first {
                cursor += gap + gap_extra;
            }
            first = false;
            let cs = self.style_of(*child);
            let m = resolve_margin(&cs, self.dpr);
            let (x_off, _) = resolve_offsets(&cs, self.dpr);
            let cx = match x_off {
                Some(v) => content_x + v + m.left,
                None => match align {
                    AlignItems::Start | AlignItems::Stretch => content_x + m.left,
                    AlignItems::Center => content_x + (effective_content_w - s.w) / 2.0 + m.left,
                    AlignItems::End => content_x + (effective_content_w - s.w) + m.left,
                },
            };
            if !cx.is_finite() {
                panic!("layout: column align x non-finite ({cx})");
            }
            self.reposition(*child, cx, cursor + m.top);
            cursor += s.h + m.y();
        }
        let mut extent_h = extent_h_pre;
        for child in children {
            if self.is_portal(*child) {
                continue;
            }
            let cs = self.style_of(*child);
            let (_, ay_off) = resolve_offsets(&cs, self.dpr);
            let Some(ay) = ay_off else {
                continue;
            };
            let stretch_here =
                !block && align == AlignItems::Stretch && cs.w.is_none() && !cs.fill_width;
            let cw = if block || cs.fill_width || (stretch_here && content_w.is_some()) {
                content_w
            } else {
                None
            };
            let (x_off, _) = resolve_offsets(&cs, self.dpr);
            let cx = x_off.map(|v| content_x + v).unwrap_or(content_x);
            self.layout_node(
                *child,
                cx,
                content_y + ay,
                cw,
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            // Excluded from auto height (decision 70: virtualized slots must
            // not grow the content; `content_size` floors it instead).
        }
        self.layout_portals(
            &portals,
            content_x,
            content_y,
            inherited_px,
            inherited_weight,
        );
        extent_h = extent_h.max(content_floor);
        let w = explicit_w.or(given_w).unwrap_or(extent_w + pad.x());
        let h = explicit_h.unwrap_or(extent_h + pad.y());
        self.commit_box(id, ox, oy, w, h, extent_w, extent_h, Vec::new());
        Size { w, h }
    }

    /// Overlay portal layer (Round 1.4, decision 255; Round 7.21,
    /// decision 296: full-viewport + parent-anchored).
    ///
    /// Origin: a portal with `x`/`absolute_y` offsets anchors at its
    /// parent container's content origin (`parent_x`/`parent_y` —
    /// dropdown popups ride their `Select` box); without offsets it
    /// sits at the viewport origin (full-screen modals). The later
    /// `reposition` of the parent moves the whole subtree, so the
    /// anchor tracks the parent under the two-phase measure/place
    /// contract. Nested portals receive the same parent origin —
    /// offset-less ones stay viewport-anchored, exactly as before.
    ///
    /// Size: explicit `w`/`h` win. Unconstrained height defaults to
    /// the viewport (full-bleed modal dimming — decision 255's
    /// stated follow-up, now closed); an ANCHORED portal's
    /// unconstrained height hugs its content (`extent.h +
    /// 2*pad_y`, so dropdowns never balloon). Width still defaults
    /// to the viewport; the content width derives from the portal's
    /// own width (explicit or viewport — a 160px popup's list fills
    /// its box, not the window). Children with `fill_height` lay
    /// out against `content_h` through the `given_h` channel, so a
    /// modal backdrop centers like an explicitly-sized container;
    /// non-fill children ignore it. `gap` is meaningless without
    /// flow (stated ignore, the align-on-leaves precedent);
    /// margins ride children like `Stack`.
    fn layout_portal(
        &mut self,
        id: NodeId,
        parent_x: f32,
        parent_y: f32,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) -> Size {
        let style = self.style_of(id);
        let pad = resolve_pad(&style, self.dpr);
        let (explicit_w, explicit_h) = resolve_explicit(&style, self.dpr);
        let content_floor = resolve_floor(&style, self.dpr);
        let (x_off, ay_off) = resolve_offsets(&style, self.dpr);
        let anchored = x_off.is_some() || ay_off.is_some();
        let ox = x_off.map(|v| parent_x + v).unwrap_or(0.0);
        let oy = ay_off.map(|v| parent_y + v).unwrap_or(0.0);
        let portal_w = explicit_w.unwrap_or(self.viewport_w);
        let content_x = ox + pad.left;
        let content_y = oy + pad.top;
        let content_w = (portal_w - pad.x()).max(0.0);
        let content_h = (explicit_h.unwrap_or(self.viewport_h) - pad.y()).max(0.0);
        let children = self
            .rec
            .get(id)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        let mut extent = Size { w: 0.0, h: 0.0 };
        let mut portals: Vec<NodeId> = Vec::new();
        for child in &children {
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            let m = resolve_margin(&cs, self.dpr);
            let (ox_off, ay_off) = resolve_offsets(&cs, self.dpr);
            let cx = ox_off.map(|v| content_x + v).unwrap_or(content_x);
            let cy = ay_off.map(|ay| content_y + ay).unwrap_or(content_y);
            let s = self.layout_node(
                *child,
                cx + m.left,
                cy + m.top,
                Some(content_w),
                Some(content_h),
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            extent.w = extent.w.max(s.w + m.x() + ox_off.unwrap_or(0.0));
            extent.h = extent.h.max(s.h + m.y() + ay_off.unwrap_or(0.0));
        }
        self.layout_portals(&portals, parent_x, parent_y, inherited_px, inherited_weight);
        let w = explicit_w.unwrap_or(self.viewport_w);
        let h = explicit_h.unwrap_or(if anchored {
            extent.h + pad.y()
        } else {
            self.viewport_h
        });
        self.commit_box(
            id,
            ox,
            oy,
            w,
            h,
            extent.w,
            extent.h.max(content_floor),
            Vec::new(),
        );
        Size { w, h }
    }

    /// Stack: children overlaid at the content origin (+ per-axis
    /// overrides); extent is the max child extent. Decision 237: pads
    /// inset the content origin (auto sizes add the pairs); Round
    /// 11.1: asymmetric sides flow through (see [`Pad`]).
    /// `absolute_y` offsets from the content origin, which is `oy`
    /// when vertical pads are absent for backward compatibility.
    #[allow(clippy::too_many_arguments)]
    fn layout_stack(
        &mut self,
        id: NodeId,
        children: &[NodeId],
        ox: f32,
        oy: f32,
        content_x: f32,
        content_y: f32,
        pad: Pad,
        explicit_w: Option<f32>,
        explicit_h: Option<f32>,
        given_w: Option<f32>,
        content_floor: f32,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) -> Size {
        // Round 1.2 (decision 253): only Row wraps — Stack/ScrollArea
        // with Wrap refuse loudly (same unimplemented-axis rule as
        // Column/Div in `layout_vertical`).
        if self
            .style_of(id)
            .flex_wrap
            .is_some_and(|w| w == FlexWrap::Wrap)
        {
            panic!(
                "layout: flex_wrap=Wrap on Stack is not supported in v1 — only Row wraps; remove Wrap or use a Row"
            );
        }
        // Margins offset overlaid children and inflate the max extent
        // (Decision 249, same rule as the flex containers). Portals
        // never join the stack (Round 1.4 — viewport-anchored after).
        let mut extent = Size { w: 0.0, h: 0.0 };
        let mut portals: Vec<NodeId> = Vec::new();
        for child in children {
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            let m = resolve_margin(&cs, self.dpr);
            let (x_off, ay_off) = resolve_offsets(&cs, self.dpr);
            let cx = x_off.map(|v| content_x + v).unwrap_or(content_x) + m.left;
            let cy = ay_off.map(|v| content_y + v).unwrap_or(content_y) + m.top;
            let s = self.layout_node(
                *child,
                cx,
                cy,
                None,
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            extent.w = extent.w.max(cx - content_x + s.w + m.right);
            extent.h = extent.h.max(cy - content_y + s.h + m.bottom);
        }
        self.layout_portals(
            &portals,
            content_x,
            content_y,
            inherited_px,
            inherited_weight,
        );
        let w = explicit_w.or(given_w).unwrap_or(extent.w + pad.x());
        let h = explicit_h.unwrap_or(extent.h + pad.y());
        self.commit_box(
            id,
            ox,
            oy,
            w,
            h,
            extent.w,
            extent.h.max(content_floor),
            Vec::new(),
        );
        Size { w, h }
    }

    /// ScrollArea: explicit viewport (fallback: children extent); normal
    /// children stack vertically like a Column; `absolute_y` children pin
    /// to content offsets (the §4.2 virtualized-slot shape);
    /// `content_size` floors the scroll extent. Decision 237: pads inset
    /// the content box (auto sizes add the pairs); Round 11.1:
    /// asymmetric sides flow through (see [`Pad`]). `absolute_y`
    /// offsets from the content origin.
    #[allow(clippy::too_many_arguments)]
    fn layout_scroll(
        &mut self,
        id: NodeId,
        children: &[NodeId],
        ox: f32,
        oy: f32,
        content_x: f32,
        content_y: f32,
        pad: Pad,
        gap: f32,
        explicit_w: Option<f32>,
        explicit_h: Option<f32>,
        given_w: Option<f32>,
        content_floor: f32,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) -> Size {
        // Round 1.2 (decision 253): only Row wraps (see `layout_stack`).
        if self
            .style_of(id)
            .flex_wrap
            .is_some_and(|w| w == FlexWrap::Wrap)
        {
            panic!(
                "layout: flex_wrap=Wrap on ScrollArea is not supported in v1 — only Row wraps; remove Wrap or use a Row"
            );
        }
        let outer_w = explicit_w.or(given_w);
        let content_w = outer_w.map(|w| (w - pad.x()).max(0.0));
        let mut cursor = content_y;
        let mut first = true;
        let mut extent = Size { w: 0.0, h: 0.0 };
        let mut portals: Vec<NodeId> = Vec::new();
        for child in children {
            // Portals never join parent flow (Round 1.4).
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            if cs.absolute_y.is_some() {
                continue;
            }
            let cw = if cs.fill_width { content_w } else { None };
            let s = self.layout_node(
                *child,
                0.0,
                0.0,
                cw,
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            if !first {
                cursor += gap;
            }
            first = false;
            // Margins offset the child and ride the advance; the
            // `absolute_y` arm below bypasses them (Decision 249 —
            // out-of-flow, same class as the Row/Column bypass).
            let m = resolve_margin(&cs, self.dpr);
            let (x_off, _) = resolve_offsets(&cs, self.dpr);
            let cx = x_off.map(|v| content_x + v).unwrap_or(content_x) + m.left;
            self.reposition(*child, cx, cursor + m.top);
            cursor += s.h + m.y();
            extent.w = extent.w.max(s.w + m.x());
            extent.h = cursor - content_y;
        }
        for child in children {
            if self.is_portal(*child) {
                continue;
            }
            let cs = self.style_of(*child);
            let (x_off, ay_off) = resolve_offsets(&cs, self.dpr);
            let Some(ay) = ay_off else {
                continue;
            };
            let cw = if cs.fill_width { content_w } else { None };
            let cx = x_off.map(|v| content_x + v).unwrap_or(content_x);
            let s = self.layout_node(
                *child,
                cx,
                content_y + ay,
                cw,
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            extent.w = extent.w.max(s.w);
            extent.h = extent.h.max(ay + s.h);
        }
        self.layout_portals(
            &portals,
            content_x,
            content_y,
            inherited_px,
            inherited_weight,
        );
        extent.h = extent.h.max(content_floor);
        let w = explicit_w.or(given_w).unwrap_or(extent.w + pad.x());
        let h = explicit_h.unwrap_or(extent.h + pad.y());
        self.commit_box(id, ox, oy, w, h, extent.w, extent.h, Vec::new());
        Size { w, h }
    }

    /// Minimal 2D grid container (Phase 36 PR2a, decision 353 — G15):
    /// column/row templates from [`Style::grid_cols`]/
    /// [`Style::grid_rows`](crate::style::Style) ([`GridTrack`]
    /// tracks), children in row-major auto-flow with
    /// [`Style::col_span`]/[`Style::row_span`].
    ///
    /// Rules (all stated, never silent):
    /// - Empty `grid_cols` = one `Auto` track; empty `grid_rows` =
    ///   fully implicit `Auto` rows. Rows beyond the template append
    ///   implicit `Auto` rows (CSS auto-flow rule).
    /// - `Px` tracks are fixed; `Fr` tracks split the leftover after
    ///   fixed + content tracks proportionally (unconstrained parents
    ///   have no leftover — `Fr` falls back to `Auto`, the
    ///   Wrap-unconstrained precedent). `Auto` tracks size to the max
    ///   intrinsic of their non-spanning children.
    /// - Spans default 1; span 0 refuses loudly; a column span wider
    ///   than the template refuses loudly (ambiguous — never a silent
    ///   clamp). A span not fitting the row remainder wraps to the
    ///   next row (CSS auto-flow placement).
    /// - No explicit placement (no col/row start — spans only);
    ///   `align_items`/`justify_content` do not apply inside cells
    ///   (children sit top-left of their cell — stated ignore, the
    ///   portal-gap precedent); `gap` spaces tracks on both axes;
    ///   pads inset the content box; margins ride the child origin
    ///   (Decision 249) and count in the track content.
    /// - `x`/`absolute_y` children bypass flow (out-of-flow, the
    ///   Row/Column bypass precedent) and never grow the extent;
    ///   portals never join flow (Round 1.4). `flex_wrap = Wrap` on a
    ///   grid refuses loudly (only Row wraps — the ScrollArea
    ///   precedent).
    #[allow(clippy::too_many_arguments)]
    fn layout_grid(
        &mut self,
        id: NodeId,
        children: &[NodeId],
        ox: f32,
        oy: f32,
        content_x: f32,
        content_y: f32,
        pad: Pad,
        gap: f32,
        explicit_w: Option<f32>,
        explicit_h: Option<f32>,
        given_w: Option<f32>,
        content_floor: f32,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) -> Size {
        if self
            .style_of(id)
            .flex_wrap
            .is_some_and(|w| w == FlexWrap::Wrap)
        {
            panic!(
                "layout: flex_wrap=Wrap on Grid is not supported — only Row wraps; remove Wrap or use a Row"
            );
        }
        let template = self.style_of(id);
        let cols: Vec<GridTrack> = if template.grid_cols.is_empty() {
            vec![GridTrack::Auto]
        } else {
            template.grid_cols.clone()
        };
        let row_tpl: Vec<GridTrack> = template.grid_rows.clone();
        let ncols = cols.len();
        // Flow collection + auto-flow placement (row-major with span
        // wrap; occupancy marks every covered cell).
        let mut flow: Vec<(NodeId, usize, usize, u32, u32)> = Vec::new();
        let mut portals: Vec<NodeId> = Vec::new();
        let mut occupied: Vec<Vec<bool>> = Vec::new();
        let ensure_row = |occupied: &mut Vec<Vec<bool>>, r: usize, ncols: usize| {
            while occupied.len() <= r {
                occupied.push(vec![false; ncols]);
            }
        };
        for child in children {
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            if cs.x.is_some() || cs.absolute_y.is_some() {
                continue; // out-of-flow; placed after
            }
            let cspan = cs.col_span.unwrap_or(1);
            let rspan = cs.row_span.unwrap_or(1);
            if cspan == 0 || rspan == 0 {
                panic!(
                    "layout: grid span of 0 refuses loudly — spans start at 1 (child {child:?})"
                );
            }
            if cspan as usize > ncols {
                panic!(
                    "layout: grid col_span {cspan} wider than the {ncols}-column template — ambiguous, never a silent clamp (child {child:?})"
                );
            }
            // Row-major cursor: first (r, c) whose span fits free cells.
            let (mut r, mut c) = flow
                .last()
                .map(|(_, lr, lc, ls, _)| (*lr, *lc + *ls as usize))
                .unwrap_or((0, 0));
            loop {
                if c + cspan as usize > ncols {
                    r += 1;
                    c = 0;
                }
                ensure_row(&mut occupied, r + rspan as usize - 1, ncols);
                let free = (0..cspan as usize).all(|dc| !occupied[r][c + dc]);
                if free {
                    break;
                }
                c += 1;
            }
            ensure_row(&mut occupied, r + rspan as usize - 1, ncols);
            for dr in 0..rspan as usize {
                for dc in 0..cspan as usize {
                    occupied[r + dr][c + dc] = true;
                }
            }
            flow.push((*child, r, c, cspan, rspan));
        }
        let nrows = occupied.len();
        // Phase 1: intrinsic measure (origin, re-laid into cells after).
        let mut sizes: HashMap<NodeId, Size> = HashMap::new();
        for (child, _, _, _, _) in &flow {
            let s = self.layout_node(
                *child,
                0.0,
                0.0,
                None,
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            sizes.insert(*child, s);
        }
        // Column widths: Px fixed; Auto = max non-spanning content;
        // Fr = leftover share (Auto fallback when unconstrained).
        let outer_w = explicit_w.or(given_w);
        let content_w = outer_w.map(|w| (w - pad.x()).max(0.0));
        let gaps_w = gap * ncols.saturating_sub(1) as f32;
        let mut col_w = vec![0.0f32; ncols];
        for (ci, track) in cols.iter().enumerate() {
            match track {
                GridTrack::Px(px) => col_w[ci] = px.get() * self.dpr,
                GridTrack::Auto => {
                    let mut best = 0.0f32;
                    for (child, _, c, cspan, _) in &flow {
                        if *cspan == 1 && *c == ci {
                            let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                            let cs = self.style_of(*child);
                            let m = resolve_margin(&cs, self.dpr);
                            best = best.max(s.w + m.x());
                        }
                    }
                    col_w[ci] = best;
                }
                GridTrack::Fr(_) => {}
            }
        }
        let total_fr: f32 = cols
            .iter()
            .filter_map(|t| match t {
                GridTrack::Fr(w) => Some(w.get()),
                _ => None,
            })
            .sum();
        let mut fr_fallback_auto = false;
        if total_fr > 0.0 {
            match content_w {
                Some(cw) => {
                    let fixed: f32 = col_w.iter().sum();
                    let remainder = (cw - fixed - gaps_w).max(0.0);
                    check_leftover("grid fr columns", remainder);
                    if !remainder.is_finite() || !total_fr.is_finite() {
                        panic!(
                            "layout: grid fr share non-finite (remainder {remainder}, total {total_fr})"
                        );
                    }
                    for (ci, track) in cols.iter().enumerate() {
                        if let GridTrack::Fr(w) = track {
                            col_w[ci] = remainder * w.get() / total_fr;
                        }
                    }
                }
                None => fr_fallback_auto = true,
            }
        }
        if fr_fallback_auto {
            for (ci, track) in cols.iter().enumerate() {
                if matches!(track, GridTrack::Fr(_)) {
                    let mut best = 0.0f32;
                    for (child, _, c, cspan, _) in &flow {
                        if *cspan == 1 && *c == ci {
                            let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                            let cs = self.style_of(*child);
                            let m = resolve_margin(&cs, self.dpr);
                            best = best.max(s.w + m.x());
                        }
                    }
                    col_w[ci] = best;
                }
            }
        }
        // Cell widths (spanned cells sum tracks + inner gaps), then
        // phase 2: re-lay children into their cell width (re-wrap is
        // correct for text — the stretch second-pass precedent).
        let cell_x = |c: usize, span: u32| -> f32 {
            let span = span as usize;
            col_w[c..c + span].iter().sum::<f32>() + gap * span.saturating_sub(1) as f32
        };
        self.stats.layout_passes = self.stats.layout_passes.max(2);
        for (child, _, c, cspan, _) in &flow {
            let s = self.layout_node(
                *child,
                0.0,
                0.0,
                Some(cell_x(*c, *cspan)),
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            sizes.insert(*child, s);
        }
        // Row heights from second-pass sizes; Px fixed; Fr shares the
        // explicit-height remainder (Auto fallback unconstrained);
        // implicit rows (beyond the template) are always Auto.
        let mut row_h = vec![0.0f32; nrows];
        for (r, slot) in row_h.iter_mut().enumerate() {
            let track = row_tpl.get(r).copied().unwrap_or(GridTrack::Auto);
            match track {
                GridTrack::Px(px) => *slot = px.get() * self.dpr,
                GridTrack::Auto => {
                    let mut best = 0.0f32;
                    for (child, rr, _, _, rspan) in &flow {
                        if *rspan == 1 && *rr == r {
                            let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                            let cs = self.style_of(*child);
                            let m = resolve_margin(&cs, self.dpr);
                            best = best.max(s.h + m.y());
                        }
                    }
                    *slot = best;
                }
                GridTrack::Fr(_) => {}
            }
        }
        let row_fr: f32 = row_tpl
            .iter()
            .filter_map(|t| match t {
                GridTrack::Fr(w) => Some(w.get()),
                _ => None,
            })
            .sum();
        if row_fr > 0.0 {
            match explicit_h {
                Some(eh) => {
                    let content_h = (eh - pad.y()).max(0.0);
                    let gaps_h = gap * nrows.saturating_sub(1) as f32;
                    let fixed: f32 = row_h.iter().sum();
                    let remainder = (content_h - fixed - gaps_h).max(0.0);
                    check_leftover("grid fr rows", remainder);
                    if !remainder.is_finite() || !row_fr.is_finite() {
                        panic!(
                            "layout: grid fr row share non-finite (remainder {remainder}, total {row_fr})"
                        );
                    }
                    for (r, track) in row_tpl.iter().enumerate() {
                        if let GridTrack::Fr(w) = track {
                            row_h[r] = remainder * w.get() / row_fr;
                        }
                    }
                }
                None => {
                    for (r, track) in row_tpl.iter().enumerate() {
                        if matches!(track, GridTrack::Fr(_)) {
                            let mut best = 0.0f32;
                            for (child, rr, _, _, rspan) in &flow {
                                if *rspan == 1 && *rr == r {
                                    let s = sizes
                                        .get(child)
                                        .copied()
                                        .unwrap_or(Size { w: 0.0, h: 0.0 });
                                    let cs = self.style_of(*child);
                                    let m = resolve_margin(&cs, self.dpr);
                                    best = best.max(s.h + m.y());
                                }
                            }
                            row_h[r] = best;
                        }
                    }
                }
            }
        }
        // Place: cell origins accumulate tracks + gaps; margins offset
        // the child origin (Decision 249); spanning children cover
        // their tracks + inner gaps.
        let mut col_x = vec![0.0f32; ncols + 1];
        for ci in 0..ncols {
            col_x[ci + 1] = col_x[ci] + col_w[ci] + gap;
        }
        let mut row_y = vec![0.0f32; nrows + 1];
        for r in 0..nrows {
            row_y[r + 1] = row_y[r] + row_h[r] + gap;
        }
        for (child, r, c, _, _) in &flow {
            let cs = self.style_of(*child);
            let m = resolve_margin(&cs, self.dpr);
            let cx = content_x + col_x[*c] + m.left;
            let cy = content_y + row_y[*r] + m.top;
            if !cx.is_finite() || !cy.is_finite() {
                panic!("layout: grid cell origin non-finite ({cx}, {cy})");
            }
            self.reposition(*child, cx, cy);
        }
        // Out-of-flow second pass (x/absolute_y bypass flow and the
        // extent — the Row/Column bypass precedent).
        for child in children {
            if self.is_portal(*child) {
                continue;
            }
            let cs = self.style_of(*child);
            let (x_off, ay_off) = resolve_offsets(&cs, self.dpr);
            if x_off.is_none() && ay_off.is_none() {
                continue;
            }
            let cx = x_off.map(|v| content_x + v).unwrap_or(content_x);
            let cy = ay_off.map(|ay| content_y + ay).unwrap_or(content_y);
            self.layout_node(
                *child,
                cx,
                cy,
                None,
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
        }
        self.layout_portals(
            &portals,
            content_x,
            content_y,
            inherited_px,
            inherited_weight,
        );
        let extent_w: f32 = col_w.iter().sum::<f32>() + gaps_w;
        let extent_h: f32 = row_h.iter().sum::<f32>() + gap * nrows.saturating_sub(1) as f32;
        let extent_h = extent_h.max(content_floor);
        let w = explicit_w.or(given_w).unwrap_or(extent_w + pad.x());
        let h = explicit_h.unwrap_or(extent_h + pad.y());
        self.commit_box(id, ox, oy, w, h, extent_w, extent_h, Vec::new());
        Size { w, h }
    }

    /// Grows a committed box's outer height in place (Row Stretch, Decision
    /// 237): the subtree stays top-aligned — descendants keep their relative
    /// positions, only the outer `h` changes. Content extents and text lines
    /// are untouched (the parent's content report stays an extent, not the
    /// container area). Panics loudly on non-finite targets.
    fn set_box_h(&mut self, id: NodeId, new_h: f32) {
        if !new_h.is_finite() {
            panic!("layout: stretch target h is non-finite ({new_h})");
        }
        if let Some(n) = self.rec.node_mut(id) {
            if let Some(b) = n.layout.as_mut() {
                b.h = new_h;
            }
        }
    }

    /// Moves a laid subtree by translating its committed box and every
    /// descendant box (two-phase measure-at-origin then place: keeps the
    /// flex code linear without threading cursors through recursion).
    /// Viewport-anchored portals are NOT moved (Round 7.21 follow-up):
    /// an offset-less portal lives in viewport space, never parent
    /// space — dragging it with a repositioned ancestor slides a
    /// full-window modal dim to the trigger's position. Portals WITH
    /// offsets are parent-relative and keep tracking (dropdown
    /// popups). Containers never reposition a portal directly (they
    /// skip them), so portals are only ever met as descendants here.
    fn reposition(&mut self, id: NodeId, x: f32, y: f32) {
        let (dx, dy) = match self.rec.get(id) {
            Some(n) => match &n.layout {
                Some(b) => (x - b.x, y - b.y),
                None => return,
            },
            None => return,
        };
        // Positions were snapped at commit; translate in snapped space so a
        // second commit is a fixed point (idempotent repositioning).
        let dx = round_to_device_px(dx, self.dpr);
        let dy = round_to_device_px(dy, self.dpr);
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if cur != id && self.is_viewport_portal(cur) {
                continue;
            }
            let kids = match self.rec.get(cur) {
                Some(n) => n.children.clone(),
                None => continue,
            };
            if let Some(n) = self.rec.node_mut(cur) {
                if let Some(b) = n.layout.as_mut() {
                    b.x = round_to_device_px(b.x + dx, self.dpr);
                    b.y = round_to_device_px(b.y + dy, self.dpr);
                }
            }
            stack.extend(kids);
        }
    }

    /// True for offset-less overlay portals (Round 7.21 follow-up):
    /// viewport-space layers that [`LayoutCtx::reposition`] must not
    /// translate with their parent.
    fn is_viewport_portal(&self, id: NodeId) -> bool {
        let Some(n) = self.rec.get(id) else {
            return false;
        };
        if n.tag != Tag::Portal {
            return false;
        }
        let style = self.style_of(id);
        style.x.is_none() && style.absolute_y.is_none()
    }

    /// Text leaf: measure (cached) then wrap/ellipsis into lines.
    #[allow(clippy::too_many_arguments)]
    fn layout_text_leaf(
        &mut self,
        id: NodeId,
        text: &str,
        size_px: f32,
        weight: crate::text::FontWeight,
        ox: f32,
        oy: f32,
        constrain_w: Option<f32>,
        explicit_w: Option<f32>,
        explicit_h: Option<f32>,
    ) -> Size {
        // U8 scoped sizing (decision 189): an empty FIELD payload
        // measures as one space — the space advance is the width
        // minimum and the font ascent/descent is the line box, both
        // measured, never invented. Static empty text stays zero
        // (the v1 bound); unmeasured text (no service) stays zero
        // here and is owned browser-side by the DOM backend.
        // Round 1.1 (decision 252): both widths are constraints —
        // NaN/negative never becomes a silent wrap shape.
        if let Some(w) = constrain_w {
            check_constrain_width("constrain_w", w);
        }
        if let Some(w) = explicit_w {
            check_constrain_width("explicit_w", w);
        }
        let measure_text = if text.is_empty() && self.field_payload(id) {
            " "
        } else {
            text
        };
        let measured = self.measure(id, measure_text, size_px, weight);
        let (shaped, ascent, descent, line_gap) = match measured {
            Some(m) => (m.shaped.clone(), m.ascent, m.descent, m.line_gap),
            None => {
                self.commit_box(id, ox, oy, 0.0, 0.0, 0.0, 0.0, Vec::new());
                return Size { w: 0.0, h: 0.0 };
            }
        };
        let text_len = shaped.text_len_bytes;
        if shaped.clusters.is_empty() || text_len == 0 {
            let w = explicit_w.or(constrain_w).unwrap_or(0.0);
            let h = explicit_h.unwrap_or(0.0);
            self.commit_box(id, ox, oy, w, h, 0.0, 0.0, Vec::new());
            return Size { w, h };
        }
        let ellipsis_adv = if self.engine.config.ellipsis {
            Some(self.ellipsis_advance(size_px, weight))
        } else {
            None
        };
        let avail = constrain_w.unwrap_or(f32::INFINITY);
        // Opportunity-driven wrapping (v2 item 2): when a break source
        // is installed, `layout_text` consumes its offsets the way it
        // consumes the `ShapedRun`. Without one, legacy greedy wrap
        // (all M3 behavior preserved).
        let mut lines = match self.engine.break_source() {
            Some(source) => {
                let breaks = source.opportunities(measure_text);
                layout_text_with_breaks(
                    &shaped,
                    measure_text,
                    avail,
                    ellipsis_adv,
                    ascent,
                    descent,
                    line_gap,
                    &breaks,
                )
            }
            None => layout_text(
                &shaped,
                measure_text,
                avail,
                ellipsis_adv,
                ascent,
                descent,
                line_gap,
            ),
        };
        // Post-pass context the pure order/wrap step cannot own (M7,
        // decision 110): exact em size + resolved per-run families.
        let em_size = size_px * self.dpr;
        let requested = self.engine.config.family.clone();
        for line in &mut lines {
            line.em_size = em_size;
            for run in &mut line.runs {
                if run.family.is_empty() {
                    run.family = self.engine.family_of(run.font_id, &requested);
                }
            }
        }
        self.stats.lines_laid += lines.len();
        let content_w = lines.iter().map(|l| l.width).fold(0.0f32, f32::max);
        let content_h = lines.last().map(|l| l.y + l.height).unwrap_or(0.0);
        let w = explicit_w.or(constrain_w).unwrap_or(content_w);
        let h = explicit_h.unwrap_or(content_h);
        self.commit_box(id, ox, oy, w, h, content_w, content_h, lines);
        Size { w, h }
    }

    /// Measure↔layout protocol: shape through the [`TextService`] iff the
    /// cache key (bytes, size, weight, family, dpr) misses. Returns None
    /// when no service is installed (headless M2 frames) or the text is
    /// empty — both size zero, never a failure.
    fn measure(
        &mut self,
        id: NodeId,
        text: &str,
        size_px: f32,
        weight: crate::text::FontWeight,
    ) -> Option<MeasuredText> {
        let cfg = &self.engine.config;
        let key = measure_key(text, size_px, weight, cfg);
        if let Some(n) = self.rec.get(id) {
            if let Some(m) = &n.measured {
                if m.key == key {
                    return Some(m.clone());
                }
            }
        }
        if text.is_empty() {
            return None;
        }
        let service = self.service?;
        let style = TextStyle {
            family: cfg.family.clone(),
            font_size_px: size_px,
            device_pixel_ratio: cfg.device_pixel_ratio,
            weight,
            style: crate::text::FontStyle::Normal,
            stretch: crate::text::FontStretch::NORMAL,
            letter_spacing_px: 0.0,
            locale: "en-US".to_string(),
        };
        let family = cfg.family.clone();
        let (shaped, shaped_count) = if text.contains('\n') {
            // Real backends map no font for '\n' (loud refusal), so
            // each '\n'-paragraph shapes whole on its own and stitches
            // into one run with '\n' gaps (decision 196: joining across
            // a hard break is meaningless; joining across soft breaks
            // inside each paragraph is preserved; re-wrap still shapes
            // nothing). Texts without newlines take the single shape
            // call below, exactly as before.
            match shape_paragraphs(service, text, &style, &family, size_px) {
                Some((run, n)) => (run, n),
                None => return None,
            }
        } else {
            match service.shape(text, &style) {
                Ok(r) => (r, 1),
                Err(TextError::EmptyText) => return None,
                Err(e) => panic!(
                    "layout: text measurement failed (family {:?}, {size_px}px): {e} — \
                     a shaping failure is a backend/config bug, never silent",
                    cfg.family,
                ),
            }
        };
        let metrics = service.measure_line(&shaped);
        self.stats.nodes_shaped += shaped_count;
        let measured = MeasuredText {
            key,
            shaped,
            width: metrics.width,
            ascent: metrics.ascent,
            descent: metrics.descent,
            line_gap: metrics.line_gap,
        };
        if let Some(n) = self.rec.node_mut(id) {
            n.measured = Some(measured.clone());
        }
        Some(measured)
    }

    /// True when `id` is the absorbed text payload of a field (U8
    /// scoped sizing, decision 189): leaves never carry semantics
    /// themselves — the field element owns them for its payload
    /// child (the `TextField` conversion shape). Static text has
    /// no such parent and stays zero when empty.
    fn field_payload(&self, id: NodeId) -> bool {
        let parent = self.rec.get(id).and_then(|n| n.parent);
        parent.is_some_and(|p| {
            self.rec
                .get(p)
                .and_then(|n| n.semantics.as_ref())
                .is_some_and(|s| s.role == crate::semantics::Role::TextField)
        })
    }

    /// Amortized ellipsis advance per style (one `shape("…")` per distinct
    /// style — family, size, weight, dpr; graceful zero when the backend
    /// cannot shape it — decoration must not crash layout).
    fn ellipsis_advance(&mut self, size_px: f32, weight: crate::text::FontWeight) -> f32 {
        let cfg = &self.engine.config;
        let key = (
            fnv1a64(cfg.family.as_bytes()),
            size_px.to_bits(),
            cfg.device_pixel_ratio.to_bits(),
            weight.0,
        );
        if let Some(w) = self.engine.ellipsis_widths.get(&key) {
            return *w;
        }
        let w = match self.service {
            Some(service) => {
                let style = TextStyle {
                    family: cfg.family.clone(),
                    font_size_px: size_px,
                    device_pixel_ratio: cfg.device_pixel_ratio,
                    weight,
                    style: crate::text::FontStyle::Normal,
                    stretch: crate::text::FontStretch::NORMAL,
                    letter_spacing_px: 0.0,
                    locale: "en-US".to_string(),
                };
                service
                    .shape("…", &style)
                    .map(|r| r.total_advance)
                    .unwrap_or(0.0)
            }
            None => 0.0,
        };
        self.engine.ellipsis_widths.insert(key, w);
        self.stats.nodes_shaped += 1;
        w
    }
}

fn resolve_text_px(hint: Option<TextClass>, inherited: f32, cfg: &LayoutTextConfig) -> f32 {
    match hint {
        Some(TextClass::TitleSmall) => cfg.title_px,
        Some(TextClass::BodySecondary) => cfg.body_px,
        // Custom sizes are absolute CSS px (decision 239) — a zero size
        // is refused loudly, never laid out as a silent zero box.
        Some(TextClass::Custom { size_px, .. }) => {
            if size_px == 0 {
                panic!(
                    "layout: TextClass::Custom size_px is 0 — zero-size text never lays out silently"
                );
            }
            size_px as f32
        }
        None => inherited,
    }
}

/// Author-specified shaper weight (decision 239): the two tokens resolve
/// `NORMAL`; `Custom` carries its weight; hint-less text (including the
/// measured bare-text leaf inside a `Text` wrapper) inherits. Weights
/// outside the `1..=999` DWRITE scale are refused loudly — a backend must
/// never receive a weight it cannot interpret.
fn resolve_text_weight(
    hint: Option<TextClass>,
    inherited: crate::text::FontWeight,
) -> crate::text::FontWeight {
    match hint {
        Some(TextClass::Custom { weight, .. }) => {
            if weight.0 == 0 || weight.0 > 999 {
                panic!(
                    "layout: TextClass::Custom weight is out of range ({}) — expected 1..=999, never laid out silently",
                    weight.0
                );
            }
            weight
        }
        Some(_) => crate::text::FontWeight::NORMAL,
        None => inherited,
    }
}

/// Shapes each `'\n'`-separated paragraph whole and stitches the runs
/// into one (byte ranges rebased to the full text, glyph ranges
/// rebased to the stitched glyph vec, advances summed,
/// `text_len_bytes` covering the `'\n'` gaps). Returns the run plus
/// the piece count for `nodes_shaped`, or None when every piece is
/// empty (e.g. `"\n"` — the leaf then commits a zero box, exactly
/// like empty text). A piece-level failure panics with the same loud
/// measurement message as the single-shape path.
fn shape_paragraphs(
    service: &dyn TextService,
    text: &str,
    style: &TextStyle,
    family: &str,
    size_px: f32,
) -> Option<(ShapedRun, usize)> {
    let mut start = 0usize;
    let mut pieces: Vec<(usize, &str)> = Vec::new();
    for (i, _) in text.match_indices('\n') {
        pieces.push((start, &text[start..i]));
        start = i + 1;
    }
    pieces.push((start, &text[start..]));
    let mut glyphs: Vec<crate::text::ShapedGlyph> = Vec::new();
    let mut runs: Vec<crate::text::TextRun> = Vec::new();
    let mut clusters: Vec<crate::text::Cluster> = Vec::new();
    let mut total = 0.0f32;
    let mut count = 0usize;
    for (off, piece) in pieces {
        if piece.is_empty() {
            continue;
        }
        let shaped = match service.shape(piece, style) {
            Ok(r) => r,
            Err(TextError::EmptyText) => continue,
            Err(e) => panic!(
                "layout: text measurement failed (family {family:?}, {size_px}px): {e} — \
                 a shaping failure is a backend/config bug, never silent",
            ),
        };
        let g0 = glyphs.len();
        glyphs.extend(shaped.glyphs.iter().copied());
        for mut run in shaped.runs {
            run.byte_range.0 += off;
            run.byte_range.1 += off;
            run.glyph_range.0 += g0;
            run.glyph_range.1 += g0;
            runs.push(run);
        }
        for mut cluster in shaped.clusters {
            cluster.byte_range.0 += off;
            cluster.byte_range.1 += off;
            cluster.glyph_range.0 += g0;
            cluster.glyph_range.1 += g0;
            clusters.push(cluster);
        }
        total += shaped.total_advance;
        count += 1;
    }
    if count == 0 {
        return None;
    }
    Some((
        ShapedRun {
            glyphs,
            runs,
            clusters,
            total_advance: total,
            text_len_bytes: text.len(),
        },
        count,
    ))
}

// ---------------------------------------------------------------------------
// Inline text: wrap + BiDi visual ordering + ellipsis (pure over ShapedRun)
// ---------------------------------------------------------------------------

/// Shaper-local font identity for the run covering `byte_start` (M7,
/// decision 110): clusters partition the text and runs partition the
/// text, so every cluster start belongs to exactly one run; the
/// unreachable fallback is `FontId(0)` (never a silent substitution —
/// the id rides the op for diagnostics either way).
fn run_font_of(shaped: &ShapedRun, byte_start: usize) -> FontId {
    shaped
        .runs
        .iter()
        .find(|r| byte_start >= r.byte_range.0 && byte_start < r.byte_range.1)
        .map(|r| r.font_id)
        .unwrap_or(FontId(0))
}

/// Logical cluster geometry: cumulative advances in source order.
fn logical_widths(shaped: &ShapedRun) -> Vec<f32> {
    let mut widths = Vec::with_capacity(shaped.clusters.len());
    for c in &shaped.clusters {
        let w: f32 = shaped.glyphs
            [c.glyph_range.0.min(shaped.glyphs.len())..c.glyph_range.1.min(shaped.glyphs.len())]
            .iter()
            .map(|g| g.x_advance)
            .sum();
        widths.push(w);
    }
    widths
}

/// Embedding level of a cluster for visual ordering (UBA-lite, decision
/// 76): LTR runs read level 0; RTL runs read level 1, except ASCII digits
/// (EN) inside an RTL run, which form an LTR island at level 2 (UBA I2:
/// `EN` at an odd level goes up one). Neutrals ride their run's level (the
/// N1 grouping the backend's run analysis already encodes: the space
/// between Latin and Arabic reads LTR, the one between Arabic and digits
/// reads RTL). Arabic-Indic digits stay level 1 (documented limit — AN
/// handling is future work); paragraph base direction is LTR (v1 bound).
fn cluster_level(shaped: &ShapedRun, text: &str, cluster_idx: usize) -> u8 {
    let Some(cluster) = shaped.clusters.get(cluster_idx) else {
        return 0;
    };
    let rtl = shaped
        .runs
        .iter()
        .find(|r| cluster.byte_range.0 >= r.byte_range.0 && cluster.byte_range.0 < r.byte_range.1)
        .map(|r| r.rtl)
        .unwrap_or(false);
    if !rtl {
        return 0;
    }
    let is_ascii_digit = text
        .get(cluster.byte_range.0..cluster.byte_range.1)
        .map(|s| s.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or(false);
    if is_ascii_digit {
        2
    } else {
        1
    }
}

/// One cluster placed in visual order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrderedCluster {
    /// Index into the [`ShapedRun`]'s cluster list (logical identity).
    pub index: usize,
    /// Visual x relative to the line origin (device px, subpixel).
    pub x: f32,
    pub width: f32,
    /// Segment direction (odd embedding level). Digit islands in RTL runs
    /// read false (they lay out LTR); caret edges follow this, not the
    /// run flag.
    pub rtl: bool,
}

/// UBA-lite visual ordering for one line's clusters (given as logical
/// indices): standard level-based reversal (highest level first, maximal
/// `level >= l` sequences reversed), then mirroring inside odd-level
/// segments. LTR text is untouched; RTL sequences mirror; digit islands
/// move as LTR units. Output is left-to-right with visual xs.
pub fn order_visual(shaped: &ShapedRun, text: &str, logical: &[usize]) -> Vec<OrderedCluster> {
    let widths = logical_widths(shaped);
    // Segments: consecutive clusters sharing an embedding level.
    let mut segments: Vec<(u8, Vec<usize>)> = Vec::new();
    for &ci in logical {
        let level = cluster_level(shaped, text, ci);
        match segments.last_mut() {
            Some((l, v)) if *l == level => v.push(ci),
            _ => segments.push((level, vec![ci])),
        }
    }
    let max_level = segments.iter().map(|(l, _)| *l).max().unwrap_or(0);
    for l in (1..=max_level).rev() {
        let mut i = 0;
        while i < segments.len() {
            if segments[i].0 < l {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < segments.len() && segments[j].0 >= l {
                j += 1;
            }
            segments[i..j].reverse();
            i = j;
        }
    }
    // Flatten: odd-level segments mirror their clusters.
    let mut visual: Vec<OrderedCluster> = Vec::with_capacity(logical.len());
    for (level, members) in &segments {
        let rtl = level % 2 == 1;
        if rtl {
            for &ci in members.iter().rev() {
                visual.push(OrderedCluster {
                    index: ci,
                    x: 0.0,
                    width: widths[ci],
                    rtl,
                });
            }
        } else {
            for &ci in members {
                visual.push(OrderedCluster {
                    index: ci,
                    x: 0.0,
                    width: widths[ci],
                    rtl,
                });
            }
        }
    }
    let mut x = 0.0f32;
    for o in &mut visual {
        o.x = x;
        x += o.width;
    }
    visual
}

/// Paragraph split: logical cluster indices cut at hard breaks.
/// Two cluster styles are accepted (decision 196): clusters COVERING
/// `'\n'` (backend-shaped newlines, e.g. fakes) are dropped and cut;
/// `'\n'` bytes in UNCOVERED gaps (stitched per-paragraph shaping --
/// real backends map no font for `'\n`, so the engine shapes each
/// `'\n`-paragraph whole and stitches) cut one paragraph per newline.
/// Clusters walk in byte order (sorted -- a no-op for logical-order
/// runs), so both styles compose.
fn split_paragraphs(shaped: &ShapedRun, text: &str) -> Vec<Vec<usize>> {
    let bytes = text.as_bytes();
    let mut order: Vec<usize> = (0..shaped.clusters.len()).collect();
    order.sort_by_key(|&ci| shaped.clusters[ci].byte_range.0);
    let mut paragraphs: Vec<Vec<usize>> = vec![Vec::new()];
    let mut cursor = 0usize;
    let cut_gap = |gap: &[u8], paragraphs: &mut Vec<Vec<usize>>| {
        for _ in gap.iter().filter(|&&b| b == b'\n') {
            paragraphs.push(Vec::new());
        }
    };
    for ci in order {
        let (s, e) = shaped.clusters[ci].byte_range;
        if s > cursor {
            if let Some(gap) = bytes.get(cursor..s.min(text.len())) {
                cut_gap(gap, &mut paragraphs);
            }
            cursor = cursor.max(s);
        }
        if bytes.get(s..e).is_some_and(|b| b.contains(&b'\n')) {
            paragraphs.push(Vec::new());
            cursor = cursor.max(e);
        } else {
            paragraphs.last_mut().expect("paragraph").push(ci);
            cursor = cursor.max(e);
        }
    }
    if cursor < text.len() {
        if let Some(gap) = bytes.get(cursor..) {
            cut_gap(gap, &mut paragraphs);
        }
    }
    paragraphs
}

/// Lay out shaped text into lines: `\n` hard breaks, greedy width-driven
/// wrap at cluster boundaries over cached advances (no re-shape), optional
/// single-line ellipsis truncation. Pure over the [`ShapedRun`].
///
/// This is the legacy path (no break source installed): it splits words
/// anywhere. Install a [`BreakSource`] on the engine to wrap at
/// opportunities only -- see [`layout_text_with_breaks`].
pub fn layout_text(
    shaped: &ShapedRun,
    text: &str,
    avail_w: f32,
    ellipsis_adv: Option<f32>,
    ascent: f32,
    descent: f32,
    line_gap: f32,
) -> Vec<LaidLine> {
    // Round 1.1 (decision 252): the wrap width is a constraint.
    // NaN previously read as infinite (silent single line);
    // negative wrapped every cluster alone (silent shredding).
    check_constrain_width("avail_w", avail_w);
    if shaped.clusters.is_empty() {
        return Vec::new();
    }
    let widths = logical_widths(shaped);
    let paragraphs = split_paragraphs(shaped, text);
    let finite = avail_w.is_finite();
    let mut line_of_clusters: Vec<Vec<usize>> = Vec::new();
    if let Some(e_adv) = ellipsis_adv {
        // Optional-v1 ellipsis: single line, truncate the first paragraph's
        // logical tail to `avail - ellipsis`, append the marker (decision 73).
        if finite {
            let budget = (avail_w - e_adv).max(0.0);
            let mut kept: Vec<usize> = Vec::new();
            let mut used = 0.0f32;
            for &ci in &paragraphs[0] {
                if used + widths[ci] > budget && !kept.is_empty() {
                    break;
                }
                kept.push(ci);
                used += widths[ci];
                if used >= budget {
                    break;
                }
            }
            line_of_clusters.push(kept);
        } else if !paragraphs.is_empty() {
            line_of_clusters.push(paragraphs[0].clone());
        }
    } else {
        for para in &paragraphs {
            if !finite {
                line_of_clusters.push(para.clone());
                continue;
            }
            // Greedy wrap: break before the first cluster that overflows a
            // non-empty line; an over-wide single cluster stands alone.
            let mut line: Vec<usize> = Vec::new();
            let mut used = 0.0f32;
            for &ci in para {
                if !line.is_empty() && used + widths[ci] > avail_w {
                    line_of_clusters.push(std::mem::take(&mut line));
                    used = 0.0;
                }
                line.push(ci);
                used += widths[ci];
            }
            line_of_clusters.push(line);
        }
    }
    emit_lines(
        shaped,
        text,
        &line_of_clusters,
        ellipsis_adv,
        finite,
        ascent,
        descent,
        line_gap,
    )
}

/// One visual line per logical cluster list: UBA-lite visual ordering,
/// run/cluster emission, optional ellipsis marker. Shared by the greedy
/// wrap path and the opportunity wrap path, so downstream geometry is
/// identical for identical line contents.
#[allow(clippy::too_many_arguments)]
fn emit_lines(
    shaped: &ShapedRun,
    text: &str,
    line_of_clusters: &[Vec<usize>],
    ellipsis_adv: Option<f32>,
    finite: bool,
    ascent: f32,
    descent: f32,
    line_gap: f32,
) -> Vec<LaidLine> {
    let line_h = ascent + descent;
    let mut lines: Vec<LaidLine> = Vec::new();
    let mut y = 0.0f32;
    for (li, logical) in line_of_clusters.iter().enumerate() {
        let ordered = order_visual(shaped, text, logical);
        let width: f32 = ordered.iter().map(|o| o.width).sum();
        // Runs: group the line's visual clusters by TextRun (visual order).
        let mut runs: Vec<LaidRun> = Vec::new();
        let mut clusters: Vec<LaidCluster> = Vec::new();
        // Marker clusters reference the text end; map them to no run.
        let ellipsis_here = ellipsis_adv.is_some() && li == 0;
        for o in &ordered {
            let c = &shaped.clusters[o.index];
            clusters.push(LaidCluster {
                byte_range: c.byte_range,
                x: o.x,
                width: o.width,
                rtl: o.rtl,
                ellipsis: false,
            });
            let mut pen = o.x;
            let glyphs_in: &[crate::text::ShapedGlyph] =
                &shaped.glyphs[c.glyph_range.0.min(shaped.glyphs.len())
                    ..c.glyph_range.1.min(shaped.glyphs.len())];
            let laid: Vec<LaidGlyph> = glyphs_in
                .iter()
                .map(|g| {
                    let laid = LaidGlyph {
                        glyph_id: g.glyph_id,
                        x: pen,
                        x_advance: g.x_advance,
                    };
                    pen += g.x_advance;
                    laid
                })
                .collect();
            // One LaidRun per cluster keeps run/cluster alignment exact for
            // v1 scripts (no cross-cluster run merging to misattribute).
            // Font identity comes from the source TextRun covering the
            // cluster's first byte (M7, decision 110); the resolvable
            // family name is filled post-pass by the layout caller.
            runs.push(LaidRun {
                byte_range: c.byte_range,
                rtl: o.rtl,
                glyphs: laid,
                font_id: run_font_of(shaped, c.byte_range.0),
                family: String::new(),
            });
        }
        if ellipsis_here && finite {
            clusters.push(LaidCluster {
                byte_range: (shaped.text_len_bytes, shaped.text_len_bytes),
                x: width,
                width: ellipsis_adv.unwrap_or(0.0),
                rtl: false,
                ellipsis: true,
            });
            runs.push(LaidRun {
                byte_range: (shaped.text_len_bytes, shaped.text_len_bytes),
                rtl: false,
                glyphs: Vec::new(),
                font_id: FontId(0),
                family: String::new(),
            });
        }
        let full_width = if ellipsis_here && finite {
            width + ellipsis_adv.unwrap_or(0.0)
        } else {
            width
        };
        lines.push(LaidLine {
            y,
            height: line_h,
            baseline: ascent,
            // Post-pass context (see `layout_text_leaf`): the pure
            // order/wrap step owns no measure context, so em size arrives
            // from the caller, not from here.
            em_size: 0.0,
            width: full_width,
            runs,
            clusters,
        });
        y += line_h + line_gap;
    }
    // Drop the trailing gap: box height is last line's bottom.
    let _ = line_gap;
    lines
}

/// Lay out shaped text into lines at break opportunities only (v2 item
/// 2, decisions 194-195): `\n` hard breaks plus opportunity-driven soft
/// wrapping over cached advances (no re-shape — the M3 rule stays).
/// Pure over the [`ShapedRun`] plus `breaks` (break-after byte offsets
/// from a [`BreakSource`]), consumed the way the run itself is.
///
/// Rules (decision 194), all pinned in the paragraph corpus:
///
/// - Breaks happen only at offsets that end a cluster — stray offsets
///   are inert, so a cluster is never split (locked shaping rule).
/// - Between consecutive opportunities, clusters form an atomic span:
///   a span that overflows a non-empty line starts the next line
///   whole; an over-wide span on an empty line pushes whole (it may
///   overflow) — a single over-wide cluster stands alone, the greedy
///   rule preserved.
/// - Trailing ASCII blanks (space/tab) at a soft break are trimmed —
///   they ride neither line. Paragraph-final blanks are kept (hard
///   breaks don't trim).
/// - Wrap-point forward affinity (decision 195): the break byte
///   belongs to the next line's leading caret (see
///   [`LayoutBox::caret_position`] and [`LaidLine::caret_x`]).
///
/// Optional-v1 ellipsis keeps the greedy single-line truncation
/// (opportunities govern wrapping, never truncation).
#[allow(clippy::too_many_arguments)]
pub fn layout_text_with_breaks(
    shaped: &ShapedRun,
    text: &str,
    avail_w: f32,
    ellipsis_adv: Option<f32>,
    ascent: f32,
    descent: f32,
    line_gap: f32,
    breaks: &[usize],
) -> Vec<LaidLine> {
    // Round 1.1 (decision 252): same constraint rule as the greedy
    // path — NaN/negative never wraps silently.
    check_constrain_width("avail_w", avail_w);
    if shaped.clusters.is_empty() {
        return Vec::new();
    }
    if ellipsis_adv.is_some() {
        return layout_text(
            shaped,
            text,
            avail_w,
            ellipsis_adv,
            ascent,
            descent,
            line_gap,
        );
    }
    let widths = logical_widths(shaped);
    let paragraphs = split_paragraphs(shaped, text);
    if !avail_w.is_finite() {
        let mut all: Vec<Vec<usize>> = Vec::with_capacity(paragraphs.len());
        for para in &paragraphs {
            all.push(para.clone());
        }
        return emit_lines(shaped, text, &all, None, true, ascent, descent, line_gap);
    }
    let allowed: HashSet<usize> = breaks.iter().copied().collect();
    let mut line_of_clusters: Vec<Vec<usize>> = Vec::new();
    for para in &paragraphs {
        wrap_paragraph(
            shaped,
            text,
            para,
            &widths,
            avail_w,
            &allowed,
            &mut line_of_clusters,
        );
    }
    emit_lines(
        shaped,
        text,
        &line_of_clusters,
        None,
        true,
        ascent,
        descent,
        line_gap,
    )
}

/// Greedy span packing for one paragraph: spans (clusters between
/// consecutive opportunities) are atomic; lines fill until the next
/// span overflows. Appends committed lines (soft breaks trimmed,
/// paragraph end untrimmed) to `out`.
fn wrap_paragraph(
    shaped: &ShapedRun,
    text: &str,
    para: &[usize],
    widths: &[f32],
    avail_w: f32,
    allowed: &HashSet<usize>,
    out: &mut Vec<Vec<usize>>,
) {
    if para.is_empty() {
        // Empty paragraph (adjacent `\n`s): one empty line, same as greedy.
        out.push(Vec::new());
        return;
    }
    // Span segmentation: cut after every cluster whose end is an
    // opportunity. Stray offsets cut nothing (never inside a cluster).
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    for (k, &ci) in para.iter().enumerate() {
        if allowed.contains(&shaped.clusters[ci].byte_range.1) {
            spans.push((start, k + 1));
            start = k + 1;
        }
    }
    if start < para.len() {
        spans.push((start, para.len()));
    }
    let mut line: Vec<usize> = Vec::new();
    let mut used = 0.0f32;
    for (a, b) in spans {
        let w: f32 = para[a..b].iter().map(|&ci| widths[ci]).sum();
        if !line.is_empty() && used + w > avail_w {
            out.push(trim_trailing_blanks(
                shaped,
                text,
                std::mem::take(&mut line),
            ));
            used = 0.0;
        }
        line.extend(para[a..b].iter().copied());
        used += w;
    }
    out.push(line);
}

/// Strips trailing ASCII-blank clusters (space/tab — the characters
/// UAX breaks after) from a soft-broken line. Whole clusters only,
/// never into a cluster; a non-blank tail (hyphens, CJK, NBSP) stays.
fn trim_trailing_blanks(shaped: &ShapedRun, text: &str, mut line: Vec<usize>) -> Vec<usize> {
    while let Some(&ci) = line.last() {
        let (s, e) = shaped.clusters[ci].byte_range;
        let blank = text
            .get(s..e)
            .is_some_and(|t| !t.is_empty() && t.chars().all(|c| c == ' ' || c == '\t'));
        if !blank {
            break;
        }
        line.pop();
    }
    line
}

// ---------------------------------------------------------------------------
// Portable unit tests (synthetic runs; no backend)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{Cluster, FontId, FontMetrics, ShapedGlyph, TextRun};

    pub(crate) fn synthetic_run(pieces: &[(u32, f32, bool)]) -> ShapedRun {
        // pieces: (byte_len, advance, rtl) — one cluster + one glyph each,
        // runs merged across consecutive same-direction pieces.
        let mut glyphs = Vec::new();
        let mut clusters = Vec::new();
        let mut runs = Vec::new();
        let mut byte = 0usize;
        let metrics = FontMetrics {
            ascent: 12.0,
            descent: 4.0,
            line_gap: 2.0,
        };
        let mut i = 0;
        while i < pieces.len() {
            let rtl = pieces[i].2;
            let run_start_byte = byte;
            let run_start_glyph = glyphs.len();
            while i < pieces.len() && pieces[i].2 == rtl {
                let (len, adv, _) = pieces[i];
                glyphs.push(ShapedGlyph {
                    glyph_id: i as u32,
                    x_advance: adv,
                    x_offset: 0.0,
                    y_offset: 0.0,
                });
                clusters.push(Cluster {
                    byte_range: (byte, byte + len as usize),
                    glyph_range: (glyphs.len() - 1, glyphs.len()),
                });
                byte += len as usize;
                i += 1;
            }
            runs.push(TextRun {
                byte_range: (run_start_byte, byte),
                glyph_range: (run_start_glyph, glyphs.len()),
                rtl,
                script: 0,
                font_id: FontId(0),
                font_metrics: metrics,
            });
        }
        let total_advance = glyphs.iter().map(|g| g.x_advance).sum();
        ShapedRun {
            glyphs,
            runs,
            clusters,
            total_advance,
            text_len_bytes: byte,
        }
    }

    #[test]
    fn ltr_text_orders_unchanged() {
        let run = synthetic_run(&[(1, 10.0, false), (1, 10.0, false), (1, 10.0, false)]);
        let ordered = order_visual(&run, "abc", &[0, 1, 2]);
        let xs: Vec<f32> = ordered.iter().map(|o| o.x).collect();
        assert_eq!(xs, vec![0.0, 10.0, 20.0]);
        assert!(ordered.iter().all(|o| !o.rtl));
    }

    #[test]
    fn rtl_sequence_reverses_with_mirrored_positions() {
        // "abc مرحبا 123"-shaped: LTR(3) + RTL(2) + LTR(3), 10px each.
        let run = synthetic_run(&[
            (1, 10.0, false),
            (1, 10.0, false),
            (1, 10.0, false),
            (2, 10.0, true),
            (2, 10.0, true),
            (1, 10.0, false),
            (1, 10.0, false),
            (1, 10.0, false),
        ]);
        let logical: Vec<usize> = (0..8).collect();
        let ordered = order_visual(&run, "abc\u{645}\u{631}def", &logical);
        // Visual cluster index order: LTR head, mirrored RTL, LTR tail.
        let idx: Vec<usize> = ordered.iter().map(|o| o.index).collect();
        assert_eq!(idx, vec![0, 1, 2, 4, 3, 5, 6, 7]);
        // Total width preserved (reorder, not re-measure).
        let total: f32 = ordered.iter().map(|o| o.width).sum();
        assert_eq!(total, 80.0);
        // RTL segment occupies [30, 50); first logical RTL cluster (3) sits
        // rightmost at x=40 (mirror), second (4) at x=30.
        assert_eq!(ordered[3].x, 30.0);
        assert_eq!(ordered[3].index, 4);
        assert_eq!(ordered[4].x, 40.0);
        assert_eq!(ordered[4].index, 3);
        // RTL flags ride along.
        assert!(ordered[3].rtl && ordered[4].rtl);
    }

    #[test]
    fn visual_caret_follows_logical_prefix_end() {
        let run = synthetic_run(&[
            (1, 10.0, false),
            (1, 10.0, false),
            (1, 10.0, false),
            (2, 10.0, true),
            (2, 10.0, true),
            (1, 10.0, false),
            (1, 10.0, false),
            (1, 10.0, false),
        ]);
        let lines = layout_text(
            &run,
            "abc\u{645}\u{631}def",
            f32::INFINITY,
            None,
            12.0,
            4.0,
            2.0,
        );
        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        // Byte 0: paragraph-direction edge.
        assert_eq!(line.caret_x(0), 0.0);
        // LTR→RTL boundary (byte 3): forward affinity — the next (RTL)
        // character begins at its right edge, x=50.
        assert_eq!(line.caret_x(3), 50.0);
        // Mid-Arabic boundary (byte 5, start of second RTL cluster):
        // forward affinity → right edge of its slot, x=40.
        assert_eq!(line.caret_x(5), 40.0);
        // RTL→LTR boundary (byte 7): the next (LTR) character begins at
        // its left edge, x=50.
        assert_eq!(line.caret_x(7), 50.0);
        // Trailing caret: the line width.
        assert_eq!(line.caret_x(10), 80.0);
    }

    #[test]
    fn source_order_caret_diverges_where_visual_collapses() {
        // The M3 bidi gate in miniature: source-order caret math
        // (ShapedRun::caret_x, logical leading edges) vs visual carets.
        let run = synthetic_run(&[
            (1, 10.0, false),
            (1, 10.0, false),
            (1, 10.0, false),
            (2, 10.0, true),
            (2, 10.0, true),
            (1, 10.0, false),
            (1, 10.0, false),
            (1, 10.0, false),
        ]);
        // Source-order caret at the LTR→RTL boundary (byte 3): logical
        // leading edge of the first RTL cluster = 30.0.
        assert_eq!(run.caret_x(3), 30.0);
        let lines = layout_text(
            &run,
            "abc\u{645}\u{631}def",
            f32::INFINITY,
            None,
            12.0,
            4.0,
            2.0,
        );
        // Visual caret at the same byte (forward affinity): 50.0 — the
        // flip magnitude at this boundary is 20px here (10px clusters;
        // ~65px in the real corpus with real advances).
        assert_eq!(lines[0].caret_x(3), 50.0);
    }

    #[test]
    fn digit_island_in_rtl_run_stays_ltr_and_moves_as_unit() {
        // "ab" + U+0645 + "12": LTR pair, one RTL letter, two EN digits in
        // the same RTL run (the corpus tail in miniature).
        let run = synthetic_run(&[
            (1, 10.0, false),
            (1, 10.0, false),
            (2, 10.0, true),
            (1, 10.0, true),
            (1, 10.0, true),
        ]);
        let text = "ab\u{645}12";
        assert_eq!(text.len(), 6);
        let logical: Vec<usize> = (0..5).collect();
        let ordered = order_visual(&run, text, &logical);
        // Levels [0,0,1,2,2]: the island moves left of the RTL letter.
        let idx: Vec<usize> = ordered.iter().map(|o| o.index).collect();
        assert_eq!(idx, vec![0, 1, 3, 4, 2]);
        // Island clusters read LTR (caret at a digit start = left edge).
        assert!(!ordered[2].rtl && !ordered[3].rtl);
        assert!(ordered[4].rtl);
        let lines = layout_text(&run, text, f32::INFINITY, None, 12.0, 4.0, 2.0);
        assert_eq!(lines[0].caret_x(4), 20.0);
        // The RTL letter's slot is rightmost: forward caret at its start.
        assert_eq!(lines[0].caret_x(2), 50.0);
    }

    #[test]
    fn wrap_breaks_at_cluster_boundaries_without_reshape() {
        // 8 clusters × 10px; avail 25 → lines of 2 (greedy: 20 used, third
        // would make 30 > 25).
        let run = synthetic_run(&[(1, 10.0, false); 8]);
        let lines = layout_text(&run, "abcdefgh", 25.0, None, 12.0, 4.0, 2.0);
        assert_eq!(lines.len(), 4);
        for line in &lines {
            assert_eq!(line.width, 20.0);
            assert_eq!(line.clusters.len(), 2);
        }
        // Line stacking: height 16 each + 2 gap.
        assert_eq!(lines[1].y, 18.0);
        // Over-wide single cluster stands alone (overflow, never split).
        let wide = synthetic_run(&[(1, 100.0, false), (1, 10.0, false)]);
        let lines = layout_text(&wide, "ab", 25.0, None, 12.0, 4.0, 2.0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].width, 100.0);
    }

    #[test]
    fn newline_splits_lines() {
        let run = synthetic_run(&[(1, 10.0, false), (1, 0.0, false), (1, 10.0, false)]);
        let lines = layout_text(&run, "a\nb", f32::INFINITY, None, 12.0, 4.0, 2.0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].clusters.len(), 1);
        assert_eq!(lines[1].clusters.len(), 1);
    }

    #[test]
    fn ellipsis_truncates_single_line_with_marker() {
        let run = synthetic_run(&[(1, 10.0, false); 8]);
        let lines = layout_text(&run, "abcdefgh", 25.0, Some(10.0), 12.0, 4.0, 2.0);
        assert_eq!(lines.len(), 1, "ellipsis forces single-line");
        // Budget 25-10=15 → one 10px cluster + marker.
        assert_eq!(lines[0].clusters.iter().filter(|c| !c.ellipsis).count(), 1);
        let marker = lines[0]
            .clusters
            .iter()
            .find(|c| c.ellipsis)
            .expect("marker");
        assert_eq!(marker.width, 10.0);
        assert_eq!(marker.byte_range, (8, 8));
        assert_eq!(lines[0].width, 20.0);
    }

    #[test]
    fn empty_run_lays_no_lines() {
        let run = synthetic_run(&[]);
        let lines = layout_text(&run, "", f32::INFINITY, None, 12.0, 4.0, 2.0);
        assert!(lines.is_empty());
    }

    fn wrap8(breaks: &[usize], avail: f32) -> Vec<LaidLine> {
        // "aa bb cc": 8 one-byte clusters × 10px, spaces at bytes 2, 5.
        let run = synthetic_run(&[(1, 10.0, false); 8]);
        layout_text_with_breaks(&run, "aa bb cc", avail, None, 12.0, 4.0, 2.0, breaks)
    }

    #[test]
    fn breaks_wrap_at_opportunities_only_with_trim() {
        // Opportunities after each space (3, 6); avail 45 fits one
        // "word " span (30) but never two (60).
        let lines = wrap8(&[3, 6], 45.0);
        assert_eq!(lines.len(), 3);
        let starts: Vec<Vec<usize>> = lines
            .iter()
            .map(|l| l.clusters.iter().map(|c| c.byte_range.0).collect())
            .collect();
        assert_eq!(starts, vec![vec![0, 1], vec![3, 4], vec![6, 7]]);
        // Trailing spaces trimmed at both soft breaks: width 20, not 30.
        assert_eq!(lines[0].width, 20.0);
        assert_eq!(lines[1].width, 20.0);
        // Paragraph-final line untrimmed (no trailing blank here anyway).
        assert_eq!(lines[2].width, 20.0);
        assert_eq!(lines[1].y, 18.0);
    }

    #[test]
    fn forward_affinity_break_byte_reads_next_leading() {
        let lines = wrap8(&[3, 6], 45.0);
        let b = LayoutBox {
            lines,
            ..Default::default()
        };
        // Byte 2 (trimmed space) belongs to line 1's leading caret.
        assert_eq!(b.caret_position(2), (1, 0.0));
        // Byte 3 (line 1's first byte) stays on line 1.
        assert_eq!(b.caret_position(3), (1, 0.0));
        // Byte 5 (second trimmed space) belongs to line 2's leading caret.
        assert_eq!(b.caret_position(5), (2, 0.0));
        // Trailing caret: last line's end.
        assert_eq!(b.caret_position(8), (2, 20.0));
    }

    #[test]
    fn overwide_span_pushes_whole_and_single_cluster_stands_alone() {
        // No opportunities: the whole paragraph is one atomic span --
        // over-wide it pushes whole onto a single overflowing line.
        let run = synthetic_run(&[(1, 10.0, false); 8]);
        let lines = layout_text_with_breaks(&run, "abcdefgh", 25.0, None, 12.0, 4.0, 2.0, &[]);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].width, 80.0);
        // One opportunity mid-word: spans "a" (100px, over-wide) + "b".
        let wide = synthetic_run(&[(1, 100.0, false), (1, 10.0, false)]);
        let lines = layout_text_with_breaks(&wide, "ab", 25.0, None, 12.0, 4.0, 2.0, &[1]);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].width, 100.0);
        assert_eq!(lines[1].width, 10.0);
    }

    #[test]
    fn stray_offsets_never_split_clusters() {
        // Clusters (0,1),(1,3),(3,4): offset 2 is mid-cluster — inert.
        let run = synthetic_run(&[(1, 10.0, false), (2, 10.0, false), (1, 10.0, false)]);
        let text = "a\u{e9}b";
        let lines = layout_text_with_breaks(&run, text, 15.0, None, 12.0, 4.0, 2.0, &[2]);
        assert_eq!(lines.len(), 1, "over-wide span pushes whole, never splits");
        assert_eq!(lines[0].clusters.len(), 3);
    }

    #[test]
    fn infinite_width_ignores_breaks() {
        let lines = wrap8(&[3, 6], f32::INFINITY);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].clusters.len(), 8);
    }

    #[test]
    fn ellipsis_keeps_greedy_truncation() {
        // Opportunities would wrap, but ellipsis truncates single-line.
        let run = synthetic_run(&[(1, 10.0, false); 8]);
        let lines =
            layout_text_with_breaks(&run, "abcdefgh", 25.0, Some(10.0), 12.0, 4.0, 2.0, &[4]);
        assert_eq!(lines.len(), 1, "ellipsis forces single-line");
        assert_eq!(lines[0].clusters.iter().filter(|c| !c.ellipsis).count(), 1);
    }

    #[test]
    fn gap_newlines_split_like_cluster_newlines() {
        // Stitched per-paragraph shaping (decision 196): no cluster
        // covers the '\n' -- the gap does. "a\nb" as stitched runs.
        let run = synthetic_run(&[(1, 10.0, false), (1, 10.0, false)]);
        let mut stitched = run;
        stitched.clusters[1].byte_range = (2, 3);
        stitched.text_len_bytes = 3;
        let lines = layout_text(&stitched, "a\nb", f32::INFINITY, None, 12.0, 4.0, 2.0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].clusters[0].byte_range, (0, 1));
        assert_eq!(lines[1].clusters[0].byte_range, (2, 3));
    }

    #[test]
    fn adjacent_newlines_yield_empty_middle_line() {
        // "a\n\nb": the gap holds two newlines → three paragraphs.
        let run = synthetic_run(&[(1, 10.0, false), (1, 10.0, false)]);
        let mut stitched = run;
        stitched.clusters[1].byte_range = (3, 4);
        stitched.text_len_bytes = 4;
        let lines = layout_text(&stitched, "a\n\nb", f32::INFINITY, None, 12.0, 4.0, 2.0);
        assert_eq!(lines.len(), 3);
        assert!(lines[1].clusters.is_empty());
    }

    /// Round 17.2 (decision 318): the brief formula pins exactly —
    /// `h = max(24, viewport² / content)`, `y` linear in the
    /// clamped offset over the travel.
    #[test]
    fn scrollbar_thumb_pins_the_brief_formula() {
        // 200px viewport over 600px content at rest.
        let t = scrollbar_thumb(200.0, 600.0, 0.0).expect("overflow thumbs");
        assert!(
            (t.h - 200.0 * 200.0 / 600.0).abs() < 1e-3,
            "h pins vp²/c, got {}",
            t.h
        );
        assert_eq!(t.y, 0.0, "rest parks at top");
        assert_eq!(scrollbar_max_offset(200.0, 600.0), 400.0);
        // Full travel: bottom parks the thumb at travel exactly.
        let full = scrollbar_thumb(200.0, 600.0, 400.0).expect("bottom thumb");
        assert!(
            (full.y - (200.0 - t.h)).abs() < 1e-3,
            "bottom parks at travel, got {}",
            full.y
        );
        // Halfway parks halfway.
        let mid = scrollbar_thumb(200.0, 600.0, 200.0).expect("mid thumb");
        assert!(
            (mid.y - (200.0 - t.h) / 2.0).abs() < 1e-3,
            "linear ratio, got {}",
            mid.y
        );
        // Min clamp: 200px over 2000px wants 20px, gets 24.
        let small = scrollbar_thumb(200.0, 2000.0, 0.0).expect("min thumb");
        assert_eq!(small.h, SCROLLBAR_MIN_THUMB_PX);
        // No overflow, degenerate, and clamped overscroll.
        assert_eq!(scrollbar_thumb(200.0, 200.0, 0.0), None);
        assert_eq!(scrollbar_thumb(200.0, 100.0, 0.0), None);
        assert_eq!(scrollbar_thumb(0.0, 600.0, 0.0), None);
        assert_eq!(
            scrollbar_thumb(200.0, 600.0, 9999.0),
            scrollbar_thumb(200.0, 600.0, 400.0),
            "overscroll clamps to max"
        );
        assert_eq!(
            scrollbar_thumb(200.0, 600.0, -50.0),
            scrollbar_thumb(200.0, 600.0, 0.0),
            "negative clamps to top"
        );
    }

    /// Phase 36 PR2b (decision 354): the horizontal twin pins the same
    /// formula transposed — `w = max(24, viewport² / content)`, `x`
    /// linear in the clamped offset over the travel.
    #[test]
    fn scrollbar_thumb_x_pins_the_transposed_formula() {
        let t = scrollbar_thumb_x(200.0, 600.0, 0.0).expect("overflow thumbs");
        assert!(
            (t.w - 200.0 * 200.0 / 600.0).abs() < 1e-3,
            "w pins vp²/c, got {}",
            t.w
        );
        assert_eq!(t.x, 0.0, "rest parks at left");
        let full = scrollbar_thumb_x(200.0, 600.0, 400.0).expect("right thumb");
        assert!(
            (full.x - (200.0 - t.w)).abs() < 1e-3,
            "right parks at travel, got {}",
            full.x
        );
        let mid = scrollbar_thumb_x(200.0, 600.0, 200.0).expect("mid thumb");
        assert!(
            (mid.x - (200.0 - t.w) / 2.0).abs() < 1e-3,
            "linear ratio, got {}",
            mid.x
        );
        assert_eq!(scrollbar_thumb_x(200.0, 200.0, 0.0), None);
        assert_eq!(scrollbar_thumb_x(200.0, 100.0, 0.0), None);
        assert_eq!(scrollbar_thumb_x(0.0, 600.0, 0.0), None);
    }
}
