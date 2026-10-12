// Web resize proof (decision 470): the scene follows its container.
// Serves `apps/sera-login/web` (fluid `#oppa-root` shell) over a raw
// static server (port 8934, kept clear of the login/webapp/sink
// rigs), boots (Edge default, Firefox over BiDi with
// `OPPA_BROWSER=firefox` — the driver.mjs recipe), then resizes the
// viewport twice and asserts the email field tracks the re-centering
// card — behavior proves the observer → binding → refit → patch →
// applier chain end to end, with zero console errors. The headless
// contract (validation, park, quiet, refit-shape) rides
// `crates/oppa-web/tests/resize_web.rs`.
// Exit codes: 0 pass, 1 gated fail, 2 environment.
import puppeteer from 'puppeteer-core';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { isFirefox, launchFirefox } from './driver.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const webDir = path.resolve(here, '..', '..', 'apps', 'sera-login', 'web');
const shotDir = path.resolve(here, '..', '..', 'target');
const PORT = 8934;

const EDGE_CANDIDATES = [
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
];
const edge = EDGE_CANDIDATES.find((p) => fs.existsSync(p));
if (!edge && !isFirefox) {
  console.error('resize-web: no Edge binary found');
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
  console.error('resize-web: raw server failed: ' + e.message);
  process.exit(2);
});

let browser = null;
let ffCleanup = null;
let pass = false;
const fail = (msg) => {
  throw new Error(msg);
};
try {
  if (isFirefox) {
    ({ browser, cleanup: ffCleanup } = await launchFirefox());
  } else {
    browser = await puppeteer.launch({
      executablePath: edge,
      headless: true,
      args: ['--hide-scrollbars'],
      defaultViewport: { width: 1280, height: 800, deviceScaleFactor: 1 },
    });
  }
  const page = await browser.newPage();
  if (isFirefox) {
    await page.setViewport({ width: 1280, height: 800, deviceScaleFactor: 1 });
  }
  const errors = [];
  page.on('pageerror', (e) => errors.push('pageerror: ' + (e.stack || e.message)));
  page.on('console', (m) => {
    if (m.type() === 'error') errors.push('console: ' + m.text());
  });
  await page.goto(`http://127.0.0.1:${PORT}/index.html`);
  await page.waitForFunction(() => window.__oppaReady === true, { timeout: 25000 });
  await new Promise((r) => setTimeout(r, 1500));

  const fieldCenter = () =>
    page.evaluate(() => {
      const inputs = [...document.querySelectorAll('#oppa-root input[data-pid]')];
      const el = inputs[0];
      if (!el) return null;
      const r = el.getBoundingClientRect();
      return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
    });
  const c0 = await fieldCenter();
  if (!c0) fail('email field not found at 1280x800');
  console.log(`resize-web: boot field center (${c0.x.toFixed(1)}, ${c0.y.toFixed(1)})`);
  // The 921px card centers in 1280: email input center lands right
  // of viewport center by construction (~880); a fixed 800x600
  // viewport would pin it near ~620 instead. Wide tolerance —
  // direction and order of magnitude are the assertion here.
  if (c0.x < 750 || c0.x > 1000) fail(`boot field x off-card: ${c0.x}`);
  await page.screenshot({ path: path.join(shotDir, 'sera-resize-1280.png') });

  // Widen + grow: the card re-centers, so the field tracks right
  // (+160 for +320 width) and down (+50 for +100 height).
  await page.setViewport({ width: 1600, height: 900, deviceScaleFactor: 1 });
  await new Promise((r) => setTimeout(r, 1500));
  const c1 = await fieldCenter();
  if (!c1) fail('email field lost after widen');
  const dx = c1.x - c0.x;
  const dy = c1.y - c0.y;
  console.log(`resize-web: moved field center (${c1.x.toFixed(1)}, ${c1.y.toFixed(1)}) dx=${dx.toFixed(1)} dy=${dy.toFixed(1)}`);
  if (dx < 100 || dx > 220) fail(`field did not track horizontal re-center: dx=${dx}`);
  if (dy < 20 || dy > 80) fail(`field did not track vertical re-center: dy=${dy}`);
  await page.screenshot({ path: path.join(shotDir, 'sera-resize-1600.png') });

  // Shrink below boot: layout follows down too (no stuck viewport).
  // 1600->1000 is -600 wide, so the centered field tracks -300.
  await page.setViewport({ width: 1000, height: 700, deviceScaleFactor: 1 });
  await new Promise((r) => setTimeout(r, 1500));
  const c2 = await fieldCenter();
  if (!c2) fail('email field lost after shrink');
  const dx2 = c2.x - c1.x;
  console.log(`resize-web: shrunk field center (${c2.x.toFixed(1)}, ${c2.y.toFixed(1)}) dx=${dx2.toFixed(1)}`);
  if (dx2 > -240 || dx2 < -360) fail(`field did not track shrink: dx=${dx2}`);
  if (errors.length) fail('console errors: ' + errors.join(' | '));
  console.log('resize-web: PASS, zero console errors');
  pass = true;
} catch (e) {
  console.error('resize-web FAIL: ' + (e && e.message ? e.message : e));
} finally {
  await browser?.close();
  await ffCleanup?.();
  server.kill();
}
process.exit(pass ? 0 : 1);
