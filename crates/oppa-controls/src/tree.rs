use crate::common::*;
use crate::virtual_list::vlist_window;
use oppa::*;
use oppa_macros::Props;
use std::rc::Rc;
// ---------------------------------------------------------------------------
// Tree (Phase 38b, G12)
// ---------------------------------------------------------------------------

/// One hierarchy node: identity + label + parent link (`None` =
/// root). Plain data over an author-owned [`Collection`](oppa::Collection)
/// (commit order = sibling order — roots and children each keep it).
#[derive(Clone, PartialEq, Debug)]
pub struct TreeNode {
    pub id: SharedString,
    pub label: SharedString,
    pub parent: Option<SharedString>,
}

impl TreeNode {
    pub fn root(id: &str, label: &str) -> Self {
        Self {
            id: SharedString::from(id),
            label: SharedString::from(label),
            parent: None,
        }
    }

    pub fn child(id: &str, label: &str, parent: &str) -> Self {
        Self {
            id: SharedString::from(id),
            label: SharedString::from(label),
            parent: Some(SharedString::from(parent)),
        }
    }
}

/// Tree props (controlled — `expanded` holds the open node ids,
/// `selected` the chosen node; both author-owned signals). The
/// hierarchy rides `nodes` (a [`Collection`](oppa::Collection) of
/// [`TreeNode`]); the visible flatten re-derives live (appends grow
/// the tree, removals collapse it — same frame, no remount).
/// `row_height` is fixed per row (variable heights stay out —
/// the Blocked prefix-sum rule); the viewport scrolls natively with
/// an attached [`Scrollbar`].
#[derive(Clone, Props)]
pub struct TreeProps {
    pub nodes: oppa::Collection<TreeNode>,
    pub expanded: Signal<Vec<SharedString>>,
    pub selected: Signal<Option<SharedString>>,
    pub label: SharedString,
    pub width: f32,
    pub height: f32,
    pub row_height: f32,
    pub overscan: usize,
    pub enabled: bool,
    pub debug: SharedString,
}

/// Flattens the hierarchy to visible `(row, depth)` pairs in
/// commit order (pure — headless-testable): roots first, each open
/// node's children (commit order) recursed. Unknown parents and
/// cycles refuse loudly (authoring bugs, never silent skips —
/// a child naming a missing parent, or a parent loop, panics).
pub fn tree_visible(
    rows: &[oppa::Row<TreeNode>],
    expanded: &[SharedString],
) -> Vec<(oppa::Row<TreeNode>, usize)> {
    use std::collections::HashSet;
    let ids: HashSet<&str> = rows.iter().map(|r| r.value.id.as_ref()).collect();
    for row in rows {
        if let Some(p) = row.value.parent.as_ref() {
            assert!(
                ids.contains(p.as_ref()),
                "tree child {:?} names missing parent {p:?} — refused, never silently dropped",
                row.value.id.to_string(),
            );
        }
    }
    fn visit(
        rows: &[oppa::Row<TreeNode>],
        parent: Option<&str>,
        expanded: &[SharedString],
        depth: usize,
        stack: &mut Vec<SharedString>,
        out: &mut Vec<(oppa::Row<TreeNode>, usize)>,
    ) {
        for row in rows
            .iter()
            .filter(|r| r.value.parent.as_ref().map(|p| p.as_ref()) == parent)
        {
            let id = row.value.id.clone();
            assert!(
                !stack.iter().any(|s| s == &id),
                "tree parent loop at {id:?} — refused, never an infinite flatten"
            );
            out.push((row.clone(), depth));
            if expanded.iter().any(|e| e == &id) {
                stack.push(id.clone());
                let parent: SharedString = id;
                visit(rows, Some(parent.as_ref()), expanded, depth + 1, stack, out);
                stack.pop();
            }
        }
    }
    let mut out = Vec::new();
    let mut stack = Vec::new();
    visit(rows, None, expanded, 0, &mut stack, &mut out);
    out
}

/// Toggles `id` in the expanded list (pure — headless-testable):
/// present → removed, absent → appended (append order is render
/// order only, never semantic).
pub fn tree_toggled(expanded: &[SharedString], id: &SharedString) -> Vec<SharedString> {
    if expanded.iter().any(|e| e == id) {
        expanded.iter().filter(|e| *e != id).cloned().collect()
    } else {
        let mut next = expanded.to_vec();
        next.push(id.clone());
        next
    }
}

