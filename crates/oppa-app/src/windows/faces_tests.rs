use super::*;
use oppa::text::{FontWeight, TextService, TextStyle};

/// Round 7.2: injection covers every recorded id (regular +
/// bold), memoizes repeats, and leaves the loop paintable.
/// Real DirectWrite (this box shapes Segoe UI headlessly —
/// the win_ime tests already rely on real Win32 windows).
#[test]
fn recorded_faces_inject_covering_bold() {
    let dwrite = DWriteTextService::new().expect("DirectWrite present");
    let regular = dwrite
        .shape("Ag", &TextStyle::new("Segoe UI", 16.0))
        .expect("shapes");
    let mut bold_style = TextStyle::new("Segoe UI", 16.0);
    bold_style.weight = FontWeight::BOLD;
    let bold = dwrite.shape("Ag", &bold_style).expect("shapes bold");
    assert_ne!(
        regular.runs[0].font_id, bold.runs[0].font_id,
        "probe spans two ids"
    );
    let mut loop_ = DesktopLoop::new(
        200,
        150,
        Box::new(DWriteTextService::new().expect("DirectWrite present")),
        "Segoe UI",
    )
    .expect("headless loop builds");
    let mut done = HashSet::new();
    let fresh = inject_recorded_faces(&mut loop_, &dwrite, &mut done);
    assert_eq!(fresh.len(), 2, "regular + bold inject, got {fresh:?}");
    assert!(done.contains(&regular.runs[0].font_id));
    assert!(done.contains(&bold.runs[0].font_id));
    assert!(
        inject_recorded_faces(&mut loop_, &dwrite, &mut done).is_empty(),
        "repeats memoize (no file IO twice)"
    );
    loop_.repaint().expect("loop still paints");
}

/// Round 7.9: the Select chevron ("▾" U+25BE) shapes through
/// DirectWrite (fallback-covered); "✓" (U+2713) shapes too (kept
/// as backend coverage although the Checkbox check went vector
/// in Round 7.15 — no control depends on it anymore).
/// Real DirectWrite, headless like the test above.
#[test]
fn check_and_chevron_glyphs_shape() {
    let dwrite = DWriteTextService::new().expect("DirectWrite present");
    for glyph in ["✓", "▾"] {
        let run = dwrite
            .shape(glyph, &TextStyle::new("Segoe UI", 14.0))
            .unwrap_or_else(|e| panic!("shapes {glyph:?}: {e:?}"));
        assert!(
            !run.glyphs.is_empty(),
            "{glyph:?} shapes no glyphs (tofu downstream)"
        );
    }
}
