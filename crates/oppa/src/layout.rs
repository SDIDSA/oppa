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
use crate::style::{AlignItems, Color, FlexWrap, GridTrack, JustifyContent, Style};
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
    /// Span paint ink (Phase 36 PR3, decision 355): `Some` exactly on
    /// runs of a multi-span paragraph (resolved from the span table
    /// post-pass — `None` on single-style text, and builders split
    /// `DrawOp::Text` at ink boundaries so single-ink scenes keep
    /// byte-identical op counts).
    pub ink: Option<Color>,
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

/// Rich cache key (Phase 36 PR3, decision 355): the joined bytes plus
/// every span's weight and ink — a span re-style re-shapes exactly
/// like a byte change (never a stale-span hit). Inks ride the key
/// (they do not move layout, but the key owns cache identity — a
/// cheaper miss than a second table).
fn measure_key_rich(
    joined: &str,
    spans: &[crate::vnode::TextSpan],
    size_px: f32,
    cfg: &LayoutTextConfig,
) -> TextMeasureKey {
    let mut buf: Vec<u8> = Vec::with_capacity(joined.len() + spans.len() * 8);
    buf.extend_from_slice(joined.as_bytes());
    for s in spans {
        buf.extend_from_slice(&s.weight.0.to_le_bytes());
        buf.extend_from_slice(&s.ink.map(|c| c.0).unwrap_or(u32::MAX).to_le_bytes());
        buf.push(0xFF);
    }
    TextMeasureKey {
        bytes_hash: fnv1a64(&buf),
        size_bits: size_px.to_bits(),
        dpr_bits: cfg.device_pixel_ratio.to_bits(),
        family_hash: fnv1a64(cfg.family.as_bytes()),
        weight: spans.first().map(|s| s.weight.0).unwrap_or(400),
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
                    let key = match &n.rich_text {
                        Some(spans) => measure_key_rich(text, spans, size, cfg),
                        None => measure_key(text, size, weight, cfg),
                    };
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
pub(crate) fn check_constrain_width(name: &str, v: f32) {
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
            // Retained canvas (Phase 36 PR4, decision 358): opaque
            // leaf — explicit size or zero, children refused, text
            // ops shaped into the box lines.
            Tag::Canvas => {
                self.layout_canvas(id, &children, ox, oy, explicit_w, explicit_h, given_w)
            }
            Tag::Text => unreachable!("text nodes handled above"),
        }
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
        // Rich path (Phase 36 PR3): nodes carrying a span table shape
        // each non-empty span with its own weight and join (never
        // re-shape); single-style nodes keep the cached `measure`
        // path exactly.
        let spans = self.rec.get(id).and_then(|n| n.rich_text.clone());
        let measured = match &spans {
            Some(spans) => self.measure_rich(id, measure_text, spans, size_px),
            None => self.measure(id, measure_text, size_px, weight),
        };
        let (shaped, ascent, descent, line_gap) = match measured {
            Some(m) => (m.shaped.clone(), m.ascent, m.descent, m.line_gap),
            None => {
                self.commit_box(id, ox, oy, 0.0, 0.0, 0.0, 0.0, Vec::new());
                return Size { w: 0.0, h: 0.0 };
            }
        };
        // Span table for the ink post-pass (`None` on single-style
        // text — every run keeps `ink: None`, builders keep
        // byte-identical op counts).
        let span_table: Option<(Vec<usize>, Vec<Option<Color>>)> = spans.map(|spans| {
            let mut ends = Vec::with_capacity(spans.len());
            let mut inks = Vec::with_capacity(spans.len());
            let mut acc = 0usize;
            for s in &spans {
                acc += s.text.len();
                ends.push(acc);
                inks.push(s.ink);
            }
            (ends, inks)
        });
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
        // Phase 36 PR3 adds span ink the same way (leading affinity
        // at boundaries — the `span_index_for_byte` rule).
        let em_size = size_px * self.dpr;
        let requested = self.engine.config.family.clone();
        for line in &mut lines {
            line.em_size = em_size;
            for run in &mut line.runs {
                if run.family.is_empty() {
                    run.family = self.engine.family_of(run.font_id, &requested);
                }
                if run.ink.is_none() {
                    if let Some((ends, inks)) = &span_table {
                        if let Some(si) = crate::text::span_index_for_byte(ends, run.byte_range.0) {
                            run.ink = inks[si];
                        }
                    }
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

    /// Multi-span measurement (Phase 36 PR3, decision 355): shapes
    /// each non-empty span with its own weight and joins (the
    /// `join_shaped_runs` rule — never re-shape). The joined run is
    /// cached on the node under the rich key (span weights/inks ride
    /// it — a span re-style re-shapes). `None` when no service is
    /// installed or every span is empty (the `measure` zero rule).
    /// Per-span shapes bypass the node cache (one slot holds the
    /// joined run — per-span slots are a profiled follow-up,
    /// stated).
    fn measure_rich(
        &mut self,
        id: NodeId,
        joined: &str,
        spans: &[crate::vnode::TextSpan],
        size_px: f32,
    ) -> Option<MeasuredText> {
        let cfg = &self.engine.config;
        let key = measure_key_rich(joined, spans, size_px, cfg);
        if let Some(n) = self.rec.get(id) {
            if let Some(m) = &n.measured {
                if m.key == key {
                    return Some(m.clone());
                }
            }
        }
        let service = self.service?;
        let mut runs: Vec<ShapedRun> = Vec::with_capacity(spans.len());
        let mut shaped_count: usize = 0;
        for span in spans {
            if span.text.is_empty() {
                continue; // inert — zero bytes, never shaped
            }
            let style = TextStyle {
                family: cfg.family.clone(),
                font_size_px: size_px,
                device_pixel_ratio: cfg.device_pixel_ratio,
                weight: span.weight,
                style: crate::text::FontStyle::Normal,
                stretch: crate::text::FontStretch::NORMAL,
                letter_spacing_px: 0.0,
                locale: "en-US".to_string(),
            };
            match service.shape(&span.text, &style) {
                Ok(r) => {
                    shaped_count += 1;
                    runs.push(r);
                }
                Err(TextError::EmptyText) => {}
                Err(e) => panic!(
                    "layout: rich span measurement failed (family {:?}, {size_px}px): {e} — \
                     a shaping failure is a backend/config bug, never silent",
                    cfg.family,
                ),
            }
        }
        if runs.is_empty() {
            return None;
        }
        let shaped = crate::text::join_shaped_runs(&runs);
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

pub mod text;
pub use text::*;
pub(crate) mod containers;
pub(crate) mod flex;
pub(crate) mod flex_vertical;
pub(crate) mod flex_wrap;
pub(crate) mod grid;

#[cfg(test)]
mod tests;
