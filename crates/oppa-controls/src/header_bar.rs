use crate::common::*;
use oppa::*;
use std::cell::RefCell;
use std::rc::Rc;
// ---------------------------------------------------------------------------
// HeaderBar (decision 482 — the `Extended`-mode in-app header)
// ---------------------------------------------------------------------------

/// Caption-button width in px (the platform caption-button shape:
/// narrow press targets packed at the bar's trailing edge).
pub const CAPTION_BUTTON_W: f32 = 46.0;
/// Caption glyph render size in px (decision 487, user-supplied
/// filled set): true Win11 caption shapes (rounded 10px designs on
/// an 8.5 viewBox, rendered at the flat design size) — baked at 4x
/// through the shared cache, so they stay smooth on the
/// un-antialiased GPU path (the decision-478 baked-icon rule). The
/// button `Div` centers its content on both axes, so the 10px leaf
/// sits exactly centered in the 46px target.
pub const CAPTION_ICON_PX: f32 = 10.0;

/// Native close-hover fill (decision 486): the OS close-hover red
/// (`#E81123` on both palettes — the one caption color that never
/// themes, matching the native button).
pub const CLOSE_HOVER_BG: Color = Color(0xFF_E81123);

/// Win11 caption glyph shapes (user-supplied filled set: rounded
/// 10px designs on an 8.5 viewBox — minimize bar, maximize square,
/// overlapping-squares restore, diagonal close cross; filled with
/// the palette ink per deposit, see [`CaptionIcons::deposit`]).
const GLYPH_MINIMIZE: &str = "m 0.4234,4.6755 q -0.0872,0 -0.164,-0.0332 Q 0.1827,4.6091 0.1245,4.551 0.0664,4.4929 0.0332,4.4161 0,4.3393 0,4.2521 0,4.1649 0.0332,4.0881 0.0664,4.0113 0.1245,3.9511 0.1827,3.8909 0.2595,3.8577 0.3362,3.8245 0.4234,3.8245 H 8.0783 q 0.0872,0 0.164,0.0332 0.0768,0.0332 0.1349,0.0934 0.0581,0.0602 0.0913,0.137 0.0332,0.0768 0.0332,0.164 0,0.0872 -0.0332,0.164 -0.0332,0.0768 -0.0913,0.1349 -0.0581,0.0581 -0.1349,0.0913 -0.0768,0.0332 -0.164,0.0332 z";
const GLYPH_MAXIMIZE: &str = "M 1.2534,8.5 Q 1.0044,8.5 0.7761,8.3983 0.5479,8.2966 0.3756,8.1244 0.2034,7.9521 0.1017,7.7239 0,7.4956 0,7.2466 V 1.2534 Q 0,1.0044 0.1017,0.7761 0.2034,0.5479 0.3756,0.3756 0.5479,0.2034 0.7761,0.1017 1.0044,0 1.2534,0 H 7.2466 Q 7.4956,0 7.7239,0.1017 7.9521,0.2034 8.1244,0.3756 8.2966,0.5479 8.3983,0.7761 8.5,1.0044 8.5,1.2534 V 7.2466 Q 8.5,7.4956 8.3983,7.7239 8.2966,7.9521 8.1244,8.1244 7.9521,8.2966 7.7239,8.3983 7.4956,8.5 7.2466,8.5 Z M 7.2258,7.6492 q 0.0872,0 0.1639,-0.0332 0.0768,-0.0332 0.1349,-0.0913 0.0581,-0.0581 0.0913,-0.1349 0.0332,-0.0768 0.0332,-0.1639 V 1.2742 q 0,-0.0872 -0.0332,-0.1639 Q 7.5828,1.0334 7.5247,0.9753 7.4666,0.9172 7.3898,0.884 7.313,0.8508 7.2258,0.8508 H 1.2742 q -0.0872,0 -0.1639,0.0332 -0.0768,0.0332 -0.1349,0.0913 Q 0.9172,1.0334 0.884,1.1102 0.8508,1.187 0.8508,1.2742 v 5.9517 q 0,0.0872 0.0332,0.1639 0.0332,0.0768 0.0913,0.1349 0.0581,0.0581 0.1349,0.0913 0.0768,0.0332 0.1639,0.0332 z";
const GLYPH_RESTORE: &str = "m 7.6492,2.5193 q 0,-0.3445 -0.1370,-0.6495 Q 7.3752,1.5647 7.1407,1.3385 6.9063,1.1123 6.5970,0.9816 6.2878,0.8508 5.9475,0.8508 H 1.7722 Q 1.8386,0.6599 1.9590,0.5022 2.0793,0.3445 2.2371,0.2324 2.3948,0.1204 2.5836,0.0602 2.7725,0 2.9758,0 H 5.9475 Q 6.4746,0 6.9395,0.2013 7.4043,0.4026 7.7509,0.7471 8.0974,1.0916 8.2987,1.5564 8.5,2.0212 8.5,2.5483 V 5.5242 Q 8.5,5.7275 8.4398,5.9164 8.3796,6.1052 8.2676,6.2629 8.1555,6.4207 7.9978,6.541 7.8401,6.6614 7.6492,6.7278 Z M 1.2534,8.5 Q 1.0044,8.5 0.7761,8.3983 0.5479,8.2966 0.3756,8.1244 0.2034,7.9521 0.1017,7.7239 0,7.4956 0,7.2466 V 2.9551 Q 0,2.7019 0.1017,2.4757 0.2034,2.2495 0.3756,2.0773 0.5479,1.905 0.774,1.8033 1.0002,1.7017 1.2534,1.7017 h 4.2915 q 0.2532,0 0.4814,0.1017 0.2283,0.1017 0.3984,0.2719 0.1702,0.1702 0.2719,0.3984 0.1017,0.2283 0.1017,0.4814 V 7.2466 q 0,0.2532 -0.1017,0.4794 Q 6.595,7.9521 6.4227,8.1244 6.2505,8.2966 6.0243,8.3983 5.7981,8.5 5.5449,8.5 Z M 5.5242,7.6492 q 0.0872,0 0.1639,-0.0332 0.0768,-0.0332 0.1370,-0.0913 0.0602,-0.0581 0.0934,-0.1349 0.0332,-0.0768 0.0332,-0.1639 v -4.25 q 0,-0.0872 -0.0332,-0.166 Q 5.8853,2.731 5.8271,2.6729 5.769,2.6147 5.6902,2.5815 5.6113,2.5483 5.5242,2.5483 h -4.25 q -0.0872,0 -0.1639,0.0332 -0.0768,0.0332 -0.1349,0.0934 -0.0581,0.0602 -0.0913,0.137 -0.0332,0.0768 -0.0332,0.1639 v 4.25 q 0,0.0872 0.0332,0.1639 0.0332,0.0768 0.0913,0.1349 0.0581,0.0581 0.1349,0.0913 0.0768,0.0332 0.1639,0.0332 z";
const GLYPH_CLOSE: &str = "M 4.25,4.8518 0.7263,8.3755 Q 0.6018,8.5 0.4275,8.5 0.2449,8.5 0.1224,8.3776 0,8.2551 0,8.0725 0,7.8982 0.1245,7.7737 L 3.6482,4.25 0.1245,0.7263 Q 0,0.6018 0,0.4233 0,0.3362 0.0332,0.2573 0.0664,0.1785 0.1245,0.1224 0.1826,0.0664 0.2615,0.0332 0.3403,0 0.4275,0 0.6018,0 0.7263,0.1245 L 4.25,3.6482 7.7737,0.1245 Q 7.8982,0 8.0767,0 q 0.0872,0 0.1639,0.0332 0.0768,0.0332 0.1349,0.0913 0.0581,0.0581 0.0913,0.1349 Q 8.5,0.3362 8.5,0.4233 8.5,0.6018 8.3755,0.7263 L 4.8518,4.25 8.3755,7.7737 Q 8.5,7.8982 8.5,8.0725 8.5,8.1597 8.4668,8.2385 8.4336,8.3174 8.3776,8.3755 8.3215,8.4336 8.2427,8.4668 8.1638,8.5 8.0767,8.5 7.8982,8.5 7.7737,8.3755 Z";

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
    /// Overlapping-squares restore glyph for the maximized state
    /// (decision 495 — same themed ink as maximize).
    pub restore: oppa::ImageId,
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
                "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 8.5 8.5\">\
                 <path d=\"{shape}\" fill=\"{shape_ink}\"/>\
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
            restore: themed("restore", GLYPH_RESTORE),
            close: themed("close", GLYPH_CLOSE),
            close_white: bake("oppa/caption/close-white", GLYPH_CLOSE, INK_WHITE),
        }
    }
}

