//! Bundled fonts for platform-independent rendering: the same
//! bytes on Windows, Linux, Android, and Web, so shaped advances,
//! atlas coverage, and screenshots agree everywhere (no system font
//! directory exists on Web, and system sets differ per OS).
//!
//! `DEJAVU_SANS` is DejaVu Sans Regular (see
//! `fonts/LICENSE-DejaVu.txt`); the file is a byte copy of the
//! `oppa-text-rustybuzz` suite asset, which stays the acceptance
//! reference. Load it through
//! `RustybuzzService::from_bytes_with_chain` for shaping plus the
//! GPU/CPU atlas injections for rasterization.
//!
//! `READEX_PRO_*` are Readex Pro static instances (Regular 400,
//! SemiBold 600, Bold 700 — instantiated from the variable font at
//! `fonts/ReadexPro-Variable.ttf`, see `fonts/LICENSE-ReadexPro.txt`
//! for the SIL OFL terms) for apps wanting a geometric-humanist
//! default instead of DejaVu. `READEX_PRO_FACES` is the
//! `from_bytes_with_chain` input in weight order; the variable file
//! additionally serves the single-face Web `@font-face` (browsers
//! interpolate weights from it while the DOM backend emits no
//! `font-weight` — see the `oppa-web` font docs).

/// DejaVu Sans Regular font bytes (single face, index 0).
pub const DEJAVU_SANS: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");

/// Family name as the name table reports it (the fallback-chain
/// entry and the requested family for shaping).
pub const DEJAVU_SANS_FAMILY: &str = "DejaVu Sans";

/// Readex Pro Regular (400) font bytes.
pub const READEX_PRO_REGULAR: &[u8] = include_bytes!("../fonts/ReadexPro-Regular.ttf");

/// Readex Pro SemiBold (600) font bytes.
pub const READEX_PRO_SEMIBOLD: &[u8] = include_bytes!("../fonts/ReadexPro-SemiBold.ttf");

/// Readex Pro Bold (700) font bytes.
pub const READEX_PRO_BOLD: &[u8] = include_bytes!("../fonts/ReadexPro-Bold.ttf");

/// Readex Pro variable font bytes (wght axis) — the Web display
/// face (single `@font-face`, browsers interpolate weights).
pub const READEX_PRO_VARIABLE: &[u8] = include_bytes!("../fonts/ReadexPro-Variable.ttf");

/// Family name as the name table reports it.
pub const READEX_PRO_FAMILY: &str = "Readex Pro";

/// Static faces in weight order — the `from_bytes_with_chain`
/// input (labels double as file stems).
pub const READEX_PRO_FACES: &[(&str, &[u8])] = &[
    ("ReadexPro-Regular.ttf", READEX_PRO_REGULAR),
    ("ReadexPro-SemiBold.ttf", READEX_PRO_SEMIBOLD),
    ("ReadexPro-Bold.ttf", READEX_PRO_BOLD),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_bytes_are_a_nonempty_sfnt_face() {
        // sfnt version tag + non-empty; shaping-level checks live
        // in `oppa-text-rustybuzz` (from-bytes parity test) and the
        // cross-platform example's pixel proof.
        assert!(!DEJAVU_SANS.is_empty());
        assert_eq!(&DEJAVU_SANS[0..4], &[0x00, 0x01, 0x00, 0x00]);
        assert_eq!(DEJAVU_SANS_FAMILY, "DejaVu Sans");
    }

    #[test]
    fn readex_faces_are_nonempty_sfnt_with_family() {
        for bytes in [
            READEX_PRO_REGULAR,
            READEX_PRO_SEMIBOLD,
            READEX_PRO_BOLD,
            READEX_PRO_VARIABLE,
        ] {
            assert!(!bytes.is_empty());
            // sfnt 1.0 outlines in every file (statics and the
            // variable source alike), never a stub.
            assert_eq!(&bytes[0..4], &[0x00, 0x01, 0x00, 0x00]);
            assert!(bytes.len() > 100_000, "unexpected stub");
        }
        assert_eq!(READEX_PRO_FAMILY, "Readex Pro");
        assert_eq!(READEX_PRO_FACES.len(), 3);
    }
}
