//! The Stylebook root, written once (no `cfg`, no platform imports).

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use oppa::{
    AlignItems, Column, Ctx, Div, JustifyContent, Props, Row, SharedString, Signal, Style, Text,
    VNode, WindowStyle,
};
use oppa_controls::{Button, ButtonProps, HeaderBar, HeaderBarProps};

/// Display name per style (the window title reuses it).
pub fn style_title(style: WindowStyle) -> &'static str {
    match style {
        WindowStyle::Decorated => "Decorated",
        WindowStyle::Undecorated => "Undecorated",
        WindowStyle::Transparent => "Transparent",
        WindowStyle::Extended => "Extended",
    }
}

/// One-line contract per style (what the tester verifies by eye).
pub fn style_blurb(style: WindowStyle) -> &'static str {
    match style {
        WindowStyle::Decorated => {
            "System title bar, borders and caption buttons. The standard OS window."
        }
        WindowStyle::Undecorated => {
            "Frameless with a solid background. No title bar — the borders still resize."
        }
        WindowStyle::Transparent => {
            "Frameless and see-through: the desktop shows around this floating card."
        }
        WindowStyle::Extended => {
            "Frameless but OS-managed: drag the bar above, snap and tile like a native window."
        }
    }
}

/// Move hint per style (decision 482 shape: only `Extended`
/// publishes an OS drag region — every other style moves through
/// the system menu, and the card says so instead of leaving a
/// dead-feeling window).
pub fn style_hint(style: WindowStyle) -> &'static str {
    match style {
        WindowStyle::Extended => {
            "drag the bar · double-click it to maximize · right-click it for the system menu"
        }
        _ => "this style never drags — Alt+Space, then M, then arrow keys moves it",
    }
}

/// Relaunch spelling per style (the card footer names the exact
/// command running it).
pub fn style_arg(style: WindowStyle) -> &'static str {
    match style {
        WindowStyle::Decorated => "decorated",
        WindowStyle::Undecorated => "undecorated",
        WindowStyle::Transparent => "transparent",
        WindowStyle::Extended => "extended",
    }
}

/// Runner-visible caption outbox (the studio `StudioHooks`
/// publish-once precedent — signals are runtime-bound, so the root
/// publishes them on first render and the desktop poll hook drains
/// them into the loop-installed `WindowControl`).
#[derive(Clone, Default)]
pub struct ChromeHooks {
    inner: Rc<RefCell<Option<ChromeInner>>>,
}

#[derive(Clone)]
struct ChromeInner {
    minimize: Signal<bool>,
    maximize: Signal<bool>,
    close: Signal<bool>,
}

impl ChromeHooks {
    fn publish(&self, inner: ChromeInner) {
        self.inner.borrow_mut().replace(inner);
    }

    /// Reads the published handles (`None` before first render).
    pub fn get(&self) -> Option<(Signal<bool>, Signal<bool>, Signal<bool>)> {
        self.inner
            .borrow()
            .clone()
            .map(|i| (i.minimize, i.maximize, i.close))
    }
}

/// Stylebook props: the launch style (desktop argv selects it; web
/// and Android mount `Decorated`) plus the caption outbox.
#[derive(Clone)]
pub struct StylebookProps {
    pub style: WindowStyle,
    pub hooks: ChromeHooks,
}

impl Props for StylebookProps {}

/// One elegant root under every style: an in-app `HeaderBar` where
/// the OS provides none (frameless modes — `Decorated` keeps the
/// native title bar with no duplicate chrome), then a centered card
/// with the style name, its contract, a counter, and the relaunch
/// hint. The tree paints no page background (the surface clear owns
/// it — opaque theme color, or the desktop itself on Transparent).
pub fn StylebookRoot(ctx: &Ctx, props: &StylebookProps) -> VNode {
    let style = props.style;
    // Level-triggered caption requests (drained exactly once by the
    // desktop poll hook — the studio `exit_requested` shape).
    let minimize = ctx.signal(false);
    let maximize = ctx.signal(false);
    let close = ctx.signal(false);
    props.hooks.publish(ChromeInner {
        minimize: minimize.clone(),
        maximize: maximize.clone(),
        close: close.clone(),
    });
    let clicks = ctx.signal(0u32);
    let bump = clicks.clone();
    let (m_min, m_max, m_close) = (minimize.clone(), maximize.clone(), close.clone());

    let card = Div("stylebook-card")
        .style(
            Style::new()
                .w(420)
                .bg(ctx.theme().tokens().surface)
                .radius(14)
                .pad_x(28)
                .pad_top(24)
                .pad_bottom(24),
        )
        .children([
            VNode::from(Text {
                text: SharedString::from("WINDOW STYLE"),
                style: Text::body_secondary,
            }),
            VNode::from(Text::new(style_title(style)).size(26).bold()),
            VNode::from(Text {
                text: SharedString::from(style_blurb(style)),
                style: Text::body_secondary,
            }),
            Row("stylebook-counter-row")
                .style(Style::new().align_items(AlignItems::Center).gap(12))
                .children([
                    VNode::from(Text::new(format!("{} presses", clicks.get())).size(16)),
                    ctx.child_auto(
                        &ButtonProps::new("Count up", move || bump.set(bump.get() + 1)),
                        Button,
                    ),
                ]),
            VNode::from(Text {
                text: SharedString::from(format!(
                    "cargo run -p stylebook -- --style {}",
                    style_arg(style)
                )),
                style: Text::body_secondary,
            }),
            VNode::from(Text {
                text: SharedString::from(style_hint(style)),
                style: Text::body_secondary,
            }),
        ]);

    // A viewport-tall root (decision 476) centers the card at any
    // window size; the header docks above it in frameless modes.
    let mut page: Vec<VNode> = Vec::new();
    if style.is_frameless() {
        let header = HeaderBarProps::new(&format!("Stylebook — {}", style_title(style)))
            .on_minimize(move || m_min.set(true))
            .on_maximize(move || m_max.set(true))
            .on_close(move || m_close.set(true));
        page.push(ctx.child("stylebook::header", 1, &header, HeaderBar));
    }
    page.push(
        Row("stylebook-center")
            .style(
                Style::new()
                    .fill_width()
                    .flex_grow(1)
                    .align_items(AlignItems::Center)
                    .justify_content(JustifyContent::Center),
            )
            .child(card),
    );
    Column::new()
        .style(
            Style::new()
                .fill_width()
                .fill_height()
                .align_items(AlignItems::Center),
        )
        .children(page)
}
