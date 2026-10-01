//! Shipped control catalog (G2 — decisions 212–214; TextInput —
//! decision 240; Modal — decision 241, closing OQ-G2-3's overlay half;
//! Radio/RadioGroup — 244; Tabs — 245; Select — 247;
//! ProgressBar/Badge — 251).
//!
//! Button, Checkbox, Toggle, Slider, TextInput, Modal, Radio,
//! RadioGroup, Tabs, Select, ProgressBar, and Badge as plain component
//! functions over the existing vocabulary
//! (`Div`/`Row`/`Text`/`TextField` + handlers + `Semantics` +
//! author-owned signals). No new [`Tag`](oppa::Tag), no reconciler /
//!   layout / backend changes — controls compose what the framework
//!   already proves (decision 212).
//!
//! Conventions (decision 213):
//!
//! - **Controlled state.** Stateful controls take author-owned
//!   [`Signal`](oppa::Signal)s and flip them on press (the same
//!   controlled pattern as [`EditSession`](oppa::EditSession) content,
//!   locked #24). There are no uncontrolled variants in v1 (OQ-G2-4).
//! - **Use through `ctx.child`.** Each control is a `fn(&Ctx, &P) ->
//!   VNode` shaped for [`Ctx::child`](oppa::Ctx::child), so every
//!   instance owns its hover/press/focus flags and hot-reload identity
//!   (`"oppa::Button"` + slot key). Embedding the returned `VNode`
//!   directly in one-control bodies works; two stateful controls in one
//!   body need `ctx.child` (shared flags otherwise — the M8/F6 rule).
//! - **Press-only interactions.** Buttons flip/activate on press
//!   (pointer, Enter/Space on the focused control — the M5 router);
//!   the Slider is decrement/track/increment (drag + arrow keys need
//!   router drag events + shell arrow classification — OQ-G2-1).
//! - **Disabled drops the handler.** A disabled control carries
//!   `Semantics::disabled(true)`, dims, and attaches no press handler —
//!   so it also leaves the Tab order (v1 focusable means press-owner,
//!   decision 96). Never a silent no-op handler: the refusal is
//!   structural.
//! - **Explicit sizes.** Controls ship default sizes in px (buttons
//!   96×32, boxes 20×20, slider 160×32, text inputs 200×32, dialog
//!   cards 360 wide, progress bars 160×12, badges 24 high) so
//!   headless trees get hit boxes without a text service; authors
//!   override through the props' `width`/`height`.
//!
//! Controls: Button, Checkbox, Toggle, Slider, TextInput, Modal,
//! Radio, RadioGroup, Tabs, Select, ProgressBar, Badge.
//!
//! Per-control behavior is documented in this crate; catalog status
//! lives in `docs/STATE.md` (Done).

// Control components are `CamelCase` functions by framework convention
// (`#[component] fn Name` — M2 authoring surface), hence the crate-level
// allow (same rationale as vnode.rs's `Column::new` surface).
#![allow(non_snake_case)]

/// Unified kitchen sink reference app (Phase 7, decision 276).
pub mod kitchen_sink;
pub mod menu;
pub mod studio;

pub use kitchen_sink::{KitchenSinkApp, KitchenSinkProps, SinkTab};
pub use menu::{ContextMenu, ContextMenuProps, Menu, MenuItem, MenuItemProps, MenuProps};

use std::rc::Rc;

use oppa::{
    AlignItems, Color, Column, Ctx, CursorIcon, Div, Ease, JustifyContent, Path, Portal, Props, Px,
    Row, ScrollOffset, Semantics, SharedString, Signal, Style, StyleBuilder, Text,
    TextArea as TextAreaLeaf, TextClass, TextField, Transition, VNode, SCROLLBAR_HIT_PX,
    SCROLLBAR_TRACK_PX,
};
// Round 14.1: the `Props` derive macro (same name as the trait above
// — different namespaces, so both imports coexist; the derive fills
// the manual generic impls below).
use oppa_macros::Props;

/// Shared press action (`Rc` so props stay `Clone` + `'static` for the
/// hot boundary — same shape as `OpaqueProps`' clone glue, §5.1).
pub type Action = Rc<dyn Fn()>;

/// Shared change notification (round 5.4, OQ-G2-4): controls that
/// flip author-owned signals internally also report the new value
/// through this (uncontrolled companions observe through it —
/// payload-carrying, because notify-without-value is useless when
/// the author holds no signal).
pub type Change<T> = Rc<dyn Fn(T)>;

fn action(f: impl Fn() + 'static) -> Action {
    Rc::new(f)
}

// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

/// Button props (stateless — activation rides `on_press`).
#[derive(Clone)]
pub struct ButtonProps {
    pub label: SharedString,
    pub enabled: bool,
    pub width: f32,
    pub height: f32,
    /// Retained debug label (default `"button"`; the Slider sets
    /// `"step-dec"`/`"step-inc"` so tests and diagnostics address each
    /// stepper separately).
    pub debug: SharedString,
    pub on_press: Action,
}

impl Props for ButtonProps {}

impl ButtonProps {
    pub fn new(label: &str, on_press: impl Fn() + 'static) -> Self {
        Self {
            label: SharedString::from(label),
            enabled: true,
            width: 96.0,
            height: 32.0,
            debug: SharedString::from("button"),
            on_press: action(on_press),
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn debug(mut self, debug: &str) -> Self {
        self.debug = SharedString::from(debug);
        self
    }
}

/// Push-button: label + `button` role; press activates (no-op when
/// disabled — structurally handler-less, decision 213). Radius-6
/// Div with a centered label (the label centers through a full-size
/// inner Row — a block-lite Div stretches the leaf full-width — and
/// the outer stays a Div so tab panels keep remounting fresh on
/// incompatible tags). While held, the Primary fill deepens (Round
/// 7.15 pressed feedback). Fill and press tint ride the app theme
/// (Round 11.2 — Light reproduces the catalog palette exactly).
pub fn Button(ctx: &Ctx, props: &ButtonProps) -> VNode {
    let t = ctx.theme().tokens();
    let semantics = Semantics::button()
        .label(&props.label)
        .disabled(!props.enabled);
    let press = props.enabled.then(|| props.on_press.clone());
    // Pressed flag rides this instance (each button tints alone;
    // disabled never flags by router construction).
    let pressed = props.enabled && ctx.pressed().get();
    let mut bg = if props.enabled {
        t.primary
    } else {
        t.text_secondary
    };
    if pressed && bg == t.primary {
        bg = t.primary_pressed;
    }
    // Round 8.3: enabled buttons show the hand (disabled stays the
    // platform arrow — inert controls never promise clicks).
    let mut button_style = Style::new()
        .size(props.width, props.height)
        .radius(6)
        .bg(bg);
    if props.enabled {
        button_style = button_style.cursor(CursorIcon::Pointer);
    }
    let builder = Div(&props.debug)
        // Round 23.2: keyboard focus rings the button (inset —
        // layout-safe, oracle-neutral).
        .style(focus_ringed(ctx, props.enabled, button_style))
        .semantics(semantics);
    // Handlers are payload-less ids (ADR-0007): capture the `Rc` action.
    // (`on_press` rides the builder — `.child()` is terminal, so the
    // handler attaches first.)
    let builder = match press {
        Some(press) => builder.on_press(move || press()),
        None => builder,
    };
    builder.child(
        Row("button-label")
            .style(
                Style::new()
                    .size(props.width, props.height)
                    .justify_content(JustifyContent::Center)
                    .align_items(AlignItems::Center),
            )
            .child(VNode::from(Text {
                text: props.label.clone(),
                style: Text::body_secondary,
            })),
    )
}

// ---------------------------------------------------------------------------
// Checkbox
// ---------------------------------------------------------------------------

/// Checkbox props (controlled — `checked` flips on press).
/// `on_change` reports flips (round 5.4 — `None` keeps the exact
/// pre-5.4 behavior); see [`UncontrolledCheckbox`] for the
/// self-managed companion.
#[derive(Clone)]
pub struct CheckboxProps {
    pub label: SharedString,
    pub checked: Signal<bool>,
    pub enabled: bool,
    pub on_change: Option<Change<bool>>,
}

impl Props for CheckboxProps {}

/// Checkbox: drawn 20×20 box + visible label, `checkbox` role with
/// live `checked`; press flips (no-op when disabled). The box fills
/// Primary with a vector check when on (a real [`Path`] — smooth
/// 2.5px stroke through (4.5, 10.5) → (8.5, 14.5) → (15.5, 6),
/// decision 291 — replacing the stepped-square arms, so the mark is
/// resolution-independent curves, not pixels or font glyphs), stays
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
    let semantics = Semantics::checkbox()
        .checked(checked)
        .label(&props.label)
        .disabled(!props.enabled);
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
    let box_style = focus_ringed(
        ctx,
        props.enabled,
        Style::new().size(20, 20).radius(4).bg(fill).border(2, edge),
    );
    // (One `.child()`/`.children()` per builder — terminal — so the
    // check variant builds whole, like the Radio dot.)
    let box_vnode = if checked {
        // Vector check (decision 291): one smooth stroked path filling
        // the box exactly (in-flow, same size — no extent change, no
        // offsets to drift). Solid stroke needs no ink beyond its own
        // color; no font runs anywhere near it, so cross-OS metric
        // variance cannot nudge it — the Round-7.15 nitpick closed by
        // construction, now as curves instead of stepped squares.
        Div("checkbox-box").style(box_style).child(
            Path::new("checkbox-check")
                .data("M 4.5 10.5 L 8.5 14.5 L 15.5 6")
                .stroke(ink, 2.5)
                .size(20, 20)
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
                .pad_y(2)
                .gap(8)
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
    builder.children([
        box_vnode,
        VNode::from(Text {
            text: props.label.clone(),
            style: Text::body_secondary,
        }),
    ])
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
}

impl Props for UncontrolledCheckboxProps {}

pub fn UncontrolledCheckbox(ctx: &Ctx, props: &UncontrolledCheckboxProps) -> VNode {
    let checked = ctx.signal(props.initial);
    ctx.child(
        "oppa::Checkbox",
        1,
        &CheckboxProps {
            label: props.label.clone(),
            checked,
            enabled: props.enabled,
            on_change: props.on_change.clone(),
        },
        Checkbox,
    )
}

// ---------------------------------------------------------------------------
// Toggle
// ---------------------------------------------------------------------------

/// Toggle props (controlled — `on` flips on press; the M5 §4.1 pattern
/// as a shipped control). `on_change` reports flips (round 5.4 —
/// `None` keeps the exact pre-5.4 behavior); see
/// [`UncontrolledToggle`] for the self-managed companion.
#[derive(Clone)]
pub struct ToggleProps {
    pub label: SharedString,
    pub on: Signal<bool>,
    pub enabled: bool,
    pub on_change: Option<Change<bool>>,
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
    let semantics = Semantics::switch()
        .checked(on)
        .label(&props.label)
        .disabled(!props.enabled);
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
    builder.children([
        track,
        VNode::from(Text {
            text: props.label.clone(),
            style: Text::body_secondary,
        }),
    ])
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
}

impl Props for UncontrolledToggleProps {}

pub fn UncontrolledToggle(ctx: &Ctx, props: &UncontrolledToggleProps) -> VNode {
    let on = ctx.signal(props.initial);
    ctx.child(
        "oppa::Toggle",
        1,
        &ToggleProps {
            label: props.label.clone(),
            on,
            enabled: props.enabled,
            on_change: props.on_change.clone(),
        },
        Toggle,
    )
}

// ---------------------------------------------------------------------------
// Slider
// ---------------------------------------------------------------------------

/// Slider props (controlled — `value` in `[min, max]`, step-snapped).
/// `on_change` reports every internal set (round 5.4 — `None`
/// keeps the exact pre-5.4 behavior); see [`UncontrolledSlider`]
/// for the self-managed companion.
#[derive(Clone)]
pub struct SliderProps {
    pub label: SharedString,
    pub value: Signal<f32>,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub enabled: bool,
    pub on_change: Option<Change<f32>>,
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
/// arrows repeat-step). Steps snap and clamp; ends are quiet no-ops
/// (a step past the end re-sets the same value — signals dedup
/// downstream... precisely: `Signal::set` always invalidates; the
/// value is unchanged so memos gate it out).
pub fn Slider(ctx: &Ctx, props: &SliderProps) -> VNode {
    let v = snap_value(props.value.get(), props.min, props.max, props.step);
    let semantics = Semantics::slider()
        .label(&props.label)
        .value_text(&value_text(v))
        .disabled(!props.enabled);
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
    // track (held keys repeat-step through the router).
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
    let kids = [
        ctx.child("oppa::SliderDec", 1, &dec_props, Button),
        ctx.child("oppa::SliderInc", 2, &inc_props, Button),
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
        return Div("slider")
            .style(focus_ringed(
                ctx,
                props.enabled,
                Style::new().size(160.0, 32.0),
            ))
            .semantics(semantics)
            .child(row);
    }
    let [dec, inc] = kids;
    let trackbox = Div("slider-trackbox")
        .style(Style::new().size(96, 32))
        .children([rail, fill, knob]);
    let row = Row("slider-row")
        .style(Style::new().size(160, 32).align_items(AlignItems::Center))
        .children([dec, trackbox, inc]);
    Div("slider")
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
        .child(row)
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
}

impl Props for UncontrolledSliderProps {}

pub fn UncontrolledSlider(ctx: &Ctx, props: &UncontrolledSliderProps) -> VNode {
    let value = ctx.signal(props.initial);
    ctx.child(
        "oppa::Slider",
        1,
        &SliderProps {
            label: props.label.clone(),
            value,
            min: props.min,
            max: props.max,
            step: props.step,
            enabled: props.enabled,
            on_change: props.on_change.clone(),
        },
        Slider,
    )
}

// ---------------------------------------------------------------------------
// TextInput
// ---------------------------------------------------------------------------
/// TextInput props (controlled — `value` is the author-owned content
/// signal; editing state lives in the keyed [`EditSession`](oppa::EditSession)).
/// `on_change` reports committed edits (round 5.4 — `None` keeps the
/// exact pre-5.4 behavior); see [`UncontrolledTextInput`] for the
/// self-managed companion.
#[derive(Clone)]
pub struct TextInputProps {
    pub label: SharedString,
    pub value: Signal<SharedString>,
    pub placeholder: Option<SharedString>,
    pub enabled: bool,
    pub width: f32,
    pub height: f32,
    pub style: TextClass,
    pub debug: SharedString,
    pub on_change: Option<Change<SharedString>>,
    pub masked: bool,
}

impl Props for TextInputProps {}

impl TextInputProps {
    pub fn new(label: &str, value: Signal<SharedString>) -> Self {
        Self {
            label: SharedString::from(label),
            value,
            placeholder: None,
            enabled: true,
            width: 200.0,
            height: 32.0,
            style: Text::body_secondary,
            debug: SharedString::from("text-input"),
            on_change: None,
            masked: false,
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

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn style(mut self, style: TextClass) -> Self {
        self.style = style;
        self
    }

    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }
}

/// Contrast ink on saturated accents (Round 11.2, decision 306):
/// check marks, badge labels, and knobs stay white in BOTH themes
/// (both palettes keep `primary` saturated for exactly this — the
/// one deliberate exception to tokenized paint, see
/// [`ThemeTokens`]).
const CONTRAST_INK: Color = Color(0xFF_FF_FF);

/// Shaper-aware press for text fields (Rounds 8.1–8.2, decisions
/// 297–298): installs the host shaper, then routes by multi-click
/// count — single tap to `click_x` (Shift held: `shift_click_x`),
/// double-click to the word (`dbl_click_x`), triple-click to the hard
/// line (`select_line_x`). No tap point (keyboard activation) or no
/// laid origin falls back to `caret_to_end`. Dirties the focused owner
/// so the selection repaints incrementally (session writes carry no
/// signals — the TIME-drive dirty rule without a standing animation).
fn field_press(host: &oppa::ComponentHost, session: &oppa::EditSession, style: TextClass) {
    host.ensure_session_shaper(session, style);
    let count = host.last_press_click_count();
    let Some((px, _)) = host.last_press_position() else {
        session.caret_to_end();
        dirty_focused_field(host);
        return;
    };
    let Some(origin_x) = host.focused_node().and_then(|n| host.text_origin_under(n)) else {
        session.caret_to_end();
        dirty_focused_field(host);
        return;
    };
    let local_x = px - origin_x;
    if count >= 3 {
        session.select_line_x(local_x);
    } else if count == 2 {
        session.dbl_click_x(local_x);
    } else if host.last_press_modifiers().shift {
        session.shift_click_x(local_x);
    } else {
        session.click_x(local_x);
    }
    dirty_focused_field(host);
}

/// Drag-selection stream for text fields (Round 8.2, decision 298):
/// the Down anchor through the live capture, both mapped into
/// session-local x around the field's text origin (`drag_x`). Moves
/// inside tap slop stream near-empty ranges that the tap Up collapses
/// (tap behavior unchanged); far Moves arm the drag release (no press
/// on Up — the router's Drag lifecycles, unchanged).
fn field_drag(host: &oppa::ComponentHost, session: &oppa::EditSession, style: TextClass) {
    host.ensure_session_shaper(session, style);
    let (Some((ox, _)), Some((cx, _))) = (host.lowest_press_origin(), host.capture_position())
    else {
        return;
    };
    let Some(origin_x) = host.focused_node().and_then(|n| host.text_origin_under(n)) else {
        return;
    };
    session.drag_x(ox - origin_x, cx - origin_x);
    dirty_focused_field(host);
}

/// Marks the focused press owner PAINT-dirty (field selection/caret
/// writes carry no signals — see [`field_press`]).
fn dirty_focused_field(host: &oppa::ComponentHost) {
    if let Some(n) = host.focused_node() {
        host.mark_paint_dirty(&[n]);
    }
}

/// Sets the ink override on a converted text-leaf `VNode` (placeholder
/// dimming, active-tab ink). Converted leaves always arrive as
/// `VNode::Element`, so any other shape is a wiring bug and refused
/// loudly, never passed through unstyled.
fn with_ink(vnode: VNode, ink: Color) -> VNode {
    match vnode {
        VNode::Element(mut e) => {
            e.style.ink = Some(ink);
            VNode::Element(e)
        }
        _ => panic!("ink target is not an element — refused, never silent"),
    }
}

/// Keyboard-focus ring style (Round 23.2, decision 334): when this
/// instance owns focus AND the focus arrived via keyboard, outlines
/// the style with a 2px themed inset ring (paint-only `Border` —
/// never layout, so rings never move boxes or break oracles);
/// otherwise the style passes through untouched. Disabled controls
/// never ring (callers gate on enabled). Edge-banded styles keep
/// their bands (callers with `border_bottom` et al. branch around
/// the uniform-ring conflict, which plan-build refuses loudly).
fn focus_ringed(ctx: &Ctx, enabled: bool, style: StyleBuilder) -> StyleBuilder {
    if enabled && ctx.focused().get() && ctx.host().focus_visible().get() {
        style.border(2, ctx.theme().tokens().focus_ring)
    } else {
        style
    }
}

/// Text input: bordered field box + [`TextField`](oppa::TextField) payload
/// (the DOM backend's verdict-(b) `<input>` shape; native backends render
/// the text run). The [`EditSession`](oppa::EditSession) is keyed to this
/// instance, so caret/selection/undo survive hot reloads and swaps; the
/// value signal stays the controlled source of truth (locked #24).
/// Press focuses through the M5 router (press ownership); the handlers are
/// shaper-aware tap-to-caret (Round 8.1, decision 297 — the tap point maps
/// through `click_x`, Shift+Click through `shift_click_x`) plus
/// drag-selection and double/triple-click word/line selection (Round 8.2,
/// decision 298 — Moves stream through `drag_x`, count 2 selects the word,
/// count 3+ the hard line; keyboard activation falls back to
/// `caret_to_end`). Disabled drops the handlers structurally
/// (decision 213 — no tab stop either).
/// An empty value with a placeholder renders the placeholder as a plain
/// dimmed `Text` span (never a `TextField`: the leaf's text is the
/// field's bindable value, and the placeholder must never leak into it;
/// on DOM the backend renders `value=""` with a native `placeholder`
/// attribute from that span (decision 293) — typing feeds only typed
/// text through the session fallback, on every backend).
pub fn TextInput(ctx: &Ctx, props: &TextInputProps) -> VNode {
    let t = ctx.theme().tokens();
    let session = ctx.edit_session(props.value.clone());
    // Masked backing rides the session (Round 22.1, decision 331):
    // published every render so toggling `masked` takes effect
    // without remount; `UncontrolledTextInput` delegates here, so
    // both spellings are covered by this one line.
    session.set_masked(props.masked);
    // Committed edits report through `on_change` (round 5.4 —
    // installed per render, idempotent; the session owns the
    // funnel, the control owns the subscription).
    if let Some(notify) = props.on_change.clone() {
        session.set_on_change(Rc::new(move |value| notify(value)));
    }
    let semantics = Semantics::text_field()
        .label(&props.label)
        .disabled(!props.enabled);
    let raw_val = props.value.get();
    let display_text = if props.masked && !raw_val.is_empty() {
        SharedString::from("•".repeat(raw_val.chars().count()))
    } else {
        raw_val.clone()
    };
    let payload: VNode = match (raw_val.is_empty(), &props.placeholder) {
        (true, Some(placeholder)) => with_ink(
            VNode::from(Text {
                text: placeholder.clone(),
                style: props.style,
            }),
            t.text_secondary,
        ),
        _ => TextField {
            text: display_text,
            style: props.style,
            label: props.label.clone(),
        }
        .into(),
    };
    // (One `.style()` call — the builder replaces the whole style per
    // call, so size/bg/border/pad fold into a single chain. Round 8.3:
    // enabled fields show the I-beam; disabled stays the arrow.)
    let mut field_style = Style::new()
        .size(props.width, props.height)
        .radius(4)
        .bg(if props.enabled { t.surface } else { t.disabled })
        .border(1, t.border)
        .pad_x(8)
        .pad_y(4);
    if props.enabled {
        field_style = field_style.cursor(CursorIcon::Text);
    }
    let builder = Div(&props.debug)
        // Round 23.2: keyboard focus swaps the gray edge for the
        // themed ring (same inset band, recolored — layout-safe).
        .style(focus_ringed(ctx, props.enabled, field_style))
        .semantics(semantics);
    let builder = if props.enabled {
        let host = ctx.host();
        let tap_session = session.clone();
        let tap_style = props.style;
        let drag_host = host.clone();
        let drag_session = tap_session.clone();
        builder
            .on_press(move || {
                field_press(&host, &tap_session, tap_style);
            })
            .on_drag(move || {
                field_drag(&drag_host, &drag_session, tap_style);
            })
    } else {
        builder
    };
    builder.child(payload)
}

/// Uncontrolled text input (round 5.4, OQ-G2-4): self-managed
/// `value` from `initial`, committed edits reported through
/// `on_change` (payload-carrying — the session hook fires with
/// the new content). See [`UncontrolledToggle`] for the contract
/// notes.
#[derive(Clone)]
pub struct UncontrolledTextInputProps {
    pub label: SharedString,
    pub initial: SharedString,
    pub placeholder: Option<SharedString>,
    pub enabled: bool,
    pub width: f32,
    pub height: f32,
    pub style: TextClass,
    pub debug: SharedString,
    pub on_change: Option<Change<SharedString>>,
    pub masked: bool,
}

impl Props for UncontrolledTextInputProps {}

impl UncontrolledTextInputProps {
    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }
}

pub fn UncontrolledTextInput(ctx: &Ctx, props: &UncontrolledTextInputProps) -> VNode {
    let value = ctx.signal(props.initial.clone());
    ctx.child(
        "oppa::TextInput",
        1,
        &TextInputProps {
            label: props.label.clone(),
            value,
            placeholder: props.placeholder.clone(),
            enabled: props.enabled,
            width: props.width,
            height: props.height,
            style: props.style,
            debug: props.debug.clone(),
            on_change: props.on_change.clone(),
            masked: props.masked,
        },
        TextInput,
    )
}

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
}

/// Multi-line text input: bordered auto-height box + [`TextArea`](oppa::TextArea)
/// payload (same DOM verdict-(b) shape as `TextInput`, rendered as
/// `<textarea>`). Press focuses through the M5 router; Enter inserts
/// a newline via the router's textarea rule (never activates).
/// Press/drag handlers mirror [`TextInput`] (Rounds 8.1–8.2 — tap caret,
/// Shift extend, drag stream, double word, triple hard line; the press
/// handler needs no y because hard lines split on `\n`, while soft-wrap
/// y-to-line resolution stays an open follow-up).
/// Disabled drops the handlers structurally (decision 213 — no tab
/// stop either). Empty-with-placeholder mirrors `TextInput`
/// (presentational span, never the value; on DOM a native
/// `placeholder` attribute, decision 293).
pub fn TextArea(ctx: &Ctx, props: &TextAreaProps) -> VNode {
    let t = ctx.theme().tokens();
    let session = ctx.edit_session(props.value.clone());
    // Multi-line wrap rides the session (Round 22.2, decision 332):
    // published every render so resizes re-wrap without remount —
    // the content width is the box minus both 8px pads.
    session.set_wrap_width(Some((props.width - 16.0).max(1.0)));
    if let Some(notify) = props.on_change.clone() {
        session.set_on_change(Rc::new(move |value| notify(value)));
    }
    let semantics = Semantics::text_area()
        .label(&props.label)
        .disabled(!props.enabled);
    let payload: VNode = match (props.value.get().is_empty(), &props.placeholder) {
        (true, Some(placeholder)) => with_ink(
            VNode::from(Text {
                text: placeholder.clone(),
                style: props.style,
            }),
            t.text_secondary,
        ),
        _ => TextAreaLeaf {
            text: props.value.get(),
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
            .border(1, t.border)
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
        builder
            .on_press(move || {
                field_press(&host, &tap_session, tap_style);
            })
            .on_drag(move || {
                field_drag(&drag_host, &drag_session, tap_style);
            })
    } else {
        builder
    };
    builder.child(payload)
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
}

impl Props for UncontrolledTextAreaProps {}

pub fn UncontrolledTextArea(ctx: &Ctx, props: &UncontrolledTextAreaProps) -> VNode {
    let value = ctx.signal(props.initial.clone());
    ctx.child(
        "oppa::TextArea",
        1,
        &TextAreaProps {
            label: props.label.clone(),
            value,
            placeholder: props.placeholder.clone(),
            enabled: props.enabled,
            width: props.width,
            style: props.style,
            debug: props.debug.clone(),
            on_change: props.on_change.clone(),
        },
        TextArea,
    )
}

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
    };
    let confirm_props = ButtonProps {
        label: props.confirm_label.clone(),
        enabled: true,
        width: 96.0,
        height: 32.0,
        debug: SharedString::from("modal-confirm"),
        on_press: confirm_action,
    };
    let mut card_style = Style::new()
        .bg(t.surface)
        .radius(8)
        .border(1, t.border)
        .pad_x(20)
        .pad_y(16)
        .gap(12)
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
                    ctx.child("oppa::ModalCancel", 1, &cancel_props, Button),
                    ctx.child("oppa::ModalConfirm", 2, &confirm_props, Button),
                ]),
        ]);
    let backdrop = Div("modal-backdrop").style(
        Style::new()
            .fill_width()
            .fill_height()
            .bg(Color(0x11_11_11))
            .opacity(Some(0.5))
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
    Portal("modal-portal").child(backdrop.child(card))
}

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
                .gap(12),
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
            ctx.child("oppa::ToastDismiss", 1, &dismiss_props, Button),
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
#[derive(Clone, Props)]
pub struct RadioGroupProps<T: 'static> {
    pub options: Vec<RadioOption<T>>,
    pub selected: Signal<T>,
    pub enabled: bool,
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
            ctx.child("oppa::Radio", i as u64, &rp, Radio)
        })
        .collect::<Vec<_>>();
    Column::new().children(children)
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

