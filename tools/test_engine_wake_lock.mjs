// Screen Wake Lock in the Rust engine (web/wake-lock.js): held while a ride is
// on screen, released when the pause menu opens, requested again on resume.
// A recording stand-in replaces navigator.wakeLock (host browsers deny the
// real lock without a user activation policy). Host browser evidence.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const until = (page, predicate) => page.waitForFunction(predicate, null, { timeout: 30000 });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    const log = window.wakeLog = [];
    Object.defineProperty(navigator, 'wakeLock', { configurable: true, value: {
      request: async type => { log.push(`request:${type}`); const lock = { released: false, release: async () => { lock.released = true; log.push('release'); } }; return lock; },
    } });
  });
  await page.goto(url('/engine.html?venue=env01&uiDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).wakeLock === true);
  assert.deepEqual(await page.evaluate(() => window.wakeLog), ['request:screen']);
  await page.mouse.click(640, 400);
  await key(page, 'Escape');
  await until(page, () => window.wakeLog.includes('release'));
  await key(page, 'Escape');
  await until(page, () => window.wakeLog.filter(e => e === 'request:screen').length === 2);
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).wakeLock === true);
  assertNoErrors(errors);
  console.log(`PASS: Rust screen wake lock: ${(await page.evaluate(() => window.wakeLog)).join(', ')}`);
} finally {
  await browser.close();
}
