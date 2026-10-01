//! v2 item 2 slice proof -- shared rustybuzz core through the vendored
//! DejaVu Sans anchor (decision 197): the stub corpus's golden
//! paragraphs replayed through real shaping + the real
//! `UnicodeBreakSource`, same avail widths.
//!
//! Provenance: `test-fonts/DejaVuSans.ttf` (sha256 recorded in
//! `LICENSE-DejaVu.txt`, Bitstream Vera -- verified permissive at
//! build) never updates, so advances are byte-exact here, not
//! tol-banded. Break positions are byte-exact on all sets
//! (text-derived). Narrower real advances legitimately pack MORE per
//! line than the uniform fake (P2b, P3) -- every line boundary stays
//! inside the opportunity set; that subset relation is asserted, not
//! just the widths. Metrics (16px: ascent 14.8515625, descent
//! 3.7734375, gap 0) ride each layout. Non-ASCII corpus is
//! `\u{...}` escapes (decision 45).

use oppa::layout::layout_text_with_breaks;
use oppa::text::{BreakSource, TextService, TextStyle};
use oppa_linebreak::UnicodeBreakSource;
use oppa_text_rustybuzz::RustybuzzService;
use std::path::PathBuf;

fn service() -> RustybuzzService {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-fonts");
    assert!(
        dir.join("DejaVuSans.ttf").exists(),
        "vendored anchor missing: {}",
        dir.display()
    );
    let (svc, skipped) = RustybuzzService::from_dir_with_chain(&dir, &[]).expect("font dir");
    assert!(skipped.is_empty(), "{skipped:?}");
    svc
}

fn style(size: f32) -> TextStyle {
    TextStyle::new("DejaVu Sans", size)
}

/// Shape whole, break after: one shape call feeds every line (Q5 --
/// the M3 no-re-shape rule). Returns laid lines + the shaped run.
fn lay(
    svc: &RustybuzzService,
    text: &str,
    size: f32,
    avail: f32,
) -> (
    Vec<oppa::layout::LaidLine>,
    oppa::text::ShapedRun,
    Vec<usize>,
) {
    let run = svc.shape(text, &style(size)).expect("shape");
    let again = svc.shape(text, &style(size)).expect("re-shape");
    assert_eq!(run, again, "shaping is deterministic");
    let breaks = UnicodeBreakSource::new().opportunities(text);
    let m = svc.measure_line(&run);
    let lines = layout_text_with_breaks(
        &run, text, avail, None, m.ascent, m.descent, m.line_gap, &breaks,
    );
    (lines, run, breaks)
}

fn spans(lines: &[oppa::layout::LaidLine]) -> Vec<((usize, usize), f32)> {
    lines
        .iter()
        .map(|l| {
            let s = l.clusters.iter().map(|c| c.byte_range.0).min().unwrap_or(0);
            let e = l.clusters.iter().map(|c| c.byte_range.1).max().unwrap_or(0);
            ((s, e), l.width)
        })
        .collect()
}

/// Every wrapped line starts where an opportunity offered (line
/// ends follow by the trim rule -- trailing blanks are cut, so ends
/// are not themselves offsets). First line starts at 0.
fn assert_opportunity_only(lines: &[oppa::layout::LaidLine], breaks: &[usize], _len: usize) {
    for (i, line) in lines.iter().enumerate() {
        let s = line.clusters.iter().map(|c| c.byte_range.0).min().unwrap();
        if i == 0 {
            assert_eq!(s, 0, "first line starts at 0");
        } else {
            assert!(
                breaks.contains(&s),
                "line {i} starts at {s}, not an opportunity"
            );
        }
    }
}

const P1: &str = "The quick brown fox jumps";

#[test]
fn latin_prose_breaks_match_corpus_widths_anchored() {
    let svc = service();
    // Title 16px.
    let (lines, _, breaks) = lay(&svc, P1, 16.0, 100.0);
    assert_eq!(breaks, vec![4, 10, 16, 20]);
    let got = spans(&lines);
    assert_eq!(got.len(), 3);
    assert_eq!(got[0].0, (0, 9));
    assert_eq!(got[0].1, 77.64844);
    assert_eq!(got[1].0, (10, 19));
    assert_eq!(got[1].1, 78.88281);
    assert_eq!(got[2].0, (20, 25));
    assert_eq!(got[2].1, 48.664063);
    assert_opportunity_only(&lines, &breaks, P1.len());
    // Body 14px: same break bytes, scaled advances.
    let (lines, _, breaks) = lay(&svc, P1, 14.0, 100.0);
    assert_eq!(breaks, vec![4, 10, 16, 20]);
    let got = spans(&lines);
    assert_eq!(got[0], ((0, 9), 67.94238));
    assert_eq!(got[1], ((10, 19), 69.02246));
    assert_eq!(got[2], ((20, 25), 42.581055));
}

