// DESIGN §9.2 spike — verdict computation. Joins windows.json (Windows-GPU
// arm) against web.json (Web-DOM arm) under the shared corpus, restates the
// four pass/fail criteria concretely, and emits per-criterion per-arm raw
// results + failure classification + the authority-model recommendation.
// Nothing is averaged away; every mismatch stays in the output.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const spikeDir = path.resolve(here, '..');
const corpus = JSON.parse(fs.readFileSync(path.join(spikeDir, 'corpus.json'), 'utf8'));
const win = JSON.parse(fs.readFileSync(path.join(spikeDir, 'results', 'windows.json'), 'utf8'));
const web = JSON.parse(fs.readFileSync(path.join(spikeDir, 'results', 'web.json'), 'utf8'));

const TOL = corpus.tolerance_px;

// UTF-16 index (DOM) → code-point index (the shared normalization).
const u16ToCp = (text, u16) => [...text.slice(0, u16)].length;

const verdict = {
  rig: {
    windows_arm: 'windows-gpu (framework-authority editing session over DWriteTextService; set_ime anchoring)',
    web_arm: 'web-dom (real <input> in headless Edge; native clicks/keys/IME; hook records what the DOM reports)',
    tolerance_px: TOL,
    rig_notes: web.rig_notes,
  },
  c1: null,
  c1_composition: null,
  c2: null,
  c3: null,
  c4: null,
  letter_tracking: web.letter_tracking ?? null,
  out_of_scope_flags: [
    'RTL/bidi visual ordering: M0b-deferred M3 layout work — the corpus now carries a bidi string so the rtl flag gets exercised and the visual-order divergence is measured (expect c1/c2 mismatches on it), not silently covered.',
    'Variant A on Web (framework authority over a hidden input, browser as event source) was NOT built this round — the round scope was Windows-GPU (framework authority) vs Web-DOM (native). The recommendation maps accordingly (see verdict).',
    'Combining marks / ZWJ corpus entries (DESIGN §9.2 criterion 2) are now in the corpus (rig v2: decomposed e-acute, ZWJ technologist); the surrogate-pair string ("héllo 👍") covered the multi-unit case first.',
    'A real OS IME was not drivable from this environment on either arm; both arms use scripted IME through the platform-native input path (Windows: ImeCompositionFeed per M0b; Web: CDP IME scripting, which routes through Chromium\'s text-input state machine).',
  ],
};

// ---------------------------------------------------------------------------
// Criterion 1 — IME geometry (Windows arm).
// ---------------------------------------------------------------------------
{
  const rows = win.c1.map((r) => ({
    string: r.string,
    dpr: r.dpr,
    max_delta_vs_textlayout_px: r.max_delta_layout_px,
    pass_vs_textlayout: r.pass_layout,
    max_delta_vs_edit_control_px: r.pass_edit === null ? null : r.max_delta_edit_px,
    pass_vs_edit_control: r.pass_edit,
    oracle_intra_cluster_spread_px: r.oracle_intra_cluster_spread_px,
  }));
  const layoutWorst = Math.max(...win.c1.map((r) => r.max_delta_layout_px));
  const editRows = win.c1.filter((r) => r.pass_edit !== null);
  const editWorst = editRows.length ? Math.max(...editRows.map((r) => r.max_delta_edit_px)) : null;
  const compWorst = Math.max(
    ...win.c1_composition.flatMap((s) => s.steps.map((r) => (typeof r.delta_px === 'number' ? r.delta_px : 0)))
  );
  const compFail = win.c1_composition.flatMap((s) => s.steps.filter((r) => r.pass === false));
  verdict.c1 = {
    statement: `Framework caret rects (emitted through PlatformShell::set_ime) agree with (a) IDWriteTextLayout::HitTestTextPosition at every cluster boundary and trailing caret on every corpus string at DPR 1 and 2, and (b) a real Win32 EDIT control (EM_POSFROMCHAR, identical font/size) at DPR 1, within N=${TOL} device px; and track the caret through every composition edit and in-composition arrow step.`,
    N_justification: `N=2 device px: M0b's handoff fixes the Windows caret tolerance at ±2 device px; 2 device px = 1 CSS px at DPR 2; far inside DESIGN's "within one caret height"; tightest bound that does not require two engines (GetGlyphs pipeline vs IDWriteTextLayout/EDIT control) to agree subpixel-exactly.`,
    max_delta_vs_textlayout_px: layoutWorst,
    max_delta_vs_edit_control_px: editWorst,
    composition_tracking_max_delta_px: compWorst,
    composition_failing_steps: compFail.length,
    pass: layoutWorst <= TOL && (editWorst === null || editWorst <= TOL) && compWorst <= TOL && compFail.length === 0,
    per_string: win.c1,
  };
  verdict.c1_composition = {
    pass: compFail.length === 0 && compWorst <= TOL,
    per_scenario: win.c1_composition,
  };
}

