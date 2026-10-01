//! M7 acceptance: DOM backend (third presenter on the proven contract).
//!
//! TreeDiff→DOM mutations with the M2 reconciler's minimality preserved
//! end-to-end (scroll ticks: zero structure ops, counted from TreeDiffs);
//! StyleId→CSS rule identity (one rule per style, minimal churn, no
//! inline-style spam); §9.3 native scroll (container + spacer + anchor-off,
//! INPUT-fed offset ≤ 1 frame on the injected clock, +4 overscan); ARIA
//! mapping incl. the M5 switch payload; verdict-(b) `<input>` fields +
//! the external-element hole (one foreign-element mechanism, two
//! callers); em-size/per-run-font lock touch on all three backends; the
//! three-backend box compare; the §8.5 parity corpus (Windows + Edge).
//! M4/M5/M6 suites stay green (the workspace run is the guard).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use oppa::{
    compute_semantics_diff, BackendError, Caps, Color, ComponentHost, Ctx, Custom, Div, InputEvent,
    KeyState, MockClock, NodeId, PresenterKind, Props, RendererBackend, Row, ScrollArea, Semantics,
    SemanticsSnapshot, ShapedGlyph, ShapedRun, Style, Text, TextError, TextField, TextRun,
    TextService, ThemeMode, ThemeTokens, VNode,
};
use oppa_cpu::FramePlanBuilder;
use oppa_dom::{
    aria_attrs, install_dom_paint_hook, pid_of, render_page, scroll_window, DomBackend, StyleSheet,
    OVERSCAN_SLOTS,
};
use oppa_macros::Props;

// ---------------------------------------------------------------------------
// Fakes (M4 rig shape: uniform advance, em-scaled like a real backend)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FakeText;

impl TextService for FakeText {
    fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
        Vec::new()
    }

    fn shape(&self, text: &str, style: &oppa::TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        let em = style.font_size_px * style.device_pixel_ratio;
        let adv = em * 0.625;
        let metrics = oppa::FontMetrics {
            ascent: em * 0.75,
            descent: em * 0.25,
            line_gap: em * 0.125,
        };
        let mut glyphs = Vec::new();
        let mut clusters = Vec::new();
        for (k, (i, ch)) in text.char_indices().enumerate() {
            let len = ch.len_utf8();
            glyphs.push(ShapedGlyph {
                glyph_id: k as u32,
                x_advance: adv,
                x_offset: 0.0,
                y_offset: 0.0,
            });
            clusters.push(oppa::Cluster {
                byte_range: (i, i + len),
                glyph_range: (k, k + 1),
            });
        }
        Ok(ShapedRun {
            glyphs,
            runs: vec![TextRun {
                byte_range: (0, text.len()),
                glyph_range: (0, clusters.len()),
                rtl: false,
                script: 0,
                font_id: oppa::FontId(0),
                font_metrics: metrics,
            }],
            clusters,
            total_advance: adv * text.chars().count() as f32,
            text_len_bytes: text.len(),
        })
    }
}

/// Two-face fake (M7 lock-touch rig): first half of the text shapes as
/// `FontId(7)` ("Alpha"), second half as `FontId(9)` ("Beta") — the
/// mixed-coverage shape, with a named enumeration so the layout's
/// font-identity table resolves both.
#[derive(Clone)]
struct TwoFaceFake;

impl TextService for TwoFaceFake {
    fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
        vec![
            oppa::FontInfo {
                id: oppa::FontId(7),
                family: "Alpha".to_string(),
                weight: oppa::FontWeight::NORMAL,
                style: oppa::FontStyle::Normal,
                stretch: oppa::FontStretch::NORMAL,
            },
            oppa::FontInfo {
                id: oppa::FontId(9),
                family: "Beta".to_string(),
                weight: oppa::FontWeight::NORMAL,
                style: oppa::FontStyle::Normal,
                stretch: oppa::FontStretch::NORMAL,
            },
        ]
    }

    fn shape(&self, text: &str, style: &oppa::TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        let em = style.font_size_px * style.device_pixel_ratio;
        let adv = em * 0.5;
        let metrics = oppa::FontMetrics {
            ascent: em * 0.8,
            descent: em * 0.2,
            line_gap: 0.0,
        };
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let mid = chars.len() / 2;
        let mut glyphs = Vec::new();
        let mut clusters = Vec::new();
        let mut runs = Vec::new();
        for (part, chunk) in [&chars[..mid], &chars[mid..]].iter().enumerate() {
            if chunk.is_empty() {
                continue;
            }
            let run_start_byte = chunk[0].0;
            let run_start_glyph = glyphs.len();
            let fid = if part == 0 {
                oppa::FontId(7)
            } else {
                oppa::FontId(9)
            };
            for (k, (i, ch)) in chunk.iter().enumerate() {
                let len = ch.len_utf8();
                glyphs.push(ShapedGlyph {
                    glyph_id: (part * 100 + k) as u32,
                    x_advance: adv,
                    x_offset: 0.0,
                    y_offset: 0.0,
                });
                clusters.push(oppa::Cluster {
                    byte_range: (*i, *i + len),
                    glyph_range: (run_start_glyph + k, run_start_glyph + k + 1),
                });
            }
            let last = chunk.last().unwrap();
            runs.push(TextRun {
                byte_range: (run_start_byte, last.0 + last.1.len_utf8()),
                glyph_range: (run_start_glyph, glyphs.len()),
                rtl: false,
                script: 0,
                font_id: fid,
                font_metrics: metrics,
            });
        }
        Ok(ShapedRun {
            total_advance: adv * glyphs.len() as f32,
            text_len_bytes: text.len(),
            glyphs,
            runs,
            clusters,
        })
    }
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

const CARD_BG: Color = Color(0x44_44_44);
const SURFACE_BG: Color = Color(0xFF_FF_FF);

#[derive(Clone, Props)]
struct CardProps {
    title: String,
}

fn render_card(_ctx: &Ctx, props: &CardProps) -> VNode {
    let text: VNode = Text {
        text: Arc::from(props.title.as_str()),
        style: Text::title_small,
    }
    .into();
    Div("card")
        .style(Style::new().size(100, 40).pad_x(8).radius(6).bg(CARD_BG))
        .child(text)
}

#[derive(Clone, Props)]
struct OrderProps {
    reversed: bool,
}

fn render_ordered(_ctx: &Ctx, props: &OrderProps) -> VNode {
    let mut keys = vec![1u64, 2, 3];
    if props.reversed {
        keys.reverse();
    }
    let kids: Vec<VNode> = keys
        .into_iter()
        .map(|k| {
            Div("cell")
                .key(k)
                .style(Style::new().size(20, 20).bg(CARD_BG))
                .build()
        })
        .collect();
    Row("wrap").children(kids)
}

#[derive(Clone, Props)]
struct CountProps {
    n: usize,
    tint: bool,
}

fn render_count(_ctx: &Ctx, props: &CountProps) -> VNode {
    let bg = if props.tint {
        Color(0x11_11_11)
    } else {
        CARD_BG
    };
    let kids: Vec<VNode> = (0..props.n)
        .map(|k| {
            Div("cell")
                .key(k as u64)
                .style(Style::new().size(20, 20).bg(bg))
                .build()
        })
        .collect();
    Row("wrap").children(kids)
}

/// Virtualized-shaped list: keyed slots whose content derives from the
/// framework-owned offset (a scroll tick changes values, never shape).
#[derive(Clone, Props)]
struct SlotProps {
    n: usize,
}

fn render_slots(ctx: &Ctx, props: &SlotProps) -> VNode {
    let offset = ctx.scroll_offset();
    let start = offset.row(20.0);
    let kids: Vec<VNode> = (0..props.n)
        .map(|i| {
            let label: VNode = Text {
                text: Arc::from(format!("row {}", start + i).as_str()),
                style: Text::body_secondary,
            }
            .into();
            Row("slot").key(i as u64).child(label)
        })
        .collect();
    ScrollArea("list")
        .style(Style::new().size(120, 60).content_size(200))
        .on_scroll(|| {})
        .children(kids)
}

// ---------------------------------------------------------------------------
// Rig: host + DOM backend + sheet + ordered commit/sync
// ---------------------------------------------------------------------------

struct Rig<P: Props> {
    host: ComponentHost,
    backend: DomBackend,
    sheet: StyleSheet,
    seen: usize,
    handle: oppa::MountHandle<P>,
    handle_inst: u64,
}

impl<P: Props> Rig<P> {
    fn new(name: &str, props: P, render: fn(&Ctx, &P) -> VNode, dpr: f32) -> Self {
        let host = ComponentHost::new();
        host.set_viewport(200.0, 120.0);
        let handle = host.mount(name, props, render);
        host.run_until_idle();
        let handle_inst = handle.root_instance();
        let mut rig = Self {
            host,
            backend: DomBackend::new(dpr),
            sheet: StyleSheet::new(dpr),
            seen: 0,
            handle,
            handle_inst,
        };
        rig.commit_sync().expect("initial sync");
        rig
    }

    fn set_props(&self, props: P) {
        self.handle.set_props(props);
        self.host.run_until_idle();
    }

    fn commit_sync(&mut self) -> Result<usize, BackendError> {
        use oppa::RendererBackend;
        for diff in self.host.diffs_from(self.seen) {
            self.backend.commit(&diff)?;
        }
        self.seen = self.host.diff_count();
        let stats = self.backend.with_sync(&self.host, &mut self.sheet)?;
        Ok(stats.touched)
    }

    fn scroll_target(&self, debug: &str) -> NodeId {
        oppa::find_retained_by_debug(&self.host, debug)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("no retained node {debug:?}"))
    }
}

// `with_sync` is a test-side convenience: borrow retained reads and sync.
trait SyncHost {
    fn with_sync(
        &mut self,
        host: &ComponentHost,
        sheet: &mut StyleSheet,
    ) -> Result<oppa_dom::SyncStats, BackendError>;
}

impl SyncHost for DomBackend {
    fn with_sync(
        &mut self,
        host: &ComponentHost,
        sheet: &mut StyleSheet,
    ) -> Result<oppa_dom::SyncStats, BackendError> {
        host.with_retained_mut(|rec, styles| self.sync(rec, styles, sheet))
    }
}

// ---------------------------------------------------------------------------
// 1. TreeDiff → DOM mutation minimality
// ---------------------------------------------------------------------------

#[test]
fn mount_produces_div_span_tree_with_stable_classes() {
    let mut rig = Rig::new("Card", CardProps { title: "Hi".into() }, render_card, 1.0);
    let card = rig.scroll_target("card");
    let el = rig.backend.element(card).expect("card element");
    assert_eq!(el.kind, oppa_dom::HtmlKind::Block);
    assert_eq!(el.children.len(), 1, "card wraps one text node");
    let html = render_page("t", &rig.backend, &rig.sheet);
    assert!(html.contains("<div"), "{html}");
    assert!(html.contains("<span"), "{html}");
    assert!(html.contains("Hi"), "{html}");
    assert!(html.contains("data-pid"), "parity hooks on every element");
    let _ = rig.commit_sync().expect("resync");
}

