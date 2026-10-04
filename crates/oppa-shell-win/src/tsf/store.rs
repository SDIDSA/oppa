//! TIP-facing text store: `StoreInner` state shared by the COM object, the sinks,
//! and the bridge (UTF-16/ACP units; the TIP owns it mid-composition).

use windows::Win32::UI::TextServices::ITextStoreACPSink;

use crate::win::ImeMessage;
// ---------------------------------------------------------------------------
// ITextStoreACP text store + composition sinks (text-store round)
// ---------------------------------------------------------------------------
//
// Design (recorded, not assumed): the store is the TIP-facing copy of the
// one field's text (UTF-16/ACP units). The host mirrors the settled
// session into it while no composition is active (`sync_external`);
// while a TIP composition is active the TIP owns it. TIP transactions
// are translated into the SAME `ImeMessage`s the IMM path produces
// (`StartComposition` / `Composition{comp/result}` / `EndComposition`)
// and queued through the same `ShellEvent::Ime` pipeline, so the mapper
// and session semantics are untouched. The composition span is tracked
// from the TIP's own SetText/Insert calls (no lock juggling: all data is
// already in our buffer when the sink callbacks fire).

/// Shared state between the COM store object, the sinks, and the bridge.
pub(super) struct StoreInner {
    pub(super) text: Vec<u16>,
    pub(super) sel_start: i32,
    pub(super) sel_end: i32,
    pub(super) lock_flags: u32,
    pub(super) pending_lock: Option<u32>,
    pub(super) store_sink: Option<ITextStoreACPSink>,
    pub(super) comp_active: bool,
    pub(super) comp_start: i32,
    pub(super) comp_end: i32,
    pub(super) log: Vec<String>,
}

impl StoreInner {
    pub(super) fn len(&self) -> i32 {
        self.text.len() as i32
    }

    pub(super) fn clamp(&self, acp: i32) -> i32 {
        acp.clamp(0, self.len())
    }

    pub(super) fn as_string(&self) -> String {
        String::from_utf16_lossy(&self.text)
    }

    pub(super) fn span_text(&self) -> String {
        let s = self.comp_start.clamp(0, self.len()) as usize;
        let e = self.comp_end.clamp(s as i32, self.len()) as usize;
        String::from_utf16_lossy(&self.text[s..e])
    }

    /// UTF-16 ACP index → UTF-8 byte index (same walk as the host mapper).
    pub(super) fn acp_to_byte(text: &str, mut acp: usize) -> usize {
        for (byte, ch) in text.char_indices() {
            let units = ch.len_utf16();
            if acp < units {
                return byte;
            }
            acp -= units;
        }
        text.len()
    }

    pub(super) fn byte_to_acp(text: &str, byte: usize) -> usize {
        let mut acp = 0;
        for (b, ch) in text.char_indices() {
            if b >= byte {
                break;
            }
            acp += ch.len_utf16();
        }
        acp
    }

    /// Maintain the tracked composition span from a TIP SetText/Insert
    /// covering `acp_start` and inserting `inserted` units.
    pub(super) fn track_span(&mut self, acp_start: i32, inserted: i32) {
        if !self.comp_active {
            return;
        }
        self.comp_start = self.comp_start.min(acp_start);
        self.comp_end = (self.comp_end.max(acp_start + inserted)).max(self.comp_start);
    }

    pub(super) fn start_composition(&mut self) {
        self.comp_active = true;
        self.comp_start = self.sel_start.min(self.sel_end);
        self.comp_end = self.sel_start.max(self.sel_end);
        self.log.push(format!(
            "store: composition started (span ({},{}))",
            self.comp_start, self.comp_end
        ));
    }

    /// The current composition as an IMM-shaped update message.
    pub(super) fn composition_message(&self) -> Option<ImeMessage> {
        if !self.comp_active {
            return None;
        }
        let comp = self.span_text();
        let text = self.as_string();
        let caret_acp = self
            .sel_end
            .clamp(self.comp_start, self.comp_end.max(self.comp_start));
        let cursor = (caret_acp - self.comp_start).max(0) as usize;
        Some(ImeMessage::Composition {
            gcs_flags: 0,
            comp: Some(comp),
            attrs: None,
            cursor_pos: Self::acp_to_byte(&text, cursor) as i32,
            delta_start: 0,
            result: None,
        })
    }

    /// Mirror the IMM flow: a non-empty final span commits (result, then
    /// END so the mapper's `comp_active` clears), an empty one cancels
    /// (END only → the mapper drops the buffer).
    pub(super) fn end_composition(&mut self) -> Vec<ImeMessage> {
        self.comp_active = false;
        let committed = self.span_text();
        self.log.push(format!(
            "store: composition ended (final span text {committed:?})"
        ));
        if committed.is_empty() {
            vec![ImeMessage::EndComposition]
        } else {
            vec![
                ImeMessage::Composition {
                    gcs_flags: 0,
                    comp: None,
                    attrs: None,
                    cursor_pos: 0,
                    delta_start: 0,
                    result: Some(committed),
                },
                ImeMessage::EndComposition,
            ]
        }
    }
}