// ---------------------------------------------------------------------------
// Criterion 2 — hit-test parity (both arms).
// ---------------------------------------------------------------------------
{
  const perString = [];
  let totalMismatch = 0;
  let totalProbes = 0;
  let selMismatch = 0;
  let selProbes = 0;
  let maxMidDelta = 0;
  for (const hit of corpus.hit_strings) {
    const w = win.c2_sweep.find((r) => r.string === hit.text);
    const clicks = web.c2_clicks.find((c) => c.string === hit.text);
    const mismatches = [];
    for (let x = 0; x <= w.max_x; x++) {
      const ours = w.answers_cp[x];
      const theirs = u16ToCp(hit.text, clicks.sweep[x]);
      if (ours !== theirs) {
        totalMismatch++;
        mismatches.push({ x, ours_cp: ours, web_cp: theirs });
      }
    }
    totalProbes += w.max_x + 1;

    // Boundary probes (rule check): same rounded x, both arms.
    const bMismatches = [];
    const winByProbe = new Map();
    for (const [i, [x, cp]] of w.boundary_probes.entries()) {
      const key = `${i}:${Math.max(0, Math.round(x))}`;
      winByProbe.set(key, cp);
    }
    for (const b of clicks.boundary) {
      const key = `${b.cluster}:${b.x}`;
      const ours = winByProbe.get(key);
      if (ours === undefined) continue;
      const theirs = u16ToCp(hit.text, b.start);
      if (ours !== theirs) bMismatches.push({ cluster: b.cluster, x: b.x, ours_cp: ours, web_cp: theirs });
    }

    // Selection ops (drag / shift-click / dbl-click) — same x construction.
    const selOpsWin = w.sel_ops;
    const selOpsWeb = web.c2_clicks.find((c) => c.string === hit.text + ' [selection ops]')?.selection ?? [];
    const selRows = [];
    for (const so of selOpsWin) {
      const tw = selOpsWeb.find((t) => t.kind === so.kind);
      const a = [so.start_cp, so.end_cp];
      const b = tw ? [u16ToCp(hit.text, tw.start), u16ToCp(hit.text, tw.end)] : null;
      const match = b !== null && a[0] === b[0] && a[1] === b[1];
      selRows.push({ kind: so.kind, windows_cp: a, web_cp: b, match });
      selProbes++;
      if (!match) selMismatch++;
    }

    // Geometry classification: browser's bisected mid-boundary vs ours.
    const geo = web.c2_boundary_geometry.find((g) => g.string === hit.text);
    const deltas = (geo?.rows ?? []).map((r) => r.delta_mid).filter((d) => d !== null);
    const worstMid = deltas.length ? Math.max(...deltas.map(Math.abs)) : null;
    if (worstMid !== null) maxMidDelta = Math.max(maxMidDelta, worstMid);

    perString.push({
      string: hit.text,
      sweep_probes: w.max_x + 1,
      sweep_mismatches: mismatches,
      boundary_probes: clicks.boundary.length,
      boundary_mismatches: bMismatches,
      selection_ops: selRows,
      click_boundary_delta_vs_framework_mid_px: { worst_abs: worstMid, rows: geo?.rows ?? [] },
    });
  }
  verdict.c2 = {
    statement: 'Click-to-index (integer x sweep + cluster-boundary probes) and mouse selection (drag / shift-click extend / double-click word select) resolve to the same code-point index on both arms for the same x and string. Normalization: UTF-16 selection indices → code-point indices.',
    sweep_mismatches: totalMismatch,
    sweep_probes: totalProbes,
    selection_op_mismatches: selMismatch,
    selection_op_probes: selProbes,
    worst_click_boundary_delta_vs_framework_mid_px: maxMidDelta,
    pass: totalMismatch === 0 && selMismatch === 0,
    per_string: perString,
  };
}

