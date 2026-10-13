use super::*;
use oppa::input::InputEvent;
use oppa::{ComponentHost, Ctx, CursorIcon, Props, VNode};

#[derive(Clone)]
struct LinkCase {
    props: LinkProps,
}
impl Props for LinkCase {}

fn link_render(ctx: &Ctx, p: &LinkCase) -> VNode {
    ctx.child_auto(&p.props, Link)
}

fn hover_at(host: &ComponentHost, debug: &str) {
    let id = node_by_debug(host, debug);
    let b = host.committed_box(id).expect("link has a hit box");
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Move,
        x: b.x + b.w / 2.0,
        y: b.y + b.h / 2.0,
        modifiers: oppa::Modifiers::NONE,
    });
    host.run_until_idle();
}

/// Border bands on the retained style (the underline rule —
/// paint-only inset bands, never layout).
fn has_bands(host: &ComponentHost, debug: &str) -> bool {
    let id = node_by_debug(host, debug);
    host.with_retained_mut(|rec, styles| {
        rec.get(id)
            .and_then(|n| styles.get(n.style))
            .map(|s| s.border_edges.is_some())
            .unwrap_or(false)
    })
}

#[test]
fn link_press_activates_with_button_semantics() {
    let host = ComponentHost::new();
    let fired = host.runtime().signal(false);
    let set = fired.clone();
    let props = LinkProps::new("More", move || set.set(true));
    host.mount("Lnk", LinkCase { props }, link_render);
    host.run_until_idle();
    let id = node_by_debug(&host, "link");
    let sem = host.retained_semantics(id).expect("semantics");
    assert_eq!(sem.role, oppa::Role::Button);
    assert_eq!(sem.label.as_deref(), Some("More"));
    assert!(!sem.disabled);
    press_node(&host, id);
    assert!(fired.get(), "press activates");
}

#[test]
fn link_underlines_only_on_hover_by_default() {
    let host = ComponentHost::new();
    let props = LinkProps::new("More", || {}).debug("lnk");
    host.mount("LnkH", LinkCase { props }, link_render);
    host.run_until_idle();
    assert!(!has_bands(&host, "lnk"), "at rest: no underline");
    hover_at(&host, "lnk");
    assert!(has_bands(&host, "lnk"), "hovered: underline");
    assert_eq!(
        host.hover_cursor(),
        Some(CursorIcon::Pointer),
        "hovered link shows the hand"
    );
}

#[test]
fn link_underline_modes_hold() {
    let host = ComponentHost::new();
    let props = LinkProps::new("More", || {})
        .underline(LinkUnderline::Always)
        .debug("lnk-a");
    host.mount("LnkA", LinkCase { props }, link_render);
    host.run_until_idle();
    assert!(has_bands(&host, "lnk-a"), "Always underlines at rest");
    let host = ComponentHost::new();
    let props = LinkProps::new("More", || {})
        .underline(LinkUnderline::Never)
        .debug("lnk-n");
    host.mount("LnkN", LinkCase { props }, link_render);
    host.run_until_idle();
    hover_at(&host, "lnk-n");
    assert!(!has_bands(&host, "lnk-n"), "Never never underlines");
}

#[test]
fn link_ink_defaults_to_theme_primary_and_overrides() {
    use oppa::Color;
    let host = ComponentHost::new();
    let props = LinkProps::new("More", || {}).debug("lnk-p");
    host.mount("LnkP", LinkCase { props }, link_render);
    host.run_until_idle();
    let leaf = find_retained_by_debug(&host, "text")
        .into_iter()
        .next()
        .expect("label leaf");
    let ink = host.with_retained_mut(|rec, styles| {
        rec.get(leaf)
            .and_then(|n| styles.get(n.style))
            .and_then(|s| s.ink)
    });
    assert_eq!(
        ink,
        Some(host.theme_tokens().primary),
        "default ink is theme primary, never text white"
    );
    let host = ComponentHost::new();
    let props = LinkProps::new("More", || {})
        .ink(Color(0xFF_29_74_EE))
        .debug("lnk-o");
    host.mount("LnkO", LinkCase { props }, link_render);
    host.run_until_idle();
    let leaf = find_retained_by_debug(&host, "text")
        .into_iter()
        .next()
        .expect("label leaf");
    let ink = host.with_retained_mut(|rec, styles| {
        rec.get(leaf)
            .and_then(|n| styles.get(n.style))
            .and_then(|s| s.ink)
    });
    assert_eq!(
        ink,
        Some(Color(0xFF_29_74_EE)),
        "ink override wins over the theme"
    );
}

#[test]
fn link_disabled_stays_plain() {
    let host = ComponentHost::new();
    let props = LinkProps::new("More", || {}).disabled().debug("lnk-d");
    host.mount("LnkD", LinkCase { props }, link_render);
    host.run_until_idle();
    let id = node_by_debug(&host, "lnk-d");
    assert!(
        host.retained_handlers(id).is_empty(),
        "disabled carries no handler"
    );
    assert!(
        !host.tab_order().contains(&id),
        "disabled leaves the tab order"
    );
    hover_at(&host, "lnk-d");
    assert!(!has_bands(&host, "lnk-d"), "disabled never underlines");
}
