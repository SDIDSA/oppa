//! Android TextService slice (v1 remainder, Gap 2): the
//! `oppa::text::TextService` contract over the emulator's own font
//! files, shaped by the shared rustybuzz core
//! (`oppa-text-rustybuzz`).
//!
//! This crate is the Android half: the fallback chain (names as the
//! name-table reports them on the emulator image) plus the same
//! `from_dir` loading surface the device code uses. All
//! shaping/measure semantics live in the core — this wrapper adds
//! no behavior, so the on-device byte proof (`device_shapes_match_reference`
//! in `tests/shape_android.rs`) guards the refactor, not just the
//! feature.
//!
//! Machine-local test asset: `test-fonts/` holds byte copies pulled
//! from the emulator (`adb exec-out cat /system/fonts/<f>` —
//! `Roboto-Regular.ttf`, `NotoNaskhArabic-Regular.ttf`,
//! `NotoSansCJK-Regular.ttc`, `NotoColorEmoji.ttf`). Missing dir
//! fails loudly with the pull commands. On-device the service reads
//! `/system/fonts` directly. The JNI half (enumerating the
//! *platform's* font list via `android.graphics.fonts.SystemFonts`)
//! lives in the app crate, which owns the `JavaVM`.

use std::path::Path;

use oppa::text::{FontId, FontInfo, ShapedRun, TextError, TextService, TextStyle};
use oppa_text_rustybuzz::RustybuzzService;

/// Fallback families after the requested one (names as the
/// name-table reports them on the emulator image). `Noto Sans
/// Symbols` sits right after `Roboto`: symbol chars the UI fonts
/// lack (Select chevrons U+25B4/U+25BE — the Round 7.6 on-device
/// panic: no chain face covered them) resolve to the symbol font,
/// while everything Roboto covers keeps resolving there first.
/// Absent families skip by construction (the host asset set has no
/// Symbols face — host shaping stays byte-identical).
pub fn android_fallback_chain() -> Vec<String> {
    vec![
        "Roboto".to_string(),
        "Noto Sans Symbols".to_string(),
        "Noto Sans CJK JP".to_string(),
        "Noto Sans CJK KR".to_string(),
        "Noto Sans CJK SC".to_string(),
        "Noto Sans CJK TC".to_string(),
        "Noto Sans CJK HK".to_string(),
        "Noto Naskh Arabic".to_string(),
        "Noto Color Emoji".to_string(),
    ]
}

/// The Android text service: font files from one directory, shaped
/// through [`android_fallback_chain`].
pub struct AndroidTextService {
    inner: RustybuzzService,
}

impl AndroidTextService {
    /// Loads every `.ttf`/`.otf`/`.ttc` in `dir` (sorted by file
    /// name, so family indices are deterministic for a stable font
    /// set). Unparseable files are skipped loudly in the returned
    /// note list — `(service, skipped)`; a directory with zero
    /// usable faces is an `Err`.
    pub fn from_dir(dir: &Path) -> Result<(Self, Vec<String>), String> {
        let (inner, skipped) =
            RustybuzzService::from_dir_with_chain(dir, &android_fallback_chain())?;
        Ok((Self { inner }, skipped))
    }

    /// The font directory this service loads from.
    pub fn dir(&self) -> &Path {
        self.inner.dir()
    }

    /// Distinct family names in index order.
    pub fn families(&self) -> &[String] {
        self.inner.families()
    }

    /// All font IDs present in the service's loaded faces (renderer
    /// injection — mirrors `oppa-text-linux`; the app feeds every id
    /// into both backends so text encodes instead of failing loudly
    /// at paint).
    pub fn all_font_ids(&self) -> Vec<FontId> {
        self.inner.all_font_ids()
    }

    /// Raw bytes + collection index of the face behind `id`.
    pub fn face_bytes(&self, id: FontId) -> Option<(&[u8], u32)> {
        self.inner.face_bytes(id)
    }
}

impl TextService for AndroidTextService {
    fn enumerate_fonts(&self) -> Vec<FontInfo> {
        self.inner.enumerate_fonts()
    }

    fn shape(&self, text: &str, style: &TextStyle) -> Result<ShapedRun, TextError> {
        self.inner.shape(text, style)
    }
}
