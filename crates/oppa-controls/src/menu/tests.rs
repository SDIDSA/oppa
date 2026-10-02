use super::*;
use oppa::input::{keys, InputEvent, KeyState};
use oppa::{ComponentHost, Modifiers, PointerAction, PointerButton};

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

/// Handlerless anchor content (the wrapper must own presses
/// under the cursor — routing dispatches secondary events on
/// the capture owner only, so an interactive descendant would
/// capture instead; interactive anchors declare their own
/// `on_context_menu` and drive a controlled [`Menu`], see its
/// docs).
#[derive(Clone)]
struct AnchorProps;
impl Props for AnchorProps {}

fn anchor_content(_ctx: &Ctx, _p: &AnchorProps) -> VNode {
    Div("anchor-content")
        .style(Style::new().size(200, 100))
        .child(VNode::from(Text::new(SharedString::from("file.txt"))))
}

/// Screen root: an outside press owner around the menu (taps
/// outside the anchor must land on a live owner to move focus
/// — the blur edge owns dismissal).
#[derive(Clone)]
struct ScreenProps {
    fired: Vec<Signal<bool>>,
}
impl Props for ScreenProps {}

fn menu_items(fired: &[Signal<bool>]) -> Vec<MenuItemProps> {
    vec![
        MenuItemProps::new("Cut", {
            let f = fired[0].clone();
            move || f.set(true)
        }),
        MenuItemProps::new("Copy", {
            let f = fired[1].clone();
            move || f.set(true)
        })
        .disabled(),
        MenuItemProps::separator(),
        MenuItemProps::new("Paste", {
            let f = fired[3].clone();
            move || f.set(true)
        }),
    ]
}

fn screen_content(ctx: &Ctx, p: &ScreenProps) -> VNode {
    let cm = ContextMenuProps {
        items: menu_items(&p.fired),
        content: anchor_content,
        content_props: AnchorProps,
        width: 160.0,
    };
    Div("screen")
        .style(Style::new().size(400, 300))
        .on_press(|| {})
        .child(ctx.child("oppa::ScreenMenu", 1, &cm, ContextMenu))
}

/// Mounts one context menu over screen content; returns the
/// host, one invoked-flag per item, and the menu instance id
/// behind the hit-test labels.
fn menu_harness() -> (ComponentHost, Vec<Signal<bool>>, u64) {
    menu_harness_on(ComponentHost::new())
}

/// Same mount on a caller-owned host (clocked hosts drive slow
/// drag-selects deterministically).
fn menu_harness_on(host: ComponentHost) -> (ComponentHost, Vec<Signal<bool>>, u64) {
    host.set_viewport(400.0, 300.0);
    let rt = host.runtime();
    let fired: Vec<Signal<bool>> = (0..4).map(|_| rt.signal(false)).collect();
    let handle = host.mount(
        "S",
        ScreenProps {
            fired: fired.clone(),
        },
        screen_content,
    );
    host.run_until_idle();
    // Menu instance id behind the hit-test labels (screen key
    // 1 → context menu, its key 2 → menu).
    let cm = host
        .lookup_child(handle.root_instance(), 1)
        .expect("context menu child instance");
    let menu = host.lookup_child(cm, 2).expect("menu child instance");
    (host, fired, menu)
}

fn right_click(host: &ComponentHost, x: f32, y: f32) {
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: PointerAction::Down {
            button: PointerButton::Secondary,
        },
        x,
        y,
        modifiers: Modifiers::NONE,
    });
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: PointerAction::Up {
            button: PointerButton::Secondary,
        },
        x,
        y,
        modifiers: Modifiers::NONE,
    });
    host.run_until_idle();
}

fn tap(host: &ComponentHost, x: f32, y: f32) {
    host.inject_input(InputEvent::pointer_down(x, y));
    host.inject_input(InputEvent::pointer_up(x, y));
    host.run_until_idle();
}

fn hover_at(host: &ComponentHost, x: f32, y: f32) {
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: PointerAction::Move,
        x,
        y,
        modifiers: Modifiers::NONE,
    });
    host.run_until_idle();
}

fn press_down(host: &ComponentHost, x: f32, y: f32) {
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: PointerAction::Down {
            button: PointerButton::Primary,
        },
        x,
        y,
        modifiers: Modifiers::NONE,
    });
    host.run_until_idle();
}

fn move_to(host: &ComponentHost, x: f32, y: f32) {
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: PointerAction::Move,
        x,
        y,
        modifiers: Modifiers::NONE,
    });
    host.run_until_idle();
}

