use super::*;
use crate::clipboard::{Clipboard, ClipboardError, InMemoryClipboard};
use crate::component::{ComponentHost, Ctx};
use crate::ime::ImeCompositionHandler;
use crate::input::InputEvent;
use crate::text::{Cluster, FontId, FontMetrics, ShapedGlyph, TextError, TextRun};
use crate::vnode::{Div, VNode};

/// Fake shaper (spike-test parity): one glyph per char, 10px per
/// ASCII char, 16px per CJK.
struct FakeService;

impl TextService for FakeService {
    fn enumerate_fonts(&self) -> Vec<crate::text::FontInfo> {
        Vec::new()
    }
    fn shape(&self, text: &str, _style: &TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        let glyphs: Vec<ShapedGlyph> = text
            .chars()
            .map(|c| ShapedGlyph {
                glyph_id: c as u32,
                x_advance: if c.is_ascii() { 10.0 } else { 16.0 },
                x_offset: 0.0,
                y_offset: 0.0,
            })
            .collect();
        let mut clusters = Vec::new();
        for (gi, (byte, c)) in text.char_indices().enumerate() {
            clusters.push(Cluster {
                byte_range: (byte, byte + c.len_utf8()),
                glyph_range: (gi, gi + 1),
            });
        }
        let glyph_count = glyphs.len();
        let total: f32 = glyphs.iter().map(|g| g.x_advance).sum();
        Ok(ShapedRun {
            glyphs,
            runs: vec![TextRun {
                byte_range: (0, text.len()),
                glyph_range: (0, glyph_count),
                rtl: false,
                script: 0,
                font_id: FontId(0),
                font_metrics: FontMetrics {
                    ascent: 12.0,
                    descent: 4.0,
                    line_gap: 0.0,
                },
            }],
            clusters,
            total_advance: total,
            text_len_bytes: text.len(),
        })
    }
}

fn session(base: &str) -> EditSession {
    let rt = Runtime::new();
    let content = rt.signal(SharedString::from(base));
    let s = EditSession::new(rt, content);
    s.set_shaper(Rc::new(FakeService), TextStyle::new("Fake", 16.0));
    s.caret_to_end();
    s
}

fn session_no_shaper(base: &str) -> EditSession {
    let rt = Runtime::new();
    let content = rt.signal(SharedString::from(base));
    EditSession::new(rt, content)
}

#[test]
fn insert_replaces_selection_and_moves_caret() {
    let s = session("Hello world");
    s.caret_to_start();
    s.caret_move(3);
    // 10px per ASCII char: x=55 = trailing half of the space cluster
    // [50,60) → byte 6 → selection [3,6).
    s.shift_click_x(55.0);
    assert_eq!(s.selection(), (3, 6));
    s.insert("!");
    assert_eq!(s.content_text(), "Hel!world");
    assert_eq!(s.observable().caret, 4);
}

#[test]
fn typing_run_coalesces_into_one_undo() {
    let s = session("");
    s.insert("a");
    s.insert("b");
    s.insert("c");
    assert_eq!(s.content_text(), "abc");
    assert_eq!(s.undo_depth(), 1);
    s.undo();
    assert_eq!(s.content_text(), "");
    assert_eq!(s.redo_depth(), 1);
    s.redo();
    assert_eq!(s.content_text(), "abc");
}

#[test]
fn caret_move_breaks_the_insert_run() {
    let s = session("");
    s.insert("a");
    s.caret_move(-1);
    s.caret_move(1);
    s.insert("b");
    assert_eq!(s.content_text(), "ab");
    assert_eq!(s.undo_depth(), 2);
    s.undo();
    assert_eq!(s.content_text(), "a");
    s.undo();
    assert_eq!(s.content_text(), "");
}

#[test]
fn new_edit_clears_redo() {
    let s = session("");
    s.insert("a");
    s.caret_to_start();
    s.insert("b");
    s.undo();
    assert_eq!(s.redo_depth(), 1);
    s.caret_to_end();
    s.insert("c");
    assert_eq!(s.redo_depth(), 0);
    assert_eq!(s.content_text(), "ac");
}

#[test]
fn backspace_run_coalesces_and_forward_is_separate() {
    let s = session("");
    s.insert("abc");
    s.caret_to_start(); // breaks the insert run
    s.caret_to_end();
    s.backspace();
    s.backspace();
    assert_eq!(s.content_text(), "a");
    // Undo entries: pre-"abc" + pre-backspace-run = 2.
    assert_eq!(s.undo_depth(), 2);
    s.undo();
    assert_eq!(s.content_text(), "abc");
    s.redo();
    assert_eq!(s.content_text(), "a");
    // Delete-forward at end is a no-op (no undo entry).
    s.caret_to_end();
    let depth = s.undo_depth();
    s.delete_forward();
    assert_eq!(s.content_text(), "a");
    assert_eq!(s.undo_depth(), depth);
}

#[test]
fn backspace_at_start_is_noop() {
    let s = session("a");
    s.caret_to_start();
    let depth = s.undo_depth();
    s.backspace();
    assert_eq!(s.content_text(), "a");
    assert_eq!(s.undo_depth(), depth);
}

#[test]
fn undo_is_bounded_and_redo_restores() {
    let s = session_no_shaper("");
    for _ in 0..(EDIT_UNDO_DEPTH + 8) {
        s.caret_to_end(); // breaks coalescing: every insert is discrete
        s.insert("x");
    }
    assert_eq!(s.undo_depth(), EDIT_UNDO_DEPTH);
    assert_eq!(s.content_text().len(), EDIT_UNDO_DEPTH + 8);
    for _ in 0..EDIT_UNDO_DEPTH {
        s.undo();
    }
    assert_eq!(s.content_text().len(), 8, "oldest entries were dropped");
    // Redo restores everything still stacked.
    for _ in 0..EDIT_UNDO_DEPTH {
        s.redo();
    }
    assert_eq!(s.content_text().len(), EDIT_UNDO_DEPTH + 8);
    // Undo/redo past the stacks are quiet no-ops.
    s.undo();
    s.redo();
}

