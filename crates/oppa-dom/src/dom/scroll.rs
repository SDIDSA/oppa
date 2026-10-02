use super::*;

/// §9.3 v1 overscan constant: virtualized windows pad ±4 slots around
/// the visible range (window lag, not tearing — the offset trails the
/// browser by ≤ 1 frame, so overscan covers the stale frame).
pub const OVERSCAN_SLOTS: u32 = 4;

/// Visible row window with overscan (pure helper M8's virtualization
/// consumes): `(first, one_past_last)` row indices covering
/// `[offset, offset + viewport)` padded by [`OVERSCAN_SLOTS`],
/// clamped to `[0, row_count]`. Uniform rows (v1 scope).
pub fn scroll_window(
    offset_px: f32,
    row_h: f32,
    viewport_h: f32,
    row_count: usize,
) -> (usize, usize) {
    scroll_window_overscan(
        offset_px,
        row_h,
        viewport_h,
        row_count,
        OVERSCAN_SLOTS as usize,
    )
}

/// [`scroll_window`] with an explicit overscan (M8 overscan decision:
/// the sweep measures the repaint bound at the DESIGN-sketch `+2` and
/// the shipped `+4` through this helper; [`scroll_window`] stays the
/// one-constant production path).
pub fn scroll_window_overscan(
    offset_px: f32,
    row_h: f32,
    viewport_h: f32,
    row_count: usize,
    over: usize,
) -> (usize, usize) {
    if row_h <= 0.0 || row_count == 0 {
        return (0, 0);
    }
    let first = (offset_px / row_h).floor().max(0.0) as usize;
    let last = ((offset_px + viewport_h) / row_h).ceil().max(0.0) as usize;
    (first.saturating_sub(over), (last + over).min(row_count))
}

impl DomBackend {
    /// Browser-reported `scrollTop` for a scroll container (None = never
    /// observed — never a silent zero).
    pub fn scroll_top(&self, target: NodeId) -> Option<f32> {
        self.scroll_tops.get(&target).copied()
    }

    /// Records the browser's scroll position (§9.3 INPUT-mapping entry:
    /// the shell calls this when mapping the browser scroll event, then
    /// injects the corresponding `InputEvent::Scroll`). Returns the
    /// previous position (None on first sight — the delta mapper's input).
    pub fn note_browser_scroll(&mut self, target: NodeId, top_px: f32) -> Option<f32> {
        self.scroll_tops.insert(target, top_px)
    }
}
