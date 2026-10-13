pub const CONTRAST_INK: Color = Color(0xFF_FF_FF);

use oppa::{
    AlignItems, Color, Column, Ctx, Div, FontWeight, JustifyContent, Path, Row, Semantics,
    SharedString, Style, StyleBuilder, Text, TextClass, VNode,
};
use std::rc::Rc;

/// Shared press action (`Rc` so props stay `Clone` + `'static` for the
/// hot boundary — same shape as `OpaqueProps`' clone glue, §5.1).
pub type Action = Rc<dyn Fn()>;

/// Shared change notification (round 5.4, OQ-G2-4): controls that
/// flip author-owned signals internally also report the new value
/// through this (uncontrolled companions observe through it —
/// payload-carrying, because notify-without-value is useless when
/// the author holds no signal).
pub type Change<T> = Rc<dyn Fn(T)>;

pub fn action(f: impl Fn() + 'static) -> Action {
    Rc::new(f)
}

/// Decodes one SVG asset and deposits it into the shared image
/// cache at `px`×`px` (decision 478 — the baked-icon pipeline for
/// control icon slots: resvg rasterizes with full anti-aliasing,
/// so icons stay smooth on the un-antialiased GPU path; every
/// backend serves the same pixels from the one deposit). Returns
/// the cache id the `icon` props take. Refuses loudly (a broken
/// vendored asset is an authoring bug — broken icons never paint
/// blank; oversize targets hit the decoder's dims cap).
pub fn deposit_svg_icon(
    images: &oppa::ImageCache,
    key: &str,
    svg: &[u8],
    px: u32,
) -> oppa::ImageId {
    let img = oppa_image::decode_svg(svg, Some(px), Some(px)).unwrap_or_else(|e| {
        panic!("controls: icon {key:?} SVG decode failed ({e}) — refused, never a blank icon")
    });
    images.insert_pixels(key, img.width, img.height, img.rgba)
}

pub fn field_press(host: &oppa::ComponentHost, session: &oppa::EditSession, style: TextClass) {
    host.ensure_session_shaper(session, style);
    let count = host.last_press_click_count();
    let Some((px, _)) = host.last_press_position() else {
        session.caret_to_end();
        dirty_focused_field(host);
        return;
    };
    let Some(origin_x) = host.focused_node().and_then(|n| host.text_origin_under(n)) else {
        session.caret_to_end();
        dirty_focused_field(host);
        return;
    };
    let local_x = px - origin_x;
    if count >= 3 {
        session.select_line_x(local_x);
    } else if count == 2 {
        session.dbl_click_x(local_x);
    } else if host.last_press_modifiers().shift {
        session.shift_click_x(local_x);
    } else {
        session.click_x(local_x);
    }
    dirty_focused_field(host);
}

/// Hold-to-select for text fields (decision 448): a still hold
/// past the long-press deadline word-selects around the hold point
/// (the double-click rule — `dbl_click_x` — so hold and double-tap
/// agree exactly, the native long-press shape). The router publishes
/// the hold point as the tap point on fire and consumes the
/// following Up, so this is the gesture's only placement; the pump's
/// selection sync then raises the system toolbar. Quick taps never
/// reach here (Up removes the arm before the deadline).
/// Decision 459: a hold that lands on no word (empty field,
/// separator, past-the-end) arms the collapsed-caret toolbar instead
/// (Select-all + Paste — pasting needs no selection); composing
/// holds stay quiet (the platform owns the field mid-composition).
pub fn field_long_press(host: &oppa::ComponentHost, session: &oppa::EditSession, style: TextClass) {
    host.ensure_session_shaper(session, style);
    let Some((px, _)) = host.last_press_position() else {
        return;
    };
    let Some(origin_x) = host.focused_node().and_then(|n| host.text_origin_under(n)) else {
        return;
    };
    session.dbl_click_x(px - origin_x);
    let (a, b) = session.selection();
    if a == b && !session.is_composing() {
        host.arm_caret_toolbar();
    }
    dirty_focused_field(host);
}

