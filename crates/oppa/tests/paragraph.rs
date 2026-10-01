//! v2 item 2 golden paragraph corpus (decision 197): wrapping at
//! break opportunities only, committed lines + advances, per-byte
//! carets incl. wrap-point affinity and trailing carets.
//!
//! Provenance (text-derived, font-update-proof): break positions are
//! byte offsets from UAX #14 rules -- spaces break after, CJK breaks
//! between every pair, `/` breaks after slash runs, words never
//! split -- each rule pinned in `oppa-linebreak`'s unit tests
//! (Unicode 15.0.0). The stub below hand-lists those same offsets,
//! so this file pins the LAYOUT rules font-independently; the slice
//! suites (`oppa-text-dwrite`, `oppa-text-rustybuzz`) replay the
//! identical paragraphs through the real crate + real fonts.
//! Advances come from the uniform fake (0.625em/char, byte-exact),
//! at both text classes (title 16px -> 10px/char, body 14px ->
//! 8.75px/char). Non-ASCII corpus is `\u{...}` escapes (decision 45).

#![allow(non_snake_case)]

use oppa::{
    BreakSource, Cluster, Div, FontId, FontMetrics, LayoutBox, LayoutLedger, Reconciler, Runtime,
    ShapedGlyph, ShapedRun, Style, Text, TextError, TextRun, TextService, VNode,
};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Uniform fake TextService (per-char clusters, configurable rtl ranges)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FakeText {
    calls: Rc<Cell<usize>>,
    rtl_ranges: Vec<(usize, usize)>,
}

