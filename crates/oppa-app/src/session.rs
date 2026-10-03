// ---------------------------------------------------------------------------
// SceneSession + ScenePresenter — the platform-free loop seam
// ---------------------------------------------------------------------------
//
// The loop logic the pixel loops (desktop CPU/GPU through `RgbPresenter`,
// Android through the same adapter) and the web DOM loop share: host +
// `FramePlanBuilder` + diff-cursor replay into presenters, selection/caret
// overlay publish, theme-mode publish with the page-background refit
// trigger, plan build, image sync, the step ladder, the resize/DPR rules,
// and the plan/caret telemetry. The adapter sibling — [`RgbPresenter`]
// (pixel surfaces) or the web `DomPresenter` — owns only its own present
// legs (surfaces, present machinery, readout / DOM sync machinery).
//
// Order inside the loop rule carries behavior semantics: commit →
// selection/caret publish → theme publish (+ refit trigger on page-bg
// change) → plan build → telemetry → set_images → paint. Moved verbatim
// from the decision-397 `SceneCore::repaint`; error wording ("desktop
// loop: ...") stays load-bearing — tests grep it.

use super::*;
use oppa::{CaretPaint, FramePlan, ImageCache, SelectionPaint, ThemeMode, ThemeTokens};
use oppa_cpu::FramePlanBuilder;

/// One presenter adapter of the shared [`SceneSession`] loop: the
/// mechanical seam both present sides fill. Method names match the
/// backend APIs (`commit`, `set_selection`, `set_caret`,
/// `set_theme_mode`, `set_images`, `paint`) so implementations are
/// mechanical. DOM-only specifics (`sync`, `take_patch`, `render_page`)
/// deliberately stay OUT of this trait — they are backend-adapter
/// state, not engine logic.
pub trait ScenePresenter {
    /// Replays the scene's diffs per the adapter's cursor policy (the
    /// pixel adapters replay every diff so fresh surfaces present the
    /// whole scene; the DOM adapter walks its committed cursor).
    fn commit(&mut self, host: &ComponentHost) -> Result<(), String>;

    /// Publishes the focused selection overlay (the pixel adapters
    /// stamp it through the shared plan; the DOM adapter highlights
    /// both the DOM divs and the plan rasterizer leg).
    fn set_selection(&mut self, sel: Option<SelectionPaint>);

    /// Publishes the focused caret overlay.
    fn set_caret(&mut self, caret: Option<CaretPaint>);

    /// Publishes the host theme mode (the page-chrome leg the DOM
    /// adapter forwards into the stylesheet; the pixel adapters chase
    /// the page background through the refit contract below).
    fn set_theme_mode(&mut self, mode: ThemeMode) -> Result<(), String>;

    /// Syncs the adapter's static-image deposits (the pixel adapters
    /// deposit into the active arm(s) at paint; the DOM adapter hands
    /// the cache to the backend).
    fn set_images(&mut self, cache: &ImageCache);

    /// Recreates the adapter's paint surface(s) at `width_px` /
    /// `height_px` device px with the tracked page background and
    /// replays the scene commits (the shared resize/DPR/theme rebase
    /// leg).
    fn refit(
        &mut self,
        host: &ComponentHost,
        width_px: u32,
        height_px: u32,
        background: oppa::Color,
    ) -> Result<(), String>;

    /// The present step: paints `plan` (and, for the DOM adapter,
    /// syncs the page first — the DOM sync needs the committed plan
    /// order, so it rides the adapter, not the trait contract). An
    /// adapter re-derives what it needs from `host` here. Returns the
    /// adapter's frame metric (plan damage-rect count for the pixel
    /// adapters, touched-work count for the DOM adapter). Pixels
    /// failures are loud `Err`s.
    fn paint(&mut self, host: &ComponentHost, plan: &FramePlan) -> Result<usize, String>;
}

