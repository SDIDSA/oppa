//! Text leaves: `Text`, `TextField`, `TextArea`, multi-span `RichText` (all reconcile to `Tag::Text`).

use crate::style::{Color, Style};
use crate::text::FontWeight;

use super::{Element, SharedString, Tag, VNode};

/// Text-leaf style classes (`Text::title_small`, `Text::body_secondary` in
/// §4.2, plus author-chosen sizes/weights via [`Text::new`] in decision
/// 239). These are authoring tokens, not the shaper's
/// [`TextStyle`](crate::text::TextStyle): they resolve to styles downstream.
/// `Custom` sizes are absolute CSS px (config-independent, unlike the two
/// tokens which resolve through [`LayoutTextConfig`](crate::layout::LayoutTextConfig)).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TextClass {
    TitleSmall,
    BodySecondary,
    Custom { size_px: u16, weight: FontWeight },
}

/// One styled span of a [`RichText`] paragraph (Phase 36 PR3,
/// decision 355): contiguous text sharing one shaper weight and one
/// paint ink. Spans share the paragraph's size (one [`TextClass`]
/// per `RichText` node — mixed sizes stay out of minimal RichText,
/// stated, never silently unified): weight varies the shaping (each
/// span shapes separately — shape-per-span, never re-shape, the M3
/// no-re-shape rule), ink varies the paint (per-run ink downstream).
/// No cross-span ligatures/kerning (shaping sees one span at a time —
/// stated shaping boundary, the Q5 shape-per-line answer applied to
/// spans). Empty spans are inert (zero bytes — skipped at layout,
/// never a silent break).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextSpan {
    pub text: SharedString,
    pub weight: FontWeight,
    pub ink: Option<Color>,
}

impl TextSpan {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            weight: FontWeight::NORMAL,
            ink: None,
        }
    }

    /// Shaper weight for this span (e.g. `FontWeight::BOLD`).
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Paint ink for this span (`None` = the node's ink — style ink,
    /// then theme `text_primary`, the `resolve_ink` rule).
    pub fn ink(mut self, color: Color) -> Self {
        self.ink = Some(color);
        self
    }
}

/// Multi-span text leaf (Phase 36 PR3, decision 355 — G22 display
/// half): a paragraph of [`TextSpan`]s sharing one [`TextClass`]
/// size. Concatenated bytes are the paragraph (caret/selection math
/// is paragraph-relative, unchanged); each span shapes with its own
/// weight and paints with its own ink.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RichText {
    pub spans: Vec<TextSpan>,
    pub style: TextClass,
}

impl RichText {
    pub fn new(spans: Vec<TextSpan>) -> Self {
        Self {
            spans,
            style: Text::body_secondary,
        }
    }

    /// Paragraph size class (shared by all spans — see [`TextSpan`]).
    pub fn style(mut self, style: TextClass) -> Self {
        self.style = style;
        self
    }

    /// Concatenated paragraph bytes (the caret/selection space).
    pub fn joined_text(&self) -> String {
        let mut out = String::new();
        for s in &self.spans {
            out.push_str(&s.text);
        }
        out
    }
}

impl From<RichText> for VNode {
    fn from(t: RichText) -> Self {
        VNode::RichText(t)
    }
}

/// `TextField { text, style, label }` leaf (M7, decision 113): an
/// editable field per DESIGN locked #24 — **no new [`Tag`]**, just the
/// behavior flag. Renders as a `Tag::Text` element carrying
/// [`Semantics::text_field`](crate::semantics::Semantics) (the
/// presenter-recognized special case: the DOM backend owns editing
/// authority for it under verdict (b), locked #27) with the value as
/// its text child — the exact shape the `Text` leaf uses, so layout,
/// measurement, and the reconciler treat it as text and only the
/// presenter branches.
#[derive(Clone, Debug)]
pub struct TextField {
    pub text: SharedString,
    pub style: TextClass,
    /// Accessible name (required — text-edit a11y from day one, #24).
    pub label: SharedString,
}

/// `TextArea { text, style, label }` leaf (Round 5.1): the
/// multi-line sibling of [`TextField`] — same text shape (layout
/// wraps paragraphs, the engine already flows `\n`), same
/// presenter-owned editing, but the
/// [`Semantics::text_area`](crate::semantics::Semantics) role so
/// routers insert newlines on Enter (instead of activating) and
/// backends render `<textarea>`.
#[derive(Clone, Debug)]
pub struct TextArea {
    pub text: SharedString,
    pub style: TextClass,
    /// Accessible name (required — same rule as `TextField`).
    pub label: SharedString,
}

/// `Text { text, style }` leaf (§4.2).
#[derive(Clone, Debug)]
pub struct Text {
    pub text: SharedString,
    pub style: TextClass,
}

