//! On-device text validation (v1 remainder, Gap 2): shapes the
//! reference corpus through `/system/fonts` with
//! `oppa-text-android` and writes `shapes.txt` in the exact line
//! format the host reference test compares (`text | id:adv ... |
//! total`), plus the JNI platform-font record (`fonts_jni.txt` via
//! [`super::fonts_jni`]).
//!
//! Corpus entries are `\u{...}` escapes only (decision 45 — no
//! literal non-ASCII in source).

use std::path::Path;

use oppa::text::{TextService, TextStyle};

/// The reference corpus (must match `reference_lines` in
/// `oppa-text-android/tests/shape_android.rs` — same strings, same
/// order, same style).
const CORPUS: [&str; 9] = [
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

/// Shapes the corpus and writes `shapes.txt` + `fonts_jni.txt`.
/// Returns the meta-t.txt record line. Loud on any failure.
pub fn validate_text(app: &android_activity::AndroidApp, dir: &Path) -> Result<String, String> {
    use std::time::Instant;
    let t = Instant::now();
    let (svc, skipped) =
        oppa_text_android::AndroidTextService::from_dir(Path::new("/system/fonts"))
            .map_err(|e| format!("load /system/fonts: {e}"))?;
    let load_ms = t.elapsed().as_secs_f64() * 1000.0;
    let style = TextStyle::new("Roboto", 16.0);
    let mut lines = Vec::with_capacity(CORPUS.len());
    for text in CORPUS {
        let run = svc
            .shape(text, &style)
            .map_err(|e| format!("shape {text:?}: {e:?}"))?;
        let glyphs: Vec<String> = run
            .glyphs
            .iter()
            .map(|g| format!("{}:{:.4}", g.glyph_id, g.x_advance))
            .collect();
        lines.push(format!(
            "{text} | {} | {:.4}",
            glyphs.join(" "),
            run.total_advance
        ));
    }
    let shape_ms = t.elapsed().as_secs_f64() * 1000.0;
    let body = lines.join("\n") + "\n";
    std::fs::write(dir.join("shapes.txt"), &body).map_err(|e| format!("write shapes.txt: {e}"))?;
    let jni_record = super::fonts_jni::query_system_fonts(app)?;
    std::fs::write(dir.join("fonts_jni.txt"), format!("{jni_record}\n"))
        .map_err(|e| format!("write fonts_jni.txt: {e}"))?;
    let jni_count = jni_record.lines().count();
    let faces = svc.enumerate_fonts().len();
    Ok(format!(
        "text_faces={faces} text_skipped={} text_load_ms={load_ms:.0} text_shape_ms={shape_ms:.0} jni_fonts={jni_count}",
        skipped.len(),
    ))
}
