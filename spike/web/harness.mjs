// DESIGN §9.2 spike — Web-DOM arm harness.
//
// Drives a REAL <input type="text"> in a real Chromium (Edge) through its
// native input machinery — real clicks/drags/keys, IME through the browser's
// own composition pipeline (CDP IME scripting routes through the same
// text-input state machine an OS IME drives) — and records everything the
// DOM reports back (composition events, beforeinput, selection API). The
// hook script in index.html only records: no JS-side IME handling, no
// selection overriding, no custom editors.
//
// Inputs:  spike/corpus.json + spike/results/windows.json (cluster tables,
//          op x-coordinates, expected suite states).
// Output:  spike/results/web.json (raw per-probe/per-step records).
import puppeteer from 'puppeteer-core';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const spikeDir = path.resolve(here, '..');
const root = path.resolve(spikeDir, '..');
const corpus = JSON.parse(fs.readFileSync(path.join(spikeDir, 'corpus.json'), 'utf8'));
const win = JSON.parse(fs.readFileSync(path.join(root, 'spike', 'results', 'windows.json'), 'utf8'));

const EDGE_CANDIDATES = [
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
];
const edge = EDGE_CANDIDATES.find((p) => fs.existsSync(p));

const results = {
  arm: 'web-dom',
  rig_version: corpus.rig_version,
  browser: edge ? 'edge' : 'unknown',
  c2_clicks: [],
  c2_caret_rect: [],
  c2_boundary_geometry: [],
  c3_scenarios: [],
  c4_suites: [],
  rig_notes: [],
};

let compSpan = { start: null, text: '' };
let lastEventCount = 0;

// JS has no byte offsets; convert corpus byte offsets to code-point indices.
const byteToCp = (str, byte) => {
  let b = 0;
  let cp = 0;
  for (const ch of str) {
    if (b >= byte) break;
    b += Buffer.byteLength(ch, 'utf8');
    cp++;
  }
  return cp;
};
const utf16Len = (str) => [...str].length;

const browser = await puppeteer.launch({
  executablePath: edge,
  headless: true,
  args: [
    // Subpixel-faithful advances, matching the framework arm's shaping rule
    // (advances stay subpixel; rounding only at commit positions). GDI-style
    // hinting would quantize every advance ~1px; recorded as a rig factor.
    '--font-render-hinting=none',
    '--disable-lcd-text',
    '--hide-scrollbars',
  ],
  defaultViewport: { width: 500, height: 120, deviceScaleFactor: 1 },
});
const page = await browser.newPage();
await page.goto('file:///' + path.join(spikeDir, 'web', 'index.html').replaceAll('\\', '/'));
await page.evaluate((stack) => window.__setFont(stack), corpus.font.font_stack.join(', '));
const client = await page.createCDPSession();

const CLICK_Y = 30; // inside the field's line box

async function mouse(type, x, y, opts = {}) {
  await client.send('Input.dispatchMouseEvent', {
    type,
    x,
    y,
    button: 'left',
    buttons: opts.buttons ?? 0,
    clickCount: opts.clickCount ?? 1,
    modifiers: opts.modifiers ?? 0,
  });
}

async function click(x, y = CLICK_Y, opts = {}) {
  await mouse('mousePressed', x, y, { ...opts, buttons: 1 });
  await mouse('mouseReleased', x, y, { buttons: 0, clickCount: opts.clickCount ?? 1 });
}

async function state() {
  return page.evaluate(() => window.__state());
}
const resetEvents = () => page.evaluate(() => window.__resetEvents());
const events = () => page.evaluate(() => window.__events);

async function resetField(value) {
  await page.evaluate((v) => {
    const f = document.getElementById('field');
    f.value = v;
    f.focus();
    const end = v.length;
    f.setSelectionRange(end, end);
  }, value);
  await resetEvents();
}

