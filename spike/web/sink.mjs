// Kitchen sink on web (Round 7.17): the sink Edge pass.
//
// Serves crates/oppa-web/web over a raw python http.server, loads
// sink.html (the `WebApp.new_sink()` root) in headless Edge, and
// drives every tab through the real input pipeline (pointer events
// -> wasm inject -> DOM swap): tab switches, modal confirm,
// checkbox vector check, select pick, slider step, text typing,
// mocked fetch, file pick + counter. Writes
// spike/results/sink.json with the verdict.
//
// Exit codes: 0 pass, 1 gated fail, 2 environment (no Edge/server).
import puppeteer from 'puppeteer-core';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

const here = path.dirname(fileURLToPath(import.meta.url));
const webDir = path.resolve(here, '..', '..', 'crates', 'oppa-web', 'web');
const resultsDir = path.resolve(here, '..', 'results');
const PORT = 8932;

const EDGE_CANDIDATES = [
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
];
const edge = EDGE_CANDIDATES.find((p) => fs.existsSync(p));
if (!edge) {
  console.error('sink: no Edge binary found (loud requirement, never a silent pass)');
  process.exit(2);
}

function serve() {
  return new Promise((resolve, reject) => {
    const proc = spawn('python', ['-m', 'http.server', String(PORT)], {
      cwd: webDir,
      stdio: 'ignore',
      windowsHide: true,
    });
    proc.on('error', reject);
    const start = Date.now();
    const tick = () => {
      fetch(`http://127.0.0.1:${PORT}/sink.html`, { method: 'HEAD' }).then(
        (r) => (r.ok ? resolve(proc) : retry()),
        () => retry(),
      );
    };
    const retry = () => {
      if (Date.now() - start > 15000) {
        reject(new Error('raw server never came up'));
      } else {
        setTimeout(tick, 250);
      }
    };
    tick();
  });
}

const server = await serve().catch((e) => {
  console.error('sink: raw server failed: ' + e.message);
  process.exit(2);
});

const verdict = {
  tool: 'sink',
  browser: null,
  ready: false,
  title: false,
  form_roles: false,
  checkbox_svg: false,
  select_pick: null,
  slider_step: null,
  text_typing: null,
  layout_chips: false,
  modal_confirm: null,
  platform_pick: null,
  platform_count: null,
  fetch_mock: null,
  demo_still_boots: false,
  pass: false,
  note: '',
};