fn release(host: &ComponentHost, x: f32, y: f32) {
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: PointerAction::Up {
            button: PointerButton::Primary,
        },
        x,
        y,
        modifiers: Modifiers::NONE,
    });
    host.run_until_idle();
}

/// Center of row `i` in menu instance `inst` (device px).
fn row_center(host: &ComponentHost, inst: u64, i: usize) -> (f32, f32) {
    let label = format!("menu-item-{inst}-{i}");
    let id = find_retained_by_debug(host, &label)
        .into_iter()
        .next()
        .expect("row node");
    let b = host.committed_box(id).expect("row box");
    (b.x + b.w / 2.0, b.y + b.h / 2.0)
}

fn key(host: &ComponentHost, code: u32) {
    host.inject_input(InputEvent::key(code, KeyState::Pressed));
    host.run_until_idle();
}

fn popup_present(host: &ComponentHost, inst: u64) -> bool {
    // NOTE: portals keep their birth debug label through content
    // updates (reconciler never rewrites `debug`), so presence
    // reads the list plate, not the portal.
    !find_retained_by_debug(host, &format!("menu-list-{inst}")).is_empty()
}

/// Round 17.1: right-click mounts the menu at the cursor.
#[test]
fn context_menu_mounts_at_cursor() {
    let (host, _, inst) = menu_harness();
    assert!(!popup_present(&host, inst), "closed until right-clicked");
    right_click(&host, 50.0, 60.0);
    let list = find_retained_by_debug(&host, &format!("menu-list-{inst}"));
    assert_eq!(list.len(), 1, "one list plate mounted");
    let b = host.committed_box(list[0]).expect("laid list");
    assert!(
        approx(b.x, 50.0) && approx(b.y, 60.0),
        "plate anchors at the cursor, got ({}, {})",
        b.x,
        b.y
    );
}

/// Round 17.1: Enter invokes the highlighted (first) item and
/// closes.
#[test]
fn enter_activates_highlighted_item() {
    let (host, fired, inst) = menu_harness();
    right_click(&host, 50.0, 60.0);
    assert!(popup_present(&host, inst));
    key(&host, keys::ENTER);
    assert!(fired[0].get(), "first item invoked");
    assert!(fired[1..].iter().all(|f| !f.get()), "nothing else invoked");
    assert!(!popup_present(&host, inst), "invoke closes");
}

/// Round 17.1: arrows move the cursor (skipping the disabled
/// row and the separator, wrapping at the ends) and Enter
/// invokes wherever it stands.
#[test]
fn arrows_move_highlight_and_enter_invokes() {
    let (host, fired, _) = menu_harness();
    // Down from Cut skips disabled Copy + separator → Paste.
    right_click(&host, 50.0, 60.0);
    key(&host, keys::DOWN);
    key(&host, keys::ENTER);
    assert!(fired[3].get(), "Paste invoked after one Down");
    assert!(!fired[0].get());
    // Up from Cut wraps to Paste.
    let (host, fired, _) = menu_harness();
    right_click(&host, 50.0, 60.0);
    key(&host, keys::UP);
    key(&host, keys::ENTER);
    assert!(fired[3].get(), "Up wraps to Paste");
}

/// Round 17.1: Escape dismisses without invoking.
#[test]
fn escape_dismisses_without_invoking() {
    let (host, fired, inst) = menu_harness();
    right_click(&host, 50.0, 60.0);
    assert!(popup_present(&host, inst));
    key(&host, keys::ESCAPE);
    assert!(!popup_present(&host, inst), "Escape closes");
    assert!(fired.iter().all(|f| !f.get()), "dismissal invokes nothing");
}

/// Round 17.1: a primary tap outside dismisses without invoking
/// (focus leaves the menu — the one blur edge).
#[test]
fn outside_click_dismisses_without_invoking() {
    let (host, fired, inst) = menu_harness();
    right_click(&host, 50.0, 60.0);
    assert!(popup_present(&host, inst));
    tap(&host, 300.0, 250.0);
    assert!(!popup_present(&host, inst), "outside tap closes");
    assert!(fired.iter().all(|f| !f.get()), "dismissal invokes nothing");
}

/// Round 17.1: tapping a disabled row invokes nothing and keeps
/// the menu open (native no-op).
#[test]
fn disabled_row_ignores_taps_and_stays_open() {
    let (host, fired, inst) = menu_harness();
    right_click(&host, 50.0, 60.0);
    let label = format!("menu-item-{inst}-1");
    let id = find_retained_by_debug(&host, &label)
        .into_iter()
        .next()
        .expect("disabled row node");
    let b = host.committed_box(id).expect("disabled row box");
    tap(&host, b.x + b.w / 2.0, b.y + b.h / 2.0);
    assert!(
        fired.iter().all(|f| !f.get()),
        "disabled tap invokes nothing"
    );
    assert!(popup_present(&host, inst), "menu stays open");
}

