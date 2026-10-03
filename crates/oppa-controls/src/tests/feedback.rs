use super::*;
use oppa::input::{keys, InputEvent, KeyState};
use oppa::{
    find_retained_by_debug, Color, ComponentHost, Ctx, Div, Props, ScrollOffset, SharedString,
    Style, Text, VNode,
};
use oppa_macros::Props;

// Phase 36 PR2b (decision 354): horizontal scrollbar axis.
// ------------------------------------------------------------------

/// Horizontal rig: hand-rolled `ScrollArea` (200×200 at the
/// origin, content `content_w`×200) plus a horizontal `Scrollbar`
/// sharing the rig's x-half offset (the 2D `scroll_2d` handle's
/// `x()` view — same signal family, never a second source).
#[derive(Clone)]
struct HScrollRigProps {
    content_w: f32,
}
impl Props for HScrollRigProps {}

fn hscroll_rig(ctx: &Ctx, p: &HScrollRigProps) -> VNode {
    let offset = ctx.scroll_2d();
    Div("hscroll-screen")
        .style(Style::new().size(400, 300))
        .children([
            oppa::ScrollArea("hscroll-list")
                .style(Style::new().size(200, 200).content_size(p.content_w))
                .on_scroll(|| {})
                .child(
                    Div("hscroll-content")
                        .style(Style::new().size(p.content_w, 200).x(-offset.get().x))
                        .build(),
                ),
            ctx.child(
                "oppa::HScrollRigBar",
                1,
                &ScrollbarProps {
                    target: SharedString::from("hscroll-list"),
                    offset: offset.x(),
                    idle_hide_ms: None,
                    axis: ScrollbarAxis::Horizontal,
                },
                Scrollbar,
            ),
        ])
}

fn hscrollbar_harness(content_w: f32) -> (ComponentHost, ScrollOffset) {
    let host = ComponentHost::new();
    host.set_viewport(400.0, 300.0);
    let handle = host.mount("HS", HScrollRigProps { content_w }, hscroll_rig);
    host.run_until_idle();
    let both = host
        .instance_scroll_2d(handle.root_instance())
        .expect("rig 2D handle");
    (host, both.x())
}

/// Horizontal chrome rects (track spans the viewport width at
/// the target's bottom edge; thumb pins the transposed ratio).
fn hchrome_rects(plan: &oppa::FramePlan) -> ChromePair {
    let mut track = Vec::new();
    let mut thumb = Vec::new();
    for op in &plan.ops {
        if let oppa::DrawOp::Rect { x, y, w, h, .. } = op {
            if approx(*y, 188.0) && approx(*h, 12.0) {
                if approx(*x, 0.0) && approx(*w, 200.0) {
                    track.push((*x, *y, *w, *h));
                } else {
                    thumb.push((*x, *y, *w, *h));
                }
            }
        }
    }
    (track, thumb)
}

