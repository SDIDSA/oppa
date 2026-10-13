use oppa::*;
use oppa_macros::Props;
// ---------------------------------------------------------------------------
// Scrollbar
// ---------------------------------------------------------------------------

/// Scrollbar props: the target `ScrollArea`'s debug label (unique
/// per area — geometry and hit-testing address it) plus the offset
/// signal shared with the content positioning (authors own one
/// signal driving rows and thumb together, the `scroll_app`
/// shape — `VirtualList`/`DataGrid` share their instance offset
/// with their attached overlay, see `vlist_scrollbar`).
#[derive(Clone, Props)]
pub struct ScrollbarProps {
    pub target: SharedString,
    pub offset: ScrollOffset,
    /// Wall-clock idle fade (Round 21.2, decision 329): `Some(ms)`
    /// hides the thumb `ms` after the last scroll or pointer event
    /// through the Round 21.1 timer hook (last event wins —
    /// re-renders re-arm); `None` keeps the event-driven hide only
    /// (visible until the next pointer move). `VirtualList` /
    /// `DataGrid` attach with `Some(1200)`.
    pub idle_hide_ms: Option<u64>,
    /// Scrollbar axis (Phase 36 PR2b, decision 354 — G15): `Vertical`
    /// (default) rides the `y` offset over `content_h`; `Horizontal`
    /// rides the `x` half over `content_w` (callers pass the 2D
    /// handle's `x()` view — same signal family, never a second
    /// source). Geometry, thumb, drag, and keys transpose exactly.
    pub axis: ScrollbarAxis,
}

/// Scrollbar axis (Phase 36 PR2b, decision 354 — G15): which offset
/// half and which content extent a [`Scrollbar`] overlay follows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ScrollbarAxis {
    #[default]
    Vertical,
    Horizontal,
}

