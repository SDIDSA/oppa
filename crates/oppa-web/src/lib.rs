//! Minimal shippable web story (v1 remainder, Gap 6): the M7 DOM
//! backend behind a wasm event-loop binding.
//!
//! [`WebApp`] owns one `ComponentHost` (toggle scene, `MockClock`
//! time), a `DomBackend` + `StyleSheet`, and the diff cursor. The
//! JS bootstrap (`web/bootstrap.js`) wires browser events into it:
//! pointer/key input → inject + run + sync, `requestAnimationFrame`
//! timestamps → [`WebApp::tick`] (TIME tails — the toggle's 120 ms
//! transition settles through frames, decision 122), and applies the
//! keyed DOM patch when the backend reports touched elements
//! (Round 12.1 — subtrees no patch op mentions are never touched).
//!
//! Bounds (stated, not hidden):
//!
//! - Interactive bindings return patch JSON (`PagePatch::to_json`);
//!   `html()` stays the full-page initial mount (and primes the
//!   patch snapshot). Unprimed snapshots and remount-class root
//!   changes fall back to a full swap inside a reload op.
//! - Text measures through the bundled DejaVu Sans rustybuzz
//!   service (Round 6.3 — the zero-width-text gap is closed;
//!   CJK/emoji outside DejaVu coverage refuse loudly per the
//!   never-tofu contract, and color emoji render stays a
//!   follow-up).
//! - No new renderer features: the pixels are the browser's (locked
//!   #2), the module only feeds structure + style + ARIA.
//! - Time comes from rAF timestamps (the module never reads a wall
//!   clock — `std::time::Instant` is unavailable on
//!   `wasm32-unknown-unknown`).

use oppa::{Color, ComponentHost, Ctx, Semantics, Style, VNode};
/// Keyed-patch addressing for binding authors (Round 12.1): pid
/// strings for [`WebApp::text`] come from the rendered markup.
pub use oppa_dom::pid_of;
use wasm_bindgen::prelude::*;

pub mod storage;

/// Reusable wasm host harness (Round 29.1, decision 344).
mod host;
pub use host::WasmHost;

const VW: f32 = 800.0;
const VH: f32 = 600.0;

/// Demo image source (Round 4.4): a 16x16 red rect as an SVG data
/// URI — same-origin fetch-free (no server MIME involved), decodable
/// by every browser. The cache key IS the src (content-addressed
/// by key string — the portable image reference).
const DOT_SRC: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='16' height='16'%3E%3Crect width='16' height='16' fill='red'/%3E%3C/svg%3E";

/// Scene props: app-owned shared state (Round 4.2 nav stack, Round
/// 4.3 settings switch — both live outside components so bindings
/// and scene share them; components render them like any state).
#[derive(Clone)]
struct SceneProps {
    nav: oppa::Signal<oppa::NavStack>,
    settings_on: oppa::Signal<bool>,
    /// Preloaded demo image id (Round 4.4 — the cache lives in
    /// `WebApp`, the id rides props like every other scene input).
    img_dot: oppa::ImageId,
}

impl oppa::Props for SceneProps {}

