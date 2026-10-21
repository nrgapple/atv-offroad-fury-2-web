// Enhanced/High wheel re-projection in the Rust engine's browser build
// (graphics.js wheelReprojection, game.js reprojectWheels): an autodriven
// Enhanced ATV redraws wheels sunk into the visible render-grid terrain,
// each lift clamped to MAX_WHEEL_LIFT; Original never reprojects. Host
// browser evidence (#ui-state settings.wheelLift sampled every frame).
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const script = [{ frames: 30, input: {} }, { frames: 600, input: { throttle: 1, steer: 0.35 } }];
const browser = await launch();
try {
  for (const graphics of ['enhanced', 'original']) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
    await page.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      window.__lift = { frames: 0, lifted: 0, max: 0, enabled: null };
      const sample = () => {
        const settings = JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.settings;
        if (settings) {
          const s = window.__lift; s.frames++; s.enabled = settings.wheelReprojection;
          if (settings.wheelLift > 0) { s.lifted++; s.max = Math.max(s.max, settings.wheelLift); }
        }
        requestAnimationFrame(sample);
      };
      requestAnimationFrame(sample);
    });
    await page.goto(url(`/engine.html?venue=env01&graphics=${graphics}&weather=off&uiDiagnostics=1&autodrive=${encodeURIComponent(JSON.stringify(script))}`));
    await page.mouse.click(320, 180);
    await page.waitForFunction(() => window.__lift.frames > 900, null, { timeout: 240000 });
    const lift = await page.evaluate(() => window.__lift);
    if (graphics === 'enhanced') {
      assert.equal(lift.enabled, true);
      assert.ok(lift.lifted > 0, JSON.stringify(lift));
      assert.ok(lift.max <= 0.5 + 1e-6, JSON.stringify(lift));
    } else {
      assert.equal(lift.enabled, false);
      assert.equal(lift.lifted, 0, JSON.stringify(lift));
    }
    assertNoErrors(errors);
    console.log(`  ${graphics}: ${lift.lifted}/${lift.frames} frames lifted, max ${lift.max.toFixed(3)} ft`);
    await page.close();
  }
  console.log('PASS: Rust Enhanced wheel re-projection lifts sunk wheels; Original keeps the simulated pose');
} finally {
  await browser.close();
}
