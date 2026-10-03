use super::*;
use oppa::ComponentHost;

/// Thumb + track rects by geometry (track spans the viewport
/// height at the target's right edge; thumb pins the brief
/// ratio) — color identifies the part only in dark mode, so
/// geometry leads everywhere.
fn chrome_rects(plan: &oppa::FramePlan) -> ChromePair {
    let mut track = Vec::new();
    let mut thumb = Vec::new();
    for op in &plan.ops {
        if let oppa::DrawOp::Rect { x, y, w, h, .. } = op {
            if approx(*x, 188.0) && approx(*w, 12.0) {
                if approx(*y, 0.0) && approx(*h, 200.0) {
                    track.push((*x, *y, *w, *h));
                } else {
                    thumb.push((*x, *y, *w, *h));
                }
            }
        }
    }
    (track, thumb)
}

fn track_tap(host: &ComponentHost, x: f32, y: f32) {
    tap_at(host, x, y, false);
}

/// Round 17.2: no overflow renders no chrome; overflow idles
/// hidden (opacity targets zero paint nothing — presence reads
/// the plan, never node labels).
#[test]
fn scrollbar_hidden_without_overflow_and_when_idle() {
    // Content fits: no chrome rects anywhere in the plan.
    let (host, _) = scrollbar_harness(100.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects(&plan);
    assert!(track.is_empty() && thumb.is_empty(), "fits: paints nothing");
    // Overflow, never touched: the raw plan carries no chrome rects.
    let (host, _) = scrollbar_harness(600.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects(&plan);
    assert!(track.is_empty() && thumb.is_empty(), "idle paints nothing");
}

/// Follow-up: hovering the transparent gutter outside the
/// painted 12px bar (but inside the 20px hit node) summons the
/// chrome — no pixel-hunting the bar itself.
#[test]
fn scrollbar_shows_on_gutter_hover_outside_painted_bar() {
    let (host, _) = scrollbar_harness(600.0);
    hover(&host, 184.0, 100.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects(&plan);
    assert_eq!(track.len(), 1, "gutter hover paints track, got {track:?}");
    assert_eq!(thumb.len(), 1, "gutter hover paints thumb, got {thumb:?}");
    // Painted chrome keeps the 12px geometry (the gutter paints
    // nothing itself).
    assert!(approx(track[0].0, 188.0) && approx(track[0].2, 12.0));
}

/// Round 17.2: hover fades the chrome in, leave fades it out
/// (raw targets flip immediately; the transition animates).
#[test]
fn scrollbar_shows_on_hover_and_hides_on_leave() {
    let (host, _) = scrollbar_harness(600.0);
    hover(&host, 194.0, 100.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects(&plan);
    assert_eq!(track.len(), 1, "track paints, got {track:?}");
    assert_eq!(thumb.len(), 1, "thumb paints, got {thumb:?}");
    assert!(
        approx(thumb[0].3, 200.0 * 200.0 / 600.0),
        "thumb pins vp²/c, got {:?}",
        thumb[0]
    );
    hover(&host, 350.0, 290.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = chrome_rects(&plan);
    assert!(track.is_empty() && thumb.is_empty(), "leave hides");
}

/// Round 17.2 (the brief verification, exactly): dragging the
/// thumb from top to bottom scrolls the content to 100% of its
/// extent — and back again.
#[test]
fn thumb_drag_top_to_bottom_scrolls_to_max() {
    let (host, offset) = scrollbar_harness(600.0);
    // Thumb parks 0..66.67 at rest; grab its top, drag to the
    // track bottom: ratio 400/133.33 lands exactly on max.
    drag(&host, 194.0, 5.0, 194.0, 199.0);
    assert!(
        approx(offset.get(), 400.0),
        "100% extent, got {}",
        offset.get()
    );
    let plan = scrollbar_plan(&host);
    let (_, thumb) = chrome_rects(&plan);
    assert_eq!(thumb.len(), 1, "thumb paints after drag");
    assert!(
        approx(thumb[0].1, (200.0f32 - 200.0 * 200.0 / 600.0).round()),
        "thumb parks at travel, got {:?}",
        thumb[0]
    );
    // And back: grab the bottom-parked thumb, drag to top.
    drag(&host, 194.0, 195.0, 194.0, 2.0);
    assert!(
        approx(offset.get(), 0.0),
        "back to top, got {}",
        offset.get()
    );
}

/// Round 17.2: track taps page by viewport, clamped at both
/// ends; thumb taps hold (dragging is the gesture).
#[test]
fn track_click_pages_and_clamps() {
    let (host, offset) = scrollbar_harness(600.0);
    // Below the rest thumb (0..66.67): +200.
    track_tap(&host, 194.0, 150.0);
    assert!(
        approx(offset.get(), 200.0),
        "pages down, got {}",
        offset.get()
    );
    // Above the moved thumb (66.67..133.33): −200.
    track_tap(&host, 194.0, 10.0);
    assert!(approx(offset.get(), 0.0), "pages up, got {}", offset.get());
    // Clamp: paging past the end parks on max, never beyond.
    track_tap(&host, 194.0, 150.0);
    track_tap(&host, 194.0, 190.0);
    assert!(
        approx(offset.get(), 400.0),
        "clamps on max, got {}",
        offset.get()
    );
    track_tap(&host, 194.0, 190.0);
    assert!(approx(offset.get(), 400.0), "stays parked");
    // Thumb tap holds (no page, no invoke — dragging owns it).
    track_tap(&host, 194.0, 170.0);
    assert!(approx(offset.get(), 400.0), "thumb tap holds");
}

// ------------------------------------------------------------------
// Round 21.2 (decision 329): VirtualList/DataGrid attachment +
// wall-clock idle fade.
// ------------------------------------------------------------------