/// Round 17.1: tapping an enabled row invokes it and closes.
#[test]
fn row_tap_invokes_and_closes() {
    let (host, fired, inst) = menu_harness();
    right_click(&host, 50.0, 60.0);
    let label = format!("menu-item-{inst}-3");
    let id = find_retained_by_debug(&host, &label)
        .into_iter()
        .next()
        .expect("paste row node");
    let b = host.committed_box(id).expect("paste row box");
    tap(&host, b.x + b.w / 2.0, b.y + b.h / 2.0);
    assert!(fired[3].get(), "Paste invoked by tap");
    assert!(!popup_present(&host, inst), "tap closes");
}

/// Round 17.1: a standalone dropdown menu is fully author
/// controlled — `open` shows at the given anchor, a row tap
/// invokes and clears it, and blur never auto-dismisses
/// (Select parity, documented).
#[test]
fn standalone_menu_is_author_controlled() {
    let host = ComponentHost::new();
    host.set_viewport(400.0, 300.0);
    let rt = host.runtime();
    let invoked = rt.signal(false);
    let open = rt.signal(false);
    let items = vec![MenuItemProps::new("Go", {
        let invoked = invoked.clone();
        move || invoked.set(true)
    })];
    let handle = host.mount(
        "D",
        MenuProps {
            items,
            open: open.clone(),
            anchor: (20.0, 30.0),
            width: 160.0,
            highlight: None,
            anchor_focus: None,
        },
        Menu,
    );
    host.run_until_idle();
    // The mount root IS the menu instance, so hit-test labels
    // hang off it.
    let inst = handle.root_instance();
    assert!(!popup_present(&host, inst), "controlled closed");
    open.set(true);
    host.run_until_idle();
    let list = find_retained_by_debug(&host, &format!("menu-list-{inst}"));
    assert_eq!(list.len(), 1, "author open shows");
    let b = host.committed_box(list[0]).expect("laid list");
    assert!(
        approx(b.x, 20.0) && approx(b.y, 30.0),
        "plate anchors where told, got ({}, {})",
        b.x,
        b.y
    );
    // Blur never auto-dismisses standalone menus.
    key(&host, keys::ESCAPE);
    assert!(popup_present(&host, inst), "standalone survives Escape");
    // Row tap invokes and clears `open` (Menu writes the
    // controlled signal like Select does).
    let label = format!("menu-item-{inst}-0");
    let id = find_retained_by_debug(&host, &label)
        .into_iter()
        .next()
        .expect("menu row node");
    let b = host.committed_box(id).expect("menu row box");
    tap(&host, b.x + b.w / 2.0, b.y + b.h / 2.0);
    assert!(invoked.get(), "row tap invokes");
    assert!(!open.get(), "tap clears the controlled signal");
    assert!(!popup_present(&host, inst), "tap closes");
}

// Pure nav edges: empty, all-disabled, wrap, clamp.
#[test]
fn highlight_math_covers_edges() {
    use super::{effective_highlight, step_highlight};
    let on = || MenuItemProps::new("x", || {});
    let off = || MenuItemProps::new("x", || {}).disabled();
    let sep = MenuItemProps::separator;
    assert_eq!(effective_highlight(&[], 0), None);
    assert_eq!(step_highlight(&[], 0, 1), 0);
    assert_eq!(effective_highlight(&[off(), off()], 0), None);
    assert_eq!(step_highlight(&[off(), off()], 0, 1), 0);
    // Stored-beyond-end clamps, then seeks forward.
    let items = vec![off(), on(), sep(), on()];
    assert_eq!(effective_highlight(&items, 99), Some(3));
    assert_eq!(effective_highlight(&items, 2), Some(3));
    // Wrap both directions, skipping the unselectable.
    assert_eq!(step_highlight(&items, 3, 1), 1);
    assert_eq!(step_highlight(&items, 1, -1), 3);
    assert_eq!(step_highlight(&items, 0, -1), 3);
    assert_eq!(step_highlight(&items, 3, -1), 1);
}

