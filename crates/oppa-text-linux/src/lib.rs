//! Linux TextService slice (v1 remainder, Gap 4): the
//! `oppa::text::TextService` contract over the system's own font
//! files, shaped by the shared rustybuzz core
//! (`oppa-text-rustybuzz` — no system HarfBuzz/fontconfig linkage,
//! which is what keeps this slice buildable without sudo).
//!
//! This crate is the Linux half: the fallback chain (names as the
//! name-table reports them on Ubuntu — DejaVu/Ubuntu/Noto sets)
//! plus the system font directory. All shaping/measure semantics
//! live in the core.

use std::path::{Path, PathBuf};

use oppa::text::{FontInfo, ShapedRun, TextError, TextService, TextStyle};
use oppa_text_rustybuzz::RustybuzzService;

/// Fallback families after the requested one (names as the
/// name-table reports them on Ubuntu 26.04: DejaVu, Ubuntu, Noto).
pub fn linux_fallback_chain() -> Vec<String> {
    vec![
        "DejaVu Sans".to_string(),
        "Ubuntu".to_string(),
        "Noto Sans".to_string(),
        "Noto Sans CJK JP".to_string(),
        "Noto Sans CJK KR".to_string(),
        "Noto Sans CJK SC".to_string(),
        "Noto Sans CJK TC".to_string(),
        "Noto Sans CJK HK".to_string(),
        "Noto Sans Arabic".to_string(),
        "Noto Naskh Arabic".to_string(),
        "Noto Color Emoji".to_string(),
    ]
}

/// System font directory (`$OPPA_LINUX_FONTS` overrides for tests).
pub fn system_font_dir() -> PathBuf {
    if let Ok(d) = std::env::var("OPPA_LINUX_FONTS") {
        return PathBuf::from(d);
    }
    PathBuf::from("/usr/share/fonts")
}

/// The Linux text service: font files from one directory, shaped
/// through [`linux_fallback_chain`].
pub struct LinuxTextService {
    inner: RustybuzzService,
}

impl LinuxTextService {
    /// Loads every `.ttf`/`.otf`/`.ttc` in `dir` (sorted by file
    /// name, so family indices are deterministic for a stable font
    /// set). Unparseable files are skipped loudly in the returned
    /// note list — `(service, skipped)`; a directory with zero
    /// usable faces is an `Err`.
    pub fn from_dir(dir: &Path) -> Result<(Self, Vec<String>), String> {
        let (inner, skipped) = RustybuzzService::from_dir_with_chain(dir, &linux_fallback_chain())?;
        Ok((Self { inner }, skipped))
    }

    /// Loads [`system_font_dir`] (loud when the system has no
    /// fonts — a headless container without font packages).
    pub fn system() -> Result<(Self, Vec<String>), String> {
        Self::from_dir(&system_font_dir())
    }

    /// The font directory this service loads from.
    pub fn dir(&self) -> &Path {
        self.inner.dir()
    }

    /// Distinct family names in index order.
    pub fn families(&self) -> &[String] {
        self.inner.families()
    }

    /// Raw bytes + collection index of the face behind `id`.
    pub fn face_bytes(&self, id: oppa::text::FontId) -> Option<(&[u8], u32)> {
        self.inner.face_bytes(id)
    }

    /// All font IDs present in the service's loaded faces.
    pub fn all_font_ids(&self) -> Vec<oppa::text::FontId> {
        self.inner.all_font_ids()
    }
}

impl TextService for LinuxTextService {
    fn enumerate_fonts(&self) -> Vec<FontInfo> {
        self.inner.enumerate_fonts()
    }

    fn shape(&self, text: &str, style: &TextStyle) -> Result<ShapedRun, TextError> {
        self.inner.shape(text, style)
    }
}
