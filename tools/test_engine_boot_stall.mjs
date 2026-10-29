// A first load that stalls (research/PERFORMANCE.md "Phone menus and loading"): when the
// engine's WebAssembly never arrives, "Now Loading..." offers "Still loading?
// Reload" after 20 s, a 44px target that reloads the page. Content-free: the
// engine never starts.
// Run: node tools/run_browser_tests.mjs --only test_engine_boot_stall.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 390, height: 844 }, hasTouch: true }, errors);
  // The WebAssembly request never completes (a stalled transfer).
  await page.route('**/atv_app_bg.wasm*', () => {});
  await page.goto(url('/play.html'), { waitUntil: 'domcontentloaded' });
  assert.equal(await page.locator('.loading-retry').count(), 0, 'no reload offer at once');
  const retry = page.locator('.loading-retry');
  await retry.waitFor({ state: 'visible', timeout: 30000 });
  const box = await retry.boundingBox();
  assert.ok(box.height >= 44 && box.width >= 44, `target ${JSON.stringify(box)}`);
  assert.ok(box.y + box.height <= 844, 'on screen');
  // Tap while still stalled (releasing the request first would let the
  // engine start and close the loader under the tap). The page reloads.
  const reloaded = page.waitForEvent('load', { timeout: 30000 });
  await retry.tap();
  await reloaded;
  await page.unroute('**/atv_app_bg.wasm*');
  assertNoErrors(errors);
  console.log('PASS stalled first load offers a reload');
} finally {
  await browser.close();
}
