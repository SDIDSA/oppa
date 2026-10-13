// Login web proof: the portable login page (`WebApp.new_login()` over
// `web/login.html`) driven through the real browser pipeline — boot,
// empty-submit validation, typed valid login, welcome verdict, zero
// console errors. Spawns its own raw static server (port 8933, kept
// clear of the webapp/sink rigs) and kills it at the end, like
// webapp.mjs.
// Exit codes: 0 pass, 1 gated fail, 2 environment.
import puppeteer from 'puppeteer-core';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

const here = path.dirname(fileURLToPath(import.meta.url));
const webDir = path.resolve(here, '..', '..', 'crates', 'oppa-web', 'web');
const PORT = 8933;
const shot = path.resolve(here, '..', '..', 'target', 'login-web.png');

const EDGE_CANDIDATES = [
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
];
const edge = EDGE_CANDIDATES.find((p) => fs.existsSync(p));
if (!edge) {
  console.error('login-web: no Edge binary found');
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
      fetch(`http://127.0.0.1:${PORT}/login.html`, { method: 'HEAD' }).then(
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
  console.error('login-web: raw server failed: ' + e.message);
  process.exit(2);
});

let browser = null;
let pass = false;
const fail = (msg) => {
  throw new Error(msg);
};
try {
  browser = await puppeteer.launch({
    executablePath: edge,
    headless: true,
    args: ['--hide-scrollbars'],
    defaultViewport: { width: 900, height: 700, deviceScaleFactor: 1 },
  });
  const page = await browser.newPage();
  // Trusted text path on Edge (dom_text.mjs): CDP insertText lands
  // directly in the focused field; key-by-key keyboard.type races
  // the framework's synchronous keydown commits.
  const client = await page.createCDPSession();
  const insertText = (text) => client.send('Input.insertText', { text });
  const errors = [];
  page.on('pageerror', (e) => errors.push('pageerror: ' + e.message));
  page.on('console', (m) => {
    if (m.type() === 'error') errors.push('console: ' + m.text());
  });
  await page.goto(`http://127.0.0.1:${PORT}/login.html`);
  await page.waitForFunction(() => window.__oppaReady === true, { timeout: 15000 });

  const text = () =>
    page.evaluate(() => document.getElementById('oppa-root').textContent ?? '');
  const t0 = await text();
  if (!t0.includes('Sign in')) fail('no Sign in title, got: ' + JSON.stringify(t0.slice(0, 200)));
  console.log('login-web: boot ok, form renders');
  // Boot chrome: in-theme from the first frame (no first-click
  // heal needed) — settled pages never interact.
  const bootBg = await page.evaluate(() => document.body.style.background);
  if (!bootBg) fail('boot chrome missing (body background empty before any click)');
  console.log('login-web: boot chrome (' + bootBg + ')');

  // Click the Sign in button through the real pointer pipeline.
  const btn = await page.evaluate(() => {
    const els = [...document.querySelectorAll('#oppa-root [role="button"]')];
    const el = els.find((e) => (e.textContent || '').includes('Sign in'));
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  });
  if (!btn) fail('Sign in button not found');
  await page.mouse.click(btn.x, btn.y);
  await page.waitForFunction(
    () => document.getElementById('oppa-root').textContent.includes('Fix the highlighted fields'),
    { timeout: 10000 },
  );
  console.log('login-web: empty submit validates');
  // Page chrome heals on first interaction (the boot stanza is
  // consumed by the priming sync, so the first patch re-emits it —
  // no more light shell under dark controls).
  const bodyBg = await page.evaluate(() => document.body.style.background);
  if (!bodyBg) fail('page chrome never healed (body background empty)');
  console.log('login-web: chrome heals (' + bodyBg + ')');

  // Type into the email + password fields through the real input
  // channel (fresh geometry per field — commits re-render around
  // the focused field, so handles go stale; coordinates do not).
  const fieldCenter = (i) =>
    page.evaluate((n) => {
      const inputs = [...document.querySelectorAll('#oppa-root input[data-pid]')];
      const el = inputs[n];
      if (!el) return null;
      const r = el.getBoundingClientRect();
      return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
    }, i);
  let c0 = await fieldCenter(0);
  if (!c0) fail('email field not found');
  await page.mouse.click(c0.x, c0.y);
  await insertText('you@example.com');
  let c1 = await fieldCenter(1);
  if (!c1) fail('password field not found');
  await page.mouse.click(c1.x, c1.y);
  await insertText('long-enough');
  await page.waitForFunction(
    () => document.getElementById('oppa-root').textContent.includes('Welcome, you@example.com!') ||
      document.querySelectorAll('#oppa-root input[data-pid]')[0].value === 'you@example.com',
    { timeout: 10000 },
  );
  // Press Sign in again for the welcome verdict.
  await page.mouse.click(btn.x, btn.y);
  await page.waitForFunction(
    () => document.getElementById('oppa-root').textContent.includes('Welcome, you@example.com!'),
    { timeout: 10000 },
  );
  console.log('login-web: valid submit welcomes');

  await page.screenshot({ path: shot });
  console.log('login-web: screenshot ' + shot);
  if (errors.length) fail('console errors: ' + errors.join(' | '));
  console.log('login-web: PASS, zero console errors');
  pass = true;
} catch (e) {
  console.error('login-web FAIL: ' + (e && e.message ? e.message : e));
} finally {
  await browser?.close();
  server.kill();
}
process.exit(pass ? 0 : 1);
