use crate::common::*;
use oppa::*;
use oppa_macros::Props;
// ---------------------------------------------------------------------------
// Radio + RadioGroup
// ---------------------------------------------------------------------------

/// Radio props (controlled — `selected` is display state; activation
/// rides `on_select`, and the group wires it to its signal).
#[derive(Clone)]
pub struct RadioProps {
    pub label: SharedString,
    pub selected: bool,
    pub enabled: bool,
    pub on_select: Action,
}

impl Props for RadioProps {}

/// Radio option: one choice in a [`RadioGroup`] (value + label).
#[derive(Clone)]
pub struct RadioOption<T> {
    pub value: T,
    pub label: SharedString,
}

/// RadioGroup props (controlled — `selected` holds the chosen value;
/// clicking an option sets it, so exactly one option reads selected).
///
/// Form validation (Phase 38a, decision 366 — G7): same four marks
/// as [`CheckboxProps`]; the marks announce on the group container
/// (role `Generic` — there is no group role in [`Semantics`], so the
/// payload carries only the marks, never an invented role).
#[derive(Clone, Props)]
pub struct RadioGroupProps<T: 'static> {
    pub options: Vec<RadioOption<T>>,
    pub selected: Signal<T>,
    pub enabled: bool,
    pub invalid: bool,
    pub required: bool,
    pub error_message: Option<SharedString>,
    pub helper_text: Option<SharedString>,
}

/// Radio button: 18×18 circle indicator (8×8 centered dot when
/// selected) + label, `radio` role with live `selected`; press
/// selects (no-op when disabled — structurally handler-less,
/// decision 213). The dot pins at (5, 5) via `x`/`absolute_y`
/// ((18 − 8) / 2 — the knob-in-track out-of-flow precedent), so the
/// indicator stays a plain `Div` with exact centering.
pub fn Radio(ctx: &Ctx, props: &RadioProps) -> VNode {
    let t = ctx.theme().tokens();
    let semantics = Semantics::radio(props.selected)
        .label(&props.label)
        .disabled(!props.enabled);
    let indicator = Div("radio-indicator").style(
        Style::new()
            .size(18, 18)
            .circle()
            .border(1, t.border)
            .bg(if props.enabled { t.surface } else { t.disabled }),
    );
    // (One `.child()` per builder — terminal — so the dot attaches
    // conditionally before the label joins.)
    let indicator = if props.selected {
        indicator.child(
            Div("radio-dot")
                .style(
                    Style::new()
                        .size(8, 8)
                        .circle()
                        .bg(t.primary)
                        .x(5)
                        .absolute_y(5),
                )
                .build(),
        )
    } else {
        indicator.build()
    };
    let builder = Row("radio")
        .style(
            Style::new()
                .pad_x(4)
                .pad_y(2)
                .gap(8)
                .align_items(AlignItems::Center),
        )
        .semantics(semantics);
    // Handlers are payload-less ids (ADR-0007): capture the `Rc` action.
    let builder = if props.enabled {
        let select = props.on_select.clone();
        builder.on_press(move || select())
    } else {
        builder
    };
    builder.children([
        indicator,
        VNode::from(Text {
            text: props.label.clone(),
            style: Text::body_secondary,
        }),
    ])
}

/// RadioGroup: a column of [`Radio`] options over a controlled
/// `Signal<T>` — clicking an option sets the signal to that option's
/// value, so selection is mutually exclusive by construction (the
/// group owns exclusivity; each radio only reports its own
/// `selected`). Options mount through `ctx.child` (own flags per
/// instance — the M8/F6 rule).
pub fn RadioGroup<T: Clone + PartialEq + 'static>(ctx: &Ctx, props: &RadioGroupProps<T>) -> VNode {
    let current = props.selected.get();
    let children = props
        .options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let selected = props.selected.clone();
            let value = opt.value.clone();
            let rp = RadioProps {
                label: opt.label.clone(),
                selected: current == opt.value,
                enabled: props.enabled,
                on_select: action(move || selected.set(value.clone())),
            };
            ctx.child_keyed(i as u64, &rp, Radio)
        })
        .collect::<Vec<_>>();
    // Phase 38a: validation marks ride the group container only when
    // set (valid groups keep the exact tree); the footer stacks
    // under the options only when a message is present.
    let validation_active = props.invalid || props.required || props.error_message.is_some();
    let root = if validation_active {
        Column::new()
            .semantics(with_validation_marks(
                Semantics::default(),
                props.invalid,
                props.required,
                &props.error_message,
            ))
            .children(children)
    } else {
        Column::new().children(children)
    };
    with_validation_footer(
        root,
        ctx,
        "radio-group",
        props.invalid,
        &props.error_message,
        &props.helper_text,
    )
}
