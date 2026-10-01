//! The shared pass/fail rig (DESIGN §9.2): corpus strings, the IME scenario
//! scripts, and the shared editing-operation suite, each defined exactly
//! once here and serialized to `corpus.json` so the Web-DOM arm (spike/web/)
//! drives the *same* corpus with the *same* expectations instead of a
//! hand-mirrored copy.
//!
//! Corpus scope, as handed to this round: ASCII baseline ("Hello world"),
//! the M0b multi-byte strings ("héllo", "日本語" — plus a "héllo 👍"
//! composite so surrogate-pair clamping is exercised by the same click
//! probes), and the zh/ja composition sequences. New this round (DESIGN
//! §2.3 blocking condition (b)): one bidi string (Latin + Arabic +
//! digits, exercising *visual reordering* — the rtl flag M0b built gets
//! its first real exercise, while visual-order verdicts are expected to
//! diverge per the M3 bidi-ordering deferral and are classified, not
//! covered up), one decomposed combining-mark string ("cafe\u{301}",
//! pairing with precomposed "héllo" to test grapheme-cluster parity),
//! and one ZWJ emoji sequence (single-cluster-ness). Op-suite bases are
//! unchanged: editing ops on bidi/combining/ZWJ text is deeper scope
//! than this measurement round (geometry + hit-test + cluster
//! round-trips); stated, not silently skipped.

use crate::json::J;
use oppa::text::{FontId, FontInfo, ShapedRun, TextService, TextStyle};

pub const RIG_VERSION: u32 = 2;

/// §9.2 criterion 1's tolerance placeholder, concretized: **2 device px**.
/// Matches M0b's handoff note ("caret ±2 device px, never across cluster
/// boundaries"), is 1 CSS px at DPR 2, and is far inside DESIGN's
/// "within one caret height" bound for candidate-window anchoring — but it
/// is also the *tightest* bound that does not demand two different
/// rendering engines (here: GetGlyphs/GetGlyphPlacements vs
/// IDWriteTextLayout) agree subpixel-exactly.
pub const TOLERANCE_PX: f32 = 2.0;

pub const FONT_FAMILY: &str = "Segoe UI";
pub const FONT_SIZE_PX: f32 = 16.0;
/// The M0b letter-tracking convention probe (advance on every glyph except
/// the run's last); the Web arm measures the CSS `letter-spacing` equivalent
/// against this so the cross-backend convention gap is compared, not assumed.
pub const TRACKING_PX: f32 = 1.0;

/// Strings the hit-test/selection criteria run over (criterion 2 and the
/// mouse-driven part of the corpus). The last three are the §2.3(b)
/// extension: bidi visual-reordering, decomposed combining mark, ZWJ
/// sequence (see module docs for what each exercises and what verdicts
/// are expected to diverge).
pub fn hit_strings() -> Vec<&'static str> {
    vec![
        "Hello world",
        "héllo",
        "日本語",
        "héllo 👍",
        "abc مرحبا 123",
        // Decomposed e-acute: 'e' + U+0301 COMBINING ACUTE ACCENT
        // (explicit escape so the form is unambiguous in source).
        "cafe\u{301}",
        "a👩‍💻b",
    ]
}

/// Strings criterion 1's caret geometry runs over: the hit strings plus the
/// in-composition composites (raw pinyin/romaji, committed mixed content).
pub fn anchor_strings() -> Vec<&'static str> {
    let mut v = hit_strings();
    v.extend(["nihao", "你好world", "こんにちは", "abc日本語x"]);
    v
}

/// One grapheme cluster's two-way mapping, with this arm's caret geometry:
/// `u0/u1` = UTF-16 code-unit span (the DOM's index space), `b0/b1` = UTF-8
/// byte span (the framework's), `x` = cluster leading edge in device px at
/// DPR 1, `w` = cluster advance.
#[derive(Clone, Copy, Debug)]
pub struct ClusterCell {
    pub u0: u32,
    pub u1: u32,
    pub b0: u32,
    pub b1: u32,
    pub x: f32,
    pub w: f32,
}

