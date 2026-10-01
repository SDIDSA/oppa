// M7 §8.5 parity corpus: engine-measured vs browser-rendered, flat subset.
//
// Loads the oppa-dom-generated page (spike/results/parity_page.html — every
// element absolutely positioned, browsers lay out nothing) beside the
// engine's expectations (spike/results/parity_expected.json — committed
// boxes + text widths), measures every [data-pid] via getBoundingClientRect
// in headless Edge, and writes spike/results/parity.json with per-row
// verdicts. Box rows and text-WIDTH rows gate the verdict (flat subset);
// text-HEIGHT rows are record-only (browser line box vs engine metrics is
// a known second-order difference — listed, never hidden, never gating).
//
// Tolerances (stated, not tuned-to-green): boxes ±0.5px (explicit engine
// boxes vs explicit CSS placement — subpixel positions must survive);
// text widths ±1.0px (DWrite vs browser shaping — the M1 rig found
// untracked widths exact, so 1.0 carries real margin).
import puppeteer from 'puppeteer-core';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const resultsDir = path.resolve(here, '..', 'results');
const expected = JSON.parse(fs.readFileSync(path.join(resultsDir, 'parity_expected.json'), 'utf8'));
const pageFile = path.join(resultsDir, 'parity_page.html');

const BOX_TOL = 0.5;
const TEXT_W_TOL = 1.0;

const EDGE_CANDIDATES = [
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
];
const edge = EDGE_CANDIDATES.find((p) => fs.existsSync(p));
if (!edge) {
  console.error('M7 parity: no Edge binary found (loud requirement, never a silent pass)');
  process.exit(2);
}

const browser = await puppeteer.launch({
  executablePath: edge,
  headless: true,
  args: ['--font-render-hinting=none', '--disable-lcd-text', '--hide-scrollbars'],
  defaultViewport: { width: 500, height: 400, deviceScaleFactor: 1 },
});
const page = await browser.newPage();
await page.goto('file:///' + pageFile.replaceAll('\\', '/'));

const dpr = await page.evaluate(() => window.devicePixelRatio);
const ua = await browser.userAgent();

const measured = await page.evaluate(() => {
  const out = {};
  for (const el of document.querySelectorAll('[data-pid]')) {
    const r = el.getBoundingClientRect();
    out[el.getAttribute('data-pid')] = { x: r.x, y: r.y, w: r.width, h: r.height };
  }
  return out;
});

const rows = [];
let green = 0;
let gated = 0;
for (const c of expected.cases) {
  const m = measured[c.pid];
  if (!m) {
    rows.push({ pid: c.pid, kind: c.kind, verdict: 'fail', note: 'missing in DOM' });
    gated++;
    continue;
  }
  if (c.kind === 'box') {
    gated++;
    const dx = Math.abs(m.x - c.x);
    const dy = Math.abs(m.y - c.y);
    const dw = Math.abs(m.w - c.w);
    const dh = Math.abs(m.h - c.h);
    const ok = dx <= BOX_TOL && dy <= BOX_TOL && dw <= BOX_TOL && dh <= BOX_TOL;
    if (ok) green++;
    rows.push({
      pid: c.pid, kind: 'box', verdict: ok ? 'pass' : 'fail',
      engine: { x: c.x, y: c.y, w: c.w, h: c.h },
      browser: { x: m.x, y: m.y, w: m.w, h: m.h },
      deltas: { dx, dy, dw, dh },
    });
  } else if (c.kind === 'text') {
    gated++;
    const dx = Math.abs(m.x - c.x);
    const dy = Math.abs(m.y - c.y);
    const dw = Math.abs(m.w - c.w);
    const ok = dx <= BOX_TOL && dy <= BOX_TOL && dw <= TEXT_W_TOL;
    if (ok) green++;
    rows.push({
      pid: c.pid, kind: 'text-width', verdict: ok ? 'pass' : 'fail',
      engine: { x: c.x, y: c.y, w: c.w }, browser: { x: m.x, y: m.y, w: m.w },
      deltas: { dx, dy, dw },
    });
    // Record-only height row (never gating — see header).
    rows.push({
      pid: c.pid, kind: 'text-height-record', verdict: 'record',
      engine: { h: c.h }, browser: { h: m.h }, deltas: { dh: Math.abs(m.h - c.h) },
    });
  }
}

const verdict = {
  tool: 'm7-parity',
  browser: ua,
  devicePixelRatio: dpr,
  dpr_ok: dpr === 1,
  box_tol: BOX_TOL,
  text_w_tol: TEXT_W_TOL,
  gated_rows: gated,
  green_rows: green,
  match_rate: gated === 0 ? 0 : green / gated,
  pass: dpr === 1 && gated > 0 && green === gated,
  rows,
};
fs.writeFileSync(path.join(resultsDir, 'parity.json'), JSON.stringify(verdict, null, 2));
console.log(`M7 parity: ${green}/${gated} gated rows green (match_rate=${verdict.match_rate})`);
for (const r of rows) {
  if (r.verdict === 'fail') console.log(`  FAIL ${r.kind} ${r.pid} deltas=${JSON.stringify(r.deltas)}`);
  if (r.verdict === 'record') console.log(`  RECORD ${r.kind} ${r.pid} engine_h=${r.engine.h} browser_h=${r.browser.h}`);
}
await browser.close();
process.exit(verdict.pass ? 0 : 1);
