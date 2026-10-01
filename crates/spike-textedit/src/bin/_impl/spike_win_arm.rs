// The §9.2 spike's Windows-GPU arm runner. Emits:
// - `spike/corpus.json` — the shared rig (strings + cluster tables + IME
//   scenario scripts + editing-op suites) for the Web-DOM arm;
// - `spike/results/windows.json` — this arm's raw results for criteria 1
//   (caret geometry vs two platform references), 3 (composition event
//   fidelity), and 4 (shared editing-operation suite). Criterion 2's
//   Windows-side hit-test answers are produced here too, embedded in the
//   `c4` click rows and, for the sweep, by `c2_probes`.
//
// Every number is written raw; no averaging happens anywhere in the rig.
// (Round 19.2: this body is included into the bin's `#[cfg(windows)] mod
// imp` — inner doc comments are illegal in an include! expansion, so the
// historical `//!` block became plain comments; the crate-level doc now
// lives on the bin wrapper.)

use oppa::reactive::Runtime;
use oppa::text::{FontInfo, TextService, TextStyle};
use oppa_text_dwrite::DWriteTextService;
use spike_textedit::json::J;
use spike_textedit::oracle::{EditControlOracle, LayoutOracle};
use spike_textedit::rig::{
    edit_op_json, ime_scenario_json, ime_scenarios, CorpusString, EditOp, RIG_VERSION,
    TOLERANCE_PX, TRACKING_PX,
};
use spike_textedit::session::{run_ime_steps, EditingSession, SessionState};
use std::cell::RefCell;
use std::rc::Rc;

const FONT_FAMILY: &str = spike_textedit::rig::FONT_FAMILY;
const FONT_SIZE_PX: f32 = spike_textedit::rig::FONT_SIZE_PX;

fn style_for(dpr: f32) -> TextStyle {
    let mut style = TextStyle::new(FONT_FAMILY, FONT_SIZE_PX);
    style.device_pixel_ratio = dpr;
    style
}

fn spike_dir() -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let root = std::path::Path::new(manifest)
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    root.join("spike")
}

fn write_file(path: &std::path::Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create results dir");
    }
    std::fs::write(path, contents.as_bytes()).expect("write result file");
}

fn null() -> J {
    J::N(f64::NAN) // the writer emits `null` for non-finite
}

