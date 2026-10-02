use crate::common::*;
use oppa::*;
// ---------------------------------------------------------------------------
// Toggle
// ---------------------------------------------------------------------------

/// Toggle props (controlled — `on` flips on press; the M5 §4.1 pattern
/// as a shipped control). `on_change` reports flips (round 5.4 —
/// `None` keeps the exact pre-5.4 behavior); see
/// [`UncontrolledToggle`] for the self-managed companion.
///
/// Form validation (Phase 38a, decision 366 — G7): same four marks
/// as [`CheckboxProps`] (announced `invalid` / `required` /
/// `error_message`, visual `helper_text`; all default off/empty).
#[derive(Clone)]
pub struct ToggleProps {
    pub label: SharedString,
    pub on: Signal<bool>,
    pub enabled: bool,
    pub on_change: Option<Change<bool>>,
    pub invalid: bool,
    pub required: bool,
    pub error_message: Option<SharedString>,
    pub helper_text: Option<SharedString>,
}

impl Props for ToggleProps {}

/// Toggle switch: pill track + sliding knob + visible label,
/// `switch` role with live `checked`; press flips (no-op when
/// disabled). The knob pins like the Radio dot (20px knob in a
/// 44×24 track: x = 2 off / 22 on, y = 2); the track fills Primary
/// when on, Dim gray when off, EE when disabled (catalog palette —
/// the label used to be semantics-only while the track read
/// "on"/"off" text; now the label shows and the track is silent).
/// While held, an on-track deepens to the pressed tint (Round 7.15).
/// Track fills ride the app theme (Round 11.2); the knob stays
/// literal white (chrome, theme-invariant like the check mark).
pub fn Toggle(ctx: &Ctx, props: &ToggleProps) -> VNode {
    let t = ctx.theme().tokens();
    let on = props.on.get();
    let semantics = with_validation_marks(
        Semantics::switch()
            .checked(on)
            .label(&props.label)
            .disabled(!props.enabled),
        props.invalid,
        props.required,
        &props.error_message,
    );
    let press = if props.enabled {
        let (on, notify) = (props.on.clone(), props.on_change.clone());
        Some(action(move || {
            let next = !on.get();
            on.set(next);
            if let Some(notify) = notify.as_ref() {
                notify(next);
            }
        }))
    } else {
        None
    };
    // Pressed flag rides this instance (each switch tints alone;
    // disabled never flags by router construction).
    let pressed = props.enabled && ctx.pressed().get();
    let track = Div("toggle-track")
        .style(Style::new().size(44, 24).radius(12).bg(if !props.enabled {
            t.disabled
        } else if on {
            if pressed {
                t.primary_pressed
            } else {
                t.primary
            }
        } else {
            t.text_secondary
        }))
        .child(
            Div("toggle-knob")
                .style(
                    Style::new()
                        .size(20, 20)
                        .circle()
                        .bg(CONTRAST_INK)
                        .x(if on { 22 } else { 2 })
                        .absolute_y(2),
                )
                .build(),
        );
    let builder = Row("toggle")
        .style(focus_ringed(ctx, props.enabled, {
            // Round 8.3: enabled rows show the hand (disabled stays
            // the platform arrow — see `Button`).
            let mut s = Style::new()
                .pad_x(4)
                .pad_y(2)
                .gap(8)
                .align_items(AlignItems::Center);
            if props.enabled {
                s = s.cursor(CursorIcon::Pointer);
            }
            s
        }))
        .semantics(semantics);
    // Handlers are payload-less ids (ADR-0007): capture the `Rc` action.
    // (The whole row is pressable — a bigger hit target than the track.)
    let builder = match press {
        Some(press) => builder.on_press(move || press()),
        None => builder,
    };
    let root = builder.children([
        track,
        VNode::from(Text {
            text: props.label.clone(),
            style: Text::body_secondary,
        }),
    ]);
    // Phase 38a: validation footer only when a message is present.
    with_validation_footer(
        root,
        ctx,
        "toggle",
        props.invalid,
        &props.error_message,
        &props.helper_text,
    )
}

/// Uncontrolled toggle (round 5.4, OQ-G2-4): self-managed `on`
/// signal from `initial`, flips reported through `on_change`
/// (payload-carrying — the author holds no signal, so notify-
/// without-value would be useless). For fire-and-forget UI omit
/// `on_change`; for synced UI prefer controlled [`Toggle`].
#[derive(Clone)]
pub struct UncontrolledToggleProps {
    pub label: SharedString,
    pub initial: bool,
    pub enabled: bool,
    pub on_change: Option<Change<bool>>,
    pub invalid: bool,
    pub required: bool,
    pub error_message: Option<SharedString>,
    pub helper_text: Option<SharedString>,
}

impl Props for UncontrolledToggleProps {}

pub fn UncontrolledToggle(ctx: &Ctx, props: &UncontrolledToggleProps) -> VNode {
    let on = ctx.signal(props.initial);
    ctx.child_auto(
        &ToggleProps {
            label: props.label.clone(),
            on,
            enabled: props.enabled,
            on_change: props.on_change.clone(),
            invalid: props.invalid,
            required: props.required,
            error_message: props.error_message.clone(),
            helper_text: props.helper_text.clone(),
        },
        Toggle,
    )
}
