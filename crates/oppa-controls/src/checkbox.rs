use crate::common::*;
use oppa::*;
// ---------------------------------------------------------------------------
// Checkbox
// ---------------------------------------------------------------------------

/// Checkbox props (controlled — `checked` flips on press).
/// `on_change` reports flips (round 5.4 — `None` keeps the exact
/// pre-5.4 behavior); see [`UncontrolledCheckbox`] for the
/// self-managed companion.
///
/// Form validation (Phase 38a, decision 366 — G7): `invalid` /
/// `required` / `error_message` announce through [`Semantics`]
/// (validators stay app-side); `helper_text` renders a dimmed
/// caption under the row (visual-only, never announced). All four
/// default off/empty — plain checkboxes keep byte-identical trees.
#[derive(Clone)]
pub struct CheckboxProps {
    pub label: SharedString,
    pub checked: Signal<bool>,
    pub enabled: bool,
    pub on_change: Option<Change<bool>>,
    pub invalid: bool,
    pub required: bool,
    pub error_message: Option<SharedString>,
    pub helper_text: Option<SharedString>,
    /// Box edge in px (default 20.0 — the shipped box; authors may
    /// shrink it, e.g. 18.0, without touching shared pixels
    /// elsewhere; the vector check scales with it, see below).
    pub box_px: f32,
    /// Box corner radius in px (default 4.0 — the shipped radius).
    pub box_radius: f32,
    /// Label text class (default `Text::body_secondary` — the shipped
    /// label; authors may pass a `Custom` size, e.g. 15px).
    pub label_style: TextClass,
    /// Label ink override (default `None` — the theme's text ink;
    /// authors may pass a dimmed ink for secondary labels).
    pub label_ink: Option<Color>,
    /// Check-mark ink override (default `None` — literal white, the
    /// theme-invariant contrast ink on the saturated fill; authors
    /// may pass a dark ink for a dark-on-accent mark).
    pub check_ink: Option<Color>,
    /// Check-mark scale about the box center (default 1.0 — the mark
    /// fills the box; authors may shrink it, e.g. 0.85, with the
    /// stroke scaling along so the mark stays proportional).
    pub check_scale: f32,
}

impl Props for CheckboxProps {}