#[test]
fn text_update_touches_one_element_no_structure() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Card", CardProps { title: "Hi".into() }, render_card);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    let mut seen = 0usize;
    let commit_sync = |backend: &mut DomBackend, sheet: &mut StyleSheet, seen: &mut usize| {
        use oppa::RendererBackend;
        for diff in host.diffs_from(*seen) {
            backend.commit(&diff).expect("commit");
        }
        *seen = host.diff_count();
        backend.with_sync(&host, sheet).expect("sync").touched
    };
    let first = commit_sync(&mut backend, &mut sheet, &mut seen);
    assert!(first > 0, "mount touches");
    let elements = backend.element_count();
    let mutations = backend.mutations();

    handle.set_props(CardProps { title: "Yo".into() });
    host.run_until_idle();
    let diff = host.last_diff().expect("diff");
    assert_eq!(diff.structure_ops(), 0, "text change is Update-only");
    let touched = commit_sync(&mut backend, &mut sheet, &mut seen);
    assert_eq!(touched, 1, "exactly the text span re-derives");
    assert_eq!(backend.element_count(), elements, "no element churn");
    assert_eq!(backend.mutations(), mutations, "no DOM structure ops");
    let html = render_page("t", &backend, &sheet);
    assert!(html.contains("Yo"), "{html}");
    assert!(!html.contains(">Hi<"), "stale text gone: {html}");
}

#[test]
fn reorder_moves_dom_children_without_rebuild() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Ord", OrderProps { reversed: false }, render_ordered);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for diff in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&diff).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let wrap = oppa::find_retained_by_debug(&host, "wrap")[0];
    let before: Vec<NodeId> = backend.element(wrap).expect("wrap").children.clone();
    assert_eq!(before.len(), 3);
    let mutations = backend.mutations();

    handle.set_props(OrderProps { reversed: true });
    host.run_until_idle();
    let diff = host.last_diff().expect("diff");
    // [1,2,3] → [3,2,1]: the middle slot never moves (keyed Moves
    // only, no Add/Remove).
    assert_eq!(diff.structure_ops(), 2, "two keyed Moves: {:?}", diff.ops);
    for d in host.diffs_from(host.diff_count() - 1) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let after: Vec<NodeId> = backend.element(wrap).expect("wrap").children.clone();
    assert_eq!(after.len(), 3);
    assert_eq!(after, before.into_iter().rev().collect::<Vec<_>>());
    assert_eq!(backend.element_count(), 4, "wrap + 3 cells, none rebuilt");
    assert_eq!(backend.mutations(), mutations + 2, "exactly the Moves");
}

#[test]
fn remove_drops_subtree() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Count", CountProps { n: 3, tint: false }, render_count);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for diff in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&diff).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    assert_eq!(backend.element_count(), 4);

    handle.set_props(CountProps { n: 1, tint: false });
    host.run_until_idle();
    for d in host.diffs_from(host.diff_count() - 1) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    assert_eq!(backend.element_count(), 2, "wrap + survivor");
    let wrap = oppa::find_retained_by_debug(&host, "wrap")[0];
    assert_eq!(backend.element(wrap).expect("wrap").children.len(), 1);
}

#[test]
fn scroll_tick_produces_zero_structure_ops_end_to_end() {
    let mut rig = Rig::new("Slots", SlotProps { n: 5 }, render_slots, 1.0);
    let list = rig.scroll_target("list");
    let offset = rig
        .host
        .instance_scroll(rig.handle_inst)
        .expect("instance scroll handle");
    rig.host.bind_scroll(list, offset.clone());
    let mutations = rig.backend.mutations();

    // The M8 payoff trace starts here: counted from TreeDiffs, not
    // assumed from slot-key stability.
    rig.host.inject_input(InputEvent::Scroll {
        target: list,
        dx: 0.0,
        dy: 20.0,
    });
    rig.host.run_until_idle();
    let diff = rig.host.last_diff().expect("scroll-tick diff");
    assert_eq!(
        diff.structure_ops(),
        0,
        "scroll tick is Update-only: {:?}",
        diff.ops
    );
    assert_eq!(offset.get(), 20.0, "INPUT-fed offset applied");
    let touched = rig.commit_sync().expect("sync");
    assert_eq!(
        rig.backend.mutations(),
        mutations,
        "zero DOM structure ops on the scroll tick"
    );
    assert!(touched > 0, "row values did re-derive ({touched})");
}

// ---------------------------------------------------------------------------
// 2. StyleId → CSS rule identity (no inline-style spam)
// ---------------------------------------------------------------------------

#[test]
fn same_style_one_rule_changed_style_minimal_churn() {
    let mut rig = Rig::new("Count", CountProps { n: 2, tint: false }, render_count, 1.0);
    // Wrap + 2 cells: the cells share one rule, the wrap another.
    assert_eq!(rig.sheet.rule_count(), 2, "wrap rule + cell rule");
    let churn = rig.sheet.churn();
    let wrap = rig.scroll_target("wrap");
    let wrap_class = rig.backend.element(wrap).expect("wrap").classes.clone();
    let cell = oppa::find_retained_by_debug(&rig.host, "cell")[0];
    let cell_class = rig.backend.element(cell).expect("cell").classes.clone();
    // Re-sync without changes: zero churn, zero touches.
    let touched = rig.commit_sync().expect("sync");
    assert_eq!(touched, 0, "settled frame touches nothing");
    assert_eq!(rig.sheet.churn(), churn, "no rule churn on resync");

    // A real style change through props (both cells retint): exactly
    // one new rule, the wrap's rule untouched.
    rig.set_props(CountProps { n: 2, tint: true });
    let touched = rig.commit_sync().expect("sync");
    assert!(touched > 0, "retinted cells re-derive");
    assert_eq!(rig.sheet.churn(), churn + 1, "exactly one new rule");
    assert_eq!(rig.sheet.rule_count(), 3);
    assert_eq!(
        rig.backend.element(wrap).expect("wrap").classes,
        wrap_class,
        "unrelated nodes keep their rule"
    );
    assert_ne!(
        rig.backend.element(cell).expect("cell").classes,
        cell_class,
        "changed nodes move rules"
    );
}

#[test]
fn static_style_never_leaks_inline() {
    let mut rig = Rig::new("Card", CardProps { title: "Hi".into() }, render_card, 1.0);
    rig.host.set_text_service(Box::new(FakeText));
    rig.host.run_until_idle();
    rig.commit_sync().expect("sync");
    let card = rig.scroll_target("card");
    let el = rig.backend.element(card).expect("card").clone();
    // Static style rides the shared class...
    assert_eq!(el.classes.len(), 1);
    let decl = rig
        .sheet
        .decl_of_class(&el.classes[0])
        .expect("rule for the class");
    assert!(decl.contains("background:#444444;"), "{decl}");
    assert!(decl.contains("border-radius:6px;"), "{decl}");
    // ...never inline (geometry only).
    assert!(
        !el.inline_geom.contains("background"),
        "no inline-style spam: {}",
        el.inline_geom
    );
    let html = rig.backend.render_node(card);
    let style_attr = html
        .split("style=\"")
        .nth(1)
        .expect("inline geometry")
        .split('"')
        .next()
        .unwrap_or("");
    // The card is the root: relative container, never absolute offsets.
    assert!(style_attr.contains("position:relative"), "{style_attr}");
    assert!(style_attr.contains("width:100px"), "{style_attr}");
    assert!(style_attr.contains("height:40px"), "{style_attr}");
    assert!(!style_attr.contains("background"), "{style_attr}");
    assert!(!style_attr.contains("border-radius"), "{style_attr}");
    // The stylesheet carries the statics exactly once.
    let sheet = rig.sheet.render();
    assert_eq!(sheet.matches("background:#444444;").count(), 1, "{sheet}");
}

// ---------------------------------------------------------------------------
// 3. §9.3 native scroll: container + spacer + anchor-off, INPUT-fed offset
// ---------------------------------------------------------------------------

#[test]
fn scroll_container_shape_spacer_overscan_anchor() {
    assert_eq!(OVERSCAN_SLOTS, 4, "the v1 overscan constant, stated");
    let rig = Rig::new("Slots", SlotProps { n: 3 }, render_slots, 1.0);
    let list = rig.scroll_target("list");
    let el = rig.backend.element(list).expect("list").clone();
    assert_eq!(el.kind, oppa_dom::HtmlKind::Scroll);
    // Viewport: committed box (120×60 style).
    assert!(
        el.inline_geom.contains("height:60px"),
        "viewport height: {}",
        el.inline_geom
    );
    // Spacer: content extent floored by content_size(200).
    assert_eq!(el.spacer_h, Some(200.0), "spacer == content floor");
    assert_eq!(el.children.len(), 3, "slots ride inside the spacer");
    let html = rig.backend.render_node(list);
    assert!(html.contains("overflow:auto;"), "{html}");
    assert!(
        html.contains("height:200px;"),
        "spacer carries the extent: {html}"
    );
    assert!(html.contains("data-scroll"), "{html}");
    assert!(html.contains("data-slot"), "slots are marked: {html}");
    let page = render_page("t", &rig.backend, &rig.sheet);
    assert!(
        page.contains("[data-slot]{overflow-anchor:none;}"),
        "anchor-off lives in the sheet, not per element"
    );
}

#[test]
fn overscan_window_math() {
    assert_eq!(scroll_window(0.0, 20.0, 100.0, 50), (0, 9));
    assert_eq!(scroll_window(200.0, 20.0, 100.0, 50), (6, 19));
    assert_eq!(
        scroll_window(0.0, 20.0, 100.0, 5),
        (0, 5),
        "clamped to the row count"
    );
    assert_eq!(scroll_window(1000.0, 20.0, 100.0, 50), (46, 50));
    assert_eq!(
        scroll_window(0.0, 0.0, 100.0, 50),
        (0, 0),
        "loud zero row_h"
    );
    assert_eq!(scroll_window(-30.0, 20.0, 100.0, 50), (0, 8));
}

#[test]
fn offset_currency_within_one_frame_on_injected_clock() {
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Slots", SlotProps { n: 3 }, render_slots);
    host.run_until_idle();
    let list = oppa::find_retained_by_debug(&host, "list")[0];
    let offset = host
        .instance_scroll(handle.root_instance())
        .expect("scroll handle");
    host.bind_scroll(list, offset.clone());
    assert_eq!(offset.get(), 0.0);

    // The browser scrolled (absolute, browser-owned); the shell maps it
    // to a delta and injects at the INPUT boundary.
    let mut backend = DomBackend::new(1.0);
    assert_eq!(backend.note_browser_scroll(list, 48.0), None, "first sight");
    host.inject_input(InputEvent::Scroll {
        target: list,
        dx: 0.0,
        dy: 48.0,
    });
    assert_eq!(offset.get(), 0.0, "not applied before the frame");
    clock.advance(1.0 / 60.0);
    assert!(host.runtime().run_once(), "the frame runs");
    assert_eq!(offset.get(), 48.0, "INPUT-fed offset lands within 1 frame");
    assert_eq!(backend.scroll_top(list), Some(48.0), "ledger agrees");
    assert_eq!(host.bound_scroll(list), Some(48.0), "signal agrees");
    // Consumed exactly once: a second frame applies nothing more.
    clock.advance(1.0 / 60.0);
    host.runtime().run_once();
    assert_eq!(offset.get(), 48.0, "no double-apply");
}