/// Round 21.3: pointer hover moves the same highlight the
/// arrows drive (Enter invokes wherever the cursor stands);
/// hovering a disabled row never steals it.
#[test]
fn hover_highlight_follows_pointer_and_unifies_with_arrows() {
    let (host, fired, inst) = menu_harness();
    right_click(&host, 50.0, 60.0);
    // Hover Paste (row 3): Enter invokes it with no arrow keys.
    let (px, py) = row_center(&host, inst, 3);
    hover_at(&host, px, py);
    key(&host, keys::ENTER);
    assert!(fired[3].get(), "hover-highlighted Paste invokes on Enter");
    assert!(!fired[0].get());
    // Hover the disabled Copy (row 1): the cursor does not move
    // there — Enter still invokes Cut (row 0).
    let (host, fired, inst) = menu_harness();
    right_click(&host, 50.0, 60.0);
    let (cx, cy) = row_center(&host, inst, 1);
    hover_at(&host, cx, cy);
    key(&host, keys::ENTER);
    assert!(fired[0].get(), "disabled hover never steals the cursor");
    assert!(!fired[3].get());
}

/// Round 21.3: a menu spawned 5px from the corner flips inside
/// the viewport (standalone plate here; `ContextMenu` shares
/// the same `Menu` child, so the same clamp applies).
#[test]
fn menu_clamps_near_viewport_edges() {
    let host = ComponentHost::new();
    host.set_viewport(400.0, 300.0);
    let rt = host.runtime();
    let open = rt.signal(false);
    let items = vec![
        MenuItemProps::new("One", || {}),
        MenuItemProps::new("Two", || {}),
        MenuItemProps::new("Three", || {}),
    ];
    let handle = host.mount(
        "D",
        MenuProps {
            items,
            open: open.clone(),
            anchor: (395.0, 295.0),
            width: 160.0,
            highlight: None,
            anchor_focus: None,
        },
        Menu,
    );
    host.run_until_idle();
    let inst = handle.root_instance();
    assert!(!popup_present(&host, inst), "controlled closed");
    open.set(true);
    host.run_until_idle();
    let list = find_retained_by_debug(&host, &format!("menu-list-{inst}"))
        .into_iter()
        .next()
        .expect("clamped plate");
    let b = host.committed_box(list).expect("laid plate");
    assert!(
        b.x >= 0.0 && b.y >= 0.0 && b.x + b.w <= 400.0 && b.y + b.h <= 300.0,
        "plate flips inside the viewport, got ({}, {}) {}x{}",
        b.x,
        b.y,
        b.w,
        b.h
    );
}

/// Round 21.3: press-drag-release into an open menu invokes the
/// release row and closes — through the wrapper owner (press
/// starts on wrapper chrome) and through the list owner
/// (standalone press starts on the list itself).
#[test]
fn press_drag_release_invokes_row_and_closes() {
    // Wrapper path: open, press wrapper chrome at (10,10),
    // drag onto Paste, release. Slow (clocked past the swipe
    // window — a fast far lift is a swipe, quiet by taxonomy;
    // drag-select is the deliberate kind).
    use std::rc::Rc;
    let clock = Rc::new(oppa::MockClock::new());
    let (host, fired, inst) = menu_harness_on(ComponentHost::with_clock(clock.clone()));
    right_click(&host, 50.0, 60.0);
    assert!(popup_present(&host, inst));
    let (px, py) = row_center(&host, inst, 3);
    press_down(&host, 10.0, 10.0);
    move_to(&host, px, py);
    clock.advance(0.6);
    release(&host, px, py);
    assert!(fired[3].get(), "drag-release invokes Paste");
    assert!(!popup_present(&host, inst), "invoke closes");
    // List path: standalone two-row menu, Down on row 0, slow
    // drag to row 1, Up invokes row 1 only (same swipe-window
    // rule as above).
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(400.0, 300.0);
    let rt = host.runtime();
    let f0 = rt.signal(false);
    let f1 = rt.signal(false);
    let open = rt.signal(false);
    let mk = |f: Signal<bool>, label: &str| {
        let ff = f.clone();
        MenuItemProps::new(label, move || ff.set(true))
    };
    let handle = host.mount(
        "D2",
        MenuProps {
            items: vec![mk(f0.clone(), "A"), mk(f1.clone(), "B")],
            open: open.clone(),
            anchor: (20.0, 30.0),
            width: 160.0,
            highlight: None,
            anchor_focus: None,
        },
        Menu,
    );
    host.run_until_idle();
    let inst = handle.root_instance();
    open.set(true);
    host.run_until_idle();
    let (ax, ay) = row_center(&host, inst, 0);
    let (bx, by) = row_center(&host, inst, 1);
    press_down(&host, ax, ay);
    move_to(&host, bx, by);
    clock.advance(0.6);
    release(&host, bx, by);
    assert!(!f0.get(), "dragged-off row stays quiet");
    assert!(f1.get(), "release row invokes");
    assert!(!popup_present(&host, inst), "invoke closes");
}