pub fn main() {
    let service: Rc<dyn TextService> =
        Rc::new(DWriteTextService::new().expect("DirectWrite factory"));
    let fonts = service.enumerate_fonts();

    // ------------------------------------------------------------------
    // Shared corpus (single source of truth for both arms).
    // ------------------------------------------------------------------
    let hits: Vec<CorpusString> = spike_textedit::rig::hit_strings()
        .into_iter()
        .map(|text| shape_corpus(service.as_ref(), &fonts, text).expect("corpus shaping"))
        .collect();
    let mut anchors: Vec<CorpusString> = hits.clone();
    for text in spike_textedit::rig::anchor_strings() {
        if !anchors.iter().any(|c| c.text == text) {
            anchors.push(shape_corpus(service.as_ref(), &fonts, text).expect("corpus shaping"));
        }
    }
    // Op-suite bases get their own cluster tables (suites click inside
    // strings like "日本語x" that are not in the anchor set).
    let mut suite_bases: Vec<CorpusString> = Vec::new();
    for suite in spike_textedit::rig::op_suites() {
        if !suite_bases.iter().any(|c| c.text == suite.base) {
            suite_bases.push(
                shape_corpus(service.as_ref(), &fonts, suite.base).expect("suite base shaping"),
            );
        }
    }
    let mut font_stack: Vec<String> = vec![FONT_FAMILY.to_string()];
    for cs in &anchors {
        if !cs.mapped_family.is_empty() && !font_stack.contains(&cs.mapped_family) {
            font_stack.push(cs.mapped_family.clone());
        }
    }

    let corpus = J::o(vec![
        ("rig_version".into(), J::n(RIG_VERSION as f64)),
        ("tolerance_px".into(), J::n(TOLERANCE_PX as f64)),
        (
            "font".into(),
            J::o(vec![
                ("family".into(), J::s(FONT_FAMILY)),
                ("size_px".into(), J::n(FONT_SIZE_PX as f64)),
                ("dpr".into(), J::n(1.0)),
                ("tracking_px".into(), J::n(TRACKING_PX as f64)),
                (
                    "font_stack".into(),
                    J::a(font_stack.iter().map(J::s).collect()),
                ),
            ]),
        ),
        (
            "hit_strings".into(),
            J::a(
                hits.iter()
                    .map(spike_textedit::rig::corpus_string_json)
                    .collect(),
            ),
        ),
        (
            "anchor_strings".into(),
            J::a(
                anchors
                    .iter()
                    .map(spike_textedit::rig::corpus_string_json)
                    .collect(),
            ),
        ),
        (
            "ime_scenarios".into(),
            J::a(ime_scenarios().iter().map(ime_scenario_json).collect()),
        ),
        (
            "op_suites".into(),
            J::a(
                spike_textedit::rig::op_suites()
                    .iter()
                    .map(spike_textedit::rig::op_suite_json)
                    .collect(),
            ),
        ),
        (
            "suite_bases".into(),
            J::a(
                suite_bases
                    .iter()
                    .map(spike_textedit::rig::corpus_string_json)
                    .collect(),
            ),
        ),
    ]);
    write_file(&spike_dir().join("corpus.json"), &corpus.render());
    println!("corpus.json written");

    // ------------------------------------------------------------------
    // Criterion 1: caret rects vs the two platform references.
    // ------------------------------------------------------------------
    let edit_control = match build_edit_control() {
        Some(edit) => Some(edit),
        None => {
            println!("EDIT-control oracle unusable in this environment");
            None
        }
    };
    let c1 = criterion1(service.as_ref(), edit_control.as_ref());

    // ------------------------------------------------------------------
    // Criteria 1 (composition tracking), 3, 4, and 2 (Windows-side sweep).
    // ------------------------------------------------------------------
    let results = run_windows_arm(&service, &anchors);
    let out = J::o(vec![
        ("arm".into(), J::s("windows-gpu")),
        ("rig_version".into(), J::n(RIG_VERSION as f64)),
        ("tolerance_px".into(), J::n(TOLERANCE_PX as f64)),
        (
            "edit_control_oracle_usable".into(),
            J::b(edit_control.is_some()),
        ),
        ("c1".into(), c1),
        ("c1_composition".into(), results.c1_composition),
        ("c2_sweep".into(), results.c2_sweep),
        ("c3".into(), results.c3),
        ("c4".into(), results.c4),
    ]);
    write_file(
        &spike_dir().join("results").join("windows.json"),
        &out.render(),
    );
    println!("windows.json written");
}

fn shape_corpus(
    service: &dyn TextService,
    fonts: &[FontInfo],
    text: &str,
) -> Result<CorpusString, String> {
    spike_textedit::rig::shape_corpus_string(service, fonts, text)
}

// ---------------------------------------------------------------------------
// Criterion 1: framework caret x vs IDWriteTextLayout and (DPR 1) the EDIT
// control, per cluster boundary + trailing caret; oracle intra-cluster
// spread must be zero (carets never split clusters) on both sides.
// ---------------------------------------------------------------------------

fn build_edit_control() -> Option<EditControlOracle> {
    let edit = EditControlOracle::new(FONT_SIZE_PX).ok()?;
    // Sanity: the control's formatting rect must be real and EM_POSFROMCHAR
    // must answer sanely for "Hi" before the rig trusts it.
    let xs = edit.caret_x_per_unit("Hi");
    let sane = xs.len() == 3 && xs[0] >= 0.0 && xs[0] < 100.0 && xs[1] >= xs[0];
    sane.then_some(edit)
}

