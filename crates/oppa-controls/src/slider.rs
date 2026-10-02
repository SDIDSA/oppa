use crate::button::{Button, ButtonProps};
use crate::common::*;
use oppa::*;
// ---------------------------------------------------------------------------
// Slider
// ---------------------------------------------------------------------------

/// Slider props (controlled — `value` in `[min, max]`, step-snapped).
/// `on_change` reports every internal set (round 5.4 — `None`
/// keeps the exact pre-5.4 behavior); see [`UncontrolledSlider`]
/// for the self-managed companion.
///
/// Form validation (Phase 38a, decision 366 — G7): same four marks
/// as [`CheckboxProps`]; the numeric range announces through
/// `value_num` / `min_value` / `max_value` (Phase 38a, decision
/// 367 — G18) alongside the human `value_text`.
#[derive(Clone)]
pub struct SliderProps {
    pub label: SharedString,
    pub value: Signal<f32>,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub enabled: bool,
    pub on_change: Option<Change<f32>>,
    pub invalid: bool,
    pub required: bool,
    pub error_message: Option<SharedString>,
    pub helper_text: Option<SharedString>,
}

impl Props for SliderProps {}

/// Snaps `v` to the step grid from `min`, then clamps to `[min, max]`
/// (pure — headless-testable; panics loudly on non-positive step or
/// `max < min`, never a silent misbehave).
pub fn snap_value(v: f32, min: f32, max: f32, step: f32) -> f32 {
    if step.is_nan() || step <= 0.0 {
        panic!("slider step must be positive, got {step} — refused, never silent");
    }
    if max < min {
        panic!("slider max ({max}) < min ({min}) — refused, never silent");
    }
    let steps = ((v - min) / step).round();
    (min + steps * step).clamp(min, max)
}

/// Renders `value` for the `value_text` announcement (whole numbers
/// without decimals — `"50 percent"`, not `"50.0 percent"`).
pub fn value_text(value: f32) -> String {
    if value == value.trunc() {
        format!("{} percent", value as i64)
    } else {
        format!("{value:.1} percent")
    }
}

/// Sets a snapped slider value and reports it (round 5.4 — the
/// single funnel for the step/drag/arrow sites, so notification
/// cannot drift from any one path).
fn apply_slider_value(
    value: &Signal<f32>,
    notify: &Option<Change<f32>>,
    v: f32,
    min: f32,
    max: f32,
    step: f32,
) {
    let snapped = snap_value(v, min, max, step);
    value.set(snapped);
    if let Some(notify) = notify.as_ref() {
        notify(snapped);
    }
}

