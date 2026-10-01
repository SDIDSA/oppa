// Web app story (v1 remainder, Gap 6): the minimal shippable path
// run against the M7 harness substrate (raw static server +
// headless Chromium/Edge via puppeteer-core, no bundler).
//
// Serves crates/oppa-web/web over a raw python http.server,
// loads index.html in headless Edge, waits for the wasm bootstrap,
// clicks the toggle twice through the real input pipeline
// (pointer events -> wasm inject -> DOM swap), and asserts the
// ARIA switch flips false -> true -> false. Writes
// spike/results/webapp.json with the verdict.
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
const PORT = 8931;

const EDGE_CANDIDATES = [
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
];
const edge = EDGE_CANDIDATES.find((p) => fs.existsSync(p));
if (!edge) {
  console.error('webapp: no Edge binary found (loud requirement, never a silent pass)');
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
    // Raw server readiness: poll the port instead of sleeping blind.
    const start = Date.now();
    const tick = () => {
      fetch(`http://127.0.0.1:${PORT}/index.html`, { method: 'HEAD' }).then(
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
  console.error('webapp: raw server failed: ' + e.message);
  process.exit(2);
});

const verdict = {
  tool: 'webapp',
  browser: null,
  ready: false,
  initial: null,
  after_click1: null,
  after_click2: null,
  fetch_quote: null,
  nav_push_url_ok: null,
  nav_forward_settings: null,
  storage_persisted_on: false,
  storage_persisted_off: false,
  pass: false,
  note: '',
};

let browser = null;
try {
  browser = await puppeteer.launch({
    executablePath: edge,
    headless: true,
    args: ['--hide-scrollbars'],
    defaultViewport: { width: 900, height: 700, deviceScaleFactor: 1 },
  });
  verdict.browser = await browser.userAgent();
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push('pageerror: ' + e.message));
  page.on('console', (m) => {
    if (m.type() === 'error') errors.push('console: ' + m.text());
  });
  await page.goto(`http://127.0.0.1:${PORT}/index.html`);
  await page.waitForFunction(() => window.__oppaReady === true, { timeout: 15000 });
  verdict.ready = true;

  // Round 4.3 storage: clear origin storage, reload for a
  // deterministic off start (a previous run's persisted toggle
  // must never leak into this one's initial state).
  await page.evaluate(() => window.localStorage.clear());
  await page.reload();
  await page.waitForFunction(() => window.__oppaReady === true, { timeout: 15000 });

  const state = () => page.evaluate(() => window.__oppaState ?? null);
  const roleCount = () =>
    page.evaluate(() => document.querySelectorAll('[role="switch"]').length);

  verdict.initial = await state();
  const roles0 = await roleCount();

  // Click the toggle center (container-relative 22,12) twice.
  const box = await page.evaluate(() => {
    const r = document.getElementById('oppa-root').getBoundingClientRect();
    return { x: r.x, y: r.y };
  });
  await page.mouse.click(box.x + 22, box.y + 12);
  await page.waitForFunction(() => window.__oppaState === 'true', { timeout: 10000 });
  verdict.after_click1 = await state();
  await page.mouse.click(box.x + 22, box.y + 12);
  await page.waitForFunction(() => window.__oppaState === 'false', { timeout: 10000 });
  verdict.after_click2 = await state();
  const roles1 = await roleCount();

  // Round 4.3 storage E2E (before any pushState: reloads must hit
  // the servable page — a reload at a pushed URL 404s on the raw
  // server): flip on, reload, still on (persisted through
  // localStorage); flip off, reload, still off. The waits are the
  // assertions (a lost write renders the wrong state and times
  // out loudly, never passes quiet).
  await page.mouse.click(box.x + 22, box.y + 12);
  await page.waitForFunction(() => window.__oppaState === 'true', { timeout: 10000 });
  await page.reload();
  await page.waitForFunction(() => window.__oppaReady === true, { timeout: 15000 });
  await page.waitForFunction(() => window.__oppaState === 'true', { timeout: 10000 });
  verdict.storage_persisted_on = true;
  await page.mouse.click(box.x + 22, box.y + 12);
  await page.waitForFunction(() => window.__oppaState === 'false', { timeout: 10000 });
  await page.reload();
  await page.waitForFunction(() => window.__oppaReady === true, { timeout: 15000 });
  await page.waitForFunction(() => window.__oppaState === 'false', { timeout: 10000 });
  verdict.storage_persisted_off = true;

  // Round 4.1 fetch E2E: the Fetch button starts a same-origin
  // fetch() around the wasm start/resolve bridge; the quote div
  // renders the resolved body (stale generations would paint
  // nothing — the wait would time out loudly, never pass quiet).
  await page.click('#oppa-fetch');
  await page.waitForFunction(
    () => document.body.textContent.includes('wasm paints'),
    { timeout: 10000 },
  );
  verdict.fetch_quote = await page.evaluate(() =>
    document.getElementById('oppa-root').textContent.includes('wasm paints')
  );

  // Round 4.2 history E2E: Settings pushes (URL + route render),
  // browser back pops (popstate replace), forward re-lands.
  const routeText = () =>
    page.evaluate(() => document.getElementById('oppa-root').textContent ?? '');
  await page.click('#oppa-settings');
  await page.waitForFunction(
    () => document.getElementById('oppa-root').textContent.includes('route:settings'),
    { timeout: 10000 },
  );
  const urlAfterPush = await page.url();
  await page.goBack();
  await page.waitForFunction(
    () => document.getElementById('oppa-root').textContent.includes('route:home'),
    { timeout: 10000 },
  );
  await page.goForward();
  await page.waitForFunction(
    () => document.getElementById('oppa-root').textContent.includes('route:settings'),
    { timeout: 10000 },
  );
  verdict.nav_push_url_ok = urlAfterPush.endsWith('/settings');
  verdict.nav_forward_settings = (await routeText()).includes('route:settings');

  // Round 4.4 images: the scene `<img>` decodes in-browser
  // (naturalWidth proves decode, not just markup — a broken src
  // completes with width 0 and fails loudly here, never quiet).
  verdict.img_decoded = await page.evaluate(() => {
    const img = document.querySelector('#oppa-root img');
    return !!img && img.complete && img.naturalWidth === 16;
  });

  const errorsOk = errors.length === 0;
  verdict.pass =
    verdict.initial === 'false' &&
    roles0 === 1 &&
    verdict.after_click1 === 'true' &&
    verdict.after_click2 === 'false' &&
    roles1 === 1 &&
    verdict.fetch_quote === true &&
    verdict.nav_push_url_ok === true &&
    verdict.nav_forward_settings === true &&
    verdict.img_decoded === true &&
    verdict.storage_persisted_on === true &&
    verdict.storage_persisted_off === true &&
    errorsOk;
  verdict.note = errorsOk ? '' : errors.join(' | ');
  if (!verdict.pass && errorsOk) {
    verdict.note = `states=${verdict.initial}/${verdict.after_click1}/${verdict.after_click2} roles=${roles0}/${roles1} fetch=${verdict.fetch_quote} nav=${verdict.nav_push_url_ok}/${verdict.nav_forward_settings} storage=${verdict.storage_persisted_on}/${verdict.storage_persisted_off} img=${verdict.img_decoded}`;
  }
} catch (e) {
  verdict.note = 'harness exception: ' + (e && e.message ? e.message : e);
}

fs.mkdirSync(resultsDir, { recursive: true });
fs.writeFileSync(path.join(resultsDir, 'webapp.json'), JSON.stringify(verdict, null, 2));
console.log(
  `webapp: ready=${verdict.ready} states=${verdict.initial}/${verdict.after_click1}/${verdict.after_click2} fetch=${verdict.fetch_quote} nav=${verdict.nav_push_url_ok}/${verdict.nav_forward_settings} storage=${verdict.storage_persisted_on}/${verdict.storage_persisted_off} img=${verdict.img_decoded} pass=${verdict.pass}${verdict.note ? ' note=' + verdict.note : ''}`,
);
await browser?.close();
server.kill();
process.exit(verdict.pass ? 0 : 1);