fn criterion1(service: &dyn TextService, edit: Option<&EditControlOracle>) -> J {
    let layout1 = LayoutOracle::new(FONT_FAMILY, FONT_SIZE_PX, 1.0).expect("layout oracle dpr1");
    let layout2 = LayoutOracle::new(FONT_FAMILY, FONT_SIZE_PX, 2.0).expect("layout oracle dpr2");
    let mut rows = Vec::new();
    for text in spike_textedit::rig::anchor_strings() {
        for dpr in [1.0f32, 2.0] {
            let run = service
                .shape(text, &style_for(dpr))
                .expect("anchor shaping");
            let oracle = if dpr == 1.0 {
                layout1.caret_x_per_unit(text).expect("oracle")
            } else {
                layout2.caret_x_per_unit(text).expect("oracle")
            };
            let mut max_delta = 0.0f32;
            let mut worst: Option<usize> = None;
            let mut max_spread = 0.0f32;
            for (i, cluster) in run.clusters.iter().enumerate() {
                let ours = run.caret_x(cluster.byte_range.0);
                let u0 = cluster_utf16_start(text, cluster.byte_range.0);
                let lead = oracle[u0];
                let delta = (ours - lead).abs();
                if delta > max_delta {
                    max_delta = delta;
                    worst = Some(i);
                }
                let u1 = cluster_utf16_start(text, cluster.byte_range.1);
                let lo = oracle[u0..u1].iter().cloned().fold(f32::MAX, f32::min);
                let hi = oracle[u0..u1].iter().cloned().fold(f32::MIN, f32::max);
                max_spread = max_spread.max(hi - lo);
            }
            // Trailing caret: ours = total advance.
            let ours_end = run.total_advance;
            let oracle_end = *oracle.last().expect("trailing entry");
            let end_delta = (ours_end - oracle_end).abs();
            if end_delta > max_delta {
                max_delta = end_delta;
                worst = Some(run.clusters.len());
            }
            let mut edit_delta: Option<f32> = None;
            // The EDIT control is a 96-dpi (DPR-1) instrument: its
            // coordinates only compare against DPR-1 runs.
            if let (1.0, Some(edit)) = (dpr, edit) {
                let xs = edit.caret_x_per_unit(text);
                let mut e_max = 0.0f32;
                for cluster in &run.clusters {
                    let ours = run.caret_x(cluster.byte_range.0);
                    let lead = xs[cluster_utf16_start(text, cluster.byte_range.0)];
                    e_max = e_max.max((ours - lead).abs());
                }
                e_max = e_max.max((ours_end - xs[xs.len() - 1]).abs());
                edit_delta = Some(e_max);
            }
            rows.push(J::o(vec![
                ("string".into(), J::s(text)),
                ("dpr".into(), J::n(dpr as f64)),
                ("max_delta_layout_px".into(), J::n(max_delta as f64)),
                (
                    "worst_index".into(),
                    worst.map(|c| J::n(c as f64)).unwrap_or(null()),
                ),
                ("pass_layout".into(), J::b(max_delta <= TOLERANCE_PX)),
                (
                    "max_delta_edit_px".into(),
                    edit_delta.map(|d| J::n(d as f64)).unwrap_or(null()),
                ),
                (
                    "pass_edit".into(),
                    edit_delta
                        .map(|d| J::b(d <= TOLERANCE_PX))
                        .unwrap_or(null()),
                ),
                (
                    "oracle_intra_cluster_spread_px".into(),
                    J::n(max_spread as f64),
                ),
            ]));
        }
    }
    J::a(rows)
}

