#![cfg(windows)]

//! Windows DirectWrite backend for the `oppa::text::TextService` contract
//! (BUILD-ORDER M0b: enumerate, shape, measure, cluster map, DPR scaling).
//!
//! Pipeline per shaped string: UTF-8 → UTF-16 (with a code-unit → UTF-8
//! byte-offset table), script + bidi analysis through DirectWrite's
//! analyzer (implemented as the two COM callback objects the API requires),
//! font selection per run through the system font fallback
//! (`MapCharacters`), then `GetGlyphs` + `GetGlyphPlacements` per mapped
//! piece. All positions and advances come back in device px: the em size
//! handed to DirectWrite is `font_size_px * device_pixel_ratio` (§8.2
//! subpixel/DPR scaling lives here; §8.8 commit-position rounding stays the
//! shared helper).
//!
//! Bounds (v1, per DESIGN §2.3/§9.2): single-line; runs are emitted in
//! source order for an LTR base direction (true bidi reordering is layout
//! engine work, M3); no typographic features are passed (plain shaping), so
//! for the spike's covered scripts each cluster is exactly one glyph.

use oppa::text::{
    Cluster, FontId, FontInfo, FontMetrics, ShapedGlyph, ShapedRun, TextError, TextRun,
    TextService, TextStyle,
};
use windows::core::{Interface, PCWSTR};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFont, IDWriteFontFace, IDWriteFontFallback, IDWriteFontFamily,
    IDWriteLocalizedStrings, IDWriteNumberSubstitution, IDWriteTextAnalysisSink,
    IDWriteTextAnalysisSink_Impl, IDWriteTextAnalysisSource, IDWriteTextAnalysisSource_Impl,
    IDWriteTextAnalyzer, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_METRICS, DWRITE_FONT_STRETCH,
    DWRITE_FONT_STYLE, DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_STYLE_OBLIQUE, DWRITE_FONT_WEIGHT, DWRITE_GLYPH_OFFSET, DWRITE_LINE_BREAKPOINT,
    DWRITE_READING_DIRECTION, DWRITE_READING_DIRECTION_LEFT_TO_RIGHT, DWRITE_SCRIPT_ANALYSIS,
    DWRITE_SHAPING_GLYPH_PROPERTIES, DWRITE_SHAPING_TEXT_PROPERTIES,
};

/// The physical font file (path + the face index within it, for TTC
/// collections) behind a shaped face. The first file of the face is the
/// source; a face with multiple files is not produced by the system
/// collection path this backend uses.
fn face_file_reference(face: &IDWriteFontFace) -> windows::core::Result<(String, u32)> {
    use windows::Win32::Graphics::DirectWrite::{IDWriteFontFile, IDWriteLocalFontFileLoader};

    // Two-step `GetFiles`: first call with a null buffer yields the file
    // count; the second fills the buffer (passing 0 as the capacity on
    // the single call reads as E_INVALIDARG — the M6 font-cache fix).
    let mut count = 0u32;
    unsafe {
        face.GetFiles(&mut count, None)?;
    }
    if count == 0 {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            -1,
        )));
    }
    let mut files: Vec<Option<IDWriteFontFile>> = vec![None; count as usize];
    let mut written = count;
    unsafe {
        face.GetFiles(&mut written, Some(files.as_mut_ptr()))?;
        if written == 0 {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                -1,
            )));
        }
        let Some(file) = files.into_iter().flatten().next() else {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                -1,
            )));
        };
        let index = face.GetIndex();
        // The reference key is opaque — resolve it through the local file
        // loader (the old "key IS the path" reading yields garbage like
        // the M6 probe's `"...*SEGOEUI.TTF"` prefix — same fix).
        let loader = file.GetLoader()?;
        let local: IDWriteLocalFontFileLoader = loader.cast()?;
        let mut key: *mut core::ffi::c_void = core::ptr::null_mut();
        let mut key_size = 0u32;
        file.GetReferenceKey(&mut key, &mut key_size)?;
        let len = local.GetFilePathLengthFromKey(key, key_size)? as usize;
        if len == 0 {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                -1,
            )));
        }
        let mut buf = vec![0u16; len + 1];
        local.GetFilePathFromKey(key, key_size, &mut buf)?;
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let path = String::from_utf16_lossy(&buf[..end]);
        Ok((path, index))
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[windows_core::implement(IDWriteTextAnalysisSource)]
struct AnalysisSource {
    text: Vec<u16>,
    locale: Vec<u16>,
}

