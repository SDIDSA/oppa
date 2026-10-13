//! Runtime window-chrome seam (Round 16.3, decision 316).
//!
//! Title, min/max sizes, and fullscreen change while the app runs
//! — through runner-installed handles, never statics: the desktop
//! loop forwards its window calls into an installed
//! [`WindowControl`], and headless loops with no control stay quiet
//! no-ops (window chrome is a presentational hint like `set_cursor`
//! — advisory, never a wiring bug). The close veto rides beside
//! it: runners consult the loop-installed handler on `WM_CLOSE` /
//! `CloseRequested` and suppress exit when it refuses, so
//! unsaved-changes flows can mount instead of dying.

use std::cell::RefCell;
use std::rc::Rc;

use crate::component::ComponentHost;
use crate::shell::EventKind;

/// Retained-tree debug label of the `HeaderBar` control root
/// (decision 482): the controls catalog stamps this on the bar's
/// outer node, and [`window_drag_rects`] finds every such node to
/// publish the `Extended`-mode OS drag region. Apps never write
/// this label by hand (a hand-labeled node would silently become a
/// drag region — the loud-rule precedent: use the control).
pub const HEADER_BAR_DEBUG: &str = "oppa::HeaderBar";

/// Caption-button slot (decision 485): which native caption zone a
/// committed box maps to. The runner publishes one zone per wired
/// `HeaderBar` caption button (`header-min` / `header-max` /
/// `header-close` debug labels, in that order — see
/// [`native_caption_zones`]); shells answer their platform caption
/// codes over them (Windows `HTMINBUTTON` / `HTMAXBUTTON` /
/// `HTCLOSE`, with hover glow, press, and snap layouts owned by the
/// OS — the JavaFX `EXTENDED` superimposed-buttons shape).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CaptionButton {
    Minimize,
    Maximize,
    Close,
}

impl CaptionButton {
    /// Debug label of the matching `HeaderBar` caption node, in
    /// zone order (minimize, maximize, close — left to right).
    pub fn debug_label(self) -> &'static str {
        match self {
            CaptionButton::Minimize => "header-min",
            CaptionButton::Maximize => "header-max",
            CaptionButton::Close => "header-close",
        }
    }
}

/// Native caption zones for `WindowStyle::Extended` (decision 485):
/// the committed boxes of the `HeaderBar` caption buttons, in
/// min/max/close order, skipping missing or unlaid nodes (a subset
/// renders a subset — never a fabricated zone). The boxes are the
/// app's own button geometry, so OS zones and button visuals align
/// by construction. Untracked (paint/runner bookkeeping, never
/// effects).
///
/// The zones only take effect where the shell implements them
/// (Windows answers the `HT*BUTTON` codes with fully native
/// behavior — real actions, hover glow, Win11 snap layouts — while
/// the app's `on_*` actions stay wired as the Linux/custom/off
/// fallback, shadowed harmlessly where the OS answers first; the
/// close veto still consults through the shared `WM_CLOSE` path on
/// every close, native or bridged).
pub fn native_caption_zones(host: &ComponentHost) -> Vec<(CaptionButton, [f32; 4])> {
    host.with_retained_mut(|rec, _| {
        let mut out = Vec::new();
        for button in [
            CaptionButton::Minimize,
            CaptionButton::Maximize,
            CaptionButton::Close,
        ] {
            let mut ids = rec.find_by_debug(button.debug_label());
            ids.sort();
            for id in ids {
                let Some(node) = rec.get(id) else {
                    continue;
                };
                let Some(layout) = node.layout.clone() else {
                    continue;
                };
                out.push((button, [layout.x, layout.y, layout.w, layout.h]));
            }
        }
        out
    })
}

