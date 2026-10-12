use crate::button::{Button, ButtonProps};
use crate::common::*;
use crate::text_input::{TextInput, TextInputProps};
use oppa::*;
use oppa_macros::Props;
use std::rc::Rc;
// ---------------------------------------------------------------------------
// DatePicker (Phase 38b, G14)
// ---------------------------------------------------------------------------

/// Calendar date (proleptic Gregorian, no timezone — a picked day,
/// never an instant). `month` is 1–12, `day` 1–31 (validated by
/// [`Date::valid`]); ordering is chronological (derived `Ord`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

/// Date failure (loud by construction — authoring bugs panic,
/// user input parses to `None`).
#[derive(Clone, Debug, PartialEq)]
pub enum DateError {
    OutOfMonth(u8),
    OutOfDay(u8),
}

impl Date {
    /// Builds a date, refusing out-of-range months/days loudly
    /// (including Feb 30 — validated against the leap rule, never
    /// silently normalized like a lenient parser would).
    pub fn new(year: i32, month: u8, day: u8) -> Self {
        if !(1..=12).contains(&month) {
            panic!("date month must be 1..=12, got {month} — refused, never silent");
        }
        let dim = days_in_month(year, month);
        if day < 1 || day > dim {
            panic!(
                "date day must be 1..={dim} for {year}-{month:02}, got {day} — refused, never silent"
            );
        }
        Self { year, month, day }
    }

    /// Checked construction for parsed input (`None` on any
    /// out-of-range field — user input is a query outcome, never a
    /// failure).
    pub fn valid(year: i32, month: u8, day: u8) -> Option<Self> {
        if !(1..=12).contains(&month) {
            return None;
        }
        let dim = days_in_month(year, month);
        if day < 1 || day > dim {
            return None;
        }
        Some(Self { year, month, day })
    }
}

/// Leap rule (Gregorian — divisible by 4, except centuries unless
/// divisible by 400).
pub fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Month length (pure — headless-testable; panics loudly on month
/// 0/13+, never a silent 30).
pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        m => panic!("month must be 1..=12, got {m} — refused, never silent"),
    }
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm — pure,
/// headless-testable; the weekday/month-shift backbone).
fn days_from_civil(year: i32, month: u8, day: u8) -> i32 {
    let (mut y, m) = (year, month as i32);
    y -= (m <= 2) as i32;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i32) -> Date {
    let zz = z + 719468;
    let era = if zz >= 0 { zz } else { zz - 146096 } / 146097;
    let doe = zz - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    y += (m <= 2) as i32;
    Date {
        year: y,
        month: m,
        day: d,
    }
}

/// Monday-based weekday of a date (0 = Monday … 6 = Sunday; pure —
/// 1970-01-01 was a Thursday, hence the +3).
pub fn weekday_monday0(date: Date) -> u8 {
    (days_from_civil(date.year, date.month, date.day) + 3).rem_euclid(7) as u8
}

/// Shifts a date by whole days, clamping the result into
/// `[min, max]` (pure — the arrow-key backbone; out-of-range ends
/// pin, never wrap).
pub fn shift_days(date: Date, delta: i32, min: Option<Date>, max: Option<Date>) -> Date {
    let moved = civil_from_days(days_from_civil(date.year, date.month, date.day) + delta);
    clamp_date(moved, min, max)
}

/// Clamps a date into `[min, max]` (pure — `None` bounds are open).
pub fn clamp_date(date: Date, min: Option<Date>, max: Option<Date>) -> Date {
    let mut d = date;
    if let Some(lo) = min {
        if d < lo {
            d = lo;
        }
    }
    if let Some(hi) = max {
        if d > hi {
            d = hi;
        }
    }
    d
}

