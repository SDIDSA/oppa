use super::*;
use oppa::input::{keys, InputEvent, KeyState};
use oppa::{find_retained_by_debug, ComponentHost, SharedString};

fn field_tap_point(host: &ComponentHost, debug: &str, local_x: f32) -> (f32, f32) {
    let field = node_by_debug(host, debug);
    let origin_x = host
        .text_origin_under(field)
        .expect("field lays a text origin");
    let b = host.committed_box(field).expect("field has a hit box");
    (origin_x + local_x, b.y + b.h / 2.0)
}

#[test]
fn text_input_tap_places_caret_at_cluster_boundaries() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hello"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    // Origin tap → byte 0.
    let (x0, y0) = field_tap_point(&host, "text-input", 0.0);
    tap_at(&host, x0, y0, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.caret(), 0);
    assert_eq!(sess.selection(), (0, 0));
    // Mid-word (x=20: third cluster [17.5,26.25) leading half) → byte 2.
    let (xm, ym) = field_tap_point(&host, "text-input", 20.0);
    tap_at(&host, xm, ym, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.caret(), 2);
    assert_eq!(sess.selection(), (2, 2));
    // Past the end (5 × 8.75 = 43.75 + 10) → byte 5.
    let (xe, ye) = field_tap_point(&host, "text-input", 53.75);
    tap_at(&host, xe, ye, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.caret(), 5);
    assert_eq!(sess.selection(), (5, 5));
}

#[test]
fn text_input_shift_click_extends_selection() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hello"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    let (x0, y0) = field_tap_point(&host, "text-input", 0.0);
    tap_at(&host, x0, y0, false);
    assert_eq!(host.focused_field_session().expect("session").caret(), 0);
    let (xe, ye) = field_tap_point(&host, "text-input", 53.75);
    tap_at(&host, xe, ye, true);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.selection(), (0, 5));
    assert_eq!(sess.caret(), 5);
}

#[test]
fn text_input_keyboard_activation_parks_at_end() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hello"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    let (x0, y0) = field_tap_point(&host, "text-input", 0.0);
    tap_at(&host, x0, y0, false);
    assert_eq!(host.focused_field_session().expect("session").caret(), 0);
    let field = node_by_debug(&host, "text-input");
    host.inject_input(InputEvent::Focus { node: Some(field) });
    host.run_until_idle();
    host.inject_input(InputEvent::key(keys::ENTER, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(
        host.focused_field_session().expect("session").caret(),
        5,
        "keyboard press falls back to end (no tap point)"
    );
}

#[test]
fn text_area_tap_places_caret() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hi"));
    host.mount("TA", TextAreaProps::new("Notes", value.clone()), TextArea);
    host.run_until_idle();
    let (x0, y0) = field_tap_point(&host, "text-area", 0.0);
    tap_at(&host, x0, y0, false);
    assert_eq!(host.focused_field_session().expect("session").caret(), 0);
    // "hi" = 2 × 8.75 = 17.5 wide; +10 lands past the end → byte 2.
    let (xe, ye) = field_tap_point(&host, "text-area", 27.5);
    tap_at(&host, xe, ye, false);
    assert_eq!(host.focused_field_session().expect("session").caret(), 2);
}

/// Round 8.2 (decision 298): pointer drag streams `drag_x` — Down
/// at byte 2, Moves to byte 8, Up classifies Drag (no press) and
/// the session holds `(2, 8)` with the caret at 8.
fn drag_at(host: &ComponentHost, x1: f32, y: f32, x2: f32) {
    use oppa::{PointerAction, PointerButton};
    let moves = [x1 + (x2 - x1) * 0.5, x2];
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: PointerAction::Down {
            button: PointerButton::Primary,
        },
        x: x1,
        y,
        modifiers: oppa::Modifiers::NONE,
    });
    for x in moves {
        host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: PointerAction::Move,
            x,
            y,
            modifiers: oppa::Modifiers::NONE,
        });
    }
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: PointerAction::Up {
            button: PointerButton::Primary,
        },
        x: x2,
        y,
        modifiers: oppa::Modifiers::NONE,
    });
    host.run_until_idle();
}

