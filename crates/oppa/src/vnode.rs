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
//!   (hence the module-level `non_snake_case` allow in
//!   [`element`](self::element)).

mod element;
mod media;
mod text;

pub use element::{Column, Custom, Div, ElementBuilder, Grid, Portal, Row, ScrollArea, Stack};
pub use media::{
    extract_svg_path, scale_svg_path, svg_path_data, Canvas, CanvasOp, CanvasSpec, Img, Path,
    PathSpec, StrokeDesc,
};
pub use text::{RichText, Text, TextArea, TextBuilder, TextClass, TextField, TextSpan};

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::handlers::{HandlerFn, HandlerId};
use crate::semantics::Semantics;
use crate::shell::EventKind;
use crate::style::Style;

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
    /// Retained immediate-mode surface (Phase 36 PR4, decision 358):
    /// author-supplied vector content painted from a [`CanvasSpec`]
    /// payload. Lays out block-lite like `Image` (explicit size or
    /// zero); paints the spec's ops translated by the committed box
    /// origin — `Rect`/`RRect`/`Path` directly, `Text` through the
    /// text service (shaped at layout, appended to the box lines with
    /// the op's ink, so builders split paint without new `DrawOp`s).
    /// Childless by construction (children refuse loudly — a canvas
    /// paints its spec); payload changes repaint without re-layout
    /// (geometry comes from the style box); a restart boundary for
    /// dirty walks (payload dirt never crosses into children —
    /// there are none — and layout never re-enters on paint).
    Canvas,
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
        VNode::Text(_) | VNode::RichText(_) | VNode::Hole => {}
    }
}

/// Stamps the creating instance on every unstamped element in
/// `vnode` (decision 519 — the node-ownership twin of
/// [`stamp_handler_owner`]). Inner children stamp first (their
/// `Ctx::child` return passes through the outer render), so outer
/// runs only fill `None`s — nesting composes, innermost wins.
/// Text leaves carry no stamp (they inherit the parent owner at
/// build time); fragments recurse; holes hold nothing.
pub fn stamp_node_owner(vnode: &VNode, owner: u64) {
    match vnode {
        VNode::Element(e) => {
            if e.owner.get().is_none() {
                e.owner.set(Some(owner));
            }
            for child in &e.children {
                stamp_node_owner(child, owner);
            }
        }
        VNode::Fragment(fs) => {
            for child in fs {
                stamp_node_owner(child, owner);
            }
        }
        VNode::Text(_) | VNode::RichText(_) | VNode::Hole => {}
    }
}

/// Ephemeral element: what component bodies assemble.
///
/// `owner` (decision 519) is the component instance whose run built this
/// element, stamped by `Ctx::child`/`run_instance` at render time
/// (innermost wins, like the handler stamp above — the reconciler
/// retains it per node so subtree removal can identify dead
/// instances). `None` = unstamped (hand-built VNodes — the
/// reconciler inherits the parent owner for those).
#[derive(Debug)]
pub struct Element {
    pub tag: Tag,
    pub debug: String,
    pub key: Option<u64>,
    pub owner: std::cell::Cell<Option<u64>>,
    pub style: Style,
    /// Leaf-class hint carried by `Text` conversions (title vs body); diffed
    /// like style. `None` for non-text elements.
    pub text_hint: Option<text::TextClass>,
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
    /// Canvas payload carried by [`Canvas`] conversions (Phase 36 PR4,
    /// decision 358): retained vector ops in local space. Diffed like
    /// `path` — a payload change repaints, never re-lays-out.
    /// `None` for non-canvas elements.
    pub canvas: Option<CanvasSpec>,
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
    /// Multi-span text leaf (Phase 36 PR3, decision 355): reconciles
    /// to a `Tag::Text` retained node carrying the joined bytes plus
    /// the span table (backends key off the tag — no tag change).
    RichText(text::RichText),
    Fragment(Vec<VNode>),
    Hole,
}

impl VNode {
    pub fn hole() -> Self {
        VNode::Hole
    }

    /// Sets the text-ink override on a converted text-leaf `VNode`
    /// (decision 510 — the `oppa-controls::with_ink` helper, now
    /// shared: placeholder dimming, label inks, active-tab ink all
    /// ride this). Converted leaves always arrive as
    /// `VNode::Element`, so any other shape is a wiring bug and
    /// refused loudly, never passed through unstyled.
    pub fn with_ink(self, ink: crate::style::Color) -> Self {
        match self {
            VNode::Element(mut e) => {
                e.style.ink = Some(ink);
                VNode::Element(e)
            }
            _ => panic!("ink target is not an element — refused, never silent"),
        }
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

pub(super) fn auto_handler_id(debug: &str) -> HandlerId {
    // Creation-ordered auto ids (deterministic per build: §9.1's
    // call-site-stable tie-break is the same ordering claim). Stable
    // symbol-hash handler naming arrives with the #[component] manifest in
    // M2b; M2 handlers created through builders are headless-test scope.
    let n = AUTO_HANDLER_SEQ.fetch_add(1, Ordering::Relaxed);
    HandlerId::from_symbol(&format!("auto:{debug}#{n}"))
}

#[cfg(test)]
mod tests;
