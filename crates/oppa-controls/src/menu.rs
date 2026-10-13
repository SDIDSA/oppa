//! Menu primitives (Round 17.1, decision 317): [`MenuItem`] rows,
//! the [`Menu`] anchored portal list, and [`ContextMenu`] (anchor
//! wrapper + cursor-anchored menu) — keyboard navigation, disabled
//! items, separators, and light-dismiss, composed from proven
//! primitives only (decision 212).
//!
//! Focus design (no new framework surface — routers own focus,
//! components never move it):
//!
//! - A secondary press focuses the press owner on Down (router
//!   rule, any button) and publishes the cursor tap point on Up
//!   (decision 301 — menu handlers anchor through it).
//! - The anchor wrapper (context menus) or the list container
//!   (tabbed-into dropdowns) holds focus for the whole open
//!   session; rows are deliberately NOT press owners (an owner
//!   row would steal focus mid-gesture and unmount the menu
//!   before its own release — the SelectOption shape cannot
//!   survive a focus-derived show rule, documented, not silent).
//! - Visibility derives from focus (`open && (anchor focus ||
//!   list focus)`) — outside presses, Escape (blur via the
//!   back-chain), and tab-outs all funnel through one blur rule,
//!   so every dismissal path is the same edge. Attached-mode
//!   dismissal writes `open` back to false on that edge (guarded,
//!   terminating — settling falsifies the guard); standalone
//!   dropdowns (`anchor_focus: None`) keep full author control
//!   (Select parity — no light-dismiss).
//! - Activation disambiguates exactly like tap-to-caret (Round
//!   8.1): a press WITH a tap point hit-tests rows by committed
//!   box (debug labels, the slider-trackbox precedent); a press
//!   WITHOUT one (keyboard Enter — the router clears the point on
//!   keyboard activation) invokes the highlight.
//! - Same-value [`Signal::set`] still invalidates (no equality
//!   gate) — every highlight write below is guarded by a change
//!   check, so redundant keys never spin repaints.
//!
//! Deliberately v1 (open questions, not silent gaps): no
//! per-row press owners (the focus rule above — rows stay
//! handlerless by design); right-held single-gesture drag-select
//! stays out of reach (tap-to-open means the menu opens on
//! secondary-UP, so nothing is open mid right-drag —
//! press-drag-release over an open menu is the supported shape).

use oppa::{
    find_retained_by_debug, ComponentHost, Ctx, Div, Portal, Props, SharedString, Signal, Style,
    Text, VNode,
};
use oppa_macros::Props;

use super::{action, Action};

// ---------------------------------------------------------------------------
// MenuItem
// ---------------------------------------------------------------------------

/// One menu row (visual unit — interaction rides the list and
/// anchor owners, never the row, so focus never leaves the open
/// session mid-gesture; see the module docs).
#[derive(Clone, Props)]
pub struct MenuItemProps {
    /// Row label.
    pub label: SharedString,
    /// Disabled rows render dimmed, skip keyboard nav, and ignore
    /// pointer hits (the menu stays open — native no-op).
    pub enabled: bool,
    /// Separator rows render a 1px rule (non-interactive, skipped
    /// in nav; pointer hits land as dead padding and dismiss).
    pub separator: bool,
    /// Keyboard cursor (only the effective highlight paints —
    /// derived in [`Menu`], never set here).
    pub highlighted: bool,
    /// Retained debug label (set per index by [`Menu`] —
    /// `"menu-item-{menu}-{i}"` — the hit-test address).
    pub debug: SharedString,
    /// Activation payload (ignored while disabled).
    pub on_select: Action,
}

impl MenuItemProps {
    pub fn new(label: &str, on_select: impl Fn() + 'static) -> Self {
        Self {
            label: SharedString::from(label),
            enabled: true,
            separator: false,
            highlighted: false,
            debug: SharedString::from("menu-item"),
            on_select: action(on_select),
        }
    }

