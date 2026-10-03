//! Reusable wasm host harness (Round 29.1, decision 344): the
//! mechanical rig every browser app needs -- the shared
//! [`SceneSession`] loop engine (`oppa_app`, decision 398) behind a
//! `DomBackend` + `StyleSheet` presenter adapter, with the
//! inject/settle/sync binding bodies -- and zero demo state.
//! Extracted verbatim from `WebApp` (which now composes it and
//! keeps only its nav/settings toggle); behavior is identical,
//! pinned by the existing host-side suite.
//!
//! App crates wrap this in ~25 lines of `#[wasm_bindgen]` glue
//! over their own root (see `templates/hello-web`): the harness is plain Rust on purpose
//! (exported constructors cannot be generic over props).

use std::rc::Rc;

use oppa::{
    CaretPaint, Color, ComponentHost, Ctx, ImageCache, MockClock, RendererBackend, SelectionPaint,
    SurfaceDesc, ThemeMode,
};
use oppa_app::{ScenePresenter, SceneSession};
use oppa_cpu::FramePlanBuilder;
use oppa_dom::{render_page, DomBackend, StyleSheet};

use super::{VH, VW};

/// The DOM presenter adapter of the shared [`SceneSession`] loop
/// (decision 398 — the web leg joins the one app-loop engine). The
/// adapter is deliberately THIN: it holds only backend-adapter
/// state (backend + stylesheet + surface + the committed diff
/// cursor) and the DOM-only mechanics stay beside it (`sync` inside
/// [`ScenePresenter::paint`], `take_patch`/`render_page` in
/// [`WasmHost`] methods — never engine logic). Its cursor policy:
/// replay everything once (`sync_and_render` keeps untouched frames
/// intact by counting, not by skipping the sync).
struct DomPresenter {
    dom: DomBackend,
    sheet: StyleSheet,
    surface: oppa::SurfaceId,
    cursor: usize,
}

impl DomPresenter {
    /// Diffs the keyed DOM patch (Round 12.1, decision 307) — DOM
    /// mechanics live here, not on the presenter trait.
    fn take_patch(&mut self) -> oppa_dom::PagePatch {
        let DomPresenter { dom, sheet, .. } = self;
        dom.take_patch(sheet)
    }
}

impl ScenePresenter for DomPresenter {
    /// Replays the scene diffs the session has not committed here
    /// yet (one committed diff cursor). Backend failures are loud
    /// (the old `expect` wording, behavior unchanged).
    fn commit(&mut self, host: &ComponentHost) -> Result<(), String> {
        for diff in host.diffs_from(self.cursor) {
            self.dom.commit(&diff).expect("dom commit failed");
        }
        self.cursor = host.diff_count();
        Ok(())
    }

    /// The focused selection rides both the DOM highlight divs and
    /// the shared-plan rasterizer path (Round 8.2: the DOM half).
    fn set_selection(&mut self, sel: Option<SelectionPaint>) {
        self.dom.set_selection(sel);
    }

    /// The caret bar rides the same glue (Round 15.1: the DOM half).
    fn set_caret(&mut self, caret: Option<CaretPaint>) {
        self.dom.set_caret(caret);
    }

    /// The page chrome follows the host mode every frame (startup
    /// resolved it from `prefers-color-scheme`; toggles flow through
    /// the patch stanza).
    fn set_theme_mode(&mut self, mode: ThemeMode) -> Result<(), String> {
        self.dom.set_theme_mode(mode);
        Ok(())
    }

    fn set_images(&mut self, cache: &ImageCache) {
        self.dom.set_images(cache.clone());
    }

    /// The shared refit contract (resize/DPR/theme rebase): recreate
    /// the DOM surface at the new extent and replay the scene from
    /// zero, byte-identical to the pixel adapters' rebase rule. The
    /// web loop never resizes today (fixed 800x600 viewport), so
    /// this arm proves unreachable in practice — present for the
    /// engine contract, loud on misuse.
    fn refit(
        &mut self,
        host: &ComponentHost,
        width_px: u32,
        height_px: u32,
        background: Color,
    ) -> Result<(), String> {
        let next = self
            .dom
            .create_surface(SurfaceDesc {
                width_px,
                height_px,
                background,
            })
            .map_err(|e| format!("dom presenter: create surface: {e:?}"))?;
        let old = self.surface;
        self.surface = next;
        let _ = self.dom.destroy_surface(old);
        self.cursor = 0;
        for diff in host.diffs_from(0) {
            self.dom.commit(&diff).expect("dom commit failed");
        }
        self.cursor = host.diff_count();
        Ok(())
    }