/// Round 8.2 (decision 298): drag byte 2→8 selects `(2, 8)` and the
/// shared plan carries one themed background Rect with exact bounds
/// (what CPU and Vello both paint — no backend selection logic).
#[test]
fn text_input_drag_selects_range_with_exact_background_bounds() {
    use oppa::{DrawOp, SELECTION_FILL};
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hello world"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    let field = node_by_debug(&host, "text-input");
    let origin_x = host.text_origin_under(field).expect("text origin");
    let y = host.committed_box(field).expect("hit box").y + 16.0;
    // Byte b lands in the leading half: b × 8.75 + 2.0.
    drag_at(
        &host,
        origin_x + 2.0 * 8.75 + 2.0,
        y,
        origin_x + 8.0 * 8.75 + 2.0,
    );
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.selection(), (2, 8));
    assert_eq!(sess.caret(), 8);
    // Plan bounds: caret_x(2) = 17.5 → caret_x(8) = 70.0.
    let sel = host.focused_selection_paint().expect("paint selection");
    assert_eq!(sel.range, (2, 8));
    let builder = oppa_cpu::FramePlanBuilder::new(1.0);
    builder.set_selection(Some(sel));
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let rects: Vec<(f32, f32, f32, f32)> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rect {
                x, y, w, h, color, ..
            } if *color == SELECTION_FILL => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect();
    assert_eq!(rects.len(), 1, "one background rect, got {rects:?}");
    let (rx, ry, rw, rh) = rects[0];
    assert!(
        approx(rx, origin_x + 17.5) && approx(rw, 52.5),
        "x/w pin cluster edges, got ({rx}, {rw})"
    );
    // y/h pin the laid line (lines live on the inner "text" leaf —
    // the "field" element carries the box origin, not the lines).
    let laid = find_retained_by_debug(&host, "text")
        .into_iter()
        .filter_map(|id| host.committed_box(id))
        .find(|b| !b.lines.is_empty())
        .expect("laid text leaf");
    let expect = laid.selection_rects((2, 8));
    assert_eq!(expect.len(), 1, "one laid line overlaps");
    assert!(
        approx(ry, expect[0][1]) && approx(rh, expect[0][3] - expect[0][1]),
        "y/h pin the laid line, got ({ry}, {rh}) laid {laid:?}"
    );
    // Collapsing the selection removes the Rect incrementally (the
    // handler dirties the field — no stale highlight survives).
    let (x0, _) = field_tap_point(&host, "text-input", 0.0);
    tap_at(&host, x0, y, false);
    assert_eq!(host.focused_field_session().expect("session").caret(), 0);
    let builder = oppa_cpu::FramePlanBuilder::new(1.0);
    builder.set_selection(host.focused_selection_paint());
    let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    assert!(
        !plan.ops.iter().any(|op| matches!(op,
                DrawOp::Rect { color, .. } if *color == SELECTION_FILL)),
        "collapsed selection paints no highlight"
    );
}