    pub fn separator() -> Self {
        Self {
            label: SharedString::from(""),
            enabled: false,
            separator: true,
            highlighted: false,
            debug: SharedString::from("menu-separator"),
            on_select: action(|| {}),
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// One menu row: highlighted rows read bold contrast ink on the
/// primary fill (the `CONTRAST_INK`-on-saturated-accents rule);
/// disabled rows read dimmed (the catalog dim rule); separators
/// read a 1px border rule. Rows carry the `MenuItem` role (decision
/// 352 — migrated off the v1 `list_item` analog so Invoke-class AT
/// actions attach to the real affordance: UIA Invoke, DOM
/// `menuitem`, AT-SPI `menu item` + `click`). Highlight still rides
/// `selected` (the shared selection rule, unchanged).
pub fn MenuItem(ctx: &Ctx, props: &MenuItemProps) -> VNode {
    use oppa::Semantics;
    let t = ctx.theme().tokens();
    if props.separator {
        return Div(&props.debug)
            .style(Style::new().fill_width().h(1).bg(t.border))
            .build();
    }
    let label = if props.highlighted {
        VNode::from(Text::new(props.label.clone()).bold())
    } else {
        VNode::from(Text::new(props.label.clone()))
    };
    // Contrast ink on the saturated highlight (decision 306's one
    // deliberate exception); dimmed ink while disabled; inherited
    // body ink otherwise (no override to clear).
    let label = if props.highlighted {
        super::with_ink(label, super::CONTRAST_INK)
    } else if !props.enabled {
        super::with_ink(label, t.text_secondary)
    } else {
        label
    };
    let mut style = Style::new()
        .fill_width()
        .pad_x(8)
        .pad_y(6)
        // Decision 514: highlight swaps tween (keyboard rove and
        // pointer hover ride the same cursor).
        .transition(oppa::Transition::micro());
    if props.highlighted {
        style = style.bg(t.primary);
    }
    Div(&props.debug)
        .style(style)
        .semantics(
            Semantics::menu_item()
                .selected(props.highlighted)
                .label(&props.label)
                .disabled(!props.enabled),
        )
        .child(label)
}

// ---------------------------------------------------------------------------
// Pure nav + hit-test helpers (headless-tested below)
// ---------------------------------------------------------------------------

/// Rows that keyboard nav and invocation may land on.
fn is_selectable(it: &MenuItemProps) -> bool {
    !it.separator && it.enabled
}

/// Effective highlight: the stored index when selectable, else the
/// next selectable forward (wrapping), else `None` (empty or
/// all-disabled — Enter no-ops, nothing paints highlighted).
fn effective_highlight(items: &[MenuItemProps], stored: usize) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    let mut i = stored.min(items.len() - 1);
    for _ in 0..items.len() {
        if is_selectable(&items[i]) {
            return Some(i);
        }
        i = (i + 1) % items.len();
    }
    None
}

/// One arrow step from `from` (wrapping, skipping separators and
/// disabled rows; `dir` is +1 down / -1 up). No selectable row
/// keeps the index (never invents a highlight).
fn step_highlight(items: &[MenuItemProps], from: usize, dir: i32) -> usize {
    if items.is_empty() {
        return 0;
    }
    let len = items.len();
    let mut i = from.min(len - 1);
    for _ in 0..len {
        i = (i as i32 + dir).rem_euclid(len as i32) as usize;
        if is_selectable(&items[i]) {
            return i;
        }
    }
    from.min(len - 1)
}

/// Invokes the effective highlight (Enter path) and closes.
/// No-op when nothing is effectively highlighted.
fn activate_highlighted(items: &[MenuItemProps], highlight: &Signal<usize>, open: &Signal<bool>) {
    if let Some(i) = effective_highlight(items, highlight.get()) {
        if let Some(it) = items.get(i) {
            if is_selectable(it) {
                it.on_select.clone()();
                open.set(false);
            }
        }
    }
}

/// Row index under a device-px point, if any (pointer path —
/// committed boxes by hit-test label, the slider-trackbox
/// precedent; separators count (dead padding dismisses), gaps do
/// not).
fn hit_item(
    host: &ComponentHost,
    menu_inst: u64,
    items_len: usize,
    x: f32,
    y: f32,
) -> Option<usize> {
    for i in 0..items_len {
        let label = format!("menu-item-{menu_inst}-{i}");
        for id in find_retained_by_debug(host, &label) {
            // Decision 511: test in the row's local space (hit
            // follows the visual under ancestor transforms).
            let (lx, ly) = host.local_point(id, x, y);
            if let Some(b) = host.committed_box(id) {
                if lx >= b.x && lx < b.x + b.w && ly >= b.y && ly < b.y + b.h {
                    return Some(i);
                }
            }
        }
    }
    None
}

/// Clamps/flips a popup origin into the viewport (Round 21.3,
/// decision 330 — shared by [`Menu`], [`ContextMenu`] (through
/// `Menu`), and `Tooltip`): overflow flips to the anchor's far
/// side first (native menu behavior — open leftward/upward), then
/// pins at zero (a popup larger than the viewport pins top-left —
/// never negative, never silently overflowing). `size` is zero on
/// the first frame (extent unknown until layout settles) — that
/// pass renders unclamped and the settled pass clamps (the
/// Scrollbar settled-box precedent, terminating).
pub(crate) fn clamp_popup_anchor(
    viewport: (f32, f32),
    anchor: (f32, f32),
    size: (f32, f32),
) -> (f32, f32) {
    let (vw, vh) = viewport;
    let (ax, ay) = anchor;
    let (w, h) = size;
    let x = if ax + w > vw {
        (ax - w).max(0.0)
    } else {
        ax.max(0.0)
    };
    let y = if ay + h > vh {
        (ay - h).max(0.0)
    } else {
        ay.max(0.0)
    };
    (x, y)
}

/// Invokes the enabled row under a list release point and closes
/// (Round 21.3 — shared by tap-release `on_press` and drag-release
/// `on_drag_release`): disabled hits no-op with the menu open
/// (native no-op — never a dismiss); dead padding dismisses;
/// outside releases never reach here (the router's inside rule
/// owns slide-off cancel) and ignore defensively.
fn list_release_row(
    host: &ComponentHost,
    inst: u64,
    items: &[MenuItemProps],
    open: &Signal<bool>,
    x: f32,
    y: f32,
) {
    match hit_item(host, inst, items.len(), x, y) {
        Some(i) => {
            if let Some(it) = items.get(i) {
                if is_selectable(it) {
                    it.on_select.clone()();
                    open.set(false);
                }
            }
        }
        None if in_list_box(host, inst, x, y) => open.set(false),
        None => {}
    }
}

/// Wrapper release rule (Round 21.3 — shared by tap-release
/// `on_press` and drag-release `on_drag_release`): an enabled row
/// under the release point invokes and closes; anything else
/// dismisses (disabled rows dismiss without invoking — the
/// wrapper's pre-21.3 rule, unchanged).
fn wrapper_release_row(
    host: &ComponentHost,
    menu_inst: Option<u64>,
    items: &[MenuItemProps],
    open: &Signal<bool>,
    x: f32,
    y: f32,
) {
    let hit = menu_inst.and_then(|m| hit_item(host, m, items.len(), x, y));
    match hit.and_then(|i| items.get(i)) {
        Some(it) if is_selectable(it) => {
            it.on_select.clone()();
            open.set(false);
        }
        _ => open.set(false),
    }
}

/// True when a device-px point lands inside the list box (dead
/// padding clicks dismiss; outside clicks never reach here — the
/// blur rule owns them).
fn in_list_box(host: &ComponentHost, menu_inst: u64, x: f32, y: f32) -> bool {
    let label = format!("menu-list-{menu_inst}");
    find_retained_by_debug(host, &label).into_iter().any(|id| {
        // Decision 511: test in the list's local space.
        let (lx, ly) = host.local_point(id, x, y);
        host.committed_box(id)
            .is_some_and(|b| lx >= b.x && lx < b.x + b.w && ly >= b.y && ly < b.y + b.h)
    })
}

// ---------------------------------------------------------------------------
// Menu
// ---------------------------------------------------------------------------

/// Anchored menu list (controlled visibility + author-positioned
/// anchor; [`ContextMenu`] wires focus, cursor, and dismissal, and
/// dropdown authors wire `open` + `anchor` themselves).
#[derive(Clone, Props)]
pub struct MenuProps {
    /// Rows (separators + disabled included — nav and hit-test
    /// understand both).
    pub items: Vec<MenuItemProps>,
    /// Controlled visibility (authors own it; attached mode also
    /// clears it on blur-dismiss — Select writes `open` the same
    /// way).
    pub open: Signal<bool>,
    /// Portal offset from the menu root origin, device px
    /// (cursor-anchored by [`ContextMenu`], box-anchored by
    /// dropdown authors — the Select popup precedent).
    pub anchor: (f32, f32),
    /// List width, device px (rows stretch full-width so the
    /// highlight bar is uniform).
    pub width: f32,
    /// Shared keyboard cursor (`None` = menu-owned internal —
    /// dropdown authors pass their own to reset it; [`ContextMenu`]
    /// always passes its own and resets on open).
    pub highlight: Option<Signal<usize>>,
    /// Anchor focus for attached mode (`Some` = context menus:
    /// visibility derives from focus and blur dismisses;
    /// `None` = standalone dropdowns: `open` alone shows, the
    /// author owns dismissal — Select parity).
    pub anchor_focus: Option<Signal<bool>>,
}

/// Anchored portal list: themed plate, keyboard cursor, pointer
/// hit-test activation, attached-mode blur dismiss. Closed
/// renders an explicit 0×0 portal (flow containers skip portals —
/// no phantom spacing, the Modal precedent).
pub fn Menu(ctx: &Ctx, props: &MenuProps) -> VNode {
    // Decision 514: appear-fade visibility first (the guarded reset
    // must run even on the closed path, or the next open would mount
    // already shown).
    let shown = crate::motion::enter_shown(ctx, &props.open);
    if !props.open.get() {
        return Portal("menu-closed").style(Style::new().size(0, 0)).build();
    }
    // Visibility derivation (attached mode only): the menu shows
    // while the anchor wrapper or the list holds focus. Outside
    // presses, Escape (blur via the back-chain), and tab-outs all
    // clear both — one blur edge for every dismissal path.
    let attached = props.anchor_focus.is_some();
    let show = props
        .anchor_focus
        .as_ref()
        .is_none_or(|f| f.get() || ctx.focused().get());
    if attached && !show {
        // Sticky dismiss on the blur edge (guarded terminating
        // write — settling falsifies the guard, so this runs
        // exactly once per dismissal).
        props.open.set(false);
        return Portal("menu-closed").style(Style::new().size(0, 0)).build();
    }
    let t = ctx.theme().tokens();
    let highlight = props
        .highlight
        .clone()
        .unwrap_or_else(|| ctx.signal(0usize));
    let eff = effective_highlight(&props.items, highlight.get());
    let inst = ctx.instance_id();
    let rows = props
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let row = MenuItemProps {
                highlighted: eff == Some(i),
                debug: SharedString::from(format!("menu-item-{inst}-{i}").as_str()),
                ..item.clone()
            };
            ctx.child_keyed(i as u64, &row, MenuItem)
        })
        .collect::<Vec<_>>();
    // The list owns presses (hit-test activation) and arrow keys
    // (tabbed-into dropdowns); rows stay handlerless so focus
    // never fragments mid-gesture (see the module docs).
    let host = ctx.host();
    let items = props.items.clone();
    let open = props.open.clone();
    let highlight_key = highlight.clone();
    // The list press closure owns its host clone (the clamp +
    // hover reads below keep the body handle — the ContextMenu
    // multi-closure precedent).
    let host_press = host.clone();
    // Hover highlight (Round 21.3, decision 330): moves over the
    // Press-owning list bump `hover_move` (rows stay handlerless —
    // the focus rule above), the bump re-renders, and the mouse
    // position (id 0, the locked mouse contract) hit-tests into the
    // SAME highlight the arrows drive (unified cursor). Guarded
    // writes only (same-value sets still invalidate). Touch
    // drag-selects invoke on release through the id-agnostic tap
    // point instead (no hover id to name mid-drag).
    let _ = ctx.hover_move().get();
    if let Some((mx, my)) = host.pointer_position(0) {
        if let Some(i) = hit_item(&host, inst, items.len(), mx, my) {
            if items.get(i).is_some_and(is_selectable) && highlight.get() != i {
                highlight.set(i);
            }
        }
    }
    let list = Div(format!("menu-list-{inst}").as_str())
        .style(
            Style::new()
                .w(props.width)
                .radius(4)
                .border(1, t.border)
                .bg(t.surface)
                .pad_y(4)
                // Decision 514: the plate fades + rises on open
                // (appear-first — exits snap, see SPEC).
                .opacity(Some(if shown.get() { 1.0 } else { 0.0 }))
                .transform(if shown.get() {
                    oppa::Transform::identity()
                } else {
                    oppa::Transform::translate(0, crate::motion::ENTER_RISE_PX)
                })
                .transition(oppa::Transition::enter()),
        )
        .semantics(oppa::Semantics::default().label("menu"))
        .on_press(move || {
            // Activation disambiguates like tap-to-caret: a press
            // WITH a tap point hit-tests rows (Select field_press
            // precedent); keyboard activation clears the point, so
            // a press WITHOUT one invokes the highlight.
            match host_press.last_press_position() {
                None => activate_highlighted(&items, &highlight_key, &open),
                Some((x, y)) => list_release_row(&host_press, inst, &items, &open, x, y),
            }
        })
        .on_drag_release({
            let host_release = host.clone();
            let items = props.items.clone();
            let open = props.open.clone();
            move || {
                // Round 21.3 drag-select: a press-held release over
                // the list invokes the release row (press-drag
                // into an open menu); slide-off releases never
                // arrive (the router's inside rule cancels them).
                if let Some((x, y)) = host_release.last_drag_release() {
                    list_release_row(&host_release, inst, &items, &open, x, y);
                }
            }
        })
        .on_key_down({
            let highlight = highlight.clone();
            let items = props.items.clone();
            move || {
                let next = step_highlight(&items, highlight.get(), 1);
                if next != highlight.get() {
                    highlight.set(next);
                }
            }
        })
        .on_key_up({
            let highlight = highlight.clone();
            let items = props.items.clone();
            move || {
                let next = step_highlight(&items, highlight.get(), -1);
                if next != highlight.get() {
                    highlight.set(next);
                }
            }
        })
        .children(rows);
    // Viewport-edge clamping (Round 21.3, decision 330): flip to
    // the anchor's far side when the plate would overflow, pin at
    // zero past that. One settled read per anchor value: the
    // pre-layout pass renders at the raw anchor (box unknown) and
    // stays unplaced so the layout publish re-runs exactly once
    // more; the placed pass commits the clamped origin and never
    // reads settled again — a persistent generation subscription
    // here re-renders every publish, and each re-render re-dirties
    // layout through the row children, so it must be one-shot
    // (this spin, found headlessly, is why).
    let placed = ctx.signal(None::<((f32, f32), (f32, f32))>);
    let origin = match placed.get() {
        Some((a, o)) if a == props.anchor => o,
        _ => {
            let label = format!("menu-list-{inst}");
            match host.settled_box_by_debug(&label).map(|b| b.h) {
                None => props.anchor,
                Some(h) => {
                    let (vw, vh) = host.viewport_size();
                    let o = clamp_popup_anchor((vw, vh), props.anchor, (props.width, h));
                    if placed.get() != Some((props.anchor, o)) {
                        placed.set(Some((props.anchor, o)));
                    }
                    o
                }
            }
        }
    };
    Portal("menu-popup")
        .style(Style::new().x(origin.0).absolute_y(origin.1).w(props.width))
        .child(list)
}