#[derive(Clone, Debug)]
pub struct CorpusString {
    pub text: String,
    pub utf16_len: u32,
    pub width_dpr1: f32,
    pub width_tracking_dpr1: f32,
    pub clusters: Vec<ClusterCell>,
    /// The family DirectWrite's fallback actually mapped (first run) — the
    /// Web arm pins its CSS font stack to these so both engines render the
    /// same face; a mismatch here is a geometry finding, not a rule one.
    pub mapped_family: String,
}

pub fn shape_corpus_string(
    service: &dyn TextService,
    fonts: &[FontInfo],
    text: &str,
) -> Result<CorpusString, String> {
    let mut style = TextStyle::new(FONT_FAMILY, FONT_SIZE_PX);
    style.device_pixel_ratio = 1.0;
    let run = service
        .shape(text, &style)
        .map_err(|e| format!("shaping {text:?}: {e}"))?;
    let mut tracking_style = style.clone();
    tracking_style.letter_spacing_px = TRACKING_PX;
    let tracking_run = service
        .shape(text, &tracking_style)
        .map_err(|e| format!("shaping {text:?} (tracking): {e}"))?;
    let mapped_family = font_family_of(fonts, run.runs.first().map(|r| r.font_id));
    let clusters = run
        .clusters
        .iter()
        .map(|c| ClusterCell {
            u0: utf16_len_before(text, c.byte_range.0),
            u1: utf16_len_before(text, c.byte_range.1),
            b0: c.byte_range.0 as u32,
            b1: c.byte_range.1 as u32,
            x: run.caret_x(c.byte_range.0),
            w: run.caret_x(c.byte_range.1) - run.caret_x(c.byte_range.0),
        })
        .collect();
    Ok(CorpusString {
        text: text.to_string(),
        utf16_len: text.encode_utf16().count() as u32,
        width_dpr1: run.total_advance,
        width_tracking_dpr1: tracking_run_width(&tracking_run),
        clusters,
        mapped_family,
    })
}

fn tracking_run_width(run: &ShapedRun) -> f32 {
    run.total_advance
}

fn utf16_len_before(text: &str, byte: usize) -> u32 {
    text[..byte].encode_utf16().count() as u32
}

fn font_family_of(fonts: &[FontInfo], id: Option<FontId>) -> String {
    id.and_then(|id| fonts.iter().find(|f| f.id == id))
        .map(|f| f.family.clone())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// IME scenario scripts (criterion 3): the same normalized event sequences the
// Windows arm feeds through `ImeCompositionFeed` and the Web arm drives
// through the browser's native IME input path (CDP `Input.imeSetComposition`
// / `Input.imeCommitText` — the browser's own composition pipeline, not a
// custom JS IME handler). "Candidate selection" is emulated on both arms as
// commit-with-different-text (nihao → 你好 / konnitiha → 今日は): a real OS
// IME's candidate UI is not scriptable in either rig, and the spike's
// question is the *event stream + in-progress state*, not the candidate UI.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum ImeStep {
    Start {
        start_byte: usize,
    },
    Update {
        composition: String,
        caret_byte: usize,
    },
    Commit {
        committed: String,
    },
    Cancel,
    /// Focus leaves the field mid-composition. Windows-arm session policy:
    /// cancel (commits nothing). Browser behavior is whatever Chromium does —
    /// the comparison is the point.
    FocusLoss,
    /// IME-requested content deletion (normalized surface, content bytes).
    DeleteRange {
        range: (usize, usize),
    },
}

#[derive(Clone, Debug)]
pub struct ImeScenario {
    pub name: &'static str,
    pub base: &'static str,
    pub language: &'static str,
    pub steps: Vec<ImeStep>,
}

