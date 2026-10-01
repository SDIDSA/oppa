//! Reusable wasm host harness (Round 29.1, decision 344): the
//! mechanical rig every browser app needs -- `ComponentHost` +
//! `MockClock`, `DomBackend` + `StyleSheet`, diff cursor, and the
//! inject/settle/sync binding bodies -- with zero demo state.
//! Extracted verbatim from `WebApp` (which now composes it and
//! keeps only its nav/settings toggle); behavior is identical,
//! pinned by the existing host-side suite.
//!
//! App crates wrap this in ~25 lines of `#[wasm_bindgen]` glue
//! over their own root (see `docs/09-api/web-app.md` and
//! `templates/hello-web`): the harness is plain Rust on purpose
//! (exported constructors cannot be generic over props).

use std::rc::Rc;

use oppa::{ComponentHost, Ctx, MockClock, RendererBackend};
use oppa_cpu::FramePlanBuilder;
use oppa_dom::{render_page, DomBackend, StyleSheet};

use super::{VH, VW};

/// Commit new diffs, sync the DOM, and repaint a real plan (the
/// shared head of every render path -- full pages and patches alike).
/// Returns the sync stats (the patch/full decision reads `touched`).
#[allow(clippy::too_many_arguments)]
fn commit_and_sync(
    host: &ComponentHost,
    dom: &mut DomBackend,
    sheet: &mut StyleSheet,
    surface: oppa::SurfaceId,
    builder: &FramePlanBuilder,
    cursor: &mut usize,
) -> usize {
    for diff in host.diffs_from(*cursor) {
        dom.commit(&diff).expect("dom commit failed");
    }
    *cursor = host.diff_count();
    // Round 8.2: the focused session's selection rides both the DOM
    // highlight divs and the shared-plan rasterizer path.
    // Round 15.1: the caret bar rides the same glue.
    let selection = host.focused_selection_paint();
    let caret = host.focused_caret_paint();
    dom.set_selection(selection);
    dom.set_caret(caret);
    builder.set_selection(selection);
    builder.set_caret(caret);
    // Theme contract round: the build theme + page chrome follow
    // the host mode every frame (startup already resolved it from
    // `prefers-color-scheme`; toggles flow through the patch
    // stanza and the plan's default ink alike).
    let mode = host.theme().mode();
    builder.set_theme_mode(mode);
    dom.set_theme_mode(mode);
    let stats = host
        .with_retained_mut(|rec, styles| dom.sync(rec, styles, sheet).expect("dom sync failed"));
    let plan = host.with_retained_mut(|rec, styles| builder.build_full(rec, styles));
    dom.paint(surface, &plan).expect("dom paint failed");
    stats.touched
}

/// Commit new diffs, sync the DOM, repaint a real plan, and diff a
/// keyed patch (Round 12.1, decision 307). Returns `(patch, touched)`
/// -- the bootstrap applies the patch only when `touched > 0`
/// (untouched frames emit nothing, and no op ever targets the
/// focused input -- focus/caret/IME survive by construction).
fn sync_and_render(
    host: &ComponentHost,
    dom: &mut DomBackend,
    sheet: &mut StyleSheet,
    surface: oppa::SurfaceId,
    builder: &FramePlanBuilder,
    cursor: &mut usize,
) -> (String, usize) {
    let touched = commit_and_sync(host, dom, sheet, surface, builder, cursor);
    if touched == 0 {
        return (String::new(), 0);
    }
    let patch = dom.take_patch();
    if !patch.full && !patch.is_empty() {
        return (patch.to_json(), touched);
    }
    // Loud fallback (unprimed snapshot or remount-class root change --
    // today's full swap inside a reload op, focus net in the applier;
    // proved unreachable in the settled suite, kept for completeness).
    let html = render_page("oppa web", dom, sheet);
    let reload = format!(
        "{{\"v\":1,\"full\":true,\"reload\":{},\"swaps\":[],\"attrs\":[],\"sels\":[],\"spacers\":[],\"removes\":[],\"places\":[]}}",
        oppa_dom::json_escape(&html)
    );
    (reload, touched)
}

/// Shared host boot (Rounds 6.3-6.4): clock + viewport + the
/// bundled DejaVu Sans rustybuzz service + `DejaVu Sans` layout
/// config. Every scene boots identically -- scenes differ,
/// measurement never does.
fn boot_host() -> (Rc<MockClock>, ComponentHost) {
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
    (clock, host)
}