/// Custom chrome factory (the Splitter-pane precedent — `VNode` is
/// move-only, so factories rebuild per render; the output carries the
/// app's own debug labels and hit-testing — handlerless visuals drag
/// with the bar, press-owning descendants subtract from the drag
/// region like caption buttons do).
pub type ChromeView = Rc<dyn Fn(&Ctx) -> VNode>;

/// `HeaderBar` props: the drag-region trait for
/// [`WindowStyle::Extended`](oppa::WindowStyle) windows (decision 494).
///
/// Not a prebuilt title bar — a marker the app's own chrome
/// identifies as. The outer node carries the [`HEADER_BAR_DEBUG`]
/// label, so runners publish its committed box (minus press-owning
/// descendants) as the OS "drag to move" region. The bar renders no
/// visuals of its own (no fill, no title, no buttons) — it fills the
/// available width and packs to its content height, while all chrome
/// geometry and styling belong to the composed content. The container
/// itself is handlerless by construction (an owner press would
/// swallow the gesture into the framework instead of the OS —
/// stated, not silent).
///
/// Pair with [`CaptionButtons`] (placed anywhere inside the chrome)
/// where the window needs native buttons; leave them out for a
/// buttonless bar. Never mount under `Undecorated` (a bare rectangle
/// owns no OS behaviors — chrome there is purely decorative).
#[derive(Clone, Default)]
pub struct HeaderBarProps {
    /// The app's chrome (an empty bar publishes no drag region).
    pub content: Option<ChromeView>,
}

