//! Ephemeral VNode layer (DESIGN §2.2): components are plain functions
//! returning tree literals; the retained tree (reconciler.rs) is what
//! renderers and layout see.
//!
//! Interpretation deltas vs DESIGN §2.2, all mechanical (full list in the M2
//! ROUNDS entry):
//!
//! - `Element.debug` (the `"track"`/`"slot"` builder label) is extra: a
//!   diagnostic label with no retained meaning, so test failures and the
//!   cycle printer can name nodes.
//! - `Element.style` is a resolved [`Style`](crate::style::Style) value, not
//!   a `StyleId`: interning happens once at the reconcile boundary (one
//!   table owned by the reconciler), so component bodies stay pure values.
//! - Pending handler closures ride the ephemeral builder and are drained
//!   into the [`HandlerRegistry`](crate::handlers::HandlerRegistry) at
//!   reconcile time; the retained node keeps ids only (locked #11 holds:
//!   the retained tree is still closure-free, copyable by id).
//! - `Column::new()` / capitalized constructors mirror DESIGN verbatim
//!   (hence the module-level `non_snake_case` allow).

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::handlers::{HandlerFn, HandlerId};
use crate::semantics::Semantics;
use crate::shell::EventKind;
use crate::style::{Color, IntoPx, Style};
use crate::text::FontWeight;

/// Shared strings: the `.clone()` tax paid deliberately, interned away
/// where hot (`StyleId`, `Arc<str>` — DESIGN §3.1).
pub type SharedString = Arc<str>;

/// Ordered child list type (DESIGN §2.2's `Children`).
pub type Children = Vec<VNode>;

/// Closed-set tag (DESIGN §2.2, locked #17 mitigation built in): a small
/// closed set plus the `Custom(fn_id)` escape hatch, so most "new node
/// type" needs are new component functions (hot) rather than new core
/// variants (cold restart).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tag {
    Div,
    Stack,
    Row,
    Column,
    Text,
    Image,
    ScrollArea,
    /// Minimal 2D grid container (Phase 36 PR2a, decision 353 — G15):
    /// column/row templates ride [`Style::grid_cols`]/
    /// [`Style::grid_rows`] ([`GridTrack`](crate::style::GridTrack)),
    /// children place row-major auto-flow with
    /// [`Style::col_span`]/[`Style::row_span`]. Lays out 2D like a
    /// block container with explicit tracks (fixed `Px`, proportional
    /// `Fr`, content `Auto`); paints its background shape like `Div`
    /// (no new [`DrawOp`](crate::render::DrawOp), no backend change —
    /// the `border`-as-`Rect` precedent).
    Grid,
    /// Vector shape (decision 291): a resolution-independent SVG-path
    /// leaf. Carries a [`PathSpec`] payload (see [`Path`]); lays out
    /// block-lite like `Image` (explicit size or zero); paints one
    /// [`DrawOp::Path`](crate::render::DrawOp) per frame-plan build.
    Path,
    /// Overlay layer (Round 1.4, decision 255): viewport-anchored,
    /// out-of-flow — parents skip portal children in flow (no extent
    /// contribution), the engine lays the portal at the viewport origin
    /// with the viewport width, hit-testing tries portals first, and the
    /// FramePlan builder emits them last (top z-layer). Unmounting a
    /// portal clears focus/captures inside it (host input hygiene).
    Portal,
    Custom(u64),
}

/// Content-addressed image handle. Values come from
/// [`ImageCache`](crate::component::ImageCache); theDrop/display-list half
/// is backend scope (M4+).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ImageId(pub u64);

/// A handler attached in the ephemeral layer: the stable id plus the
/// not-yet-registered closure. The reconciler drains `pending` into the
/// registry at commit time (first commit: under the builder id, which the
/// retained node adopts; later runs: rebound under the retained id — the M2
/// handler-identity rule) and retains only `(kind, id)`.
///
/// `pending` is a `RefCell` so the diff can take closures through the shared
/// references the keyed child matcher holds — interior mutability confined
/// to the ephemeral commit, never visible retained-side.
///
/// `owner` (M8, finding F6) is the component instance whose run built this
/// attachment, stamped by `Ctx::child`/`run_instance` at render time (the
/// reconciler drains closures in the root effect, where the running
/// instance is always the root — without the stamp every inline child's
/// handler would attribute to the root and per-slot flags could never
/// address a slot). `None` = unstamped (headless reconciler tests that
/// build VNodes by hand — the runtime falls back to the running owner).
pub struct HandlerAttachment {
    pub kind: EventKind,
    pub id: HandlerId,
    pub pending: RefCell<Option<HandlerFn>>,
    pub owner: std::cell::Cell<Option<u64>>,
}