impl FakeText {
    fn new(rtl_ranges: Vec<(usize, usize)>) -> (Self, Rc<Cell<usize>>) {
        let calls = Rc::new(Cell::new(0));
        (
            Self {
                calls: calls.clone(),
                rtl_ranges,
            },
            calls,
        )
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
// Stub break source: hand-listed UAX offsets (see header provenance)
// ---------------------------------------------------------------------------

struct StubBreaks(Vec<usize>);

impl BreakSource for StubBreaks {
    fn opportunities(&self, _text: &str) -> Vec<usize> {
        self.0.clone()
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Rig {
    rt: Runtime,
    rec: Reconciler,
    styles: oppa::Interner<Style>,
    ledger: LayoutLedger,
    fake: FakeText,
}

impl Rig {
    fn new(fake: FakeText) -> Self {
        let rt = Runtime::new();
        Self {
            ledger: LayoutLedger::new(&rt),
            rt,
            rec: Reconciler::new(),
            styles: oppa::Interner::new(),
            fake,
        }
    }

    fn with_breaks(fake: FakeText, breaks: Vec<usize>) -> Self {
        let mut rig = Self::new(fake);
        rig.ledger
            .set_break_source(Some(Rc::new(StubBreaks(breaks))));
        rig
    }

    fn layout(&mut self, vnode: VNode) {
        self.rec.reconcile(&self.rt, &mut self.styles, false, vnode);
        self.ledger
            .run(&mut self.rec, &self.styles, Some(&self.fake), 800.0, 600.0);
    }

    fn relayout(&mut self, vnode: VNode) {
        self.layout(vnode)
    }

    fn leaf(&self) -> LayoutBox {
        self.rec
            .find_by_debug("text")
            .into_iter()
            .filter_map(|id| LayoutLedger::committed(&self.rec, id))
            .find(|b| !b.lines.is_empty())
            .expect("text leaf with lines")
    }
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

/// One paragraph in a width-constrained Div, both text classes.
/// Returns (title_box, title_shapes, body_box, body_shapes).
fn layout_both(
    text: &str,
    avail: f32,
    breaks: Vec<usize>,
    rtl_ranges: Vec<(usize, usize)>,
) -> (LayoutBox, usize, LayoutBox, usize) {
    let tree = |style| {
        Div("root").child(
            Div("t")
                .style(Style::new().size(avail, 200))
                .child(VNode::from(Text {
                    text: Arc::from(text),
                    style,
                })),
        )
    };
    let (fake, calls) = FakeText::new(rtl_ranges.clone());
    let mut rig = Rig::with_breaks(fake, breaks.clone());
    rig.layout(tree(Text::title_small));
    let title = rig.leaf();
    let title_calls = calls.get();
    let (fake, calls) = FakeText::new(rtl_ranges);
    let mut rig = Rig::with_breaks(fake, breaks);
    rig.layout(tree(Text::body_secondary));
    let body = rig.leaf();
    (title, title_calls, body, calls.get())
}

/// Byte span + width of every committed line.
fn line_spans(b: &LayoutBox) -> Vec<((usize, usize), f32)> {
    b.lines
        .iter()
        .map(|l| {
            let s = l.clusters.iter().map(|c| c.byte_range.0).min().unwrap_or(0);
            let e = l.clusters.iter().map(|c| c.byte_range.1).max().unwrap_or(0);
            ((s, e), l.width)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// P1 latin prose -- wraps at spaces only, trim, affinity, both classes
// ---------------------------------------------------------------------------

const P1: &str = "The quick brown fox jumps";

#[test]
fn latin_prose_wraps_at_spaces_only() {
    // Breaks after each space (4, 10, 16, 20); avail 100 fits one
    // "word " span (title: 60) beside "The " (40) but never two.
    let (title, calls, body, body_calls) = layout_both(P1, 100.0, vec![4, 10, 16, 20], vec![]);
    assert_eq!(calls, 1, "one shape for the paragraph");
    assert_eq!(body_calls, 1);
    // Committed lines byte-exact on both classes (text-derived).
    for b in [&title, &body] {
        let spans: Vec<(usize, usize)> = line_spans(b).iter().map(|(r, _)| *r).collect();
        assert_eq!(spans, vec![(0, 9), (10, 19), (20, 25)], "break bytes");
    }
    // Advances byte-exact per class (uniform fake).
    let widths: Vec<f32> = line_spans(&title).iter().map(|(_, w)| *w).collect();
    assert_eq!(widths, vec![90.0, 90.0, 50.0]);
    let widths: Vec<f32> = line_spans(&body).iter().map(|(_, w)| *w).collect();
    assert_eq!(widths, vec![78.75, 78.75, 43.75]);
    // Per-byte carets hold across wrapped lines (title shown; body
    // scales by 0.875 -- pinned below): in-cluster bytes read the
    // linear leading edge, trimmed spaces read the next leading.
    for b in 0..9usize {
        assert_eq!(title.caret_position(b), (0, b as f32 * 10.0), "byte {b}");
    }
    assert_eq!(title.caret_position(9), (1, 0.0), "trimmed space affinity");
    for b in 10..19usize {
        assert_eq!(
            title.caret_position(b),
            (1, (b - 10) as f32 * 10.0),
            "byte {b}"
        );
    }
    assert_eq!(title.caret_position(19), (2, 0.0), "trimmed space affinity");
    for b in 20..25usize {
        assert_eq!(
            title.caret_position(b),
            (2, (b - 20) as f32 * 10.0),
            "byte {b}"
        );
    }
    assert_eq!(title.caret_position(25), (2, 50.0), "trailing caret");
    // Body class: affinity + trailing + one mid-line sample.
    assert_eq!(body.caret_position(9), (1, 0.0));
    assert_eq!(body.caret_position(19), (2, 0.0));
    assert!(approx(body.caret_position(25).1, 43.75));
    assert!(approx(body.caret_position(14).1, 35.0));
    // Content height: 3 title lines x 16 + 2 gaps x 2.
    let content = title.content_h;
    assert!(approx(content, 52.0), "content_h = {content}");
}

// ---------------------------------------------------------------------------
// P2 over-wide word -- pushes whole, overflows, never splits
// ---------------------------------------------------------------------------

const P2: &str = "Go Supercalifragilistic ok";

#[test]
fn overwide_word_pushes_whole() {
    assert_eq!(P2.len(), 26);
    let (title, calls, body, _) = layout_both(P2, 60.0, vec![3, 24], vec![]);
    assert_eq!(calls, 1);
    let spans = line_spans(&title);
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[0].0, (0, 2), "Go (space trimmed)");
    assert!(approx(spans[0].1, 20.0));
    // The 20-letter word stands whole on its own overflowing line.
    assert_eq!(spans[1].0, (3, 23));
    assert!(approx(spans[1].1, 200.0), "overflow width = {}", spans[1].1);
    assert_eq!(title.lines[1].clusters.len(), 20, "never split mid-word");
    assert_eq!(spans[2].0, (24, 26));
    // Affinity around the pushed span (title + body).
    assert_eq!(title.caret_position(2), (1, 0.0));
    assert_eq!(title.caret_position(23), (2, 0.0));
    assert_eq!(title.caret_position(26), (2, 20.0));
    assert_eq!(body.caret_position(2), (1, 0.0));
    assert_eq!(body.caret_position(23), (2, 0.0));
    assert!(approx(body.caret_position(26).1, 17.5));
}

// ---------------------------------------------------------------------------
// P2b URL -- wraps after slash runs only
// ---------------------------------------------------------------------------

const P2B: &str = "See https://x.io/a/b end";

#[test]
fn url_wraps_after_slash_runs_only() {
    assert_eq!(P2B.len(), 24);
    let (title, _, body, _) = layout_both(P2B, 60.0, vec![4, 12, 17, 19, 21], vec![]);
    let spans = line_spans(&title);
    let ranges: Vec<(usize, usize)> = spans.iter().map(|(r, _)| *r).collect();
    assert_eq!(
        ranges,
        vec![(0, 3), (4, 12), (12, 17), (17, 20), (21, 24)],
        "no mid-token split"
    );
    let widths: Vec<f32> = spans.iter().map(|(_, w)| *w).collect();
    assert_eq!(widths, vec![30.0, 80.0, 50.0, 30.0, 30.0]);
    assert_eq!(title.caret_position(3), (1, 0.0));
    assert_eq!(title.caret_position(20), (4, 0.0));
    assert_eq!(title.caret_position(24), (4, 30.0));
    // Body class: same break bytes, scaled advances.
    let ranges: Vec<(usize, usize)> = line_spans(&body).iter().map(|(r, _)| *r).collect();
    assert_eq!(ranges, vec![(0, 3), (4, 12), (12, 17), (17, 20), (21, 24)]);
}

// ---------------------------------------------------------------------------
// P3 Arabic RTL -- shape-whole-then-break (Q5), visual mirror per line
// ---------------------------------------------------------------------------

const P3: &str = "\u{645}\u{631}\u{62D}\u{628}\u{627} \u{627}\u{644}\u{639}\u{627}\u{644}\u{645} \u{0643}\u{0628}\u{064A}\u{0631}";

#[test]
fn arabic_paragraph_shapes_once_wraps_mirrored() {
    assert_eq!(P3.len(), 32);
    let rtl = vec![(0, 10), (11, 23), (24, 32)];
    // Avail 90 packs three lines on BOTH classes (title spans
    // 60/70/40, body 52.5/61.25/35) -- same break bytes, scaled widths.
    let (title, calls, body, _) = layout_both(P3, 90.0, vec![11, 24], rtl);
    // Q5: one whole-paragraph shape feeds all three lines (M3
    // no-re-shape rule holds under opportunity wrapping).
    assert_eq!(calls, 1, "shape-whole-paragraph-then-break");
    let spans = line_spans(&title);
    let ranges: Vec<(usize, usize)> = spans.iter().map(|(r, _)| *r).collect();
    assert_eq!(ranges, vec![(0, 10), (11, 23), (24, 32)]);
    let widths: Vec<f32> = spans.iter().map(|(_, w)| *w).collect();
    assert_eq!(widths, vec![50.0, 60.0, 40.0]);
    // Each line's single RTL run mirrors: logical-last cluster first.
    assert_eq!(title.lines[0].clusters[0].byte_range, (8, 10));
    assert_eq!(title.lines[1].clusters[0].byte_range, (21, 23));
    // RTL carets: logical start reads the right edge, trailing the left.
    assert!(approx(title.lines[0].caret_x(0), 50.0));
    assert!(approx(title.lines[0].caret_x(10), 0.0));
    // Box-level: byte 10 (trimmed space) takes line 2's leading caret.
    assert_eq!(title.caret_position(10), (1, 60.0));
    assert_eq!(title.caret_position(23), (2, 40.0));
    assert_eq!(title.caret_position(32), (2, 0.0), "RTL trailing at left");
    // Body class mirrors identically, scaled (same 3-line packing).
    let spans: Vec<(usize, usize)> = line_spans(&body).iter().map(|(r, _)| *r).collect();
    assert_eq!(spans, vec![(0, 10), (11, 23), (24, 32)]);
    assert!(approx(body.lines[0].caret_x(0), 43.75));
    assert_eq!(body.caret_position(10), (1, 52.5));
    assert_eq!(body.caret_position(23), (2, 35.0));
    assert_eq!(body.caret_position(32), (2, 0.0));
}

// ---------------------------------------------------------------------------
// P4 CJK no-space -- wraps between characters
// ---------------------------------------------------------------------------

const P4: &str = "\u{65E5}\u{672C}\u{8A9E}\u{6587}\u{5B57}\u{5217}";

#[test]
fn cjk_wraps_between_chars() {
    assert_eq!(P4.len(), 18);
    // Avail 40 packs two lines on both classes (title 4+2 chars,
    // body 4+2 chars at 8.75) -- same break bytes, scaled advances.
    let (title, _, body, _) = layout_both(P4, 40.0, vec![3, 6, 9, 12, 15], vec![]);
    for b in [&title, &body] {
        let spans: Vec<(usize, usize)> = line_spans(b).iter().map(|(r, _)| *r).collect();
        assert_eq!(spans, vec![(0, 12), (12, 18)]);
    }
    let widths: Vec<f32> = line_spans(&title).iter().map(|(_, w)| *w).collect();
    assert_eq!(widths, vec![40.0, 20.0]);
    assert_eq!(title.caret_position(12), (1, 0.0));
    assert_eq!(title.caret_position(18), (1, 20.0));
    let widths: Vec<f32> = line_spans(&body).iter().map(|(_, w)| *w).collect();
    assert_eq!(widths, vec![35.0, 17.5]);
    assert_eq!(body.caret_position(12), (1, 0.0));
}

// ---------------------------------------------------------------------------
// P5 mixed script with \n -- hard splits + soft wraps + affinity
// ---------------------------------------------------------------------------

const P5: &str = "aa bb \u{4F60}\u{597D}\ncc dd";

#[test]
fn mixed_newline_hard_split_with_soft_wraps() {
    assert_eq!(P5.len(), 18);
    // Two shapes: one per '\n'-paragraph (decision 196 -- the engine
    // shapes each hard-break paragraph whole, then wraps soft without
    // re-shaping). Breaks hand-listed: spaces [3, 6, 16] + CJK pair 9.
    let rtl: Vec<(usize, usize)> = vec![];
    let (fake, calls) = FakeText::new(rtl.clone());
    let tree = || {
        Div("root").child(
            Div("t")
                .style(Style::new().size(60, 200))
                .child(VNode::from(Text {
                    text: Arc::from(P5),
                    style: Text::title_small,
                })),
        )
    };
    let mut rig = Rig::with_breaks(fake, vec![3, 6, 9, 16]);
    rig.layout(tree());
    assert_eq!(calls.get(), 2, "one whole shape per hard-break paragraph");
    let title = rig.leaf();
    rig.relayout(tree());
    assert_eq!(calls.get(), 2, "re-wrap shapes nothing");
    let spans = line_spans(&title);
    let ranges: Vec<(usize, usize)> = spans.iter().map(|(r, _)| *r).collect();
    assert_eq!(ranges, vec![(0, 5), (6, 12), (13, 18)]);
    let widths: Vec<f32> = spans.iter().map(|(_, w)| *w).collect();
    assert_eq!(widths, vec![50.0, 20.0, 50.0]);
    // Soft-break affinity (trimmed space) and hard-break affinity
    // (newline byte) both read the next line's leading caret.
    assert_eq!(title.caret_position(5), (1, 0.0));
    assert_eq!(title.caret_position(12), (2, 0.0), "newline byte forward");
    assert_eq!(title.caret_position(18), (2, 50.0));
    // Body class through the same stitched path (2 shapes).
    let (fake, calls) = FakeText::new(vec![]);
    let mut rig = Rig::with_breaks(fake, vec![3, 6, 9, 16]);
    rig.layout(
        Div("root").child(
            Div("t")
                .style(Style::new().size(60, 200))
                .child(VNode::from(Text {
                    text: Arc::from(P5),
                    style: Text::body_secondary,
                })),
        ),
    );
    assert_eq!(calls.get(), 2);
    let body = rig.leaf();
    let ranges: Vec<(usize, usize)> = line_spans(&body).iter().map(|(r, _)| *r).collect();
    assert_eq!(ranges, vec![(0, 5), (6, 12), (13, 18)]);
    assert_eq!(body.caret_position(12), (2, 0.0));
}

// ---------------------------------------------------------------------------
// No break source -> legacy greedy (mid-word splits preserved)
// ---------------------------------------------------------------------------

#[test]
fn greedy_default_splits_midword() {
    // Same P1 text/width WITHOUT a source: greedy lines of 10 chars,
    // trailing spaces kept -- the exact M3 behavior item 2 replaces
    // only when a source is installed.
    let (fake, _calls) = FakeText::new(vec![]);
    let mut rig = Rig::new(fake);
    rig.layout(
        Div("root").child(
            Div("t")
                .style(Style::new().size(100, 200))
                .child(VNode::from(Text {
                    text: Arc::from(P1),
                    style: Text::title_small,
                })),
        ),
    );
    let b = rig.leaf();
    let spans = line_spans(&b);
    let ranges: Vec<(usize, usize)> = spans.iter().map(|(r, _)| *r).collect();
    assert_eq!(
        ranges,
        vec![(0, 10), (10, 20), (20, 25)],
        "greedy keeps spaces"
    );
    let widths: Vec<f32> = spans.iter().map(|(_, w)| *w).collect();
    assert_eq!(widths, vec![100.0, 100.0, 50.0]);
}