impl Props for HeaderBarProps {}

impl HeaderBarProps {
    pub fn new() -> Self {
        Self { content: None }
    }

    /// The custom chrome identifying as the bar (the factory runs
    /// per render — the Splitter-pane precedent).
    pub fn content(mut self, f: impl Fn(&Ctx) -> VNode + 'static) -> Self {
        self.content = Some(Rc::new(f));
        self
    }
}

/// The drag-region trait: transparent chrome marker filling the
/// available width and packing to its content height (a bar spans
/// the window by construction — full-width chrome is the whole
/// point, and `fill_width` is what lets composed chrome resolve its
/// own widths against a finite parent). Handlerless by
/// construction; nested press owners keep normal hit-testing while
/// bare chrome drags the OS window.
pub fn HeaderBar(ctx: &Ctx, props: &HeaderBarProps) -> VNode {
    let inner = match &props.content {
        Some(view) => view(ctx),
        None => Div("header-empty").style(Style::new().size(0, 0)).build(),
    };
    // One Row root (the runners publish THIS node's box as the drag
    // region).
    Row(HEADER_BAR_DEBUG)
        .style(Style::new().fill_width())
        .child(inner)
}

/// `CaptionButtons` props: the optional native buttons for
/// [`WindowStyle::Extended`](oppa::WindowStyle) windows (decision 494).
///
/// Place inside `HeaderBar` chrome where the buttons belong
/// (usually trailing — the app owns the surrounding layout). Each
/// provided button is a real press target with the hand cursor;
/// missing buttons render nothing (never a dead affordance, the
/// decision-213 precedent). Wire the actions unconditionally — on
/// Windows `Extended` with `native_caption_buttons` (the default)
/// the OS answers first over the published zones (real actions,
/// hover glow, snap layouts) and the wired actions stand down
/// harmlessly, while on Linux (where no OS zones exist) the same
/// declared buttons bridge the press. Either way the close veto
/// still consults through the shared close path. With the flag off
/// the app declares no buttons at all (drag-only bar) — then this
/// marker is simply absent. Declare no buttons at all and the row
/// stays zero-size (no zones, no presses).
#[derive(Clone)]
pub struct CaptionButtonsProps {
    /// Cell height in px (default 40; width is a fixed 46 per cell —
    /// the platform caption-button shape; explicit so headless trees
    /// get a hit box without a text service, the catalog convention).
    pub height: f32,
    pub on_minimize: Option<Action>,
    pub on_maximize: Option<Action>,
    pub on_close: Option<Action>,
}

impl Props for CaptionButtonsProps {}

