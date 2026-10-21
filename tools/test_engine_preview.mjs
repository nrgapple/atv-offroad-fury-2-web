// The shared Rust engine page starts its WebAssembly module and renders.
// Content-free (CI): the module initializes and reports the missing package.
// With content: ENV01 renders frames in Chromium and WebKit, and holding W
// drives the original quad (the status line reports its speed).
import assert from 'node:assert/strict';
import { assertNoErrors, contentAvailable, launch, newPage, url } from './browser/harness.mjs';

for (const engine of ['chromium', 'webkit']) {
  const browser = await launch({ engine });
  try {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
    await page.goto(url('/engine.html?venue=env01'));
    const status = () => page.locator('#status').textContent();
    if (contentAvailable) {
      await page.waitForFunction(() => /\d+ fps/.test(document.querySelector('#status').textContent), null, { timeout: 120000 });
      assert.match(await status(), /^ENV01 · \d+ fps .* [1-9]\d* tris · Gl .* · \d+ MPH$/);
      await page.locator('canvas').click();
      await page.keyboard.down('KeyW');
      await page.waitForFunction(() => /· [1-9]\d* MPH$/.test(document.querySelector('#status').textContent), null, { timeout: 15000 });
      await page.keyboard.up('KeyW');
    } else {
      await page.waitForFunction(() => /content\/manifest\.json|content package/.test(document.querySelector('#status').textContent), null, { timeout: 60000 });
    }
    assertNoErrors(errors);
    console.log(`PASS: ${engine} Rust engine preview: ${await status()}`);
  } finally {
    await browser.close();
  }
}