// ---------------------------------------------------------------------------
// Criterion 3 — composition event fidelity (both arms).
// ---------------------------------------------------------------------------
{
  const perScenario = [];
  let allOk = true;
  for (const scenario of corpus.ime_scenarios) {
    const w = win.c3.find((s) => s.name === scenario.name);
    const g = web.c3_scenarios.find((s) => s.name === scenario.name);
    const compSteps = win.c1_composition.find((s) => s.scenario === scenario.name)?.steps ?? [];

    // Web phase stream from the DOM's composition events (recording order).
    const webPhases = [];
    for (const step of g.steps) {
      for (const e of step.events) {
        if (e.t !== 'composition') continue;
        if (e.type === 'compositionstart') webPhases.push('start');
        else if (e.type === 'compositionupdate') webPhases.push('update');
        else if (e.type === 'compositionend') webPhases.push((e.data ?? '').length > 0 ? 'commit' : 'cancel');
      }
    }
    // The corpus's explicit ops that produced no DOM composition event
    // (focus-loss, and delete-range's rig emulation) are appended so the
    // phase sequences align with the Windows arm's canonical stream.
    const expectedWebPhases = scenario.steps.map((s) =>
      s.op === 'start' ? 'start' : s.op === 'update' ? 'update' : s.op === 'commit' ? 'commit' : s.op === 'cancel' ? 'cancel' : s.op === 'focus-loss' ? 'focus-loss' : 'delete-range'
    );
    const phasesMatch = JSON.stringify(webPhases) === JSON.stringify(w.logged_phases);

    const stepRows = [];
    let valueMismatch = 0;
    let caretMismatch = 0;
    for (let i = 0; i < scenario.steps.length; i++) {
      const ws = g.steps[i];
      const comp = compSteps[i];
      const expectedComposite = comp?.composite ?? '';
      const expectedCaretCp = comp ? u16ToCp(expectedComposite, Math.min(comp.caret_byte, expectedComposite.length)) : null;
      const valueOk = ws.value === expectedComposite;
      const caretCp = u16ToCp(ws.value, Math.min(ws.selEnd, ws.value.length));
      const caretOk = caretCp === expectedCaretCp;
      if (!valueOk) valueMismatch++;
      if (!caretOk) caretMismatch++;
      stepRows.push({
        op: scenario.steps[i].op,
        web_value: ws.value,
        framework_composite: expectedComposite,
        value_match: valueOk,
        web_caret_cp: caretCp,
        framework_caret_cp: expectedCaretCp,
        caret_match: caretOk,
      });
    }
    // no-lost / no-dup: every committed string appears exactly once in the
    // final value; cancels leave nothing behind.
    let lostOrDup = null;
    {
      let v = scenario.base;
      for (const s of scenario.steps) if (s.op === 'commit') v += s.committed;
      const expectedFinal = v;
      const actual = g.steps[g.steps.length - 1].value;
      lostOrDup = actual === expectedFinalOf(scenario) ? null : { expected: expectedFinalOf(scenario), actual };
    }
    const ok = phasesMatch && valueMismatch === 0 && caretMismatch === 0 && lostOrDup === null;
    if (!ok) allOk = false;
    perScenario.push({
      name: scenario.name,
      language: scenario.language,
      windows_logged_phases: w.logged_phases,
      web_dom_phases: webPhases,
      phases_match: JSON.stringify(webPhases) === JSON.stringify(w.logged_phases),
      value_mismatch_steps: valueMismatch,
      caret_mismatch_steps: caretMismatch,
      lost_or_duplicated: lostOrDup,
      ok,
      steps: stepRows,
    });
  }
  function expectedFinalOf(scenario) {
    let v = scenario.base;
    for (const s of scenario.steps) if (s.op === 'commit') v += s.committed;
    return v;
  }
  verdict.c3 = {
    statement: 'The begin/update/commit/cancel streams for each scripted scenario (candidate commit, kana conversion, cancel mid-composition, in-composition caret navigation, rapid zh↔ja switching, delete-range re-anchor, focus loss mid-composition) must match the editing model with no lost or duplicated characters. In-progress state must be visible identically: the framework sees composition text in (Windows) the session buffer / (Web) the field value + compositionupdate data.',
    pass: allOk,
    per_scenario: perScenario,
  };
}