#[test]
fn composition_commit_is_atomic_and_anchored() {
    let mut s = session("abc ");
    s.composition_started(4);
    s.composition_updated("nihao", 9);
    s.composition_committed("你好");
    assert_eq!(s.content_text(), "abc 你好");
    // 你好 = 6 UTF-8 bytes; caret = anchor 4 + 6.
    assert_eq!(s.observable().caret, 10);
    assert_eq!(s.composition_text(), "");
    // One undo restores the pre-composition state.
    assert_eq!(s.undo_depth(), 1);
    s.undo();
    assert_eq!(s.content_text(), "abc ");
}

#[test]
fn focus_loss_commits_per_locked_27() {
    let mut s = session("");
    s.composition_started(0);
    s.composition_updated("nih", 3);
    s.notify_focus_lost();
    assert_eq!(
        s.content_text(),
        "nih",
        "focus loss commits the in-progress composition (locked #27)"
    );
    assert!(!s.is_composing());
    // The commit is the atomic unit: one undo restores pre-composition.
    s.undo();
    assert_eq!(s.content_text(), "");
    // No composition active: no-op, no undo entry.
    let depth = s.undo_depth();
    s.notify_focus_lost();
    assert_eq!(s.undo_depth(), depth);
}

#[test]
fn composition_cancel_commits_nothing() {
    let mut s = session("");
    s.composition_started(0);
    s.composition_updated("nih", 3);
    s.composition_cancelled();
    assert_eq!(s.content_text(), "");
    assert_eq!(s.composition_text(), "");
}

#[test]
fn composition_epoch_bumps_on_signal_less_changes_only() {
    let mut s = session("");
    let epoch = s.composition_epoch();
    assert_eq!(epoch.get(), 0);
    s.composition_started(0);
    assert_eq!(epoch.get(), 1);
    s.composition_updated("nih", 3);
    assert_eq!(epoch.get(), 2);
    // Commits re-render through on_change, never the epoch.
    s.composition_committed("nih");
    assert_eq!(epoch.get(), 2, "commits ride on_change, not the epoch");
    assert!(!s.is_composing());
    // A fresh composition bumps again; cancel closes it.
    s.composition_started(3);
    assert_eq!(epoch.get(), 3);
    s.composition_cancelled();
    assert_eq!(epoch.get(), 4);
    // Idle edits (plain inserts) never touch the epoch.
    s.insert("x");
    assert_eq!(epoch.get(), 4);
}

#[test]
fn delete_range_reanchors_active_composition() {
    let mut s = session("abc");
    s.composition_started(3);
    s.composition_updated("nihao", 8);
    s.delete_range((0, 3));
    s.composition_committed("你好");
    assert_eq!(s.content_text(), "你好");
    assert_eq!(s.observable().caret, 6);
}

#[test]
fn word_rule_latin_and_ideographs() {
    // "Hello 日本語x": "Hello " = 6 chars × 10px = 60px; CJK 16px each.
    let s = session("Hello 日本語x");
    s.dbl_click_x(80.0); // leading half of 本 [76,92) → own word
    assert_eq!(s.selection(), (9, 12), "ideograph is its own word");
    s.dbl_click_x(4.0); // leading half of 'e' → "Hello" [0,5)
    assert_eq!(s.selection(), (0, 5));
    s.dbl_click_x(54.0); // inside the space → collapsed at caret
    assert_eq!(s.selection().0, s.selection().1);
}

#[test]
fn word_rule_emoji_runs_and_zwj_glue() {
    // Round 31 (decision 347): "a<popper>b" is three units (UAX
    // #29 breaks AHLetter x pictograph); FakeService measures
    // ASCII 10px, other chars 16px, so a[0,10), popper[10,26),
    // b[26,36) with bytes a[0,1), popper[1,5), b[5,6).
    let s = session("a\u{1F389}b");
    s.dbl_click_x(4.0);
    assert_eq!(s.selection(), (0, 1), "latin stays its own word");
    s.dbl_click_x(15.0);
    assert_eq!(s.selection(), (1, 5), "emoji selects its run");
    s.dbl_click_x(30.0);
    assert_eq!(s.selection(), (5, 6));
    // Word steps cross the same edges.
    s.caret_to_start();
    s.word_move(1);
    assert_eq!(s.caret(), 1, "stops at the latin end");
    s.word_move(1);
    assert_eq!(s.caret(), 5, "then the emoji run end");
    s.word_move(1);
    assert_eq!(s.caret(), 6, "lands on the trailing edge");
    // ZWJ family man+ZWJ+woman+ZWJ+girl (4+3+4+3+4 = 18 bytes)
    // selects whole: ZWJ keeps the emoji run open across the
    // join sequence.
    let f = session("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}");
    f.dbl_click_x(30.0); // inside the woman pictograph
    assert_eq!(f.selection(), (0, 18), "ZWJ sequence is one unit");
}

