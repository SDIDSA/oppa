// ---------------------------------------------------------------------------
// Caret, selection, movement, and hit-testing
// ---------------------------------------------------------------------------

use super::geometry::{
    byte_at_x, hard_line_byte, line_containing, line_index_containing, word_class, x_in_line,
    VisualLine, WordClass,
};
use super::*;

impl EditSession {
    pub fn click_x(&self, x: f32) {
        self.ensure_clamped();
        let Some(byte) = self.hit_test(x) else {
            return;
        };
        *self.inner.caret.borrow_mut() = byte;
        *self.inner.sel.borrow_mut() = (byte, byte);
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
        self.inner.rt.request_frame();
    }

    pub fn shift_click_x(&self, x: f32) {
        self.ensure_clamped();
        let Some(byte) = self.hit_test(x) else {
            return;
        };
        // Keyboard anchor survives into the click (native
        // Shift+Click extends from the selection anchor, then the
        // click re-seeds it).
        let anchor = self.extend_anchor();
        *self.inner.sel.borrow_mut() = if byte >= anchor {
            (anchor, byte)
        } else {
            (byte, anchor)
        };
        *self.inner.caret.borrow_mut() = byte;
        *self.inner.shift_anchor.borrow_mut() = Some(anchor);
        // Horizontal motion resets column affinity (line moves
        // re-seed it — see `preferred_x`).
        self.inner.preferred_x.set(None);
        self.break_run();
        self.note_caret_activity();
        self.inner.rt.request_frame();
    }

    pub fn drag_x(&self, x1: f32, x2: f32) {
        self.ensure_clamped();
        let (Some(b1), Some(b2)) = (self.hit_test(x1), self.hit_test(x2)) else {
            return;
        };
        *self.inner.sel.borrow_mut() = (b1.min(b2), b1.max(b2));
        *self.inner.caret.borrow_mut() = b1.max(b2);
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
        self.inner.rt.request_frame();
    }

    pub fn dbl_click_x(&self, x: f32) {
        self.ensure_clamped();
        let Some(byte) = self.hit_test(x) else {
            return;
        };
        let (a, b) = self.word_range(byte);
        *self.inner.sel.borrow_mut() = (a, b);
        *self.inner.caret.borrow_mut() = b;
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
        self.inner.rt.request_frame();
    }

    /// Line selection for a triple-click (Round 8.2, decision 298): the
    /// hard line (`\n`-delimited; soft wraps stay laid-out flow, not
    /// selection units in v1) containing the hit byte. Single-line
    /// fields select the whole content. No-op with no shaper (the
    /// decision-207 graceful rule — pointer ops never panic setup
    /// state); caret lands at the line end.
    pub fn select_line_x(&self, x: f32) {
        self.ensure_clamped();
        let Some(byte) = self.hit_test(x) else {
            return;
        };
        let text = self.composite_text();
        let start = text[..byte].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let end = text[byte..]
            .find('\n')
            .map(|i| byte + i)
            .unwrap_or(text.len());
        let (start, end) = (clamp_byte(&text, start), clamp_byte(&text, end));
        *self.inner.sel.borrow_mut() = (start, end);
        *self.inner.caret.borrow_mut() = end;
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
        self.inner.rt.request_frame();
    }

    pub fn caret_move(&self, steps: i32) {
        self.ensure_clamped();
        // Collapse-first (native arrows): an open selection
        // collapses toward the step direction before stepping.
        let (a, b) = ordered(*self.inner.sel.borrow());
        if a != b {
            let edge = if steps < 0 { a } else { b };
            *self.inner.caret.borrow_mut() = edge;
            *self.inner.sel.borrow_mut() = (edge, edge);
            self.clear_caret_anchors();
            self.break_run();
            self.note_caret_activity();
            return;
        }
        let byte = self.caret_boundary(steps);
        *self.inner.caret.borrow_mut() = byte;
        *self.inner.sel.borrow_mut() = (byte, byte);
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
    }

