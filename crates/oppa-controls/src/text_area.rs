use crate::common::*;
use oppa::Props;
use oppa::{
    Ctx, CursorIcon, Div, Px, Semantics, SharedString, Signal, Style, Text,
    TextArea as TextAreaLeaf, TextClass, VNode,
};
use std::rc::Rc;
// ---------------------------------------------------------------------------
// TextArea
// ---------------------------------------------------------------------------

/// TextArea props (Round 5.1): the multi-line sibling of
/// [`TextInputProps`] — controlled `value`, same session/placeholder/
/// disabled rules, but explicit width with content-driven height
/// (the box grows with wrapped lines — no fixed height, no scroll
/// primitive in v1, stated) and the `TextArea` payload (Enter
/// inserts a newline through the router instead of activating).
/// `on_change` reports committed edits (round 5.4 — `None` keeps
/// the exact pre-5.4 behavior).
///
/// Form validation (Phase 38a, decision 366 — G7): same four marks
/// as [`TextInputProps`].
#[derive(Clone)]
pub struct TextAreaProps {
    pub label: SharedString,
    pub value: Signal<SharedString>,
    pub placeholder: Option<SharedString>,
    pub enabled: bool,
    pub width: f32,
    pub style: TextClass,
    pub debug: SharedString,
    pub on_change: Option<Change<SharedString>>,
    pub invalid: bool,
    pub required: bool,
    pub error_message: Option<SharedString>,
    pub helper_text: Option<SharedString>,
    pub focused: Option<Signal<bool>>,
}

impl Props for TextAreaProps {}

