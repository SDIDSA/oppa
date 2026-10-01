//! M3 acceptance: the framework-owned layout engine (locked #6).
//!
//! Unit tests on hand-built retained trees (builders → `Reconciler`, no
//! renderer): flex rows/columns, block-lite, absolute positioning,
//! padding/gap, fill_width/fill_height, margins, content_size. Inline text through a counting
//! fake `TextService` (single-line measurement, BiDi visual ordering on the
//! Latin+Arabic+digits corpus shape, wrap without re-shape, optional-v1
//! ellipsis). Dirty-subtree discipline (style-only changes skip the run;
//! layout-affecting and text changes re-measure exactly the dirty subtree).
//! One-frame-delay contract (settled reads observe previous-frame values,
//! then re-run). Shared DPR rounding (commit positions only).

#![allow(non_snake_case)]

use oppa::{
    find_retained_by_debug, AlignItems, Column, ComponentHost, Ctx, Div, FlexWrap, FontWeight,
    Interner, JustifyContent, LayoutLedger, LayoutStats, LayoutTextConfig, Portal, Px, Reconciler,
    Row, Runtime, ScrollArea, Style, Text, TextClass, TextField, VNode,
};
use oppa::{Cluster, FontId, FontMetrics, ShapedGlyph, ShapedRun, TextError, TextRun, TextService};
use oppa_macros::Props;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Counting fake TextService (uniform advance, byte-exact clusters, optional
// rtl byte ranges). Advances scale with the em size like a real backend.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FakeText {
    calls: Rc<Cell<usize>>,
    rtl_ranges: Vec<(usize, usize)>,
    /// Weights observed per `shape` call (decision 239 proof surface;
    /// existing tests ignore it — additive only).
    weights: Rc<RefCell<Vec<FontWeight>>>,
}

impl FakeText {
    fn new() -> (Self, Rc<Cell<usize>>) {
        let calls = Rc::new(Cell::new(0));
        (
            Self {
                calls: calls.clone(),
                rtl_ranges: Vec::new(),
                weights: Rc::new(RefCell::new(Vec::new())),
            },
            calls,
        )
    }

    fn bidi() -> (Self, Rc<Cell<usize>>) {
        let (mut fake, calls) = Self::new();
        // Mirrors the backend's single-tail-run resolution: the whole
        // Arabic+space+digits tail reads rtl (the engine's level-2 island
        // recovers the digit order).
        fake.rtl_ranges.push((4, 12));
        (fake, calls)
    }
}

impl TextService for FakeText {
    fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
        Vec::new()
    }

    fn shape(&self, text: &str, style: &oppa::TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        self.calls.set(self.calls.get() + 1);
        self.weights.borrow_mut().push(style.weight);
        let em = style.font_size_px * style.device_pixel_ratio;
        let adv = em * 0.625;
        let metrics = FontMetrics {
            ascent: em * 0.75,
            descent: em * 0.25,
            line_gap: em * 0.125,
        };
        let mut glyphs = Vec::new();
        let mut clusters = Vec::new();
        let mut runs = Vec::new();
        let mut run_start = 0usize;
        let mut run_glyph = 0usize;
        let mut run_rtl = false;
        let mut first = true;
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        for (k, (i, ch)) in chars.iter().enumerate() {
            let len = ch.len_utf8();
            let rtl = self.rtl_ranges.iter().any(|(s, e)| *i >= *s && *i < *e);
            if first || rtl != run_rtl {
                if !first {
                    runs.push(TextRun {
                        byte_range: (run_start, *i),
                        glyph_range: (run_glyph, glyphs.len()),
                        rtl: run_rtl,
                        script: 0,
                        font_id: FontId(0),
                        font_metrics: metrics,
                    });
                }
                run_start = *i;
                run_glyph = glyphs.len();
                run_rtl = rtl;
                first = false;
            }
            glyphs.push(ShapedGlyph {
                glyph_id: k as u32,
                x_advance: adv,
                x_offset: 0.0,
                y_offset: 0.0,
            });
            clusters.push(Cluster {
                byte_range: (*i, *i + len),
                glyph_range: (glyphs.len() - 1, glyphs.len()),
            });
            if k + 1 == chars.len() {
                runs.push(TextRun {
                    byte_range: (run_start, *i + len),
                    glyph_range: (run_glyph, glyphs.len()),
                    rtl: run_rtl,
                    script: 0,
                    font_id: FontId(0),
                    font_metrics: metrics,
                });
            }
        }
        let total_advance = adv * glyphs.len() as f32;
        Ok(ShapedRun {
            glyphs,
            runs,
            clusters,
            total_advance,
            text_len_bytes: text.len(),
        })
    }
}

// ---------------------------------------------------------------------------
// Harness: hand-built tree → Reconciler → LayoutLedger run
// ---------------------------------------------------------------------------

struct Rig {
    rt: Runtime,
    rec: Reconciler,
    styles: Interner<Style>,
    ledger: LayoutLedger,
    fake: FakeText,
    calls: Rc<Cell<usize>>,
}

impl Rig {
    fn new(fake: FakeText, calls: Rc<Cell<usize>>) -> Self {
        let rt = Runtime::new();
        Self {
            ledger: LayoutLedger::new(&rt),
            rt,
            rec: Reconciler::new(),
            styles: Interner::new(),
            fake,
            calls,
        }
    }

    fn layout(&mut self, vnode: VNode) -> LayoutStats {
        self.rec.reconcile(&self.rt, &mut self.styles, false, vnode);
        self.ledger
            .run(&mut self.rec, &self.styles, Some(&self.fake), 800.0, 600.0)
    }

    fn relayout(&mut self, vnode: VNode) -> LayoutStats {
        self.layout(vnode)
    }

    fn only(&self, debug: &str) -> oppa::NodeId {
        let mut ids = find_ids(&self.rec, debug);
        assert_eq!(ids.len(), 1, "expected one {debug} node, got {}", ids.len());
        ids.pop().unwrap()
    }

    fn shape_calls(&self) -> usize {
        self.calls.get()
    }
}

fn find_ids(rec: &Reconciler, debug: &str) -> Vec<oppa::NodeId> {
    rec.find_by_debug(debug)
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

// ---------------------------------------------------------------------------
// Flex rows/columns, block-lite, padding/gap, fill_width
// ---------------------------------------------------------------------------

#[test]
fn flex_row_positions_sizes_gaps_padding() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(Row("r").style(Style::new().pad_x(10).gap(4)).children([
        Div("a").style(Style::new().size(100, 20)).build(),
        Div("b").style(Style::new().size(50, 30)).build(),
    ]));
    let stats = rig.layout(tree);
    assert!(!stats.empty);
    let a = rig.only("a");
    let b = rig.only("b");
    let r = rig.only("r");
    let ba = LayoutLedger::committed(&rig.rec, a).expect("box a");
    let bb = LayoutLedger::committed(&rig.rec, b).expect("box b");
    let br = LayoutLedger::committed(&rig.rec, r).expect("box r");
    // content_x = 0 + pad 10; a spans 10..110; gap 4; b at 114.
    assert!(approx(ba.x, 10.0), "a.x = {}", ba.x);
    assert!(approx(ba.w, 100.0));
    assert!(approx(bb.x, 114.0), "b.x = {}", bb.x);
    assert!(approx(bb.w, 50.0));
    assert!(approx(br.h, 30.0), "row h = max child, got {}", br.h);
    assert!(approx(br.w, 800.0), "row fills block width, got {}", br.w);
    assert!(
        approx(br.content_w, 154.0),
        "content 100+4+50, got {}",
        br.content_w
    );
}

#[test]
fn column_fill_width_and_intrinsic() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(Column::new().gap(2).children([
        Div("fixed").style(Style::new().size(60, 10)).build(),
        Div("fill").style(Style::new().h(12).fill_width()).build(),
    ]));
    rig.layout(tree);
    let f = rig.only("fixed");
    let w = rig.only("fill");
    let bf = LayoutLedger::committed(&rig.rec, f).expect("fixed");
    let bw = LayoutLedger::committed(&rig.rec, w).expect("fill");
    assert!(approx(bf.w, 60.0), "intrinsic kept, got {}", bf.w);
    assert!(
        approx(bw.w, 800.0),
        "fill spans content width, got {}",
        bw.w
    );
    assert!(approx(bw.y, bf.y + 10.0 + 2.0), "gap 2, got {}", bw.y);
}

#[test]
fn block_lite_div_children_fill_width() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(Div("kid").style(Style::new().h(10)).build());
    rig.layout(tree);
    let k = rig.only("kid");
    let bk = LayoutLedger::committed(&rig.rec, k).expect("kid");
    assert!(approx(bk.w, 800.0), "block child full width, got {}", bk.w);
}

