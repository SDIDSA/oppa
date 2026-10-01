//! Regression tests for the Windows-GPU arm's editing session (the model
//! under test). No DirectWrite dependency: a synthetic TextService with
//! hand-built cluster maps, mirroring M0b's core-test approach.

use oppa::ime::ImeCompositionHandler;
use oppa::reactive::Runtime;
use oppa::text::{
    Cluster, FontMetrics, ShapedGlyph, ShapedRun, TextError, TextRun, TextService, TextStyle,
};
use spike_textedit::rig::ImeStep;
use spike_textedit::session::{run_ime_steps, EditingSession};
use std::cell::RefCell;
use std::rc::Rc;

/// Fake shaper: one glyph per char, 10px per ASCII char, 16px per CJK.
struct FakeService;

impl TextService for FakeService {
    fn enumerate_fonts(&self) -> Vec<oppa::text::FontInfo> {
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
                font_id: oppa::text::FontId(0),
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

fn session(base: &str) -> EditingSession {
    let rt = Runtime::new();
    EditingSession::new(
        rt,
        Rc::new(FakeService),
        TextStyle::new("Fake", 16.0),
        base.to_string(),
        Rc::new(RefCell::new(Vec::new())),
    )
}

fn cp_index(session: &EditingSession, content: &str, caret: usize) -> usize {
    let _ = session;
    content[..caret].chars().count()
}

#[test]
fn insert_replaces_selection_and_moves_caret() {
    let mut s = session("Hello world");
    s.caret_move(3);
    // Fake advances: 10px per ASCII char. x=55 = trailing half of the space
    // cluster [50,60) → byte 6 → selection [3,6).
    s.shift_click_x(55.0);
    assert_eq!(s.selection(), (3, 6));
    s.insert("!");
    // [3,6) removed ("lo "), "!" inserted → "Hel!world".
    assert_eq!(s.content.get(), "Hel!world");
    assert_eq!(s.observable().caret, 4);
}

#[test]
fn undo_is_single_level_and_restores_state() {
    let mut s = session("ab");
    s.caret_to_end();
    s.insert("c");
    assert_eq!(s.content.get(), "abc");
    s.undo();
    assert_eq!(s.content.get(), "ab");
    assert_eq!(s.observable().caret, 2);
    // Second undo: stack empty, no-op.
    s.undo();
    assert_eq!(s.content.get(), "ab");
}

#[test]
fn word_rule_latin_and_ideographs() {
    // "Hello 日本語x": "Hello " = 6 chars × 10px = 60px; each CJK char 16px.
    let mut s = session("Hello 日本語x");
    s.dbl_click_x(80.0); // leading half of 本 [76,92) → 本 is its own word
    assert_eq!(s.selection(), (9, 12), "ideograph is its own word");
    s.dbl_click_x(4.0); // leading half of 'e' → "Hello" [0,5)
    assert_eq!(s.selection(), (0, 5));
    s.dbl_click_x(54.0); // inside the space → nothing selected at the caret
    assert_eq!(s.selection().0, s.selection().1);
}

#[test]
fn composition_commit_is_atomic_and_anchored() {
    let mut s = session("abc ");
    run_ime_steps(
        &mut s,
        &[
            ImeStep::Start { start_byte: 4 },
            ImeStep::Update {
                composition: "nihao".to_string(),
                caret_byte: 9,
            },
            ImeStep::Commit {
                committed: "你好".to_string(),
            },
        ],
    );
    assert_eq!(s.content.get(), "abc 你好");
    // 你好 = 6 UTF-8 bytes; caret = anchor 4 + 6.
    assert_eq!(s.observable().caret, 10);
    assert_eq!(s.composition_string(), "");
}

#[test]
fn composition_cancel_commits_nothing() {
    let mut s = session("");
    run_ime_steps(
        &mut s,
        &[
            ImeStep::Start { start_byte: 0 },
            ImeStep::Update {
                composition: "nih".to_string(),
                caret_byte: 3,
            },
            ImeStep::Cancel,
        ],
    );
    assert_eq!(s.content.get(), "");
    assert_eq!(s.composition_string(), "");
}

#[test]
fn delete_range_reanchors_active_composition() {
    let mut s = session("abc");
    run_ime_steps(
        &mut s,
        &[
            ImeStep::Start { start_byte: 3 },
            ImeStep::Update {
                composition: "nihao".to_string(),
                caret_byte: 8,
            },
            ImeStep::DeleteRange { range: (0, 3) },
            ImeStep::Commit {
                committed: "你好".to_string(),
            },
        ],
    );
    assert_eq!(s.content.get(), "你好");
    assert_eq!(s.observable().caret, 6);
}

#[test]
fn focus_loss_cancels_per_session_policy() {
    let mut s = session("");
    run_ime_steps(
        &mut s,
        &[
            ImeStep::Start { start_byte: 0 },
            ImeStep::Update {
                composition: "nih".to_string(),
                caret_byte: 3,
            },
            ImeStep::FocusLoss,
        ],
    );
    assert_eq!(
        s.content.get(),
        "",
        "focus loss commits nothing (session policy)"
    );
}

#[test]
fn caret_rect_follows_composite_and_undo_snapshot() {
    let mut s = session("");
    run_ime_steps(
        &mut s,
        &[
            ImeStep::Start { start_byte: 0 },
            ImeStep::Update {
                composition: "ab".to_string(),
                caret_byte: 2,
            },
        ],
    );
    // Composite "ab" = 2 ASCII glyphs × 10px → caret at composition end = 20px.
    let rect = s.caret_rect().expect("caret rect for composite");
    assert_eq!(rect.x, 20.0);
    assert_eq!(s.composite_text(), "ab");
    // The pre-composition snapshot is the undo unit for the atomic commit.
    s.composition_committed("你好");
    assert_eq!(s.content.get(), "你好");
    s.undo();
    assert_eq!(
        s.content.get(),
        "",
        "undo of an IME commit restores pre-composition state"
    );
}

#[test]
fn canonical_stream_records_every_event_exactly_once() {
    let mut s = session("");
    run_ime_steps(
        &mut s,
        &[
            ImeStep::Start { start_byte: 0 },
            ImeStep::Update {
                composition: "ni".to_string(),
                caret_byte: 2,
            },
            ImeStep::Commit {
                committed: "你好".to_string(),
            },
        ],
    );
    let canon = s.take_canonical();
    let phases: Vec<&str> = canon.iter().map(|c| c.phase).collect();
    assert_eq!(phases, vec!["start", "update", "commit"]);
    assert_eq!(canon[2].content, "你好");
    assert_eq!(cp_index(&s, &canon[2].content, canon[2].caret_byte), 2);
}

#[test]
fn caret_move_steps_by_cluster() {
    let mut s = session("日本語x");
    s.caret_move(2);
    assert_eq!(cp_index(&s, &s.content.get(), s.observable().caret), 2);
    s.caret_move(-5);
    assert_eq!(
        cp_index(&s, &s.content.get(), s.observable().caret),
        0,
        "clamped at start"
    );
    s.caret_move(99);
    assert_eq!(
        cp_index(&s, &s.content.get(), s.observable().caret),
        4,
        "clamped at end"
    );
}