fn cluster_utf16_start(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

// ---------------------------------------------------------------------------
// Criteria 3 (composition fidelity + observable in-progress state),
// criterion 1's composition-tracking rows, criterion 4 (shared op suite),
// and criterion 2's Windows-side x→index sweep (the Web arm answers the
// same probe list; compare.mjs joins them).
// ---------------------------------------------------------------------------

struct WinArm {
    c1_composition: J,
    c2_sweep: J,
    c3: J,
    c4: J,
}

fn run_windows_arm(service: &Rc<dyn TextService>, anchors: &[CorpusString]) -> WinArm {
    let rt = Runtime::new();
    let ime_ops: Rc<RefCell<Vec<oppa::ime::ImeOps>>> = Rc::new(RefCell::new(Vec::new()));
    rt.set_shell(Box::new(spike_textedit::session::RecordingShell {
        ime_ops: ime_ops.clone(),
    }));
    let layout = LayoutOracle::new(FONT_FAMILY, FONT_SIZE_PX, 1.0).expect("layout oracle");

    let mut c1_rows = Vec::new();
    let mut c3_rows = Vec::new();
    for scenario in ime_scenarios() {
        let mut session = new_session(&rt, service, scenario.base, ime_ops.clone());
        let mut step_rows = Vec::new();
        for (i, step) in scenario.steps.iter().enumerate() {
            run_ime_steps(&mut session, std::slice::from_ref(step));
            session.commit_frame();
            let canon = session.canonical.last().cloned();
            let rect = {
                let ops = ime_ops.borrow();
                ops.iter().rev().find_map(|op| match *op {
                    oppa::ime::ImeOps::SetCaretRect {
                        x,
                        y,
                        width,
                        height,
                    } => Some((x, y, width, height)),
                    _ => None,
                })
            };
            ime_ops.borrow_mut().clear();
            let composite = session.composite_text();
            let caret_byte = session.composite_caret_byte();
            let (oracle_x, delta, pass): (Option<f32>, Option<f32>, Option<bool>) = if composite
                .is_empty()
            {
                (None, None, Some(true))
            } else {
                match layout.caret_x_per_unit(&composite) {
                    Ok(oracle) => {
                        let unit = cluster_utf16_start(&composite, caret_byte.min(composite.len()));
                        let ox = oracle
                            .get(unit)
                            .copied()
                            .unwrap_or(oracle[oracle.len() - 1]);
                        let d = rect.map(|(x, _, _, _)| (x - ox).abs());
                        (Some(ox), d, d.map(|d| d <= TOLERANCE_PX))
                    }
                    Err(_) => (None, None, None),
                }
            };
            step_rows.push(J::o(vec![
                ("step".into(), J::n(i as f64)),
                (
                    "phase".into(),
                    canon.as_ref().map(|c| J::s(c.phase)).unwrap_or(null()),
                ),
                ("composite".into(), J::s(composite)),
                ("caret_byte".into(), J::n(caret_byte as f64)),
                (
                    "rect_x".into(),
                    rect.map(|r| J::n(r.0 as f64)).unwrap_or(null()),
                ),
                (
                    "oracle_x".into(),
                    oracle_x.map(|x| J::n(x as f64)).unwrap_or(null()),
                ),
                (
                    "delta_px".into(),
                    delta.map(|d| J::n(d as f64)).unwrap_or(null()),
                ),
                ("pass".into(), pass.map(J::b).unwrap_or(null())),
            ]));
        }
        // Criterion 3: the canonical stream must equal the script, and the
        // content/composition state must follow the session's semantics.
        let canon = session.take_canonical();
        let expected: Vec<String> = scenario.steps.iter().map(phase_of).collect();
        let logged: Vec<String> = canon.iter().map(|c| c.phase.to_string()).collect();
        let ok = logged == expected && state_stream_matches(&scenario, &canon);
        c3_rows.push(J::o(vec![
            ("name".into(), J::s(scenario.name)),
            ("language".into(), J::s(scenario.language)),
            ("ok".into(), J::b(ok)),
            (
                "expected_phases".into(),
                J::a(expected.iter().map(J::s).collect()),
            ),
            (
                "logged_phases".into(),
                J::a(logged.iter().map(J::s).collect()),
            ),
            ("steps".into(), J::a(canon.iter().map(canon_json).collect())),
        ]));
        c1_rows.push(J::o(vec![
            ("scenario".into(), J::s(scenario.name)),
            ("steps".into(), J::a(step_rows)),
        ]));
    }

    // Criterion 2, Windows side: the x→index sweep the Web arm answers with
    // real clicks. Both arms answer the same integer x set per string.
    let mut c2_rows = Vec::new();
    for hit in anchors
        .iter()
        .filter(|h| spike_textedit::rig::hit_strings().contains(&h.text.as_str()))
    {
        let run = service
            .shape(&hit.text, &style_for(1.0))
            .expect("hit shaping");
        let max_x = hit.width_dpr1.ceil() as i32 + 2;
        let mut answers = Vec::new();
        for x in 0..=max_x {
            let byte = run.byte_offset_for_x(x as f32);
            answers.push(J::n(byte_to_cp(&hit.text, byte) as f64));
        }
        // The same mouse-selection ops the Web arm runs with the same x
        // construction (cluster k = min(k, n-1)); answered through the
        // editing session (the framework's native mechanism).
        let mut sel_rows = Vec::new();
        {
            let n = hit.clusters.len();
            let x_of = |index: usize| -> f32 {
                let i = index.min(n.saturating_sub(1));
                hit.clusters.get(i).map(|c| c.x).unwrap_or(hit.width_dpr1) + 1.0
            };
            let mut session = new_session(&rt, service, &hit.text, ime_ops.clone());
            let record = |kind: &str, st: &SessionState| -> J {
                J::o(vec![
                    ("kind".into(), J::s(kind)),
                    ("start_cp".into(), J::n(sel_cp(st).0 as f64)),
                    ("end_cp".into(), J::n(sel_cp(st).1 as f64)),
                ])
            };
            session.drag_x(x_of(0), x_of(3.min(n.saturating_sub(1))));
            sel_rows.push(record("drag", &session.observable()));
            session.click_x(x_of(0));
            session.shift_click_x(x_of(2.min(n.saturating_sub(1))));
            sel_rows.push(record("shift-click", &session.observable()));
            session.dbl_click_x(x_of(1.min(n.saturating_sub(1))));
            sel_rows.push(record("dbl-click", &session.observable()));
        }
        c2_rows.push(J::o(vec![
            ("string".into(), J::s(hit.text.clone())),
            ("max_x".into(), J::n(max_x as f64)),
            ("answers_cp".into(), J::a(answers)),
            // Same probes at the cluster boundaries ±1 (rule checks):
            (
                "boundary_probes".into(),
                J::a(
                    hit.clusters
                        .iter()
                        .enumerate()
                        .flat_map(|(i, c)| {
                            let px = |v: f32| {
                                J::a(vec![
                                    J::n(v as f64),
                                    J::n(byte_to_cp(&hit.text, run.byte_offset_for_x(v)) as f64),
                                ])
                            };
                            vec![
                                J::a(vec![J::n(i as f64), px(c.x - 1.0)]),
                                J::a(vec![J::n(i as f64), px(c.x + 1.0)]),
                                J::a(vec![J::n(i as f64), px(c.x + c.w * 0.5)]),
                                J::a(vec![J::n(i as f64), px(c.x + c.w - 1.0)]),
                            ]
                        })
                        .collect(),
                ),
            ),
            ("sel_ops".into(), J::a(sel_rows)),
        ]));
    }

    // Criterion 4: shared editing-operation suite.
    let mut c4_rows = Vec::new();
    for suite in spike_textedit::rig::op_suites() {
        let mut session = new_session(&rt, service, suite.base, ime_ops.clone());
        let mut step_rows = Vec::new();
        for (i, op) in suite.steps.iter().enumerate() {
            // Geometry-addressed ops click at the framework arm's own cluster
            // leading edge (+1 px, the leading half) of the CURRENT composite,
            // so both arms click the same x against the same current value.
            let mut op_x: Vec<f32> = Vec::new();
            match op {
                EditOp::ClickCluster { index } | EditOp::DblClickCluster { index } => {
                    op_x.push(session.cluster_leading_x(*index) + 1.0)
                }
                EditOp::DragCluster { from, to } | EditOp::ShiftClickCluster { from, to } => {
                    op_x.push(session.cluster_leading_x(*from) + 1.0);
                    op_x.push(session.cluster_leading_x(*to) + 1.0);
                }
                _ => {}
            }
            apply_op(&mut session, op, &op_x);
            session.commit_frame();
            let st = session.observable();
            step_rows.push(J::o(vec![
                ("step".into(), J::n(i as f64)),
                ("op".into(), edit_op_json(op)),
                (
                    "op_x".into(),
                    J::a(op_x.iter().map(|v| J::n(*v as f64)).collect()),
                ),
                ("value".into(), J::s(st.content.clone())),
                ("caret_cp".into(), J::n(caret_cp(&st) as f64)),
                (
                    "sel".into(),
                    J::a(vec![J::n(sel_cp(&st).0 as f64), J::n(sel_cp(&st).1 as f64)]),
                ),
                ("composition".into(), J::s(st.composition.clone())),
            ]));
        }
        c4_rows.push(J::o(vec![
            ("name".into(), J::s(suite.name)),
            ("base".into(), J::s(suite.base)),
            ("steps".into(), J::a(step_rows)),
        ]));
    }

    WinArm {
        c1_composition: J::a(c1_rows),
        c2_sweep: J::a(c2_rows),
        c3: J::a(c3_rows),
        c4: J::a(c4_rows),
    }
}

fn byte_to_cp(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].chars().count()
}

