use crate::common::*;
use oppa::*;
use oppa_macros::Props;
use std::rc::Rc;
// ---------------------------------------------------------------------------
// Splitter (Phase 38b, G13)
// ---------------------------------------------------------------------------

/// Splitter axis: `Vertical` splits left/right (the divider is a
/// vertical bar, `ColResize`); `Horizontal` splits top/bottom
/// (`RowResize`). One axis per instance (the Scrollbar-axis
/// precedent — decision 354).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SplitterAxis {
    #[default]
    Vertical,
    Horizontal,
}

/// Splitter props (controlled — `fraction` is the author-owned
/// first-pane share of the main axis, `0..1` clamped to the minima
/// below). Panes are factories (`Rc`, the TabItem-content
/// precedent — `VNode` is move-only, so factories rebuild per
/// render; child `(name, key)` pairs inside panes must be unique
/// across BOTH panes, the Tabs-namespace rule).
#[derive(Clone, Props)]
pub struct SplitterProps {
    pub fraction: Signal<f32>,
    pub axis: SplitterAxis,
    pub width: f32,
    pub height: f32,
    pub divider_px: f32,
    pub min_first_px: f32,
    pub min_second_px: f32,
    pub enabled: bool,
    pub debug: SharedString,
    pub first: Rc<dyn Fn(&Ctx) -> VNode>,
    pub second: Rc<dyn Fn(&Ctx) -> VNode>,
    pub on_change: Option<Change<f32>>,
}

/// Clamps `frac` into the minima (pure — headless-testable): the
/// first pane keeps `min_first_px`, the second `min_second_px`
/// (the divider straddles the fraction edge, hence the half-divider
/// terms). Panics loudly on non-finite/negative geometry or minima
/// exceeding the total (authoring contradictions, never silent
/// snaps — the Grid-span precedent).
pub fn splitter_fraction(
    frac: f32,
    total: f32,
    divider: f32,
    min_first: f32,
    min_second: f32,
) -> f32 {
    if !frac.is_finite() {
        panic!("splitter fraction must be finite, got {frac} — refused, never silent");
    }
    if !(total.is_finite() && total > 0.0) {
        panic!("splitter total must be finite positive, got {total} — refused, never silent");
    }
    if !(divider.is_finite() && divider >= 0.0) {
        panic!(
            "splitter divider must be finite non-negative, got {divider} — refused, never silent"
        );
    }
    if !(min_first.is_finite() && min_first >= 0.0) {
        panic!("splitter min_first_px must be finite non-negative, got {min_first} — refused, never silent");
    }
    if !(min_second.is_finite() && min_second >= 0.0) {
        panic!("splitter min_second_px must be finite non-negative, got {min_second} — refused, never silent");
    }
    if min_first + min_second + divider > total {
        panic!(
            "splitter minima ({min_first} + {min_second} + divider {divider}) exceed total {total} — refused, never a silent overlap"
        );
    }
    let lo = (min_first + divider / 2.0) / total;
    let hi = 1.0 - (min_second + divider / 2.0) / total;
    frac.clamp(lo, hi)
}

/// Sets a clamped splitter fraction and reports it (the single
/// funnel for the drag/arrow sites — the `apply_slider_value`
/// precedent, so notification cannot drift from any one path).
fn apply_split_fraction(
    fraction: &Signal<f32>,
    notify: &Option<Change<f32>>,
    frac: f32,
    total: f32,
    divider: f32,
    min_first: f32,
    min_second: f32,
) {
    let clamped = splitter_fraction(frac, total, divider, min_first, min_second);
    fraction.set(clamped);
    if let Some(notify) = notify.as_ref() {
        notify(clamped);
    }
}