#[test]
fn word_rule_regional_pairs() {
    // Round 32 (decision 348, WB15/WB16): consecutive RIs pair
    // from the last non-RI; FakeService measures 16px per RI
    // (4 bytes each).
    let s = session("\u{1F1EB}\u{1F1F7}");
    s.dbl_click_x(8.0); // first flag, first half
    assert_eq!(s.selection(), (0, 8), "pair selects whole");
    s.dbl_click_x(24.0); // second half
    assert_eq!(s.selection(), (0, 8), "either half selects the pair");
    // Four RIs are two pairs, aligned from the run start.
    let q = session("\u{1F1EB}\u{1F1F7}\u{1F1E9}\u{1F1EA}");
    q.dbl_click_x(8.0);
    assert_eq!(q.selection(), (0, 8), "first pair");
    q.dbl_click_x(40.0); // third flag
    assert_eq!(q.selection(), (8, 16), "second pair");
    q.dbl_click_x(56.0); // fourth flag
    assert_eq!(q.selection(), (8, 16), "pair tail aligns back");
    // Letters and pictographs bound the run on both sides.
    let m = session("A\u{1F1EB}\u{1F1F7}");
    m.dbl_click_x(4.0);
    assert_eq!(m.selection(), (0, 1), "letter stays alone");
    m.dbl_click_x(15.0);
    assert_eq!(m.selection(), (1, 9), "pair after a letter");
    let e = session("\u{1F389}\u{1F1EB}\u{1F1F7}");
    e.dbl_click_x(6.0);
    assert_eq!(e.selection(), (0, 4), "pictograph stays alone");
    e.dbl_click_x(24.0);
    assert_eq!(e.selection(), (4, 12), "pair after a pictograph");
    // Steps cross pair edges.
    q.caret_to_start();
    q.word_move(1);
    assert_eq!(q.caret(), 8, "stops at the pair end");
    q.word_move(1);
    assert_eq!(q.caret(), 16, "then the run end");
}

/// Merging shaper (production-shaped clusters without a font
/// stack): combining marks U+0300-036F extend the previous
/// cluster instead of opening their own -- what rustybuzz and
/// DirectWrite report per the closed combining parity.
struct MergingFake;

impl TextService for MergingFake {
    fn enumerate_fonts(&self) -> Vec<crate::text::FontInfo> {
        Vec::new()
    }
    fn shape(&self, text: &str, _style: &TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        let glyphs: Vec<ShapedGlyph> = text
            .chars()
            .map(|c| ShapedGlyph {
                glyph_id: c as u32,
                x_advance: if c.is_ascii() { 10.0 } else { 16.0 },
                x_offset: 0.0,
                y_offset: 0.0,
            })
            .collect();
        let mut clusters = Vec::new();
        for (gi, (byte, c)) in text.char_indices().enumerate() {
            let combining = ('̀'..='ͯ').contains(&c);
            if combining && !clusters.is_empty() {
                let last: &mut Cluster = clusters.last_mut().unwrap();
                last.byte_range.1 = byte + c.len_utf8();
                last.glyph_range.1 = gi + 1;
            } else {
                clusters.push(Cluster {
                    byte_range: (byte, byte + c.len_utf8()),
                    glyph_range: (gi, gi + 1),
                });
            }
        }
        let glyph_count = glyphs.len();
        let total: f32 = glyphs.iter().map(|g| g.x_advance).sum();
        Ok(ShapedRun {
            glyphs,
            runs: vec![TextRun {
                byte_range: (0, text.len()),
                glyph_range: (0, glyph_count),
                rtl: false,
                script: 0,
                font_id: FontId(0),
                font_metrics: FontMetrics {
                    ascent: 12.0,
                    descent: 4.0,
                    line_gap: 0.0,
                },
            }],
            clusters,
            total_advance: total,
            text_len_bytes: text.len(),
        })
    }
}

fn session_merging(base: &str) -> EditSession {
    let rt = Runtime::new();
    let content = rt.signal(SharedString::from(base));
    let s = EditSession::new(rt, content);
    s.set_shaper(Rc::new(MergingFake), TextStyle::new("Fake", 16.0));
    s.caret_to_end();
    s
}

#[test]
fn caret_steps_clusters_not_scalars() {
    // Round 33 (decision 349): decomposed e-acute "cafe\u{301}"
    // (6 bytes) shapes one [3,6) cluster -- arrows cross it in
    // one press instead of parking mid-cluster.
    let s = session_merging("café");
    assert_eq!(s.caret(), 6);
    s.caret_left();
    assert_eq!(s.caret(), 3, "one press crosses the cluster");
    s.caret_left();
    assert_eq!(s.caret(), 2);
    s.caret_right();
    assert_eq!(s.caret(), 3);
    s.caret_right();
    assert_eq!(s.caret(), 6, "right lands past the cluster");
    // No-shaper fallback stays scalar (decision 207).
    let t = session_no_shaper("café");
    t.caret_to_end();
    t.caret_left();
    assert_eq!(t.caret(), 4, "shaperless steps scalars");
}

#[test]
fn select_line_x_picks_the_hard_line() {
    // FakeService: 10px per ASCII char. "ab\ncd": bytes 0-2, \n at
    // 2, "cd" at 3-5.
    let s = session("ab\ncd");
    s.select_line_x(35.0); // trailing half of 'c' [30,40) → byte 4
    assert_eq!(s.selection(), (3, 5), "second hard line");
    s.select_line_x(5.0); // leading half of 'a' → byte 0
    assert_eq!(s.selection(), (0, 2), "first hard line");
    // Single-line content selects whole.
    let t = session("hello");
    t.select_line_x(12.0);
    assert_eq!(t.selection(), (0, 5));
    // No shaper: graceful no-op (decision 207 — never a panic).
    let u = session_no_shaper("hello");
    u.select_line_x(12.0);
    assert_eq!(u.selection(), (0, 0));
}