    /// Clears the caret anchors (plain horizontal moves, pointer
    /// ops, and content mutations re-seed them — see `shift_anchor`
    /// and `preferred_x`). Line moves manage `shift_anchor`
    /// directly (plain line steps clear it, extends keep it) and
    /// never touch `preferred_x`.
    pub(crate) fn clear_caret_anchors(&self) {
        *self.inner.shift_anchor.borrow_mut() = None;
        self.inner.preferred_x.set(None);
    }

    /// The fixed end of a shift extension (Round 22.1): the live
    /// anchor when a shift gesture runs, else the far end of any
    /// open selection (select-all extends from 0), else the caret.
    pub(crate) fn extend_anchor(&self) -> usize {
        if let Some(a) = *self.inner.shift_anchor.borrow() {
            return a;
        }
        let caret = *self.inner.caret.borrow();
        let (a, b) = ordered(*self.inner.sel.borrow());
        if a == b {
            caret
        } else if caret == a {
            b
        } else {
            a
        }
    }

    /// Shift-extended caret move (Round 22.1 — Shift+Left /
    /// Shift+Right): spans from the shift anchor (persisted across
    /// presses, so selections accumulate natively) to the new
    /// caret.
    pub fn extend_caret(&self, steps: i32) {
        self.ensure_clamped();
        let anchor = self.extend_anchor();
        let byte = self.caret_boundary(steps);
        *self.inner.sel.borrow_mut() = (anchor.min(byte), anchor.max(byte));
        *self.inner.caret.borrow_mut() = byte;
        *self.inner.shift_anchor.borrow_mut() = Some(anchor);
        self.inner.preferred_x.set(None);
        self.break_run();
        self.note_caret_activity();
    }

    /// Word-step caret move (Round 22.1, decision 331 — Ctrl+Left /
    /// Ctrl+Right): `steps` word edges from the caret (mid-word
    /// stops at the word end first, then word starts; CJK steps per
    /// character; separators never land — see
    /// [`EditSession::word_boundary`]). Collapses the selection
    /// like [`EditSession::caret_move`] (open selections collapse
    /// toward the step direction first).
    pub fn word_move(&self, steps: i32) {
        self.ensure_clamped();
        let (a, b) = ordered(*self.inner.sel.borrow());
        if a != b {
            let edge = if steps < 0 { a } else { b };
            *self.inner.caret.borrow_mut() = edge;
            *self.inner.sel.borrow_mut() = (edge, edge);
            self.clear_caret_anchors();
            self.break_run();
            self.note_caret_activity();
            return;
        }
        let byte = self.word_boundary(steps);
        *self.inner.caret.borrow_mut() = byte;
        *self.inner.sel.borrow_mut() = (byte, byte);
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
    }

    /// Word-step selection extension (Round 22.1 — Ctrl+Shift+Left /
    /// Ctrl+Shift+Right): spans from the shift anchor (persisted,
    /// like [`EditSession::extend_caret`]) to the word-stepped
    /// caret.
    pub fn extend_word(&self, steps: i32) {
        self.ensure_clamped();
        let anchor = self.extend_anchor();
        let byte = self.word_boundary(steps);
        *self.inner.sel.borrow_mut() = (anchor.min(byte), anchor.max(byte));
        *self.inner.caret.borrow_mut() = byte;
        *self.inner.shift_anchor.borrow_mut() = Some(anchor);
        self.inner.preferred_x.set(None);
        self.break_run();
        self.note_caret_activity();
    }

    /// Single-step navigation vocabulary (Round 22.1, decision 331 —
    /// the `DesktopLoop` shortcut table names these, one per
    /// key shape, so runners never spell raw step counts).
    pub fn caret_left(&self) {
        self.caret_move(-1);
    }

    pub fn caret_right(&self) {
        self.caret_move(1);
    }

    pub fn extend_left(&self) {
        self.extend_caret(-1);
    }

    pub fn extend_right(&self) {
        self.extend_caret(1);
    }

    pub fn word_left(&self) {
        self.word_move(-1);
    }

    pub fn word_right(&self) {
        self.word_move(1);
    }

    pub fn extend_word_left(&self) {
        self.extend_word(-1);
    }

    pub fn extend_word_right(&self) {
        self.extend_word(1);
    }