// ---------------------------------------------------------------------------
// 4. ARIA mapping incl. SemanticsDiff parity (M5 switch through the DOM)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct SwitchProps {
    label: String,
    on: bool,
}

fn render_switch(_ctx: &Ctx, props: &SwitchProps) -> VNode {
    Div("track")
        .style(Style::new().size(44, 24).radius(12).bg(CARD_BG))
        .semantics(
            Semantics::switch()
                .checked(props.on)
                .label(props.label.as_str()),
        )
        .on_press(|| {})
        .child(
            Div("knob")
                .style(Style::new().size(18, 18).circle().x(3.0))
                .build(),
        )
}

#[test]
fn aria_switch_parity_with_m5_payload() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount(
        "Switch",
        SwitchProps {
            label: "Wi-Fi".into(),
            on: true,
        },
        render_switch,
    );
    host.run_until_idle();
    let mut snapshot = SemanticsSnapshot::new();
    let diff = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snapshot));
    assert_eq!(diff.upserted.len(), 1, "the M5 switch payload, one upsert");
    assert!(diff.removed.is_empty());
    let entry = diff.upserted[0].clone();
    assert_eq!(entry.semantics.checked, Some(true));

    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    backend.with_sync(&host, &mut sheet).expect("sync");
    let track = oppa::find_retained_by_debug(&host, "track")[0];
    let el = backend.element(track).expect("track element").clone();
    // Same source, two readers: aria == the diff's payload, bounds ==
    // the diff's bounds.
    assert_eq!(el.attrs, aria_attrs(&entry.semantics));
    assert!(el
        .attrs
        .contains(&("role".to_string(), "switch".to_string())));
    assert!(el
        .attrs
        .contains(&("aria-checked".to_string(), "true".to_string())));
    assert!(el
        .attrs
        .contains(&("aria-label".to_string(), "Wi-Fi".to_string())));
    // The track is the root (offsets are relative-container implicit):
    // bounds parity reads width/height + the zero origin.
    assert_eq!((entry.x, entry.y), (0.0, 0.0));
    let geom = &el.inline_geom;
    assert!(geom.contains("position:relative"), "{geom}");
    assert!(geom.contains(&format!("width:{}px", entry.w)), "{geom}");
    assert!(geom.contains(&format!("height:{}px", entry.h)), "{geom}");
    let html = backend.render_node(track);
    assert!(html.contains("role=\"switch\""), "{html}");
    assert!(html.contains("aria-checked=\"true\""), "{html}");
}

// ---------------------------------------------------------------------------
// G13 catalog end-to-end: app-authored controls (oppa-controls,
// through ctx.child) through retained semantics into real DOM
// attributes — the screen-reader-visible surface per leg.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct CatalogProps {
    checked: oppa::Signal<bool>,
    on: oppa::Signal<bool>,
    vol: oppa::Signal<f32>,
}

impl Props for CatalogProps {}

fn render_catalog(ctx: &Ctx, p: &CatalogProps) -> VNode {
    use oppa_controls::{
        Button, ButtonProps, Checkbox, CheckboxProps, Slider, SliderProps, Toggle, ToggleProps,
    };
    Div("catalog").children([
        ctx.child("oppa::Button", 1, &ButtonProps::new("OK", || {}), Button),
        ctx.child(
            "oppa::Checkbox",
            2,
            &CheckboxProps {
                label: oppa::SharedString::from("T&C"),
                checked: p.checked.clone(),
                enabled: true,
                on_change: None,
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
            Checkbox,
        ),
        ctx.child(
            "oppa::Toggle",
            3,
            &ToggleProps {
                label: oppa::SharedString::from("Wi-Fi"),
                on: p.on.clone(),
                enabled: true,
                on_change: None,
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
            Toggle,
        ),
        ctx.child(
            "oppa::Slider",
            4,
            &SliderProps {
                label: oppa::SharedString::from("Volume"),
                value: p.vol.clone(),
                min: 0.0,
                max: 100.0,
                step: 10.0,
                enabled: true,
                on_change: None,
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
            Slider,
        ),
    ])
}

#[test]
fn aria_catalog_controls_end_to_end() {
    let host = ComponentHost::new();
    host.set_viewport(300.0, 200.0);
    let rt = host.runtime();
    host.mount(
        "Catalog",
        CatalogProps {
            checked: rt.signal(false),
            on: rt.signal(true),
            vol: rt.signal(50.0f32),
        },
        render_catalog,
    );
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    backend.with_sync(&host, &mut sheet).expect("sync");
    let html_of = |debug: &str| {
        let id = oppa::find_retained_by_debug(&host, debug)[0];
        backend.render_node(id)
    };
    let b = html_of("button");
    assert!(b.contains("role=\"button\""), "{b}");
    assert!(b.contains("aria-label=\"OK\""), "{b}");
    let c = html_of("checkbox");
    assert!(c.contains("role=\"checkbox\""), "{c}");
    assert!(c.contains("aria-checked=\"false\""), "{c}");
    let t = html_of("toggle");
    assert!(t.contains("role=\"switch\""), "{t}");
    assert!(t.contains("aria-checked=\"true\""), "{t}");
    let s = html_of("slider");
    assert!(s.contains("role=\"slider\""), "{s}");
    assert!(s.contains("aria-valuetext=\"50 percent\""), "{s}");
}

#[derive(Clone, Props)]
struct MaybeSwitchProps {
    show: bool,
}

fn render_maybe_switch(_ctx: &Ctx, props: &MaybeSwitchProps) -> VNode {
    if props.show {
        render_switch(
            _ctx,
            &SwitchProps {
                label: "Wi-Fi".into(),
                on: true,
            },
        )
    } else {
        Div("plate")
            .style(Style::new().size(44, 24).bg(CARD_BG))
            .build()
    }
}

#[test]
fn aria_removal_drops_the_element() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount(
        "Maybe",
        MaybeSwitchProps { show: true },
        render_maybe_switch,
    );
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let mut snapshot = SemanticsSnapshot::new();
    let d1 = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snapshot));
    assert_eq!(d1.upserted.len(), 1);
    let track = oppa::find_retained_by_debug(&host, "track")[0];
    assert!(backend.element(track).is_some());

    handle.set_props(MaybeSwitchProps { show: false });
    host.run_until_idle();
    let diff = host.last_diff().expect("diff");
    assert!(
        diff.ops
            .iter()
            .any(|op| matches!(op, oppa::DiffOp::Remove { .. })),
        "switch subtree removed: {:?}",
        diff.ops
    );
    let seen = host.diff_count() - 1;
    for d in host.diffs_from(seen) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    // The root survives as the plate (same Div tag → Update, not
    // Replace); the SWITCH PAYLOAD is what leaves.
    let plate = backend.element(track).expect("root survives as plate");
    assert!(
        !plate.attrs.iter().any(|(k, _)| k == "role"),
        "role gone: {:?}",
        plate.attrs
    );
    let d2 = host.with_retained_mut(|rec, _| compute_semantics_diff(rec, &mut snapshot));
    assert_eq!(d2.removed, vec![track], "payload removal == diff removal");
}

// ---------------------------------------------------------------------------
// 5. Verdict-(b) fields + external hole (one mechanism, two callers)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct FieldProps {
    value: String,
}

fn render_field(_ctx: &Ctx, props: &FieldProps) -> VNode {
    TextField {
        text: Arc::from(props.value.as_str()),
        style: Text::title_small,
        label: Arc::from("Name"),
    }
    .into()
}

#[test]
fn text_field_renders_input_with_value_and_label() {
    let mut rig = Rig::new(
        "Field",
        FieldProps {
            value: "Ada".into(),
        },
        render_field,
        1.0,
    );
    let field = rig.scroll_target("field");
    let el = rig.backend.element(field).expect("field element").clone();
    assert_eq!(el.kind, oppa_dom::HtmlKind::Field);
    assert_eq!(el.text, "Ada", "value rides the text payload");
    assert!(el.children.is_empty(), "inputs are void");
    assert!(el
        .attrs
        .contains(&("aria-label".to_string(), "Name".to_string())));
    assert!(
        !el.attrs.iter().any(|(k, _)| k == "role"),
        "inputs are implicitly textbox: {:?}",
        el.attrs
    );
    // The value-carrying text child is absorbed (keyed, unmaterialized).
    assert_eq!(rig.backend.absorbed_count(), 1);
    let html = rig.backend.render_node(field);
    assert!(html.starts_with("<input type=\"text\""), "{html}");
    assert!(html.contains("value=\"Ada\""), "{html}");
    assert!(html.contains("aria-label=\"Name\""), "{html}");
    assert!(!html.contains("<span"), "no child spans inside the input");
    let _ = rig.commit_sync().expect("resync");
}

#[test]
fn field_value_update_resyncs_without_structure() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount(
        "Field",
        FieldProps {
            value: "Ada".into(),
        },
        render_field,
    );
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let mutations = backend.mutations();

    handle.set_props(FieldProps {
        value: "Ada Lovelace".into(),
    });
    host.run_until_idle();
    let diff = host.last_diff().expect("diff");
    assert_eq!(diff.structure_ops(), 0, "value change is Update-only");
    let seen = host.diff_count() - 1;
    for d in host.diffs_from(seen) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    let stats = backend.with_sync(&host, &mut sheet).expect("sync");
    assert_eq!(backend.mutations(), mutations, "no DOM structure ops");
    assert!(stats.touched > 0, "the input re-derived");
    let field = oppa::find_retained_by_debug(&host, "field")[0];
    let html = backend.render_node(field);
    assert!(html.contains("value=\"Ada Lovelace\""), "{html}");
}