/// The platform-free scene session: host + plan builder + the loop
/// rule above + the resize/DPR rules. Presenters differ per adapter;
/// the session is the one place the loop lives.
pub struct SceneSession {
    pub(crate) host: ComponentHost,
    pub(crate) builder: FramePlanBuilder,
    /// Paint surface size in device px (physical pixels).
    pub(crate) viewport: (u32, u32),
    /// Output scale: device px per CSS px (Round 2.4, OQ-G10-2).
    /// The host viewport, layout config, and builder all derive
    /// from (`viewport`, `dpr`) — never set one without the others
    /// (see [`SceneSession::resize`] / [`SceneSession::set_device_pixel_ratio`]).
    pub(crate) dpr: f32,
    /// Page-background contract state (the theme policy leg): both
    /// paint surfaces clear to the host theme's `background`; a
    /// toggle refits them instead of presenting a stale page.
    pub(crate) page_bg: oppa::Color,
    /// The last committed frame plan (Round 15.2, decision 313 —
    /// test/presenter observability: proves which overlay `Rect`s a
    /// repaint committed; one retained plan, refreshed every
    /// repaint, never a growing history).
    last_plan: Option<FramePlan>,
    /// Caret overlay presence at the last repaint (Round 15.2,
    /// decision 313 — the blink demand: the flip writes no signals,
    /// so the tick path compares presence instead of waiting for
    /// dirt that never comes).
    last_caret_shown: bool,
}

impl SceneSession {
    /// Wraps an existing host + builder + initial viewport (both
    /// construction orders converge here through the adapters'
    /// boots; the page background seeds from the host theme).
    pub fn from_parts(
        host: ComponentHost,
        builder: FramePlanBuilder,
        viewport: (u32, u32),
        dpr: f32,
    ) -> Self {
        let page_bg = ThemeTokens::of(host.theme().mode()).background;
        Self {
            host,
            builder,
            viewport,
            dpr,
            page_bg,
            last_plan: None,
            last_caret_shown: false,
        }
    }

    /// The host (shared by every runner and the testkit escape-hatch
    /// rule).
    pub fn host(&self) -> &ComponentHost {
        &self.host
    }

    /// The last committed frame plan (decision 313; `None` before
    /// the first repaint, never a live borrow).
    pub fn last_plan(&self) -> Option<FramePlan> {
        self.last_plan.clone()
    }

    /// True when the caret overlay's presence flipped since the last
    /// repaint (decision 313 — the blink demand). Pure over the
    /// host state above.
    pub fn caret_overlay_dirty(&self) -> bool {
        self.host.focused_caret_paint().is_some() != self.last_caret_shown
    }

    /// The unified loop rule (see the module docs): one
    /// implementation of the prologue every adapter shares. Returns
    /// the plan's damage-rect count; presenter failures are loud.
    pub fn repaint<P: ScenePresenter + ?Sized>(&mut self, pres: &mut P) -> Result<usize, String> {
        pres.commit(&self.host)?;
        // Round 15.2 (decision 313): the text overlays ride every
        // committed frame — the shared build-scoped rule from the
        // paint hooks (selection + caret resolved outside the
        // retained borrow). Before this, live presented frames
        // committed with both overlays `None`: zero selection rects
        // and zero carets on GPU and GDI alike.
        let selection = self.host.focused_selection_paint();
        let caret = self.host.focused_caret_paint();
        pres.set_selection(selection);
        pres.set_caret(caret);
        self.builder.set_selection(selection);
        self.builder.set_caret(caret);
        // Theme contract round: the build theme + page chrome follow
        // the host mode every frame, and a toggled page background
        // refits the presenter's surfaces before painting (same
        // one-refit path as resize/DPR — steady state is a color
        // compare, never surface churn).
        let mode = self.host.theme().mode();
        self.builder.set_theme_mode(mode);
        pres.set_theme_mode(mode)?;
        let want_bg = ThemeTokens::of(mode).background;
        if want_bg != self.page_bg {
            self.page_bg = want_bg;
            let (w, h) = self.viewport;
            pres.refit(&self.host, w, h, want_bg)?;
        }
        let plan = self
            .host
            .with_retained_mut(|rec, styles| self.builder.build_full(rec, styles));
        self.last_caret_shown = self.builder.caret().is_some();
        self.last_plan = Some(plan.clone());
        let images = self.host.images();
        pres.set_images(&images);
        // The adapter's frame metric (see the `paint` contract
        // above): plan damage-rect count for the pixel adapters
        // (`RgbPresenter::paint` returns exactly this), touched-work
        // count for the DOM adapter (returning plan damage here sent
        // damage-but-untouched DOM frames down the full-page
        // fallback — e.g. background presses and converged value
        // re-feeds reported work that never touched the page).
        let touched = pres.paint(&self.host, &plan)?;
        Ok(touched)
    }