/// Round 15.1 (decision 312): a focused field emits a 2px caret
/// bar at the active cluster's leading edge, and moving the caret
/// repositions the bar (the shared plan's caret `Rect` — what CPU
/// and Vello both paint). A non-collapsed selection suppresses
/// the bar (the highlight paints instead — one overlay at a time).
#[test]
fn text_input_focused_caret_paints_two_px_bar_and_repositions() {
    use oppa::{CaretPaint, DrawOp, CARET_WIDTH_PX};
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hello world"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    // Tap byte 0 → collapsed caret at the leading edge (the tap
    // resets the blink phase, so the bar is solid-visible here —
    // no clock control needed for the position pin).
    let field = node_by_debug(&host, "text-input");
    let origin_x = host.text_origin_under(field).expect("text origin");
    let y = host.committed_box(field).expect("hit box").y + 16.0;
    let (x0, _) = field_tap_point(&host, "text-input", 0.0);
    tap_at(&host, x0, y, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.caret(), 0);
    assert_eq!(sess.selection(), (0, 0));
    let caret: CaretPaint = host.focused_caret_paint().expect("visible caret bar");
    assert_eq!(caret.field, field);
    assert!(
        approx(caret.x, origin_x),
        "bar at the leading edge of byte 0, got {} vs origin {origin_x}",
        caret.x
    );
    assert!(caret.h > 0.0, "positive bar height, got {}", caret.h);
    let builder = oppa_cpu::FramePlanBuilder::new(1.0);
    builder.set_caret(host.focused_caret_paint());
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    let bars: Vec<(f32, f32, f32, f32)> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rect {
                x, y, w, h, color, ..
            } if *color == caret.color && approx(*w, CARET_WIDTH_PX) => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 1, "exactly one caret bar, got {bars:?}");
    assert!(
        approx(bars[0].0, origin_x) && approx(bars[0].1, caret.y) && approx(bars[0].3, caret.h),
        "bar pins the resolved caret box, got {:?} vs ({}, {}, {})",
        bars[0],
        caret.x,
        caret.y,
        caret.h
    );
    // Move the caret to byte 5 → the bar repositions by exactly
    // five FakeText advances (8.75px/char).
    let (x5, _) = field_tap_point(&host, "text-input", 5.0 * 8.75 + 2.0);
    tap_at(&host, x5, y, false);
    assert_eq!(host.focused_field_session().expect("session").caret(), 5);
    let moved = host.focused_caret_paint().expect("visible caret bar");
    assert!(
        approx(moved.x, origin_x + 5.0 * 8.75),
        "bar at the leading edge of byte 5, got {} vs {}",
        moved.x,
        origin_x + 5.0 * 8.75
    );
    // Drag-selecting a range hides the bar (highlight instead).
    drag_at(
        &host,
        origin_x + 2.0 * 8.75 + 2.0,
        y,
        origin_x + 8.0 * 8.75 + 2.0,
    );
    assert_eq!(
        host.focused_field_session().expect("session").selection(),
        (2, 8)
    );
    assert!(
        host.focused_caret_paint().is_none(),
        "selected range paints the highlight, not the bar"
    );
}

/// Round 8.2 (decision 298): two fast taps on "world" select the
/// word (`dbl_click_x` — bytes 6..11), three select the hard line.
#[test]
fn text_input_double_click_selects_word_triple_selects_line() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("hello world"));
    host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
    host.run_until_idle();
    // Byte 7 (in "world"): 7 × 8.75 + 2.0 local.
    let (x, y) = field_tap_point(&host, "text-input", 7.0 * 8.75 + 2.0);
    tap_at(&host, x, y, false);
    tap_at(&host, x, y, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.selection(), (6, 11), "double-click selects the word");
    assert_eq!(sess.caret(), 11);
    tap_at(&host, x, y, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(sess.selection(), (0, 11), "triple-click selects the line");
}

/// Round 8.2 (decision 298): triple-click in a TextArea selects the
/// hard `\n` line under the pointer, not the whole value.
#[test]
fn text_area_triple_click_selects_hard_line() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(SharedString::from("ab\ncd"));
    host.mount("TA", TextAreaProps::new("Notes", value.clone()), TextArea);
    host.run_until_idle();
    // Byte 3 ("c"): 3 × 8.75 + 2.0 local.
    let (x, y) = field_tap_point(&host, "text-area", 3.0 * 8.75 + 2.0);
    tap_at(&host, x, y, false);
    tap_at(&host, x, y, false);
    tap_at(&host, x, y, false);
    let sess = host.focused_field_session().expect("session");
    assert_eq!(
        sess.selection(),
        (3, 5),
        "triple-click selects the hard line"
    );
}
