// Presentation interpolation in the Rust engine's browser build at a
// simulated 120 Hz display: requestAnimationFrame and performance.now run on
// a virtual clock, an autodrive script plays through the live frame loop, and
// every presented pose (`?presentTrace=1`) must match the JavaScript game's
// presenters fed the same delivered states (tools/presentation_replay.mjs).
// An Enhanced ATV and a motorcycle interpolate; an Original ATV snaps.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';
import { replay } from './presentation_replay.mjs';

const script = [{ frames: 30, input: {} }, { frames: 240, input: { throttle: 1, steer: 0.4 } }];
const browser = await launch();
try {
  for (const [quad, graphics, expected] of [[1, 'enhanced', true], [29, 'original', true], [1, 'original', false]]) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
    await page.addInitScript(() => {
      // A 120 Hz display on a virtual clock (frames run back to back).
      let now = 0;
      performance.now = () => now;
      window.requestAnimationFrame = callback => setTimeout(() => { now += 1000 / 120; callback(now); }, 0);
    });
    await page.goto(url(`/play.html?venue=env01&quad=${quad}&graphics=${graphics}&weather=off&presentTrace=1&autodrive=${encodeURIComponent(JSON.stringify(script))}`));
    await page.mouse.click(320, 180);
    await page.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 240000 });
    const log = JSON.parse(await page.locator('#trace').textContent());
    const report = replay(log);
    assert.equal(report.mismatched, 0, JSON.stringify(report));
    for (const [key, value] of Object.entries(report.worst)) assert.ok(value < 1e-3, `${key} ${value}`);
    if (expected) {
      assert.ok(report.interpolated > report.samples * 0.9, JSON.stringify(report));
      assert.deepEqual([...new Set(report.alphaPattern)].sort(), [0, 0.5], JSON.stringify(report.alphaPattern));
    } else assert.equal(report.interpolated, 0);
    assertNoErrors(errors);
    console.log(`  quad ${quad} ${graphics}: ${report.interpolated}/${report.samples} frames interpolated, worst ${report.worst.origin.toExponential(1)} ft`);
    await page.close();
  }
  console.log('PASS: Rust engine 120 Hz presentation matches the JavaScript presenters');
} finally {
  await browser.close();
}
