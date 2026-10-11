// Opt-in original Caribou terrain + Rust/WASM positive preload check.
// Drives only through actual Chromium touch events; no physics/state injection.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { writeFile } from 'node:fs/promises';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
  const errors = []; page.on('pageerror', e => errors.push(e.message));
  const url = new URL('/play.html', process.env.ATV_TEST_URL || 'http://127.0.0.1:4173');
  url.searchParams.set('drive', '1'); url.searchParams.set('environment', 'env01');
  await page.goto(url.href);
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation && !document.querySelector('#touch-controls').hidden, null, { timeout: 90000 });
  const cdp = await page.context().newCDPSession(page);
  const touch = (type, points = []) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: points.map(point => ({ radiusX: 6, radiusY: 6, force: 1, ...point })) });
  const left = { id: 1, x: 60, y: 560 }, drive = { ...left, y: 490 };
  const right = { id: 2, x: 310, y: 530 };
  await page.evaluate(() => {
    window.rampFrames = []; let previous = 0;
    function record() {
      const state = window.atvTerrainStatus;
      if (state.simulationSteps !== previous) {
        previous = state.simulationSteps;
        window.rampFrames.push({ step: previous, frame: state.frame, input: state.input });
      }
      requestAnimationFrame(record);
    }
    requestAnimationFrame(record);
  });
  await touch('touchStart', [left]); await touch('touchMove', [drive]); await touch('touchStart', [drive, right]);
  await page.waitForFunction(() => window.atvTerrainStatus.frame.preload_armed, null, { timeout: 45000, polling: 'raf' });
  const armed = await page.evaluate(() => window.atvTerrainStatus.frame);
  // CDP touchEnd carries the released finger, preserving the left throttle.
  await touch('touchEnd', [right]);
  await page.waitForFunction(() => window.rampFrames.some(s => s.frame.preload_active), null, { timeout: 5000, polling: 'raf' });
  if (process.env.ATV_TOUCH_RAMP_SCREENSHOT) await page.screenshot({ path: process.env.ATV_TOUCH_RAMP_SCREENSHOT });
  await page.waitForFunction(() => window.rampFrames.some(s => s.frame.preload_active && !s.frame.grounded.some(Boolean)), null, { timeout: 5000, polling: 'raf' });
  if (process.env.ATV_TOUCH_AIR_AXES === '1') {
    const before = await page.evaluate(() => window.atvTerrainStatus.frame);
    await touch('touchMove', [{ ...drive, x: 95, y: 525 }]);
    await page.waitForFunction(() => window.rampFrames.some(s => !s.frame.grounded.some(Boolean) && s.input.lean > .4 && s.input.steer < -.4 && s.input.throttle === 1), null, { timeout: 5000, polling: 'raf' });
    const controlled = await page.evaluate(() => window.rampFrames.filter(s => !s.frame.grounded.some(Boolean) && s.input.lean > .4).at(-1));
    assert(controlled.frame.local_angular_velocity[0] < -.01, `Original ATV01 AeroMomentScaleX=-2 produces negative pitch rate for backward lean: ${JSON.stringify({ before:before.local_angular_velocity, after:controlled.frame.local_angular_velocity })}`);
    assert.notDeepEqual(controlled.frame.orientation, before.orientation, 'Native body orientation responds; no pose injection');
  }
  await touch('touchCancel');
  const frames = await page.evaluate(() => window.rampFrames);
  const active = frames.filter(s => s.frame.preload_active);
  assert(active.length > 0); assert(active.some(s => s.frame.preload_strength > 0));
  assert(active.some(s => s.frame.linear_velocity[1] > 0));
  assert(active.some(s => !s.frame.grounded.some(Boolean)));
  assert(active.some(s => s.input.throttle === 1), 'The left finger must keep driving after right release');
  assert.deepEqual(errors, []);
  if (process.env.ATV_TOUCH_RAMP_TRACE) await writeFile(process.env.ATV_TOUCH_RAMP_TRACE, JSON.stringify({ armed, frames, errors }, null, 2));
  console.log(`PASS: actual Caribou ramp, native arming and active preload, airborne contacts; upward speed at arm ${armed.linear_velocity[1].toFixed(2)} ft/s, peak active ${Math.max(...active.map(s => s.frame.linear_velocity[1])).toFixed(2)} ft/s. No injected state/forces.`);
} finally { await browser.close(); }