impl Text {
    // Lowercase by design: mirrors the §4.2 `Text::title_small` authoring
    // surface verbatim (associated-class tokens, not constants).
    #[allow(non_upper_case_globals)]
    pub const title_small: TextClass = TextClass::TitleSmall;
    #[allow(non_upper_case_globals)]
    pub const body_secondary: TextClass = TextClass::BodySecondary;

    /// Starts the chainable builder for author-chosen sizes/weights
    /// (decision 239): `Text::new("Heading").size(32).bold().build()`.
    /// Untouched, the builder yields `BodySecondary` (the v1 default —
    /// zero breakage for existing call sites).
    ///
    /// Returns a builder rather than `Self` by design (the §4 surface) —
    /// hence the explicit allow (same pattern as `Style::new`).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(text: impl Into<SharedString>) -> TextBuilder {
        TextBuilder {
            text: text.into(),
            size_px: None,
            weight: FontWeight::NORMAL,
            ink: None,
        }
    }
}

/// Chainable builder for [`Text`] with author-chosen size/weight
/// (decision 239). Either modifier (or both) may be set; `.build()`
/// yields a [`Text`] (and converts into a [`VNode`] directly).
#[derive(Clone, Debug)]
pub struct TextBuilder {
    text: SharedString,
    size_px: Option<u16>,
    weight: FontWeight,
    ink: Option<Color>,
}

impl TextBuilder {
    /// Absolute CSS-px size (a `0` is stored and refused loudly at
    /// layout, per the loud-failures rule — never silently laid out).
    pub fn size(mut self, px: u16) -> Self {
        self.size_px = Some(px);
        self
    }

    /// Author-specified shaper weight (e.g. `FontWeight::BOLD`).
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Shorthand for `.weight(FontWeight::BOLD)`.
    pub fn bold(mut self) -> Self {
        self.weight = FontWeight::BOLD;
        self
    }

    /// Paint ink for the leaf (decision 510 — the `with_ink` wrapper
    /// as a chain link: `Text::new("Hi").size(14).ink(DIM).into()`).
    /// `None` (default) keeps the theme resolution exactly as before.
    pub fn ink(mut self, color: Color) -> Self {
        self.ink = Some(color);
        self
    }

    pub fn build(self) -> Text {
        match self.size_px {
            None if self.weight == FontWeight::NORMAL => Text {
                text: self.text,
                style: Text::body_secondary,
            },
            None => Text {
                // Weight-only: no size was authored. Custom sizes are
                // absolute (config-independent), so there is no token to
                // fall back to — default to the body size of the default
                // config (14px) and state it here, not silently.
                // (Interpretation decision, recorded in rounds.md 239.)
                text: self.text,
                style: TextClass::Custom {
                    size_px: 14,
                    weight: self.weight,
                },
            },
            Some(size_px) => Text {
                text: self.text,
                style: TextClass::Custom {
                    size_px,
                    weight: self.weight,
                },
            },
        }
    }
}

impl From<TextBuilder> for VNode {
    fn from(b: TextBuilder) -> Self {
        let ink = b.ink;
        let mut vnode = VNode::from(b.build());
        if let Some(color) = ink {
            vnode = vnode.with_ink(color);
        }
        vnode
    }
}

impl From<Text> for VNode {
    fn from(t: Text) -> Self {
        VNode::Element(Box::new(Element {
            tag: Tag::Text,
            debug: "text".to_string(),
            key: None,
            style: Style::default(),
            text_hint: Some(t.style),
            image: None,
            path: None,
            canvas: None,
            semantics: None,
            handlers: Vec::new(),
            children: vec![VNode::Text(t.text)],
        }))
    }
}

impl From<TextField> for VNode {
    fn from(t: TextField) -> Self {
        VNode::Element(Box::new(Element {
            tag: Tag::Text,
            debug: "field".to_string(),
            key: None,
            style: Style::default(),
            text_hint: Some(t.style),
            image: None,
            path: None,
            canvas: None,
            semantics: Some(crate::semantics::Semantics::text_field().label(&t.label)),
            handlers: Vec::new(),
            children: vec![VNode::Text(t.text)],
        }))
    }
}

impl From<TextArea> for VNode {
    fn from(t: TextArea) -> Self {
        VNode::Element(Box::new(Element {
            tag: Tag::Text,
            debug: "area".to_string(),
            key: None,
            style: Style::default(),
            text_hint: Some(t.style),
            image: None,
            path: None,
            canvas: None,
            semantics: Some(crate::semantics::Semantics::text_area().label(&t.label)),
            handlers: Vec::new(),
            children: vec![VNode::Text(t.text)],
        }))
    }
}