// ---------------------------------------------------------------------------
// Criterion 2: click → index sweep, boundary probes, selection ops,
// caret-rect record, boundary-geometry diagnostic.
// ---------------------------------------------------------------------------
for (const hit of corpus.hit_strings) {
  const { text, width_dpr1, clusters } = hit;
  await resetField(text);

  const sweep = [];
  const maxX = Math.ceil(width_dpr1) + 2;
  for (let x = 0; x <= maxX; x++) {
    await click(x);
    const st = await state();
    sweep.push(st.start);
  }

  const boundary = [];
  for (let i = 0; i < clusters.length; i++) {
    const c = clusters[i];
    for (const x of [Math.round(c.x - 1), Math.round(c.x + 1), Math.round(c.x + c.w * 0.5), Math.round(c.x + c.w - 1)]) {
      await click(Math.max(0, x));
      const st = await state();
      boundary.push({ cluster: i, x, start: st.start, end: st.end });
    }
  }

  const midX = Math.round(width_dpr1 * 0.5);
  await click(midX);
  const caret = await state();

  results.c2_clicks.push({ string: text, max_x: maxX, sweep, boundary });
  results.c2_caret_rect.push({ string: text, at_x: midX, caret_rect: caret.caretRect, start: caret.start, end: caret.end });

  // Mouse-driven selection over the multi-byte strings (corpus requirement):
  // real drag, shift-click extend, double-click word select. The same ops run
  // through the Windows session (c2_sel in windows.json) with the same x
  // construction (cluster k = min(k, n-1)).
  {
    const selOps = [];
    const dragTo = Math.min(3, clusters.length - 1);
    {
      const from = clusters[0].x + 1;
      const to = clusters[dragTo].x + 1;
      await mouse('mousePressed', from, CLICK_Y, { buttons: 1 });
      for (let x = from; x <= to; x += 4) await mouse('mouseMoved', x, CLICK_Y, { buttons: 1 });
      await mouse('mouseMoved', to, CLICK_Y, { buttons: 1 });
      await mouse('mouseReleased', to, CLICK_Y, { buttons: 0 });
      const st = await state();
      selOps.push({ kind: 'drag', from_x: from, to_x: to, start: st.start, end: st.end });
    }
    {
      await click(clusters[0].x + 1);
      await click(clusters[Math.min(2, clusters.length - 1)].x + 1, CLICK_Y, { modifiers: 8 });
      const st = await state();
      selOps.push({ kind: 'shift-click', start: st.start, end: st.end });
    }
    {
      const x = clusters[Math.min(1, clusters.length - 1)].x + 1;
      await mouse('mousePressed', x, CLICK_Y, { buttons: 1, clickCount: 1 });
      await mouse('mouseReleased', x, CLICK_Y, { buttons: 0, clickCount: 1 });
      await mouse('mousePressed', x, CLICK_Y, { buttons: 1, clickCount: 2 });
      await mouse('mouseReleased', x, CLICK_Y, { buttons: 0, clickCount: 2 });
      const st = await state();
      selOps.push({ kind: 'dbl-click', x, start: st.start, end: st.end });
    }
    results.c2_clicks.push({ string: text + ' [selection ops]', selection: selOps });
  }
}

// Boundary-geometry diagnostic (classification aid, not a pass/fail surface):
// bisect the INPUT's own real-click boundary for each cluster (smallest x
// where the browser's native click mapping yields index >= i+1), vs the
// Windows arm's boundary x. This measures the geometry of the NATIVE click
// pipeline directly (the div/caretRangeFromPoint variant uses a different
// snap rule and is recorded separately as a note).
for (const hit of corpus.hit_strings) {
  const rows = [];
  await resetField(hit.text);
  for (let i = 0; i < hit.clusters.length; i++) {
    const c = hit.clusters[i];
    const trailing = i + 1 < hit.clusters.length ? hit.clusters[i + 1].x : hit.width_dpr1;
    const target = i + 1;
    let lo = 0;
    let hi = Math.round(hit.width_dpr1) + 6;
    let browserX = null;
    for (let k = 0; k < 14; k++) {
      const mid = (lo + hi) / 2;
      await click(Math.max(0, Math.round(mid)));
      const s = (await state()).start;
      if (s === null || s === undefined) break;
      if (s >= target) hi = mid;
      else lo = mid;
      browserX = hi;
    }
    rows.push({ cluster: i, ours_x: trailing, ours_mid: trailing - c.w / 2, browser_x: browserX, delta_mid: browserX === null ? null : browserX - (trailing - c.w / 2) });
  }
  results.c2_boundary_geometry.push({ string: hit.text, rows, basis: 'real input clicks (1px-resolution bisection)' });
}

