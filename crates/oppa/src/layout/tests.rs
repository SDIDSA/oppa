use super::*;
use crate::text::{Cluster, FontId, FontMetrics, ShapedGlyph, TextRun};

pub(crate) fn synthetic_run(pieces: &[(u32, f32, bool)]) -> ShapedRun {
    // pieces: (byte_len, advance, rtl) — one cluster + one glyph each,
    // runs merged across consecutive same-direction pieces.
    let mut glyphs = Vec::new();
    let mut clusters = Vec::new();
    let mut runs = Vec::new();
    let mut byte = 0usize;
    let metrics = FontMetrics {
        ascent: 12.0,
        descent: 4.0,
        line_gap: 2.0,
    };
    let mut i = 0;
    while i < pieces.len() {
        let rtl = pieces[i].2;
        let run_start_byte = byte;
        let run_start_glyph = glyphs.len();
        while i < pieces.len() && pieces[i].2 == rtl {
            let (len, adv, _) = pieces[i];
            glyphs.push(ShapedGlyph {
                glyph_id: i as u32,
                x_advance: adv,
                x_offset: 0.0,
                y_offset: 0.0,
            });
            clusters.push(Cluster {
                byte_range: (byte, byte + len as usize),
                glyph_range: (glyphs.len() - 1, glyphs.len()),
            });
            byte += len as usize;
            i += 1;
        }
        runs.push(TextRun {
            byte_range: (run_start_byte, byte),
            glyph_range: (run_start_glyph, glyphs.len()),
            rtl,
            script: 0,
            font_id: FontId(0),
            font_metrics: metrics,
        });
    }
    let total_advance = glyphs.iter().map(|g| g.x_advance).sum();
    ShapedRun {
        glyphs,
        runs,
        clusters,
        total_advance,
        text_len_bytes: byte,
    }
}

#[test]
fn ltr_text_orders_unchanged() {
    let run = synthetic_run(&[(1, 10.0, false), (1, 10.0, false), (1, 10.0, false)]);
    let ordered = order_visual(&run, "abc", &[0, 1, 2]);
    let xs: Vec<f32> = ordered.iter().map(|o| o.x).collect();
    assert_eq!(xs, vec![0.0, 10.0, 20.0]);
    assert!(ordered.iter().all(|o| !o.rtl));
}

#[test]
fn rtl_sequence_reverses_with_mirrored_positions() {
    // "abc مرحبا 123"-shaped: LTR(3) + RTL(2) + LTR(3), 10px each.
    let run = synthetic_run(&[
        (1, 10.0, false),
        (1, 10.0, false),
        (1, 10.0, false),
        (2, 10.0, true),
        (2, 10.0, true),
        (1, 10.0, false),
        (1, 10.0, false),
        (1, 10.0, false),
    ]);
    let logical: Vec<usize> = (0..8).collect();
    let ordered = order_visual(&run, "abc\u{645}\u{631}def", &logical);
    // Visual cluster index order: LTR head, mirrored RTL, LTR tail.
    let idx: Vec<usize> = ordered.iter().map(|o| o.index).collect();
    assert_eq!(idx, vec![0, 1, 2, 4, 3, 5, 6, 7]);
    // Total width preserved (reorder, not re-measure).
    let total: f32 = ordered.iter().map(|o| o.width).sum();
    assert_eq!(total, 80.0);
    // RTL segment occupies [30, 50); first logical RTL cluster (3) sits
    // rightmost at x=40 (mirror), second (4) at x=30.
    assert_eq!(ordered[3].x, 30.0);
    assert_eq!(ordered[3].index, 4);
    assert_eq!(ordered[4].x, 40.0);
    assert_eq!(ordered[4].index, 3);
    // RTL flags ride along.
    assert!(ordered[3].rtl && ordered[4].rtl);
}

#[test]
fn visual_caret_follows_logical_prefix_end() {
    let run = synthetic_run(&[
        (1, 10.0, false),
        (1, 10.0, false),
        (1, 10.0, false),
        (2, 10.0, true),
        (2, 10.0, true),
        (1, 10.0, false),
        (1, 10.0, false),
        (1, 10.0, false),
    ]);
    let lines = layout_text(
        &run,
        "abc\u{645}\u{631}def",
        f32::INFINITY,
        None,
        12.0,
        4.0,
        2.0,
    );
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    // Byte 0: paragraph-direction edge.
    assert_eq!(line.caret_x(0), 0.0);
    // LTR→RTL boundary (byte 3): forward affinity — the next (RTL)
    // character begins at its right edge, x=50.
    assert_eq!(line.caret_x(3), 50.0);
    // Mid-Arabic boundary (byte 5, start of second RTL cluster):
    // forward affinity → right edge of its slot, x=40.
    assert_eq!(line.caret_x(5), 40.0);
    // RTL→LTR boundary (byte 7): the next (LTR) character begins at
    // its left edge, x=50.
    assert_eq!(line.caret_x(7), 50.0);
    // Trailing caret: the line width.
    assert_eq!(line.caret_x(10), 80.0);
}