// ---------------------------------------------------------------------------
// ContextMenu
// ---------------------------------------------------------------------------

/// Cursor-anchored menu over author content (Round 17.1 — the
/// right-click primitive): wraps one content tree, opens the menu
/// at the cursor on `on_context_menu`, and owns open/anchor/cursor
/// state internally (cursor geometry is framework-known — forcing
/// it through author signals would re-plumb the tap point at every
/// call site, the field_press precedent).
///
/// Press-owner contract (router rule — secondary events dispatch
/// on the capture owner only): the wrapper opens for right-clicks
/// whose capture owner is the wrapper itself, so anchor content
/// should be handlerless (labels, plates, rows). Interactive
/// descendants (buttons, fields) capture themselves — they declare
/// their own `on_context_menu` and drive a controlled [`Menu`]
/// directly (same five lines as below, with their own signals):
/// read `host.last_press_position()` into an anchor signal, reset
/// a highlight signal, and set an open signal.
#[derive(Clone, Props)]
pub struct ContextMenuProps<C> {
    /// Menu rows.
    pub items: Vec<MenuItemProps>,
    /// Anchor content render (fn pointer — the M8/F6 rule; hot
    /// swaps re-resolve through the manifest like every child).
    pub content: fn(&Ctx, &C) -> VNode,
    /// Anchor content props.
    pub content_props: C,
    /// Menu width, device px.
    pub width: f32,
}