    /// Settles and repaints iff the caret overlay flipped (Round
    /// 15.2, decision 313 — the timer-tick path runners call when
    /// no input arrived): returns `true` when it repainted, `false`
    /// when settled-quiet (never a spin — a quiet poll schedules
    /// nothing). Paint failures are loud `Err`s like the loop rule.
    pub fn poll_blink<P: ScenePresenter + ?Sized>(&mut self, pres: &mut P) -> Result<bool, String> {
        self.host.run_until_idle();
        if self.caret_overlay_dirty() {
            self.repaint(pres)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Fires due component timers, settling + repainting when
    /// anything fired (Round 21.1, decision 328 — the timer-tick
    /// path runners call on wake): returns the fired count. Zero
    /// when nothing was due (quiet). Paint failures are loud `Err`s.
    pub fn tick_timers<P: ScenePresenter + ?Sized>(
        &mut self,
        pres: &mut P,
    ) -> Result<usize, String> {
        let now = self.host.now_ms();
        let fired = self.host.tick_timers(now);
        if fired > 0 {
            self.host.run_until_idle();
            self.repaint(pres)?;
        }
        Ok(fired)
    }

    /// Tracks a resize: refits the viewport and rebuilds the paint
    /// surface (positions derive from the viewport, so refit alone
    /// leaves the scene stale). `width_px`/`height_px` are device px; the host
    /// viewport divides by the DPR (identity at 1.0 — the only
    /// previously reachable state). Layout invalidation rides
    /// `set_viewport` (decision 248 — the trailing settle actually
    /// re-runs layout now). Same-size calls are a no-op. Loud on
    /// failure.
    pub fn resize<P: ScenePresenter + ?Sized>(
        &mut self,
        pres: &mut P,
        width_px: u32,
        height_px: u32,
    ) -> Result<(), String> {
        let (w, h) = (width_px.max(1), height_px.max(1));
        if (w, h) == self.viewport {
            return Ok(());
        }
        self.viewport = (w, h);
        self.host
            .set_viewport(w as f32 / self.dpr, h as f32 / self.dpr);
        pres.refit(&self.host, w, h, self.page_bg)?;
        self.host.run_until_idle();
        Ok(())
    }

    /// Current output scale (device px per CSS px).
    pub fn dpr(&self) -> f32 {
        self.dpr
    }

    /// Viewport in device px.
    pub fn viewport(&self) -> (u32, u32) {
        self.viewport
    }

    /// Re-bases the loop on a new device pixel ratio keeping the CSS
    /// viewport stable (Round 2.4, OQ-G10-2 — the runners call this
    /// on DPI/scale changes): layout config + builder follow, the
    /// host viewport rescales to CSS, the presenter refits to device
    /// px, and layout re-runs (measure keys carry the DPR, so text
    /// re-shapes; the `set_viewport` dirt from decision 248 reflows
    /// boxes — nothing stale survives at the new scale). Same-value
    /// calls are a no-op. Non-finite or non-positive scales panic
    /// loudly (a scale is never zero/NaN — shells map raw OS values
    /// through `dpr_from_dpi`/`dpr_from_scale_factor` first).
    pub fn set_device_pixel_ratio<P: ScenePresenter + ?Sized>(
        &mut self,
        pres: &mut P,
        dpr: f32,
    ) -> Result<(), String> {
        if !dpr.is_finite() || dpr <= 0.0 {
            panic!(
                "desktop loop: device pixel ratio {dpr} refused — scales are finite and positive"
            );
        }
        if dpr == self.dpr {
            return Ok(());
        }
        // CSS size is viewport_px / old_dpr (exact at 1.0, the only
        // previously reachable state — no drift by construction).
        let css_w = self.viewport.0 as f32 / self.dpr;
        let css_h = self.viewport.1 as f32 / self.dpr;
        self.dpr = dpr;
        let mut config = self.host.layout_config();
        config.device_pixel_ratio = dpr;
        self.host.set_layout_config(config);
        self.builder.set_dpr(dpr);
        self.host.set_viewport(css_w, css_h);
        let w = (css_w * dpr).round().max(1.0) as u32;
        let h = (css_h * dpr).round().max(1.0) as u32;
        self.viewport = (w, h);
        pres.refit(&self.host, w, h, self.page_bg)?;
        self.host.run_until_idle();
        Ok(())
    }
}