    /// Publishes the multi-line wrap width in device px (Round
    /// 22.2, decision 332): `None` (default) = single-line;
    /// `Some(w)` = multi-line wrapping at `w` (`TextArea` publishes
    /// `width - padding` every render). Must be finite and positive
    /// when set (loud panic otherwise — a wrap width is never
    /// zero/NaN, the scene-scale rule).
    pub fn set_wrap_width(&self, width: Option<f32>) {
        if let Some(w) = width {
            assert!(
                w.is_finite() && w > 0.0,
                "set_wrap_width: width must be finite and positive, got {w}"
            );
        }
        self.inner.wrap_width.set(width);
    }

    /// The published wrap width, if multi-line (see
    /// [`EditSession::set_wrap_width`]).
    pub fn wrap_width(&self) -> Option<f32> {
        self.inner.wrap_width.get()
    }

    /// Whether this session navigates visual lines (see
    /// [`EditSession::set_wrap_width`] — the loop routes Up/Down
    /// here only when true).
    pub fn is_multiline(&self) -> bool {
        self.wrap_width().is_some()
    }

    /// Moves the caret one visual line up, preserving the
    /// preferred-x anchor across lines (Round 22.2 — ArrowUp):
    /// short lines clamp the caret but never the anchor. At the
    /// first line the caret goes to the line start. Collapses open
    /// selections toward the step first (the plain-move rule). A
    /// loud no-op without a wrap width (single-line sessions never
    /// route here — the loop guards on `is_multiline`).
    pub fn line_up(&self) {
        self.line_step(-1, false);
    }

    /// Moves the caret one visual line down (Round 22.2 —
    /// ArrowDown): mirror of [`EditSession::line_up` (at the last
    /// line the caret goes to the text end).
    pub fn line_down(&self) {
        self.line_step(1, false);
    }

    /// Extends one visual line up (Round 22.2 — Shift+ArrowUp):
    /// spans from the shift anchor to the anchored line caret.
    pub fn extend_line_up(&self) {
        self.line_step(-1, true);
    }

    /// Extends one visual line down (Round 22.2 — Shift+ArrowDown).
    pub fn extend_line_down(&self) {
        self.line_step(1, true);
    }

    /// Shared vertical step (dir −1 up / +1 down, extend selects):
    /// collapse-first for plain moves, shift-anchor spans for
    /// extends, preferred-x preserved across the run.
    pub(crate) fn line_step(&self, dir: i32, extend: bool) {
        self.ensure_clamped();
        let (a, b) = ordered(*self.inner.sel.borrow());
        if !extend && a != b {
            let edge = if dir < 0 { a } else { b };
            *self.inner.caret.borrow_mut() = edge;
            *self.inner.sel.borrow_mut() = (edge, edge);
            *self.inner.shift_anchor.borrow_mut() = None;
            self.break_run();
            self.note_caret_activity();
            return;
        }
        let caret = *self.inner.caret.borrow();
        let anchor = if extend { self.extend_anchor() } else { caret };
        let x = self
            .inner
            .preferred_x
            .get()
            .unwrap_or_else(|| self.caret_visual_x(caret));
        let byte = self.line_byte(dir, caret, x);
        if extend {
            *self.inner.sel.borrow_mut() = (anchor.min(byte), anchor.max(byte));
            *self.inner.shift_anchor.borrow_mut() = Some(anchor);
        } else {
            *self.inner.sel.borrow_mut() = (byte, byte);
            *self.inner.shift_anchor.borrow_mut() = None;
        }
        *self.inner.caret.borrow_mut() = byte;
        // Preserve the anchor across the run (set once — short
        // lines must not shrink it).
        if self.inner.preferred_x.get().is_none() {
            self.inner.preferred_x.set(Some(x));
        }
        self.break_run();
        self.note_caret_activity();
    }

    /// Caret x in device px, composite-relative (Round 22.2): the
    /// visual-line lookup when shaped, else the composite start
    /// (graceful without a shaper — decision-207 rule).
    pub(crate) fn caret_visual_x(&self, byte: usize) -> f32 {
        let composite = self.composite_text();
        if composite.is_empty() {
            return 0.0;
        }
        let Some(lines) = self.shaped_visual_lines(&composite) else {
            return 0.0;
        };
        let byte = byte.min(composite.len());
        let line = line_containing(&lines, &composite, byte);
        x_in_line(line, byte)
    }