fn caret_cp(st: &SessionState) -> usize {
    if st.composition.is_empty() {
        st.content[..st.caret.min(st.content.len())].chars().count()
    } else {
        // Caret is composite coordinates; compare within the composite.
        let composite = &st.content; // suites never compose; still safe
        composite[..st.caret.min(composite.len())].chars().count()
    }
}

fn sel_cp(st: &SessionState) -> (usize, usize) {
    let to_cp = |b: usize| st.content[..b.min(st.content.len())].chars().count();
    (to_cp(st.sel.0), to_cp(st.sel.1))
}

fn apply_op(session: &mut EditingSession, op: &EditOp, op_x: &[f32]) {
    match op {
        EditOp::Insert { text } => session.insert(text),
        EditOp::CaretMove { steps } => session.caret_move(*steps),
        EditOp::ClickCluster { .. } => session.click_x(op_x[0]),
        EditOp::DragCluster { .. } => session.drag_x(op_x[0], op_x[1]),
        EditOp::ShiftClickCluster { .. } => {
            session.click_x(op_x[0]);
            session.shift_click_x(op_x[1]);
        }
        EditOp::DblClickCluster { .. } => session.dbl_click_x(op_x[0]),
        EditOp::EndKey => session.caret_to_end(),
        EditOp::HomeKey => session.caret_to_start(),
        EditOp::Undo => session.undo(),
    }
}