#[test]
fn row_fill_children_split_remainder() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Row with explicit width 200, pad 10, one fixed 60 + two fills:
    // remainder = 200 - 20 - 60 - gaps(2×4) = 112 → 56 each.
    let tree: VNode = Div("root").child(
        Row("r")
            .style(Style::new().size(200, 40).pad_x(10).gap(4))
            .children([
                Div("fixed").style(Style::new().size(60, 10)).build(),
                Div("f1").style(Style::new().h(10).fill_width()).build(),
                Div("f2").style(Style::new().h(10).fill_width()).build(),
            ]),
    );
    let stats = rig.layout(tree);
    assert_eq!(stats.layout_passes, 2, "fill redistribution is pass 2");
    let b1 = LayoutLedger::committed(&rig.rec, rig.only("f1")).expect("f1");
    let b2 = LayoutLedger::committed(&rig.rec, rig.only("f2")).expect("f2");
    assert!(approx(b1.w, 56.0), "f1 share, got {}", b1.w);
    assert!(approx(b2.w, 56.0), "f2 share, got {}", b2.w);
    assert!(approx(b2.x, b1.x + 56.0 + 4.0), "gap between fills");
}

// ---------------------------------------------------------------------------
// Absolute positioning (.x / .absolute_y in scope, both §4 examples)
// ---------------------------------------------------------------------------

#[test]
fn absolute_x_offsets_within_parent_flow_kept() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // §4.1 knob shape: track 44×24, knob 18×18 at x=23.
    let tree: VNode = Div("root").child(
        Div("track")
            .style(Style::new().size(44, 24))
            .child(Div("knob").style(Style::new().size(18, 18).x(23)).build()),
    );
    rig.layout(tree);
    let track = LayoutLedger::committed(&rig.rec, rig.only("track")).expect("track");
    let knob = LayoutLedger::committed(&rig.rec, rig.only("knob")).expect("knob");
    assert!(
        approx(knob.x, track.x + 23.0),
        "knob.x = track.x + 23, got {}",
        knob.x
    );
    assert!(approx(knob.y, track.y), "y stays in flow, got {}", knob.y);
}

#[test]
fn absolute_y_pins_and_leaves_auto_height() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root")
        .child(Div("list").child(Div("slot").style(Style::new().h(24).absolute_y(48)).build()));
    rig.layout(tree);
    let list = LayoutLedger::committed(&rig.rec, rig.only("list")).expect("list");
    let slot = LayoutLedger::committed(&rig.rec, rig.only("slot")).expect("slot");
    assert!(approx(slot.y, list.y + 48.0), "slot pinned, got {}", slot.y);
    assert!(
        approx(list.h, 0.0),
        "absolute child grows nothing, got {}",
        list.h
    );
}

// ---------------------------------------------------------------------------
// ScrollArea + content_size (fixed-height virtualization geometry)
// ---------------------------------------------------------------------------

#[test]
fn scrollarea_content_size_and_slot_geometry() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(
        ScrollArea("list")
            .style(Style::new().size(300, 200).content_size(1000))
            .children((0..3).map(|i| {
                Row("slot")
                    .key(i)
                    .style(Style::new().h(40).absolute_y(i as f32 * 40.0))
                    .build()
            })),
    );
    rig.layout(tree);
    let area = LayoutLedger::committed(&rig.rec, rig.only("list")).expect("area");
    assert!(approx(area.w, 300.0));
    assert!(approx(area.h, 200.0));
    assert!(
        approx(area.content_h, 1000.0),
        "spacer extent, got {}",
        area.content_h
    );
    let slots = find_ids(&rig.rec, "slot");
    assert_eq!(slots.len(), 3);
    let mut ys: Vec<f32> = slots
        .iter()
        .map(|id| LayoutLedger::committed(&rig.rec, *id).expect("slot").y)
        .collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // Slot geometry is exactly the absolute_y offsets (§4.2 slot math).
    assert!(approx(ys[0], area.y + 0.0));
    assert!(approx(ys[1], area.y + 40.0));
    assert!(approx(ys[2], area.y + 80.0));
}

// ---------------------------------------------------------------------------
// Inline text: single-line measurement, hints, empty text
// ---------------------------------------------------------------------------

#[test]
fn text_single_line_measures_through_service() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Bare text leaf in a Row (intrinsic): default 14px → 8.75px/char.
    let tree: VNode = Div("root").child(Row("r").child(VNode::Text(Arc::from("hello"))));
    let stats = rig.layout(tree);
    assert_eq!(stats.nodes_shaped, 1);
    assert_eq!(rig.shape_calls(), 1);
    let id = rig.only("text");
    let b = LayoutLedger::committed(&rig.rec, id).expect("text box");
    assert!(approx(b.w, 43.75), "w = {}", b.w);
    assert!(approx(b.h, 14.0), "h = ascent+descent = {}", b.h);
    assert_eq!(b.lines.len(), 1);
    assert_eq!(b.lines[0].runs.len(), 5);
}

#[test]
fn text_hint_selects_size() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // title_small 16px → 10px/char; body_secondary 14px → 8.75px/char.
    // Row wrapper: leaves size intrinsically (a Div would full-width them).
    let tree: VNode = Div("root").child(Row("r").children([
        VNode::from(Text {
            text: Arc::from("ab"),
            style: Text::title_small,
        }),
        VNode::from(Text {
            text: Arc::from("ab"),
            style: Text::body_secondary,
        }),
    ]));
    rig.layout(tree);
    let ids = find_ids(&rig.rec, "text");
    // Two wrappers + two leaves share the debug label; leaves hold lines.
    let mut widths: Vec<f32> = ids
        .iter()
        .filter_map(|id| LayoutLedger::committed(&rig.rec, *id))
        .filter(|b| !b.lines.is_empty())
        .map(|b| b.w)
        .collect();
    widths.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(widths.len(), 2);
    assert!(approx(widths[0], 17.5), "body 2×8.75, got {}", widths[0]);
    assert!(approx(widths[1], 20.0), "title 2×10, got {}", widths[1]);
}

#[test]
fn empty_text_is_zero_without_shape_call() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(VNode::Text(Arc::from("")));
    let stats = rig.layout(tree);
    assert_eq!(stats.nodes_shaped, 0);
    assert_eq!(rig.shape_calls(), 0);
    let b = LayoutLedger::committed(&rig.rec, rig.only("text")).expect("box");
    // Block leaf spans the content width; the measurement is in content.
    assert!(approx(b.content_w, 0.0) && approx(b.content_h, 0.0));
    assert!(b.lines.is_empty());
}

#[test]
fn empty_field_measures_one_space() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Decision 189: the empty field payload measures as one space
    // (title_small 16px fake → 10px advance; line box 12+4=16) so
    // the box never collapses. Static empty text above stays zero.
    // Row wrapper: leaves size intrinsically (a Div would full-width
    // them — same rule as text_single_line_measures_through_service).
    let tree: VNode = Div("root").child(
        Row("r").child(
            TextField {
                text: Arc::from(""),
                style: Text::title_small,
                label: Arc::from("Name"),
            }
            .into(),
        ),
    );
    rig.layout(tree);
    let elem = LayoutLedger::committed(&rig.rec, rig.only("field")).expect("field box");
    assert!(approx(elem.w, 10.0), "w = {}", elem.w);
    assert!(approx(elem.h, 16.0), "h = {}", elem.h);
    let leaf = LayoutLedger::committed(&rig.rec, rig.only("text")).expect("payload box");
    assert!(!leaf.lines.is_empty(), "space lines committed");
    assert!(approx(leaf.w, 10.0), "leaf w = {}", leaf.w);
}

// ---------------------------------------------------------------------------
// BiDi visual ordering on the Latin+Arabic+digits corpus shape
// ---------------------------------------------------------------------------