/// Shifts the shown month, keeping the day where the target month
/// allows (clamped to its length — Jan 31 → Feb 28/29, never a
/// silent March spill). Returns `None` when the shifted month lies
/// fully outside `[min, max]` (the prev/next disable rule).
pub fn shift_month(date: Date, delta: i32, min: Option<Date>, max: Option<Date>) -> Option<Date> {
    let total = date.year as i64 * 12 + (date.month as i64 - 1) + delta as i64;
    let y = total.div_euclid(12) as i32;
    let m = (total.rem_euclid(12) + 1) as u8;
    let dim = days_in_month(y, m);
    let first = Date {
        year: y,
        month: m,
        day: 1,
    };
    let last = Date {
        year: y,
        month: m,
        day: dim,
    };
    if let Some(lo) = min {
        if last < lo {
            return None;
        }
    }
    if let Some(hi) = max {
        if first > hi {
            return None;
        }
    }
    Some(Date {
        year: y,
        month: m,
        day: date.day.min(dim),
    })
}

/// Formats `YYYY-MM-DD` (the text-bridge spelling — fixed width,
/// zero-padded, lexicographically sortable).
pub fn format_date(date: Date) -> SharedString {
    SharedString::from(format!(
        "{:04}-{:02}-{:02}",
        date.year, date.month, date.day
    ))
}