pub fn ime_scenarios() -> Vec<ImeScenario> {
    vec![
        ImeScenario {
            name: "zh_nihao_candidate_commit",
            base: "",
            language: "zh",
            steps: vec![
                ImeStep::Start { start_byte: 0 },
                ImeStep::Update {
                    composition: "ni".into(),
                    caret_byte: 2,
                },
                ImeStep::Update {
                    composition: "nih".into(),
                    caret_byte: 3,
                },
                ImeStep::Update {
                    composition: "niha".into(),
                    caret_byte: 4,
                },
                ImeStep::Update {
                    composition: "nihao".into(),
                    caret_byte: 5,
                },
                // Candidate picked: 你好 (of 你好/拟好/…), not the literal pinyin.
                ImeStep::Commit {
                    committed: "你好".into(),
                },
            ],
        },
        ImeScenario {
            name: "ja_kana_conversion_then_candidate",
            base: "",
            language: "ja",
            steps: vec![
                ImeStep::Start { start_byte: 0 },
                ImeStep::Update {
                    composition: "k".into(),
                    caret_byte: 1,
                },
                ImeStep::Update {
                    composition: "ko".into(),
                    caret_byte: 2,
                },
                ImeStep::Update {
                    composition: "kon".into(),
                    caret_byte: 3,
                },
                ImeStep::Update {
                    composition: "konn".into(),
                    caret_byte: 4,
                },
                ImeStep::Update {
                    composition: "konni".into(),
                    caret_byte: 5,
                },
                ImeStep::Update {
                    composition: "konniti".into(),
                    caret_byte: 6,
                },
                ImeStep::Update {
                    composition: "konnitiha".into(),
                    caret_byte: 8,
                },
                // The IME's own romaji→kana phase: composition text changes
                // shape without more keys (multi-candidate machinery engaged).
                ImeStep::Update {
                    composition: "こんにちは".into(),
                    caret_byte: 15,
                },
                // Candidate 今日は picked.
                ImeStep::Commit {
                    committed: "今日は".into(),
                },
            ],
        },
        ImeScenario {
            name: "cancel_mid_composition",
            base: "",
            language: "zh",
            steps: vec![
                ImeStep::Start { start_byte: 0 },
                ImeStep::Update {
                    composition: "ni".into(),
                    caret_byte: 2,
                },
                ImeStep::Update {
                    composition: "nih".into(),
                    caret_byte: 3,
                },
                ImeStep::Cancel,
            ],
        },
        ImeScenario {
            name: "caret_nav_inside_composition",
            base: "abc ",
            language: "zh",
            // In-composition arrow navigation: the caret moves inside the
            // composition; the candidate anchor must track it (criterion 1's
            // "tracked through in-composition arrow navigation").
            steps: vec![
                ImeStep::Start { start_byte: 4 },
                ImeStep::Update {
                    composition: "nihao".into(),
                    caret_byte: 9,
                },
                ImeStep::Update {
                    composition: "nihao".into(),
                    caret_byte: 7,
                },
                ImeStep::Update {
                    composition: "nihao".into(),
                    caret_byte: 9,
                },
                ImeStep::Commit {
                    committed: "你好".into(),
                },
            ],
        },
        ImeScenario {
            name: "rapid_zh_ja_switch",
            base: "",
            language: "zh/ja",
            steps: vec![
                ImeStep::Start { start_byte: 0 },
                ImeStep::Update {
                    composition: "ni".into(),
                    caret_byte: 2,
                },
                ImeStep::Cancel,
                ImeStep::Start { start_byte: 0 },
                ImeStep::Update {
                    composition: "ko".into(),
                    caret_byte: 2,
                },
                ImeStep::Update {
                    composition: "kon".into(),
                    caret_byte: 3,
                },
                ImeStep::Commit {
                    committed: "今".into(),
                },
            ],
        },
        ImeScenario {
            name: "delete_range_mid_composition",
            base: "abc",
            language: "zh",
            steps: vec![
                ImeStep::Start { start_byte: 3 },
                ImeStep::Update {
                    composition: "nihao".into(),
                    caret_byte: 8,
                },
                // IME re-anchors: pre-delete the committed "abc" under the
                // composition (the normalized DeleteRange event's real job).
                ImeStep::DeleteRange { range: (0, 3) },
                ImeStep::Commit {
                    committed: "你好".into(),
                },
            ],
        },
        ImeScenario {
            name: "focus_loss_mid_composition",
            base: "",
            language: "zh",
            steps: vec![
                ImeStep::Start { start_byte: 0 },
                ImeStep::Update {
                    composition: "nih".into(),
                    caret_byte: 3,
                },
                ImeStep::FocusLoss,
            ],
        },
    ]
}

