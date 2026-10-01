import puppeteer from 'puppeteer-core';

const browser = await puppeteer.launch({
  executablePath: 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
  headless: true,
  args: ['--font-render-hinting=none'],
  defaultViewport: { width: 500, height: 120, deviceScaleFactor: 1 },
});
const page = await browser.newPage();
await page.goto('file:///C:/Users/zinou/Desktop/oppa/spike/web/index.html');
const r = await page.evaluate(() => {
  const probe = document.getElementById('probe');
  const measure = (t) => {
    probe.textContent = t;
    const range = document.createRange();
    range.selectNodeContents(probe.firstChild);
    const rect = range.getBoundingClientRect();
    return { w: rect.width, left: rect.left };
  };
  const offsets = (t) => {
    probe.textContent = t;
    const out = [];
    for (let x = 0; x <= 40; x += 0.5) {
      const rg = document.caretRangeFromPoint(x, 76);
      out.push(rg ? rg.startOffset : null);
    }
    return out;
  };
  return {
    segCheck: document.fonts.check('16px "Segoe UI"'),
    yaheiCheck: document.fonts.check('16px "Microsoft YaHei"'),
    computedFont: getComputedStyle(probe).fontFamily,
    hello: measure('héllo'),
    hi: measure('Hi'),
    offsets_Hi: offsets('Hi'),
    offsets_hello: offsets('héllo'),
  };
});
console.log(JSON.stringify(r, null, 1));
await browser.close();