/// Drag-selection stream for text fields (Round 8.2, decision 298):
/// the Down anchor through the live capture, both mapped into
/// session-local x around the field's text origin (`drag_x`). Moves
/// inside tap slop stream near-empty ranges that the tap Up collapses
/// (tap behavior unchanged); far Moves arm the drag release (no press
/// on Up — the router's Drag lifecycles, unchanged).
///
/// Decision 454: a confirmed handle grab streams the armed edge
/// instead (the router confirms past tap slop and marks the arm
/// consumed, so no tap/hold/swipe/release can double-dispatch —
/// this is the drag's only placement).
pub fn field_drag(host: &oppa::ComponentHost, session: &oppa::EditSession, style: TextClass) {
    // Decision 454: a handle candidate swallows the whole move
    // stream — confirmed drags move the armed edge, in-slop moves
    // are quiet no-ops (streaming `drag_x` under an unconfirmed grab
    // would clobber the anchor the edge drag keeps). Only a plain
    // (non-handle) Down streams `drag_x`.
    if let Some(edge) = host.handle_candidate_edge() {
        if host.handle_drag_edge().is_some() {
            field_handle_drag(host, session, style, edge);
        }
        return;
    }
    host.ensure_session_shaper(session, style);
    let (Some((ox, _)), Some((cx, _))) = (host.lowest_press_origin(), host.capture_position())
    else {
        return;
    };
    let Some(origin_x) = host.focused_node().and_then(|n| host.text_origin_under(n)) else {
        return;
    };
    session.drag_x(ox - origin_x, cx - origin_x);
    dirty_focused_field(host);
}

/// Renderer-drawn handle drag (decision 454): the live capture point
/// maps into session-local x around the field's text origin, then the
/// armed edge moves through `move_handle_edge` (caret follows the
/// dragged edge; the anchor never moves). Graceful no-ops (no shaper,
/// no capture, no laid text) mirror the `field_drag` rule — pointer
/// ops never panic setup state.
pub fn field_handle_drag(
    host: &oppa::ComponentHost,
    session: &oppa::EditSession,
    style: TextClass,
    edge: oppa::HandleEdge,
) {
    host.ensure_session_shaper(session, style);
    let Some((cx, _)) = host.capture_position() else {
        return;
    };
    let Some(origin_x) = host.focused_node().and_then(|n| host.text_origin_under(n)) else {
        return;
    };
    let Some(byte) = session.byte_at_x(cx - origin_x) else {
        return;
    };
    session.move_handle_edge(edge, byte);
    dirty_focused_field(host);
}

/// Marks the focused press owner PAINT-dirty (field selection/caret
/// writes carry no signals — see [`field_press`]).
pub fn dirty_focused_field(host: &oppa::ComponentHost) {
    if let Some(n) = host.focused_node() {
        host.mark_paint_dirty(&[n]);
    }
}

/// Sets the ink override on a converted text-leaf `VNode` (placeholder
/// dimming, active-tab ink). Converted leaves always arrive as
/// `VNode::Element`, so any other shape is a wiring bug and refused
/// loudly, never passed through unstyled.
///
/// Thin delegate over [`VNode::with_ink`](oppa::VNode::with_ink) (decision
/// 510 — the core now owns it, so hand-built `Text` chains can ink
/// inline via `Text::new(..).ink(..)` instead of wrapping here).
pub fn with_ink(vnode: VNode, ink: Color) -> VNode {
    vnode.with_ink(ink)
}

