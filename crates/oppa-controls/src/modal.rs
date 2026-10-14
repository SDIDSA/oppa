use crate::button::{Button, ButtonProps};
use crate::common::*;
use oppa::*;
// ---------------------------------------------------------------------------
// Modal
// ---------------------------------------------------------------------------

/// Modal dialog props (controlled — `open` is the author-owned visibility
/// signal; actions are optional callbacks).
#[derive(Clone)]
pub struct ModalProps {
    pub title: SharedString,
    pub open: Signal<bool>,
    pub width: f32,
    pub backdrop_dismiss: bool,
    pub on_confirm: Option<Action>,
    pub on_cancel: Option<Action>,
    pub confirm_label: SharedString,
    pub cancel_label: SharedString,
}

impl Props for ModalProps {}

impl ModalProps {
    pub fn new(title: &str, open: Signal<bool>) -> Self {
        Self {
            title: SharedString::from(title),
            open,
            width: 360.0,
            backdrop_dismiss: true,
            on_confirm: None,
            on_cancel: None,
            confirm_label: SharedString::from("OK"),
            cancel_label: SharedString::from("Cancel"),
        }
    }

    pub fn on_confirm(mut self, f: impl Fn() + 'static) -> Self {
        self.on_confirm = Some(action(f));
        self
    }