/// OS window-drag regions for `WindowStyle::Extended` (decision
/// 482): the committed boxes of every `HeaderBar` node, minus the
/// boxes of press-owning descendants (buttons, fields, menus keep
/// normal hit-testing — press ownership routes the gesture,
/// decision 96 — while the bare bar chrome drags the OS window).
/// Rects are `[x, y, w, h]` in device px (committed-box space, the
/// same space the shells hit-test). Empty when no header bar is
/// laid out (never `None` — no bar means no drag region, stated).
/// Degenerate boxes (zero/negative extent, e.g. a content-free bar)
/// never publish — a zero-area drag region would swallow `HTCLIENT`
/// answers into dead `HTCAPTION` strips.
/// Untracked (paint/runner bookkeeping, never effects).
pub fn window_drag_rects(host: &ComponentHost) -> Vec<[f32; 4]> {
    host.with_retained_mut(|rec, _| {
        let bars = rec.find_by_debug(HEADER_BAR_DEBUG);
        let mut out = Vec::new();
        for bar in bars {
            let Some(node) = rec.get(bar) else {
                continue;
            };
            let Some(layout) = node.layout.clone() else {
                continue;
            };
            if layout.w <= 0.0 || layout.h <= 0.0 {
                continue;
            }
            let mut rects = vec![[layout.x, layout.y, layout.w, layout.h]];
            for blocker in press_blockers_under(rec, bar) {
                rects = subtract_rect(rects, blocker);
                if rects.is_empty() {
                    break;
                }
            }
            out.extend(rects);
        }
        out
    })
}

/// Title-bar box for `WM_GETTITLEBARINFOEX` (decision 491): the
/// bounding union of every laid-out `HeaderBar` box (`None` with no
/// bar — the shell then keeps default handling). Unsubtracted (the
/// OS wants the whole bar, blockers included) and untracked (like
/// the drag regions — paint/runner bookkeeping, never effects).
pub fn title_bar_rect(host: &ComponentHost) -> Option<[f32; 4]> {
    host.with_retained_mut(|rec, _| {
        let mut acc: Option<[f32; 4]> = None;
        for bar in rec.find_by_debug(HEADER_BAR_DEBUG) {
            let Some(node) = rec.get(bar) else {
                continue;
            };
            let Some(layout) = node.layout.clone() else {
                continue;
            };
            let r = [layout.x, layout.y, layout.w, layout.h];
            acc = Some(match acc {
                None => r,
                Some(a) => [
                    a[0].min(r[0]),
                    a[1].min(r[1]),
                    (a[0] + a[2]).max(r[0] + r[2]) - a[0].min(r[0]),
                    (a[1] + a[3]).max(r[1] + r[3]) - a[1].min(r[1]),
                ],
            });
        }
        acc
    })
}

/// Published OS drag regions for a launch style (decision 482):
/// the [`window_drag_rects`] geometry when
/// [`WindowStyle::wants_drag_region`] holds (`Extended`), empty
/// otherwise (a `HeaderBar` under any other style is plain chrome —
/// the runners publish through this one policy point instead of
/// re-deciding per platform, so geometry and gating never drift).
/// Untracked (paint/runner bookkeeping, never effects).
pub fn drag_regions_for_style(host: &ComponentHost, style: WindowStyle) -> Vec<[f32; 4]> {
    if style.wants_drag_region() {
        window_drag_rects(host)
    } else {
        Vec::new()
    }
}

/// Committed boxes of press-owning descendants of `root` (the
/// interactive children that keep their own hit-testing inside an
/// `Extended` drag region). `root` itself is excluded (its own
/// press handler, if any, still drags — the bar chrome owns the
/// gesture unless a child claims it).
fn press_blockers_under(
    rec: &crate::reconciler::Reconciler,
    root: crate::arena::NodeId,
) -> Vec<[f32; 4]> {
    let mut out = Vec::new();
    let mut stack: Vec<crate::arena::NodeId> = Vec::new();
    if let Some(node) = rec.get(root) {
        stack.extend(node.children.iter().copied());
    }
    while let Some(id) = stack.pop() {
        let Some(node) = rec.get(id) else {
            continue;
        };
        if node
            .handlers
            .iter()
            .any(|(kind, _)| *kind == EventKind::Press)
        {
            if let Some(layout) = node.layout.clone() {
                out.push([layout.x, layout.y, layout.w, layout.h]);
            }
        }
        stack.extend(node.children.iter().copied());
    }
    out
}