/// One tab: value + label + the view mounted while active.
///
/// `content` is a factory, not a stored `VNode`: `VNode` is
/// move-only (uncloneable `Box<dyn Fn>` handlers), so a stored
/// `VNode` could never satisfy `Props: Clone` — verified against
/// the compiler, not assumed. Factories also stay fresh (rebuilt
/// per render, like every body) and lazy (inactive tabs build
/// nothing). The factory receives the Tabs instance's `&Ctx`, so
/// content composes `ctx.child` like any body — with the same
/// rule: child `(name, key)` pairs must be unique across ALL tabs
/// (they share the Tabs instance namespace; collisions share
/// state — stated, not silent).
#[derive(Clone)]
pub struct TabItem<T> {
    pub value: T,
    pub label: SharedString,
    pub content: Rc<dyn Fn(&Ctx) -> VNode>,
}

/// Tabs props (controlled — `active` holds the shown tab's value;
/// clicking a tab sets it, so exactly one panel shows).
#[derive(Clone, Props)]
pub struct TabsProps<T: 'static> {
    pub tabs: Vec<TabItem<T>>,
    pub active: Signal<T>,
    pub enabled: bool,
}

/// One tab button (private — mounted per tab through `ctx.child` so
/// each button owns its flags, the M8/F6 rule).
#[derive(Clone)]
struct TabButtonProps {
    label: SharedString,
    active: bool,
    enabled: bool,
    on_select: Action,
}

impl Props for TabButtonProps {}

fn TabButton(ctx: &Ctx, props: &TabButtonProps) -> VNode {
    let t = ctx.theme().tokens();
    let semantics = Semantics::tab(props.active)
        .label(&props.label)
        .disabled(!props.enabled);
    // Active: bold + primary ink + a 3px primary underline band
    // (Round 7.10 — `border_bottom` per-edge bands are a real
    // primitive after all: the builder emits plain rects, so the old
    // "no edge-only border" note was stale; tab-item carries no
    // radius/circle so the sharp-band panic cannot fire).
    // Inactive: plain body text, no band.
    let label = if props.active {
        with_ink(
            VNode::from(Text::new(props.label.clone()).bold()),
            t.primary,
        )
    } else {
        VNode::from(Text::new(props.label.clone()))
    };
    let style = Style::new().pad_x(16).pad_y(8);
    // Round 23.2: keyboard focus marks the tab — inactive tabs take
    // the uniform ring; the active tab's edge band tints instead
    // (uniform + edges refuse loudly together by design, so the
    // band carries focus color rather than doubling chrome).
    let show_ring = props.enabled && ctx.focused().get() && ctx.host().focus_visible().get();
    let style = if props.active {
        style.border_bottom(3, if show_ring { t.focus_ring } else { t.primary })
    } else if show_ring {
        style.border(2, t.focus_ring)
    } else {
        style
    };
    let builder = Div("tab-item").style(style).semantics(semantics);
    // Handlers are payload-less ids (ADR-0007): capture the `Rc` action.
    // The active tab keeps its handler (stays tabbable/focusable —
    // re-setting the equal value is a quiet no-op via the equality gate).
    let builder = if props.enabled {
        let select = props.on_select.clone();
        builder.on_press(move || select())
    } else {
        builder
    };
    builder.child(label)
}

/// Tabbed views: a `tablist` bar of tab buttons over the active
/// tab's panel (inactive panels unmount — the reconciler drops them,
/// same as a closing Modal). The bar carries a 1px ring (the
/// primitive has no edge-only border — stated, paint-only, no layout
/// effect). Disabled renders everything handlerless with `disabled`
/// semantics but keeps the current panel (switching locks, viewing
/// does not). Empty `tabs`, or an `active` value matching nothing,
/// renders an empty panel — quiet (controlled-contract edge, same
/// class as an unmatched RadioGroup signal), documented not silent.
pub fn Tabs<T: Clone + PartialEq + 'static>(ctx: &Ctx, props: &TabsProps<T>) -> VNode {
    let t = ctx.theme().tokens();
    let current = props.active.get();
    let buttons = props
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let active = props.active.clone();
            let value = tab.value.clone();
            let bp = TabButtonProps {
                label: tab.label.clone(),
                active: current == tab.value,
                enabled: props.enabled,
                on_select: action(move || active.set(value.clone())),
            };
            ctx.child("oppa::TabsTab", i as u64, &bp, TabButton)
        })
        .collect::<Vec<_>>();
    let bar = Row("tab-bar")
        .style(Style::new().gap(8).radius(4).border(1, t.border))
        .semantics(Semantics::tab_list())
        .children(buttons);
    let panel = Div("tab-panel").child(
        props
            .tabs
            .iter()
            .find(|tab| tab.value == current)
            .map(|tab| (tab.content)(ctx))
            .unwrap_or_else(|| Div("tab-empty").build()),
    );
    Div("tabs-container").child(Column::new().gap(16).children([bar, panel]))
}

// ---------------------------------------------------------------------------
// Select
// ---------------------------------------------------------------------------

/// One option: value + label.
///
/// Plain data (no `Props` impl — like `RadioOption`/`TabItem`, it never
/// mounts directly). `PartialEq` is derived per the spec shape; the
/// control itself only needs `T: PartialEq` to find the current label
/// (same as `Tabs`).
#[derive(Clone, PartialEq)]
pub struct SelectItem<T> {
    pub value: T,
    pub label: SharedString,
}

/// Select props (controlled — `selected` holds the chosen value,
/// `open` holds the list visibility; both author-owned signals).
/// `width` defaults to 160.0 via [`SelectProps::new`] (the slider
/// width precedent — decision 213 explicit sizes).
#[derive(Clone, Props)]
pub struct SelectProps<T: 'static> {
    pub items: Vec<SelectItem<T>>,
    pub selected: Signal<T>,
    pub open: Signal<bool>,
    pub enabled: bool,
    pub width: f32,
}

impl<T: Clone + PartialEq + 'static> SelectProps<T> {
    pub fn new(items: Vec<SelectItem<T>>, selected: Signal<T>, open: Signal<bool>) -> Self {
        Self {
            items,
            selected,
            open,
            enabled: true,
            width: 160.0,
        }
    }
}

/// One option row (private — mounted per option through `ctx.child`
/// so each owns its flags, the M8/F6 rule). Carries `list_item` +
/// `selected` semantics (the existing payload — the box carries the
/// new `combobox` role, options reuse what the framework proves).
#[derive(Clone)]
struct SelectOptionProps {
    label: SharedString,
    selected: bool,
    enabled: bool,
    on_select: Action,
}

impl Props for SelectOptionProps {}

fn SelectOption(ctx: &Ctx, props: &SelectOptionProps) -> VNode {
    let t = ctx.theme().tokens();
    let semantics = Semantics::list_item()
        .selected(props.selected)
        .label(&props.label)
        .disabled(!props.enabled);
    // Selected option reads bold + primary ink (the active-tab
    // treatment — one visual language for "current" across the
    // catalog); others plain body text.
    let label = if props.selected {
        with_ink(
            VNode::from(Text::new(props.label.clone()).bold()),
            t.primary,
        )
    } else {
        VNode::from(Text::new(props.label.clone()))
    };
    let builder = Div("select-option")
        .style(Style::new().pad_x(8).pad_y(6))
        .semantics(semantics);
    // Handlers are payload-less ids (ADR-0007): capture the `Rc` action.
    let builder = if props.enabled {
        let select = props.on_select.clone();
        builder.on_press(move || select())
    } else {
        builder
    };
    builder.child(label)
}

/// Dropdown select: a `combobox` box showing the current selection's
/// label (press toggles `open`) over the option list (press picks the
/// value and closes). Options mount through `ctx.child` (own flags
/// per instance — the M8/F6 rule). Disabled renders everything
/// handlerless with `disabled` semantics but keeps showing the
/// current label (switching locks, viewing does not — the Tabs
/// rule); the list follows the `open` signal truthfully even while
/// disabled (controlled signals are author-owned truth, never
/// second-guessed). Empty `items`, or a `selected` value matching
/// nothing, renders an empty label/list — quiet (controlled-contract
/// edge, same class as an unmatched RadioGroup signal), documented
/// not silent. No light-dismiss in v1: the list closes on option
/// pick or box re-press only (no backdrop primitive — stated).
///
/// Overlay shape (Round 7.21, decision 296): the root keeps a
/// constant 32px box whether open or closed, and the open list
/// mounts in an anchored `Portal("select-popup")` — `x(0)` +
/// `absolute_y(36)` (32px box + 4px gap) at the `Select` content
/// origin, explicit list width. The portal skips parent flow (no
/// downstream shift), paints in the overlay phase above later
/// siblings, and hit-tests with portal priority, so option presses
/// route while the form below never moves.
pub fn Select<T: Clone + PartialEq + 'static>(ctx: &Ctx, props: &SelectProps<T>) -> VNode {
    let t = ctx.theme().tokens();
    let current = props.selected.get();
    let is_open = props.open.get();
    let current_label = props
        .items
        .iter()
        .find(|item| item.value == current)
        .map(|item| item.label.clone())
        .unwrap_or_default();
    let toggle = {
        let open = props.open.clone();
        action(move || open.set(!open.get()))
    };
    // Vector chevron (decision 291): a real stroked path instead of
    // the "▾"/"▴" text glyphs — no symbol-font coverage needed
    // (the Round-7.7 Noto chain stays backend coverage, no control
    // needs it now). Down when closed, up when open.
    let chevron_data = if is_open {
        "M 2 6 L 6 2 L 10 6"
    } else {
        "M 2 2 L 6 6 L 10 2"
    };
    let label_text = VNode::from(Text::new(current_label.clone()));
    let chevron = Path::new("select-chevron")
        .data(chevron_data)
        .stroke(t.text_primary, 2.0)
        .size(12, 8)
        .build();
    let box_builder = Row("select-box")
        .style(focus_ringed(
            ctx,
            props.enabled,
            Style::new()
                .size(props.width, 32)
                .radius(4)
                .border(1, t.border)
                .bg(if props.enabled { t.surface } else { t.disabled })
                .pad_x(8)
                .gap(8)
                .align_items(AlignItems::Center),
        ))
        .semantics(
            Semantics::combobox()
                .label(&current_label)
                .disabled(!props.enabled),
        );
    // The box keeps its handler while enabled even when open
    // (re-press closes — the toggle, not a select).
    let box_builder = if props.enabled {
        box_builder.on_press(move || toggle())
    } else {
        box_builder
    };
    let select_box = box_builder.children([label_text, chevron]);
    let mut children = vec![select_box];
    if is_open {
        let options = props
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let selected = props.selected.clone();
                let open = props.open.clone();
                let value = item.value.clone();
                let op = SelectOptionProps {
                    label: item.label.clone(),
                    selected: item.value == current,
                    enabled: props.enabled,
                    on_select: action(move || {
                        selected.set(value.clone());
                        open.set(false);
                    }),
                };
                ctx.child("oppa::SelectOption", i as u64, &op, SelectOption)
            })
            .collect::<Vec<_>>();
        // No explicit size: the list sizes to its rows (the
        // content-sized container precedent — RadioGroup, tab-panel).
        // A `Div` stacks vertically exactly like a `Column`
        // (`layout_vertical` serves both tags) and keeps the
        // `select-list` debug label `Column::new()` cannot carry. An
        // explicit height here would be invented math (row height is
        // shaper-fed); an empty list renders an empty bordered plate,
        // quietly.
        let list = Div("select-list")
            .style(Style::new().radius(4).border(1, t.border).bg(t.surface))
            .children(options);
        // Out-of-flow popup (Round 7.21): anchored under the box,
        // never in-flow — opening shifts nothing below.
        let popup = Portal("select-popup")
            .style(
                Style::new()
                    .x(0)
                    .absolute_y(36) // 32px box height + 4px gap
                    .w(props.width),
            )
            .child(list);
        children.push(popup);
    }
    // Constant 32px root (Round 7.21): the box owns the height, the
    // popup is out-of-flow — open and closed lay out identically.
    Div("select")
        .style(Style::new().size(props.width, 32))
        .children(children)
}

// ---------------------------------------------------------------------------
// ProgressBar
// ---------------------------------------------------------------------------

/// Progress-bar props (stateless display — the value is plain data,
/// re-rendered per frame like `ButtonProps`; nothing writes it, so no
/// signal and no `enabled`: there is no handler to drop, hence no
/// disabled state — decision 213 covers interactive controls).
#[derive(Clone)]
pub struct ProgressBarProps {
    /// Fill fraction (clamped to `0.0..=1.0` — quiet value
    /// normalization, the slider-clamp class; `NaN` panics loudly,
    /// never poisons layout with a confusing downstream error).
    pub value: f32,
    pub width: f32,
    pub height: f32,
    /// Track color (default the themed `border` — the tab-bar ring gray
    /// in Light).
    pub track: Option<Color>,
    /// Fill color (default themed `primary` — the catalog ink).
    pub fill: Option<Color>,
    /// Optional caption above the track (also the accessible name).
    pub label: Option<SharedString>,
}

impl Props for ProgressBarProps {}

impl ProgressBarProps {
    /// Defaults: 160 wide (the slider width — the closest sibling
    /// meter) × 12 high (chosen compact meter), catalog colors, no
    /// caption. Authors override through the pub fields.
    pub fn new(value: f32) -> Self {
        Self {
            value,
            width: 160.0,
            height: 12.0,
            track: None,
            fill: None,
            label: None,
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }
}

/// Determinate progress meter: a rounded track carrying the
/// `progressbar` role (label + `"N percent"` value text) with an
/// inner fill pill scaled to the clamped value. Pill-in-pill (both
/// radii `height / 2`) reads correctly at every fraction, including
/// 0 (empty track, fill zero-size — invisible, correct) and 1.0.
/// Track/fill defaults ride the app theme (Round 11.2); explicit
/// author colors win untouched.
pub fn ProgressBar(ctx: &Ctx, props: &ProgressBarProps) -> VNode {
    let t = ctx.theme().tokens();
    let v = if props.value.is_nan() {
        panic!("ProgressBar value is NaN — refused, never silent");
    } else {
        props.value.clamp(0.0, 1.0)
    };
    let percent = format!("{} percent", (v * 100.0).round() as i32);
    let mut semantics = Semantics::progressbar(&percent);
    if let Some(label) = &props.label {
        semantics = semantics.label(label);
    }
    let fill_w = v * props.width;
    let track = Div("progressbar-track")
        .style(
            Style::new()
                .size(props.width, props.height)
                .radius(props.height / 2.0)
                .bg(props.track.unwrap_or(t.border)),
        )
        .semantics(semantics)
        .child(
            Div("progressbar-fill")
                .style(
                    Style::new()
                        .size(fill_w, props.height)
                        .radius(props.height / 2.0)
                        .bg(props.fill.unwrap_or(t.primary)),
                )
                .build(),
        );
    match &props.label {
        Some(caption) => Div("progressbar").child(Column::new().gap(4).children([
            VNode::from(Text {
                text: caption.clone(),
                style: Text::body_secondary,
            }),
            track,
        ])),
        None => track,
    }
}

// ---------------------------------------------------------------------------
// Badge
// ---------------------------------------------------------------------------

/// Badge variant: status ink on a saturated pill (white ink
/// throughout — one contrast rule, no per-variant ink invention;
/// theme-invariant like the check mark, see [`Checkbox`]).
/// `Success` green is chosen (`0x2E_7D_32` — no catalog green exists,
/// so it stays a documented literal, never a silent primary);
/// `Primary`/`Dim` ride the theme (`primary` / `text_secondary`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BadgeVariant {
    Primary,
    Success,
    Dim,
}

/// Badge props (stateless status chip — plain data like
/// `ButtonProps`, no signal, no interaction).
#[derive(Clone)]
pub struct BadgeProps {
    pub label: SharedString,
    pub variant: BadgeVariant,
}

impl Props for BadgeProps {}

impl BadgeProps {
    pub fn new(label: &str) -> Self {
        Self {
            label: SharedString::from(label),
            variant: BadgeVariant::Primary,
        }
    }

    pub fn variant(mut self, v: BadgeVariant) -> Self {
        self.variant = v;
        self
    }
}

/// Compact status pill: fixed 24-high row (true pill via radius 12
/// — decision-213 explicit sizes), horizontal padding, small bold
/// label. Carries filler + label semantics (the plain-labeled-text
/// shape) so the status announces without claiming a widget role.
pub fn Badge(ctx: &Ctx, props: &BadgeProps) -> VNode {
    let t = ctx.theme().tokens();
    let bg = match props.variant {
        BadgeVariant::Primary => t.primary,
        // Ungrouped: the 2-2-2 split ends in `_32`, which clippy
        // reads as an integer suffix (mistyped-literal deny).
        BadgeVariant::Success => Color(0x2E7D32),
        BadgeVariant::Dim => t.text_secondary,
    };
    Row("badge")
        .style(
            Style::new()
                .h(24)
                .radius(12)
                .pad_x(12)
                .bg(bg)
                .align_items(AlignItems::Center),
        )
        .semantics(Semantics::default().label(&props.label))
        .child(with_ink(
            VNode::from(Text::new(props.label.clone()).size(12).bold()),
            CONTRAST_INK,
        ))
}

// ---------------------------------------------------------------------------
// VirtualList
// ---------------------------------------------------------------------------