impl TextAreaProps {
    pub fn new(label: &str, value: Signal<SharedString>) -> Self {
        Self {
            label: SharedString::from(label),
            value,
            placeholder: None,
            enabled: true,
            width: 200.0,
            style: Text::body_secondary,
            debug: SharedString::from("text-area"),
            on_change: None,
            invalid: false,
            required: false,
            error_message: None,
            helper_text: None,
            focused: None,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn style(mut self, style: TextClass) -> Self {
        self.style = style;
        self
    }

    /// Focus state binding: mirrors the field's focus state into the author's
    /// signal.
    pub fn focused(mut self, sig: Signal<bool>) -> Self {
        self.focused = Some(sig);
        self
    }

    /// Alias for [`focused`](Self::focused).
    pub fn focus_signal(mut self, sig: Signal<bool>) -> Self {
        self.focused = Some(sig);
        self
    }
}

/// Multi-line text input: bordered auto-height box + [`TextArea`](oppa::TextArea)
/// payload (same DOM verdict-(b) shape as `TextInput`, rendered as
/// `<textarea>`). Press focuses through the M5 router; Enter inserts
/// a newline via the router's textarea rule (never activates).
/// Press/drag handlers mirror [`TextInput`] (Rounds 8.1–8.2 — tap caret,
/// Shift extend, drag stream, double word, triple hard line; the press
/// handler needs no y because hard lines split on `\n`, while soft-wrap
/// y-to-line resolution stays an open follow-up) plus hold-to-select
/// (decision 448 — same double-tap word rule as `TextInput`).
/// Disabled drops the handlers structurally (decision 213 — no tab
/// stop either). Empty-with-placeholder mirrors `TextInput`
/// (presentational span, never the value; on DOM a native
/// `placeholder` attribute, decision 293).
pub fn TextArea(ctx: &Ctx, props: &TextAreaProps) -> VNode {
    if let Some(ref sig) = props.focused {
        let is_focused = ctx.focused().get();
        if oppa::untrack(|| sig.get()) != is_focused {
            sig.set(is_focused);
        }
    }
    let t = ctx.theme().tokens();
    let session = ctx.edit_session(props.value.clone());
    // Multi-line wrap rides the session (Round 22.2, decision 332):
    // published every render so resizes re-wrap without remount —
    // the content width is the box minus both 8px pads.
    session.set_wrap_width(Some((props.width - 16.0).max(1.0)));
    if let Some(notify) = props.on_change.clone() {
        session.set_on_change(Rc::new(move |value| notify(value)));
    }
    // Phase 38a (decision 366): validation marks announce; the edge
    // tints `error` while invalid.
    let semantics = with_validation_marks(
        Semantics::text_area()
            .label(&props.label)
            .disabled(!props.enabled),
        props.invalid,
        props.required,
        &props.error_message,
    );
    let raw_val = props.value.get();
    // Live preedit (decision 461 — same rule as `TextInput`):
    // composing sessions render the display composite inline and
    // subscribe to the composition epoch; idle sessions keep the
    // exact old shape.
    let _composition_epoch = session.composition_epoch().get();
    let display_text = if session.is_composing() {
        SharedString::from(session.display_text())
    } else {
        raw_val.clone()
    };
    let payload: VNode = match (display_text.is_empty(), &props.placeholder) {
        (true, Some(placeholder)) => with_ink(
            VNode::from(Text {
                text: placeholder.clone(),
                style: props.style,
            }),
            t.text_secondary,
        ),
        _ => TextAreaLeaf {
            text: display_text,
            style: props.style,
            label: props.label.clone(),
        }
        .into(),
    };
    // Width-only box (decision-238 patch pattern — no width-only
    // builder; height stays auto so the box grows with lines).
    // Radius 4 (Round 7.11 — the rectangular-controls rule:
    // checkbox/input/select/tab-bar share one corner).
    // Round 8.3: enabled areas show the I-beam (disabled: arrow).
    // Round 23.2: keyboard focus swaps the gray edge for the
    // themed ring (helper runs pre-build — the built style below
    // only patches the width).
    let mut box_style = focus_ringed(
        ctx,
        props.enabled,
        Style::new()
            .radius(4)
            .bg(if props.enabled { t.surface } else { t.disabled })
            .border(1, field_edge(ctx, props.invalid))
            .pad_x(8)
            .pad_y(4),
    );
    if props.enabled {
        box_style = box_style.cursor(CursorIcon::Text);
    }
    let mut box_style = box_style.build();
    box_style.w = Some(Px::of(props.width));
    let builder = Div(&props.debug).style(box_style).semantics(semantics);
    let builder = if props.enabled {
        let host = ctx.host();
        let tap_session = session.clone();
        let tap_style = props.style;
        let drag_host = host.clone();
        let drag_session = tap_session.clone();
        let hold_host = host.clone();
        let hold_session = tap_session.clone();
        builder
            .on_press(move || {
                field_press(&host, &tap_session, tap_style);
            })
            .on_long_press(move || {
                field_long_press(&hold_host, &hold_session, tap_style);
            })
            .on_drag(move || {
                field_drag(&drag_host, &drag_session, tap_style);
            })
    } else {
        builder
    };
    // Phase 38a: footer stacks under the area only when a message
    // is present (wrapper debug derives from the area debug).
    let debug = props.debug.clone();
    let root = builder.child(payload);
    with_validation_footer(
        root,
        ctx,
        &debug,
        props.invalid,
        &props.error_message,
        &props.helper_text,
    )
}

/// Uncontrolled text area (round 5.4, OQ-G2-4): self-managed
/// `value` from `initial`, committed edits reported through
/// `on_change`. See [`UncontrolledToggle`] for the contract notes.
#[derive(Clone)]
pub struct UncontrolledTextAreaProps {
    pub label: SharedString,
    pub initial: SharedString,
    pub placeholder: Option<SharedString>,
    pub enabled: bool,
    pub width: f32,
    pub style: TextClass,
    pub debug: SharedString,
    pub on_change: Option<Change<SharedString>>,
    pub invalid: bool,
    pub required: bool,
    pub error_message: Option<SharedString>,
    pub helper_text: Option<SharedString>,
    pub focused: Option<Signal<bool>>,
}

impl Props for UncontrolledTextAreaProps {}

impl UncontrolledTextAreaProps {
    pub fn focused(mut self, sig: Signal<bool>) -> Self {
        self.focused = Some(sig);
        self
    }

    pub fn focus_signal(mut self, sig: Signal<bool>) -> Self {
        self.focused = Some(sig);
        self
    }
}

pub fn UncontrolledTextArea(ctx: &Ctx, props: &UncontrolledTextAreaProps) -> VNode {
    let value = ctx.signal(props.initial.clone());
    ctx.child_auto(
        &TextAreaProps {
            label: props.label.clone(),
            value,
            placeholder: props.placeholder.clone(),
            enabled: props.enabled,
            width: props.width,
            style: props.style,
            debug: props.debug.clone(),
            on_change: props.on_change.clone(),
            invalid: props.invalid,
            required: props.required,
            error_message: props.error_message.clone(),
            helper_text: props.helper_text.clone(),
            focused: props.focused.clone(),
        },
        TextArea,
    )
}
