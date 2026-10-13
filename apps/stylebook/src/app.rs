//! The Stylebook root, written once (no `cfg`, no platform imports).

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use oppa::{
    AlignItems, Column, Ctx, Div, JustifyContent, Props, Row, SharedString, Signal, Style, Text,
    VNode, WindowStyle,
};
use oppa_controls::{
    Button, ButtonProps, CaptionButtons, CaptionButtonsProps, HeaderBar, HeaderBarProps,
};

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
/// dead-feeling window; decision 494: `Undecorated` is a bare popup
/// with no system menu at all, so the taskbar owns it).
pub fn style_hint(style: WindowStyle) -> &'static str {
    match style {
        WindowStyle::Extended => {
            "drag the bar · double-click it to maximize · right-click it for the system menu"
        }
        WindowStyle::Undecorated => {
            "bare rectangle, no chrome — move and close it from the taskbar"
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

const STYLE_NAMES: &str = "decorated|undecorated|transparent|extended";

/// Parses one `--style=` value (pure — headless-testable): `None`
/// (flag absent) means `Extended`. Anything else is a loud `Err`
/// naming the valid spellings (never a silent default — an unknown
/// chrome must not boot looking like a choice).
pub fn parse_style_arg(arg: Option<&str>) -> Result<WindowStyle, String> {
    match arg {
        None => Ok(WindowStyle::Extended),
        Some("decorated") => Ok(WindowStyle::Decorated),
        Some("undecorated") => Ok(WindowStyle::Undecorated),
        Some("transparent") => Ok(WindowStyle::Transparent),
        Some("extended") => Ok(WindowStyle::Extended),
        Some(other) => Err(format!(
            "stylebook: unknown style {other:?} (expected {STYLE_NAMES})"
        )),
    }
}

/// Reads the `--native-buttons` opt-in (pure — headless-testable):
/// present declares the native caption buttons in `Extended` mode
/// (decision 485 — the OS answers the zones first on Windows, the
/// same buttons bridge on Linux); absent draws no buttons at all
/// (drag-only bar — the native-off shape).
pub fn parse_native_buttons_flag(args: &[String]) -> bool {
    args.iter().any(|a| a == "--native-buttons")
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
/// and Android mount `Decorated`), whether the native caption
/// buttons are declared in `Extended` mode (decision 485
/// `--native-buttons` demo — the card footer names the live mode;
/// off means a buttonless drag-only bar), plus the caption outbox.
#[derive(Clone)]
pub struct StylebookProps {
    pub style: WindowStyle,
    pub native: bool,
    pub hooks: ChromeHooks,
}

impl Props for StylebookProps {}

/// One elegant root under every style: custom `HeaderBar` chrome
/// where the OS provides none (`Extended` only — `Decorated` keeps
/// the native title bar with no duplicate chrome, and
/// `Undecorated`/`Transparent` stay bare rectangles), then a
/// centered card with the style name, its contract, a counter, and
/// the relaunch hint. The tree paints no page background (the
/// surface clear owns it — opaque theme color, or the desktop
/// itself on Transparent).
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
                    "cargo run -p stylebook -- --style {}{}",
                    style_arg(style),
                    if props.native {
                        " --native-buttons"
                    } else {
                        ""
                    },
                )),
                style: Text::body_secondary,
            }),
            VNode::from(Text {
                text: SharedString::from(style_hint(style)),
                style: Text::body_secondary,
            }),
            VNode::from(Text {
                text: SharedString::from(if props.native {
                    "caption: native OS zones (fallback wired)"
                } else {
                    "caption: none (drag-only bar)"
                }),
                style: Text::body_secondary,
            }),
        ]);

    // A viewport-tall root (decision 476) centers the card at any
    // window size; the custom chrome docks above it in `Extended`
    // mode only (decision 494 — every other style owns no in-app
    // chrome at all).
    let mut page: Vec<VNode> = Vec::new();
    if style == WindowStyle::Extended {
        let declare_buttons = props.native;
        let buttons = CaptionButtonsProps::new()
            .on_minimize(move || m_min.set(true))
            .on_maximize(move || m_max.set(true))
            .on_close(move || m_close.set(true));
        let title = format!("Stylebook — {}", style_title(style));
        let header = HeaderBarProps::new().content(move |ctx| {
            let surface = ctx.theme().tokens().surface;
            let title_node = VNode::from(Text::new(title.clone()).size(13));
            if declare_buttons {
                Row("stylebook-chrome")
                    .style(
                        Style::new()
                            .fill_width()
                            .h(40)
                            .bg(surface)
                            .align_items(AlignItems::Center)
                            .justify_content(JustifyContent::SpaceBetween)
                            .pad_left(12),
                    )
                    .children([title_node, ctx.child_auto(&buttons, CaptionButtons)])
            } else {
                Row("stylebook-chrome")
                    .style(
                        Style::new()
                            .fill_width()
                            .h(40)
                            .bg(surface)
                            .align_items(AlignItems::Center)
                            .pad_left(12),
                    )
                    .child(title_node)
            }
        });
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
