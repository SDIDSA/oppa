// ---------------------------------------------------------------------------
// Scrollbar geometry (pure math, device px)
// ---------------------------------------------------------------------------

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