/// Parses `YYYY-MM-DD` (strict — 4/2/2 digits with `-` separators;
/// anything else is `None`, never a loud parse — user input is a
/// query outcome). Out-of-range fields are `None` too (Feb 30
/// parses to nothing, never a silent March 2).
pub fn parse_date(s: &str) -> Option<Date> {
    let mut parts = s.split('-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    let year: i32 = y.parse().ok()?;
    let month: u8 = m.parse().ok()?;
    let day: u8 = d.parse().ok()?;
    Date::valid(year, month, day)
}

/// English month names (the popup header — fixed strings, never
/// locale lookups; localization is app-side, stated).
pub const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Monday-first two-letter weekday headers.
pub const WEEKDAY_HEADERS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

/// DatePicker props (controlled — `value` is the picked day,
/// `open` the month-popup visibility; both author-owned signals).
/// `min`/`max` bound every path (arrows pin, month flips refuse
/// past the bound, out-of-range days render handlerless, parses
/// outside the bound are ignored — one bound, never per-path
/// second-guessing).
#[derive(Clone, Props)]
pub struct DatePickerProps {
    pub value: Signal<Date>,
    pub open: Signal<bool>,
    pub min: Option<Date>,
    pub max: Option<Date>,
    pub enabled: bool,
    pub width: f32,
    pub label: SharedString,
}

fn date_in_bounds(date: Date, min: Option<Date>, max: Option<Date>) -> bool {
    clamp_date(date, min, max) == date
}

/// Month-grid popup date picker: a `combobox` box (a [`TextInput`]
/// parse bridge showing `YYYY-MM-DD` + a chevron toggle) over a
/// `Portal` month [`Grid`](oppa::Grid) (the Select-popup
/// precedent — `x(0)` + `absolute_y(36)`, out-of-flow, constant
/// 32px root). Typing a valid in-bounds date commits it (invalid
/// text is a quiet no-op — the controlled-contract edge, same class
/// as an unmatched Select signal); picking a day sets the value and
/// closes; prev/next flip the shown month (the value's month — the
/// popup carries no cursor state, so month and value cannot drift).
/// Arrows on the focused toggle step the value (Left/Right ±1 day,
/// Up/Down ±7, `Home` month-first, `End` month-last — the Slider
/// key precedent); Enter/Space toggles through the router pulse.
/// Disabled drops every handler structurally (decision 213).
pub fn DatePicker(ctx: &Ctx, props: &DatePickerProps) -> VNode {
    let t = ctx.theme().tokens();
    let value = props.value.get();
    let is_open = props.open.get();
    // Text bridge (the studio-inspector latch): the text signal
    // mirrors the value; typing parses back into it.
    let text = ctx.signal(format_date(value));
    let spelled = format_date(value);
    if text.get() != spelled {
        text.set(spelled.clone());
    }
    let (p_value, p_min, p_max) = (props.value.clone(), props.min, props.max);
    let commit: Change<SharedString> = Rc::new(move |s| {
        let Some(d) = parse_date(&s) else { return };
        if d != p_value.get() && date_in_bounds(d, p_min, p_max) {
            p_value.set(d);
        }
    });
    let text_props = TextInputProps {
        label: props.label.clone(),
        value: text,
        placeholder: Some(SharedString::from("YYYY-MM-DD")),
        enabled: props.enabled,
        width: (props.width - 40.0).max(80.0),
        height: 32.0,
        style: Text::body_secondary,
        debug: SharedString::from("date-text"),
        on_change: Some(commit),
        masked: false,
        invalid: false,
        required: false,
        error_message: None,
        helper_text: None,
        borderless: true,
        icon: None,
        icon_size: 16.0,
    };
    let toggle = {
        let open = props.open.clone();
        action(move || open.set(!open.get()))
    };
    let chevron = Path::new("date-chevron")
        .data(if is_open {
            "M 2 6 L 6 2 L 10 6"
        } else {
            "M 2 2 L 6 6 L 10 2"
        })
        .stroke(t.text_primary, 2.0)
        .size(12, 8)
        .build();
    // Keyboard day-steps (the Slider key precedent): arrows pin to
    // the bounds (quiet no-ops past them); Home/End jump to the
    // month edges. Enter/Space toggle through the router pulse.
    let step = |delta: i32| {
        let (v, min, max) = (props.value.clone(), props.min, props.max);
        move || {
            let next = shift_days(v.get(), delta, min, max);
            if next != v.get() {
                v.set(next);
            }
        }
    };
    let edge = |to_first: bool| {
        let (v, min, max) = (props.value.clone(), props.min, props.max);
        move || {
            let cur = v.get();
            let dim = days_in_month(cur.year, cur.month);
            let end = if to_first {
                Date {
                    year: cur.year,
                    month: cur.month,
                    day: 1,
                }
            } else {
                Date {
                    year: cur.year,
                    month: cur.month,
                    day: dim,
                }
            };
            let next = clamp_date(end, min, max);
            if next != cur {
                v.set(next);
            }
        }
    };
    let box_builder = Row("date-box")
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
                .label(&spelled)
                .disabled(!props.enabled),
        );
    let box_builder = if props.enabled {
        box_builder
            .on_press(move || toggle())
            .on_key_left(step(-1))
            .on_key_right(step(1))
            .on_key_up(step(-7))
            .on_key_down(step(7))
            .on_key_home(edge(true))
            .on_key_end(edge(false))
    } else {
        box_builder
    };
    // (The TextInput child owns its own press/focus — taps into the
    // text focus the field for typing; taps on the chevron half climb
    // to the box press and toggle the popup (knob→track precedent).
    // Keyboard users step days with the box arrows from either half.)
    let text_child = ctx.child_auto(&text_props, TextInput);
    let select_box = box_builder.children([text_child, chevron]);
    let mut children = vec![select_box];
    if is_open {
        let dim = days_in_month(value.year, value.month);
        let lead = weekday_monday0(Date {
            year: value.year,
            month: value.month,
            day: 1,
        }) as usize;
        let title = SharedString::from(format!(
            "{} {}",
            MONTH_NAMES[(value.month - 1) as usize],
            value.year
        ));
        let can_prev = shift_month(value, -1, props.min, props.max).is_some();
        let can_next = shift_month(value, 1, props.min, props.max).is_some();
        let prev_value = props.value.clone();
        let prev_btn = if props.enabled && can_prev {
            let pv = prev_value.clone();
            let (pmin, pmax) = (props.min, props.max);
            ctx.child_keyed(
                1000,
                &ButtonProps {
                    label: SharedString::from("<"),
                    enabled: true,
                    width: 32.0,
                    height: 28.0,
                    debug: SharedString::from("date-prev"),
                    on_press: action(move || {
                        if let Some(d) = shift_month(pv.get(), -1, pmin, pmax) {
                            pv.set(Date {
                                day: pv.get().day.min(days_in_month(d.year, d.month)),
                                ..d
                            });
                        }
                    }),
                    icon: None,
                    icon_size: 16.0,
                },
                Button,
            )
        } else {
            Div("date-prev-off")
                .style(Style::new().size(32, 28).bg(t.disabled))
                .build()
        };
        let next_btn = if props.enabled && can_next {
            let nv = prev_value.clone();
            let (pmin, pmax) = (props.min, props.max);
            ctx.child_keyed(
                1001,
                &ButtonProps {
                    label: SharedString::from(">"),
                    enabled: true,
                    width: 32.0,
                    height: 28.0,
                    debug: SharedString::from("date-next"),
                    on_press: action(move || {
                        if let Some(d) = shift_month(nv.get(), 1, pmin, pmax) {
                            nv.set(Date {
                                day: nv.get().day.min(days_in_month(d.year, d.month)),
                                ..d
                            });
                        }
                    }),
                    icon: None,
                    icon_size: 16.0,
                },
                Button,
            )
        } else {
            Div("date-next-off")
                .style(Style::new().size(32, 28).bg(t.disabled))
                .build()
        };
        let header = Row("date-header")
            .style(Style::new().gap(8).align_items(AlignItems::Center))
            .children([prev_btn, VNode::from(Text::new(title).bold()), next_btn]);
        let mut cells: Vec<VNode> = WEEKDAY_HEADERS
            .iter()
            .map(|w| {
                with_ink(
                    VNode::from(Text::new(SharedString::from(*w)).size(12)),
                    t.text_secondary,
                )
            })
            .collect();
        for _ in 0..lead {
            cells.push(Div("date-blank").style(Style::new().size(28, 28)).build());
        }
        for day in 1..=dim {
            let d = Date {
                year: value.year,
                month: value.month,
                day,
            };
            let in_bounds = date_in_bounds(d, props.min, props.max);
            let is_sel = d == value;
            let mut day_label = Text::new(SharedString::from(format!("{day}"))).size(12);
            if is_sel {
                day_label = day_label.bold();
            }
            let day_text = if is_sel {
                with_ink(VNode::from(day_label), t.primary)
            } else {
                VNode::from(day_label)
            };
            let cell = if props.enabled && in_bounds {
                let (vv, oo) = (props.value.clone(), props.open.clone());
                Div(format!("date-day-{day}").as_str())
                    .style(
                        Style::new()
                            .size(28, 28)
                            .radius(4)
                            .bg(if is_sel { t.disabled } else { t.surface })
                            .align_items(AlignItems::Center),
                    )
                    .on_press(move || {
                        vv.set(d);
                        oo.set(false);
                    })
                    .child(day_text)
            } else {
                Div(format!("date-day-{day}").as_str())
                    .style(
                        Style::new()
                            .size(28, 28)
                            .bg(t.disabled)
                            .align_items(AlignItems::Center),
                    )
                    .child(with_ink(day_text, t.text_secondary))
            };
            cells.push(cell);
        }
        let grid = oppa::Grid("date-grid")
            .style(
                Style::new()
                    .grid_cols(vec![oppa::GridTrack::Fr(Px::of(1.0)); 7])
                    .gap(2),
            )
            .children(cells);
        let panel = Div("date-panel")
            .style(
                Style::new()
                    .radius(4)
                    .border(1, t.border)
                    .bg(t.surface)
                    .pad_x(8)
                    .pad_y(8)
                    .gap(8),
            )
            .children([header, grid]);
        let popup = Portal("date-popup")
            .style(Style::new().x(0).absolute_y(36).w(props.width))
            .child(panel);
        children.push(popup);
    }
    // Constant 32px root (the Select-popup precedent): the box owns
    // the height, the popup is out-of-flow.
    Div("date")
        .style(Style::new().size(props.width, 32))
        .children(children)
}