fn web_scene(ctx: &Ctx, props: &SceneProps) -> VNode {
    let is_on = props.settings_on.clone();
    let bg = if is_on.get() {
        Color(0x44_44_44)
    } else {
        Color(0x55_55_55)
    };
    let s = is_on.clone();
    // Round 4.1 fetch demo: the status line renders the keyed
    // fetch signal (Idle shows the hint; the bootstrap's fetch
    // button drives `fetch_start`/`fetch_resolve` around a
    // same-origin `fetch()` — E2E without touching the toggle).
    let quote = ctx.fetch_state::<String>(ctx.fetch_key("demo:quote"));
    let status = match quote.get() {
        oppa::FetchState::Idle => "tap Fetch for a quote".to_string(),
        oppa::FetchState::Loading => "loading...".to_string(),
        oppa::FetchState::Ready(text) => text,
        oppa::FetchState::Failed(e) => e,
    };
    let toggle = oppa::Div("toggle")
        .style(Style::new().size(44, 24).bg(bg))
        .semantics(Semantics::switch().checked(is_on.get()).label("Wi-Fi"))
        .on_press(move || s.set(!s.get()))
        .build();
    let quote_line = oppa::Div("quote")
        .style(Style::new().size(400, 24))
        .child(oppa::VNode::from(oppa::Text::new(status).size(14)));
    // Round 4.2 routes: the panel renders the stack top
    // (`route:{name}` — the harness asserts the text flips on
    // push/pop without parsing structure).
    let current = props
        .nav
        .get()
        .current()
        .map(|r| r.name.clone())
        .unwrap_or_default();
    let route_line = oppa::Div("route")
        .style(Style::new().size(400, 24))
        .child(oppa::VNode::from(
            oppa::Text::new(format!("route:{current}")).size(14),
        ));
    // Round 4.4 demo image: cache id rides props (preloaded in
    // `WebApp::new`), rendered as a real `<img>` by the backend.
    let dot = oppa::VNode::from(oppa::Img {
        src: props.img_dot,
        size: 16.0,
        radius: 0.0,
    });
    oppa::Div("screen")
        .style(Style::new().size(VW, VH).bg(Color(0xFF_FF_FF)))
        .children([toggle, quote_line, route_line, dot])
}

#[wasm_bindgen]
pub struct WebApp {
    shell: WasmHost,
    nav: oppa::Signal<oppa::NavStack>,
    settings: SettingsStore,
    settings_on: oppa::Signal<bool>,
}

/// Settings backend: localStorage where it opens, memory where it
/// does not (Round 4.3 — one field type, runtime fallback).
enum AnyBackend {
    Local(storage::LocalBackend),
    Mem(storage::MemBackend),
}

impl storage::StrStore for AnyBackend {
    fn str_get(&self, key: &str) -> Result<Option<String>, String> {
        match self {
            AnyBackend::Local(b) => b.str_get(key),
            AnyBackend::Mem(b) => b.str_get(key),
        }
    }

    fn str_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        match self {
            AnyBackend::Local(b) => b.str_set(key, value),
            AnyBackend::Mem(b) => b.str_set(key, value),
        }
    }

    fn str_remove(&mut self, key: &str) -> Result<(), String> {
        match self {
            AnyBackend::Local(b) => b.str_remove(key),
            AnyBackend::Mem(b) => b.str_remove(key),
        }
    }

    fn str_clear(&mut self) -> Result<(), String> {
        match self {
            AnyBackend::Local(b) => b.str_clear(),
            AnyBackend::Mem(b) => b.str_clear(),
        }
    }
}

/// The demo settings key (`settings:toggle` → wire
/// `oppa:settings:toggle`).
const TOGGLE_KEY: &str = "settings:toggle";

/// The toggle's settings store type (used in exactly one field).
type SettingsStore = storage::BrowserKv<AnyBackend>;

/// Reads the boot toggle (missing/corrupt → off — the demo
/// contract; a backend failure here also reads off rather than
/// refusing construction, documented).
fn settings_boot<S: storage::StrStore>(settings: &storage::BrowserKv<S>) -> bool {
    use oppa::store::KvStore;
    matches!(settings.get(TOGGLE_KEY), Ok(Some(v)) if v == b"1")
}

/// Reads the boot path on wasm (`/settings` → replace; `/`,
/// `/index.html`, or unparsable → home fallback — standard SPA
/// behavior, URL stays as-is). Host builds skip (no window —
/// compile-time gate, the DPR precedent).
#[cfg(target_arch = "wasm32")]
fn boot_path() -> Option<String> {
    web_sys::window()?.location().pathname().ok()
}

/// traps off-wasm; same cfg-gate discipline as DPR).
fn push_url(url: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(history) = window.history() {
                let _ = history.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(url));
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = url;
    }
}

/// Replaces the history entry on wasm (no-op on host — see
/// `push_url`).
fn replace_url(url: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(history) = window.history() {
                let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(url));
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = url;
    }
}

