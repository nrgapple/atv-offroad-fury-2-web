// Rust engine: the ride HUD and the Pause menu draw at the display's full
// density on high-density phones while the 3D scene draws at the adaptive
// render ratio (1.5) inside it and is stretched under the UI (#457).
// Checks the surface and scene sizes at a DPR 3 phone both ways up and a
// folding phone open (DPR 2).
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4177 node tools/test_engine_crisp_hud.mjs
// ATV_CRISP_SCREENSHOTS=DIR saves the ride and Pause screens per case.
import assert from 'node:assert/strict';
import path from 'node:path';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const cases = [
  { name: 'phone-portrait', width: 390, height: 844, dpr: 3 },
  { name: 'phone-landscape', width: 844, height: 390, dpr: 3 },
  { name: 'fold-open', width: 1000, height: 704, dpr: 2 }
];
const report = {};
const errors = [];
const browser = await launch();
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
try {
  for (const c of cases) {
    const page = await newPage(
      browser,
      { viewport: { width: c.width, height: c.height }, deviceScaleFactor: c.dpr, hasTouch: true, isMobile: true },
      errors
    );
    await page.addInitScript(() => {
      Object.defineProperty(document, 'hasFocus', { value: () => true });
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    });
    const shot = async name => {
      if (process.env.ATV_CRISP_SCREENSHOTS)
        await page.screenshot({ path: path.join(process.env.ATV_CRISP_SCREENSHOTS, `${name}-${c.name}.png`) });
    };
    await page.goto(url('/play.html?map=env01&play=1&weather=off&uiDiagnostics=1'));
    await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').active === true, null, {
      timeout: 300000
    });
    await page.waitForTimeout(1500); // Time-based on purpose: the HUD and first frames settle.
    const ride = await state(page);
    const canvas = await page.evaluate(() => {
      const canvas = document.querySelector('#game');
      return [canvas.width, canvas.height];
    });
    // The surface (and so the HUD) at the display's density...
    assert.deepEqual(ride.surface, [c.width * c.dpr, c.height * c.dpr], `${c.name} surface`);
    assert.deepEqual(canvas, ride.surface, `${c.name} canvas`);
    // ...and the scene at the 1.5 render ratio (the host is fast enough not
    // to step it down within the first seconds).
    const scene = ride.sceneSize.map((n, i) => n / [c.width, c.height][i]);
    assert.ok(scene.every(r => Math.abs(r - 1.5) < 0.01), `${c.name} scene ratio ${scene}`);
    await shot('ride');
    await page.keyboard.press('Escape');
    await page.waitForFunction(
      () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === 'pause',
      null,
      { timeout: 10000 }
    );
    await page.waitForTimeout(600); // Time-based on purpose: the Pause menu's slide.
    const paused = await state(page);
    assert.deepEqual(paused.surface, ride.surface, `${c.name} paused surface`);
    await shot('pause');
    report[c.name] = { surface: ride.surface, scene: ride.sceneSize };
    await page.close();
  }
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