impl std::fmt::Debug for HandlerAttachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandlerAttachment")
            .field("kind", &self.kind)
            .field("id", &self.id)
            .field("pending", &self.pending.borrow().is_some())
            .field("owner", &self.owner.get())
            .finish()
    }
}

/// Stamps the creating instance on every unstamped handler attachment in
/// `vnode` (M8, finding F6). Inner children stamp first (their `Ctx::child`
/// return passes through the outer render), so outer runs only fill
/// `None`s — nesting composes, innermost wins.
pub fn stamp_handler_owner(vnode: &VNode, owner: u64) {
    match vnode {
        VNode::Element(e) => {
            for h in &e.handlers {
                if h.owner.get().is_none() {
                    h.owner.set(Some(owner));
                }
            }
            for child in &e.children {
                stamp_handler_owner(child, owner);
            }
        }
        VNode::Fragment(fs) => {
            for child in fs {
                stamp_handler_owner(child, owner);
            }
        }
        VNode::Text(_) | VNode::Hole => {}
    }
}

/// Ephemeral element: what component bodies assemble.
#[derive(Debug)]
pub struct Element {
    pub tag: Tag,
    pub debug: String,
    pub key: Option<u64>,
    pub style: Style,
    /// Leaf-class hint carried by `Text` conversions (title vs body); diffed
    /// like style. `None` for non-text elements.
    pub text_hint: Option<TextClass>,
    /// Image source carried by `Img` conversions (round 4.4): the
    /// cache id whose key is the portable image reference (a URL on
    /// web, a cache key natively). Diffed like style — a src change
    /// re-renders. `None` for non-image elements.
    pub image: Option<ImageId>,
    /// Vector payload carried by [`Path`] conversions (decision 291):
    /// the SVG path data plus its fill/stroke. Diffed like `image` —
    /// a payload change repaints (never re-lays-out: geometry comes
    /// from the style box). `None` for non-path elements.
    pub path: Option<PathSpec>,
    pub semantics: Option<Semantics>,
    pub handlers: Vec<HandlerAttachment>,
    pub children: Children,
}

/// The ephemeral tree (DESIGN §2.2). Discarded after each reconcile; stable
/// identity lives in the retained tree only. The element variant is boxed:
/// this tree churns every commit, so the big variant must not ride the
/// stack through every match.
#[derive(Debug)]
pub enum VNode {
    Element(Box<Element>),
    Text(SharedString),
    Fragment(Vec<VNode>),
    Hole,
}

impl VNode {
    pub fn hole() -> Self {
        VNode::Hole
    }

    pub fn is_hole(&self) -> bool {
        matches!(self, VNode::Hole)
    }

    /// The explicit stable identity for lists/hot-reload (§2.2 `key`).
    pub fn key(&self) -> Option<u64> {
        match self {
            VNode::Element(e) => e.key,
            _ => None,
        }
    }
}

impl From<Element> for VNode {
    fn from(e: Element) -> Self {
        VNode::Element(Box::new(e))
    }
}

static AUTO_HANDLER_SEQ: AtomicU64 = AtomicU64::new(1);

fn auto_handler_id(debug: &str) -> HandlerId {
    // Creation-ordered auto ids (deterministic per build: §9.1's
    // call-site-stable tie-break is the same ordering claim). Stable
    // symbol-hash handler naming arrives with the #[component] manifest in
    // M2b; M2 handlers created through builders are headless-test scope.
    let n = AUTO_HANDLER_SEQ.fetch_add(1, Ordering::Relaxed);
    HandlerId::from_symbol(&format!("auto:{debug}#{n}"))
}

/// Chainable element builder — the `Div("track").style(...).child(...)`
/// shape. Terminal methods (`.child` / `.children` / `.build`) produce the
/// `VNode` the component returns.
#[derive(Debug)]
pub struct ElementBuilder {
    inner: Element,
}

