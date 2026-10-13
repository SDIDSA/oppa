//! Element builders: the chainable `Div("track").style(...).child(...)` authoring surface.

#![allow(non_snake_case)]

use std::cell::RefCell;

use crate::semantics::Semantics;
use crate::shell::EventKind;
use crate::style::Style;

use super::{auto_handler_id, Element, HandlerAttachment, Tag, VNode};

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
                canvas: None,
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

    /// Text-ink override for `Text` leaves under this node (decision
    /// 510 — the builder-side `with_ink`: falls back to the build
    /// theme's `text_primary` when absent, the `resolve_ink` rule).
    pub fn ink(mut self, color: crate::style::Color) -> Self {
        self.inner.style.ink = Some(color);
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

    /// Home-key handler (Phase 38a, decision 367): fired by the
    /// router when the focused owner declares one (jump to range
    /// minimum on Slider); otherwise the generic Key handler runs
    /// (the arrow precedent), else quiet. Requires focus.
    pub fn on_key_home(mut self, f: impl Fn() + 'static) -> Self {
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::KeyHome,
            id: auto_handler_id(&self.inner.debug),
            pending: RefCell::new(Some(Box::new(f))),
            owner: std::cell::Cell::new(None),
        });
        self
    }

    /// End-key handler (Phase 38a, decision 367 — see
    /// [`ElementBuilder::on_key_home`]; jumps to range maximum).
    pub fn on_key_end(mut self, f: impl Fn() + 'static) -> Self {
        self.inner.handlers.push(HandlerAttachment {
            kind: EventKind::KeyEnd,
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
