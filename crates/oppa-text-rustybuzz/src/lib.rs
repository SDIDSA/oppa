//! Shared rustybuzz TextService core: the
//! `oppa::text::TextService` contract over rustybuzz (pure-Rust
//! HarfBuzz-class shaper — no system HarfBuzz/fontconfig needed,
//! which is what keeps every slice buildable without sudo and
//! runnable on Android/Linux targets) plus a font directory's own
//! font files. Platform wrappers (`oppa-text-android`,
//! `oppa-text-linux`) supply the directory + the fallback chain;
//! all shaping/measure semantics live here, once.
//!
//! Bounds (v1, mirroring the DirectWrite slice's stated bounds):
//! single-line; runs emitted in source order for an LTR base
//! direction (true bidi reordering stays layout-engine work); plain
//! shaping (no typographic feature list), so covered scripts shape
//! cluster-clean.
//!
//! Interpretation rules (stated; exercised by each wrapper's
//! acceptance suite):
//!
//! - `FontId = family_index * 4096 + face_index_within_family`
//!   (the DirectWrite scheme; `family_index` is the position in this
//!   service's distinct-family list).
//! - Unknown requested family → loud `FontNotFound` before shaping
//!   (same rule as DirectWrite — the fallback chain covers missing
//!   *glyphs*, never missing *families*).
//! - Itemization is a compact built-in script classifier (explicit
//!   ranges in `script_class`): Common/Inherit characters attach to
//!   the current item; anything unlisted joins the Latin class and
//!   is coverage-enforced (a missing glyph is a loud `Backend`
//!   error naming the character, never tofu).
//! - Face choice per item: requested-family faces first, then the
//!   caller-supplied chain (family names in preference order);
//!   exact (weight, style) matches sort before coverage-only
//!   matches, so a Bold face never silently stands in for a
//!   requested Regular; the first face covering every item
//!   character wins.
//! - Advances/offsets are device px: rustybuzz font-unit positions
//!   scaled by `em_size / units_per_em` (`em_size = font_size_px *
//!   dpr`, the shared rule).
//! - Letter tracking widens every advance except the run's final
//!   glyph (the trailing caret equals the run width — the M0b rule).
//! - Metrics: ascender/descent positive device px (`descent =
//!   -descender`), line gap scaled.
//! - `script` is the ISO 15924 *numeric* code of the item's script
//!   (`script_number` table; 0 = unknown).
//! - Faces are parsed per `shape` call in v1 (no self-referential
//!   cache; the layout measure-cache bounds call count — the
//!   corpus timing test keeps this honest).

use std::path::{Path, PathBuf};

use oppa::text::{
    Cluster, FontId, FontInfo, FontMetrics, FontStretch, FontStyle, FontWeight, ShapedGlyph,
    ShapedRun, TextError, TextRun, TextService, TextStyle,
};

/// One loaded face file entry (bytes owned; parsed per shape call).
struct LoadedFace {
    bytes: Vec<u8>,
    ttc_index: u32,
    family: String,
    weight: u16,
    style: FontStyle,
    file_name: String,
}

/// The rustybuzz text service: font files from one directory,
/// shaped through a caller-supplied fallback chain.
pub struct RustybuzzService {
    dir: PathBuf,
    faces: Vec<LoadedFace>,
    families: Vec<String>,
    chain: Vec<String>,
}