#[test]
fn bidi_visual_order_collapses_source_order_divergence() {
    let (fake, calls) = FakeText::bidi();
    let mut rig = Rig::new(fake, calls);
    // Corpus string (escapes per decision 45): "abc " + U+0645 U+0631 +
    // " 123". Fake: 10px/char at 16px em... default leaf is 14px →
    // 8.75px/char; force title size via hint for round numbers.
    let text = "abc \u{645}\u{631} 123";
    assert_eq!(text.len(), 12);
    let tree: VNode = Div("root").child(VNode::from(Text {
        text: Arc::from(text),
        style: Text::title_small,
    }));
    rig.layout(tree);
    let ids = find_ids(&rig.rec, "text");
    let line = ids
        .iter()
        .filter_map(|id| LayoutLedger::committed(&rig.rec, *id))
        .find(|b| !b.lines.is_empty())
        .expect("leaf box")
        .lines
        .into_iter()
        .next()
        .expect("one line");
    // 10 chars × 10px: width preserved by the reorder.
    assert!(approx(line.width, 100.0), "width = {}", line.width);
    // Visual order: LTR head, EN digit island (LTR unit), mirrored RTL
    // tail (space + Arabic pair). The fake mirrors the backend's
    // single-tail-run resolution, so the island path is exercised.
    let bytes: Vec<(usize, usize)> = line.clusters.iter().map(|c| c.byte_range).collect();
    assert_eq!(
        bytes,
        vec![
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 4),
            (9, 10),
            (10, 11),
            (11, 12),
            (8, 9),
            (6, 8),
            (4, 6)
        ],
        "island + mirrored tail"
    );
    // Forward-affinity carets: Arabic start (byte 4) sits at the mirrored
    // tail's right edge (100), not the source-order 40; the digit island
    // starts LTR at 40; the trailing caret follows the last logical char
    // (island '3' → 70), not the line width.
    assert!(
        approx(line.caret_x(4), 100.0),
        "visual caret = {}",
        line.caret_x(4)
    );
    assert!(approx(line.caret_x(8), 80.0));
    assert!(approx(line.caret_x(9), 40.0));
    assert!(
        approx(line.caret_x(12), 70.0),
        "trailing = {}",
        line.caret_x(12)
    );
}

// ---------------------------------------------------------------------------
// Wrap re-flows over cached advances (the measured round-trip count)
// ---------------------------------------------------------------------------

#[test]
fn wrap_reflows_without_reshape() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Title 16px → 10px/char; explicit w=25 → lines of 2 → 4 lines.
    let tree = || {
        Div("root").child(
            Div("t")
                .style(Style::new().size(25, 100))
                .child(VNode::Text(Arc::from("abcdefgh"))),
        )
    };
    let s1 = rig.layout(tree());
    assert_eq!(s1.nodes_shaped, 1, "one shape for the text");
    // Lines live on the text leaf (containers carry none).
    let leaf = rig.only("text");
    let l1 = LayoutLedger::committed(&rig.rec, leaf).expect("leaf");
    assert_eq!(l1.lines.len(), 4, "lines of 2 at w=25");
    // Re-run with w=30 (layout-affecting style change, same text): re-wrap
    // to lines of 3 with zero new shapes.
    let tree2 = Div("root").child(
        Div("t")
            .style(Style::new().size(30, 100))
            .child(VNode::Text(Arc::from("abcdefgh"))),
    );
    let s2 = rig.relayout(tree2);
    assert_eq!(rig.shape_calls(), 1, "re-wrap shapes nothing");
    assert_eq!(s2.nodes_shaped, 0);
    let l2 = LayoutLedger::committed(&rig.rec, leaf).expect("leaf2");
    assert_eq!(
        l2.lines.len(),
        3,
        "lines of 3 at w=30, got {}",
        l2.lines.len()
    );
}

// ---------------------------------------------------------------------------
// Ellipsis (optional-v1, behind the config flag)
// ---------------------------------------------------------------------------

#[test]
fn ellipsis_truncates_single_line_when_enabled() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    rig.ledger.set_config(LayoutTextConfig {
        ellipsis: true,
        ..Default::default()
    });
    let tree: VNode = Div("root").child(
        Div("t")
            .style(Style::new().size(25, 100))
            .child(VNode::Text(Arc::from("abcdefgh"))),
    );
    rig.layout(tree);
    // Default 14px → 8.75px/char; budget 25-8.75=16.25 → 1 cluster + marker.
    let b = LayoutLedger::committed(&rig.rec, rig.only("text")).expect("leaf");
    assert_eq!(b.lines.len(), 1, "ellipsis forces single-line");
    assert!(
        b.lines[0].clusters.iter().any(|c| c.ellipsis),
        "marker present"
    );
    // One text shape + one amortized ellipsis-glyph shape.
    assert_eq!(rig.shape_calls(), 2, "shapes = {}", rig.shape_calls());
}

// ---------------------------------------------------------------------------
// Dirty-subtree discipline
// ---------------------------------------------------------------------------

#[test]
fn dirty_discipline_style_only_skips_text_remeasure() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree = |bg: u32| {
        Div("root").children([
            Div("a")
                .style(Style::new().size(100, 20).bg(oppa::Color(bg)))
                .child(VNode::Text(Arc::from("alpha"))),
            Div("b")
                .style(Style::new().size(100, 20))
                .child(VNode::Text(Arc::from("beta"))),
        ])
    };
    let s1 = rig.layout(tree(1));
    assert_eq!(s1.nodes_shaped, 2);
    // Style-only change (bg is not in the layout-affecting subset):
    // the run is skipped entirely — zero shapes, empty stats.
    let s2 = rig.relayout(tree(2));
    assert!(s2.empty, "no LAYOUT dirt → run skipped");
    assert_eq!(s2.nodes_shaped, 0);
    assert_eq!(rig.shape_calls(), 2);
    // Layout-affecting change with unchanged text: re-layout, still zero
    // shapes (measure cache keyed by bytes+style, not flags).
    let tree3 = Div("root").children([
        Div("a")
            .style(Style::new().size(120, 20).bg(oppa::Color(2)))
            .child(VNode::Text(Arc::from("alpha"))),
        Div("b")
            .style(Style::new().size(100, 20))
            .child(VNode::Text(Arc::from("beta"))),
    ]);
    let s3 = rig.relayout(tree3);
    assert!(!s3.empty);
    assert_eq!(s3.nodes_shaped, 0, "cache absorbs flag dirt");
    assert_eq!(rig.shape_calls(), 2);
    // Text change (TEXT dirt, no LAYOUT flag — decision 69): exactly the
    // dirty subtree re-measures — one shape.
    let tree4 = Div("root").children([
        Div("a")
            .style(Style::new().size(120, 20).bg(oppa::Color(2)))
            .child(VNode::Text(Arc::from("alpha!"))),
        Div("b")
            .style(Style::new().size(100, 20))
            .child(VNode::Text(Arc::from("beta"))),
    ]);
    let s4 = rig.relayout(tree4);
    assert_eq!(s4.nodes_shaped, 1, "exactly the changed leaf");
    assert_eq!(rig.shape_calls(), 3);
}

// ---------------------------------------------------------------------------
// One-frame-delay contract (host-level: LAYOUT-phase publish, EFFECTS read)
// ---------------------------------------------------------------------------

#[derive(Clone, Props)]
struct PanelProps {
    text: String,
}

fn Panel(_ctx: &Ctx, props: &PanelProps) -> VNode {
    Div("panel").child(VNode::Text(Arc::from(props.text.as_str())))
}

#[test]
fn settled_reads_observe_previous_frame_then_rerun() {
    let (fake, calls) = FakeText::new();
    let host = ComponentHost::new();
    host.set_text_service(Box::new(fake));
    let handle = host.mount(
        "Panel",
        PanelProps {
            text: "hello".to_string(),
        },
        Panel,
    );
    let text_id = find_retained_by_debug(&host, "text")[0];
    // Reader effect: settled content width into a signal (write-only inside).
    let rt = host.runtime();
    let seen = rt.signal(-1.0f32);
    let count = Rc::new(Cell::new(0u32));
    let seen2 = seen.clone();
    let host2 = host.clone();
    let count2 = count.clone();
    let _effect = rt.effect(move || {
        let w = host2
            .settled_box(text_id)
            .map(|b| b.content_w)
            .unwrap_or(-1.0);
        seen2.set(w);
        count2.set(count2.get() + 1);
    });
    // Synchronous creation run observes pre-layout state.
    assert_eq!(seen.get(), -1.0);
    assert_eq!(count.get(), 1);
    // Frame 1: LAYOUT publishes (gen 0→1); the reader still holds the old.
    assert!(rt.run_once());
    assert_eq!(seen.get(), -1.0, "previous-frame value during frame 1");
    assert_eq!(count.get(), 1);
    // Frame 2: the reader re-runs in EFFECTS with the settled box.
    assert!(rt.run_once());
    assert_eq!(count.get(), 2);
    assert!(
        approx(seen.get(), 43.75),
        "5 chars × 8.75, got {}",
        seen.get()
    );
    assert!(!rt.run_once(), "idle: no spurious frames");
    // Update the text (TEXT dirt, no LAYOUT flag): exactly one re-measure,
    // and the delay repeats — frame 1 still shows the old width.
    let calls_before = calls.get();
    handle.set_props(PanelProps {
        text: "hello!!".to_string(),
    });
    assert!(rt.run_once());
    assert_eq!(calls.get(), calls_before + 1, "exactly the dirty leaf");
    assert!(
        approx(seen.get(), 43.75),
        "old width during frame 1, got {}",
        seen.get()
    );
    assert!(rt.run_once());
    assert!(
        approx(seen.get(), 61.25),
        "7 chars × 8.75, got {}",
        seen.get()
    );
    assert!(!rt.run_once(), "idle again");
}