#[test]
fn source_order_caret_diverges_where_visual_collapses() {
    // The M3 bidi gate in miniature: source-order caret math
    // (ShapedRun::caret_x, logical leading edges) vs visual carets.
    let run = synthetic_run(&[
        (1, 10.0, false),
        (1, 10.0, false),
        (1, 10.0, false),
        (2, 10.0, true),
        (2, 10.0, true),
        (1, 10.0, false),
        (1, 10.0, false),
        (1, 10.0, false),
    ]);
    // Source-order caret at the LTR→RTL boundary (byte 3): logical
    // leading edge of the first RTL cluster = 30.0.
    assert_eq!(run.caret_x(3), 30.0);
    let lines = layout_text(
        &run,
        "abc\u{645}\u{631}def",
        f32::INFINITY,
        None,
        12.0,
        4.0,
        2.0,
    );
    // Visual caret at the same byte (forward affinity): 50.0 — the
    // flip magnitude at this boundary is 20px here (10px clusters;
    // ~65px in the real corpus with real advances).
    assert_eq!(lines[0].caret_x(3), 50.0);
}

#[test]
fn digit_island_in_rtl_run_stays_ltr_and_moves_as_unit() {
    // "ab" + U+0645 + "12": LTR pair, one RTL letter, two EN digits in
    // the same RTL run (the corpus tail in miniature).
    let run = synthetic_run(&[
        (1, 10.0, false),
        (1, 10.0, false),
        (2, 10.0, true),
        (1, 10.0, true),
        (1, 10.0, true),
    ]);
    let text = "ab\u{645}12";
    assert_eq!(text.len(), 6);
    let logical: Vec<usize> = (0..5).collect();
    let ordered = order_visual(&run, text, &logical);
    // Levels [0,0,1,2,2]: the island moves left of the RTL letter.
    let idx: Vec<usize> = ordered.iter().map(|o| o.index).collect();
    assert_eq!(idx, vec![0, 1, 3, 4, 2]);
    // Island clusters read LTR (caret at a digit start = left edge).
    assert!(!ordered[2].rtl && !ordered[3].rtl);
    assert!(ordered[4].rtl);
    let lines = layout_text(&run, text, f32::INFINITY, None, 12.0, 4.0, 2.0);
    assert_eq!(lines[0].caret_x(4), 20.0);
    // The RTL letter's slot is rightmost: forward caret at its start.
    assert_eq!(lines[0].caret_x(2), 50.0);
}

#[test]
fn wrap_breaks_at_cluster_boundaries_without_reshape() {
    // 8 clusters × 10px; avail 25 → lines of 2 (greedy: 20 used, third
    // would make 30 > 25).
    let run = synthetic_run(&[(1, 10.0, false); 8]);
    let lines = layout_text(&run, "abcdefgh", 25.0, None, 12.0, 4.0, 2.0);
    assert_eq!(lines.len(), 4);
    for line in &lines {
        assert_eq!(line.width, 20.0);
        assert_eq!(line.clusters.len(), 2);
    }
    // Line stacking: height 16 each + 2 gap.
    assert_eq!(lines[1].y, 18.0);
    // Over-wide single cluster stands alone (overflow, never split).
    let wide = synthetic_run(&[(1, 100.0, false), (1, 10.0, false)]);
    let lines = layout_text(&wide, "ab", 25.0, None, 12.0, 4.0, 2.0);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].width, 100.0);
}

#[test]
fn newline_splits_lines() {
    let run = synthetic_run(&[(1, 10.0, false), (1, 0.0, false), (1, 10.0, false)]);
    let lines = layout_text(&run, "a\nb", f32::INFINITY, None, 12.0, 4.0, 2.0);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].clusters.len(), 1);
    assert_eq!(lines[1].clusters.len(), 1);
}

#[test]
fn ellipsis_truncates_single_line_with_marker() {
    let run = synthetic_run(&[(1, 10.0, false); 8]);
    let lines = layout_text(&run, "abcdefgh", 25.0, Some(10.0), 12.0, 4.0, 2.0);
    assert_eq!(lines.len(), 1, "ellipsis forces single-line");
    // Budget 25-10=15 → one 10px cluster + marker.
    assert_eq!(lines[0].clusters.iter().filter(|c| !c.ellipsis).count(), 1);
    let marker = lines[0]
        .clusters
        .iter()
        .find(|c| c.ellipsis)
        .expect("marker");
    assert_eq!(marker.width, 10.0);
    assert_eq!(marker.byte_range, (8, 8));
    assert_eq!(lines[0].width, 20.0);
}

