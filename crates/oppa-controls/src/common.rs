pub const CONTRAST_INK: Color = Color(0xFF_FF_FF);

use oppa::{Color, Ctx, Div, Semantics, SharedString, StyleBuilder, Text, TextClass, VNode};
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
pub fn with_ink(vnode: VNode, ink: Color) -> VNode {
    match vnode {
        VNode::Element(mut e) => {
            e.style.ink = Some(ink);
            VNode::Element(e)
        }
        _ => panic!("ink target is not an element — refused, never silent"),
    }
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