    /// The present step: sync the DOM against the committed scene
    /// (the touched-work count a patch frame reports) and paint the
    /// plan. Backend failures are loud (unchanged `expect` wording).
    fn paint(&mut self, host: &ComponentHost, plan: &oppa::FramePlan) -> Result<usize, String> {
        let stats = host.with_retained_mut(|rec, styles| {
            self.dom
                .sync(rec, styles, &mut self.sheet)
                .expect("dom sync failed")
        });
        self.dom
            .paint(self.surface, plan)
            .expect("dom paint failed");
        Ok(stats.touched)
    }
}

/// Shared DOM shell bring-up (Round 6.4): backend + stylesheet +
/// surface over `images`, then the first sync. Returns the shell
/// parts with the diff cursor past the initial render.
#[allow(clippy::type_complexity)]
fn boot_shell(host: ComponentHost, images: ImageCache) -> (SceneSession, DomPresenter) {
    // The shared session loop (decision 398): host + plan builder +
    // telemetry; the DOM adapter carries the rest.
    let mut session = SceneSession::from_parts(
        host,
        FramePlanBuilder::new(1.0),
        (VW as u32, VH as u32),
        1.0,
    );
    let mut dom = DomBackend::new(1.0);
    dom.set_images(images.clone());
    // Phase 36 PR3 (decision 355): the browser shapes display text
    // with the bundled DejaVu bytes — the same bytes the host
    // measures with — closing the decision-81 display-vs-measure
    // drift at the page level (residual browser-shaper differences
    // are stated tolerance, not contract).
    dom.register_font(oppa_fonts::DEJAVU_SANS_FAMILY, oppa_fonts::DEJAVU_SANS);
    let surface = dom
        .create_surface(oppa::SurfaceDesc {
            width_px: VW as u32,
            height_px: VH as u32,
            background: Color(0xFF_FF_FF),
        })
        .expect("dom surface failed");
    let mut pres = DomPresenter {
        dom,
        sheet: StyleSheet::new(1.0),
        surface,
        cursor: 0,
    };
    let (html, _) = sync_and_render(&mut session, &mut pres);
    let _ = html;
    (session, pres)
}

/// Reads the browser's `prefers-color-scheme` (Round 16.2,
/// decision 315): true when the OS asks for dark. Host builds
/// read false by cfg-gate -- `web_sys` traps on non-wasm targets
/// instead of returning `None`, so the fallback is compile-time,
/// never a runtime guess (the DPR precedent).
pub(crate) fn system_prefers_dark() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok())
            .flatten()
            .is_some_and(|q| q.matches())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// Pluggable scene runner: the mechanical rig with no demo state.
/// App crates mount their root through [`WasmHost::mount_root`]
/// and forward browser events into the binding methods; `WebApp`
/// itself composes this with its nav/settings toggle.
pub struct WasmHost {
    clock: Rc<MockClock>,
    /// The shared session loop (decision 398): host + plan builder +
    /// telemetry — the same engine the desktop/Android pixel loops
    /// drive through `SceneCore`.
    session: SceneSession,
    pres: DomPresenter,
}

/// Commit new diffs, sync the DOM, and repaint a real plan (the
/// shared head of every render path -- full pages and patches alike),
/// then diff a keyed patch (Round 12.1, decision 307). Returns
/// `(patch, touched)` -- the bootstrap applies the patch only when
/// `touched > 0` (untouched frames emit nothing, and no op ever
/// targets the focused input -- focus/caret/IME survive by
/// construction).
fn sync_and_render(session: &mut SceneSession, pres: &mut DomPresenter) -> (String, usize) {
    // The one loop rule (decision 398): commit replay, overlay
    // publish, theme publish, plan build, image sync, present
    // (the DOM sync rides the adapter's paint).
    let touched = session.repaint(pres).expect("dom session repaint failed");
    if touched == 0 {
        return (String::new(), 0);
    }
    let patch = pres.take_patch();
    if !patch.full && !patch.is_empty() {
        return (patch.to_json(), touched);
    }
    // Loud fallback (unprimed snapshot or remount-class root change --
    // today's full swap inside a reload op, focus net in the applier;
    // proved unreachable in the settled suite, kept for completeness).
    // The full page embeds tree + sheet alike, so prime the snapshot
    // and the shipped revision together (decision 392).
    let html = render_page("oppa web", &pres.dom, &pres.sheet);
    pres.dom.mark_rendered(&pres.sheet);
    let reload = format!(
        "{{\"v\":1,\"full\":true,\"reload\":{},\"swaps\":[],\"attrs\":[],\"sels\":[],\"spacers\":[],\"removes\":[],\"places\":[]}}",
        oppa_dom::json_escape(&html)
    );
    (reload, touched)
}