const P2: &str = "Go Supercalifragilistic ok";

#[test]
fn overwide_word_pushes_whole() {
    let svc = service();
    let (lines, _, breaks) = lay(&svc, P2, 16.0, 60.0);
    assert_eq!(breaks, vec![3, 24]);
    let got = spans(&lines);
    assert_eq!(got.len(), 3);
    assert_eq!(got[0], ((0, 2), 22.1875));
    assert_eq!(got[1].0, (3, 23));
    assert_eq!(got[1].1, 147.375, "over-wide span overflows whole");
    assert_eq!(lines[1].clusters.len(), 20, "never split mid-word");
    assert_eq!(got[2], ((24, 26), 19.054688));
}

const P2B: &str = "See https://x.io/a/b end";

#[test]
fn url_wraps_at_opportunities_only() {
    let svc = service();
    let (lines, _, breaks) = lay(&svc, P2B, 16.0, 60.0);
    assert_eq!(breaks, vec![4, 12, 17, 19, 21]);
    // Narrower real advances merge "x.io/a/" and "b end" past breaks
    // the uniform fake takes -- every boundary stays in-set.
    let got = spans(&lines);
    assert_eq!(got.len(), 4);
    assert_eq!(got[0], ((0, 3), 29.84375));
    assert_eq!(got[1], ((4, 12), 57.351563));
    assert_eq!(got[2], ((12, 19), 49.375));
    assert_eq!(got[3], ((19, 24), 45.382813));
    assert_opportunity_only(&lines, &breaks, P2B.len());
}

const P3: &str = "\u{645}\u{631}\u{62D}\u{628}\u{627} \u{627}\u{644}\u{639}\u{627}\u{644}\u{645} \u{0643}\u{0628}\u{064A}\u{0631}";

#[test]
fn arabic_shapes_whole_wraps_mirrored() {
    let svc = service();
    // Q5 on the shared core: one whole-paragraph shape (single RTL
    // run -- spaces attach per the itemizer) feeds both lines; joined
    // forms span the line-break boundary because nothing re-shapes.
    let (lines, run, breaks) = lay(&svc, P3, 16.0, 90.0);
    assert_eq!(breaks, vec![11, 24]);
    assert_eq!(run.runs.len(), 1);
    assert!(run.runs[0].rtl);
    let got = spans(&lines);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0], ((0, 23), 79.515625));
    assert_eq!(got[1], ((24, 32), 26.101563));
    assert_opportunity_only(&lines, &breaks, P3.len());
    // The first line's single RTL run mirrors wholesale.
    assert_eq!(lines[0].clusters[0].byte_range, (21, 23));
    assert_eq!(lines[1].clusters[0].byte_range, (30, 32));
    // Affinity: byte 23 (trimmed space) reads line 2's leading caret.
    let b = oppa::layout::LayoutBox {
        lines,
        ..Default::default()
    };
    let (li, x) = b.caret_position(23);
    assert_eq!(li, 1);
    assert!(x > 0.0 && x <= 26.101563, "leading caret, got {x}");
    // Body class: same breaks, scaled packing.
    let (lines, _, breaks) = lay(&svc, P3, 14.0, 90.0);
    assert_eq!(breaks, vec![11, 24]);
    let got = spans(&lines);
    assert_eq!(got[0], ((0, 23), 69.57617));
    assert_eq!(got[1], ((24, 32), 22.838867));
}

#[test]
fn cjk_without_coverage_fails_loud_never_tofu() {
    // The single vendored face has no CJK coverage: the chain covers
    // missing *glyphs* within a valid family, never invents them.
    let svc = service();
    let err = svc
        .shape(
            "\u{65E5}\u{672C}\u{8A9E}\u{6587}\u{5B57}\u{5217}",
            &style(16.0),
        )
        .expect_err("CJK must not shape on DejaVu Sans alone");
    let msg = format!("{err}");
    assert!(msg.contains("U+65E5"), "names the uncovered char: {msg}");
}