impl IDWriteTextAnalysisSource_Impl for AnalysisSource_Impl {
    fn GetTextAtPosition(
        &self,
        textposition: u32,
        textstring: *mut *mut u16,
        textlength: *mut u32,
    ) -> windows::core::Result<()> {
        let len = self.text.len() as u32;
        unsafe {
            if textposition >= len {
                *textstring = std::ptr::null_mut();
                *textlength = 0;
            } else {
                *textstring = self.text.as_ptr().add(textposition as usize) as *mut u16;
                *textlength = len - textposition;
            }
        }
        Ok(())
    }

    fn GetTextBeforePosition(
        &self,
        textposition: u32,
        textstring: *mut *mut u16,
        textlength: *mut u32,
    ) -> windows::core::Result<()> {
        let len = self.text.len() as u32;
        let end = textposition.min(len);
        unsafe {
            if end == 0 {
                *textstring = std::ptr::null_mut();
                *textlength = 0;
            } else {
                *textstring = self.text.as_ptr() as *mut u16;
                *textlength = end;
            }
        }
        Ok(())
    }

    fn GetParagraphReadingDirection(&self) -> DWRITE_READING_DIRECTION {
        DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
    }

    fn GetLocaleName(
        &self,
        textposition: u32,
        textlength: *mut u32,
        localename: *mut *mut u16,
    ) -> windows::core::Result<()> {
        unsafe {
            // Exact remaining length. A `u32::MAX` sentinel works for the
            // first `MapCharacters` call but makes every *subsequent* call on
            // the same source fail with E_INVALIDARG — the spike's multi-piece
            // corpus caught this (fixed round: M1 spike).
            let len = self.text.len() as u32;
            *textlength = len - textposition.min(len);
            *localename = self.locale.as_ptr() as *mut u16;
        }
        Ok(())
    }

    fn GetNumberSubstitution(
        &self,
        _textposition: u32,
        _textlength: *mut u32,
        numbersubstitution: windows_core::OutRef<IDWriteNumberSubstitution>,
    ) -> windows::core::Result<()> {
        numbersubstitution.write(None)
    }
}

#[windows_core::implement(IDWriteTextAnalysisSink)]
struct AnalysisSink {
    scripts: std::rc::Rc<std::cell::RefCell<Vec<(u32, u32, DWRITE_SCRIPT_ANALYSIS)>>>,
    bidi: std::rc::Rc<std::cell::RefCell<Vec<(u32, u32, bool)>>>,
}

impl IDWriteTextAnalysisSink_Impl for AnalysisSink_Impl {
    fn SetScriptAnalysis(
        &self,
        textposition: u32,
        textlength: u32,
        scriptanalysis: *const DWRITE_SCRIPT_ANALYSIS,
    ) -> windows::core::Result<()> {
        let script = unsafe { *scriptanalysis };
        self.scripts
            .borrow_mut()
            .push((textposition, textlength, script));
        Ok(())
    }

