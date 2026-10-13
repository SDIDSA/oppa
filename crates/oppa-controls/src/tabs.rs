use crate::common::*;
use oppa::*;
use oppa_macros::Props;
use std::rc::Rc;
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
            ctx.child_keyed(i as u64, &bp, TabButton)
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
    Div("tabs-container").child(
        Column::new()
            .style(Style::new().align_items(AlignItems::Center).gap(16))
            .children([bar, panel]),
    )
}