#[test]
fn empty_run_lays_no_lines() {
    let run = synthetic_run(&[]);
    let lines = layout_text(&run, "", f32::INFINITY, None, 12.0, 4.0, 2.0);
    assert!(lines.is_empty());
}

fn wrap8(breaks: &[usize], avail: f32) -> Vec<LaidLine> {
    // "aa bb cc": 8 one-byte clusters × 10px, spaces at bytes 2, 5.
    let run = synthetic_run(&[(1, 10.0, false); 8]);
    layout_text_with_breaks(&run, "aa bb cc", avail, None, 12.0, 4.0, 2.0, breaks)
}

#[test]
fn breaks_wrap_at_opportunities_only_with_trim() {
    // Opportunities after each space (3, 6); avail 45 fits one
    // "word " span (30) but never two (60).
    let lines = wrap8(&[3, 6], 45.0);
    assert_eq!(lines.len(), 3);
    let starts: Vec<Vec<usize>> = lines
        .iter()
        .map(|l| l.clusters.iter().map(|c| c.byte_range.0).collect())
        .collect();
    assert_eq!(starts, vec![vec![0, 1], vec![3, 4], vec![6, 7]]);
    // Trailing spaces trimmed at both soft breaks: width 20, not 30.
    assert_eq!(lines[0].width, 20.0);
    assert_eq!(lines[1].width, 20.0);
    // Paragraph-final line untrimmed (no trailing blank here anyway).
    assert_eq!(lines[2].width, 20.0);
    assert_eq!(lines[1].y, 18.0);
}

#[test]
fn forward_affinity_break_byte_reads_next_leading() {
    let lines = wrap8(&[3, 6], 45.0);
    let b = LayoutBox {
        lines,
        ..Default::default()
    };
    // Byte 2 (trimmed space) belongs to line 1's leading caret.
    assert_eq!(b.caret_position(2), (1, 0.0));
    // Byte 3 (line 1's first byte) stays on line 1.
    assert_eq!(b.caret_position(3), (1, 0.0));
    // Byte 5 (second trimmed space) belongs to line 2's leading caret.
    assert_eq!(b.caret_position(5), (2, 0.0));
    // Trailing caret: last line's end.
    assert_eq!(b.caret_position(8), (2, 20.0));
}

#[test]
fn overwide_span_pushes_whole_and_single_cluster_stands_alone() {
    // No opportunities: the whole paragraph is one atomic span --
    // over-wide it pushes whole onto a single overflowing line.
    let run = synthetic_run(&[(1, 10.0, false); 8]);
    let lines = layout_text_with_breaks(&run, "abcdefgh", 25.0, None, 12.0, 4.0, 2.0, &[]);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].width, 80.0);
    // One opportunity mid-word: spans "a" (100px, over-wide) + "b".
    let wide = synthetic_run(&[(1, 100.0, false), (1, 10.0, false)]);
    let lines = layout_text_with_breaks(&wide, "ab", 25.0, None, 12.0, 4.0, 2.0, &[1]);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].width, 100.0);
    assert_eq!(lines[1].width, 10.0);
}

#[test]
fn stray_offsets_never_split_clusters() {
    // Clusters (0,1),(1,3),(3,4): offset 2 is mid-cluster — inert.
    let run = synthetic_run(&[(1, 10.0, false), (2, 10.0, false), (1, 10.0, false)]);
    let text = "a\u{e9}b";
    let lines = layout_text_with_breaks(&run, text, 15.0, None, 12.0, 4.0, 2.0, &[2]);
    assert_eq!(lines.len(), 1, "over-wide span pushes whole, never splits");
    assert_eq!(lines[0].clusters.len(), 3);
}

#[test]
fn infinite_width_ignores_breaks() {
    let lines = wrap8(&[3, 6], f32::INFINITY);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].clusters.len(), 8);
}

#[test]
fn ellipsis_keeps_greedy_truncation() {
    // Opportunities would wrap, but ellipsis truncates single-line.
    let run = synthetic_run(&[(1, 10.0, false); 8]);
    let lines = layout_text_with_breaks(&run, "abcdefgh", 25.0, Some(10.0), 12.0, 4.0, 2.0, &[4]);
    assert_eq!(lines.len(), 1, "ellipsis forces single-line");
    assert_eq!(lines[0].clusters.iter().filter(|c| !c.ellipsis).count(), 1);
}

