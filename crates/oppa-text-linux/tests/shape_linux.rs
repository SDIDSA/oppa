//! Linux text wrapper acceptance (Gap 4): chain + directory
//! logic run everywhere; font shaping runs on Linux only
//! (`cfg(target_os)` — the DirectWrite precedent: platform tests
//! assume platform assets, loudly).

use oppa_text_linux::{linux_fallback_chain, LinuxTextService};
use std::path::PathBuf;

// Runs everywhere: the chain names the fonts the wrapper falls
// back through (DejaVu/Ubuntu/Noto sets present on Ubuntu).
#[test]
fn chain_names_linux_sets() {
    let chain = linux_fallback_chain();
    assert!(chain.contains(&"DejaVu Sans".to_string()));
    assert!(chain.contains(&"Ubuntu".to_string()));
    assert!(chain.contains(&"Noto Color Emoji".to_string()));
}

// Runs everywhere: env override + missing dir is a loud Err.
#[test]
fn dir_override_and_loud_missing() {
    let missing = PathBuf::from("/definitely/not/a/font/dir-xyz");
    assert!(LinuxTextService::from_dir(&missing).is_err());
}

// Everything below needs real Linux fonts.
#[cfg(target_os = "linux")]
mod linux_only {
    use super::*;
    use oppa::text::{TextService, TextStyle};
    use oppa_text_linux::system_font_dir;

    fn service() -> LinuxTextService {
        let (svc, skipped) =
            LinuxTextService::system().expect("system fonts load (needs font packages)");
        assert!(skipped.is_empty(), "unexpected skipped faces: {skipped:?}");
        svc
    }

    #[test]
    fn enumerate_has_dejavu_or_ubuntu() {
        let svc = service();
        let fonts = svc.enumerate_fonts();
        assert!(!fonts.is_empty());
        assert!(
            fonts
                .iter()
                .any(|f| f.family == "DejaVu Sans" || f.family == "Ubuntu"),
            "families: {:?}",
            svc.families()
        );
    }

    #[test]
    fn latin_shapes_with_clusters() {
        let svc = service();
        let families = svc.families().to_vec();
        let family = ["DejaVu Sans", "Ubuntu", "Noto Sans"]
            .into_iter()
            .find(|f| families.iter().any(|g| g == f))
            .expect("a latin family");
        let run = svc
            .shape("Hello world", &TextStyle::new(family, 16.0))
            .expect("shape latin");
        assert_eq!(run.glyphs.len(), 11);
        assert!(run.total_advance > 0.0);
        let measured = svc.measure_line(&run);
        assert!((measured.width - run.total_advance).abs() < 1e-4);
        assert!(measured.ascent > 0.0 && measured.descent > 0.0);
    }

    #[test]
    fn dpr_scales_and_empty_is_loud() {
        let svc = service();
        let families = svc.families().to_vec();
        let family = ["DejaVu Sans", "Ubuntu", "Noto Sans"]
            .into_iter()
            .find(|f| families.iter().any(|g| g == f))
            .expect("a latin family");
        let run1 = svc
            .shape("Hi", &TextStyle::new(family, 16.0))
            .expect("dpr1");
        let mut style2 = TextStyle::new(family, 16.0);
        style2.device_pixel_ratio = 2.0;
        let run2 = svc.shape("Hi", &style2).expect("dpr2");
        assert!((run2.total_advance - 2.0 * run1.total_advance).abs() < 1e-3);
        assert!(svc.shape("", &TextStyle::new(family, 16.0)).is_err());
        assert!(svc
            .shape("Hi", &TextStyle::new("No Such Family XYZ", 16.0))
            .is_err());
    }

    #[test]
    fn system_dir_points_at_fonts() {
        // Env override is honored (test isolation without fonts).
        let dir = system_font_dir();
        assert!(dir.exists(), "no font dir at {}", dir.display());
    }

    /// G9 fallback contract (Ok-or-loud, never tofu): CJK shapes
    /// through the chain where Noto CJK is installed, and refuses
    /// loudly (naming codepoints) where not. Both arms are correct
    /// behavior — the failure this guards against is silent .notdef.
    #[test]
    fn cjk_fallback_is_ok_or_loud_never_tofu() {
        let svc = service();
        let families = svc.families().to_vec();
        let family = ["DejaVu Sans", "Ubuntu", "Noto Sans"]
            .into_iter()
            .find(|f| families.iter().any(|g| g == f))
            .expect("a latin family");
        let text = "Hi \u{65E5}\u{672C}\u{8A9E}";
        match svc.shape(text, &TextStyle::new(family, 16.0)) {
            Ok(run) => {
                assert!(!run.glyphs.is_empty());
                assert_eq!(run.text_len_bytes, text.len());
            }
            Err(e) => {
                let msg = e.to_string();
                assert!(msg.contains("U+65E5"), "refusal names the uncovered: {msg}");
            }
        }
    }

    /// Same contract for emoji (chain-routed to Noto Color Emoji;
    /// shaping resolves ids where installed — color *rendering* is
    /// OQ-G9-2, not this test).
    #[test]
    fn emoji_fallback_is_ok_or_loud_never_tofu() {
        let svc = service();
        let families = svc.families().to_vec();
        let family = ["DejaVu Sans", "Ubuntu", "Noto Sans"]
            .into_iter()
            .find(|f| families.iter().any(|g| g == f))
            .expect("a latin family");
        let text = "Hi \u{1F600}";
        match svc.shape(text, &TextStyle::new(family, 16.0)) {
            Ok(run) => {
                assert!(run.runs.iter().any(|r| r.script == 990));
            }
            Err(e) => {
                assert!(
                    e.to_string().contains("U+1F600"),
                    "refusal names the uncovered: {e}"
                );
            }
        }
    }
}