#[test]
fn caret_rect_follows_composite() {
    let s = session_no_shaper("");
    assert_eq!(s.caret_rect(), None, "no shaper → no caret rect");
    let mut s = session("");
    s.composition_started(0);
    s.composition_updated("ab", 2);
    // Composite "ab" = 2 ASCII glyphs × 10px → caret at 20px.
    let rect = s.caret_rect().expect("caret rect for composite");
    assert_eq!(rect.x, 20.0);
    assert_eq!(s.composite_text(), "ab");
}

/// Round 15.1 (decision 312): the blink phase is visible at birth,
/// hides for the second half of every 1s cycle, and any caret or
/// content op resets it to solid-visible immediately.
#[test]
fn caret_blink_phase_toggles_on_clock_and_ops_reset_it() {
    use crate::clock::MockClock;
    let clock = Rc::new(MockClock::new());
    let rt = Runtime::with_clock(clock.clone());
    let content = rt.signal(SharedString::from("hello"));
    let s = EditSession::new(rt, content);
    assert!(s.caret_visible(), "birth-visible: epoch == now");
    assert_eq!(s.caret_epoch(), 0.0);
    clock.advance(0.49);
    assert!(s.caret_visible(), "still the visible half");
    clock.advance(0.02);
    assert!(!s.caret_visible(), "hidden half at t=0.51");
    s.caret_move(-1);
    assert!(s.caret_visible(), "caret move resets to solid");
    assert_eq!(s.caret_epoch(), 0.51);
    clock.advance(0.6);
    assert!(!s.caret_visible(), "hidden again without activity");
    s.insert("x");
    assert!(s.caret_visible(), "typing resets to solid");
    assert_eq!(s.content_text(), "xhello");
    assert!(
        (s.caret_epoch() - 1.11).abs() < 1e-9,
        "epoch pins the reset instant, got {}",
        s.caret_epoch()
    );
}

#[test]
fn no_shaper_pointer_ops_are_noops() {
    let s = session_no_shaper("hello");
    s.caret_to_end();
    s.click_x(5.0);
    assert_eq!(s.caret(), 5, "caret unchanged without a shaper");
    // Char-boundary ops always work without a shaper.
    s.caret_move(-2);
    assert_eq!(s.caret(), 3);
    s.insert("!");
    assert_eq!(s.content_text(), "hel!lo");
}

#[test]
fn mid_char_offsets_floor_to_boundaries() {
    let mut s = session_no_shaper("aé");
    // 'é' = bytes 1–3. A platform offset of 2 must floor to 1.
    s.composition_started(2);
    assert_eq!(s.composition_start_byte(), Some(1));
    s.composition_updated("x", 2);
    s.composition_committed("x");
    assert_eq!(s.content_text(), "axé");
}

#[test]
fn apply_platform_text_replaces_and_undoes() {
    let s = session("hello");
    let depth = s.undo_depth();
    s.apply_platform_text("hello"); // identical → no-op
    assert_eq!(s.undo_depth(), depth);
    s.apply_platform_text("hello world");
    assert_eq!(s.content_text(), "hello world");
    assert_eq!(s.caret(), 11);
    s.undo();
    assert_eq!(s.content_text(), "hello");
}

#[test]
fn insert_is_noop_while_composing() {
    let mut s = session("ab");
    s.composition_started(2);
    s.insert("X");
    assert_eq!(s.content_text(), "ab");
    assert_eq!(s.composite_text(), "ab");
    s.composition_cancelled();
}

// -- clipboard copy/cut/paste (G3, decision 211) ---------------------------

#[test]
fn copy_collapsed_leaves_clipboard_untouched() {
    let s = session("hello");
    s.caret_to_start();
    let mut clip = InMemoryClipboard::with_text("keep");
    assert_eq!(
        s.copy_selection_to(&mut clip).expect("copy refuses loudly"),
        None
    );
    assert_eq!(
        clip.read_text_now().expect("reads"),
        Some("keep".to_string()),
        "collapsed copy touches nothing"
    );
}

#[test]
fn copy_range_writes_clipboard() {
    let s = session("Hello world");
    s.caret_to_start();
    s.caret_move(5);
    s.shift_click_x(55.0); // selection [5,6) = " "
    let mut clip = InMemoryClipboard::new();
    assert_eq!(
        s.copy_selection_to(&mut clip).expect("copy refuses loudly"),
        Some(" ".to_string())
    );
    assert_eq!(clip.read_text_now().expect("reads"), Some(" ".to_string()));
    assert_eq!(s.content_text(), "Hello world", "copy does not mutate");
}

#[test]
fn cut_removes_and_undo_restores() {
    let s = session("hello");
    s.select_all();
    let mut clip = InMemoryClipboard::new();
    assert_eq!(
        s.cut_selection_to(&mut clip).expect("cut refuses loudly"),
        Some("hello".to_string())
    );
    assert_eq!(s.content_text(), "");
    assert_eq!(
        clip.read_text_now().expect("reads"),
        Some("hello".to_string())
    );
    s.undo();
    assert_eq!(s.content_text(), "hello");
}

#[test]
fn cut_is_noop_while_composing() {
    let mut s = session("ab");
    s.caret_to_end();
    s.composition_started(2);
    s.composition_updated("xy", 4);
    let mut clip = InMemoryClipboard::new();
    assert_eq!(
        s.cut_selection_to(&mut clip).expect("cut refuses loudly"),
        None
    );
    assert_eq!(
        clip.read_text_now().expect("reads"),
        None,
        "clipboard untouched mid-composition"
    );
    assert_eq!(s.content_text(), "ab");
    s.composition_cancelled();
}

