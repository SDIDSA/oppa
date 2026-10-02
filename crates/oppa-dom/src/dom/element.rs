use super::*;

/// Element shape (the tag mapping).
#[derive(Clone, PartialEq, Debug)]
pub enum HtmlKind {
    /// `div` (Div/Row/Column/Stack + Text wrappers).
    Block,
    /// `span` (text leaves).
    Text,
    /// `<input type="text">` (verdict-(b) fields — void, value-carrying).
    Field,
    /// `<textarea>` (round 5.1 — verdict-(b) multi-line fields,
    /// content-carrying; same absorbed-children + font rules as
    /// `Field`).
    Area,
    /// Overflow container + spacer (ScrollArea).
    Scroll,
    /// `<img>` (round 4.4 — real image element, src resolved; void,
    /// sized by geometry like every other leaf).
    Image {
        /// Escaped-at-render `src` (cache key — a URL on web).
                 src: String,
        /// Escaped-at-render `alt` (semantics label, else empty).
                 alt: String,
    },
    /// Inline `<svg>` (decision 291 — the `Tag::Path` leaf; void,
    /// sized by geometry like `<img>`). `view` is the CSS-px box the
    /// `viewBox` maps 1:1 (no scaling distortion — authoring px map
    /// straight through, the DPR-1 contract the rasterizers share).
    Vector {
        /// Escaped-at-render SVG path data (the retained payload).
                 data: String,
        /// `fill` hex (`#rrggbb`) or `"none"`.
                 fill: String,
        /// `stroke` hex or `"none"`.
                 stroke: String,
        /// Stroke width in authoring px (present only when stroked).
                 stroke_width: Option<f32>,
        /// CSS-px view size (`viewBox="0 0 w h"`).
                 view: (f32, f32),
    },
    /// Inline `<svg>` with retained children (Phase 36 PR4, decision
    /// 358 — the `Tag::Canvas` leaf; void, sized by geometry like
    /// `<img>`). `inner` is the pre-rendered shape markup (rects,
    /// paths, texts in authoring px, 1:1 like `Vector`); laid canvas
    /// text does NOT double-render as spans (the node carries no text
    /// payload — `text_runs` slices empty, so spans stay empty).
    Canvas {
        /// Escaped-at-render inner SVG markup (built at derive).
                 inner: String,
        /// CSS-px view size (`viewBox="0 0 w h"`).
                 view: (f32, f32),
    },
    /// Marked hole (`div[data-external]`, Custom id carried).
    External(u64),
}

/// One text run inside a DOM element (Phase 36 PR3): the fallback
/// family, the run text, and the span ink as `#rrggbb` (`None` =
/// inherit — no `color:` declaration, the node's own ink wins).
#[derive(Clone, PartialEq, Debug)]
pub struct DomRun {
    pub family: String,
    pub ink: Option<String>,
    pub text: String,
}