    /// Byte `dir` lines from `caret` at preferred x `x` (Round
    /// 22.2): hard and wrapped lines alike, column affinity by
    /// midpoint. Past the first line goes to the line start, past
    /// the last to the text end. Without a shaper, hard-line
    /// character columns (graceful, decision-207).
    pub(crate) fn line_byte(&self, dir: i32, caret: usize, x: f32) -> usize {
        let composite = self.composite_text();
        if composite.is_empty() {
            return 0;
        }
        let caret = caret.min(composite.len());
        let Some(lines) = self.shaped_visual_lines(&composite) else {
            return hard_line_byte(&composite, caret, dir, x);
        };
        let li = line_index_containing(&lines, &composite, caret);
        if dir < 0 {
            if li == 0 {
                return lines[0].start;
            }
            return byte_at_x(&lines[li - 1], &composite, x);
        }
        if li + 1 >= lines.len() {
            return composite.len();
        }
        byte_at_x(&lines[li + 1], &composite, x)
    }

    /// Visual lines over the composite (Round 22.2): shaped +
    /// wrapped at the published width (UAX #14 opportunities flow
    /// through the shaper path — `layout_text` consumes the
    /// installed break source the way the engine does). `None`
    /// with no wrap width, empty text, or no shaper (callers fall
    /// back to hard lines).
    pub(crate) fn shaped_visual_lines(&self, composite: &str) -> Option<Vec<VisualLine>> {
        let width = self.wrap_width()?;
        if composite.is_empty() {
            return None;
        }
        let run = self.shape_cached(composite)?;
        let metrics = run.runs.first().map(|r| r.font_metrics)?;
        let laid = crate::layout::layout_text(
            &run,
            composite,
            width,
            None,
            metrics.ascent,
            metrics.descent,
            metrics.line_gap,
        );
        // Line starts: each laid line's first cluster (visual
        // order = logical for the LTR wrap scope here; bidi
        // line-edge affinity follows the suite-guided standing
        // item) plus every hard-line start (blank lines own no
        // clusters but still break).
        let mut breaks = vec![0usize];
        for line in &laid {
            if let Some(c) = line.clusters.first() {
                breaks.push(c.byte_range.0);
            }
        }
        let mut s = 0usize;
        for part in composite.split_inclusive('\n') {
            s += part.len();
            // Hard starts break (the trailing length is a span end,
            // never a break — else caret==len strands on a phantom
            // empty span).
            if s < composite.len() {
                breaks.push(s);
            }
        }
        breaks.sort_unstable();
        breaks.dedup();
        // Cluster x-map per span (logical byte order; x is visual
        // device px, composite-relative). Newline clusters map to
        // no span (terminators, not content — past-end clamps to
        // the content end instead).
        let mut all: Vec<(usize, usize, f32, f32)> = Vec::new();
        for line in &laid {
            for c in &line.clusters {
                let (s, e) = c.byte_range;
                if composite.as_bytes()[s.min(composite.len())..e.min(composite.len())]
                    .contains(&b'\n')
                {
                    continue;
                }
                all.push((s, e, c.x, c.width));
            }
        }
        all.sort_by_key(|c| c.0);
        let mut out = Vec::with_capacity(breaks.len());
        for (i, &start) in breaks.iter().enumerate() {
            let end = breaks.get(i + 1).copied().unwrap_or(composite.len());
            let clusters = all
                .iter()
                .copied()
                .filter(|c| c.0 >= start && c.0 < end.max(start + 1))
                .collect();
            out.push(VisualLine {
                start,
                end,
                clusters,
            });
        }
        Some(out)
    }