impl WasmHost {
    /// Boots the rig (clock + DejaVu measuring + OS theme) without
    /// mounting: for roots needing pre-mount wiring (demo signals,
    /// preloaded images) -- mount + settle through the plain host
    /// API, then finish with [`WasmHost::shell`].
    pub fn boot() -> (Rc<MockClock>, ComponentHost) {
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        host.set_viewport(VW, VH);
        // Round 6.3: the bundled DejaVu Sans rustybuzz service --
        // the same bytes on every platform, so web text measures
        // real advances instead of zero (the documented
        // serviceless rule it replaces). The expect is the loud
        // arm (fixed bytes that fail to parse are a packaging
        // bug, never a silent zero); skip notes name no faces
        // here (one known-good face in, zero expected out).
        let (service, skipped) = oppa_text_rustybuzz::RustybuzzService::from_bytes_with_chain(
            &[("DejaVuSans.ttf", oppa_fonts::DEJAVU_SANS)],
            &[],
        )
        .expect("oppa-web: bundled DejaVu Sans parses");
        let _ = skipped;
        host.set_text_service(Box::new(service));
        host.set_layout_config(oppa::LayoutTextConfig {
            family: oppa_fonts::DEJAVU_SANS_FAMILY.to_string(),
            ..Default::default()
        });
        host.set_presenter_name("DOM");
        // OS theme at startup (Round 16.2): first paint already
        // matches the system (host builds read Light -- the
        // cfg-fallback in `system_prefers_dark`, never a trap).
        host.set_theme(if system_prefers_dark() {
            oppa::ThemeMode::Dark
        } else {
            oppa::ThemeMode::Light
        });
        (clock, host)
    }

    /// Builds the DOM shell over a mounted + settled host (diff
    /// cursor past the initial render). Pair with [`WasmHost::boot`].
    pub fn shell(host: ComponentHost, clock: Rc<MockClock>, images: ImageCache) -> Self {
        let (session, pres) = boot_shell(host, images);
        Self {
            clock,
            session,
            pres,
        }
    }

    /// Boots the rig, mounts `render`, and primes the shell (diff
    /// cursor past the initial render). The one-call path for roots
    /// with no pre-mount wiring (props carry everything).
    pub fn mount_root<P: oppa::Props>(
        name: &str,
        props: P,
        render: fn(&Ctx, &P) -> oppa::VNode,
    ) -> Self {
        let (clock, host) = Self::boot();
        host.mount(name, props, render);
        host.run_until_idle();
        let images = host.images();
        Self::shell(host, clock, images)
    }

    /// The full host (the testkit escape-hatch rule applied to the
    /// web shell: the bindings cover the common path while tests
    /// read geometry, semantics, and tab order through here).
    pub fn host(&self) -> &ComponentHost {
        self.session.host()
    }

    /// Full-page render (the initial mount; also primes the patch
    /// snapshot and the shipped sheet revision). Fires the shared
    /// loop rule once, then renders the page (DOM mechanics).
    pub fn html(&mut self) -> String {
        self.session
            .repaint(&mut self.pres)
            .expect("dom session repaint failed");
        let html = render_page("oppa web", &self.pres.dom, &self.pres.sheet);
        self.pres.dom.mark_rendered(&self.pres.sheet);
        html
    }

    /// Syncs the DOM and returns the DOM patch JSON (`None` when
    /// untouched) -- the shared tail of the event bindings.
    pub(crate) fn sync_page(&mut self) -> Option<String> {
        let (patch, touched) = sync_and_render(&mut self.session, &mut self.pres);
        if touched == 0 {
            None
        } else {
            Some(patch)
        }
    }

    /// Pointer tap at CSS px: down + up through the shared pipeline.
    /// Returns the DOM patch JSON, or `None` when untouched.
    pub fn click(&mut self, x: f32, y: f32) -> Option<String> {
        self.session
            .host()
            .inject_input(oppa::InputEvent::pointer_down(x, y));
        self.session
            .host()
            .inject_input(oppa::InputEvent::pointer_up(x, y));
        self.session.host().run_until_idle();
        self.sync_page()
    }