/// Subtracts `cut` from every rect in `rects` (axis-aligned
/// difference, up to 4 survivors per rect). Degenerate results
/// (zero/negative extent) are dropped, never published (a
/// zero-area drag region would swallow `HTCLIENT` answers into
/// dead `HTCAPTION` strips — refused here, not at the OS).
fn subtract_rect(rects: Vec<[f32; 4]>, cut: [f32; 4]) -> Vec<[f32; 4]> {
    let mut out = Vec::new();
    for [x, y, w, h] in rects {
        let ix0 = x.max(cut[0]);
        let iy0 = y.max(cut[1]);
        let ix1 = (x + w).min(cut[0] + cut[2]);
        let iy1 = (y + h).min(cut[1] + cut[3]);
        if ix0 >= ix1 || iy0 >= iy1 {
            out.push([x, y, w, h]);
            continue;
        }
        // Left / right / top / bottom slabs around the overlap.
        let pieces = [
            [x, y, ix0 - x, h],
            [ix1, y, x + w - ix1, h],
            [ix0, y, ix1 - ix0, iy0 - y],
            [ix0, iy1, ix1 - ix0, y + h - iy1],
        ];
        out.extend(pieces.into_iter().filter(|r| r[2] > 0.0 && r[3] > 0.0));
    }
    out
}

/// Runtime window icon (Round 20.3, decision 326): an RGBA8
/// pixmap (row-major, top-left first) for the OS window chrome
/// (taskbar / title bar / alt-tab). Constructible only through
/// [`WindowIcon::new`] — zero dimensions and buffer-length
/// mismatches are loud `Err`s there, never a silently dropped or
/// misread icon; backends receive only validated icons.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowIcon {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

impl WindowIcon {
    /// Validates loudly: `width > 0`, `height > 0`, and
    /// `rgba.len() == width * height * 4`.
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err(format!(
                "window icon refused: dimensions must be nonzero, got {width}x{height}"
            ));
        }
        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(format!(
                "window icon refused: {} bytes != {width}x{height}x4 ({expected})",
                rgba.len()
            ));
        }
        Ok(Self {
            rgba,
            width,
            height,
        })
    }

    /// Raw RGBA8 bytes (row-major).
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

/// Configurable window chrome (decision 482): how the OS frames the
/// top-level window. Selected once at launch through the desktop
/// runner's `WindowOptions::with_window_style` (the runner maps it
/// into shell creation flags before the window exists — restyling a
/// live window is out of scope and refused by keeping the field
/// launch-only, never a silent half-apply).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum WindowStyle {
    /// System title bar + borders, system caption buttons
    /// (min / max / close), solid default background. The standard
    /// OS window — every pre-482 app behaves exactly this way.
    #[default]
    Decorated,
    /// Bare rectangle: no OS title bar, no caption buttons, no OS
    /// window behaviors at all (no edge resizing, no snapping, no
    /// shadow — decision 494). Solid background. Anything the app
    /// wants there it wires itself (minimize / maximize through
    /// [`WindowControl`], close through `ctx.request_close()`).
    Undecorated,
    /// Frameless window with a fully transparent background
    /// (`alpha = 0` clear + premultiplied-alpha swapchain where the
    /// compositor supports it). Content provides its own shape; the
    /// page background never paints (a set page color is ignored,
    /// never blended — stated, not silent).
    Transparent,
    /// Client-area-extended / CSD hybrid: native window frame with
    /// no system title bar (decision 494) — the OS keeps managing
    /// the window (edge resizing, snapping/tiling) while the app
    /// composes its own chrome identifying as the `HeaderBar` drag
    /// trait (interactive dragging over that region, nested controls
    /// keep hit-testing). Native caption buttons are optional: the
    /// app places `CaptionButtons` markers where they belong (the OS
    /// answers first over the published zones, or the same buttons
    /// bridge the press where no zones exist); with none declared
    /// the bar is drag-only.
    Extended,
}

impl WindowStyle {
    /// True for every frameless variant (everything but
    /// [`WindowStyle::Decorated`]) — the shell strips the system
    /// title bar and borders at creation.
    pub fn is_frameless(self) -> bool {
        !matches!(self, WindowStyle::Decorated)
    }

    /// True only for [`WindowStyle::Transparent`] — the runner
    /// configures a premultiplied-alpha swapchain and clears to
    /// `alpha = 0` instead of the page background.
    pub fn is_transparent(self) -> bool {
        matches!(self, WindowStyle::Transparent)
    }

    /// True only for [`WindowStyle::Extended`] — the runner
    /// publishes the scene's `HeaderBar` drag regions to the shell
    /// (Windows `HTCAPTION` hit-tests, Linux `drag_window` trigger)
    /// so the designated region drags the OS window while nested
    /// child controls keep normal hit-testing.
    pub fn wants_drag_region(self) -> bool {
        matches!(self, WindowStyle::Extended)
    }
}

