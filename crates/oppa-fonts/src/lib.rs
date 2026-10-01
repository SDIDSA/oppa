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

/// DejaVu Sans Regular font bytes (single face, index 0).
pub const DEJAVU_SANS: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");

/// Family name as the name table reports it (the fallback-chain
/// entry and the requested family for shaping).
pub const DEJAVU_SANS_FAMILY: &str = "DejaVu Sans";

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
}