// ---------------------------------------------------------------------------
// Letter-tracking convention probe (the M0b cross-backend contract gap):
// CSS letter-spacing adds spacing after the LAST character too (per CSS),
// while M0b's convention adds it to every advance EXCEPT the run's final
// glyph so the trailing caret equals the run width. Measure both on the DOM
// side so the divergence is compared, not assumed.
// ---------------------------------------------------------------------------
{
  const tracked = await page.evaluate((text, spacing) => {
    const probe = document.getElementById('probe');
    probe.style.letterSpacing = spacing + 'px';
    probe.textContent = text;
    const range = document.createRange();
    range.selectNodeContents(probe.firstChild);
    const w = range.getBoundingClientRect().width;
    probe.style.letterSpacing = '0px';
    return w;
  }, 'Hello world', corpus.font.tracking_px);
  const plain = await page.evaluate(() => {
    const probe = document.getElementById('probe');
    probe.textContent = 'Hello world';
    const range = document.createRange();
    range.selectNodeContents(probe.firstChild);
    return range.getBoundingClientRect().width;
  });
  const base = corpus.hit_strings.find((s) => s.text === 'Hello world');
  results.letter_tracking = {
    string: 'Hello world',
    css_letter_spacing_px: corpus.font.tracking_px,
    dom_width_plain: plain,
    dom_width_tracked: tracked,
    framework_width_plain: base.width_dpr1,
    framework_width_tracked: base.width_tracking_dpr1,
    convention: 'M0b: spacing on every advance except the final glyph (trailing caret == run width); CSS letter-spacing: after every character including the final one',
  };
}

// ---------------------------------------------------------------------------
// Criterion 3: IME via the browser's native IME input path. The CDP IME
// scripting surface's param shape varies by Chromium build — discover it
// empirically and record which shape worked.
// ---------------------------------------------------------------------------
async function tryCall(method, params) {
  try {
    await client.send(method, params);
    return true;
  } catch (e) {
    return { error: String(e?.message ?? e).slice(0, 300) };
  }
}

const compShapes = [
  (start, end, text, caret) => ({ selectionStart: start, selectionEnd: end, compositionText: text, compositionCaret: caret }),
  (start, end, text, caret) => ({ selectionStart: start, selectionEnd: end, text, newCursor: caret }),
  (start, end, text, caret) => ({ selectionStart: start, selectionEnd: end, compositionText: text, newCursor: caret }),
  (start, end, text, caret) => ({ selectionStart: start, selectionEnd: end, text, caret }),
];
let setCompositionShape = null;
{
  await resetField('');
  for (const shape of compShapes) {
    const res = await tryCall('Input.imeSetComposition', shape(0, 0, 'a', 1));
    const st = await state();
    if (res === true && st.value.includes('a')) {
      setCompositionShape = shape;
      break;
    }
  }
  if (setCompositionShape) {
    await imeCancelRaw(0, 1);
    await imeCancelRaw(0, 0);
    results.rig_notes.push({ rig: 'Input.imeSetComposition works; param shape resolved' });
  } else {
    results.rig_notes.push({ rig: 'no Input.imeSetComposition shape accepted — criterion 3 web is rig-limited, NOT a model verdict' });
  }
}

