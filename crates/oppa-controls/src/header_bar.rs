use crate::common::*;
use oppa::*;
// ---------------------------------------------------------------------------
// HeaderBar (decision 482 — the `Extended`-mode in-app header)
// ---------------------------------------------------------------------------

/// Caption-button width in px (the platform caption-button shape:
/// narrow press targets packed at the bar's trailing edge).
pub const CAPTION_BUTTON_W: f32 = 46.0;

/// Caption glyph render size in px (decision 487): true Win11
/// caption shapes drawn on a 16px Segoe-style box (10px strokes,
/// vertically centered) — baked at 4x through the shared cache, so
/// they stay smooth on the un-antialiased GPU path (the decision-478
/// baked-icon rule). The button `Div` centers its content on both
/// axes, so the 16px leaf sits exactly centered in the 46px target.
pub const CAPTION_ICON_PX: f32 = 16.0;

/// Native close-hover fill (decision 486): the OS close-hover red
/// (`#E81123` on both palettes — the one caption color that never
/// themes, matching the native button).
pub const CLOSE_HOVER_BG: Color = Color(0xFF_E81123);

/// Win11 caption glyph shapes (decision 487 — Segoe-Fluent-metric
/// strokes on a 16px box: 10px minimize bar vertically centered,
/// 9px maximize square, diagonal close cross; 1.2px strokes). Ink is
/// filled per deposit (see [`CaptionIcons::deposit`]).
const GLYPH_MINIMIZE: &str = "M3 8h10";
const GLYPH_MAXIMIZE: &str = "M3.5 3.5h9v9h-9z";
const GLYPH_CLOSE: &str = "M3.5 3.5l9 9M12.5 3.5l-9 9";

/// Caption glyph inks: the exact palette text inks (light `#111111`,
/// dark `#F5F5F5` — sampled off `ThemeTokens`, never approximated)
/// plus unthemed white for the close hover on signal red.
const INK_LIGHT: &str = "#111111";
const INK_DARK: &str = "#F5F5F5";
const INK_WHITE: &str = "#FFFFFF";

/// Baked caption glyph ids (decision 487): one deposit per
/// button/ink (re-deposits replace, same id — the cache rule — so
/// per-render deposits cost microsecond resvg work only when the
/// header re-renders, with no once-slot machinery and no growth).
#[derive(Clone, Copy, Debug)]
pub(crate) struct CaptionIcons {
    pub minimize: oppa::ImageId,
    pub maximize: oppa::ImageId,
    pub close: oppa::ImageId,
    /// White close glyph for the signal-red hover (both palettes).
    pub close_white: oppa::ImageId,
}

impl CaptionIcons {
    /// Deposits the themed glyph set into the shared cache (pure
    /// function of `(images, mode)` — light/dark variants follow the
    /// palette, close-white rides along for the hover).
    pub(crate) fn deposit(images: &oppa::ImageCache, mode: ThemeMode) -> Self {
        let ink = match mode {
            ThemeMode::Light => INK_LIGHT,
            ThemeMode::Dark => INK_DARK,
        };
        let bake = |key: &str, shape: &str, shape_ink: &str| {
            let svg = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\">\
                 <path d=\"{shape}\" fill=\"none\" stroke=\"{shape_ink}\" stroke-width=\"1.2\"/>\
                 </svg>"
            );
            deposit_svg_icon(images, key, svg.as_bytes(), (CAPTION_ICON_PX * 4.0) as u32)
        };
        let themed = |name: &str, shape: &str| {
            let key = match mode {
                ThemeMode::Light => format!("oppa/caption/{name}-light"),
                ThemeMode::Dark => format!("oppa/caption/{name}-dark"),
            };
            bake(&key, shape, ink)
        };
        Self {
            minimize: themed("min", GLYPH_MINIMIZE),
            maximize: themed("max", GLYPH_MAXIMIZE),
            close: themed("close", GLYPH_CLOSE),
            close_white: bake("oppa/caption/close-white", GLYPH_CLOSE, INK_WHITE),
        }
    }
}

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
    // Decision 489: caption buttons dock flush to the bar's top-right
    // (native position, zero inset) — the trailing pad drops whenever
    // buttons render, so the close box ends exactly at the bar edge
    // (and the published zones with it, by construction). The title
    // keeps its leading inset either way.
    let mut bar_style = Style::new()
        .fill_width()
        .h(props.height)
        .bg(t.surface)
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::SpaceBetween)
        .pad_left(12);
    if props.on_minimize.is_none() && props.on_maximize.is_none() && props.on_close.is_none() {
        bar_style = bar_style.pad_right(12);
    }
    // Decision 487 baked glyphs (replacing the ASCII `_`/`□`/`×`
    // approximations with true Win11 caption shapes): the themed set
    // follows the palette live — a theme toggle re-renders with the
    // matching inks — while the close hover always wears white on
    // signal red.
    let icons = CaptionIcons::deposit(&ctx.images(), ctx.theme().mode());
    let buttons: Vec<(CaptionButton, &str, &Option<Action>, oppa::ImageId)> = {
        let close_glyph = if ctx.caption_hover().get() == Some(CaptionButton::Close) {
            icons.close_white
        } else {
            icons.close
        };
        vec![
            (
                CaptionButton::Minimize,
                "header-min",
                &props.on_minimize,
                icons.minimize,
            ),
            (
                CaptionButton::Maximize,
                "header-max",
                &props.on_maximize,
                icons.maximize,
            ),
            (
                CaptionButton::Close,
                "header-close",
                &props.on_close,
                close_glyph,
            ),
        ]
    };
    // Decision 486 hover glow (the OS paints no hover on a stripped
    // frame): the tracked hover signal re-renders the bar on flips;
    // the hovered button tints — close goes signal red (with the
    // white glyph above), others take a quiet themed wash.
    // Paint-only (bg never moves boxes, so laid geometry is
    // identical hovered or not).
    let hover = ctx.caption_hover().get();
    let mut trailing: Vec<VNode> = Vec::new();
    for (button, debug, slot, image) in buttons {
        if let Some(on_press) = slot {
            let press = on_press.clone();
            let mut cell = Style::new()
                .size(CAPTION_BUTTON_W, props.height)
                .align_items(AlignItems::Center)
                .justify_content(JustifyContent::Center)
                .cursor(CursorIcon::Pointer);
            if hover == Some(button) {
                cell = cell.bg(if button == CaptionButton::Close {
                    CLOSE_HOVER_BG
                } else {
                    t.border
                });
            }
            trailing.push(Div(debug).style(cell).on_press(move || press()).child(
                crate::ImageView(
                    ctx,
                    &crate::ImageViewProps {
                        image,
                        size: CAPTION_ICON_PX,
                        radius: 0.0,
                        alt: SharedString::from(""),
                    },
                ),
            ));
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
