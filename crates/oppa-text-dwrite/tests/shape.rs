//! M0b proofs against the real DirectWrite system fonts: glyph counts and
//! advances hand-checked for Segoe UI, multi-byte cluster byte mapping
//! (UTF-16 code-unit ↔ UTF-8 byte offsets), caret round-trips, DPR scaling.

#![cfg(windows)]

use oppa::text::{
    round_to_device_px, FontMetrics, FontStyle, ShapedGlyph, ShapedRun, TextError, TextService,
    TextStyle,
};
use oppa_text_dwrite::DWriteTextService;

fn segoe(size_px: f32, dpr: f32) -> TextStyle {
    let mut style = TextStyle::new("Segoe UI", size_px);
    style.device_pixel_ratio = dpr;
    style
}

fn shape_ok(text: &str, style: &TextStyle) -> ShapedRun {
    let service = DWriteTextService::new().expect("DirectWrite factory");
    service.shape(text, style).expect("shape")
}

#[test]
fn shape_of_known_string_has_expected_glyph_count_and_advances() {
    let run = shape_ok("Hi", &segoe(16.0, 1.0));
    assert_eq!(run.glyph_count(), 2, "Hi shapes to exactly two glyphs");
    assert!(
        run.glyphs.iter().all(|g| g.x_advance > 0.0),
        "all advances positive"
    );
    assert!(run.runs[0].font_metrics.ascent > 0.0);
    assert!(run.runs[0].font_metrics.descent > 0.0);
    // Deterministic: the same input produces the identical run.
    let again = shape_ok("Hi", &segoe(16.0, 1.0));
    assert_eq!(run, again, "shaping is deterministic");
}

#[test]
fn run_width_equals_sum_of_advances() {
    let run = shape_ok("Width", &segoe(16.0, 1.0));
    let sum: f32 = run.glyphs.iter().map(|g| g.x_advance).sum();
    assert!(
        (run.total_advance - sum).abs() < 1e-4,
        "width is the advance sum"
    );
    let measured = {
        let service = DWriteTextService::new().unwrap();
        service.measure_line(&run)
    };
    assert!((measured.width - run.total_advance).abs() < 1e-4);
    assert!(measured.ascent > 0.0 && measured.descent > 0.0);
}

#[test]
fn uppercase_glyphs_are_wider_than_lowercase_i() {
    // Hand-checked for Segoe UI: 'W' advances notably more than 'i'.
    let w = shape_ok("W", &segoe(16.0, 1.0));
    let i = shape_ok("i", &segoe(16.0, 1.0));
    assert!(
        w.total_advance > i.total_advance * 3.0,
        "'W' ({}) vs 'i' ({})",
        w.total_advance,
        i.total_advance
    );
}

#[test]
fn accented_multibyte_cluster_byte_map_round_trips() {
    // "héllo": é is 2 UTF-8 bytes / 1 UTF-16 unit.
    let run = shape_ok("héllo", &segoe(16.0, 1.0));
    assert_eq!(run.glyph_count(), 5);
    let cluster_starts: Vec<usize> = run.clusters.iter().map(|c| c.byte_range.0).collect();
    assert_eq!(
        cluster_starts,
        vec![0, 1, 3, 4, 5],
        "é's cluster starts at byte 1"
    );
    // Every byte maps to its cluster's glyph; bytes inside é snap to it.
    for (byte, expected_glyph) in [(0usize, 0usize), (1, 1), (2, 1), (3, 2), (4, 3), (5, 4)] {
        assert_eq!(
            run.glyph_index_for_byte_offset(byte),
            Some(expected_glyph),
            "byte {byte}"
        );
    }
    // Glyph → byte: é's glyph resolves to its cluster start.
    assert_eq!(run.byte_offset_for_glyph_index(1), Some(1));
}

#[test]
fn cjk_fallback_shapes_through_the_system_fallback() {
    // Base family Segoe UI has no CJK coverage; MapCharacters must map the
    // run to a fallback face and the byte map must stay intact.
    let run = shape_ok("日本語", &segoe(16.0, 1.0));
    assert_eq!(run.glyph_count(), 3);
    let cluster_starts: Vec<usize> = run.clusters.iter().map(|c| c.byte_range.0).collect();
    assert_eq!(
        cluster_starts,
        vec![0, 3, 6],
        "each CJK char is 3 UTF-8 bytes"
    );
    assert!(run.glyphs.iter().all(|g| g.x_advance > 0.0));
    for byte in [0usize, 1, 2, 3, 4, 5, 6, 7, 8] {
        let expected = byte / 3;
        assert_eq!(
            run.glyph_index_for_byte_offset(byte),
            Some(expected),
            "byte {byte} in char {expected}"
        );
    }
}