// ---------------------------------------------------------------------------
// Shared editing-operation suite (criterion 4: "one model"). Operations are
// logical, not keys: both arms' drivers map them to native mechanisms — the
// Windows session does its own mapping; the DOM arm does whatever the real
// browser field natively does (typed keys, real clicks/drags, Ctrl+Z).
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum EditOp {
    /// Insert text at the caret, replacing any selection.
    Insert {
        text: String,
    },
    /// Move the caret ±N cluster/character steps (arrow keys).
    CaretMove {
        steps: i32,
    },
    /// Click 1 px inside cluster `index` → caret at its leading edge.
    ClickCluster {
        index: usize,
    },
    /// Pointer-down inside `from`, drag to inside `to`, release.
    DragCluster {
        from: usize,
        to: usize,
    },
    /// Click inside `from`, then shift-click inside `to` → extend.
    ShiftClickCluster {
        from: usize,
        to: usize,
    },
    /// Double-click inside cluster `index` → word select.
    DblClickCluster {
        index: usize,
    },
    EndKey,
    HomeKey,
    /// Single-level undo (v1 scope): restores the state before the last edit.
    Undo,
}

#[derive(Clone, Debug)]
pub struct OpSuite {
    pub name: &'static str,
    pub base: &'static str,
    pub steps: Vec<EditOp>,
}

pub fn op_suites() -> Vec<OpSuite> {
    vec![
        OpSuite {
            name: "latin_edit",
            base: "Hello world",
            steps: vec![
                EditOp::ClickCluster { index: 0 },
                EditOp::Insert { text: "X".into() },
                EditOp::CaretMove { steps: 2 },
                EditOp::DragCluster { from: 3, to: 8 },
                EditOp::Insert { text: "!".into() },
                EditOp::Undo,
                EditOp::EndKey,
                EditOp::Insert { text: "!".into() },
                EditOp::Undo,
            ],
        },
        OpSuite {
            name: "multibyte_edit",
            base: "日本語x",
            steps: vec![
                EditOp::ClickCluster { index: 1 },
                EditOp::Insert { text: "あ".into() },
                EditOp::Undo,
                EditOp::ShiftClickCluster { from: 1, to: 3 },
                EditOp::DblClickCluster { index: 1 },
            ],
        },
        OpSuite {
            name: "undo_granularity",
            base: "Hello world",
            // Deliberately exposes undo-granularity differences: two typed
            // characters in one burst, then a single undo. Windows session =
            // single-level (restores pre-'b'); browsers coalesce typed bursts
            // (restores pre-'ab'). Part of the criterion-4 evidence either way.
            steps: vec![
                EditOp::ClickCluster { index: 0 },
                EditOp::Insert { text: "a".into() },
                EditOp::Insert { text: "b".into() },
                EditOp::Undo,
            ],
        },
    ]
}

// ---------------------------------------------------------------------------
// corpus.json serialization (the Web arm's single input)
// ---------------------------------------------------------------------------

pub fn cluster_cell_json(c: &ClusterCell) -> J {
    J::o(vec![
        ("u0".into(), J::n(c.u0 as f64)),
        ("u1".into(), J::n(c.u1 as f64)),
        ("b0".into(), J::n(c.b0 as f64)),
        ("b1".into(), J::n(c.b1 as f64)),
        ("x".into(), J::n(c.x as f64)),
        ("w".into(), J::n(c.w as f64)),
    ])
}