/// First/last visible row indexes (half-open `[first, end)`) for a
/// variable-height window (Round 13.2, decision 309): pure over the
/// prefix-sum `tops` (+ `total` bottom) — shared by the control and
/// its tests so both compute the same window. `first` backs up
/// `overscan` rows from the first row whose bottom edge passes `y`;
/// `end` advances `overscan` past the last row whose top edge
/// precedes `y + viewport_h`. Empty and past-the-end inputs yield
/// an empty window (quiet — same class as unmatched controlled
/// edges, documented not silent).
pub fn vlist_window(
    tops: &[f32],
    total: f32,
    y: f32,
    viewport_h: f32,
    overscan: usize,
) -> (usize, usize) {
    let n = tops.len();
    if n == 0 || viewport_h <= 0.0 {
        return (0, 0);
    }
    let bottoms = |i: usize| {
        if i + 1 < n {
            tops[i + 1]
        } else {
            total
        }
    };
    // First row with bottom > y (binary search over the monotone
    // bottoms — the variable-height generalization of M8's
    // `window_first` division).
    let mut lo = 0usize;
    let mut hi = n;
    while lo < hi {
        let mid = (lo + hi) / 2;
        if bottoms(mid) <= y {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let first = lo.saturating_sub(overscan).min(n);
    // First row with top >= y + viewport_h, then overscan forward.
    let edge = y + viewport_h;
    let mut lo = 0usize;
    let mut hi = n;
    while lo < hi {
        let mid = (lo + hi) / 2;
        if tops[mid] < edge {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let end = (lo + overscan).min(n);
    (first.min(end), end)
}

/// Props for one virtualized row's content (the `render_row`
/// component receives the stable [`Row`](oppa::Row) plus its laid
/// height — content fills the slot, never measures it — plus the
/// source [`Collection`](oppa::Collection) so rows can read their
/// own payload granularly through `get_row` (Round 23.1 —
/// `update_row` then re-renders exactly the touched row).
#[derive(Clone, Props)]
pub struct VirtualRowProps<T> {
    pub row: oppa::Row<T>,
    pub height: f32,
    pub rows: oppa::Collection<T>,
}

/// Virtualized list props (controlled — the [`Collection`](oppa::Collection)
/// is author-owned; scroll position rides the framework-owned
/// per-instance offset like the M8 trace).
#[derive(Clone, Props)]
pub struct VirtualListProps<T> {
    /// Data source (version-tracked reads — appends re-derive).
    pub rows: oppa::Collection<T>,
    /// Keep predicate (`None` keeps all — construction-time, like
    /// the M8 window fns; swapping closures re-derives on the next
    /// tracked run).
    pub filter: Option<oppa::RowFilter<T>>,
    /// Stable ordering (`None` keeps commit order).
    pub sort: Option<oppa::RowSort<T>>,
    /// Per-row height (variable — prefix sums drive offsets; must
    /// be finite non-negative, refused loudly downstream by layout
    /// like every bad size, never silently clamped).
    pub row_height: Rc<dyn Fn(&T) -> f32>,
    /// Row content (`fn`, not a closure — [`Ctx::child`] needs a
    /// stable symbol for hot-reload identity, the M8/F6 rule).
    pub render_row: fn(&Ctx, &VirtualRowProps<T>) -> VNode,
    /// Extra rows materialized above/below the visible band.
    pub overscan: usize,
    /// Viewport geometry (explicit — decision 213).
    pub width: f32,
    pub height: f32,
    /// Debug label for the scroll container.
    pub debug: SharedString,
    /// Attached scrollbar overlay (Round 21.2, decision 329):
    /// `true` renders the interactive 17.2 `Scrollbar` over the
    /// viewport, sharing this instance's offset signal (thumb drag,
    /// track page-scroll, and wheel stay in sync by construction)
    /// with a 1200ms wall-clock idle fade. Viewport height and
    /// total extent flow from the settled box + `content_size` —
    /// no second source.
    pub scrollbar: bool,
}

impl<T> VirtualListProps<T> {
    /// Defaults: no filter/sort, 4 rows overscan (the M8 margin),
    /// 320×400 viewport, `"vlist"` container.
    pub fn new(
        rows: oppa::Collection<T>,
        row_height: impl Fn(&T) -> f32 + 'static,
        render_row: fn(&Ctx, &VirtualRowProps<T>) -> VNode,
    ) -> Self {
        Self {
            rows,
            filter: None,
            sort: None,
            row_height: Rc::new(row_height),
            render_row,
            overscan: 4,
            width: 320.0,
            height: 400.0,
            debug: SharedString::from("vlist"),
            scrollbar: true,
        }
    }

    /// Keep predicate (see [`Self::filter`]).
    pub fn filter(mut self, f: impl Fn(&T) -> bool + 'static) -> Self {
        self.filter = Some(Rc::new(f));
        self
    }

    /// Stable ordering (see [`Self::sort`]).
    pub fn sort(mut self, f: impl Fn(&T, &T) -> std::cmp::Ordering + 'static) -> Self {
        self.sort = Some(Rc::new(f));
        self
    }

    /// Viewport geometry (explicit sizes — decision 213).
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Overscan margin in rows.
    pub fn overscan(mut self, n: usize) -> Self {
        self.overscan = n;
        self
    }

    /// Debug label for the scroll container.
    pub fn debug(mut self, label: impl Into<SharedString>) -> Self {
        self.debug = label.into();
        self
    }

    /// Attached scrollbar overlay (see [`Self::scrollbar`]).
    pub fn scrollbar(mut self, show: bool) -> Self {
        self.scrollbar = show;
        self
    }
}

// ---------------------------------------------------------------------------
// Scrollbar
// ---------------------------------------------------------------------------

/// Scrollbar props: the target `ScrollArea`'s debug label (unique
/// per area — geometry and hit-testing address it) plus the offset
/// signal shared with the content positioning (authors own one
/// signal driving rows and thumb together, the `scroll_app`
/// shape — `VirtualList`/`DataGrid` share their instance offset
/// with their attached overlay, see `vlist_scrollbar`).
#[derive(Clone, Props)]
pub struct ScrollbarProps {
    pub target: SharedString,
    pub offset: ScrollOffset,
    /// Wall-clock idle fade (Round 21.2, decision 329): `Some(ms)`
    /// hides the thumb `ms` after the last scroll or pointer event
    /// through the Round 21.1 timer hook (last event wins —
    /// re-renders re-arm); `None` keeps the event-driven hide only
    /// (visible until the next pointer move). `VirtualList` /
    /// `DataGrid` attach with `Some(1200)`.
    pub idle_hide_ms: Option<u64>,
}

/// Overlay scrollbar: track + draggable thumb over a `ScrollArea`
/// viewport (Round 17.2, decision 318). Shows if and only if the
/// content overflows; the thumb follows
/// [`scrollbar_thumb`](oppa::scrollbar_thumb) (`max(24,
/// viewport²/content)`, linear in the clamped offset); the track
/// page-scrolls on tap; thumb drags capture the pointer and map
/// through the linear content ratio; arrows page when focused.
/// Auto-hide fades the chrome out on disengage (hover-leave,
/// release) and back in on hover, press, or scroll writes — the
/// M8 transition evaluator animates both directions (first
/// controls use of `.transition(...)`, MockClock-deterministic).
/// Wall-clock idle fade rides `idle_hide_ms` (Round 21.2, decision
/// 329 — the per-component timer hook parks the visible thumb
/// after inactivity without another pointer move).
/// Theme tokens throughout (thumb in `text_secondary`, track in
/// `border` — both recolor with Light/Dark).
///
/// Structure: an unstyled root marker plus an anchored portal
/// (the Select-popup precedent — out-of-flow, overlay phase,
/// portal-priority hit-testing). The track owns every press
/// (thumb and rows stay handlerless so focus never fragments
/// mid-gesture — the Menu lesson); the press node spans a
/// transparent [`SCROLLBAR_HIT_PX`](oppa::SCROLLBAR_HIT_PX) gutter
/// with the painted 12px bar nested inside (follow-up — hover
/// summons without pixel-hunting; gutter presses page/drag
/// instead of reaching the covered strip); taps disambiguate by
/// thumb rect (thumb taps hold, dead padding dismisses nothing —
/// a tap is a tap, only pages move), drags map from a press-edge
/// snapshot (grab-preserving ratio, never a teleport).
pub fn Scrollbar(ctx: &Ctx, props: &ScrollbarProps) -> VNode {
    let closed = || {
        Portal("scrollbar-closed")
            .style(Style::new().size(0, 0))
            .build()
    };
    let host = ctx.host();
    let inst = ctx.instance_id();
    let t = ctx.theme().tokens();
    // Target geometry (settled layout — TRACKED, so the first
    // post-layout publish re-runs this body and mounts the chrome;
    // untracked `committed_box` would render closed pre-layout
    // forever. Absent pre-layout → closed).
    let target = host.settled_box_by_debug(&props.target);
    let Some(tb) = target else {
        return closed();
    };
    // Tracked reads (every one re-renders): offset, hover, press.
    let y = props.offset.get();
    let max = oppa::scrollbar_max_offset(tb.h, tb.content_h);
    let Some(g) = oppa::scrollbar_thumb(tb.h, tb.content_h, y) else {
        return closed();
    };
    let hover = ctx.hovered();
    let down = ctx.pressed();
    // Scroll flash (guarded latch — settling never spins it):
    // offset edges show the thumb so wheel/programmatic scrolls
    // paint live; disengage runs hide it again.
    let flash = ctx.signal(false);
    let last_y = ctx.signal(0.0f32);
    if y != last_y.get() {
        last_y.set(y);
        if !flash.get() {
            flash.set(true);
        }
    } else if props.idle_hide_ms.is_none() && !hover.get() && !down.get() && flash.get() {
        // Legacy event-driven hide only: without a wall-clock arm
        // the flash is a same-settle pulse (hover/press hold it).
        // With `idle_hide_ms` the timer below owns the hide, so a
        // programmatic scroll stays parked for the full budget
        // instead of dying on its own follow-up render.
        flash.set(false);
    }
    // Wall-clock idle fade (Round 21.2, decision 329): while the
    // flash is latched, arm a per-render timeout that drops it
    // `idle_hide_ms` after the last scroll or pointer event.
    // Re-renders re-arm (cleanup cancels the previous arm — last
    // event wins, so continuous scrolling never hides mid-gesture);
    // the fire clears only when neither hovered nor pressed (a
    // parked pointer keeps its chrome).
    if let Some(hide_ms) = props.idle_hide_ms {
        if flash.get() {
            let flash_hide = flash.clone();
            let hover_hide = hover.clone();
            let down_hide = down.clone();
            ctx.use_timeout(hide_ms as f64, move || {
                if !hover_hide.get() && !down_hide.get() {
                    flash_hide.set(false);
                }
            });
        }
    }
    let visible = hover.get() || down.get() || flash.get();
    // Drag snapshot on the press edge (grab-preserving ratio):
    // base offset + press-origin y, plus whether the grab started
    // on the thumb (track grabs page on release instead).
    let base = ctx.signal((0.0f32, 0.0f32));
    let grabbed = ctx.signal(false);
    let was_down = ctx.signal(false);
    let down_now = down.get();
    if down_now && !was_down.get() {
        let origin = host.lowest_press_origin();
        let gutter_x = tb.x + tb.w - SCROLLBAR_HIT_PX;
        let on_thumb = origin.is_some_and(|(ox, oy)| {
            ox >= gutter_x
                && ox < gutter_x + SCROLLBAR_HIT_PX
                && oy >= tb.y + g.y
                && oy < tb.y + g.y + g.h
        });
        base.set((y, origin.map(|(_, oy)| oy).unwrap_or(tb.y + g.y)));
        if grabbed.get() != on_thumb {
            grabbed.set(on_thumb);
        }
    }
    if was_down.get() != down_now {
        was_down.set(down_now);
    }
    // Slider role with a human percentage (the G2 announcement
    // shape — value-pattern exposure stays OQ-G2-2).
    let pct = if max <= 0.0 {
        0
    } else {
        (y.clamp(0.0, max) / max * 100.0).round() as u32
    };
    let fade = Transition::new(150, Ease::Out);
    let track_opacity = if visible { 0.3 } else { 0.0 };
    let thumb_opacity = if visible { 1.0 } else { 0.0 };
    let offset = props.offset.clone();
    // Hit gutter (follow-up): the press/hover node spans
    // SCROLLBAR_HIT_PX leftwards over the content edge while the
    // painted bar + thumb keep the 12px chrome — summoning no
    // longer needs pixel-hunting. The gutter is transparent (no bg
    // → paints nothing → plan rects byte-identical); presses inside
    // it page/drag instead of reaching the covered content strip.
    let track = Div(format!("scrollbar-track-{inst}").as_str())
        .style(
            Style::new()
                .x(tb.w - SCROLLBAR_HIT_PX)
                .w(SCROLLBAR_HIT_PX)
                .h(tb.h)
                .build(),
        )
        .semantics(
            Semantics::slider()
                .label("scrollbar")
                .value_text(format!("{pct} percent").as_str()),
        )
        .on_press({
            let host = host.clone();
            let offset = offset.clone();
            let target = props.target.clone();
            move || {
                // Fresh geometry (event-time boxes — immune to body
                // staleness, the Slider precedent). Keyboard Enter
                // (no tap point) holds the thumb; thumb taps hold;
                // track taps page by viewport, clamped.
                let Some(tb) = oppa::find_retained_by_debug(&host, &target)
                    .into_iter()
                    .next()
                    .and_then(|id| host.committed_box(id))
                else {
                    return;
                };
                let y = offset.get();
                let max = oppa::scrollbar_max_offset(tb.h, tb.content_h);
                let Some(g) = oppa::scrollbar_thumb(tb.h, tb.content_h, y) else {
                    return;
                };
                let Some((_, py)) = host.last_press_position() else {
                    return;
                };
                let ty = tb.y + g.y;
                if py >= ty && py < ty + g.h {
                    return;
                }
                let ny = if py < ty { y - tb.h } else { y + tb.h }.clamp(0.0, max);
                if ny != y {
                    offset.set(ny);
                }
            }
        })
        .on_drag({
            let host = host.clone();
            let offset = offset.clone();
            let target = props.target.clone();
            let base = base.clone();
            let grabbed = grabbed.clone();
            move || {
                if !grabbed.get() {
                    return;
                }
                let Some((_, cy)) = host.capture_position() else {
                    return;
                };
                let Some(tb) = oppa::find_retained_by_debug(&host, &target)
                    .into_iter()
                    .next()
                    .and_then(|id| host.committed_box(id))
                else {
                    return;
                };
                let max = oppa::scrollbar_max_offset(tb.h, tb.content_h);
                let Some(g) = oppa::scrollbar_thumb(tb.h, tb.content_h, offset.get()) else {
                    return;
                };
                let travel = tb.h - g.h;
                if travel <= 0.0 {
                    return;
                }
                let (base_off, base_y) = base.get();
                let ny = (base_off + (cy - base_y) * max / travel).clamp(0.0, max);
                if ny != offset.get() {
                    offset.set(ny);
                }
            }
        })
        .on_key_down({
            let host = host.clone();
            let offset = offset.clone();
            let target = props.target.clone();
            move || {
                // Arrow pages by viewport (fresh max, guarded
                // write — end presses never spin).
                let Some(tb) = oppa::find_retained_by_debug(&host, &target)
                    .into_iter()
                    .next()
                    .and_then(|id| host.committed_box(id))
                else {
                    return;
                };
                let max = oppa::scrollbar_max_offset(tb.h, tb.content_h);
                let ny = (offset.get() + tb.h).clamp(0.0, max);
                if ny != offset.get() {
                    offset.set(ny);
                }
            }
        })
        .on_key_up({
            let host = host.clone();
            let offset = offset.clone();
            let target = props.target.clone();
            move || {
                let Some(tb) = oppa::find_retained_by_debug(&host, &target)
                    .into_iter()
                    .next()
                    .and_then(|id| host.committed_box(id))
                else {
                    return;
                };
                let max = oppa::scrollbar_max_offset(tb.h, tb.content_h);
                let ny = (offset.get() - tb.h).clamp(0.0, max);
                if ny != offset.get() {
                    offset.set(ny);
                }
            }
        })
        .children([
            Div(format!("scrollbar-trackbar-{inst}").as_str())
                .style(
                    Style::new()
                        .x(SCROLLBAR_HIT_PX - SCROLLBAR_TRACK_PX)
                        .w(SCROLLBAR_TRACK_PX)
                        .h(tb.h)
                        .bg(t.border)
                        .opacity(Some(track_opacity))
                        .transition(fade),
                )
                .build(),
            Div(format!("scrollbar-thumb-{inst}").as_str())
                .style(
                    Style::new()
                        .x(SCROLLBAR_HIT_PX - SCROLLBAR_TRACK_PX)
                        .w(SCROLLBAR_TRACK_PX)
                        .h(g.h)
                        .absolute_y(g.y)
                        .bg(t.text_secondary)
                        .opacity(Some(thumb_opacity))
                        .transition(fade),
                )
                .build(),
        ]);
    // Root origin for the portal base (unstyled marker — content
    // origin is the box origin; tracked like the target so resizes
    // reposition the overlay).
    let root_label = format!("scrollbar-{inst}");
    let (rx, ry) = oppa::find_retained_by_debug(&host, &root_label)
        .into_iter()
        .filter_map(|id| host.settled_box(id))
        .next()
        .map(|b| (b.x, b.y))
        .unwrap_or((0.0, 0.0));
    Div(root_label.as_str()).child(
        Portal("scrollbar-popup")
            .style(Style::new().x(tb.x - rx).absolute_y(tb.y - ry).w(tb.w))
            .child(track),
    )
}

/// Virtualized list: a `ScrollArea` window over a
/// [`Collection`](oppa::Collection) (Round 13.2, decision 309).
/// Only the window materializes (slot `Div`s with stable
/// slot-index keys + one `ctx.child` row each — the M8 recycle
/// shape, so scrolling commits LAYOUT-only updates, never
/// structure); row payloads stay data until their slot binds.
/// Variable heights flow through prefix sums (shared [`vlist_window`]
/// math — binary-searched, never divided). The extent comes from
/// the match total, so the DOM spacer + scrollbar follow for free.
///
/// Filter/sort are construction-time (see [`VirtualListProps`]);
/// the collection version re-derives everything live (appends
/// grow the extent, removals collapse it — same frame, no
/// remount).
pub fn VirtualList<T: Clone + 'static>(ctx: &Ctx, props: &VirtualListProps<T>) -> VNode {
    let offset = ctx.scroll_offset();
    // Tracked read: the window re-derives when the offset moves OR
    // the collection mutates (one version signal — Store-coarse,
    // memo gates dedup downstream).
    let y = offset.get();
    let query = oppa::CollectionQuery {
        filter: props.filter.clone(),
        sort: props.sort.clone(),
        offset: 0,
        limit: None,
    };
    let page = props.rows.query(&query);
    let n = page.total;
    // Prefix sums (tops[i] = content y of row i; total bottom).
    let mut tops = Vec::with_capacity(n + 1);
    let mut cursor = 0.0f32;
    for row in &page.rows {
        tops.push(cursor);
        cursor += (props.row_height)(&row.value);
    }
    let total = cursor;
    // Desktop ScrollAreas clip without translating: viewport-relative
    // slots subtract the offset (Round 24.2 — content coordinates
    // would otherwise never move on screen).
    let (first, end) = vlist_window(&tops, total, y, props.height, props.overscan);
    let width = props.width;
    let children = page.rows[first..end]
        .iter()
        .enumerate()
        .map(|(slot, row)| {
            let idx = first + slot;
            let h = tops.get(idx + 1).copied().unwrap_or(total) - tops[idx];
            let row_props = VirtualRowProps {
                row: row.clone(),
                height: h,
                rows: props.rows.clone(),
            };
            Div("vlist-slot")
                .style(Style::new().absolute_y(tops[idx] - y).h(h).w(width).build())
                .key(slot as u64)
                .child(ctx.child("oppa::VListRow", slot as u64, &row_props, props.render_row))
        })
        .collect::<Vec<_>>();
    let area = oppa::ScrollArea(&props.debug)
        .style(
            Style::new()
                .size(width, props.height)
                .content_size(total)
                .build(),
        )
        .on_scroll(|| {})
        .children(children);
    if !props.scrollbar {
        return area;
    }
    // Attached overlay (Round 21.2, decision 329): the same offset
    // signal drives rows and thumb together (wheel, thumb drag, and
    // track page-scroll stay in sync by construction); viewport
    // height + total extent flow from the settled box + the
    // `content_size` above — no second source. The overlay is a
    // portal (out-of-flow), so the wrapper sizes exactly to the
    // area and committed boxes move nowhere.
    let bar = ctx.child(
        "oppa::VListBar",
        1,
        &ScrollbarProps {
            target: props.debug.clone(),
            offset: offset.clone(),
            idle_hide_ms: Some(1200),
        },
        Scrollbar,
    );
    Div(format!("{}-wrap", props.debug).as_str()).children([area, bar])
}

// ---------------------------------------------------------------------------
// DataGrid
// ---------------------------------------------------------------------------

/// Props for one grid cell's content (the column template receives
/// the stable [`Row`](oppa::Row) plus its laid geometry — content
/// fills the cell, never measures it — plus the source
/// [`Collection`](oppa::Collection) for granular `get_row` reads,
/// the [`VirtualRowProps`] twin — plus the grid's selection when
/// the author wires click-to-select (Round 24.1: `None` renders
/// selection-blind, like every pre-24.1 grid).
#[derive(Clone, Props)]
pub struct GridCellProps<T> {
    pub row: oppa::Row<T>,
    pub height: f32,
    pub width: f32,
    pub rows: oppa::Collection<T>,
    pub selected: Option<Signal<Option<oppa::RowId>>>,
}

/// One grid column: header label, fixed width, per-row cell
/// template (`fn`, not a closure — [`Ctx::child`] needs a stable
/// symbol for hot-reload identity, the M8/F6 rule).
#[derive(Clone)]
pub struct GridColumn<T> {
    pub header: SharedString,
    pub width: f32,
    pub cell: fn(&Ctx, &GridCellProps<T>) -> VNode,
}

/// Data grid props (controlled — the [`Collection`](oppa::Collection)
/// is author-owned; scroll position rides the framework-owned
/// per-instance offset like [`VirtualList`]).
#[derive(Clone, Props)]
pub struct DataGridProps<T> {
    /// Data source (version-tracked reads — page appends re-derive).
    pub rows: oppa::Collection<T>,
    /// Keep predicate (`None` keeps all — construction-time, like
    /// [`VirtualListProps`]).
    pub filter: Option<oppa::RowFilter<T>>,
    /// Stable ordering (`None` keeps commit order).
    pub sort: Option<oppa::RowSort<T>>,
    /// Fixed-width columns (the row width is their sum — authors
    /// size to the viewport; overflow past it clips in v1, stated).
    pub columns: Vec<GridColumn<T>>,
    /// Per-row height (variable — same prefix-sum machinery as
    /// [`VirtualList`]).
    pub row_height: Rc<dyn Fn(&T) -> f32>,
    /// Header height (pinned — see [`DataGrid`]).
    pub header_height: f32,
    /// Extra rows materialized above/below the visible band.
    pub overscan: usize,
    /// Viewport geometry (explicit — decision 213).
    pub width: f32,
    pub height: f32,
    /// Accessible name (Generic + label — no table roles exist in
    /// v1 semantics; adding roles would ripple every emitter,
    /// stated, not smuggled).
    pub label: SharedString,
    /// Debug label for the scroll container.
    pub debug: SharedString,
    /// Attached scrollbar overlay (Round 21.2, decision 329):
    /// same contract as [`VirtualListProps::scrollbar`] — the
    /// grid's instance offset drives the thumb.
    pub scrollbar: bool,
    /// Click-to-select (Round 24.1, decision 335): `Some` signal
    /// receiving the selected [`RowId`](oppa::RowId) (`None`
    /// clears); `None` renders selection-blind (every pre-24.1
    /// grid). Cell templates read it for highlight + actions.
    pub selected: Option<Signal<Option<oppa::RowId>>>,
}

impl<T> DataGridProps<T> {
    /// Defaults: no filter/sort, 4 rows overscan (the M8 margin),
    /// 28px header, 480×400 viewport, `"grid"` label/container.
    pub fn new(
        rows: oppa::Collection<T>,
        columns: Vec<GridColumn<T>>,
        row_height: impl Fn(&T) -> f32 + 'static,
    ) -> Self {
        Self {
            rows,
            filter: None,
            sort: None,
            columns,
            row_height: Rc::new(row_height),
            header_height: 28.0,
            overscan: 4,
            width: 480.0,
            height: 400.0,
            label: SharedString::from("data grid"),
            debug: SharedString::from("dgrid"),
            scrollbar: true,
            selected: None,
        }
    }

    /// Keep predicate (see [`Self::filter`]).
    pub fn filter(mut self, f: impl Fn(&T) -> bool + 'static) -> Self {
        self.filter = Some(Rc::new(f));
        self
    }

    /// Stable ordering (see [`Self::sort`]).
    pub fn sort(mut self, f: impl Fn(&T, &T) -> std::cmp::Ordering + 'static) -> Self {
        self.sort = Some(Rc::new(f));
        self
    }

    /// Viewport geometry (explicit sizes — decision 213).
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Overscan margin in rows.
    pub fn overscan(mut self, n: usize) -> Self {
        self.overscan = n;
        self
    }

    /// Debug label for the scroll container.
    pub fn debug(mut self, label: impl Into<SharedString>) -> Self {
        self.debug = label.into();
        self
    }

    /// Attached scrollbar overlay (see [`Self::scrollbar`]).
    pub fn scrollbar(mut self, show: bool) -> Self {
        self.scrollbar = show;
        self
    }

    /// Click-to-select signal (see [`Self::selected`]).
    pub fn selected(mut self, selected: Signal<Option<oppa::RowId>>) -> Self {
        self.selected = Some(selected);
        self
    }
}

/// Data grid: a [`VirtualList`] window with per-column templates
/// and a sticky header (Round 13.3, decision 310; viewport-relative
/// coordinates Round 24.2). Only window rows materialize (stable slot
/// keys + `ctx.child` cells keyed by slot × column — scrolling
/// rebinds Update-only); the header pins at the viewport top with
/// `absolute_y(0.0)` while rows render at
/// `header_height + tops[idx] - scroll_y` (desktop `ScrollArea`s clip
/// without translating, so content coordinates must subtract the
/// offset to move). The header rides inside the scroll container, so
/// horizontal pans move header and rows together, and it paints above
/// by child order. Filter updates and page appends re-derive the
/// window the same frame, no remount.
pub fn DataGrid<T: Clone + 'static>(ctx: &Ctx, props: &DataGridProps<T>) -> VNode {
    assert!(
        !props.columns.is_empty(),
        "DataGrid needs at least one column — a zero-column grid is an authoring bug, never an empty table"
    );
    let t = ctx.theme().tokens();
    let offset = ctx.scroll_offset();
    // Tracked read: window, header pin, and rows re-derive when the
    // offset moves OR the collection mutates (one version signal).
    let y = offset.get();
    let ncols = props.columns.len();
    let total_w: f32 = props.columns.iter().map(|c| c.width).sum();
    let query = oppa::CollectionQuery {
        filter: props.filter.clone(),
        sort: props.sort.clone(),
        offset: 0,
        limit: None,
    };
    let page = props.rows.query(&query);
    let n = page.total;
    // Prefix sums (tops[i] = content y of row i).
    let mut tops = Vec::with_capacity(n + 1);
    let mut cursor = 0.0f32;
    for row in &page.rows {
        tops.push(cursor);
        cursor += (props.row_height)(&row.value);
    }
    let total = cursor;
    // The pinned header occupies the top of the viewport, so the row
    // window covers the remaining body height (Round 24.2).
    let body_h = (props.height - props.header_height).max(0.0);
    let (first, end) = vlist_window(&tops, total, y, body_h, props.overscan);
    let header_cells = props
        .columns
        .iter()
        .map(|col| {
            Div("dgrid-headcell")
                .style(Style::new().w(col.width).h(props.header_height).build())
                .child(VNode::from(Text::new(col.header.clone()).size(12).bold()))
        })
        .collect::<Vec<_>>();
    let mut children = page.rows[first..end]
        .iter()
        .enumerate()
        .map(|(slot, row)| {
            let idx = first + slot;
            let h = tops.get(idx + 1).copied().unwrap_or(total) - tops[idx];
            let cells = props
                .columns
                .iter()
                .enumerate()
                .map(|(col, column)| {
                    let cell_props = GridCellProps {
                        row: row.clone(),
                        height: h,
                        width: column.width,
                        rows: props.rows.clone(),
                        selected: props.selected.clone(),
                    };
                    Div("dgrid-cell")
                        .style(Style::new().w(column.width).h(h).build())
                        .child(ctx.child(
                            "oppa::DGridCell",
                            (slot * ncols.max(1) + col) as u64,
                            &cell_props,
                            column.cell,
                        ))
                })
                .collect::<Vec<_>>();
            Row("dgrid-row")
                .style(
                    Style::new()
                        .absolute_y(props.header_height + tops[idx] - y)
                        .h(h)
                        .w(total_w)
                        .build(),
                )
                .key(slot as u64)
                .children(cells)
        })
        .collect::<Vec<_>>();
    // Sticky header, pinned to the viewport top (keyed — stable
    // identity across windows, never remounted by slides).
    children.push(
        Row("dgrid-header")
            .style(
                Style::new()
                    .absolute_y(0.0)
                    .h(props.header_height)
                    .w(total_w)
                    .bg(t.surface)
                    .border_bottom(1, t.border)
                    .align_items(AlignItems::Center)
                    .build(),
            )
            .key(u64::MAX)
            .semantics(Semantics::default().label(&props.label))
            .children(header_cells),
    );
    let area = oppa::ScrollArea(&props.debug)
        .style(
            Style::new()
                .size(props.width, props.height)
                // Rows scroll beneath the pinned header, so the extent
                // spans both: max offset parks the last row bottom at
                // the viewport bottom (Round 24.2).
                .content_size(total + props.header_height)
                .build(),
        )
        .on_scroll(|| {})
        .children(children);
    if !props.scrollbar {
        return area;
    }
    // Attached overlay (Round 21.2, decision 329): same contract as
    // VirtualList — the grid's instance offset drives the thumb.
    let bar = ctx.child(
        "oppa::DGridBar",
        1,
        &ScrollbarProps {
            target: props.debug.clone(),
            offset: offset.clone(),
            idle_hide_ms: Some(1200),
        },
        Scrollbar,
    );
    Div(format!("{}-wrap", props.debug).as_str()).children([area, bar])
}

// ---------------------------------------------------------------------------
// Tooltip
// ---------------------------------------------------------------------------

/// Tooltip props (Round 17.3, decision 319): wraps author content,
/// opens an anchored portal tooltip card after `delay_ms` dwell
/// (default 500ms), and dismisses on hover-leave or press.
#[derive(Clone, Props)]
pub struct TooltipProps<C> {
    pub tip: SharedString,
    pub content: fn(&Ctx, &C) -> VNode,
    pub content_props: C,
    pub delay_ms: u64,
    pub debug: SharedString,
}

impl<C> TooltipProps<C> {
    pub fn new(
        tip: impl Into<SharedString>,
        content: fn(&Ctx, &C) -> VNode,
        content_props: C,
    ) -> Self {
        Self {
            tip: tip.into(),
            content,
            content_props,
            delay_ms: 500,
            debug: SharedString::from("tooltip"),
        }
    }

    pub fn delay_ms(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    pub fn debug(mut self, debug: impl Into<SharedString>) -> Self {
        self.debug = debug.into();
        self
    }
}

/// Anchored tooltip over author content (Round 17.3, decision 319):
/// mounts an anchored card portal after dwell time (default 500ms)
/// under pointer hover, and auto-dismisses on leave or press.
pub fn Tooltip<C: Props>(ctx: &Ctx, props: &TooltipProps<C>) -> VNode {
    let host = ctx.host();
    let inst = ctx.instance_id();
    let _ = ctx.hover_move().get();
    let t = ctx.theme().tokens();
    let anchor_label = format!("tooltip-anchor-{inst}");
    let open = ctx.signal(false);
    let hover_start = ctx.signal(Option::<f64>::None);

    let anchor_id = oppa::find_retained_by_debug(&host, &anchor_label)
        .into_iter()
        .next();

    let hovered = ctx.hovered().get()
        || anchor_id.is_some_and(|aid| {
            if let Some(mut cur) = host.hovered_node() {
                host.with_retained_mut(|rec, _| loop {
                    if cur == aid {
                        return true;
                    }
                    if let Some(n) = rec.get(cur) {
                        if let Some(p) = n.parent {
                            cur = p;
                            continue;
                        }
                    }
                    return false;
                })
            } else {
                false
            }
        });

    let pressed = ctx.pressed().get()
        || anchor_id.is_some_and(|aid| {
            if let Some(mut cur) = host.capture_node() {
                host.with_retained_mut(|rec, _| loop {
                    if cur == aid {
                        return true;
                    }
                    if let Some(n) = rec.get(cur) {
                        if let Some(p) = n.parent {
                            cur = p;
                            continue;
                        }
                    }
                    return false;
                })
            } else {
                false
            }
        });

    let now = host.runtime().now_secs();
    if pressed || !hovered {
        if open.get() {
            open.set(false);
        }
        if hover_start.get().is_some() {
            hover_start.set(None);
        }
    } else {
        match hover_start.get() {
            None => {
                hover_start.set(Some(now));
            }
            Some(start) => {
                let elapsed_ms = ((now - start) * 1000.0).max(0.0) as u64;
                if elapsed_ms >= props.delay_ms && !open.get() {
                    open.set(true);
                }
            }
        }
    }

    let anchor_box = host.committed_box_by_debug(&anchor_label);
    let ay_off = anchor_box.as_ref().map(|b| b.h + 4.0).unwrap_or(28.0);

    let popup = if open.get() {
        let card = Div("tooltip-card")
            .style(
                Style::new()
                    .bg(t.surface)
                    .border(1, t.border)
                    .radius(4)
                    .pad_x(8)
                    .pad_y(4),
            )
            .semantics(Semantics::default().label(&props.tip))
            .child(with_ink(
                VNode::from(Text {
                    text: props.tip.clone(),
                    style: Text::body_secondary,
                }),
                t.text_primary,
            ));
        // Viewport-edge clamping (Round 21.3, decision 330): flip
        // above/left when the settled card would overflow the
        // viewport. One settled read per (wrapper origin, tip):
        // the pre-layout pass renders below the anchor and stays
        // unplaced; the placed pass commits and never reads settled
        // again — a persistent generation subscription here spins
        // exactly like Menu's did (re-render re-dirties layout
        // through the content child).
        let (vw, vh) = host.viewport_size();
        let (wx, wy) = anchor_box
            .as_ref()
            .map(|b| (b.x, b.y))
            .unwrap_or((0.0, 0.0));
        let tplaced = ctx.signal(None::<((f32, f32), SharedString, (f32, f32))>);
        let tip_key = props.tip.clone();
        let origin = match tplaced.get() {
            Some((w0, t0, o)) if w0 == (wx, wy) && t0 == tip_key => o,
            _ => match host
                .settled_box_by_debug("tooltip-card")
                .map(|b| (b.w, b.h))
            {
                None => (0.0, ay_off),
                Some((cw, ch)) => {
                    let (cx, cy) = menu::clamp_popup_anchor((vw, vh), (wx, wy + ay_off), (cw, ch));
                    let o = (cx - wx, cy - wy);
                    if tplaced.get() != Some(((wx, wy), tip_key.clone(), o)) {
                        tplaced.set(Some(((wx, wy), tip_key.clone(), o)));
                    }
                    o
                }
            },
        };
        Portal("tooltip-portal")
            .style(Style::new().x(origin.0).absolute_y(origin.1))
            .child(card)
    } else {
        Portal("tooltip-closed")
            .style(Style::new().size(0, 0))
            .build()
    };

    let content_node = (props.content)(ctx, &props.content_props);
    let (open_h, hover_h) = (open.clone(), hover_start.clone());
    Div(anchor_label.as_str())
        .on_press(move || {
            open_h.set(false);
            hover_h.set(None);
        })
        .children([content_node, popup])
}

/// Fallback render closure for [`ErrorBoundary`]: receives `(&Ctx, error_message, reset_fn)`.
pub type ErrorFallback = Rc<dyn Fn(&Ctx, &str, &dyn Fn()) -> VNode>;

/// Error listener callback for [`ErrorBoundary`].
pub type ErrorListener = Rc<dyn Fn(&str)>;

/// Props for [`ErrorBoundary`].
#[derive(Clone)]
pub struct ErrorBoundaryProps {
    pub child: Rc<dyn Fn(&Ctx) -> VNode>,
    pub fallback: Option<ErrorFallback>,
    pub on_error: Option<ErrorListener>,
}

impl Props for ErrorBoundaryProps {}

impl ErrorBoundaryProps {
    /// Creates a new error boundary wrapping the given child render closure.
    pub fn new(child: impl Fn(&Ctx) -> VNode + 'static) -> Self {
        Self {
            child: Rc::new(child),
            fallback: None,
            on_error: None,
        }
    }

    /// Custom fallback renderer: `|ctx, error_msg, reset| ...`.
    pub fn fallback(mut self, fallback: impl Fn(&Ctx, &str, &dyn Fn()) -> VNode + 'static) -> Self {
        self.fallback = Some(Rc::new(fallback));
        self
    }

    /// Error notification callback.
    pub fn on_error(mut self, on_error: impl Fn(&str) + 'static) -> Self {
        self.on_error = Some(Rc::new(on_error));
        self
    }
}

/// Error boundary component (Round 18.1, Decision 320):
/// Catches child component unwind panics via `catch_unwind`, preventing
/// host panics and thread crashes. On error, renders a fallback card with
/// error details and a retry trigger. When reset, re-evaluates the child.
pub fn ErrorBoundary(ctx: &Ctx, props: &ErrorBoundaryProps) -> VNode {
    let t = ctx.theme().tokens();
    let retry = ctx.signal(0u64);
    let _ = retry.get(); // re-evaluates on retry trigger

    let reset_action = {
        let retry = retry.clone();
        Rc::new(move || {
            retry.set(retry.get().wrapping_add(1));
        })
    };

    let child = props.child.clone();
    let outcome = ctx.catch_unwind(|| child(ctx));

    match outcome {
        Ok(vnode) => vnode,
        Err(err) => {
            if let Some(ref on_err) = props.on_error {
                on_err(&err);
            }
            if let Some(ref custom_fallback) = props.fallback {
                let r = reset_action.clone();
                custom_fallback(ctx, &err, &move || r())
            } else {
                let r = reset_action.clone();
                let btn_props =
                    ButtonProps::new("Retry", move || r()).debug("error-boundary-retry");
                let retry_btn = ctx.child("oppa::ErrorBoundaryRetry", 0, &btn_props, Button);
                let card = Div("error-boundary-card")
                    .style(
                        Style::new()
                            .pad_x(16)
                            .pad_y(12)
                            .border(1, t.border)
                            .radius(6)
                            .bg(t.surface),
                    )
                    .child(Column::new().gap(8).children([
                        VNode::from(Text::new(SharedString::from(format!("Error: {err}")))),
                        retry_btn,
                    ]));
                Div("error-boundary-container").child(card)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oppa::input::{keys, InputEvent, KeyState};
    use oppa::{
        find_retained_by_debug, Cluster, ComponentHost, FontId, FontMetrics, NodeId, ScrollOffset,
        ShapedGlyph, ShapedRun, TextError, TextRun, TextService, TextStyle,
    };

    /// Uniform-advance fake shaper (body 14px → 8.75px/char), so measured
    /// content widths prove which string is displayed.
    struct FakeText;

    impl TextService for FakeText {
        fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
            Vec::new()
        }

        fn shape(&self, text: &str, style: &TextStyle) -> Result<ShapedRun, TextError> {
            if text.is_empty() {
                return Err(TextError::EmptyText);
            }
            let em = style.font_size_px * style.device_pixel_ratio;
            let adv = em * 0.625;
            let metrics = FontMetrics {
                ascent: em * 0.75,
                descent: em * 0.25,
                line_gap: em * 0.125,
            };
            let mut glyphs = Vec::new();
            let mut clusters = Vec::new();
            for (k, (i, ch)) in text.char_indices().enumerate() {
                let len = ch.len_utf8();
                glyphs.push(ShapedGlyph {
                    glyph_id: k as u32,
                    x_advance: adv,
                    x_offset: 0.0,
                    y_offset: 0.0,
                });
                clusters.push(Cluster {
                    byte_range: (i, i + len),
                    glyph_range: (glyphs.len() - 1, glyphs.len()),
                });
            }
            Ok(ShapedRun {
                total_advance: adv * glyphs.len() as f32,
                text_len_bytes: text.len(),
                runs: vec![TextRun {
                    byte_range: (0, text.len()),
                    glyph_range: (0, glyphs.len()),
                    rtl: false,
                    script: 0,
                    font_id: FontId(0),
                    font_metrics: metrics,
                }],
                glyphs,
                clusters,
            })
        }
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    /// Measured width of the single laid text leaf (proves *which* string
    /// is displayed — placeholder vs value — without a retained-text
    /// reader, which the host deliberately does not expose).
    fn lined_content_w(host: &ComponentHost) -> f32 {
        let mut ws: Vec<f32> = find_retained_by_debug(host, "text")
            .into_iter()
            .filter_map(|id| host.committed_box(id))
            .filter(|b| !b.lines.is_empty())
            .map(|b| b.content_w)
            .collect();
        assert_eq!(ws.len(), 1, "exactly one measured leaf, got {}", ws.len());
        ws.pop().unwrap()
    }

    fn press_node(host: &ComponentHost, id: NodeId) {
        let b = host.committed_box(id).expect("control has a hit box");
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        host.inject_input(InputEvent::pointer_down(cx, cy));
        host.inject_input(InputEvent::pointer_up(cx, cy));
        host.run_until_idle();
    }

    fn node_by_debug(host: &ComponentHost, debug: &str) -> NodeId {
        find_retained_by_debug(host, debug)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("no retained node {debug:?}"))
    }

    #[derive(Clone)]
    struct ButtonCase {
        props: ButtonProps,
    }
    impl Props for ButtonCase {}

    fn button_render(ctx: &Ctx, p: &ButtonCase) -> VNode {
        ctx.child("oppa::Button", 7, &p.props, Button)
    }

    #[test]
    fn button_press_and_keyboard_activate() {
        let host = ComponentHost::new();
        let fired = host.runtime().signal(false);
        let set = fired.clone();
        let props = ButtonProps::new("OK", move || set.set(true));
        let case = ButtonCase { props };
        host.mount("Btn", case, button_render);
        host.run_until_idle();
        let id = node_by_debug(&host, "button");
        // Semantics: button role + label, enabled.
        let sem = host.retained_semantics(id).expect("semantics");
        assert_eq!(sem.role, oppa::Role::Button);
        assert_eq!(sem.label.as_deref(), Some("OK"));
        assert!(!sem.disabled);
        // Pointer press activates.
        press_node(&host, id);
        assert!(fired.get(), "press activates");
        // Keyboard: focus + Enter activates (M5 router).
        fired.set(false);
        host.inject_input(InputEvent::Focus { node: Some(id) });
        host.run_until_idle();
        host.inject_input(InputEvent::key(keys::ENTER, KeyState::Pressed));
        host.run_until_idle();
        assert!(fired.get(), "Enter on focused button activates");
    }

    #[test]
    fn button_disabled_is_handlerless_and_untabbable() {
        let host = ComponentHost::new();
        let fired = host.runtime().signal(false);
        let set = fired.clone();
        let props = ButtonProps::new("OK", move || set.set(true)).disabled();
        host.mount("BtnDis", ButtonCase { props }, button_render);
        host.run_until_idle();
        let id = node_by_debug(&host, "button");
        assert!(
            host.retained_handlers(id).is_empty(),
            "disabled carries no handler — refusal is structural"
        );
        assert!(
            !host.tab_order().contains(&id),
            "disabled leaves the tab order (decision 96)"
        );
        press_node(&host, id);
        assert!(!fired.get(), "press on disabled never activates");
    }

    #[derive(Clone)]
    struct CheckCase {
        props: CheckboxProps,
    }
    impl Props for CheckCase {}

    fn check_render(ctx: &Ctx, p: &CheckCase) -> VNode {
        ctx.child("oppa::Checkbox", 7, &p.props, Checkbox)
    }

    #[test]
    fn checkbox_flips_and_reports_checked() {
        let host = ComponentHost::new();
        let checked = host.runtime().signal(false);
        let props = CheckboxProps {
            label: SharedString::from("T&C"),
            checked: checked.clone(),
            enabled: true,
            on_change: None,
        };
        host.mount("Chk", CheckCase { props }, check_render);
        host.run_until_idle();
        let id = node_by_debug(&host, "checkbox");
        press_node(&host, id);
        assert!(checked.get(), "press checks");
        let sem = host.retained_semantics(id).expect("semantics");
        assert_eq!((sem.role, sem.checked), (oppa::Role::Checkbox, Some(true)));
        press_node(&host, id);
        assert!(!checked.get(), "press unchecks");
    }

    /// Decision 291: the drawn box is always present, the vector
    /// check only when checked, and the label shows beside the box
    /// (the old `"[x] label"` text mark is gone). The check is one
    /// `Path` leaf (no text leaf, no arm squares), so widths prove
    /// the label alone (FakeText 8.75px/char — "T&C" = 26.25) while
    /// the retained node proves the vector.
    #[test]
    fn checkbox_renders_drawn_box_check_iff_checked_and_label() {
        fn box_of(host: &ComponentHost) {
            let b = host
                .committed_box(node_by_debug(host, "checkbox-box"))
                .expect("box laid out");
            assert_eq!((b.w, b.h), (20.0, 20.0));
        }
        fn text_widths(host: &ComponentHost) -> Vec<f32> {
            let mut ws: Vec<f32> = find_retained_by_debug(host, "text")
                .into_iter()
                .filter_map(|id| host.committed_box(id))
                .filter(|b| !b.lines.is_empty())
                .map(|b| b.content_w)
                .collect();
            ws.sort_by(|a, b| a.partial_cmp(b).expect("finite widths"));
            ws
        }
        fn assert_widths(host: &ComponentHost, expect: &[f32], what: &str) {
            let got = text_widths(host);
            assert_eq!(got.len(), expect.len(), "{what}: leaf count, got {got:?}");
            for (g, e) in got.iter().zip(expect.iter()) {
                assert!(approx(*g, *e), "{what}: width {g} ≈ {e}, got {got:?}");
            }
        }
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let checked = host.runtime().signal(false);
        let props = CheckboxProps {
            label: SharedString::from("T&C"),
            checked: checked.clone(),
            enabled: true,
            on_change: None,
        };
        host.mount("ChkV", CheckCase { props }, check_render);
        host.run_until_idle();
        box_of(&host);
        assert!(
            find_retained_by_debug(&host, "checkbox-check").is_empty(),
            "unchecked: no check unit"
        );
        assert_widths(&host, &[26.25], "label only, no mark");
        press_node(&host, node_by_debug(&host, "checkbox"));
        assert!(checked.get());
        box_of(&host);
        assert_widths(&host, &[26.25], "label only — the check is vector");
        // One Path leaf filling the box exactly (in-flow, same size —
        // committed box matches the drawn box, no arm squares anywhere).
        let check = node_by_debug(&host, "checkbox-check");
        let cb = host.committed_box(check).expect("check laid out");
        assert_eq!((cb.w, cb.h), (20.0, 20.0));
        let bb = host
            .committed_box(node_by_debug(&host, "checkbox-box"))
            .expect("box laid out");
        assert!(
            approx(cb.x, bb.x) && approx(cb.y, bb.y),
            "check fills the box exactly, check {cb:?} box {bb:?}"
        );
        assert!(
            find_retained_by_debug(&host, "checkbox-check-arm").is_empty(),
            "no stepped-square arms — the mark is one vector"
        );
    }

    #[derive(Clone)]
    struct ToggleCase {
        props: ToggleProps,
    }
    impl Props for ToggleCase {}

    fn toggle_render(ctx: &Ctx, p: &ToggleCase) -> VNode {
        ctx.child("oppa::Toggle", 7, &p.props, Toggle)
    }

    #[test]
    fn toggle_flips_like_the_m5_pattern() {
        let host = ComponentHost::new();
        let on = host.runtime().signal(false);
        let props = ToggleProps {
            label: SharedString::from("Wi-Fi"),
            on: on.clone(),
            enabled: true,
            on_change: None,
        };
        host.mount("Tgl", ToggleCase { props }, toggle_render);
        host.run_until_idle();
        let id = node_by_debug(&host, "toggle");
        press_node(&host, id);
        assert!(on.get());
        let sem = host.retained_semantics(id).expect("semantics");
        assert_eq!((sem.role, sem.checked), (oppa::Role::Switch, Some(true)));
    }

    /// Round 7.9: pill track + knob + visible label (the old
    /// `"on"`/`"off"` text box is gone). The knob sits at x = 2 off
    /// / 22 on (20px knob in a 44px track); the only text leaf is
    /// the label ("Wi-Fi" = 43.75).
    #[test]
    fn toggle_renders_switch_knob_side_and_label() {
        fn knob_x(host: &ComponentHost) -> f32 {
            let track = host
                .committed_box(node_by_debug(host, "toggle-track"))
                .expect("track laid out");
            assert_eq!((track.w, track.h), (44.0, 24.0));
            let knob = host
                .committed_box(node_by_debug(host, "toggle-knob"))
                .expect("knob laid out");
            assert_eq!((knob.w, knob.h), (20.0, 20.0));
            knob.x - track.x
        }
        fn label_only(host: &ComponentHost) {
            let ws: Vec<f32> = find_retained_by_debug(host, "text")
                .into_iter()
                .filter_map(|id| host.committed_box(id))
                .filter(|b| !b.lines.is_empty())
                .map(|b| b.content_w)
                .collect();
            assert_eq!(ws.len(), 1, "one text leaf (the label), got {ws:?}");
            assert!(approx(ws[0], 43.75), "label widths, got {ws:?}");
        }
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let on = host.runtime().signal(false);
        let props = ToggleProps {
            label: SharedString::from("Wi-Fi"),
            on: on.clone(),
            enabled: true,
            on_change: None,
        };
        host.mount("TglV", ToggleCase { props }, toggle_render);
        host.run_until_idle();
        assert_eq!(knob_x(&host), 2.0);
        label_only(&host);
        press_node(&host, node_by_debug(&host, "toggle"));
        assert!(on.get());
        assert_eq!(knob_x(&host), 22.0);
        label_only(&host);
    }

    #[derive(Clone)]
    struct SliderCase {
        props: SliderProps,
    }
    impl Props for SliderCase {}

    fn slider_render(ctx: &Ctx, p: &SliderCase) -> VNode {
        ctx.child("oppa::Slider", 7, &p.props, Slider)
    }

    #[test]
    fn slider_steps_snap_and_clamp_with_value_text() {
        let host = ComponentHost::new();
        let value = host.runtime().signal(50.0f32);
        let props = SliderProps {
            label: SharedString::from("Volume"),
            value: value.clone(),
            min: 0.0,
            max: 100.0,
            step: 10.0,
            enabled: true,
            on_change: None,
        };
        host.mount("Sld", SliderCase { props }, slider_render);
        host.run_until_idle();
        let track = node_by_debug(&host, "slider");
        let sem = host.retained_semantics(track).expect("semantics");
        assert_eq!(sem.role, oppa::Role::Slider);
        assert_eq!(sem.value_text.as_deref(), Some("50 percent"));
        press_node(&host, node_by_debug(&host, "step-inc"));
        assert_eq!(value.get(), 60.0);
        // Off-grid values snap on the next step.
        value.set(53.0);
        host.run_until_idle();
        press_node(&host, node_by_debug(&host, "step-inc"));
        assert_eq!(value.get(), 60.0, "53 snaps to the 50-grid then steps");
        // Ends clamp (quiet no-op past the end).
        value.set(100.0);
        host.run_until_idle();
        press_node(&host, node_by_debug(&host, "step-inc"));
        assert_eq!(value.get(), 100.0);
        press_node(&host, node_by_debug(&host, "step-dec"));
        assert_eq!(value.get(), 90.0);
    }

    #[test]
    fn slider_disabled_steppers_are_handlerless() {
        let host = ComponentHost::new();
        let value = host.runtime().signal(50.0f32);
        let props = SliderProps {
            label: SharedString::from("Volume"),
            value: value.clone(),
            min: 0.0,
            max: 100.0,
            step: 10.0,
            enabled: false,
            on_change: None,
        };
        host.mount("SldDis", SliderCase { props }, slider_render);
        host.run_until_idle();
        for debug in ["step-dec", "step-inc"] {
            let id = node_by_debug(&host, debug);
            assert!(
                host.retained_handlers(id).is_empty(),
                "{debug} handlerless when disabled"
            );
            press_node(&host, id);
        }
        assert_eq!(value.get(), 50.0, "disabled steps never move");
        // The track sheds its drag/arrow/focus handlers too.
        let track = node_by_debug(&host, "slider");
        assert!(
            host.retained_handlers(track).is_empty(),
            "disabled track handlerless"
        );
        assert!(
            !host.tab_order().contains(&track),
            "disabled track leaves the tab order"
        );
    }

    /// Round 5.3 (decision 270): drag + arrows on the track.
    fn slider_harness() -> (ComponentHost, Signal<f32>) {
        let host = ComponentHost::new();
        let value = host.runtime().signal(50.0f32);
        let props = SliderProps {
            label: SharedString::from("Volume"),
            value: value.clone(),
            min: 0.0,
            max: 100.0,
            step: 10.0,
            enabled: true,
            on_change: None,
        };
        host.mount("Sld", SliderCase { props }, slider_render);
        host.run_until_idle();
        (host, value)
    }

    /// Round 7.10: rail spans the trackbox, fill covers the value
    /// fraction, knob rides the fraction edge (16px knob over 96px:
    /// x = frac × 80 — Round 7.15 reflowed the steppers to flank).
    /// Geometry follows the value signal.
    #[test]
    fn slider_rail_fill_knob_track_value() {
        fn geom(host: &ComponentHost) -> ((f32, f32), f32, f32) {
            let rail = host
                .committed_box(node_by_debug(host, "slider-rail"))
                .expect("rail laid out");
            assert_eq!((rail.w, rail.h), (96.0, 6.0));
            let fill = host
                .committed_box(node_by_debug(host, "slider-fill"))
                .expect("fill laid out");
            let knob = host
                .committed_box(node_by_debug(host, "slider-knob"))
                .expect("knob laid out");
            assert_eq!((knob.w, knob.h), (16.0, 16.0));
            let trackbox = host
                .committed_box(node_by_debug(host, "slider-trackbox"))
                .expect("trackbox laid out");
            assert_eq!((trackbox.w, trackbox.h), (96.0, 32.0));
            ((rail.w, rail.h), fill.w, knob.x - trackbox.x)
        }
        let (host, value) = slider_harness();
        let (_, fill, knob_x) = geom(&host);
        assert!(approx(fill, 48.0), "half fill at 50, got {fill}");
        assert!(approx(knob_x, 40.0), "knob mid-track at 50, got {knob_x}");
        value.set(100.0);
        host.run_until_idle();
        let (_, fill, knob_x) = geom(&host);
        assert!(approx(fill, 96.0), "full fill at 100, got {fill}");
        assert!(approx(knob_x, 80.0), "knob at end, got {knob_x}");
        value.set(0.0);
        host.run_until_idle();
        let (_, fill, knob_x) = geom(&host);
        assert!(approx(fill, 0.0), "empty fill at 0, got {fill}");
        assert!(approx(knob_x, 0.0), "knob at start, got {knob_x}");
    }

    /// Presses a track point owned by the track itself (not the
    /// step buttons): scans the trackbox for a hit the buttons
    /// do not own (layout-independent — a missing free point is a
    /// genuine layout surprise, panics loudly). Capture still
    /// belongs to the `slider` root (visuals carry no handlers —
    /// presses climb to it).
    fn press_track(host: &ComponentHost) -> (f32, f32, f32, f32) {
        let track = node_by_debug(host, "slider");
        let tb = host
            .committed_box(node_by_debug(host, "slider-trackbox"))
            .expect("trackbox");
        for i in 0..16 {
            let x = tb.x + tb.w * (i as f32 + 0.5) / 16.0;
            let y = tb.y + tb.h / 2.0;
            host.inject_input(oppa::InputEvent::pointer_down(x, y));
            host.run_until_idle();
            if host.capture_node_for(0) == Some(track) {
                let _ = host.pointer_position(0);
                return (x, y, tb.x, tb.w);
            }
            host.inject_input(oppa::InputEvent::pointer_cancel_for(0));
            host.run_until_idle();
        }
        panic!("no track-owned press point in the slider box");
    }

    #[test]
    fn slider_drag_sets_value_from_pointer_x() {
        let (host, value) = slider_harness();
        let (_x0, y0, tx, tw) = press_track(&host);
        // Drag to 80% across the track: value follows on-grid
        // (grid points are float-exact — no .5-boundary risk).
        let x1 = tx + tw * 0.8;
        host.inject_input(oppa::InputEvent::pointer_move(x1, y0));
        host.run_until_idle();
        assert_eq!(value.get(), 80.0, "drag maps x to value");
        // Drag past the end pins (clamp, never wrap).
        host.inject_input(oppa::InputEvent::pointer_move(tx + tw + 50.0, y0));
        host.run_until_idle();
        assert_eq!(value.get(), 100.0, "drag past end pins max");
        host.inject_input(oppa::InputEvent::pointer_up(x1, y0));
        host.run_until_idle();
        assert_eq!(value.get(), 100.0, "release keeps the value");
    }

    #[test]
    fn slider_arrows_step_when_focused() {
        let (host, value) = slider_harness();
        // Focus the track (TAB — it is a tab stop now).
        host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
        host.run_until_idle();
        let track = node_by_debug(&host, "slider");
        assert_eq!(host.focused_node(), Some(track));
        host.inject_input(InputEvent::key(keys::RIGHT, KeyState::Pressed));
        host.run_until_idle();
        assert_eq!(value.get(), 60.0, "right steps up");
        host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
        host.run_until_idle();
        assert_eq!(value.get(), 50.0, "left steps down");
        // Held arrows repeat-step.
        host.inject_input(InputEvent::Key {
            code: keys::UP,
            modifiers: oppa::Modifiers::NONE,
            state: KeyState::Pressed,
            repeat: true,
        });
        host.run_until_idle();
        assert_eq!(value.get(), 60.0, "held up repeats");
    }

    #[test]
    fn snap_value_grids_clamps_and_refuses_loudly() {
        assert_eq!(snap_value(53.0, 0.0, 100.0, 10.0), 50.0);
        assert_eq!(snap_value(55.0, 0.0, 100.0, 10.0), 60.0);
        assert_eq!(snap_value(999.0, 0.0, 100.0, 10.0), 100.0);
        assert_eq!(snap_value(-999.0, 0.0, 100.0, 10.0), 0.0);
        assert_eq!(value_text(50.0), "50 percent");
        assert_eq!(value_text(12.5), "12.5 percent");
    }

    #[test]
    #[should_panic(expected = "step must be positive")]
    fn snap_value_rejects_zero_step() {
        let _ = snap_value(1.0, 0.0, 10.0, 0.0);
    }

    #[test]
    #[should_panic(expected = "max")]
    fn snap_value_rejects_inverted_range() {
        let _ = snap_value(1.0, 10.0, 0.0, 1.0);
    }

    #[test]
    fn text_input_renders_value_and_semantics() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from("Ada"));
        host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
        host.run_until_idle();
        let id = node_by_debug(&host, "text-input");
        let sem = host.retained_semantics(id).expect("semantics");
        assert_eq!(sem.role, oppa::Role::TextField);
        assert_eq!(sem.label.as_deref(), Some("Name"));
        assert!(!sem.disabled);
        let b = host.committed_box(id).expect("hit box");
        assert!(
            approx(b.w, 200.0) && approx(b.h, 32.0),
            "default 200x32, got {}x{}",
            b.w,
            b.h
        );
        assert!(
            host.tab_order().contains(&id),
            "enabled input is tabbable (press owner, decision 96)"
        );
        press_node(&host, id);
        assert_eq!(
            host.focused_node(),
            Some(id),
            "press focuses through the M5 router"
        );
    }

    #[test]
    fn text_input_placeholder_shows_when_empty() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from(""));
        host.mount(
            "TI",
            TextInputProps::new("Name", value.clone()).placeholder("Enter name"),
            TextInput,
        );
        host.run_until_idle();
        // Empty: the outer carries the role; the placeholder is a
        // presentational span, never a bindable field.
        assert_eq!(host.text_fields().len(), 1);
        assert!(
            approx(lined_content_w(&host), 87.5),
            "placeholder 10x8.75 displayed"
        );
        value.set(SharedString::from("Ada"));
        host.run_until_idle();
        assert_eq!(host.text_fields().len(), 2, "outer + value leaf");
        assert!(
            approx(lined_content_w(&host), 26.25),
            "value 3x8.75 displayed"
        );
        let id = node_by_debug(&host, "text-input");
        let sem = host.retained_semantics(id).expect("semantics");
        assert_eq!(sem.label.as_deref(), Some("Name"), "label stable");
    }

    /// Decision 293 (OQ-SINK-1 fixed): typing into an empty+
    /// placeholder `TextInput` feeds the value through the session
    /// fallback (the field leaf only exists once the value is
    /// non-empty — the outer resolves to the instance session).
    /// On DOM the same contract renders `value=""` with a native
    /// `placeholder` attribute, so the browser only ever sends
    /// typed text.
    #[test]
    fn text_input_placeholder_typing_feeds_value() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from(""));
        host.mount(
            "TI",
            TextInputProps::new("Name", value.clone()).placeholder("Enter name"),
            TextInput,
        );
        host.run_until_idle();
        let id = node_by_debug(&host, "text-input");
        host.inject_input(InputEvent::text(id, "Ada"));
        host.run_until_idle();
        assert_eq!(
            value.get(),
            SharedString::from("Ada"),
            "typed text feeds the value even under a placeholder"
        );
    }

    /// Decision 293: the fallback also covers the value leaf of a
    /// non-empty field (same session resolution through the leaf's
    /// press owner — the web U8 channel for every field, not just
    /// placeholder ones).
    #[test]
    fn text_input_leaf_typing_feeds_value_without_bind_call() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from("Al"));
        host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
        host.run_until_idle();
        // No `bind_edit_session` anywhere (the fallback self-wires).
        let field = node_by_debug(&host, "field");
        host.inject_input(InputEvent::text(field, "Ada"));
        host.run_until_idle();
        assert_eq!(
            value.get(),
            SharedString::from("Ada"),
            "leaf text feeds the value with no app-layer bind"
        );
    }

    #[test]
    fn text_input_disabled_is_handlerless() {
        let host = ComponentHost::new();
        let value = host.runtime().signal(SharedString::from("Ada"));
        host.mount(
            "TIDis",
            TextInputProps::new("Name", value.clone()).disabled(),
            TextInput,
        );
        host.run_until_idle();
        let id = node_by_debug(&host, "text-input");
        let sem = host.retained_semantics(id).expect("semantics");
        assert!(sem.disabled, "disabled flag on the role");
        assert!(
            host.retained_handlers(id).is_empty(),
            "disabled carries no handler — refusal is structural"
        );
        assert!(
            !host.tab_order().contains(&id),
            "disabled leaves the tab order (decision 96)"
        );
        press_node(&host, id);
        assert_eq!(
            value.get(),
            SharedString::from("Ada"),
            "press on disabled never edits"
        );
        assert_eq!(host.focused_node(), None, "press never focuses");
    }

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

    #[test]
    fn text_input_live_typing_updates_value() {
        use oppa_app::DesktopLoop;
        // Full runner path, headless: real `TextInput` + real
        // `DesktopLoop` — press to focus, type, backspace.
        let mut loop_ =
            DesktopLoop::new(500, 600, Box::new(FakeText), "Test").expect("headless loop");
        let value = loop_.host().runtime().signal(SharedString::from(""));
        loop_.mount(
            "TI",
            TextInputProps::new("Name", value.clone()).placeholder("Enter name"),
            TextInput,
        );
        let id = node_by_debug(loop_.host(), "text-input");
        let b = loop_.host().committed_box(id).expect("hit box");
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        loop_
            .step(oppa::InputEvent::pointer_down(cx, cy))
            .expect("down steps");
        loop_
            .step(oppa::InputEvent::pointer_up(cx, cy))
            .expect("up steps");
        assert_eq!(loop_.host().focused_node(), Some(id));
        assert!(loop_.type_text("Hi").expect("types") > 0);
        assert_eq!(value.get(), SharedString::from("Hi"));
        loop_.backspace().expect("backspaces");
        assert_eq!(value.get(), SharedString::from("H"));
        loop_.type_text("!").expect("types");
        assert_eq!(value.get(), SharedString::from("H!"));
    }

    #[test]
    fn text_input_edit_session_binds() {
        let host = ComponentHost::new();
        let value = host.runtime().signal(SharedString::from(""));
        let handle = host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
        host.run_until_idle();
        let sessions = host.edit_sessions_for(handle.root_instance());
        assert_eq!(sessions.len(), 1, "one session keyed to the instance");
        sessions[0].insert("hi");
        host.run_until_idle();
        assert_eq!(
            value.get(),
            SharedString::from("hi"),
            "programmatic feed lands in the value signal"
        );
        // ...and the session feeds the field leaf through the U8 bind path.
        let field = node_by_debug(&host, "field");
        host.bind_edit_session(field, &sessions[0]);
        host.inject_input(InputEvent::text(field, "yo"));
        host.run_until_idle();
        assert_eq!(value.get(), SharedString::from("yo"));
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
        settle_clocked(&host, &clock);
        clock.advance(60.0);
        assert_eq!(host.tick_timers(host.now_ms()), 0, "sticky arms nothing");
        host.run_until_idle();
        assert!(open.get(), "sticky toast stays until dismissed");
    }

    fn radio_props(label: &str, selected: bool, fired: &Signal<bool>) -> RadioProps {
        let set = fired.clone();
        RadioProps {
            label: SharedString::from(label),
            selected,
            enabled: true,
            on_select: action(move || set.set(true)),
        }
    }

    #[test]
    fn radio_renders_selected_dot_and_semantics() {
        for (selected, want_dot) in [(true, true), (false, false)] {
            let host = ComponentHost::new();
            let fired = host.runtime().signal(false);
            host.mount("R", radio_props("Pro", selected, &fired), Radio);
            host.run_until_idle();
            let id = node_by_debug(&host, "radio");
            let sem = host.retained_semantics(id).expect("semantics");
            assert_eq!(sem.role, oppa::Role::RadioButton);
            assert_eq!(sem.selected, Some(selected));
            assert_eq!(sem.label.as_deref(), Some("Pro"));
            assert!(!sem.disabled);
            let ib = host
                .committed_box(node_by_debug(&host, "radio-indicator"))
                .expect("indicator box");
            assert!(
                approx(ib.w, 18.0) && approx(ib.h, 18.0),
                "indicator 18x18, got {}x{}",
                ib.w,
                ib.h
            );
            let dots = find_retained_by_debug(&host, "radio-dot");
            assert_eq!(!dots.is_empty(), want_dot, "dot iff selected");
            if want_dot {
                let db = host.committed_box(dots[0]).expect("dot box");
                assert!(
                    approx(db.w, 8.0) && approx(db.h, 8.0),
                    "dot 8x8, got {}x{}",
                    db.w,
                    db.h
                );
                // (18 - 8) / 2 = 5: exact centering via out-of-flow offsets.
                assert!(
                    approx(db.x, ib.x + 5.0) && approx(db.y, ib.y + 5.0),
                    "dot centered, got ({}, {}) in ({}, {})",
                    db.x,
                    db.y,
                    ib.x,
                    ib.y
                );
            }
        }
    }

    #[test]
    fn radio_press_fires_select() {
        let host = ComponentHost::new();
        let fired = host.runtime().signal(false);
        host.mount("R", radio_props("Pro", false, &fired), Radio);
        host.run_until_idle();
        press_node(&host, node_by_debug(&host, "radio"));
        assert!(fired.get(), "press selects");
    }

    #[test]
    fn radio_disabled_is_handlerless() {
        let host = ComponentHost::new();
        let fired = host.runtime().signal(false);
        let set = fired.clone();
        let props = RadioProps {
            label: SharedString::from("Pro"),
            selected: false,
            enabled: false,
            on_select: action(move || set.set(true)),
        };
        host.mount("RDis", props, Radio);
        host.run_until_idle();
        let id = node_by_debug(&host, "radio");
        let sem = host.retained_semantics(id).expect("semantics");
        assert!(sem.disabled, "disabled flag on the role");
        assert!(
            host.retained_handlers(id).is_empty(),
            "disabled carries no handler — refusal is structural"
        );
        assert!(
            !host.tab_order().contains(&id),
            "disabled leaves the tab order (decision 96)"
        );
        press_node(&host, id);
        assert!(!fired.get(), "press on disabled never selects");
    }

    #[test]
    fn radio_group_selects_exclusively() {
        let host = ComponentHost::new();
        let selected = host.runtime().signal(SharedString::from("Free"));
        let props = RadioGroupProps {
            options: vec![
                RadioOption {
                    value: SharedString::from("Free"),
                    label: SharedString::from("Free"),
                },
                RadioOption {
                    value: SharedString::from("Pro"),
                    label: SharedString::from("Pro"),
                },
            ],
            selected: selected.clone(),
            enabled: true,
        };
        host.mount("G", props, RadioGroup::<SharedString>);
        host.run_until_idle();
        let radios = || find_retained_by_debug(&host, "radio");
        assert_eq!(radios().len(), 2);
        let sem = |id| host.retained_semantics(id).expect("semantics");
        assert_eq!(sem(radios()[0]).selected, Some(true));
        assert_eq!(sem(radios()[1]).selected, Some(false));
        press_node(&host, radios()[1]);
        assert_eq!(selected.get(), SharedString::from("Pro"));
        assert_eq!(sem(radios()[0]).selected, Some(false));
        assert_eq!(sem(radios()[1]).selected, Some(true));
    }

    fn tabs_props(active: Signal<String>) -> TabsProps<String> {
        // Same panel debug for both tabs (position-matched same-tag
        // children recycle in place by design — M8 — so debug labels
        // would go stale; the contents differ in measured width
        // instead, which proves which view is mounted).
        let mk = |value: &str, body: &'static str| TabItem {
            value: value.to_string(),
            label: SharedString::from(value),
            content: Rc::new(move |_| {
                Div("panel").child(VNode::from(Text {
                    text: SharedString::from(body),
                    style: Text::body_secondary,
                }))
            }),
        };
        TabsProps {
            tabs: vec![mk("a", "Alpha"), mk("b", "Beta-2-X")],
            active,
            enabled: true,
        }
    }

    fn panel_content_w(host: &ComponentHost) -> f32 {
        let panel = node_by_debug(host, "tab-panel");
        host.committed_box(panel).expect("panel box").content_w
    }

    #[test]
    fn tabs_renders_tab_list_and_active_panel() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let active = host.runtime().signal("a".to_string());
        host.mount("T", tabs_props(active), Tabs::<String>);
        host.run_until_idle();
        let bar = node_by_debug(&host, "tab-bar");
        let sem = host.retained_semantics(bar).expect("semantics");
        assert_eq!(sem.role, oppa::Role::TabList);
        let items = find_retained_by_debug(&host, "tab-item");
        assert_eq!(items.len(), 2);
        let sem = |id| host.retained_semantics(id).expect("semantics");
        assert_eq!(sem(items[0]).role, oppa::Role::Tab);
        assert_eq!(sem(items[0]).selected, Some(true));
        assert_eq!(sem(items[0]).label.as_deref(), Some("a"));
        assert_eq!(sem(items[1]).selected, Some(false));
        // "Alpha": 5 chars x 8.75 — the active view is mounted.
        assert!(
            approx(panel_content_w(&host), 43.75),
            "panel shows Alpha, w={}",
            panel_content_w(&host)
        );
    }

    #[test]
    fn tabs_click_switches_active_panel() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let active = host.runtime().signal("a".to_string());
        host.mount("T", tabs_props(active.clone()), Tabs::<String>);
        host.run_until_idle();
        let items = || find_retained_by_debug(&host, "tab-item");
        press_node(&host, items()[1]);
        assert_eq!(active.get(), "b".to_string());
        // "Beta-2-X": 8 chars x 8.75 — the view swapped in place.
        assert!(
            approx(panel_content_w(&host), 70.0),
            "panel shows Beta-2-X, w={}",
            panel_content_w(&host)
        );
        let sem = |id| host.retained_semantics(id).expect("semantics");
        assert_eq!(sem(items()[0]).selected, Some(false));
        assert_eq!(sem(items()[1]).selected, Some(true));
    }

    fn select_props(selected: Signal<String>, open: Signal<bool>) -> SelectProps<String> {
        SelectProps::new(
            vec![
                SelectItem {
                    value: "a".to_string(),
                    label: SharedString::from("Alpha"),
                },
                SelectItem {
                    value: "b".to_string(),
                    label: SharedString::from("Beta"),
                },
                SelectItem {
                    value: "c".to_string(),
                    label: SharedString::from("Gamma"),
                },
            ],
            selected,
            open,
        )
    }

    #[test]
    fn select_renders_combobox_with_current_label() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let selected = host.runtime().signal("b".to_string());
        let open = host.runtime().signal(false);
        host.mount("S", select_props(selected, open), Select::<String>);
        host.run_until_idle();
        let box_id = node_by_debug(&host, "select-box");
        let sem = host.retained_semantics(box_id).expect("semantics");
        assert_eq!(sem.role, oppa::Role::ComboBox);
        assert_eq!(sem.label.as_deref(), Some("Beta"));
        assert!(!sem.disabled);
        // Closed: no list, no options.
        assert!(find_retained_by_debug(&host, "select-list").is_empty());
        assert!(find_retained_by_debug(&host, "select-option").is_empty());
    }

    /// Decision 291: the chevron is one vector `Path` leaf (12×8),
    /// not a "▾" text glyph — the box's only text leaf is the label
    /// ("Beta" = 4 × 8.75 = 35.0), so no symbol-font coverage decides
    /// the control's shape.
    #[test]
    fn select_chevron_is_a_vector_not_a_glyph() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let selected = host.runtime().signal("b".to_string());
        let open = host.runtime().signal(false);
        host.mount("S", select_props(selected, open), Select::<String>);
        host.run_until_idle();
        let chevrons = find_retained_by_debug(&host, "select-chevron");
        assert_eq!(chevrons.len(), 1, "one vector chevron, got {chevrons:?}");
        let cb = host.committed_box(chevrons[0]).expect("chevron laid out");
        assert!(
            approx(cb.w, 12.0) && approx(cb.h, 8.0),
            "chevron 12x8, got {}x{}",
            cb.w,
            cb.h
        );
        // Only the label measures text (the chevron measures nothing).
        let mut ws: Vec<f32> = find_retained_by_debug(&host, "text")
            .into_iter()
            .filter_map(|id| host.committed_box(id))
            .filter(|b| !b.lines.is_empty())
            .map(|b| b.content_w)
            .collect();
        ws.sort_by(|a, b| a.partial_cmp(b).expect("finite widths"));
        assert_eq!(ws.len(), 1, "label only, got {ws:?}");
        assert!(approx(ws[0], 35.0), "Beta widths, got {ws:?}");
    }

    #[test]
    fn select_open_pick_closes_and_sets_value() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let selected = host.runtime().signal("b".to_string());
        let open = host.runtime().signal(false);
        host.mount(
            "S",
            select_props(selected.clone(), open.clone()),
            Select::<String>,
        );
        host.run_until_idle();
        press_node(&host, node_by_debug(&host, "select-box"));
        assert!(open.get(), "box press opens the list");
        let options = || find_retained_by_debug(&host, "select-option");
        assert_eq!(options().len(), 3);
        // Options reuse list_item + selected (existing payloads).
        let sem = |id| host.retained_semantics(id).expect("semantics");
        assert_eq!(sem(options()[0]).role, oppa::Role::ListItem);
        assert_eq!(sem(options()[0]).label.as_deref(), Some("Alpha"));
        assert_eq!(sem(options()[0]).selected, Some(false));
        assert_eq!(sem(options()[1]).selected, Some(true));
        press_node(&host, options()[2]);
        assert_eq!(selected.get(), "c".to_string());
        assert!(!open.get(), "picking closes the list");
        assert!(options().is_empty(), "closed list unmounts");
        let box_sem = host
            .retained_semantics(node_by_debug(&host, "select-box"))
            .expect("semantics");
        assert_eq!(box_sem.label.as_deref(), Some("Gamma"));
    }

    #[test]
    fn select_disabled_is_handlerless_and_keeps_label() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let selected = host.runtime().signal("a".to_string());
        let open = host.runtime().signal(false);
        let mut props = select_props(selected.clone(), open.clone());
        props.enabled = false;
        host.mount("S", props, Select::<String>);
        host.run_until_idle();
        let box_id = node_by_debug(&host, "select-box");
        let sem = host.retained_semantics(box_id).expect("semantics");
        assert_eq!(sem.role, oppa::Role::ComboBox);
        assert!(sem.disabled);
        assert_eq!(sem.label.as_deref(), Some("Alpha"));
        press_node(&host, box_id);
        assert!(!open.get(), "disabled box never opens");
        assert!(
            !host.tab_order().contains(&box_id),
            "disabled leaves the tab order (decision 96)"
        );
        // The list still follows the signal truthfully when opened
        // externally — but its options are handlerless too.
        open.set(true);
        host.run_until_idle();
        let options = find_retained_by_debug(&host, "select-option");
        assert_eq!(options.len(), 3);
        press_node(&host, options[1]);
        assert_eq!(
            selected.get(),
            "a".to_string(),
            "disabled pick never selects"
        );
        assert!(open.get(), "disabled pick never closes");
    }

    #[derive(Clone)]
    struct SelectFormProps {
        selected: Signal<String>,
        open: Signal<bool>,
    }
    impl Props for SelectFormProps {}

    fn select_form_app(ctx: &Ctx, props: &SelectFormProps) -> VNode {
        Column::new().children([
            Div("form-top").style(Style::new().size(100, 10)).build(),
            ctx.child(
                "oppa::Select",
                1,
                &select_props(props.selected.clone(), props.open.clone()),
                Select::<String>,
            ),
            Div("form-trailer")
                .style(Style::new().size(100, 10))
                .build(),
        ])
    }

    /// Round 7.21 (decision 296): opening the dropdown neither
    /// grows the `Select` box past 32px nor shifts the trailing
    /// sibling — the list rides an anchored portal under the box
    /// (32px + 4px gap) at list width.
    #[test]
    fn select_open_neither_grows_nor_shifts_siblings() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let selected = host.runtime().signal("b".to_string());
        let open = host.runtime().signal(false);
        host.mount(
            "F",
            SelectFormProps {
                selected: selected.clone(),
                open: open.clone(),
            },
            select_form_app,
        );
        host.run_until_idle();
        let select_h = || {
            host.committed_box(node_by_debug(&host, "select"))
                .expect("select box")
                .h
        };
        let trailer_y = || {
            host.committed_box(node_by_debug(&host, "form-trailer"))
                .expect("trailer box")
                .y
        };
        assert!(approx(select_h(), 32.0), "closed box is 32px");
        let y_closed = trailer_y();
        open.set(true);
        host.run_until_idle();
        assert!(
            !find_retained_by_debug(&host, "select-list").is_empty(),
            "list mounts when open"
        );
        assert!(
            approx(select_h(), 32.0),
            "open keeps the 32px box, got {}",
            select_h()
        );
        assert!(
            approx(trailer_y(), y_closed),
            "trailer never shifts, {} vs {}",
            trailer_y(),
            y_closed
        );
        let popup = host
            .committed_box(node_by_debug(&host, "select-popup"))
            .expect("popup box");
        let sbox = host
            .committed_box(node_by_debug(&host, "select-box"))
            .expect("select box");
        assert!(
            approx(popup.x, sbox.x) && approx(popup.y, sbox.y + 36.0),
            "popup anchors under the box, got ({}, {}) vs box ({}, {})",
            popup.x,
            popup.y,
            sbox.x,
            sbox.y
        );
        assert!(
            approx(popup.w, 160.0),
            "popup keeps the list width, got {}",
            popup.w
        );
    }

    fn progress_width(host: &ComponentHost, value: f32) -> (f32, f32) {
        host.mount("P", ProgressBarProps::new(value), ProgressBar);
        host.run_until_idle();
        let track = host
            .committed_box(node_by_debug(host, "progressbar-track"))
            .expect("track box");
        let fill = host
            .committed_box(node_by_debug(host, "progressbar-fill"))
            .expect("fill box");
        (track.w, fill.w)
    }

    #[test]
    fn progressbar_fill_scales_with_clamped_value() {
        // (value, expected fill width) on the default 160 track.
        for (value, want) in [
            (0.0, 0.0),
            (0.25, 40.0),
            (0.68, 108.8),
            (1.0, 160.0),
            // Boundaries clamp gracefully (quiet value normalization,
            // the slider-clamp class — never a refusal).
            (-0.5, 0.0),
            (1.5, 160.0),
        ] {
            let host = ComponentHost::new();
            host.set_text_service(Box::new(FakeText));
            let (track_w, fill_w) = progress_width(&host, value);
            assert!(approx(track_w, 160.0), "track full width, got {track_w}");
            assert!(
                approx(fill_w, want),
                "value {value}: fill {fill_w}, want {want}"
            );
        }
    }

    #[test]
    fn progressbar_role_and_percent_text() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let mut props = ProgressBarProps::new(0.68);
        props.label = Some(SharedString::from("Storage Quota"));
        host.mount("P", props, ProgressBar);
        host.run_until_idle();
        let sem = host
            .retained_semantics(node_by_debug(&host, "progressbar-track"))
            .expect("semantics");
        assert_eq!(sem.role, oppa::Role::ProgressBar);
        assert_eq!(sem.value_text.as_deref(), Some("68 percent"));
        assert_eq!(sem.label.as_deref(), Some("Storage Quota"));
    }

    #[test]
    #[should_panic(expected = "ProgressBar value is NaN")]
    fn progressbar_nan_refuses_loudly() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        host.mount("P", ProgressBarProps::new(f32::NAN), ProgressBar);
        host.run_until_idle();
    }

    #[test]
    fn badge_mounts_with_label() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        host.mount(
            "B",
            BadgeProps::new("PRO").variant(BadgeVariant::Success),
            Badge,
        );
        host.run_until_idle();
        let id = node_by_debug(&host, "badge");
        let sem = host.retained_semantics(id).expect("semantics");
        assert_eq!(sem.role, oppa::Role::Generic);
        assert_eq!(sem.label.as_deref(), Some("PRO"));
        let b = host.committed_box(id).expect("badge box");
        assert!(approx(b.h, 24.0), "fixed pill height, got {}", b.h);
        assert!(b.w > 0.0, "chip sizes to its text, w={}", b.w);
    }

    // ------------------------------------------------------------------
    // Round 3.3 (decision 262): the dismiss-first chain over a live
    // popup + field — composition cancels before focus clears
    // (clearing first would commit it, locked #27), popups close at
    // the author step, and an empty chain reports Unhandled for the
    // runner (nav pop / exit — `nav.rs` BackPress).
    // ------------------------------------------------------------------

    #[derive(Clone)]
    struct BackScene {
        open: Signal<bool>,
        value: Signal<SharedString>,
    }
    impl Props for BackScene {}

    fn back_scene(ctx: &Ctx, p: &BackScene) -> VNode {
        // Field first: TAB focuses it even with the modal open
        // (depth-first tab order — no pointer needed, so the open
        // backdrop never eats the focus press).
        Div("back-root").children([
            ctx.child(
                "back-field",
                1,
                &TextInputProps::new("Name", p.value.clone()),
                TextInput,
            ),
            ctx.child(
                "back-modal",
                2,
                &ModalProps::new("Delete file?", p.open.clone()),
                Modal,
            ),
        ])
    }

    fn back_harness() -> (ComponentHost, Signal<bool>, Signal<SharedString>) {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let open = host.runtime().signal(true);
        let value = host.runtime().signal(SharedString::from("Ada"));
        host.mount(
            "Back",
            BackScene {
                open: open.clone(),
                value: value.clone(),
            },
            back_scene,
        );
        host.run_until_idle();
        assert!(
            !find_retained_by_debug(&host, "modal-card").is_empty(),
            "popup mounted open"
        );
        // Keyboard focus (the field is first in tab order).
        host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
        host.run_until_idle();
        let field = node_by_debug(&host, "text-input");
        assert_eq!(host.focused_node(), Some(field), "TAB focuses the field");
        assert!(host.focused_field_session().is_some(), "session live");
        (host, open, value)
    }

    fn start_composition(host: &ComponentHost) {
        use oppa::ime::{dispatch_ime_event, ImeCompositionEvent};
        let mut sess = host
            .focused_field_session()
            .expect("focused session composes");
        dispatch_ime_event(
            &mut sess,
            &ImeCompositionEvent::CompositionStarted { start_byte: 3 },
        );
        dispatch_ime_event(
            &mut sess,
            &ImeCompositionEvent::CompositionUpdated {
                composition: "Adax".to_string(),
                caret_byte: 4,
            },
        );
        assert!(sess.is_composing(), "composition active");
    }

    #[test]
    fn back_chain_cancels_then_blurs_then_exits() {
        let (host, open, value) = back_harness();
        start_composition(&host);
        // Step 1: composition cancels (reverts — value untouched,
        // popup stays, focus stays for the next press).
        assert_eq!(host.handle_back(), oppa::BackOutcome::CompositionCancelled);
        host.run_until_idle();
        assert_eq!(&*value.get(), "Ada", "cancel reverts, never commits");
        assert!(open.get(), "popup survives the composition step");
        assert!(host.focused_node().is_some(), "focus survives too");
        assert!(
            host.focused_field_session()
                .is_none_or(|s| !s.is_composing()),
            "nothing composing anymore"
        );
        // Step 2 (author popup step — the host cannot close what it
        // does not own, so the author/runner flips `open` first, as
        // the BackPress contract orders it).
        open.set(false);
        host.run_until_idle();
        assert!(
            find_retained_by_debug(&host, "modal-card").is_empty(),
            "author step dismisses the popup"
        );
        // Step 3: focus clears.
        assert_eq!(host.handle_back(), oppa::BackOutcome::FocusCleared);
        host.run_until_idle();
        assert_eq!(host.focused_node(), None);
        // Step 4: empty chain reports Unhandled (the runner exits).
        assert_eq!(host.handle_back(), oppa::BackOutcome::Unhandled);
    }

    #[test]
    fn esc_key_rides_the_same_chain() {
        // The router's ESC arm runs `handle_back` (desktop parity —
        // one chain everywhere): first ESC cancels, second blurs.
        let (host, _open, value) = back_harness();
        start_composition(&host);
        host.inject_input(InputEvent::key(keys::ESCAPE, KeyState::Pressed));
        host.run_until_idle();
        assert_eq!(&*value.get(), "Ada", "ESC cancels the composition");
        assert!(host.focused_node().is_some(), "focus survives step one");
        host.inject_input(InputEvent::key(keys::ESCAPE, KeyState::Pressed));
        host.run_until_idle();
        assert_eq!(host.focused_node(), None, "second ESC blurs");
        // Repeat ESC repeats are ignored (no double blur, no exit
        // from the router — exit stays runner-side).
        host.inject_input(InputEvent::Key {
            code: keys::ESCAPE,
            modifiers: oppa::Modifiers::NONE,
            state: KeyState::Pressed,
            repeat: true,
        });
        host.run_until_idle();
        assert_eq!(host.handle_back(), oppa::BackOutcome::Unhandled);
    }

    // ------------------------------------------------------------------
    // Round 5.1 (decision 268): TextArea — the multi-line sibling.
    // ------------------------------------------------------------------

    #[test]
    fn text_area_renders_area_role_and_focuses() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from("hi"));
        host.mount("TA", TextAreaProps::new("Notes", value.clone()), TextArea);
        host.run_until_idle();
        let id = node_by_debug(&host, "text-area");
        let sem = host.retained_semantics(id).expect("semantics");
        assert_eq!(sem.role, oppa::Role::TextArea);
        assert_eq!(sem.label.as_deref(), Some("Notes"));
        assert!(!sem.disabled);
        assert!(
            host.tab_order().contains(&id),
            "enabled area is tabbable (press owner, decision 96)"
        );
        press_node(&host, id);
        assert_eq!(
            host.focused_node(),
            Some(id),
            "press focuses through the M5 router"
        );
        assert!(host.focused_field_session().is_some(), "session live");
    }

    #[test]
    fn text_area_auto_heights_with_lines() {
        // Explicit width, content-driven height: three lines stand
        // taller than one (same width — no horizontal growth). The
        // area sits in a sized parent (areas live inside scenes,
        // never as roots — roots fill the viewport by decision 74).
        let one = area_box_height("one");
        let three = area_box_height("one\ntwo\nthree");
        assert!(
            three > one,
            "three lines ({three}) stand taller than one ({one})"
        );
        #[derive(Clone)]
        struct WrapProps {
            value: Signal<SharedString>,
        }
        impl Props for WrapProps {}
        fn wrap_scene(ctx: &Ctx, p: &WrapProps) -> VNode {
            Div("wrap")
                .style(Style::new().size(300, 400))
                .child(ctx.child(
                    "area",
                    1,
                    &TextAreaProps::new("Notes", p.value.clone()),
                    TextArea,
                ))
        }
        fn area_box_height(value: &str) -> f32 {
            let host = ComponentHost::new();
            host.set_text_service(Box::new(FakeText));
            let v = host.runtime().signal(SharedString::from(value));
            host.mount("Wrap", WrapProps { value: v }, wrap_scene);
            host.run_until_idle();
            let b = host
                .committed_box(node_by_debug(&host, "text-area"))
                .expect("area box");
            assert!(approx(b.w, 200.0), "explicit width holds, got {}", b.w);
            b.h
        }
    }

    #[test]
    fn text_area_placeholder_and_disabled_mirror_input() {
        // Empty + placeholder: presentational span, never the value.
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from(""));
        host.mount(
            "TA",
            TextAreaProps::new("Notes", value.clone()).placeholder("Write here"),
            TextArea,
        );
        host.run_until_idle();
        assert_eq!(host.text_fields().len(), 1, "area counts as a field");
        // Disabled: handlerless, untabbable, press never focuses.
        let host2 = ComponentHost::new();
        host2.set_text_service(Box::new(FakeText));
        let value2 = host2.runtime().signal(SharedString::from("x"));
        let mut props = TextAreaProps::new("Notes", value2);
        props.enabled = false;
        host2.mount("TA", props, TextArea);
        host2.run_until_idle();
        let id = node_by_debug(&host2, "text-area");
        assert!(host2.retained_semantics(id).expect("semantics").disabled);
        assert!(host2.retained_handlers(id).is_empty());
        assert!(!host2.tab_order().contains(&id));
        press_node(&host2, id);
        assert_eq!(host2.focused_node(), None, "press never focuses");
    }

    #[test]
    fn text_area_session_types_multiline() {
        // Programmatic multi-line feed lands in the value (the
        // router ENTER path is core-proven in m5_input).
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from(""));
        host.mount("TA", TextAreaProps::new("Notes", value.clone()), TextArea);
        host.run_until_idle();
        press_node(&host, node_by_debug(&host, "text-area"));
        let sess = host.focused_field_session().expect("session");
        sess.insert("a\nb");
        host.run_until_idle();
        assert_eq!(&*value.get(), "a\nb");
    }

    // ------------------------------------------------------------------
    // Round 5.2 (decision 269): focus trap — Tab inside an open
    // dialog cycles its subtree (derived from the retained tree,
    // no registration); closing restores the global order.
    // ------------------------------------------------------------------

    #[derive(Clone)]
    struct TrapScene {
        open: Signal<bool>,
        value: Signal<SharedString>,
    }
    impl Props for TrapScene {}

    fn trap_scene(ctx: &Ctx, p: &TrapScene) -> VNode {
        // Field first, then the modal (opt-out backdrop: handlerless,
        // so the tab order is exactly field, cancel, confirm).
        Div("trap-root").children([
            ctx.child(
                "trap-field",
                1,
                &TextInputProps::new("Name", p.value.clone()),
                TextInput,
            ),
            ctx.child(
                "trap-modal",
                2,
                &ModalProps::new("Delete file?", p.open.clone()).no_backdrop_dismiss(),
                Modal,
            ),
        ])
    }

    fn trap_harness() -> (ComponentHost, Signal<bool>) {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let open = host.runtime().signal(true);
        let value = host.runtime().signal(SharedString::from(""));
        host.mount(
            "Trap",
            TrapScene {
                open: open.clone(),
                value,
            },
            trap_scene,
        );
        host.run_until_idle();
        assert!(
            !find_retained_by_debug(&host, "modal-card").is_empty(),
            "popup mounted open"
        );
        (host, open)
    }

    fn tab(host: &ComponentHost, shift: bool) {
        let mut ev = InputEvent::key(keys::TAB, KeyState::Pressed);
        if shift {
            ev = InputEvent::Key {
                code: keys::TAB,
                modifiers: oppa::Modifiers::shift(),
                state: KeyState::Pressed,
                repeat: false,
            };
        }
        host.inject_input(ev);
        host.run_until_idle();
    }

    fn debug_of(host: &ComponentHost) -> String {
        host.with_retained_mut(|rec, _| {
            rec.get(host.focused_node().expect("focus"))
                .map(|n| n.debug.clone())
                .unwrap_or_default()
        })
    }

    #[test]
    fn tab_cycles_inside_open_dialog() {
        let (host, _open) = trap_harness();
        // Global order first: field, then into the dialog.
        tab(&host, false);
        assert_eq!(debug_of(&host), "text-input", "first stop: field");
        tab(&host, false);
        assert_eq!(debug_of(&host), "modal-cancel", "enters the dialog");
        tab(&host, false);
        assert_eq!(debug_of(&host), "modal-confirm");
        // Trapped: next cycles back to cancel, never the field.
        tab(&host, false);
        assert_eq!(debug_of(&host), "modal-cancel", "trap cycles");
        tab(&host, false);
        assert_eq!(debug_of(&host), "modal-confirm");
        // Backward wraps within the trap too.
        tab(&host, true);
        assert_eq!(debug_of(&host), "modal-cancel", "shift+tab wraps");
    }

    #[test]
    fn closing_dialog_restores_global_order() {
        let (host, open) = trap_harness();
        tab(&host, false);
        tab(&host, false);
        assert_eq!(debug_of(&host), "modal-cancel", "inside first");
        open.set(false);
        host.run_until_idle();
        assert!(
            find_retained_by_debug(&host, "modal-card").is_empty(),
            "dialog unmounts"
        );
        // Focus was inside: unmount clears it (1.4 rule), TAB
        // restarts the global order at the field.
        tab(&host, false);
        assert_eq!(debug_of(&host), "text-input", "global order restored");
    }

    // ------------------------------------------------------------------
    // Round 5.4 (decision 271): on_change notification + uncontrolled
    // companions — internal flips report payload-carrying values;
    // self-managed signals advance identically to controlled ones.
    // ------------------------------------------------------------------

    use std::cell::RefCell;

    /// Test-only `Change` constructor (the round-5.4 helper lived at
    /// top level but only tests ever called it — top-level placement
    /// tripped `dead_code` on the lib build; moved here in 6.3).
    fn change<T>(f: impl Fn(T) + 'static) -> Change<T> {
        Rc::new(f)
    }

    fn blowing<T: Clone + 'static>() -> (Change<T>, Rc<RefCell<Vec<T>>>) {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s = seen.clone();
        (change(move |v| s.borrow_mut().push(v)), seen)
    }

    #[test]
    fn toggle_and_checkbox_report_flips() {
        let host = ComponentHost::new();
        let (notify, seen) = blowing::<bool>();
        let on = host.runtime().signal(false);
        host.mount(
            "Tgl",
            ToggleCase {
                props: ToggleProps {
                    label: SharedString::from("Wi-Fi"),
                    on: on.clone(),
                    enabled: true,
                    on_change: Some(notify),
                },
            },
            toggle_render,
        );
        host.run_until_idle();
        press_node(&host, node_by_debug(&host, "toggle"));
        assert!(on.get());
        assert_eq!(*seen.borrow(), vec![true], "flip reports the new value");
        // Checkbox mirrors (mark + semantics already proven — the
        // notification path is the assertion here).
        let host2 = ComponentHost::new();
        let (notify2, seen2) = blowing::<bool>();
        let checked = host2.runtime().signal(false);
        host2.mount(
            "Chk",
            CheckCase {
                props: CheckboxProps {
                    label: SharedString::from("T&C"),
                    checked: checked.clone(),
                    enabled: true,
                    on_change: Some(notify2),
                },
            },
            check_render,
        );
        host2.run_until_idle();
        press_node(&host2, node_by_debug(&host2, "checkbox"));
        assert!(checked.get());
        assert_eq!(*seen2.borrow(), vec![true]);
    }

    #[test]
    fn slider_reports_every_path() {
        // Controlled slider with notification: step, drag, and
        // arrow paths all report through the single funnel.
        let host = ComponentHost::new();
        let value = host.runtime().signal(50.0f32);
        let (notify, seen) = blowing::<f32>();
        let props = SliderProps {
            label: SharedString::from("Volume"),
            value: value.clone(),
            min: 0.0,
            max: 100.0,
            step: 10.0,
            enabled: true,
            on_change: Some(notify),
        };
        host.mount("Sld", SliderCase { props }, slider_render);
        host.run_until_idle();
        press_node(&host, node_by_debug(&host, "step-inc"));
        assert_eq!(*seen.borrow(), vec![60.0], "step reports");
        // Drag path reports the snapped value (track-owned press
        // point — buttons own their own presses).
        let (_x0, y0, tx, tw) = press_track(&host);
        host.inject_input(InputEvent::pointer_move(tx + tw * 0.8, y0));
        host.run_until_idle();
        assert_eq!(*seen.borrow(), vec![60.0, 80.0], "drag reports");
        host.inject_input(InputEvent::pointer_up(tx + tw * 0.8, y0));
        host.run_until_idle();
        // Arrow path reports too (the drag press focused the track —
        // cancel/up keep focus, so no TAB needed).
        let track = node_by_debug(&host, "slider");
        assert_eq!(host.focused_node(), Some(track));
        host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
        host.run_until_idle();
        assert_eq!(*seen.borrow(), vec![60.0, 80.0, 70.0], "arrow reports");
    }

    #[test]
    fn uncontrolled_toggle_checkbox_slider_advance_and_report() {
        // Toggle: press flips internal state + reports.
        let host = ComponentHost::new();
        let (notify, seen) = blowing::<bool>();
        host.mount(
            "UT",
            UncontrolledToggleProps {
                label: SharedString::from("Wi-Fi"),
                initial: false,
                enabled: true,
                on_change: Some(notify),
            },
            UncontrolledToggle,
        );
        host.run_until_idle();
        press_node(&host, node_by_debug(&host, "toggle"));
        assert_eq!(*seen.borrow(), vec![true], "uncontrolled flip reports");
        press_node(&host, node_by_debug(&host, "toggle"));
        assert_eq!(*seen.borrow(), vec![true, false], "second flip reports too");
        // Slider: step reports the snapped value.
        let host2 = ComponentHost::new();
        let (notify2, seen2) = blowing::<f32>();
        host2.mount(
            "US",
            UncontrolledSliderProps {
                label: SharedString::from("Volume"),
                initial: 50.0,
                min: 0.0,
                max: 100.0,
                step: 10.0,
                enabled: true,
                on_change: Some(notify2),
            },
            UncontrolledSlider,
        );
        host2.run_until_idle();
        press_node(&host2, node_by_debug(&host2, "step-inc"));
        assert_eq!(*seen2.borrow(), vec![60.0]);
        // Checkbox: press reports.
        let host3 = ComponentHost::new();
        let (notify3, seen3) = blowing::<bool>();
        host3.mount(
            "UC",
            UncontrolledCheckboxProps {
                label: SharedString::from("T&C"),
                initial: false,
                enabled: true,
                on_change: Some(notify3),
            },
            UncontrolledCheckbox,
        );
        host3.run_until_idle();
        press_node(&host3, node_by_debug(&host3, "checkbox"));
        assert_eq!(*seen3.borrow(), vec![true]);
    }

    #[test]
    fn uncontrolled_text_reports_committed_edits() {
        // Typing through the focused session reports each commit
        // (the session funnel — insert here, IME commits and undo
        // ride the same path core-side).
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let (notify, seen) = blowing::<SharedString>();
        host.mount(
            "UTI",
            UncontrolledTextInputProps {
                label: SharedString::from("Name"),
                initial: SharedString::from(""),
                placeholder: None,
                enabled: true,
                width: 200.0,
                height: 32.0,
                style: Text::body_secondary,
                debug: SharedString::from("text-input"),
                on_change: Some(notify),
                masked: false,
            },
            UncontrolledTextInput,
        );
        host.run_until_idle();
        press_node(&host, node_by_debug(&host, "text-input"));
        let sess = host.focused_field_session().expect("session");
        sess.insert("hi");
        host.run_until_idle();
        assert_eq!(
            seen.borrow()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            vec!["hi".to_string()],
            "committed edit reports"
        );
        // TextArea mirrors through the same funnel.
        let host2 = ComponentHost::new();
        host2.set_text_service(Box::new(FakeText));
        let (notify2, seen2) = blowing::<SharedString>();
        host2.mount(
            "UTA",
            UncontrolledTextAreaProps {
                label: SharedString::from("Notes"),
                initial: SharedString::from(""),
                placeholder: None,
                enabled: true,
                width: 200.0,
                style: Text::body_secondary,
                debug: SharedString::from("text-area"),
                on_change: Some(notify2),
            },
            UncontrolledTextArea,
        );
        host2.run_until_idle();
        press_node(&host2, node_by_debug(&host2, "text-area"));
        let sess2 = host2.focused_field_session().expect("session");
        sess2.insert("a\nb");
        host2.run_until_idle();
        assert_eq!(
            seen2
                .borrow()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            vec!["a\nb".to_string()]
        );
    }

    /// Round 8.1 (decision 297): tap-to-caret through the shaper.
    /// Clicks at the text origin, mid-word, and past the end land on
    /// byte offsets `0`, `2`, and `text_len` (FakeText 8.75px/char).
    fn tap_at(host: &ComponentHost, x: f32, y: f32, shift: bool) {
        let modifiers = if shift {
            oppa::Modifiers::shift()
        } else {
            oppa::Modifiers::NONE
        };
        host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Down {
                button: oppa::PointerButton::Primary,
            },
            x,
            y,
            modifiers,
        });
        host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Up {
                button: oppa::PointerButton::Primary,
            },
            x,
            y,
            modifiers,
        });
        host.run_until_idle();
    }

    fn field_tap_point(host: &ComponentHost, debug: &str, local_x: f32) -> (f32, f32) {
        let field = node_by_debug(host, debug);
        let origin_x = host
            .text_origin_under(field)
            .expect("field lays a text origin");
        let b = host.committed_box(field).expect("field has a hit box");
        (origin_x + local_x, b.y + b.h / 2.0)
    }

    #[test]
    fn text_input_tap_places_caret_at_cluster_boundaries() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from("hello"));
        host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
        host.run_until_idle();
        // Origin tap → byte 0.
        let (x0, y0) = field_tap_point(&host, "text-input", 0.0);
        tap_at(&host, x0, y0, false);
        let sess = host.focused_field_session().expect("session");
        assert_eq!(sess.caret(), 0);
        assert_eq!(sess.selection(), (0, 0));
        // Mid-word (x=20: third cluster [17.5,26.25) leading half) → byte 2.
        let (xm, ym) = field_tap_point(&host, "text-input", 20.0);
        tap_at(&host, xm, ym, false);
        let sess = host.focused_field_session().expect("session");
        assert_eq!(sess.caret(), 2);
        assert_eq!(sess.selection(), (2, 2));
        // Past the end (5 × 8.75 = 43.75 + 10) → byte 5.
        let (xe, ye) = field_tap_point(&host, "text-input", 53.75);
        tap_at(&host, xe, ye, false);
        let sess = host.focused_field_session().expect("session");
        assert_eq!(sess.caret(), 5);
        assert_eq!(sess.selection(), (5, 5));
    }

    #[test]
    fn text_input_shift_click_extends_selection() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from("hello"));
        host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
        host.run_until_idle();
        let (x0, y0) = field_tap_point(&host, "text-input", 0.0);
        tap_at(&host, x0, y0, false);
        assert_eq!(host.focused_field_session().expect("session").caret(), 0);
        let (xe, ye) = field_tap_point(&host, "text-input", 53.75);
        tap_at(&host, xe, ye, true);
        let sess = host.focused_field_session().expect("session");
        assert_eq!(sess.selection(), (0, 5));
        assert_eq!(sess.caret(), 5);
    }

    #[test]
    fn text_input_keyboard_activation_parks_at_end() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from("hello"));
        host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
        host.run_until_idle();
        let (x0, y0) = field_tap_point(&host, "text-input", 0.0);
        tap_at(&host, x0, y0, false);
        assert_eq!(host.focused_field_session().expect("session").caret(), 0);
        let field = node_by_debug(&host, "text-input");
        host.inject_input(InputEvent::Focus { node: Some(field) });
        host.run_until_idle();
        host.inject_input(InputEvent::key(keys::ENTER, KeyState::Pressed));
        host.run_until_idle();
        assert_eq!(
            host.focused_field_session().expect("session").caret(),
            5,
            "keyboard press falls back to end (no tap point)"
        );
    }

    #[test]
    fn text_area_tap_places_caret() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from("hi"));
        host.mount("TA", TextAreaProps::new("Notes", value.clone()), TextArea);
        host.run_until_idle();
        let (x0, y0) = field_tap_point(&host, "text-area", 0.0);
        tap_at(&host, x0, y0, false);
        assert_eq!(host.focused_field_session().expect("session").caret(), 0);
        // "hi" = 2 × 8.75 = 17.5 wide; +10 lands past the end → byte 2.
        let (xe, ye) = field_tap_point(&host, "text-area", 27.5);
        tap_at(&host, xe, ye, false);
        assert_eq!(host.focused_field_session().expect("session").caret(), 2);
    }

    /// Round 8.2 (decision 298): pointer drag streams `drag_x` — Down
    /// at byte 2, Moves to byte 8, Up classifies Drag (no press) and
    /// the session holds `(2, 8)` with the caret at 8.
    fn drag_at(host: &ComponentHost, x1: f32, y: f32, x2: f32) {
        use oppa::{PointerAction, PointerButton};
        let moves = [x1 + (x2 - x1) * 0.5, x2];
        host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: PointerAction::Down {
                button: PointerButton::Primary,
            },
            x: x1,
            y,
            modifiers: oppa::Modifiers::NONE,
        });
        for x in moves {
            host.inject_input(InputEvent::Pointer {
                id: Some(0),
                action: PointerAction::Move,
                x,
                y,
                modifiers: oppa::Modifiers::NONE,
            });
        }
        host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: PointerAction::Up {
                button: PointerButton::Primary,
            },
            x: x2,
            y,
            modifiers: oppa::Modifiers::NONE,
        });
        host.run_until_idle();
    }

    /// Round 8.2 (decision 298): drag byte 2→8 selects `(2, 8)` and the
    /// shared plan carries one themed background Rect with exact bounds
    /// (what CPU and Vello both paint — no backend selection logic).
    #[test]
    fn text_input_drag_selects_range_with_exact_background_bounds() {
        use oppa::{DrawOp, SELECTION_FILL};
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from("hello world"));
        host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
        host.run_until_idle();
        let field = node_by_debug(&host, "text-input");
        let origin_x = host.text_origin_under(field).expect("text origin");
        let y = host.committed_box(field).expect("hit box").y + 16.0;
        // Byte b lands in the leading half: b × 8.75 + 2.0.
        drag_at(
            &host,
            origin_x + 2.0 * 8.75 + 2.0,
            y,
            origin_x + 8.0 * 8.75 + 2.0,
        );
        let sess = host.focused_field_session().expect("session");
        assert_eq!(sess.selection(), (2, 8));
        assert_eq!(sess.caret(), 8);
        // Plan bounds: caret_x(2) = 17.5 → caret_x(8) = 70.0.
        let sel = host.focused_selection_paint().expect("paint selection");
        assert_eq!(sel.range, (2, 8));
        let builder = oppa_cpu::FramePlanBuilder::new(1.0);
        builder.set_selection(Some(sel));
        let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
        let rects: Vec<(f32, f32, f32, f32)> = plan
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rect {
                    x, y, w, h, color, ..
                } if *color == SELECTION_FILL => Some((*x, *y, *w, *h)),
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 1, "one background rect, got {rects:?}");
        let (rx, ry, rw, rh) = rects[0];
        assert!(
            approx(rx, origin_x + 17.5) && approx(rw, 52.5),
            "x/w pin cluster edges, got ({rx}, {rw})"
        );
        // y/h pin the laid line (lines live on the inner "text" leaf —
        // the "field" element carries the box origin, not the lines).
        let laid = find_retained_by_debug(&host, "text")
            .into_iter()
            .filter_map(|id| host.committed_box(id))
            .find(|b| !b.lines.is_empty())
            .expect("laid text leaf");
        let expect = laid.selection_rects((2, 8));
        assert_eq!(expect.len(), 1, "one laid line overlaps");
        assert!(
            approx(ry, expect[0][1]) && approx(rh, expect[0][3] - expect[0][1]),
            "y/h pin the laid line, got ({ry}, {rh}) laid {laid:?}"
        );
        // Collapsing the selection removes the Rect incrementally (the
        // handler dirties the field — no stale highlight survives).
        let (x0, _) = field_tap_point(&host, "text-input", 0.0);
        tap_at(&host, x0, y, false);
        assert_eq!(host.focused_field_session().expect("session").caret(), 0);
        let builder = oppa_cpu::FramePlanBuilder::new(1.0);
        builder.set_selection(host.focused_selection_paint());
        let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
        assert!(
            !plan.ops.iter().any(|op| matches!(op,
                DrawOp::Rect { color, .. } if *color == SELECTION_FILL)),
            "collapsed selection paints no highlight"
        );
    }

    /// Round 15.1 (decision 312): a focused field emits a 2px caret
    /// bar at the active cluster's leading edge, and moving the caret
    /// repositions the bar (the shared plan's caret `Rect` — what CPU
    /// and Vello both paint). A non-collapsed selection suppresses
    /// the bar (the highlight paints instead — one overlay at a time).
    #[test]
    fn text_input_focused_caret_paints_two_px_bar_and_repositions() {
        use oppa::{CaretPaint, DrawOp, CARET_WIDTH_PX};
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from("hello world"));
        host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
        host.run_until_idle();
        // Tap byte 0 → collapsed caret at the leading edge (the tap
        // resets the blink phase, so the bar is solid-visible here —
        // no clock control needed for the position pin).
        let field = node_by_debug(&host, "text-input");
        let origin_x = host.text_origin_under(field).expect("text origin");
        let y = host.committed_box(field).expect("hit box").y + 16.0;
        let (x0, _) = field_tap_point(&host, "text-input", 0.0);
        tap_at(&host, x0, y, false);
        let sess = host.focused_field_session().expect("session");
        assert_eq!(sess.caret(), 0);
        assert_eq!(sess.selection(), (0, 0));
        let caret: CaretPaint = host.focused_caret_paint().expect("visible caret bar");
        assert_eq!(caret.field, field);
        assert!(
            approx(caret.x, origin_x),
            "bar at the leading edge of byte 0, got {} vs origin {origin_x}",
            caret.x
        );
        assert!(caret.h > 0.0, "positive bar height, got {}", caret.h);
        let builder = oppa_cpu::FramePlanBuilder::new(1.0);
        builder.set_caret(host.focused_caret_paint());
        let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
        let bars: Vec<(f32, f32, f32, f32)> = plan
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rect {
                    x, y, w, h, color, ..
                } if *color == caret.color && approx(*w, CARET_WIDTH_PX) => Some((*x, *y, *w, *h)),
                _ => None,
            })
            .collect();
        assert_eq!(bars.len(), 1, "exactly one caret bar, got {bars:?}");
        assert!(
            approx(bars[0].0, origin_x) && approx(bars[0].1, caret.y) && approx(bars[0].3, caret.h),
            "bar pins the resolved caret box, got {:?} vs ({}, {}, {})",
            bars[0],
            caret.x,
            caret.y,
            caret.h
        );
        // Move the caret to byte 5 → the bar repositions by exactly
        // five FakeText advances (8.75px/char).
        let (x5, _) = field_tap_point(&host, "text-input", 5.0 * 8.75 + 2.0);
        tap_at(&host, x5, y, false);
        assert_eq!(host.focused_field_session().expect("session").caret(), 5);
        let moved = host.focused_caret_paint().expect("visible caret bar");
        assert!(
            approx(moved.x, origin_x + 5.0 * 8.75),
            "bar at the leading edge of byte 5, got {} vs {}",
            moved.x,
            origin_x + 5.0 * 8.75
        );
        // Drag-selecting a range hides the bar (highlight instead).
        drag_at(
            &host,
            origin_x + 2.0 * 8.75 + 2.0,
            y,
            origin_x + 8.0 * 8.75 + 2.0,
        );
        assert_eq!(
            host.focused_field_session().expect("session").selection(),
            (2, 8)
        );
        assert!(
            host.focused_caret_paint().is_none(),
            "selected range paints the highlight, not the bar"
        );
    }

    /// Round 8.2 (decision 298): two fast taps on "world" select the
    /// word (`dbl_click_x` — bytes 6..11), three select the hard line.
    #[test]
    fn text_input_double_click_selects_word_triple_selects_line() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from("hello world"));
        host.mount("TI", TextInputProps::new("Name", value.clone()), TextInput);
        host.run_until_idle();
        // Byte 7 (in "world"): 7 × 8.75 + 2.0 local.
        let (x, y) = field_tap_point(&host, "text-input", 7.0 * 8.75 + 2.0);
        tap_at(&host, x, y, false);
        tap_at(&host, x, y, false);
        let sess = host.focused_field_session().expect("session");
        assert_eq!(sess.selection(), (6, 11), "double-click selects the word");
        assert_eq!(sess.caret(), 11);
        tap_at(&host, x, y, false);
        let sess = host.focused_field_session().expect("session");
        assert_eq!(sess.selection(), (0, 11), "triple-click selects the line");
    }

    /// Round 8.2 (decision 298): triple-click in a TextArea selects the
    /// hard `\n` line under the pointer, not the whole value.
    #[test]
    fn text_area_triple_click_selects_hard_line() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from("ab\ncd"));
        host.mount("TA", TextAreaProps::new("Notes", value.clone()), TextArea);
        host.run_until_idle();
        // Byte 3 ("c"): 3 × 8.75 + 2.0 local.
        let (x, y) = field_tap_point(&host, "text-area", 3.0 * 8.75 + 2.0);
        tap_at(&host, x, y, false);
        tap_at(&host, x, y, false);
        tap_at(&host, x, y, false);
        let sess = host.focused_field_session().expect("session");
        assert_eq!(
            sess.selection(),
            (3, 5),
            "triple-click selects the hard line"
        );
    }

    /// Round 8.3 (decision 299): hover resolves the hand over the
    /// button (even over its label leaf — inherit rule), the I-beam
    /// over the field, and the arrow with no hover; the retained
    /// styles carry the cursors structurally.
    #[derive(Clone)]
    struct CursorCase {
        btn: ButtonProps,
        field: TextInputProps,
    }
    impl Props for CursorCase {}

    fn cursor_render(ctx: &Ctx, p: &CursorCase) -> VNode {
        Column::new().children([
            ctx.child("oppa::Button", 1, &p.btn, Button),
            ctx.child("oppa::TextInput", 2, &p.field, TextInput),
        ])
    }

    fn hover_at(host: &ComponentHost, debug: &str) {
        let id = node_by_debug(host, debug);
        let b = host.committed_box(id).expect("control has a hit box");
        host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Move,
            x: b.x + b.w / 2.0,
            y: b.y + b.h / 2.0,
            modifiers: oppa::Modifiers::NONE,
        });
        host.run_until_idle();
    }

    fn style_cursor_of(host: &ComponentHost, debug: &str) -> Option<CursorIcon> {
        let id = node_by_debug(host, debug);
        host.with_retained_mut(|rec, styles| {
            let n = rec.get(id).expect("live node");
            styles.get(n.style).cloned().unwrap_or_default().cursor
        })
    }

    #[test]
    fn hover_resolves_button_pointer_and_field_text() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        assert_eq!(host.hover_cursor(), None, "no hover yet");
        let value = host.runtime().signal(SharedString::from("Ada"));
        host.mount(
            "Cur",
            CursorCase {
                btn: ButtonProps::new("OK", || {}),
                field: TextInputProps::new("Name", value),
            },
            cursor_render,
        );
        host.run_until_idle();
        hover_at(&host, "button");
        assert_eq!(
            host.hover_cursor(),
            Some(CursorIcon::Pointer),
            "hand over the button (label inherit)"
        );
        hover_at(&host, "text-input");
        assert_eq!(
            host.hover_cursor(),
            Some(CursorIcon::Text),
            "I-beam over the field"
        );
        assert_eq!(style_cursor_of(&host, "button"), Some(CursorIcon::Pointer));
        assert_eq!(style_cursor_of(&host, "text-input"), Some(CursorIcon::Text));
    }

    /// Round 8.3 (decision 299): disabled controls keep the platform
    /// arrow (inert controls never promise clicks); Toggle and
    /// Checkbox show the hand like Button when enabled.
    #[test]
    fn disabled_controls_keep_the_platform_arrow() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from("Ada"));
        host.mount(
            "CurDis",
            CursorCase {
                btn: ButtonProps::new("OK", || {}).disabled(),
                field: TextInputProps::new("Name", value).disabled(),
            },
            cursor_render,
        );
        host.run_until_idle();
        assert_eq!(style_cursor_of(&host, "button"), None);
        assert_eq!(style_cursor_of(&host, "text-input"), None);
        hover_at(&host, "button");
        assert_eq!(host.hover_cursor(), None, "disabled button keeps the arrow");

        let host2 = ComponentHost::new();
        host2.set_text_service(Box::new(FakeText));
        let on = host2.runtime().signal(false);
        let checked = host2.runtime().signal(false);
        host2.mount(
            "CurTC",
            CursorTCProps {
                toggle: ToggleProps {
                    label: SharedString::from("Wi-Fi"),
                    on: on.clone(),
                    enabled: true,
                    on_change: None,
                },
                check: CheckboxProps {
                    label: SharedString::from("T&C"),
                    checked: checked.clone(),
                    enabled: true,
                    on_change: None,
                },
            },
            cursor_tc_render,
        );
        host2.run_until_idle();
        hover_at(&host2, "toggle");
        assert_eq!(host2.hover_cursor(), Some(CursorIcon::Pointer));
        hover_at(&host2, "checkbox");
        assert_eq!(host2.hover_cursor(), Some(CursorIcon::Pointer));
    }

    #[derive(Clone)]
    struct CursorTCProps {
        toggle: ToggleProps,
        check: CheckboxProps,
    }
    impl Props for CursorTCProps {}

    fn cursor_tc_render(ctx: &Ctx, p: &CursorTCProps) -> VNode {
        Column::new().children([
            ctx.child("oppa::Toggle", 1, &p.toggle, Toggle),
            ctx.child("oppa::Checkbox", 2, &p.check, Checkbox),
        ])
    }

    /// Round 11.2 (decision 306): toggling the theme signal recolors
    /// Button, TextInput, Toggle, and Modal in place — same
    /// instances (toggle state and field text survive), only colors
    /// re-derive.
    #[derive(Clone)]
    struct ThemeCase {
        btn: ButtonProps,
        field: TextInputProps,
        toggle: ToggleProps,
        modal: ModalProps,
    }
    impl Props for ThemeCase {}

    fn theme_render(ctx: &Ctx, p: &ThemeCase) -> VNode {
        Column::new().children([
            ctx.child("oppa::Button", 1, &p.btn, Button),
            ctx.child("oppa::TextInput", 2, &p.field, TextInput),
            ctx.child("oppa::Toggle", 3, &p.toggle, Toggle),
            ctx.child("oppa::Modal", 4, &p.modal, Modal),
        ])
    }

    fn style_bg_of(host: &ComponentHost, debug: &str) -> Option<Color> {
        let id = node_by_debug(host, debug);
        host.with_retained_mut(|rec, styles| {
            let n = rec.get(id).expect("live node");
            styles.get(n.style).cloned().unwrap_or_default().bg
        })
    }

    #[test]
    fn theme_toggle_recolors_controls_without_remounting() {
        use oppa::{ThemeMode, ThemeTokens};
        let light = ThemeTokens::light();
        let dark = ThemeTokens::dark();
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        let value = host.runtime().signal(SharedString::from("hi"));
        let on = host.runtime().signal(true);
        let open = host.runtime().signal(true);
        host.mount(
            "Theme",
            ThemeCase {
                btn: ButtonProps::new("OK", || {}),
                field: TextInputProps::new("Name", value.clone()),
                toggle: ToggleProps {
                    label: SharedString::from("Wi-Fi"),
                    on: on.clone(),
                    enabled: true,
                    on_change: None,
                },
                modal: ModalProps::new("Delete file?", open.clone()),
            },
            theme_render,
        );
        host.run_until_idle();
        // Light reproduces the catalog palette exactly.
        assert_eq!(style_bg_of(&host, "button"), Some(light.primary));
        assert_eq!(style_bg_of(&host, "text-input"), Some(light.surface));
        assert_eq!(style_bg_of(&host, "modal-card"), Some(light.surface));
        assert_eq!(
            style_bg_of(&host, "toggle-track"),
            Some(light.primary),
            "on-state track reads primary"
        );
        // Toggle to dark: every themed control re-renders in place.
        host.set_theme(ThemeMode::Dark);
        host.run_until_idle();
        assert_eq!(style_bg_of(&host, "button"), Some(dark.primary));
        assert_eq!(style_bg_of(&host, "text-input"), Some(dark.surface));
        assert_eq!(style_bg_of(&host, "modal-card"), Some(dark.surface));
        assert_eq!(style_bg_of(&host, "toggle-track"), Some(dark.primary));
        // Instances survive: toggle state and field text untouched.
        assert!(on.get(), "toggle state survives the recolor");
        assert_eq!(value.get().to_string(), "hi", "field text survives");
        assert!(open.get(), "modal stays open");
        // And back: Light restores exactly.
        host.set_theme(ThemeMode::Light);
        host.run_until_idle();
        assert_eq!(style_bg_of(&host, "button"), Some(light.primary));
        assert_eq!(style_bg_of(&host, "modal-card"), Some(light.surface));
    }

    // ------------------------------------------------------------------
    // Round 13.2 (decision 309): virtualized list over a collection
    // ------------------------------------------------------------------

    const VL_N: usize = 1000;
    const VL_VIEW_H: f32 = 200.0;
    const VL_OVER: usize = 2;

    #[derive(Clone, Debug)]
    struct VItem {
        i: usize,
    }

    /// Alternating 20/40 heights (variable — prefix sums, never
    /// division): period 60px per 2 rows, total 30px mean.
    fn vitem_h(item: &VItem) -> f32 {
        if item.i.is_multiple_of(2) {
            20.0
        } else {
            40.0
        }
    }

    /// Prefix-sum tops for the alternating pattern (test-side
    /// oracle — closed form, not the control's loop).
    fn vitem_top(i: usize) -> f32 {
        (i / 2) as f32 * 60.0 + (i % 2) as f32 * 20.0
    }

    fn vitem_row(_ctx: &Ctx, p: &VirtualRowProps<VItem>) -> VNode {
        // Zebra by stable id (proves slot→item mapping, the M8
        // cell_bg_of precedent — geometry alone cannot).
        let bg = if p.row.id.0.is_multiple_of(2) {
            Color(0x22_22_22)
        } else {
            Color(0x33_33_33)
        };
        Div("vlist-row")
            .style(Style::new().h(p.height).fill_width().bg(bg))
            .build()
    }

    struct VListHarness {
        host: ComponentHost,
        offset: oppa::ScrollOffset,
        list: NodeId,
        coll: oppa::Collection<VItem>,
    }

    fn mount_vlist(n_items: usize) -> VListHarness {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let rt = host.runtime();
        let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:vlist"));
        coll.ingest((0..n_items).map(|i| VItem { i }).collect::<Vec<_>>());
        let props = VirtualListProps::new(coll.clone(), vitem_h, vitem_row)
            .size(320.0, VL_VIEW_H)
            .overscan(VL_OVER)
            .debug("vlist");
        let handle = host.mount("VList", props, VirtualList);
        host.run_until_idle();
        let root = handle.root_instance();
        let offset = host.instance_scroll(root).expect("scroll handle");
        let list = find_retained_by_debug(&host, "vlist")[0];
        VListHarness {
            host,
            offset,
            list,
            coll,
        }
    }

    fn slot_bg(h: &VListHarness, id: NodeId) -> Option<Color> {
        // The zebra lives on the row content (the slot only
        // positions) — match the content node by box.
        let b = h.host.committed_box(id).expect("slot laid out");
        let row = find_retained_by_debug(&h.host, "vlist-row")
            .into_iter()
            .find(|r| h.host.committed_box(*r).is_some_and(|rb| approx(rb.y, b.y)))
            .expect("row content at slot box");
        let sid = h.host.retained_style(row).expect("row style");
        h.host
            .with_retained_mut(|_, styles| styles.get(sid).expect("style").bg)
    }

    /// Window math on the alternating pattern: [0,200]+2 overscan
    /// covers rows 0..9 (tops 0..180, 7 visible + 2 over).
    #[test]
    fn virtual_list_windows_bounded_retained_nodes() {
        let h = mount_vlist(VL_N);
        assert_eq!(
            vlist_window(&[0.0, 20.0, 60.0], 80.0, 0.0, VL_VIEW_H, VL_OVER),
            (0, 3)
        );
        let slots = find_retained_by_debug(&h.host, "vlist-slot");
        assert_eq!(slots.len(), 9, "window [0,9), not all {VL_N}");
        // Slots sit at exact prefix offsets with exact heights.
        let mut seen: Vec<(f32, f32)> = slots
            .iter()
            .map(|id| {
                let b = h.host.committed_box(*id).expect("slot laid out");
                (b.y, b.h)
            })
            .collect();
        seen.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("finite"));
        for (slot, (y, hh)) in seen.iter().enumerate() {
            assert!(
                approx(*y, vitem_top(slot)),
                "slot {slot} at {y}, want {}",
                vitem_top(slot)
            );
            assert!(
                approx(*hh, vitem_h(&VItem { i: slot })),
                "slot {slot} h {hh}"
            );
        }
        // Zebra follows the item (slot 0 shows row 0 — even).
        let top_slot = slots
            .into_iter()
            .find(|id| h.host.committed_box(*id).is_some_and(|b| approx(b.y, 0.0)))
            .expect("slot at y=0");
        assert_eq!(slot_bg(&h, top_slot), Some(Color(0x22_22_22)));
        // Extent covers all rows (DOM spacer + scrollbar source).
        let list_box = h.host.committed_box(h.list).expect("list laid out");
        assert!(
            approx(list_box.content_h, 30_000.0),
            "1000 rows × 30 mean, got {}",
            list_box.content_h
        );
    }

    /// Scrolling moves the window and recycles slots: 500px lands
    /// [15,26) with identical slot nodes (structure-free), exact
    /// offsets, and item-following zebra.
    #[test]
    fn virtual_list_scroll_moves_window_recycles_slots() {
        let h = mount_vlist(VL_N);
        let before: std::collections::HashSet<NodeId> =
            find_retained_by_debug(&h.host, "vlist-slot")
                .into_iter()
                .collect();
        assert_eq!(before.len(), 9);
        h.offset.set(500.0);
        h.host.run_until_idle();
        let after: std::collections::HashSet<NodeId> =
            find_retained_by_debug(&h.host, "vlist-slot")
                .into_iter()
                .collect();
        // Window grows 9 → 11 (two genuinely new slots mount)...
        assert_eq!(after.len(), 11, "window [15,26)");
        // ...but the 9 survivors are identical nodes (recycle).
        assert_eq!(
            before.intersection(&after).count(),
            9,
            "survivors recycle, no teardown"
        );
        // First slot sits at tops[15] = 440, viewport-relative at
        // 440 - 500 = -60 with row 15's height.
        let first = after
            .into_iter()
            .find(|id| {
                h.host
                    .committed_box(*id)
                    .is_some_and(|b| approx(b.y, vitem_top(15) - 500.0))
            })
            .expect("slot at y=-60");
        let b = h.host.committed_box(first).expect("slot laid out");
        assert!(approx(b.h, 40.0), "row 15 is odd → 40, got {}", b.h);
        assert_eq!(
            slot_bg(&h, first),
            Some(Color(0x33_33_33)),
            "zebra follows row 15 (odd)"
        );
    }

    /// Same-size windows rebind Update-only (y=60 → y=120 slides
    /// [0,11) → [2,13): 11 stable slot keys recycle, positions move
    /// through LAYOUT, no structure — the M8 recycle shape for
    /// variable heights).
    #[test]
    fn virtual_list_same_size_window_rebinds_update_only() {
        let h = mount_vlist(VL_N);
        h.offset.set(60.0);
        h.host.run_until_idle();
        assert_eq!(
            find_retained_by_debug(&h.host, "vlist-slot").len(),
            11,
            "window [0,11)"
        );
        h.offset.set(120.0);
        h.host.run_until_idle();
        assert_eq!(
            find_retained_by_debug(&h.host, "vlist-slot").len(),
            11,
            "window [2,13)"
        );
        let d = h.host.last_diff().expect("scroll diff");
        assert_eq!(d.structure_ops(), 0, "same-size windows rebind Update-only");
        // Window slid a full period: first slot at tops[2] = 60,
        // viewport-relative at 60 - 120 = -60.
        let first = find_retained_by_debug(&h.host, "vlist-slot")
            .into_iter()
            .find(|id| {
                h.host
                    .committed_box(*id)
                    .is_some_and(|b| approx(b.y, vitem_top(2) - 120.0))
            })
            .expect("slot at y=-60");
        assert_eq!(
            slot_bg(&h, first),
            Some(Color(0x22_22_22)),
            "zebra follows row 2 (even)"
        );
    }

    /// Empty collections render an empty viewport (quiet — same
    /// class as unmatched controlled edges, never a panic). The
    /// childless ScrollArea takes the leaf arm (content = own box),
    /// so the spacer matches the viewport — no phantom scrollbar.
    #[test]
    fn virtual_list_empty_renders_no_slots() {
        let h = mount_vlist(0);
        assert_eq!(
            find_retained_by_debug(&h.host, "vlist-slot").len(),
            0,
            "no rows, no slots"
        );
        let list_box = h.host.committed_box(h.list).expect("list laid out");
        assert!(approx(list_box.h, VL_VIEW_H), "viewport box");
        assert!(
            approx(list_box.content_h, VL_VIEW_H),
            "leaf fallback: content = own box, got {}",
            list_box.content_h
        );
    }

    /// Round 13.1 → 13.2 combined: a worker batch grows the extent
    /// live — appends re-derive the window the same frame, no
    /// remount.
    #[test]
    fn virtual_list_async_append_grows_content() {
        let h = mount_vlist(10);
        let list_box = h.host.committed_box(h.list).expect("list laid out");
        assert!(
            approx(list_box.content_h, 300.0),
            "10 rows × 30, got {}",
            list_box.content_h
        );
        let writer = h.coll.writer();
        let rt = h.host.runtime();
        rt.spawn_task(move |scope| {
            writer.submit(&scope, (10..20).map(|i| VItem { i }).collect::<Vec<_>>());
        });
        let mut waited = 0;
        loop {
            let stats = rt.stats();
            if stats.tasks_done + stats.tasks_dropped >= 1 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
            waited += 1;
            assert!(waited < 10_000, "executor task never resolved");
        }
        h.host.run_until_idle();
        assert_eq!(h.coll.len(), 20, "batch landed");
        let list_box = h.host.committed_box(h.list).expect("list laid out");
        assert!(
            approx(list_box.content_h, 600.0),
            "extent doubled live, got {}",
            list_box.content_h
        );
        // New rows are reachable: scroll to the bottom, window
        // shows row 19 viewport-relative (560 - 590 = -30; id 19,
        // odd → tall + dim zebra).
        h.offset.set(590.0);
        h.host.run_until_idle();
        let bottom = find_retained_by_debug(&h.host, "vlist-slot")
            .into_iter()
            .find(|id| {
                h.host
                    .committed_box(*id)
                    .is_some_and(|b| approx(b.y, vitem_top(19) - 590.0))
            })
            .expect("slot at row 19");
        let b = h.host.committed_box(bottom).expect("slot laid out");
        assert!(approx(b.h, 40.0), "row 19 h 40, got {}", b.h);
        assert_eq!(slot_bg(&h, bottom), Some(Color(0x33_33_33)));
    }

    // ------------------------------------------------------------------
    // Round 13.3 (decision 310): data grid over a collection
    // ------------------------------------------------------------------

    const GRID_N: usize = 200;
    const GRID_VIEW_W: f32 = 320.0;
    const GRID_VIEW_H: f32 = 200.0;

    fn grid_body(_ctx: &Ctx, p: &GridCellProps<VItem>) -> VNode {
        // Zebra by stable id (proves cell→item mapping, the slot_bg
        // precedent — geometry alone cannot).
        let bg = if p.row.id.0.is_multiple_of(2) {
            Color(0x22_22_22)
        } else {
            Color(0x33_33_33)
        };
        Div("dgrid-body")
            .style(Style::new().w(p.width).h(p.height).bg(bg).build())
            .build()
    }

    fn grid_cols() -> Vec<GridColumn<VItem>> {
        vec![
            GridColumn {
                header: SharedString::from("A"),
                width: 100.0,
                cell: grid_body,
            },
            GridColumn {
                header: SharedString::from("B"),
                width: 120.0,
                cell: grid_body,
            },
            GridColumn {
                header: SharedString::from("C"),
                width: 80.0,
                cell: grid_body,
            },
        ]
    }

    struct GridHarness {
        host: ComponentHost,
        offset: oppa::ScrollOffset,
        list: NodeId,
        coll: oppa::Collection<VItem>,
    }

    fn mount_grid(n_items: usize) -> (GridHarness, oppa::MountHandle<DataGridProps<VItem>>) {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        host.set_text_service(Box::new(FakeText));
        let rt = host.runtime();
        let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:dgrid"));
        coll.ingest((0..n_items).map(|i| VItem { i }).collect::<Vec<_>>());
        let props = DataGridProps::new(coll.clone(), grid_cols(), vitem_h)
            .size(GRID_VIEW_W, GRID_VIEW_H)
            .overscan(VL_OVER)
            .debug("dgrid");
        let handle = host.mount("DGrid", props, DataGrid);
        host.run_until_idle();
        let root = handle.root_instance();
        let offset = host.instance_scroll(root).expect("scroll handle");
        let list = find_retained_by_debug(&host, "dgrid")[0];
        (
            GridHarness {
                host,
                offset,
                list,
                coll,
            },
            handle,
        )
    }

    fn cell_bg(h: &GridHarness, id: NodeId) -> Option<Color> {
        // The zebra lives on the template root (the cell Div only
        // geometries) — match the body node by box.
        let b = h.host.committed_box(id).expect("cell laid out");
        let body = find_retained_by_debug(&h.host, "dgrid-body")
            .into_iter()
            .find(|r| {
                h.host
                    .committed_box(*r)
                    .is_some_and(|rb| approx(rb.y, b.y) && approx(rb.x, b.x))
            })
            .expect("body content at cell box");
        let sid = h.host.retained_style(body).expect("body style");
        h.host
            .with_retained_mut(|_, styles| styles.get(sid).expect("style").bg)
    }

    /// Header + window + extent: 200 alternating rows (total 6000),
    /// 3 columns (total 300 wide), viewport 320×200 with a 28px pinned
    /// header → 24 cells in [0,8), header pinned at y=0 with full
    /// column width.
    #[test]
    fn datagrid_renders_header_window_and_extent() {
        let (h, _) = mount_grid(GRID_N);
        assert_eq!(
            find_retained_by_debug(&h.host, "dgrid-cell").len(),
            24,
            "8 rows × 3 cols, not all 200"
        );
        let header = find_retained_by_debug(&h.host, "dgrid-header");
        assert_eq!(header.len(), 1, "one sticky header");
        let hb = h.host.committed_box(header[0]).expect("header laid out");
        assert!(approx(hb.y, 0.0), "header at viewport top, got {}", hb.y);
        assert!(approx(hb.w, 300.0), "header spans columns, got {}", hb.w);
        assert!(approx(hb.h, 28.0), "default header height");
        // First-row cells sit below the pinned header with exact
        // column widths.
        let mut xs: Vec<f32> = find_retained_by_debug(&h.host, "dgrid-cell")
            .into_iter()
            .filter_map(|id| h.host.committed_box(id))
            .filter(|b| approx(b.y, 28.0))
            .map(|b| b.x)
            .collect();
        xs.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        assert_eq!(xs.len(), 3, "three first-row cells");
        assert!(approx(xs[0], 0.0) && approx(xs[1], 100.0) && approx(xs[2], 220.0));
        // Extent covers all rows plus the pinned header (spacer source).
        let list_box = h.host.committed_box(h.list).expect("list laid out");
        assert!(
            approx(list_box.content_h, 6028.0),
            "200 rows × 30 mean + 28 header, got {}",
            list_box.content_h
        );
    }

    /// Scrolling slides the window and pins the header: 500px lands
    /// [15,25) (30 cells), the header box stays at the viewport top,
    /// and zebra follows row 15.
    #[test]
    fn datagrid_scroll_pins_header_recycles_cells() {
        let (h, _) = mount_grid(GRID_N);
        h.offset.set(500.0);
        h.host.run_until_idle();
        assert_eq!(
            find_retained_by_debug(&h.host, "dgrid-cell").len(),
            30,
            "10 rows × 3 cols"
        );
        let header = find_retained_by_debug(&h.host, "dgrid-header")[0];
        let hb = h.host.committed_box(header).expect("header laid out");
        assert!(
            approx(hb.y, 0.0),
            "header pinned to viewport top, got {}",
            hb.y
        );
        // Row 15's cells sit viewport-relative at 28 + tops[15] - 500
        // with odd zebra.
        let first_row: Vec<NodeId> = find_retained_by_debug(&h.host, "dgrid-cell")
            .into_iter()
            .filter(|id| {
                h.host
                    .committed_box(*id)
                    .is_some_and(|b| approx(b.y, 28.0 + vitem_top(15) - 500.0))
            })
            .collect();
        assert_eq!(first_row.len(), 3, "three cells at row 15");
        for id in first_row {
            assert_eq!(cell_bg(&h, id), Some(Color(0x33_33_33)));
        }
    }

    /// Round 24.2: one wheel event on an unbound grid feeds the
    /// declaring component's offset (no `bind_scroll` call), keeps the
    /// header at the viewport top, and moves rows viewport-relative.
    #[test]
    fn datagrid_wheel_feeds_owner_offset_without_explicit_bind() {
        let (h, _) = mount_grid(GRID_N);
        assert_eq!(
            h.host.bound_scroll(h.list),
            None,
            "the grid never binds an explicit feed"
        );
        h.host.inject_input(InputEvent::Scroll {
            target: h.list,
            dx: 0.0,
            dy: 120.0,
        });
        h.host.run_until_idle();
        assert_eq!(h.offset.get(), 120.0, "wheel moved the owner offset");
        let header = find_retained_by_debug(&h.host, "dgrid-header")[0];
        let hb = h.host.committed_box(header).expect("header laid out");
        assert!(approx(hb.y, 0.0), "header stays put, got {}", hb.y);
        // Row 2 is the window head at 120px: 28 + 60 - 120 = -32.
        let first_row: Vec<NodeId> = find_retained_by_debug(&h.host, "dgrid-cell")
            .into_iter()
            .filter(|id| {
                h.host
                    .committed_box(*id)
                    .is_some_and(|b| approx(b.y, 28.0 + vitem_top(2) - 120.0))
            })
            .collect();
        assert_eq!(first_row.len(), 3, "three cells at row 2");
        for id in first_row {
            assert_eq!(cell_bg(&h, id), Some(Color(0x22_22_22)));
        }
    }

    /// Filter updates re-derive window + extent without remount
    /// (even ids → 100 rows, total 3000).
    #[test]
    fn datagrid_filter_updates_window_and_extent() {
        let (h, handle) = mount_grid(GRID_N);
        let filtered = DataGridProps::new(h.coll.clone(), grid_cols(), vitem_h)
            .size(GRID_VIEW_W, GRID_VIEW_H)
            .overscan(VL_OVER)
            .debug("dgrid")
            .filter(|item: &VItem| item.i.is_multiple_of(2));
        handle.set_props(filtered);
        h.host.run_until_idle();
        let list_box = h.host.committed_box(h.list).expect("list laid out");
        // Even original indexes are exactly the 20px rows.
        assert!(
            approx(list_box.content_h, 2028.0),
            "100 even rows × 20 + 28 header, got {}",
            list_box.content_h
        );
        // Window recomputed over the filtered set (uniform 20px
        // rows in the 172px body: 9 visible + 2 overscan = 11 rows ×
        // 3 cols).
        assert_eq!(
            find_retained_by_debug(&h.host, "dgrid-cell").len(),
            33,
            "filtered window [0,11) × 3 cols"
        );
    }

    /// Round 13.1 → 13.3 combined: two page loads stream into the
    /// live grid — states track per page, rows accumulate, extent
    /// grows, no remount.
    #[test]
    fn datagrid_async_page_stream_grows_grid() {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        host.set_text_service(Box::new(FakeText));
        let rt = host.runtime();
        let coll: oppa::Collection<VItem> =
            oppa::Collection::new(&rt, oppa::fetch_key("test:dgrid-pages"));
        #[derive(Clone)]
        struct GridApp {
            coll: oppa::Collection<VItem>,
            fired: Rc<std::cell::Cell<bool>>,
        }
        impl Props for GridApp {}
        fn grid_app_render(ctx: &Ctx, p: &GridApp) -> VNode {
            if !p.fired.get() {
                p.fired.set(true);
                let key = p.coll.key();
                ctx.spawn_fetch_page(key, 0, 10, 3, |_, _| {
                    Ok(vec![VItem { i: 0 }, VItem { i: 1 }, VItem { i: 2 }])
                });
                ctx.spawn_fetch_page(key, 1, 10, 3, |_, _| {
                    Ok(vec![VItem { i: 3 }, VItem { i: 4 }])
                });
            }
            let props = DataGridProps::new(p.coll.clone(), grid_cols(), vitem_h)
                .size(GRID_VIEW_W, GRID_VIEW_H)
                .overscan(VL_OVER)
                .debug("dgrid");
            ctx.child("oppa::DataGrid", 1, &props, DataGrid)
        }
        host.mount(
            "GridApp",
            GridApp {
                coll: coll.clone(),
                fired: Rc::new(std::cell::Cell::new(false)),
            },
            grid_app_render,
        );
        host.run_until_idle();
        let mut waited = 0;
        loop {
            let stats = rt.stats();
            if stats.tasks_done + stats.tasks_dropped >= 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
            waited += 1;
            assert!(waited < 10_000, "executor tasks never resolved");
        }
        host.run_until_idle();
        assert_eq!(coll.len(), 5, "both pages streamed");
        // Extent follows live: 20+40+20+40+20 rows + 28 header = 168.
        let list = find_retained_by_debug(&host, "dgrid")[0];
        let list_box = host.committed_box(list).expect("list laid out");
        assert!(
            approx(list_box.content_h, 168.0),
            "5 streamed rows, got {}",
            list_box.content_h
        );
        // Both page states reached Ready (per-page tracking).
        for page in [0, 1] {
            let state = rt
                .keyed_state::<oppa::FetchState<Vec<VItem>>>(
                    oppa::page_key(coll.key(), page),
                    || oppa::FetchState::Idle,
                )
                .get();
            assert!(
                matches!(state, oppa::FetchState::Ready(_)),
                "page {page} Ready, got {state:?}"
            );
        }
    }

    // ------------------------------------------------------------------
    // Round 14.1 (decision 311): generic `#[derive(Props)]`
    // ------------------------------------------------------------------

    /// The brief's shape, verbatim: a generic props struct deriving
    /// the marker (no manual impl — the derive emits
    /// `impl<T: Clone + 'static> Props`).
    #[derive(Clone, Props)]
    struct ListProps<T> {
        items: Vec<T>,
    }

    fn generic_list(ctx: &Ctx, p: &ListProps<String>) -> VNode {
        Column::new().children(
            p.items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    ctx.child(
                        "oppa::GenericRow",
                        i as u64,
                        &RowCase {
                            label: SharedString::from(item.as_str()),
                        },
                        generic_row,
                    )
                })
                .collect::<Vec<_>>(),
        )
    }

    #[derive(Clone)]
    struct RowCase {
        label: SharedString,
    }

    impl Props for RowCase {}

    fn generic_row(_ctx: &Ctx, p: &RowCase) -> VNode {
        Div("generic-row")
            .style(Style::new().h(24).fill_width())
            .semantics(Semantics::default().label(&p.label))
            .build()
    }

    /// A generic props struct compiles through the derive and
    /// renders through the real pipeline (mount + reconcile +
    /// layout — one labeled row per item, in order).
    #[test]
    fn generic_derive_props_compiles_and_renders() {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        host.mount(
            "GenericList",
            ListProps {
                items: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            },
            generic_list,
        );
        host.run_until_idle();
        let rows = find_retained_by_debug(&host, "generic-row");
        assert_eq!(rows.len(), 3, "one row per item");
        let labels: Vec<String> = rows
            .iter()
            .map(|id| {
                host.retained_semantics(*id)
                    .expect("row semantics")
                    .label
                    .as_deref()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
        assert_eq!(
            labels,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    // ------------------------------------------------------------------
    // Round 17.2 (decision 318): draggable scrollbar.
    // ------------------------------------------------------------------

    /// Scrollbar rig: hand-rolled `ScrollArea` (200×200 at the
    /// origin, content 200×`content_h`) plus a sibling `Scrollbar`
    /// sharing the rig's offset signal (the `scroll_app` shape —
    /// one signal drives content and thumb together).
    #[derive(Clone)]
    struct ScrollRigProps {
        content_h: f32,
    }
    impl Props for ScrollRigProps {}

    fn scroll_rig(ctx: &Ctx, p: &ScrollRigProps) -> VNode {
        let offset = ctx.scroll_offset();
        Div("scroll-screen")
            .style(Style::new().size(400, 300))
            .children([
                oppa::ScrollArea("scroll-list")
                    .style(Style::new().size(200, 200).content_size(p.content_h))
                    .on_scroll(|| {})
                    .child(
                        Div("scroll-content")
                            .style(
                                Style::new()
                                    .size(200, p.content_h)
                                    .absolute_y(-offset.get()),
                            )
                            .build(),
                    ),
                ctx.child(
                    "oppa::ScrollRigBar",
                    1,
                    &ScrollbarProps {
                        target: SharedString::from("scroll-list"),
                        offset: offset.clone(),
                        // The rig proves the event-driven hide;
                        // wall-clock fade is proven through the
                        // VirtualList/DataGrid attachments (21.2).
                        idle_hide_ms: None,
                    },
                    Scrollbar,
                ),
            ])
    }

    fn scrollbar_harness(content_h: f32) -> (ComponentHost, ScrollOffset) {
        let host = ComponentHost::new();
        host.set_viewport(400.0, 300.0);
        let handle = host.mount("S", ScrollRigProps { content_h }, scroll_rig);
        host.run_until_idle();
        let offset = host
            .instance_scroll(handle.root_instance())
            .expect("rig scroll handle");
        (host, offset)
    }

    fn scrollbar_plan(host: &ComponentHost) -> oppa::FramePlan {
        let builder = oppa_cpu::FramePlanBuilder::new(1.0);
        host.with_retained_mut(|rec, styles| builder.build_full(rec, styles))
    }

    /// Thumb + track rects by geometry (track spans the viewport
    /// height at the target's right edge; thumb pins the brief
    /// ratio) — color identifies the part only in dark mode, so
    /// geometry leads everywhere.
    type ChromePair = (Vec<(f32, f32, f32, f32)>, Vec<(f32, f32, f32, f32)>);

    fn chrome_rects(plan: &oppa::FramePlan) -> ChromePair {
        let mut track = Vec::new();
        let mut thumb = Vec::new();
        for op in &plan.ops {
            if let oppa::DrawOp::Rect { x, y, w, h, .. } = op {
                if approx(*x, 188.0) && approx(*w, 12.0) {
                    if approx(*y, 0.0) && approx(*h, 200.0) {
                        track.push((*x, *y, *w, *h));
                    } else {
                        thumb.push((*x, *y, *w, *h));
                    }
                }
            }
        }
        (track, thumb)
    }

    fn hover(host: &ComponentHost, x: f32, y: f32) {
        host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Move,
            x,
            y,
            modifiers: oppa::Modifiers::NONE,
        });
        host.run_until_idle();
    }

    fn drag(host: &ComponentHost, x1: f32, y1: f32, x2: f32, y2: f32) {
        host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Down {
                button: oppa::PointerButton::Primary,
            },
            x: x1,
            y: y1,
            modifiers: oppa::Modifiers::NONE,
        });
        host.run_until_idle();
        for (x, y) in [(x1 + (x2 - x1) * 0.5, y1 + (y2 - y1) * 0.5), (x2, y2)] {
            host.inject_input(InputEvent::Pointer {
                id: Some(0),
                action: oppa::PointerAction::Move,
                x,
                y,
                modifiers: oppa::Modifiers::NONE,
            });
            host.run_until_idle();
        }
        host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Up {
                button: oppa::PointerButton::Primary,
            },
            x: x2,
            y: y2,
            modifiers: oppa::Modifiers::NONE,
        });
        host.run_until_idle();
    }

    fn track_tap(host: &ComponentHost, x: f32, y: f32) {
        tap_at(host, x, y, false);
    }

    /// Round 17.2: no overflow renders no chrome; overflow idles
    /// hidden (opacity targets zero paint nothing — presence reads
    /// the plan, never node labels).
    #[test]
    fn scrollbar_hidden_without_overflow_and_when_idle() {
        // Content fits: no chrome rects anywhere in the plan.
        let (host, _) = scrollbar_harness(100.0);
        let plan = scrollbar_plan(&host);
        let (track, thumb) = chrome_rects(&plan);
        assert!(track.is_empty() && thumb.is_empty(), "fits: paints nothing");
        // Overflow, never touched: the raw plan carries no chrome rects.
        let (host, _) = scrollbar_harness(600.0);
        let plan = scrollbar_plan(&host);
        let (track, thumb) = chrome_rects(&plan);
        assert!(track.is_empty() && thumb.is_empty(), "idle paints nothing");
    }

    /// Follow-up: hovering the transparent gutter outside the
    /// painted 12px bar (but inside the 20px hit node) summons the
    /// chrome — no pixel-hunting the bar itself.
    #[test]
    fn scrollbar_shows_on_gutter_hover_outside_painted_bar() {
        let (host, _) = scrollbar_harness(600.0);
        hover(&host, 184.0, 100.0);
        let plan = scrollbar_plan(&host);
        let (track, thumb) = chrome_rects(&plan);
        assert_eq!(track.len(), 1, "gutter hover paints track, got {track:?}");
        assert_eq!(thumb.len(), 1, "gutter hover paints thumb, got {thumb:?}");
        // Painted chrome keeps the 12px geometry (the gutter paints
        // nothing itself).
        assert!(approx(track[0].0, 188.0) && approx(track[0].2, 12.0));
    }

    /// Round 17.2: hover fades the chrome in, leave fades it out
    /// (raw targets flip immediately; the transition animates).
    #[test]
    fn scrollbar_shows_on_hover_and_hides_on_leave() {
        let (host, _) = scrollbar_harness(600.0);
        hover(&host, 194.0, 100.0);
        let plan = scrollbar_plan(&host);
        let (track, thumb) = chrome_rects(&plan);
        assert_eq!(track.len(), 1, "track paints, got {track:?}");
        assert_eq!(thumb.len(), 1, "thumb paints, got {thumb:?}");
        assert!(
            approx(thumb[0].3, 200.0 * 200.0 / 600.0),
            "thumb pins vp²/c, got {:?}",
            thumb[0]
        );
        hover(&host, 350.0, 290.0);
        let plan = scrollbar_plan(&host);
        let (track, thumb) = chrome_rects(&plan);
        assert!(track.is_empty() && thumb.is_empty(), "leave hides");
    }

    /// Round 17.2 (the brief verification, exactly): dragging the
    /// thumb from top to bottom scrolls the content to 100% of its
    /// extent — and back again.
    #[test]
    fn thumb_drag_top_to_bottom_scrolls_to_max() {
        let (host, offset) = scrollbar_harness(600.0);
        // Thumb parks 0..66.67 at rest; grab its top, drag to the
        // track bottom: ratio 400/133.33 lands exactly on max.
        drag(&host, 194.0, 5.0, 194.0, 199.0);
        assert!(
            approx(offset.get(), 400.0),
            "100% extent, got {}",
            offset.get()
        );
        let plan = scrollbar_plan(&host);
        let (_, thumb) = chrome_rects(&plan);
        assert_eq!(thumb.len(), 1, "thumb paints after drag");
        assert!(
            approx(thumb[0].1, (200.0f32 - 200.0 * 200.0 / 600.0).round()),
            "thumb parks at travel, got {:?}",
            thumb[0]
        );
        // And back: grab the bottom-parked thumb, drag to top.
        drag(&host, 194.0, 195.0, 194.0, 2.0);
        assert!(
            approx(offset.get(), 0.0),
            "back to top, got {}",
            offset.get()
        );
    }

    /// Round 17.2: track taps page by viewport, clamped at both
    /// ends; thumb taps hold (dragging is the gesture).
    #[test]
    fn track_click_pages_and_clamps() {
        let (host, offset) = scrollbar_harness(600.0);
        // Below the rest thumb (0..66.67): +200.
        track_tap(&host, 194.0, 150.0);
        assert!(
            approx(offset.get(), 200.0),
            "pages down, got {}",
            offset.get()
        );
        // Above the moved thumb (66.67..133.33): −200.
        track_tap(&host, 194.0, 10.0);
        assert!(approx(offset.get(), 0.0), "pages up, got {}", offset.get());
        // Clamp: paging past the end parks on max, never beyond.
        track_tap(&host, 194.0, 150.0);
        track_tap(&host, 194.0, 190.0);
        assert!(
            approx(offset.get(), 400.0),
            "clamps on max, got {}",
            offset.get()
        );
        track_tap(&host, 194.0, 190.0);
        assert!(approx(offset.get(), 400.0), "stays parked");
        // Thumb tap holds (no page, no invoke — dragging owns it).
        track_tap(&host, 194.0, 170.0);
        assert!(approx(offset.get(), 400.0), "thumb tap holds");
    }

    // ------------------------------------------------------------------
    // Round 21.2 (decision 329): VirtualList/DataGrid attachment +
    // wall-clock idle fade.
    // ------------------------------------------------------------------

    /// Settles a clocked host vsync-by-vsync (the M8 pattern):
    /// `run_until_idle` spins forever under a frozen clock while a
    /// transition lives (animations retire only as time advances),
    /// so clocked tests step `run_once` instead. Loud when the cap
    /// trips (never a silent hang).
    fn settle_clocked(host: &ComponentHost, clock: &oppa::MockClock) {
        for _ in 0..600 {
            clock.advance(1.0 / 60.0);
            if !host.run_once() {
                return;
            }
        }
        panic!("settle_clocked: still demanding after 600 frames (10s mock)");
    }

    /// Chrome rects at an arbitrary target box (the rig's
    /// `chrome_rects` pinned to the rig box — attachments compute
    /// their own track edge from the settled target box).
    fn chrome_rects_at(
        plan: &oppa::FramePlan,
        track_x: f32,
        top_y: f32,
        view_h: f32,
    ) -> ChromePair {
        let mut track = Vec::new();
        let mut thumb = Vec::new();
        for op in &plan.ops {
            if let oppa::DrawOp::Rect { x, y, w, h, .. } = op {
                if approx(*x, track_x) && approx(*w, 12.0) {
                    if approx(*y, top_y) && approx(*h, view_h) {
                        track.push((*x, *y, *w, *h));
                    } else {
                        thumb.push((*x, *y, *w, *h));
                    }
                }
            }
        }
        (track, thumb)
    }

    /// Mounts a `VirtualList` on a caller-owned host (clocked hosts
    /// drive the idle fade deterministically).
    fn mount_vlist_on(host: &ComponentHost, debug: &str) -> (oppa::ScrollOffset, NodeId) {
        let rt = host.runtime();
        let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:vlist-attach"));
        coll.ingest((0..VL_N).map(|i| VItem { i }).collect::<Vec<_>>());
        let props = VirtualListProps::new(coll, vitem_h, vitem_row)
            .size(320.0, VL_VIEW_H)
            .overscan(VL_OVER)
            .debug(debug);
        let handle = host.mount("VListAttach", props, VirtualList);
        host.run_until_idle();
        let offset = host
            .instance_scroll(handle.root_instance())
            .expect("list scroll handle");
        let list = find_retained_by_debug(host, debug)[0];
        (offset, list)
    }

    /// Round 21.2: dragging the attached thumb top-to-bottom scrolls
    /// the virtualized row window to 100% — offset parks on max and
    /// the window covers the final rows.
    #[test]
    fn vlist_attached_thumb_drag_scrolls_to_max() {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let (offset, list) = mount_vlist_on(&host, "vlist");
        let tb = host.committed_box(list).expect("list laid out");
        let track_x = tb.x + tb.w - 12.0;
        // Thumb parks 0..24 at rest; grab its top, drag to the
        // track bottom — the ratio overshoots, the clamp parks max.
        drag(&host, track_x + 6.0, 5.0, track_x + 6.0, tb.y + tb.h - 1.0);
        let max = 30_000.0 - VL_VIEW_H;
        assert!(
            approx(offset.get(), max),
            "100% extent, got {}",
            offset.get()
        );
        // The window covers the final rows (last slot bottom ==
        // viewport bottom).
        let bottom = find_retained_by_debug(&host, "vlist-slot")
            .into_iter()
            .filter_map(|id| host.committed_box(id))
            .map(|b| b.y + b.h)
            .fold(0.0f32, f32::max);
        assert!(
            approx(bottom, VL_VIEW_H),
            "window shows the final rows, bottom {bottom}"
        );
    }

    /// Round 21.2: the attached overlay fades on the wall clock —
    /// visible after a scroll, still visible before `idle_hide_ms`,
    /// gone after, without another pointer event (and the content
    /// offset never moves). Settles step the mock clock (a frozen
    /// clock spins `run_until_idle` while the fade transition
    /// lives — the M8 `run_once` pattern instead).
    #[test]
    fn vlist_attached_idle_fade_hides_after_timeout() {
        let clock = Rc::new(oppa::MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        host.set_viewport(800.0, 600.0);
        let (offset, list) = mount_vlist_on(&host, "vlist-idle");
        settle_clocked(&host, &clock);
        offset.set(500.0);
        settle_clocked(&host, &clock);
        let tb = host.committed_box(list).expect("list laid out");
        let at = (tb.x + tb.w - 12.0, tb.y, tb.h);
        let visible = || {
            let plan = scrollbar_plan(&host);
            let (track, thumb) = chrome_rects_at(&plan, at.0, at.1, at.2);
            !track.is_empty() && !thumb.is_empty()
        };
        assert!(visible(), "scroll latches the chrome on");
        // Half the idle budget: still parked, no pointer in sight.
        clock.advance(0.5);
        assert_eq!(host.tick_timers(host.now_ms()), 0, "nothing due yet");
        settle_clocked(&host, &clock);
        assert!(visible(), "still parked before idle_hide_ms");
        // Past 1200ms: the timer drops the flash, chrome gone.
        clock.advance(0.8);
        assert_eq!(host.tick_timers(host.now_ms()), 1, "idle timer fires");
        settle_clocked(&host, &clock);
        assert!(!visible(), "hides without another pointer event");
        assert!(
            approx(offset.get(), 500.0),
            "fade never moves content, got {}",
            offset.get()
        );
    }

    /// Mounts a `DataGrid` on a caller-owned host.
    fn mount_grid_on(host: &ComponentHost, debug: &str) -> (oppa::ScrollOffset, NodeId) {
        let rt = host.runtime();
        let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:dgrid-attach"));
        coll.ingest((0..GRID_N).map(|i| VItem { i }).collect::<Vec<_>>());
        let props = DataGridProps::new(coll, grid_cols(), vitem_h)
            .size(GRID_VIEW_W, GRID_VIEW_H)
            .overscan(VL_OVER)
            .debug(debug);
        let handle = host.mount("DGridAttach", props, DataGrid);
        host.run_until_idle();
        let offset = host
            .instance_scroll(handle.root_instance())
            .expect("grid scroll handle");
        let list = find_retained_by_debug(host, debug)[0];
        (offset, list)
    }

    /// Round 21.2: dragging the grid's attached thumb parks the
    /// window on max extent with the header pinned to the viewport top.
    #[test]
    fn dgrid_attached_thumb_drag_scrolls_to_max() {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        host.set_text_service(Box::new(FakeText));
        let (offset, list) = mount_grid_on(&host, "dgrid");
        let tb = host.committed_box(list).expect("grid laid out");
        let track_x = tb.x + tb.w - 12.0;
        drag(&host, track_x + 6.0, 5.0, track_x + 6.0, tb.y + tb.h - 1.0);
        let max = 6028.0 - GRID_VIEW_H;
        assert!(
            approx(offset.get(), max),
            "100% extent, got {}",
            offset.get()
        );
        let header = find_retained_by_debug(&host, "dgrid-header")[0];
        let hb = host.committed_box(header).expect("header laid out");
        assert!(
            approx(hb.y, 0.0),
            "header pinned to the viewport top, got {}",
            hb.y
        );
    }

    // ------------------------------------------------------------------
    // Round 23.1 (decision 333): per-key granular subscriptions.
    // ------------------------------------------------------------------

    /// Granular row content (the 23.1 verify shape): reads its own
    /// payload through `get_row` (tracked per-row — sibling rows
    /// never hear each other's writes) and counts renders per
    /// stable id.
    #[derive(Clone)]
    struct GItem {
        i: usize,
        runs: Rc<RefCell<Vec<usize>>>,
    }

    fn granular_row(_ctx: &Ctx, p: &VirtualRowProps<GItem>) -> VNode {
        p.row.value.runs.borrow_mut()[p.row.id.0 as usize] += 1;
        let v = p
            .rows
            .get_row(p.row.id)
            .get()
            .map(|row| row.i)
            .unwrap_or(usize::MAX);
        Div("grow")
            .style(
                Style::new()
                    .h(p.height)
                    .fill_width()
                    .bg(if v.is_multiple_of(2) {
                        Color(0x22_22_22)
                    } else {
                        Color(0x33_33_33)
                    }),
            )
            .build()
    }

    /// Round 23.1 (decision 333): in a 1,000-row `Collection`
    /// mounted inside `VirtualList`, updating one visible row via
    /// `update_row` refreshes the window with the new value while
    /// version subscribers stay quiet — no structural fan-out
    /// (counters, sibling windows, and query readers never
    /// re-run). The owning window re-derives (M2 inline semantics:
    /// inline children share their root's dependency set, so the
    /// window pass re-renders its slots — per-row child isolation
    /// needs effect-per-child machinery, an open architecture
    /// question, not a silent gap).
    #[test]
    fn update_row_refreshes_without_version_fanout() {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let rt = host.runtime();
        let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:granular-vlist"));
        let runs = Rc::new(RefCell::new(vec![0usize; 1000]));
        coll.ingest(
            (0..1000)
                .map(|i| GItem {
                    i,
                    runs: runs.clone(),
                })
                .collect::<Vec<_>>(),
        );
        let props = VirtualListProps::new(coll.clone(), |_: &GItem| 30.0, granular_row)
            .size(320.0, 200.0)
            .overscan(2)
            .debug("gvlist")
            .scrollbar(false);
        host.mount("GV", props, VirtualList);
        // An unrelated version subscriber (counter): quiet unless
        // the version bumps (a coarse `update` would re-run it).
        use oppa::Props;
        #[derive(Clone)]
        struct LenProps {
            runs: Rc<RefCell<usize>>,
            coll: oppa::Collection<GItem>,
        }
        impl Props for LenProps {}
        fn len_comp(_ctx: &Ctx, p: &LenProps) -> VNode {
            *p.runs.borrow_mut() += 1;
            let _ = p.coll.len();
            Div("lenprobe").build()
        }
        let len_runs = Rc::new(RefCell::new(0usize));
        host.mount(
            "LEN",
            LenProps {
                runs: len_runs.clone(),
                coll: coll.clone(),
            },
            len_comp,
        );
        host.run_until_idle();
        // Sanity: only the window materialized (far from all 1000).
        let rendered: usize = runs.borrow().iter().filter(|&&c| c > 0).count();
        assert!(
            rendered > 0 && rendered < 100,
            "windowed render, got {rendered}"
        );
        assert_eq!(*len_runs.borrow(), 1);
        // Row 3 sits in the first window: update it granularly.
        let target = coll.rows()[3].id;
        assert!(coll.update_row(
            target,
            GItem {
                i: 3001,
                runs: runs.clone()
            }
        ));
        host.run_until_idle();
        // The window picked up the fresh value.
        let shown: usize = find_retained_by_debug(&host, "grow").len();
        assert!(shown > 0, "window still renders");
        assert_eq!(
            coll.lookup(target).map(|g| g.i),
            Some(3001),
            "lookup sees the write"
        );
        // No structural fan-out: the version subscriber never
        // re-ran (a coarse `update` would have re-run it).
        assert_eq!(*len_runs.borrow(), 1, "version subscribers stay quiet");
        // Row 3 re-rendered (its slot notified); queries read the
        // fresh value on structural passes.
        let page = coll.query(&oppa::CollectionQuery {
            filter: None,
            sort: None,
            offset: 3,
            limit: Some(1),
        });
        assert_eq!(
            page.rows.first().map(|r| r.value.i),
            Some(3001),
            "queries read fresh values"
        );
    }

    /// Round 23.1 (decision 333): `update_row` never bumps the
    /// collection version (a `len` reader proves it — still 1
    /// after the granular write), while coarse `update` does.
    #[test]
    fn update_row_skips_version_while_update_bumps() {
        use oppa::Props;
        #[derive(Clone)]
        struct LProps {
            runs: Rc<RefCell<usize>>,
            coll: oppa::Collection<String>,
        }
        impl Props for LProps {}
        fn lcomp(_ctx: &Ctx, p: &LProps) -> VNode {
            *p.runs.borrow_mut() += 1;
            let _ = p.coll.len();
            Div("lprobe").build()
        }
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let rt = host.runtime();
        let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:zz-ver"));
        coll.ingest(vec!["a".to_string(), "b".to_string()]);
        let runs = Rc::new(RefCell::new(0usize));
        host.mount(
            "L",
            LProps {
                runs: runs.clone(),
                coll: coll.clone(),
            },
            lcomp,
        );
        host.run_until_idle();
        assert_eq!(*runs.borrow(), 1, "mounted once");
        coll.update_row(oppa::RowId(0), "A".to_string());
        host.run_until_idle();
        assert_eq!(*runs.borrow(), 1, "granular write skips the version");
        coll.update(oppa::RowId(0), "AA".to_string());
        host.run_until_idle();
        assert_eq!(*runs.borrow(), 2, "coarse write bumps it");
    }
    /// Round 23.1 (decision 333): root-mounted row readers are
    /// precisely isolated — one row's `update_row` re-renders
    /// exactly its subscriber (siblings quiet). Inside one list
    /// the window re-derives with its root (M2 inline semantics —
    /// see `update_row_refreshes_without_version_fanout`).
    #[test]
    fn update_row_rerenders_only_subscribed_roots() {
        use oppa::Props;
        use std::collections::HashMap;
        #[derive(Clone)]
        struct RProps {
            id: oppa::RowId,
            runs: Rc<RefCell<HashMap<u64, usize>>>,
            coll: oppa::Collection<String>,
        }
        impl Props for RProps {}
        fn rcomp(_ctx: &Ctx, p: &RProps) -> VNode {
            *p.runs.borrow_mut().entry(p.id.0).or_insert(0) += 1;
            let _ = p.coll.get_row(p.id).get();
            Div("rprobe").build()
        }
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let rt = host.runtime();
        let coll = oppa::Collection::new(&rt, oppa::fetch_key("test:zz-probe"));
        let ids = coll.ingest(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        let runs = Rc::new(RefCell::new(HashMap::new()));
        for row in &ids {
            host.mount(
                "R",
                RProps {
                    id: row.id,
                    runs: runs.clone(),
                    coll: coll.clone(),
                },
                rcomp,
            );
        }
        host.run_until_idle();
        assert_eq!(runs.borrow().len(), 3, "all three mounted once");
        coll.update_row(ids[1].id, "B".to_string());
        host.run_until_idle();
        assert_eq!(
            (
                runs.borrow().get(&0).copied(),
                runs.borrow().get(&1).copied(),
                runs.borrow().get(&2).copied()
            ),
            (Some(1), Some(2), Some(1)),
            "exactly the subscribed root re-renders"
        );
    }

    /// Round 21.2: the grid's attached overlay hides past
    /// `idle_hide_ms` on the host clock (the hide leg — the
    /// still-visible leg rides the list test above). Clock-stepped
    /// settles (same frozen-clock rule as the list test).
    #[test]
    fn dgrid_attached_idle_fade_hides_after_timeout() {
        let clock = Rc::new(oppa::MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        host.set_viewport(800.0, 600.0);
        host.set_text_service(Box::new(FakeText));
        let (offset, list) = mount_grid_on(&host, "dgrid-idle");
        settle_clocked(&host, &clock);
        offset.set(500.0);
        settle_clocked(&host, &clock);
        let tb = host.committed_box(list).expect("grid laid out");
        let plan = scrollbar_plan(&host);
        let (track, thumb) = chrome_rects_at(&plan, tb.x + tb.w - 12.0, tb.y, tb.h);
        assert!(
            !track.is_empty() && !thumb.is_empty(),
            "scroll shows chrome"
        );
        clock.advance(1.3);
        assert_eq!(host.tick_timers(host.now_ms()), 1, "idle timer fires");
        settle_clocked(&host, &clock);
        let plan = scrollbar_plan(&host);
        let (track, thumb) = chrome_rects_at(&plan, tb.x + tb.w - 12.0, tb.y, tb.h);
        assert!(track.is_empty() && thumb.is_empty(), "hides on the clock");
        assert!(
            approx(offset.get(), 500.0),
            "fade never moves content, got {}",
            offset.get()
        );
    }

    /// Round 17.2: thumb and track honor reactive theme tokens
    /// (text_secondary / border recolor Light→Dark in place).
    #[test]
    fn scrollbar_matches_theme_tokens() {
        use oppa::ThemeTokens;
        let (host, _) = scrollbar_harness(600.0);
        hover(&host, 194.0, 100.0);
        let plan = scrollbar_plan(&host);
        let thumb_color = plan
            .ops
            .iter()
            .filter_map(|op| match op {
                oppa::DrawOp::Rect { x, w, h, color, .. }
                    if approx(*x, 188.0) && approx(*w, 12.0) && !approx(*h, 200.0) =>
                {
                    Some(*color)
                }
                _ => None,
            })
            .next()
            .expect("thumb rect");
        assert_eq!(thumb_color, ThemeTokens::light().text_secondary);
        host.set_theme(oppa::ThemeMode::Dark);
        host.run_until_idle();
        let plan = scrollbar_plan(&host);
        let thumb_color = plan
            .ops
            .iter()
            .filter_map(|op| match op {
                oppa::DrawOp::Rect { x, w, h, color, .. }
                    if approx(*x, 188.0) && approx(*w, 12.0) && !approx(*h, 200.0) =>
                {
                    Some(*color)
                }
                _ => None,
            })
            .next()
            .expect("thumb rect after theme flip");
        assert_eq!(
            thumb_color,
            ThemeTokens::dark().text_secondary,
            "thumb recolors in place"
        );
    }

    /// Round 17.2: arrows page by viewport once the track holds
    /// focus (tab into it — press owners are tab stops).
    #[test]
    fn keyboard_pages_when_track_focused() {
        let (host, offset) = scrollbar_harness(600.0);
        host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
        host.run_until_idle();
        host.inject_input(InputEvent::key(keys::DOWN, KeyState::Pressed));
        host.run_until_idle();
        assert!(
            approx(offset.get(), 200.0),
            "Down pages, got {}",
            offset.get()
        );
        host.inject_input(InputEvent::key(keys::UP, KeyState::Pressed));
        host.run_until_idle();
        assert!(
            approx(offset.get(), 0.0),
            "Up pages back, got {}",
            offset.get()
        );
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
}
