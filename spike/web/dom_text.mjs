// M7 verdict-(b) text/edit path: the shared editing-operation suite
// through the DOM backend's real `<input>` (spike verdict (b), locked #27).
//
// The op SEQUENCE mirrors `crates/spike-textedit/src/rig.rs::op_suites`
// (latin_edit, multibyte dblclick word rule, undo_granularity) — the rig
// stays the single source of truth; this script re-drives the same ops
// natively against oppa-dom-generated field HTML and records value + caret
// + selection at every step into spike/results/dom_text.json. Geometry ops
// resolve empirically (click-scan boundary mapping, same method as the
// spike's c2 bisect) — no engine numbers are trusted for targeting.
//
// Normalizations (stated, same class as §9.3 scroll normalization):
// - input chrome stripped in-page (border/padding/margin 0) so text starts
//   at the content box;
// - caret-to-0 via Home key (keyboard-equivalent of ClickCluster{0});
// - typing via trusted `Input.insertText` (the harness's own typing path).
import puppeteer from 'puppeteer-core';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const resultsDir = path.resolve(here, '..', 'results');
const pageFile = path.join(resultsDir, 'dom_field.html');

const EDGE_CANDIDATES = [
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
];
const edge = EDGE_CANDIDATES.find((p) => fs.existsSync(p));
if (!edge) {
  console.error('M7 dom_text: no Edge binary found (loud requirement, never a silent pass)');
  process.exit(2);
}

const browser = await puppeteer.launch({
  executablePath: edge,
  headless: true,
  args: ['--font-render-hinting=none', '--disable-lcd-text', '--hide-scrollbars'],
  defaultViewport: { width: 500, height: 200, deviceScaleFactor: 1 },
});
const page = await browser.newPage();
const client = await page.createCDPSession();
await page.goto('file:///' + pageFile.replaceAll('\\', '/'));
// Test-only chrome normalization (see header): production HTML keeps its chrome.
await page.evaluate(() => {
  const s = document.createElement('style');
  s.textContent = 'input{border:0!important;padding:0!important;margin:0!important;}';
  document.head.appendChild(s);
});

const state = () => page.evaluate(() => {
  const el = document.querySelector('input');
  return { value: el.value, start: el.selectionStart, end: el.selectionEnd };
});
const click = async (x, y, clickCount = 1) => {
  await client.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount });
  await client.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount });
};
const dblclick = async (x, y) => {
  await click(x, y, 1);
  await click(x, y, 2);
};
const drag = async (x1, y, x2) => {
  await client.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: x1, y, button: 'left', buttons: 1, clickCount: 1 });
  await client.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: x2, y, button: 'left', buttons: 1 });
  await client.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: x2, y, button: 'left', buttons: 0, clickCount: 1 });
};
const type = (text) => client.send('Input.insertText', { text });
const key = async (vk, keyName, modifiers = 0) => {
  await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', modifiers, windowsVirtualKeyCode: vk, key: keyName });
  await client.send('Input.dispatchKeyEvent', { type: 'keyUp', modifiers, windowsVirtualKeyCode: vk, key: keyName });
};
const HOME = 36, END = 35, RIGHT = 39, KEY_Z = 90;

// Click-scan boundary map: sweep x across the input, record where the
// reported caret changes (each plateau = one cluster; edges = boundaries).
async function boundaries() {
  const rect = await page.evaluate(() => {
    const r = document.querySelector('input').getBoundingClientRect();
    return { x: r.x, y: r.y, h: r.height };
  });
  const y = rect.y + rect.h / 2;
  const edges = [];
  let prev = null;
  for (let x = Math.floor(rect.x); x < rect.x + 400; x += 1) {
    await click(x, y);
    const st = await state();
    if (st.start === st.end && st.start !== prev) {
      edges.push({ caret: st.start, x });
      prev = st.start;
    }
    if (prev !== null && st.start === (await state()).end && x > rect.x + 300 && st.start === prev && edges.length > 0) {
      // settled past the text end (trailing caret plateau); keep a tail sample then stop
      if (x > edges[edges.length - 1].x + 30) break;
    }
  }
  return { rect, edges };
}
const mid = (edges, caret) => {
  const i = edges.findIndex((e) => e.caret === caret);
  if (i < 0) return null;
  const lo = edges[i].x;
  const hi = i + 1 < edges.length ? edges[i + 1].x : lo + 20;
  return (lo + hi) / 2;
};

const suites = [];
const divergences = [];
const check = (suite, op, cond, got, want) => {
  suite.steps.push({ op, ...got, want, verdict: cond ? 'pass' : 'fail' });
  if (!cond) suite.failed = (suite.failed ?? 0) + 1;
};