    /// Word starts + word ends over the current composite (the
    /// Ctrl-step address space, Round 22.1): every `Word`-run start
    /// and end, every `Ideograph` char start and end, plus both text
    /// ends — separators never bound. Returns `(starts, edges)`
    /// (both sorted, deduplicated; `starts` excludes the trailing
    /// edge, which opens no word).
    pub(crate) fn word_edges(&self) -> (Vec<usize>, Vec<usize>) {
        let text = self.composite_text();
        let mut starts = Vec::new();
        let mut edges = vec![0, text.len()];
        let mut run: Option<(usize, WordClass, usize)> = None;
        for (i, c) in text.char_indices() {
            match word_class(c) {
                WordClass::Word => {
                    if !matches!(run, Some((_, WordClass::Word, _))) {
                        // A class change bounds both sides (close the
                        // old run, open the new -- alnum and emoji
                        // never share a unit, UAX #29 breaks them).
                        if let Some((s, _, _)) = run.take() {
                            starts.push(s);
                            edges.push(i);
                        }
                        run = Some((i, WordClass::Word, 0));
                        edges.push(i);
                    }
                }
                WordClass::Regional => {
                    // UAX #29 WB15/WB16: pair from the last non-RI.
                    // An odd trailing count completes the pair (keep
                    // the run); an even count (or any other run)
                    // opens a fresh pair.
                    match run {
                        Some((s, WordClass::Regional, t)) if t % 2 == 1 => {
                            run = Some((s, WordClass::Regional, t + 1));
                        }
                        _ => {
                            if let Some((s, _, _)) = run.take() {
                                starts.push(s);
                                edges.push(i);
                            }
                            run = Some((i, WordClass::Regional, 1));
                            edges.push(i);
                        }
                    }
                }
                WordClass::Emoji => {
                    // ZWJ is transparent inside a regional run
                    // (WB11-class join -- the family sequence stays
                    // one unit); everywhere else the class rule
                    // holds (VS16 included: it breaks RI runs).
                    if c as u32 == 0x200D {
                        if let Some((_, WordClass::Regional, _)) = run {
                            continue;
                        }
                    }
                    if !matches!(run, Some((_, WordClass::Emoji, _))) {
                        if let Some((s, _, _)) = run.take() {
                            starts.push(s);
                            edges.push(i);
                        }
                        run = Some((i, WordClass::Emoji, 0));
                        edges.push(i);
                    }
                }
                WordClass::Ideograph => {
                    if let Some((s, _, _)) = run.take() {
                        starts.push(s);
                        edges.push(i);
                    }
                    starts.push(i);
                    edges.push(i);
                    edges.push(i + c.len_utf8());
                }
                WordClass::Separator => {
                    if let Some((s, _, _)) = run.take() {
                        starts.push(s);
                        edges.push(i);
                    }
                }
            }
        }
        if let Some((s, _, _)) = run {
            starts.push(s);
            edges.push(text.len());
        }
        starts.sort_unstable();
        starts.dedup();
        edges.sort_unstable();
        edges.dedup();
        (starts, edges)
    }

    /// The byte boundary `steps` word edges from the current caret
    /// (positive forward, negative back, clamped at both ends):
    /// Right lands on the next edge (mid-word stops at the word
    /// end first, then the next word start); Left lands on the
    /// previous edge, except from a word start it skips to the
    /// previous word start (the Windows two-phase convention —
    /// separators never land).
    pub(crate) fn word_boundary(&self, steps: i32) -> usize {
        let text = self.composite_text();
        let caret = self.composite_caret_byte().min(text.len());
        let (starts, edges) = self.word_edges();
        let mut pos = caret;
        if steps >= 0 {
            for _ in 0..steps {
                pos = edges
                    .iter()
                    .copied()
                    .find(|&b| b > pos)
                    .unwrap_or(text.len());
            }
        } else {
            for _ in 0..-steps {
                if pos > 0 && pos < text.len() && starts.contains(&pos) {
                    pos = starts.iter().copied().rev().find(|&b| b < pos).unwrap_or(0);
                } else {
                    pos = edges.iter().copied().rev().find(|&b| b < pos).unwrap_or(0);
                }
            }
        }
        pos
    }