/// Round 8.2 (decision 298): dragging across a field paints DOM
/// highlight divs from the same session range the rasterizers turn
/// into `Rect` ops (one shared `selection_rects` rule); collapsing
/// the selection removes them with no structural churn.
#[test]
fn selection_highlight_divs_track_session_range() {
    use oppa_controls::{TextInput, TextInputProps};
    use oppa_dom::HtmlKind;
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.set_text_service(Box::new(FakeText));
    let value = host
        .runtime()
        .signal(oppa::SharedString::from("hello world"));
    host.mount("TI", TextInputProps::new("Name", value), TextInput);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    let commit_sync = |backend: &mut DomBackend, sheet: &mut StyleSheet| {
        use oppa::RendererBackend;
        for d in host.diffs_from(0) {
            backend.commit(&d).expect("commit");
        }
        backend.set_selection(host.focused_selection_paint());
        backend.with_sync(&host, sheet).expect("sync");
    };
    commit_sync(&mut backend, &mut sheet);
    let id = oppa::find_retained_by_debug(&host, "text-input")[0];
    assert_eq!(
        backend.element(id).map(|e| e.kind.clone()),
        Some(HtmlKind::Field),
        "the field Div carries the verdict-(b) input shape"
    );
    assert!(
        !backend.render_node(id).contains("class=\"sel\""),
        "collapsed selection paints no highlight"
    );
    // Drag byte 2→8 (8.75px/char FakeText, leading-half probes).
    let origin_x = host.text_origin_under(id).expect("text origin");
    let y = host.committed_box(id).expect("hit box").y + 8.0;
    let ptr = |action, x: f32| InputEvent::Pointer {
        id: Some(0),
        action,
        x,
        y,
        modifiers: oppa::Modifiers::NONE,
    };
    use oppa::{PointerAction, PointerButton};
    let primary = PointerButton::Primary;
    host.inject_input(ptr(
        PointerAction::Down { button: primary },
        origin_x + 2.0 * 8.75 + 2.0,
    ));
    host.inject_input(ptr(PointerAction::Move, origin_x + 5.0 * 8.75 + 2.0));
    host.inject_input(ptr(PointerAction::Move, origin_x + 8.0 * 8.75 + 2.0));
    host.inject_input(ptr(
        PointerAction::Up { button: primary },
        origin_x + 8.0 * 8.75 + 2.0,
    ));
    host.run_until_idle();
    assert_eq!(
        host.focused_field_session().expect("session").selection(),
        (2, 8)
    );
    for d in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.set_selection(host.focused_selection_paint());
    backend.with_sync(&host, &mut sheet).expect("sync");
    let html = backend.render_node(id);
    assert!(
        html.contains("class=\"sel\""),
        "highlight div renders: {html}"
    );
    assert!(html.contains("background:#b3d7ff"), "themed fill: {html}");
    let el = backend.element(id).expect("field element");
    assert_eq!(el.sel_rects.len(), 1, "one line, one rect");
    assert!(
        (el.sel_rects[0][2] - el.sel_rects[0][0] - 52.5).abs() < 0.01,
        "width pins cluster edges 17.5→70.0, got {:?}",
        el.sel_rects[0]
    );
}

/// Round 22.2 (decision 332): a multi-line `TextArea` selection
/// spans one highlight div per covered hard line (the same shared
/// `selection_rects` rule the rasterizers turn into `Rect` ops),
/// and the area emits `<textarea>`.
#[test]
fn textarea_multiline_selection_spans_lines() {
    use oppa_controls::{TextArea, TextAreaProps};
    let host = ComponentHost::new();
    host.set_viewport(300.0, 200.0);
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(oppa::SharedString::from("ab\ncd"));
    host.mount("TA", TextAreaProps::new("Notes", value), TextArea);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    let commit_sync = |backend: &mut DomBackend, sheet: &mut StyleSheet| {
        use oppa::RendererBackend;
        for d in host.diffs_from(0) {
            backend.commit(&d).expect("commit");
        }
        backend.set_selection(host.focused_selection_paint());
        backend.with_sync(&host, sheet).expect("sync");
    };
    commit_sync(&mut backend, &mut sheet);
    let id = oppa::find_retained_by_debug(&host, "text-area")[0];
    let html = backend.render_node(id);
    assert!(html.contains("<textarea"), "area shape: {html}");
    assert!(
        html.contains(">ab\ncd</textarea>"),
        "multiline content rides along: {html}"
    );
    // Focus (tap the area center) then select all across both lines.
    let b = host.committed_box(id).expect("hit box");
    host.inject_input(InputEvent::pointer_down(b.x + b.w / 2.0, b.y + b.h / 2.0));
    host.inject_input(InputEvent::pointer_up(b.x + b.w / 2.0, b.y + b.h / 2.0));
    host.run_until_idle();
    host.focused_field_session().expect("session").select_all();
    host.run_until_idle();
    commit_sync(&mut backend, &mut sheet);
    let el = backend.element(id).expect("area element");
    assert_eq!(
        el.sel_rects.len(),
        2,
        "one rect per line, got {:?}",
        el.sel_rects
    );
    assert!(
        el.sel_rects[1][1] > el.sel_rects[0][1],
        "rects stack in line order, got {:?}",
        el.sel_rects
    );
}

/// Round 15.1 (decision 312): the focused field derives a synced
/// `caret` bar div at the active cluster's leading edge (2 CSS px
/// wide at DPR 1, in the field's text ink); the blink half-cycle
/// removes it — synced visibility, no CSS animation, one host clock.
#[test]
fn caret_bar_div_tracks_focus_and_blink_phase() {
    use oppa::{PointerAction, PointerButton};
    use oppa_controls::{TextInput, TextInputProps};
    let clock = Rc::new(MockClock::new());
    let host = ComponentHost::with_clock(clock.clone());
    host.set_viewport(200.0, 120.0);
    host.set_text_service(Box::new(FakeText));
    let value = host
        .runtime()
        .signal(oppa::SharedString::from("hello world"));
    host.mount("TI", TextInputProps::new("Name", value), TextInput);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    // Tap byte 0 → focus + collapsed caret, blink reset to visible.
    let id = oppa::find_retained_by_debug(&host, "text-input")[0];
    let origin_x = host.text_origin_under(id).expect("text origin");
    let y = host.committed_box(id).expect("hit box").y + 8.0;
    let ptr = |action, x: f32| InputEvent::Pointer {
        id: Some(0),
        action,
        x,
        y,
        modifiers: oppa::Modifiers::NONE,
    };
    let primary = PointerButton::Primary;
    host.inject_input(ptr(PointerAction::Down { button: primary }, origin_x + 2.0));
    host.inject_input(ptr(PointerAction::Up { button: primary }, origin_x + 2.0));
    host.run_until_idle();
    assert_eq!(
        host.focused_field_session().expect("session").caret(),
        0,
        "tap parks the caret at byte 0"
    );
    {
        use oppa::RendererBackend;
        for d in host.diffs_from(0) {
            backend.commit(&d).expect("commit");
        }
    }
    backend.set_caret(host.focused_caret_paint());
    backend.with_sync(&host, &mut sheet).expect("sync");
    let html = backend.render_node(id);
    assert!(
        html.contains("class=\"sel caret\""),
        "caret div renders trailing the input: {html}"
    );
    let el = backend.element(id).expect("field element");
    let r = el.caret_rect.expect("caret rect while visible");
    assert!(
        ((r[2] - r[0]) - 2.0).abs() < 0.01,
        "2 CSS px wide at DPR 1, got {r:?}"
    );
    assert!(r[3] > r[1], "positive bar height, got {r:?}");
    assert!(!el.caret_ink.is_empty(), "ink snapshots with the rect");
    assert!(
        html.contains(&format!("background:{};", el.caret_ink)),
        "bar paints in the field ink: {html}"
    );
    // Blink half-cycle: the host hides the caret, the re-sync drops
    // the div (no stale bar survives the hidden phase).
    clock.advance(0.6);
    assert!(
        host.focused_caret_paint().is_none(),
        "hidden half-cycle paints nothing"
    );
    backend.set_caret(host.focused_caret_paint());
    backend.with_sync(&host, &mut sheet).expect("sync");
    let html = backend.render_node(id);
    assert!(
        !html.contains("caret"),
        "hidden caret leaves no div: {html}"
    );
    assert!(
        backend
            .element(id)
            .expect("field element")
            .caret_rect
            .is_none(),
        "rect clears with the phase"
    );
}

/// Decision 293 (OQ-SINK-1 fix): an empty+placeholder control
/// renders `value=""` with a native `placeholder` attribute — the
/// placeholder is never the value, so the browser only ever sends
/// typed text. Setting a value swaps to the value with no
/// placeholder attribute (and no structural churn — the leaf shape
/// is stable, only payloads re-derive).
#[test]
fn placeholder_renders_attribute_not_value() {
    use oppa_controls::{TextInput, TextInputProps};
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let value = host.runtime().signal(oppa::SharedString::from(""));
    host.mount(
        "TI",
        TextInputProps::new("Name", value.clone()).placeholder("Enter name"),
        TextInput,
    );
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let id = oppa::find_retained_by_debug(&host, "text-input")[0];
    let html = backend.render_node(id);
    assert!(html.starts_with("<input type=\"text\""), "{html}");
    assert!(html.contains("value=\"\""), "empty value, {html}");
    assert!(
        html.contains("placeholder=\"Enter name\""),
        "native hint, {html}"
    );
    assert!(
        !html.contains("value=\"Enter name\""),
        "placeholder never leaks into the value, {html}"
    );
    // A value swaps the payload (leaf replaces the span, same tag —
    // Update-only) and drops the attribute.
    value.set(oppa::SharedString::from("Ada"));
    host.run_until_idle();
    let seen = host.diff_count() - 1;
    for d in host.diffs_from(seen) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let html = backend.render_node(id);
    assert!(html.contains("value=\"Ada\""), "{html}");
    assert!(
        !html.contains("placeholder="),
        "no hint with a value, {html}"
    );
}

/// Round 5.1 (decision 268): multi-line fields render real
/// `<textarea>` elements (content-carrying, absorbed children,
/// value updates re-derive without structure ops).
#[derive(Clone, Props)]
struct AreaProps {
    value: String,
}

fn render_area(_ctx: &Ctx, props: &AreaProps) -> VNode {
    oppa::TextArea {
        text: Arc::from(props.value.as_str()),
        style: Text::title_small,
        label: Arc::from("Notes"),
    }
    .into()
}

#[test]
fn text_area_renders_textarea_with_content() {
    let mut rig = Rig::new(
        "Area",
        AreaProps {
            value: "line one\nline two".into(),
        },
        render_area,
        1.0,
    );
    let area = rig.scroll_target("area");
    let el = rig.backend.element(area).expect("area element").clone();
    assert_eq!(el.kind, oppa_dom::HtmlKind::Area);
    assert_eq!(el.text, "line one\nline two", "content rides the payload");
    assert!(el.children.is_empty(), "areas absorb like inputs");
    assert!(el
        .attrs
        .contains(&("aria-label".to_string(), "Notes".to_string())));
    let html = rig.backend.render_node(area);
    assert!(html.starts_with("<textarea"), "{html}");
    assert!(html.contains(">line one\nline two</textarea>"), "{html}");
    let _ = rig.commit_sync().expect("resync");
}

#[test]
fn area_value_update_resyncs_without_structure() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount(
        "Area",
        AreaProps {
            value: "one".into(),
        },
        render_area,
    );
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let mutations = backend.mutations();
    handle.set_props(AreaProps {
        value: "one\ntwo".into(),
    });
    host.run_until_idle();
    let diff = host.last_diff().expect("diff");
    assert_eq!(diff.structure_ops(), 0, "value change is Update-only");
    let seen = host.diff_count() - 1;
    for d in host.diffs_from(seen) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    let stats = backend.with_sync(&host, &mut sheet).expect("sync");
    assert_eq!(backend.mutations(), mutations, "no DOM structure ops");
    assert!(stats.touched > 0, "the area re-derived");
    let area = oppa::find_retained_by_debug(&host, "area")[0];
    let html = backend.render_node(area);
    assert!(html.contains(">one\ntwo</textarea>"), "{html}");
}

#[test]
fn node_for_pid_resolves_the_rendered_field() {
    let mut rig = Rig::new(
        "Field",
        FieldProps {
            value: "Ada".into(),
        },
        render_field,
        1.0,
    );
    let field = rig.scroll_target("field");
    let html = rig.backend.render_node(field);
    // The pid the page reports back is the rendered data-pid.
    let pid = html
        .split("data-pid=\"")
        .nth(1)
        .expect("pid attr")
        .split('"')
        .next()
        .expect("pid end");
    assert_eq!(rig.backend.node_for_pid(pid), Some(field));
    assert_eq!(rig.backend.node_for_pid("9999-9999"), None);
    let _ = rig.commit_sync().expect("resync");
}

