//! v2 item 2 slice proof -- Windows DirectWrite through the engine
//! (decision 198): the stub corpus's golden paragraphs replayed with
//! real Segoe UI shaping + the real `UnicodeBreakSource`, committed
//! through `LayoutLedger` with `set_break_source` (the production
//! plumbing, not a direct `layout_text` call).
//!
//! Zero backend-specific code: the break layer consumes dwrite's
//! native `ShapedRun`s identically (same pure function, same
//! opportunity type). Break positions are opportunity-EXACT (pinned
//! byte ranges); advances are TOL-BANDED -- `|actual - expected| <=
//! max(1.0px, 5% x expected)` -- because Segoe UI ships with Windows
//! and updates under builds. A drift beyond the band is a font
//! update to re-baseline, never a silent pass.
//!
//! Machine-local font debt: Segoe UI (system) + the CJK fallback
//! face dwrite resolved on this box (P4). P5 (`\n`) shapes because
//! the engine splits hard-break paragraphs before shaping
//! (decision 196) -- direct whole-text shaping still refuses the
//! newline run loudly (pre-existing slice bound, unchanged).
//! Non-ASCII corpus is `\u{...}` escapes (decision 45).

#![cfg(windows)]

use oppa::{Div, LayoutBox, LayoutLedger, Reconciler, Runtime, Style, Text, VNode};
use oppa_linebreak::UnicodeBreakSource;
use oppa_text_dwrite::DWriteTextService;
use std::rc::Rc;
use std::sync::Arc;

struct Rig {
    rt: Runtime,
    rec: Reconciler,
    styles: oppa::Interner<Style>,
    ledger: LayoutLedger,
    svc: DWriteTextService,
}

impl Rig {
    fn new() -> Self {
        let rt = Runtime::new();
        let mut ledger = LayoutLedger::new(&rt);
        ledger.set_break_source(Some(Rc::new(UnicodeBreakSource::new())));
        Self {
            ledger,
            rt,
            rec: Reconciler::new(),
            styles: oppa::Interner::new(),
            svc: DWriteTextService::new().expect("DirectWrite factory"),
        }
    }

