//! Headless app-test seam (G15 — decision 234).
//!
//! `docs/ARCHITECTURE.md` (test harness) covers the framework side;
//! assert story — every round's tests reinvented the same rig
//! (`ComponentHost` + inject + `run_until_idle` + retained reads).
//! This crate productizes exactly that rig, composing public API
//! only (no new framework surface, no test-only backdoors — what
//! the harness can do, the app can do).
//!
//! ```rust
//! use oppa_testkit::Harness;
//!
//! let app = Harness::new();
//! // app.mount("MyScreen", props, render);
//! // app.tap("ok-button");
//! // assert_eq!(app.host().capture_count(), 0);
//! ```
//!
//! Design (decision 234; rationale in git history):
//!
//! - **Loud lookups.** `node(debug)` / `center(debug)` panic naming
//!   the missing label (a renamed debug label is a wiring bug, and
//!   `Option`-plumbing it through every test would normalize
//!   silence). Unboxed nodes (no committed box yet) panic the same
//!   way — pump first.
//! - **Clock-gated time.** `advance(secs)` needs `with_clock` (a
//!   `MockClock` the test owns); without one it panics loudly
//!   instead of advancing nothing. Deterministic holds, taps, and
//!   transitions without sleeps.
//! - **Escape hatch, not a cage.** `host()` exposes the full
//!   `ComponentHost` (inject exotic events, read router state,
//!   drive reloads) — the harness covers the common path, never
//!   blocks the rare one.

use std::rc::Rc;

use oppa::{
    ComponentHost, Ctx, InputEvent, KeyState, MockClock, Modifiers, MountHandle, NodeId, Props,
    VNode,
};

/// Headless app rig: mount, pump, tap, assert.
pub struct Harness {
    host: ComponentHost,
    clock: Option<Rc<MockClock>>,
}

impl Harness {
    /// 800×600 viewport on the system clock (real-time holds need a
    /// live loop — use [`Harness::with_clock`] for time).
    pub fn new() -> Self {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        Self { host, clock: None }
    }

    /// Viewport + owned mock clock (returned alongside for reads;
    /// `advance` uses the same instance).
    pub fn with_clock() -> (Self, Rc<MockClock>) {
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        host.set_viewport(800.0, 600.0);
        (
            Self {
                host,
                clock: Some(clock.clone()),
            },
            clock,
        )
    }

    pub fn set_viewport(&self, w: f32, h: f32) {
        self.host.set_viewport(w, h);
    }

    pub fn mount<P: Props>(
        &self,
        name: &str,
        props: P,
        render: fn(&Ctx, &P) -> VNode,
    ) -> MountHandle<P> {
        let handle = self.host.mount(name, props, render);
        self.host.run_until_idle();
        handle
    }

    /// Pump until idle (returns frames run).
    pub fn run_idle(&self) -> usize {
        self.host.run_until_idle()
    }

    pub fn run_once(&self) -> bool {
        self.host.run_once()
    }