/// Keyboard-focus ring style (Round 23.2, decision 334): when this
/// instance owns focus AND the focus arrived via keyboard, outlines
/// the style with a 2px themed inset ring (paint-only `Border` —
/// never layout, so rings never move boxes or break oracles);
/// otherwise the style passes through untouched. Disabled controls
/// never ring (callers gate on enabled). Edge-banded styles keep
/// their bands (callers with `border_bottom` et al. branch around
/// the uniform-ring conflict, which plan-build refuses loudly).
pub fn focus_ringed(ctx: &Ctx, enabled: bool, style: StyleBuilder) -> StyleBuilder {
    if enabled && ctx.focused().get() && ctx.host().focus_visible().get() {
        style.border(2, ctx.theme().tokens().focus_ring)
    } else {
        style
    }
}

/// Form-validation marks (Phase 38a, decision 366 — G7): `invalid` /
/// `required` / `error_message` ride [`Semantics`] (validators stay
/// app-side — the payload only announces, the decision-352 shape);
/// `helper_text` is visual-only (never announced). Applies the three
/// announced marks onto a payload under construction.
pub fn with_validation_marks(
    base: Semantics,
    invalid: bool,
    required: bool,
    error_message: &Option<SharedString>,
) -> Semantics {
    let mut s = base.invalid(invalid).required(required);
    if let Some(msg) = error_message {
        s = s.error_message(msg);
    }
    s
}

/// Validation footer line (Phase 38a, decision 366): `error_message`
/// wins while `invalid`, else `helper_text`; `None` renders nothing
/// (callers skip the wrapper — valid controls keep byte-identical
/// trees). Error text paints in the theme `error` ink, helper text
/// in `text_secondary`; both at 12px (the chip-caption precedent).
pub fn validation_footer(
    ctx: &Ctx,
    debug: &str,
    invalid: bool,
    error_message: &Option<SharedString>,
    helper_text: &Option<SharedString>,
) -> Option<VNode> {
    let t = ctx.theme().tokens();
    let (text, ink) = match (invalid, error_message, helper_text) {
        (true, Some(msg), _) => (msg.clone(), t.error),
        (_, _, Some(help)) => (help.clone(), t.text_secondary),
        _ => return None,
    };
    // Debug label rides the caller's field (findable in dumps).
    let _ = debug;
    Some(with_ink(VNode::from(Text::new(text).size(12)), ink))
}

/// Wraps a control root with its validation footer (Phase 38a):
/// no message → the root returns unwrapped (existing oracles and
/// tab orders byte-identical); a message → `Div("<debug>-field")`
/// stacking the root over the footer line. The wrapper is a plain
/// block `Div` (vertical stack, no style — sizes to content).
pub fn with_validation_footer(
    root: VNode,
    ctx: &Ctx,
    debug: &str,
    invalid: bool,
    error_message: &Option<SharedString>,
    helper_text: &Option<SharedString>,
) -> VNode {
    let Some(foot) = validation_footer(ctx, debug, invalid, error_message, helper_text) else {
        return root;
    };
    Div(format!("{debug}-field").as_str()).children([root, foot])
}

/// Invalid field edge (Phase 38a): `error` ink while `invalid`,
/// else the theme `border` (valid controls keep exact pixels).
pub fn field_edge(ctx: &Ctx, invalid: bool) -> Color {
    if invalid {
        ctx.theme().tokens().error
    } else {
        ctx.theme().tokens().border
    }
}

// ---------------------------------------------------------------------------
// VectorIcon (decision 510 — the sera-login `IconSpec`, now shared)
// ---------------------------------------------------------------------------

/// One native vector icon: single-path SVG data scaled to a square
/// box with its paint ink (decision 291 `Path` leaves — filled
/// outlines suit the tessellator; every backend renders the same
/// pixels). `Clone`, so icon sets ride app state like
/// [`ImageCache`](oppa::ImageCache) ids do.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorIcon {
    pub debug: SharedString,
    pub data: SharedString,
    pub color: Color,
    pub size: f32,
}

impl VectorIcon {
    /// Parses single-path SVG markup (`include_str!` of a vendored
    /// `.svg`), scaling its coordinates to `target_size` (see
    /// [`Path::from_svg`](oppa::Path::from_svg)). Panics loudly on
    /// missing path data (never a blank icon).
    pub fn from_svg(debug: &str, svg: &str, target_size: f32, color: Color) -> Self {
        Self {
            debug: SharedString::from(debug),
            data: oppa::svg_path_data(svg, target_size, debug),
            color,
            size: target_size,
        }
    }

