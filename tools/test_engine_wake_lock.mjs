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
  const log = (await page.evaluate(() => window.wakeLog)).join(', ');
  // wake-lock.test.mjs: a refused request retries on the next gesture
  // (pointerup or keydown) ...
  for (const gesture of ['pointer', 'key']) {
    const refused = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await refused.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      const log = (window.wakeLog = []);
      Object.defineProperty(navigator, 'wakeLock', { configurable: true, value: {
        request: async type => {
          log.push(`request:${type}`);
          if (log.length === 1) throw new DOMException('needs a gesture', 'NotAllowedError');
          const lock = { released: false, release: async () => { lock.released = true; log.push('release'); } };
          return lock;
        },
      } });
    });
    await refused.goto(url('/engine.html?venue=env01&uiDiagnostics=1'));
    await refused.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
    await until(refused, () => window.wakeLog.length === 1);
    await nextFrames(refused, 10);
    assert.equal(JSON.parse(await refused.locator('#ui-state').textContent()).wakeLock, false, 'refused: not held');
    if (gesture === 'pointer') await refused.mouse.click(640, 400);
    else await key(refused, 'KeyQ');
    await until(refused, () => JSON.parse(document.querySelector('#ui-state').textContent).wakeLock === true);
    assert.equal((await refused.evaluate(() => window.wakeLog)).length, 2, `${gesture} gesture retries once`);
    await refused.close();
  }
  // ... and a lock granted after the ride left the screen is released at once.
  const late = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await late.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    const log = (window.wakeLog = []);
    Object.defineProperty(navigator, 'wakeLock', { configurable: true, value: {
      request: type => {
        log.push(`request:${type}`);
        return new Promise(resolve => {
          window.grantWake = () => {
            const lock = { released: false, release: async () => { lock.released = true; log.push('release'); } };
            resolve(lock);
          };
        });
      },
    } });
  });
  await late.goto(url('/engine.html?venue=env01&uiDiagnostics=1'));
  await late.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await until(late, () => typeof window.grantWake === 'function');
  await late.mouse.click(640, 400);
  await key(late, 'Escape');
  await until(late, () => JSON.parse(document.querySelector('#ui-state').textContent).paused === true);
  await late.evaluate(() => window.grantWake());
  await until(late, () => window.wakeLog.includes('release'));
  await nextFrames(late, 5);
  assert.equal(JSON.parse(await late.locator('#ui-state').textContent()).wakeLock, false, 'late lock is not kept');
  assertNoErrors(errors);
  console.log(`PASS: Rust screen wake lock: ${log}; refused requests retry on a pointer or key gesture; a late lock is released`);
} finally {
  await browser.close();
}