/// One DOM element (presenter-side, keyed by [`NodeId`]).
#[derive(Clone, PartialEq, Debug)]
pub struct DomElement {
    pub node: NodeId,
    pub kind: HtmlKind,
    /// Shared style classes (StyleId → rule, stable).
    pub classes: Vec<String>,
    /// Per-node geometry (absolute box — per-node data, honestly inline).
    pub inline_geom: String,
    /// Per-node text style (measured family/size — per-node data).
    pub inline_font: String,
    /// Span content / input value (raw; escaped at render).
    pub text: String,
    /// Per-run segmentation (fallback runs → inner spans): family
    /// plus the run text plus the span ink (`None` = inherit — the
    /// node's own ink, the `resolve_ink` rule). Phase 36 PR3 merges
    /// on `(family, ink)` so multi-ink paragraphs render one `<span>`
    /// per ink run with an honest `color:` declaration.
    pub runs: Vec<DomRun>,
    /// ARIA + foreign attributes (deterministic order).
    pub attrs: Vec<(String, String)>,
    /// Retained child order (fields: empty — absorbed).
    pub children: Vec<NodeId>,
    /// Scroll containers: content extent, CSS px (the spacer height).
    pub spacer_h: Option<f32>,
    /// Text-selection highlight rects (Round 8.2): CSS-px boxes relative
    /// to this element's positioned origin, rendered as `sel` divs ahead
    /// of content (background order). Non-empty only on the selected
    /// field's container.
    pub sel_rects: Vec<[f32; 4]>,
    /// Caret bar rect (Round 15.1, decision 312): CSS-px box relative
    /// to this element's positioned origin, rendered as a trailing
    /// `caret` div (above the glyphs, after the `sel` divs). `Some`
    /// only on the focused field's container while the blink phase is
    /// visible — synced visibility (no CSS animation), so every
    /// presenter blinks from the host clock by construction. The bar
    /// paints in [`caret_ink`](Self::caret_ink) (snapshot together —
    /// stale geometry must never meet a live color).
    pub caret_rect: Option<[f32; 4]>,
    /// Caret bar fill as `#rrggbb` (Round 15.1): the focused field's
    /// text ink at derive time. Empty when [`caret_rect`](Self::caret_rect)
    /// is `None` (absence is state, never leftover markup).
    pub caret_ink: String,
    /// Native placeholder hint for fields (decision 293): the
    /// presentational text the control renders while the value is
    /// empty — raw, escaped at render into the `placeholder`
    /// attribute. Empty for value-carrying fields (no attribute
    /// emitted) and for every non-field kind.
    pub placeholder: String,
    /// Binding-edge suppression (M8, §9.4): set when this element
    /// re-derived under a stamped commit — its style attribute carries
    /// `transition:none` for exactly that commit, so recycled slots
    /// never phantom-animate on Web either.
    pub no_transition: bool,
    /// Stepped keyframe values (Phase 36 PR4, decision 357): the live
    /// keyframe track's evaluated `background-color`/`opacity`,
    /// re-declared inline every frame while the track runs (the Q4
    /// stepped path — plain tweens keep the CSS mapping and leave
    /// this empty). Empty when no keyframe track is live (unkeyframed
    /// markup byte-identical).
    pub inline_kf: String,
}

/// Per-sync accounting (the static-frame instrument).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SyncStats {
    /// Elements whose serialization changed (0 on settled frames).
    pub touched: usize,
    /// Live elements after the sync.
    pub elements: usize,
}

/// Own open-tag sync (Round 12.1): class/style/attribute/value state
/// for one live element — applied in place, children untouched, so
/// focus and caret survive by construction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttrPatch {
    /// Target `data-pid`.
    pub pid: String,
    /// Full class list (space-joined, may be empty).
    pub cls: String,
    /// Full inline style (unescaped here — escaped at JSON).
    pub style: String,
    /// Full attribute list (replaces, minus `data-pid`).
    pub attrs: Vec<(String, String)>,
    /// Stale attribute names the applier removes (disabled lifted,
    /// placeholder consumed — absence is state, never leftover).
    pub drops: Vec<String>,
    /// Field value property (`Some` for Field/Area only — applied
    /// as the live `.value`, never the attribute).
    pub value: Option<String>,
    /// Own-text HTML (`Some` for non-field kinds whose text/runs
    /// changed — applied as `innerHTML` only when the live element
    /// has no element children, otherwise skipped with a loud
    /// console warning: wrapper text with live fields inside is
    /// degenerate — layout ignores wrapper text by construction).
    pub text: Option<String>,
}

/// Final child order for one affected parent (Round 12.1): live kids
/// move in place (focus-preserving), new kids arrive as HTML blobs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlacePatch {
    /// Parent `data-pid` (scroll containers resolve to `.spacer`).
    pub parent: String,
    /// Final ordered child pids.
    pub kids: Vec<String>,
    /// HTML blobs for new kids, `(pid, html)`.
    pub blobs: Vec<(String, String)>,
}

/// Page-chrome theme stanza (theme contract round): the body
/// background + default ink the applier writes onto
/// `document.body` — CSS inheritance carries both to every
/// non-inked node, so no per-node patch is needed and explicit
/// `color:` still wins. Hex strings (`#rrggbb`), same encoding as
/// every other color on the wire.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemePatch {
    /// Page background (`tokens.background`).
    pub bg: String,
    /// Default text ink (`tokens.text_primary`).
    pub ink: String,
}

