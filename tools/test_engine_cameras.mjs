// Chase camera: Assisted and the Crash camera choice in the Rust engine's
// browser build (UphillSightline, crashPresentation in render-support.js).
// An autodriven ENV05 climb with the saved Assisted choice orbits the follow
// camera (0 < orbit <= 0.52 rad); Original never does; the saved crash
// camera choice reaches the ride. Host browser evidence; the presentation
// maths are checked against web/render-support.js in atv-game's unit tests.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const script = [{ frames: 30, input: {} }, { frames: 900, input: { throttle: 1, steer: 0.3 } }];
const browser = await launch();
try {
  for (const [chase, crash] of [['assisted', 'original'], ['original', 'tight']]) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
    await page.addInitScript(([chase, crash]) => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      localStorage.setItem('atv-chase-camera', chase);
      localStorage.setItem('atv-crash-camera', crash);
      window.__orbit = { frames: 0, lifted: 0, max: 0 };
      const sample = () => {
        const s = JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.settings;
        if (s) { const o = window.__orbit; o.frames++; o.assist = s.chaseAssist; o.tight = s.crashCameraTight; if (s.hillOrbit > 1e-4) { o.lifted++; o.max = Math.max(o.max, s.hillOrbit); } }
        requestAnimationFrame(sample);
      };
      requestAnimationFrame(sample);
    }, [chase, crash]);
    await page.goto(url(`/play.html?venue=env05&weather=off&uiDiagnostics=1&autodrive=${encodeURIComponent(JSON.stringify(script))}`));
    await page.mouse.click(320, 180);
    await page.waitForFunction(() => window.__orbit.frames > 900, null, { timeout: 240000 });
    const orbit = await page.evaluate(() => window.__orbit);
    assert.equal(orbit.assist, chase === 'assisted');
    assert.equal(orbit.tight, crash === 'tight');
    if (chase === 'assisted') {
      assert.ok(orbit.lifted > 0, JSON.stringify(orbit));
      assert.ok(orbit.max <= 0.52, JSON.stringify(orbit));
    } else assert.equal(orbit.lifted, 0, JSON.stringify(orbit));
    assertNoErrors(errors);
    console.log(`  chase ${chase}, crash ${crash}: ${orbit.lifted}/${orbit.frames} frames orbited, max ${orbit.max.toFixed(3)} rad`);
    await page.close();
  }
  console.log('PASS: Rust chase and crash camera choices');
} finally {
  await browser.close();
}
