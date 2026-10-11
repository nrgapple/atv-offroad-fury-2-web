// Opt-in original-content browser touch integration. No vehicle state injection.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
  const errors = []; page.on('pageerror', e => errors.push(e.message));
  await page.addInitScript(() => {
    window.touchSamples = [];
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function(data, ...rest) {
      if (data.op === 50) window.touchSamples.push({ input: JSON.parse(new TextDecoder().decode(data.bytes)).input, speed: window.atvTerrainStatus?.frame?.speed });
      return post.call(this, data, ...rest);
    };
  });
  await page.goto(new URL('/play.html', process.env.ATV_TEST_URL || 'http://127.0.0.1:4173').href);
  await page.waitForFunction(() => document.querySelector('#frontend').dataset.page === 'title', null, { timeout: 90000 });
  await page.keyboard.press('Enter');
  await page.getByRole('button', { name: 'Single Player', exact: true }).tap();
  await page.getByRole('button', { name: 'Freestyle', exact: true }).tap();
  await page.getByRole('button', { name: 'Ride Caribou Peak', exact: true }).tap();
  await page.waitForFunction(() => window.atvTerrainStatus.simulation && document.querySelector('#frontend').hidden, null, { timeout: 90000 });
  const read = () => page.evaluate(() => window.atvTerrainStatus);
  const steps = async count => { const target = (await read()).simulationSteps + count; await page.waitForFunction(target => window.atvTerrainStatus.simulationSteps >= target, target, { timeout: 90000 }); };
  const samples = () => page.evaluate(() => window.touchSamples);
  const resetSamples = () => page.evaluate(() => { window.touchSamples = []; });
  const cdp = await page.context().newCDPSession(page);
  const touch = (type, points = []) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: points.map(p => ({ radiusX: 6, radiusY: 6, force: 1, ...p })) });
  assert.equal(await page.locator('#touch-controls button').count(), 0, 'No separate action buttons');
  assert.equal(await page.evaluate(() => getComputedStyle(document.body).webkitUserSelect), 'none');
  for (const position of [{ x: 250, y: 240 }, { x: 340, y: 640 }, { x: 387, y: 420 }]) {
    await page.waitForFunction(() => window.atvTerrainStatus.frame.grounded.some(Boolean), null, { timeout: 10000 });
    const right = { id: 1, ...position };
    await resetSamples(); await touch('touchStart', [right]); await steps(3);
    assert.equal((await read()).input.preload_axis, 1); assert.equal((await read()).input.lean, 0);
    await touch('touchMove', [{ ...right, y: right.y - 70 }]); await steps(3);
    assert.equal((await read()).input.lean, -1); assert.equal((await read()).input.preload_axis, 1);
    await touch('touchMove', [{ ...right, y: right.y + 70 }]); await steps(3);
    assert.equal((await read()).input.lean, 1); assert.equal((await read()).input.preload_axis, 1);
    assert.equal((await read()).frame.preload_active, false);
    await touch('touchEnd'); await steps(3);
    assert.equal((await samples()).filter(s => s.input.preload_axis === -1).length, 1);
    assert.equal((await read()).input.lean, 0);
  }
  console.log('Anywhere on the right holds preload; drag leans without firing; release submits once.');
  const right = { id: 1, x: 310, y: 530 };
  await resetSamples(); await touch('touchStart', [right]); await steps(2); await touch('touchCancel'); await steps(3);
  assert(!(await samples()).some(s => s.input.preload_axis === -1));
  const left = { id: 2, x: 60, y: 620 }, forward = { ...left, y: 550 };
  await touch('touchStart', [left]); await touch('touchMove', [forward]); await touch('touchStart', [forward, right]); await steps(15);
  let state = await read(); assert.equal(state.input.throttle, 1); assert.equal(state.input.preload_axis, 1);
  if (process.env.ATV_TOUCH_SCREENSHOT) await page.screenshot({ path: process.env.ATV_TOUCH_SCREENSHOT });
  await resetSamples(); await touch('touchEnd', [right]); await steps(3);
  assert.equal((await read()).input.throttle, 1); assert.equal((await samples()).filter(s => s.input.preload_axis === -1).length, 1);
  await steps(15); assert((await read()).frame.speed > 1);
  await resetSamples(); await touch('touchMove', [{ ...left, y: 690 }]);
  await page.waitForFunction(() => window.atvTerrainStatus.input.brake > .9, null, { timeout: 10000 });
  assert.equal((await read()).input.reverse, false);
  await page.waitForFunction(() => window.atvTerrainStatus.input.reverse && window.atvTerrainStatus.frame.speed < -2, null, { timeout: 45000 });
  { const log = await samples(), start = log.findIndex(s => s.input.brake > .9); assert(start >= 0); assert(log.slice(start).filter(s => s.speed > 1).every(s => !s.input.reverse && s.input.throttle === 0), JSON.stringify(log)); }
  await resetSamples(); await touch('touchMove', [forward]);
  await page.waitForFunction(() => window.atvTerrainStatus.input.brake > .9, null, { timeout: 10000 });
  assert.equal((await read()).input.reverse, true);
  await page.waitForFunction(() => !window.atvTerrainStatus.input.reverse && window.atvTerrainStatus.frame.speed > 2, null, { timeout: 45000 });
  { const log = await samples(), start = log.findIndex(s => s.input.brake > .9); assert(start >= 0); assert(log.slice(start).filter(s => s.speed < -1).every(s => s.input.reverse && s.input.throttle === 0), JSON.stringify(log)); }
  await touch('touchCancel'); await steps(3);
  console.log('Left back brakes before reverse, and forward brakes reverse motion before changing gear.');
  await touch('touchStart', [right]); await steps(2); await resetSamples();
  await page.evaluate(() => window.dispatchEvent(new Event('blur'))); await touch('touchCancel');
  await page.waitForTimeout(100); const paused = (await read()).frame;
  await page.waitForTimeout(100); assert.deepEqual((await read()).frame, paused);
  await page.evaluate(() => window.dispatchEvent(new Event('focus'))); await steps(3);
  assert(!(await samples()).some(s => s.input.preload_axis === -1));
  await touch('touchStart', [right]); await steps(2); await resetSamples();
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => document.querySelector('#frontend').dataset.page === 'pause');
  assert(await page.locator('#touch-controls').isHidden()); await touch('touchCancel');
  await page.waitForTimeout(100); const menuFrame = (await read()).frame;
  await page.waitForTimeout(100); assert.deepEqual((await read()).frame, menuFrame);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 390);
  const log = await browser.newPage(); await log.goto(new URL('/devlog.html', page.url()).href);
  assert.notEqual(await log.evaluate(() => getComputedStyle(document.body).webkitUserSelect), 'none', 'Dev log remains selectable');
  assert.deepEqual(errors, []);
  console.log('PASS: whole-right hold/lean/release, safe automatic direction changes, cancellation, multi-touch, focus/menu pause, game text selection suppressed; dev log unaffected.');
} finally { await browser.close(); }
