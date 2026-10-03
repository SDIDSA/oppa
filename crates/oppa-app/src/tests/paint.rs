use super::*;

/// Clock-injected field rig (Round 15.2 — the headless blink
/// harness: a `MockClock` owner advances time explicitly, so flip
/// demand proves without sleeps).
fn clock_harness() -> (DesktopLoop, Signal<SharedString>, Rc<oppa::MockClock>) {
    let clock = Rc::new(oppa::MockClock::new());
    let loop_ = DesktopLoop::with_clock(200, 150, Box::new(FakeText), "Test", clock.clone())
        .expect("headless loop builds");
    let value = loop_.host().runtime().signal(SharedString::from(""));
    loop_.mount(
        "Field",
        FieldProps {
            value: value.clone(),
        },
        field_app,
    );
    (loop_, value, clock)
}

/// Round 15.2 (decision 313): selecting text in a focused field
/// commits themed selection `Rect`s in the loop's display list —
/// live frames ran `build_full` with the overlay `None`, so GPU
/// and GDI presented zero rects.
#[test]
fn desktop_repaint_commits_selection_rects() {
    let (mut loop_, _) = field_harness();
    focus_field(&mut loop_);
    loop_.type_text("hello world").expect("types");
    // Select-all through the session (collapsed caret → open
    // range), then commit exactly like the input path does.
    loop_
        .host()
        .focused_field_session()
        .expect("session")
        .select_all();
    loop_.repaint().expect("repaints");
    let plan = loop_.last_plan().expect("committed plan");
    let sels: Vec<(f32, f32, f32, f32)> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            oppa::DrawOp::Rect {
                x, y, w, h, color, ..
            } if *color == oppa::SELECTION_FILL => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect();
    assert_eq!(sels.len(), 1, "one line, one highlight, got {sels:?}");
    let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
        .into_iter()
        .next()
        .expect("field box");
    let origin_x = loop_.host().text_origin_under(field).expect("text origin");
    // 11 FakeText advances at 8.75px from the text origin.
    assert!(
        (sels[0].0 - origin_x).abs() < 1e-3 && (sels[0].2 - 96.25).abs() < 1e-2,
        "highlight pins the selected bytes, got {:?} vs origin {origin_x}",
        sels[0]
    );
    // One overlay at a time: the open range hides the caret bar.
    assert!(
        !plan.ops.iter().any(|op| matches!(op,
                oppa::DrawOp::Rect { w, color, .. }
                    if (*w - oppa::CARET_WIDTH_PX).abs() < 1e-3 && *color == oppa::INK)),
        "selected range paints no caret bar"
    );
}

/// Round 15.2 (decision 313): a focused collapsed caret commits
/// its 2px bar in the loop's display list (the same zero-bar gap
/// as the selection — one fix covers both overlays).
#[test]
fn desktop_repaint_commits_caret_bar() {
    let (mut loop_, _) = field_harness();
    focus_field(&mut loop_);
    // The tap reset the blink phase, and typing resets it again —
    // the bar is solid-visible at commit time (no clock needed
    // for the position pin).
    loop_.type_text("hello").expect("types");
    let plan = loop_.last_plan().expect("type_text repaints");
    let bars: Vec<(f32, f32, f32, f32)> = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            oppa::DrawOp::Rect {
                x, y, w, h, color, ..
            } if (*w - oppa::CARET_WIDTH_PX).abs() < 1e-3 && *color == oppa::INK => {
                Some((*x, *y, *w, *h))
            }
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 1, "exactly one caret bar, got {bars:?}");
    let field = oppa::find_retained_by_debug(loop_.host(), "field-box")
        .into_iter()
        .next()
        .expect("field box");
    let origin_x = loop_.host().text_origin_under(field).expect("text origin");
    // Trailing caret after five FakeText advances.
    assert!(
        (bars[0].0 - (origin_x + 5.0 * 8.75)).abs() < 1e-2,
        "bar at the trailing edge, got {:?} vs {}",
        bars[0],
        origin_x + 43.75
    );
    assert!(bars[0].3 > 0.0, "positive bar height");
}

/// Round 15.2 (decision 313): the blink flip demands a repaint
/// with no input — `caret_overlay_dirty` goes true on the hidden
/// half-cycle, `poll_blink` commits it (the bar leaves the plan),
/// and the tick horizon points at the next flip.
#[test]
fn blink_flip_demands_and_poll_repaints() {
    let (mut loop_, _, clock) = clock_harness();
    focus_field(&mut loop_);
    loop_.type_text("hi").expect("types");
    assert!(
        !loop_.caret_overlay_dirty(),
        "settled: snapshot matches the committed overlay"
    );
    let wake = loop_
        .blink_tick_in_secs()
        .expect("focused caret arms a tick");
    assert!(
        wake > 0.0 && wake <= 0.5,
        "horizon within the visible half, got {wake}"
    );
    clock.advance(0.6);
    assert!(loop_.caret_overlay_dirty(), "hidden flip demands a repaint");
    assert!(loop_.poll_blink().expect("polls"), "poll commits the flip");
    let plan = loop_.last_plan().expect("committed plan");
    assert!(
        !plan.ops.iter().any(|op| matches!(op,
                oppa::DrawOp::Rect { w, color, .. }
                    if (*w - oppa::CARET_WIDTH_PX).abs() < 1e-3 && *color == oppa::INK)),
        "hidden phase commits no bar"
    );
    assert!(!loop_.caret_overlay_dirty(), "snapshot follows the flip");
    assert!(
        !loop_.poll_blink().expect("polls"),
        "settled poll stays quiet"
    );
    let next = loop_.blink_tick_in_secs().expect("next flip armed");
    assert!(
        next > 0.3 && next <= 0.5,
        "wake points at the visible flip, got {next}"
    );
}