impl RustybuzzService {
    /// Loads every `.ttf`/`.otf`/`.ttc` under `dir`, recursively
    /// (distros nest families in subdirectories; Android's flat
    /// `/system/fonts` is the depth-0 case), sorted by path, so
    /// family indices are deterministic for a stable font set,
    /// shaping through `chain` after the requested family.
    /// Unparseable files are skipped loudly in the returned
    /// note list — `(service, skipped)`; a directory with zero
    /// usable faces is an `Err`.
    pub fn from_dir_with_chain(
        dir: &Path,
        chain: &[String],
    ) -> Result<(Self, Vec<String>), String> {
        let mut files: Vec<PathBuf> = Vec::new();
        collect_font_files(dir, &mut files)
            .map_err(|e| format!("font dir {} unreadable: {e}", dir.display()))?;
        files.sort();
        if files.is_empty() {
            return Err(format!("no font files in {}", dir.display()));
        }
        let mut inputs: Vec<(String, String, String, Vec<u8>)> = Vec::new();
        let mut skipped = Vec::new();
        for path in &files {
            let label = path.display().to_string();
            match std::fs::read(path) {
                Ok(b) => {
                    let stem = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let file_name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    inputs.push((label, stem, file_name, b));
                }
                Err(e) => {
                    skipped.push(format!("{label}: unreadable ({e})"));
                }
            }
        }
        let (faces, mut load_skipped) = load_faces(&inputs);
        skipped.append(&mut load_skipped);
        if faces.is_empty() {
            return Err(format!(
                "no usable faces in {} (skipped: {:?})",
                dir.display(),
                skipped
            ));
        }
        Ok((Self::from_faces(dir.to_path_buf(), faces, chain), skipped))
    }

    /// Loads faces from in-memory font bytes (the Web/bundled path —
    /// no font directory exists on wasm; the same bytes shape
    /// identically everywhere). `fonts` is (label, bytes) pairs in
    /// priority order; the label stands in for the file stem when
    /// the name table carries no family name, and prefixes skip
    /// notes. Family indices follow slice order (deterministic for
    /// stable input, like the sorted dir walk).
    pub fn from_bytes_with_chain(
        fonts: &[(&str, &[u8])],
        chain: &[String],
    ) -> Result<(Self, Vec<String>), String> {
        if fonts.is_empty() {
            return Err("no font bytes".to_string());
        }
        let inputs: Vec<(String, String, String, Vec<u8>)> = fonts
            .iter()
            .map(|(label, bytes)| {
                (
                    label.to_string(),
                    label.to_string(),
                    label.to_string(),
                    bytes.to_vec(),
                )
            })
            .collect();
        let (faces, skipped) = load_faces(&inputs);
        if faces.is_empty() {
            return Err(format!("no usable faces in memory (skipped: {skipped:?})"));
        }
        Ok((
            Self::from_faces(PathBuf::from("<memory>"), faces, chain),
            skipped,
        ))
    }

    /// Assembles a service from loaded faces (shared tail of both
    /// constructors; family order = face order).
    fn from_faces(dir: PathBuf, faces: Vec<LoadedFace>, chain: &[String]) -> Self {
        let mut families: Vec<String> = Vec::new();
        for face in &faces {
            if !families.contains(&face.family) {
                families.push(face.family.clone());
            }
        }
        Self {
            dir,
            faces,
            families,
            chain: chain.to_vec(),
        }
    }

    /// The font directory this service loads from (`<memory>`
    /// for [`from_bytes_with_chain`](Self::from_bytes_with_chain)).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Distinct family names in index order.
    pub fn families(&self) -> &[String] {
        &self.families
    }

    fn family_index(&self, family: &str) -> Result<usize, TextError> {
        self.families
            .iter()
            .position(|f| f.eq_ignore_ascii_case(family))
            .ok_or_else(|| TextError::FontNotFound(family.to_string()))
    }

    fn font_id_of(&self, face_pos: usize) -> FontId {
        let family = &self.faces[face_pos].family;
        let family_idx = self.families.iter().position(|f| f == family).unwrap_or(0);
        let within = self.faces[..face_pos]
            .iter()
            .filter(|f| &f.family == family)
            .count() as u32;
        FontId(family_idx as u32 * 4096 + within)
    }

    /// All font IDs present in the service's loaded faces (for renderer
    /// atlas / outline injection).
    pub fn all_font_ids(&self) -> Vec<FontId> {
        (0..self.faces.len()).map(|i| self.font_id_of(i)).collect()
    }

    /// The fallback chain (family names in preference order after
    /// the requested family).
    pub fn chain(&self) -> &[String] {
        &self.chain
    }