    fn SetLineBreakpoints(
        &self,
        _textposition: u32,
        _textlength: u32,
        _linebreakpoints: *const DWRITE_LINE_BREAKPOINT,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn SetBidiLevel(
        &self,
        textposition: u32,
        textlength: u32,
        _explicitlevel: u8,
        resolvedlevel: u8,
    ) -> windows::core::Result<()> {
        self.bidi
            .borrow_mut()
            .push((textposition, textlength, resolvedlevel & 1 == 1));
        Ok(())
    }

    fn SetNumberSubstitution(
        &self,
        _textposition: u32,
        _textlength: u32,
        _numbersubstitution: windows_core::Ref<IDWriteNumberSubstitution>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Windows-backed [`TextService`]: shared factory + analyzer + system font
/// collection + system fallback. `!Send` (COM pointers); use on the UI
/// thread, exactly like the reactive core.
///
/// `Clone` shares the shaped-face record (`font_files`) between the
/// clones — the app runner keeps one clone beside the host-owned one
/// so raster-face injection can cover every id the layout shaped
/// (bold/fallback faces included), not just a startup probe.
#[derive(Clone)]
pub struct DWriteTextService {
    analyzer: IDWriteTextAnalyzer,
    collection: windows::Win32::Graphics::DirectWrite::IDWriteFontCollection,
    fallback: IDWriteFontFallback,
    locale: Vec<u16>,
    /// Debug-renderer support (M1 remainder's Vello shell): for each font
    /// id the mapper produced, the physical font file path + the face
    /// index within it (TTCs). Recorded on first sight during `shape`.
    /// Shared across clones (see the struct docs).
    font_files: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<FontId, (String, u32)>>>,
}

impl DWriteTextService {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let factory: windows::Win32::Graphics::DirectWrite::IDWriteFactory =
                DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let analyzer = factory.CreateTextAnalyzer()?;
            let mut collection: Option<
                windows::Win32::Graphics::DirectWrite::IDWriteFontCollection,
            > = None;
            factory.GetSystemFontCollection(&mut collection, false)?;
            let collection = collection.expect("system font collection missing");
            let factory2: windows::Win32::Graphics::DirectWrite::IDWriteFactory2 =
                factory.cast()?;
            let fallback = factory2.GetSystemFontFallback()?;
            let locale = wide("en-US");
            Ok(Self {
                analyzer,
                collection,
                fallback,
                locale,
                font_files: std::rc::Rc::new(std::cell::RefCell::new(
                    std::collections::HashMap::new(),
                )),
            })
        }
    }

    /// The physical font file (path + face index within it, for TTCs)
    /// backing `font_id`, if this service has shaped through it yet.
    /// Debug-renderer support (M1 remainder): the real Vello backend (M3+)
    /// owns its glyph-atlas path and will not consume this.
    pub fn font_file_source(&self, font_id: FontId) -> Option<(String, u32)> {
        self.font_files.borrow().get(&font_id).cloned()
    }

    /// Every font id this service (or any clone sharing its record)
    /// has shaped through so far, sorted for deterministic injection
    /// order. Backs the runner's raster-face top-up (Round 7.2):
    /// every id here names a face the CPU backend may need for real
    /// glyphs instead of advance-cell bars.
    pub fn recorded_font_ids(&self) -> Vec<FontId> {
        let mut ids: Vec<FontId> = self.font_files.borrow().keys().copied().collect();
        ids.sort();
        ids
    }

    fn family_index(&self, family: &str) -> std::result::Result<u32, TextError> {
        unsafe {
            let name = wide(family);
            let mut index = 0u32;
            let mut exists = windows::core::BOOL::default();
            self.collection
                .FindFamilyName(PCWSTR(name.as_ptr()), &mut index, &mut exists)
                .map_err(|e| TextError::Backend(e.to_string()))?;
            if !exists.as_bool() {
                return Err(TextError::FontNotFound(family.to_string()));
            }
            Ok(index)
        }
    }

    /// Stable `FontId` for a font: family index × 4096 + the index of the
    /// first family font matching (weight, style, stretch).
    fn font_id_of(&self, font: &IDWriteFont) -> std::result::Result<FontId, TextError> {
        unsafe {
            let family: IDWriteFontFamily = font
                .GetFontFamily()
                .map_err(|e| TextError::Backend(e.to_string()))?;
            let names: IDWriteLocalizedStrings = family
                .GetFamilyNames()
                .map_err(|e| TextError::Backend(e.to_string()))?;
            let Some(name) = family_name(&names) else {
                return Err(TextError::Backend(
                    "mapped font has no family name".to_string(),
                ));
            };
            let fi = self.family_index(&name)?;
            let fam: IDWriteFontFamily = self
                .collection
                .GetFontFamily(fi)
                .map_err(|e| TextError::Backend(e.to_string()))?;
            let count = fam.GetFontCount();
            let (w, s, st) = (font.GetWeight(), font.GetStyle(), font.GetStretch());
            for f in 0..count {
                if let Ok(candidate) = fam.GetFont(f) {
                    if candidate.GetWeight() == w
                        && candidate.GetStyle() == s
                        && candidate.GetStretch() == st
                    {
                        return Ok(FontId(fi * 4096 + f));
                    }
                }
            }
            Ok(FontId(fi * 4096))
        }
    }
}

fn family_name(names: &IDWriteLocalizedStrings) -> Option<String> {
    unsafe {
        let count = names.GetCount();
        if count == 0 {
            return None;
        }
        let index = 0u32;
        let Ok(len) = names.GetStringLength(index) else {
            return None;
        };
        let mut buf = vec![0u16; len as usize + 1];
        names.GetString(index, &mut buf).ok()?;
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }
}

fn font_style(style: &oppa::text::FontStyle) -> DWRITE_FONT_STYLE {
    match style {
        oppa::text::FontStyle::Normal => DWRITE_FONT_STYLE_NORMAL,
        oppa::text::FontStyle::Italic => DWRITE_FONT_STYLE_ITALIC,
        oppa::text::FontStyle::Oblique => DWRITE_FONT_STYLE_OBLIQUE,
    }
}

fn face_metrics(face: &IDWriteFontFace, em_size: f32) -> FontMetrics {
    let mut metrics = DWRITE_FONT_METRICS::default();
    unsafe { face.GetMetrics(&mut metrics) };
    let scale = em_size / metrics.designUnitsPerEm.max(1) as f32;
    FontMetrics {
        ascent: metrics.ascent as f32 * scale,
        descent: metrics.descent as f32 * scale,
        line_gap: metrics.lineGap as f32 * scale,
    }
}

/// Per-UTF-16-code-unit table of the UTF-8 byte offset where that unit's
/// character starts; surrogate pairs map both units to the pair's start.
/// One entry past the end maps to the text length.
fn byte_offset_table(text: &str) -> Vec<usize> {
    let mut table = Vec::with_capacity(text.len() + 1);
    for (byte, ch) in text.char_indices() {
        for _ in 0..ch.len_utf16() {
            table.push(byte);
        }
    }
    table.push(text.len());
    table
}

/// True for UTF-16 units encoding Cc control characters (C0, DEL,
/// C1) — the `char::is_control` set, unit by unit. Never true for a
/// surrogate half, so astral text never takes the control path.
fn is_control_unit(unit: u16) -> bool {
    unit < 0x20 || (0x7F..=0x9F).contains(&unit)
}

impl TextService for DWriteTextService {
    fn enumerate_fonts(&self) -> Vec<FontInfo> {
        let mut out = Vec::new();
        unsafe {
            let family_count = self.collection.GetFontFamilyCount();
            for fi in 0..family_count {
                let Ok(family) = self.collection.GetFontFamily(fi) else {
                    continue;
                };
                let Ok(names) = family.GetFamilyNames() else {
                    continue;
                };
                let Some(name) = family_name(&names) else {
                    continue;
                };
                let font_count = family.GetFontCount();
                for f in 0..font_count {
                    let Ok(font) = family.GetFont(f) else {
                        continue;
                    };
                    out.push(FontInfo {
                        id: FontId(fi * 4096 + f),
                        family: name.clone(),
                        weight: oppa::text::FontWeight(font.GetWeight().0 as u16),
                        style: match font.GetStyle() {
                            DWRITE_FONT_STYLE_ITALIC => oppa::text::FontStyle::Italic,
                            DWRITE_FONT_STYLE_OBLIQUE => oppa::text::FontStyle::Oblique,
                            _ => oppa::text::FontStyle::Normal,
                        },
                        stretch: oppa::text::FontStretch(font.GetStretch().0 as u16),
                    });
                }
            }
        }
        out
    }