/// Runtime window chrome (Round 16.3, decision 316). `&self`
/// methods (shared-handle friendly — backends hold OS handles or
/// `Rc`-shared state, never `&mut` exclusivity). Best-effort
/// presentational hints: a dead or not-yet-open window absorbs
/// calls silently (documented per backend, never a surprise
/// hang — dialogs block, chrome never does).
pub trait WindowControl {
    /// Retitles the window.
    fn set_title(&self, title: &str);
    /// Constrains the resizable floor (`None` clears it back to
    /// the OS default).
    fn set_min_size(&self, size: Option<(u32, u32)>);
    /// Constrains the resizable ceiling (`None` clears it back to
    /// the OS default).
    fn set_max_size(&self, size: Option<(u32, u32)>);
    /// Toggles borderless fullscreen (state-preserving: exit
    /// restores the pre-fullscreen style and rect).
    fn set_fullscreen(&self, fullscreen: bool);
    /// Installs the runtime window icon (`None` restores the OS
    /// default). Receives only [`WindowIcon::new`]-validated icons —
    /// a backend never re-validates dimensions, it converts.
    fn set_icon(&self, icon: Option<WindowIcon>);
    /// Minimizes the window to the taskbar/dock (decision 482 — the
    /// `Extended`-mode caption bridge: the app's own header button
    /// calls this; the OS system button is gone with the title bar).
    /// Default no-op (the `set_ime` precedent — backends without a
    /// window absorb it; headless loops stay quiet no-ops).
    fn minimize(&self) {}
    /// Toggles maximized/restored (decision 482 — same bridge as
    /// [`WindowControl::minimize`]; snapping/tiling stays OS-owned
    /// and needs no call). Default no-op (same precedent).
    fn toggle_maximize(&self) {}
}

/// One recorded chrome call (headless observability — tests prove
/// each loop method forwards exactly).
#[derive(Clone, Debug, PartialEq)]
pub enum WindowCall {
    SetTitle(String),
    SetMinSize(Option<(u32, u32)>),
    SetMaxSize(Option<(u32, u32)>),
    SetFullscreen(bool),
    SetIcon(Option<WindowIcon>),
    /// Recorded [`WindowControl::minimize`] call (decision 482).
    Minimize,
    /// Recorded [`WindowControl::toggle_maximize`] call (decision 482).
    ToggleMaximize,
}

/// Headless/test window control: records every call in order (no
/// window to drive — forwarding proofs, never behavior proofs).
/// Clones share one record (install a clone, read through the
/// original — the loop owns its handle, the test keeps its own).
#[derive(Clone, Debug, Default)]
pub struct ScriptedWindowControl {
    calls: Rc<RefCell<Vec<WindowCall>>>,
}

impl ScriptedWindowControl {
    pub fn new() -> Self {
        Self::default()
    }

    /// Recorded calls so far, in order.
    pub fn calls(&self) -> Vec<WindowCall> {
        self.calls.borrow().clone()
    }
}

impl WindowControl for ScriptedWindowControl {
    fn set_title(&self, title: &str) {
        self.calls
            .borrow_mut()
            .push(WindowCall::SetTitle(title.to_string()));
    }

    fn set_min_size(&self, size: Option<(u32, u32)>) {
        self.calls.borrow_mut().push(WindowCall::SetMinSize(size));
    }

    fn set_max_size(&self, size: Option<(u32, u32)>) {
        self.calls.borrow_mut().push(WindowCall::SetMaxSize(size));
    }

    fn set_fullscreen(&self, fullscreen: bool) {
        self.calls
            .borrow_mut()
            .push(WindowCall::SetFullscreen(fullscreen));
    }

    fn set_icon(&self, icon: Option<WindowIcon>) {
        self.calls.borrow_mut().push(WindowCall::SetIcon(icon));
    }

    fn minimize(&self) {
        self.calls.borrow_mut().push(WindowCall::Minimize);
    }