// ---------------------------------------------------------------------------
// Shared DPR rounding (commit positions only)
// ---------------------------------------------------------------------------

#[test]
fn dpr_rounding_applies_to_positions_not_extents() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    rig.ledger.set_config(LayoutTextConfig {
        device_pixel_ratio: 2.0,
        ..Default::default()
    });
    // x=3.3 CSS px → 6.6 device → snaps to 6.5; w=10.33 → 20.66 kept.
    let tree: VNode = Div("root").child(Div("k").style(Style::new().size(10.33, 5).x(3.3)).build());
    rig.layout(tree);
    let b = LayoutLedger::committed(&rig.rec, rig.only("k")).expect("k");
    assert!(approx(b.x, 6.5), "snapped commit position, got {}", b.x);
    assert!(
        (b.w - 20.66).abs() < 1e-2,
        "subpixel extent kept, got {}",
        b.w
    );
}

// ---------------------------------------------------------------------------
// Decision 237: vertical padding + flex alignment (exact coordinates)
// ---------------------------------------------------------------------------

#[test]
fn pad_y_div_auto_height_includes_padding() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Div pad_y 6, kid h=20 fills width: outer h = 20 + 12 = 32.
    let tree: VNode = Div("root").child(
        Div("pad")
            .style(Style::new().pad_y(6))
            .child(Div("kid").style(Style::new().h(20)).build()),
    );
    rig.layout(tree);
    let pad = LayoutLedger::committed(&rig.rec, rig.only("pad")).expect("pad");
    let kid = LayoutLedger::committed(&rig.rec, rig.only("kid")).expect("kid");
    assert!(approx(pad.x, 0.0), "pad.x = {}", pad.x);
    assert!(approx(pad.y, 0.0), "pad.y = {}", pad.y);
    assert!(approx(pad.w, 800.0), "pad fills block width, got {}", pad.w);
    assert!(approx(pad.h, 32.0), "20 + 2*6, got {}", pad.h);
    assert!(approx(pad.content_h, 20.0), "content = {}", pad.content_h);
    assert!(approx(kid.x, 0.0), "kid.x = {}", kid.x);
    assert!(approx(kid.y, 6.0), "kid.y = pad top, got {}", kid.y);
    assert!(approx(kid.w, 800.0), "kid fills, got {}", kid.w);
    assert!(approx(kid.h, 20.0), "kid.h = {}", kid.h);
}

#[test]
fn pad_y_row_auto_height_offsets_children() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Row auto height (fills block width 800), pad_y 4, gap 2:
    // max child 16 → h = 16 + 8 = 24; children at y = 4.
    let tree: VNode = Div("root").child(Row("r").style(Style::new().pad_y(4).gap(2)).children([
        Div("a").style(Style::new().size(60, 10)).build(),
        Div("b").style(Style::new().size(40, 16)).build(),
    ]));
    rig.layout(tree);
    let r = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("r");
    let a = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let b = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    assert!(approx(r.w, 800.0), "row fills, got {}", r.w);
    assert!(approx(r.h, 24.0), "16 + 2*4, got {}", r.h);
    assert!(approx(r.content_h, 16.0), "content = {}", r.content_h);
    assert!(approx(a.x, 0.0), "a.x = {}", a.x);
    assert!(approx(a.y, 4.0), "a.y = {}", a.y);
    assert!(approx(a.w, 60.0) && approx(a.h, 10.0));
    assert!(approx(b.x, 62.0), "60 + gap 2, got {}", b.x);
    assert!(approx(b.y, 4.0), "b.y = {}", b.y);
    assert!(approx(b.w, 40.0) && approx(b.h, 16.0));
}

#[test]
fn pad_y_column_auto_height_stacks_from_content() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Column fills block width, pad_y 5, gap 2: 10 + 2 + 8 = 20 content,
    // h = 20 + 10 = 30. First at y=5, second at 5+10+2=17.
    let tree: VNode =
        Div("root").child(Column::new().style(Style::new().pad_y(5).gap(2)).children([
            Div("a").style(Style::new().size(60, 10)).build(),
            Div("b").style(Style::new().size(40, 8)).build(),
        ]));
    rig.layout(tree);
    // Column::new() uses debug "column"; distinct kids carry the asserts.
    let a = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let b = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    assert!(approx(a.x, 0.0), "a.x = {}", a.x);
    assert!(approx(a.y, 5.0), "a.y = {}", a.y);
    assert!(approx(a.w, 60.0) && approx(a.h, 10.0));
    assert!(approx(b.x, 0.0), "b.x = {}", b.x);
    assert!(approx(b.y, 17.0), "b.y = {}", b.y);
    assert!(approx(b.w, 40.0) && approx(b.h, 8.0));
    let col_id = find_ids(&rig.rec, "column").pop().expect("column");
    let col = LayoutLedger::committed(&rig.rec, col_id).expect("col");
    assert!(approx(col.w, 800.0), "col fills, got {}", col.w);
    assert!(approx(col.h, 30.0), "20 + 2*5, got {}", col.h);
    assert!(approx(col.content_h, 20.0), "content = {}", col.content_h);
}

#[test]
fn row_center_and_space_between_exact() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Row 300x50, pad 10/5, gap base 4, SpaceBetween + Center.
    // content 280x40, extent 40*3+4*2=128, leftover 152 → extra 76/gap.
    // xs: 10, 130, 250. Center ys: h10 → 20, h20 → 15.
    let tree: VNode = Div("root").child(
        Row("r")
            .style(
                Style::new()
                    .size(300, 50)
                    .pad_x(10)
                    .pad_y(5)
                    .gap(4)
                    .align_items(AlignItems::Center)
                    .justify_content(JustifyContent::SpaceBetween),
            )
            .children([
                Div("a").style(Style::new().size(40, 10)).build(),
                Div("b").style(Style::new().size(40, 20)).build(),
                Div("c").style(Style::new().size(40, 10)).build(),
            ]),
    );
    rig.layout(tree);
    let r = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("r");
    let a = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let b = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    let c = LayoutLedger::committed(&rig.rec, rig.only("c")).expect("c");
    assert!(approx(r.w, 300.0) && approx(r.h, 50.0));
    assert!(approx(r.x, 0.0) && approx(r.y, 0.0));
    assert!(approx(a.x, 10.0), "a.x = {}", a.x);
    assert!(approx(a.y, 20.0), "5 + (40-10)/2, got {}", a.y);
    assert!(approx(a.w, 40.0) && approx(a.h, 10.0));
    assert!(approx(b.x, 130.0), "10+40+80, got {}", b.x);
    assert!(approx(b.y, 15.0), "5 + (40-20)/2, got {}", b.y);
    assert!(approx(c.x, 250.0), "130+40+80, got {}", c.x);
    assert!(approx(c.y, 20.0), "c.y = {}", c.y);
}

#[test]
fn column_center_both_axes_exact() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Column 200x120, pad 10/5, gap 4, Center/Center.
    // content 180x110, extent_h 34, leftover 76 → start +38.
    // ys: 43, 67. xs: w60 → 70, w40 → 80.
    let tree: VNode = Div("root").child(
        Column::new()
            .style(
                Style::new()
                    .size(200, 120)
                    .pad_x(10)
                    .pad_y(5)
                    .gap(4)
                    .align_items(AlignItems::Center)
                    .justify_content(JustifyContent::Center),
            )
            .children([
                Div("a").style(Style::new().size(60, 20)).build(),
                Div("b").style(Style::new().size(40, 10)).build(),
            ]),
    );
    rig.layout(tree);
    let a = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let b = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    assert!(approx(a.x, 70.0), "10 + (180-60)/2, got {}", a.x);
    assert!(approx(a.y, 43.0), "5 + 38, got {}", a.y);
    assert!(approx(a.w, 60.0) && approx(a.h, 20.0));
    assert!(approx(b.x, 80.0), "10 + (180-40)/2, got {}", b.x);
    assert!(approx(b.y, 67.0), "43+20+4, got {}", b.y);
    assert!(approx(b.w, 40.0) && approx(b.h, 10.0));
}