const commitCandidates = [
  { method: 'Input.imeCommitText', params: (start, end, text) => ({ text, selectionStart: start, selectionEnd: end }) },
  { method: 'Ime.imeCommitText', params: (start, end, text) => ({ text, selectionStart: start, selectionEnd: end }) },
  { method: 'Input.imeCommitText', params: (start, end, text) => ({ text, relativeSelectionStart: start, relativeSelectionEnd: end }) },
];
let commitPath = null;
{
  await resetField('');
  const probeLog = [];
  for (const cand of commitCandidates) {
    if (setCompositionShape) await tryCall('Input.imeSetComposition', setCompositionShape(0, 0, 'x', 1));
    const res = await tryCall(cand.method, cand.params(0, 1, 'b'));
    const st = await state();
    probeLog.push({ method: cand.method, params: Object.keys(cand.params(0, 1, 'x')), ok: res === true, error: res === true ? null : res.error, value: st.value });
    if (res === true && st.value.includes('b')) {
      commitPath = cand;
      break;
    }
  }
  // Native commit shape probe: Input.insertText *during* an active
  // composition — a real OS IME's commit flows through the same
  // TextInputClient::InsertText path (compositionend-with-data).
  if (!commitPath) {
    await resetField('');
    if (setCompositionShape) await tryCall('Input.imeSetComposition', setCompositionShape(0, 0, 'x', 1));
    const evBefore = (await events()).length;
    await client.send('Input.insertText', { text: 'b' });
    const st = await state();
    const evs = (await events()).slice(evBefore);
    probeLog.push({ method: 'Input.insertText during composition', ok: st.value === 'b', value: st.value, events: evs.map((e) => e.t + ':' + (e.type ?? e.inputType) + ':' + (e.data ?? '')) });
    if (st.value === 'b' && evs.some((e) => e.t === 'composition' && e.type === 'compositionend')) {
      commitPath = { method: 'Input.insertText', params: (_s, _e, text) => ({ text }) };
    }
  }
  results.rig_notes.push({ rig: 'commit probing', probe_log: probeLog });
  results.rig_notes.push({ rig: 'commit path: ' + (commitPath ? commitPath.method : 'none — cancel+insertText fallback (recorded shape difference)') });
}

async function imeCancelRaw(start, end) {
  if (!setCompositionShape) return false;
  return (await tryCall('Input.imeSetComposition', setCompositionShape(start, end, '', 0))) === true;
}
async function imeStartUpdate(start, end, text, caretInComposition) {
  if (!setCompositionShape) return false;
  return (await tryCall('Input.imeSetComposition', setCompositionShape(start, end, text, caretInComposition))) === true;
}
async function imeCommit(start, end, text) {
  if (commitPath) return (await tryCall(commitPath.method, commitPath.params(start, end, text))) === true;
  // Rig fallback: cancel the composition, then insert (a different event
  // shape — the compare step judges this separately).
  await imeCancelRaw(start, end);
  await client.send('Input.insertText', { text });
  return true;
}

for (const scenario of corpus.ime_scenarios) {
  await resetField(scenario.base);
  compSpan = { start: null, text: '' };
  lastEventCount = 0;
  const steps = [];
  for (const step of scenario.steps) {
    if (step.op === 'start') {
      compSpan.start = step.start_byte;
      compSpan.text = '';
      await imeStartUpdate(step.start_byte, step.start_byte, '', 0);
    } else if (step.op === 'update') {
      const s = compSpan.start ?? byteToCp(scenario.base, scenario.base.length);
      const s16 = [...scenario.base].length === scenario.base.length ? s : s; // ASCII bases only
      await imeStartUpdate(s16, s16 + utf16Len(compSpan.text), step.composition, byteToCp(step.composition, step.caret_byte - s));
      compSpan.text = step.composition;
    } else if (step.op === 'commit') {
      const s = compSpan.start ?? [...scenario.base].length;
      await imeCommit(s, s + utf16Len(compSpan.text), step.committed);
      compSpan.text = '';
      compSpan.start = s + utf16Len(step.committed);
    } else if (step.op === 'cancel') {
      const s = compSpan.start ?? [...scenario.base].length;
      await imeCancelRaw(s, s + utf16Len(compSpan.text));
      compSpan.text = '';
    } else if (step.op === 'delete-range') {
      // The IME re-anchors over committed content it deletes; emulate via the
      // composition span itself (select the range, restart the composition).
      await page.evaluate((r) => {
        const f = document.getElementById('field');
        f.setSelectionRange(r[0], r[1]);
      }, step.range);
      compSpan.start = step.range[0];
      compSpan.text = '';
      await imeStartUpdate(step.range[0], step.range[1], '', 0);
    } else if (step.op === 'focus-loss') {
      await page.evaluate(() => document.getElementById('field').blur());
    }
    const st = await state();
    const evs = await events();
    steps.push({
      op: step.op,
      value: st.value,
      selStart: st.start,
      selEnd: st.end,
      events: evs.slice(lastEventCount),
    });
    lastEventCount = evs.length;
  }
  results.c3_scenarios.push({
    name: scenario.name,
    language: scenario.language,
    base: scenario.base,
    steps,
    events: await events(),
  });
  await page.evaluate(() => document.getElementById('field').focus());
}