impl Default for CaptionButtonsProps {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptionButtonsProps {
    pub fn new() -> Self {
        Self {
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

/// Optional native caption buttons: standard 46px cells with true
/// Win11 glyphs, placed by the app inside its `HeaderBar` chrome.
/// Cells center their 10px leaf on both axes; the close hover wears
/// white on signal red while the others take a quiet themed wash
/// (paint-only hover — geometry never moves).
pub fn CaptionButtons(ctx: &Ctx, props: &CaptionButtonsProps) -> VNode {
    let t = ctx.theme().tokens();
    // Caption cells dock flush inside their row (native position,
    // zero inset) — the close box ends exactly at the row's trailing
    // edge, so the app docks the row itself flush and the published
    // zones land where the OS expects caption buttons.
    // Decision 487 baked glyphs (replacing the ASCII `_`/`□`/`×`
    // approximations with true Win11 caption shapes): the themed set
    // follows the palette live — a theme toggle re-renders with the
    // matching inks — while the close hover always wears white on
    // signal red.
    let icons = CaptionIcons::deposit(&ctx.images(), ctx.theme().mode());
    // Decision 495: the maximize cell wears the overlapping-squares
    // restore glyph while the window is maximized (tracked signal —
    // flips re-render exactly like hover).
    let maximize_glyph = if ctx.maximized().get() {
        icons.restore
    } else {
        icons.maximize
    };
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
                maximize_glyph,
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
    // No buttons declared: zero-size row (no zones, no presses —
    // callers skip the marker entirely in that shape).
    if trailing.is_empty() {
        return Row("header-caption-buttons")
            .style(Style::new().size(0, 0))
            .build();
    }
    Row("header-caption-buttons")
        .style(Style::new().h(props.height))
        .children(trailing)
}

// ---------------------------------------------------------------------------
// CaptionOutbox (decision 510 — the Stylebook/sera-login caption bridge,
// now shared)
// ---------------------------------------------------------------------------

/// One drained caption request (decision 510): level-triggered button
/// signals converge here so the desktop poll hook stays three lines.
/// Reading clears the signal (each press drains exactly once — the
/// level-triggered Stylebook rule, never an edge that replays).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaptionAction {
    Minimize,
    Maximize,
    Close,
}

/// Published caption handles (minimize, maximize, close — one
/// level-triggered request signal each).
pub type CaptionSignals = (Signal<bool>, Signal<bool>, Signal<bool>);

/// Runner-visible caption outbox (the Stylebook publish-once
/// precedent — signals are runtime-bound, so the root publishes them
/// on first render and the desktop poll hook drains them into the
/// loop-installed `WindowControl`: minimize/maximize forward there,
/// close arms `ctx.request_close()` so the close veto still
/// consults). Barless designs publish it too (nothing arms the
/// signals then — the hook drains nothing and the bar stays a pure
/// drag region), keeping the bridge shape identical everywhere.
#[derive(Clone, Default)]
pub struct CaptionOutbox {
    inner: Rc<RefCell<Option<CaptionSignals>>>,
}

impl CaptionOutbox {
    /// Publishes the level-triggered request signals (called once per
    /// root render — replacing is idempotent, the cache rule).
    pub fn publish(&self, minimize: Signal<bool>, maximize: Signal<bool>, close: Signal<bool>) {
        self.inner.borrow_mut().replace((minimize, maximize, close));
    }

    /// Reads the published handles (`None` before first render).
    pub fn get(&self) -> Option<CaptionSignals> {
        self.inner.borrow().clone()
    }

    /// Drains armed requests oldest-first (minimize, maximize, close),
    /// clearing each as it goes. The poll hook matches on the
    /// returned actions (`loop_.minimize()`,
    /// `loop_.toggle_maximize()`, `loop_.host().request_close()`).
    pub fn take_actions(&self) -> Vec<CaptionAction> {
        let mut out = Vec::new();
        if let Some((minimize, maximize, close)) = self.inner.borrow().clone() {
            if minimize.get() {
                minimize.set(false);
                out.push(CaptionAction::Minimize);
            }
            if maximize.get() {
                maximize.set(false);
                out.push(CaptionAction::Maximize);
            }
            if close.get() {
                close.set(false);
                out.push(CaptionAction::Close);
            }
        }
        out
    }
}