#[test]
fn surrogate_pair_emoji_is_one_cluster() {
    // "👍" is 4 UTF-8 bytes, 2 UTF-16 code units, 1 glyph (or a ZWJ-class
    // cluster); no byte inside the pair may split it.
    let run = shape_ok("👍", &segoe(16.0, 1.0));
    assert_eq!(run.clusters.len(), 1, "the emoji is exactly one cluster");
    assert_eq!(run.clusters[0].byte_range, (0, 4));
    for byte in 0..4 {
        assert_eq!(
            run.glyph_index_for_byte_offset(byte),
            Some(0),
            "byte {byte}"
        );
    }
    assert_eq!(run.caret_x(4), run.total_advance);
}

#[test]
fn caret_positions_round_trip_at_cluster_boundaries() {
    let run = shape_ok("héllo", &segoe(16.0, 1.0));
    // Monotone non-decreasing across bytes.
    let xs: Vec<f32> = (0..=5usize).map(|b| run.caret_x(b)).collect();
    for pair in xs.windows(2) {
        assert!(pair[1] >= pair[0], "caret x monotone");
    }
    // Round-trip at every cluster boundary byte.
    for byte in [0usize, 1, 3, 4, 5] {
        assert_eq!(
            run.byte_offset_for_x(run.caret_x(byte)),
            byte,
            "round-trip byte {byte}"
        );
    }
    // Trailing caret: x == run width resolves past the last cluster.
    assert_eq!(run.byte_offset_for_x(run.total_advance), 6);
    // Clicking the middle of a cluster snaps to a cluster boundary byte.
    let mid = run.caret_x(1) + (run.caret_x(3) - run.caret_x(1)) * 0.75;
    let snapped = run.byte_offset_for_x(mid);
    assert!(
        [1usize, 3].contains(&snapped),
        "mid-cluster x {mid} resolves to a cluster edge, got {snapped}"
    );
}

#[test]
fn device_pixel_ratio_scales_every_advance() {
    let one = shape_ok("Hi", &segoe(16.0, 1.0));
    let two = shape_ok("Hi", &segoe(16.0, 2.0));
    for (a, b) in one.glyphs.iter().zip(&two.glyphs) {
        let ratio = b.x_advance / a.x_advance;
        assert!((ratio - 2.0).abs() < 1e-3, "advance ratio {ratio}");
    }
    assert!((two.total_advance - one.total_advance * 2.0).abs() < 1e-3);
    // Shared rounding rule snaps to the device grid: 13.37*2 = 26.74 -> 27 -> 13.5.
    assert_eq!(round_to_device_px(13.37, 2.0), 13.5);
}

#[test]
fn letter_spacing_extends_inter_glyph_gaps_not_the_last_glyph() {
    let plain_style = segoe(16.0, 1.0);
    let mut spaced_style = segoe(16.0, 1.0);
    spaced_style.letter_spacing_px = 2.0;
    let plain_run = shape_ok("abcd", &plain_style);
    let spaced_run = shape_ok("abcd", &spaced_style);
    for (i, (pa, pb)) in plain_run.glyphs.iter().zip(&spaced_run.glyphs).enumerate() {
        let expected_extra = if i + 1 == plain_run.glyphs.len() {
            0.0
        } else {
            2.0
        };
        assert!(
            (pb.x_advance - pa.x_advance - expected_extra).abs() < 1e-4,
            "glyph {i}: {} vs {}",
            pa.x_advance,
            pb.x_advance
        );
    }
    // Trailing caret = after three widened gaps (plain width + 3*spacing).
    assert!(
        (spaced_run.caret_x(4) - (plain_run.caret_x(4) + 3.0 * 2.0)).abs() < 1e-3,
        "trailing caret {} vs {}",
        spaced_run.caret_x(4),
        plain_run.caret_x(4)
    );
}

#[test]
fn enumerate_fonts_includes_segoe_ui() {
    let service = DWriteTextService::new().expect("DirectWrite factory");
    let fonts = service.enumerate_fonts();
    assert!(!fonts.is_empty());
    assert!(
        fonts.iter().any(|f| f.family == "Segoe UI"),
        "system font enumeration should contain Segoe UI"
    );
}