/// Slider: decrement button + `slider` role track + increment button
/// (round 5.3, OQ-G2-1 — the track drags and arrows step: press
/// focuses + captures, moves set the value from the pointer x over
/// the track box, Left/Down step down and Right/Up step up; held
/// arrows repeat-step). Phase 38a (decision 367): the thumb drag is
/// the continuous pointer-capture stream (`Down` captures on the
/// root press, every `Move` re-maps through `capture_position`,
/// `Up`/`Cancel` clears — the 2D Scrollbar precedent, decision
/// 354); `Home`/`End` jump to `min`/`max`; the numeric range
/// announces through `value_num`/`min_value`/`max_value` (G18) next
/// to the human `value_text`. Steps snap and clamp; ends are quiet
/// no-ops (a step past the end re-sets the same value — signals dedup
/// downstream... precisely: `Signal::set` always invalidates; the
/// value is unchanged so memos gate it out).
pub fn Slider(ctx: &Ctx, props: &SliderProps) -> VNode {
    let v = snap_value(props.value.get(), props.min, props.max, props.step);
    let semantics = with_validation_marks(
        Semantics::slider()
            .label(&props.label)
            .value_text(&value_text(v))
            .value_num(v)
            .min_value(props.min)
            .max_value(props.max)
            .disabled(!props.enabled),
        props.invalid,
        props.required,
        &props.error_message,
    );
    let dec_props = ButtonProps {
        label: SharedString::from("-"),
        enabled: props.enabled,
        width: 32.0,
        height: 32.0,
        debug: SharedString::from("step-dec"),
        on_press: {
            let (value, min, max, step, notify) = (
                props.value.clone(),
                props.min,
                props.max,
                props.step,
                props.on_change.clone(),
            );
            action(move || apply_slider_value(&value, &notify, value.get() - step, min, max, step))
        },
    };
    let inc_props = ButtonProps {
        label: SharedString::from("+"),
        enabled: props.enabled,
        width: 32.0,
        height: 32.0,
        debug: SharedString::from("step-inc"),
        on_press: {
            let (value, min, max, step, notify) = (
                props.value.clone(),
                props.min,
                props.max,
                props.step,
                props.on_change.clone(),
            );
            action(move || apply_slider_value(&value, &notify, value.get() + step, min, max, step))
        },
    };
    // Track drag (round 5.3): the pointer x over the track box maps
    // to the value range (snapped + clamped — out-of-box drags pin
    // the ends, never wrap). Single-drag v1 bound: concurrent
    // multi-finger drags resolve to the primary capture (stated in
    // the `on_drag` docs).
    let host = ctx.host();
    let (d_value, d_min, d_max, d_step, d_notify) = (
        props.value.clone(),
        props.min,
        props.max,
        props.step,
        props.on_change.clone(),
    );
    let drag_action = move || {
        let Some((x, _)) = host.capture_position() else {
            return;
        };
        // Maps over the trackbox (Round 7.15 reflow — the rail box,
        // not the full row: steppers flank it now).
        let track = oppa::find_retained_by_debug(&host, "slider-trackbox")
            .into_iter()
            .next()
            .expect("slider trackbox retained");
        let b = host.committed_box(track).expect("slider trackbox laid out");
        let frac = ((x - b.x) / b.w).clamp(0.0, 1.0);
        apply_slider_value(
            &d_value,
            &d_notify,
            d_min + frac * (d_max - d_min),
            d_min,
            d_max,
            d_step,
        );
    };
    // Arrow steps (round 5.3): directional handlers on the focused
    // track (held keys repeat-step through the router). Phase 38a:
    // Home/End jump to the range ends (the text-field caret
    // precedent — same keys, range ends instead of content ends).
    let step_action = |dir: f32| {
        let (value, min, max, step, notify) = (
            props.value.clone(),
            props.min,
            props.max,
            props.step,
            props.on_change.clone(),
        );
        move || apply_slider_value(&value, &notify, value.get() + dir * step, min, max, step)
    };
    let jump_action = |to_min: bool| {
        let (value, min, max, step, notify) = (
            props.value.clone(),
            props.min,
            props.max,
            props.step,
            props.on_change.clone(),
        );
        move || {
            apply_slider_value(
                &value,
                &notify,
                if to_min { min } else { max },
                min,
                max,
                step,
            )
        }
    };
    let kids = [
        ctx.child_auto(&dec_props, Button),
        ctx.child_auto(&inc_props, Button),
    ];
    // Visible rail + fill + knob (Round 7.10 — the track used to be
    // an unfilled box; Round 7.15 reflows the steppers to flank the
    // rail): the root stays Div("slider") 160×32 with all handlers
    // (no identity churn — the 7.10 lesson), holding one Row of
    // [dec 32px | 96px trackbox | inc 32px]; rail/fill/knob pin
    // trackbox-relative like the Radio dot, carry no handlers
    // (presses climb to the root), and the drag map reads the
    // trackbox. Fill covers the value fraction in Primary (Dim gray
    // when disabled); the 16px knob rides at the fraction edge.
    let frac = ((v - props.min) / (props.max - props.min)).clamp(0.0, 1.0);
    let t = ctx.theme().tokens();
    let (fill_ink, knob_edge) = if props.enabled {
        (t.primary, t.primary)
    } else {
        (t.text_secondary, t.text_secondary)
    };
    let rail = Div("slider-rail")
        .style(
            Style::new()
                .size(96, 6)
                .radius(3)
                .bg(t.disabled)
                .x(0)
                .absolute_y(13),
        )
        .build();
    let fill = Div("slider-fill")
        .style(
            Style::new()
                .size((96.0 * frac).clamp(0.0, 96.0), 6)
                .radius(3)
                .bg(fill_ink)
                .x(0)
                .absolute_y(13),
        )
        .build();
    let knob = Div("slider-knob")
        .style(
            Style::new()
                .size(16, 16)
                .circle()
                .bg(CONTRAST_INK)
                .border(2, knob_edge)
                .x(frac * (96.0 - 16.0))
                .absolute_y(8),
        )
        .build();
    if !props.enabled {
        // Disabled drops every handler structurally (decision 213 —
        // no tab stop either) but keeps the dimmed buttons.
        // (VNodes are move-only — destructure, never clone.)
        let [dec, inc] = kids;
        let trackbox = Div("slider-trackbox")
            .style(Style::new().size(96, 32))
            .children([rail, fill, knob]);
        let row = Row("slider-row")
            .style(Style::new().size(160, 32).align_items(AlignItems::Center))
            .children([dec, trackbox, inc]);
        let root = Div("slider")
            .style(focus_ringed(
                ctx,
                props.enabled,
                Style::new().size(160.0, 32.0),
            ))
            .semantics(semantics)
            .child(row);
        // Phase 38a: validation footer rides the disabled path too
        // (announcement + message never depend on enabled).
        return with_validation_footer(
            root,
            ctx,
            "slider",
            props.invalid,
            &props.error_message,
            &props.helper_text,
        );
    }
    let [dec, inc] = kids;
    let trackbox = Div("slider-trackbox")
        .style(Style::new().size(96, 32))
        .children([rail, fill, knob]);
    let row = Row("slider-row")
        .style(Style::new().size(160, 32).align_items(AlignItems::Center))
        .children([dec, trackbox, inc]);
    let root = Div("slider")
        .style(focus_ringed(
            ctx,
            props.enabled,
            Style::new().size(160.0, 32.0),
        ))
        .semantics(semantics)
        // Focus-only press (capture owner for drags + tab stop +
        // focus for arrows — the router does the work, the closure
        // is intentionally empty).
        .on_press(|| {})
        .on_drag(drag_action)
        .on_key_left(step_action(-1.0))
        .on_key_down(step_action(-1.0))
        .on_key_right(step_action(1.0))
        .on_key_up(step_action(1.0))
        .on_key_home(jump_action(true))
        .on_key_end(jump_action(false))
        .child(row);
    // Phase 38a: validation footer only when a message is present.
    with_validation_footer(
        root,
        ctx,
        "slider",
        props.invalid,
        &props.error_message,
        &props.helper_text,
    )
}

/// Uncontrolled slider (round 5.4, OQ-G2-4): self-managed `value`
/// from `initial` (snapped on first read through the normal step
/// path — an off-grid initial snaps on the first interaction, the
/// controlled snap rule, unchanged), every set reported through
/// `on_change`. See [`UncontrolledToggle`] for the contract notes.
#[derive(Clone)]
pub struct UncontrolledSliderProps {
    pub label: SharedString,
    pub initial: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub enabled: bool,
    pub on_change: Option<Change<f32>>,
    pub invalid: bool,
    pub required: bool,
    pub error_message: Option<SharedString>,
    pub helper_text: Option<SharedString>,
}

impl Props for UncontrolledSliderProps {}

pub fn UncontrolledSlider(ctx: &Ctx, props: &UncontrolledSliderProps) -> VNode {
    let value = ctx.signal(props.initial);
    ctx.child_auto(
        &SliderProps {
            label: props.label.clone(),
            value,
            min: props.min,
            max: props.max,
            step: props.step,
            enabled: props.enabled,
            on_change: props.on_change.clone(),
            invalid: props.invalid,
            required: props.required,
            error_message: props.error_message.clone(),
            helper_text: props.helper_text.clone(),
        },
        Slider,
    )
}