#[test]
fn row_stretch_grows_unconstrained_keeps_fixed() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Row 200x60 Stretch: fixed 50x20 stays; w-only 30 grows 0 → 60.
    let stretch_style = Style {
        w: Some(Px::of(30.0)),
        ..Default::default()
    };
    let tree: VNode = Div("root").child(
        Row("r")
            .style(Style::new().size(200, 60).align_items(AlignItems::Stretch))
            .children([
                Div("fixed").style(Style::new().size(50, 20)).build(),
                Div("grow").style(stretch_style).build(),
            ]),
    );
    rig.layout(tree);
    let r = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("r");
    let f = LayoutLedger::committed(&rig.rec, rig.only("fixed")).expect("fixed");
    let g = LayoutLedger::committed(&rig.rec, rig.only("grow")).expect("grow");
    assert!(approx(r.w, 200.0) && approx(r.h, 60.0));
    assert!(approx(f.x, 0.0) && approx(f.y, 0.0));
    assert!(approx(f.w, 50.0) && approx(f.h, 20.0), "fixed keeps h");
    assert!(approx(g.x, 50.0), "g.x = {}", g.x);
    assert!(approx(g.y, 0.0), "g.y = {}", g.y);
    assert!(approx(g.w, 30.0), "w keeps, got {}", g.w);
    assert!(approx(g.h, 60.0), "h stretches to content 60, got {}", g.h);
}

#[test]
fn column_stretch_fills_unconstrained_width() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Column 120 wide Stretch: fixed 50x20 stays; h-only 25 fills to 120.
    let tree: VNode = Div("root").child(
        Column::new()
            .style(Style::new().size(120, 100).align_items(AlignItems::Stretch))
            .children([
                Div("fixed").style(Style::new().size(50, 20)).build(),
                Div("grow").style(Style::new().h(25)).build(),
            ]),
    );
    rig.layout(tree);
    let f = LayoutLedger::committed(&rig.rec, rig.only("fixed")).expect("fixed");
    let g = LayoutLedger::committed(&rig.rec, rig.only("grow")).expect("grow");
    assert!(approx(f.x, 0.0) && approx(f.y, 0.0));
    assert!(approx(f.w, 50.0) && approx(f.h, 20.0), "fixed keeps w");
    assert!(approx(g.x, 0.0), "g.x = {}", g.x);
    assert!(approx(g.y, 20.0), "g.y = {}", g.y);
    assert!(
        approx(g.w, 120.0),
        "w stretches to content 120, got {}",
        g.w
    );
    assert!(approx(g.h, 25.0), "h keeps, got {}", g.h);
}

#[test]
#[should_panic(expected = "non-finite")]
fn layout_nan_pad_rejects_loudly() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(
        Div("bad")
            .style(Style::new().pad_y(f32::NAN))
            .child(Div("kid").style(Style::new().size(10, 10)).build()),
    );
    rig.layout(tree);
}

// ---------------------------------------------------------------------------
// Decision 239: arbitrary font sizes + weights (exact coordinates)
// ---------------------------------------------------------------------------

#[test]
fn custom_size_scales_geometry_exactly() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Fake advance = em * 0.625, line box = em * (0.75 + 0.25) = em.
    // title_small 16px "ab" → 20 x 16; custom 32px "ab" → 40 x 32.
    let tree: VNode = Div("root").child(Row("r").children([
        VNode::from(Text {
            text: Arc::from("ab"),
            style: Text::title_small,
        }),
        VNode::from(Text::new("ab").size(32).build()),
    ]));
    rig.layout(tree);
    let ids = find_ids(&rig.rec, "text");
    let mut wh: Vec<(f32, f32)> = ids
        .iter()
        .filter_map(|id| LayoutLedger::committed(&rig.rec, *id))
        .filter(|b| !b.lines.is_empty())
        .map(|b| (b.w, b.h))
        .collect();
    wh.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert_eq!(wh.len(), 2);
    assert!(
        approx(wh[0].0, 20.0) && approx(wh[0].1, 16.0),
        "title 2x10, h 16, got {:?}",
        wh[0]
    );
    assert!(
        approx(wh[1].0, 40.0) && approx(wh[1].1, 32.0),
        "custom32 2x20, h 32, got {:?}",
        wh[1]
    );
}

#[test]
fn text_builder_default_matches_body_secondary() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Untouched builder == BodySecondary: body 14px → 8.75px/char.
    let tree: VNode = Div("root").child(Row("r").children([
        VNode::from(Text {
            text: Arc::from("ab"),
            style: Text::body_secondary,
        }),
        VNode::from(Text::new("ab").build()),
    ]));
    rig.layout(tree);
    let ids = find_ids(&rig.rec, "text");
    let mut widths: Vec<f32> = ids
        .iter()
        .filter_map(|id| LayoutLedger::committed(&rig.rec, *id))
        .filter(|b| !b.lines.is_empty())
        .map(|b| b.w)
        .collect();
    widths.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(widths.len(), 2);
    assert!(approx(widths[0], 17.5), "body, got {}", widths[0]);
    assert!(
        approx(widths[1], 17.5),
        "builder default, got {}",
        widths[1]
    );
}

#[test]
fn bold_weight_reaches_the_shaper() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode =
        Div("root").child(Row("r").child(VNode::from(Text::new("ab").size(16).bold().build())));
    rig.layout(tree);
    assert_eq!(*rig.fake.weights.borrow(), vec![FontWeight::BOLD]);
    // Weight carries no geometry in the fake (as in real backends, where
    // only the shaper's advances change): 16px → 20 x 16 like title.
    // (Wrapper + leaf share the "text" label — the leaf holds lines.)
    let leaf = find_ids(&rig.rec, "text")
        .into_iter()
        .filter_map(|id| LayoutLedger::committed(&rig.rec, id))
        .find(|b| !b.lines.is_empty())
        .expect("leaf box");
    assert!(approx(leaf.w, 20.0), "w = {}", leaf.w);
    assert!(approx(leaf.h, 16.0), "h = {}", leaf.h);
}

#[test]
fn weight_change_remasures_not_aliases() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree = |style| {
        Div("root").child(Row("r").child(VNode::from(Text {
            text: Arc::from("alpha"),
            style,
        })))
    };
    let s1 = rig.layout(tree(Text::new("alpha").size(16).bold().build().style));
    assert_eq!(s1.nodes_shaped, 1);
    // Same bytes, same size, different weight: the key misses — exactly
    // one re-shape, never a bold-cache alias for the regular style.
    let s2 = rig.relayout(tree(Text::new("alpha").size(16).build().style));
    assert_eq!(s2.nodes_shaped, 1, "weight flip re-shapes once");
    assert_eq!(rig.shape_calls(), 2);
    assert_eq!(
        *rig.fake.weights.borrow(),
        vec![FontWeight::BOLD, FontWeight::NORMAL]
    );
}

#[test]
#[should_panic(expected = "size_px is 0")]
fn custom_zero_size_rejects_loudly() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(Row("r").child(VNode::from(Text {
        text: Arc::from("hi"),
        style: TextClass::Custom {
            size_px: 0,
            weight: FontWeight::NORMAL,
        },
    })));
    rig.layout(tree);
}

// ---------------------------------------------------------------------------
// Decision 249: fill_height + margins
// ---------------------------------------------------------------------------

#[test]
fn column_fill_height_splits_remaining_height() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Column h=300, gap 10, two fill children (intrinsic 0 — the
    // explicit height belongs on the container, never the fill
    // children, or the share would never size them):
    // remainder = 300 - 0 - 10 = 290 → 145 each.
    let tree: VNode = Div("root").child(
        Column::new()
            .style(Style::new().size(200, 300))
            .gap(10)
            .children([
                Div("f1").style(Style::new().fill_height()).build(),
                Div("f2").style(Style::new().fill_height()).build(),
            ]),
    );
    rig.layout(tree);
    let b1 = LayoutLedger::committed(&rig.rec, rig.only("f1")).expect("f1");
    let b2 = LayoutLedger::committed(&rig.rec, rig.only("f2")).expect("f2");
    assert!(approx(b1.h, 145.0), "f1 share, got {}", b1.h);
    assert!(approx(b2.h, 145.0), "f2 share, got {}", b2.h);
    assert!(approx(b1.y, 0.0), "f1 at top, got {}", b1.y);
    assert!(approx(b2.y, 145.0 + 10.0), "gap 10 after f1, got {}", b2.y);
}

#[test]
fn row_fill_height_cross_axis() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Row h=100, pad_y 10 → content height 80; the fill child
    // (intrinsic 0) grows to exactly the content height at the
    // content top (align defaults to Start).
    let tree: VNode = Div("root").child(
        Row("r")
            .style(Style::new().size(200, 100).pad_y(10))
            .children([Div("f").style(Style::new().fill_height()).build()]),
    );
    rig.layout(tree);
    let bf = LayoutLedger::committed(&rig.rec, rig.only("f")).expect("f");
    assert!(
        approx(bf.h, 80.0),
        "fill spans content height, got {}",
        bf.h
    );
    assert!(approx(bf.y, 10.0), "content top, got {}", bf.y);
    let br = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("r");
    assert!(approx(br.h, 100.0), "row keeps explicit h, got {}", br.h);
}