/// One keyed DOM patch (Round 12.1, decision 307): the minimal op
/// list transitioning the live page to the synced tree. The
/// bootstrap applies ops by `data-pid` — subtrees no op mentions
/// are never touched, so focused inputs keep focus/caret, IME
/// compositions survive, and media keeps playing.
#[derive(Clone, Debug, Default)]
pub struct PagePatch {
    /// Full outerHTML swaps: leaves, or field-free subtrees — never
    /// an ancestor of a live field. `(pid, html)`.
    pub swaps: Vec<(String, String)>,
    /// Own open-tag syncs (children untouched).
    pub attrs: Vec<AttrPatch>,
    /// Trailing `.sel` highlight replacement (field containers).
    /// `(field pid, html)`.
    pub sels: Vec<(String, String)>,
    /// `.spacer` height syncs (scroll containers). `(pid, height px)`.
    pub spacers: Vec<(String, f32)>,
    /// Topmost removals (descendants ride along).
    pub removes: Vec<String>,
    /// Final child orders with new-kid blobs.
    pub places: Vec<PlacePatch>,
    /// Page-chrome theme (theme contract round): `Some` exactly
    /// when the mode moved since the last patch — the applier's
    /// body-style write. Change-only, never per-frame noise.
    pub theme: Option<ThemePatch>,
    /// Root identity changed (remount-class event): the patch cannot
    /// describe it — the caller falls back to a full swap.
    pub full: bool,
}

impl PagePatch {
    /// True when no op describes anything (settled frames — a
    /// pending theme stanza counts as work, so bare toggles emit).
    pub fn is_empty(&self) -> bool {
        !self.full
            && self.swaps.is_empty()
            && self.attrs.is_empty()
            && self.sels.is_empty()
            && self.spacers.is_empty()
            && self.removes.is_empty()
            && self.places.is_empty()
            && self.theme.is_none()
    }

    /// Serializes the patch for the bootstrap applier (hand-rolled —
    /// `oppa-dom` carries no serde; strings escape per JSON).
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\"v\":1");
        out.push_str(&format!(",\"full\":{}", self.full));
        out.push_str(",\"swaps\":[");
        for (i, (pid, html)) in self.swaps.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"pid\":{},\"html\":{}}}",
                json_escape(pid),
                json_escape(html)
            ));
        }
        out.push_str("],\"attrs\":[");
        for (i, a) in self.attrs.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"pid\":{},\"cls\":{},\"style\":{},\"attrs\":[",
                json_escape(&a.pid),
                json_escape(&a.cls),
                json_escape(&a.style)
            ));
            for (j, (k, v)) in a.attrs.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                out.push_str(&format!("[{},{}]", json_escape(k), json_escape(v)));
            }
            out.push_str("],\"drops\":[");
            for (j, k) in a.drops.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                out.push_str(&json_escape(k));
            }
            out.push_str(&format!(
                "],\"value\":{},\"text\":{}}}",
                match &a.value {
                    Some(v) => json_escape(v),
                    None => "null".to_string(),
                },
                match &a.text {
                    Some(t) => json_escape(t),
                    None => "null".to_string(),
                }
            ));
        }
        out.push_str("],\"sels\":[");
        for (i, (pid, html)) in self.sels.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"pid\":{},\"html\":{}}}",
                json_escape(pid),
                json_escape(html)
            ));
        }
        out.push_str("],\"spacers\":[");
        for (i, (pid, h)) in self.spacers.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"pid\":{},\"h\":{}}}",
                json_escape(pid),
                css_num(*h)
            ));
        }
        out.push_str("],\"removes\":[");
        for (i, pid) in self.removes.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&json_escape(pid));
        }
        out.push_str("],\"places\":[");
        for (i, p) in self.places.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"parent\":{},\"kids\":[",
                json_escape(&p.parent)
            ));
            for (j, kid) in p.kids.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                out.push_str(&json_escape(kid));
            }
            out.push_str("],\"blobs\":[");
            for (j, (pid, html)) in p.blobs.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                out.push_str(&format!(
                    "{{\"pid\":{},\"html\":{}}}",
                    json_escape(pid),
                    json_escape(html)
                ));
            }
            out.push_str("]}");
        }
        out.push_str("],\"theme\":");
        match &self.theme {
            Some(t) => out.push_str(&format!(
                "{{\"bg\":{},\"ink\":{}}}",
                json_escape(&t.bg),
                json_escape(&t.ink)
            )),
            None => out.push_str("null"),
        }
        out.push('}');
        out
    }
}

/// Own-state equality ignoring child order (Round 12.1): structural
/// moves re-list children without touching paint — the patch must
/// not also emit an attrs op for the same element.
pub(crate) fn same_own(a: &DomElement, b: &DomElement) -> bool {
    let (mut x, mut y) = (a.clone(), b.clone());
    x.children.clear();
    y.children.clear();
    x == y
}

/// JSON string escaping (Round 12.1 — patch transport; mirrors the
/// HTML `esc` boundary rule: escape at serialization, raw inside).
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