    fn shape(&self, text: &str, style: &TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        // Loud family check up front: DirectWrite's fallback would silently
        // substitute a default font for a missing family; the framework's
        // rule is loud failures, and the fallback's job is missing *glyphs*,
        // not missing *families*.
        self.family_index(&style.family)?;
        let utf16_text: Vec<u16> = text.encode_utf16().collect();
        let len = utf16_text.len() as u32;
        let source: IDWriteTextAnalysisSource = AnalysisSource {
            text: utf16_text.clone(),
            locale: self.locale.clone(),
        }
        .into();
        let scripts = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let bidi = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink: IDWriteTextAnalysisSink = AnalysisSink {
            scripts: scripts.clone(),
            bidi: bidi.clone(),
        }
        .into();
        unsafe {
            self.analyzer
                .AnalyzeScript(&source, 0, len, &sink)
                .map_err(|e| TextError::Backend(e.to_string()))?;
            self.analyzer
                .AnalyzeBidi(&source, 0, len, &sink)
                .map_err(|e| TextError::Backend(e.to_string()))?;
        }
        let byte_at = byte_offset_table(text);
        let mut glyphs: Vec<ShapedGlyph> = Vec::new();
        let mut runs: Vec<TextRun> = Vec::new();
        let mut clusters: Vec<Cluster> = Vec::new();
        let total = shape_script_runs(
            self,
            &utf16_text,
            &byte_at,
            &source,
            scripts.borrow().clone(),
            bidi.borrow().clone(),
            &mut glyphs,
            &mut runs,
            &mut clusters,
            style,
        )?;
        Ok(ShapedRun {
            glyphs,
            runs,
            clusters,
            total_advance: total,
            text_len_bytes: text.len(),
        })
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn shape_script_runs(
    service: &DWriteTextService,
    utf16_text: &[u16],
    byte_at: &[usize],
    source: &IDWriteTextAnalysisSource,
    script_runs: Vec<(u32, u32, DWRITE_SCRIPT_ANALYSIS)>,
    bidi_runs: Vec<(u32, u32, bool)>,
    glyphs: &mut Vec<ShapedGlyph>,
    runs: &mut Vec<TextRun>,
    clusters: &mut Vec<Cluster>,
    style: &TextStyle,
) -> std::result::Result<f32, TextError> {
    let family_w = wide(&style.family);
    let family_name_ptr = PCWSTR(family_w.as_ptr());
    let em_size = style.em_size();
    let mut total = 0.0f32;
    for (start, run_len, script) in script_runs {
        let rtl = bidi_runs
            .iter()
            .find(|(s, l, _)| start >= *s && start < s + l)
            .map(|(_, _, r)| *r)
            .unwrap_or(false);
        let end = start + run_len;
        let mut pos = start;
        while pos < end {
            let mut mapped_len = 0u32;
            let mut mapped_font: Option<IDWriteFont> = None;
            let mut map_scale = 1.0f32;
            unsafe {
                service
                    .fallback
                    .MapCharacters(
                        source,
                        pos,
                        end - pos,
                        &service.collection,
                        family_name_ptr,
                        DWRITE_FONT_WEIGHT(style.weight.0 as i32),
                        font_style(&style.style),
                        DWRITE_FONT_STRETCH(style.stretch.0 as i32),
                        &mut mapped_len,
                        &mut mapped_font,
                        &mut map_scale,
                    )
                    .map_err(|e| TextError::Backend(e.to_string()))?;
            }
            let Some(font) = mapped_font else {
                // No font maps this slice. Control characters (newline,
                // tab, the C0/C1 ranges) legitimately appear in editable
                // text and map to NO font anywhere — refusing them
                // crashes the app on ordinary input (a TextArea holding
                // "1456\n"). They shape as one zero-advance cluster with
                // no glyphs: nothing to paint (never tofu), no width
                // (carets sit at the visible edge), bytes still covered
                // (cluster math round-trips). Genuinely unmapped VISIBLE
                // characters stay loud below (the fallback contract —
                // never a silent skip).
                let mut cpos = pos;
                while cpos < end && is_control_unit(utf16_text[cpos as usize]) {
                    cpos += 1;
                }
                if cpos == pos {
                    return Err(TextError::Backend(format!(
                        "no font mapped for run at byte {}",
                        byte_at.get(pos as usize).copied().unwrap_or_default()
                    )));
                }
                push_control_cluster(&mut *clusters, glyphs.len(), byte_at, pos, cpos);
                pos = cpos;
                continue;
            };
            if mapped_len == 0 {
                return Err(TextError::Backend("MapCharacters stalled".to_string()));
            }
            let face: IDWriteFontFace =
                unsafe { font.CreateFontFace() }.map_err(|e| TextError::Backend(e.to_string()))?;
            // Record the physical file for the debug renderer (first sight).
            let font_id = service.font_id_of(&font)?;
            if !service.font_files.borrow().contains_key(&font_id) {
                if let Ok(source) = face_file_reference(&face) {
                    service.font_files.borrow_mut().insert(font_id, source);
                }
            }
            // A mapped slice can still hold control characters (some
            // fallback fonts claim \r or \t with real advances) —
            // split them out so controls never take width or paint,
            // wherever the fallback drew the line. Visible sub-runs
            // shape exactly as before (own `shape_piece` call per
            // contiguous run, so letter tracking and cluster maps
            // are unchanged); controls break runs like any shaping
            // boundary.
            let mut sub = pos;
            let sub_end = pos + mapped_len;
            while sub < sub_end {
                if is_control_unit(utf16_text[sub as usize]) {
                    let mut cend = sub + 1;
                    while cend < sub_end && is_control_unit(utf16_text[cend as usize]) {
                        cend += 1;
                    }
                    push_control_cluster(&mut *clusters, glyphs.len(), byte_at, sub, cend);
                    sub = cend;
                    continue;
                }
                let mut vend = sub + 1;
                while vend < sub_end && !is_control_unit(utf16_text[vend as usize]) {
                    vend += 1;
                }
                let (piece_glyphs, piece_clusters, piece_advance) = shape_piece(
                    &service.analyzer,
                    utf16_text,
                    sub,
                    vend - sub,
                    script,
                    rtl,
                    &face,
                    em_size,
                    style.letter_spacing_px,
                )?;
                let glyph_start = glyphs.len();
                for cluster in piece_clusters {
                    clusters.push(Cluster {
                        byte_range: (
                            byte_at[sub as usize + cluster.0],
                            byte_at[sub as usize + cluster.1],
                        ),
                        glyph_range: (cluster.2 + glyph_start, cluster.3 + glyph_start),
                    });
                }
                glyphs.extend(piece_glyphs);
                runs.push(TextRun {
                    byte_range: (
                        byte_at.get(sub as usize).copied().unwrap_or_default(),
                        byte_at.get(vend as usize).copied().unwrap_or_default(),
                    ),
                    glyph_range: (glyph_start, glyphs.len()),
                    rtl,
                    script: script.script,
                    font_id,
                    font_metrics: face_metrics(&face, em_size),
                });
                total += piece_advance;
                sub = vend;
            }
            pos = sub_end;
        }
    }
    Ok(total)
}

/// Emits one zero-advance cluster with no glyphs over UTF-16
/// `[start, end)` (a maximal control run): nothing to paint, no
/// width, bytes still covered. No `TextRun` — there is no font
/// identity to record, and an empty-glyph run would only carry an
/// invented one; the core's documented fallbacks cover the gap
/// (`run_font_of` → `FontId(0)`, run level → LTR).
fn push_control_cluster(
    clusters: &mut Vec<Cluster>,
    glyph_count: usize,
    byte_at: &[usize],
    start: u32,
    end: u32,
) {
    clusters.push(Cluster {
        byte_range: (
            byte_at.get(start as usize).copied().unwrap_or_default(),
            byte_at.get(end as usize).copied().unwrap_or_default(),
        ),
        glyph_range: (glyph_count, glyph_count),
    });
}

/// Shapes the piece `[pos, pos + piece_len)` of `utf16_text`. Returns glyphs
/// (piece-local), clusters as `(utf16_start, utf16_end, glyph_start,
/// glyph_end)`, and the piece advance in device px.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn shape_piece(
    analyzer: &IDWriteTextAnalyzer,
    utf16_text: &[u16],
    pos: u32,
    piece_len: u32,
    script: DWRITE_SCRIPT_ANALYSIS,
    rtl: bool,
    face: &IDWriteFontFace,
    em_size: f32,
    letter_spacing: f32,
) -> std::result::Result<(Vec<ShapedGlyph>, Vec<(usize, usize, usize, usize)>, f32), TextError> {
    let slice = &utf16_text[pos as usize..pos as usize + piece_len as usize];
    let locale = wide("en-US");
    let locale_ptr = PCWSTR(locale.as_ptr());
    let mut max_glyphs = slice.len() + 512;
    let (cluster_map, glyph_ids, mut advances, offsets);
    loop {
        let mut cluster_try = vec![0u16; slice.len()];
        let mut text_props = vec![DWRITE_SHAPING_TEXT_PROPERTIES::default(); slice.len()];
        let mut ids_try = vec![0u16; max_glyphs];
        let mut props_try = vec![DWRITE_SHAPING_GLYPH_PROPERTIES::default(); max_glyphs];
        let mut actual = 0u32;
        unsafe {
            analyzer
                .GetGlyphs(
                    PCWSTR(slice.as_ptr()),
                    slice.len() as u32,
                    face,
                    false,
                    rtl,
                    &script,
                    locale_ptr,
                    None::<&IDWriteNumberSubstitution>,
                    None,
                    None,
                    0,
                    max_glyphs as u32,
                    cluster_try.as_mut_ptr(),
                    text_props.as_mut_ptr(),
                    ids_try.as_mut_ptr(),
                    props_try.as_mut_ptr(),
                    &mut actual,
                )
                .map_err(|e| TextError::Backend(e.to_string()))?;
        }
        if actual as usize > max_glyphs {
            max_glyphs *= 2;
            continue;
        }
        let mut advance_try = vec![0f32; actual as usize];
        let mut offset_try = vec![DWRITE_GLYPH_OFFSET::default(); actual as usize];
        unsafe {
            analyzer
                .GetGlyphPlacements(
                    PCWSTR(slice.as_ptr()),
                    cluster_try.as_ptr(),
                    text_props.as_mut_ptr(),
                    slice.len() as u32,
                    ids_try.as_ptr(),
                    props_try.as_ptr(),
                    actual,
                    face,
                    em_size,
                    false,
                    rtl,
                    &script,
                    locale_ptr,
                    None,
                    None,
                    0,
                    advance_try.as_mut_ptr(),
                    offset_try.as_mut_ptr(),
                )
                .map_err(|e| TextError::Backend(e.to_string()))?;
        }
        cluster_map = cluster_try;
        glyph_ids = ids_try;
        advances = advance_try;
        offsets = offset_try;
        break;
    }
    // Letter tracking: added to every advance except the run's final glyph,
    // so the trailing caret position equals the run width.
    if advances.len() > 1 {
        let last = advances.len() - 1;
        for a in &mut advances[..last] {
            *a += letter_spacing;
        }
    }
    let out_glyphs: Vec<ShapedGlyph> = glyph_ids
        .iter()
        .zip(&advances)
        .zip(&offsets)
        .map(|((&id, &adv), &off)| ShapedGlyph {
            glyph_id: id as u32,
            x_advance: adv,
            x_offset: off.advanceOffset,
            y_offset: off.ascenderOffset,
        })
        .collect();
    let mut out_clusters = Vec::new();
    if !cluster_map.is_empty() {
        let mut cluster_start = 0usize;
        let mut cluster_glyph = cluster_map[0] as usize;
        for (u, &unit) in cluster_map.iter().enumerate().skip(1) {
            let next_glyph = unit as usize;
            if next_glyph != cluster_glyph {
                out_clusters.push((cluster_start, u, cluster_glyph, next_glyph));
                cluster_start = u;
                cluster_glyph = next_glyph;
            }
        }
        out_clusters.push((
            cluster_start,
            cluster_map.len(),
            cluster_glyph,
            advances.len(),
        ));
    }
    let total_advance: f32 = advances.iter().sum();
    Ok((out_glyphs, out_clusters, total_advance))
}
