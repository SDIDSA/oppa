// Shared browser driver (decision 418): Edge over CDP (the unchanged
// default path) or Firefox over native WebDriver BiDi.
//
// Firefox recipe (proven live against Firefox 157 + puppeteer-core 23):
// `puppeteer.launch({product: 'firefox'})` does NOT take here — spawn
// headless with `--remote-debugging-port` and connect to
// `ws://127.0.0.1:PORT/session` with `protocol: 'webDriverBiDi'`
// (the `/session` suffix is required; the launcher appends it
// automatically). Callers branch on `BROWSER`: the Edge blocks stay
// byte-identical, the Firefox arm calls `launchFirefox`.
import puppeteer from 'puppeteer-core';
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

export const BROWSER = process.env.OPPA_BROWSER ?? 'edge';
export const isFirefox = BROWSER === 'firefox';

const FIREFOX_EXE = 'C:\\Program Files\\Mozilla Firefox\\firefox.exe';

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

/// Spawn headless Firefox with a BiDi port and attach puppeteer-core
/// over the BiDi WebSocket. Returns `{ browser, cleanup }`; the caller
/// owns `browser.close()` + `cleanup()` (kills the spawned process).
export async function launchFirefox() {
  if (!fs.existsSync(FIREFOX_EXE)) {
    throw new Error(`firefox not found at ${FIREFOX_EXE}`);
  }
  const profile = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'oppa-ff-'));
  const port = 9226 + Math.floor(Math.random() * 1000);
  const child = spawn(
    FIREFOX_EXE,
    [
      '--headless',
      '--no-remote',
      '--profile',
      profile,
      `--remote-debugging-port=${port}`,
      'about:blank',
    ],
    { stdio: ['ignore', 'ignore', 'ignore'] }
  );
  const endpoint = `ws://127.0.0.1:${port}/session`;
  let browser = null;
  let lastErr = null;
  for (let i = 0; i < 100; i++) {
    await sleep(300);
    try {
      browser = await puppeteer.connect({
        browserWSEndpoint: endpoint,
        protocol: 'webDriverBiDi',
      });
      break;
    } catch (e) {
      lastErr = e;
      if (child.exitCode !== null) {
        throw new Error(`firefox exited early: ${lastErr}`);
      }
    }
  }
  if (!browser) {
    child.kill();
    throw new Error(`firefox BiDi connect failed: ${lastErr}`);
  }
  return {
    browser,
    cleanup: async () => {
      try {
        await browser.close();
      } catch {
        // Already gone — fall through to kill.
      }
      child.kill();
      // The browser may still hold profile files for a beat after
      // close (sqlite-shm EBUSY on Windows) — retry, then let the
      // OS tmp cleaner have the leftovers rather than failing the run.
      for (let i = 0; i < 10; i++) {
        try {
          await fs.promises.rm(profile, { recursive: true, force: true });
          return;
        } catch {
          await sleep(500);
        }
      }
      console.error(`driver: profile left behind at ${profile} (browser still releasing files)`);
    },
  };
}
