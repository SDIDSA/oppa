//! M3 bidi gate: the deferred visual-ordering item (locked #29's only open
//! freeze point) closes here. Shapes the spike corpus string
//! "abc مرحبا 123" (escapes per decision 45) through the real DirectWrite
//! backend, lays it out with `oppa::layout`, and asserts the per-boundary
//! visual carets against the canonical oracle (`IDWriteTextLayout` +
//! `HitTestTextPosition`, the same reference the spike's criterion 1 used).
//! The source-order caret math must show the recorded run-order-flip-class
//! divergence; the layout's visual carets must collapse it to ±2px.

#![cfg(windows)]

use oppa::layout::layout_text;
use oppa::text::{TextService, TextStyle};
use oppa_text_dwrite::DWriteTextService;
use windows::core::PCWSTR;
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_HIT_TEST_METRICS,
};

/// Corpus string: "abc " + U+0645 U+0631 U+062D U+0628 U+0627 + " 123".
const BIDI_TEXT: &str = "abc \u{645}\u{631}\u{62D}\u{628}\u{627} 123";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Oracle carets per UTF-16 unit (device px at DPR 1, text-origin-relative).
fn oracle_carets(text: &str) -> Vec<f32> {
    unsafe {
        let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).unwrap();
        let family_w = wide("Segoe UI");
        let locale_w = wide("en-US");
        let format = factory
            .CreateTextFormat(
                PCWSTR(family_w.as_ptr()),
                None::<&windows::Win32::Graphics::DirectWrite::IDWriteFontCollection>,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                16.0,
                PCWSTR(locale_w.as_ptr()),
            )
            .unwrap();
        let text_w: Vec<u16> = text.encode_utf16().collect();
        let layout = factory
            .CreateTextLayout(&text_w, &format, 1.0e6, 1.0e6)
            .unwrap();
        let mut out = Vec::with_capacity(text_w.len() + 1);
        for unit in 0..=(text_w.len() as u32) {
            let mut ox = 0.0f32;
            let mut oy = 0.0f32;
            let mut metrics = DWRITE_HIT_TEST_METRICS::default();
            layout
                .HitTestTextPosition(unit, false, &mut ox, &mut oy, &mut metrics)
                .unwrap();
            out.push(ox);
        }
        out
    }
}

fn utf16_before(text: &str, byte: usize) -> u32 {
    text[..byte.min(text.len())].encode_utf16().count() as u32
}

#[test]
fn bidi_source_order_diverges_and_layout_collapses_it() {
    assert_eq!(BIDI_TEXT.len(), 18, "5 Arabic chars × 2 bytes + 8 Latin");
    let service = DWriteTextService::new().expect("factory");
    let mut style = TextStyle::new("Segoe UI", 16.0);
    style.device_pixel_ratio = 1.0;
    let run = service.shape(BIDI_TEXT, &style).expect("shape bidi");
    assert_eq!(run.clusters.len(), 13, "corpus rig v2 cluster count");
    assert_eq!(run.runs.len(), 2, "one LTR head, one RTL tail");
    assert_eq!(run.runs[0].byte_range, (0, 4));
    assert!(!run.runs[0].rtl);
    // DirectWrite resolves the whole tail — Arabic + space + digits — as
    // one RTL run (the engine's level-2 island recovers the digit order).
    assert_eq!(run.runs[1].byte_range, (4, 18));
    assert!(run.runs[1].rtl);
    // Glyph storage is logical-order (monotonic 1:1 here — the mirror
    // assumption the engine lays out under).
    for (k, c) in run.clusters.iter().enumerate() {
        assert_eq!(c.glyph_range, (k, k + 1), "cluster {k}");
    }

    let oracle = oracle_carets(BIDI_TEXT);
    // Cluster-boundary bytes (logical), including the trailing caret.
    let mut boundaries: Vec<usize> = run.clusters.iter().map(|c| c.byte_range.0).collect();
    boundaries.push(run.text_len_bytes);

    // (a) Source-order caret math diverges from the oracle at bidi
    // boundaries — the recorded run-order-flip class (65px DPR1 in the
    // spike; asserted here as Redis-class magnitude, not the exact font-
    // version-sensitive number).
    let mut worst_source = 0.0f32;
    let mut worst_at = 0usize;
    for &b in &boundaries {
        let mine = run.caret_x(b);
        let theirs = oracle[utf16_before(BIDI_TEXT, b) as usize];
        let d = (mine - theirs).abs();
        if d > worst_source {
            worst_source = d;
            worst_at = b;
        }
    }
    println!("worst source-order divergence: {worst_source:.2}px at byte {worst_at}");
    assert!(
        worst_source > 25.0,
        "the test must reproduce a run-order flip (>{}px), got {worst_source:.2}px at byte {worst_at}",
        25.0
    );

    // (b) The layout engine's visual carets collapse it to the ±2px
    // criterion-1 tolerance at every cluster boundary.
    let metrics = service.measure_line(&run);
    let lines = layout_text(
        &run,
        BIDI_TEXT,
        f32::INFINITY,
        None,
        metrics.ascent,
        metrics.descent,
        metrics.line_gap,
    );
    assert_eq!(lines.len(), 1, "unconstrained bidi line stays single");
    // Width preserved by the reorder (reorder, not re-measure).
    assert!(
        (lines[0].width - run.total_advance).abs() < 1e-3,
        "visual width {} vs shaped {}",
        lines[0].width,
        run.total_advance
    );
    let mut worst_visual = 0.0f32;
    for &b in &boundaries {
        let mine = lines[0].caret_x(b);
        let theirs = oracle[utf16_before(BIDI_TEXT, b) as usize];
        let d = (mine - theirs).abs();
        println!(
            "byte {b:2}: layout {mine:8.3} oracle {theirs:8.3} source {:8.3}",
            run.caret_x(b)
        );
        if d > worst_visual {
            worst_visual = d;
        }
        assert!(
            d <= 2.0,
            "visual caret at byte {b}: layout {mine:.3} vs oracle {theirs:.3}"
        );
    }
    println!("worst visual residual: {worst_visual:.3}px (was {worst_source:.2}px source-order)");
}