    /// The byte boundary `steps` caret positions from the current caret
    /// (char-stepped over the current composite, clamped at both ends).
    pub(crate) fn caret_boundary(&self, steps: i32) -> usize {
        let text = self.composite_text();
        let caret = self.composite_caret_byte().min(text.len());
        // Round 33 (decision 349): step shaped clusters, not
        // scalars -- one press crosses a base+combining-mark
        // cluster (production shapers merge them per the closed
        // combining parity). No shaper degrades to scalar bounds
        // (decision 207 -- never a panic, just finer steps).
        let mut boundaries: Vec<usize> = match self.shape_cached(&text) {
            Some(run) => {
                let mut b: Vec<usize> = run.clusters.iter().map(|c| c.byte_range.0).collect();
                b.sort_unstable();
                b.dedup();
                b
            }
            None => text.char_indices().map(|(b, _)| b).collect(),
        };
        boundaries.push(text.len());
        let idx = boundaries
            .iter()
            .position(|&b| b >= caret)
            .unwrap_or(boundaries.len() - 1);
        let target = (idx as i64 + steps as i64).clamp(0, boundaries.len() as i64 - 1) as usize;
        boundaries[target]
    }

    pub fn caret_to_end(&self) {
        let end = self.composite_text().len();
        *self.inner.caret.borrow_mut() = end;
        *self.inner.sel.borrow_mut() = (end, end);
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
    }

    pub fn caret_to_start(&self) {
        *self.inner.caret.borrow_mut() = 0;
        *self.inner.sel.borrow_mut() = (0, 0);
        self.clear_caret_anchors();
        self.break_run();
        self.note_caret_activity();
    }

    /// Extends the selection to the content start (Round 22.1 —
    /// Shift+Home): spans from the shift anchor (persisted) to
    /// byte 0, caret leading.
    pub fn extend_to_start(&self) {
        self.ensure_clamped();
        let anchor = self.extend_anchor();
        *self.inner.sel.borrow_mut() = (0, anchor);
        *self.inner.caret.borrow_mut() = 0;
        *self.inner.shift_anchor.borrow_mut() = Some(anchor);
        self.inner.preferred_x.set(None);
        self.break_run();
        self.note_caret_activity();
    }

    /// Extends the selection to the content end (Round 22.1 —
    /// Shift+End): spans from the shift anchor to the trailing
    /// edge, caret leading.
    pub fn extend_to_end(&self) {
        self.ensure_clamped();
        let anchor = self.extend_anchor();
        let end = self.composite_text().len();
        *self.inner.sel.borrow_mut() = (anchor, end);
        *self.inner.caret.borrow_mut() = end;
        *self.inner.shift_anchor.borrow_mut() = Some(anchor);
        self.inner.preferred_x.set(None);
        self.break_run();
        self.note_caret_activity();
    }

    /// Select-all: the selection spans the whole composite, the caret at
    /// its trailing edge (spike parity). Re-seeds the shift anchor
    /// at 0 (a following Shift+arrow extends from the head, native).
    pub fn select_all(&self) {
        let end = self.composite_text().len();
        *self.inner.caret.borrow_mut() = end;
        *self.inner.sel.borrow_mut() = (0, end);
        *self.inner.shift_anchor.borrow_mut() = Some(0);
        self.inner.preferred_x.set(None);
        self.break_run();
        self.note_caret_activity();
    }

    /// Shapes `text` through the installed shaper. `None` with no shaper
    /// (graceful — decision 207); panics loudly when an installed shaper
    /// fails on non-empty text (backend contract violation).
    pub(crate) fn shape_cached(&self, text: &str) -> Option<ShapedRun> {
        if let Some((cached, run)) = self.inner.shaped_cache.borrow().as_ref() {
            if cached == text {
                return Some(run.clone());
            }
        }
        let (service, style) = self.inner.shaper.borrow().as_ref().cloned()?;
        let run = service.shape(text, &style).unwrap_or_else(|e| {
            panic!(
                "installed text shaper failed on {text:?}: {e:?} — \
                 non-empty text must shape (backend contract, decision 207)"
            )
        });
        *self.inner.shaped_cache.borrow_mut() = Some((text.to_string(), run.clone()));
        Some(run)
    }

    /// x (device px, composite-relative) → composite byte via the
    /// cluster-midpoint leading-edge rule. `None` with no shaper.
    pub(crate) fn hit_test(&self, x: f32) -> Option<usize> {
        let composite = self.composite_text();
        if composite.is_empty() {
            return Some(0);
        }
        let run = self.shape_cached(&composite)?;
        Some(run.byte_offset_for_x(x))
    }

