//! Headless proof of the Stylebook root: the same tree every
//! `--style` mounts — no window, no platform code.

use oppa::{drag_regions_for_style, find_retained_by_debug, WindowStyle};
use oppa_testkit::Harness;
use stylebook::{style_blurb, style_title, ChromeHooks, StylebookProps, StylebookRoot};

fn mount_style(style: WindowStyle) -> Harness {
    let app = Harness::new();
    app.mount(
        "Stylebook",
        StylebookProps {
            style,
            hooks: ChromeHooks::default(),
        },
        StylebookRoot,
    );
    app
}

fn has_node(app: &Harness, debug: &str) -> bool {
    !find_retained_by_debug(app.host(), debug).is_empty()
}

/// Decision 482: every style mounts its card with a working
/// counter (the scene stays interactive under each chrome).
#[test]
fn counter_counts_under_every_style() {
    for style in [
        WindowStyle::Decorated,
        WindowStyle::Undecorated,
        WindowStyle::Transparent,
        WindowStyle::Extended,
    ] {
        let app = mount_style(style);
        assert!(has_node(&app, "stylebook-card"), "{style:?} cards");
        app.tap("button");
        app.run_idle();
        let count: Vec<_> = find_retained_by_debug(app.host(), "button")
            .into_iter()
            .filter_map(|id| app.host().committed_box(id))
            .collect();
        assert_eq!(count.len(), 1, "{style:?} keeps one counter button");
    }
}

/// Decision 482: the in-app header (with bridged caption buttons)
/// renders exactly in frameless modes — `Decorated` keeps the native
/// title bar with no duplicate chrome.
#[test]
fn header_and_caption_buttons_render_only_when_frameless() {
    for style in [
        WindowStyle::Undecorated,
        WindowStyle::Transparent,
        WindowStyle::Extended,
    ] {
        let app = mount_style(style);
        assert!(has_node(&app, oppa::HEADER_BAR_DEBUG), "{style:?} bars");
        for debug in ["header-min", "header-max", "header-close"] {
            assert!(has_node(&app, debug), "{style:?} bridges {debug}");
        }
    }
    let app = mount_style(WindowStyle::Decorated);
    assert!(
        !has_node(&app, oppa::HEADER_BAR_DEBUG),
        "decorated keeps the native bar only"
    );
    assert!(
        !has_node(&app, "header-close"),
        "decorated bridges no caption buttons"
    );
}

/// Decision 482: only `Extended` publishes an OS drag region (the
/// bar chrome minus its caption buttons) — the runners publish
/// through the one `drag_regions_for_style` policy point, so every
/// other style publishes nothing even with a header mounted.
#[test]
fn drag_region_publishes_only_in_extended() {
    for style in [
        WindowStyle::Decorated,
        WindowStyle::Undecorated,
        WindowStyle::Transparent,
    ] {
        let app = mount_style(style);
        assert!(
            drag_regions_for_style(app.host(), style).is_empty(),
            "{style:?} publishes no drag region"
        );
    }
    let app = mount_style(WindowStyle::Extended);
    assert!(
        !drag_regions_for_style(app.host(), WindowStyle::Extended).is_empty(),
        "extended publishes the bar region"
    );
}

/// Decision 482: style metadata names every variant exactly once
/// (the window title and card copy read it — no silent fallback
/// string anywhere).
#[test]
fn style_metadata_covers_every_variant() {
    assert_eq!(
        [
            style_title(WindowStyle::Decorated),
            style_title(WindowStyle::Undecorated),
            style_title(WindowStyle::Transparent),
            style_title(WindowStyle::Extended),
        ],
        ["Decorated", "Undecorated", "Transparent", "Extended"]
    );
    for style in [
        WindowStyle::Decorated,
        WindowStyle::Undecorated,
        WindowStyle::Transparent,
        WindowStyle::Extended,
    ] {
        assert!(
            !style_blurb(style).is_empty(),
            "{style:?} carries its contract line"
        );
    }
}