/// Cursor-anchored menu: the wrapper owns the anchor press (focus
/// for the whole session), opens at the cursor tap point, delegates
/// arrows/Enter while open, and dismisses primary wrapper taps.
/// The menu portal anchors at cursor-minus-wrapper-origin (both
/// device px — settled layout, so the box always exists by open
/// time; a pre-layout open falls back to raw cursor coords).
pub fn ContextMenu<C: Props>(ctx: &Ctx, props: &ContextMenuProps<C>) -> VNode {
    let open = ctx.signal(false);
    let anchor = ctx.signal((0.0f32, 0.0f32));
    let highlight = ctx.signal(0usize);
    let host = ctx.host();
    let inst = ctx.instance_id();
    let anchor_label = format!("context-menu-anchor-{inst}");
    // Open at the cursor (decision-301 tap facts — the Up handler
    // runs after the tap point publishes).
    let host_open = host.clone();
    let (open_h, anchor_h, highlight_h) = (open.clone(), anchor.clone(), highlight.clone());
    // Arrow delegation (focus sits on the wrapper for the whole
    // session — the list never needs to take it).
    let (items_down, highlight_down, open_down) =
        (props.items.clone(), highlight.clone(), open.clone());
    let (items_up, highlight_up, open_up) = (props.items.clone(), highlight.clone(), open.clone());
    // Enter/dismiss on the wrapper press: keyboard activation (no
    // tap point) invokes the highlight; wrapper-chrome taps dismiss
    // (menu taps route to the nearer list owner and never arrive).
    // Release handling (tap + drag) shares one rule per path — the
    // clones below feed each declaring closure (Round 21.3).
    let (items_press, highlight_press, open_press) =
        (props.items.clone(), highlight.clone(), open.clone());
    let (host_tap, items_tap, open_tap) = (host.clone(), items_press.clone(), open_press.clone());
    let (host_drag, items_drag, open_drag) =
        (host.clone(), items_press.clone(), open_press.clone());
    let content = ctx.child_auto(&props.content_props, props.content);
    // Cursor-minus-wrapper-origin into portal-local coords (both
    // device px; wrapper layout is settled by open time). The menu
    // portal mounts INSIDE the wrapper (not beside it) so the
    // offset base is exactly the wrapper origin — no layout
    // assumption about siblings.
    let (ox, oy) = find_retained_by_debug(&host, &anchor_label)
        .into_iter()
        .filter_map(|id| host.committed_box(id))
        .next()
        .map(|b| (b.x, b.y))
        .unwrap_or((0.0, 0.0));
    let (cx, cy) = anchor.get();
    let menu_props = MenuProps {
        items: props.items.clone(),
        open: open.clone(),
        anchor: (cx - ox, cy - oy),
        width: props.width,
        highlight: Some(highlight.clone()),
        anchor_focus: Some(ctx.focused()),
    };
    // Manual key 2 (not `child_auto`): the wrapper's press/drag
    // handlers address this child via `lookup_child(inst, 2)` —
    // load-bearing keys stay manual (decision 360).
    let menu = ctx.child("oppa::ContextMenuMenu", 2, &menu_props, Menu);
    Div(anchor_label.as_str())
        .on_context_menu(move || {
            if let Some((x, y)) = host_open.last_press_position() {
                anchor_h.set((x, y));
            }
            if highlight_h.get() != 0 {
                highlight_h.set(0);
            }
            open_h.set(true);
        })
        .on_key_down(move || {
            if !open_down.get() {
                return;
            }
            let next = step_highlight(&items_down, highlight_down.get(), 1);
            if next != highlight_down.get() {
                highlight_down.set(next);
            }
        })
        .on_key_up(move || {
            if !open_up.get() {
                return;
            }
            let next = step_highlight(&items_up, highlight_up.get(), -1);
            if next != highlight_up.get() {
                highlight_up.set(next);
            }
        })
        .on_press(move || {
            if !open_tap.get() {
                return;
            }
            if host_tap.last_press_position().is_none() {
                activate_highlighted(&items_tap, &highlight_press, &open_tap);
            } else if let Some((x, y)) = host_tap.last_press_position() {
                // Round 21.3 tap-release: enabled rows invoke,
                // anything else dismisses (the wrapper's rule).
                wrapper_release_row(
                    &host_tap,
                    host_tap.lookup_child(inst, 2),
                    &items_tap,
                    &open_tap,
                    x,
                    y,
                );
            }
        })
        .on_drag_release({
            move || {
                // Round 21.3 drag-select: a press-held release over
                // an enabled row invokes it and closes; anything
                // else dismisses (same rule as tap-release above;
                // slide-off releases never arrive — the router's
                // inside rule cancels them).
                if !open_drag.get() {
                    return;
                }
                if let Some((x, y)) = host_drag.last_drag_release() {
                    wrapper_release_row(
                        &host_drag,
                        host_drag.lookup_child(inst, 2),
                        &items_drag,
                        &open_drag,
                        x,
                        y,
                    );
                }
            }
        })
        .children([content, menu])
}

#[cfg(test)]
mod tests;