let browser = null;
try {
  browser = await puppeteer.launch({
    executablePath: edge,
    headless: true,
    args: ['--hide-scrollbars'],
    defaultViewport: { width: 900, height: 900, deviceScaleFactor: 1 },
  });
  verdict.browser = await browser.userAgent();
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push('pageerror: ' + e.message));
  page.on('console', (m) => {
    if (m.type() === 'error') errors.push('console: ' + m.text());
  });
  const bodyText = () =>
    page.evaluate(() => document.getElementById('oppa-root').textContent ?? '');
  // Clicks the deepest element whose full text is `text`,
  // optionally scoped under a selector (tabs, buttons, and options
  // are divs — text matching is the address, never hand-written
  // coordinates: a relayout must never silently re-target;
  // deepest-match wins so containers never swallow their labels).
  // The click itself goes through trusted `page.mouse` input (real
  // pointer events into the bootstrap — in-page `.click()` fires a
  // coord-less `click` with no pointerdown/up, which the binding
  // never sees). Throws loudly on no-match.
  const clickText = (text, scopeSel = '#oppa-root') =>
    page
      .evaluate(
        (text, scopeSel) => {
          const scope = document.querySelector(scopeSel);
          if (!scope) return { error: 'no-scope' };
          const norm = (s) => s.replace(/\s+/g, ' ').trim();
          let best = null;
          let bestDepth = -1;
          for (const el of scope.querySelectorAll('*')) {
            if (norm(el.textContent) !== text) continue;
            let d = 0;
            let p = el;
            while (p && p !== scope) {
              d += 1;
              p = p.parentElement;
            }
            if (d > bestDepth) {
              bestDepth = d;
              best = el;
            }
          }
          if (!best) return { error: 'no-match' };
          const r = best.getBoundingClientRect();
          return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
        },
        text,
        scopeSel,
      )
      .then(async (pt) => {
        if (pt.error) {
          throw new Error(`click ${JSON.stringify(text)} under ${scopeSel}: ${pt.error}`);
        }
        await page.mouse.click(pt.x, pt.y);
      });

  await page.goto(`http://127.0.0.1:${PORT}/sink.html`);
  await page.waitForFunction(() => window.__oppaReady === true, { timeout: 20000 });
  verdict.ready = true;

  const t0 = await bodyText();
  verdict.title = t0.includes('Oppa Kitchen Sink');
  const roleCount = (role) =>
    page.evaluate((r) => document.querySelectorAll(`#oppa-root [role="${r}"]`).length, role);
  const roles = {
    checkbox: await roleCount('checkbox'),
    switch: await roleCount('switch'),
    slider: await roleCount('slider'),
    combobox: await roleCount('combobox'),
    tab: await roleCount('tab'),
  };
  verdict.form_roles =
    roles.checkbox === 1 && roles.switch === 1 && roles.slider === 1 &&
    roles.combobox === 1 && roles.tab === 4;
  if (!verdict.form_roles) {
    throw new Error('form roles mismatch: ' + JSON.stringify(roles));
  }

  // Checkbox: clicking the label row checks it and the vector mark
  // appears (aria-checked + a second inline svg beside the chevron).
  const svgBefore = await page.evaluate(() =>
    document.querySelectorAll('#oppa-root svg').length);
  await clickText('Accept terms');
  await page.waitForFunction(
    () => document.querySelector('#oppa-root [role="checkbox"]')?.getAttribute('aria-checked') === 'true',
    { timeout: 10000 },
  );
  const svgAfter = await page.evaluate(() =>
    document.querySelectorAll('#oppa-root svg').length);
  verdict.checkbox_svg = svgAfter === svgBefore + 1;

  // Select: open through the combobox, pick Mint.
  await page
    .evaluate(() => {
      const r = document
        .querySelector('#oppa-root [role="combobox"]')
        .getBoundingClientRect();
      return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
    })
    .then((pt) => page.mouse.click(pt.x, pt.y));
  await page.waitForFunction(() => document.body.textContent.includes('Mint'), { timeout: 10000 });
  await clickText('Mint');
  await page.waitForFunction(
    () => document.querySelector('#oppa-root [role="combobox"]')?.getAttribute('aria-label') === 'Mint',
    { timeout: 10000 },
  );
  verdict.select_pick = 'Mint';

  // Slider stepper: + steps 50 -> 55 (the status line renders it).
  await clickText('+');
  await page.waitForFunction(
    () => document.body.textContent.includes('volume: 55'),
    { timeout: 10000 },
  );
  verdict.slider_step = '55';

  // Text typing through the verdict-(b) input (U8 channel).
  // KNOWN GAP (OQ-SINK-1, Round 7.17): this step FAILS — typing
  // into the empty+placeholder Name field is silently dropped (the
  // field leaf only exists once the value is non-empty, so the text
  // event hits the unbound outer and dies quietly; on DOM the same
  // gap renders the placeholder AS the input value). Pinned
  // headlessly by the ignored
  // `text_input_placeholder_typing_feeds_value` control test. The
  // failure is isolated here (recorded, never thrown) so the
  // remaining legs still prove out in the same run.
  try {
    await page.click('#oppa-root input');
    await page.keyboard.type('Ada');
    await page.waitForFunction(() => document.body.textContent.includes('name: Ada'), {
      timeout: 10000,
    });
    verdict.text_typing = 'Ada';
  } catch (e) {
    verdict.text_typing = 'OQ-SINK-1: ' + (e && e.message ? e.message : e);
  }

  // Layout tab: chips + effect cards.
  await clickText('Layout', '#oppa-root [role="tablist"]');
  await page.waitForFunction(() => document.body.textContent.includes('Gamma'), { timeout: 10000 });
  verdict.layout_chips = await bodyText().then((t) => t.includes('soft shadow'));

  // Overlays tab: modal opens, confirms, closes with the callback.
  await clickText('Overlays', '#oppa-root [role="tablist"]');
  await clickText('Open dialog');
  await page.waitForFunction(
    () => document.querySelector('#oppa-root [role="dialog"]') !== null,
    { timeout: 10000 },
  );
  await clickText('OK', '#oppa-root [role="dialog"]');
  await page.waitForFunction(
    () =>
      document.querySelector('#oppa-root [role="dialog"]') === null &&
      document.body.textContent.includes('confirmed: true'),
    { timeout: 10000 },
  );
  verdict.modal_confirm = true;

  // Platform tab: mocked fetch, scripted pick, persistent count.
  await clickText('Platform', '#oppa-root [role="tablist"]');
  await clickText('Fetch quote');
  await page.waitForFunction(
    () => document.body.textContent.includes('DejaVu shapes everywhere'),
    { timeout: 10000 },
  );
  verdict.fetch_mock = true;
  await clickText('Pick a file');
  await page.waitForFunction(
    () => document.body.textContent.includes('picked demo-pick.png'),
    { timeout: 10000 },
  );
  verdict.platform_pick = true;
  await clickText('Count++');
  await page.waitForFunction(() => document.body.textContent.includes('count 1'), { timeout: 10000 });
  verdict.platform_count = true;

  // Demo regression guard: the default page still boots cleanly
  // (the pkg rebuild serves both pages — one bundle, two roots).
  await page.goto(`http://127.0.0.1:${PORT}/index.html`);
  await page.waitForFunction(() => window.__oppaReady === true, { timeout: 20000 });
  verdict.demo_still_boots = true;

  const errorsOk = errors.length === 0;
  verdict.pass =
    verdict.ready && verdict.title && verdict.form_roles && verdict.checkbox_svg &&
    verdict.select_pick === 'Mint' && verdict.slider_step === '55' &&
    verdict.text_typing === 'Ada' && verdict.layout_chips && verdict.modal_confirm &&
    verdict.fetch_mock && verdict.platform_pick && verdict.platform_count &&
    verdict.demo_still_boots && errorsOk;
  verdict.note = errorsOk ? '' : errors.join(' | ');
  if (!verdict.pass && errorsOk) {
    verdict.note = `gating: typing=${verdict.text_typing}`;
  }
} catch (e) {
  verdict.note = 'harness exception: ' + (e && e.message ? e.message : e);
}

fs.mkdirSync(resultsDir, { recursive: true });
fs.writeFileSync(path.join(resultsDir, 'sink.json'), JSON.stringify(verdict, null, 2));
console.log(`sink: ready=${verdict.ready} title=${verdict.title} roles=${verdict.form_roles} check=${verdict.checkbox_svg} pick=${verdict.select_pick} step=${verdict.slider_step} typing=${verdict.text_typing} chips=${verdict.layout_chips} modal=${verdict.modal_confirm} fetch=${verdict.fetch_mock} file=${verdict.platform_pick} count=${verdict.platform_count} demo=${verdict.demo_still_boots} pass=${verdict.pass}${verdict.note ? ' note=' + verdict.note : ''}`);
await browser?.close();
server.kill();
process.exit(verdict.pass ? 0 : 1);