#[test]
fn unknown_family_fails_loudly() {
    let service = DWriteTextService::new().unwrap();
    let err = service
        .shape("Hi", &TextStyle::new("No Such Family Anywhere", 16.0))
        .expect_err("unknown family must error");
    assert!(matches!(err, TextError::FontNotFound(_)));
}

#[test]
fn empty_text_refuses_to_shape() {
    let service = DWriteTextService::new().unwrap();
    let err = service
        .shape("", &segoe(16.0, 1.0))
        .expect_err("empty text");
    assert!(matches!(err, TextError::EmptyText));
}

#[test]
fn font_style_axis_is_honored_in_enumeration_shape() {
    let service = DWriteTextService::new().unwrap();
    let mut italic = segoe(16.0, 1.0);
    italic.style = FontStyle::Italic;
    let run = service.shape("Hi", &italic).expect("italic shape");
    assert_eq!(run.glyph_count(), 2);
}

#[allow(dead_code)]
fn unused_helpers() {
    let _ = FontMetrics {
        ascent: 0.0,
        descent: 0.0,
        line_gap: 0.0,
    };
    let _: Option<ShapedGlyph> = None;
}

// DESIGN §2.3(b) corpus pins: the rtl flag M0b built gets its first
// assertion, and the combining/ZWJ cluster shapes the arms measured
// are pinned at unit level. All strings ASCII-escaped so the forms
// are unambiguous in source.

#[test]
fn bidi_arabic_span_carries_rtl_flag() {
    // "abc " + U+0645 U+0631 U+062D U+0628 U+0627 + " 123": the Arabic
    // bytes are 4..14. Every one must sit in an rtl run (SetBidiLevel
    // resolved-level odd), Latin runs must not.
    let run = shape_ok(
        "abc \u{645}\u{631}\u{62d}\u{628}\u{627} 123",
        &segoe(16.0, 1.0),
    );
    for b in 4..14 {
        assert!(
            run.runs
                .iter()
                .any(|r| r.rtl && r.byte_range.0 <= b && b < r.byte_range.1),
            "Arabic byte {b} sits in an rtl run"
        );
    }
    assert!(run.runs.iter().any(|r| !r.rtl), "Latin runs stay LTR");
}

#[test]
fn decomposed_combining_acute_merges_one_cluster() {
    // "cafe" + U+0301: e (byte 3..4) + combining mark (4..6) shape as a
    // single cluster (3,6): identical granularity to precomposed e-acute.
    let run = shape_ok("cafe\u{301}", &segoe(16.0, 1.0));
    assert!(
        run.clusters.iter().any(|c| c.byte_range == (3, 6)),
        "e + combining acute is one cluster, got {:?}",
        run.clusters
            .iter()
            .map(|c| c.byte_range)
            .collect::<Vec<_>>()
    );
}

#[test]
fn zwj_emoji_sequence_is_one_cluster() {
    // "a" + U+1F469 U+200D U+1F4BB + "b": the 11-byte emoji span
    // (bytes 1..12) shapes as a single cluster.
    let run = shape_ok("a\u{1f469}\u{200d}\u{1f4bb}b", &segoe(16.0, 1.0));
    assert!(
        run.clusters.iter().any(|c| c.byte_range == (1, 12)),
        "ZWJ sequence is one cluster, got {:?}",
        run.clusters
            .iter()
            .map(|c| c.byte_range)
            .collect::<Vec<_>>()
    );
}