#[test]
fn paste_inserts_as_discrete_undo_unit() {
    let s = session_no_shaper("");
    s.insert("a");
    let depth_after_typing = s.undo_depth();
    let mut clip = InMemoryClipboard::with_text("BC");
    assert_eq!(
        s.paste_from(&mut clip),
        Ok(PasteOutcome::Pasted("BC".to_string()))
    );
    assert_eq!(s.content_text(), "aBC");
    // The paste broke the typing run: one more entry, and undo
    // removes only the paste.
    assert_eq!(s.undo_depth(), depth_after_typing + 1);
    s.undo();
    assert_eq!(s.content_text(), "a");
}

#[test]
fn paste_empty_is_quiet_noop_without_undo() {
    let s = session("hi");
    s.caret_to_end();
    let depth = s.undo_depth();
    let mut clip = InMemoryClipboard::new();
    assert_eq!(s.paste_from(&mut clip), Ok(PasteOutcome::Empty));
    assert_eq!(s.content_text(), "hi");
    assert_eq!(s.undo_depth(), depth, "no undo entry for empty paste");
}

/// Pending-read stub: `poll_read` is `None` until settled — the web
/// backend's shape. Paste must surface `Err(Pending)`, never block
/// and never mutate.
struct PendingClipboard {
    settled: Option<String>,
}

impl Clipboard for PendingClipboard {
    fn write_text(&mut self, _text: &str) -> Result<(), ClipboardError> {
        Ok(())
    }
    fn clear(&mut self) -> Result<(), ClipboardError> {
        Ok(())
    }
    fn request_read(&mut self) {}
    fn poll_read(&mut self) -> Option<Result<Option<String>, ClipboardError>> {
        self.settled.clone().map(|t| Ok(Some(t)))
    }
}

#[test]
fn paste_pending_propagates_without_mutating() {
    let s = session("hi");
    s.caret_to_end();
    let depth = s.undo_depth();
    let mut clip = PendingClipboard { settled: None };
    assert_eq!(
        s.paste_from(&mut clip),
        Err(ClipboardError::Pending),
        "pending read surfaces loudly"
    );
    assert_eq!(s.content_text(), "hi");
    assert_eq!(s.undo_depth(), depth);
    // Once settled, the same session pastes normally.
    clip.settled = Some("!".to_string());
    assert_eq!(
        s.paste_from(&mut clip),
        Ok(PasteOutcome::Pasted("!".to_string()))
    );
    assert_eq!(s.content_text(), "hi!");
}

#[test]
fn paste_while_composing_is_named_noop() {
    let mut s = session("ab");
    s.composition_started(2);
    s.composition_updated("xy", 4);
    let depth = s.undo_depth();
    let mut clip = InMemoryClipboard::with_text("ZZ");
    assert_eq!(s.paste_from(&mut clip), Ok(PasteOutcome::WhileComposing));
    assert_eq!(s.content_text(), "ab", "committed content untouched");
    assert_eq!(s.undo_depth(), depth);
    s.composition_cancelled();
}

// -- word-step + extend-to-ends + masked (Round 22.1, decision 331) ---

#[test]
fn word_move_steps_latin_runs_with_end_stops() {
    // "hello world": edges at 0, 5, 6, 11 — Right stops at the
    // word end first, then the next word start.
    let s = session("hello world");
    s.caret_to_start();
    s.word_move(1);
    assert_eq!(s.caret(), 5, "mid-word stops at the word end");
    assert_eq!(s.selection(), (5, 5), "word move collapses");
    s.word_move(1);
    assert_eq!(s.caret(), 6, "then the next word start");
    s.word_move(1);
    assert_eq!(s.caret(), 11, "lands on the trailing edge");
    s.word_move(1);
    assert_eq!(s.caret(), 11, "pins at the end");
    s.word_move(-1);
    assert_eq!(s.caret(), 6, "steps back an edge");
    // From a word start, Left skips to the previous word start.
    s.word_move(-1);
    assert_eq!(s.caret(), 0, "word start skips back a word");
    s.word_move(-1);
    assert_eq!(s.caret(), 0, "pins at the start");
}

#[test]
fn word_move_steps_ideographs_per_character() {
    // Each CJK char is its own word: A日B本C steps per char.
    let s = session("A日B本C");
    s.caret_to_start();
    for want in [1usize, 4, 5, 8, 9] {
        s.word_move(1);
        assert_eq!(s.caret(), want, "ideographs step per character");
    }
    s.word_move(1);
    assert_eq!(s.caret(), 9, "pins at the end");
    s.word_move(-2);
    assert_eq!(s.caret(), 5, "steps back two words");
}

#[test]
fn extend_word_spans_from_anchor_to_word_edge() {
    let s = session("hello world");
    s.caret_to_start();
    s.caret_move(2);
    s.extend_word(1);
    assert_eq!(s.selection(), (2, 5), "anchor to word end");
    assert_eq!(s.caret(), 5);
    // The anchor persists (native accumulation): extending back
    // past it flips the selection around the anchor.
    s.extend_word(-1);
    assert_eq!(s.selection(), (0, 2));
    assert_eq!(s.caret(), 0);
}

#[test]
fn extend_to_ends_span_from_anchor() {
    let s = session("hello world");
    s.caret_to_start();
    s.caret_move(5);
    s.extend_to_end();
    assert_eq!(s.selection(), (5, 11), "Shift+End spans to the edge");
    assert_eq!(s.caret(), 11);
    // Anchor persists: Shift+Home spans back from the same
    // anchor instead of re-anchoring at the caret.
    s.extend_to_start();
    assert_eq!(s.selection(), (0, 5), "Shift+Home spans from the anchor");
    assert_eq!(s.caret(), 0);
}