    pub fn on_cancel(mut self, f: impl Fn() + 'static) -> Self {
        self.on_cancel = Some(action(f));
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn no_backdrop_dismiss(mut self) -> Self {
        self.backdrop_dismiss = false;
        self
    }
}

/// Shared dismiss behavior (backdrop tap and Cancel): close, then run the
/// cancel callback when one is configured.
fn dismiss(open: &Signal<bool>, on_cancel: &Option<Action>) -> Action {
    let open = open.clone();
    let on_cancel = on_cancel.clone();
    action(move || {
        open.set(false);
        if let Some(f) = on_cancel.as_ref() {
            f();
        }
    })
}

/// Modal dialog: dimmed overlay + centered card (title, end-aligned
/// Cancel/Confirm). Composes only proven primitives (decision 212) plus
/// the Round-1.4 `Portal` overlay layer (decision 255) with Round-7.21
/// full-viewport portals (decision 296).
///
/// Structure (open): `Portal("modal-portal")` — offset-less, so it
///   sits at the viewport origin at full viewport size — holding
///   `Div("modal-backdrop")` (`fill_width` + `fill_height` over the
///   portal's content box, dim `0x11_11_11` at half opacity, dismiss
///   on press) holding `Div("modal-card")` (explicit width, centered
///   on both axes by the backdrop's `justify_content: Center` +
///   `align_items: Center`).
/// Deviations from the naive sketch, each verified against the layout
/// arms rather than assumed:
/// - No `modal-overlay` Row anymore (Round 7.21): the portal itself
///   is viewport-sized, so the backdrop fills through the portal's
///   `given_h` channel and centers internally — one less layer, and
///   the dim covers 100% of the window instead of a card-high band.
/// - Presses route to the nearest press-owner ancestor (knob→track
///   precedent): the action buttons capture their own presses; every
///   other press in the overlay dismisses through the backdrop. Portal
///   hit priority puts the overlay above app content (decision 255);
///   the full-viewport box means an open modal captures every press
///   (previously presses outside the thin band fell through to the
///   app — the strip artifact, now closed by construction);
///   closing unmounts the portal, which clears focus/captures inside
///   it. Tab inside the open card cycles the card's buttons (round
///   5.2 focus trap — derived from the retained tree, no
///   registration; closing restores the global order).
///   Structure (closed): `Portal("modal-closed")` at explicit 0×0 —
///   flow containers skip portals entirely, so a closed modal leaves
///   zero gap in parent flex layouts (no phantom spacing).
pub fn Modal(ctx: &Ctx, props: &ModalProps) -> VNode {
    // Round 11.2: the card rides the theme (surface + ring); the
    // backdrop scrim stays absolute black-with-opacity (a veil holds
    // in both themes — theme-invariant by design, like contrast ink).
    let t = ctx.theme().tokens();
    // Decision 514: appear-fade visibility first (the guarded reset
    // must run on the closed path, or the next open would mount
    // already shown).
    let shown = crate::motion::enter_shown(ctx, &props.open);
    if !props.open.get() {
        return Portal("modal-closed")
            .style(Style::new().size(0, 0))
            .build();
    }
    let dismiss_action = dismiss(&props.open, &props.on_cancel);
    let confirm_action = {
        let open = props.open.clone();
        let on_confirm = props.on_confirm.clone();
        action(move || {
            if let Some(f) = on_confirm.as_ref() {
                f();
            }
            open.set(false);
        })
    };
    let cancel_props = ButtonProps {
        label: props.cancel_label.clone(),
        enabled: true,
        width: 96.0,
        height: 32.0,
        debug: SharedString::from("modal-cancel"),
        on_press: dismiss_action.clone(),
        icon: None,
        icon_size: 16.0,
        vector_icon: None,
        fill: None,
        pressed_fill: None,
        ink: None,
        radius: None,
        text_style: None,
        label_center: crate::button::ButtonLabelCenter::Group,
        full_width: false,
        content_pad: 0.0,
    };
    let confirm_props = ButtonProps {
        label: props.confirm_label.clone(),
        enabled: true,
        width: 96.0,
        height: 32.0,
        debug: SharedString::from("modal-confirm"),
        on_press: confirm_action,
        icon: None,
        icon_size: 16.0,
        vector_icon: None,
        fill: None,
        pressed_fill: None,
        ink: None,
        radius: None,
        text_style: None,
        label_center: crate::button::ButtonLabelCenter::Group,
        full_width: false,
        content_pad: 0.0,
    };
    let mut card_style = Style::new()
        .bg(t.surface)
        .radius(8)
        .border(1, t.border)
        .pad_x(20)
        .pad_y(16)
        .gap(12)
        // Decision 514: the card fades + rises on open
        // (appear-first — exits snap, see SPEC).
        .opacity(Some(if shown.get() { 1.0 } else { 0.0 }))
        .transform(if shown.get() {
            oppa::Transform::identity()
        } else {
            oppa::Transform::translate(0, crate::motion::ENTER_RISE_PX)
        })
        .transition(oppa::Transition::enter())
        // Decision 520: dismissal fade safety (the backdrop below
        // pins as the removed root; this rides frozen while nested).
        .exit(oppa::Transition::exit())
        .build();
    // (No width-only builder — the explicit card width patches the
    // built style, decision-238 precedent.)
    card_style.w = Some(Px::of(props.width));
    let card = Div("modal-card")
        .style(card_style)
        .semantics(Semantics::dialog().label(&props.title))
        .children([
            VNode::from(Text::new(props.title.clone()).size(18).bold()),
            Row("modal-actions")
                .style(Style::new().gap(8).justify_content(JustifyContent::End))
                .children([
                    ctx.child_auto(&cancel_props, Button),
                    ctx.child_auto(&confirm_props, Button),
                ]),
        ]);
    let backdrop = Div("modal-backdrop").style(
        Style::new()
            .fill_width()
            .fill_height()
            .bg(Color(0x11_11_11))
            // Decision 514: the scrim fades with the card (the 0.5
            // veil is the settled value).
            .opacity(Some(if shown.get() { 0.5 } else { 0.0 }))
            .transition(oppa::Transition::enter())
            // Decision 520: dismissal fades the whole overlay out
            // (the card rides frozen under this envelope — one pin
            // per dismiss, no double-paint).
            .exit(oppa::Transition::exit())
            .justify_content(JustifyContent::Center)
            .align_items(AlignItems::Center),
    );
    // (`backdrop_dismiss: false` leaves the backdrop visual-only —
    // no silent no-op handler, decision 213.)
    let backdrop = if props.backdrop_dismiss {
        backdrop.on_press(move || dismiss_action())
    } else {
        backdrop
    };
    Portal("modal-portal")
        .style(
            Style::new()
                // Decision 520: dismissal fade safety (the backdrop
                // above pins as the removed root; this rides only if
                // the structure inverts).
                .exit(oppa::Transition::exit()),
        )
        .child(backdrop.child(card))
}