#[test]
fn recorded_ids_cover_bold_and_share_across_clones() {
    // Round 7.2: the record starts empty; shaping records the
    // regular id; a bold shape records a second, weight-distinct
    // id (Segoe UI ships both faces - the bars' root cause); a
    // clone shares the record (the runner keeps one beside the
    // host-owned service).
    use oppa::text::FontWeight;
    let service = DWriteTextService::new().expect("DirectWrite factory");
    assert!(service.recorded_font_ids().is_empty(), "nothing shaped yet");
    let regular = service.shape("Ag", &segoe(16.0, 1.0)).expect("shapes");
    let regular_id = regular.runs[0].font_id;
    assert_eq!(service.recorded_font_ids(), vec![regular_id]);
    assert!(
        service.font_file_source(regular_id).is_some(),
        "regular resolves"
    );
    let mut bold_style = segoe(16.0, 1.0);
    bold_style.weight = FontWeight::BOLD;
    let bold = service.shape("Ag", &bold_style).expect("shapes bold");
    let bold_id = bold.runs[0].font_id;
    assert_ne!(bold_id, regular_id, "bold is weight-distinct");
    assert_eq!(
        service.recorded_font_ids(),
        vec![regular_id.min(bold_id), regular_id.max(bold_id)]
    );
    assert!(service.font_file_source(bold_id).is_some(), "bold resolves");
    // Clone shares: shaping through the clone lands in the same record.
    let twin = service.clone();
    let mut italic_style = segoe(16.0, 1.0);
    italic_style.style = FontStyle::Italic;
    let italic = twin.shape("Ag", &italic_style).ok();
    match italic {
        Some(run) => {
            let italic_id = run.runs[0].font_id;
            assert!(
                service.recorded_font_ids().contains(&italic_id),
                "shared record"
            );
            assert!(
                twin.recorded_font_ids().contains(&regular_id),
                "shared both ways"
            );
        }
        None => {
            // Segoe UI may lack a true italic face on minimal
            // installs - refusal is loud, never silent tofu.
            assert!(service.recorded_font_ids().contains(&regular_id));
        }
    }
}

/// Preexisting-bug fix (the sink crashed shaping a TextArea holding
/// "1456\n"): control-only runs shape as zero-advance glyphless
/// clusters instead of refusing — editable text with newlines must
/// never crash the app.
#[test]
fn control_runs_shape_as_zero_advance_clusters() {
    let plain = shape_ok("1456", &segoe(16.0, 1.0));
    let run = shape_ok("1456\n", &segoe(16.0, 1.0));
    assert!(
        (run.total_advance - plain.total_advance).abs() < 1e-4,
        "trailing newline adds no width, {} vs {}",
        run.total_advance,
        plain.total_advance
    );
    assert_eq!(run.text_len_bytes, 5);
    assert_eq!(
        run.glyph_count(),
        plain.glyph_count(),
        "no glyphs added for the newline"
    );
    let last = run.clusters.last().expect("newline cluster");
    assert_eq!(
        last.byte_range,
        (4, 5),
        "newline bytes covered, got {:?}",
        last.byte_range
    );
    assert_eq!(
        last.glyph_range.0, last.glyph_range.1,
        "no glyphs for controls"
    );
    // Carets sit at the visible edge on both sides of the newline.
    assert!((run.caret_x(4) - plain.total_advance).abs() < 1e-4);
    assert!((run.caret_x(5) - plain.total_advance).abs() < 1e-4);
    // Hit-testing past the text still resolves to the end.
    assert_eq!(run.byte_offset_for_x(run.total_advance + 50.0), 5);
}

#[test]
fn lone_newline_shapes_to_zero_size() {
    let run = shape_ok("\n", &segoe(16.0, 1.0));
    assert_eq!(run.total_advance, 0.0);
    assert_eq!(run.glyph_count(), 0);
    assert_eq!(run.text_len_bytes, 1);
    assert_eq!(run.clusters.len(), 1);
    assert_eq!(run.clusters[0].byte_range, (0, 1));
}

#[test]
fn crlf_pair_shapes_as_zero_advance_clusters() {
    let a = shape_ok("a", &segoe(16.0, 1.0));
    let b = shape_ok("b", &segoe(16.0, 1.0));
    let run = shape_ok("a\r\nb", &segoe(16.0, 1.0));
    assert!(
        (run.total_advance - (a.total_advance + b.total_advance)).abs() < 1e-4,
        "crlf adds no width, got {}",
        run.total_advance
    );
    // One cluster per control unit (the split follows the fallback's
    // own run lines — \r maps to a font, \n to none — both zeroed).
    let ranges: Vec<(usize, usize)> = run.clusters.iter().map(|c| c.byte_range).collect();
    assert_eq!(
        ranges,
        vec![(0, 1), (1, 2), (2, 3), (3, 4)],
        "got {ranges:?}"
    );
    assert_eq!(run.glyph_count(), 2, "no glyphs for controls");
    assert!((run.caret_x(1) - a.total_advance).abs() < 1e-4);
    assert!((run.caret_x(2) - a.total_advance).abs() < 1e-4);
    assert!((run.caret_x(3) - a.total_advance).abs() < 1e-4);
}
