//! Glyph atlas from shaped runs (M6 item 2; DESIGN §2.3).
//!
//! Pre-shaped, pre-positioned cells in, font bytes out: the atlas never
//! re-shapes or re-lays-out. M7 (decision 110) ends the M6 single-face
//! bound (finding F3's remainder): [`DrawOp::Text`](oppa::DrawOp) now
//! carries per-run font identity, so the atlas holds one face per
//! distinct shaper-reported [`FontId`](oppa::FontId) plus the default
//! face. Face selection per run is explicit-id → default → loud refusal
//! (a Text op with no usable face at all is a loud
//! [`BackendError::UnsupportedOp`](oppa::BackendError), never tofu).

use std::collections::HashMap;

use vello::peniko::{Blob, FontData};

/// One encoded glyph placement: the fidelity-inspection record. `x` is the
/// device-px pen position relative to the text box origin (subpixel — the
/// atlas never rounds); `advance` is the shaped advance, unmodified;
/// `font` names the face the run encoded against (the per-run selection
/// instrument).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EncodedGlyph {
    pub glyph_id: u32,
    pub x: f32,
    pub advance: f32,
    pub font: oppa::FontId,
}

/// The atlas: one default face plus per-id faces, and the per-encode
/// placement log the fidelity tests assert against.
pub struct GlyphAtlas {
    font: Option<FontData>,
    /// Face index within the file (TTCs); recorded for the matrix log.
    pub face_index: u32,
    /// Bytes injected (for the matrix log: which file backs the face).
    pub bytes_len: usize,
    /// Per-id faces (fallback runs, mixed-coverage text).
    faces: HashMap<oppa::FontId, (FontData, u32, usize)>,
    log: Vec<EncodedGlyph>,
}

impl GlyphAtlas {
    pub fn new() -> Self {
        Self {
            font: None,
            face_index: 0,
            bytes_len: 0,
            faces: HashMap::new(),
            log: Vec::new(),
        }
    }

    /// Injects the surface's default font face (file bytes + face index).
    /// Replaces any previous default; per-id faces survive (they refine,
    /// never widen, the default).
    pub fn set_font_bytes(&mut self, bytes: Vec<u8>, index: u32) {
        self.bytes_len = bytes.len();
        self.face_index = index;
        let blob = Blob::new(std::sync::Arc::new(bytes));
        self.font = Some(FontData::new(blob, index));
    }

    /// Injects the face for one shaper-reported font id (a fallback run's
    /// file bytes + face index). Replaces any previous face for the id.
    pub fn set_font_for(&mut self, id: oppa::FontId, bytes: Vec<u8>, index: u32) {
        let len = bytes.len();
        let blob = Blob::new(std::sync::Arc::new(bytes));
        self.faces
            .insert(id, (FontData::new(blob, index), index, len));
    }

    pub fn has_font(&self) -> bool {
        self.font.is_some() || !self.faces.is_empty()
    }

    /// Face ids currently held (default excluded — diagnostics only).
    pub fn face_ids(&self) -> Vec<oppa::FontId> {
        let mut ids: Vec<oppa::FontId> = self.faces.keys().copied().collect();
        ids.sort();
        ids
    }

    /// Selects the face for one run: explicit id first, default second,
    /// nothing third (the caller turns `None` into the loud refusal).
    /// Public for the per-run selection tests (the draw path needs a
    /// real font; the selection discipline does not).
    pub fn face_for(&self, id: oppa::FontId) -> Option<&FontData> {
        self.faces
            .get(&id)
            .map(|(f, _, _)| f)
            .or(self.font.as_ref())
    }

    /// Records one run's placements (pen math over shaped advances —
    /// the same prefix-sum the CPU cell filler walks, never re-shaped).
    pub(crate) fn log_run(
        &mut self,
        base_x: f32,
        glyphs: &[oppa::PlacedGlyph],
        font: oppa::FontId,
    ) {
        let mut pen = base_x;
        for g in glyphs {
            self.log.push(EncodedGlyph {
                glyph_id: g.glyph_id,
                x: pen,
                advance: g.advance,
                font,
            });
            pen += g.advance;
        }
    }

    pub(crate) fn clear_log(&mut self) {
        self.log.clear();
    }

    /// Placement log of the latest encode (fidelity assertions read this).
    pub fn placements(&self) -> &[EncodedGlyph] {
        &self.log
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}