    /// Pointer move at CSS px (hover). Returns the DOM patch JSON,
    /// or `None` when untouched (moves over handler-less gaps
    /// change nothing and must never fail -- decision 95).
    pub fn hover(&mut self, x: f32, y: f32) -> Option<String> {
        self.session
            .host()
            .inject_input(oppa::InputEvent::pointer_move(x, y));
        self.session.host().run_until_idle();
        self.sync_page()
    }

    /// Key event (`code` is the framework keycode). Returns the DOM
    /// patch JSON, or `None` when untouched.
    pub fn key(&mut self, code: u32, pressed: bool) -> Option<String> {
        self.session.host().inject_input(oppa::InputEvent::Key {
            code,
            modifiers: oppa::Modifiers::NONE,
            state: if pressed {
                oppa::KeyState::Pressed
            } else {
                oppa::KeyState::Released
            },
            repeat: false,
        });
        self.session.host().run_until_idle();
        self.sync_page()
    }

    /// Text value from a DOM field (U8): resolves the rendered
    /// `data-pid` to the retained node and feeds the full value.
    /// Returns the DOM patch JSON, or `None` when untouched or the
    /// pid is unknown (stale post-swap markup -- never a silent
    /// node, never a panic).
    pub fn text(&mut self, pid: &str, value: &str) -> Option<String> {
        let target = self.pres.dom.node_for_pid(pid)?;
        self.session
            .host()
            .inject_input(oppa::InputEvent::text(target, value));
        self.session.host().run_until_idle();
        self.sync_page()
    }

    /// Starts a platform fetch (Round 4.1, web fetch -- the wasm half
    /// of decision 221): sets the named keyed signal to `Loading`
    /// and returns its generation for the promise callback. The key
    /// derives from `name` through `fetch_key` (one source of truth
    /// -- JS never sees raw keys). Generations ride `f64` (exact to
    /// 2^53 -- counters never approach it).
    pub fn fetch_start(&mut self, name: &str) -> f64 {
        let key = oppa::fetch_key(name);
        let gen = self.session.host().start_fetch(key) as f64;
        // Loading is synchronous (the native rule -- first paint
        // already shows it): settle before returning so the next
        // `html()` syncs it.
        self.session.host().run_until_idle();
        gen
    }

    /// Resolves a platform fetch from the promise callback (Round
    /// 4.1): `ok` selects `Ready(text)` vs `Failed(text)` (HTTP
    /// status / exception message -- the bootstrap decides the
    /// text). Applies only when `generation` is still current
    /// (stale results discard -> `None`, never half-swapped state;
    /// garbage generations saturate through `as u64` into a
    /// mismatch -- discarded, never panicking). Returns the DOM patch
    /// JSON, or `None` when discarded or untouched.
    pub fn fetch_resolve(
        &mut self,
        name: &str,
        generation: f64,
        ok: bool,
        text: String,
    ) -> Option<String> {
        let key = oppa::fetch_key(name);
        let result = if ok { Ok(text) } else { Err(text) };
        if !self
            .session
            .host()
            .resolve_fetch(key, generation as u64, result)
        {
            return None;
        }
        self.session.host().run_until_idle();
        self.sync_page()
    }

    /// rAF tick: advances TIME to `now_ms` and runs the frame.
    /// Returns the DOM patch JSON, or `None` when untouched
    /// (transition tails settle through these calls).
    pub fn tick(&mut self, now_ms: f64) -> Option<String> {
        self.clock.set(now_ms / 1000.0);
        // Round 21.1 (decision 328): due component timers fire on
        // the frame clock before the settle (callbacks schedule --
        // the settle below runs them -- then the sync renders).
        self.session.host().tick_timers(now_ms);
        self.session.host().run_until_idle();
        self.sync_page()
    }

    /// Follows the OS light/dark flip (Round 16.2, decision 315 --
    /// the bootstrap's `matchMedia` listener calls this on
    /// `change`): sets the reactive theme from the live query when
    /// it differs (same-value writes still invalidate -- the signal
    /// carries no equality gate -- so a matching reading returns
    /// `None` without touching anything, never a repaint spin).
    /// Returns the DOM patch JSON, or `None` when untouched.
    pub fn sync_system_theme(&mut self) -> Option<String> {
        let mode = if system_prefers_dark() {
            oppa::ThemeMode::Dark
        } else {
            oppa::ThemeMode::Light
        };
        if self.session.host().theme().mode() == mode {
            return None;
        }
        self.session.host().set_theme(mode);
        self.session.host().run_until_idle();
        self.sync_page()
    }
}
