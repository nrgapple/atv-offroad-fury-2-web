// Rust engine: the Remastered graphics preset (authored, above High) builds
// its shaders and finish pass on WebGL2 and changes the picture, and High
// stays without it.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4177 node tools/test_engine_remastered.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const report = {};
const errors = [];
const browser = await launch();
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
try {
  const shots = {};
  for (const graphics of ['high', 'remastered']) {
    const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
    await page.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
    await page.goto(url(`/play.html?venue=env05&weather=off&graphics=${graphics}&uiDiagnostics=1`));
    await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').active === true, null, { timeout: 300000 });
    await page.waitForTimeout(1500); // Time-based on purpose: a few drawn frames.
    const s = (await state(page)).settings;
    report[graphics] = { remaster: s.remaster, worldShadows: s.worldShadows, shadowMap: s.shadowMap, anisotropy: s.anisotropy };
    shots[graphics] = await page.screenshot(process.env.ATV_REMASTER_SCREENSHOT && graphics === 'remastered'
      ? { path: process.env.ATV_REMASTER_SCREENSHOT } : {});
    await page.close();
  }
  assert.equal(report.high.remaster, false, JSON.stringify(report));
  assert.equal(report.remastered.remaster, true, JSON.stringify(report));
  // The world shadow map: 2048 in the browser, none below Remastered.
  assert.deepEqual([report.high.worldShadows, report.remastered.worldShadows], [0, 2048], JSON.stringify(report));
  // Remastered keeps everything High has.
  assert.deepEqual([report.remastered.shadowMap, report.remastered.anisotropy], [report.high.shadowMap, report.high.anisotropy]);
  assert.ok(!shots.high.equals(shots.remastered), 'the Remastered picture differs from High');
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