/// One tree row (private — mounted per visible slot through
/// `ctx.child_keyed`, so slots recycle like [`VirtualList`] slots:
/// expanding rebinds slot content, never remounts the viewport).
#[derive(Clone)]
struct TreeRowProps {
    node: oppa::Row<TreeNode>,
    depth: usize,
    selected: bool,
    expanded: bool,
    has_children: bool,
    row_height: f32,
    width: f32,
    enabled: bool,
    on_select: Action,
    on_toggle: Action,
    nav: Rc<dyn Fn(NavDir)>,
}

/// Tree keyboard direction (one payload-less handler per direction —
/// the arrow precedent, lock #11 stands).
#[derive(Clone, Copy)]
enum NavDir {
    Up,
    Down,
    Left,
    Right,
}

impl Props for TreeRowProps {}

/// Vector chevron data (the Select-chevron precedent — real stroked
/// paths, no symbol-font coverage): right when collapsed, down when
/// open; leaves render a 12px spacer so labels align.
fn tree_chevron(expanded: bool) -> &'static str {
    if expanded {
        "M 2 4 L 6 9 L 10 4"
    } else {
        "M 4 2 L 9 6 L 4 10"
    }
}

fn TreeRow(ctx: &Ctx, props: &TreeRowProps) -> VNode {
    let t = ctx.theme().tokens();
    let semantics = Semantics::tree_item(props.selected)
        .label(&props.node.value.label)
        .disabled(!props.enabled);
    let chevron = if props.has_children {
        Path::new("tree-chevron")
            .data(tree_chevron(props.expanded))
            .stroke(t.text_primary, 2.0)
            .size(12, 12)
            .build()
    } else {
        Div("tree-spacer").style(Style::new().size(12, 12)).build()
    };
    // The chevron owns the toggle press (Enter/Space on it toggles
    // through the router pulse); the row owns select + arrows.
    let chevron = if props.enabled && props.has_children {
        let toggle = props.on_toggle.clone();
        Div("tree-chevron-hit")
            .style(Style::new().size(20, 20).align_items(AlignItems::Center))
            .on_press(move || toggle())
            .child(chevron)
    } else {
        Div("tree-chevron-hit")
            .style(Style::new().size(20, 20).align_items(AlignItems::Center))
            .child(chevron)
    };
    let mut label = Text::new(props.node.value.label.clone());
    if props.selected {
        label = label.bold();
    }
    let label_vnode = if props.selected {
        with_ink(VNode::from(label), t.primary)
    } else {
        VNode::from(label)
    };
    let row_style = Style::new()
        .size(props.width, props.row_height)
        .pad_left(props.depth as f32 * 16.0)
        .gap(4)
        .align_items(AlignItems::Center);
    let builder = Row("tree-row")
        .style(focus_ringed(ctx, props.enabled, row_style))
        .semantics(semantics);
    // All four arrows share the row press owner (handlers attach
    // pre-`child` — `.child()`/`.children()` are terminal, so every
    // handler rides the chain above the row content).
    let builder = if props.enabled {
        let select = props.on_select.clone();
        let nav_u = props.nav.clone();
        let nav_d = props.nav.clone();
        let nav_l = props.nav.clone();
        let nav_r = props.nav.clone();
        builder
            .on_press(move || select())
            .on_key_up(move || nav_u(NavDir::Up))
            .on_key_down(move || nav_d(NavDir::Down))
            .on_key_left(move || nav_l(NavDir::Left))
            .on_key_right(move || nav_r(NavDir::Right))
    } else {
        builder
    };
    builder.children([chevron, label_vnode])
}