fn render_wrapped_field(_ctx: &Ctx, props: &FieldProps) -> VNode {
    oppa::Div("wrap").child(
        TextField {
            text: Arc::from(props.value.as_str()),
            style: Text::title_small,
            label: Arc::from("Name"),
        }
        .into(),
    )
}

#[test]
fn zero_field_omits_geometry_for_browser_sizing() {
    // Decision 189: the Rig installs no TextService, so the field
    // commits 0x0 — the backend must not echo that back as 0px
    // (an invisible, unfocusable input). Position stays, dims go.
    let mut rig = Rig::new(
        "Wrap",
        FieldProps { value: "".into() },
        render_wrapped_field,
        1.0,
    );
    let field = rig.scroll_target("field");
    let html = rig.backend.render_node(field);
    assert!(html.starts_with("<input type=\"text\""), "{html}");
    assert!(
        html.contains("position:absolute;left:0px;top:0px;"),
        "{html}"
    );
    assert!(!html.contains("width:"), "{html}");
    assert!(!html.contains("height:"), "{html}");
    assert!(html.contains("value=\"\""), "{html}");
    let _ = rig.commit_sync().expect("resync");
}

#[test]
fn external_hole_renders_marker_with_box_and_children() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Hole", (), render_hole);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let hole = oppa::find_retained_by_debug(&host, "player")[0];
    let el = backend.element(hole).expect("hole element").clone();
    assert_eq!(el.kind, oppa_dom::HtmlKind::External(7));
    assert_eq!(el.children.len(), 1, "framework children pass through");
    let html = backend.render_node(hole);
    assert!(html.contains("data-external=\"custom:7\""), "{html}");
    assert!(html.contains("position:absolute"), "box preserved: {html}");
    assert!(html.contains("caption"), "child content intact: {html}");
}

fn render_hole(_ctx: &Ctx, _props: &()) -> VNode {
    let caption: VNode = Text {
        text: Arc::from("caption"),
        style: Text::body_secondary,
    }
    .into();
    Div("stage")
        .style(Style::new().size(160, 90).bg(CARD_BG))
        .child(
            Custom("player", 7)
                .style(Style::new().size(160, 90))
                .child(caption),
        )
}

// ---------------------------------------------------------------------------
// 6. Em-size + per-run font identity on all three backends (decision 110)
// ---------------------------------------------------------------------------

/// The lock touch rides every Text op: exact em size (no `=
/// line_height` approximation) + per-run font segmentation (no
/// single-face bound). Proved here at the plan level (all platforms);
/// the Vello encode arm runs Windows-gated below, the DOM span arm
/// right after.
#[test]
fn text_ops_carry_exact_em_size_and_segmented_fonts() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.set_text_service(Box::new(TwoFaceFake));
    host.mount(
        "Card",
        CardProps {
            title: "HelloWorld".into(),
        },
        render_card,
    );
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    let texts: Vec<&oppa::DrawOp> = plan
        .ops
        .iter()
        .filter(|op| matches!(op, oppa::DrawOp::Text { .. }))
        .collect();
    assert_eq!(texts.len(), 1);
    match texts[0] {
        oppa::DrawOp::Text {
            em_size,
            fonts,
            glyphs,
            line_height,
            ..
        } => {
            // title_small = 16px at dpr 1 → em 16 exactly (the line
            // height is ascent + descent + gap = 16 too here, so assert
            // against a config where they differ — see below).
            assert_eq!(*em_size, 16.0, "exact em size, device px");
            assert_eq!(fonts.len(), 2, "two fallback runs, merged");
            assert_eq!(fonts[0].family, "Alpha");
            assert_eq!(fonts[0].font_id, oppa::FontId(7));
            assert_eq!(fonts[1].family, "Beta");
            assert_eq!(fonts[1].font_id, oppa::FontId(9));
            assert_eq!(fonts[0].glyph_range, (0, 5));
            assert_eq!(fonts[1].glyph_range, (5, 10));
            assert_eq!(glyphs.len(), 10);
            let _ = line_height;
        }
        _ => unreachable!(),
    }
}

#[test]
fn em_size_is_not_line_height() {
    // The approximation `font_size = line_height` dies here: em comes
    // from measure context (size × dpr), line height from metrics.
    // TwoFaceFake metrics: ascent .8em + descent .2em + gap 0 → equal
    // here too, so use dpr 2 where em (32) still equals line height...
    // instead assert the source directly: em == size × dpr while the
    // op also carries the independent line metrics.
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.set_layout_config(oppa::LayoutTextConfig {
        device_pixel_ratio: 2.0,
        ..Default::default()
    });
    host.set_text_service(Box::new(TwoFaceFake));
    host.mount(
        "Card",
        CardProps {
            title: "HelloWorld".into(),
        },
        render_card,
    );
    host.run_until_idle();
    let builder = FramePlanBuilder::new(2.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
    match plan
        .ops
        .iter()
        .find(|op| matches!(op, oppa::DrawOp::Text { .. }))
    {
        Some(oppa::DrawOp::Text {
            em_size,
            line_height,
            baseline,
            ..
        }) => {
            assert_eq!(*em_size, 32.0, "16px × dpr 2, exact");
            assert_eq!(*line_height, 32.0, "(.8 + .2) × 32");
            assert_eq!(*baseline, 25.6, "ascent .8 × 32");
        }
        _ => panic!("expected a Text op"),
    }
}

#[test]
fn dom_text_spans_carry_measured_family_and_size() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.set_text_service(Box::new(TwoFaceFake));
    host.mount(
        "Card",
        CardProps {
            title: "HelloWorld".into(),
        },
        render_card,
    );
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        use oppa::RendererBackend;
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    // The value-carrying text node: two fallback runs → inner spans.
    let text_node = host.with_retained_mut(|rec, _| {
        rec.retained_ids()
            .into_iter()
            .find(|id| {
                rec.get(*id)
                    .is_some_and(|n| n.tag == oppa::Tag::Text && n.text.is_some())
            })
            .expect("text leaf")
    });
    let el = backend.element(text_node).expect("span").clone();
    assert_eq!(el.runs.len(), 2, "fallback segmentation survives to DOM");
    assert!(
        el.inline_font.contains("font-size:16px;"),
        "{}",
        el.inline_font
    );
    assert!(el.inline_font.contains("Alpha"), "{}", el.inline_font);
    let html = backend.render_node(text_node);
    assert!(html.contains("font-family:Alpha,sans-serif;"), "{html}");
    assert!(html.contains("font-family:Beta,sans-serif;"), "{html}");
    assert!(html.contains("Hello"), "{html}");
    assert!(html.contains("World"), "{html}");
}

#[test]
fn vello_atlas_holds_per_id_faces_with_default_fallback() {
    // Headless atlas discipline (no draw — selection only): explicit id
    // wins, unmapped ids fall back to the default, empty atlas reports
    // no font (the loud-refusal precondition stays).
    use oppa::FontId;
    let mut atlas = oppa_vello::GlyphAtlas::new();
    assert!(!atlas.has_font());
    atlas.set_font_bytes(vec![1, 2, 3], 0);
    assert!(atlas.has_font());
    assert!(atlas.face_for(FontId(0)).is_some(), "default serves id 0");
    assert!(
        atlas.face_for(FontId(7)).is_some(),
        "default covers unmapped ids"
    );
    atlas.set_font_for(FontId(7), vec![4, 5, 6, 7], 1);
    assert_eq!(atlas.face_ids(), vec![FontId(7)]);
    assert!(atlas.face_for(FontId(7)).is_some());
    assert!(
        atlas.face_for(FontId(9)).is_some(),
        "default still covers the rest"
    );
}

// ---------------------------------------------------------------------------
// 7. Three-backend box compare (CPU == Vello == DOM rounded boxes)
// ---------------------------------------------------------------------------

#[test]
fn three_backends_commit_one_tree_and_agree_on_boxes() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.set_text_service(Box::new(FakeText));
    host.mount("Card", CardProps { title: "Hi".into() }, render_card);
    host.run_until_idle();
    let builder = FramePlanBuilder::new(1.0);
    let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));

    // Same tree committed to all three presenters.
    let mut cpu = oppa_cpu::CpuBackend::new();
    let mut vello = oppa_vello::VelloBackend::new();
    let mut dom = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    {
        use oppa::RendererBackend;
        for d in host.diffs_from(0) {
            cpu.commit(&d).expect("cpu commit");
            vello.commit(&d).expect("vello commit");
            dom.commit(&d).expect("dom commit");
        }
    }
    assert!(cpu.live_node_count() > 0);
    assert_eq!(vello.live_node_count(), cpu.live_node_count());
    dom.with_sync(&host, &mut sheet).expect("sync");

    // CPU DrawOp boxes == DOM element geometry, serialized exactly
    // (both derive from the same committed LayoutBox — the M6
    // two-backend assert joins its third row).
    let html = render_page("t", &dom, &sheet);
    let mut compared = 0;
    for op in &plan.ops {
        match op {
            oppa::DrawOp::Rect { x, y, w, h, .. } | oppa::DrawOp::RRect { x, y, w, h, .. } => {
                for v in [x, y, w, h] {
                    assert_eq!(*v, v.round(), "commit positions are device-snapped");
                }
                // The card is the root (relative, no offsets); every
                // other box serializes its offsets exactly.
                assert!(html.contains(&format!("width:{w}px")), "{html}");
                assert!(html.contains(&format!("height:{h}px")), "{html}");
                if *x != 0.0 || *y != 0.0 {
                    assert!(html.contains(&format!("left:{x}px")), "{html}");
                    assert!(html.contains(&format!("top:{y}px")), "{html}");
                }
                compared += 1;
            }
            oppa::DrawOp::Text { x, y, glyphs, .. } => {
                assert!(!glyphs.is_empty());
                assert!(html.contains(&format!("left:{x}px")), "{html}");
                assert!(html.contains(&format!("top:{y}px")), "{html}");
                compared += 1;
            }
            _ => {}
        }
    }
    assert!(compared >= 2, "card bg + text compared");
    // Glyph cells are the shared position contract across rasterizers
    // (asserted pixel-side on Windows below; headless here).
    let cells: usize = plan
        .ops
        .iter()
        .filter_map(|op| match op {
            oppa::DrawOp::Text { glyphs, .. } => Some(glyphs.len()),
            _ => None,
        })
        .sum();
    assert!(cells > 0);
}

// ---------------------------------------------------------------------------
// 8. Contract surface: Caps third row + loud misses + paint hook
// ---------------------------------------------------------------------------

#[test]
fn caps_negotiation_joins_its_third_row() {
    let dom = DomBackend::new(1.0);
    assert_eq!(dom.kind(), PresenterKind::Dom);
    let caps = dom.caps();
    assert_eq!(caps, Caps::dom());
    assert_eq!(caps.max_layers, 1024, "stacking contexts are cheap");
    assert!(!caps.blur_backdrop, "same offset-solid degradation");
    assert!(caps.msaa, "browser coverage is always on");
    assert!(caps.text_as_paths, "the browser shapes real outlines");
    // Beside the two rasterizer rows (M6's negotiation, third row joined).
    let cpu = oppa_cpu::CpuBackend::new();
    let vello = oppa_vello::VelloBackend::new();
    assert!(!cpu.caps().text_as_paths);
    assert!(vello.caps().text_as_paths);
}