pub fn corpus_string_json(cs: &CorpusString) -> J {
    J::o(vec![
        ("text".into(), J::s(cs.text.clone())),
        ("utf16_len".into(), J::n(cs.utf16_len as f64)),
        ("width_dpr1".into(), J::n(cs.width_dpr1 as f64)),
        (
            "width_tracking_dpr1".into(),
            J::n(cs.width_tracking_dpr1 as f64),
        ),
        (
            "clusters".into(),
            J::a(cs.clusters.iter().map(cluster_cell_json).collect()),
        ),
        ("mapped_family".into(), J::s(cs.mapped_family.clone())),
    ])
}

pub fn ime_step_json(s: &ImeStep) -> J {
    match s {
        ImeStep::Start { start_byte } => J::o(vec![
            ("op".into(), J::s("start")),
            ("start_byte".into(), J::n(*start_byte as f64)),
        ]),
        ImeStep::Update {
            composition,
            caret_byte,
        } => J::o(vec![
            ("op".into(), J::s("update")),
            ("composition".into(), J::s(composition.clone())),
            ("caret_byte".into(), J::n(*caret_byte as f64)),
        ]),
        ImeStep::Commit { committed } => J::o(vec![
            ("op".into(), J::s("commit")),
            ("committed".into(), J::s(committed.clone())),
        ]),
        ImeStep::Cancel => J::o(vec![("op".into(), J::s("cancel"))]),
        ImeStep::FocusLoss => J::o(vec![("op".into(), J::s("focus-loss"))]),
        ImeStep::DeleteRange { range } => J::o(vec![
            ("op".into(), J::s("delete-range")),
            (
                "range".into(),
                J::a(vec![J::n(range.0 as f64), J::n(range.1 as f64)]),
            ),
        ]),
    }
}

pub fn ime_scenario_json(s: &ImeScenario) -> J {
    J::o(vec![
        ("name".into(), J::s(s.name)),
        ("base".into(), J::s(s.base)),
        ("language".into(), J::s(s.language)),
        (
            "steps".into(),
            J::a(s.steps.iter().map(ime_step_json).collect()),
        ),
    ])
}

pub fn edit_op_json(op: &EditOp) -> J {
    match op {
        EditOp::Insert { text } => J::o(vec![
            ("op".into(), J::s("insert")),
            ("text".into(), J::s(text.clone())),
        ]),
        EditOp::CaretMove { steps } => J::o(vec![
            ("op".into(), J::s("caret-move")),
            ("steps".into(), J::n(*steps as f64)),
        ]),
        EditOp::ClickCluster { index } => J::o(vec![
            ("op".into(), J::s("click")),
            ("index".into(), J::n(*index as f64)),
        ]),
        EditOp::DragCluster { from, to } => J::o(vec![
            ("op".into(), J::s("drag")),
            ("from".into(), J::n(*from as f64)),
            ("to".into(), J::n(*to as f64)),
        ]),
        EditOp::ShiftClickCluster { from, to } => J::o(vec![
            ("op".into(), J::s("shift-click")),
            ("from".into(), J::n(*from as f64)),
            ("to".into(), J::n(*to as f64)),
        ]),
        EditOp::DblClickCluster { index } => J::o(vec![
            ("op".into(), J::s("dbl-click")),
            ("index".into(), J::n(*index as f64)),
        ]),
        EditOp::EndKey => J::o(vec![("op".into(), J::s("end"))]),
        EditOp::HomeKey => J::o(vec![("op".into(), J::s("home"))]),
        EditOp::Undo => J::o(vec![("op".into(), J::s("undo"))]),
    }
}

pub fn op_suite_json(s: &OpSuite) -> J {
    J::o(vec![
        ("name".into(), J::s(s.name)),
        ("base".into(), J::s(s.base)),
        (
            "steps".into(),
            J::a(s.steps.iter().map(edit_op_json).collect()),
        ),
    ])
}