    /// Loads a single-path SVG file from disk and scales it to
    /// `target_size` (native desktop/test targets — the browser has
    /// no filesystem, so `include_str!` + [`from_svg`](Self::from_svg)
    /// is the portable spelling). Panics loudly on read or parse
    /// failure (never a blank icon).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_file(debug: &str, file_path: &str, target_size: f32, color: Color) -> Self {
        let content = std::fs::read_to_string(file_path).unwrap_or_else(|e| {
            panic!("controls: icon {debug:?} read failed at {file_path} ({e})")
        });
        Self::from_svg(debug, &content, target_size, color)
    }

    /// Renders the icon as a native `Path` leaf with an optional
    /// accessible label (`""` leaves semantics unset — decorative
    /// icons are never tab stops).
    pub fn leaf(&self, alt: &str) -> VNode {
        let mut vnode = Path::new(&self.debug)
            .data(&self.data)
            .fill(self.color)
            .size(self.size, self.size)
            .build();
        if !alt.is_empty() {
            if let VNode::Element(ref mut el) = vnode {
                el.semantics = Some(Semantics::default().label(alt));
            }
        }
        vnode
    }
}

// ---------------------------------------------------------------------------
// FieldChrome (decision 510 — the sera-login `credential_field`, now shared)
// ---------------------------------------------------------------------------

/// Composite input chrome (decision 480's focus-signal rule,
/// generalized): a labeled container box owning bg/border/radius/pads
/// around a borderless editing field, with the error line in a slot
/// below the box (a footer inside the horizontal chrome would
/// flex-squeeze beside the field — the `DatePicker` precedent).
/// Plain function vocabulary (`icon`/`field` are move-only `VNode`s,
/// so this is a builder, not a component — no hover/press state of
/// its own). Every dimension overrides; defaults reproduce the
/// sera-login measured chrome (12px left pad, 18px icon slot, 10px
/// gap, 14px vertical pads, 12px radius, 5px label inset, 8px
/// label-to-box rhythm).
pub struct FieldChrome {
    debug: String,
    label: SharedString,
    label_size: u16,
    label_ink: Option<Color>,
    chrome_bg: Option<Color>,
    edge: Option<Color>,
    edge_focused: Option<Color>,
    edge_invalid: Option<Color>,
    radius: f32,
    min_w: Option<f32>,
    reserve_error: bool,
    error_size: u16,
}

impl FieldChrome {
    pub fn new(debug: &str, label: impl Into<SharedString>) -> Self {
        Self {
            debug: debug.to_string(),
            label: label.into(),
            label_size: 14,
            label_ink: None,
            chrome_bg: None,
            edge: None,
            edge_focused: None,
            edge_invalid: None,
            radius: 12.0,
            min_w: Some(150.0),
            reserve_error: true,
            error_size: 12,
        }
    }

    pub fn label_size(mut self, px: u16) -> Self {
        self.label_size = px;
        self
    }

    pub fn label_ink(mut self, ink: Color) -> Self {
        self.label_ink = Some(ink);
        self
    }

    pub fn chrome_bg(mut self, bg: Color) -> Self {
        self.chrome_bg = Some(bg);
        self
    }

    /// Idle border ink (default: the theme `border`).
    pub fn edge(mut self, edge: Color) -> Self {
        self.edge = Some(edge);
        self
    }

    /// Border ink while the inner field owns focus (default: the
    /// theme `focus_ring`-adjacent primary — callers pass their
    /// focus blue explicitly; unset falls back to `edge`).
    pub fn edge_focused(mut self, edge: Color) -> Self {
        self.edge_focused = Some(edge);
        self
    }