/// Hierarchy view over a [`Collection`](oppa::Collection): visible
/// rows flatten per render ([`tree_visible`]), window through the
/// shared [`vlist_window`] math (binary-searched tops, overscan —
/// the [`VirtualList`] precedent, fixed `row_height`), and recycle
/// per-slot instances (`child_keyed` on the slot — expanding
/// rebinds content, never remounts).
///
/// Interaction: row press selects (Enter/Space pulse it through the
/// router); chevron press toggles expansion; arrows on the focused
/// row move selection (Up/Down to the visible neighbors), collapse
/// / ascend (Left: collapse when open, else select the parent),
/// expand / descend (Right: expand when closed with children, else
/// select the first child). Ends and leaves are quiet no-ops
/// (controlled-contract edges, documented not silent). Disabled
/// renders everything handlerless with `disabled` semantics.
pub fn Tree(ctx: &Ctx, props: &TreeProps) -> VNode {
    let offset = ctx.scroll_offset();
    let y = offset.get();
    let page = props.nodes.query(&oppa::CollectionQuery {
        filter: None,
        sort: None,
        offset: 0,
        limit: None,
    });
    let expanded = props.expanded.get();
    let selected = props.selected.get();
    let visible = tree_visible(&page.rows, &expanded);
    let n = visible.len();
    let rh = props.row_height;
    assert!(
        rh.is_finite() && rh > 0.0,
        "tree row_height must be finite positive, got {rh} — refused, never a silent collapse"
    );
    let mut tops = Vec::with_capacity(n + 1);
    let mut cursor = 0.0f32;
    for _ in 0..n {
        tops.push(cursor);
        cursor += rh;
    }
    let total = cursor;
    let (first, end) = vlist_window(&tops, total, y, props.height, props.overscan);
    let width = props.width;
    let children = visible[first..end]
        .iter()
        .enumerate()
        .map(|(slot, (row, depth))| {
            let idx = first + slot;
            let id = row.value.id.clone();
            let has_children = page
                .rows
                .iter()
                .any(|r| r.value.parent.as_ref() == Some(&id));
            let is_open = expanded.iter().any(|e| e == &id);
            let is_sel = selected.as_ref() == Some(&id);
            // Event-time nav (recomputes the visible list from live
            // signals — the Slider fresh-geometry precedent, so keys
            // never act on a stale flatten).
            let nav_nodes = props.nodes.clone();
            let nav_expanded = props.expanded.clone();
            let nav_selected = props.selected.clone();
            let nav = Rc::new(move |dir: NavDir| {
                let page = nav_nodes.query(&oppa::CollectionQuery {
                    filter: None,
                    sort: None,
                    offset: 0,
                    limit: None,
                });
                let exp = nav_expanded.get();
                let vis = tree_visible(&page.rows, &exp);
                let sel = nav_selected.get();
                let at = vis
                    .iter()
                    .position(|(r, _)| Some(&r.value.id) == sel.as_ref());
                match dir {
                    NavDir::Up => {
                        if let Some(i) = at {
                            if i > 0 {
                                nav_selected.set(Some(vis[i - 1].0.value.id.clone()));
                            }
                        } else if !vis.is_empty() {
                            nav_selected.set(Some(vis[0].0.value.id.clone()));
                        }
                    }
                    NavDir::Down => {
                        if let Some(i) = at {
                            if i + 1 < vis.len() {
                                nav_selected.set(Some(vis[i + 1].0.value.id.clone()));
                            }
                        } else if !vis.is_empty() {
                            nav_selected.set(Some(vis[0].0.value.id.clone()));
                        }
                    }
                    NavDir::Left => {
                        let Some(i) = at else { return };
                        let (row, _) = &vis[i];
                        if exp.iter().any(|e| e == &row.value.id) {
                            nav_expanded.set(tree_toggled(&exp, &row.value.id));
                        } else if let Some(p) = row.value.parent.clone() {
                            nav_selected.set(Some(p));
                        }
                    }
                    NavDir::Right => {
                        let Some(i) = at else { return };
                        let (row, _) = &vis[i];
                        let kids = page
                            .rows
                            .iter()
                            .any(|r| r.value.parent.as_ref() == Some(&row.value.id));
                        if kids && !exp.iter().any(|e| e == &row.value.id) {
                            nav_expanded.set(tree_toggled(&exp, &row.value.id));
                        } else if kids {
                            let first_kid = page
                                .rows
                                .iter()
                                .find(|r| r.value.parent.as_ref() == Some(&row.value.id));
                            if let Some(k) = first_kid {
                                nav_selected.set(Some(k.value.id.clone()));
                            }
                        }
                    }
                }
            });
            let sel_id = id.clone();
            let sel_sig = props.selected.clone();
            let exp_sig = props.expanded.clone();
            let exp_list = expanded.clone();
            let row_props = TreeRowProps {
                node: row.clone(),
                depth: *depth,
                selected: is_sel,
                expanded: is_open,
                has_children,
                row_height: rh,
                width,
                enabled: props.enabled,
                on_select: action(move || sel_sig.set(Some(sel_id.clone()))),
                on_toggle: action(move || exp_sig.set(tree_toggled(&exp_list, &id))),
                nav,
            };
            Div("tree-slot")
                .style(
                    Style::new()
                        .absolute_y(tops[idx] - y)
                        .h(rh)
                        .w(width)
                        .build(),
                )
                .key(slot as u64)
                .child(ctx.child_keyed(slot as u64, &row_props, TreeRow))
        })
        .collect::<Vec<_>>();
    let area = oppa::ScrollArea(&props.debug)
        .style(
            Style::new()
                .size(width, props.height)
                .content_size(total)
                .build(),
        )
        .semantics(
            Semantics::tree()
                .label(&props.label)
                .disabled(!props.enabled),
        )
        .on_scroll(|| {})
        .children(children);
    // Disabled rows render handlerless inside (TreeRow gates on
    // enabled — decision 213); the scroll container itself always
    // scrolls (a viewport, not a control).
    area
}