    /// Raw bytes + collection index of the face behind `id` (atlas
    /// injection for renderers: Vello `set_font_for` and CPU
    /// `set_font_for` take the same pair the file-backed path
    /// resolves through `font_file_source`). `None` for unknown
    /// ids — loud at the call site, never a fallback face.
    pub fn face_bytes(&self, id: FontId) -> Option<(&[u8], u32)> {
        let family_idx = (id.0 / 4096) as usize;
        let within = (id.0 % 4096) as usize;
        let name = self.families.get(family_idx)?;
        let mut seen = 0;
        for face in &self.faces {
            if &face.family == name {
                if seen == within {
                    return Some((&face.bytes, face.ttc_index));
                }
                seen += 1;
            }
        }
        None
    }

    /// Faces of one family, in load order.
    fn family_faces(&self, family_idx: usize) -> Vec<usize> {
        let name = &self.families[family_idx];
        self.faces
            .iter()
            .enumerate()
            .filter(|(_, f)| &f.family == name)
            .map(|(i, _)| i)
            .collect()
    }

    /// Face choice for one script item: requested family first, then
    /// the fallback chain; within each family exact (weight, style)
    /// matches precede coverage-only matches (a Bold face must never
    /// silently stand in for a requested Regular — the device's
    /// fuller font dir exposed exactly this). First face covering
    /// every item character wins. Loud `Backend` error otherwise.
    fn face_for(
        &self,
        family_idx: usize,
        item: &str,
        weight: FontWeight,
        style: &FontStyle,
    ) -> Result<usize, TextError> {
        let mut chain: Vec<usize> = self.family_faces(family_idx);
        for fallback in &self.chain {
            if let Some(idx) = self.families.iter().position(|f| f == fallback) {
                for pos in self.family_faces(idx) {
                    if !chain.contains(&pos) {
                        chain.push(pos);
                    }
                }
            }
        }
        // Exact (weight, style) matches first, then coverage-only.
        chain.sort_by_key(|pos| {
            let face = &self.faces[*pos];
            (face.weight != weight.0 || &face.style != style) as u8
        });
        for pos in chain {
            let face = &self.faces[pos];
            if let Ok(parsed) = rustybuzz::ttf_parser::Face::parse(&face.bytes, face.ttc_index) {
                if item.chars().all(|ch| parsed.glyph_index(ch).is_some()) {
                    return Ok(pos);
                }
            }
        }
        let missing: String = item
            .chars()
            .filter(|ch| {
                !self.faces.iter().any(|face| {
                    rustybuzz::ttf_parser::Face::parse(&face.bytes, face.ttc_index)
                        .ok()
                        .is_some_and(|parsed| parsed.glyph_index(*ch).is_some())
                })
            })
            .take(4)
            .map(|ch| format!("U+{:04X} ", ch as u32))
            .collect();
        let missing: String = if missing.is_empty() {
            item.chars()
                .take(4)
                .map(|ch| format!("U+{:04X} ", ch as u32))
                .collect()
        } else {
            missing
        };
        Err(TextError::Backend(format!(
            "no chain face covers {item:?} (uncovered: {missing})",
        )))
    }
}

/// Recursive font-file collection (no new deps — a small
/// `read_dir` recursion; symlinks to directories are not followed,
/// unreadable entries fail loudly).
fn collect_font_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("walk {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("font dir walk failed: {e}"))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|e| format!("file type {}: {e}", path.display()))?;
        if kind.is_dir() {
            collect_font_files(&path, out)?;
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
            out.push(path);
        }
    }
    Ok(())
}

/// Shared face loader: (display label, stem fallback, file-name
/// fallback, bytes) in.
/// Display labels prefix skip notes; the stem fills in when the
/// name table carries no family name; the file name fills the face
/// record (error notes). Behavior is identical for dir and memory
/// input (same strings, same order).
fn load_faces(inputs: &[(String, String, String, Vec<u8>)]) -> (Vec<LoadedFace>, Vec<String>) {
    let mut faces = Vec::new();
    let mut skipped = Vec::new();
    for (label, stem, file_name, bytes) in inputs {
        let count = rustybuzz::ttf_parser::fonts_in_collection(bytes)
            .unwrap_or(1)
            .max(1);
        let mut usable = 0;
        for index in 0..count {
            match face_record(bytes, index, stem, file_name) {
                Some(face) => {
                    faces.push(face);
                    usable += 1;
                }
                None => {
                    skipped.push(format!("{label}: face {index} unparseable"));
                }
            }
        }
        if usable == 0 && !skipped.iter().any(|s| s.starts_with(label)) {
            skipped.push(format!("{label}: no usable faces"));
        }
    }
    (faces, skipped)
}