    /// Border ink while an error shows (default: the theme `error`).
    pub fn edge_invalid(mut self, edge: Color) -> Self {
        self.edge_invalid = Some(edge);
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Minimum chrome width (default `Some(150)`; `None` lays out
    /// content-width).
    pub fn min_w(mut self, min_w: Option<f32>) -> Self {
        self.min_w = min_w;
        self
    }

    /// Whether the error slot always renders (default true: idle
    /// holds one space in the error ink, so showing an error never
    /// moves the form — the reserved-slot rule).
    pub fn reserve_error(mut self, reserve: bool) -> Self {
        self.reserve_error = reserve;
        self
    }

    pub fn error_size(mut self, px: u16) -> Self {
        self.error_size = px;
        self
    }

    /// Builds the chrome around `icon` (the 18px leading slot) and
    /// `field` (a borderless editing field taking exactly its text
    /// line). `error` paints below the box in a right-aligned slot;
    /// `focused` picks the focus edge.
    pub fn build(
        self,
        ctx: &Ctx,
        icon: VNode,
        field: VNode,
        error: Option<SharedString>,
        focused: bool,
    ) -> VNode {
        let t = ctx.theme().tokens();
        let slot = Row(format!("{}-slot", self.debug).as_str())
            .style(
                Style::new()
                    .square(18)
                    .h(20)
                    .align_items(AlignItems::Center)
                    .justify_content(JustifyContent::Center),
            )
            .child(icon);
        let edge_color = if focused {
            self.edge_focused.or(self.edge).unwrap_or(t.primary_pressed)
        } else if error.is_some() {
            self.edge_invalid.unwrap_or(t.error)
        } else {
            self.edge.unwrap_or(t.border)
        };
        let mut chrome_style = Style::new()
            .fill_width()
            .bg(self.chrome_bg.unwrap_or(t.surface))
            .radius(self.radius)
            .border(1.3, edge_color)
            .pad_top(14)
            .pad_bottom(14)
            .pad_left(12)
            .pad_right(12)
            .gap(10)
            .margin_top(8)
            .align_items(AlignItems::Center);
        if let Some(min_w) = self.min_w {
            chrome_style = chrome_style.min_w(min_w);
        }
        let chrome = Row(self.debug.as_str())
            .style(chrome_style)
            .children([slot, field]);
        let label_row = Row(format!("{}-labelrow", self.debug).as_str())
            .style(Style::new().pad_left(5))
            .child(with_ink(
                VNode::from(
                    Text::new(self.label.clone())
                        .size(self.label_size)
                        .weight(FontWeight::MEDIUM),
                ),
                self.label_ink.unwrap_or(t.text_secondary),
            ));
        let mut rows = vec![label_row, chrome];
        if let Some(slot) =
            field_error_slot(ctx, &self.debug, error, self.reserve_error, self.error_size)
        {
            rows.push(slot);
        }
        Column::new()
            .style(Style::new().fill_width().gap(0))
            .children(rows)
    }
}

/// The error slot below a field chrome: right-aligned, one step
/// below the label size. Always renders when `reserve` (idle holds
/// one space — a space paints no glyphs, so the slot keeps its exact
/// height invisibly); otherwise renders only while an error shows.
fn field_error_slot(
    ctx: &Ctx,
    debug: &str,
    error: Option<SharedString>,
    reserve: bool,
    size_px: u16,
) -> Option<VNode> {
    let t = ctx.theme().tokens();
    let message = match (error, reserve) {
        (Some(msg), _) => msg,
        (None, true) => SharedString::from(" "),
        (None, false) => return None,
    };
    Some(
        Row(format!("{debug}-err").as_str())
            .style(
                Style::new()
                    .fill_width()
                    .margin_top(4)
                    .pad_left(6)
                    .pad_bottom(4)
                    .justify_content(JustifyContent::End),
            )
            .child(with_ink(
                VNode::from(Text::new(message).size(size_px)),
                t.error,
            )),
    )
}
