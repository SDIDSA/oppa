use super::*;
use oppa::input::InputEvent;
use oppa::{find_retained_by_debug, Column, ComponentHost, Ctx, Div, Props, Signal, Style, VNode};
use std::rc::Rc;

#[test]
fn modal_hidden_when_closed() {
    let host = ComponentHost::new();
    let open = host.runtime().signal(false);
    host.mount("M", ModalProps::new("Delete file?", open), Modal);
    host.run_until_idle();
    assert!(
        find_retained_by_debug(&host, "modal-card").is_empty(),
        "no dialog mounted while closed"
    );
    assert!(
        find_retained_by_debug(&host, "modal-backdrop").is_empty()
            && find_retained_by_debug(&host, "modal-overlay").is_empty()
    );
    let id = node_by_debug(&host, "modal-closed");
    let b = host.committed_box(id).expect("closed box");
    assert!(
        approx(b.w, 0.0) && approx(b.h, 0.0),
        "zero hit-testable dialog area"
    );
    assert!(
        host.retained_handlers(id).is_empty(),
        "closed modal is inert"
    );
}

#[derive(Clone)]
struct ModalGapProps {
    open: Signal<bool>,
}
impl Props for ModalGapProps {}

fn modal_gap_app(ctx: &Ctx, props: &ModalGapProps) -> VNode {
    Column::new().children([
        Div("gap-top").style(Style::new().size(100, 10)).build(),
        ctx.child(
            "oppa::Modal",
            1,
            &ModalProps::new("Delete?", props.open.clone()),
            Modal,
        ),
        Div("gap-bottom").style(Style::new().size(100, 10)).build(),
    ])
}

/// Round 7.21 (decision 296): the closed modal is a skipped
/// portal, so it leaves zero gap in the parent column — and
/// opening mounts the overlay without moving the column either.
#[test]
fn modal_closed_introduces_zero_gap_in_parent_column() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(false);
    host.mount("G", ModalGapProps { open: open.clone() }, modal_gap_app);
    host.run_until_idle();
    let bottom_y = || {
        host.committed_box(node_by_debug(&host, "gap-bottom"))
            .expect("bottom box")
            .y
    };
    assert!(
        approx(bottom_y(), 10.0),
        "closed modal leaves zero gap, bottom at y={}",
        bottom_y()
    );
    open.set(true);
    host.run_until_idle();
    assert!(
        !find_retained_by_debug(&host, "modal-card").is_empty(),
        "dialog mounts when open"
    );
    assert!(
        approx(bottom_y(), 10.0),
        "open modal leaves the column untouched, bottom at y={}",
        bottom_y()
    );
}

#[derive(Clone)]
struct ModalNestedProps {
    open: Signal<bool>,
}
impl Props for ModalNestedProps {}

/// The eyeball scenario: the dialog is triggered from content
/// nested inside padded layout (a tab panel), not from the root.
fn modal_nested_app(ctx: &Ctx, props: &ModalNestedProps) -> VNode {
    Div("pad")
        .style(Style::new().pad_x(50).pad_y(40))
        .child(Column::new().children([
            Div("gap-top").style(Style::new().size(100, 10)).build(),
            ctx.child(
                "oppa::Modal",
                1,
                &ModalProps::new("Delete?", props.open.clone()),
                Modal,
            ),
        ]))
}

/// Round 7.21 follow-up (user eyeball): a modal mounted inside
/// nested content still dims from the viewport origin — the
/// repositioned ancestors must not drag the portal along.
#[test]
fn modal_nested_in_content_still_dims_from_viewport_origin() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount(
        "N",
        ModalNestedProps { open: open.clone() },
        modal_nested_app,
    );
    host.run_until_idle();
    let bb = host
        .committed_box(node_by_debug(&host, "modal-backdrop"))
        .expect("backdrop box");
    assert!(
        approx(bb.x, 0.0) && approx(bb.y, 0.0),
        "backdrop at viewport origin, got ({}, {})",
        bb.x,
        bb.y
    );
    assert!(
        approx(bb.w, 800.0) && approx(bb.h, 600.0),
        "backdrop fills the viewport, got {}x{}",
        bb.w,
        bb.h
    );
    let cb = host
        .committed_box(node_by_debug(&host, "modal-card"))
        .expect("card box");
    assert!(
        approx(cb.x, (800.0 - cb.w) / 2.0) && approx(cb.y, (600.0 - cb.h) / 2.0),
        "card centered in the window, got ({}, {})",
        cb.x,
        cb.y
    );
}