/// Checkbox: 20×20 box by default (`box_px`, exact pre-change pixels
/// when 20.0) + visible label (`Text::body_secondary` unless
/// `label_style` says otherwise, theme ink unless `label_ink` says
/// otherwise), `checkbox` role with
/// live `checked`; press flips (no-op when disabled). The row pads 4
/// around with a 10px box-to-label gap. The box fills
/// Primary with a vector check when on (a real [`Path`] — the shipped
/// 2.5px white stroke through (4.5, 10.5) → (8.5, 14.5) → (15.5, 6),
/// decision 291 — scaled by `box_px / 20` (times `check_scale`,
/// recentered, so smaller boxes and marks keep the same curves
/// proportionally) in `check_ink` when set, replacing the
/// stepped-square arms, so the mark is resolution-independent
/// curves, not pixels or font glyphs), stays
/// white with a gray border when off, and washes to EE when disabled
/// (strictly the catalog palette — Primary / Dim gray / white / EE,
/// no new colors). While held, a checked box deepens to the pressed
/// tint (Round 7.15 tactile feedback); the label rides beside the box
/// instead of inside a text mark. Fills ride the app theme (Round
/// 11.2); the check stays literal white (contrast ink on the
/// saturated accent — theme-invariant by design, both palettes keep
/// `primary` saturated for exactly this).
pub fn Checkbox(ctx: &Ctx, props: &CheckboxProps) -> VNode {
    let t = ctx.theme().tokens();
    let checked = props.checked.get();
    let semantics = with_validation_marks(
        Semantics::checkbox()
            .checked(checked)
            .label(&props.label)
            .disabled(!props.enabled),
        props.invalid,
        props.required,
        &props.error_message,
    );
    let press = if props.enabled {
        let (checked, notify) = (props.checked.clone(), props.on_change.clone());
        Some(action(move || {
            let next = !checked.get();
            checked.set(next);
            if let Some(notify) = notify.as_ref() {
                notify(next);
            }
        }))
    } else {
        None
    };
    // Pressed flag rides this instance (each checkbox tints alone;
    // disabled never flags by router construction).
    let pressed = props.enabled && ctx.pressed().get();
    // Check ink stays literal white (theme-invariant contrast ink —
    // see the control docs).
    let (fill, edge, ink) = if !props.enabled {
        (t.disabled, t.text_secondary, t.text_secondary)
    } else if checked {
        let fill = if pressed {
            t.primary_pressed
        } else {
            t.primary
        };
        (fill, fill, CONTRAST_INK)
    } else {
        (t.surface, t.border, CONTRAST_INK)
    };
    // Phase 38a: an invalid box edges in `error` (announced through
    // the payload above); valid boxes keep the exact palette.
    let edge = if props.invalid { t.error } else { edge };
    let box_style = focus_ringed(
        ctx,
        props.enabled,
        Style::new()
            .size(props.box_px, props.box_px)
            .radius(props.box_radius)
            .bg(fill)
            .border(2, edge),
    );
    // (One `.child()`/`.children()` per builder — terminal — so the
    // check variant builds whole, like the Radio dot.)
    // The check scales with the box (k = 1 at the shipped 20px, where
    // the formatted data reproduces the exact shipped path string)
    // times `check_scale`, recentered so a shrunken mark stays
    // centered — the stroke scales along, keeping it proportional.
    let k = props.box_px / 20.0 * props.check_scale;
    let off = (props.box_px - 20.0 * k) / 2.0;
    let check_data = format!(
        "M {} {} L {} {} L {} {}",
        4.5 * k + off,
        10.5 * k + off,
        8.5 * k + off,
        14.5 * k + off,
        15.5 * k + off,
        6.0 * k + off
    );
    let check_ink = props.check_ink.unwrap_or(ink);
    let box_vnode = if checked {
        // Vector check (decision 291): one smooth stroked path filling
        // the box exactly (in-flow, same size — no extent change, no
        // offsets to drift). Solid stroke needs no ink beyond its own
        // color; no font runs anywhere near it, so cross-OS metric
        // variance cannot nudge it — the Round-7.15 nitpick closed by
        // construction, now as curves instead of stepped squares.
        Div("checkbox-box").style(box_style).child(
            Path::new("checkbox-check")
                .data(&check_data)
                .stroke(check_ink, 2.5 * k)
                .size(props.box_px, props.box_px)
                .build(),
        )
    } else {
        Div("checkbox-box").style(box_style).build()
    };
    let builder = Row("checkbox")
        .style({
            // Round 8.3: enabled rows show the hand (disabled stays
            // the platform arrow — see `Button`).
            let mut s = Style::new()
                .pad_x(4)
                .pad_y(4)
                .gap(10)
                .align_items(AlignItems::Center);
            if props.enabled {
                s = s.cursor(CursorIcon::Pointer);
            }
            s
        })
        .semantics(semantics);
    // Handlers are payload-less ids (ADR-0007): capture the `Rc` action.
    // (The whole row is pressable — a bigger hit target than the box.)
    let builder = match press {
        Some(press) => builder.on_press(move || press()),
        None => builder,
    };
    let label_leaf = VNode::from(Text {
        text: props.label.clone(),
        style: props.label_style,
    });
    let label_leaf = match props.label_ink {
        Some(ink) => with_ink(label_leaf, ink),
        None => label_leaf,
    };
    let root = builder.children([box_vnode, label_leaf]);
    // Phase 38a: validation footer stacks under the row only when a
    // message is present (valid rows keep the exact tree).
    with_validation_footer(
        root,
        ctx,
        "checkbox",
        props.invalid,
        &props.error_message,
        &props.helper_text,
    )
}

/// Uncontrolled checkbox (round 5.4, OQ-G2-4): self-managed
/// `checked` from `initial`, flips reported through `on_change`.
/// See [`UncontrolledToggle`] for the contract notes.
#[derive(Clone)]
pub struct UncontrolledCheckboxProps {
    pub label: SharedString,
    pub initial: bool,
    pub enabled: bool,
    pub on_change: Option<Change<bool>>,
    pub invalid: bool,
    pub required: bool,
    pub error_message: Option<SharedString>,
    pub helper_text: Option<SharedString>,
    pub box_px: f32,
    pub box_radius: f32,
    pub label_style: TextClass,
    pub label_ink: Option<Color>,
    pub check_ink: Option<Color>,
    pub check_scale: f32,
}

impl Props for UncontrolledCheckboxProps {}

pub fn UncontrolledCheckbox(ctx: &Ctx, props: &UncontrolledCheckboxProps) -> VNode {
    let checked = ctx.signal(props.initial);
    ctx.child_auto(
        &CheckboxProps {
            label: props.label.clone(),
            checked,
            enabled: props.enabled,
            on_change: props.on_change.clone(),
            invalid: props.invalid,
            required: props.required,
            error_message: props.error_message.clone(),
            helper_text: props.helper_text.clone(),
            box_px: props.box_px,
            box_radius: props.box_radius,
            label_style: props.label_style,
            label_ink: props.label_ink,
            check_ink: props.check_ink,
            check_scale: props.check_scale,
        },
        Checkbox,
    )
}
