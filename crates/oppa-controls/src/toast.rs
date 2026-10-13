use crate::button::{Button, ButtonProps};
use oppa::*;
// ---------------------------------------------------------------------------
// Toast
// ---------------------------------------------------------------------------

/// Toast severity (tints the leading dot only — the card rides the
/// theme like Modal; fixed decorative literals per the decision-323
/// precedent, not new theme tokens).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ToastVariant {
    #[default]
    Info,
    Success,
    Error,
}

/// Toast props (controlled — `open` is the author-owned visibility
/// signal, the Modal precedent; the toast never owns lifetime).
#[derive(Clone)]
pub struct ToastProps {
    pub message: SharedString,
    pub open: Signal<bool>,
    pub variant: ToastVariant,
    /// Wall-clock auto-dismiss (`use_timeout`, the Scrollbar-idle
    /// precedent — each render while open re-arms, cleanup cancels
    /// the previous arm). `None` is sticky (manual dismiss only).
    pub auto_dismiss_ms: Option<u64>,
    pub debug: SharedString,
}

impl Props for ToastProps {}

impl ToastProps {
    pub fn new(message: &str, open: Signal<bool>) -> Self {
        Self {
            message: SharedString::from(message),
            open,
            variant: ToastVariant::Info,
            auto_dismiss_ms: Some(4000),
            debug: SharedString::from("toast"),
        }
    }

    pub fn variant(mut self, v: ToastVariant) -> Self {
        self.variant = v;
        self
    }

    pub fn auto_dismiss_ms(mut self, ms: Option<u64>) -> Self {
        self.auto_dismiss_ms = ms;
        self
    }

    pub fn sticky(mut self) -> Self {
        self.auto_dismiss_ms = None;
        self
    }

    pub fn debug(mut self, debug: &str) -> Self {
        self.debug = SharedString::from(debug);
        self
    }
}

/// Transient non-modal feedback: a bottom-center card (variant dot +
/// message + Dismiss) over a handler-less full-viewport anchor, so
/// presses outside the card fall through to the app (the inverse of
/// Modal's capturing backdrop — a toast must never block input).
/// `Status` semantics carry the message (decision 337); the toast
/// never takes focus.
///
/// Structure (open): `Portal("toast-portal")` holding
/// `Div("toast-anchor")` (`fill_width` + `fill_height`, main-axis
/// `End` for the bottom edge, cross-axis `Center`, 24px bottom pad)
/// holding `Div("toast-card")` (themed surface + ring, message row +
/// dismiss button).
/// Structure (closed): `Portal("toast-closed")` at explicit 0×0 —
/// the Modal precedent, zero gap in parent flex layouts.
pub fn Toast(ctx: &Ctx, props: &ToastProps) -> VNode {
    let t = ctx.theme().tokens();
    // Decision 514: appear-fade visibility first (the guarded reset
    // must run on the closed path, or the next show would mount
    // already shown).
    let shown = crate::motion::enter_shown(ctx, &props.open);
    if !props.open.get() {
        return Portal("toast-closed")
            .style(Style::new().size(0, 0))
            .build();
    }
    if let Some(ms) = props.auto_dismiss_ms {
        let open = props.open.clone();
        ctx.use_timeout(ms as f64, move || {
            open.set(false);
        });
    }
    let dot = match props.variant {
        ToastVariant::Info => t.primary,
        ToastVariant::Success => Color(0x2E7D32),
        ToastVariant::Error => Color(0xC62828),
    };
    let open = props.open.clone();
    let dismiss_props = ButtonProps::new("Dismiss", move || open.set(false))
        .size(72.0, 28.0)
        .debug("toast-dismiss");
    let card = Div("toast-card")
        .style(
            Style::new()
                .bg(t.surface)
                .radius(8)
                .border(1, t.border)
                .pad_x(16)
                .pad_y(12)
                .gap(12)
                // Decision 514: fades + rises on show (appear-first —
                // exits snap, see SPEC).
                .opacity(Some(if shown.get() { 1.0 } else { 0.0 }))
                .transform(if shown.get() {
                    oppa::Transform::identity()
                } else {
                    oppa::Transform::translate(0, crate::motion::ENTER_RISE_PX)
                })
                .transition(oppa::Transition::enter()),
        )
        .semantics(Semantics::status().label(&props.message))
        .children([
            Row("toast-row")
                .style(Style::new().gap(10).align_items(AlignItems::Center))
                .children([
                    Div("toast-dot")
                        .style(Style::new().size(8, 8).radius(4).bg(dot))
                        .build(),
                    VNode::from(Text {
                        text: props.message.clone(),
                        style: Text::body_secondary,
                    }),
                ]),
            ctx.child_auto(&dismiss_props, Button),
        ]);
    let anchor = Div("toast-anchor").style(
        Style::new()
            .fill_width()
            .fill_height()
            .justify_content(JustifyContent::End)
            .align_items(AlignItems::Center)
            .pad_bottom(24),
    );
    Portal("toast-portal").child(anchor.child(card))
}