/// Overlay scrollbar: track + draggable thumb over a `ScrollArea`
/// viewport (Round 17.2, decision 318; horizontal axis Phase 36 PR2b,
/// decision 354). Shows if and only if the content overflows on the
/// followed axis; the thumb follows the axis thumb math (`max(24,
/// viewport²/content)`, linear in the clamped offset); the track
/// page-scrolls on tap; thumb drags capture the pointer and map
/// through the linear content ratio; arrows page when focused
/// (Down/Up vertical, Right/Left horizontal).
/// Auto-hide fades the chrome out on disengage (hover-leave,
/// release) and back in on hover, press, or scroll writes — the
/// M8 transition evaluator animates both directions (first
/// controls use of `.transition(...)`, MockClock-deterministic).
/// Wall-clock idle fade rides `idle_hide_ms` (Round 21.2, decision
/// 329 — the per-component timer hook parks the visible thumb
/// after inactivity without another pointer move).
/// Theme tokens throughout (thumb in `text_secondary`, track in
/// `border` — both recolor with Light/Dark).
///
/// Structure: an unstyled root marker plus an anchored portal
/// (the Select-popup precedent — out-of-flow, overlay phase,
/// portal-priority hit-testing). The track owns every press
/// (thumb and rows stay handlerless so focus never fragments
/// mid-gesture — the Menu lesson); the press node spans a
/// transparent [`SCROLLBAR_HIT_PX`](oppa::SCROLLBAR_HIT_PX) gutter
/// with the painted 12px bar nested inside (follow-up — hover
/// summons without pixel-hunting; gutter presses page/drag
/// instead of reaching the covered strip); taps disambiguate by
/// thumb rect (thumb taps hold, dead padding dismisses nothing —
/// a tap is a tap, only pages move), drags map from a press-edge
/// snapshot (grab-preserving ratio, never a teleport).
pub fn Scrollbar(ctx: &Ctx, props: &ScrollbarProps) -> VNode {
    let closed = || {
        Portal("scrollbar-closed")
            .style(Style::new().size(0, 0))
            .build()
    };
    let host = ctx.host();
    let inst = ctx.instance_id();
    let t = ctx.theme().tokens();
    // Target geometry (settled layout — TRACKED, so the first
    // post-layout publish re-runs this body and mounts the chrome;
    // untracked `committed_box` would render closed pre-layout
    // forever. Absent pre-layout → closed).
    let target = host.settled_box_by_debug(&props.target);
    let Some(tb) = target else {
        return closed();
    };
    // Tracked reads (every one re-renders): offset, hover, press.
    // Phase 36 PR2b: axis-transposed locals — `pos` is the followed
    // offset half, `vp`/`content` the viewport/extent on that axis,
    // (`tpos`, `tlen`) the thumb origin/extent. One set of names for
    // both orientations (the layout thumb-twin precedent).
    let pos = props.offset.get();
    let (vp, content) = match props.axis {
        ScrollbarAxis::Vertical => (tb.h, tb.content_h),
        ScrollbarAxis::Horizontal => (tb.w, tb.content_w),
    };
    let max = oppa::scrollbar_max_offset(vp, content);
    let (tpos, tlen) = match props.axis {
        ScrollbarAxis::Vertical => match oppa::scrollbar_thumb(vp, content, pos) {
            Some(g) => (g.y, g.h),
            None => return closed(),
        },
        ScrollbarAxis::Horizontal => match oppa::scrollbar_thumb_x(vp, content, pos) {
            Some(g) => (g.x, g.w),
            None => return closed(),
        },
    };
    let hover = ctx.hovered();
    let down = ctx.pressed();
    // Scroll flash (guarded latch — settling never spins it):
    // offset edges show the thumb so wheel/programmatic scrolls
    // paint live; disengage runs hide it again.
    let flash = ctx.signal(false);
    let last_pos = ctx.signal(0.0f32);
    if pos != last_pos.get() {
        last_pos.set(pos);
        if !flash.get() {
            flash.set(true);
        }
    } else if props.idle_hide_ms.is_none() && !hover.get() && !down.get() && flash.get() {
        // Legacy event-driven hide only: without a wall-clock arm
        // the flash is a same-settle pulse (hover/press hold it).
        // With `idle_hide_ms` the timer below owns the hide, so a
        // programmatic scroll stays parked for the full budget
        // instead of dying on its own follow-up render.
        flash.set(false);
    }
    // Wall-clock idle fade (Round 21.2, decision 329): while the
    // flash is latched, arm a per-render timeout that drops it
    // `idle_hide_ms` after the last scroll or pointer event.
    // Re-renders re-arm (cleanup cancels the previous arm — last
    // event wins, so continuous scrolling never hides mid-gesture);
    // the fire clears only when neither hovered nor pressed (a
    // parked pointer keeps its chrome).
    if let Some(hide_ms) = props.idle_hide_ms {
        if flash.get() {
            let flash_hide = flash.clone();
            let hover_hide = hover.clone();
            let down_hide = down.clone();
            ctx.use_timeout(hide_ms as f64, move || {
                if !hover_hide.get() && !down_hide.get() {
                    flash_hide.set(false);
                }
            });
        }
    }
    let visible = hover.get() || down.get() || flash.get();
    // Drag snapshot on the press edge (grab-preserving ratio):
    // base offset + press-origin y, plus whether the grab started
    // on the thumb (track grabs page on release instead).
    let base = ctx.signal((0.0f32, 0.0f32));
    let grabbed = ctx.signal(false);
    let was_down = ctx.signal(false);
    let down_now = down.get();
    if down_now && !was_down.get() {
        let origin = host.lowest_press_origin();
        // Thumb hit-test transposed by axis (gutter strip on the
        // trailing edge; thumb rect inside it).
        let on_thumb = origin.is_some_and(|(ox, oy)| match props.axis {
            ScrollbarAxis::Vertical => {
                let gutter_x = tb.x + tb.w - SCROLLBAR_HIT_PX;
                ox >= gutter_x
                    && ox < gutter_x + SCROLLBAR_HIT_PX
                    && oy >= tb.y + tpos
                    && oy < tb.y + tpos + tlen
            }
            ScrollbarAxis::Horizontal => {
                let gutter_y = tb.y + tb.h - SCROLLBAR_HIT_PX;
                oy >= gutter_y
                    && oy < gutter_y + SCROLLBAR_HIT_PX
                    && ox >= tb.x + tpos
                    && ox < tb.x + tpos + tlen
            }
        });
        // Drag snapshot on the press edge (grab-preserving ratio):
        // base offset + press-origin coordinate on the scroll axis,
        // falling back to the thumb origin for keyboard presses.
        let fallback = match props.axis {
            ScrollbarAxis::Vertical => tb.y + tpos,
            ScrollbarAxis::Horizontal => tb.x + tpos,
        };
        let origin_coord = origin
            .map(|(ox, oy)| match props.axis {
                ScrollbarAxis::Vertical => oy,
                ScrollbarAxis::Horizontal => ox,
            })
            .unwrap_or(fallback);
        base.set((pos, origin_coord));
        if grabbed.get() != on_thumb {
            grabbed.set(on_thumb);
        }
    }
    if was_down.get() != down_now {
        was_down.set(down_now);
    }
    // Slider role with a human percentage plus the numeric value
    // (Phase 36 PR2b: the G18 RangeValue half rides along — UIA
    // RangeValue, AT-SPI Value, DOM aria-valuenow/min/max; the OQ-G2-2
    // gap stays closed here too).
    let pct = if max <= 0.0 {
        0
    } else {
        (pos.clamp(0.0, max) / max * 100.0).round() as u32
    };
    let fade = Transition::new(150, Ease::Out);
    let track_opacity = if visible { 0.3 } else { 0.0 };
    let thumb_opacity = if visible { 1.0 } else { 0.0 };
    let offset = props.offset.clone();
    let axis = props.axis;
    // Hit gutter (follow-up): the press/hover node spans
    // SCROLLBAR_HIT_PX over the content's trailing edge (right for
    // vertical, bottom for horizontal) while the painted bar + thumb
    // keep the 12px chrome — summoning no longer needs
    // pixel-hunting. The gutter is transparent (no bg → paints
    // nothing → plan rects byte-identical); presses inside it
    // page/drag instead of reaching the covered content strip.
    let track_style = match axis {
        ScrollbarAxis::Vertical => Style::new()
            .x(tb.w - SCROLLBAR_HIT_PX)
            .w(SCROLLBAR_HIT_PX)
            .h(tb.h)
            .build(),
        ScrollbarAxis::Horizontal => Style::new()
            .absolute_y(tb.h - SCROLLBAR_HIT_PX)
            .w(tb.w)
            .h(SCROLLBAR_HIT_PX)
            .build(),
    };
    let track = Div(format!("scrollbar-track-{inst}").as_str())
        .style(track_style)
        .semantics(
            Semantics::slider()
                .label("scrollbar")
                .value_text(format!("{pct} percent").as_str())
                .value_num(pos)
                .min_value(0.0)
                .max_value(max),
        )
        .on_press({
            let host = host.clone();
            let offset = offset.clone();
            let target = props.target.clone();
            move || {
                // Fresh geometry (event-time boxes — immune to body
                // staleness, the Slider precedent). Keyboard Enter
                // (no tap point) holds the thumb; thumb taps hold;
                // track taps page by viewport, clamped.
                let Some(tb) = oppa::find_retained_by_debug(&host, &target)
                    .into_iter()
                    .next()
                    .and_then(|id| host.committed_box(id))
                else {
                    return;
                };
                let pos = offset.get();
                let (vp, content) = match axis {
                    ScrollbarAxis::Vertical => (tb.h, tb.content_h),
                    ScrollbarAxis::Horizontal => (tb.w, tb.content_w),
                };
                let max = oppa::scrollbar_max_offset(vp, content);
                let (tpos, tlen) = match axis {
                    ScrollbarAxis::Vertical => match oppa::scrollbar_thumb(vp, content, pos) {
                        Some(g) => (g.y, g.h),
                        None => return,
                    },
                    ScrollbarAxis::Horizontal => match oppa::scrollbar_thumb_x(vp, content, pos) {
                        Some(g) => (g.x, g.w),
                        None => return,
                    },
                };
                let Some((px, py)) = host.last_press_position() else {
                    return;
                };
                let (coord, start, len, extent) = match axis {
                    ScrollbarAxis::Vertical => (py, tb.y + tpos, tlen, tb.h),
                    ScrollbarAxis::Horizontal => (px, tb.x + tpos, tlen, tb.w),
                };
                if coord >= start && coord < start + len {
                    return;
                }
                let npos = if coord < start {
                    pos - extent
                } else {
                    pos + extent
                }
                .clamp(0.0, max);
                if npos != pos {
                    offset.set(npos);
                }
            }
        })
        .on_drag({
            let host = host.clone();
            let offset = offset.clone();
            let target = props.target.clone();
            let base = base.clone();
            let grabbed = grabbed.clone();
            move || {
                if !grabbed.get() {
                    return;
                }
                let Some((cx, cy)) = host.capture_position() else {
                    return;
                };
                let Some(tb) = oppa::find_retained_by_debug(&host, &target)
                    .into_iter()
                    .next()
                    .and_then(|id| host.committed_box(id))
                else {
                    return;
                };
                let (vp, content, coord) = match axis {
                    ScrollbarAxis::Vertical => (tb.h, tb.content_h, cy),
                    ScrollbarAxis::Horizontal => (tb.w, tb.content_w, cx),
                };
                let max = oppa::scrollbar_max_offset(vp, content);
                let tlen = match axis {
                    ScrollbarAxis::Vertical => {
                        match oppa::scrollbar_thumb(vp, content, offset.get()) {
                            Some(g) => g.h,
                            None => return,
                        }
                    }
                    ScrollbarAxis::Horizontal => {
                        match oppa::scrollbar_thumb_x(vp, content, offset.get()) {
                            Some(g) => g.w,
                            None => return,
                        }
                    }
                };
                let travel = vp - tlen;
                if travel <= 0.0 {
                    return;
                }
                let (base_off, base_coord) = base.get();
                let npos = (base_off + (coord - base_coord) * max / travel).clamp(0.0, max);
                if npos != offset.get() {
                    offset.set(npos);
                }
            }
        });
    // Arrow pages by viewport (fresh max, guarded write — end
    // presses never spin): Down/Up on the vertical axis,
    // Right/Left on the horizontal one.
    let page_fwd: std::rc::Rc<dyn Fn()> = {
        let host = host.clone();
        let offset = offset.clone();
        let target = props.target.clone();
        std::rc::Rc::new(move || {
            let Some(tb) = oppa::find_retained_by_debug(&host, &target)
                .into_iter()
                .next()
                .and_then(|id| host.committed_box(id))
            else {
                return;
            };
            let (vp, content) = match axis {
                ScrollbarAxis::Vertical => (tb.h, tb.content_h),
                ScrollbarAxis::Horizontal => (tb.w, tb.content_w),
            };
            let max = oppa::scrollbar_max_offset(vp, content);
            let npos = (offset.get() + vp).clamp(0.0, max);
            if npos != offset.get() {
                offset.set(npos);
            }
        })
    };
    let page_back: std::rc::Rc<dyn Fn()> = {
        let host = host.clone();
        let offset = offset.clone();
        let target = props.target.clone();
        std::rc::Rc::new(move || {
            let Some(tb) = oppa::find_retained_by_debug(&host, &target)
                .into_iter()
                .next()
                .and_then(|id| host.committed_box(id))
            else {
                return;
            };
            let (vp, content) = match axis {
                ScrollbarAxis::Vertical => (tb.h, tb.content_h),
                ScrollbarAxis::Horizontal => (tb.w, tb.content_w),
            };
            let max = oppa::scrollbar_max_offset(vp, content);
            let npos = (offset.get() - vp).clamp(0.0, max);
            if npos != offset.get() {
                offset.set(npos);
            }
        })
    };
    let track = match axis {
        ScrollbarAxis::Vertical => {
            let fwd = page_fwd.clone();
            let back = page_back.clone();
            track.on_key_down(move || fwd()).on_key_up(move || back())
        }
        ScrollbarAxis::Horizontal => {
            let fwd = page_fwd.clone();
            let back = page_back.clone();
            track
                .on_key_right(move || fwd())
                .on_key_left(move || back())
        }
    };
    let (bar_style, thumb_style) = match axis {
        ScrollbarAxis::Vertical => (
            Style::new()
                .x(SCROLLBAR_HIT_PX - SCROLLBAR_TRACK_PX)
                .w(SCROLLBAR_TRACK_PX)
                .h(tb.h)
                .bg(t.border)
                .opacity(Some(track_opacity))
                .transition(fade),
            Style::new()
                .x(SCROLLBAR_HIT_PX - SCROLLBAR_TRACK_PX)
                .w(SCROLLBAR_TRACK_PX)
                .h(tlen)
                .absolute_y(tpos)
                .bg(t.text_secondary)
                .opacity(Some(thumb_opacity))
                .transition(fade),
        ),
        ScrollbarAxis::Horizontal => (
            Style::new()
                .absolute_y(SCROLLBAR_HIT_PX - SCROLLBAR_TRACK_PX)
                .h(SCROLLBAR_TRACK_PX)
                .w(tb.w)
                .bg(t.border)
                .opacity(Some(track_opacity))
                .transition(fade),
            Style::new()
                .absolute_y(SCROLLBAR_HIT_PX - SCROLLBAR_TRACK_PX)
                .h(SCROLLBAR_TRACK_PX)
                .w(tlen)
                .x(tpos)
                .bg(t.text_secondary)
                .opacity(Some(thumb_opacity))
                .transition(fade),
        ),
    };
    let track = track.children([
        Div(format!("scrollbar-trackbar-{inst}").as_str())
            .style(bar_style)
            .build(),
        Div(format!("scrollbar-thumb-{inst}").as_str())
            .style(thumb_style)
            .build(),
    ]);
    // Root origin for the portal base (unstyled marker — content
    // origin is the box origin; tracked like the target so resizes
    // reposition the overlay).
    let root_label = format!("scrollbar-{inst}");
    let (rx, ry) = oppa::find_retained_by_debug(&host, &root_label)
        .into_iter()
        .filter_map(|id| host.settled_box(id))
        .next()
        .map(|b| (b.x, b.y))
        .unwrap_or((0.0, 0.0));
    Div(root_label.as_str()).child(
        Portal("scrollbar-popup")
            .style(Style::new().x(tb.x - rx).absolute_y(tb.y - ry).w(tb.w))
            .child(track),
    )
}