    fn layout(&mut self, vnode: VNode) {
        self.rec.reconcile(&self.rt, &mut self.styles, false, vnode);
        self.ledger
            .run(&mut self.rec, &self.styles, Some(&self.svc), 800.0, 600.0);
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

/// Tolerance band for system-font advances (decision 198).
fn within_band(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() <= (1.0f32).max(0.05 * expected.abs())
}

fn check_widths(lines: &LayoutBox, expected: &[f32]) {
    assert_eq!(lines.lines.len(), expected.len(), "line count");
    for (i, (line, exp)) in lines.lines.iter().zip(expected.iter()).enumerate() {
        assert!(
            within_band(line.width, *exp),
            "line {i} width {} outside band around {exp}",
            line.width
        );
    }
}

fn ranges(b: &LayoutBox) -> Vec<(usize, usize)> {
    b.lines
        .iter()
        .map(|l| {
            let s = l.clusters.iter().map(|c| c.byte_range.0).min().unwrap_or(0);
            let e = l.clusters.iter().map(|c| c.byte_range.1).max().unwrap_or(0);
            (s, e)
        })
        .collect()
}

/// One paragraph in a width-constrained Div, both text classes.
fn layout_both(text: &str, avail: f32) -> (LayoutBox, LayoutBox) {
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
    let mut rig = Rig::new();
    rig.layout(tree(Text::title_small));
    let title = rig.leaf();
    let mut rig = Rig::new();
    rig.layout(tree(Text::body_secondary));
    let body = rig.leaf();
    (title, body)
}

const P1: &str = "The quick brown fox jumps";

#[test]
fn latin_prose_opportunity_exact_widths_banded() {
    let (title, body) = layout_both(P1, 100.0);
    for b in [&title, &body] {
        assert_eq!(
            ranges(b),
            vec![(0, 9), (10, 19), (20, 25)],
            "break bytes exact"
        );
    }
    check_widths(&title, &[67.88281, 71.07031, 42.90625]);
    check_widths(&body, &[59.39746, 62.186523, 37.54297]);
    // Affinity structure is font-independent: trimmed spaces read the
    // next line's leading caret (x = 0 exactly).
    assert_eq!(title.caret_position(9), (1, 0.0));
    assert_eq!(title.caret_position(19), (2, 0.0));
    assert_eq!(body.caret_position(9), (1, 0.0));
}

const P2: &str = "Go Supercalifragilistic ok";

#[test]
fn overwide_word_pushes_whole() {
    let (title, body) = layout_both(P2, 60.0);
    for b in [&title, &body] {
        assert_eq!(ranges(b), vec![(0, 2), (3, 23), (24, 26)]);
    }
    check_widths(&title, &[20.351563, 127.40625, 17.328125]);
    check_widths(&body, &[17.807617, 111.48047, 15.162109]);
    assert_eq!(title.lines[1].clusters.len(), 20, "never split mid-word");
    assert_eq!(title.caret_position(2), (1, 0.0));
    assert_eq!(title.caret_position(23), (2, 0.0));
}

const P2B: &str = "See https://x.io/a/b end";

#[test]
fn url_wraps_at_opportunities_only() {
    let (title, body) = layout_both(P2B, 60.0);
    for b in [&title, &body] {
        assert_eq!(ranges(b), vec![(0, 3), (4, 12), (12, 20), (21, 24)]);
    }
    check_widths(&title, &[25.234375, 52.03125, 54.078125, 26.84375]);
    check_widths(&body, &[22.080078, 45.527344, 47.31836, 23.488281]);
    assert_eq!(title.caret_position(3), (1, 0.0));
    assert_eq!(title.caret_position(20), (3, 0.0));
}

const P3: &str = "\u{645}\u{631}\u{62D}\u{628}\u{627} \u{627}\u{644}\u{639}\u{627}\u{644}\u{645} \u{0643}\u{0628}\u{064A}\u{0631}";

#[test]
fn arabic_wraps_mirrored_affinity_forward() {
    let (title, body) = layout_both(P3, 90.0);
    for b in [&title, &body] {
        assert_eq!(ranges(b), vec![(0, 23), (24, 32)]);
    }
    check_widths(&title, &[76.53125, 25.382813]);
    check_widths(&body, &[66.96484, 22.20996]);
    // Single RTL run mirrors per line (dwrite shapes logical order).
    assert_eq!(title.lines[0].clusters[0].byte_range, (21, 23));
    // Trimmed-space affinity: byte 23 reads line 2's leading caret.
    assert_eq!(title.caret_position(23).0, 1);
    assert_eq!(title.caret_position(23).1, title.lines[1].caret_x(24));
    assert_eq!(body.caret_position(23).0, 1);
}

const P4: &str = "\u{65E5}\u{672C}\u{8A9E}\u{6587}\u{5B57}\u{5217}";

#[test]
fn cjk_wraps_between_chars() {
    let (title, body) = layout_both(P4, 40.0);
    for b in [&title, &body] {
        assert_eq!(ranges(b), vec![(0, 6), (6, 12), (12, 18)]);
    }
    check_widths(&title, &[32.0, 32.0, 32.0]);
    check_widths(&body, &[28.0, 28.0, 28.0]);
    assert_eq!(title.caret_position(6), (1, 0.0));
}

const P5: &str = "aa bb \u{4F60}\u{597D}\ncc dd";

#[test]
fn mixed_newline_splits_soft_wraps() {
    // Engine-level: the '\n' paragraphs shape separately (decision
    // 196) -- dwrite never sees the newline run.
    let (title, body) = layout_both(P5, 60.0);
    for b in [&title, &body] {
        assert_eq!(ranges(b), vec![(0, 9), (9, 12), (13, 18)]);
    }
    // Segoe's narrower advances merge "aa bb " past breaks the
    // uniform fake takes -- every line still starts in-set.
    assert_eq!(title.caret_position(9), (1, 0.0));
    assert_eq!(title.caret_position(12), (2, 0.0), "newline byte forward");
    assert_eq!(body.caret_position(12), (2, 0.0));
    // Widths tol-banded (Segoe UI advances on this box).
    check_widths(&title, &[59.859375, 16.0, 38.007813]);
    check_widths(&body, &[52.376953, 14.0, 33.256836]);
}
