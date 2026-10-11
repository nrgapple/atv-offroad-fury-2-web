// Opt-in original Caribou ramp, airborne touch steering and original stunt motions.
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
  await page.waitForFunction(() => !window.atvTerrainStatus.frame.grounded.some(Boolean), null, { timeout: 5000, polling: 'raf' });
  const before = await page.evaluate(() => window.atvTerrainStatus.frame);
  // Start a NEW airborne right swipe, distinct from the grounded preload hold.
  await touch('touchStart', [drive, right]);
  const swipe = { ...right, y: right.y - 70 };
  await touch('touchMove', [drive, swipe]);
  await page.waitForFunction(() => window.atvTerrainStatus.frame.stunt?.id === 0, null, { timeout: 5000, polling: 'raf' });
  await page.waitForFunction(() => window.atvTerrainStatus.frame.stunt?.main_time > .65, null, { timeout: 5000, polling: 'raf' });
  const performing = await page.evaluate(() => window.atvTerrainStatus.frame);
  assert.equal(performing.rider_mode, 'stunt');
  assert.notDeepEqual(performing.rider_node_matrices, before.rider_node_matrices, 'Original Nac Nac MOT must change the actual rendered rider pose');
  if (process.env.ATV_TOUCH_STUNT_SCREENSHOT) await page.screenshot({ path: process.env.ATV_TOUCH_STUNT_SCREENSHOT });
  await page.waitForFunction(() => window.atvTerrainStatus.frame.stunt?.holding, null, { timeout: 5000, polling: 'raf' });
  const held = await page.evaluate(() => window.atvTerrainStatus.frame);
  assert.equal(held.rider_mode, 'hold'); assert.equal(held.stunt.name, 'Nac Nac');
  // A released modifier resumes the main motion from its paused clock.
  await touch('touchEnd', [swipe]);
  await page.waitForFunction(time => window.atvTerrainStatus.frame.stunt && !window.atvTerrainStatus.frame.stunt.holding && window.atvTerrainStatus.frame.stunt.main_time > time, held.stunt.main_time, { timeout: 5000, polling: 'raf' });
  await touch('touchCancel');
  const frames = await page.evaluate(() => window.rampFrames);
  assert(frames.some(s => s.frame.stunt && !s.frame.grounded.some(Boolean)));
  assert(frames.some(s => s.frame.preload_active));
  assert.deepEqual(errors, []);
  if (process.env.ATV_TOUCH_STUNT_TRACE) await writeFile(process.env.ATV_TOUCH_STUNT_TRACE, JSON.stringify({ armed, before, performing, held, frames, errors }, null, 2));
  console.log(`PASS: real Caribou ramp into original Nac Nac main/held MOT clips; hold at ${held.stunt.main_time.toFixed(2)}s; release resumes main animation. No injected state/forces.`);
} finally { await browser.close(); }