#[test]
fn margins_offset_child_and_expand_container() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Auto Div + one 96x32 child with margin_x 15 / margin_y 10:
    // the child sits at (15, 10) and the container grows by both
    // margins on each axis (126 x 52). The box rides a Row so it
    // sizes intrinsically — as a direct block child it would fill
    // the root width (block-lite), hiding the extent math.
    let tree: VNode = Div("root").child(
        Row("r").child(
            Div("box").child(
                Div("kid")
                    .style(Style::new().size(96, 32).margin(15, 10))
                    .build(),
            ),
        ),
    );
    rig.layout(tree);
    let bk = LayoutLedger::committed(&rig.rec, rig.only("kid")).expect("kid");
    assert!(approx(bk.x, 15.0), "margin_x offsets x, got {}", bk.x);
    assert!(approx(bk.y, 10.0), "margin_y offsets y, got {}", bk.y);
    let bb = LayoutLedger::committed(&rig.rec, rig.only("box")).expect("box");
    assert!(approx(bb.w, 126.0), "width carries margins, got {}", bb.w);
    assert!(approx(bb.h, 52.0), "height carries margins, got {}", bb.h);
}

// ---------------------------------------------------------------------------
// Round 11.1 (decision 305): asymmetric pads/margins
// ---------------------------------------------------------------------------

#[test]
fn asymmetric_pad_shifts_content_per_side() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Div pad_left 10 / pad_top 4 / pad_right 2 / pad_bottom 6 with a
    // 20-high kid: outer h = 20 + 4 + 6 = 30, kid at (10, 4); the
    // block width fills 800, so the right pad shows in content_w.
    let tree: VNode = Div("root").child(
        Div("pad")
            .style(
                Style::new()
                    .pad_left(10)
                    .pad_top(4)
                    .pad_right(2)
                    .pad_bottom(6),
            )
            .child(Div("kid").style(Style::new().h(20)).build()),
    );
    rig.layout(tree);
    let pad = LayoutLedger::committed(&rig.rec, rig.only("pad")).expect("pad");
    let kid = LayoutLedger::committed(&rig.rec, rig.only("kid")).expect("kid");
    assert!(approx(pad.h, 30.0), "20 + 4 + 6, got {}", pad.h);
    assert!(approx(kid.x, 10.0), "left pad shifts x, got {}", kid.x);
    assert!(approx(kid.y, 4.0), "top pad shifts y, got {}", kid.y);
    assert!(approx(kid.w, 788.0), "800 - 10 - 2, got {}", kid.w);
}

#[test]
fn per_side_fields_refine_symmetric_shorthands() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // pad_x 8 + pad_left 2 → left 2, right 8; margin_y 10 +
    // margin_top 3 on the kid → y 3, container grows 3 + 10.
    let tree: VNode = Div("root").child(
        Row("r").child(
            Div("box").child(
                Div("kid")
                    .style(Style::new().size(96, 32).margin_y(10).margin_top(3))
                    .build(),
            ),
        ),
    );
    rig.layout(tree);
    let bk = LayoutLedger::committed(&rig.rec, rig.only("kid")).expect("kid");
    assert!(approx(bk.y, 3.0), "per-side margin wins, got {}", bk.y);
    let bb = LayoutLedger::committed(&rig.rec, rig.only("box")).expect("box");
    assert!(approx(bb.h, 45.0), "32 + 3 + 10, got {}", bb.h);
    // Pads: pad container with shorthand + override (the kid fills —
    // no explicit width — so the right pad shows in its extent).
    let tree2: VNode = Div("root").child(
        Div("pad")
            .style(Style::new().pad_x(8).pad_left(2))
            .child(Div("kid").style(Style::new().h(10)).build()),
    );
    rig.layout(tree2);
    let kid2 = LayoutLedger::committed(&rig.rec, rig.only("kid")).expect("kid");
    assert!(approx(kid2.x, 2.0), "left override wins, got {}", kid2.x);
    assert!(approx(kid2.w, 790.0), "800 - 2 - 8, got {}", kid2.w);
}

#[test]
#[should_panic(expected = "out of range")]
fn custom_bad_weight_rejects_loudly() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(Row("r").child(VNode::from(Text {
        text: Arc::from("hi"),
        style: TextClass::Custom {
            size_px: 16,
            weight: FontWeight(0),
        },
    })));
    rig.layout(tree);
}

// ---------------------------------------------------------------------------
// Round 1.1 (decision 252): constrained widths wrap text and expand the
// container height; NaN/negative widths fail loudly, never silently.
// ---------------------------------------------------------------------------

#[test]
fn wrapped_text_expands_auto_container_height_with_exact_lines() {
    use oppa::layout::{layout_text, layout_text_with_breaks};
    // Fake metrics mirror FakeText at 14px/dpr1: adv 8.75, line 14 + gap 1.75.
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Auto-height Div, explicit w=25: "abcdefgh" (8 x 8.75 = 70) wraps to
    // lines of 2 -> 4 lines; height expands to 4*14 + 3*1.75 = 61.25.
    // (Width comes from the fixed outer; the auto-height inner proves
    // the intrinsic-height expansion — `Style` has no w-only builder.)
    let tree: VNode = Div("root").child(
        Div("outer").style(Style::new().size(25, 500)).child(
            Div("t")
                .style(Style::new().fill_width())
                .child(VNode::Text(Arc::from("abcdefgh"))),
        ),
    );
    rig.layout(tree);
    let leaf = rig.only("text");
    let lb = LayoutLedger::committed(&rig.rec, leaf).expect("leaf");
    assert_eq!(lb.lines.len(), 4, "lines of 2 at w=25");
    for (i, line) in lb.lines.iter().enumerate() {
        assert!(
            approx(line.y, i as f32 * 15.75),
            "line {i} y = {}, want {}",
            line.y,
            i as f32 * 15.75
        );
        assert!(approx(line.height, 14.0), "line {i} h = {}", line.height);
        assert!(approx(line.width, 17.5), "line {i} w = {}", line.width);
    }
    assert!(approx(lb.h, 61.25), "leaf h expands, got {}", lb.h);
    let t = LayoutLedger::committed(&rig.rec, rig.only("t")).expect("container");
    assert!(approx(t.w, 25.0), "container w, got {}", t.w);
    assert!(
        approx(t.h, 61.25),
        "container h expands to wrapped text, got {}",
        t.h
    );
    assert!(
        approx(t.content_h, 61.25),
        "content_h tracks wrapped text, got {}",
        t.content_h
    );
    // The pure wrap entry points accept the same constraint story:
    // infinite = intrinsic single line; NaN/negative panic loudly.
    let run = oppa::ShapedRun {
        glyphs: vec![ShapedGlyph {
            glyph_id: 0,
            x_advance: 10.0,
            x_offset: 0.0,
            y_offset: 0.0,
        }],
        runs: vec![TextRun {
            byte_range: (0, 1),
            glyph_range: (0, 1),
            rtl: false,
            script: 0,
            font_id: FontId(0),
            font_metrics: FontMetrics {
                ascent: 12.0,
                descent: 4.0,
                line_gap: 2.0,
            },
        }],
        clusters: vec![Cluster {
            byte_range: (0, 1),
            glyph_range: (0, 1),
        }],
        total_advance: 10.0,
        text_len_bytes: 1,
    };
    let one = layout_text(&run, "a", f32::INFINITY, None, 12.0, 4.0, 2.0);
    assert_eq!(one.len(), 1, "infinite avail stays single-line");
    let one_b = layout_text_with_breaks(&run, "a", f32::INFINITY, None, 12.0, 4.0, 2.0, &[]);
    assert_eq!(one_b.len(), 1, "infinite avail stays single-line (breaks)");
}

