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
    CaretPaint, Color, ComponentHost, Ctx, FilePickerOptions, HandlePaint, ImageCache, MockClock,
    PickError, PickedFile, RendererBackend, SelectionPaint, SurfaceDesc, ThemeMode,
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
    /// (the old `expect` wording, behavior unchanged). Decision 520:
    /// the keyframe twin eats the same diffs in the same order at
    /// the frame clock (the headless hook rig's contract — stepped
    /// keyframes, springs, and exit fades sample it during sync, so
    /// production web matches the rig instead of freezing them).
    fn commit(&mut self, host: &ComponentHost) -> Result<(), String> {
        let diffs = host.diffs_from(self.cursor);
        let now = host.runtime().now_secs();
        host.with_retained_mut(|rec, styles| {
            for diff in &diffs {
                self.dom.commit(diff).expect("dom commit failed");
            }
            self.dom.track_animation(&diffs, rec, styles, now);
        });
        self.cursor = host.diff_count();
        // Decision 520: re-showed exits cancel on the pixel legs
        // through the host table rebuild — the DOM registry drops the
        // kept ghosts explicitly through this drain instead.
        let cancelled = host.drain_cancelled_exits();
        if !cancelled.is_empty() {
            self.dom.cancel_kept_exits(&cancelled);
        }
        Ok(())
    }

    /// The focused selection rides both the DOM highlight divs and
    /// the shared-plan rasterizer path (Round 8.2: the DOM half).
    fn set_selection(&mut self, sel: Option<SelectionPaint>) {
        self.dom.set_selection(sel);
    }

    /// No caret twin (decision 472): verdict-(b) leaves the caret
    /// to the browser, so a framework bar would double the native
    /// one — this leg of the shared overlay publish is a quiet
    /// no-op (the pixel adapters still paint through the shared
    /// builder).
    fn set_caret(&mut self, _caret: Option<CaretPaint>) {}

    /// The handle pair rides the same glue (decision 454: the DOM half).
    fn set_handles(&mut self, handles: Option<(HandlePaint, HandlePaint)>) {
        self.dom.set_handles(handles);
    }

    /// The page chrome follows the host mode every frame (startup
    /// resolved it from `prefers-color-scheme`; toggles flow through
    /// the patch stanza).
    fn set_theme_mode(&mut self, mode: ThemeMode) -> Result<(), String> {
        self.dom.set_theme_mode(mode);
        Ok(())
    }

    /// The page chrome follows the host palette every frame
    /// (decision 477 — custom tokens recolor the body chrome and
    /// the theme stanza like a toggle; unset stays built-in).
    fn set_theme_tokens(&mut self, tokens: oppa::ThemeTokens) {
        self.dom.set_theme_tokens(tokens);
    }

    fn set_images(&mut self, cache: &ImageCache) {
        self.dom.set_images(cache.clone());
    }

    /// The shared refit contract (resize/DPR/theme rebase): recreate
    /// the DOM surface at the new extent and replay the scene from
    /// zero, byte-identical to the pixel adapters' rebase rule
    /// (decision 470 drives this per container resize; present for
    /// the engine contract, loud on misuse).
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
    /// Decision 520: the twin settles + samples at the frame clock on
    /// every paint (diff-less animation frames still retire tracks
    /// and step ghosts — the hook rig's contract; without this the
    /// twin goes stale between commits and fades freeze).
    fn paint(&mut self, host: &ComponentHost, plan: &oppa::FramePlan) -> Result<usize, String> {
        let now = host.runtime().now_secs();
        let stats = host.with_retained_mut(|rec, styles| {
            self.dom.track_animation(&[], rec, styles, now);
            // Decisions 521-522: publish live layout track specs
            // (the backend samples them on its own clock — host-side
            // settle must never freeze a lagging backend mid-flight).
            let specs = host.with_evaluator(|ev| ev.layout_track_specs());
            self.dom.set_layout_anim(specs);
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
    // The priming take above consumes the theme stanza into a patch
    // the browser never sees (the full page's own `<body style>` is
    // dropped by the bootstrap's `innerHTML` mount). Re-arm so the
    // first interaction re-emits the current theme and the page
    // chrome heals instead of sitting light under dark controls.
    pres.dom.rearm_theme_stanza();
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
    /// File-pick generation counter (decision 423 — the fetch
    /// generation precedent: JS never sees raw keys, only
    /// generations riding `f64`).
    pick_generation: u64,
    /// Outstanding pick request, if a chooser is open.
    pending_pick: Option<(u64, FilePickerOptions)>,
    /// Last settled pick result (level-triggered — re-reads repeat
    /// it; dismissal is `Ok(vec![])`, never a missing read).
    last_pick: Option<Result<Vec<PickedFile>, PickError>>,
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
    (full_reload(pres), touched)
}

/// Full-page reload JSON (decision 470 — factored out of the
/// settled-suite fallback above so resizes reuse the exact shape):
/// the full page embeds tree + sheet alike, so prime the snapshot
/// and the shipped revision together (decision 392).
fn full_reload(pres: &mut DomPresenter) -> String {
    // Loud fallback (unprimed snapshot or remount-class root change --
    // today's full swap inside a reload op, focus net in the applier;
    // proved unreachable in the settled suite, kept for completeness).
    // The full page embeds tree + sheet alike, so prime the snapshot
    // and the shipped revision together (decision 392).
    let html = render_page("oppa web", &pres.dom, &pres.sheet);
    pres.dom.mark_rendered(&pres.sheet);
    format!(
        "{{\"v\":1,\"full\":true,\"reload\":{},\"swaps\":[],\"attrs\":[],\"sels\":[],\"spacers\":[],\"removes\":[],\"places\":[]}}",
        oppa_dom::json_escape(&html)
    )
}

impl WasmHost {
    /// Boots the rig (clock + DejaVu measuring + OS theme) without
    /// mounting: for roots needing pre-mount wiring (demo signals,
    /// preloaded images) -- mount + settle through the plain host
    /// API, then finish with [`WasmHost::shell`].
    pub fn boot() -> (Rc<MockClock>, ComponentHost) {
        Self::boot_with_fonts(
            oppa_fonts::DEJAVU_SANS_FAMILY,
            &[("DejaVuSans.ttf", oppa_fonts::DEJAVU_SANS)],
        )
    }

    /// Boots the rig with app-bundled measuring fonts (Round 27.1):
    /// same contract as [`boot`](Self::boot), but the rustybuzz
    /// service shapes from `faces` under `family` instead of the
    /// bundled DejaVu. Pair with [`shell_with_fonts`](Self::shell_with_fonts)
    /// so the browser displays the same outlines it measured.
    pub fn boot_with_fonts(
        family: &str,
        faces: &[(&str, &[u8])],
    ) -> (Rc<MockClock>, ComponentHost) {
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        host.set_viewport(VW, VH);
        // Round 6.3: the bundled-faces rustybuzz service -- the same
        // bytes on every platform, so web text measures real advances
        // instead of zero (the documented serviceless rule it
        // replaces). The expect is the loud arm (fixed bytes that
        // fail to parse are a packaging bug, never a silent zero);
        // skip notes name no faces here (known-good faces in, zero
        // expected out).
        let (service, skipped) =
            oppa_text_rustybuzz::RustybuzzService::from_bytes_with_chain(faces, &[])
                .expect("oppa-web: bundled app fonts parse");
        let _ = skipped;
        host.set_text_service(Box::new(service));
        host.set_layout_config(oppa::LayoutTextConfig {
            family: family.to_string(),
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
            pick_generation: 0,
            pending_pick: None,
            last_pick: None,
        }
    }

    /// Builds the DOM shell with an explicit display face (Round
    /// 27.1): like [`shell`](Self::shell), plus the browser serves
    /// `family` from `display_bytes` (single `@font-face` — the
    /// variable-font rule: browsers interpolate weights from it
    /// while the DOM backend emits no `font-weight`, so display
    /// weight differences stay browser-synthesized exactly like the
    /// shipped single-face DejaVu). Registered before the first
    /// `html()` call, so the page CSS carries it before any text
    /// renders.
    pub fn shell_with_fonts(
        host: ComponentHost,
        clock: Rc<MockClock>,
        images: ImageCache,
        family: &str,
        display_bytes: &[u8],
    ) -> Self {
        let mut me = Self::shell(host, clock, images);
        me.pres.dom.register_font(family, display_bytes);
        me
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
        pump_host(&host, &clock, oppa_app::PUMP_INPUT_FRAMES);
        let images = host.images();
        Self::shell(host, clock, images)
    }

    /// Boots the rig with app-bundled fonts, mounts `render`, and
    /// primes the shell (Round 27.1): `family` + `faces` drive
    /// measuring (see [`boot_with_fonts`](Self::boot_with_fonts))
    /// while the browser displays `display_bytes` under the same
    /// family (see [`shell_with_fonts`](Self::shell_with_fonts)).
    /// The one-call path for font-overriding roots.
    pub fn mount_root_with_fonts<P: oppa::Props>(
        name: &str,
        props: P,
        render: fn(&Ctx, &P) -> oppa::VNode,
        family: &str,
        faces: &[(&str, &[u8])],
        display_bytes: &[u8],
    ) -> Self {
        let (clock, host) = Self::boot_with_fonts(family, faces);
        host.mount(name, props, render);
        pump_host(&host, &clock, oppa_app::PUMP_INPUT_FRAMES);
        let images = host.images();
        Self::shell_with_fonts(host, clock, images, family, display_bytes)
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

    /// Pumps the host with flowing mock time (decision 514): bare
    /// `run_until_idle` spins forever under a frozen clock while any
    /// transition lives (micro tweens fire on every press/hover now)
    /// — real runners advance time between pumps (rAF `tick`), so
    /// the harness steps 1/60s per demanded frame. Gesture pumps
    /// drain full input cascades
    /// ([`PUMP_INPUT_FRAMES`](oppa_app::PUMP_INPUT_FRAMES),
    /// decision 515 — live loops simply continue instead of
    /// freezing); idle trees pump zero frames with zero clock drift
    /// (identical to `run_until_idle` when quiet).
    pub(crate) fn pump(&mut self) {
        pump_host(
            self.session.host(),
            &self.clock,
            oppa_app::PUMP_INPUT_FRAMES,
        );
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
        self.pump();
        self.sync_page()
    }

    /// Pointer move at CSS px (hover). Returns the DOM patch JSON,
    /// or `None` when untouched (moves over handler-less gaps
    /// change nothing and must never fail -- decision 95).
    pub fn hover(&mut self, x: f32, y: f32) -> Option<String> {
        self.session
            .host()
            .inject_input(oppa::InputEvent::pointer_move(x, y));
        self.pump();
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
        self.pump();
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
        self.pump();
        self.sync_page()
    }

    /// Boot page chrome as stanza-shaped JSON (`{"bg","ink","dark"}`,
    /// the exact object `PagePatch.theme` carries): the full page's
    /// own `<body style>` is dropped by the bootstrap's `innerHTML`
    /// mount, so the shared bootstrap applies this at mount and the
    /// page paints in-theme from the first frame instead of healing
    /// on first interaction. Pure read (no snapshot, no marker).
    pub fn theme_chrome(&self) -> String {
        let mode = self.session.host().theme().mode();
        // Decision 477: boot chrome follows custom tokens too (an
        // overridden palette paints in-theme from the first frame,
        // same as the mode palettes always have).
        let t = self.session.host().theme_tokens();
        format!(
            "{{\"bg\":\"#{:06x}\",\"ink\":\"#{:06x}\",\"dark\":{}}}",
            t.background.0,
            t.text_primary.0,
            mode == oppa::ThemeMode::Dark
        )
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
        self.pump();
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
        self.pump();
        self.sync_page()
    }

    /// Starts a file pick (decision 423 — the web half of the
    /// bytes open seam, same generation rendezvous as
    /// [`WasmHost::fetch_start`]): records the outstanding options
    /// and returns its generation for the JS chooser callback
    /// (`f64`, exact to 2^53). A second start supersedes the first
    /// (stale resolves discard, never half-swap). The JS chooser
    /// glue (`<input type=file>` / File System Access + `FileReader`
    /// bytes) calls [`WasmHost::file_pick_resolve`] — no chooser
    /// exists yet (Blocked), so host builds resolve synthetically.
    pub fn file_pick_start(&mut self, options: FilePickerOptions) -> f64 {
        self.pick_generation += 1;
        let gen = self.pick_generation;
        self.pending_pick = Some((gen, options));
        gen as f64
    }

    /// Resolves a file pick from the chooser callback (decision 423):
    /// `Ok(files)` settles picked bytes (`[]` = dismissed),
    /// `Err` settles refusal/failure loudly. Applies only when
    /// `generation` is still current (stale results discard →
    /// `false`, never half-swapped state; garbage generations
    /// saturate through `as u64` into a mismatch — discarded, never
    /// panicking). Returns `true` when the result latched.
    pub fn file_pick_resolve(
        &mut self,
        generation: f64,
        result: Result<Vec<PickedFile>, PickError>,
    ) -> bool {
        if self.pending_pick.as_ref().map(|(g, _)| *g) != Some(generation as u64) {
            return false;
        }
        self.pending_pick = None;
        self.last_pick = Some(result);
        true
    }

    /// Outstanding pick generation, if a chooser is open.
    pub fn pending_pick_generation(&self) -> Option<u64> {
        self.pending_pick.as_ref().map(|(g, _)| *g)
    }

    /// Last settled pick result, if any (level-triggered re-reads
    /// repeat it — the [`FileOpener`](oppa::FileOpener) rule).
    pub fn last_pick_result(&self) -> Option<Result<Vec<PickedFile>, PickError>> {
        self.last_pick.clone()
    }

    /// rAF tick: advances TIME to `now_ms` and runs the frame.
    /// Returns the DOM patch JSON, or `None` when untouched
    /// (transition tails settle through these calls). Bounded
    /// (decision 515 — the clock is already set, so frames sample
    /// `now` without advancing; live loops continue next tick).
    pub fn tick(&mut self, now_ms: f64) -> Option<String> {
        self.clock.set(now_ms / 1000.0);
        // Round 21.1 (decision 328): due component timers fire on
        // the frame clock before the settle (callbacks schedule --
        // the settle below runs them -- then the sync renders).
        self.session.host().tick_timers(now_ms);
        self.session.host().pump_frames(oppa_app::PUMP_TICK_FRAMES);
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
        self.pump();
        self.sync_page()
    }

    /// Follows the scene container's size (decision 470 — the
    /// bootstrap's `ResizeObserver` calls this with the container's
    /// content-box CSS px plus the live `devicePixelRatio`): the
    /// scene fills its container, whatever size it is — the shell
    /// owns the sizing policy (fixed default, fluid opt-in), the
    /// framework follows.
    ///
    /// Loud refusals (never silent geometry): non-finite or negative
    /// `w`/`h`/`dpr`, or non-positive `dpr`, panic — a measured rect
    /// is always finite and a ratio always positive, so anything else
    /// is a wiring bug. Zero-area (`w == 0 || h == 0`, minimized or
    /// detached container) parks instead: the last scene stays, `None`
    /// returns, and the next nonzero resize resumes — a platform
    /// state, never an authoring bug. Same viewport + same DPR
    /// returns `None` without touching anything (resize storms must
    /// never spin frames on identical readings).
    ///
    /// On change: viewport first (decision-248 root invalidation),
    /// then the DPR read-modify-write through `layout_config` (a DPR
    /// flip with identical CSS sizes still reflows — decision 259),
    /// settle, then the shared refit contract (surface recreate +
    /// replay from zero) and a full reload patch — correctness first:
    /// keyed resize patches stay a follow-up, never a silent half
    /// sync. Returns the full-page reload JSON.
    pub fn resize(&mut self, w: f32, h: f32, dpr: f32) -> Option<String> {
        for (name, v) in [("w", w), ("h", h), ("dpr", dpr)] {
            if !v.is_finite() {
                panic!(
                    "oppa-web: resize {name} is non-finite ({v}) — refused, never a silent size"
                );
            }
        }
        if w < 0.0 || h < 0.0 {
            panic!("oppa-web: resize negative extent ({w}x{h}) — refused, never a silent size");
        }
        if dpr <= 0.0 {
            panic!("oppa-web: resize non-positive dpr ({dpr}) — refused, never a silent ratio");
        }
        if w == 0.0 || h == 0.0 {
            return None;
        }
        let host = self.session.host();
        let config = host.layout_config();
        // Bit-exact DPR compare (exact readings must match exactly;
        // float `==` would trip `clippy::float_cmp`).
        if host.viewport_size() == (w, h) && config.device_pixel_ratio.to_bits() == dpr.to_bits() {
            return None;
        }
        host.set_viewport(w, h);
        if config.device_pixel_ratio.to_bits() != dpr.to_bits() {
            host.set_layout_config(oppa::LayoutTextConfig {
                device_pixel_ratio: dpr,
                ..config
            });
        }
        pump_host(host, &self.clock, oppa_app::PUMP_INPUT_FRAMES);
        let bg = host.theme_tokens().background;
        self.pres
            .refit(host, w as u32, h as u32, bg)
            .expect("dom presenter refit failed");
        self.pump();
        Some(full_reload(&mut self.pres))
    }
}

/// Static flowing pump for boot paths holding host + clock directly
/// (same contract as [`WasmHost::pump`], with an explicit frame
/// budget — decision 515).
pub(crate) fn pump_host(host: &ComponentHost, clock: &MockClock, max_frames: usize) {
    for _ in 0..max_frames {
        if !host.run_once() {
            return;
        }
        clock.advance(1.0 / 60.0);
    }
}

/// Standard wasm app binding: generates the `#[wasm_bindgen]` shell
/// over [`WasmHost`] for an app root, so app crates name their root
/// once instead of hand-forwarding every binding (the template's old
/// ~40-line `HelloApp` is one line now).
///
/// ```rust,ignore
/// use oppa_web::export_app;
/// use wasm_bindgen::prelude::*;
/// export_app!(HelloApp, "Hello", hello);
/// ```
///
/// Requirements on the app crate (loud otherwise): a `wasm-bindgen`
/// dependency with `use wasm_bindgen::prelude::*;` in scope (the
/// attribute must resolve), and the `oppa-web` dependency under its
/// default name (the shell paths name it). The generated struct
/// forwards `html` / `click` / `hover` / `key` / `text` / `tick` /
/// `sync_system_theme` / `resize` / `theme_chrome` — the exact surface
/// the shared bootstrap drives.
///
/// The 6-argument arm additionally overrides the bundled fonts
/// (Round 27.1): `$family` + `$faces` drive measuring while the
/// browser displays `$display` under the same family (see
/// `mount_root_with_fonts`) — e.g. Readex Pro statics for measure
/// with the variable file for display.
#[macro_export]
macro_rules! export_app {
    ($name:ident, $title:literal, $root:path) => {
        #[wasm_bindgen]
        pub struct $name {
            app: ::oppa_web::WasmHost,
        }

        #[wasm_bindgen]
        impl $name {
            #[wasm_bindgen(constructor)]
            pub fn new() -> Self {
                Self {
                    app: ::oppa_web::WasmHost::mount_root($title, (), $root),
                }
            }

            pub fn html(&mut self) -> String {
                self.app.html()
            }

            pub fn click(&mut self, x: f32, y: f32) -> Option<String> {
                self.app.click(x, y)
            }

            pub fn hover(&mut self, x: f32, y: f32) -> Option<String> {
                self.app.hover(x, y)
            }

            pub fn key(&mut self, code: u32, pressed: bool) -> Option<String> {
                self.app.key(code, pressed)
            }

            pub fn text(&mut self, pid: &str, value: &str) -> Option<String> {
                self.app.text(pid, value)
            }

            pub fn tick(&mut self, now_ms: f64) -> Option<String> {
                self.app.tick(now_ms)
            }

            pub fn sync_system_theme(&mut self) -> Option<String> {
                self.app.sync_system_theme()
            }

            pub fn resize(&mut self, w: f32, h: f32, dpr: f32) -> Option<String> {
                self.app.resize(w, h, dpr)
            }

            pub fn theme_chrome(&self) -> String {
                self.app.theme_chrome()
            }
        }
    };
    ($name:ident, $title:literal, $root:path, $family:expr, $faces:expr, $display:expr) => {
        #[wasm_bindgen]
        pub struct $name {
            app: ::oppa_web::WasmHost,
        }

        #[wasm_bindgen]
        impl $name {
            #[wasm_bindgen(constructor)]
            pub fn new() -> Self {
                Self {
                    app: ::oppa_web::WasmHost::mount_root_with_fonts(
                        $title,
                        (),
                        $root,
                        $family,
                        $faces,
                        $display,
                    ),
                }
            }

            pub fn html(&mut self) -> String {
                self.app.html()
            }

            pub fn click(&mut self, x: f32, y: f32) -> Option<String> {
                self.app.click(x, y)
            }

            pub fn hover(&mut self, x: f32, y: f32) -> Option<String> {
                self.app.hover(x, y)
            }

            pub fn key(&mut self, code: u32, pressed: bool) -> Option<String> {
                self.app.key(code, pressed)
            }

            pub fn text(&mut self, pid: &str, value: &str) -> Option<String> {
                self.app.text(pid, value)
            }

            pub fn tick(&mut self, now_ms: f64) -> Option<String> {
                self.app.tick(now_ms)
            }

            pub fn sync_system_theme(&mut self) -> Option<String> {
                self.app.sync_system_theme()
            }

            pub fn resize(&mut self, w: f32, h: f32, dpr: f32) -> Option<String> {
                self.app.resize(w, h, dpr)
            }

            pub fn theme_chrome(&self) -> String {
                self.app.theme_chrome()
            }
        }
    };
}
