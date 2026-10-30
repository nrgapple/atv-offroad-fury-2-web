// The silver stage's white rim (research/MENU_SCREENS.md) is one continuous line.
// Portrait: the panel's lower edge sweeps from the left with an elliptical corner
// wider than half the screen (`border-radius: 75% / 56px`); the UI shader used
// to pick corners by quadrant, so the curve stopped at the centre and the
// straight edge started a step lower. Landscape: the skewed panel's left rim.
// For each column (portrait) or row (landscape) this finds the rim pixel next to
// the red backdrop and checks it exists everywhere and never jumps.
// Chromium and WebKit, DPR 1 and 3. Run: node tools/run_browser_tests.mjs --only test_engine_menu_divider.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, unlockStore, url } from './browser/harness.mjs';

const onPage = (page, name) =>
  page.waitForFunction(
    name => {
      const front = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front;
      return front?.page === name && !front.motion;
    },
    name,
    { timeout: 120000 }
  );

/** Rim positions (CSS px) along the edge, measured in an about:blank page. */
async function rim(browser, png, { width, height, dpr, portrait }) {
  const page = await browser.newPage();
  try {
    return await page.evaluate(
      async ({ data, width, height, dpr, portrait }) => {
        const image = new Image();
        image.src = `data:image/png;base64,${data}`;
        await image.decode();
        const canvas = document.createElement('canvas');
        canvas.width = image.naturalWidth;
        canvas.height = image.naturalHeight;
        const g = canvas.getContext('2d', { willReadFrequently: true });
        g.drawImage(image, 0, 0);
        const px = g.getImageData(0, 0, canvas.width, canvas.height).data;
        const at = (x, y) => {
          const i = (Math.round(y * dpr) * canvas.width + Math.round(x * dpr)) * 4;
          return [px[i], px[i + 1], px[i + 2]];
        };
        const white = ([r, g, b]) => r > 200 && g > 200 && b > 200;
        const red = ([r, g, b]) => r < 170 && g < 90 && b < 90;
        const out = [];
        const step = 1 / dpr;
        if (portrait) {
          // Lowest white pixel with the red backdrop 3px below, per column.
          for (let x = 6; x <= width - 6; x += 2) {
            let found = null;
            for (let y = height * 0.65; y >= height * 0.2; y -= step) {
              if (white(at(x, y)) && red(at(x, y + 3))) {
                found = y;
                break;
              }
            }
            out.push([x, found]);
          }
        } else {
          // Leftmost white pixel with the dark backdrop 3px to its left, per row
          // (below the top-left curve, past the rows column).
          for (let y = 100; y <= height - 30; y += 2) {
            let found = null;
            for (let x = width * 0.45; x <= width * 0.85; x += step) {
              if (white(at(x, y)) && red(at(x - 3, y))) {
                found = x;
                break;
              }
            }
            out.push([y, found]);
          }
        }
        return out;
      },
      { data: png.toString('base64'), width, height, dpr, portrait }
    );
  } finally {
    await page.close();
  }
}

const cases = [
  { width: 390, height: 844, dpr: 3, portrait: true },
  { width: 390, height: 844, dpr: 1, portrait: true },
  { width: 430, height: 932, dpr: 3, portrait: true },
  { width: 844, height: 390, dpr: 3, portrait: false },
  { width: 1280, height: 800, dpr: 1, portrait: false }
];
const errors = [];
for (const engine of ['chromium', 'webkit']) {
  const browser = await launch({ engine });
  try {
    for (const c of cases) {
      const page = await newPage(
        browser,
        { viewport: { width: c.width, height: c.height }, deviceScaleFactor: c.dpr },
        errors
      );
      await unlockStore(page);
      await page.goto(url('/play.html?uiDiagnostics=1&front=single'));
      await onPage(page, 'single');
      const points = await rim(browser, await page.screenshot(), c);
      const label = `${engine} ${c.width}x${c.height}@${c.dpr}`;
      const missing = points.filter(([, v]) => v === null).map(([k]) => k);
      assert.deepEqual(missing, [], `${label}: rim missing at ${missing.join(',')}`);
      let worst = 0;
      for (let i = 1; i < points.length; i++) worst = Math.max(worst, Math.abs(points[i][1] - points[i - 1][1]));
      // A 2px step along the curve moves the rim at most ~0.7px (steepest part);
      // the broken edge jumped over 3px at the centre.
      assert.ok(worst <= 1.5, `${label}: rim jumps ${worst.toFixed(2)}px ${JSON.stringify(points)}`);
      if (c.portrait) {
        // The sweep descends left to right, then runs flat: never back up.
        for (let i = 1; i < points.length; i++)
          assert.ok(points[i][1] >= points[i - 1][1] - 0.75, `${label}: rim rises at x=${points[i][0]}`);
      }
      console.log(`${label}: rim continuous, largest step ${worst.toFixed(2)}px`);
      await page.close();
    }
  } finally {
    await browser.close();
  }
}
assertNoErrors(errors);
console.log('PASS menu stage rim is continuous (portrait sweep, landscape skew; Chromium and WebKit, DPR 1 and 3)');