#[test]
fn modal_visible_when_open() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount("M", ModalProps::new("Delete file?", open), Modal);
    host.run_until_idle();
    let card = node_by_debug(&host, "modal-card");
    let sem = host.retained_semantics(card).expect("semantics");
    assert_eq!(sem.role, oppa::Role::Dialog);
    assert_eq!(sem.label.as_deref(), Some("Delete file?"));
    let cb = host.committed_box(card).expect("card box");
    assert!(approx(cb.w, 360.0), "explicit card width, got {}", cb.w);
    // Full-viewport dimming (Round 7.21): the backdrop covers the
    // default 800x600 host viewport, and the card centers on both
    // axes inside it.
    let bb = host
        .committed_box(node_by_debug(&host, "modal-backdrop"))
        .expect("backdrop box");
    assert!(
        approx(bb.x, 0.0) && approx(bb.y, 0.0),
        "backdrop at viewport origin, got ({}, {})",
        bb.x,
        bb.y
    );
    assert!(
        approx(bb.w, 800.0) && approx(bb.h, 600.0),
        "backdrop fills the viewport, got {}x{}",
        bb.w,
        bb.h
    );
    assert!(
        approx(cb.x, bb.x + (bb.w - cb.w) / 2.0),
        "card horizontally centered, x={}",
        cb.x
    );
    assert!(
        approx(cb.y, bb.y + (bb.h - cb.h) / 2.0),
        "card vertically centered, y={}",
        cb.y
    );
    // Title (18px: 12 chars x 11.25) + labels ("OK" 17.5, "Cancel" 52.5).
    let mut ws: Vec<f32> = find_retained_by_debug(&host, "text")
        .into_iter()
        .filter_map(|id| host.committed_box(id))
        .filter(|b| !b.lines.is_empty())
        .map(|b| b.content_w)
        .collect();
    ws.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(ws.len(), 3, "title + two button labels");
    assert!(
        approx(ws[0], 17.5) && approx(ws[1], 52.5) && approx(ws[2], 135.0),
        "OK/Cancel/title widths, got {ws:?}"
    );
    node_by_debug(&host, "modal-cancel");
    node_by_debug(&host, "modal-confirm");
}

#[test]
fn modal_backdrop_click_dismisses() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount("M", ModalProps::new("Delete file?", open.clone()), Modal);
    host.run_until_idle();
    let bb = host
        .committed_box(node_by_debug(&host, "modal-backdrop"))
        .expect("backdrop box");
    let cb = host
        .committed_box(node_by_debug(&host, "modal-card"))
        .expect("card box");
    // Left band: inside the backdrop, outside the centered card.
    let (x, y) = (bb.x + 5.0, bb.y + bb.h / 2.0);
    assert!(x < cb.x, "click lands outside the card");
    host.inject_input(InputEvent::pointer_down(x, y));
    host.inject_input(InputEvent::pointer_up(x, y));
    host.run_until_idle();
    assert!(!open.get(), "backdrop press dismisses");
    assert!(
        find_retained_by_debug(&host, "modal-card").is_empty(),
        "dialog unmounts on dismiss"
    );
    // Opt-out is structural: no handler, press is a quiet miss.
    let host2 = ComponentHost::new();
    host2.set_text_service(Box::new(FakeText));
    let open2 = host2.runtime().signal(true);
    host2.mount(
        "M2",
        ModalProps::new("Delete file?", open2.clone()).no_backdrop_dismiss(),
        Modal,
    );
    host2.run_until_idle();
    let bd2 = node_by_debug(&host2, "modal-backdrop");
    assert!(
        host2.retained_handlers(bd2).is_empty(),
        "opt-out backdrop carries no handler"
    );
    let bb2 = host2.committed_box(bd2).expect("backdrop box");
    host2.inject_input(InputEvent::pointer_down(bb2.x + 5.0, bb2.y + bb2.h / 2.0));
    host2.inject_input(InputEvent::pointer_up(bb2.x + 5.0, bb2.y + bb2.h / 2.0));
    host2.run_until_idle();
    assert!(open2.get(), "opt-out backdrop never dismisses");
}