/// Two-pane draggable divider (Slider-drag precedent): the divider
/// captures on press, every `Move` re-maps through
/// `capture_position` over the container box (fresh geometry at
/// event time — immune to body staleness), `Up`/`Cancel` clears.
/// Arrows nudge by 0.05 (Left/Down down, Right/Up up — the Slider
/// arrow precedent, axis-agnostic by design); ends clamp quietly.
/// Disabled renders the panes with a handlerless divider (no tab
/// stop either — decision 213).
pub fn Splitter(ctx: &Ctx, props: &SplitterProps) -> VNode {
    let total = match props.axis {
        SplitterAxis::Vertical => props.width,
        SplitterAxis::Horizontal => props.height,
    };
    let frac = splitter_fraction(
        props.fraction.get(),
        total,
        props.divider_px,
        props.min_first_px,
        props.min_second_px,
    );
    let d = props.divider_px;
    let first_px = frac * total - d / 2.0;
    let cross = match props.axis {
        SplitterAxis::Vertical => props.height,
        SplitterAxis::Horizontal => props.width,
    };
    let t = ctx.theme().tokens();
    let cursor = match props.axis {
        SplitterAxis::Vertical => CursorIcon::ColResize,
        SplitterAxis::Horizontal => CursorIcon::RowResize,
    };
    // Divider drag (the Slider-track precedent): the pointer
    // coordinate over the container box maps to the fraction.
    let host = ctx.host();
    let (g_frac, g_notify) = (props.fraction.clone(), props.on_change.clone());
    let (g_min_first, g_min_second) = (props.min_first_px, props.min_second_px);
    let g_axis = props.axis;
    let g_debug = props.debug.clone();
    let g_d = d;
    let drag_action = move || {
        let Some((x, y)) = host.capture_position() else {
            return;
        };
        let Some(box_id) = oppa::find_retained_by_debug(&host, &g_debug)
            .into_iter()
            .next()
        else {
            return;
        };
        let Some(b) = host.committed_box(box_id) else {
            return;
        };
        let (coord, origin, extent) = match g_axis {
            SplitterAxis::Vertical => (x, b.x, b.w),
            SplitterAxis::Horizontal => (y, b.y, b.h),
        };
        if !(extent.is_finite() && extent > 0.0) {
            return;
        }
        apply_split_fraction(
            &g_frac,
            &g_notify,
            (coord - origin) / extent,
            extent,
            g_d,
            g_min_first,
            g_min_second,
        );
    };
    let nudge = |dir: f32| {
        let (fraction, notify) = (props.fraction.clone(), props.on_change.clone());
        let (total, d, min_first, min_second) = (total, d, props.min_first_px, props.min_second_px);
        move || {
            apply_split_fraction(
                &fraction,
                &notify,
                fraction.get() + dir * 0.05,
                total,
                d,
                min_first,
                min_second,
            )
        }
    };
    // Panes size explicitly (decision 213 — no content-derived
    // sizes anywhere near a drag map).
    let (first_style, divider_style, second_style) = match props.axis {
        SplitterAxis::Vertical => (
            Style::new().size(first_px.max(0.0), cross),
            Style::new().size(d, cross).bg(t.border).cursor(cursor),
            Style::new().size((total - frac * total - d / 2.0).max(0.0), cross),
        ),
        SplitterAxis::Horizontal => (
            Style::new().size(cross, first_px.max(0.0)),
            Style::new().size(cross, d).bg(t.border).cursor(cursor),
            Style::new().size(cross, (total - frac * total - d / 2.0).max(0.0)),
        ),
    };
    let first = Div(format!("{}-first", props.debug.as_ref()).as_str())
        .style(first_style)
        .child((props.first)(ctx));
    let second = Div(format!("{}-second", props.debug.as_ref()).as_str())
        .style(second_style)
        .child((props.second)(ctx));
    let divider = Div(format!("{}-divider", props.debug.as_ref()).as_str()).style(focus_ringed(
        ctx,
        props.enabled,
        divider_style,
    ));
    let divider = if props.enabled {
        divider
            // Focus-only press (capture owner for drags + tab stop +
            // focus for arrows — the Slider-root precedent, the
            // closure is intentionally empty).
            .on_press(|| {})
            .on_drag(drag_action)
            .on_key_left(nudge(-1.0))
            .on_key_down(nudge(-1.0))
            .on_key_right(nudge(1.0))
            .on_key_up(nudge(1.0))
    } else {
        divider
    };
    // NOTE: `divider` is an `ElementBuilder` here (handlers attach
    // pre-child); the divider itself is a childless bar (paints its
    // `border`-ink fill, nothing else).
    let divider = divider.build();
    let inner = match props.axis {
        SplitterAxis::Vertical => Row(format!("{}-inner", props.debug.as_ref()).as_str())
            .children([first, divider, second]),
        SplitterAxis::Horizontal => Div(format!("{}-inner", props.debug.as_ref()).as_str())
            .children([first, divider, second]),
    };
    Div(&props.debug)
        .style(Style::new().size(props.width, props.height))
        .child(inner)
}
