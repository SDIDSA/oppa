use crate::common::*;
use oppa::*;
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