#[test]
fn gap_newlines_split_like_cluster_newlines() {
    // Stitched per-paragraph shaping (decision 196): no cluster
    // covers the '\n' -- the gap does. "a\nb" as stitched runs.
    let run = synthetic_run(&[(1, 10.0, false), (1, 10.0, false)]);
    let mut stitched = run;
    stitched.clusters[1].byte_range = (2, 3);
    stitched.text_len_bytes = 3;
    let lines = layout_text(&stitched, "a\nb", f32::INFINITY, None, 12.0, 4.0, 2.0);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].clusters[0].byte_range, (0, 1));
    assert_eq!(lines[1].clusters[0].byte_range, (2, 3));
}

#[test]
fn adjacent_newlines_yield_empty_middle_line() {
    // "a\n\nb": the gap holds two newlines → three paragraphs.
    let run = synthetic_run(&[(1, 10.0, false), (1, 10.0, false)]);
    let mut stitched = run;
    stitched.clusters[1].byte_range = (3, 4);
    stitched.text_len_bytes = 4;
    let lines = layout_text(&stitched, "a\n\nb", f32::INFINITY, None, 12.0, 4.0, 2.0);
    assert_eq!(lines.len(), 3);
    assert!(lines[1].clusters.is_empty());
}

/// Round 17.2 (decision 318): the brief formula pins exactly —
/// `h = max(24, viewport² / content)`, `y` linear in the
/// clamped offset over the travel.
#[test]
fn scrollbar_thumb_pins_the_brief_formula() {
    // 200px viewport over 600px content at rest.
    let t = scrollbar_thumb(200.0, 600.0, 0.0).expect("overflow thumbs");
    assert!(
        (t.h - 200.0 * 200.0 / 600.0).abs() < 1e-3,
        "h pins vp²/c, got {}",
        t.h
    );
    assert_eq!(t.y, 0.0, "rest parks at top");
    assert_eq!(scrollbar_max_offset(200.0, 600.0), 400.0);
    // Full travel: bottom parks the thumb at travel exactly.
    let full = scrollbar_thumb(200.0, 600.0, 400.0).expect("bottom thumb");
    assert!(
        (full.y - (200.0 - t.h)).abs() < 1e-3,
        "bottom parks at travel, got {}",
        full.y
    );
    // Halfway parks halfway.
    let mid = scrollbar_thumb(200.0, 600.0, 200.0).expect("mid thumb");
    assert!(
        (mid.y - (200.0 - t.h) / 2.0).abs() < 1e-3,
        "linear ratio, got {}",
        mid.y
    );
    // Min clamp: 200px over 2000px wants 20px, gets 24.
    let small = scrollbar_thumb(200.0, 2000.0, 0.0).expect("min thumb");
    assert_eq!(small.h, SCROLLBAR_MIN_THUMB_PX);
    // No overflow, degenerate, and clamped overscroll.
    assert_eq!(scrollbar_thumb(200.0, 200.0, 0.0), None);
    assert_eq!(scrollbar_thumb(200.0, 100.0, 0.0), None);
    assert_eq!(scrollbar_thumb(0.0, 600.0, 0.0), None);
    assert_eq!(
        scrollbar_thumb(200.0, 600.0, 9999.0),
        scrollbar_thumb(200.0, 600.0, 400.0),
        "overscroll clamps to max"
    );
    assert_eq!(
        scrollbar_thumb(200.0, 600.0, -50.0),
        scrollbar_thumb(200.0, 600.0, 0.0),
        "negative clamps to top"
    );
}

/// Phase 36 PR2b (decision 354): the horizontal twin pins the same
/// formula transposed — `w = max(24, viewport² / content)`, `x`
/// linear in the clamped offset over the travel.
#[test]
fn scrollbar_thumb_x_pins_the_transposed_formula() {
    let t = scrollbar_thumb_x(200.0, 600.0, 0.0).expect("overflow thumbs");
    assert!(
        (t.w - 200.0 * 200.0 / 600.0).abs() < 1e-3,
        "w pins vp²/c, got {}",
        t.w
    );
    assert_eq!(t.x, 0.0, "rest parks at left");
    let full = scrollbar_thumb_x(200.0, 600.0, 400.0).expect("right thumb");
    assert!(
        (full.x - (200.0 - t.w)).abs() < 1e-3,
        "right parks at travel, got {}",
        full.x
    );
    let mid = scrollbar_thumb_x(200.0, 600.0, 200.0).expect("mid thumb");
    assert!(
        (mid.x - (200.0 - t.w) / 2.0).abs() < 1e-3,
        "linear ratio, got {}",
        mid.x
    );
    assert_eq!(scrollbar_thumb_x(200.0, 200.0, 0.0), None);
    assert_eq!(scrollbar_thumb_x(200.0, 100.0, 0.0), None);
    assert_eq!(scrollbar_thumb_x(0.0, 600.0, 0.0), None);
}