#[test]
fn shift_extends_accumulate_and_plain_moves_collapse_first() {
    let s = session("hello world");
    s.caret_to_end();
    s.extend_caret(-1);
    assert_eq!(s.selection(), (10, 11));
    s.extend_caret(-1);
    assert_eq!(s.selection(), (9, 11), "repeated Shift+Left accumulates");
    s.extend_caret(-1);
    assert_eq!(s.selection(), (8, 11));
    // Plain move with an open selection collapses toward the
    // step direction first (native arrows).
    s.caret_move(-1);
    assert_eq!(s.selection(), (8, 8), "plain Left collapses to the head");
    assert_eq!(s.caret(), 8);
    s.select_all();
    s.caret_move(1);
    assert_eq!(s.selection(), (11, 11), "plain Right collapses to the tail");
    // After select-all the anchor re-seeds at 0: Shift+Left
    // extends from the head.
    s.select_all();
    s.extend_caret(-1);
    assert_eq!(s.selection(), (0, 10));
}

// -- absolute range set (decision 449: the proxy-selection report path) --
#[test]
fn select_range_orders_clamps_and_tracks_caret() {
    let s = session("hello world");
    s.select_range(6, 11);
    assert_eq!(s.selection(), (6, 11));
    assert_eq!(s.caret(), 11);
    // Reversed inputs order with the caret on the reported end
    // (handle-drag direction tracks, the drag_x precedent).
    s.select_range(11, 6);
    assert_eq!(s.selection(), (6, 11));
    assert_eq!(s.caret(), 6);
    // OOB and mid-char inputs floor safely (the clamp_byte rule).
    s.select_range(0, 99);
    assert_eq!(s.selection(), (0, 11));
    s.select_range(4, 5);
    assert_eq!(s.selection(), (4, 5));
    assert_eq!(s.caret(), 5);
    // Selection-only: content and undo stack untouched.
    assert_eq!(s.content_text(), "hello world");
    assert_eq!(s.undo_depth(), 0);
}

#[test]
fn select_range_multibyte_floors_to_boundaries() {
    let s = session("a\u{1f44d}b");
    // Thumbs-up spans bytes 1..5: edges inside it floor outward to
    // whole chars (never a half unit on either side).
    s.select_range(2, 6);
    assert_eq!(s.selection(), (1, 6));
    s.select_range(0, 2);
    assert_eq!(s.selection(), (0, 1));
}

#[test]
fn select_range_is_quiet_noop_while_composing() {
    let mut s = session("hello");
    s.select_range(0, 5);
    // Opening a composition collapses the selection itself (its own
    // rule); the report arriving after must move nothing further.
    s.composition_started(0);
    assert!(s.is_composing());
    let frozen = s.selection();
    s.select_range(1, 3);
    assert_eq!(s.selection(), frozen, "composing selection untouched");
    assert_eq!(s.content_text(), "hello", "composing content untouched");
}

// -- renderer-drawn handle drags (decision 454) --
#[test]
fn move_handle_edge_tracks_the_dragged_edge() {
    use crate::input::HandleEdge;
    let s = session("hello world");
    s.select_range(2, 8);
    // Start edge moves, anchor holds, caret follows the finger.
    s.move_handle_edge(HandleEdge::Start, 0);
    assert_eq!(s.selection(), (0, 8));
    assert_eq!(s.caret(), 0);
    // End edge moves the same way.
    s.move_handle_edge(HandleEdge::End, 11);
    assert_eq!(s.selection(), (0, 11));
    assert_eq!(s.caret(), 11);
    // Selection-only: content and undo stack untouched.
    assert_eq!(s.content_text(), "hello world");
    assert_eq!(s.undo_depth(), 0);
}

#[test]
fn move_handle_edge_never_crosses_and_ignores_collapsed() {
    use crate::input::HandleEdge;
    let mut s = session("hello world");
    s.select_range(2, 8);
    // Dragging an edge past its anchor collapses onto the anchor
    // (edges never cross, stated).
    s.move_handle_edge(HandleEdge::Start, 10);
    assert_eq!(s.selection(), (8, 8));
    assert_eq!(s.caret(), 8);
    s.select_range(2, 8);
    s.move_handle_edge(HandleEdge::End, 0);
    assert_eq!(s.selection(), (2, 2));
    assert_eq!(s.caret(), 2);
    // Collapsed selections have no handles: quiet no-op (never a
    // live path — the Down could never have armed one).
    s.move_handle_edge(HandleEdge::Start, 0);
    assert_eq!(s.selection(), (2, 2));
    // Composing owns the field: quiet no-op.
    s.select_range(0, 5);
    s.composition_started(0);
    let frozen = s.selection();
    s.move_handle_edge(HandleEdge::End, 11);
    assert_eq!(s.selection(), frozen, "composing selection untouched");
    assert_eq!(s.content_text(), "hello world");
}

#[test]
fn byte_at_x_maps_edges_and_needs_a_shaper() {
    // 10px per ASCII char: x=5.0 lands on the first cluster's
    // trailing half → byte 1; x=4.9 on its leading half → byte 0.
    let s = session("hello");
    assert_eq!(s.byte_at_x(4.9), Some(0));
    assert_eq!(s.byte_at_x(5.0), Some(1));
    // Past the end clamps to the length, never None.
    assert_eq!(s.byte_at_x(1000.0), Some(5));
    // No shaper: graceful None (pointer ops never panic setup).
    let bare = session_no_shaper("hello");
    assert_eq!(bare.byte_at_x(5.0), None);
}