/// Shared DOM shell bring-up (Round 6.4): backend + stylesheet +
/// surface over `images`, then the first sync. Returns the shell
/// parts with the diff cursor past the initial render.
#[allow(clippy::type_complexity)]
fn boot_shell(
    host: &ComponentHost,
    images: oppa::ImageCache,
) -> (
    DomBackend,
    StyleSheet,
    oppa::SurfaceId,
    FramePlanBuilder,
    usize,
) {
    let mut dom = DomBackend::new(1.0);
    dom.set_images(images);
    let surface = dom
        .create_surface(oppa::SurfaceDesc {
            width_px: VW as u32,
            height_px: VH as u32,
            background: oppa::Color(0xFF_FF_FF),
        })
        .expect("dom surface failed");
    let mut sheet = StyleSheet::new(1.0);
    let builder = FramePlanBuilder::new(1.0);
    let mut cursor = 0;
    let (html, _) = sync_and_render(host, &mut dom, &mut sheet, surface, &builder, &mut cursor);
    let _ = html;
    (dom, sheet, surface, builder, cursor)
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
    host: ComponentHost,
    clock: Rc<MockClock>,
    dom: DomBackend,
    sheet: StyleSheet,
    surface: oppa::SurfaceId,
    builder: FramePlanBuilder,
    cursor: usize,
}

impl WasmHost {
    /// Boots the rig (clock + DejaVu measuring + OS theme) without
    /// mounting: for roots needing pre-mount wiring (demo signals,
    /// preloaded images) -- mount + settle through the plain host
    /// API, then finish with [`WasmHost::shell`].
    pub fn boot() -> (Rc<MockClock>, ComponentHost) {
        let (clock, host) = boot_host();
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
    pub fn shell(host: ComponentHost, clock: Rc<MockClock>, images: oppa::ImageCache) -> Self {
        let (dom, sheet, surface, builder, cursor) = boot_shell(&host, images);
        Self {
            host,
            clock,
            dom,
            sheet,
            surface,
            builder,
            cursor,
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
        Self::shell(host, clock, oppa::ImageCache::new())
    }

    /// The full host (the testkit escape-hatch rule applied to the
    /// web shell: the bindings cover the common path while tests
    /// read geometry, semantics, and tab order through here).
    pub fn host(&self) -> &ComponentHost {
        &self.host
    }

    /// Full-page render (the initial mount; also primes the patch
    /// snapshot).
    pub fn html(&mut self) -> String {
        commit_and_sync(
            &self.host,
            &mut self.dom,
            &mut self.sheet,
            self.surface,
            &self.builder,
            &mut self.cursor,
        );
        let html = render_page("oppa web", &self.dom, &self.sheet);
        self.dom.mark_rendered();
        html
    }

    /// Syncs the DOM and returns the DOM patch JSON (`None` when
    /// untouched) -- the shared tail of the event bindings.
    pub(crate) fn sync_page(&mut self) -> Option<String> {
        let (patch, touched) = sync_and_render(
            &self.host,
            &mut self.dom,
            &mut self.sheet,
            self.surface,
            &self.builder,
            &mut self.cursor,
        );
        if touched == 0 {
            None
        } else {
            Some(patch)
        }
    }

    /// Pointer tap at CSS px: down + up through the shared pipeline.
    /// Returns the DOM patch JSON, or `None` when untouched.
    pub fn click(&mut self, x: f32, y: f32) -> Option<String> {
        self.host.inject_input(oppa::InputEvent::pointer_down(x, y));
        self.host.inject_input(oppa::InputEvent::pointer_up(x, y));
        self.host.run_until_idle();
        self.sync_page()
    }

    /// Pointer move at CSS px (hover). Returns the DOM patch JSON,
    /// or `None` when untouched (moves over handler-less gaps
    /// change nothing and must never fail -- decision 95).
    pub fn hover(&mut self, x: f32, y: f32) -> Option<String> {
        self.host.inject_input(oppa::InputEvent::pointer_move(x, y));
        self.host.run_until_idle();
        self.sync_page()
    }

    /// Key event (`code` is the framework keycode). Returns the DOM
    /// patch JSON, or `None` when untouched.
    pub fn key(&mut self, code: u32, pressed: bool) -> Option<String> {
        self.host.inject_input(oppa::InputEvent::Key {
            code,
            modifiers: oppa::Modifiers::NONE,
            state: if pressed {
                oppa::KeyState::Pressed
            } else {
                oppa::KeyState::Released
            },
            repeat: false,
        });
        self.host.run_until_idle();
        self.sync_page()
    }

    /// Text value from a DOM field (U8): resolves the rendered
    /// `data-pid` to the retained node and feeds the full value.
    /// Returns the DOM patch JSON, or `None` when untouched or the
    /// pid is unknown (stale post-swap markup -- never a silent
    /// node, never a panic).
    pub fn text(&mut self, pid: &str, value: &str) -> Option<String> {
        let target = self.dom.node_for_pid(pid)?;
        self.host
            .inject_input(oppa::InputEvent::text(target, value));
        self.host.run_until_idle();
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
        let gen = self.host.start_fetch(key) as f64;
        // Loading is synchronous (the native rule -- first paint
        // already shows it): settle before returning so the next
        // `html()` syncs it.
        self.host.run_until_idle();
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
        if !self.host.resolve_fetch(key, generation as u64, result) {
            return None;
        }
        self.host.run_until_idle();
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
        self.host.tick_timers(now_ms);
        self.host.run_until_idle();
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
        if self.host.theme().mode() == mode {
            return None;
        }
        self.host.set_theme(mode);
        self.host.run_until_idle();
        self.sync_page()
    }
}