/// Name-table family (ID 1, first Unicode record) with the given
/// stem as fallback (the file stem for dir input, the label for
/// memory input); weight from the OS/2 table with subfamily-name
/// fallback (Bold → 700 else 400); style from the head mac-style
/// bits (italic/oblique → Italic/Oblique else Normal).
fn face_record(bytes: &[u8], index: u32, stem: &str, file_name: &str) -> Option<LoadedFace> {
    let parsed = rustybuzz::ttf_parser::Face::parse(bytes, index).ok()?;
    let mut family: Option<String> = None;
    let mut subfamily: Option<String> = None;
    for record in parsed.names() {
        if record.name_id == 1 && family.is_none() {
            family = record.to_string();
        } else if record.name_id == 2 && subfamily.is_none() {
            subfamily = record.to_string();
        }
        if family.is_some() && subfamily.is_some() {
            break;
        }
    }
    let family = family.unwrap_or_else(|| stem.to_string());
    let sub = subfamily.unwrap_or_default().to_ascii_lowercase();
    let weight = parsed
        .tables()
        .os2
        .as_ref()
        .map(|os2| os2.weight().to_number())
        .unwrap_or(if sub.contains("bold") { 700 } else { 400 });
    let style = if parsed.is_italic() {
        FontStyle::Italic
    } else if parsed.is_oblique() {
        FontStyle::Oblique
    } else {
        FontStyle::Normal
    };
    Some(LoadedFace {
        bytes: bytes.to_vec(),
        ttc_index: index,
        family,
        weight,
        style,
        file_name: file_name.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Script itemization (compact built-in classifier, explicit ranges).
// ---------------------------------------------------------------------------

/// Coarse script class per character. `Common` attaches to the
/// current item (spaces, digits, punctuation, combining marks, ZWJ,
/// variation selectors); anything unlisted joins `Latin` and is
/// coverage-enforced downstream.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ScriptClass {
    Common,
    Latin,
    Arabic,
    Hebrew,
    Han,
    Hiragana,
    Katakana,
    Hangul,
    Thai,
    Devanagari,
    Greek,
    Cyrillic,
    Emoji,
}

impl ScriptClass {
    fn tag(self) -> rustybuzz::Script {
        use rustybuzz::script;
        match self {
            ScriptClass::Latin | ScriptClass::Common => script::LATIN,
            ScriptClass::Arabic => script::ARABIC,
            ScriptClass::Hebrew => script::HEBREW,
            ScriptClass::Han => script::HAN,
            ScriptClass::Hiragana => script::HIRAGANA,
            ScriptClass::Katakana => script::KATAKANA,
            ScriptClass::Hangul => script::HANGUL,
            ScriptClass::Thai => script::THAI,
            ScriptClass::Devanagari => script::DEVANAGARI,
            ScriptClass::Greek => script::GREEK,
            ScriptClass::Cyrillic => script::CYRILLIC,
            ScriptClass::Emoji => rustybuzz::Script::from_iso15924_tag(
                rustybuzz::ttf_parser::Tag::from_bytes(b"Zsye"),
            )
            .unwrap_or(rustybuzz::script::UNKNOWN),
        }
    }

    /// ISO 15924 numeric code of the class's script (0 = unknown).
    fn number(self) -> u16 {
        match self {
            ScriptClass::Latin | ScriptClass::Common => 215,
            ScriptClass::Arabic => 160,
            ScriptClass::Hebrew => 125,
            ScriptClass::Han => 500,
            ScriptClass::Hiragana => 410,
            ScriptClass::Katakana => 411,
            ScriptClass::Hangul => 286,
            ScriptClass::Thai => 352,
            ScriptClass::Devanagari => 315,
            ScriptClass::Greek => 200,
            ScriptClass::Cyrillic => 220,
            ScriptClass::Emoji => 990,
        }
    }

    fn rtl(self) -> bool {
        matches!(self, ScriptClass::Arabic | ScriptClass::Hebrew)
    }
}

fn script_class(ch: char) -> ScriptClass {
    let c = ch as u32;
    match c {
        // Common: controls/space/punct/digits/combining/ZWJ/VS.
        // (Digits 0x30-0x39 ride the 0x00-0x40 arm.)
        0x0000..=0x0040
        | 0x005B..=0x0060
        | 0x007B..=0x00A0
        | 0x2000..=0x200F
        | 0x2010..=0x205F
        | 0x0300..=0x036F
        | 0xFE00..=0xFE0F => ScriptClass::Common,
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => {
            ScriptClass::Arabic
        }
        0x0590..=0x05FF => ScriptClass::Hebrew,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF | 0x20000..=0x2A6DF => ScriptClass::Han,
        0x3040..=0x309F => ScriptClass::Hiragana,
        0x30A0..=0x30FF | 0xFF66..=0xFF9D => ScriptClass::Katakana,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => ScriptClass::Hangul,
        0x0E00..=0x0E7F => ScriptClass::Thai,
        0x0900..=0x097F => ScriptClass::Devanagari,
        0x0370..=0x03FF => ScriptClass::Greek,
        0x0400..=0x04FF => ScriptClass::Cyrillic,
        0x1F300..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF => ScriptClass::Emoji,
        // Unlisted scripts join Latin; coverage is enforced at
        // face choice (loud, never tofu).
        _ => ScriptClass::Latin,
    }
}

/// Splits `text` into `(byte_start, byte_end, class)` items:
/// class changes cut, `Common` attaches to the current item
/// (leading attachers open a Latin item).
fn itemize(text: &str) -> Vec<(usize, usize, ScriptClass)> {
    let mut items: Vec<(usize, usize, ScriptClass)> = Vec::new();
    let mut cur_class = ScriptClass::Latin;
    let mut cur_start: Option<usize> = None;
    for (byte, ch) in text.char_indices() {
        let class = script_class(ch);
        let effective = if class == ScriptClass::Common {
            match cur_start {
                Some(_) => cur_class,
                None => ScriptClass::Latin,
            }
        } else {
            class
        };
        match cur_start {
            Some(begin) if effective != cur_class => {
                items.push((begin, byte, cur_class));
                cur_start = Some(byte);
                cur_class = effective;
            }
            Some(_) => {}
            None => {
                cur_start = Some(byte);
                cur_class = effective;
            }
        }
    }
    if let Some(start) = cur_start {
        items.push((start, text.len(), cur_class));
    }
    items
}

// ---------------------------------------------------------------------------
// TextService impl.
// ---------------------------------------------------------------------------

impl TextService for RustybuzzService {
    fn enumerate_fonts(&self) -> Vec<FontInfo> {
        let mut out = Vec::new();
        for (family_idx, name) in self.families.iter().enumerate() {
            for (within, face) in self.faces.iter().filter(|f| &f.family == name).enumerate() {
                out.push(FontInfo {
                    id: FontId(family_idx as u32 * 4096 + within as u32),
                    family: name.clone(),
                    weight: FontWeight(face.weight),
                    style: face.style,
                    stretch: FontStretch::NORMAL,
                });
            }
        }
        out
    }

    fn shape(&self, text: &str, style: &TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        let family_idx = self.family_index(&style.family)?;
        let em_size = style.em_size();
        let mut glyphs: Vec<ShapedGlyph> = Vec::new();
        let mut runs: Vec<TextRun> = Vec::new();
        let mut clusters: Vec<Cluster> = Vec::new();
        let mut total = 0.0f32;
        for (start, end, class) in itemize(text) {
            let item = &text[start..end];
            let face_pos = self.face_for(family_idx, item, style.weight, &style.style)?;
            let face = &self.faces[face_pos];
            let parsed =
                rustybuzz::ttf_parser::Face::parse(&face.bytes, face.ttc_index).map_err(|_| {
                    TextError::Backend(format!("face re-parse failed: {}", face.file_name))
                })?;
            let rb_face =
                rustybuzz::Face::from_slice(&face.bytes, face.ttc_index).ok_or_else(|| {
                    TextError::Backend(format!("rustybuzz face failed: {}", face.file_name))
                })?;
            let upem = parsed.units_per_em() as f32;
            if upem <= 0.0 {
                return Err(TextError::Backend(format!(
                    "zero units_per_em: {}",
                    face.file_name
                )));
            }
            let scale = em_size / upem;
            let mut buffer = rustybuzz::UnicodeBuffer::new();
            for (rel, ch) in item.char_indices() {
                buffer.add(ch, (start + rel) as u32);
            }
            buffer.set_script(class.tag());
            buffer.set_direction(if class.rtl() {
                rustybuzz::Direction::RightToLeft
            } else {
                rustybuzz::Direction::LeftToRight
            });
            let shaped = rustybuzz::shape(&rb_face, &[], buffer);
            let infos = shaped.glyph_infos();
            let positions = shaped.glyph_positions();
            if infos.len() != positions.len() {
                return Err(TextError::Backend(format!(
                    "infos/positions length split ({} vs {})",
                    infos.len(),
                    positions.len()
                )));
            }
            let glyph_start = glyphs.len();
            for (info, pos) in infos.iter().zip(positions.iter()) {
                glyphs.push(ShapedGlyph {
                    glyph_id: info.glyph_id,
                    x_advance: pos.x_advance as f32 * scale,
                    x_offset: pos.x_offset as f32 * scale,
                    y_offset: pos.y_offset as f32 * scale,
                });
            }
            // Clusters: consecutive output glyphs sharing one input
            // cluster merge; each distinct cluster value is an input
            // char start (buffer indices are absolute byte offsets by
            // construction), so its byte range runs to the next start
            // in LOGICAL order, or the item end. Output order is
            // visual — ascending for LTR, descending for RTL — so
            // ranges resolved from output adjacency degenerate to
            // zero-length on RTL runs (decision 196: caught by the
            // item-2 Arabic proof; v1 suites pinned cluster counts
            // only). Sorted distinct starts partition every item in
            // both directions.
            let mut starts: Vec<usize> = infos
                .iter()
                .map(|info| (info.cluster as usize).clamp(start, end))
                .collect();
            starts.sort_unstable();
            starts.dedup();
            let end_of = |v: usize| -> usize {
                starts
                    .iter()
                    .find(|&&b| b > v)
                    .copied()
                    .unwrap_or(end)
                    .max(v)
            };
            let mut ci = 0;
            let mut item_clusters: Vec<Cluster> = Vec::new();
            while ci < infos.len() {
                let mut cj = ci + 1;
                while cj < infos.len() && infos[cj].cluster == infos[ci].cluster {
                    cj += 1;
                }
                let v = (infos[ci].cluster as usize).clamp(start, end);
                item_clusters.push(Cluster {
                    byte_range: (v, end_of(v)),
                    glyph_range: (glyph_start + ci, glyph_start + cj),
                });
                ci = cj;
            }
            // Logical byte order (RTL output arrives visual-reversed):
            // `order_visual` and the wrap span walk both assume list
            // order is logical (the DirectWrite slice's standing
            // assumption -- decision 196 aligns the slices here).
            // LTR items are already sorted (no-op); glyph ranges ride
            // their clusters, and all `ShapedRun` math is
            // order-independent, so LTR behavior is unchanged.
            item_clusters.sort_by_key(|c| c.byte_range.0);
            clusters.extend(item_clusters);
            let metrics = FontMetrics {
                ascent: parsed.ascender() as f32 * scale,
                descent: -(parsed.descender() as f32) * scale,
                line_gap: parsed.line_gap() as f32 * scale,
            };
            runs.push(TextRun {
                byte_range: (start, end),
                glyph_range: (glyph_start, glyphs.len()),
                rtl: class.rtl(),
                script: class.number(),
                font_id: self.font_id_of(face_pos),
                font_metrics: metrics,
            });
            total += glyphs[glyph_start..]
                .iter()
                .map(|g| g.x_advance)
                .sum::<f32>();
        }
        // Letter tracking: every advance except the run's final
        // glyph (the trailing caret equals the run width).
        if glyphs.len() > 1 && style.letter_spacing_px != 0.0 {
            let last = glyphs.len() - 1;
            for g in &mut glyphs[..last] {
                g.x_advance += style.letter_spacing_px;
            }
            total = glyphs.iter().map(|g| g.x_advance).sum();
        }
        Ok(ShapedRun {
            glyphs,
            runs,
            clusters,
            total_advance: total,
            text_len_bytes: text.len(),
        })
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn from_bytes_matches_from_dir_on_dejavu() {
        // Same bytes through both constructors shape bit-identical
        // runs (the Web/bundled path must agree with the dir path).
        let bytes: &[u8] = include_bytes!("../test-fonts/DejaVuSans.ttf");
        let chain: Vec<String> = vec![];
        let (mem, mem_skipped) =
            RustybuzzService::from_bytes_with_chain(&[("DejaVuSans.ttf", bytes)], &chain)
                .expect("memory load");
        assert!(mem_skipped.is_empty());
        assert_eq!(mem.dir().as_os_str(), "<memory>");
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fonts");
        let (disk, _) = RustybuzzService::from_dir_with_chain(&dir, &chain).expect("dir load");
        assert_eq!(mem.families(), disk.families());
        assert!(mem.families().contains(&"DejaVu Sans".to_string()));
        let style = TextStyle::new("DejaVu Sans", 64.0);
        for probe in ["FPS: 60", "FPS: 1069"] {
            assert_eq!(
                mem.shape(probe, &style).expect("shape mem"),
                disk.shape(probe, &style).expect("shape disk"),
                "probe {probe}"
            );
        }
    }

    #[test]
    fn from_bytes_rejects_empty_and_garbage_loudly() {
        let chain: Vec<String> = vec![];
        assert!(RustybuzzService::from_bytes_with_chain(&[], &chain).is_err());
        assert!(
            RustybuzzService::from_bytes_with_chain(&[("junk", b"not a font")], &chain).is_err()
        );
    }

    #[test]
    fn face_bytes_round_trips_the_shaped_id() {
        let bytes: &[u8] = include_bytes!("../test-fonts/DejaVuSans.ttf");
        let (svc, _) = RustybuzzService::from_bytes_with_chain(&[("DejaVuSans.ttf", bytes)], &[])
            .expect("memory load");
        let style = TextStyle::new("DejaVu Sans", 64.0);
        let run = svc.shape("FPS: 60", &style).expect("shape");
        let fid = run.runs[0].font_id;
        let (face_bytes, index) = svc.face_bytes(fid).expect("face bytes");
        assert_eq!(face_bytes, bytes);
        assert_eq!(index, 0);
        assert!(svc.face_bytes(FontId(4096 * 999)).is_none());
    }

    #[test]
    fn itemize_attaches_common_and_cuts_scripts() {
        // "Hi <CJK pair>" with the pair as escapes (decision 45:
        // corpus non-ASCII is ASCII escapes, never literal glyphs).
        let text = "Hi \u{4F60}\u{597D} world";
        let items = itemize(text);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].2, ScriptClass::Latin);
        assert_eq!(items[1].2, ScriptClass::Han);
        assert_eq!(items[2].2, ScriptClass::Latin);
        // Byte ranges partition the text.
        assert_eq!(items[0].0, 0);
        assert_eq!(items[2].1, text.len());
    }

    #[test]
    fn itemize_arabic_is_rtl_single_item() {
        // Arabic greeting (U+0645 U+0631 U+062D U+0628 U+0627) as escapes.
        let items = itemize("\u{645}\u{631}\u{62D}\u{628}\u{627}");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].2, ScriptClass::Arabic);
        assert!(items[0].2.rtl());
    }

    /// G9 never-tofu contract (runs everywhere — bundled DejaVu only):
    /// CJK and emoji are classified and routed, but DejaVu covers
    /// neither, and the empty chain offers nothing — so shaping
    /// refuses loudly, naming the uncovered codepoints. Tofu
    /// (silent .notdef) would be the failure this guards against.
    fn dejavu_only() -> RustybuzzService {
        let bytes: &[u8] = include_bytes!("../test-fonts/DejaVuSans.ttf");
        RustybuzzService::from_bytes_with_chain(&[("DejaVuSans.ttf", bytes)], &[])
            .expect("memory load")
            .0
    }

    #[test]
    fn uncovered_cjk_refuses_naming_codepoints() {
        let svc = dejavu_only();
        let style = TextStyle::new("DejaVu Sans", 16.0);
        let err = svc
            .shape("Hi \u{65E5}\u{672C}\u{8A9E}", &style)
            .expect_err("DejaVu covers no Han — loud, never tofu");
        let msg = err.to_string();
        assert!(
            msg.contains("U+65E5") && msg.contains("U+672C"),
            "uncovered codepoints named: {msg}"
        );
    }

    #[test]
    fn emoji_shapes_with_script_tag_when_covered() {
        // Grinning face U+1F600 as an escape (decision 45: corpus
        // non-ASCII is escapes, never literal glyphs). DejaVu Sans
        // genuinely covers it (glyph 5857 — found by running this
        // test, not assumed), so this is the positive proof: the
        // Emoji item shapes as its own run with script 990, one
        // cluster across all 4 UTF-8 bytes.
        let svc = dejavu_only();
        let style = TextStyle::new("DejaVu Sans", 16.0);
        let run = svc.shape("Hi \u{1F600}", &style).expect("emoji covered");
        assert_eq!(run.runs.len(), 2);
        assert_eq!(run.runs[1].script, 990);
        assert_eq!(run.runs[1].byte_range, (3, 7));
        assert!(run.clusters.iter().any(|c| c.byte_range == (3, 7)));
    }

    #[test]
    fn uncovered_private_use_refuses_naming_codepoints() {
        // U+E000 (Private Use Area): DejaVu maps nothing there, so
        // the coverage enforcement fires loudly with the codepoint
        // named — the never-tofu guarantee for any uncovered char,
        // emoji included where the chain offers no face.
        let svc = dejavu_only();
        let style = TextStyle::new("DejaVu Sans", 16.0);
        let err = svc
            .shape("Hi \u{E000}", &style)
            .expect_err("PUA uncovered — loud, never tofu");
        assert!(
            err.to_string().contains("U+E000"),
            "uncovered char named: {err}"
        );
    }

    #[test]
    fn emoji_itemizes_as_emoji_class() {
        let items = itemize("A\u{1F600}B");
        assert_eq!(items.len(), 3);
        assert_eq!(items[1].2, ScriptClass::Emoji);
        assert!(!items[1].2.rtl());
        assert_eq!(items[1].2.number(), 990);
    }

    #[test]
    fn absent_chain_entries_are_skipped_not_fatal() {
        // A chain naming fonts the service never loaded must not break
        // Latin shaping (chain misses are tolerated; missing *families*
        // in the request are what refuse).
        let bytes: &[u8] = include_bytes!("../test-fonts/DejaVuSans.ttf");
        let chain = vec![
            "No Such Family XYZ".to_string(),
            "Noto Sans CJK SC".to_string(),
        ];
        let (svc, _) =
            RustybuzzService::from_bytes_with_chain(&[("DejaVuSans.ttf", bytes)], &chain)
                .expect("memory load");
        let run = svc
            .shape("Hello", &TextStyle::new("DejaVu Sans", 16.0))
            .expect("latin shapes despite chain misses");
        assert_eq!(run.glyphs.len(), 5);
    }
}