// -- masked display mapping (decision 458: bullets on screen,
// cleartext in session) --
#[test]
fn masked_display_text_masks_per_char() {
    let s = session("aé");
    assert_eq!(s.display_text(), "aé", "unmasked shows content");
    s.set_masked(true);
    assert_eq!(s.display_text(), "••", "one bullet per char");
    let empty = session("");
    empty.set_masked(true);
    assert_eq!(empty.display_text(), "");
}

#[test]
fn masked_byte_mapping_round_trips_through_char_indices() {
    let s = session("aé");
    s.set_masked(true);
    // Content "aé": 'a' = byte 0, 'é' = bytes 1..3. Display "••":
    // first bullet = bytes 0..3, second = bytes 3..6.
    assert_eq!(s.display_byte_to_composite(0), 0);
    assert_eq!(s.display_byte_to_composite(3), 1);
    assert_eq!(s.display_byte_to_composite(99), 3);
    assert_eq!(s.composite_byte_to_display(0), 0);
    assert_eq!(s.composite_byte_to_display(1), 3);
    assert_eq!(s.composite_byte_to_display(99), 6);
    // Unmasked is the plain clamp (mid-char floors to the boundary).
    let u = session("aé");
    assert_eq!(u.display_byte_to_composite(2), 1);
    assert_eq!(u.composite_byte_to_display(2), 1);
}

#[test]
fn masked_pointer_ops_land_on_content_chars() {
    // FakeText advances 10px/char at size 16: masked "hello" lays
    // six bullets; tapping the second bullet maps to content byte 1
    // (not bullet byte 4, which would strand the caret mid-content).
    let s = session("hello");
    s.set_masked(true);
    assert_eq!(s.byte_at_x(15.0), Some(1));
    s.click_x(15.0);
    assert_eq!(s.selection(), (1, 1));
    // Double-tap selects the CONTENT word, not bullet separators.
    s.dbl_click_x(25.0);
    assert_eq!(s.selection(), (0, 5));
    // Caret paint resolves through display clusters without panic.
    assert!(s.caret_rect().is_some());
}

#[test]
fn masked_sessions_refuse_copy_and_cut_loudly_quiet() {
    let s = session("secret");
    s.set_masked(true);
    assert!(s.is_masked());
    s.select_all();
    let mut clip = InMemoryClipboard::new();
    assert_eq!(
        s.copy_selection_to(&mut clip)
            .expect("masked copy is Ok(None)"),
        None,
        "masked copy exfiltrates nothing"
    );
    assert_eq!(
        clip.read_text_now().expect("reads"),
        None,
        "clipboard untouched"
    );
    assert_eq!(
        s.cut_selection_to(&mut clip)
            .expect("masked cut is Ok(None)"),
        None,
        "masked cut exfiltrates nothing"
    );
    assert_eq!(s.content_text(), "secret", "masked cut deletes nothing");
    // Paste still lands (no exfiltration involved).
    clip.write_text("xy").expect("writes");
    assert_eq!(
        s.paste_from(&mut clip),
        Ok(PasteOutcome::Pasted("xy".to_string()))
    );
    assert_eq!(s.content_text(), "xy", "paste replaces the selection");
    // Unmasked sessions behave exactly as before.
    let u = session("plain");
    assert!(!u.is_masked(), "sessions start unmasked");
    u.select_all();
    let mut clip2 = InMemoryClipboard::new();
    assert_eq!(
        u.copy_selection_to(&mut clip2).expect("copy works"),
        Some("plain".to_string())
    );
}

// -- visual lines (Round 22.2, decision 332) -------------------------------

#[test]
fn enter_inserts_newline_and_moves_caret() {
    let s = session("ab");
    s.insert("\n");
    assert_eq!(s.content_text(), "ab\n");
    assert_eq!(s.caret(), 3, "caret advances past the newline");
    s.insert("c");
    assert_eq!(s.content_text(), "ab\nc");
}

#[test]
fn line_up_down_travels_hard_lines_with_column_affinity() {
    // FakeService: 10px per ASCII char; no wrap at 1000px.
    let s = session("ab\ncdefghij");
    s.set_wrap_width(Some(1000.0));
    assert!(s.is_multiline());
    s.caret_to_end();
    // End of "cdefghij" (x=80) → Up onto "ab" end.
    s.line_up();
    assert_eq!(s.caret(), 2, "Up lands on the short line end");
    // Up again goes to the line start; Down walks back out on
    // the preserved anchor (past the short end to the text end).
    s.line_up();
    assert_eq!(s.caret(), 0, "Up from the first line goes home");
    s.line_down();
    assert_eq!(s.caret(), 11, "Down rides the anchor past the end");
    // Fresh column mid-line round-trips exactly.
    s.caret_to_start();
    s.caret_move(2);
    s.line_down();
    assert_eq!(s.caret(), 5, "Down keeps the column");
    s.line_up();
    assert_eq!(s.caret(), 2, "Up restores it");
}

#[test]
fn line_up_down_travels_wrapped_lines() {
    // 10px chars at 35px wrap: "abcdef" → "abc" + "def".
    let s = session("abcdef");
    s.set_wrap_width(Some(35.0));
    s.caret_to_start();
    s.caret_move(2);
    s.line_down();
    assert_eq!(s.caret(), 5, "Down keeps the column across the wrap");
    s.line_up();
    assert_eq!(s.caret(), 2, "Up round-trips the column");
}