/// Phase 36 PR2b: the horizontal thumb paints transposed (same
/// ratio as the vertical brief) and hides without overflow.
#[test]
fn horizontal_thumb_paints_transposed_and_hides_when_fit() {
    let (host, _) = hscrollbar_harness(300.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = hchrome_rects(&plan);
    assert!(track.is_empty() && thumb.is_empty(), "idle paints nothing");
    hover(&host, 100.0, 194.0);
    let plan = scrollbar_plan(&host);
    let (track, thumb) = hchrome_rects(&plan);
    assert_eq!(track.len(), 1, "gutter hover paints track, got {track:?}");
    assert_eq!(thumb.len(), 1, "gutter hover paints thumb, got {thumb:?}");
    assert!(
        approx(thumb[0].2, 200.0 * 200.0 / 300.0),
        "thumb pins vp²/c, got {:?}",
        thumb[0]
    );
    assert!(
        approx(thumb[0].0, 0.0),
        "rest parks at left, got {:?}",
        thumb[0]
    );
    // No overflow: nothing paints even on hover.
    let (host2, _) = hscrollbar_harness(100.0);
    hover(&host2, 100.0, 194.0);
    let plan2 = scrollbar_plan(&host2);
    let (track2, thumb2) = hchrome_rects(&plan2);
    assert!(
        track2.is_empty() && thumb2.is_empty(),
        "fits: paints nothing"
    );
}

/// Phase 36 PR2b: dragging the horizontal thumb left-to-right
/// scrolls the content to 100% of its `content_w` overflow.
#[test]
fn horizontal_thumb_drag_scrolls_to_max() {
    let (host, offset) = hscrollbar_harness(600.0);
    // Thumb at rest: 0..66.67 on the 200 track; drag to the right
    // edge (through the midpoint, like the vertical proof).
    drag(&host, 33.0, 194.0, 190.0, 194.0);
    assert!(
        approx(offset.get(), 400.0),
        "drag parks at max, got {}",
        offset.get()
    );
    let plan = scrollbar_plan(&host);
    let (_, thumb) = hchrome_rects(&plan);
    assert_eq!(thumb.len(), 1, "thumb paints after drag");
    assert!(
        approx(thumb[0].0, (200.0f32 - 200.0 * 200.0 / 600.0).round()),
        "thumb parks at travel, got {:?}",
        thumb[0]
    );
}

/// Phase 36 PR2b: Right/Left page by viewport on the horizontal
/// axis, and the track semantics carry the G18 numeric value.
#[test]
fn horizontal_keys_page_and_semantics_carry_range() {
    let (host, offset) = hscrollbar_harness(600.0);
    host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
    host.run_until_idle();
    host.inject_input(InputEvent::key(keys::RIGHT, KeyState::Pressed));
    host.run_until_idle();
    assert!(
        approx(offset.get(), 200.0),
        "Right pages, got {}",
        offset.get()
    );
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    assert!(
        approx(offset.get(), 0.0),
        "Left pages back, got {}",
        offset.get()
    );
    // Range half (decision 352, wired in PR2b): the slider payload
    // names the numeric position, not just the human percent.
    let track = host.with_retained_mut(|rec, _| {
        rec.retained_ids()
            .into_iter()
            .find(|id| {
                rec.get(*id)
                    .is_some_and(|n| n.debug.starts_with("scrollbar-track-"))
            })
            .expect("track node lives")
    });
    let sem = host.retained_semantics(track).expect("track semantics");
    assert_eq!(sem.role, oppa::Role::Slider);
    assert_eq!(sem.value_num, Some(oppa::Num::of(0.0)));
    assert_eq!(sem.min_value, Some(oppa::Num::of(0.0)));
    assert_eq!(sem.max_value, Some(oppa::Num::of(400.0)));
}

/// Round 22.1 (decision 331): `masked` publishes into the
/// session every render (and clears back) — the loop's
/// copy/cut guard reads this flag, so toggling needs no
/// remount. Unmasked fields publish false.
#[test]
fn masked_flag_publishes_into_session() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(SharedString::from("secret"));
    let handle = host.mount(
        "TI",
        TextInputProps::new("Password", value.clone()).masked(true),
        TextInput,
    );
    host.run_until_idle();
    let sessions = host.edit_sessions_for(handle.root_instance());
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0].is_masked(), "masked prop reaches the session");
    // Toggling the prop clears the flag without a remount.
    handle.set_props(TextInputProps::new("Password", value.clone()));
    host.run_until_idle();
    let sessions = host.edit_sessions_for(handle.root_instance());
    assert!(!sessions[0].is_masked(), "unmasked publishes false");
}

/// Round 22.2 (decision 332): `TextArea` publishes its content
/// width (box minus both pads) as the session wrap width, so
/// vertical caret travel wraps where the area wraps.
#[test]
fn textarea_publishes_wrap_width() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(SharedString::from("hi"));
    let handle = host.mount(
        "TA",
        TextAreaProps::new("Notes", value.clone()).width(200.0),
        TextArea,
    );
    host.run_until_idle();
    let sessions = host.edit_sessions_for(handle.root_instance());
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0].wrap_width(),
        Some(184.0),
        "content width drives wrapping"
    );
    assert!(sessions[0].is_multiline());
}

