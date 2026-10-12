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
  const left = { id: 1, x: 60, y: 560 };
  const right = { id: 2, x: 310, y: 530 }, drive = { ...right, y: 460 };
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
  await touch('touchStart', [left, right]); await touch('touchMove', [left, drive]);
  await page.waitForFunction(() => window.atvTerrainStatus.frame.preload_armed, null, { timeout: 45000, polling: 'raf' });
  const armed = await page.evaluate(() => window.atvTerrainStatus.frame);
  let pullbackStep;
  if (process.env.ATV_TOUCH_AIR_AXES === '1') {
    pullbackStep = await page.evaluate(() => window.atvTerrainStatus.simulationSteps);
    // Left pullback is lean even on the ramp; right release separately stops throttle.
    await touch('touchMove', [{ ...left, y: left.y + 70 }, drive]);
  }
  // CDP touchEnd carries the released finger, preserving the left command.
  await touch('touchEnd', [drive]);
  await page.waitForFunction(() => window.rampFrames.some(s => s.frame.preload_active), null, { timeout: 5000, polling: 'raf' });
  if (process.env.ATV_TOUCH_RAMP_SCREENSHOT && !pullbackStep) await page.screenshot({ path: process.env.ATV_TOUCH_RAMP_SCREENSHOT });
  await page.waitForFunction(() => window.rampFrames.some(s => s.frame.preload_active && !s.frame.grounded.some(Boolean)), null, { timeout: 5000, polling: 'raf' });
  if (process.env.ATV_TOUCH_AIR_AXES === '1') {
    await page.waitForFunction(step => window.rampFrames.some(s => s.step > step && !s.frame.grounded.some(Boolean) && s.input.lean === 1), pullbackStep, { timeout: 5000, polling: 'raf' });
    const controlled = await page.evaluate(step => window.rampFrames.find(s => s.step > step && !s.frame.grounded.some(Boolean) && s.input.lean === 1), pullbackStep);
    const firstAir = await page.evaluate(step => window.rampFrames.find(s => s.step > step && !s.frame.grounded.some(Boolean)), pullbackStep);
    assert.equal(controlled.input.brake, 0, 'Left lean must never introduce braking');
    assert.equal(controlled.input.throttle, 0, 'Right release stops throttle in air');
    assert(controlled.step - firstAir.step <= 8, 'Held left pullback remains native lean at takeoff');
    // Pitch momentum from the ramp does not disappear instantly. Observe the
    // subsequent native response instead of requiring an injected rate change.
    await page.waitForFunction(step => window.rampFrames.some(s => s.step > step && !s.frame.grounded.some(Boolean) && s.input.lean === 1 && s.frame.local_angular_velocity[0] < -.01), controlled.step, {timeout:5000,polling:'raf'});
    assert.notDeepEqual(controlled.frame.orientation, armed.orientation, 'Native body orientation responds; no pose injection');
  }
  await touch('touchCancel');
  if (process.env.ATV_TOUCH_RAMP_SCREENSHOT && pullbackStep) await page.screenshot({ path: process.env.ATV_TOUCH_RAMP_SCREENSHOT });
  let landed;
  if (!pullbackStep) {
    await page.waitForFunction(() => window.atvTerrainStatus.frame.grounded.every(Boolean) && window.atvTerrainStatus.input.lean === 0 && window.atvTerrainStatus.input.brake === 0, null, { timeout: 15000, polling: 'raf' });
    landed = await page.evaluate(() => ({ step:window.atvTerrainStatus.simulationSteps, origin:window.atvTerrainStatus.frame.origin }));
    await touch('touchStart', [right]); await touch('touchMove', [drive]);
    await page.waitForFunction(start => window.atvTerrainStatus.simulationSteps > start.step + 30 && window.atvTerrainStatus.input.throttle === 1 && Math.hypot(...window.atvTerrainStatus.frame.origin.map((value,index) => value-start.origin[index])) > 2, landed, { timeout:15000,polling:'raf' });
    await touch('touchCancel');
  }
  const frames = await page.evaluate(() => window.rampFrames);
  const active = frames.filter(s => s.frame.preload_active);
  assert(active.length > 0); assert(active.some(s => s.frame.preload_strength > 0));
  assert(active.some(s => s.frame.linear_velocity[1] > 0));
  assert(active.some(s => !s.frame.grounded.some(Boolean)));
  assert(active.every(s => s.input.throttle === 0), 'Right release stops throttle throughout active preload');
  assert.deepEqual(errors, []);
  if (process.env.ATV_TOUCH_RAMP_TRACE) await writeFile(process.env.ATV_TOUCH_RAMP_TRACE, JSON.stringify({ armed, pullbackStep, landed, frames, errors }, null, 2));
  console.log(`PASS: actual Caribou ramp, native arming and active preload, airborne contacts; upward speed at arm ${armed.linear_velocity[1].toFixed(2)} ft/s, peak active ${Math.max(...active.map(s => s.frame.linear_velocity[1])).toFixed(2)} ft/s. No injected state/forces.`);
} finally { await browser.close(); }