#[test]
fn extend_line_spans_from_anchor() {
    let s = session("ab\ncd");
    s.set_wrap_width(Some(1000.0));
    s.caret_to_end();
    s.extend_line_up();
    assert_eq!(s.selection(), (2, 5), "Shift+Up spans to the line above");
    assert_eq!(s.caret(), 2);
    // The anchor persists: Shift+Down walks back onto it,
    // collapsing the selection.
    s.extend_line_down();
    assert_eq!(s.selection(), (5, 5), "Shift+Down returns to the anchor");
    assert_eq!(s.caret(), 5);
}

#[test]
fn line_ops_without_wrap_width_take_hard_lines() {
    // Graceful without a shaper-driven wrap (decision-207):
    // hard lines with character columns, never a panic.
    let s = session("ab\ncd");
    assert!(!s.is_multiline(), "single-line by default");
    s.caret_to_end();
    s.line_up();
    assert_eq!(s.caret(), 2, "Up takes the hard line above");
    s.line_down();
    assert_eq!(s.caret(), 5, "Down round-trips the column");
}

#[test]
fn horizontal_moves_reset_column_affinity() {
    let s = session("abcdef");
    s.set_wrap_width(Some(35.0));
    s.caret_to_start();
    s.caret_move(2);
    s.line_down();
    assert_eq!(s.caret(), 5);
    // A horizontal move re-seeds the anchor: Up travels from
    // the fresh column, then Down round-trips it.
    s.caret_move(-1);
    assert_eq!(s.caret(), 4);
    s.line_up();
    assert_eq!(s.caret(), 1, "Up travels from the fresh column");
    s.line_down();
    assert_eq!(s.caret(), 4, "Down round-trips it");
}

fn field_render(ctx: &Ctx, _p: &()) -> VNode {
    let content = ctx.signal(SharedString::from("hi"));
    let _sess = ctx.edit_session(content);
    Div("root").build()
}

#[test]
fn ctx_edit_session_persists_across_runs() {
    let host = ComponentHost::new();
    let handle = host.mount("Field", (), field_render);
    let root = handle.root_instance();
    host.run_until_idle();
    let sessions = host.edit_sessions_for(root);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].content_text(), "hi");
    // Mutate through the handle, re-render with new props, prove the
    // state was not re-seeded (same instance + same call site).
    let sess = sessions[0].clone();
    sess.caret_to_end();
    sess.insert("!");
    handle.set_props(());
    host.run_until_idle();
    let sessions = host.edit_sessions_for(root);
    assert_eq!(sessions.len(), 1, "same call site → same session");
    assert_eq!(sessions[0].content_text(), "hi!");
}

fn focus_render(ctx: &Ctx, _p: &()) -> VNode {
    let content = ctx.signal(SharedString::from("hi"));
    let _sess = ctx.edit_session(content);
    Div("root").children([
        Div("a").on_press(|| {}).build(),
        Div("b").on_press(|| {}).build(),
    ])
}

#[test]
fn focus_change_auto_commits_composition() {
    use crate::component::find_retained_by_debug;
    let host = ComponentHost::new();
    let handle = host.mount("FocusField", (), focus_render);
    let root = handle.root_instance();
    host.run_until_idle();
    let a = find_retained_by_debug(&host, "a")[0];
    let b = find_retained_by_debug(&host, "b")[0];
    host.inject_input(InputEvent::Focus { node: Some(a) });
    host.run_until_idle();
    let mut sess = host.edit_sessions_for(root)[0].clone();
    sess.composition_started(2);
    sess.composition_updated("xy", 4);
    assert!(host.edit_sessions_for(root)[0].is_composing());
    host.inject_input(InputEvent::Focus { node: Some(b) });
    host.run_until_idle();
    let sess = &host.edit_sessions_for(root)[0];
    assert!(!sess.is_composing(), "focus change commits the composition");
    assert_eq!(sess.content_text(), "hixy");
}

#[test]
fn bind_edit_session_feeds_text_input() {
    use crate::component::find_retained_by_debug;
    let host = ComponentHost::new();
    let handle = host.mount("FeedField", (), focus_render);
    let root = handle.root_instance();
    host.run_until_idle();
    let a = find_retained_by_debug(&host, "a")[0];
    let sess = host.edit_sessions_for(root)[0].clone();
    host.bind_edit_session(a, &sess);
    host.inject_input(InputEvent::text(a, "typed!"));
    host.run_until_idle();
    assert_eq!(sess.content_text(), "typed!");
}

/// Round 5.4 (decision 271): the committed-content observer
/// fires with the new content on every `set_content` funnel
/// pass (insert, delete, undo) and never on live composition
/// updates (composition does not touch content).
#[test]
fn on_change_fires_on_commits_never_on_live_composition() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let mut s = session("");
    let seen = Rc::new(RefCell::new(Vec::new()));
    let record = seen.clone();
    s.set_on_change(Rc::new(move |value: SharedString| {
        record.borrow_mut().push(value.to_string())
    }));
    s.insert("a");
    assert_eq!(*seen.borrow(), vec!["a".to_string()]);
    s.insert("b");
    assert_eq!(*seen.borrow(), vec!["a".to_string(), "ab".to_string()]);
    // Live composition updates content nothing (observer quiet).
    s.composition_started(2);
    s.composition_updated("ax", 3);
    assert!(s.is_composing());
    assert_eq!(seen.borrow().len(), 2, "quiet mid-composition");
    // Commit lands through the funnel (observer fires).
    s.composition_committed("ax");
    assert_eq!(s.content_text(), "abax");
    assert_eq!(
        seen.borrow().last().cloned(),
        Some("abax".to_string()),
        "commit reports"
    );
    // Undo restores through the funnel too.
    s.undo();
    assert_eq!(
        seen.borrow().last().cloned(),
        Some("ab".to_string()),
        "undo reports"
    );
}