/// Round 17.3 (decision 319): masked TextInput renders bullet
/// characters while the EditSession and bound value retain cleartext.
#[test]
fn password_masked_renders_bullets_with_cleartext_intact() {
    let host = ComponentHost::new();
    let value = host.runtime().signal(SharedString::from("secret123"));
    let _handle = host.mount(
        "TI",
        TextInputProps::new("Password", value.clone()).masked(true),
        TextInput,
    );
    host.run_until_idle();

    // 1. Cleartext intact in author-owned signal
    assert_eq!(&*value.get(), "secret123");

    // 2. Displayed node text is bullets of matching length
    let text_ids = find_retained_by_debug(&host, "text");
    let leaf_text = host
        .with_retained_mut(|rec, _| {
            text_ids
                .into_iter()
                .find_map(|id| rec.get(id).and_then(|n| n.text.clone()))
        })
        .expect("text leaf");
    assert_eq!(&*leaf_text, "•••••••••");

    // 3. Typing / inserting appends cleartext and grows bullet cluster
    press_node(&host, node_by_debug(&host, "text-input"));
    let session = host.focused_field_session().expect("focused session");
    session.insert("4");
    host.run_until_idle();

    assert_eq!(&*value.get(), "4secret123");
    let text_ids = find_retained_by_debug(&host, "text");
    let leaf_text = host
        .with_retained_mut(|rec, _| {
            text_ids
                .into_iter()
                .find_map(|id| rec.get(id).and_then(|n| n.text.clone()))
        })
        .expect("text leaf after insert");
    assert_eq!(&*leaf_text, "••••••••••");

    // 4. Empty value with placeholder displays placeholder, not bullets
    let empty_val = host.runtime().signal(SharedString::from(""));
    host.mount(
        "TI2",
        TextInputProps::new("Password", empty_val)
            .placeholder("Enter password")
            .masked(true),
        TextInput,
    );
    host.run_until_idle();
    let text_ids = find_retained_by_debug(&host, "text");
    let ph_text = host
        .with_retained_mut(|rec, _| {
            text_ids
                .into_iter()
                .filter_map(|id| rec.get(id).and_then(|n| n.text.clone()))
                .find(|t| &**t == "Enter password")
        })
        .expect("placeholder leaf");
    assert_eq!(&*ph_text, "Enter password");
}

#[derive(Clone, Props)]
struct TooltipAnchorProps;

fn tooltip_anchor(_ctx: &Ctx, _p: &TooltipAnchorProps) -> VNode {
    Div("anchor-btn")
        .style(Style::new().size(80, 30).bg(Color(0x33_33_33)))
        .build()
}

fn tooltip_screen(ctx: &Ctx, _p: &()) -> VNode {
    let tp = TooltipProps::new("Helpful tip", tooltip_anchor, TooltipAnchorProps);
    Div("screen")
        .style(Style::new().size(400, 300))
        .child(ctx.child("oppa::Tooltip", 1, &tp, Tooltip))
}