// ---------------------------------------------------------------------------
// Criterion 4: shared editing-op suite driven natively. Geometry ops use the
// Windows arm's resolved op_x (same x, same current value on both arms);
// insert/caret-move/undo are real keys; drag/shift/dbl-click are real mice.
// ---------------------------------------------------------------------------
const cpToSel = (value, cp) => [...value].slice(0, cp).reduce((n, ch) => n + ch.length, 0);

for (const suite of corpus.op_suites) {
  const winSuite = win.c4.find((s) => s.name === suite.name);
  await resetField(suite.base);
  const rows = [];
  for (let i = 0; i < suite.steps.length; i++) {
    const step = suite.steps[i];
    const opX = winSuite?.steps[i]?.op_x ?? [];
    if (step.op === 'insert') {
      await page.keyboard.type(step.text);
    } else if (step.op === 'caret-move') {
      for (let k = 0; k < Math.abs(step.steps); k++) {
        await page.keyboard.press(step.steps > 0 ? 'ArrowRight' : 'ArrowLeft');
      }
    } else if (step.op === 'click') {
      await click(Math.round(opX[0]));
    } else if (step.op === 'drag') {
      const from = Math.round(opX[0]);
      const to = Math.round(opX[1]);
      await mouse('mousePressed', from, CLICK_Y, { buttons: 1 });
      for (let x = from; x <= to; x += 4) await mouse('mouseMoved', x, CLICK_Y, { buttons: 1 });
      await mouse('mouseMoved', to, CLICK_Y, { buttons: 1 });
      await mouse('mouseReleased', to, CLICK_Y, { buttons: 0 });
    } else if (step.op === 'shift-click') {
      await click(Math.round(opX[0]));
      await click(Math.round(opX[1]), CLICK_Y, { modifiers: 8 });
    } else if (step.op === 'dbl-click') {
      const x = Math.round(opX[0]);
      await mouse('mousePressed', x, CLICK_Y, { buttons: 1, clickCount: 1 });
      await mouse('mouseReleased', x, CLICK_Y, { buttons: 0, clickCount: 1 });
      await mouse('mousePressed', x, CLICK_Y, { buttons: 1, clickCount: 2 });
      await mouse('mouseReleased', x, CLICK_Y, { buttons: 0, clickCount: 2 });
    } else if (step.op === 'end') {
      await page.keyboard.press('End');
    } else if (step.op === 'home') {
      await page.keyboard.press('Home');
    } else if (step.op === 'undo') {
      await page.keyboard.down('Control');
      await page.keyboard.press('z');
      await page.keyboard.up('Control');
    }
    const st = await state();
    rows.push({
      step: i,
      op: step.op,
      op_x: opX,
      value: st.value,
      selStart: st.start,
      selEnd: st.end,
      caret_cp: cpToSel(st.value, st.end),
      sel_cp: [cpToSel(st.value, st.start), cpToSel(st.value, st.end)],
      expected: winSuite?.steps[i]
        ? { value: winSuite.steps[i].value, caret_cp: winSuite.steps[i].caret_cp, sel_cp: winSuite.steps[i].sel }
        : null,
    });
  }
  results.c4_suites.push({ name: suite.name, base: suite.base, steps: rows });
}

await browser.close();

fs.writeFileSync(path.join(spikeDir, 'results', 'web.json'), JSON.stringify(results, null, 1));
console.log('web.json written');