#[test]
fn row_fill_wrapper_text_wraps_into_share_expanding_row_height() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Outer 200 wide; Row fills it (auto height); fixed 60x10 + fill Div.
    // Share = 200 - 60 = 140 -> 20-char text (8.75/char) wraps 16 + 4;
    // fill h = 2*14 + 1.75 = 29.75; row h expands to it.
    let tree: VNode = Div("root").child(
        Div("outer").style(Style::new().size(200, 500)).child(
            Row("r").style(Style::new().fill_width()).children([
                Div("fixed").style(Style::new().size(60, 10)).build(),
                Div("f1")
                    .style(Style::new().fill_width())
                    .child(VNode::Text(Arc::from("abcdefghijklmnopqrst"))),
            ]),
        ),
    );
    rig.layout(tree);
    let leaf = rig.only("text");
    let lb = LayoutLedger::committed(&rig.rec, leaf).expect("leaf");
    assert_eq!(
        lb.lines.len(),
        2,
        "fill share wraps 16+4, got {}",
        lb.lines.len()
    );
    assert!(
        approx(lb.lines[0].width, 140.0),
        "line0 w = {}",
        lb.lines[0].width
    );
    assert!(
        approx(lb.lines[1].width, 35.0),
        "line1 w = {}",
        lb.lines[1].width
    );
    assert!(approx(lb.lines[1].y, 15.75), "line1 y = {}", lb.lines[1].y);
    let f1 = LayoutLedger::committed(&rig.rec, rig.only("f1")).expect("f1");
    assert!(approx(f1.w, 140.0), "fill share w, got {}", f1.w);
    assert!(approx(f1.h, 29.75), "fill h expands, got {}", f1.h);
    let r = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("row");
    assert!(
        approx(r.h, 29.75),
        "row h expands to wrapped fill child, got {}",
        r.h
    );
}

#[test]
#[should_panic(expected = "NaN")]
fn layout_text_nan_avail_panics_loudly() {
    use oppa::layout::layout_text;
    let run = oppa::ShapedRun {
        glyphs: vec![ShapedGlyph {
            glyph_id: 0,
            x_advance: 10.0,
            x_offset: 0.0,
            y_offset: 0.0,
        }],
        runs: vec![TextRun {
            byte_range: (0, 1),
            glyph_range: (0, 1),
            rtl: false,
            script: 0,
            font_id: FontId(0),
            font_metrics: FontMetrics {
                ascent: 12.0,
                descent: 4.0,
                line_gap: 2.0,
            },
        }],
        clusters: vec![Cluster {
            byte_range: (0, 1),
            glyph_range: (0, 1),
        }],
        total_advance: 10.0,
        text_len_bytes: 1,
    };
    let _ = layout_text(&run, "a", f32::NAN, None, 12.0, 4.0, 2.0);
}

#[test]
#[should_panic(expected = "negative")]
fn layout_text_negative_avail_panics_loudly() {
    use oppa::layout::layout_text_with_breaks;
    let run = oppa::ShapedRun {
        glyphs: vec![ShapedGlyph {
            glyph_id: 0,
            x_advance: 10.0,
            x_offset: 0.0,
            y_offset: 0.0,
        }],
        runs: vec![TextRun {
            byte_range: (0, 1),
            glyph_range: (0, 1),
            rtl: false,
            script: 0,
            font_id: FontId(0),
            font_metrics: FontMetrics {
                ascent: 12.0,
                descent: 4.0,
                line_gap: 2.0,
            },
        }],
        clusters: vec![Cluster {
            byte_range: (0, 1),
            glyph_range: (0, 1),
        }],
        total_advance: 10.0,
        text_len_bytes: 1,
    };
    let _ = layout_text_with_breaks(&run, "a", -5.0, None, 12.0, 4.0, 2.0, &[]);
}

#[test]
#[should_panic(expected = "NaN")]
fn ledger_nan_viewport_panics_loudly() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    rig.rec.reconcile(
        &rig.rt,
        &mut rig.styles,
        false,
        Div("root").child(Div("a").style(Style::new().size(10, 10)).build()),
    );
    let fake2 = rig.fake.clone();
    rig.ledger
        .run(&mut rig.rec, &rig.styles, Some(&fake2), f32::NAN, 600.0);
}

// ---------------------------------------------------------------------------
// Round 1.2 (decision 253): Row flex-wrap line-breaking with exact
// container bounds; NoWrap default preserved; non-Row Wrap loud.
// ---------------------------------------------------------------------------

#[test]
fn row_wrap_breaks_into_lines_with_exact_geometry() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Outer 100 wide; Row fills it (auto height) with gap 4 and Wrap.
    // Three 60-wide children: each breaks alone (60+4+60 > 100).
    // Lines h 10/12/8; extent_h = 10+12+8+2*4 = 38.
    let tree: VNode = Div("root").child(
        Div("outer").style(Style::new().size(100, 400)).child(
            Row("r")
                .style(Style::new().fill_width().gap(4).flex_wrap(FlexWrap::Wrap))
                .children([
                    Div("a").style(Style::new().size(60, 10)).build(),
                    Div("b").style(Style::new().size(60, 12)).build(),
                    Div("c").style(Style::new().size(60, 8)).build(),
                ]),
        ),
    );
    rig.layout(tree);
    let ba = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let bb = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    let bc = LayoutLedger::committed(&rig.rec, rig.only("c")).expect("c");
    let br = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("row");
    assert!(approx(ba.x, 0.0) && approx(ba.y, 0.0), "a at origin");
    assert!(
        approx(bb.x, 0.0) && approx(bb.y, 14.0),
        "b line 2, got {}/{}",
        bb.x,
        bb.y
    );
    assert!(
        approx(bc.x, 0.0) && approx(bc.y, 30.0),
        "c line 3, got {}/{}",
        bc.x,
        bc.y
    );
    assert!(approx(br.w, 100.0), "row w, got {}", br.w);
    assert!(approx(br.h, 38.0), "row h expands to 3 lines, got {}", br.h);
    assert!(
        approx(br.content_w, 60.0),
        "content max line, got {}",
        br.content_w
    );
    assert!(
        approx(br.content_h, 38.0),
        "content_h, got {}",
        br.content_h
    );
}

#[test]
fn row_wrap_multi_child_lines_center_cross_axis_per_line() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // 100-wide, gap 4, Center: [40x10, 40x16] share line 1 (84 wide),
    // 40x10 breaks to line 2. Line heights 16/10; a centers at y=3.
    let tree: VNode = Div("root").child(
        Div("outer").style(Style::new().size(100, 400)).child(
            Row("r")
                .style(
                    Style::new()
                        .fill_width()
                        .gap(4)
                        .flex_wrap(FlexWrap::Wrap)
                        .align_items(AlignItems::Center),
                )
                .children([
                    Div("a").style(Style::new().size(40, 10)).build(),
                    Div("b").style(Style::new().size(40, 16)).build(),
                    Div("c").style(Style::new().size(40, 10)).build(),
                ]),
        ),
    );
    rig.layout(tree);
    let ba = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let bb = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    let bc = LayoutLedger::committed(&rig.rec, rig.only("c")).expect("c");
    let br = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("row");
    assert!(
        approx(ba.x, 0.0) && approx(ba.y, 3.0),
        "a centered, got {}/{}",
        ba.x,
        ba.y
    );
    assert!(
        approx(bb.x, 44.0) && approx(bb.y, 0.0),
        "b line top, got {}/{}",
        bb.x,
        bb.y
    );
    assert!(
        approx(bc.x, 0.0) && approx(bc.y, 20.0),
        "c line 2, got {}/{}",
        bc.x,
        bc.y
    );
    assert!(approx(br.h, 30.0), "row h = 16+10+4, got {}", br.h);
    assert!(
        approx(br.content_w, 84.0),
        "content max line, got {}",
        br.content_w
    );
}

#[test]
fn row_wrap_fill_takes_its_line_remainder() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // 100-wide, gap 4: fixed 60x10 + fill (empty Div, h 0).
    // Fill joins line 1 (60+4 <= 100) and takes 100-60-4 = 36.
    let tree: VNode = Div("root").child(
        Div("outer").style(Style::new().size(100, 400)).child(
            Row("r")
                .style(Style::new().fill_width().gap(4).flex_wrap(FlexWrap::Wrap))
                .children([
                    Div("fixed").style(Style::new().size(60, 10)).build(),
                    Div("f").style(Style::new().fill_width()).build(),
                ]),
        ),
    );
    rig.layout(tree);
    let bf = LayoutLedger::committed(&rig.rec, rig.only("f")).expect("f");
    let br = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("row");
    assert!(approx(bf.w, 36.0), "fill share of line, got {}", bf.w);
    assert!(approx(bf.x, 64.0), "fill x after gap, got {}", bf.x);
    assert!(approx(br.h, 10.0), "single line h, got {}", br.h);
    assert!(
        approx(br.content_w, 100.0),
        "line fills width, got {}",
        br.content_w
    );
}

#[test]
fn row_nowrap_default_overflows_single_line_under_constraint() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // No flex_wrap set: two 60-wide children in a 100x50 row stay on one
    // line and overflow (the shipped single-line flex, byte-identical).
    let tree: VNode =
        Div("root").child(Row("r").style(Style::new().size(100, 50).gap(4)).children([
            Div("a").style(Style::new().size(60, 10)).build(),
            Div("b").style(Style::new().size(60, 10)).build(),
        ]));
    rig.layout(tree);
    let ba = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let bb = LayoutLedger::committed(&rig.rec, rig.only("b")).expect("b");
    let br = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("row");
    assert!(approx(ba.y, bb.y), "single line, got {}/{}", ba.y, bb.y);
    assert!(approx(bb.x, 64.0), "b placed past overflow, got {}", bb.x);
    assert!(
        approx(br.w, 100.0) && approx(br.h, 50.0),
        "explicit box kept"
    );
    assert!(
        approx(br.content_w, 124.0),
        "overflow extent, got {}",
        br.content_w
    );
}

