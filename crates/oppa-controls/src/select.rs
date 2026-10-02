use crate::common::*;
use oppa::*;
use oppa_macros::Props;
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
///
/// Form validation (Phase 38a, decision 366 — G7): same four marks
/// as [`CheckboxProps`]; the marks announce on the `combobox` box.
#[derive(Clone, Props)]
pub struct SelectProps<T: 'static> {
    pub items: Vec<SelectItem<T>>,
    pub selected: Signal<T>,
    pub open: Signal<bool>,
    pub enabled: bool,
    pub width: f32,
    pub invalid: bool,
    pub required: bool,
    pub error_message: Option<SharedString>,
    pub helper_text: Option<SharedString>,
}

impl<T: Clone + PartialEq + 'static> SelectProps<T> {
    pub fn new(items: Vec<SelectItem<T>>, selected: Signal<T>, open: Signal<bool>) -> Self {
        Self {
            items,
            selected,
            open,
            enabled: true,
            width: 160.0,
            invalid: false,
            required: false,
            error_message: None,
            helper_text: None,
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
                .border(1, field_edge(ctx, props.invalid))
                .bg(if props.enabled { t.surface } else { t.disabled })
                .pad_x(8)
                .gap(8)
                .align_items(AlignItems::Center),
        ))
        .semantics(with_validation_marks(
            Semantics::combobox()
                .label(&current_label)
                .disabled(!props.enabled),
            props.invalid,
            props.required,
            &props.error_message,
        ));
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
                ctx.child_keyed(i as u64, &op, SelectOption)
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
    // Phase 38a: validation footer stacks under the box only when a
    // message is present (valid selects keep the exact tree).
    let root = Div("select")
        .style(Style::new().size(props.width, 32))
        .children(children);
    with_validation_footer(
        root,
        ctx,
        "select",
        props.invalid,
        &props.error_message,
        &props.helper_text,
    )
}