impl ElementBuilder {
    fn new(tag: Tag, debug: &str) -> Self {
        Self {
            inner: Element {
                tag,
                debug: debug.to_string(),
                key: None,
                style: Style::default(),
                text_hint: None,
                image: None,
                path: None,
                semantics: None,
                handlers: Vec::new(),
                children: Vec::new(),
            },
        }
    }

    pub fn key(mut self, key: u64) -> Self {
        self.inner.key = Some(key);
        self
    }

    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.inner.style = style.into();
        self
    }

    pub fn semantics(mut self, s: Semantics) -> Self {
        self.inner.semantics = Some(s);
        self
    }

    /// Attaches a press handler closure. The closure is registered with the
    /// runtime registry at reconcile time under an auto id; the retained
    /// node keeps the id only (lock #11).
    pub fn on_press(mut self, f: impl Fn() + 'static) -> Self {
        let id = auto_handler_id(&self.inner.debug);
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::Press,
            id,
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// Attaches a hold handler closure (OQ-G11-2): declares the node's
    /// distinct long-press action, fired by the router when a hold
    /// passes the deadline without leaving the slop. Owners without
    /// one fall back to their press handler (additive — tap behavior
    /// never changes). Requires a press handler on the same node to
    /// arm at all (press ownership routes the hold — decision 96
    /// stands, so a hold-only node stays inert); payload-less like
    /// every handler (lock #11).
    pub fn on_long_press(mut self, f: impl Fn() + 'static) -> Self {
        let id = auto_handler_id(&self.inner.debug);
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::LongPress,
            id,
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// Attaches a swipe handler closure (Round 3.2, OQ-G11-1):
    /// declares the node's fling action, fired by the router when a
    /// fast far lift starts on it (capture owner — no inside check,
    /// the Android touch-target rule). Owners without one stay
    /// quiet on swipes (a swipe is not a tap, so it must not press
    /// — unhandled-key precedent); requires a press handler on the
    /// same node to arm at all (press ownership routes the gesture —
    /// decision 96 stands, so a swipe-only node stays inert, the
    /// hold-only precedent). Payload-less like every handler
    /// (lock #11 — direction-aware swipe is a follow-up).
    pub fn on_swipe(mut self, f: impl Fn() + 'static) -> Self {
        let id = auto_handler_id(&self.inner.debug);
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::Swipe,
            id,
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// Attaches a drag-move handler closure (Round 5.3, OQ-G2-1):
    /// fired by the router on every Move while this node is the
    /// pointer's capture owner (press first — decision 96 stands,
    /// so a drag-only node stays inert). The pointer position rides
    /// alongside, never inside the closure (lock #11): read it with
    /// [`ComponentHost::pointer_position`](crate::component::ComponentHost::pointer_position)
    /// for the lowest-id live capture plus the node's committed box
    /// (single-drag v1 bound — concurrent multi-finger drags on one
    /// node resolve to the primary capture, stated). Payload-less
    /// like every handler.
    pub fn on_drag(mut self, f: impl Fn() + 'static) -> Self {
        let id = auto_handler_id(&self.inner.debug);
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::Drag,
            id,
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// Attaches a drag-release handler closure (Round 21.3, decision
    /// 330): declares the node's plain-drag-release action, fired
    /// by the router when a slow far lift starts on it (capture
    /// owner) and lands inside its subtree — the menu drag-select
    /// shape (press, drag onto a row, release invokes). Owners
    /// without one stay quiet on drag-releases (a drag-release is
    /// not a tap — unhandled-key precedent); requires a press
    /// handler on the same node to arm at all (decision 96
    /// stands). The release point rides
    /// [`ComponentHost::last_drag_release`](crate::component::ComponentHost::last_drag_release),
    /// never the closure (lock #11). Payload-less like every
    /// handler.
    pub fn on_drag_release(mut self, f: impl Fn() + 'static) -> Self {
        let id = auto_handler_id(&self.inner.debug);
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::DragRelease,
            id,
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// Attaches directional arrow-key handler closures (Round 5.3,
    /// OQ-G2-1): fired by the router when the focused owner declares
    /// one (Left/Up/Down/Right — held keys repeat-step, no repeat
    /// suppression). Four kinds instead of one keyed payload (the
    /// Press/LongPress/Swipe precedent — lock #11 stands).
    /// Undeclared directions fall back to the generic Key handler
    /// (existing ambient path), else quiet. Like all key handling,
    /// requires focus (Tab/click first — decision 96).
    pub fn on_key_left(mut self, f: impl Fn() + 'static) -> Self {
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::KeyLeft,
            id: auto_handler_id(&self.inner.debug),
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// Up-arrow handler (see [`ElementBuilder::on_key_left`]).
    pub fn on_key_up(mut self, f: impl Fn() + 'static) -> Self {
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::KeyUp,
            id: auto_handler_id(&self.inner.debug),
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// Right-arrow handler (see [`ElementBuilder::on_key_left`]).
    pub fn on_key_right(mut self, f: impl Fn() + 'static) -> Self {
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::KeyRight,
            id: auto_handler_id(&self.inner.debug),
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// Down-arrow handler (see [`ElementBuilder::on_key_left`]).
    pub fn on_key_down(mut self, f: impl Fn() + 'static) -> Self {
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::KeyDown,
            id: auto_handler_id(&self.inner.debug),
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// Attaches a secondary-press handler closure (Round 9.2, decision
    /// 301): the raw right-click tap, fired by the router on the
    /// capture owner when declared (quiet otherwise — a secondary tap
    /// is never a primary `Press`, the unhandled-key precedent).
    /// Requires a press handler on the same node to arm at all (press
    /// ownership routes the tap — decision 96 stands, so a
    /// secondary-only node stays inert, the hold/swipe precedent).
    /// Payload-less like every handler (lock #11).
    pub fn on_secondary_press(mut self, f: impl Fn() + 'static) -> Self {
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::SecondaryPress,
            id: auto_handler_id(&self.inner.debug),
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// Attaches a context-menu handler closure (Round 9.2, decision
    /// 301): the semantic right-click, fired right after
    /// `on_secondary_press` on the capture owner when declared (each
    /// fires independently when declared — DOM order precedent).
    /// Same arming rule as [`ElementBuilder::on_secondary_press`].
    /// Payload-less like every handler (lock #11).
    pub fn on_context_menu(mut self, f: impl Fn() + 'static) -> Self {
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::ContextMenu,
            id: auto_handler_id(&self.inner.debug),
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// Attaches a scroll handler closure (M7, decision 112): declares
    /// the node a scroll target so routed
    /// [`InputEvent::Scroll`](crate::input::InputEvent) events dispatch
    /// instead of failing loudly (M5's miss rule stands). The offset
    /// feeds are [`ComponentHost::bind_scroll`] (vertical `dy`) and
    /// [`ComponentHost::bind_scroll_x`] (horizontal `dx`, Round 9.3 —
    /// opt-in per container); the closure is authoring intent, kept
    /// payload-less like every handler (lock #11).
    pub fn on_scroll(mut self, f: impl Fn() + 'static) -> Self {
        let id = auto_handler_id(&self.inner.debug);
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::Scroll,
            id,
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// Attaches an IME handler closure (M7, decision 112): declares the
    /// node an IME target so routed
    /// [`InputEvent::Ime`](crate::input::InputEvent) events dispatch
    /// instead of failing loudly. Payload-less like every handler.
    pub fn on_ime(mut self, f: impl Fn() + 'static) -> Self {
        let id = auto_handler_id(&self.inner.debug);
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::Ime,
            id,
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// Column-gap shorthand the §4.2 example chains on the element rather
    /// than inside `.style(...)`; stored into the style payload.
    pub fn gap(mut self, g: impl crate::style::IntoPx) -> Self {
        self.inner.style.gap = Some(g.into_px());
        self
    }

    /// Scroll extent of a `ScrollArea` spacer (§4.2 `content_size`).
    pub fn content_size(mut self, s: impl crate::style::IntoPx) -> Self {
        self.inner.style.content_size = Some(s.into_px());
        self
    }

    pub fn child(mut self, c: VNode) -> VNode {
        self.inner.children.push(c);
        VNode::Element(Box::new(self.inner))
    }

    pub fn children(mut self, cs: impl IntoIterator<Item = VNode>) -> VNode {
        self.inner.children.extend(cs);
        VNode::Element(Box::new(self.inner))
    }

    /// Terminal for childless elements (the §4 knob: `Div("knob").style(..)`
    /// plus this one mechanical `.build()` — a bare chained builder is not
    /// a `VNode` on stable Rust).
    pub fn build(self) -> VNode {
        VNode::Element(Box::new(self.inner))
    }
}

/// `Div("track")` — DESIGN §4.1 verbatim constructor shape.
pub fn Div(debug: &str) -> ElementBuilder {
    ElementBuilder::new(Tag::Div, debug)
}

/// `Row("slot")` / `Row("cell")` — §4.2 constructor shape.
pub fn Row(debug: &str) -> ElementBuilder {
    ElementBuilder::new(Tag::Row, debug)
}

/// `Stack(..)` — closed-set member, exercised by tests.
pub fn Stack(debug: &str) -> ElementBuilder {
    ElementBuilder::new(Tag::Stack, debug)
}

/// `ScrollArea("list")` — §4.2 constructor shape.
pub fn ScrollArea(debug: &str) -> ElementBuilder {
    ElementBuilder::new(Tag::ScrollArea, debug)
}

/// `Grid("sheet")` — Phase 36 PR2a (decision 353, G15) constructor
/// shape: the template rides the style (`.grid_cols(..)` /
/// `.grid_rows(..)`), children place row-major auto-flow.
pub fn Grid(debug: &str) -> ElementBuilder {
    ElementBuilder::new(Tag::Grid, debug)
}

/// `Portal("menu")` — overlay layer (Round 1.4, decision 255):
/// viewport-anchored out-of-flow container; children overlay at the
/// viewport origin, each constrained to the viewport width (so rows
/// fill and center exactly like root children). No anchor positioning
/// in v1 (follow-up) — popups compose full-width rows with
/// alignment, like `Modal` does.
pub fn Portal(debug: &str) -> ElementBuilder {
    ElementBuilder::new(Tag::Portal, debug)
}

/// `Custom("player", 7)` — the closed-set escape hatch (DESIGN §2.2,
/// locked #17 mitigation): presenter-recognized foreign content. The
/// DOM backend renders it as the external-element hole
/// (`data-external` marker, same foreign-element mechanism as
/// verdict-(b)'s `<input>` — M7 decision 113); GPU backends lay it out
/// block-lite and paint its background shape (no foreign pixels in v1).
pub fn Custom(debug: &str, id: u64) -> ElementBuilder {
    ElementBuilder::new(Tag::Custom(id), debug)
}

/// `Column::new().gap(2).children(..)` — §4.2 constructor shape (returns the
/// element builder, not `Self`, by design — hence the explicit allow).
pub struct Column;

impl Column {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> ElementBuilder {
        ElementBuilder::new(Tag::Column, "column")
    }
}

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
        VNode::from(b.build())
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
            semantics: Some(crate::semantics::Semantics::text_area().label(&t.label)),
            handlers: Vec::new(),
            children: vec![VNode::Text(t.text)],
        }))
    }
}

/// `Img { src, size, radius }` leaf (§4.2). `src` is the content-addressed
/// id the image-cache service returns; decode/mailbox plumbing is M4+/M7
/// scope (§9.1 workers). Round 4.4 threads `src` into the retained
/// node (it used to drop here) — backends resolve the id through
/// the cache (DOM: key as URL; native: pixels by id).
#[derive(Clone, Copy, Debug)]
pub struct Img {
    pub src: ImageId,
    pub size: f32,
    pub radius: f32,
}

impl From<Img> for VNode {
    fn from(i: Img) -> Self {
        let mut style = Style::default();
        style.w = Some(crate::style::Px::of(i.size));
        style.h = Some(crate::style::Px::of(i.size));
        style.radius = Some(crate::style::Px::of(i.radius));
        VNode::Element(Box::new(Element {
            tag: Tag::Image,
            debug: "img".to_string(),
            key: None,
            style,
            text_hint: None,
            image: Some(i.src),
            path: None,
            semantics: None,
            handlers: Vec::new(),
            children: Vec::new(),
        }))
    }
}

/// Stroke paint for a [`PathSpec`] (decision 291): solid color at a
/// CSS-px width. Caps/joins are fixed round on every backend (icons
/// and checkmarks need soft ends; parameterizing them is a
/// follow-up) — one rule, so CPU/Vello/DOM agree by construction.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct StrokeDesc {
    pub color: Color,
    pub width: f32,
}

/// Vector payload for [`Tag::Path`] nodes (decision 291): standard
/// SVG path data (`M … L … C … Z`, absolute or relative) in the
/// node's local space (origin at the committed box top-left, units
/// are authoring px — 1:1 with device px at DPR 1; HiDPI vector
/// scaling is a follow-up, recorded as an open question), plus an
/// optional solid fill and an optional stroke. At least one of
/// `fill`/`stroke` is `Some` (enforced by [`Path::build`], rechecked
/// loudly at plan build for hand-built elements — an invisible
/// vector is an authoring bug, never a silent no-op).
#[derive(Clone, PartialEq, Debug)]
pub struct PathSpec {
    pub data: SharedString,
    pub fill: Option<Color>,
    pub stroke: Option<StrokeDesc>,
}

/// `Path::new("checkbox-check")` — the vector-leaf authoring surface
/// (decision 291): chainable like [`Text::new`], terminal
/// [`.build()`](Path::build) producing the `VNode` the component
/// returns. The leaf is childless with an explicit [`Tag::Path`]
/// tag; geometry rides `.size(...)` (explicit style w/h, so layout
/// commits a real box for hit-testing and damage); [`.offset(...)`
/// ](Path::offset) pins it out-of-flow (the knob-in-track
/// precedent) when it must sit inside a fixed box.
#[derive(Clone, Debug)]
pub struct Path {
    debug: String,
    data: Option<SharedString>,
    fill: Option<Color>,
    stroke: Option<StrokeDesc>,
    style: Style,
}

impl Path {
    pub fn new(debug: &str) -> Self {
        Self {
            debug: debug.to_string(),
            data: None,
            fill: None,
            stroke: None,
            style: Style::default(),
        }
    }

    /// SVG path data (e.g. `"M 0 0 L 10 10 Z"`). Required — a path
    /// without data refuses loudly at [`.build()`](Path::build).
    pub fn data(mut self, d: &str) -> Self {
        self.data = Some(SharedString::from(d));
        self
    }

    /// Solid fill paint.
    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }

    /// Solid stroke paint at a CSS-px width (round caps/joins on
    /// every backend — see [`StrokeDesc`]). Non-finite or negative
    /// widths refuse loudly at [`.build()`](Path::build).
    pub fn stroke(mut self, color: Color, width: f32) -> Self {
        self.stroke = Some(StrokeDesc { color, width });
        self
    }

    /// Explicit box (CSS px, both spellings like `.size(20, 20)`).
    /// Without it the leaf lays out 0×0 (it still paints its data —
    /// overflow, like text — but hit-testing and damage see nothing,
    /// so sized paths are the rule, bare ones the loud exception in
    /// tests, never silent layout).
    pub fn size(mut self, w: impl IntoPx, h: impl IntoPx) -> Self {
        self.style.w = Some(w.into_px());
        self.style.h = Some(h.into_px());
        self
    }

    /// Out-of-flow offset within the parent (the knob-in-track
    /// precedent — bypasses flow and extents, so a check can sit at
    /// the box origin without resizing it).
    pub fn offset(mut self, x: impl IntoPx, y: impl IntoPx) -> Self {
        self.style.x = Some(x.into_px());
        self.style.absolute_y = Some(y.into_px());
        self
    }

    /// Terminal: produces the `VNode`. Panics loudly (never an
    /// invisible leaf) when data is missing/blank, when neither
    /// fill nor stroke is set, or when any numeric is non-finite
    /// or negative.
    pub fn build(self) -> VNode {
        let data = self
            .data
            .filter(|d| !d.trim().is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "oppa::Path {:?}: no path data — call .data(...) (an invisible vector never builds silently)",
                    self.debug
                )
            });
        if self.fill.is_none() && self.stroke.is_none() {
            panic!(
                "oppa::Path {:?}: neither fill nor stroke — set one (an invisible vector never builds silently)",
                self.debug
            );
        }
        if let Some(s) = self.stroke {
            if !s.width.is_finite() {
                panic!(
                    "oppa::Path {:?}: stroke width is non-finite ({}) — NaN/Inf never paints silently",
                    self.debug,
                    s.width
                );
            }
            if s.width < 0.0 {
                panic!(
                    "oppa::Path {:?}: stroke width is negative ({}) — negative widths never paint silently",
                    self.debug,
                    s.width
                );
            }
        }
        for (name, v) in [
            ("w", self.style.w),
            ("h", self.style.h),
            ("x", self.style.x),
            ("absolute_y", self.style.absolute_y),
        ] {
            if let Some(px) = v {
                let f = px.get();
                if !f.is_finite() {
                    panic!(
                        "oppa::Path {:?}: {name} is non-finite ({f}) — NaN/Inf never lays out silently",
                        self.debug
                    );
                }
                if f < 0.0 {
                    panic!(
                        "oppa::Path {:?}: {name} is negative ({f}) — negative geometry never lays out silently",
                        self.debug
                    );
                }
            }
        }
        VNode::Element(Box::new(Element {
            tag: Tag::Path,
            debug: self.debug,
            key: None,
            style: self.style,
            text_hint: None,
            image: None,
            path: Some(PathSpec {
                data,
                fill: self.fill,
                stroke: self.stroke,
            }),
            semantics: None,
            handlers: Vec::new(),
            children: Vec::new(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_chain_shapes() {
        let v: VNode = Div("track").style(Style::new().size(44, 24)).build();
        match &v {
            VNode::Element(e) => {
                assert_eq!(e.tag, Tag::Div);
                assert_eq!(e.debug, "track");
                assert_eq!(e.key, None);
            }
            _ => panic!("expected element"),
        }
        let v: VNode = Row("slot").key(3).child(VNode::Hole);
        assert_eq!(v.key(), Some(3));
        let v: VNode = Column::new().gap(2).children([VNode::Hole, VNode::Hole]);
        match v {
            VNode::Element(e) => assert_eq!(e.children.len(), 2),
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn leaf_structs_convert() {
        let t: VNode = Text {
            text: Arc::from("hi"),
            style: Text::title_small,
        }
        .into();
        assert!(matches!(t, VNode::Element(_)));
        let i: VNode = Img {
            src: ImageId(7),
            size: 36.0,
            radius: 18.0,
        }
        .into();
        assert!(matches!(i, VNode::Element(_)));
    }

    /// Round 4.4: `Img` conversion preserves the cache id (it used
    /// to drop `src` — backends had nothing to resolve).
    #[test]
    fn img_conversion_carries_src() {
        let v: VNode = Img {
            src: ImageId(7),
            size: 36.0,
            radius: 18.0,
        }
        .into();
        match v {
            VNode::Element(e) => {
                assert_eq!(e.tag, Tag::Image);
                assert_eq!(e.image, Some(ImageId(7)));
            }
            _ => panic!("expected element"),
        }
    }

    /// Decision 291: `Path` builds a tagged leaf carrying its spec
    /// (data + fill/stroke), with geometry in the style box.
    #[test]
    fn path_build_carries_spec_and_geometry() {
        let v: VNode = Path::new("check")
            .data("M 0 0 L 10 10 Z")
            .fill(Color(0xFF_00_00))
            .stroke(Color(0x00_00_00), 2.0)
            .size(20, 20)
            .offset(1, 2)
            .build();
        match v {
            VNode::Element(e) => {
                assert_eq!(e.tag, Tag::Path);
                assert_eq!(e.debug, "check");
                let spec = e.path.expect("path payload");
                assert_eq!(spec.data.as_ref(), "M 0 0 L 10 10 Z");
                assert_eq!(spec.fill, Some(Color(0xFF_00_00)));
                assert_eq!(
                    spec.stroke,
                    Some(StrokeDesc {
                        color: Color(0x00_00_00),
                        width: 2.0
                    })
                );
                assert_eq!(e.style.w.map(crate::style::Px::get), Some(20.0));
                assert_eq!(e.style.h.map(crate::style::Px::get), Some(20.0));
                assert!(e.children.is_empty(), "path leaves are childless");
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    #[should_panic(expected = "no path data")]
    fn path_without_data_panics_loudly() {
        let _ = Path::new("nodata").fill(Color(1)).build();
    }

    #[test]
    #[should_panic(expected = "neither fill nor stroke")]
    fn path_without_paint_panics_loudly() {
        let _ = Path::new("invisible").data("M 0 0 L 1 1").build();
    }

    #[test]
    #[should_panic(expected = "non-finite")]
    fn path_with_nan_stroke_panics_loudly() {
        let _ = Path::new("nan")
            .data("M 0 0 L 1 1")
            .stroke(Color(1), f32::NAN)
            .build();
    }

    #[test]
    #[should_panic(expected = "negative")]
    fn path_with_negative_stroke_panics_loudly() {
        let _ = Path::new("neg")
            .data("M 0 0 L 1 1")
            .stroke(Color(1), -1.0)
            .build();
    }
}