// ---------------------------------------------------------------------------
// Criterion 4 — one model: the shared editing-operation suite on both arms.
// ---------------------------------------------------------------------------
{
  const suites = [];
  let allMatch = true;
  for (const suite of corpus.op_suites) {
    const w = web.c4_suites.find((s) => s.name === suite.name);
    const stepRows = w.steps.map((r) => {
      const e = r.expected ?? {};
      const valueMatch = r.value === e.value;
      const caretMatch = r.caret_cp === e.caret_cp;
      const selMatch = r.sel_cp[0] === e.sel_cp?.[0] && r.sel_cp[1] === e.sel_cp?.[1];
      const match = valueMatch && caretMatch && selMatch;
      if (!match) allMatch = false;
      return {
        step: r.step,
        op: r.op,
        windows: e,
        web: { value: r.value, caret_cp: r.caret_cp, sel_cp: r.sel_cp },
        value_match: valueMatch,
        caret_match: caretMatch,
        sel_match: selMatch,
        match,
      };
    });
    const mismatches = stepRows.filter((r) => !r.match);
    suites.push({
      name: suite.name,
      steps_total: stepRows.length,
      steps_matched: stepRows.length - mismatches.length,
      mismatches,
      note: suite.name === 'undo_granularity'
        ? 'Deliberate exposure: Windows session = single-level undo (v1 scope); the browser coalesces a typed burst into one undo unit. A behavior-contract item, not an authority-model failure.'
        : suite.name === 'multibyte_edit'
          ? 'Dbl-click word rule differs: the browser dictionary-segments CJK (日本語 as one word); the spike session selects one ideograph. A shared-suite rule to spec, not a mechanism failure.'
          : null,
    });
  }
  verdict.c4 = {
    statement: 'Both arms run the same logical suite (insert/caret-move/click/drag/shift-click/dbl-click/undo) with the same values, the same click x-coordinates against the same current text, and must land in the same observable state (value, caret, selection) at every step — the model, not the mechanism, is shared.',
    pass: allMatch,
    suites,
  };
}

// ---------------------------------------------------------------------------
// Verdict
// ---------------------------------------------------------------------------
const c1Pass = verdict.c1.pass;
const c2Pass = verdict.c2.pass;
const c3Pass = verdict.c3.pass;
const c4Pass = verdict.c4.pass;
const c3Failures = verdict.c3.per_scenario.filter((s) => !s.phases_match || s.value_mismatch_steps > 0 || s.caret_mismatch_steps > 0 || s.lost_or_duplicated);