/// Route the current URL names (Round 4.2): wasm parses
/// `location.pathname` (params included — forward/back land
/// with full state); `/`, `/index.html`, and unparsable paths
/// fall back to `home` (standard SPA behavior). Host builds
/// read `home` (no window — compile-time gate).
fn current_path_route() -> oppa::Route {
    #[cfg(target_arch = "wasm32")]
    {
        let path = web_sys::window()
            .and_then(|w| w.location().pathname().ok())
            .unwrap_or_default();
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() || trimmed == "index.html" {
            return oppa::Route::new("home").expect("static home route");
        }
        oppa::Route::parse(trimmed)
            .unwrap_or_else(|_| oppa::Route::new("home").expect("static home route"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        oppa::Route::new("home").expect("static home route")
    }
}

#[wasm_bindgen]
impl WebApp {
    /// Mounts the scene and renders the first page.
    #[wasm_bindgen(constructor)]
    pub fn new() -> WebApp {
        // Rig boots through the shared harness (Round 29.1); the
        // demo wires its signals + image before mounting (pre-mount
        // wiring is why this uses boot/shell, not mount_root).
        let (clock, host) = WasmHost::boot();
        let nav = host.runtime().signal(oppa::NavStack::new(
            oppa::Route::new("home").expect("static home route"),
        ));
        // Boot deep-link: adopt the served path when it names a
        // route (single-page demo — sub-path serving stays out).
        #[cfg(target_arch = "wasm32")]
        if let Some(path) = boot_path() {
            let trimmed = path.trim_matches('/');
            if !trimmed.is_empty() && trimmed != "index.html" {
                if let Ok(route) = oppa::Route::parse(trimmed) {
                    nav.update(|mut stack| {
                        stack.replace(route);
                        stack
                    });
                }
            }
        }
        // Settings store (Round 4.3): localStorage where it opens,
        // memory fallback where it does not (privacy mode — the
        // settings then last the session, stated). Boot restores
        // the toggle (missing/corrupt reads default off — the demo
        // contract, documented).
        let settings = Self::open_settings();
        let settings_on = host.runtime().signal(settings_boot(&settings));
        // Demo image id (Round 4.4): preloaded before mount so the
        // scene and the backend resolve the same handle.
        let images = oppa::ImageCache::new();
        let img_dot = images.load(DOT_SRC);
        host.mount(
            "WebScene",
            SceneProps {
                nav: nav.clone(),
                settings_on: settings_on.clone(),
                img_dot,
            },
            web_scene,
        );
        host.run_until_idle();
        let shell = WasmHost::shell(host, clock, images);
        WebApp {
            shell,
            nav,
            settings,
            settings_on,
        }
    }

    /// Mounts the shared kitchen-sink showcase instead of the demo
    /// scene (Round 7.17): the same shell (clock, DejaVu measuring,
    /// DOM backend, settings store) with `KitchenSinkApp` as the
    /// root — the web leg of the sink's every-target claim
    /// (desktop `run_desktop`, Android `mount_app`). Served as
    /// `sink.html` (the demo `new()` stays the default entry
    /// point); the sink Edge harness (`spike/web/sink.mjs`) drives
    /// this constructor. Demo-only state (nav, settings toggle,
    /// demo image) still rides along inertly — custom scenes own
    /// their state through `props`, like every `new_with_root`
    /// root.
    pub fn new_sink() -> WebApp {
        WebApp::new_with_root("KitchenSink", (), oppa_controls::KitchenSinkApp)
    }

    /// Opens the settings store (Round 4.3): localStorage where it
    /// opens, memory fallback where it does not (privacy mode —
    /// settings then last the session, stated).
    fn open_settings() -> SettingsStore {
        let backend = storage::LocalBackend::open()
            .map(AnyBackend::Local)
            .unwrap_or_else(|| AnyBackend::Mem(storage::MemBackend::new()));
        storage::BrowserKv::new(backend)
    }

    /// Live device pixel ratio (G10, decision 226): the browser's
    /// `window.devicePixelRatio` through the shared
    /// `dpr_from_scale_factor` rule. Host builds (tests) read 1.0 by
    /// cfg-gate — `web_sys` traps on non-wasm targets instead of
    /// returning `None`, so the fallback is compile-time, never a
    /// runtime guess.
    pub fn device_pixel_ratio(&self) -> f32 {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = self;
            web_sys::window()
                .map(|w| oppa::dpr_from_scale_factor(w.device_pixel_ratio()))
                .unwrap_or(1.0)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = self;
            1.0
        }
    }

    /// Follows the OS light/dark flip (Round 16.2, decision 315 —
    /// the bootstrap's `matchMedia` listener calls this on
    /// `change`): sets the reactive theme from the live query when
    /// it differs (same-value writes still invalidate — the signal
    /// carries no equality gate — so a matching reading returns
    /// `None` without touching anything, never a repaint spin).
    /// Returns the DOM patch JSON, or `None` when untouched.
    pub fn sync_system_theme(&mut self) -> Option<String> {
        self.shell.sync_system_theme()
    }

    /// Current full-page HTML (the bootstrap's initial mount —
    /// primes the patch snapshot, so later patches diff against
    /// exactly this tree).
    pub fn html(&mut self) -> String {
        self.shell.html()
    }

    /// Pointer tap at CSS px: down + up through the shared pipeline.
    /// Returns the DOM patch JSON, or `None` when untouched. A
    /// touched frame persists the toggle (Round 4.3 — settings
    /// follow paint, so a stored value never disagrees with the
    /// pixels; persist failures panic loudly, harness-gated).
    pub fn click(&mut self, x: f32, y: f32) -> Option<String> {
        let patch = self.shell.click(x, y);
        if patch.is_some() {
            use oppa::store::KvStore;
            let value = if self.settings_on.get() { "1" } else { "0" };
            self.settings
                .set(TOGGLE_KEY, value.as_bytes().to_vec())
                .expect("settings persist after paint");
        }
        patch
    }

    /// Pointer move at CSS px (hover). Returns the DOM patch JSON,
    /// or `None` when untouched (moves over handler-less gaps
    /// change nothing and must never fail — decision 95).
    pub fn hover(&mut self, x: f32, y: f32) -> Option<String> {
        self.shell.hover(x, y)
    }

    /// Key event (`code` is the framework keycode). Returns the DOM
    /// patch JSON, or `None` when untouched.
    pub fn key(&mut self, code: u32, pressed: bool) -> Option<String> {
        self.shell.key(code, pressed)
    }

    /// Text value from a DOM field (U8): resolves the rendered
    /// `data-pid` to the retained node and feeds the full value.
    /// Returns the DOM patch JSON, or `None` when untouched or the
    /// pid is unknown (stale post-swap markup — never a silent
    /// node, never a panic).
    pub fn text(&mut self, pid: &str, value: &str) -> Option<String> {
        self.shell.text(pid, value)
    }

    /// Starts a platform fetch (Round 4.1, web fetch — the wasm half
    /// of decision 221): sets the named keyed signal to `Loading`
    /// and returns its generation for the promise callback. The key
    /// derives from `name` through `fetch_key` (one source of truth
    /// — JS never sees raw keys). Generations ride `f64` (exact to
    /// 2^53 — counters never approach it).
    pub fn fetch_start(&mut self, name: &str) -> f64 {
        self.shell.fetch_start(name)
    }

    /// Resolves a platform fetch from the promise callback (Round
    /// 4.1): `ok` selects `Ready(text)` vs `Failed(text)` (HTTP
    /// status / exception message — the bootstrap decides the
    /// text). Applies only when `generation` is still current
    /// (stale results discard → `None`, never half-swapped state;
    /// garbage generations saturate through `as u64` into a
    /// mismatch — discarded, never panicking). Returns the DOM patch
    /// JSON, or `None` when discarded or untouched.
    pub fn fetch_resolve(
        &mut self,
        name: &str,
        generation: f64,
        ok: bool,
        text: String,
    ) -> Option<String> {
        self.shell.fetch_resolve(name, generation, ok, text)
    }

    /// Pushes a route (Round 4.2, OQ-G6-1 — app→history half):
    /// stack push + `history.pushState` (wasm only; host builds
    /// skip the history call, same cfg-gate as DPR) + render.
    /// Invalid names panic loudly (authoring bug — the bootstrap
    /// passes developer-chosen names, never user input). Returns
    /// the DOM patch JSON, or `None` when untouched.
    pub fn nav_push(&mut self, name: &str) -> Option<String> {
        let route = oppa::Route::new(name).expect("nav_push: invalid route name");
        let url = format!("/{}", route.to_path());
        self.nav.update(|mut stack| {
            stack.push(route);
            stack
        });
        push_url(&url);
        self.shell.host().run_until_idle();
        self.shell.sync_page()
    }

    /// Replaces the top route (Round 4.2): stack replace +
    /// `history.replaceState` + render. Same loudness + patch rules
    /// as `nav_push`.
    pub fn nav_replace(&mut self, name: &str) -> Option<String> {
        let route = oppa::Route::new(name).expect("nav_replace: invalid route name");
        let url = format!("/{}", route.to_path());
        self.nav.update(|mut stack| {
            stack.replace(route);
            stack
        });
        replace_url(&url);
        self.shell.host().run_until_idle();
        self.shell.sync_page()
    }

    /// Handles a `popstate` (Round 4.2, OQ-G6-1 — history→app
    /// half): the URL is the popstate state (no `state` object
    /// needed). A URL matching the entry below the top is a true
    /// back → pop; anything else (forward, or a divergent entry)
    /// replaces the top — so the stack tracks browser depth
    /// instead of accumulating duplicates. Unparsable URLs fall
    /// back to `home` (standard SPA behavior — the URL stays,
    /// content is home). Returns the DOM patch JSON, or `None`
    /// when untouched.
    pub fn nav_pop(&mut self) -> Option<String> {
        let route = current_path_route();
        self.nav.update(|mut stack| {
            let depth = stack.depth();
            let back_match = depth >= 2 && stack.entries().get(depth - 2) == Some(&route);
            if back_match {
                stack.pop();
            } else {
                stack.replace(route);
            }
            stack
        });
        self.shell.host().run_until_idle();
        self.shell.sync_page()
    }

    /// rAF tick: advances TIME to `now_ms` and runs the frame.
    /// Returns the DOM patch JSON, or `None` when untouched
    /// (transition tails settle through these calls).
    pub fn tick(&mut self, now_ms: f64) -> Option<String> {
        self.shell.tick(now_ms)
    }
}

impl Default for WebApp {
    fn default() -> Self {
        Self::new()
    }
}

/// Pluggable scene runner (Round 6.4, decision 275): plain Rust,
/// deliberately NOT `wasm_bindgen` (exported constructors cannot
/// be generic over props). The demo `new()` stays the
/// backward-compatible JS entry point mounting `web_scene`;
/// native hosts, tests, and future JS scene registrations mount
/// any root component through this instead of forking the shell.
impl WebApp {
    pub fn new_with_root<P: oppa::Props>(
        name: &str,
        props: P,
        render: fn(&Ctx, &P) -> VNode,
    ) -> Self {
        // Same shell + demo state as new(), any root component.
        let shell = WasmHost::mount_root(name, props, render);
        let nav = shell.host().runtime().signal(
            oppa::Route::new("home")
                .map(oppa::NavStack::new)
                .expect("static home route"),
        );
        let settings = Self::open_settings();
        let settings_on = shell.host().runtime().signal(settings_boot(&settings));
        WebApp {
            shell,
            nav,
            settings,
            settings_on,
        }
    }

    /// The full host (Round 7.17 — the testkit escape-hatch rule
    /// applied to the web shell: the bindings cover the common path
    /// — taps, keys, text — while tests read geometry, semantics,
    /// and tab order through here instead of inventing coordinates).
    pub fn host(&self) -> &ComponentHost {
        &self.shell.host()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Host-side smoke (no browser): mount HTML carries the switch
    /// role unchecked; click flips to checked; second click flips
    /// back. The wasm boundary is a thin wrapper over exactly this.
    #[test]
    fn toggle_flips_through_binding_calls() {
        let mut app = WebApp::new();
        let first = app.html();
        assert!(first.contains("switch"), "role surfaced");
        assert!(
            first.contains("false") || !first.contains("true"),
            "starts unchecked"
        );
        let on = app.click(22.0, 12.0).expect("click changes DOM");
        assert_ne!(on, first);
        assert!(on.contains("true"), "checked after click");
        let off = app.click(22.0, 12.0).expect("second click changes DOM");
        assert_ne!(off, on);
        // A miss touches nothing (None, not an empty page).
        assert!(app.click(700.0, 500.0).is_none()); // Hover streams (the binding's mouse reality): gaps change
                                                    // nothing and never fail; the toggle still flips after.
        assert!(app.hover(700.0, 500.0).is_none());
        assert!(app.hover(22.0, 12.0).is_none());
        let on_again = app.click(22.0, 12.0).expect("flip after hovers");
        assert!(on_again.contains("true"));
        // rAF tick with no animation pending touches nothing.
        assert!(app.tick(1000.0).is_none());
        // Text from an unknown pid is a quiet None (stale markup —
        // the demo scene has no fields, so every pid is unknown).
        assert!(app.text("9-9", "x").is_none());
    }

    /// Headless/host DPR reads 1.0 (no window — the documented
    /// default; real browsers report through the same call).
    #[test]
    fn headless_device_pixel_ratio_is_one() {
        let app = WebApp::new();
        assert_eq!(app.device_pixel_ratio(), 1.0);
    }

    /// Round 4.1 (web fetch): start → Loading renders, resolve →
    /// Ready renders, stale generation discards, failure renders.
    /// Same calls the bootstrap makes around `fetch()` — the wasm
    /// boundary is a thin wrapper over exactly this.
    #[test]
    fn fetch_start_resolve_and_stale_discard() {
        let mut app = WebApp::new();
        let first = app.html();
        assert!(first.contains("tap Fetch for a quote"), "idle hint renders");
        let g1 = app.fetch_start("demo:quote");
        assert_eq!(g1, 1.0, "first generation");
        let loading = app.html();
        assert!(loading.contains("loading..."), "Loading renders");
        // Re-fetch before resolve: the first generation goes stale.
        let g2 = app.fetch_start("demo:quote");
        assert_eq!(g2, 2.0);
        assert!(
            app.fetch_resolve("demo:quote", g1, true, "stale".to_string())
                .is_none(),
            "stale result discards (no HTML, no state)"
        );
        assert!(app.html().contains("loading..."), "still Loading");
        let ready = app
            .fetch_resolve("demo:quote", g2, true, "hello".to_string())
            .expect("current generation applies");
        assert!(ready.contains("hello"), "Ready renders");
        // Failure shape renders too.
        let g3 = app.fetch_start("demo:quote");
        let failed = app
            .fetch_resolve("demo:quote", g3, false, "http 404".to_string())
            .expect("failure applies");
        assert!(failed.contains("http 404"), "Failed renders");
        // Garbage generation saturates into a mismatch (never panics).
        assert!(
            app.fetch_resolve("demo:quote", f64::NAN, true, "x".to_string())
                .is_none(),
            "NaN generation discards"
        );
    }

    /// Round 4.2 (web history): push renders the route, popstate
    /// (host reads `home`) replaces back, replace swaps. Same
    /// calls the bootstrap makes around `pushState`/`popstate` —
    /// the wasm boundary is a thin wrapper over exactly this
    /// (history calls themselves are cfg-gated no-ops here).
    #[test]
    fn nav_push_pop_replace_render_routes() {
        let mut app = WebApp::new();
        assert!(app.html().contains("route:home"), "boots home");
        let settings = app.nav_push("settings").expect("push renders");
        assert!(settings.contains("route:settings"), "push renders");
        assert_eq!(app.nav.get().depth(), 2);
        // Popstate on host reads `home` (no window) → replaces back.
        let home = app.nav_pop().expect("pop renders");
        assert!(home.contains("route:home"), "popstate replaces");
        assert_eq!(app.nav.get().depth(), 1);
        // Replace swaps the top (depth stable).
        let again = app.nav_replace("settings").expect("replace renders");
        assert!(again.contains("route:settings"));
        assert_eq!(app.nav.get().depth(), 1);
        // Params survive the stack (encode/decode round-trip).
        let mut app2 = WebApp::new();
        app2.nav_push("settings");
        let stack = app2.nav.get();
        assert_eq!(stack.current().map(|r| r.name.as_str()), Some("settings"));
    }

    /// Invalid route names refuse loudly (authoring bug — never a
    /// silent push of garbage).
    #[test]
    #[should_panic(expected = "invalid route name")]
    fn nav_push_invalid_name_panics() {
        let mut app = WebApp::new();
        let _ = app.nav_push("bad name");
    }

    /// Round 4.3 (web storage): a touched click persists the
    /// toggle into the app's store (browser E2E proves the
    /// cross-reload half — shared localStorage; here the write
    /// path and the boot-parse rule).
    #[test]
    fn click_persists_toggle_and_boot_parses() {
        use oppa::store::KvStore;
        let mut app = WebApp::new();
        assert_eq!(
            app.settings.get("settings:toggle").expect("reads"),
            None,
            "fresh store starts empty"
        );
        app.click(22.0, 12.0).expect("flip on persists");
        assert_eq!(
            app.settings.get("settings:toggle").expect("reads"),
            Some(b"1".to_vec()),
            "click persists on"
        );
        app.click(22.0, 12.0).expect("flip off persists");
        assert_eq!(
            app.settings.get("settings:toggle").expect("reads"),
            Some(b"0".to_vec()),
            "click persists off"
        );
        // Boot parse rule: "1" restores on, anything else off.
        let mut mem = crate::storage::BrowserKv::new(crate::storage::MemBackend::new());
        assert!(!settings_boot(&mem), "missing reads off");
        mem.set("settings:toggle", b"1".to_vec()).expect("seeds");
        assert!(settings_boot(&mem), "\"1\" restores on");
        mem.set("settings:toggle", b"yes".to_vec()).expect("seeds");
        assert!(!settings_boot(&mem), "corrupt reads off, never panics");
    }

    /// Round 4.4 (web images): the scene's preloaded id renders a
    /// real `<img>` with the cache key as source (data URI —
    /// same-origin fetch-free, no server MIME involved).
    #[test]
    fn scene_image_renders_img_with_src() {
        let mut app = WebApp::new();
        let html = app.html();
        assert!(html.contains("<img "), "real img element, {html}");
        assert!(
            html.contains("src=\"data:image/svg+xml,"),
            "cache key is the source, {html}"
        );
        assert!(html.contains("width:16px;"), "explicit geometry, {html}");
        assert!(html.contains("alt=\"\""), "empty alt, {html}");
    }

    /// Round 6.3 (web text measurement): a `Text` node inside
    /// `WebApp` lays out with realistic, non-zero advance width
    /// through the bundled DejaVu Sans service (previously zero —
    /// the serviceless rule). Finite everywhere (NaN would be the
    /// silent-poison failure the framework refuses).
    #[test]
    fn webapp_text_measures_nonzero_width() {
        let mut app = WebApp::new();
        let _ = app.html();
        let ids = oppa::find_retained_by_debug(&app.host(), "text");
        assert!(!ids.is_empty(), "scene renders text leaves");
        let mut nonzero = 0;
        for id in &ids {
            let b = app
                .host()
                .committed_box(*id)
                .unwrap_or_else(|| panic!("text leaf has a committed box — pump first"));
            assert!(
                b.w.is_finite() && b.h.is_finite() && b.content_w.is_finite(),
                "no NaN dimensions, got {b:?}"
            );
            if b.content_w > 0.0 {
                nonzero += 1;
            }
        }
        assert!(
            nonzero > 0,
            "text measures non-zero width through the bundled service"
        );
    }

    /// Round 6.4 (pluggable runner): mounting a custom root
    /// component renders its content into the DOM, and pressing it
    /// updates the DOM through the same bindings. The default
    /// constructor still mounts the demo scene (backward compat).
    #[test]
    fn custom_root_mounts_and_updates_dom() {
        use oppa::SharedString;

        #[derive(Clone)]
        struct ProbeProps {
            label: SharedString,
        }

        impl oppa::Props for ProbeProps {}

        // Press counter lives in an instance signal (same runtime
        // by construction) and renders into the label — the DOM
        // text itself proves the press landed.
        fn probe_scene(ctx: &Ctx, props: &ProbeProps) -> VNode {
            let count = ctx.signal(0u32);
            let label = format!("{} pressed {}", props.label, count.get());
            oppa::Div("probe")
                .style(Style::new().size(200, 48).bg(Color(0x22_66_CC)))
                .semantics(Semantics::button().label(&props.label))
                .on_press(move || count.set(count.get() + 1))
                .child(VNode::from(oppa::Text::new(label)))
        }

        // Default constructor: still the demo scene.
        let mut demo = WebApp::new();
        assert!(demo.html().contains("route:home"), "demo scene intact");

        // Custom root: its label renders, its press re-renders.
        let mut app = WebApp::new_with_root(
            "Probe",
            ProbeProps {
                label: SharedString::from("Probe me"),
            },
            probe_scene,
        );
        let first = app.html();
        assert!(first.contains("Probe me pressed 0"), "custom scene renders");
        assert!(!first.contains("route:home"), "demo scene not mounted");
        let id = oppa::find_retained_by_debug(&app.host(), "probe")
            .into_iter()
            .next()
            .expect("probe node");
        let b = app.host().committed_box(id).expect("probe box");
        let updated = app
            .click(b.x + b.w / 2.0, b.y + b.h / 2.0)
            .expect("press touches the DOM");
        assert!(
            updated.contains("Probe me pressed 1"),
            "press updates the custom scene"
        );
    }

    /// Round 29.1 (decision 344): the harness mounts a custom root
    /// with no demo state -- the product promise (thin bindgen glue
    /// over mount_root + click, not a rig fork).
    #[test]
    fn wasm_host_mounts_custom_root_without_demo() {
        use oppa::SharedString;
        #[derive(Clone)]
        struct BareProps;
        impl oppa::Props for BareProps {}
        fn bare(ctx: &Ctx, _: &BareProps) -> VNode {
            let n = ctx.signal(0u32);
            let c = n.clone();
            oppa::Div("bare")
                .style(Style::new().size(200, 48))
                .on_press(move || c.set(c.get() + 1))
                .child(VNode::from(oppa::Text::new(format!("n={}", n.get()))))
        }
        let mut app = WasmHost::mount_root("Bare", BareProps, bare);
        let first = app.html();
        assert!(first.contains("n=0"), "custom root renders");
        assert!(!first.contains("route:home"), "no demo scene");
        let id = oppa::find_retained_by_debug(app.host(), "bare")
            .into_iter()
            .next()
            .expect("bare node");
        let b = app.host().committed_box(id).expect("bare box");
        let updated = app
            .click(b.x + b.w / 2.0, b.y + b.h / 2.0)
            .expect("press touches the DOM");
        assert!(updated.contains("n=1"), "press re-renders");
    }

    /// Round 16.2 (decision 315): host builds (no browser) read a
    /// light fallback at boot and a sync stays a quiet no-op —
    /// never a `web_sys` trap, never a repaint spin.
    #[test]
    fn system_theme_falls_back_to_light_off_browser() {
        assert!(
            !host::system_prefers_dark(),
            "no browser means no dark query"
        );
        let mut app = WebApp::new();
        let _ = app.html();
        assert_eq!(app.host().theme().mode(), oppa::ThemeMode::Light);
        assert_eq!(
            app.sync_system_theme(),
            None,
            "matching reading touches nothing"
        );
    }
}
