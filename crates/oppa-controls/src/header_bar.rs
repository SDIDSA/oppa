use crate::common::*;
use oppa::*;
// ---------------------------------------------------------------------------
// HeaderBar (decision 482 — the `Extended`-mode in-app header)
// ---------------------------------------------------------------------------

/// Caption-button width in px (the platform caption-button shape:
/// narrow press targets packed at the bar's trailing edge).
pub const CAPTION_BUTTON_W: f32 = 46.0;

/// Native close-hover fill (decision 486): the OS close-hover red
/// (`#E81123` on both palettes — the one caption color that never
/// themes, matching the native button).
pub const CLOSE_HOVER_BG: Color = Color(0xFF_E81123);

/// `HeaderBar` props: the in-app title bar for
/// [`WindowStyle::Extended`](oppa::WindowStyle) windows.
///
/// The outer node carries the [`HEADER_BAR_DEBUG`] label, so the
/// runners publish its committed box (minus press-owning children)
/// as the OS drag region — bare bar chrome drags the window while
/// nested caption buttons keep normal hit-testing. The bar itself
/// is handlerless by construction (an owner press would swallow the
/// gesture into the framework instead of the OS — stated, not
/// silent).
///
/// Caption buttons are opt-in [`Action`]s: provide the ones the
/// window needs and wire them to the loop-installed
/// [`WindowControl`](oppa::WindowControl) (minimize / maximize) or
/// `ctx.request_close()` (close — the veto consult runs, so an
/// unsaved-changes modal can mount instead of dying). Missing
/// buttons render nothing (never a dead affordance, the disabled
/// structural precedent — decision 213).
///
/// Decision 485 (native caption buttons): wire the actions
/// unconditionally — on Windows `Extended` with
/// `native_caption_buttons` (the default) the OS answers first over
/// the published zones (real actions, hover glow, snap layouts)
/// and the wired actions stand down harmlessly; with the flag off
/// (or on Linux, where no OS zones exist) the same actions bridge
/// the press. Either way the close veto still consults through the
/// shared close path.
#[derive(Clone)]
pub struct HeaderBarProps {
    pub title: SharedString,
    /// Bar height in px (default 40 — room for the 32px caption
    /// targets plus breathing room; explicit so headless trees get
    /// a hit box without a text service, the catalog convention).
    pub height: f32,
    pub on_minimize: Option<Action>,
    pub on_maximize: Option<Action>,
    pub on_close: Option<Action>,
}

impl Props for HeaderBarProps {}

impl HeaderBarProps {
    pub fn new(title: &str) -> Self {
        Self {
            title: SharedString::from(title),
            height: 40.0,
            on_minimize: None,
            on_maximize: None,
            on_close: None,
        }
    }

    pub fn height(mut self, px: f32) -> Self {
        self.height = px;
        self
    }

    /// Bridges the minimize caption button (the app forwards to the
    /// loop's [`WindowControl`](oppa::WindowControl) `minimize`).
    pub fn on_minimize(mut self, f: impl Fn() + 'static) -> Self {
        self.on_minimize = Some(action(f));
        self
    }

    /// Bridges the maximize/restore caption button (the app forwards
    /// to the loop's [`WindowControl`](oppa::WindowControl)
    /// `toggle_maximize`).
    pub fn on_maximize(mut self, f: impl Fn() + 'static) -> Self {
        self.on_maximize = Some(action(f));
        self
    }

    /// Bridges the close caption button (the app calls
    /// `ctx.request_close()` or runs a custom flow first).
    pub fn on_close(mut self, f: impl Fn() + 'static) -> Self {
        self.on_close = Some(action(f));
        self
    }
}

/// In-app title bar for `Extended` windows: title text leading, the
/// provided caption buttons trailing. Handlerless chrome (the OS
/// owns the drag gesture over it); each provided button is a real
/// press target with the hand cursor (missing buttons leave no
/// trace). Fill rides the app theme's surface token so the bar
/// reads as chrome on every palette.
pub fn HeaderBar(ctx: &Ctx, props: &HeaderBarProps) -> VNode {
    let t = ctx.theme().tokens();
    let bar_style = Style::new()
        .fill_width()
        .h(props.height)
        .bg(t.surface)
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::SpaceBetween)
        .pad_x(12);
    let buttons: Vec<(CaptionButton, &str, &Option<Action>, &str)> = vec![
        (
            CaptionButton::Minimize,
            "header-min",
            &props.on_minimize,
            "_",
        ),
        (
            CaptionButton::Maximize,
            "header-max",
            &props.on_maximize,
            "□",
        ),
        (CaptionButton::Close, "header-close", &props.on_close, "×"),
    ];
    // Decision 486 hover glow (the OS paints no hover on a stripped
    // frame): the tracked hover signal re-renders the bar on flips;
    // the hovered button tints — close goes signal red with light
    // ink (the native close-hover shape), others take a quiet
    // themed wash. Paint-only (bg + ink never move boxes, so laid
    // geometry is identical hovered or not).
    let hover = ctx.caption_hover().get();
    let mut trailing: Vec<VNode> = Vec::new();
    for (button, debug, slot, glyph) in buttons {
        if let Some(on_press) = slot {
            let press = on_press.clone();
            let mut cell = Style::new()
                .size(CAPTION_BUTTON_W, props.height)
                .align_items(AlignItems::Center)
                .justify_content(JustifyContent::Center)
                .cursor(CursorIcon::Pointer);
            let mut glyph_node = VNode::from(Text::new(glyph).size(14));
            if hover == Some(button) {
                if button == CaptionButton::Close {
                    cell = cell.bg(CLOSE_HOVER_BG);
                    glyph_node = with_ink(glyph_node, CONTRAST_INK);
                } else {
                    cell = cell.bg(t.border);
                }
            }
            trailing.push(
                Div(debug)
                    .style(cell)
                    .on_press(move || press())
                    .child(glyph_node),
            );
        }
    }
    let mut kids = vec![VNode::from(Text::new(props.title.clone()).size(13))];
    if !trailing.is_empty() {
        kids.push(
            Row("header-caption-buttons")
                .style(Style::new().h(props.height))
                .children(trailing),
        );
    }
    // One Row root (the runners publish THIS node's box as the drag
    // region): the title plus the trailing button row lay out as its
    // two flex items under `SpaceBetween`.
    Row(HEADER_BAR_DEBUG).style(bar_style).children(kids)
}