    fn toggle_maximize(&self) {
        self.calls.borrow_mut().push(WindowCall::ToggleMaximize);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_control_records_every_call() {
        let c = ScriptedWindowControl::new();
        c.set_title("Hi");
        c.set_min_size(Some((100, 80)));
        c.set_max_size(None);
        c.set_fullscreen(true);
        let icon = WindowIcon::new(vec![255; 2 * 2 * 4], 2, 2).expect("valid icon builds");
        c.set_icon(Some(icon.clone()));
        c.set_icon(None);
        c.minimize();
        c.toggle_maximize();
        assert_eq!(
            c.calls(),
            vec![
                WindowCall::SetTitle("Hi".to_string()),
                WindowCall::SetMinSize(Some((100, 80))),
                WindowCall::SetMaxSize(None),
                WindowCall::SetFullscreen(true),
                WindowCall::SetIcon(Some(icon)),
                WindowCall::SetIcon(None),
                WindowCall::Minimize,
                WindowCall::ToggleMaximize,
            ]
        );
    }

    /// Decision 482: `WindowStyle` defaults to `Decorated` (every
    /// pre-482 app keeps the standard OS window with zero changes)
    /// and each variant reports its shell contract honestly.
    #[test]
    fn window_style_defaults_decorated_with_honest_predicates() {
        assert_eq!(WindowStyle::default(), WindowStyle::Decorated);
        assert!(!WindowStyle::Decorated.is_frameless());
        assert!(!WindowStyle::Decorated.is_transparent());
        assert!(!WindowStyle::Decorated.wants_drag_region());
        for style in [
            WindowStyle::Undecorated,
            WindowStyle::Transparent,
            WindowStyle::Extended,
        ] {
            assert!(style.is_frameless(), "{style:?} strips the title bar");
        }
        assert!(WindowStyle::Transparent.is_transparent());
        assert!(!WindowStyle::Undecorated.is_transparent());
        assert!(!WindowStyle::Extended.is_transparent());
        assert!(WindowStyle::Extended.wants_drag_region());
        assert!(!WindowStyle::Undecorated.wants_drag_region());
        assert!(!WindowStyle::Transparent.wants_drag_region());
    }

    #[test]
    fn window_icon_new_refuses_bad_shapes_loudly() {
        assert!(WindowIcon::new(vec![0; 16], 2, 2).is_ok());
        assert!(WindowIcon::new(vec![], 0, 2).is_err(), "zero width refused");
        assert!(
            WindowIcon::new(vec![], 2, 0).is_err(),
            "zero height refused"
        );
        assert!(
            WindowIcon::new(vec![0; 15], 2, 2).is_err(),
            "short buffer refused"
        );
        assert!(
            WindowIcon::new(vec![0; 17], 2, 2).is_err(),
            "long buffer refused"
        );
        assert!(
            WindowIcon::new(vec![0; 16], 2, 2)
                .expect("valid")
                .rgba()
                .len()
                == 16,
            "accessors expose the validated pixmap"
        );
    }

    /// Decision 482: rect subtraction keeps the bar minus its
    /// blockers (a centered button splits the bar into draggable
    /// slabs; a miss leaves the bar whole; a full cover leaves
    /// nothing — never a degenerate strip).
    #[test]
    fn subtract_rect_splits_around_blockers() {
        // Miss: the bar survives whole.
        assert_eq!(
            subtract_rect(vec![[0.0, 0.0, 100.0, 40.0]], [200.0, 0.0, 10.0, 10.0]),
            vec![[0.0, 0.0, 100.0, 40.0]]
        );
        // Centered button across the full bar height: only the
        // left slab survives (right/top/bottom pieces are
        // degenerate and drop, never published).
        assert_eq!(
            subtract_rect(vec![[0.0, 0.0, 100.0, 40.0]], [80.0, 0.0, 20.0, 40.0]),
            vec![[0.0, 0.0, 80.0, 40.0]]
        );
        // Full cover: nothing remains draggable.
        assert!(subtract_rect(vec![[0.0, 0.0, 100.0, 40.0]], [0.0, 0.0, 100.0, 40.0]).is_empty());
    }

    /// Decision 494: degenerate bar boxes (zero/negative extent,
    /// e.g. a content-free `HeaderBar`) publish no drag region — a
    /// zero-area region would swallow `HTCLIENT` answers into dead
    /// `HTCAPTION` strips.
    #[test]
    fn window_drag_rects_drop_degenerate_bar_boxes() {
        use crate::component::{ComponentHost, Ctx, Props};
        use crate::style::Style;
        use crate::vnode::{Div, Row, VNode};
        #[derive(Clone)]
        struct Flat;
        impl Props for Flat {}
        fn flat_root(_ctx: &Ctx, _p: &Flat) -> VNode {
            Row(HEADER_BAR_DEBUG)
                .style(Style::new().size(0, 0))
                .child(Div("flat-leaf").style(Style::new().size(0, 0)).build())
        }
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        host.mount("Flat", Flat, flat_root);
        host.run_until_idle();
        assert!(
            window_drag_rects(&host).is_empty(),
            "content-free bar publishes nothing"
        );
    }

    /// Decision 485: `native_caption_zones` returns the caption
    /// button boxes in min/max/close order with exact geometry,
    /// skipping missing nodes (a subset renders a subset — never a
    /// fabricated zone). The tree below mounts reversed
    /// (close/max/min) to prove the order comes from the slot
    /// vocabulary, never tree order.
    #[test]
    fn native_caption_zones_follow_slot_order_with_exact_boxes() {
        use crate::component::{ComponentHost, Ctx, Props};
        use crate::style::Style;
        use crate::vnode::{Div, Row, VNode};
        #[derive(Clone)]
        struct ZoneProps {
            skip_close: bool,
        }
        impl Props for ZoneProps {}
        fn zone_root(_ctx: &Ctx, p: &ZoneProps) -> VNode {
            let mut kids = vec![
                Div("header-close").style(Style::new().size(46, 40)).build(),
                Div("header-max").style(Style::new().size(46, 40)).build(),
                Div("header-min").style(Style::new().size(46, 40)).build(),
            ];
            if p.skip_close {
                kids.remove(0);
            }
            Row(HEADER_BAR_DEBUG)
                .style(Style::new().size(300, 40))
                .children(kids)
        }
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        host.mount("Zones", ZoneProps { skip_close: false }, zone_root);
        host.run_until_idle();
        assert_eq!(
            native_caption_zones(&host),
            vec![
                (CaptionButton::Minimize, [92.0, 0.0, 46.0, 40.0]),
                (CaptionButton::Maximize, [46.0, 0.0, 46.0, 40.0]),
                (CaptionButton::Close, [0.0, 0.0, 46.0, 40.0]),
            ],
            "slot order wins over tree order, boxes exact"
        );
        let bare = ComponentHost::new();
        bare.set_viewport(800.0, 600.0);
        bare.mount("Zones", ZoneProps { skip_close: true }, zone_root);
        bare.run_until_idle();
        assert_eq!(
            native_caption_zones(&bare),
            vec![
                (CaptionButton::Minimize, [46.0, 0.0, 46.0, 40.0]),
                (CaptionButton::Maximize, [0.0, 0.0, 46.0, 40.0]),
            ],
            "missing nodes skip quietly"
        );
        assert_eq!(
            CaptionButton::Minimize.debug_label(),
            "header-min",
            "labels name the control's nodes"
        );
    }

    /// Decision 491: `title_bar_rect` unions every laid-out
    /// `HeaderBar` box unsubtracted (the OS wants the whole bar for
    /// `WM_GETTITLEBARINFOEX`, blockers included) and goes `None`
    /// with no bar.
    #[test]
    fn title_bar_rect_unions_bars_unsubtracted() {
        use crate::component::{ComponentHost, Ctx, Props};
        use crate::style::Style;
        use crate::vnode::{Div, Row, VNode};
        #[derive(Clone)]
        struct BarOnly;
        impl Props for BarOnly {}
        fn bar_root(_ctx: &Ctx, _p: &BarOnly) -> VNode {
            Row(HEADER_BAR_DEBUG)
                .style(Style::new().size(300, 40))
                .children([
                    Div("header-min").style(Style::new().size(46, 40)).build(),
                    Div("header-max").style(Style::new().size(46, 40)).build(),
                ])
        }
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        host.mount("Bar", BarOnly, bar_root);
        host.run_until_idle();
        assert_eq!(
            title_bar_rect(&host),
            Some([0.0, 0.0, 300.0, 40.0]),
            "union covers the whole bar, buttons included"
        );
        #[derive(Clone)]
        struct Bare;
        impl Props for Bare {}
        fn bare_root(_ctx: &Ctx, _p: &Bare) -> VNode {
            Div("plain").style(Style::new().size(10, 10)).build()
        }
        let empty = ComponentHost::new();
        empty.set_viewport(800.0, 600.0);
        empty.mount("Bare", Bare, bare_root);
        empty.run_until_idle();
        assert_eq!(title_bar_rect(&empty), None, "no bar means no rect");
    }
}