/// Round 17.3 (decision 319): tooltip card mounts after 500ms
/// dwell, dismisses immediately on hover-leave or press.
#[test]
fn tooltip_dwell_mounts_card_and_dismisses_on_leave_or_press() {
    use std::rc::Rc;
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(400.0, 300.0);
    host.mount("S", (), tooltip_screen);
    host.run_until_idle();

    // 1. Initial state: no hover, tooltip-card not in tree
    assert!(
        find_retained_by_debug(&host, "tooltip-card").is_empty(),
        "initially closed"
    );

    // 2. Hover at t = 0: pointer enters anchor (40, 15)
    hover(&host, 40.0, 15.0);
    assert!(
        find_retained_by_debug(&host, "tooltip-card").is_empty(),
        "closed before dwell"
    );

    // 3. Hover before 500ms (e.g. 200ms): still closed
    clock.advance(0.2);
    hover(&host, 42.0, 15.0);
    assert!(
        find_retained_by_debug(&host, "tooltip-card").is_empty(),
        "closed at 200ms"
    );

    // 4. Dwell reaches 500ms: tooltip-card mounts
    clock.advance(0.35); // 0.2 + 0.35 = 0.55s (550ms >= 500ms)
    hover(&host, 42.0, 15.0);
    assert_eq!(
        find_retained_by_debug(&host, "tooltip-card").len(),
        1,
        "mounts after 500ms dwell"
    );

    // Card displays the tip text
    let card_ids = find_retained_by_debug(&host, "tooltip-card");
    let tip_text = host
        .with_retained_mut(|rec, _| {
            for id in card_ids {
                let mut stack = vec![id];
                while let Some(cur) = stack.pop() {
                    if let Some(node) = rec.get(cur) {
                        if let Some(ref text) = node.text {
                            return Some(text.clone());
                        }
                        stack.extend(node.children.iter().copied());
                    }
                }
            }
            None
        })
        .expect("tip text");
    assert_eq!(&*tip_text, "Helpful tip");

    // 5. Hover-leave dismisses immediately
    hover(&host, 300.0, 200.0);
    assert!(
        find_retained_by_debug(&host, "tooltip-card").is_empty(),
        "dismisses on leave"
    );

    // 6. Dwell again to mount, then press dismisses
    hover(&host, 40.0, 15.0);
    clock.advance(0.6);
    hover(&host, 40.0, 15.0);
    assert_eq!(
        find_retained_by_debug(&host, "tooltip-card").len(),
        1,
        "re-mounts on dwell"
    );

    // Press on anchor dismisses
    tap_at(&host, 40.0, 15.0, false);
    assert!(
        find_retained_by_debug(&host, "tooltip-card").is_empty(),
        "dismisses on press"
    );
}

/// Round 21.3 (decision 330): a tooltip spawned 5px from the
/// corner flips inside the viewport (long tip forces both-axis
/// overflow below/right of the anchor).
#[test]
fn tooltip_clamps_near_viewport_edges() {
    use std::rc::Rc;
    #[derive(Clone, Props)]
    struct CornerProps;
    fn corner_anchor(_ctx: &Ctx, _p: &CornerProps) -> VNode {
        Div("corner-btn")
            .style(Style::new().size(60, 20).bg(Color(0x33_33_33)))
            .build()
    }
    fn corner_screen(ctx: &Ctx, _p: &()) -> VNode {
        let tp = TooltipProps::new(
            "A much longer tooltip tip text here",
            corner_anchor,
            CornerProps,
        );
        // Anchor top-left at (335, 275): below/right overflow.
        Div("screen").style(Style::new().size(400, 300)).child(
            Div("corner-wrap")
                .style(Style::new().x(335.0).absolute_y(275.0).w(60.0).h(20.0))
                .child(ctx.child("oppa::CornerTip", 1, &tp, Tooltip)),
        )
    }
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(400.0, 300.0);
    host.set_text_service(Box::new(FakeText));
    host.mount("S", (), corner_screen);
    host.run_until_idle();
    // Dwell the anchor center (365, 285) past 500ms.
    hover(&host, 365.0, 285.0);
    clock.advance(0.6);
    hover(&host, 365.0, 285.0);
    let card = find_retained_by_debug(&host, "tooltip-card")
        .into_iter()
        .next()
        .expect("card mounts on dwell");
    let b = host.committed_box(card).expect("laid card");
    assert!(
        b.x >= 0.0 && b.y >= 0.0 && b.x + b.w <= 400.0 && b.y + b.h <= 300.0,
        "card flips inside the viewport, got ({}, {}) {}x{}",
        b.x,
        b.y,
        b.w,
        b.h
    );
}