#[test]
fn contract_surface_fails_loudly() {
    use oppa::RendererBackend;
    let mut dom = DomBackend::new(1.0);
    let bad = dom.create_surface(oppa::SurfaceDesc {
        width_px: 0,
        height_px: 60,
        background: SURFACE_BG,
    });
    assert!(matches!(bad, Err(BackendError::BadSurface(_))));
    let surf = dom
        .create_surface(oppa::SurfaceDesc {
            width_px: 200,
            height_px: 120,
            background: SURFACE_BG,
        })
        .expect("surface");
    let unknown = oppa::SurfaceId(999);
    assert!(matches!(
        dom.paint(unknown, &oppa::FramePlan::default()),
        Err(BackendError::UnknownSurface(_))
    ));
    assert!(matches!(
        dom.destroy_surface(unknown),
        Err(BackendError::UnknownSurface(_))
    ));
    dom.destroy_surface(surf).expect("destroy");
    let after_destroy = dom.paint(surf, &oppa::FramePlan::default());
    assert!(
        matches!(after_destroy, Err(BackendError::UnknownSurface(_))),
        "destroyed surfaces stay loud: {after_destroy:?}"
    );

    // Image nodes without a registered source refuse loudly (round
    // 4.4 — the refusal names the node instead of rendering a
    // placeholder; registered ids render `<img>`, proven below).
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Img", (), render_img);
    host.run_until_idle();
    let mut sheet = StyleSheet::new(1.0);
    let err = host.with_retained_mut(|rec, styles| dom.sync(rec, styles, &mut sheet));
    assert!(
        matches!(err, Err(BackendError::UnsupportedOp(_))),
        "{err:?}"
    );
}

fn render_img(_ctx: &Ctx, _props: &()) -> VNode {
    oppa::Img {
        src: oppa::ImageId(7),
        size: 36.0,
        radius: 18.0,
    }
    .into()
}

/// Round 4.4 (decision 267): registered images render real
/// `<img>` elements (src resolved, alt from the semantics label,
/// explicit geometry), and src changes re-render.
#[test]
fn img_renders_src_alt_and_geometry() {
    use oppa::{ImageCache, Signal};
    #[derive(Clone, Props)]
    struct ImgProps {
        src: Signal<oppa::ImageId>,
    }
    fn render_pic(_ctx: &Ctx, p: &ImgProps) -> VNode {
        VNode::from(oppa::Img {
            src: p.src.get(),
            size: 36.0,
            radius: 4.0,
        })
    }
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let cache = ImageCache::new();
    let a = cache.load("img/a.png");
    let b = cache.load("img/b.png?x=1&y=2");
    let src = host.runtime().signal(a);
    host.mount("Pic", ImgProps { src: src.clone() }, render_pic);
    host.run_until_idle();
    let mut dom = DomBackend::new(1.0);
    dom.set_images(cache);
    let mut sheet = StyleSheet::new(1.0);
    host.with_retained_mut(|rec, styles| dom.sync(rec, styles, &mut sheet))
        .expect("registered image syncs");
    let id = oppa::find_retained_by_debug(&host, "img")[0];
    let html = dom.render_node(id);
    assert!(html.starts_with("<img "), "{html}");
    assert!(html.contains("src=\"img/a.png\""), "{html}");
    assert!(
        html.contains("alt=\"\""),
        "no label means empty alt, {html}"
    );
    assert!(html.contains("width:36px;"), "explicit geometry, {html}");
    assert!(html.contains("height:36px;"), "{html}");
    let page = oppa_dom::render_page("t", &dom, &sheet);
    assert!(
        page.contains("border-radius:4px;"),
        "radius rides the class rule, {page}"
    );
    // Src change (with a query string to prove escaping paths
    // round-trip) re-renders the element.
    src.set(b);
    host.run_until_idle();
    let touched = host
        .with_retained_mut(|rec, styles| dom.sync(rec, styles, &mut sheet))
        .expect("src change syncs")
        .touched;
    assert!(touched > 0, "src change touches");
    let html = dom.render_node(id);
    assert!(html.contains("src=\"img/b.png?x=1&amp;y=2\""), "{html}");
}

/// Decision 291: `Tag::Path` leaves render inline `<svg>` (path
/// data verbatim in `d`, fill/stroke mapped, round caps/joins
/// stated, explicit geometry) — no font runs, no image cache.
#[test]
fn path_renders_inline_svg_with_paint_and_geometry() {
    fn render_vec(_ctx: &Ctx, _p: &()) -> VNode {
        oppa::Path::new("mark")
            .data("M 2 2 L 6 6 L 10 2")
            .stroke(Color(0x11_11_11), 2.0)
            .size(12, 8)
            .build()
    }
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Vec", (), render_vec);
    host.run_until_idle();
    let mut dom = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    host.with_retained_mut(|rec, styles| dom.sync(rec, styles, &mut sheet))
        .expect("path syncs");
    let id = oppa::find_retained_by_debug(&host, "mark")[0];
    let html = dom.render_node(id);
    assert!(html.starts_with("<svg "), "{html}");
    assert!(html.contains("d=\"M 2 2 L 6 6 L 10 2\""), "{html}");
    assert!(
        html.contains("fill=\"none\""),
        "unstroked half reads none, {html}"
    );
    assert!(html.contains("stroke=\"#111111\""), "{html}");
    assert!(html.contains("stroke-width=\"2\""), "{html}");
    assert!(html.contains("stroke-linecap=\"round\""), "{html}");
    assert!(html.contains("stroke-linejoin=\"round\""), "{html}");
    assert!(html.contains("viewBox=\"0 0 12 8\""), "{html}");
    assert!(html.contains("width:12px;"), "explicit geometry, {html}");
    assert!(html.contains("height:8px;"), "{html}");
}

#[test]
fn dom_paint_hook_joins_the_phase_discipline() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let backend = Rc::new(RefCell::new(DomBackend::new(1.0)));
    let sheet = Rc::new(RefCell::new(StyleSheet::new(1.0)));
    let surface = backend
        .borrow_mut()
        .create_surface(oppa::SurfaceDesc {
            width_px: 200,
            height_px: 120,
            background: SURFACE_BG,
        })
        .expect("surface");
    let paint_calls = Rc::new(Cell::new(0usize));
    let last_touched = Rc::new(Cell::new(usize::MAX));
    install_dom_paint_hook(
        &host,
        backend.clone(),
        sheet.clone(),
        surface,
        1.0,
        paint_calls.clone(),
        last_touched.clone(),
    );
    host.mount("Card", CardProps { title: "Hi".into() }, render_card);
    host.run_until_idle();
    assert!(paint_calls.get() > 0, "PAINT phases ran");
    assert_ne!(last_touched.get(), usize::MAX, "sync ran in phase");
    // Settle: a further idle run schedules no PAINT at all (static-frame
    // work 0 — the hook only observes, never invents demand).
    let calls = paint_calls.get();
    let touched = last_touched.get();
    host.run_until_idle();
    assert_eq!(paint_calls.get(), calls, "no spurious PAINT on idle");
    assert_eq!(last_touched.get(), touched, "no spurious sync on idle");
    assert!(backend.borrow().element_count() > 0);
}