fn phase_of(step: &spike_textedit::rig::ImeStep) -> String {
    use spike_textedit::rig::ImeStep::*;
    match step {
        Start { .. } => "start",
        Update { .. } => "update",
        Commit { .. } => "commit",
        Cancel => "cancel",
        FocusLoss => "cancel", // the session's focus-loss policy: cancel
        DeleteRange { .. } => "delete-range",
    }
    .to_string()
}

/// Replays the scenario through the session's documented semantics and
/// checks the canonical stream's (content, composition) after every step.
fn state_stream_matches(
    scenario: &spike_textedit::rig::ImeScenario,
    canon: &[spike_textedit::session::CanonStep],
) -> bool {
    use spike_textedit::rig::ImeStep::*;
    let mut content = scenario.base.to_string();
    let mut composition = String::new();
    let mut start = 0usize;
    let mut caret_byte = 0usize;
    for (step, observed) in scenario.steps.iter().zip(canon) {
        match step {
            Start { start_byte } => {
                start = *start_byte;
                composition.clear();
            }
            Update {
                composition: text,
                caret_byte: caret,
            } => {
                composition = text.clone();
                caret_byte = *caret;
            }
            Commit { committed } => {
                let mut new = String::new();
                new.push_str(&content[..start]);
                new.push_str(committed);
                new.push_str(&content[start..]);
                content = new;
                composition.clear();
                caret_byte = start + committed.len();
            }
            Cancel | FocusLoss => {
                composition.clear();
            }
            DeleteRange { range } => {
                let removed = range.1 - range.0;
                let mut new = String::new();
                new.push_str(&content[..range.0]);
                new.push_str(&content[range.1..]);
                content = new;
                if start >= range.1 {
                    start -= removed;
                } else if start > range.0 {
                    start = range.0;
                }
                caret_byte = start;
            }
        }
        if observed.content != content || observed.composition != composition {
            return false;
        }
    }
    let _ = caret_byte;
    true
}

fn canon_json(c: &spike_textedit::session::CanonStep) -> J {
    J::o(vec![
        ("phase".into(), J::s(c.phase)),
        ("text".into(), J::s(c.text.clone())),
        ("caret_byte".into(), J::n(c.caret_byte as f64)),
        ("content".into(), J::s(c.content.clone())),
        ("composition".into(), J::s(c.composition.clone())),
    ])
}

fn new_session(
    rt: &Runtime,
    service: &Rc<dyn TextService>,
    base: &str,
    ime_ops: Rc<RefCell<Vec<oppa::ime::ImeOps>>>,
) -> EditingSession {
    EditingSession::new(
        rt.clone(),
        service.clone(),
        style_for(1.0),
        base.to_string(),
        ime_ops,
    )
}
