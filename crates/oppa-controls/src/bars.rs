use crate::common::*;
use crate::menu::{Menu, MenuItemProps, MenuProps};
use oppa::*;
use oppa_macros::Props;
// ---------------------------------------------------------------------------
// Toolbar + Menubar + FilePicker (Phase 38c, G21)
// ---------------------------------------------------------------------------

/// One bar cell (shared Toolbar/Menubar recipe): label + enabled +
/// optional separator. Separators render 1px vertical rules
/// (non-interactive, skipped in nav; pointer hits land as quiet
/// no-ops — the menu dead-padding precedent, without dismissal
/// since bars never close).
#[derive(Clone)]
pub struct BarItem {
    pub label: SharedString,
    pub enabled: bool,
    pub separator: bool,
    pub on_press: Action,
}

impl BarItem {
    pub fn new(label: &str, on_press: impl Fn() + 'static) -> Self {
        Self {
            label: SharedString::from(label),
            enabled: true,
            separator: false,
            on_press: action(on_press),
        }
    }

    pub fn separator() -> Self {
        Self {
            label: SharedString::from(""),
            enabled: false,
            separator: true,
            on_press: action(|| {}),
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// One menubar title: label + the dropdown rows it opens.
#[derive(Clone)]
pub struct MenuTitle {
    pub label: SharedString,
    pub items: Vec<MenuItemProps>,
    pub enabled: bool,
}

impl MenuTitle {
    pub fn new(label: &str, items: Vec<MenuItemProps>) -> Self {
        Self {
            label: SharedString::from(label),
            items,
            enabled: true,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Shared bar-row contract (one recipe, never two spellings of the
/// same cursor — the `RowFilter`/`RowSort` shared-alias precedent).
trait BarRow {
    fn selectable(&self) -> bool;
}

impl BarRow for BarItem {
    fn selectable(&self) -> bool {
        !self.separator && self.enabled
    }
}

impl BarRow for MenuTitle {
    fn selectable(&self) -> bool {
        self.enabled && !self.items.is_empty()
    }
}

/// Effective highlight: the stored index when selectable, else the
/// next selectable forward (wrapping), else `None` (empty or
/// all-disabled — Enter no-ops, nothing paints highlighted). The
/// [`Menu`] `effective_highlight` twin for horizontal bars.
fn bar_effective<T: BarRow>(items: &[T], stored: usize) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    let mut i = stored.min(items.len() - 1);
    for _ in 0..items.len() {
        if items[i].selectable() {
            return Some(i);
        }
        i = (i + 1) % items.len();
    }
    None
}

/// One arrow step (wrapping, skipping separators and disabled
/// cells; `dir` is +1 right / −1 left). No selectable cell keeps
/// the index (never invents a highlight). The [`Menu`]
/// `step_highlight` twin for horizontal bars.
fn bar_step<T: BarRow>(items: &[T], from: usize, dir: i32) -> usize {
    if items.is_empty() {
        return 0;
    }
    let len = items.len();
    let mut i = from.min(len - 1);
    for _ in 0..len {
        i = (i as i32 + dir).rem_euclid(len as i32) as usize;
        if items[i].selectable() {
            return i;
        }
    }
    from.min(len - 1)
}

/// Cell index under a device-px point, if any (pointer path —
/// committed boxes by hit-test label, the slider-trackbox
/// precedent; separators count, gaps do not — the Menu `hit_item`
/// twin with caller-owned labels).
fn bar_hit(
    host: &oppa::ComponentHost,
    label: &dyn Fn(usize) -> String,
    len: usize,
    x: f32,
    y: f32,
) -> Option<usize> {
    for i in 0..len {
        for id in oppa::find_retained_by_debug(host, &label(i)) {
            if let Some(b) = host.committed_box(id) {
                if x >= b.x && x < b.x + b.w && y >= b.y && y < b.y + b.h {
                    return Some(i);
                }
            }
        }
    }
    None
}

/// Toolbar props: a horizontal command bar (row-based, one tab
/// stop — arrows rove, Enter activates; the Menu focus precedent:
/// routers own focus so components never move it — the container
/// holds framework focus while a highlight cursor roves the
/// cells).
#[derive(Clone, Props)]
pub struct ToolbarProps {
    pub items: Vec<BarItem>,
    pub enabled: bool,
    pub label: SharedString,
    pub cell_width: f32,
    pub height: f32,
    pub debug: SharedString,
}

impl ToolbarProps {
    pub fn new(items: Vec<BarItem>) -> Self {
        Self {
            items,
            enabled: true,
            label: SharedString::from("Toolbar"),
            cell_width: 64.0,
            height: 32.0,
            debug: SharedString::from("toolbar"),
        }
    }
}

/// Horizontal command bar over the shared recipe: cells render
/// handlerless inside a single press-owner container (focus never
/// fragments mid-gesture — the Menu-list precedent); pointer taps
/// hit-test by committed box and invoke enabled cells (separators
/// and disabled hits are quiet no-ops — bars never close, so there
/// is nothing to dismiss); Left/Right rove the highlight cursor
/// (wrapping, skipping separators/disabled); Enter/Space invokes
/// the effective highlight through the router pulse (keyboard
/// activation carries no tap point — the Menu disambiguation
/// rule). Disabled renders everything handlerless with `disabled`
/// semantics. The highlight cursor is instance-owned interaction
/// state (the pressed-flag precedent — no author signal, unlike
/// menu `open`).
pub fn Toolbar(ctx: &Ctx, props: &ToolbarProps) -> VNode {
    let t = ctx.theme().tokens();
    let highlight = ctx.signal(0usize);
    let eff = bar_effective(&props.items, highlight.get());
    // Cell labels ride the author debug only (the Slider-trackbox
    // precedent — fixed labels, sibling bars use distinct debugs).
    let prefix = props.debug.as_ref().to_string();
    let cells = props
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let debug = format!("{prefix}-cell-{i}");
            if item.separator {
                return Div(debug.as_str())
                    .style(Style::new().size(9, 20).align_items(AlignItems::Center))
                    .child(
                        Div(format!("{debug}-rule").as_str())
                            .style(Style::new().size(1, 20).bg(t.border))
                            .build(),
                    );
            }
            let hl = eff == Some(i);
            let mut text = Text::new(item.label.clone());
            if hl {
                text = text.bold();
            }
            let inked = if hl {
                with_ink(VNode::from(text), CONTRAST_INK)
            } else if !item.enabled || !props.enabled {
                with_ink(VNode::from(text), t.text_secondary)
            } else {
                VNode::from(text)
            };
            let mut style = Style::new()
                .size(props.cell_width, props.height)
                .radius(4)
                .align_items(AlignItems::Center);
            if hl {
                style = style.bg(t.primary);
            }
            Div(debug.as_str())
                .style(style)
                .semantics(
                    Semantics::button()
                        .label(&item.label)
                        .disabled(!item.enabled || !props.enabled),
                )
                .child(inked)
        })
        .collect::<Vec<_>>();
    let host = ctx.host();
    let items_ev = props.items.clone();
    let hl_ev = highlight.clone();
    let prefix_ev = prefix.clone();
    let press = move || {
        // Tap point → hit-test (roving follows the pointer);
        // keyboard pulse (no tap point) → invoke the effective
        // highlight (the Menu disambiguation rule).
        if let Some((x, y)) = host.last_press_position() {
            if let Some(i) = bar_hit(
                &host,
                &|i| format!("{prefix_ev}-cell-{i}"),
                items_ev.len(),
                x,
                y,
            ) {
                if let Some(it) = items_ev.get(i) {
                    if it.selectable() {
                        if hl_ev.get() != i {
                            hl_ev.set(i);
                        }
                        it.on_press.clone()();
                    }
                }
            }
            return;
        }
        if let Some(i) = bar_effective(&items_ev, hl_ev.get()) {
            if let Some(it) = items_ev.get(i) {
                if it.selectable() {
                    it.on_press.clone()();
                }
            }
        }
    };
    let rove = |dir: i32| {
        let (items, hl) = (props.items.clone(), highlight.clone());
        move || {
            let next = bar_step(&items, hl.get(), dir);
            if next != hl.get() {
                hl.set(next);
            }
        }
    };
    let builder = Row(&props.debug)
        .style(Style::new().gap(4).pad_x(4).align_items(AlignItems::Center))
        .semantics(
            Semantics::default()
                .label(&props.label)
                .disabled(!props.enabled),
        );
    let builder = if props.enabled {
        builder
            .on_press(press)
            .on_key_left(rove(-1))
            .on_key_right(rove(1))
    } else {
        builder
    };
    builder.children(cells)
}

/// Menubar props: a horizontal title bar (the Toolbar recipe —
/// one tab stop, Left/Right rove) where each title opens a
/// [`Menu`] dropdown (the Menu/Portal half). `open_title` is the
/// author-owned visibility truth (`Some(i)` = title `i` open —
/// the Select-`open` precedent); the highlight cursor stays
/// instance-owned (the Toolbar precedent).
#[derive(Clone, Props)]
pub struct MenubarProps {
    pub titles: Vec<MenuTitle>,
    pub open_title: Signal<Option<usize>>,
    pub enabled: bool,
    pub label: SharedString,
    pub title_width: f32,
    pub height: f32,
    pub menu_width: f32,
    pub debug: SharedString,
}

impl MenubarProps {
    pub fn new(titles: Vec<MenuTitle>, open_title: Signal<Option<usize>>) -> Self {
        Self {
            titles,
            open_title,
            enabled: true,
            label: SharedString::from("Menubar"),
            title_width: 96.0,
            height: 32.0,
            menu_width: 200.0,
            debug: SharedString::from("menubar"),
        }
    }
}

/// Horizontal menu bar: title cells (the Toolbar cell treatment —
/// handlerless visuals, one press-owner bar, hit-test activation,
/// Left/Right rove with wrapping) where each title press opens its
/// [`Menu`] dropdown in a `Portal` anchored under the title
/// (settled-box math, Menu cursor-anchored precedent). Down/Enter
/// opens the highlighted title; Left/Right while open moves the
/// open menu with the highlight (native menubar behavior); Escape
/// blurs through the router and the bar's own focus-edge rule
/// closes the menu (standalone `Menu`, Select parity — dismissal
/// writes back through an edge latch, guarded and terminating).
/// Invoking a row runs its action and closes (the Menu rule).
/// Disabled titles skip nav and never open.
pub fn Menubar(ctx: &Ctx, props: &MenubarProps) -> VNode {
    let t = ctx.theme().tokens();
    let highlight = ctx.signal(0usize);
    let eff = bar_effective(&props.titles, highlight.get());
    // Bar/title labels ride the author debug only (the
    // Slider-trackbox precedent — sibling bars use distinct
    // debugs, and tests address titles without an instance id).
    let prefix = props.debug.as_ref().to_string();
    let bar_debug = format!("{prefix}-bar");
    let cells = props
        .titles
        .iter()
        .enumerate()
        .map(|(i, title)| {
            let debug = format!("{prefix}-title-{i}");
            let hl = eff == Some(i);
            let open_here = props.open_title.get() == Some(i);
            let mut text = Text::new(title.label.clone());
            if hl || open_here {
                text = text.bold();
            }
            let inked = if hl || open_here {
                with_ink(VNode::from(text), CONTRAST_INK)
            } else if !title.enabled || !props.enabled {
                with_ink(VNode::from(text), t.text_secondary)
            } else {
                VNode::from(text)
            };
            let mut style = Style::new()
                .size(props.title_width, props.height)
                .radius(4)
                .align_items(AlignItems::Center);
            if hl || open_here {
                style = style.bg(t.primary);
            }
            Div(debug.as_str())
                .style(style)
                .semantics(
                    Semantics::button()
                        .label(&title.label)
                        .disabled(!title.enabled || !props.enabled),
                )
                .child(inked)
        })
        .collect::<Vec<_>>();
    let host = ctx.host();
    let titles_ev = props.titles.clone();
    let hl_ev = highlight.clone();
    let open_ev = props.open_title.clone();
    let prefix_ev = prefix.clone();
    let press = move || {
        if let Some((x, y)) = host.last_press_position() {
            if let Some(i) = bar_hit(
                &host,
                &|i| format!("{prefix_ev}-title-{i}"),
                titles_ev.len(),
                x,
                y,
            ) {
                if let Some(ti) = titles_ev.get(i) {
                    if ti.selectable() {
                        if hl_ev.get() != i {
                            hl_ev.set(i);
                        }
                        let cur = open_ev.get();
                        open_ev.set(if cur == Some(i) { None } else { Some(i) });
                    }
                }
            }
            return;
        }
        // Keyboard pulse: open the effective title.
        if let Some(i) = bar_effective(&titles_ev, hl_ev.get()) {
            if titles_ev.get(i).is_some_and(|ti| ti.selectable()) {
                open_ev.set(Some(i));
            }
        }
    };
    let rove = |dir: i32| {
        let (titles, hl, open) = (
            props.titles.clone(),
            highlight.clone(),
            props.open_title.clone(),
        );
        move || {
            let next = bar_step(&titles, hl.get(), dir);
            if next != hl.get() {
                hl.set(next);
            }
            // While a menu is open, the open menu follows the
            // highlight (native menubar behavior).
            if open.get().is_some() && titles.get(next).is_some_and(|t| t.selectable()) {
                open.set(Some(next));
            }
        }
    };
    let down = {
        let (titles, hl, open) = (
            props.titles.clone(),
            highlight.clone(),
            props.open_title.clone(),
        );
        move || {
            if let Some(i) = bar_effective(&titles, hl.get()) {
                if titles.get(i).is_some_and(|ti| ti.selectable()) {
                    open.set(Some(i));
                }
            }
        }
    };
    let builder = Row(bar_debug.as_str())
        .style(Style::new().gap(4).pad_x(4).align_items(AlignItems::Center))
        .semantics(
            Semantics::default()
                .label(&props.label)
                .disabled(!props.enabled),
        );
    let builder = if props.enabled {
        builder
            .on_press(press)
            .on_key_left(rove(-1))
            .on_key_right(rove(1))
            .on_key_down(down)
    } else {
        builder
    };
    let bar = builder.children(cells);
    // Open dropdown (at most one — the Tabs single-panel
    // precedent): the open title's rows in a Menu anchored under
    // the title (settled-box math, re-derived when layout settles).
    let open_idx = props.open_title.get();
    let Some(open_i) = open_idx else {
        return Div(format!("{prefix}-root").as_str()).children([bar]);
    };
    let Some(title) = props.titles.get(open_i) else {
        // Stale index past a shrink (controlled edge — the
        // unmatched-Tabs precedent, quiet empty, documented).
        return Div(format!("{prefix}-root").as_str()).children([bar]);
    };
    // Standalone dismissal (the Select-parity rule — the Menu below
    // carries `anchor_focus: None`, so the bar owns dismissal):
    // a bar-FOCUS edge out (Escape-blur, outside tap, tab-out)
    // closes the menu, unless focus moved into the open list itself
    // (Tab-in — membership read off the root, untracked; the
    // re-run rides `ctx.focused()`). Edges only, never states: a
    // preset-open menu (never focused) stays truthful to `open_title`
    // (controlled signals are never second-guessed — the Select
    // rule). The Menu stale-focus trap stays shut by construction
    // (standalone Menus never consult list focus for visibility).
    let bar_focused = ctx.focused().get();
    let was_focused = ctx.signal(false);
    let prev_focused = was_focused.get();
    if bar_focused != prev_focused {
        was_focused.set(bar_focused);
    }
    if prev_focused && !bar_focused {
        let host_now = ctx.host();
        let fnow = host_now.focused_node();
        // NOTE: single `with_retained_mut` borrow — `rec` methods
        // directly (the `find_retained_by_debug` helper borrows
        // again and would panic nested inside the mutable borrow).
        let in_list = fnow.is_some_and(|f| {
            host_now.with_retained_mut(|rec, _| {
                rec.find_by_debug(&format!("{prefix}-root"))
                    .into_iter()
                    .any(|r| oppa::is_within(rec, f, r))
            })
        });
        if !in_list {
            props.open_title.set(None);
            return Div(format!("{prefix}-root").as_str()).children([bar]);
        }
    }
    // Edge latch (guarded, terminating): `want` edges flow author
    // → menu; menu-side invocation flows back by clearing
    // `open_title` exactly once per edge (blur dismissal flows
    // through the focus-edge rule above — one writer per edge,
    // never two).
    let menu_open = ctx.signal(false);
    let last_want = ctx.signal(false);
    let want = true;
    if want != last_want.get() {
        last_want.set(want);
        menu_open.set(want);
    }
    if !menu_open.get() && want {
        props.open_title.set(None);
        last_want.set(false);
        return Div(format!("{prefix}-root").as_str()).children([bar]);
    }
    let host = ctx.host();
    let bar_box = host.settled_box_by_debug(bar_debug.as_str());
    let title_box = host.settled_box_by_debug(format!("{prefix}-title-{open_i}").as_str());
    let anchor = match (bar_box, title_box) {
        (Some(b), Some(tb)) => (tb.x - b.x, b.h),
        _ => (0.0, props.height),
    };
    let menu_props = MenuProps {
        items: title.items.clone(),
        open: menu_open.clone(),
        anchor,
        width: props.menu_width,
        highlight: None,
        // Standalone (Select parity — the bar owns dismissal, see
        // the focus-edge rule above): `open_title` is the only
        // visibility truth. Attached mode would consult the Menu
        // instance's focus flag, which goes stale-true when a menu
        // unmounts under keyboard focus (invoke path) and would pin
        // the next open menu past Escape — stated, not silent.
        anchor_focus: None,
    };
    let menu = ctx.child_auto(&menu_props, Menu);
    Div(format!("{prefix}-root").as_str()).children([bar, menu])
}