    /// The spike's word rule (spike parity, OQ-G1-3): contiguous
    /// alphanumeric runs form one word; each CJK ideograph/kana/hangul
    /// character is its own word; everything else is a separator
    /// (double-click selects nothing). Full dictionary segmentation is
    /// a later round.
    pub(crate) fn word_range(&self, byte: usize) -> (usize, usize) {
        let text = self.composite_text();
        if text.is_empty() {
            return (0, 0);
        }
        let byte = clamp_byte(&text, byte);
        let mut char_start = 0usize;
        let mut clicked = '\0';
        for (i, c) in text.char_indices() {
            if i > byte {
                break;
            }
            char_start = i;
            clicked = c;
        }
        match word_class(clicked) {
            WordClass::Separator => (byte, byte),
            WordClass::Ideograph => (char_start, char_start + clicked.len_utf8()),
            WordClass::Word => {
                let mut start = char_start;
                let mut end = char_start + clicked.len_utf8();
                while start > 0 {
                    let Some((prev_start, prev)) = text[..start].char_indices().last() else {
                        break;
                    };
                    if word_class(prev) != WordClass::Word {
                        break;
                    }
                    start = prev_start;
                }
                for c in text[end..].chars() {
                    if word_class(c) != WordClass::Word {
                        break;
                    }
                    end += c.len_utf8();
                }
                (start, end)
            }
            WordClass::Emoji => {
                // Emoji run expansion (Round 31): same walk with the
                // Emoji class, so ZWJ sequences select whole.
                let mut start = char_start;
                let mut end = char_start + clicked.len_utf8();
                while start > 0 {
                    let Some((prev_start, prev)) = text[..start].char_indices().last() else {
                        break;
                    };
                    if word_class(prev) != WordClass::Emoji {
                        break;
                    }
                    start = prev_start;
                }
                for c in text[end..].chars() {
                    if word_class(c) != WordClass::Emoji {
                        break;
                    }
                    end += c.len_utf8();
                }
                (start, end)
            }
            WordClass::Regional => {
                // Pair from the last non-RI (Round 32, WB15/WB16):
                // collect the consecutive run, align the clicked
                // index down to its pair, take two (or the lone
                // tail). A ZWJ glued inside an RI run selects alone
                // here (pathological input -- no real flag sequence
                // joins with ZWJ; edges still step it as one unit).
                let mut run_start = char_start;
                while run_start > 0 {
                    let Some((prev_start, prev)) = text[..run_start].char_indices().last() else {
                        break;
                    };
                    if word_class(prev) != WordClass::Regional {
                        break;
                    }
                    run_start = prev_start;
                }
                let mut ris: Vec<usize> = Vec::new();
                let mut bb = run_start;
                while bb < text.len() {
                    let Some(ch) = text[bb..].chars().next() else {
                        break;
                    };
                    if word_class(ch) != WordClass::Regional {
                        break;
                    }
                    ris.push(bb);
                    bb += ch.len_utf8();
                }
                let k = ris.iter().position(|&x| x == char_start).unwrap_or(0);
                let pair = k & !1;
                let first = ris[pair];
                let first_len = text[first..]
                    .chars()
                    .next()
                    .map(|c| c.len_utf8())
                    .unwrap_or(0);
                let end = match ris.get(pair + 1) {
                    Some(&second) => {
                        second
                            + text[second..]
                                .chars()
                                .next()
                                .map(|c| c.len_utf8())
                                .unwrap_or(0)
                    }
                    None => first + first_len,
                };
                (first, end)
            }
        }
    }
}

pub(crate) fn trim_to_depth(stack: &mut Vec<EditSnapshot>) {
    while stack.len() > EDIT_UNDO_DEPTH {
        stack.remove(0);
    }
}

pub(crate) fn ordered(sel: (usize, usize)) -> (usize, usize) {
    (sel.0.min(sel.1), sel.0.max(sel.1))
}

/// Floors `byte` onto a char boundary of `s` (clamping OOB first), so
/// platform-supplied offsets can never panic a slice (decision 207).
pub(crate) fn clamp_byte(s: &str, byte: usize) -> usize {
    let mut b = byte.min(s.len());
    while !s.is_char_boundary(b) {
        b -= 1;
    }
    b
}
