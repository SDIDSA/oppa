// ---------------------------------------------------------------------------
// IME mapping and delivery (Windows)
// ---------------------------------------------------------------------------

use super::*;

/// The app-side IME mapper: snapshotted Win32 IME messages onto the
/// focused session (Round 2.1, decision 256 — the spike's mapper
/// re-homed from a session handle to the focused-field lookup, so
/// composition follows focus instead of one rig field).
///
/// Mapping decisions (spike parity, recorded in the round log):
/// - Composition over an active selection: `CompositionStarted` is
///   fed BEFORE `DeleteRange{selection}` so the session's atomic
///   pre-composition undo snapshot captures the PRE-deletion content
///   (Ctrl+Z must restore the selection's original text).
/// - `GCS_RESULTSTR` at `WM_IME_COMPOSITION` is the commit; `END`
///   without a commit is the cancel.
/// - `GCS_CURSORPOS` is composition-relative UTF-16; mapped to UTF-8
///   bytes in composite coordinates (spike decision 20).
/// - Composition state rides the session (`is_composing`), not a
///   mapper flag — there is exactly one source of truth, so a cold
///   update anchors like the session's own race path and a cold
///   commit inserts like the session spells it.
/// - `swallow_pending` (UTF-16 units of the last commit) is the only
///   mapper state: commit echo `WM_CHAR`s consume it instead of
///   inserting. A fresh key or a new composition closes the echo
///   window (stale counts must never eat real typing); a short echo
///   resets and inserts (spike parity).
pub(crate) struct WinImeMapper {
    pub(crate) swallow_pending: usize,
}

impl WinImeMapper {
    pub(crate) fn new() -> Self {
        Self { swallow_pending: 0 }
    }

    /// One snapshotted message into normalized session events (pure
    /// over the session's observable state — headless-testable).
    pub(crate) fn map_message(
        &mut self,
        msg: &ImeMessage,
        session: &EditSession,
    ) -> Vec<ImeCompositionEvent> {
        match msg {
            ImeMessage::StartComposition => {
                self.swallow_pending = 0;
                let sel = session.selection();
                let mut out = vec![ImeCompositionEvent::CompositionStarted { start_byte: sel.0 }];
                if sel.0 != sel.1 {
                    out.push(ImeCompositionEvent::DeleteRange { range: sel });
                }
                out
            }
            ImeMessage::Composition {
                comp,
                cursor_pos,
                result,
                ..
            } => {
                if let Some(text) = result {
                    self.swallow_pending = text.encode_utf16().count();
                    return vec![ImeCompositionEvent::CompositionCommitted {
                        committed: text.clone(),
                    }];
                }
                if let Some(comp) = comp {
                    let mut out = Vec::new();
                    let start = match session.composition_start_byte() {
                        Some(s) => s,
                        None => {
                            // An update with no begin (platforms race):
                            // anchor like the session's own race path.
                            let sel = session.selection();
                            out.push(ImeCompositionEvent::CompositionStarted { start_byte: sel.0 });
                            if sel.0 != sel.1 {
                                out.push(ImeCompositionEvent::DeleteRange { range: sel });
                            }
                            sel.0
                        }
                    };
                    let caret = start + utf16_offset_to_byte(comp, (*cursor_pos).max(0) as usize);
                    out.push(ImeCompositionEvent::CompositionUpdated {
                        composition: comp.clone(),
                        caret_byte: caret,
                    });
                    out
                } else {
                    Vec::new()
                }
            }
            ImeMessage::EndComposition => {
                if session.is_composing() {
                    // END without a result commit: the IME cancelled
                    // (ESC, or a superseding action).
                    vec![ImeCompositionEvent::CompositionCancelled]
                } else {
                    Vec::new()
                }
            }
            ImeMessage::Notify { .. } | ImeMessage::SetContext { .. } => Vec::new(),
        }
    }

    /// Consumes one `WM_CHAR` as commit echo when the swallow count
    /// covers it (spike parity). Returns true when swallowed.
    pub(crate) fn swallow_char(&mut self, ch: char) -> bool {
        let units = ch.len_utf16();
        if self.swallow_pending >= units {
            self.swallow_pending -= units;
            true
        } else {
            self.swallow_pending = 0;
            false
        }
    }

    /// Closes the commit-echo window (fresh user intent — see the
    /// struct docs).
    pub(crate) fn clear_swallow(&mut self) {
        self.swallow_pending = 0;
    }
}

/// UTF-16 offset into a UTF-8 byte offset (spike parity — surrogate
/// pairs consume two units, lone units floor into the char).
pub(crate) fn utf16_offset_to_byte(text: &str, mut utf16_index: usize) -> usize {
    for (byte, ch) in text.char_indices() {
        let units = ch.len_utf16();
        if utf16_index < units {
            return byte;
        }
        utf16_index -= units;
    }
    text.len()
}

/// Drives one snapshotted IME message into the focused session and
/// re-anchors + re-syncs the candidate machinery. Quiet `Ok(false)`
/// with no focused field (the `feed_ime` focus-race precedent).
/// Returns whether anything damaged (re-present).
pub(crate) fn drive_ime(
    loop_: &mut DesktopLoop,
    shell: &mut Win32Shell,
    ime: &mut WinImeMapper,
    msg: &ImeMessage,
) -> Result<bool, String> {
    let Some(session) = loop_.host().focused_field_session() else {
        return Ok(false);
    };
    let events = ime.map_message(msg, &session);
    if events.is_empty() {
        return Ok(false);
    }
    let damage = loop_.feed_ime(&events)?;
    post_input_ime(loop_, shell);
    Ok(damage > 0)
}

/// Candidate anchor + TSF store refresh after input that may have
/// moved the caret or the focus (Round 2.1): the OS candidate window
/// follows every IME step, click, and key through the host-resolved
/// anchor (dpr-1 client px — the loop paints at dpr 1.0 into a
/// DPI-unaware window, so device px are client px); the TSF text
/// store mirrors the focused session (no-ops when TSF is off).
pub(crate) fn post_input_ime(loop_: &DesktopLoop, shell: &mut Win32Shell) {
    if let Some([x, y, width, height]) = loop_.ime_anchor() {
        shell.set_ime(ImeOps::SetCaretRect {
            x,
            y,
            width,
            height,
        });
    }
    if shell.tsf_enabled() {
        if let Some(session) = loop_.host().focused_field_session() {
            let st = session.observable();
            shell.tsf_sync_external(&st.content, st.sel);
        }
    }
}