/// Round 18.1 (decision 320): ErrorBoundary catches child panic,
/// host survives, renders fallback card, and recovers on reset.
#[test]
fn error_boundary_catches_child_panic_host_survives_and_recovers_on_retry() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let should_panic = Arc::new(AtomicBool::new(true));
    let error_log = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

    #[derive(Clone)]
    struct TestProps1 {
        sp: Arc<AtomicBool>,
        log: Arc<std::sync::Mutex<Vec<String>>>,
    }
    impl oppa::Props for TestProps1 {}

    let sp_clone = should_panic.clone();
    let log_clone = error_log.clone();

    fn child_view(should_panic: &AtomicBool) -> VNode {
        if should_panic.load(Ordering::SeqCst) {
            panic!("boom: intentional component panic");
        }
        Div("healthy-child").child(VNode::from(Text::new("Recovered successfully")))
    }

    fn screen(ctx: &Ctx, props: &TestProps1) -> VNode {
        let sp_child = props.sp.clone();
        let log_err = props.log.clone();
        let eb_props =
            ErrorBoundaryProps::new(move |_| child_view(&sp_child)).on_error(move |msg| {
                log_err.lock().unwrap().push(msg.to_string());
            });
        ctx.child("EB", 0, &eb_props, ErrorBoundary)
    }

    let host = ComponentHost::new();
    host.set_viewport(400.0, 300.0);

    // Silence stderr panic output for the intentional test panic
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    host.mount(
        "App",
        TestProps1 {
            sp: sp_clone,
            log: log_clone,
        },
        screen,
    );
    host.run_until_idle();
    std::panic::set_hook(hook);

    // 1. Host survived! Panicking child rendered fallback card.
    assert!(find_retained_by_debug(&host, "healthy-child").is_empty());
    assert_eq!(
        find_retained_by_debug(&host, "error-boundary-card").len(),
        1
    );
    assert_eq!(error_log.lock().unwrap().len(), 1);
    assert!(error_log.lock().unwrap()[0].contains("boom: intentional component panic"));

    // 2. Clear panic condition, click retry
    should_panic.store(false, Ordering::SeqCst);
    let retry_btn = node_by_debug(&host, "error-boundary-retry");
    press_node(&host, retry_btn);
    host.run_until_idle();

    // 3. Child recovered! Fallback card is gone, healthy child is mounted.
    assert_eq!(find_retained_by_debug(&host, "healthy-child").len(), 1);
    assert!(find_retained_by_debug(&host, "error-boundary-card").is_empty());
}

/// Round 18.1 (decision 320): Custom fallback renderer receives error and reset trigger.
#[test]
fn error_boundary_custom_fallback_renders_and_resets() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let should_panic = Arc::new(AtomicBool::new(true));
    let sp_clone = should_panic.clone();

    #[derive(Clone)]
    struct TestProps2 {
        sp: Arc<AtomicBool>,
    }
    impl oppa::Props for TestProps2 {}

    fn screen(ctx: &Ctx, props: &TestProps2) -> VNode {
        let sp_child = props.sp.clone();
        let eb_props = ErrorBoundaryProps::new(move |_| {
            if sp_child.load(Ordering::SeqCst) {
                panic!("custom panic payload");
            }
            Div("custom-child-ok").child(VNode::from(Text::new("Child OK")))
        })
        .fallback(|_ctx, err, _reset| {
            Div("custom-fallback")
                .semantics(oppa::Semantics::default().label(err))
                .child(VNode::from(Text::new(format!("Custom: {err}"))))
        });
        ctx.child("EB", 0, &eb_props, ErrorBoundary)
    }

    let host = ComponentHost::new();
    host.set_viewport(400.0, 300.0);

    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    host.mount("App", TestProps2 { sp: sp_clone }, screen);
    host.run_until_idle();

    // Fallback is custom-fallback with exact message
    assert!(find_retained_by_debug(&host, "custom-child-ok").is_empty());
    assert_eq!(find_retained_by_debug(&host, "custom-fallback").len(), 1);

    std::panic::set_hook(hook);
}