#[test]
fn modal_confirm_and_cancel_actions() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    let confirmed = host.runtime().signal(false);
    let cancelled = host.runtime().signal(false);
    let (c_set, x_set) = (confirmed.clone(), cancelled.clone());
    host.mount(
        "M",
        ModalProps::new("Delete file?", open.clone())
            .on_confirm(move || c_set.set(true))
            .on_cancel(move || x_set.set(true)),
        Modal,
    );
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "modal-confirm"));
    assert!(confirmed.get(), "confirm callback fires");
    assert!(!cancelled.get(), "confirm does not run cancel");
    assert!(!open.get(), "confirm closes");
    open.set(true);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "modal-cancel"));
    assert!(cancelled.get(), "cancel callback fires");
    assert!(!open.get(), "cancel closes");
}

#[derive(Clone)]
struct ToastCase {
    props: ToastProps,
}
impl Props for ToastCase {}

fn toast_render(ctx: &Ctx, p: &ToastCase) -> VNode {
    ctx.child("oppa::Toast", 13, &p.props, Toast)
}

#[test]
fn toast_hidden_when_closed() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(false);
    host.mount(
        "T",
        ToastCase {
            props: ToastProps::new("Saved", open),
        },
        toast_render,
    );
    host.run_until_idle();
    assert!(
        find_retained_by_debug(&host, "toast-card").is_empty(),
        "no toast mounted while closed"
    );
    let id = node_by_debug(&host, "toast-closed");
    let b = host.committed_box(id).expect("closed box");
    assert!(
        approx(b.w, 0.0) && approx(b.h, 0.0),
        "zero hit-testable toast area"
    );
    assert!(
        host.retained_handlers(id).is_empty(),
        "closed toast is inert"
    );
}

#[test]
fn toast_visible_with_status_semantics_when_open() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount(
        "T",
        ToastCase {
            props: ToastProps::new("Saved", open).variant(ToastVariant::Success),
        },
        toast_render,
    );
    host.run_until_idle();
    let id = node_by_debug(&host, "toast-card");
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::Status, "status role, never dialog");
    assert_eq!(sem.label.as_deref(), Some("Saved"), "message is the label");
    // The anchor is handler-less full-viewport chrome: only the
    // dismiss button carries a handler, so presses outside the
    // card fall through to the app (the inverse of Modal's
    // capturing backdrop).
    assert!(
        host.retained_handlers(node_by_debug(&host, "toast-anchor"))
            .is_empty(),
        "anchor never claims presses"
    );
}

#[test]
fn toast_dismiss_button_closes() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount(
        "T",
        ToastCase {
            props: ToastProps::new("Saved", open.clone()).sticky(),
        },
        toast_render,
    );
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "toast-dismiss"));
    assert!(!open.get(), "dismiss press closes");
    host.run_until_idle();
    assert!(
        find_retained_by_debug(&host, "toast-card").is_empty(),
        "toast unmounts on dismiss"
    );
}

#[test]
fn toast_auto_dismiss_fires_on_clock() {
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount(
        "T",
        ToastCase {
            props: ToastProps::new("Saved", open.clone()).auto_dismiss_ms(Some(4000)),
        },
        toast_render,
    );
    // Decision 514: the appear-flip arms beside the auto-dismiss
    // timer — fire it first (due immediately), then settle with
    // flowing time (the flip's entrance track retires as time
    // passes — a frozen idle pump would spin on it), so the counts
    // below read the dismiss timer alone.
    assert_eq!(host.tick_timers(host.now_ms()), 1, "flip fires");
    settle_clocked(&host, &clock);
    assert_eq!(host.tick_timers(host.now_ms()), 0, "nothing due yet");
    assert!(open.get(), "still visible before the delay");
    clock.advance(4.1);
    assert_eq!(host.tick_timers(host.now_ms()), 1, "delay timer fires");
    host.run_until_idle();
    assert!(!open.get(), "auto-dismiss closes");
}

#[test]
fn toast_sticky_never_arms_timer() {
    let clock = Rc::new(oppa::MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_text_service(Box::new(FakeText));
    let open = host.runtime().signal(true);
    host.mount(
        "T",
        ToastCase {
            props: ToastProps::new("Saved", open.clone()).sticky(),
        },
        toast_render,
    );
    // Decision 514: sticky arms no auto-dismiss timer — only the
    // appear-flip, which fires alone here (then settles with
    // flowing time like above).
    assert_eq!(host.tick_timers(host.now_ms()), 1, "flip fires");
    settle_clocked(&host, &clock);
    clock.advance(60.0);
    assert_eq!(
        host.tick_timers(host.now_ms()),
        0,
        "sticky arms nothing more"
    );
    host.run_until_idle();
    assert!(open.get(), "sticky toast stays until dismissed");
}