#[test]
#[should_panic(expected = "only Row wraps")]
fn column_wrap_refuses_loudly() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(
        Column::new()
            .style(Style::new().flex_wrap(FlexWrap::Wrap))
            .children([Div("a").style(Style::new().size(10, 10)).build()]),
    );
    rig.layout(tree);
}

// ---------------------------------------------------------------------------
// Round 1.4 (decision 255): overlay portals escape parent flow and
// anchor to the viewport.
// ---------------------------------------------------------------------------

#[test]
fn portal_escapes_parent_flow_and_anchors_to_viewport() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Rig viewport is 800x600. Column holds a fixed 100x10 child plus a
    // portal (listed first — order must not matter); the portal carries
    // a 60x12 box. Parent height excludes the portal; the portal spans
    // the viewport at the viewport origin — full height by default
    // (Round 7.21: unconstrained portals dim the whole window).
    let tree: VNode = Div("root").child(Column::new().children([
        Portal("p").child(Div("in").style(Style::new().size(60, 12)).build()),
        Div("a").style(Style::new().size(100, 10)).build(),
    ]));
    rig.layout(tree);
    let a = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    let p = LayoutLedger::committed(&rig.rec, rig.only("p")).expect("portal");
    let inner = LayoutLedger::committed(&rig.rec, rig.only("in")).expect("inner");
    assert!(approx(a.x, 0.0) && approx(a.y, 0.0), "flow child at top");
    assert!(
        approx(p.x, 0.0) && approx(p.y, 0.0),
        "portal at viewport origin"
    );
    assert!(
        approx(p.w, 800.0),
        "portal spans viewport width, got {}",
        p.w
    );
    assert!(
        approx(p.h, 600.0),
        "portal defaults to viewport height, got {}",
        p.h
    );
    assert!(
        approx(inner.x, 0.0) && approx(inner.y, 0.0),
        "portal child overlaid at the content origin"
    );
    // A portal nested in a Row (non-first position) behaves the same.
    let tree2: VNode =
        Div("root").child(Row("r").style(Style::new().size(200, 40).gap(4)).children([
            Div("fixed").style(Style::new().size(60, 10)).build(),
            Portal("p2").child(Div("in2").style(Style::new().size(60, 12)).build()),
        ]));
    rig.relayout(tree2);
    let r = LayoutLedger::committed(&rig.rec, rig.only("r")).expect("row");
    let p2 = LayoutLedger::committed(&rig.rec, rig.only("p2")).expect("portal2");
    assert!(
        approx(r.w, 200.0) && approx(r.h, 40.0),
        "explicit row box kept"
    );
    assert!(
        approx(r.content_w, 60.0),
        "portal adds no row extent or gap share, got {}",
        r.content_w
    );
    assert!(
        approx(p2.w, 800.0) && approx(p2.h, 600.0),
        "nested portal still viewport-anchored, got {}x{}",
        p2.w,
        p2.h
    );
}

/// Round 7.21 (decision 296): a portal with `x`/`absolute_y`
/// offsets anchors at its parent container's content origin (not
/// the viewport origin), keeps the explicit width, and hugs its
/// content height — the dropdown-popup shape.
#[test]
fn portal_with_offsets_anchors_to_parent_origin() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    // Root pads push the column content origin to (10, 20); the
    // portal offsets from there.
    let tree: VNode = Div("root").style(Style::new().pad_x(10).pad_y(20)).child(
        Column::new().children([
            Div("a").style(Style::new().size(100, 10)).build(),
            Portal("p")
                .style(Style::new().x(5).absolute_y(30).w(160))
                .child(Div("in").style(Style::new().size(60, 12)).build()),
        ]),
    );
    rig.layout(tree);
    let p = LayoutLedger::committed(&rig.rec, rig.only("p")).expect("portal");
    assert!(
        approx(p.x, 15.0) && approx(p.y, 50.0),
        "portal anchors at parent origin + offsets, got ({}, {})",
        p.x,
        p.y
    );
    assert!(approx(p.w, 160.0), "explicit width wins, got {}", p.w);
    assert!(
        approx(p.h, 12.0),
        "anchored portal hugs content height, got {}",
        p.h
    );
    let inner = LayoutLedger::committed(&rig.rec, rig.only("in")).expect("inner");
    assert!(
        approx(inner.x, 15.0) && approx(inner.y, 50.0),
        "portal child rides the anchor, got ({}, {})",
        inner.x,
        inner.y
    );
    assert!(
        approx(inner.w, 60.0),
        "content width follows the portal box, not the window, got {}",
        inner.w
    );
    let col = LayoutLedger::committed(&rig.rec, rig.only("column")).expect("column");
    assert!(
        approx(col.h, 10.0),
        "parent flow excludes the popup, got {}",
        col.h
    );
}

/// Round 7.21 follow-up (user eyeball): a repositioned ancestor
/// must not drag a viewport-anchored portal along — the padded root
/// pushes the column to (50, 40), but the offset-less portal stays
/// at the viewport origin while the offset portal tracks its parent.
#[test]
fn viewport_portal_ignores_parent_reposition() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").style(Style::new().pad_x(50).pad_y(40)).child(
        Column::new().children([
            Div("a").style(Style::new().size(100, 10)).build(),
            Portal("p").child(Div("in").style(Style::new().size(60, 12)).build()),
            Portal("q")
                .style(Style::new().x(5).absolute_y(30))
                .child(Div("in-q").style(Style::new().size(60, 12)).build()),
        ]),
    );
    rig.layout(tree);
    let a = LayoutLedger::committed(&rig.rec, rig.only("a")).expect("a");
    assert!(
        approx(a.x, 50.0) && approx(a.y, 40.0),
        "flow child rides the padded origin, got ({}, {})",
        a.x,
        a.y
    );
    let p = LayoutLedger::committed(&rig.rec, rig.only("p")).expect("portal");
    assert!(
        approx(p.x, 0.0) && approx(p.y, 0.0),
        "viewport portal stays at the origin, got ({}, {})",
        p.x,
        p.y
    );
    assert!(
        approx(p.w, 800.0) && approx(p.h, 600.0),
        "viewport portal still spans the window, got {}x{}",
        p.w,
        p.h
    );
    let inner = LayoutLedger::committed(&rig.rec, rig.only("in")).expect("inner");
    assert!(
        approx(inner.x, 0.0) && approx(inner.y, 0.0),
        "viewport child stays put, got ({}, {})",
        inner.x,
        inner.y
    );
    let q = LayoutLedger::committed(&rig.rec, rig.only("q")).expect("anchored");
    assert!(
        approx(q.x, 55.0) && approx(q.y, 70.0),
        "anchored portal tracks its parent, got ({}, {})",
        q.x,
        q.y
    );
}

/// Round 7.21 (decision 296): a `fill_height` portal child lays
/// out window-tall through the `given_h` channel, so viewport
/// layers center like explicitly-sized containers — the modal
/// backdrop shape (full-bleed + both-axis centering).
#[test]
fn portal_fill_height_child_expands_and_centers() {
    let (fake, calls) = FakeText::new();
    let mut rig = Rig::new(fake, calls);
    let tree: VNode = Div("root").child(
        Portal("p").child(
            Div("fill")
                .style(
                    Style::new()
                        .fill_width()
                        .fill_height()
                        .justify_content(JustifyContent::Center)
                        .align_items(AlignItems::Center),
                )
                .child(Div("in").style(Style::new().size(60, 12)).build()),
        ),
    );
    rig.layout(tree);
    let fill = LayoutLedger::committed(&rig.rec, rig.only("fill")).expect("fill");
    assert!(
        approx(fill.w, 800.0) && approx(fill.h, 600.0),
        "fill child spans the viewport, got {}x{}",
        fill.w,
        fill.h
    );
    let p = LayoutLedger::committed(&rig.rec, rig.only("p")).expect("portal");
    assert!(
        approx(p.w, 800.0) && approx(p.h, 600.0),
        "portal spans the viewport, got {}x{}",
        p.w,
        p.h
    );
    let inner = LayoutLedger::committed(&rig.rec, rig.only("in")).expect("inner");
    assert!(
        approx(inner.x, 370.0) && approx(inner.y, 294.0),
        "child centers on both axes, got ({}, {})",
        inner.x,
        inner.y
    );
}
