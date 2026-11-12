// Rust engine: the menu pages and the ride HUD lay out on a folding phone,
// open (a near-square screen with the status column beside the stage, both
// ways up) and closed (a narrow phone, both ways up), with the insets
// injected through the page's safe-area probe. Single Race's geometry (the
// name and mode clear of Start Race) is checked by atv-ui's
// `single_race_names_stay_clear_of_start_race`; this draws the same cases in
// the browser for inspection.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4177 node tools/test_engine_fold_layout.mjs
// ATV_FOLD_SCREENSHOTS=DIR saves one screenshot per case.
import assert from 'node:assert/strict';
import path from 'node:path';
import { assertNoErrors, launch, newPage, unlockStore, url } from './browser/harness.mjs';

const cases = [
  { name: 'open-landscape', width: 1000, height: 704, dpr: 2, safe: [0, 88, 0, 0] },
  { name: 'open-portrait', width: 704, height: 1000, dpr: 2, safe: [47, 0, 34, 0] },
  { name: 'closed-portrait', width: 360, height: 780, dpr: 3, safe: [47, 0, 34, 0] },
  { name: 'closed-landscape', width: 780, height: 360, dpr: 3, safe: [0, 47, 21, 47] }
];
const pages = ['main', 'single', 'race', 'tracks', 'garage'];
const report = {};
const errors = [];
const browser = await launch();
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
try {
  for (const c of cases) {
    const page = await newPage(browser, { viewport: { width: c.width, height: c.height }, deviceScaleFactor: c.dpr }, errors);
    await unlockStore(page);
    await page.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
    // The engine reads env(safe-area-inset-*) from this element's padding;
    // the CSSOM write passes the page's style CSP.
    await page.addInitScript(safe => {
      document.addEventListener('DOMContentLoaded', () => {
        const probe = document.createElement('div');
        probe.id = 'safe-area-probe';
        probe.style.setProperty('padding', safe.map(v => `${v}px`).join(' '), 'important');
        document.body.append(probe);
      });
    }, c.safe);
    const shot = async name => {
      if (process.env.ATV_FOLD_SCREENSHOTS)
        await page.screenshot({ path: path.join(process.env.ATV_FOLD_SCREENSHOTS, `${name}-${c.name}.png`) });
    };
    report[c.name] = {};
    for (const front of pages) {
      await page.goto(url(`/play.html?uiDiagnostics=1&front=${front}`));
      await page.waitForFunction(p => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === p, front, { timeout: 120000 });
      await page.waitForTimeout(800); // Time-based on purpose: photos and the page slide settle.
      const f = (await state(page)).front;
      report[c.name][front] = f.choices.length;
      if (front === 'race') assert.ok(f.choices.includes('racestart'), `${c.name}: ${JSON.stringify(f.choices)}`);
      await shot(front);
    }
    // The ride HUD at the same size.
    await page.goto(url('/play.html?venue=env05&weather=off&uiDiagnostics=1'));
    await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').active === true, null, { timeout: 300000 });
    await page.waitForTimeout(1200); // Time-based on purpose: the HUD and first frames.
    await shot('ride');
    await page.close();
  }
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