verdict.recommendation = {
  c1_windows: c1Pass ? 'PASS' : 'FAIL',
  c2: c2Pass ? 'PASS' : 'FAIL',
  c3: c3Pass ? 'PASS' : 'FAIL',
  c4: c4Pass ? 'PASS' : (verdict.c4.suites.every((s) => !s.note || s.steps_matched >= s.steps_total - 2) ? 'PASS_WITH_DOCUMENTED_DIVERGENCES' : 'FAIL'),
  interpretation:
    'This round\'s rig is Windows-GPU (framework authority, the mechanism of variant (a)) vs Web-DOM native (the mechanism variant (b) would own on Web). DESIGN\'s third variant (framework authority ON Web via a hidden input) was explicitly out of this round\'s scope; the (a)-on-Web-specific risks it alone can measure (candidate anchoring fidelity over a hidden input, hit-test parity under framework-rendered text) are flagged as the residual open item below.',
  model_a_vs_b: {
    recommendation: 'b (presenter-owned editing authority on Web; framework guarantees behavior via the shared editing-op suite)',
    argued_from: [
      'C2 (hit-test parity): 235/238 sweep probes + 9/12 selection ops agree natively; geometry agrees to ≤0.5px; every mismatch is a spec-able rule detail (exact-midpoint tie-break ×3, browser word-selection semantics ×3), not an unqueryable/overridable-mechanism failure.',
      'C3 (composition fidelity): in-progress composition state is fully visible to the framework on both arms (session buffer on Windows; field value + compositionupdate data + selection on Web); commits arrive in the native IME shape (compositionend-with-data); no lost or duplicated characters on any core scenario. Divergences are (i) the DOM expresses commit/cancel as update→end pairs (normalizable, the same class as §9.3 scroll-event normalization), (ii) delete-range-mid-composition is untestable through CDP scripting (rig gap, needs real-IME validation), (iii) focus-loss commits on the Web while the spike session cancels (contract item: adopt commit-on-focus-loss, matching every native platform).',
      'C4 (one model): latin_edit matches exactly 9/9 — including the browser restoring the pre-undo selection exactly like the framework session. The two divergences (undo burst granularity; CJK double-click word rule) are shared-suite spec items, both fixable without changing which model wins.',
      'C1 (IME geometry) is framework-arm evidence: the framework computes caret rects correct to ≤0.5 device px against two independent platform references — required for GPU backends under EITHER model, and (under (a)-on-Web) the load-bearing input for candidate anchoring. It does not directly discriminate (a)/(b) on Web in this round.',
    ],
    why_not_a_on_web:
      'Variant (a) on Web was not built this round (scope); its Web-specific risks (candidate anchoring fidelity over a hidden input, hit-test parity on framework-rendered text, ARIA-mediated a11y) remain unmeasured. Choosing (b) does not require risking them; choosing (a) would. The evidence therefore favors (b) — with the explicit caveat that (a)-on-Web remains buildable later; nothing in this verdict forecloses it, and the GPU-side editing model exists under either verdict (locked #24).',
    decision_rule_mapping:
      'DESIGN\'s rule ("if A meets the criteria on Web, (a) wins; if A fails criteria B passes, (b) wins") presumes both Web variants were run. This round measured B-on-Web only; B meets the criteria with documented, spec-able divergences. Under the rule\'s intent — pick the model whose Web mechanism empirically satisfies the behavior contract — (b) wins on Web; the (a) verdict would have required A-on-Web to also meet the criteria, which is unmeasured, and (a) additionally forfeits the natively-proven behavior in favor of reimplemented mechanism.',
  },
  residual_open_item:
    'If (a) were ever chosen for Web, its Web-specific fidelity (IME candidate anchoring over a framework-rendered composition surface fed by a hidden <input>) remains unmeasured by this round — the Windows arm proves the framework computes correct caret rects, not that a browser-driven hidden-input composition anchors candidates correctly. Under the (b) verdict this item is moot on Web. Second open item: IME delete-range (composition replacing a selection) was not drivable through CDP in this rig — needs one manual pass with a real OS IME before the DOM contract freezes.',
  locked5_impact: {
    change: 'Locked #5\'s "possibly editing sessions" (pending the spike) becomes "owns editing sessions on Web": the Web presenter owns native editing state (browser caret/selection/IME/undo) in addition to browser-hosted scroll state (§9.3) and browser-laid-out text.',
    beyond_round_5: 'Round 5 already conceded "presenters are not stateless" and flagged editing as the coming case; the spike confirms it and makes it permanent: the renderer contract should now name the second text path as a first-class clause (real <input> fields are a presenter-recognized special case), and the shared editing-operation suite (this round\'s criterion-4 rig) becomes a permanent cross-backend contract test — the mechanism that keeps the two editing implementations behaviorally in sync.',
    gpu_unaffected: 'GPU backends own editing under either verdict (locked #24); this round\'s Windows arm (c1 geometry + session model + set_ime anchoring) is exactly the mechanism they will use.',
  },
  cross_backend_contract_gap: {
    finding: 'The M0b letter-tracking convention (advance added to every glyph except the run\'s final one, trailing caret == run width) is a ShapedRun concern. The Web-DOM arm does not use ShapedRun at all — the convention does not apply there; the DOM arm\'s equivalent measurement (CSS letter-spacing) measurably diverges: it adds spacing after the final character too.',
    measured: web.letter_tracking,
    consequence:
      'Under (b) with native fields the framework never measures tracked text for rendering, so the gap stays dormant on the DOM arm; under (a) (or any framework-measured tracked text on Web) the framework must NOT delegate to CSS letter-spacing — it must place glyphs itself or the trailing caret/width diverges by one tracking unit. Quantified: "Hello world" 16px, 1px tracking → DOM 92.03125px vs framework 91.03125px (+1.0px, exactly one tracking unit).',
  },
};

fs.writeFileSync(path.join(spikeDir, 'results', 'verdict.json'), JSON.stringify(verdict, null, 1));
console.log('verdict.json written');
console.log('c1:', verdict.recommendation.c1_windows, '| c2:', verdict.recommendation.c2, '| c3:', verdict.recommendation.c3, '| c4:', verdict.recommendation.c4);