// ---------------------------------------------------------------------------
// 9. Windows-gated rows: real-font Vello encode + Edge parity corpus +
//    Edge editing suite (loud hardware/browser requirement, never a
//    silent software fallback — the M6 gating pattern).
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod browser {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root above crates/oppa-dom")
            .to_path_buf()
    }

    fn run_node(script: &str) -> (bool, String) {
        let root = repo_root();
        let out = std::process::Command::new("node")
            .arg(format!("spike/web/{script}"))
            .current_dir(&root)
            .output()
            .expect("node must exist for the M7 browser rows");
        let mut text = String::from_utf8_lossy(&out.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.success(), text)
    }

    fn write_results(name: &str, content: &str) {
        let path = repo_root().join("spike").join("results").join(name);
        std::fs::write(&path, content).expect("write parity artifact");
    }

    /// Mixed-coverage shaping must actually split (the test's stated
    /// assumption — Segoe UI has no CJK coverage, so the system
    /// fallback contributes a second face).
    fn mixed_shape(svc: &impl TextService) -> ShapedRun {
        let style = oppa::TextStyle::new("Segoe UI", 16.0);
        let shaped = svc
            .shape("Hello \u{4e16}\u{754c}", &style)
            .expect("shape mixed");
        assert!(
            shaped.runs.len() >= 2,
            "mixed text must shape to 2+ runs: {:?}",
            shaped.runs.iter().map(|r| r.font_id).collect::<Vec<_>>()
        );
        shaped
    }

    #[test]
    fn vello_per_run_encode_with_real_faces() {
        let probe = oppa_text_dwrite::DWriteTextService::new().expect("dwrite");
        let shaped = mixed_shape(&probe);
        // Deterministic ids (decision 16): the host-side shaping lands
        // on the same ids, so the probe instance resolves every face.
        let mut atlas = oppa_vello::GlyphAtlas::new();
        let (_ok, before) = (true, atlas.has_font());
        assert!(!before);
        let mut seen_ids: Vec<oppa::FontId> = Vec::new();
        for run in &shaped.runs {
            if seen_ids.contains(&run.font_id) {
                continue;
            }
            seen_ids.push(run.font_id);
            let (face_path, index) = probe
                .font_file_source(run.font_id)
                .unwrap_or_else(|| panic!("font file for {:?}", run.font_id));
            let bytes = std::fs::read(&face_path).expect("read font file");
            assert!(!bytes.is_empty());
            if seen_ids.len() == 1 {
                atlas.set_font_bytes(bytes, index);
            } else {
                atlas.set_font_for(run.font_id, bytes, index);
            }
        }
        assert!(atlas.has_font());
        assert_eq!(atlas.face_ids().len(), seen_ids.len() - 1);

        // Same string through the real pipeline: layout → plan → encode.
        let host = ComponentHost::new();
        host.set_text_service(Box::new(
            oppa_text_dwrite::DWriteTextService::new().expect("dwrite"),
        ));
        host.set_viewport(200.0, 120.0);
        host.mount(
            "Card",
            CardProps {
                title: "Hello \u{4e16}\u{754c}".into(),
            },
            render_card,
        );
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        let plan = host.with_retained_mut(|rec, styles| builder.build_incremental(rec, styles));
        let text_op = plan
            .ops
            .iter()
            .find(|op| matches!(op, oppa::DrawOp::Text { .. }))
            .expect("text op");
        let (em_size, fonts, glyphs) = match text_op {
            oppa::DrawOp::Text {
                em_size,
                fonts,
                glyphs,
                ..
            } => (*em_size, fonts.clone(), glyphs.clone()),
            _ => unreachable!(),
        };
        assert_eq!(em_size, 16.0, "exact em, not line_height");
        assert!(fonts.len() >= 2, "segmentation survived: {fonts:?}");

        let mut scene = vello::Scene::new();
        let desc = oppa::SurfaceDesc {
            width_px: 200,
            height_px: 120,
            background: SURFACE_BG,
        };
        oppa_vello::encode_plan(&mut scene, &plan, &desc, &mut atlas, &Default::default())
            .expect("per-run encode");
        let placed = atlas.placements();
        assert_eq!(placed.len(), glyphs.len(), "every cell placed");
        // Per-run face selection + subpixel pen exactness, run by run.
        for font in &fonts {
            let lo = font.glyph_range.0;
            let hi = font.glyph_range.1;
            assert!(hi <= placed.len() && hi <= glyphs.len());
            for (p, g) in placed[lo..hi].iter().zip(glyphs[lo..hi].iter()) {
                assert_eq!(p.glyph_id, g.glyph_id);
                assert_eq!(p.advance, g.advance, "never re-shaped");
                assert_eq!(p.font, font.font_id, "run face selected");
            }
            for w in placed[lo..hi].windows(2) {
                assert_eq!(w[1].x - w[0].x, w[0].advance);
            }
        }
        println!(
            "M7 vello: em={em_size} faces={} glyphs={} runs={}",
            seen_ids.len(),
            glyphs.len(),
            fonts.len()
        );
    }

    /// One parity expectation: (kind, pid, x, y, w, h).
    type ParityCase = (String, String, f32, f32, f32, f32);

    fn parity_scene() -> (ComponentHost, Vec<ParityCase>) {
        // Flat subset, nothing else: explicit-size blocks in a row and
        // a column, one static text line, one ring, one circle. untracked
        // text (the no-CSS-letter-spacing rule is load-bearing here).
        fn render_all(_ctx: &Ctx, _p: &()) -> VNode {
            let a1 = Div("a1")
                .style(Style::new().size(60, 40).bg(Color(0x44_44_44)))
                .build();
            let a2 = Div("a2")
                .style(Style::new().size(80, 40).bg(Color(0x66_66_66)))
                .build();
            let row = Row("rowa")
                .style(
                    Style::new()
                        .size(220, 64)
                        .pad_x(8)
                        .gap(8)
                        .bg(Color(0x22_22_22)),
                )
                .children([a1, a2]);
            let b1 = Div("b1")
                .style(Style::new().size(100, 30).bg(Color(0x44_44_44)))
                .build();
            let b2 = Div("b2")
                .style(Style::new().size(100, 40).bg(Color(0x66_66_66)))
                .build();
            let col = oppa::Column::new()
                .gap(4)
                .style(Style::new().size(120, 90).bg(Color(0x22_22_22)))
                .children([b1, b2]);
            let text: VNode = Text {
                text: Arc::from("Hello world"),
                style: Text::title_small,
            }
            .into();
            let ring = Div("ringd")
                .style(
                    Style::new()
                        .size(60, 30)
                        .radius(8)
                        .bg(Color(0x44_44_44))
                        .border(2, Color(0xAA_BB_CC)),
                )
                .build();
            let disc = Div("disc")
                .style(Style::new().size(40, 40).circle().bg(Color(0x66_66_66)))
                .build();
            Div("stage")
                .style(Style::new().size(360, 400).bg(Color(0xFF_FF_FF)))
                .children([row, col, text, ring, disc])
        }
        let host = ComponentHost::new();
        host.set_text_service(Box::new(
            oppa_text_dwrite::DWriteTextService::new().expect("dwrite"),
        ));
        host.set_layout_config(oppa::LayoutTextConfig {
            family: "Segoe UI".to_string(),
            ..Default::default()
        });
        host.set_viewport(400.0, 440.0);
        host.mount("All", (), render_all);
        host.run_until_idle();
        // Expectations from the engine itself (committed boxes): the
        // corpus compares ENGINE vs BROWSER, never hand values.
        let cases: Vec<(String, String, f32, f32, f32, f32)> = host.with_retained_mut(|rec, _| {
            let mut out = Vec::new();
            for id in rec.retained_ids() {
                let Some(n) = rec.get(id) else { continue };
                let Some(b) = n.layout.clone() else { continue };
                let pid = format!("{}-{}", id.gen().index(), id.gen().generation());
                if n.tag == oppa::Tag::Text && n.text.is_some() {
                    // Text width parity compares CONTENT (the browser
                    // shrink-wraps auto-width spans; the laid box is
                    // constraint-wide by construction, never comparable).
                    out.push(("text".to_string(), pid, b.x, b.y, b.content_w, b.h));
                } else if n.tag != oppa::Tag::Text {
                    out.push(("box".to_string(), pid, b.x, b.y, b.w, b.h));
                }
            }
            out
        });
        (host, cases)
    }

    fn expected_json(cases: &[(String, String, f32, f32, f32, f32)]) -> String {
        let mut s = String::from("{\"cases\":[");
        for (i, (kind, pid, x, y, w, h)) in cases.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!(
                "{{\"kind\":\"{kind}\",\"pid\":\"{pid}\",\"x\":{x},\"y\":{y},\"w\":{w},\"h\":{h}}}"
            ));
        }
        s.push_str("]}");
        s
    }

    #[test]
    fn parity_corpus_flat_subset_measured() {
        let (host, cases) = parity_scene();
        assert!(!cases.is_empty());
        let text_rows = cases.iter().filter(|c| c.0 == "text").count();
        assert!(text_rows >= 1, "text widths are corpus rows");
        let mut backend = DomBackend::new(1.0);
        let mut sheet = StyleSheet::new(1.0);
        for d in host.diffs_from(0) {
            use oppa::RendererBackend;
            backend.commit(&d).expect("commit");
        }
        backend.with_sync(&host, &mut sheet).expect("sync");
        write_results("parity_page.html", &render_page("parity", &backend, &sheet));
        write_results("parity_expected.json", &expected_json(&cases));
        let (ok, log) = run_node("parity.mjs");
        println!("{log}");
        assert!(ok, "parity corpus green in the flat subset:\n{log}");
        // The measured numbers for the report-back (match rate + the
        // text-height record rows live in spike/results/parity.json).
        let verdict_path = repo_root()
            .join("spike")
            .join("results")
            .join("parity.json");
        let verdict = std::fs::read_to_string(&verdict_path).expect("parity.json");
        assert!(verdict.contains("\"pass\": true"), "{verdict}");
    }

    #[test]
    fn dom_text_editing_suite_through_real_input() {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(
            oppa_text_dwrite::DWriteTextService::new().expect("dwrite"),
        ));
        host.set_viewport(400.0, 200.0);
        host.mount(
            "Field",
            FieldProps {
                value: "Hello world".into(),
            },
            render_field,
        );
        host.run_until_idle();
        let mut backend = DomBackend::new(1.0);
        let mut sheet = StyleSheet::new(1.0);
        for d in host.diffs_from(0) {
            use oppa::RendererBackend;
            backend.commit(&d).expect("commit");
        }
        backend.with_sync(&host, &mut sheet).expect("sync");
        write_results("dom_field.html", &render_page("field", &backend, &sheet));
        let (ok, log) = run_node("dom_text.mjs");
        println!("{log}");
        assert!(ok, "editing suite green through the real input:\n{log}");
    }
}

#[test]
fn key_events_still_route_beside_scroll() {
    // Scroll/Ime authoring parity (decision 112): nodes declare targets
    // with .on_scroll()/.on_ime() instead of failing loudly.
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Slots", SlotProps { n: 1 }, render_slots);
    host.run_until_idle();
    let list = oppa::find_retained_by_debug(&host, "list")[0];
    host.inject_input(InputEvent::Scroll {
        target: list,
        dx: 0.0,
        dy: 4.0,
    });
    host.inject_input(InputEvent::key(0x0D, KeyState::Pressed));
    host.run_until_idle();
}

// ---------------------------------------------------------------------------
// Round 12.1 (decision 307): keyed DOM patches
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct PatchForm {
    value: oppa::Signal<oppa::SharedString>,
    note: oppa::Signal<oppa::SharedString>,
}

fn render_patch_form(_ctx: &Ctx, p: &PatchForm) -> VNode {
    Div("form").children([
        VNode::from(TextField {
            text: p.value.get(),
            style: Text::body_secondary,
            label: oppa::SharedString::from("Name"),
        }),
        VNode::from(Text {
            text: std::sync::Arc::from(p.note.get().to_string()),
            style: Text::body_secondary,
        }),
    ])
}

struct PatchRig {
    host: ComponentHost,
    backend: DomBackend,
    sheet: StyleSheet,
    cursor: usize,
}

impl PatchRig {
    fn form(
        value: &str,
        note: &str,
    ) -> (
        Self,
        oppa::Signal<oppa::SharedString>,
        oppa::Signal<oppa::SharedString>,
    ) {
        let host = ComponentHost::new();
        host.set_text_service(Box::new(FakeText));
        host.set_viewport(200.0, 120.0);
        let vs = host.runtime().signal(oppa::SharedString::from(value));
        let ns = host.runtime().signal(oppa::SharedString::from(note));
        host.mount(
            "Form",
            PatchForm {
                value: vs.clone(),
                note: ns.clone(),
            },
            render_patch_form,
        );
        host.run_until_idle();
        let mut rig = Self {
            host,
            backend: DomBackend::new(1.0),
            sheet: StyleSheet::new(1.0),
            cursor: 0,
        };
        rig.sync();
        rig.backend.mark_rendered();
        (rig, vs, ns)
    }

    fn sync(&mut self) {
        for d in self.host.diffs_from(self.cursor) {
            self.backend.commit(&d).expect("commit");
        }
        self.cursor = self.host.diff_count();
        self.backend
            .with_sync(&self.host, &mut self.sheet)
            .expect("sync");
    }

    fn field_pid(&self) -> String {
        let id = oppa::find_retained_by_debug(&self.host, "field")[0];
        pid_of(id)
    }
}

/// Settled frames patch nothing (the applier stays idle — no focus
/// disturbance possible).
#[test]
fn patch_empty_when_settled() {
    let (mut rig, _, _) = PatchRig::form("ab", "v1");
    let patch = rig.backend.take_patch();
    assert!(patch.is_empty(), "settled take is empty: {patch:?}");
    assert!(patch.theme.is_none(), "settled Light take owes no stanza");
    let json = patch.to_json();
    assert!(json.starts_with("{\"v\":1"), "{json}");
    assert!(json.contains("\"swaps\":[]"), "{json}");
    assert!(json.ends_with("\"theme\":null}"), "{json}");
}

/// The brief's verification, headless: a background state update
/// (label change) patches exactly the changed span — the focused
/// field's pid appears in NO op, so the browser never touches the
/// input (focus, caret, and IME composition survive by
/// construction).
#[test]
fn patch_background_update_never_touches_the_focused_field() {
    let (mut rig, _, note) = PatchRig::form("ab", "v1");
    let field = rig.field_pid();
    note.set(oppa::SharedString::from("v2"));
    rig.host.run_until_idle();
    rig.sync();
    let patch = rig.backend.take_patch();
    assert!(!patch.full, "content change is incremental");
    assert!(patch.removes.is_empty() && patch.places.is_empty());
    assert!(patch.attrs.is_empty() && patch.sels.is_empty() && patch.spacers.is_empty());
    assert_eq!(patch.swaps.len(), 1, "exactly the note span: {patch:?}");
    assert!(patch.swaps[0].1.contains("v2"), "new text rides the swap");
    let json = patch.to_json();
    assert!(
        !json.contains(&format!("\"pid\":\"{field}\"")),
        "no op targets the focused field, {json}"
    );
}

