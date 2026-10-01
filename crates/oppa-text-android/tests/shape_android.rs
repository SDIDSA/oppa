//! Android TextService acceptance (Gap 2): hand-checked shaping
//! over the emulator's own font files (rustybuzz), mirroring the
//! DirectWrite suite's rows where the contract is shared.
//!
//! Machine-local asset: `test-fonts/` holds byte copies pulled from
//! the emulator. Missing dir fails loudly with the pull commands:
//! for each of Roboto-Regular.ttf, NotoNaskhArabic-Regular.ttf,
//! NotoSansCJK-Regular.ttc, NotoColorEmoji.ttf run
//! `adb exec-out cat /system/fonts/<f> > crates/oppa-text-android/test-fonts/<f>`
//! (binary-safe: never `adb shell cat` with a shell redirect).
//!
//! Non-ASCII corpus entries are `\u{...}` escapes (decision 45).

use oppa::text::{TextService, TextStyle};
use oppa_text_android::AndroidTextService;
use std::path::PathBuf;

fn font_dir() -> PathBuf {
    if let Ok(d) = std::env::var("OPPA_ANDROID_FONTS") {
        return PathBuf::from(d);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-fonts")
}

fn service() -> AndroidTextService {
    let dir = font_dir();
    if !dir.join("Roboto-Regular.ttf").exists() {
        panic!(
            "android font asset missing in {}: pull the emulator fonts first — \
             adb exec-out cat /system/fonts/Roboto-Regular.ttf > {}/Roboto-Regular.ttf \
             (same for NotoNaskhArabic-Regular.ttf, NotoSansCJK-Regular.ttc, NotoColorEmoji.ttf)",
            dir.display(),
            dir.display()
        );
    }
    let (svc, skipped) = AndroidTextService::from_dir(&dir).expect("android font dir loads");
    assert!(skipped.is_empty(), "unexpected skipped faces: {skipped:?}");
    svc
}

fn roboto16() -> TextStyle {
    TextStyle::new("Roboto", 16.0)
}

// "Hi": exactly 2 glyphs, positive advances, deterministic.
#[test]
fn hi_shapes_twice_identical() {
    let svc = service();
    let a = svc.shape("Hi", &roboto16()).expect("shape Hi");
    let b = svc.shape("Hi", &roboto16()).expect("re-shape Hi");
    assert_eq!(a.glyphs.len(), 2);
    assert!(a.glyphs.iter().all(|g| g.x_advance > 0.0));
    assert_eq!(a, b);
}

// Width == sum of advances; measure_line agrees; metrics positive.
#[test]
fn width_is_sum_of_advances() {
    let svc = service();
    let run = svc.shape("Hello world", &roboto16()).expect("shape");
    let sum: f32 = run.glyphs.iter().map(|g| g.x_advance).sum();
    assert!((run.total_advance - sum).abs() < 1e-4);
    let measured = svc.measure_line(&run);
    assert!((measured.width - run.total_advance).abs() < 1e-4);
    assert!(measured.ascent > 0.0 && measured.descent > 0.0);
}

// Hand-checked Roboto relation: 'W' advances > 3x 'i'.
#[test]
fn wide_narrow_relation() {
    let svc = service();
    let w = svc.shape("W", &roboto16()).expect("shape W");
    let i = svc.shape("i", &roboto16()).expect("shape i");
    assert!(w.glyphs[0].x_advance > 3.0 * i.glyphs[0].x_advance);
}

// "hello" with e-acute U+00E9: 5 glyphs, cluster byte starts
// [0, 1, 3, 4, 5]; mid-cluster snap; glyph-to-byte is the start.
#[test]
fn e_acute_clusters() {
    let svc = service();
    let run = svc.shape("h\u{E9}llo", &roboto16()).expect("shape hello");
    assert_eq!(run.glyphs.len(), 5);
    let starts: Vec<usize> = run.clusters.iter().map(|c| c.byte_range.0).collect();
    assert_eq!(starts, vec![0, 1, 3, 4, 5]);
    assert_eq!(run.glyph_index_for_byte_offset(2), Some(1));
    assert_eq!(run.byte_offset_for_glyph_index(1), Some(1));
}

// CJK trio (U+65E5 U+672C U+8A9E) through system fallback: 3
// glyphs, byte starts [0, 3, 6], every byte maps to its glyph.
#[test]
fn cjk_fallback_clusters() {
    let svc = service();
    let text = "\u{65E5}\u{672C}\u{8A9E}";
    let run = svc.shape(text, &roboto16()).expect("shape CJK");
    assert_eq!(run.glyphs.len(), 3);
    let starts: Vec<usize> = run.clusters.iter().map(|c| c.byte_range.0).collect();
    assert_eq!(starts, vec![0, 3, 6]);
    for byte in 0..text.len() {
        let glyph = run.glyph_index_for_byte_offset(byte).expect("maps");
        assert_eq!(glyph, byte / 3);
    }
    // Fallback face is CJK, not Roboto.
    assert_eq!(run.runs.len(), 1);
    assert_eq!(run.runs[0].script, 500);
}

// Thumbs-up U+1F44D: exactly 1 cluster over all 4 bytes.
#[test]
fn emoji_single_cluster() {
    let svc = service();
    let run = svc.shape("\u{1F44D}", &roboto16()).expect("shape emoji");
    assert_eq!(run.clusters.len(), 1);
    assert_eq!(run.clusters[0].byte_range, (0, 4));
    for byte in 0..4 {
        assert_eq!(run.glyph_index_for_byte_offset(byte), Some(0));
    }
}

// Caret math: monotone, boundary round-trips, mid-cluster snaps to
// edges, trailing caret == run width.
#[test]
fn caret_round_trips() {
    let svc = service();
    let run = svc.shape("h\u{E9}llo", &roboto16()).expect("shape hello");
    let mut prev = f32::NEG_INFINITY;
    for cluster in &run.clusters {
        let x = run.caret_x(cluster.byte_range.0);
        assert!(x >= prev);
        prev = x;
        assert_eq!(run.byte_offset_for_x(x + 0.25), cluster.byte_range.0);
    }
    assert_eq!(run.caret_x(run.text_len_bytes), run.total_advance);
    assert_eq!(run.glyph_index_for_byte_offset(2), Some(1));
}

// DPR 2 doubles every advance and the total width.
#[test]
fn dpr_scales_advances() {
    let svc = service();
    let run1 = svc.shape("Hi", &roboto16()).expect("shape dpr1");
    let mut style2 = roboto16();
    style2.device_pixel_ratio = 2.0;
    let run2 = svc.shape("Hi", &style2).expect("shape dpr2");
    assert_eq!(run1.glyphs.len(), run2.glyphs.len());
    for (a, b) in run1.glyphs.iter().zip(run2.glyphs.iter()) {
        assert!((b.x_advance - 2.0 * a.x_advance).abs() < 1e-4);
    }
    assert!((run2.total_advance - 2.0 * run1.total_advance).abs() < 1e-3);
}

// Letter tracking widens every inter-glyph advance except the last;
// trailing caret == plain width + (n-1) x spacing.
#[test]
fn letter_tracking_rule() {
    let svc = service();
    let plain = svc.shape("Hi", &roboto16()).expect("plain");
    let mut tracked = roboto16();
    tracked.letter_spacing_px = 2.0;
    let run = svc.shape("Hi", &tracked).expect("tracked");
    assert!((run.glyphs[0].x_advance - (plain.glyphs[0].x_advance + 2.0)).abs() < 1e-4);
    assert!((run.glyphs[1].x_advance - plain.glyphs[1].x_advance).abs() < 1e-4);
    let n = run.glyphs.len() as f32;
    assert!((run.total_advance - (plain.total_advance + (n - 1.0) * 2.0)).abs() < 1e-3);
    assert_eq!(run.caret_x(run.text_len_bytes), run.total_advance);
}

// Enumeration is non-empty and contains Roboto.
#[test]
fn enumerate_has_roboto() {
    let svc = service();
    let fonts = svc.enumerate_fonts();
    assert!(!fonts.is_empty());
    assert!(fonts.iter().any(|f| f.family == "Roboto"));
}

// Loud errors: unknown family, empty text.
#[test]
fn loud_family_and_empty() {
    let svc = service();
    let err = svc
        .shape("Hi", &TextStyle::new("No Such Family XYZ", 16.0))
        .expect_err("unknown family is loud");
    assert!(matches!(err, oppa::text::TextError::FontNotFound(_)));
    let err = svc.shape("", &roboto16()).expect_err("empty text is loud");
    assert!(matches!(err, oppa::text::TextError::EmptyText));
}

// Mixed Latin+CJK: 3 runs (Latin/Roboto, Han/CJK, Latin/Roboto)
// with scripts [215, 500, 215].
#[test]
fn mixed_latin_cjk_runs() {
    let svc = service();
    let run = svc
        .shape("Hi \u{4F60}\u{597D}world", &roboto16())
        .expect("shape mixed");
    assert_eq!(run.runs.len(), 3);
    assert_eq!(
        run.runs.iter().map(|r| r.script).collect::<Vec<_>>(),
        vec![215, 500, 215]
    );
    assert!(!run.runs[0].rtl && !run.runs[1].rtl);
    // Run byte ranges partition the text.
    assert_eq!(run.runs[0].byte_range.0, 0);
    assert_eq!(run.runs[2].byte_range.1, run.text_len_bytes);
    // Roboto on the Latin runs, CJK face in the middle.
    let families: Vec<String> = svc
        .enumerate_fonts()
        .iter()
        .map(|f| f.family.clone())
        .collect();
    let fam_of = |id: oppa::text::FontId| families[(id.0 / 4096) as usize].clone();
    assert_eq!(fam_of(run.runs[0].font_id), "Roboto");
    assert!(fam_of(run.runs[1].font_id).starts_with("Noto Sans CJK"));
    assert_eq!(fam_of(run.runs[2].font_id), "Roboto");
}

// Arabic greeting (U+645 U+631 U+62D U+628 U+627): one RTL run,
// script 160, one cluster per character, in LOGICAL byte order with
// partitioning ranges (decision 196: the shared core used to emit
// visual-ordered zero-length ranges here — counts held, ranges did
// not; the item-2 Arabic proof caught it).
#[test]
fn arabic_rtl_run() {
    let svc = service();
    let run = svc
        .shape("\u{645}\u{631}\u{62D}\u{628}\u{627}", &roboto16())
        .expect("shape arabic");
    assert_eq!(run.runs.len(), 1);
    assert!(run.runs[0].rtl);
    assert_eq!(run.runs[0].script, 160);
    assert_eq!(run.clusters.len(), 5);
    let ranges: Vec<(usize, usize)> = run.clusters.iter().map(|c| c.byte_range).collect();
    assert_eq!(ranges, vec![(0, 2), (2, 4), (4, 6), (6, 8), (8, 10)]);
}

// Shaping cost honesty: the whole corpus well under 5 s (faces
// parse per shape call in v1 — this keeps that rule measured).
#[test]
fn corpus_timing_is_bounded() {
    use std::time::Instant;
    let svc = service();
    let corpus = [
        "Hi",
        "Hello world",
        "h\u{E9}llo",
        "\u{65E5}\u{672C}\u{8A9E}",
        "Hi \u{4F60}\u{597D}world",
        "\u{645}\u{631}\u{62D}\u{628}\u{627}",
        "\u{1F44D}",
        "W",
    ];
    let t = Instant::now();
    for text in corpus {
        svc.shape(text, &roboto16()).expect("corpus shapes");
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("android text corpus: {ms:.1} ms for {}", corpus.len());
    assert!(ms < 5000.0, "corpus shaping took {ms:.1} ms");
}

/// Canonical reference lines for the on-device validation: one per
/// corpus string, `text | glyphs=id:advance ... | total`. The app
/// shapes the same corpus through `/system/fonts` and writes the
/// same lines; the device-validation test below asserts equality
/// (same font bytes, same shaper — cross-ISA float drift would
/// show here, not hide).
pub fn reference_lines(svc: &AndroidTextService) -> Vec<String> {
    let corpus = [
        "Hi",
        "Hello world",
        "h\u{E9}llo",
        "\u{65E5}\u{672C}\u{8A9E}",
        "Hi \u{4F60}\u{597D}world",
        "\u{645}\u{631}\u{62D}\u{628}\u{627}",
        "\u{1F44D}",
        "W",
        "AV",
    ];
    corpus
        .iter()
        .map(|text| reference_line(svc, text))
        .collect()
}

fn reference_line(svc: &AndroidTextService, text: &str) -> String {
    let run = svc.shape(text, &roboto16()).expect("reference shapes");
    let glyphs: Vec<String> = run
        .glyphs
        .iter()
        .map(|g| format!("{}:{:.4}", g.glyph_id, g.x_advance))
        .collect();
    format!("{} | {} | {:.4}", text, glyphs.join(" "), run.total_advance)
}

// Round 7.6: the renderer-injection surface (`all_font_ids` /
// `face_bytes`, the `oppa-text-linux` parity the Android app feeds
// into both backends) resolves every id to non-empty bytes on the
// host asset set — the same property `SceneState::setup` requires
// of `/system/fonts` on-device (loud `Err` there when empty).
#[test]
fn font_ids_resolve_to_face_bytes() {
    let svc = service();
    let ids = svc.all_font_ids();
    assert!(!ids.is_empty(), "asset set holds no faces");
    for id in &ids {
        let (bytes, _index) = svc
            .face_bytes(*id)
            .unwrap_or_else(|| panic!("face bytes missing for {id:?}"));
        assert!(!bytes.is_empty(), "empty face bytes for {id:?}");
    }
}

// On-device validation: the app writes `shapes.txt` (same line
// format) into its internal data dir; pull it to
// `crates/oppa-android-app/device-out/shapes.txt` and this test
// asserts the device shaped exactly what the host shapes.
#[test]
fn device_shapes_match_reference() {
    let device_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oppa-android-app/device-out/shapes.txt");
    if !device_path.exists() {
        panic!(
            "device shapes not pulled: run the app on the emulator, then \
             adb shell \"run-as com.oppa.app cat files/shapes.txt\" > {} \
             (via python binary-safe capture, never a shell redirect)",
            device_path.display()
        );
    }
    let device = std::fs::read_to_string(&device_path).expect("read device shapes");
    let svc = service();
    let reference = reference_lines(&svc).join("\n") + "\n";
    assert_eq!(
        device, reference,
        "on-device shaping differs from host reference"
    );
}
