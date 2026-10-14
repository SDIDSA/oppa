// ---------------------------------------------------------------------------
// Layout primitives (config, laid text runs, boxes)
// ---------------------------------------------------------------------------

use super::*;

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

/// One layout-animated box change recorded by `commit_box` (decisions
/// 521–522): an opted-in node's old and new committed boxes with its
/// animation spec. The host relativizes (FLIP), filters (scroll
/// suppression), and synthesizes tracks from these — the layout
/// crate never interpolates, it only observes.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutMove {
    /// The moved node (retired ids never record — removal drops boxes
    /// through a different path).
    pub id: NodeId,
    /// Previously committed box (always `Some` — mounts jump, they
    /// never animate from nothing).
    pub old: LayoutBox,
    /// Newly committed box (already written when recorded).
    pub new: LayoutBox,
    /// The node's `Style.layout_anim` timing (one spec drives both
    /// the FLIP and the fence on this node).
    pub spec: crate::style::Transition,
    /// Position-only (`Flip`) or size-involving (`Fence`), decided at
    /// record time from old vs new (mixed move+resize goes to the
    /// fence whole, never split).
    pub kind: LayoutMoveKind,
}

/// Layout-move kinds (decisions 521–522): FLIP animates pure-position
/// deltas through the transform machinery; the fence interpolates
/// boxes with size changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutMoveKind {
    Flip,
    Fence,
}

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
    /// ranges yield no rects. Shared by the FramePlan builder (CPU/GPU
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