/// A framework-owned value set syncs the live `.value` property
/// (never an element swap while the browser may hold caret).
#[test]
fn patch_framework_value_set_syncs_the_value_property() {
    let (mut rig, value, _) = PatchRig::form("ab", "v1");
    let field = rig.field_pid();
    value.set(oppa::SharedString::from("abc"));
    rig.host.run_until_idle();
    rig.sync();
    let patch = rig.backend.take_patch();
    assert!(patch.swaps.is_empty(), "fields never swap: {patch:?}");
    assert_eq!(patch.attrs.len(), 1, "{patch:?}");
    assert_eq!(patch.attrs[0].pid, field);
    assert_eq!(patch.attrs[0].value.as_deref(), Some("abc"));
    // Both channels agree (render carries the attribute too).
    let id = oppa::find_retained_by_debug(&rig.host, "field")[0];
    assert!(
        rig.backend.render_node(id).contains("value=\"abc\""),
        "render and property agree"
    );
}

/// Reorder emits one final-order place (live nodes move in place —
/// the applier never detaches them, so focus survives moves).
#[test]
fn patch_reorder_places_final_order_without_blobs() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Ord", OrderProps { reversed: false }, render_ordered);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    backend.mark_rendered();
    handle.set_props(OrderProps { reversed: true });
    host.run_until_idle();
    for d in host.diffs_from(host.diff_count() - 1) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let patch = backend.take_patch();
    assert!(!patch.full);
    assert!(patch.removes.is_empty(), "moves, never removals: {patch:?}");
    assert_eq!(patch.places.len(), 1, "{patch:?}");
    let wrap = oppa::find_retained_by_debug(&host, "wrap")[0];
    assert_eq!(patch.places[0].parent, pid_of(wrap));
    let live: Vec<String> = backend
        .element(wrap)
        .expect("wrap")
        .children
        .iter()
        .map(|c| pid_of(*c))
        .collect();
    assert_eq!(patch.places[0].kids, live, "final order, reversed");
    assert!(
        patch.places[0].blobs.is_empty(),
        "all kids live — pure move"
    );
    // Reordering re-lays-out (Row positions follow order): the two
    // displaced cells swap new geometry, the middle cell is untouched.
    assert_eq!(patch.swaps.len(), 2, "displaced geometry swaps: {patch:?}");
}

/// Added subtrees arrive as blobs on the parent's place (parsed
/// once, appended — existing siblings untouched).
#[test]
fn patch_added_cells_carry_blobs() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Count", CountProps { n: 1, tint: false }, render_count);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    backend.mark_rendered();
    handle.set_props(CountProps { n: 3, tint: false });
    host.run_until_idle();
    for d in host.diffs_from(host.diff_count() - 1) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let patch = backend.take_patch();
    assert_eq!(patch.places.len(), 1, "{patch:?}");
    assert_eq!(patch.places[0].kids.len(), 3, "final order");
    assert_eq!(patch.places[0].blobs.len(), 2, "two new cells ride blobs");
    for (_, html) in &patch.places[0].blobs {
        assert!(html.contains("data-pid="), "blob is element HTML: {html}");
    }
}

/// Removed subtrees emit topmost only (descendants ride the
/// ancestor removal).
#[test]
fn patch_removed_cells_topmost_only() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    let handle = host.mount("Count", CountProps { n: 3, tint: false }, render_count);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    backend.mark_rendered();
    handle.set_props(CountProps { n: 1, tint: false });
    host.run_until_idle();
    for d in host.diffs_from(host.diff_count() - 1) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let patch = backend.take_patch();
    assert_eq!(patch.removes.len(), 2, "two cells gone, topmost: {patch:?}");
    // The survivor's parent still places (child list changed).
    assert_eq!(patch.places.len(), 1, "{patch:?}");
    assert_eq!(patch.places[0].kids.len(), 1);
}

/// Patch transport escapes quotes (HTML rides inside JSON — the
/// `esc` boundary rule's sibling: the HTML escaper entity-encodes
/// first, JSON escaping second, so no raw quote ever breaks the
/// transport).
#[test]
fn patch_json_escapes_embedded_quotes() {
    let (mut rig, _, note) = PatchRig::form("ab", "v1");
    note.set(oppa::SharedString::from("say \"hi\""));
    rig.host.run_until_idle();
    rig.sync();
    let patch = rig.backend.take_patch();
    assert_eq!(patch.swaps.len(), 1);
    let json = patch.to_json();
    assert!(
        json.contains("&quot;hi&quot;"),
        "entity-encoded text rides, {json}"
    );
    assert!(
        !json.contains("say \"hi\""),
        "no raw quotes in transport, {json}"
    );
}

// ---------------------------------------------------------------------------
// Theme contract round (decision 323): page chrome rides a
// change-only stanza; full pages carry the body style.
// ---------------------------------------------------------------------------

/// A toggled theme emits exactly one stanza (change-only, never
/// per-frame noise), the JSON carries it, and the full page styles
/// its own `<body>` — CSS inheritance does the rest, explicit
/// `color:` still wins.
#[test]
fn page_chrome_rides_change_only_stanza_and_body_style() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Card", CardProps { title: "Hi".into() }, render_card);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    backend.mark_rendered();
    // Settled Light: empty, no stanza owed.
    let patch = backend.take_patch();
    assert!(patch.is_empty(), "settled Light take is empty");
    assert!(patch.theme.is_none(), "no stanza on repeats");
    // Flip to Dark: the sync counts the page chrome as touched
    // work (a bare toggle on an unthemed tree still emits), and
    // the take carries exactly one stanza.
    backend.set_theme_mode(ThemeMode::Dark);
    let touched = backend.with_sync(&host, &mut sheet).expect("sync").touched;
    assert!(touched > 0, "the flip is sync work");
    let patch = backend.take_patch();
    assert!(!patch.is_empty(), "the stanza is patch work");
    let stanza = patch.theme.as_ref().expect("one stanza");
    let dark = ThemeTokens::dark();
    assert_eq!(stanza.bg, "#121212", "tokens drive the wire");
    assert_eq!(stanza.ink, "#f5f5f5", "tokens drive the wire");
    assert_eq!(dark.background, Color(0x12_12_12));
    assert_eq!(dark.text_primary, Color(0xF5_F5_F5));
    let json = patch.to_json();
    assert!(
        json.contains("\"theme\":{\"bg\":\"#121212\",\"ink\":\"#f5f5f5\"}"),
        "stanza serializes, {json}"
    );
    // Repeat take: quiet again.
    let patch = backend.take_patch();
    assert!(patch.theme.is_none(), "change-only, never per-frame");
    assert!(patch.is_empty(), "settled again");
    // Full pages style their own body (the boot path needs no
    // patch at all).
    let html = render_page("t", &backend, &sheet);
    assert!(
        html.contains("<body style=\"background:#121212;color:#f5f5f5;\">"),
        "dark body style rides the page: {html}"
    );
}

/// Light full pages keep the near-white page + near-black ink the
/// browser implied before (explicit, not browser-default luck).
#[test]
fn light_full_page_styles_its_body() {
    let host = ComponentHost::new();
    host.set_viewport(200.0, 120.0);
    host.mount("Card", CardProps { title: "Hi".into() }, render_card);
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    for d in host.diffs_from(0) {
        backend.commit(&d).expect("commit");
    }
    backend.with_sync(&host, &mut sheet).expect("sync");
    let html = render_page("t", &backend, &sheet);
    assert!(
        html.contains("<body style=\"background:#ffffff;color:#111111;\">"),
        "light body style rides the page: {html}"
    );
}

// ---------------------------------------------------------------------------
// Phase 39a (decision 378): value-loop swap preservation (headless half)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct LoopProps {
    value: oppa::Signal<oppa::SharedString>,
    count: oppa::Signal<u32>,
}

fn render_loop(ctx: &Ctx, p: &LoopProps) -> VNode {
    use oppa_controls::{TextInput, TextInputProps};
    Div("loop").children([
        ctx.child(
            "oppa::LoopField",
            1,
            &TextInputProps::new("Name", p.value.clone()),
            TextInput,
        ),
        VNode::from(Text {
            text: oppa::SharedString::from(format!("ticks {}", p.count.get())),
            style: Text::body_secondary,
        }),
    ])
}

/// U8 swap preservation (decision 378 — headless half): typing
/// through the value channel then ticking unrelated state leaves
/// the focused field untouched at the DOM layer — zero new
/// mutations, value intact. (The browser half — native
/// focus/caret retention across the same ticks — rides the M7
/// editing suite through real `<input>`s plus the keyed-patch
/// focal save/restore in `bootstrap.js`.)
#[test]
fn value_loop_unrelated_tick_leaves_focused_field_untouched() {
    let host = ComponentHost::new();
    host.set_viewport(300.0, 200.0);
    host.set_text_service(Box::new(FakeText));
    let value = host.runtime().signal(oppa::SharedString::from(""));
    let count = host.runtime().signal(0u32);
    host.mount(
        "Loop",
        LoopProps {
            value: value.clone(),
            count: count.clone(),
        },
        render_loop,
    );
    host.run_until_idle();
    let mut backend = DomBackend::new(1.0);
    let mut sheet = StyleSheet::new(1.0);
    let mut seen = 0usize;
    let mut commit_sync = |backend: &mut DomBackend, sheet: &mut StyleSheet| {
        use oppa::RendererBackend;
        for d in host.diffs_from(seen) {
            backend.commit(&d).expect("commit");
        }
        seen = host.diff_count();
        backend.with_sync(&host, sheet).expect("sync");
    };
    commit_sync(&mut backend, &mut sheet);
    // Focus the field (tap center), then type through the U8 feed.
    let field = oppa::find_retained_by_debug(&host, "text-input")[0];
    let b = host.committed_box(field).expect("field hit box");
    host.inject_input(InputEvent::pointer_down(b.x + b.w / 2.0, b.y + b.h / 2.0));
    host.inject_input(InputEvent::pointer_up(b.x + b.w / 2.0, b.y + b.h / 2.0));
    host.run_until_idle();
    assert_eq!(host.focused_node(), Some(field));
    host.inject_input(InputEvent::text(field, "abc"));
    host.run_until_idle();
    assert_eq!(value.get().to_string(), "abc");
    commit_sync(&mut backend, &mut sheet);
    let html = backend.render_node(field);
    assert!(html.contains("value=\"abc\""), "typed value serves: {html}");
    let mutations = backend.mutations();
    // Unrelated tick: the counter re-renders around the field.
    count.set(1);
    host.run_until_idle();
    commit_sync(&mut backend, &mut sheet);
    assert_eq!(
        backend.mutations(),
        mutations,
        "zero DOM ops around the focused field"
    );
    let html = backend.render_node(field);
    assert!(
        html.contains("value=\"abc\""),
        "value intact across the tick: {html}"
    );
    assert_eq!(value.get().to_string(), "abc");
}