    /// Retained node by debug label (loud when missing).
    pub fn node(&self, debug: &str) -> NodeId {
        oppa::find_retained_by_debug(&self.host, debug)
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                panic!("testkit: no retained node {debug:?} — renamed label or unmounted tree")
            })
    }

    /// Committed-box center in device px (loud when unboxed — pump
    /// first so layout has run).
    pub fn center(&self, debug: &str) -> (f32, f32) {
        let id = self.node(debug);
        let b = self
            .host
            .committed_box(id)
            .unwrap_or_else(|| panic!("testkit: node {debug:?} has no committed box — pump first"));
        (b.x + b.w / 2.0, b.y + b.h / 2.0)
    }

    /// Tap = down + up + idle at the node's center.
    pub fn tap(&self, debug: &str) {
        let (x, y) = self.center(debug);
        self.host.inject_input(InputEvent::pointer_down(x, y));
        self.host.inject_input(InputEvent::pointer_up(x, y));
        self.host.run_until_idle();
    }

    /// Press-and-hold without release (long-press rigs pair this
    /// with [`Harness::advance`]).
    pub fn press_down(&self, debug: &str) {
        let (x, y) = self.center(debug);
        self.host.inject_input(InputEvent::pointer_down(x, y));
        self.host.run_until_idle();
    }

    /// Presses a key (no modifiers) + pumps (Round 35, decision
    /// 351 — promotes the `tab()` shape every keyboard test
    /// hand-rolls: `focus_ring_trap.rs` rides this now).
    pub fn key(&self, code: u32) {
        self.key_with(code, Modifiers::NONE);
    }

    /// Presses a key with explicit modifiers + pumps (Shift+Tab,
    /// Ctrl+A shapes — same inject + settle contract as
    /// [`Harness::key`]).
    pub fn key_with(&self, code: u32, modifiers: Modifiers) {
        self.host.inject_input(InputEvent::Key {
            code,
            modifiers,
            state: KeyState::Pressed,
            repeat: false,
        });
        self.host.run_until_idle();
    }

    /// Types printable text into the focused field's session, then
    /// pumps (Round 35, decision 351 — the headless half of
    /// `DesktopLoop::type_text` in oppa-app, minus repaint: same
    /// `>= 0x20` printable filter, same quiet miss). Returns the inserted printable-char count (`0` when
    /// nothing is focused or the input is all control chars —
    /// assert on your value signal either way).
    pub fn type_text(&self, text: &str) -> usize {
        let printable: String = text.chars().filter(|c| (*c as u32) >= 0x20).collect();
        if printable.is_empty() {
            return 0;
        }
        let Some(session) = self.host.focused_field_session() else {
            return 0;
        };
        session.insert(&printable);
        self.host.run_until_idle();
        printable.chars().count()
    }

    /// Presses the tab-order node carrying `label` + pumps (Round
    /// 35, decision 351 — promotes the `press_labeled_button`
    /// helper: semantics never go stale across in-place diffs, so
    /// this needs no debug census). Loud when the label is absent
    /// from the tab order (a renamed label is a wiring bug).
    pub fn press_labeled(&self, label: &str) {
        let id = self
            .host
            .tab_order()
            .into_iter()
            .find(|id| {
                self.host
                    .retained_semantics(*id)
                    .map(|s| s.label.as_deref() == Some(label))
                    .unwrap_or(false)
            })
            .unwrap_or_else(|| panic!("testkit: button {label:?} is in the tab order"));
        let b = self.host.committed_box(id).unwrap_or_else(|| {
            panic!("testkit: button {label:?} has no committed box — pump first")
        });
        let (x, y) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        self.host.inject_input(InputEvent::pointer_down(x, y));
        self.host.inject_input(InputEvent::pointer_up(x, y));
        self.host.run_until_idle();
    }

    /// Advances mock time, then pumps (deterministic holds and
    /// transitions — no sleeps). Loud without
    /// [`Harness::with_clock`].
    pub fn advance(&self, secs: f64) {
        let Some(clock) = &self.clock else {
            panic!("testkit: advance needs with_clock (system time cannot jump)");
        };
        clock.set(clock.get() + secs);
        self.host.run_until_idle();
    }

    /// The full host (exotic events, router reads, reloads).
    pub fn host(&self) -> &ComponentHost {
        &self.host
    }
}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oppa::RendererBackend;

    #[derive(Clone)]
    struct Toggle {
        on: oppa::Signal<bool>,
    }

    impl Props for Toggle {}

    fn render_toggle(_ctx: &Ctx, p: &Toggle) -> VNode {
        let on = p.on.clone();
        oppa::Div("track")
            .style(oppa::Style::new().size(44, 24))
            .semantics(oppa::Semantics::switch().checked(on.get()))
            .on_press(move || on.set(!on.get()))
            .build()
    }

    #[test]
    fn tap_flips_through_the_whole_pipe() {
        let app = Harness::new();
        let on = app.host().runtime().signal(false);
        app.mount("T", Toggle { on: on.clone() }, render_toggle);
        assert!(!on.get());
        app.tap("track");
        assert!(on.get(), "tap dispatches end to end");
        assert_eq!(app.host().capture_count(), 0);
    }

    #[test]
    fn hold_fires_past_deadline_deterministically() {
        let (app, clock) = Harness::with_clock();
        assert_eq!(clock.get(), 0.0);
        let on = app.host().runtime().signal(false);
        app.mount("T", Toggle { on: on.clone() }, render_toggle);
        app.press_down("track");
        assert!(!on.get());
        app.advance(0.1);
        assert!(!on.get(), "early pump never fires");
        app.advance(oppa::input::LONG_PRESS_TIMEOUT_S);
        assert!(on.get(), "hold fires past the deadline");
    }

    #[test]
    #[should_panic(expected = "no retained node")]
    fn missing_label_is_loud() {
        let app = Harness::new();
        let on = app.host().runtime().signal(false);
        app.mount("T", Toggle { on }, render_toggle);
        let _ = app.node("typo-label");
    }

    #[test]
    #[should_panic(expected = "with_clock")]
    fn advance_without_clock_is_loud() {
        let app = Harness::new();
        app.advance(1.0);
    }

    /// Round 7.1 (decision 276): the unified kitchen sink mounts,
    /// lays out, diffs, and renders without panics or NaN
    /// dimensions. Real text measurement (the web path's bundled
    /// service) proves non-zero advances, not just zero-boxes.
    ///
    /// Reconciler-honest assertions (verified in
    /// `reconciler.rs::diff_children`/`diff_node`): same-tag tab
    /// panels diff IN PLACE, so retained debugs go stale across
    /// switches (only style/semantics/handlers/text refresh —
    /// never `debug`). The test therefore proves switches through
    /// FRESH mounts (tag changes: Row-for-Div, Portal-for-Div),
    /// label-addressed presses (semantics never go stale), finite
    /// boxes, and per-tab CPU paints — never a stale census.
    #[test]
    fn kitchen_sink_mounts_lays_out_diffs_and_renders() {
        use oppa_controls::KitchenSinkApp;

        let app = Harness::new();
        let (svc, _) = oppa_text_rustybuzz::RustybuzzService::from_bytes_with_chain(
            &[("DejaVuSans.ttf", oppa_fonts::DEJAVU_SANS)],
            &[],
        )
        .expect("bundled font parses");
        app.host().set_text_service(Box::new(svc));
        app.host().set_layout_config(oppa::LayoutTextConfig {
            family: oppa_fonts::DEJAVU_SANS_FAMILY.to_string(),
            ..Default::default()
        });
        app.mount("KitchenSink", (), KitchenSinkApp);
        assert!(app.host().diff_count() > 0, "mount produces diffs");
        // Form is the initial tab (fresh mount — its debugs are valid).
        assert!(
            !oppa::find_retained_by_debug(app.host(), "text-input").is_empty(),
            "form mounts its text field"
        );
        assert_finite_boxes(app.host());

        // CPU painter with a diff cursor (one paint per tab proves
        // render-readiness past layout).
        let mut cpu = oppa_cpu::CpuBackend::new();
        let surf = cpu
            .create_surface(oppa::SurfaceDesc {
                width_px: 800,
                height_px: 600,
                background: oppa::Color(0xFF_FF_FF),
            })
            .expect("cpu surface builds");
        let mut cursor = 0;
        let px = paint_now(&app, &mut cpu, surf, &mut cursor);
        assert!(!px.is_empty(), "form paints pixels");

        let tabs = oppa::find_retained_by_debug(app.host(), "tab-item");
        assert_eq!(tabs.len(), 4, "four tab buttons mount");

        // Layout: the chips Row is a fresh mount (Row replaces the
        // form's Div at position 0 — incompatible tags replace).
        press_node(app.host(), tabs[1]);
        assert_eq!(
            oppa::find_retained_by_debug(app.host(), "sink::Layout::Chips").len(),
            1,
            "layout switches in its chips row"
        );
        assert_eq!(
            oppa::find_retained_by_debug(app.host(), "sink::Layout::Chip0").len(),
            1,
            "chips render"
        );
        assert_finite_boxes(app.host());
        let px = paint_now(&app, &mut cpu, surf, &mut cursor);
        assert!(!px.is_empty(), "layout paints pixels");

        // Overlays: the Open button is fresh (Div replaces the Row).
        press_node(app.host(), tabs[2]);
        assert_eq!(
            oppa::find_retained_by_debug(app.host(), "sink::Overlays::Open").len(),
            1,
            "overlays switch in their trigger"
        );
        assert_finite_boxes(app.host());
        // The modal opens through its label (Portal replaces the
        // Div — fresh) and closes through Cancel.
        press_labeled_button(app.host(), "Open dialog");
        assert_eq!(
            oppa::find_retained_by_debug(app.host(), "modal-card").len(),
            1,
            "modal opens its card"
        );
        assert_finite_boxes(app.host());
        let px = paint_now(&app, &mut cpu, surf, &mut cursor);
        assert!(!px.is_empty(), "open modal paints pixels");
        press_labeled_button(app.host(), "Cancel");
        assert!(
            oppa::find_retained_by_debug(app.host(), "modal-card").is_empty(),
            "cancel closes the modal"
        );

        // Platform: every position reuses same-tag nodes (stale
        // debugs by design), so the proof is behavioral — the Pick
        // press (label-addressed) changes the rendered pixels.
        press_node(app.host(), tabs[3]);
        assert_finite_boxes(app.host());
        let before = paint_now(&app, &mut cpu, surf, &mut cursor);
        press_labeled_button(app.host(), "Pick a file");
        assert_finite_boxes(app.host());
        let after = paint_now(&app, &mut cpu, surf, &mut cursor);
        assert_ne!(before, after, "picking a file repaints new pixels");
    }

    /// Theme contract round (decision 323): the showcase's Dark
    /// toggle owns the host palette, and the Layout furniture +
    /// default ink follow it — the field's hybrid (dark inputs,
    /// black text, white page) came from the toggle being a dead
    /// switch and neither default following the theme.
    #[test]
    fn sink_dark_toggle_rethemes_furniture_and_ink() {
        use oppa_controls::KitchenSinkApp;
        let app = Harness::new();
        dejavu(&app);
        app.mount("KitchenSink", (), KitchenSinkApp);
        // The Form tab's Dark toggle flips the host palette through
        // its accessible label (no debug census needed).
        press_labeled_button(app.host(), "Dark mode");
        assert_eq!(
            app.host().theme().mode(),
            oppa::ThemeMode::Dark,
            "the toggle owns the palette"
        );
        // Layout tab on: chips + cards mount under Dark.
        let tabs = oppa::find_retained_by_debug(app.host(), "tab-item");
        assert_eq!(tabs.len(), 4, "four tab buttons mount");
        press_node(app.host(), tabs[1]);
        assert_finite_boxes(app.host());
        // Runner-equivalent publish (DesktopLoop::repaint does this
        // per frame) + full plan.
        let builder = oppa_cpu::FramePlanBuilder::new(1.0);
        builder.set_theme_mode(oppa::ThemeMode::Dark);
        let plan = app
            .host()
            .with_retained_mut(|rec, styles| builder.build_full(rec, styles));
        let dark = oppa::ThemeTokens::dark();
        // Chips re-derive the deep wash (72×28 rounded).
        assert!(
            plan.ops.iter().any(|op| matches!(op,
                oppa::DrawOp::RRect { w, h, color, .. }
                if *w == 72.0 && *h == 28.0 && *color == dark.disabled)),
            "chips follow the disabled wash"
        );
        // Cards re-derive the raised surface (280×48 fills).
        assert!(
            plan.ops.iter().any(|op| matches!(op,
                oppa::DrawOp::Rect { w, h, color, .. }
                if *w == 280.0 && *h == 48.0 && *color == dark.surface)),
            "cards follow the surface"
        );
        // Default ink follows the theme; no near-black survives.
        let inks: Vec<oppa::Color> = plan
            .ops
            .iter()
            .filter_map(|op| match op {
                oppa::DrawOp::Text { ink, .. } => Some(*ink),
                _ => None,
            })
            .collect();
        assert!(!inks.is_empty(), "the scene carries text");
        assert!(
            inks.contains(&dark.text_primary),
            "defaults re-ink: {inks:?}"
        );
        assert!(
            inks.iter().all(|i| *i != oppa::render::INK),
            "no Light ink survives Dark: {inks:?}"
        );
    }

    /// Presses one retained node by id (center tap + settle).
    fn press_node(host: &oppa::ComponentHost, id: oppa::NodeId) {
        let b = host
            .committed_box(id)
            .unwrap_or_else(|| panic!("node {id:?} has a committed box — pump first"));
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        host.inject_input(oppa::InputEvent::pointer_down(cx, cy));
        host.inject_input(oppa::InputEvent::pointer_up(cx, cy));
        host.run_until_idle();
    }

    /// Presses the tab-order button carrying `label` (semantics
    /// never go stale across in-place diffs, so this needs no
    /// debug census).
    fn press_labeled_button(host: &oppa::ComponentHost, label: &str) {
        let id = host
            .tab_order()
            .into_iter()
            .find(|id| {
                host.retained_semantics(*id)
                    .map(|s| s.label.as_deref() == Some(label))
                    .unwrap_or(false)
            })
            .unwrap_or_else(|| panic!("button {label:?} is in the tab order"));
        press_node(host, id);
    }

    /// Bundled-DejaVu text service + layout config (real shaping,
    /// shared by the paint proofs).
    fn dejavu(app: &Harness) {
        let (svc, _) = oppa_text_rustybuzz::RustybuzzService::from_bytes_with_chain(
            &[("DejaVuSans.ttf", oppa_fonts::DEJAVU_SANS)],
            &[],
        )
        .expect("bundled font parses");
        app.host().set_text_service(Box::new(svc));
        app.host().set_layout_config(oppa::LayoutTextConfig {
            family: oppa_fonts::DEJAVU_SANS_FAMILY.to_string(),
            ..Default::default()
        });
    }

    /// White 800×600 CPU surface (shared by the paint proofs).
    fn painter() -> (oppa_cpu::CpuBackend, oppa::SurfaceId) {
        let mut cpu = oppa_cpu::CpuBackend::new();
        let surf = cpu
            .create_surface(oppa::SurfaceDesc {
                width_px: 800,
                height_px: 600,
                background: oppa::Color(0xFF_FF_FF),
            })
            .expect("cpu surface builds");
        (cpu, surf)
    }

    /// Paints the current tree through the CPU backend (committing
    /// only new diffs) and snapshots raw pixels for comparison.
    fn paint_now(
        app: &Harness,
        cpu: &mut oppa_cpu::CpuBackend,
        surf: oppa::SurfaceId,
        cursor: &mut usize,
    ) -> Vec<(u8, u8, u8, u8)> {
        use oppa::RendererBackend;
        for d in app.host().diffs_from(*cursor) {
            cpu.commit(&d).expect("cpu commits the tree");
        }
        *cursor = app.host().diff_count();
        let builder = oppa_cpu::FramePlanBuilder::new(1.0);
        let plan = app
            .host()
            .with_retained_mut(|rec, styles| builder.build_full(rec, styles));
        cpu.paint(surf, &plan).expect("cpu paints the plan");
        cpu.pixmap(surf)
            .expect("pixmap reads back")
            .pixels()
            .iter()
            .map(|p| (p.red(), p.green(), p.blue(), p.alpha()))
            .collect()
    }

    /// Every laid-out text leaf and tab container is finite with
    /// non-negative extents (NaN/negative dimensions are the loud
    /// layout refusal class — absent here by assertion).
    fn assert_finite_boxes(host: &oppa::ComponentHost) {
        for id in oppa::find_retained_by_debug(host, "text") {
            if let Some(b) = host.committed_box(id) {
                assert!(
                    b.x.is_finite() && b.y.is_finite() && b.w.is_finite() && b.h.is_finite(),
                    "finite text box, got {b:?}"
                );
                assert!(b.w >= 0.0 && b.h >= 0.0, "no negative extents, got {b:?}");
            }
        }
        for debug in ["tabs-container", "tab-panel", "tab-bar"] {
            for id in oppa::find_retained_by_debug(host, debug) {
                if let Some(b) = host.committed_box(id) {
                    assert!(
                        b.x.is_finite() && b.y.is_finite() && b.w.is_finite() && b.h.is_finite(),
                        "finite {debug}, got {b:?}"
                    );
                }
            }
        }
    }

    /// Round 7.10: drawn-control paint proofs (real DejaVu shaping +
    /// CPU backend — structure tests cannot see paint-only radius and
    /// edge bands). Button corners stay surface-white (radius 6, no
    /// sharp blue corner) with Primary fill inside; the active tab
    /// carries a Primary underline band (bottom 3px) over white, and
    /// the rows above it stay clean (the padding zone has no glyphs,
    /// so any Primary there would be a smeared band).
    #[test]
    fn polished_controls_paint_proofs() {
        use oppa_controls::KitchenSinkApp;
        use oppa_controls::{Button, ButtonProps};

        const WHITE: (u8, u8, u8, u8) = (255, 255, 255, 255);
        const PRIMARY: (u8, u8, u8, u8) = (0x22, 0x66, 0xCC, 255);

        // Button: lone mount, corner stays white, fill paints Primary.
        #[derive(Clone)]
        struct SoloButton {
            props: ButtonProps,
        }
        impl Props for SoloButton {}
        fn render_solo(_ctx: &Ctx, p: &SoloButton) -> VNode {
            Button(_ctx, &p.props)
        }
        let app = Harness::new();
        dejavu(&app);
        let fired = app.host().runtime().signal(false);
        let set = fired.clone();
        app.mount(
            "SoloBtn",
            SoloButton {
                props: ButtonProps::new("OK", move || set.set(true)),
            },
            render_solo,
        );
        let (mut cpu, surf) = painter();
        let mut cursor = 0;
        paint_now(&app, &mut cpu, surf, &mut cursor);
        let id = oppa::find_retained_by_debug(app.host(), "button")
            .into_iter()
            .next()
            .expect("button retained");
        let b = app.host().committed_box(id).expect("button laid out");
        assert_eq!(
            cpu.pixel_rgba(surf, b.x as u32, b.y as u32),
            Some(WHITE),
            "rounded corner leaves surface white, box {b:?}"
        );
        let blue = cpu
            .pixmap(surf)
            .expect("pixmap reads back")
            .pixels()
            .iter()
            .filter(|p| (p.red(), p.green(), p.blue(), p.alpha()) == PRIMARY)
            .count();
        assert!(
            blue > 100,
            "Primary fill paints (got {blue} exact pixels in {b:?})"
        );

        // Tabs: the active tab underlines its bottom 3px in Primary.
        let app = Harness::new();
        dejavu(&app);
        app.mount("KitchenSink", (), KitchenSinkApp);
        let tabs = oppa::find_retained_by_debug(app.host(), "tab-item");
        assert_eq!(tabs.len(), 4, "four tab buttons mount");
        press_node(app.host(), tabs[0]);
        let (mut cpu, surf) = painter();
        let mut cursor = 0;
        paint_now(&app, &mut cpu, surf, &mut cursor);
        let b = app.host().committed_box(tabs[0]).expect("tab laid out");
        let bottom = (b.y + b.h) as u32;
        let left = (b.x + 2.0) as u32;
        let mut band = 0;
        for y in bottom.saturating_sub(4)..bottom {
            for x in left..left + 10 {
                if cpu.pixel_rgba(surf, x, y) == Some(PRIMARY) {
                    band += 1;
                }
            }
        }
        assert!(band > 0, "underline band paints Primary, box {b:?}");
        let mut above = 0;
        for y in bottom.saturating_sub(10)..bottom.saturating_sub(4) {
            for x in left..left + 10 {
                if cpu.pixel_rgba(surf, x, y) == Some(PRIMARY) {
                    above += 1;
                }
            }
        }
        assert_eq!(above, 0, "no Primary above the band (box {b:?})");
    }

    /// Round 7.11: the rectangular-controls radius rule (checkbox /
    /// input / select / tab-bar share corner 4). On a gray surface,
    /// white-box corners read surface-gray (rounded) instead of box
    /// fill — the same corner probe as the Button proof, over the
    /// sink's real TextInput and Select box.
    #[test]
    fn rectangular_controls_share_radius_4() {
        use oppa_controls::KitchenSinkApp;

        const GRAY: (u8, u8, u8, u8) = (0xEE, 0xEE, 0xEE, 255);

        let app = Harness::new();
        let (svc, _) = oppa_text_rustybuzz::RustybuzzService::from_bytes_with_chain(
            &[("DejaVuSans.ttf", oppa_fonts::DEJAVU_SANS)],
            &[],
        )
        .expect("bundled font parses");
        app.host().set_text_service(Box::new(svc));
        app.host().set_layout_config(oppa::LayoutTextConfig {
            family: oppa_fonts::DEJAVU_SANS_FAMILY.to_string(),
            ..Default::default()
        });
        app.mount("KitchenSink", (), KitchenSinkApp);
        let mut cpu = oppa_cpu::CpuBackend::new();
        let surf = cpu
            .create_surface(oppa::SurfaceDesc {
                width_px: 800,
                height_px: 600,
                background: oppa::Color(0xEE_EE_EE),
            })
            .expect("cpu surface builds");
        let mut cursor = 0;
        paint_now(&app, &mut cpu, surf, &mut cursor);
        for debug in ["text-input", "select-box"] {
            let id = oppa::find_retained_by_debug(app.host(), debug)
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("{debug} retained"));
            let b = app.host().committed_box(id).expect("box laid out");
            assert_eq!(
                cpu.pixel_rgba(surf, b.x as u32, b.y as u32),
                Some(GRAY),
                "{debug} corner reads surface gray (radius), box {b:?}"
            );
        }
    }

    /// Round 7.15: pressed feedback is painted, not just
    /// structural — holding the button deepens Primary to
    /// `0x1B_52_A4`, release restores it. Drives the real router
    /// flags (down without up), so this proves the instance-flag →
    /// re-render → plan → pixel chain, not the color constant.
    #[test]
    fn pressed_button_paints_pressed_tint() {
        use oppa_controls::{Button, ButtonProps};

        const PRIMARY: (u8, u8, u8, u8) = (0x22, 0x66, 0xCC, 255);
        const PRESSED: (u8, u8, u8, u8) = (0x1B, 0x52, 0xA4, 255);

        #[derive(Clone)]
        struct SoloButton {
            props: ButtonProps,
        }
        impl Props for SoloButton {}
        fn render_solo(_ctx: &Ctx, p: &SoloButton) -> VNode {
            Button(_ctx, &p.props)
        }
        let app = Harness::new();
        dejavu(&app);
        let fired = app.host().runtime().signal(false);
        let set = fired.clone();
        app.mount(
            "SoloBtn",
            SoloButton {
                props: ButtonProps::new("OK", move || set.set(true)),
            },
            render_solo,
        );
        let (mut cpu, surf) = painter();
        let mut cursor = 0;
        paint_now(&app, &mut cpu, surf, &mut cursor);
        let id = oppa::find_retained_by_debug(app.host(), "button")
            .into_iter()
            .next()
            .expect("button retained");
        let b = app.host().committed_box(id).expect("button laid out");
        let fill_at = |cpu: &oppa_cpu::CpuBackend, dx: f32, dy: f32| {
            cpu.pixel_rgba(surf, (b.x + dx) as u32, (b.y + dy) as u32)
        };
        assert_eq!(fill_at(&cpu, 80.0, 16.0), Some(PRIMARY), "at rest");
        // Hold (down, no up): the fill deepens; the click has NOT
        // fired yet (activation rides up).
        app.press_down("button");
        assert!(!fired.get(), "hold does not activate");
        paint_now(&app, &mut cpu, surf, &mut cursor);
        assert_eq!(
            fill_at(&cpu, 80.0, 16.0),
            Some(PRESSED),
            "held fill deepens"
        );
        // Release on the button: activates and restores Primary.
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        app.host()
            .inject_input(oppa::InputEvent::pointer_up(cx, cy));
        app.run_idle();
        assert!(fired.get(), "release activates");
        paint_now(&app, &mut cpu, surf, &mut cursor);
        assert_eq!(fill_at(&cpu, 80.0, 16.0), Some(PRIMARY), "released");
    }

    /// Decision 291: the vector check paints where the geometry says —
    /// fully-covered stroke pixels read white inside the blue box
    /// (2.5px round stroke: on-segment (6,12), join disc (8,14),
    /// on-segment (11,11) — every pixel-square corner within the
    /// half-width, so analytic AA covers them exactly). No font runs
    /// anywhere near this (deterministic across OS fonts by
    /// construction — the whole point of leaving text behind, now as
    /// curves instead of stepped squares).
    #[test]
    fn checkbox_check_paints_vector_stroke() {
        use oppa_controls::{Checkbox, CheckboxProps};

        const WHITE: (u8, u8, u8, u8) = (255, 255, 255, 255);
        const PRIMARY: (u8, u8, u8, u8) = (0x22, 0x66, 0xCC, 255);

        #[derive(Clone)]
        struct SoloCheck {
            props: CheckboxProps,
        }
        impl Props for SoloCheck {}
        fn render_check(_ctx: &Ctx, p: &SoloCheck) -> VNode {
            Checkbox(_ctx, &p.props)
        }
        let app = Harness::new();
        dejavu(&app);
        let checked = app.host().runtime().signal(true);
        app.mount(
            "SoloChk",
            SoloCheck {
                props: CheckboxProps {
                    label: oppa::SharedString::from("Accept"),
                    checked: checked.clone(),
                    enabled: true,
                    on_change: None,
                },
            },
            render_check,
        );
        let (mut cpu, surf) = painter();
        let mut cursor = 0;
        paint_now(&app, &mut cpu, surf, &mut cursor);
        let id = oppa::find_retained_by_debug(app.host(), "checkbox-box")
            .into_iter()
            .next()
            .expect("box retained");
        let b = app.host().committed_box(id).expect("box laid out");
        // Stroke probes (box-relative (6,12),(8,14),(11,11)): solid
        // white, past any AA fringe.
        for (dx, dy) in [(6.0, 12.0), (8.0, 14.0), (11.0, 11.0)] {
            assert_eq!(
                cpu.pixel_rgba(surf, (b.x + dx) as u32, (b.y + dy) as u32),
                Some(WHITE),
                "stroke at ({dx}, {dy}) paints white"
            );
        }
        // Box corners and mid-band away from the stroke stay Primary.
        for (dx, dy) in [(2.0, 2.0), (17.0, 17.0), (10.0, 4.0)] {
            assert_eq!(
                cpu.pixel_rgba(surf, (b.x + dx) as u32, (b.y + dy) as u32),
                Some(PRIMARY),
                "fill at ({dx}, {dy}) stays Primary"
            );
        }
    }

    /// Round 11.1 (decision 305): asymmetric pads shift inner
    /// children per side through the whole harness pipe (mount +
    /// pump + committed boxes — the layout contract app developers
    /// observe, not engine internals).
    #[derive(Clone)]
    struct Padded {
        marker: oppa::Signal<bool>,
    }

    impl Props for Padded {}

    fn render_padded(_ctx: &Ctx, p: &Padded) -> VNode {
        let _ = p.marker.clone();
        oppa::Div("pad")
            .style(
                oppa::Style::new()
                    .size(200, 100)
                    .pad_left(24)
                    .pad_top(10)
                    .pad_right(6)
                    .pad_bottom(14),
            )
            .child(
                oppa::Div("kid")
                    .style(oppa::Style::new().size(40, 20))
                    .build(),
            )
    }

    #[test]
    fn asymmetric_pads_shift_content_per_side() {
        let app = Harness::new();
        let marker = app.host().runtime().signal(false);
        app.mount("P", Padded { marker }, render_padded);
        let pad = app.node("pad");
        let kid = app.node("kid");
        let pb = app.host().committed_box(pad).expect("pad laid out");
        let kb = app.host().committed_box(kid).expect("kid laid out");
        assert_eq!((pb.w, pb.h), (200.0, 100.0));
        assert_eq!(
            (kb.x - pb.x, kb.y - pb.y),
            (24.0, 10.0),
            "left/top pads offset the child, box {kb:?}"
        );
        assert_eq!(
            (kb.w, kb.h),
            (40.0, 20.0),
            "explicit child size keeps, box {kb:?}"
        );
    }
}