// --- latin_edit on "Hello world" (rig.rs op_suites()[0]) ---
{
  const suite = { name: 'latin_edit', steps: [] };
  let st = await state();
  check(suite, 'base', st.value === 'Hello world', st, { value: 'Hello world' });
  const { rect, edges } = await boundaries();
  const y = rect.y + rect.h / 2;
  // ClickCluster{0}: 1px inside the first cluster.
  await click(edges[0].x + 1, y);
  st = await state();
  check(suite, 'ClickCluster{0}', st.start === 0 && st.end === 0, st, { start: 0, end: 0 });
  await type('X');
  st = await state();
  check(suite, 'Insert{X}', st.value === 'XHello world' && st.start === 1, st, { value: 'XHello world', start: 1 });
  await key(RIGHT, 'ArrowRight');
  await key(RIGHT, 'ArrowRight');
  st = await state();
  check(suite, 'CaretMove{+2}', st.start === 3 && st.end === 3, st, { start: 3, end: 3 });
  // DragCluster{3→8}: middles of clusters 3 and 8 (re-map: text changed).
  const e2 = (await boundaries()).edges;
  await drag(mid(e2, 3), y, mid(e2, 8));
  st = await state();
  check(suite, 'DragCluster{3,8}', st.start === 3 && st.end === 8, st, { start: 3, end: 8 });
  await type('!');
  st = await state();
  check(suite, 'Insert{!}', st.value === 'XHe!orld' && st.start === 4, st, { value: 'XHe!orld', start: 4 });
  await key(KEY_Z, 'Z', 2);
  st = await state();
  check(
    suite, 'Undo',
    st.value === 'XHello world' && st.start === 3 && st.end === 8,
    st, { value: 'XHello world', start: 3, end: 8 },
  );
  await key(END, 'End');
  st = await state();
  check(suite, 'EndKey', st.start === st.value.length, st, { start: st.value.length });
  await type('!');
  st = await state();
  check(suite, 'Insert{!}@end', st.value === 'XHello world!', st, { value: 'XHello world!' });
  await key(KEY_Z, 'Z', 2);
  st = await state();
  check(suite, 'Undo@burst', st.value === 'XHello world', st, { value: 'XHello world' });
  suites.push(suite);
}

// --- multibyte dblclick word rule on "日本語x" (op_suites()[1] tail) ---
{
  const suite = { name: 'multibyte_edit', steps: [] };
  await key(HOME, 'Home');
  await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', modifiers: 8, windowsVirtualKeyCode: END, key: 'End' });
  await client.send('Input.dispatchKeyEvent', { type: 'keyUp', modifiers: 8, windowsVirtualKeyCode: END, key: 'End' });
  await type('日本語x');
  let st = await state();
  check(suite, 'replace-all', st.value === '日本語x', st, { value: '日本語x' });
  const { rect, edges } = await boundaries();
  const y = rect.y + rect.h / 2;
  await dblclick(mid(edges, 1), y);
  st = await state();
  // Adopted rule (locked #27, REPORT finding #2): the browser
  // dictionary-segments 日本語 as one word → [0,3).
  const ok = st.start === 0 && st.end === 3;
  check(suite, 'DblClickCluster{1}', ok, st, { start: 0, end: 3 });
  if (!ok) divergences.push({ suite: 'multibyte_edit', op: 'DblClickCluster{1}', got: st, rule: 'CJK dictionary segmentation [0,3)' });
  suites.push(suite);
}

// --- undo_granularity (op_suites()[2]): the KNOWN divergence row ---
{
  const suite = { name: 'undo_granularity', steps: [] };
  await key(HOME, 'Home');
  await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', modifiers: 8, windowsVirtualKeyCode: END, key: 'End' });
  await client.send('Input.dispatchKeyEvent', { type: 'keyUp', modifiers: 8, windowsVirtualKeyCode: END, key: 'End' });
  await type('Hello world');
  await key(HOME, 'Home');
  await type('a');
  await type('b');
  let st = await state();
  check(suite, 'type{a}{b}', st.value === 'abHello world', st, { value: 'abHello world' });
  await key(KEY_Z, 'Z', 2);
  st = await state();
  // KNOWN divergence (criterion-4 evidence): the browser coalesces the
  // typed burst (restores pre-ab); the framework session is single-level
  // (restores pre-b). Either browser shape is recorded, not failed —
  // the divergence from single-level is structural, this row names it.
  const coalesced = st.value === 'Hello world';
  const split = st.value === 'aHello world';
  divergences.push({
    suite: 'undo_granularity', op: 'Undo',
    got: st, coalesced, split,
    rule: 'browser burst coalescing vs single-level session',
  });
  suite.steps.push({ op: 'Undo', ...st, verdict: coalesced || split ? 'pass' : 'fail' });
  if (!(coalesced || split)) suite.failed = (suite.failed ?? 0) + 1;
  suites.push(suite);
}

const failed = suites.reduce((n, s) => n + (s.failed ?? 0), 0);
const verdict = {
  tool: 'm7-dom-text',
  suites,
  divergences,
  failed_steps: failed,
  pass: failed === 0,
};
fs.writeFileSync(path.join(resultsDir, 'dom_text.json'), JSON.stringify(verdict, null, 2));
console.log(`M7 dom_text: ${failed} failed steps across ${suites.length} suites`);
for (const d of divergences) console.log(`  DIVERGENCE ${d.suite} ${d.op}: ${JSON.stringify(d.got)}`);
await browser.close();
process.exit(verdict.pass ? 0 : 1);
